use crate::biomes::{Lithology, SubBiome};
use crate::caves::Carve;
use crate::scale::{len, pitch};
use crate::structures::Build;
use fallingsand_core::MaterialId;
use fallingsand_core::content::material;
use fallingsand_math::{Hash, Rng};

const MINE_SALT: Hash = Hash::label("worldgen.mine");
const TIMBER_SALT: Hash = Hash::label("worldgen.timber");

const MINE_PITCH: i32 = 2200;
const MINE_REACH: i32 = 1240;
const SET_SPACING: i32 = 26;
const SLEEPER_SPACING: i32 = 9;
const ADIT_LIMIT: i32 = 900;
const HEADFRAME: i32 = 14;

struct Stub {
    x: i32,
    direction: i32,
    length: i32,
    clearance: i32,
    face: bool,
}

struct Level {
    floor: i32,
    from_x: i32,
    to_x: i32,
    clearance: i32,
    slope: f32,
    collapses: Vec<(i32, i32, i32)>,
    stubs: Vec<Stub>,
}

struct Riser {
    x: i32,
    bottom: i32,
    top: i32,
    half: i32,
}

struct Adit {
    x: i32,
    bottom: i32,
    top: i32,
    half: i32,
}

pub struct Mine {
    seat_x: i32,
    levels: Vec<Level>,
    risers: Vec<Riser>,
    adit: Option<Adit>,
    prize: MaterialId,
}

impl Level {
    fn floor_at(&self, x: i32, seat_x: i32) -> i32 {
        self.floor + (self.slope * (x - seat_x) as f32).round() as i32
    }

    fn collapse_top(&self, x: i32) -> Option<i32> {
        self.collapses
            .iter()
            .find(|(from, to, _)| x >= *from && x <= *to)
            .map(|(_, _, top)| *top)
    }
}

fn diggable(lithology: Lithology) -> bool {
    matches!(
        lithology,
        Lithology::Soil
            | Lithology::Carbonate
            | Lithology::Clastic
            | Lithology::Igneous
            | Lithology::Crystalline
            | Lithology::Fungal
    )
}

#[allow(clippy::too_many_arguments)]
pub fn plan(
    seed: u64,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    sub_at: &dyn Fn(i32, i32) -> SubBiome,
    surface_at: &dyn Fn(i32) -> i32,
    depth_at: &dyn Fn(i32, i32) -> f32,
) -> Vec<Mine> {
    let lattice = pitch(MINE_PITCH);
    let reach = len(MINE_REACH);
    let lo_x = (min_x - reach).div_euclid(lattice);
    let hi_x = (max_x + reach).div_euclid(lattice);
    let lo_y = (min_y - reach).div_euclid(lattice);
    let hi_y = (max_y + reach).div_euclid(lattice);
    let mut out = Vec::new();
    for cell_y in lo_y..=hi_y {
        for cell_x in lo_x..=hi_x {
            let mut rng = Hash::seed(seed).salt(MINE_SALT).pos(cell_x, cell_y).rng();
            let seat_x = cell_x * lattice + rng.draw().range(0, lattice - 1);
            let seat_y = cell_y * lattice + rng.draw().range(0, lattice - 1);
            if !rng.draw().chance(0.66) {
                continue;
            }
            let sub = sub_at(seat_x, seat_y);
            if !diggable(sub.lithology) {
                continue;
            }
            let depth = depth_at(seat_x, seat_y);
            if !(0.09..0.46).contains(&depth) {
                continue;
            }
            out.push(lay_out(&mut rng, seat_x, seat_y, sub.prize, surface_at));
        }
    }
    out
}

fn lay_out(
    rng: &mut Rng,
    seat_x: i32,
    seat_y: i32,
    prize: MaterialId,
    surface_at: &dyn Fn(i32) -> i32,
) -> Mine {
    let count = rng.draw().range(1, 4);
    let spacing = len(rng.draw().range(70, 140));
    let mut levels = Vec::new();
    for index in 0..count {
        let half = len(rng.draw().range(200, 700));
        let bias = rng.draw().range(-half / 3, half / 3);
        let clearance = len(rng.draw().range(13, 19));
        let slope = (rng.draw().unit() - 0.5) * 0.05;
        let from_x = seat_x + bias - half;
        let to_x = seat_x + bias + half;

        let mut collapses = Vec::new();
        let breaks = rng.draw().range(0, 4);
        for _ in 0..breaks {
            let at = from_x + rng.draw().range(0, (to_x - from_x).max(1));
            let span = len(rng.draw().range(8, 30));
            let top = if rng.draw().chance(0.5) {
                clearance
            } else {
                clearance * 2 / 3
            };
            collapses.push((at, at + span, top));
        }

        let mut stubs = Vec::new();
        let spurs = rng.draw().range(2, 7);
        for _ in 0..spurs {
            stubs.push(Stub {
                x: from_x + rng.draw().range(0, (to_x - from_x).max(1)),
                direction: if rng.draw().bit() { 1 } else { -1 },
                length: len(rng.draw().range(30, 130)),
                clearance: len(rng.draw().range(11, 15)),
                face: rng.draw().chance(0.55),
            });
        }

        levels.push(Level {
            floor: seat_y - index * spacing,
            from_x,
            to_x,
            clearance,
            slope,
            collapses,
            stubs,
        });
    }

    let mut risers = Vec::new();
    for index in 1..levels.len() {
        let half = len(rng.draw().range(4, 8));
        let upper = &levels[index - 1];
        let lower = &levels[index];
        let from = upper.from_x.max(lower.from_x);
        let to = upper.to_x.min(lower.to_x);
        if to - from < len(24) {
            continue;
        }
        let x = from + rng.draw().range(0, to - from);
        risers.push(Riser {
            x,
            bottom: lower.floor_at(x, seat_x),
            top: upper.floor_at(x, seat_x) + upper.clearance,
            half,
        });
    }

    let portal_half = len(rng.draw().range(4, 7));
    let top = &levels[0];
    let portal_x = top.from_x + rng.draw().range(0, (top.to_x - top.from_x).max(1));
    let ceiling = top.floor_at(portal_x, seat_x) + top.clearance;
    let ground = surface_at(portal_x);
    let adit = if ground > ceiling && ground - ceiling <= len(ADIT_LIMIT) {
        Some(Adit {
            x: portal_x,
            bottom: top.floor_at(portal_x, seat_x),
            top: ground + len(4),
            half: portal_half,
        })
    } else {
        None
    };

    Mine {
        seat_x,
        levels,
        risers,
        adit,
        prize,
    }
}

impl Mine {
    pub fn carve(&self, carve: &mut Carve) {
        for level in &self.levels {
            for x in level.from_x..=level.to_x {
                let floor = level.floor_at(x, self.seat_x);
                for y in floor..=floor + level.clearance {
                    carve.open(x, y);
                }
            }
            for stub in &level.stubs {
                let floor = level.floor_at(stub.x, self.seat_x);
                for step in 0..=stub.length {
                    let x = stub.x + stub.direction * step;
                    let rise = (step as f32 * 0.08) as i32;
                    for y in (floor + rise)..=(floor + rise + stub.clearance) {
                        carve.open(x, y);
                    }
                }
            }
        }
        for riser in &self.risers {
            for x in (riser.x - riser.half)..=(riser.x + riser.half) {
                for y in riser.bottom..=riser.top {
                    carve.open(x, y);
                }
            }
        }
        if let Some(adit) = &self.adit {
            for x in (adit.x - adit.half)..=(adit.x + adit.half) {
                for y in adit.bottom..=adit.top {
                    carve.open(x, y);
                }
            }
        }
    }

    pub fn furnish(&self, seed: u64, out: &mut Vec<Build>) {
        let sets = pitch(SET_SPACING);
        let sleepers = pitch(SLEEPER_SPACING);
        for level in &self.levels {
            for x in level.from_x..=level.to_x {
                let floor = level.floor_at(x, self.seat_x);
                let ceiling = floor + level.clearance;
                if let Some(top) = level.collapse_top(x) {
                    for y in floor..=floor + top {
                        out.push(Build {
                            x,
                            y,
                            material: material::RUBBLE,
                        });
                    }
                    continue;
                }
                for y in floor + len(2)..=(floor + len(11)).min(ceiling) {
                    out.push(Build {
                        x,
                        y,
                        material: MaterialId::AIR,
                    });
                }
                if x.rem_euclid(sleepers) == 0 {
                    out.push(Build {
                        x,
                        y: floor,
                        material: material::PLANKS,
                    });
                }
                if x.rem_euclid(sets) != 0 {
                    continue;
                }
                for offset in -len(3)..=len(3) {
                    out.push(Build {
                        x: x + offset,
                        y: ceiling,
                        material: material::BEAM,
                    });
                }
                let hang = (level.clearance / 3).max(len(2));
                for step in 1..=hang {
                    out.push(Build {
                        x,
                        y: ceiling - step,
                        material: material::BEAM,
                    });
                }
                let lamp = Hash::seed(seed).salt(TIMBER_SALT).pos(x, ceiling);
                if lamp.chance(0.34) {
                    out.push(Build {
                        x: x + len(2),
                        y: ceiling - 1,
                        material: if lamp.rng().draw().chance(0.25) {
                            material::LUMEN_LAMP
                        } else {
                            material::TORCH
                        },
                    });
                }
            }
            for stub in &level.stubs {
                let seat = level.floor_at(stub.x, self.seat_x);
                for step in 0..=stub.length {
                    let x = stub.x + stub.direction * step;
                    let base = seat + (step as f32 * 0.08) as i32;
                    let top = (base + len(11)).min(base + stub.clearance);
                    for y in base + len(2)..=top {
                        out.push(Build {
                            x,
                            y,
                            material: MaterialId::AIR,
                        });
                    }
                }
                if !stub.face {
                    continue;
                }
                let floor = seat;
                let rise = (stub.length as f32 * 0.08) as i32;
                let end_x = stub.x + stub.direction * (stub.length + len(3));
                let centre_y = floor + rise + stub.clearance / 2;
                let radius = len(7);
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        if dx * dx + dy * dy > radius * radius {
                            continue;
                        }
                        let x = end_x + dx;
                        let y = centre_y + dy;
                        if Hash::seed(seed).salt(TIMBER_SALT).pos(x, y).chance(0.34) {
                            continue;
                        }
                        out.push(Build {
                            x,
                            y,
                            material: self.prize,
                        });
                    }
                }
            }
        }
        for riser in &self.risers {
            for offset in -riser.half..=riser.half {
                for y in riser.bottom + len(2)..=riser.top {
                    out.push(Build {
                        x: riser.x + offset,
                        y,
                        material: MaterialId::AIR,
                    });
                }
            }
            for y in riser.bottom..=riser.top {
                out.push(Build {
                    x: riser.x,
                    y,
                    material: material::ROPE,
                });
            }
            for offset in -riser.half..=riser.half {
                out.push(Build {
                    x: riser.x + offset,
                    y: riser.bottom,
                    material: material::PLANKS,
                });
            }
        }
        if let Some(adit) = &self.adit {
            let head = adit.top - len(4);
            let crown = head + len(HEADFRAME);
            for y in adit.bottom..=crown {
                out.push(Build {
                    x: adit.x,
                    y,
                    material: material::ROPE,
                });
            }
            for offset in [-(adit.half + 1), adit.half + 1] {
                for y in head - len(8)..=crown {
                    out.push(Build {
                        x: adit.x + offset,
                        y,
                        material: material::BEAM,
                    });
                }
            }
            for offset in -(adit.half + 1)..=(adit.half + 1) {
                out.push(Build {
                    x: adit.x + offset,
                    y: crown,
                    material: material::BEAM,
                });
            }
            for step in 1..=len(4) {
                for offset in [-(adit.half + 1 + step), adit.half + 1 + step] {
                    for y in (crown - step)..=(crown - step + 1) {
                        out.push(Build {
                            x: adit.x + offset,
                            y,
                            material: material::BEAM,
                        });
                    }
                }
            }
        }
    }
}
