use crate::biomes::{
    ISLAND_ANCHOR_X, ISLAND_ANCHOR_Y, ISLAND_CHANCE, ISLAND_MAX_Y, ISLAND_MIN_Y,
    MINESHAFT_ANCHOR_X, MINESHAFT_ANCHOR_Y, MINESHAFT_CHANCE, MINESHAFT_MAX_Y, MINESHAFT_MIN_Y,
    Palette, RUIN_ANCHOR_GRID, RUIN_CHANCE, STRUCTURE_MARGIN,
};
use crate::features::Clip;
use crate::noise::{Xorshift, hash1, hash2};
use fallingsand_core::MaterialId;

pub struct StructureCell {
    pub x: i32,
    pub y: i32,
    pub material: MaterialId,
    pub replace: bool,
}

struct Builder<'c> {
    cells: Vec<StructureCell>,
    clip: &'c Clip,
}

impl Builder<'_> {
    fn put(&mut self, x: i32, y: i32, material: MaterialId, replace: bool) {
        if x >= self.clip.min_x
            && x <= self.clip.max_x
            && y >= self.clip.min_y
            && y <= self.clip.max_y
        {
            self.cells.push(StructureCell {
                x,
                y,
                material,
                replace,
            });
        }
    }
}

const GROUND_SCAN: i32 = 48;

#[allow(clippy::too_many_arguments)]
pub fn structures_for_rect(
    seed: u64,
    palette: &Palette,
    solid: &dyn Fn(i32, i32) -> bool,
    surface_of: &dyn Fn(i32) -> i32,
    water_top: &dyn Fn(i32) -> i32,
    covered: &dyn Fn(i32, i32) -> bool,
    clip: &Clip,
) -> Vec<StructureCell> {
    let mut builder = Builder {
        cells: Vec::new(),
        clip,
    };
    ruins(
        seed,
        palette,
        solid,
        surface_of,
        water_top,
        covered,
        &mut builder,
    );
    mineshafts(seed, palette, &mut builder);
    islands(seed, palette, &mut builder);
    builder.cells
}

#[allow(clippy::too_many_arguments)]
fn ruins(
    seed: u64,
    palette: &Palette,
    solid: &dyn Fn(i32, i32) -> bool,
    surface_of: &dyn Fn(i32) -> i32,
    water_top: &dyn Fn(i32) -> i32,
    covered: &dyn Fn(i32, i32) -> bool,
    builder: &mut Builder,
) {
    let grid = RUIN_ANCHOR_GRID;
    let anchor_min = (builder.clip.min_x - STRUCTURE_MARGIN).div_euclid(grid);
    let anchor_max = (builder.clip.max_x + STRUCTURE_MARGIN).div_euclid(grid);
    for anchor in anchor_min..=anchor_max {
        let hash = hash1(seed, "ruin", anchor);
        if ((hash & 0xFF) as f32) >= RUIN_CHANCE * 256.0 {
            continue;
        }
        let mut rng = Xorshift::new(hash);
        let center = anchor * grid + grid / 4 + rng.range(0, grid / 2);
        let surface = surface_of(center);
        let Some(ground) = (surface - GROUND_SCAN..=surface + GROUND_SCAN)
            .rev()
            .find(|&y| solid(center, y))
        else {
            continue;
        };
        if ground <= water_top(center) {
            continue;
        }
        if rng.step() & 1 == 0 {
            shack(palette, &mut rng, center, ground, covered, builder);
        } else {
            tower(palette, &mut rng, center, ground, covered, builder);
        }
    }
}

fn shack(
    palette: &Palette,
    rng: &mut Xorshift,
    center: i32,
    ground: i32,
    covered: &dyn Fn(i32, i32) -> bool,
    builder: &mut Builder,
) {
    let half_w = rng.range(5, 8);
    let height = rng.range(5, 7);
    let door = if rng.step() & 1 == 0 { -1 } else { 1 };
    for dx in -half_w..=half_w {
        let x = center + dx;
        if rng.unit() > 0.12 {
            builder.put(x, ground, palette.planks, true);
        }
        if dx.abs() < half_w {
            for dy in 1..height {
                if !covered(x, ground + dy) {
                    builder.put(x, ground + dy, MaterialId::AIR, true);
                }
            }
        }
    }
    for side in [-1, 1] {
        let x = center + side * half_w;
        for dy in 1..height {
            if side == door && dy <= 3 {
                continue;
            }
            if rng.unit() > 0.18 {
                builder.put(x, ground + dy, palette.planks, true);
            }
        }
    }
    for dx in -(half_w + 1)..=(half_w + 1) {
        if rng.unit() > 0.2 {
            builder.put(center + dx, ground + height, palette.planks, true);
        }
    }
}

fn tower(
    palette: &Palette,
    rng: &mut Xorshift,
    center: i32,
    ground: i32,
    covered: &dyn Fn(i32, i32) -> bool,
    builder: &mut Builder,
) {
    let half_w = rng.range(4, 6);
    let height = rng.range(18, 30);
    let door = if rng.step() & 1 == 0 { -1 } else { 1 };
    for dy in 0..=height {
        let y = ground + dy;
        let ruin_chance = 0.05 + 0.3 * (dy as f32 / height as f32).powi(2);
        for dx in -half_w..=half_w {
            let x = center + dx;
            let wall = dx.abs() >= half_w - 1;
            if wall {
                if (1..=3).contains(&dy) && dx.signum() == door {
                    continue;
                }
                if rng.unit() > ruin_chance {
                    builder.put(x, y, palette.brick, true);
                }
            } else if dy > 0 && dy % 8 == 0 {
                builder.put(x, y, palette.planks, true);
            } else if dy > 0 && !covered(x, y) {
                builder.put(x, y, MaterialId::AIR, true);
            }
        }
    }
    for dy in 1..=2 {
        for dx in -half_w..=half_w {
            if (dx + half_w) % 4 < 2 && rng.unit() > 0.25 {
                builder.put(center + dx, ground + height + dy, palette.brick, true);
            }
        }
    }
}

fn mineshafts(seed: u64, palette: &Palette, builder: &mut Builder) {
    let anchor_min_x = (builder.clip.min_x - STRUCTURE_MARGIN).div_euclid(MINESHAFT_ANCHOR_X);
    let anchor_max_x = (builder.clip.max_x + STRUCTURE_MARGIN).div_euclid(MINESHAFT_ANCHOR_X);
    let anchor_min_y = (builder.clip.min_y - 32).div_euclid(MINESHAFT_ANCHOR_Y);
    let anchor_max_y = (builder.clip.max_y + 32).div_euclid(MINESHAFT_ANCHOR_Y);
    for anchor_y in anchor_min_y..=anchor_max_y {
        for anchor_x in anchor_min_x..=anchor_max_x {
            let hash = hash2(seed, "mineshaft", anchor_x, anchor_y);
            if ((hash & 0xFF) as f32) >= MINESHAFT_CHANCE * 256.0 {
                continue;
            }
            let mut rng = Xorshift::new(hash);
            let start_x = anchor_x * MINESHAFT_ANCHOR_X + rng.range(0, MINESHAFT_ANCHOR_X - 1);
            let start_y = anchor_y * MINESHAFT_ANCHOR_Y + rng.range(0, MINESHAFT_ANCHOR_Y - 1);
            if !(MINESHAFT_MIN_Y..=MINESHAFT_MAX_Y).contains(&start_y) {
                continue;
            }
            let dir = if rng.step() & 1 == 0 { 1 } else { -1 };
            let length = rng.range(90, 170);
            let mut carve: Vec<(i32, i32)> = Vec::new();
            let mut furnish: Vec<(i32, i32, MaterialId)> = Vec::new();
            let mut floor = start_y;
            for i in 0..length {
                let x = start_x + dir * i;
                if i % 14 == 13 {
                    floor += rng.range(-1, 1);
                }
                for dy in 1..=4 {
                    carve.push((x, floor + dy));
                }
                furnish.push((x, floor, palette.planks));
                if i % 12 == 6 {
                    for dy in 1..=3 {
                        furnish.push((x, floor + dy, palette.wood));
                    }
                    for dx in -1..=1 {
                        furnish.push((x + dx, floor + 4, palette.wood));
                    }
                }
                if rng.unit() < 0.03 {
                    furnish.push((x, floor + 1, palette.coal));
                }
            }
            for (x, y) in carve {
                builder.put(x, y, MaterialId::AIR, true);
            }
            for (x, y, material) in furnish {
                builder.put(x, y, material, true);
            }
        }
    }
}

fn islands(seed: u64, palette: &Palette, builder: &mut Builder) {
    let anchor_min_x = (builder.clip.min_x - STRUCTURE_MARGIN).div_euclid(ISLAND_ANCHOR_X);
    let anchor_max_x = (builder.clip.max_x + STRUCTURE_MARGIN).div_euclid(ISLAND_ANCHOR_X);
    let anchor_min_y = (builder.clip.min_y - 64)
        .div_euclid(ISLAND_ANCHOR_Y)
        .max(ISLAND_MIN_Y.div_euclid(ISLAND_ANCHOR_Y));
    let anchor_max_y = (builder.clip.max_y + 64)
        .div_euclid(ISLAND_ANCHOR_Y)
        .min(ISLAND_MAX_Y.div_euclid(ISLAND_ANCHOR_Y));
    for anchor_y in anchor_min_y..=anchor_max_y {
        for anchor_x in anchor_min_x..=anchor_max_x {
            let hash = hash2(seed, "island", anchor_x, anchor_y);
            if ((hash & 0xFF) as f32) >= ISLAND_CHANCE * 256.0 {
                continue;
            }
            let mut rng = Xorshift::new(hash);
            let center_x = anchor_x * ISLAND_ANCHOR_X + rng.range(0, ISLAND_ANCHOR_X - 1);
            let center_y = anchor_y * ISLAND_ANCHOR_Y + rng.range(0, ISLAND_ANCHOR_Y - 1);
            let rx = rng.range(22, 56);
            let ry_top = rx / 4 + 2;
            let ry_bottom = rx / 2 + 3;
            for dx in -rx..=rx {
                let nx = dx as f32 / rx as f32;
                let bulge = (1.0 - nx * nx).max(0.0);
                let top = (ry_top as f32 * bulge.sqrt()) as i32;
                let bottom = (ry_bottom as f32 * bulge) as i32;
                for dy in -bottom..=top {
                    let from_top = top - dy;
                    let material = if from_top < 1 {
                        palette.grass
                    } else if from_top < 5 {
                        palette.dirt
                    } else {
                        palette.stone
                    };
                    builder.put(center_x + dx, center_y + dy, material, false);
                }
            }
            if rng.unit() < 0.5 {
                let gold_x = center_x + rng.range(-rx / 3, rx / 3);
                let gold_y = center_y - rng.range(1, ry_bottom / 2);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx * dx + dy * dy <= 2 {
                            builder.put(gold_x + dx, gold_y + dy, palette.gold_ore, true);
                        }
                    }
                }
            }
            if rx > 30 && rng.unit() < 0.6 {
                let top = center_y + ry_top;
                let trunk = rng.range(8, 14);
                for dy in 1..=trunk {
                    builder.put(center_x, top + dy, palette.wood, false);
                }
                let canopy = rng.range(4, 6);
                for dy in -canopy..=canopy {
                    for dx in -canopy..=canopy {
                        if dx * dx + dy * dy <= canopy * canopy {
                            builder.put(center_x + dx, top + trunk + dy, palette.leaves, false);
                        }
                    }
                }
            }
        }
    }
}
