use super::region_codec::{decode_region, encode_region};
use super::{META, PLAYERS, REGIONS, SaveBatch, StoreError, StoredRegion, WorldMeta, parse_meta};
use crate::persistence::player_record::PlayerRecord;
use fallingsand_core::RegionPos;
use fallingsand_protocol::PlayerUuid;
use redb::{Database, ReadableDatabase};
use std::path::Path;

pub(super) struct WorldStore {
    db: Database,
}

impl WorldStore {
    pub(super) fn open(path: &Path) -> Result<Self, StoreError> {
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

    pub(super) fn load_meta(&self) -> Result<Option<WorldMeta>, StoreError> {
        let read = self.db.begin_read()?;
        let table = read.open_table(META)?;
        let Some(guard) = table.get("world")? else {
            return Ok(None);
        };
        parse_meta(guard.value()).map(Some)
    }

    pub(super) fn load_region(&self, pos: RegionPos) -> Result<Option<StoredRegion>, StoreError> {
        let read = self.db.begin_read()?;
        let table = read.open_table(REGIONS)?;
        let Some(guard) = table.get(pos.zorder_key())? else {
            return Ok(None);
        };
        decode_region(guard.value()).map(Some)
    }

    pub(super) fn load_player(&self, uuid: PlayerUuid) -> Result<Option<PlayerRecord>, StoreError> {
        let read = self.db.begin_read()?;
        let table = read.open_table(PLAYERS)?;
        let Some(guard) = table.get(uuid.0)? else {
            return Ok(None);
        };
        Ok(Some(postcard::from_bytes(guard.value())?))
    }

    pub(super) fn save_batch(&self, batch: &SaveBatch) -> Result<(), StoreError> {
        let regions = batch
            .regions
            .iter()
            .map(|(pos, region)| Ok((*pos, encode_region(region)?)))
            .collect::<Result<Vec<_>, StoreError>>()?;
        let players = batch
            .players
            .iter()
            .map(|(uuid, record)| Ok((*uuid, postcard::to_allocvec(record)?)))
            .collect::<Result<Vec<_>, StoreError>>()?;
        let meta = batch.meta.as_ref().map(postcard::to_allocvec).transpose()?;
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(REGIONS)?;
            for (pos, blob) in &regions {
                table.insert(pos.zorder_key(), blob.as_slice())?;
            }
        }
        {
            let mut table = write.open_table(PLAYERS)?;
            for (uuid, bytes) in &players {
                table.insert(uuid.0, bytes.as_slice())?;
            }
        }
        if let Some(bytes) = &meta {
            let mut table = write.open_table(META)?;
            table.insert("world", bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }
}
