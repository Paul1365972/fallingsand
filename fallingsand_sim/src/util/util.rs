use itertools::Itertools;
use rayon::prelude::ParallelIterator;
use rustc_hash::FxHashMap;

use crate::{region::DisjointRegion, util::coords::WorldChunkCoords};

pub trait DrainFilterMap<T> {
    fn drain_filter_map<E, R, P, F, U>(&mut self, extractor: E, filter: P, mapper: F) -> Vec<U>
    where
        E: Fn(&mut T) -> R,
        P: Fn(&mut T, &R) -> bool,
        F: Fn(T, R) -> U;
}

impl<T> DrainFilterMap<T> for Vec<T> {
    fn drain_filter_map<E, R, P, F, U>(&mut self, extractor: E, filter: P, mapper: F) -> Vec<U>
    where
        E: Fn(&mut T) -> R,
        P: Fn(&mut T, &R) -> bool,
        F: Fn(T, R) -> U,
    {
        let mut result = Vec::new();
        let mut index = 0;
        let mut len = self.len();
        while index < len {
            let ele = self.get_mut(index).unwrap();
            let extract = extractor(ele);
            if filter(ele, &extract) {
                result.push(mapper(self.remove(index), extract));
                len -= 1;
            } else {
                index += 1;
            }
        }
        result
    }
}

#[derive(Debug)]
pub struct ActiveChunks {
    chunks: Vec<WorldChunkCoords>,
}

impl ActiveChunks {
    fn new(chunks: Vec<WorldChunkCoords>) -> Self {
        Self { chunks }
    }

    fn get_chunks(&self) -> &Vec<WorldChunkCoords> {
        &self.chunks
    }
}

impl DisjointRegion {
    pub fn build_active_chunks(&self) -> ActiveChunks {
        let mut chunks = Vec::new();
        for offset in [(0, 0), (0, 2), (2, 0), (2, 2)] {
            let mut cell_map = FxHashMap::default();
            self.for_chunk_coords(|k| {
                let coords = k.to_world_cell_coords(offset);
                let chunks = cell_map.entry(coords).or_insert(0);
                *chunks += 1;
            });

            let chunks_coords = cell_map
                .into_iter()
                .filter(|x| x.1 == 16)
                .map(|((x, y), _)| (x * 4 + offset.0, y * 4 + offset.1))
                .flat_map(|(x, y)| {
                    [
                        (x + 1, y + 1),
                        (x + 1, y + 2),
                        (x + 2, y + 1),
                        (x + 2, y + 2),
                    ]
                })
                .map(|(x, y)| WorldChunkCoords::new(x, y));
            chunks.extend(chunks_coords)
        }
        chunks.sort();
        ActiveChunks::new(chunks)
    }
}
