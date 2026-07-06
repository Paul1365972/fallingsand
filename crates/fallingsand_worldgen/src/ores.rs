use crate::biomes::{ORE_ANCHOR_GRID, ORE_MARGIN, WorldDef};
use crate::noise::{Xorshift, hash2};
use fallingsand_core::MaterialId;

pub struct VeinCell {
    pub x: i32,
    pub y: i32,
    pub material: MaterialId,
}

pub fn veins_for_rect(
    seed: u64,
    def: &WorldDef,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
) -> Vec<VeinCell> {
    let mut cells = Vec::new();
    let anchor_min_x = (min_x - ORE_MARGIN).div_euclid(ORE_ANCHOR_GRID);
    let anchor_max_x = (max_x + ORE_MARGIN).div_euclid(ORE_ANCHOR_GRID);
    let anchor_min_y = (min_y - ORE_MARGIN).div_euclid(ORE_ANCHOR_GRID);
    let anchor_max_y = (max_y + ORE_MARGIN).div_euclid(ORE_ANCHOR_GRID);
    for anchor_y in anchor_min_y..=anchor_max_y {
        for anchor_x in anchor_min_x..=anchor_max_x {
            let hash = hash2(seed, "ore", anchor_x, anchor_y);
            let mut rng = Xorshift::new(hash);
            let center_x = anchor_x * ORE_ANCHOR_GRID + rng.range(0, ORE_ANCHOR_GRID - 1);
            let center_y = anchor_y * ORE_ANCHOR_GRID + rng.range(0, ORE_ANCHOR_GRID - 1);
            let roll = rng.unit();
            let Some(ore) = def
                .ores
                .iter()
                .find(|ore| center_y >= ore.min_y && center_y <= ore.max_y && roll < ore.chance)
            else {
                continue;
            };
            let steps = rng.range(ore.steps.0, ore.steps.1);
            let mut angle = rng.unit() * std::f32::consts::TAU;
            let (mut walk_x, mut walk_y) = (center_x as f32, center_y as f32);
            for _ in 0..steps {
                let radius = rng.range(ore.radius.0, ore.radius.1);
                let (cx, cy) = (walk_x.round() as i32, walk_y.round() as i32);
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        if dx * dx + dy * dy > radius * radius {
                            continue;
                        }
                        let (x, y) = (cx + dx, cy + dy);
                        if x < min_x || x > max_x || y < min_y || y > max_y {
                            continue;
                        }
                        cells.push(VeinCell {
                            x,
                            y,
                            material: ore.material,
                        });
                    }
                }
                angle += (rng.unit() - 0.5) * 1.4;
                walk_x += angle.cos() * 1.8;
                walk_y += angle.sin() * 1.8;
            }
        }
    }
    cells
}
