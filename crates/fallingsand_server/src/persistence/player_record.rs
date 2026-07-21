use super::StoreError;
use crate::inventory::Inventory;
use crate::player::{AvatarSnapshot, Player, PlayerLife, RestoredPlayer, ResumeSnapshot};
use fallingsand_core::{
    HOTBAR_SLOTS, Inventory as CoreInventory, ItemId, ItemStack, MAX_AIR_SECONDS, MAX_HEALTH,
    PLAYER_SLOTS, Subcell, content,
};
use fallingsand_math::SUBCELL_UNITS_PER_CELL;
use fallingsand_protocol::GameMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StackRecord {
    item: String,
    count: u32,
}

type SlotRecord = Option<StackRecord>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct PlayerRecord {
    mode: GameMode,
    selected: u8,
    inventory: Vec<SlotRecord>,
    cursor: SlotRecord,
    trash: SlotRecord,
    history: Vec<String>,
    resume: ResumeState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum ResumeState {
    Alive(AvatarRecord),
    Dead {
        view_anchor: fallingsand_core::CellPos,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct AvatarRecord {
    x: Subcell,
    y: Subcell,
    vx: Subcell,
    vy: Subcell,
    hp: f32,
    regen_delay_ticks: u64,
    air: f32,
    burning: f32,
    flying: bool,
}

fn subcell_position_fits(value: Subcell) -> bool {
    let cell = value.raw().div_euclid(i64::from(SUBCELL_UNITS_PER_CELL));
    i32::try_from(cell).is_ok()
}

fn validate_avatar_record(record: &AvatarRecord) -> Result<(), StoreError> {
    if !subcell_position_fits(record.x) || !subcell_position_fits(record.y) {
        return Err(StoreError::CorruptPlayer(
            "avatar position is outside the cell coordinate range".into(),
        ));
    }
    if !record.hp.is_finite() || !(0.0..=MAX_HEALTH).contains(&record.hp) {
        return Err(StoreError::CorruptPlayer(format!(
            "invalid avatar health {}",
            record.hp
        )));
    }
    if !record.air.is_finite() || !(0.0..=MAX_AIR_SECONDS).contains(&record.air) {
        return Err(StoreError::CorruptPlayer(format!(
            "invalid avatar air {}",
            record.air
        )));
    }
    if !record.burning.is_finite() || record.burning < 0.0 {
        return Err(StoreError::CorruptPlayer(format!(
            "invalid avatar burning duration {}",
            record.burning
        )));
    }
    Ok(())
}

impl From<AvatarRecord> for AvatarSnapshot {
    fn from(record: AvatarRecord) -> Self {
        Self {
            x: record.x,
            y: record.y,
            vx: record.vx,
            vy: record.vy,
            hp: record.hp,
            regen_delay_ticks: record.regen_delay_ticks,
            air: record.air,
            burning: record.burning,
            flying: record.flying,
        }
    }
}

impl From<&AvatarSnapshot> for AvatarRecord {
    fn from(snapshot: &AvatarSnapshot) -> Self {
        Self {
            x: snapshot.x,
            y: snapshot.y,
            vx: snapshot.vx,
            vy: snapshot.vy,
            hp: snapshot.hp,
            regen_delay_ticks: snapshot.regen_delay_ticks,
            air: snapshot.air,
            burning: snapshot.burning,
            flying: snapshot.flying,
        }
    }
}

fn stack_to_record(stack: Option<ItemStack>) -> Result<SlotRecord, StoreError> {
    let Some(stack) = stack else {
        return Ok(None);
    };
    if stack.count == 0 {
        return Err(StoreError::CorruptPlayer(format!(
            "item {} has an empty stack",
            stack.item.0
        )));
    }
    let item = content::try_item(stack.item).filter(|_| stack.item != ItemId::NONE);
    let item =
        item.ok_or_else(|| StoreError::CorruptPlayer(format!("invalid item id {}", stack.item.0)))?;
    if stack.count > item.stack_max {
        return Err(StoreError::CorruptPlayer(format!(
            "{} of item {:?} exceeds stack limit {}",
            stack.count, item.name, item.stack_max
        )));
    }
    Ok(Some(StackRecord {
        item: item.name.to_owned(),
        count: stack.count,
    }))
}

fn stack_from_record(record: &SlotRecord) -> Result<Option<ItemStack>, StoreError> {
    let Some(StackRecord { item: name, count }) = record.as_ref() else {
        return Ok(None);
    };
    if *count == 0 {
        return Err(StoreError::CorruptPlayer(format!(
            "item {name:?} has an empty stack"
        )));
    }
    match content::item_id_of(name) {
        Some(id) if id != ItemId::NONE => {
            let item = content::item(id);
            if *count > item.stack_max {
                return Err(StoreError::CorruptPlayer(format!(
                    "{count} of item {name:?} exceeds stack limit {}",
                    item.stack_max
                )));
            }
            Ok(Some(ItemStack::new(id, *count)))
        }
        _ => Err(StoreError::CorruptPlayer(format!(
            "unknown item {name:?} with count {count}"
        ))),
    }
}

fn slots_to_record(inv: &CoreInventory) -> Result<Vec<SlotRecord>, StoreError> {
    inv.slots
        .iter()
        .map(|slot| stack_to_record(*slot))
        .collect()
}

fn player_slots_from_record(list: &[SlotRecord]) -> Result<CoreInventory, StoreError> {
    if list.len() != PLAYER_SLOTS {
        return Err(StoreError::CorruptPlayer(format!(
            "expected {PLAYER_SLOTS} inventory slots, got {}",
            list.len()
        )));
    }
    let mut inv = CoreInventory::with_capacity(PLAYER_SLOTS);
    for (slot, record) in inv.slots.iter_mut().zip(list) {
        *slot = stack_from_record(record)?;
    }
    Ok(inv)
}

pub(super) fn restore_player(record: PlayerRecord) -> Result<RestoredPlayer, StoreError> {
    if record.selected as usize >= HOTBAR_SLOTS {
        return Err(StoreError::CorruptPlayer(format!(
            "invalid selected slot {}",
            record.selected
        )));
    }
    let resume = match record.resume {
        ResumeState::Alive(record) => {
            validate_avatar_record(&record)?;
            ResumeSnapshot::Alive(record.into())
        }
        ResumeState::Dead { view_anchor } => ResumeSnapshot::Dead { view_anchor },
    };
    Ok(RestoredPlayer {
        mode: record.mode,
        selected_slot: record.selected,
        inventory: Inventory::with(
            player_slots_from_record(&record.inventory)?,
            stack_from_record(&record.cursor)?,
            stack_from_record(&record.trash)?,
        ),
        history: record.history,
        resume,
    })
}

pub(super) fn snapshot_player(player: &Player) -> Result<PlayerRecord, StoreError> {
    let resume = match &player.life {
        PlayerLife::Entering(entering) => {
            ResumeState::Alive(AvatarRecord::from(&entering.materialization.template))
        }
        PlayerLife::Alive(avatar) => {
            let snapshot = AvatarSnapshot::from_avatar(avatar);
            ResumeState::Alive(AvatarRecord::from(&snapshot))
        }
        PlayerLife::Dead(dead) => ResumeState::Dead {
            view_anchor: dead.view_anchor,
        },
        PlayerLife::Reviving(reviving) => ResumeState::Dead {
            view_anchor: reviving.death.view_anchor,
        },
    };
    let record = PlayerRecord {
        mode: player.profile.mode,
        selected: player.profile.selected_slot,
        inventory: slots_to_record(&player.profile.inventory.inner)?,
        cursor: stack_to_record(player.profile.inventory.cursor)?,
        trash: stack_to_record(player.profile.inventory.trash)?,
        history: player.profile.history.clone(),
        resume,
    };
    if record.selected as usize >= HOTBAR_SLOTS {
        return Err(StoreError::CorruptPlayer(format!(
            "invalid selected slot {}",
            record.selected
        )));
    }
    if let ResumeState::Alive(avatar) = &record.resume {
        validate_avatar_record(avatar)?;
    }
    Ok(record)
}
