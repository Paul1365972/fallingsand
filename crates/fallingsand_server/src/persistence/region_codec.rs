use super::{REGION_FORMAT_VERSION, StoreError, StoredRegion};
use fallingsand_core::{
    CHUNK_AREA, Cell, CellPos, DirtyRect, MaterialId, REGION_AREA_CHUNKS, Region, Subcell, Tag,
    content,
};
use fallingsand_sim::bodies::BodyPose;
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
    poses: Vec<BodyPoseRecord>,
}

#[derive(Serialize, Deserialize)]
struct BodyPoseRecord {
    pivot: CellPos,
    x: Subcell,
    y: Subcell,
    angle: f32,
    angle_steps: u32,
}

impl From<BodyPose> for BodyPoseRecord {
    fn from(pose: BodyPose) -> Self {
        Self {
            pivot: pose.pivot,
            x: pose.x,
            y: pose.y,
            angle: pose.angle,
            angle_steps: pose.angle_steps,
        }
    }
}

impl BodyPoseRecord {
    fn restore(&self) -> Result<BodyPose, StoreError> {
        if !self.angle.is_finite() || !matches!(self.angle_steps, 64 | 128) {
            return Err(StoreError::CorruptRegion("invalid body pose".into()));
        }
        let pivot_x = Subcell::cell_center(self.pivot.x);
        let pivot_y = Subcell::cell_center(self.pivot.y);
        let reach = Subcell::from_cell(64);
        if (self.x - pivot_x).abs() > reach || (self.y - pivot_y).abs() > reach {
            return Err(StoreError::CorruptRegion(
                "body pose escapes its pivot".into(),
            ));
        }
        Ok(BodyPose {
            pivot: self.pivot,
            x: self.x,
            y: self.y,
            angle: self.angle.rem_euclid(std::f32::consts::TAU),
            angle_steps: self.angle_steps,
        })
    }
}

pub(super) fn encode_region(stored: &StoredRegion) -> Result<Vec<u8>, StoreError> {
    let chunks = stored
        .region
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
    let poses = stored.poses.iter().copied().map(Into::into).collect();
    let record = RegionRecord { chunks, poses };
    let compressed = lz4_flex::compress_prepend_size(&postcard::to_allocvec(&record)?);
    let mut blob = Vec::with_capacity(compressed.len() + 1);
    blob.push(REGION_FORMAT_VERSION);
    blob.extend_from_slice(&compressed);
    Ok(blob)
}

pub(super) fn decode_region(blob: &[u8]) -> Result<StoredRegion, StoreError> {
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
    let mut poses = record
        .poses
        .iter()
        .map(BodyPoseRecord::restore)
        .collect::<Result<Vec<_>, _>>()?;
    poses.sort_by_key(|pose| (pose.pivot.y, pose.pivot.x));
    let pose_count = poses.len();
    poses.dedup_by_key(|pose| pose.pivot);
    if poses.len() != pose_count {
        tracing::warn!(
            duplicate_poses = pose_count - poses.len(),
            "discarded duplicate body pose metadata"
        );
    }
    Ok(StoredRegion { region, poses })
}
