use fallingsand_core::{
    CHUNK_AREA, CHUNK_SIZE, Cell, ChunkOffset, DirtyRect, Fixed, Inventory as CoreInventory,
    ItemId, ItemRegistry, ItemStack, MaterialId, PLAYER_SLOTS, REGION_AREA_CHUNKS, Region,
    RegionPos,
};
use fallingsand_protocol::{GameMode, PlayerUuid};
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const REGION_FORMAT_VERSION: u8 = 7;
pub const WORLD_FORMAT_VERSION: u16 = 10;
const CELL_BYTES: usize = 7;
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
    pub age: u64,
    pub tick: u64,
}

pub type SlotRecord = Option<(String, u32)>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerRecord {
    pub x: Fixed,
    pub y: Fixed,
    pub hp: f32,
    pub mode: GameMode,
    pub air: f32,
    pub burning: f32,
    pub inventory: Vec<SlotRecord>,
    pub cursor: SlotRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DroppedRecord {
    pub x: Fixed,
    pub y: Fixed,
    pub vx: f32,
    pub vy: f32,
    pub item: String,
    pub count: u32,
    pub age_ticks: u64,
    pub pickup_delay: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RegionExtras {
    pub items: Vec<DroppedRecord>,
}

pub fn stack_to_record(reg: &ItemRegistry, stack: Option<ItemStack>) -> SlotRecord {
    let stack = stack.filter(|s| s.count > 0)?;
    let name = reg.try_get(stack.item)?.name.clone();
    Some((name, stack.count))
}

pub fn stack_from_record(reg: &ItemRegistry, record: &SlotRecord) -> Option<ItemStack> {
    let (name, count) = record.as_ref()?;
    if *count == 0 {
        return None;
    }
    match reg.id_of(name) {
        Some(id) if id != ItemId::NONE => Some(ItemStack::new(id, *count)),
        _ => {
            tracing::warn!("dropping {count} of unknown item {name:?}");
            None
        }
    }
}

pub fn slots_to_record(reg: &ItemRegistry, inv: &CoreInventory) -> Vec<SlotRecord> {
    inv.slots
        .iter()
        .map(|slot| stack_to_record(reg, *slot))
        .collect()
}

pub fn player_slots_from_record(reg: &ItemRegistry, list: &[SlotRecord]) -> CoreInventory {
    let mut inv = CoreInventory::with_capacity(PLAYER_SLOTS);
    for (slot, record) in inv.slots.iter_mut().zip(list) {
        *slot = stack_from_record(reg, record);
    }
    inv
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redb: {0}")]
    Redb(#[from] redb::Error),
    #[error("corrupt region blob: {0}")]
    CorruptRegion(String),
    #[error("corrupt record: {0}")]
    CorruptRecord(#[from] postcard::Error),
    #[error(
        "world format {0} is too old (current {WORLD_FORMAT_VERSION}); pre-release worlds carry no migrations — delete the world and create a new one"
    )]
    UnsupportedWorld(u16),
    #[error("unsupported region format {0} (server supports {REGION_FORMAT_VERSION})")]
    UnsupportedRegion(u8),
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

pub struct WorldStore {
    db: Database,
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
    ) -> Result<Option<(Region, RegionExtras)>, StoreError> {
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

fn parse_meta(bytes: &[u8]) -> Result<WorldMeta, StoreError> {
    let (version, _) = postcard::take_from_bytes::<u16>(bytes)?;
    if version != WORLD_FORMAT_VERSION {
        return Err(StoreError::UnsupportedWorld(version));
    }
    Ok(postcard::from_bytes(bytes)?)
}

pub fn encode_region(region: &Region, extras: &RegionExtras) -> Vec<u8> {
    let mut raw = Vec::with_capacity(REGION_RAW_BYTES);
    for chunk in region.chunks() {
        for cell in chunk.cells() {
            raw.extend_from_slice(&cell.material.0.to_le_bytes());
            raw.extend_from_slice(&cell.vx.to_le_bytes());
            raw.extend_from_slice(&cell.vy.to_le_bytes());
            raw.push(cell.shade_flags);
        }
    }
    for chunk in region.chunks() {
        let rect = chunk.sim_dirty();
        raw.extend_from_slice(&[rect.min_x, rect.min_y, rect.max_x, rect.max_y]);
    }
    let cell_blob = lz4_flex::compress_prepend_size(&raw);
    let extras_blob = postcard::to_allocvec(extras).expect("extras serialize");
    let mut blob = Vec::with_capacity(cell_blob.len() + extras_blob.len() + 8);
    blob.push(REGION_FORMAT_VERSION);
    blob.extend_from_slice(&(cell_blob.len() as u32).to_le_bytes());
    blob.extend_from_slice(&cell_blob);
    blob.extend_from_slice(&extras_blob);
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

pub fn decode_region(blob: &[u8]) -> Result<(Region, RegionExtras), StoreError> {
    let (&version, rest) = blob
        .split_first()
        .ok_or_else(|| StoreError::CorruptRegion("empty blob".into()))?;
    if version != REGION_FORMAT_VERSION {
        return Err(StoreError::UnsupportedRegion(version));
    }
    if rest.len() < 4 {
        return Err(StoreError::CorruptRegion("missing length header".into()));
    }
    let cell_len = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
    let body = &rest[4..];
    if body.len() < cell_len {
        return Err(StoreError::CorruptRegion("truncated cell blob".into()));
    }
    let (compressed, extras_blob) = body.split_at(cell_len);
    let raw = lz4_flex::decompress_size_prepended(compressed)
        .map_err(|err| StoreError::CorruptRegion(err.to_string()))?;
    if raw.len() != REGION_RAW_BYTES {
        return Err(StoreError::CorruptRegion(format!(
            "expected {REGION_RAW_BYTES} bytes, got {}",
            raw.len()
        )));
    }
    let mut region = Region::new();
    let mut cursor = raw[..REGION_CELL_BYTES].chunks_exact(CELL_BYTES);
    for chunk_index in 0..REGION_AREA_CHUNKS {
        let chunk = region.chunk_mut(ChunkOffset::from_index(chunk_index));
        for cell in chunk.cells_mut() {
            let raw_cell = cursor.next().expect("length checked");
            *cell = Cell {
                material: MaterialId(u16::from_le_bytes([raw_cell[0], raw_cell[1]])),
                vx: i16::from_le_bytes([raw_cell[2], raw_cell[3]]),
                vy: i16::from_le_bytes([raw_cell[4], raw_cell[5]]),
                shade_flags: raw_cell[6],
                updated: 0,
            };
        }
    }
    let rects = raw[REGION_CELL_BYTES..].chunks_exact(RECT_BYTES);
    for (chunk, bytes) in region.chunks_mut().iter_mut().zip(rects) {
        chunk.keep_bounds = decode_rect(bytes);
    }
    let extras = if extras_blob.is_empty() {
        RegionExtras::default()
    } else {
        postcard::from_bytes(extras_blob)?
    };
    Ok((region, extras))
}
