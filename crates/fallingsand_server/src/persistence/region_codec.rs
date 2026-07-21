use super::{REGION_FORMAT_VERSION, StoreError};
use fallingsand_core::{
    CHUNK_AREA, Cell, DirtyRect, MaterialId, REGION_AREA_CHUNKS, Region, Tag, content,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct CellRecord {
    material: u16,
    vx: i16,
    vy: i16,
    shade: u8,
    aux: u8,
}

impl From<Cell> for CellRecord {
    fn from(cell: Cell) -> Self {
        let cell = if cell.is_body() && content::tags(cell.material).contains(Tag::Player) {
            Cell::AIR
        } else {
            cell
        };
        Self {
            material: cell.material.0,
            vx: cell.vx,
            vy: cell.vy,
            shade: cell.shade,
            aux: cell.aux,
        }
    }
}

impl CellRecord {
    fn restore(&self) -> Result<Cell, StoreError> {
        if self.material as usize >= content::MATERIAL_COUNT {
            return Err(StoreError::CorruptRegion(format!(
                "invalid material id {}",
                self.material
            )));
        }
        Ok(Cell {
            material: MaterialId(self.material),
            vx: self.vx,
            vy: self.vy,
            shade: self.shade,
            flags: 0,
            aux: self.aux,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct ChunkRecord {
    cells: Vec<CellRecord>,
}

#[derive(Serialize, Deserialize)]
struct RegionRecord {
    chunks: Vec<ChunkRecord>,
}

pub(super) fn encode_region(region: &Region) -> Result<Vec<u8>, StoreError> {
    let chunks = region
        .chunks()
        .iter()
        .map(|chunk| {
            let cells: Vec<CellRecord> = chunk.cells().iter().map(|&cell| cell.into()).collect();
            for record in &cells {
                if record.material as usize >= content::MATERIAL_COUNT {
                    return Err(StoreError::CorruptRegion(format!(
                        "runtime cell has invalid material id {}",
                        record.material
                    )));
                }
            }
            Ok(ChunkRecord { cells })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let record = RegionRecord { chunks };
    let compressed = lz4_flex::compress_prepend_size(&postcard::to_allocvec(&record)?);
    let mut blob = Vec::with_capacity(compressed.len() + 1);
    blob.push(REGION_FORMAT_VERSION);
    blob.extend_from_slice(&compressed);
    Ok(blob)
}

pub(super) fn decode_region(blob: &[u8]) -> Result<Region, StoreError> {
    let (&version, compressed) = blob
        .split_first()
        .ok_or_else(|| StoreError::CorruptRegion("empty blob".into()))?;
    if version != REGION_FORMAT_VERSION {
        return Err(StoreError::UnsupportedRegion(version));
    }
    let raw = lz4_flex::decompress_size_prepended(compressed)
        .map_err(|err| StoreError::CorruptRegion(err.to_string()))?;
    let record: RegionRecord = postcard::from_bytes(&raw)?;
    if record.chunks.len() != REGION_AREA_CHUNKS {
        return Err(StoreError::CorruptRegion(format!(
            "expected {REGION_AREA_CHUNKS} chunks, got {}",
            record.chunks.len()
        )));
    }
    let mut region = Region::new();
    for (chunk, stored) in region.chunks_mut().iter_mut().zip(&record.chunks) {
        if stored.cells.len() != CHUNK_AREA {
            return Err(StoreError::CorruptRegion(format!(
                "expected {CHUNK_AREA} cells per chunk, got {}",
                stored.cells.len()
            )));
        }
        for (cell, stored_cell) in chunk.cells_mut().iter_mut().zip(&stored.cells) {
            *cell = stored_cell.restore()?;
        }
        chunk.sim = DirtyRect::FULL;
    }
    Ok(region)
}
