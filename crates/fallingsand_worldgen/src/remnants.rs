use crate::biomes::SubBiome;
use crate::scale::{len, pitch};
use crate::structures::{Build, Ground, Site};
use fallingsand_core::MaterialId;
use fallingsand_core::content::material;
use fallingsand_math::{Hash, Rng};

const REMNANT_SALT: Hash = Hash::label("worldgen.remnant");

const REMNANT_PITCH: i32 = 96;
const REACH: i32 = 68;
const SPAN: i32 = 26;

#[allow(clippy::too_many_arguments)]
pub fn remnants_for_rect(
    seed: u64,
    site: &Site,
    sub_at: &dyn Fn(i32, i32) -> SubBiome,
    depth_at: &dyn Fn(i32, i32) -> f32,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    out: &mut Vec<Build>,
) {
    let lattice = pitch(REMNANT_PITCH);
    let lo_x = min_x.div_euclid(lattice);
    let hi_x = max_x.div_euclid(lattice);
    let lo_y = min_y.div_euclid(lattice);
    let hi_y = max_y.div_euclid(lattice);
    for cell_y in lo_y..=hi_y {
        for cell_x in lo_x..=hi_x {
            let mut rng = Hash::seed(seed)
                .salt(REMNANT_SALT)
                .pos(cell_x, cell_y)
                .rng();
            let x = cell_x * lattice + rng.draw().range(0, lattice - 1);
            let y = cell_y * lattice + rng.draw().range(0, lattice - 1);
            if !rng.draw().chance(0.85) {
                continue;
            }
            let depth = depth_at(x, y);
            if depth < 0.012 {
                continue;
            }
            let sub = sub_at(x, y);
            place(seed, site, &mut rng, x, y, depth, &sub, out);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn place(
    seed: u64,
    site: &Site,
    rng: &mut Rng,
    x: i32,
    y: i32,
    depth: f32,
    sub: &SubBiome,
    out: &mut Vec<Build>,
) {
    let roll = rng.draw().range(0, 11);
    let span = len(SPAN);
    let reach = len(REACH);
    match roll {
        0 | 1 => {
            let Some(floor) = site.floor_near(x, y, reach, len(12)) else {
                return;
            };
            firepit(seed, site, rng, x, floor, out);
        }
        2 => {
            let Some(floor) = site.floor_near(x, y, reach, len(10)) else {
                return;
            };
            bones(rng, x, floor, out);
        }
        3 => {
            let Some(floor) = site.floor_near(x, y, reach, len(12)) else {
                return;
            };
            spoil(seed, rng, x, floor, out);
        }
        4 => {
            let Some(floor) = site.floor_near(x, y, reach, len(11)) else {
                return;
            };
            pack(rng, x, floor, out);
        }
        5 => {
            let Some(floor) = site.floor_near(x, y, reach, len(11)) else {
                return;
            };
            cache(rng, x, floor, depth, out);
        }
        6 => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(10)) else {
                return;
            };
            rope_end(rng, x, ceiling, out);
        }
        7 => {
            let Some(floor) = site.floor_near(x, y, reach, len(12)) else {
                return;
            };
            let Some(wall) = wall_beside(site, x, floor + len(4), span) else {
                return;
            };
            torch(rng, wall.0, wall.1, floor, out);
        }
        8 => {
            let Some(floor) = site.floor_near(x, y, reach, len(12)) else {
                return;
            };
            let Some(wall) = wall_beside(site, x, floor + len(5), span) else {
                return;
            };
            tally(seed, rng, wall.0, wall.1, out);
        }
        9 => {
            let Some(floor) = site.floor_near(x, y, reach, len(14)) else {
                return;
            };
            let Some(wall) = wall_beside(site, x, floor + len(6), span) else {
                return;
            };
            dig_face(seed, rng, wall.0, wall.1, sub.prize, out);
        }
        10 => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(14)) else {
                return;
            };
            ladder(rng, x, ceiling, out);
        }
        _ => {
            let Some(floor) = site.floor_near(x, y, reach, len(13)) else {
                return;
            };
            cairn(seed, rng, x, floor, sub, out);
        }
    }
}

fn wall_beside(site: &Site, x: i32, y: i32, span: i32) -> Option<(i32, i32)> {
    if site.at(x, y) != Ground::Cave {
        return None;
    }
    for step in 1..=span {
        if site.at(x + step, y) == Ground::Stone {
            return Some((x + step - 1, y));
        }
        if site.at(x - step, y) == Ground::Stone {
            return Some((x - step + 1, y));
        }
    }
    None
}

fn firepit(seed: u64, site: &Site, rng: &mut Rng, x: i32, floor: i32, out: &mut Vec<Build>) {
    let half = len(rng.draw().range(4, 8));
    if !site.bench(x, floor, half, len(10)) {
        return;
    }
    for offset in -half..=half {
        let edge = offset.abs() == half || offset.abs() == half - 1;
        let material = if edge {
            material::RUBBLE
        } else if Hash::seed(seed)
            .salt(REMNANT_SALT)
            .pos(x + offset, floor)
            .chance(0.5)
        {
            material::CHARCOAL
        } else {
            material::ASH
        };
        out.push(Build {
            x: x + offset,
            y: floor + 1,
            material,
        });
        if edge {
            out.push(Build {
                x: x + offset,
                y: floor + 2,
                material: material::RUBBLE,
            });
        }
    }
    if rng.draw().chance(0.4) {
        out.push(Build {
            x: x + half + len(2),
            y: floor + 1,
            material: material::PLANKS,
        });
    }
}

fn bones(rng: &mut Rng, x: i32, floor: i32, out: &mut Vec<Build>) {
    let count = rng.draw().range(4, 12);
    for _ in 0..count {
        let offset = rng.draw().range(-len(7), len(7));
        out.push(Build {
            x: x + offset,
            y: floor + 1,
            material: material::BONE,
        });
    }
}

fn spoil(seed: u64, rng: &mut Rng, x: i32, floor: i32, out: &mut Vec<Build>) {
    let half = len(rng.draw().range(5, 12));
    for offset in -half..=half {
        let taper = 1.0 - (offset.abs() as f32 / half as f32);
        let rise = (taper * len(6) as f32) as i32;
        for step in 1..=rise {
            let material = if Hash::seed(seed)
                .salt(REMNANT_SALT)
                .pos(x + offset, floor + step)
                .chance(0.35)
            {
                material::GRAVEL
            } else {
                material::RUBBLE
            };
            out.push(Build {
                x: x + offset,
                y: floor + step,
                material,
            });
        }
    }
}

fn pack(rng: &mut Rng, x: i32, floor: i32, out: &mut Vec<Build>) {
    let width = len(rng.draw().range(3, 6));
    for offset in 0..width {
        out.push(Build {
            x: x + offset,
            y: floor + 1,
            material: material::PLANKS,
        });
    }
    out.push(Build {
        x: x + width / 2,
        y: floor + 2,
        material: material::COAL,
    });
    if rng.draw().chance(0.5) {
        out.push(Build {
            x: x + width + len(1),
            y: floor + 1,
            material: material::TORCH,
        });
    }
    if rng.draw().chance(0.35) {
        out.push(Build {
            x: x - len(1),
            y: floor + 1,
            material: material::ROPE,
        });
    }
}

fn cache(rng: &mut Rng, x: i32, floor: i32, depth: f32, out: &mut Vec<Build>) {
    let prize = if depth < 0.12 {
        material::FLINT
    } else if depth < 0.30 {
        material::BRONZE
    } else if depth < 0.55 {
        material::IRON
    } else {
        material::STEEL
    };
    let width = len(rng.draw().range(2, 5));
    for offset in -1..=width {
        out.push(Build {
            x: x + offset,
            y: floor + 1,
            material: material::PLANKS,
        });
    }
    for offset in 0..width {
        out.push(Build {
            x: x + offset,
            y: floor + 2,
            material: prize,
        });
    }
}

fn rope_end(rng: &mut Rng, x: i32, ceiling: i32, out: &mut Vec<Build>) {
    let drop = len(rng.draw().range(3, 11));
    for step in 1..=drop {
        out.push(Build {
            x,
            y: ceiling - step,
            material: material::ROPE,
        });
    }
}

fn torch(rng: &mut Rng, x: i32, y: i32, floor: i32, out: &mut Vec<Build>) {
    if rng.draw().chance(0.55) {
        out.push(Build {
            x,
            y,
            material: material::TORCH,
        });
    } else {
        out.push(Build {
            x,
            y: floor + 1,
            material: material::CHARCOAL,
        });
        out.push(Build {
            x,
            y,
            material: material::CHARCOAL,
        });
    }
}

fn tally(seed: u64, rng: &mut Rng, x: i32, y: i32, out: &mut Vec<Build>) {
    let marks = rng.draw().range(3, 9);
    for index in 0..marks {
        let step = index * 2;
        let height = if Hash::seed(seed)
            .salt(REMNANT_SALT)
            .pos(x, index)
            .chance(0.3)
        {
            len(2)
        } else {
            len(3)
        };
        for rise in 0..height {
            out.push(Build {
                x,
                y: y + step - rise,
                material: material::CHARCOAL,
            });
        }
    }
}

fn dig_face(seed: u64, rng: &mut Rng, x: i32, y: i32, prize: MaterialId, out: &mut Vec<Build>) {
    let radius = len(rng.draw().range(3, 7));
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            if Hash::seed(seed)
                .salt(REMNANT_SALT)
                .pos(x + dx, y + dy)
                .chance(0.4)
            {
                continue;
            }
            out.push(Build {
                x: x + dx,
                y: y + dy,
                material: prize,
            });
        }
    }
}

fn ladder(rng: &mut Rng, x: i32, ceiling: i32, out: &mut Vec<Build>) {
    let drop = len(rng.draw().range(6, 18));
    let rung = len(3).max(2);
    for step in 0..=drop {
        let y = ceiling - step;
        out.push(Build {
            x,
            y,
            material: material::PLANKS,
        });
        if step > 0 && step % rung == 0 {
            for offset in -len(2)..=len(2) {
                out.push(Build {
                    x: x + offset,
                    y,
                    material: material::PLANKS,
                });
            }
        }
    }
}

fn cairn(seed: u64, rng: &mut Rng, x: i32, floor: i32, sub: &SubBiome, out: &mut Vec<Build>) {
    let height = len(rng.draw().range(5, 14));
    let buried = rng.draw().chance(0.3);
    for step in 1..=height {
        let taper = 1.0 - (step as f32 / (height + 1) as f32);
        let half = (taper * len(4) as f32) as i32;
        for offset in -half..=half {
            let material = if Hash::seed(seed)
                .salt(REMNANT_SALT)
                .pos(x + offset, floor + step)
                .chance(0.25)
            {
                sub.streak
            } else {
                sub.stone
            };
            out.push(Build {
                x: x + offset,
                y: floor + step,
                material,
            });
        }
    }
    if buried {
        for offset in -len(2)..=len(2) {
            out.push(Build {
                x: x + offset,
                y: floor - len(2),
                material: material::BONE,
            });
        }
    }
}
