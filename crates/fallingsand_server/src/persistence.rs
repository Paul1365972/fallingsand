use crate::WorldInfo;
use crate::bodies::PixelBodies;
use crate::inventory::Inventory;
use crate::player::{AvatarSnapshot, Player, PlayerLife, Players, RestoredPlayer, ResumeSnapshot};
use crate::regions::{RegionMap, collect_dirty_saves, mark_changed_regions, mark_saved};
use fallingsand_core::{
    CHUNK_AREA, CHUNK_SIZE, Calendar, Cell, CellPos, DirtyRect, HOTBAR_SLOTS,
    Inventory as CoreInventory, ItemId, ItemStack, MaterialId, PLAYER_SLOTS, REGION_AREA_CHUNKS,
    Region, RegionPos, Subcell, content,
};
use fallingsand_protocol::{GameMode, PlayerUuid};
use fallingsand_sim::bodies::{BodyParts, body_parts};
use fallingsand_sim::{CellWorld, PixelBody};
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const REGION_FORMAT_VERSION: u8 = 12;
pub const WORLD_FORMAT_VERSION: u16 = 23;
const AUTOSAVE_INTERVAL_TICKS: u64 = fallingsand_core::ticks_from_secs(10.0);
const CELL_BYTES: usize = 8;
const RECT_BYTES: usize = 4;
const REGION_CELL_BYTES: usize = REGION_AREA_CHUNKS * CHUNK_AREA * CELL_BYTES;
const REGION_RAW_BYTES: usize = REGION_CELL_BYTES + REGION_AREA_CHUNKS * RECT_BYTES;

const REGIONS: TableDefinition<u64, &[u8]> = TableDefinition::new("regions");
const PLAYERS: TableDefinition<u128, &[u8]> = TableDefinition::new("players");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMeta {
    pub format_version: u16,
    pub seed: u64,
    pub name: String,
    pub world_age: u64,
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyRecord {
    pub width: u8,
    pub height: u8,
    pub cells: Vec<(u16, u8)>,
    pub x: Subcell,
    pub y: Subcell,
    pub vx: Subcell,
    pub vy: Subcell,
    pub angle: f32,
    pub spin: f32,
    pub rest_secs: f32,
}

pub fn body_record(body: &PixelBody) -> BodyRecord {
    let parts = body_parts(body);
    BodyRecord {
        width: parts.width,
        height: parts.height,
        cells: parts
            .cells
            .iter()
            .map(|cell| (cell.material.0, cell.shade_flags))
            .collect(),
        x: parts.x,
        y: parts.y,
        vx: parts.vx,
        vy: parts.vy,
        angle: parts.angle,
        spin: parts.spin,
        rest_secs: parts.rest_secs,
    }
}

pub fn record_to_parts(record: &BodyRecord) -> BodyParts {
    BodyParts {
        width: record.width,
        height: record.height,
        cells: record
            .cells
            .iter()
            .map(|&(material, shade_flags)| {
                let mut cell = Cell::AIR;
                cell.material = MaterialId(material);
                cell.shade_flags = shade_flags;
                cell.set_body(false);
                cell
            })
            .collect(),
        x: record.x,
        y: record.y,
        vx: record.vx,
        vy: record.vy,
        angle: record.angle,
        spin: record.spin,
        rest_secs: record.rest_secs,
    }
}

pub fn body_home_region(body: &PixelBody) -> RegionPos {
    CellPos::new(body.x.floor_cell(), body.y.floor_cell()).region()
}

pub struct RegionLoad {
    pub region: Region,
    pub bodies: Vec<BodyRecord>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackRecord {
    pub item: String,
    pub count: u32,
}

pub type SlotRecord = Option<StackRecord>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerRecord {
    pub mode: GameMode,
    pub selected: u8,
    pub inventory: Vec<SlotRecord>,
    pub cursor: SlotRecord,
    pub trash: SlotRecord,
    pub history: Vec<String>,
    pub resume: ResumeState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResumeState {
    Alive(AvatarRecord),
    Dead {
        view_anchor: fallingsand_core::CellPos,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvatarRecord {
    pub x: Subcell,
    pub y: Subcell,
    pub vx: Subcell,
    pub vy: Subcell,
    pub hp: f32,
    pub regen_delay_ticks: u64,
    pub air: f32,
    pub burning: f32,
    pub flying: bool,
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

pub fn stack_to_record(stack: Option<ItemStack>) -> Result<SlotRecord, StoreError> {
    let Some(stack) = stack.filter(|stack| stack.count > 0) else {
        return Ok(None);
    };
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

pub fn stack_from_record(record: &SlotRecord) -> Result<Option<ItemStack>, StoreError> {
    let Some(StackRecord { item: name, count }) = record.as_ref() else {
        return Ok(None);
    };
    if *count == 0 {
        return Ok(None);
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

pub fn slots_to_record(inv: &CoreInventory) -> Result<Vec<SlotRecord>, StoreError> {
    inv.slots
        .iter()
        .map(|slot| stack_to_record(*slot))
        .collect()
}

pub fn player_slots_from_record(list: &[SlotRecord]) -> Result<CoreInventory, StoreError> {
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

pub fn restore_player(record: PlayerRecord) -> Result<RestoredPlayer, StoreError> {
    if record.selected as usize >= HOTBAR_SLOTS {
        return Err(StoreError::CorruptPlayer(format!(
            "invalid selected slot {}",
            record.selected
        )));
    }
    let resume = match record.resume {
        ResumeState::Alive(record) => ResumeSnapshot::Alive(record.into()),
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

pub fn snapshot_player(player: &Player) -> Result<PlayerRecord, StoreError> {
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
    Ok(PlayerRecord {
        mode: player.profile.mode,
        selected: player.profile.selected_slot,
        inventory: slots_to_record(&player.profile.inventory.inner)?,
        cursor: stack_to_record(player.profile.inventory.cursor)?,
        trash: stack_to_record(player.profile.inventory.trash)?,
        history: player.profile.history.clone(),
        resume,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redb: {0}")]
    Redb(#[from] redb::Error),
    #[error("corrupt region blob: {0}")]
    CorruptRegion(String),
    #[error("corrupt record: {0}")]
    CorruptRecord(#[from] postcard::Error),
    #[error("corrupt player record: {0}")]
    CorruptPlayer(String),
    #[error(
        "unsupported world format {0} (current {WORLD_FORMAT_VERSION}); pre-release worlds carry no migrations — delete the world and create a new one"
    )]
    UnsupportedWorld(u16),
    #[error("unsupported region format {0} (server supports {REGION_FORMAT_VERSION})")]
    UnsupportedRegion(u8),
    #[error("failed to load region {pos:?}: {source}")]
    RegionLoad {
        pos: RegionPos,
        #[source]
        source: Box<StoreError>,
    },
}

impl From<redb::DatabaseError> for StoreError {
    fn from(err: redb::DatabaseError) -> Self {
        Self::Redb(err.into())
    }
}
impl From<redb::TransactionError> for StoreError {
    fn from(err: redb::TransactionError) -> Self {
        Self::Redb(err.into())
    }
}
impl From<redb::TableError> for StoreError {
    fn from(err: redb::TableError) -> Self {
        Self::Redb(err.into())
    }
}
impl From<redb::StorageError> for StoreError {
    fn from(err: redb::StorageError) -> Self {
        Self::Redb(err.into())
    }
}
impl From<redb::CommitError> for StoreError {
    fn from(err: redb::CommitError) -> Self {
        Self::Redb(err.into())
    }
}

struct WorldStore {
    db: Database,
}

pub struct Persistence {
    store: Option<WorldStore>,
    pending_regions: BTreeMap<RegionPos, Vec<u8>>,
    pending_players: BTreeMap<PlayerUuid, PlayerRecord>,
    pending_meta: Option<WorldMeta>,
}

impl Persistence {
    pub fn open(path: Option<&Path>) -> Result<Self, StoreError> {
        Ok(Self {
            store: path.map(WorldStore::open).transpose()?,
            pending_regions: BTreeMap::new(),
            pending_players: BTreeMap::new(),
            pending_meta: None,
        })
    }

    pub fn load_meta(&self) -> Result<Option<WorldMeta>, StoreError> {
        match &self.pending_meta {
            Some(meta) => Ok(Some(meta.clone())),
            None => self.store.as_ref().map_or(Ok(None), WorldStore::load_meta),
        }
    }

    pub fn stage_meta(&mut self, meta: WorldMeta) {
        self.pending_meta = Some(meta);
    }

    pub fn flush_meta(&mut self) -> Result<(), StoreError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let Some(meta) = &self.pending_meta else {
            return Ok(());
        };
        store.save_meta(meta)?;
        self.pending_meta = None;
        Ok(())
    }

    pub fn load_region(&mut self, pos: RegionPos) -> Result<Option<RegionLoad>, StoreError> {
        if let Some(blob) = self.pending_regions.get(&pos) {
            let (region, bodies) = decode_region(blob)?;
            self.pending_regions.remove(&pos);
            return Ok(Some(RegionLoad {
                region,
                bodies,
                dirty: true,
            }));
        }
        self.store.as_ref().map_or(Ok(None), |store| {
            Ok(store.load_region(pos)?.map(|(region, bodies)| RegionLoad {
                region,
                bodies,
                dirty: false,
            }))
        })
    }

    pub fn stage_region(&mut self, pos: RegionPos, blob: Vec<u8>) {
        self.pending_regions.insert(pos, blob);
    }

    pub fn stage_regions(&mut self, regions: impl IntoIterator<Item = (RegionPos, Vec<u8>)>) {
        self.pending_regions.extend(regions);
    }

    pub fn flush_regions(&mut self) -> Result<usize, StoreError> {
        if self.pending_regions.is_empty() {
            return Ok(0);
        }
        let count = self.pending_regions.len();
        let Some(store) = &self.store else {
            return Ok(count);
        };
        let regions: Vec<_> = self
            .pending_regions
            .iter()
            .map(|(&pos, blob)| (pos, blob.clone()))
            .collect();
        store.save_regions(&regions)?;
        self.pending_regions.clear();
        Ok(count)
    }

    pub fn load_player(&mut self, uuid: PlayerUuid) -> Result<Option<PlayerRecord>, StoreError> {
        if let Some(record) = self.pending_players.remove(&uuid) {
            return Ok(Some(record));
        }
        self.store
            .as_ref()
            .map_or(Ok(None), |store| store.load_player(uuid))
    }

    pub fn stage_player(&mut self, uuid: PlayerUuid, record: PlayerRecord) {
        self.pending_players.insert(uuid, record);
    }

    pub fn flush_players(&mut self) -> Result<usize, StoreError> {
        if self.pending_players.is_empty() {
            return Ok(0);
        }
        let count = self.pending_players.len();
        let Some(store) = &self.store else {
            return Ok(count);
        };
        let players: Vec<_> = self
            .pending_players
            .iter()
            .map(|(&uuid, record)| (uuid, record.clone()))
            .collect();
        store.save_players(&players)?;
        self.pending_players.clear();
        Ok(count)
    }
}

impl WorldStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let db = Database::create(path)?;
        {
            let read = db.begin_read()?;
            match read.open_table(META) {
                Ok(table) => {
                    if let Some(guard) = table.get("world")? {
                        parse_meta(guard.value())?;
                    }
                }
                Err(redb::TableError::TableDoesNotExist(_)) => {}
                Err(err) => return Err(err.into()),
            }
        }
        let write = db.begin_write()?;
        {
            write.open_table(REGIONS)?;
            write.open_table(PLAYERS)?;
            write.open_table(META)?;
        }
        write.commit()?;
        Ok(Self { db })
    }

    pub fn load_meta(&self) -> Result<Option<WorldMeta>, StoreError> {
        let read = self.db.begin_read()?;
        let table = read.open_table(META)?;
        let Some(guard) = table.get("world")? else {
            return Ok(None);
        };
        parse_meta(guard.value()).map(Some)
    }

    pub fn save_meta(&self, meta: &WorldMeta) -> Result<(), StoreError> {
        let bytes = postcard::to_allocvec(meta)?;
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(META)?;
            table.insert("world", bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn load_region(
        &self,
        pos: RegionPos,
    ) -> Result<Option<(Region, Vec<BodyRecord>)>, StoreError> {
        let read = self.db.begin_read()?;
        let table = read.open_table(REGIONS)?;
        let Some(guard) = table.get(pos.zorder_key())? else {
            return Ok(None);
        };
        decode_region(guard.value()).map(Some)
    }

    pub fn save_regions(&self, regions: &[(RegionPos, Vec<u8>)]) -> Result<(), StoreError> {
        if regions.is_empty() {
            return Ok(());
        }
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(REGIONS)?;
            for (pos, blob) in regions {
                table.insert(pos.zorder_key(), blob.as_slice())?;
            }
        }
        write.commit()?;
        Ok(())
    }

    pub fn load_player(&self, uuid: PlayerUuid) -> Result<Option<PlayerRecord>, StoreError> {
        let read = self.db.begin_read()?;
        let table = read.open_table(PLAYERS)?;
        let Some(guard) = table.get(uuid.0)? else {
            return Ok(None);
        };
        Ok(Some(postcard::from_bytes(guard.value())?))
    }

    pub fn save_players(&self, players: &[(PlayerUuid, PlayerRecord)]) -> Result<(), StoreError> {
        if players.is_empty() {
            return Ok(());
        }
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(PLAYERS)?;
            for (uuid, record) in players {
                let bytes = postcard::to_allocvec(record)?;
                table.insert(uuid.0, bytes.as_slice())?;
            }
        }
        write.commit()?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn autosave(
    sim: &CellWorld,
    regions: &mut RegionMap,
    bodies: &PixelBodies,
    info: &WorldInfo,
    clock: &Calendar,
    players: &Players,
    persistence: &mut Persistence,
) {
    let tick = sim.tick();
    if tick == 0 || !tick.is_multiple_of(AUTOSAVE_INTERVAL_TICKS) {
        return;
    }

    let to_save = collect_dirty_saves(sim, regions, bodies);
    let saved_regions: Vec<_> = to_save.iter().map(|(pos, _)| *pos).collect();
    persistence.stage_regions(to_save);
    match persistence.flush_regions() {
        Ok(count) => {
            mark_saved(regions, saved_regions);
            if count > 0 {
                tracing::debug!("autosaved {count} regions");
            }
        }
        Err(err) => tracing::error!("autosave failed: {err}"),
    }

    for (_, player) in players.iter() {
        match snapshot_player(player) {
            Ok(record) => persistence.stage_player(player.uuid, record),
            Err(err) => tracing::error!("failed to snapshot player {}: {err}", player.uuid),
        }
    }
    if let Err(err) = persistence.flush_players() {
        tracing::error!("player autosave failed: {err}");
    }
    persistence.stage_meta(world_meta(info, clock, tick));
    if let Err(err) = persistence.flush_meta() {
        tracing::error!("meta autosave failed: {err}");
    }
}

#[allow(clippy::too_many_arguments)]
pub fn save_everything(
    sim: &mut CellWorld,
    regions: &mut RegionMap,
    bodies: &PixelBodies,
    players: &Players,
    persistence: &mut Persistence,
    info: &WorldInfo,
    clock: &Calendar,
) {
    let started = std::time::Instant::now();

    mark_changed_regions(sim, regions);
    let to_save = collect_dirty_saves(sim, regions, bodies);
    let saved_regions: Vec<_> = to_save.iter().map(|(pos, _)| *pos).collect();
    persistence.stage_regions(to_save);
    let region_count = match persistence.flush_regions() {
        Ok(count) => {
            mark_saved(regions, saved_regions);
            count
        }
        Err(err) => {
            tracing::error!("final save failed: {err}");
            0
        }
    };

    for (_, player) in players.iter() {
        match snapshot_player(player) {
            Ok(record) => persistence.stage_player(player.uuid, record),
            Err(err) => tracing::error!("failed to snapshot player {}: {err}", player.uuid),
        }
    }
    let player_count = match persistence.flush_players() {
        Ok(count) => count,
        Err(err) => {
            tracing::error!("final player save failed: {err}");
            0
        }
    };
    persistence.stage_meta(world_meta(info, clock, sim.tick()));
    if let Err(err) = persistence.flush_meta() {
        tracing::error!("final meta save failed: {err}");
    }
    tracing::info!(
        "world saved: {} regions, {} players in {:.1?}",
        region_count,
        player_count,
        started.elapsed(),
    );
}

fn world_meta(info: &WorldInfo, clock: &Calendar, tick: u64) -> WorldMeta {
    WorldMeta {
        format_version: WORLD_FORMAT_VERSION,
        seed: info.seed,
        name: info.name.clone(),
        world_age: clock.age,
        tick,
    }
}

fn parse_meta(bytes: &[u8]) -> Result<WorldMeta, StoreError> {
    let (version, _) = postcard::take_from_bytes::<u16>(bytes)?;
    if version != WORLD_FORMAT_VERSION {
        return Err(StoreError::UnsupportedWorld(version));
    }
    Ok(postcard::from_bytes(bytes)?)
}

pub fn encode_region(region: &Region, bodies: &[BodyRecord]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(REGION_RAW_BYTES);
    for chunk in region.chunks() {
        for &cell in chunk.cells() {
            let cell = if cell.is_body() { Cell::AIR } else { cell };
            raw.extend_from_slice(&cell.material.0.to_le_bytes());
            raw.extend_from_slice(&cell.vx.to_le_bytes());
            raw.extend_from_slice(&cell.vy.to_le_bytes());
            raw.push(cell.shade_flags);
            raw.push(cell.updated);
        }
    }
    for chunk in region.chunks() {
        let rect = chunk.sim_rect();
        raw.extend_from_slice(&[rect.min_x, rect.min_y, rect.max_x, rect.max_y]);
    }
    raw.extend_from_slice(&postcard::to_allocvec(bodies).expect("record serialization"));
    let cell_blob = lz4_flex::compress_prepend_size(&raw);
    let mut blob = Vec::with_capacity(cell_blob.len() + 1);
    blob.push(REGION_FORMAT_VERSION);
    blob.extend_from_slice(&cell_blob);
    blob
}

fn decode_rect(bytes: &[u8]) -> DirtyRect {
    let rect = DirtyRect::new(bytes[0], bytes[1], bytes[2], bytes[3]);
    if rect.is_empty() {
        return DirtyRect::EMPTY;
    }
    let max = (CHUNK_SIZE - 1) as u8;
    DirtyRect::new(
        rect.min_x.min(max),
        rect.min_y.min(max),
        rect.max_x.min(max),
        rect.max_y.min(max),
    )
}

pub fn decode_region(blob: &[u8]) -> Result<(Region, Vec<BodyRecord>), StoreError> {
    let (&version, compressed) = blob
        .split_first()
        .ok_or_else(|| StoreError::CorruptRegion("empty blob".into()))?;
    if version != REGION_FORMAT_VERSION {
        return Err(StoreError::UnsupportedRegion(version));
    }
    let raw = lz4_flex::decompress_size_prepended(compressed)
        .map_err(|err| StoreError::CorruptRegion(err.to_string()))?;
    if raw.len() < REGION_RAW_BYTES {
        return Err(StoreError::CorruptRegion(format!(
            "expected at least {REGION_RAW_BYTES} bytes, got {}",
            raw.len()
        )));
    }
    let bodies: Vec<BodyRecord> = postcard::from_bytes(&raw[REGION_RAW_BYTES..])?;
    for (body_index, body) in bodies.iter().enumerate() {
        if let Some((material, _)) = body
            .cells
            .iter()
            .find(|(material, _)| *material as usize >= content::MATERIAL_COUNT)
        {
            return Err(StoreError::CorruptRegion(format!(
                "body {body_index} has invalid material id {material}"
            )));
        }
    }
    let mut region = Region::new();
    let mut cursor = raw[..REGION_CELL_BYTES].chunks_exact(CELL_BYTES);
    for chunk in region.chunks_mut().iter_mut() {
        for cell in chunk.cells_mut() {
            let raw_cell = cursor.next().expect("length checked");
            let material = u16::from_le_bytes([raw_cell[0], raw_cell[1]]);
            if material as usize >= fallingsand_core::content::MATERIAL_COUNT {
                return Err(StoreError::CorruptRegion(format!(
                    "invalid material id {material}"
                )));
            }
            *cell = Cell {
                material: MaterialId(material),
                vx: i16::from_le_bytes([raw_cell[2], raw_cell[3]]),
                vy: i16::from_le_bytes([raw_cell[4], raw_cell[5]]),
                shade_flags: raw_cell[6],
                updated: raw_cell[7],
            };
        }
    }
    let rects = raw[REGION_CELL_BYTES..REGION_RAW_BYTES].chunks_exact(RECT_BYTES);
    for (chunk, bytes) in region.chunks_mut().iter_mut().zip(rects) {
        chunk.sim = decode_rect(bytes);
    }
    Ok((region, bodies))
}
