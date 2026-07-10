use crate::chunk::Chunk;
use crate::coords::ChunkOffset;

pub const REGION_SIZE_CHUNKS: usize = 8;
pub const REGION_AREA_CHUNKS: usize = REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS;
pub const REGION_SIZE_CELLS: usize = REGION_SIZE_CHUNKS * crate::chunk::CHUNK_SIZE;

#[derive(Debug, Clone)]
pub struct Region {
    chunks: Box<[Chunk; REGION_AREA_CHUNKS]>,
}

impl Default for Region {
    fn default() -> Self {
        Self::new()
    }
}

impl Region {
    pub fn new() -> Self {
        Self {
            chunks: Box::new(std::array::from_fn(|_| Chunk::new())),
        }
    }

    #[inline]
    pub fn chunk_mut(&mut self, offset: ChunkOffset) -> &mut Chunk {
        &mut self.chunks[offset.index()]
    }

    pub fn chunks(&self) -> &[Chunk; REGION_AREA_CHUNKS] {
        &self.chunks
    }

    pub fn chunks_mut(&mut self) -> &mut [Chunk; REGION_AREA_CHUNKS] {
        &mut self.chunks
    }

    pub fn into_chunks(self) -> Box<[Chunk; REGION_AREA_CHUNKS]> {
        self.chunks
    }
}
