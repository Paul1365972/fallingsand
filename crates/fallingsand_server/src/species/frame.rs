use fallingsand_core::{CellPos, MaterialId};

pub struct Frame {
    pub art: &'static str,
    pub legend: &'static [(u8, MaterialId, u8)],
}

impl Frame {
    fn rows(&self) -> impl Iterator<Item = &'static str> {
        self.art
            .lines()
            .map(str::trim)
            .filter(|row| !row.is_empty())
    }

    pub fn width(&self) -> i32 {
        self.rows().next().map_or(0, str::len) as i32
    }

    pub fn height(&self) -> i32 {
        self.rows().count() as i32
    }

    pub fn at(&self, dx: i32, dy: i32, facing_left: bool) -> Option<(MaterialId, u8)> {
        let row = self.height() - 1 - dy;
        let col = if facing_left {
            self.width() - 1 - dx
        } else {
            dx
        };
        if row < 0 || col < 0 {
            return None;
        }
        let mark = *self
            .rows()
            .nth(row as usize)?
            .as_bytes()
            .get(col as usize)?;
        self.legend
            .iter()
            .find(|&&(key, _, _)| key == mark)
            .map(|&(_, material, shade)| (material, shade))
    }

    pub fn shade(&self, dx: i32, dy: i32, facing_left: bool) -> u8 {
        self.at(dx, dy, facing_left).map_or(0, |(_, shade)| shade)
    }

    pub fn cells(&self, base: CellPos, facing_left: bool) -> Vec<(CellPos, MaterialId, u8)> {
        let mut cells = Vec::with_capacity((self.width() * self.height()) as usize);
        for dy in 0..self.height() {
            for dx in 0..self.width() {
                if let Some((material, shade)) = self.at(dx, dy, facing_left) {
                    cells.push((base.translated(dx, dy), material, shade));
                }
            }
        }
        cells
    }
}
