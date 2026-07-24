use super::rotation::{Spin, quantize_step, rotate_offset};
use fallingsand_core::{CellPos, MaterialId, Subcell, content};
use fallingsand_math::{SUBCELL_UNITS_PER_CELL, round_div};

const UNITS: i128 = SUBCELL_UNITS_PER_CELL as i128;

pub const fn material_mass(material: MaterialId) -> i64 {
    let mass = content::density_milli(material) as i64 / 1000;
    if mass > 0 { mass } else { 1 }
}

#[derive(Clone, Copy)]
pub(super) struct Slot {
    pub local: (i32, i32),
    pub material: MaterialId,
}

impl Slot {
    pub(super) fn mass(self) -> i64 {
        material_mass(self.material)
    }

    pub(super) fn bonded(self) -> bool {
        content::bond_group(self.material) != u8::MAX
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Pose {
    pub x: Subcell,
    pub y: Subcell,
    pub angle: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct Motion {
    pub x: Subcell,
    pub y: Subcell,
    pub spin: Spin,
}

impl Motion {
    pub(super) fn is_still(self) -> bool {
        self == Self::default()
    }

    pub(super) fn part(self, of: i64, over: i64) -> Self {
        Self {
            x: Subcell::from_raw(round_div(
                i128::from(self.x.raw()) * i128::from(of),
                i128::from(over),
            ) as i64),
            y: Subcell::from_raw(round_div(
                i128::from(self.y.raw()) * i128::from(of),
                i128::from(over),
            ) as i64),
            spin: self.spin.part(of, over),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Vector {
    pub x: i64,
    pub y: i64,
}

impl Vector {
    pub(super) const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    pub(super) fn of_cell(pos: CellPos) -> Self {
        Self::new(
            Subcell::cell_center(pos.x).raw(),
            Subcell::cell_center(pos.y).raw(),
        )
    }

    pub(super) fn midpoint(self, other: Self) -> Self {
        Self::new((self.x + other.x) / 2, (self.y + other.y) / 2)
    }

    pub(super) fn from(self, origin: Self) -> Self {
        Self::new(self.x - origin.x, self.y - origin.y)
    }

    pub(super) fn dot(self, other: Self) -> i128 {
        i128::from(self.x) * i128::from(other.x) + i128::from(self.y) * i128::from(other.y)
    }

    pub(super) fn cross(self, other: Self) -> i128 {
        i128::from(self.x) * i128::from(other.y) - i128::from(self.y) * i128::from(other.x)
    }

    pub(super) fn perpendicular(self) -> Self {
        Self::new(-self.y, self.x)
    }
}

pub(super) struct Frame {
    pub mass: i64,
    pub moment: i128,
    pub radius: i64,
    pub restitution: u32,
}

pub(super) fn frame(slots: &[Slot]) -> Frame {
    let mut mass = 0i128;
    let mut weighted = (0i128, 0i128);
    let mut restitution = 0;
    for slot in slots {
        let cell = i128::from(slot.mass());
        mass += cell;
        weighted.0 += cell * i128::from(slot.local.0);
        weighted.1 += cell * i128::from(slot.local.1);
        restitution = restitution.max(content::restitution_q16(slot.material));
    }
    let center = (
        round_div(weighted.0 * UNITS, mass),
        round_div(weighted.1 * UNITS, mass),
    );
    let mut moment = 0i128;
    let mut reach = 0i128;
    for slot in slots {
        let dx = i128::from(slot.local.0) * UNITS - center.0;
        let dy = i128::from(slot.local.1) * UNITS - center.1;
        moment += i128::from(slot.mass()) * (dx * dx + dy * dy);
        reach = reach.max(dx * dx + dy * dy);
    }
    Frame {
        mass: mass as i64,
        moment,
        radius: reach.isqrt() as i64 / i64::from(SUBCELL_UNITS_PER_CELL) + 1,
        restitution,
    }
}

pub(super) fn rotated_mean(slots: &[Slot], mass: i64, angle: i64) -> (i64, i64) {
    let step = quantize_step(angle);
    let mut weighted = (0i128, 0i128);
    for slot in slots {
        let (dx, dy) = rotate_offset(step, slot.local.0, slot.local.1);
        let cell = i128::from(slot.mass());
        weighted.0 += cell * i128::from(dx);
        weighted.1 += cell * i128::from(dy);
    }
    let mass = i128::from(mass);
    (
        round_div(weighted.0 * UNITS, mass) as i64,
        round_div(weighted.1 * UNITS, mass) as i64,
    )
}

pub(super) fn rasterize(slots: &[Slot], mass: i64, pose: Pose, out: &mut Vec<CellPos>) {
    let mean = rotated_mean(slots, mass, pose.angle);
    let pivot = CellPos::new(
        Subcell::from_raw(pose.x.raw() - mean.0).floor_cell(),
        Subcell::from_raw(pose.y.raw() - mean.1).floor_cell(),
    );
    let step = quantize_step(pose.angle);
    out.clear();
    out.extend(slots.iter().map(|slot| {
        let (dx, dy) = rotate_offset(step, slot.local.0, slot.local.1);
        pivot.translated(dx, dy)
    }));
}

pub(super) fn center_of(raster: &[CellPos], slots: &[Slot], mass: i64) -> (Subcell, Subcell) {
    let mut weighted = (0i128, 0i128);
    for (slot, pos) in slots.iter().zip(raster) {
        let cell = i128::from(slot.mass());
        weighted.0 += cell * i128::from(Subcell::cell_center(pos.x).raw());
        weighted.1 += cell * i128::from(Subcell::cell_center(pos.y).raw());
    }
    let mass = i128::from(mass);
    (
        Subcell::from_raw(round_div(weighted.0, mass) as i64),
        Subcell::from_raw(round_div(weighted.1, mass) as i64),
    )
}
