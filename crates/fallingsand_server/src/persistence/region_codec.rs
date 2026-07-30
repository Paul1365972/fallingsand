use super::{REGION_FORMAT_VERSION, StoreError};
use fallingsand_core::{
    CHUNK_AREA, Cell, DirtyRect, FOG_CHUNK_BYTES, FogMask, MaterialId, REGION_AREA_CHUNKS, Region,
    Tag, content,
};
use serde::{Deserialize, Serialize};

const MAX_RAW_REGION_BYTES: usize = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct CellRecord {
    material: u16,
    vx: i16,
    vy: i16,
    shade: u8,
}

impl From<Cell> for CellRecord {
    fn from(mut cell: Cell) -> Self {
        if cell.body_id().is_some() {
            if content::tags(cell.material).contains(Tag::Body) {
                cell = Cell::AIR;
            } else {
                cell.clear_body();
            }
        }
        let (vx, vy) = cell.vel();
        Self {
            material: cell.material.0,
            vx: vx as i16,
            vy: vy as i16,
            shade: cell.shade,
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
        let mut cell = Cell::new(MaterialId(self.material), 0);
        cell.shade = self.shade;
        cell.set_vel(self.vx as i32, self.vy as i32);
        Ok(cell)
    }
}

#[derive(Serialize, Deserialize)]
struct ChunkRecord {
    cells: Vec<CellRecord>,
    fog: Vec<u8>,
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
            Ok(ChunkRecord {
                cells,
                fog: chunk.fog().bytes().to_vec(),
            })
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
    let (raw_len, body) = lz4_flex::block::uncompressed_size(compressed)
        .map_err(|err| StoreError::CorruptRegion(err.to_string()))?;
    if raw_len > MAX_RAW_REGION_BYTES {
        return Err(StoreError::CorruptRegion(format!(
            "decompressed size {raw_len} exceeds {MAX_RAW_REGION_BYTES}"
        )));
    }
    let raw = lz4_flex::decompress(body, raw_len)
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
        let fog: [u8; FOG_CHUNK_BYTES] = stored.fog.as_slice().try_into().map_err(|_| {
            StoreError::CorruptRegion(format!(
                "expected {FOG_CHUNK_BYTES} fog bytes per chunk, got {}",
                stored.fog.len()
            ))
        })?;
        chunk.restore_fog(FogMask::from_bytes(fog));
        chunk.sim = DirtyRect::FULL;
    }
    Ok(region)
}
