use crate::chunk::{Field, Chunk};

struct SimulationCell<'a, T, const N: usize> {
    chunks: [[&'a mut Chunk<T, N>; 4]; 4],
}


impl<T, const N: usize> Field<T, N> {
    fn build_sim_cells(&mut self) {
        let 
        let mut region_chunks: [[&mut Chunk<T, N>; 4]; 4] = [
            [],[],[],[],
            ];
        for (i, row) in grid.iter_mut().enumerate() {
            for (j, col) in row.iter_mut().enumerate() {
                col = 1;
            }
        }
        self.sections.get
    }

    fn step() {

    }
}
