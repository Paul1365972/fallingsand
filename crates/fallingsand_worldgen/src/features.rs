use crate::biomes::{Canopy, TREE_MARGIN, WorldDef};
use crate::noise::{Xorshift, hash1};
use crate::terrain::Terrain;
use fallingsand_core::MaterialId;

pub struct FeatureCell {
    pub x: i32,
    pub y: i32,
    pub material: MaterialId,
}

const GROUND_SCAN: i32 = 40;
const MAX_SLOPE: i32 = 5;

#[allow(clippy::too_many_arguments)]
pub fn trees_for_rect(
    seed: u64,
    def: &WorldDef,
    terrain: &Terrain,
    solid: &dyn Fn(i32, i32) -> bool,
    surface_of: &dyn Fn(i32) -> i32,
    water_top: &dyn Fn(i32) -> i32,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
) -> Vec<FeatureCell> {
    let biome_count = def.biomes.len();
    let candidate = |x: i32| -> Option<u64> {
        let biome = &def.biomes[terrain.biome_at(biome_count, x)];
        let tree = biome.tree.as_ref()?;
        let hash = hash1(seed, "tree", x);
        (((hash & 0xFFFF) as f32) < tree.density * 65536.0).then_some(hash)
    };
    let ground_of = |x: i32| -> Option<i32> {
        let surface = surface_of(x);
        (surface - GROUND_SCAN..=surface + GROUND_SCAN)
            .rev()
            .find(|&y| solid(x, y))
    };

    let mut cells = Vec::new();
    for x in (min_x - TREE_MARGIN)..=(max_x + TREE_MARGIN) {
        let Some(key) = candidate(x) else {
            continue;
        };
        let biome = &def.biomes[terrain.biome_at(biome_count, x)];
        let tree = biome.tree.as_ref().expect("candidate implies tree");
        let mut winner = true;
        for dx in 1..=tree.spacing {
            let beats_left = candidate(x - dx).is_none_or(|other| other < key);
            let beats_right = candidate(x + dx).is_none_or(|other| other <= key);
            if !beats_left || !beats_right {
                winner = false;
                break;
            }
        }
        if !winner {
            continue;
        }
        let Some(ground) = ground_of(x) else {
            continue;
        };
        if ground <= water_top(x) {
            continue;
        }
        match (ground_of(x - 2), ground_of(x + 2)) {
            (Some(left), Some(right)) if (left - right).abs() <= MAX_SLOPE => {}
            _ => continue,
        }

        let mut rng = Xorshift::new(key);
        let height = rng.range(tree.trunk_height.0, tree.trunk_height.1);
        let mut push = |cx: i32, cy: i32, material: MaterialId| {
            if cx >= min_x && cx <= max_x && cy >= min_y && cy <= max_y {
                cells.push(FeatureCell {
                    x: cx,
                    y: cy,
                    material,
                });
            }
        };
        match tree.canopy {
            Canopy::Round => {
                for dy in 1..=height {
                    push(x, ground + dy, tree.wood);
                    push(x + 1, ground + dy, tree.wood);
                }
                let top = ground + height;
                let rx = rng.range(5, 9);
                let ry = rng.range(4, 7);
                for dy in -ry..=ry {
                    for dx in -rx..=rx {
                        let nx = dx as f32 / rx as f32;
                        let ny = dy as f32 / ry as f32;
                        if nx * nx + ny * ny <= 1.0 {
                            push(x + dx, top + dy, tree.leaves);
                        }
                    }
                }
            }
            Canopy::Conifer => {
                for dy in 1..=height {
                    push(x, ground + dy, tree.wood);
                }
                let apex = ground + height + 3;
                let canopy_rows = (height * 3 / 4).max(6);
                for row in 0..canopy_rows {
                    let half = 1 + row * 3 / 5;
                    let y = apex - row;
                    for dx in -half..=half {
                        push(x + dx, y, tree.leaves);
                    }
                }
            }
        }
    }
    cells
}
