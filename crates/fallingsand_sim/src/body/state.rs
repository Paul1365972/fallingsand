use super::rotation::{Spin, rotate_offset};
use crate::world::CellWorld;
use fallingsand_core::{CellPos, MaterialId, Q16, content};
use fallingsand_math::{SUBCELL_UNITS_PER_CELL, round_div};

pub(super) const CELL: i64 = SUBCELL_UNITS_PER_CELL as i64;

pub(super) fn cell_center(cell: i32) -> i64 {
    i64::from(cell) * CELL + CELL / 2
}

pub(super) fn cell_mass(material: MaterialId) -> i64 {
    i64::from(content::density_milli(material).max(1))
}

pub(super) fn bondable(material: MaterialId) -> bool {
    content::bond_group(material) != u8::MAX
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Slot {
    pub local: (i32, i32),
    pub material: MaterialId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub turns: bool,
    pub settles: bool,
    pub assists: bool,
}

impl Policy {
    pub const DEBRIS: Self = Self {
        turns: true,
        settles: true,
        assists: false,
    };
    pub const BALL: Self = Self {
        turns: true,
        settles: false,
        assists: false,
    };
    pub const MOB: Self = Self {
        turns: false,
        settles: false,
        assists: false,
    };
    pub const PLAYER: Self = Self {
        turns: false,
        settles: false,
        assists: true,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Freedoms(u8);

impl Freedoms {
    pub(super) const TURN: u8 = 1 << 0;
    pub(super) const X: u8 = 1 << 1;
    pub(super) const Y: u8 = 1 << 2;
    pub(super) const TRANSLATION: Self = Self(Self::X | Self::Y);
    pub(super) const ALL: Self = Self(Self::TURN | Self::X | Self::Y);

    pub(super) fn holds(self, bit: u8) -> bool {
        self.0 & bit != 0
    }
}

#[derive(Debug)]
pub(super) struct Body {
    pub id: u32,
    pub slots: Vec<Slot>,
    pub raster: Vec<CellPos>,
    pub anchor: CellPos,
    pub step: u32,
    pub vx: i64,
    pub vy: i64,
    pub spin: Spin,
    pub acc_x: i64,
    pub acc_y: i64,
    pub acc_turn: i64,
    pub mass: i64,
    pub moment: i128,
    pub radius: i64,
    pub restitution: Q16,
    pub friction: Q16,
    pub weight: i64,
    pub freedoms: Freedoms,
    pub settles: bool,
    pub assists: bool,
    pub was_grounded: bool,
    pub parked: bool,
}

pub(super) struct Inertia {
    pub mass: i64,
    pub moment: i128,
    pub radius: i64,
    pub restitution: Q16,
    pub friction: Q16,
}

pub(super) fn inertia(slots: &[Slot]) -> Inertia {
    let mut mass = 0i128;
    let mut weighted = (0i128, 0i128);
    let mut restitution = Q16::from_raw(0);
    let mut friction = Q16::from_raw(u32::MAX);
    for slot in slots {
        let cell = i128::from(cell_mass(slot.material));
        mass += cell;
        weighted.0 += cell * i128::from(slot.local.0);
        weighted.1 += cell * i128::from(slot.local.1);
        let material_restitution = content::restitution(slot.material);
        if material_restitution.raw() > restitution.raw() {
            restitution = material_restitution;
        }
        friction = friction.min(content::friction(slot.material));
    }
    let center = (
        round_div(weighted.0 * i128::from(CELL), mass),
        round_div(weighted.1 * i128::from(CELL), mass),
    );
    let mut moment = 0i128;
    let mut reach = 0i128;
    for slot in slots {
        let dx = i128::from(slot.local.0) * i128::from(CELL) - center.0;
        let dy = i128::from(slot.local.1) * i128::from(CELL) - center.1;
        let arm = dx * dx + dy * dy;
        moment += i128::from(cell_mass(slot.material)) * arm.max(i128::from(CELL * CELL) / 6);
        reach = reach.max(arm);
    }
    Inertia {
        mass: mass as i64,
        moment,
        radius: reach.isqrt() as i64 / CELL + 1,
        restitution,
        friction,
    }
}

pub(super) fn rotated_mean(slots: &[Slot], mass: i64, step: u32) -> (i64, i64) {
    let mut weighted = (0i128, 0i128);
    for slot in slots {
        let (dx, dy) = rotate_offset(step, slot.local.0, slot.local.1);
        let cell = i128::from(cell_mass(slot.material));
        weighted.0 += cell * i128::from(dx);
        weighted.1 += cell * i128::from(dy);
    }
    let mass = i128::from(mass);
    (
        round_div(weighted.0 * i128::from(CELL), mass) as i64,
        round_div(weighted.1 * i128::from(CELL), mass) as i64,
    )
}

pub(super) fn rasterize(slots: &[Slot], anchor: CellPos, step: u32, out: &mut Vec<CellPos>) {
    out.clear();
    out.extend(slots.iter().map(|slot| {
        let (dx, dy) = rotate_offset(step, slot.local.0, slot.local.1);
        anchor.translated(dx, dy)
    }));
}

impl Body {
    pub(super) fn com(&self) -> (i64, i64) {
        let mean = rotated_mean(&self.slots, self.mass, self.step);
        (
            cell_center(self.anchor.x) + mean.0,
            cell_center(self.anchor.y) + mean.1,
        )
    }

    pub(super) fn point_velocity(&self, com: (i64, i64), pos: CellPos) -> (i64, i64) {
        let rx = cell_center(pos.x) - com.0;
        let ry = cell_center(pos.y) - com.1;
        (
            self.vx - self.spin.speed_at(ry),
            self.vy + self.spin.speed_at(rx),
        )
    }

    pub(super) fn apply_impulse(&mut self, com: (i64, i64), pos: CellPos, jx: i64, jy: i64) {
        let rx = i128::from(cell_center(pos.x) - com.0);
        let ry = i128::from(cell_center(pos.y) - com.1);
        self.vx += round_div(i128::from(jx), i128::from(self.mass)) as i64;
        self.vy += round_div(i128::from(jy), i128::from(self.mass)) as i64;
        if self.freedoms.holds(Freedoms::TURN) {
            let torque = rx * i128::from(jy) - ry * i128::from(jx);
            self.spin += Spin::from_angular_impulse(torque, self.moment);
        }
    }

    pub(super) fn apply(&mut self, policy: Policy) {
        self.freedoms = if policy.turns {
            Freedoms::ALL
        } else {
            Freedoms::TRANSLATION
        };
        self.settles = policy.settles;
        self.assists = policy.assists;
    }

    pub(super) fn refresh_inertia(&mut self) {
        let inertia = inertia(&self.slots);
        self.mass = inertia.mass;
        self.moment = inertia.moment;
        self.radius = inertia.radius;
        self.restitution = inertia.restitution;
        self.friction = inertia.friction;
    }
}

pub(super) fn capture(world: &mut CellWorld, id: u32, cells: Vec<CellPos>) -> Body {
    let mut mass = 0i128;
    let mut weighted = (0i128, 0i128);
    for &pos in &cells {
        let material = world.get_cell(pos).expect("island is loaded").material;
        let cell = i128::from(cell_mass(material));
        mass += cell;
        weighted.0 += cell * i128::from(pos.x);
        weighted.1 += cell * i128::from(pos.y);
    }
    let anchor = CellPos::new(
        round_div(weighted.0, mass) as i32,
        round_div(weighted.1, mass) as i32,
    );
    let slots: Vec<Slot> = cells
        .iter()
        .map(|&pos| Slot {
            local: (pos.x - anchor.x, pos.y - anchor.y),
            material: world.get_cell(pos).expect("island is loaded").material,
        })
        .collect();
    for &pos in &cells {
        let mut cell = world.get_cell(pos).expect("island is loaded");
        cell.set_body(id);
        world.set(pos, cell);
    }
    let mut body = Body {
        id,
        slots,
        raster: cells,
        anchor,
        step: 0,
        vx: 0,
        vy: 0,
        spin: Spin::ZERO,
        acc_x: 0,
        acc_y: 0,
        acc_turn: 0,
        mass: 0,
        moment: 0,
        radius: 0,
        restitution: Q16::from_raw(0),
        friction: Q16::from_raw(0),
        weight: 0,
        freedoms: Freedoms::ALL,
        settles: true,
        assists: false,
        was_grounded: false,
        parked: false,
    };
    body.refresh_inertia();
    body
}

pub(super) fn release(world: &mut CellWorld, body: &Body, com: (i64, i64), pos: CellPos) {
    let Some(mut cell) = world.get_cell(pos) else {
        return;
    };
    if cell.body_id() != Some(body.id) {
        return;
    }
    let (vx, vy) = body.point_velocity(com, pos);
    cell.clear_body();
    cell.set_vel(vx as i32, vy as i32);
    world.set(pos, cell);
}
