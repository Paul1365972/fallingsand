use fallingsand_core::{CellPos, MaterialId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldEdit {
    SetCell {
        pos: CellPos,
        material: MaterialId,
    },
    FillRect {
        min: CellPos,
        max: CellPos,
        material: MaterialId,
    },
}
