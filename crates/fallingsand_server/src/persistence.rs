use fallingsand_core::{
    CHUNK_AREA, Cell, ChunkOffset, MaterialId, REGION_AREA_CHUNKS, Region, RegionPos,
};
use fallingsand_protocol::PlayerUuid;
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const REGION_FORMAT_VERSION: u8 = 1;
pub const WORLD_FORMAT_VERSION: u16 = 2;
const CELL_BYTES: usize = 3;
const REGION_RAW_BYTES: usize = REGION_AREA_CHUNKS * CHUNK_AREA * CELL_BYTES;

const REGIONS: TableDefinition<u64, &[u8]> = TableDefinition::new("regions");
const PLAYERS: TableDefinition<u128, &[u8]> = TableDefinition::new("players");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMeta {
    pub format_version: u16,
    pub seed: u64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerRecord {
    pub x: f32,
    pub y: f32,
    pub hp: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redb: {0}")]
    Redb(#[from] redb::Error),
    #[error("corrupt region blob: {0}")]
    CorruptRegion(String),
    #[error("corrupt record: {0}")]
    CorruptRecord(#[from] postcard::Error),
    #[error("unsupported world format {0} (server supports {WORLD_FORMAT_VERSION})")]
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
                        let meta: WorldMeta = postcard::from_bytes(guard.value())?;
                        if meta.format_version != WORLD_FORMAT_VERSION {
                            return Err(StoreError::UnsupportedWorld(meta.format_version));
                        }
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
        let meta: WorldMeta = postcard::from_bytes(guard.value())?;
        if meta.format_version != WORLD_FORMAT_VERSION {
            return Err(StoreError::UnsupportedWorld(meta.format_version));
        }
        Ok(Some(meta))
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

    pub fn load_region(&self, pos: RegionPos) -> Result<Option<Region>, StoreError> {
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

pub fn encode_region(region: &Region) -> Vec<u8> {
    let mut raw = Vec::with_capacity(REGION_RAW_BYTES);
    for chunk in region.chunks() {
        for cell in chunk.cells() {
            raw.extend_from_slice(&cell.material.0.to_le_bytes());
            raw.push(cell.shade_flags);
        }
    }
    let mut blob = Vec::with_capacity(raw.len() / 8 + 16);
    blob.push(REGION_FORMAT_VERSION);
    blob.extend_from_slice(&lz4_flex::compress_prepend_size(&raw));
    blob
}

pub fn decode_region(blob: &[u8]) -> Result<Region, StoreError> {
    let (&version, compressed) = blob
        .split_first()
        .ok_or_else(|| StoreError::CorruptRegion("empty blob".into()))?;
    if version != REGION_FORMAT_VERSION {
        return Err(StoreError::UnsupportedRegion(version));
    }
    let raw = lz4_flex::decompress_size_prepended(compressed)
        .map_err(|err| StoreError::CorruptRegion(err.to_string()))?;
    if raw.len() != REGION_RAW_BYTES {
        return Err(StoreError::CorruptRegion(format!(
            "expected {REGION_RAW_BYTES} bytes, got {}",
            raw.len()
        )));
    }
    let mut region = Region::new();
    let mut cursor = raw.chunks_exact(CELL_BYTES);
    for chunk_index in 0..REGION_AREA_CHUNKS {
        let chunk = region.chunk_mut(ChunkOffset::from_index(chunk_index));
        for cell in chunk.cells_mut() {
            let raw_cell = cursor.next().expect("length checked");
            *cell = Cell {
                material: MaterialId(u16::from_le_bytes([raw_cell[0], raw_cell[1]])),
                shade_flags: raw_cell[2],
                updated: 0,
            };
        }
    }
    Ok(region)
}
