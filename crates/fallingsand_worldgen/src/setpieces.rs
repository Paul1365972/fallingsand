use crate::biomes::{Lithology, SubBiome};
use crate::scale::{len, pitch};
use crate::structures::{Build, Ground, Site};
use fallingsand_core::MaterialId;
use fallingsand_core::content::material;
use fallingsand_math::{Hash, Rng};

const PIECE_SALT: Hash = Hash::label("worldgen.setpiece");

const PIECE_PITCH: i32 = 200;
const REACH: i32 = 68;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Piece {
    Aquifer,
    SandCeiling,
    Hopper,
    Firedamp,
    Magma,
    TarSeep,
    Sump,
    IcePlug,
    Keg,
}

const PIECES: [Piece; 9] = [
    Piece::Aquifer,
    Piece::SandCeiling,
    Piece::Hopper,
    Piece::Firedamp,
    Piece::Magma,
    Piece::TarSeep,
    Piece::Sump,
    Piece::IcePlug,
    Piece::Keg,
];

impl Piece {
    fn fits(self, lithology: Lithology, depth: f32) -> bool {
        match self {
            Piece::Aquifer => {
                matches!(
                    lithology,
                    Lithology::Carbonate | Lithology::Soil | Lithology::Clastic
                ) && (0.05..0.48).contains(&depth)
            }
            Piece::SandCeiling => lithology == Lithology::Clastic && (0.03..0.40).contains(&depth),
            Piece::Hopper => {
                matches!(lithology, Lithology::Clastic | Lithology::Carbonate)
                    && (0.06..0.50).contains(&depth)
            }
            Piece::Firedamp => {
                matches!(lithology, Lithology::Soil | Lithology::Clastic)
                    && (0.08..0.55).contains(&depth)
            }
            Piece::Magma => lithology == Lithology::Igneous && depth >= 0.32,
            Piece::TarSeep => lithology == Lithology::Clastic && (0.18..0.62).contains(&depth),
            Piece::Sump => {
                matches!(lithology, Lithology::Igneous | Lithology::Carbonate)
                    && (0.28..0.78).contains(&depth)
            }
            Piece::IcePlug => lithology == Lithology::Frozen && (0.05..0.55).contains(&depth),
            Piece::Keg => (0.08..0.52).contains(&depth),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn pieces_for_rect(
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
    let lattice = pitch(PIECE_PITCH);
    let lo_x = min_x.div_euclid(lattice);
    let hi_x = max_x.div_euclid(lattice);
    let lo_y = min_y.div_euclid(lattice);
    let hi_y = max_y.div_euclid(lattice);
    for cell_y in lo_y..=hi_y {
        for cell_x in lo_x..=hi_x {
            let mut rng = Hash::seed(seed).salt(PIECE_SALT).pos(cell_x, cell_y).rng();
            let x = cell_x * lattice + rng.draw().range(0, lattice - 1);
            let y = cell_y * lattice + rng.draw().range(0, lattice - 1);
            if !rng.draw().chance(0.8) {
                continue;
            }
            let depth = depth_at(x, y);
            if depth < 0.03 {
                continue;
            }
            place(seed, site, &mut rng, x, y, depth, &sub_at(x, y), out);
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
    let reach = len(REACH);
    let lithology = sub.lithology;
    let mut candidates = [Piece::Keg; PIECES.len()];
    let mut count = 0;
    for piece in PIECES {
        if piece.fits(lithology, depth) {
            candidates[count] = piece;
            count += 1;
        }
    }
    if count == 0 {
        return;
    }
    let chosen = candidates[rng.draw().range(0, count as i32 - 1) as usize];
    match chosen {
        Piece::Aquifer => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(12)) else {
                return;
            };
            let half = len(rng.draw().range(10, 26));
            lens(
                x,
                ceiling,
                half,
                len(rng.draw().range(10, 26)),
                material::WATER,
                material::CLAY,
                len(3),
                out,
            );
            stain(seed, x, ceiling, half, material::GYPSUM, out);
            drips(seed, site, x, ceiling, half, material::WATER, out);
        }
        Piece::SandCeiling => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(12)) else {
                return;
            };
            let half = len(rng.draw().range(9, 22));
            lens(
                x,
                ceiling,
                half,
                len(rng.draw().range(12, 30)),
                material::SAND,
                material::SANDSTONE,
                len(2),
                out,
            );
            heap(seed, site, x, ceiling, half / 2, material::SAND, out);
        }
        Piece::Hopper => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(14)) else {
                return;
            };
            let half = len(rng.draw().range(7, 14));
            lens(
                x,
                ceiling,
                half,
                len(rng.draw().range(14, 30)),
                material::SAND,
                material::SANDSTONE,
                len(4),
                out,
            );
            for offset in -len(2)..=len(2) {
                for step in 0..len(4) {
                    out.push(Build {
                        x: x + offset,
                        y: ceiling + step,
                        material: material::PLANKS,
                    });
                }
            }
        }
        Piece::Firedamp => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(12)) else {
                return;
            };
            let half = len(rng.draw().range(8, 18));
            lens(
                x,
                ceiling,
                half,
                len(rng.draw().range(10, 22)),
                material::FIREDAMP,
                sub.stone,
                len(3),
                out,
            );
            stain(seed, x, ceiling, half, material::SULFUR, out);
        }
        Piece::Magma => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(12)) else {
                return;
            };
            let half = len(rng.draw().range(9, 20));
            lens(
                x,
                ceiling,
                half,
                len(rng.draw().range(10, 24)),
                material::LAVA,
                material::OBSIDIAN,
                len(4),
                out,
            );
            stain(seed, x, ceiling, half, material::OBSIDIAN, out);
        }
        Piece::TarSeep => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(12)) else {
                return;
            };
            let half = len(rng.draw().range(8, 18));
            lens(
                x,
                ceiling,
                half,
                len(rng.draw().range(8, 20)),
                material::TAR,
                material::SHALE,
                len(3),
                out,
            );
            drips(seed, site, x, ceiling, half, material::TAR, out);
        }
        Piece::Sump => {
            let Some(floor) = site.floor_near(x, y, reach, len(14)) else {
                return;
            };
            let half = len(rng.draw().range(9, 20));
            if !site.bench(x, floor, half, len(12)) {
                return;
            }
            basin(
                x,
                floor,
                half,
                len(rng.draw().range(6, 16)),
                if lithology == Lithology::Igneous {
                    material::ACID
                } else {
                    material::BRINE
                },
                material::GYPSUM,
                out,
            );
        }
        Piece::IcePlug => {
            let Some(ceiling) = site.ceiling_near(x, y, reach, len(12)) else {
                return;
            };
            let half = len(rng.draw().range(9, 20));
            lens(
                x,
                ceiling,
                half,
                len(rng.draw().range(12, 28)),
                material::WATER,
                material::BLUE_ICE,
                len(5),
                out,
            );
        }
        Piece::Keg => {
            let Some(floor) = site.floor_near(x, y, reach, len(12)) else {
                return;
            };
            keg(rng, x, floor, out);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lens(
    x: i32,
    ceiling: i32,
    half: i32,
    rise: i32,
    fill: MaterialId,
    crust: MaterialId,
    thickness: i32,
    out: &mut Vec<Build>,
) {
    for offset in -half..=half {
        let across = offset as f32 / half as f32;
        let taper = (1.0 - across * across).max(0.0).sqrt();
        let top = (taper * rise as f32) as i32;
        if top <= 0 {
            continue;
        }
        for step in 0..thickness {
            out.push(Build {
                x: x + offset,
                y: ceiling + step,
                material: crust,
            });
        }
        for step in thickness..thickness + top {
            out.push(Build {
                x: x + offset,
                y: ceiling + step,
                material: fill,
            });
        }
        out.push(Build {
            x: x + offset,
            y: ceiling + thickness + top,
            material: crust,
        });
    }
}

fn basin(
    x: i32,
    floor: i32,
    half: i32,
    sink: i32,
    fill: MaterialId,
    rim: MaterialId,
    out: &mut Vec<Build>,
) {
    for offset in -half..=half {
        let across = offset as f32 / half as f32;
        let taper = (1.0 - across * across).max(0.0);
        let deep = (taper * sink as f32) as i32;
        if deep <= 0 {
            out.push(Build {
                x: x + offset,
                y: floor,
                material: rim,
            });
            continue;
        }
        for step in 0..deep {
            out.push(Build {
                x: x + offset,
                y: floor - step,
                material: fill,
            });
        }
        out.push(Build {
            x: x + offset,
            y: floor - deep,
            material: rim,
        });
    }
}

fn stain(seed: u64, x: i32, ceiling: i32, half: i32, material: MaterialId, out: &mut Vec<Build>) {
    for offset in -half..=half {
        let hash = Hash::seed(seed).salt(PIECE_SALT).pos(x + offset, ceiling);
        if hash.chance(0.55) {
            continue;
        }
        out.push(Build {
            x: x + offset,
            y: ceiling - 1,
            material,
        });
    }
}

fn drips(
    seed: u64,
    site: &Site,
    x: i32,
    ceiling: i32,
    half: i32,
    material: MaterialId,
    out: &mut Vec<Build>,
) {
    for offset in -half..=half {
        if Hash::seed(seed)
            .salt(PIECE_SALT)
            .pos(x + offset, ceiling + 1)
            .chance(0.86)
        {
            continue;
        }
        let mut level = ceiling - 1;
        while level > ceiling - len(40) {
            if site.at(x + offset, level) == Ground::Stone {
                out.push(Build {
                    x: x + offset,
                    y: level + 1,
                    material,
                });
                break;
            }
            level -= 1;
        }
    }
}

fn heap(
    seed: u64,
    site: &Site,
    x: i32,
    ceiling: i32,
    half: i32,
    material: MaterialId,
    out: &mut Vec<Build>,
) {
    let mut level = ceiling - 1;
    while level > ceiling - len(40) {
        if site.at(x, level) == Ground::Stone {
            break;
        }
        level -= 1;
    }
    if level <= ceiling - len(40) {
        return;
    }
    for offset in -half..=half {
        let taper = 1.0 - (offset.abs() as f32 / half.max(1) as f32);
        let rise = (taper * len(5) as f32) as i32;
        for step in 1..=rise {
            if Hash::seed(seed)
                .salt(PIECE_SALT)
                .pos(x + offset, level + step)
                .chance(0.2)
            {
                continue;
            }
            out.push(Build {
                x: x + offset,
                y: level + step,
                material,
            });
        }
    }
}

fn keg(rng: &mut Rng, x: i32, floor: i32, out: &mut Vec<Build>) {
    let width = len(rng.draw().range(4, 8));
    let height = len(rng.draw().range(4, 7));
    for offset in -1..=width {
        for rise in 1..=height {
            let shell = offset == -1 || offset == width || rise == 1 || rise == height;
            out.push(Build {
                x: x + offset,
                y: floor + rise,
                material: if shell {
                    material::PLANKS
                } else {
                    material::GUNPOWDER
                },
            });
        }
    }
    if rng.draw().chance(0.6) {
        out.push(Build {
            x: x + width + len(3),
            y: floor + 1,
            material: material::TORCH,
        });
    }
}
