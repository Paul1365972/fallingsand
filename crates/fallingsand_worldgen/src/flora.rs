use crate::biomes::{Skin, Tree};
use crate::scale::{len, pitch};
use crate::terrain::SEA_LEVEL;
use fallingsand_core::MaterialId;
use fallingsand_core::content::material;
use fallingsand_math::{Hash, Rng};

const TREE_SALT: Hash = Hash::label("worldgen.tree");
const CROWN_REACH: i32 = 104;
const COVER_SALT: Hash = Hash::label("worldgen.cover");
const BOULDER_SALT: Hash = Hash::label("worldgen.boulder");

pub struct Growth {
    pub x: i32,
    pub y: i32,
    pub material: MaterialId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Canopy {
    Broad,
    Conifer,
    Cap,
    Bare,
}

struct Species {
    trunk: MaterialId,
    bark: Option<MaterialId>,
    foliage: MaterialId,
    gills: Option<MaterialId>,
    canopy: Canopy,
    height: (i32, i32),
    trunk_width: (i32, i32),
    clear: f32,
    spread: f32,
    branches: (i32, i32),
}

fn species(kind: Tree) -> Option<Species> {
    let base = Species {
        trunk: material::WOOD,
        bark: None,
        foliage: material::LEAVES,
        gills: None,
        canopy: Canopy::Broad,
        height: (45, 80),
        trunk_width: (3, 4),
        clear: 0.40,
        spread: 1.05,
        branches: (5, 9),
    };
    Some(match kind {
        Tree::None => return None,
        Tree::Broadleaf => base,
        Tree::Resinpine => Species {
            foliage: material::NEEDLES,
            canopy: Canopy::Conifer,
            height: (80, 130),
            trunk_width: (3, 3),
            clear: 0.22,
            spread: 0.52,
            branches: (0, 0),
            ..base
        },
        Tree::Redwood => Species {
            trunk: material::HEARTWOOD,
            bark: Some(material::HEARTWOOD),
            height: (300, 440),
            trunk_width: (8, 12),
            clear: 0.66,
            spread: 0.42,
            branches: (7, 12),
            ..base
        },
        Tree::Mangrove => Species {
            height: (40, 78),
            trunk_width: (3, 4),
            clear: 0.30,
            spread: 1.15,
            branches: (6, 10),
            ..base
        },
        Tree::Hawthorn => Species {
            height: (26, 46),
            trunk_width: (2, 3),
            clear: 0.32,
            spread: 1.1,
            branches: (4, 7),
            ..base
        },
        Tree::DwarfWillow => Species {
            height: (14, 26),
            trunk_width: (2, 2),
            clear: 0.28,
            spread: 1.2,
            branches: (3, 5),
            ..base
        },
        Tree::Snag => Species {
            trunk: material::CHARCOAL,
            foliage: material::CHARCOAL,
            canopy: Canopy::Bare,
            height: (60, 110),
            trunk_width: (2, 3),
            clear: 0.5,
            spread: 0.0,
            branches: (3, 6),
            ..base
        },
        Tree::Sporecap => Species {
            trunk: material::SHROOM_STEM,
            foliage: material::SHROOM_CAP,
            gills: Some(material::GLOWCAP),
            canopy: Canopy::Cap,
            height: (46, 96),
            trunk_width: (4, 7),
            clear: 1.0,
            spread: 0.0,
            branches: (0, 0),
            ..base
        },
    })
}

pub fn growth_for_rect(
    seed: u64,
    skin_at: &dyn Fn(i32) -> Option<Skin>,
    ground_at: &dyn Fn(i32) -> Option<i32>,
    min_x: i32,
    max_x: i32,
) -> Vec<Growth> {
    let mut out = Vec::new();

    for x in min_x..=max_x {
        let Some(skin) = skin_at(x) else {
            continue;
        };

        if skin.tree_spacing > 0
            && let Some(kind) = species(skin.tree)
        {
            let spacing = pitch(skin.tree_spacing);
            if x.rem_euclid(spacing) == 0 {
                let mut rng = Hash::seed(seed).salt(TREE_SALT).pos(x, 0).rng();
                let trunk_x = x + rng.draw().range(0, spacing - 1);
                if rng.draw().chance(0.78)
                    && let Some(ground) = ground_at(trunk_x)
                    && ground >= SEA_LEVEL - len(skin.wade)
                {
                    out.append(&mut grow_tree(seed, &kind, &mut rng, trunk_x, ground));
                }
            }
        }

        if let Some(cover) = skin.ground_cover {
            let mut rng = Hash::seed(seed).salt(COVER_SALT).pos(x, 0).rng();
            if rng.draw().chance(skin.cover_chance)
                && let Some(ground) = ground_at(x)
                && skin.submerged == (ground < SEA_LEVEL)
            {
                let height = rng.draw().range(1, len(3).max(1));
                for step in 1..=height {
                    out.push(Growth {
                        x,
                        y: ground + step,
                        material: cover,
                    });
                }
            }
        }

        if skin.boulder_spacing > 0 {
            let spacing = pitch(skin.boulder_spacing);
            if x.rem_euclid(spacing) == 0 {
                let mut rng = Hash::seed(seed).salt(BOULDER_SALT).pos(x, 0).rng();
                let seat = x + rng.draw().range(0, spacing - 1);
                if rng.draw().chance(0.6)
                    && let Some(ground) = ground_at(seat)
                    && ground >= SEA_LEVEL
                {
                    let radius = len(rng.draw().range(3, 12));
                    for dy in 0..=radius * 2 {
                        for dx in -radius..=radius {
                            let px = seat + dx;
                            let py = ground + dy;
                            let ny = dy as f32 / radius as f32 - 1.0;
                            let nx = dx as f32 / radius as f32;
                            let wobble = Hash::seed(seed).pos(px, py).unit() * 0.2;
                            if nx * nx + ny * ny < 1.0 - wobble {
                                out.push(Growth {
                                    x: px,
                                    y: py,
                                    material: skin.boulder,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn grow_tree(seed: u64, kind: &Species, rng: &mut Rng, trunk_x: i32, ground: i32) -> Vec<Growth> {
    let mut parts = Vec::new();
    let height = len(rng.draw().range(kind.height.0, kind.height.1));
    let width = len(rng.draw().range(kind.trunk_width.0, kind.trunk_width.1));
    let lean = (rng.draw().unit() - 0.5) * 0.16;
    let root = ground + 1;

    for step in 0..height {
        let taper = 1.0 - (step as f32 / height as f32) * 0.5;
        let half = ((width as f32 * taper) * 0.5).max(0.5);
        let centre = trunk_x as f32 + lean * step as f32;
        let lo = (centre - half).round() as i32;
        let hi = (centre + half).round() as i32;
        for x in lo..=hi {
            let material = match kind.bark {
                Some(bark) if hi > lo && (x == lo || x == hi) => bark,
                _ => kind.trunk,
            };
            parts.push(Growth {
                x,
                y: root + step,
                material,
            });
        }
    }

    let top = root + height;
    let apex_x = (trunk_x as f32 + lean * height as f32).round() as i32;
    let spread = (((height as f32 * kind.spread) * 0.5) as i32).min(len(CROWN_REACH));

    let branch_count = if kind.branches.1 > 0 {
        rng.draw().range(kind.branches.0, kind.branches.1)
    } else {
        0
    };
    for _ in 0..branch_count {
        let fraction = kind.clear + (1.0 - kind.clear) * rng.draw().unit();
        let base_step = (height as f32 * fraction) as i32;
        let base_y = root + base_step;
        let base_x = (trunk_x as f32 + lean * base_step as f32).round() as i32;
        let direction = if rng.draw().bit() { 1 } else { -1 };
        let reach = len(rng.draw().range(6, 26)).min(spread.max(len(5)));
        let rise = rng.draw().unit() * 0.7 + 0.15;
        let mut previous = base_y;
        for step in 1..=reach {
            let x = base_x + direction * step;
            let target = base_y + (step as f32 * rise) as i32;
            for y in previous.min(target)..=previous.max(target) {
                parts.push(Growth {
                    x,
                    y,
                    material: kind.trunk,
                });
            }
            parts.push(Growth {
                x,
                y: previous.min(target) - 1,
                material: kind.trunk,
            });
            previous = target;
        }
    }

    match kind.canopy {
        Canopy::Bare => {}
        Canopy::Broad => grow_broad(seed, kind, apex_x, top, spread.max(len(4)), &mut parts),
        Canopy::Conifer => grow_conifer(seed, kind, apex_x, top, height, &mut parts),
        Canopy::Cap => grow_cap(kind, rng, apex_x, top, width, &mut parts),
    }
    parts
}

fn knit(cells: Vec<(i32, i32)>, apex_x: i32) -> Vec<(i32, i32)> {
    let present: std::collections::HashSet<(i32, i32)> = cells.iter().copied().collect();
    let mut keep: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut stack: Vec<(i32, i32)> = Vec::new();
    for &cell in &cells {
        if cell.0 == apex_x && keep.insert(cell) {
            stack.push(cell);
        }
    }
    while let Some((x, y)) = stack.pop() {
        for step in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            if present.contains(&step) && keep.insert(step) {
                stack.push(step);
            }
        }
    }
    cells.into_iter().filter(|c| keep.contains(c)).collect()
}

fn grow_broad(
    seed: u64,
    kind: &Species,
    apex_x: i32,
    top: i32,
    radius: i32,
    parts: &mut Vec<Growth>,
) {
    let centre_y = top - radius / 3;
    let mut cells = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let nx = dx as f32 / radius as f32;
            let ny = dy as f32 / (radius as f32 * 0.88);
            let distance = nx * nx + ny * ny;
            if distance > 1.0 {
                continue;
            }
            let x = apex_x + dx;
            let y = centre_y + dy;
            if Hash::seed(seed).salt(TREE_SALT).pos(x, y).unit() < 0.10 + 0.40 * distance {
                continue;
            }
            cells.push((x, y));
        }
    }
    for (x, y) in knit(cells, apex_x) {
        parts.push(Growth {
            x,
            y,
            material: kind.foliage,
        });
    }
}

fn grow_conifer(
    seed: u64,
    kind: &Species,
    apex_x: i32,
    top: i32,
    height: i32,
    parts: &mut Vec<Growth>,
) {
    let span = (height as f32 * (1.0 - kind.clear)) as i32;
    if span <= 0 {
        return;
    }
    let widest = (height as f32 * kind.spread * 0.5)
        .max(len(3) as f32)
        .min(len(CROWN_REACH) as f32);
    let tier = len(7).max(2);
    let mut cells = Vec::new();
    for level in 0..span {
        let y = top - level;
        let growth = (level as f32 / span as f32).powf(0.75);
        let half = (widest * growth) as i32;
        let pulse = 0.58 + 0.42 * ((level % tier) as f32 / tier as f32);
        let reach = (half as f32 * pulse) as i32;
        for dx in -reach..=reach {
            let x = apex_x + dx;
            let edge = dx.abs() as f32 / reach.max(1) as f32;
            if Hash::seed(seed).salt(TREE_SALT).pos(x, y).unit() < 0.06 + 0.34 * edge * edge {
                continue;
            }
            cells.push((x, y));
        }
    }
    for (x, y) in knit(cells, apex_x) {
        parts.push(Growth {
            x,
            y,
            material: kind.foliage,
        });
    }
}

fn grow_cap(
    kind: &Species,
    rng: &mut Rng,
    apex_x: i32,
    top: i32,
    width: i32,
    parts: &mut Vec<Growth>,
) {
    let radius = (width * rng.draw().range(3, 5)).max(len(7));
    let rise = (radius as f32 * 0.55).max(len(4) as f32);
    let rim = top - (rise * 0.8) as i32;
    let mut previous = rim;
    for dx in -radius..=radius {
        let offset = dx as f32 / radius as f32;
        let arc = (1.0 - offset * offset).max(0.0).sqrt();
        let crest = rim + (rise * arc) as i32;
        let thickness = len(2) + (arc * len(3) as f32) as i32;
        let under = previous.min(crest) - thickness;
        for y in under..=previous.max(crest) {
            parts.push(Growth {
                x: apex_x + dx,
                y,
                material: kind.foliage,
            });
        }
        if let Some(gills) = kind.gills {
            let flank = offset.abs();
            let droop = if (0.30..0.88).contains(&flank) {
                len(rng.draw().range(2, 5))
            } else {
                len(1)
            };
            for step in 1..=droop {
                parts.push(Growth {
                    x: apex_x + dx,
                    y: under - step,
                    material: gills,
                });
            }
        }
        previous = crest;
    }
}
