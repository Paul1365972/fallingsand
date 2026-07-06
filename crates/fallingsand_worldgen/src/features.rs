use crate::biomes::{
    Canopy, DECOR_COLUMN_CHANCE, DECOR_SCAN_FLOOR, MUSHROOM_ANCHOR_GRID, MUSHROOM_CHANCE,
    MUSHROOM_MAX_Y, MUSHROOM_MIN_Y, Palette, TREE_MARGIN, VINE_MAX_DEPTH, WorldDef,
};
use crate::noise::{Xorshift, hash1, hash2};
use crate::terrain::Terrain;
use fallingsand_core::MaterialId;

pub struct FeatureCell {
    pub x: i32,
    pub y: i32,
    pub material: MaterialId,
}

const GROUND_SCAN: i32 = 40;
const MAX_SLOPE: i32 = 5;

pub struct Clip {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl Clip {
    fn push(&self, cells: &mut Vec<FeatureCell>, x: i32, y: i32, material: MaterialId) {
        if x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y {
            cells.push(FeatureCell { x, y, material });
        }
    }
}

fn ground_of(solid: &dyn Fn(i32, i32) -> bool, surface: i32, x: i32) -> Option<i32> {
    (surface - GROUND_SCAN..=surface + GROUND_SCAN)
        .rev()
        .find(|&y| solid(x, y))
}

#[allow(clippy::too_many_arguments)]
pub fn trees_for_rect(
    seed: u64,
    def: &WorldDef,
    palette: &Palette,
    terrain: &Terrain,
    solid: &dyn Fn(i32, i32) -> bool,
    surface_of: &dyn Fn(i32) -> i32,
    water_top: &dyn Fn(i32) -> i32,
    clip: &Clip,
) -> Vec<FeatureCell> {
    let biome_count = def.biomes.len();
    let candidate = |x: i32| -> Option<u64> {
        let biome = &def.biomes[terrain.biome_at(biome_count, x)];
        let tree = biome.tree.as_ref()?;
        let hash = hash1(seed, "tree", x);
        (((hash & 0xFFFF) as f32) < tree.density * 65536.0).then_some(hash)
    };

    let mut cells = Vec::new();
    for x in (clip.min_x - TREE_MARGIN)..=(clip.max_x + TREE_MARGIN) {
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
        let Some(ground) = ground_of(solid, surface_of(x), x) else {
            continue;
        };
        if ground <= water_top(x) {
            continue;
        }
        match (
            ground_of(solid, surface_of(x - 2), x - 2),
            ground_of(solid, surface_of(x + 2), x + 2),
        ) {
            (Some(left), Some(right)) if (left - right).abs() <= MAX_SLOPE => {}
            _ => continue,
        }

        let mut rng = Xorshift::new(key);
        let height = rng.range(tree.trunk_height.0, tree.trunk_height.1);
        let mut canopy_top: Vec<(i32, i32)> = Vec::new();
        let mut leaf = |cells: &mut Vec<FeatureCell>, cx: i32, cy: i32| {
            clip.push(cells, cx, cy, tree.leaves);
            match canopy_top.iter_mut().find(|(x, _)| *x == cx) {
                Some((_, top)) => *top = (*top).max(cy),
                None => canopy_top.push((cx, cy)),
            }
        };
        match tree.canopy {
            Canopy::Round => {
                for dy in 1..=height {
                    clip.push(&mut cells, x, ground + dy, tree.wood);
                    clip.push(&mut cells, x + 1, ground + dy, tree.wood);
                }
                let top = ground + height;
                let rx = rng.range(5, 9);
                let ry = rng.range(4, 7);
                for dy in -ry..=ry {
                    for dx in -rx..=rx {
                        let nx = dx as f32 / rx as f32;
                        let ny = dy as f32 / ry as f32;
                        if nx * nx + ny * ny <= 1.0 {
                            leaf(&mut cells, x + dx, top + dy);
                        }
                    }
                }
            }
            Canopy::Conifer => {
                for dy in 1..=height {
                    clip.push(&mut cells, x, ground + dy, tree.wood);
                }
                let apex = ground + height + 3;
                let canopy_rows = (height * 3 / 4).max(6);
                for row in 0..canopy_rows {
                    let half = 1 + row * 3 / 5;
                    let y = apex - row;
                    for dx in -half..=half {
                        leaf(&mut cells, x + dx, y);
                    }
                }
            }
        }
        if tree.snow_capped {
            for &(cx, top) in &canopy_top {
                clip.push(&mut cells, cx, top + 1, palette.snow);
            }
        }
    }
    cells
}

#[allow(clippy::too_many_arguments)]
pub fn cacti_for_rect(
    seed: u64,
    def: &WorldDef,
    palette: &Palette,
    terrain: &Terrain,
    solid: &dyn Fn(i32, i32) -> bool,
    surface_of: &dyn Fn(i32) -> i32,
    water_top: &dyn Fn(i32) -> i32,
    clip: &Clip,
) -> Vec<FeatureCell> {
    let biome_count = def.biomes.len();
    let candidate = |x: i32| -> Option<u64> {
        let biome = &def.biomes[terrain.biome_at(biome_count, x)];
        if biome.cactus_chance <= 0.0 {
            return None;
        }
        let hash = hash1(seed, "cactus", x);
        (((hash & 0xFFFF) as f32) < biome.cactus_chance * 65536.0).then_some(hash)
    };
    let mut cells = Vec::new();
    for x in (clip.min_x - 4)..=(clip.max_x + 4) {
        let Some(key) = candidate(x) else {
            continue;
        };
        let mut winner = true;
        for dx in 1..=4 {
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
        let Some(ground) = ground_of(solid, surface_of(x), x) else {
            continue;
        };
        if ground <= water_top(x) {
            continue;
        }
        let mut rng = Xorshift::new(key);
        let height = rng.range(4, 11);
        for dy in 1..=height {
            clip.push(&mut cells, x, ground + dy, palette.cactus);
        }
    }
    cells
}

#[allow(clippy::too_many_arguments)]
pub fn decorations_for_rect(
    seed: u64,
    def: &WorldDef,
    palette: &Palette,
    terrain: &Terrain,
    solid: &dyn Fn(i32, i32) -> bool,
    surface_of: &dyn Fn(i32) -> i32,
    clip: &Clip,
) -> Vec<FeatureCell> {
    let biome_count = def.biomes.len();
    let mut cells = Vec::new();
    for x in clip.min_x..=clip.max_x {
        let column_hash = hash1(seed, "decor", x);
        if ((column_hash & 0xFF) as f32) >= DECOR_COLUMN_CHANCE * 256.0 {
            continue;
        }
        let biome = &def.biomes[terrain.biome_at(biome_count, x)];
        let surface = surface_of(x);
        let scan_top = (clip.max_y + 10).min(surface - 2);
        let scan_bottom = (clip.min_y - 10).max(DECOR_SCAN_FLOOR);
        if scan_bottom > scan_top {
            continue;
        }
        let mut above_solid = solid(x, scan_top + 1);
        for y in (scan_bottom..=scan_top).rev() {
            let here = solid(x, y);
            if !here && above_solid {
                ceiling_site(seed, palette, biome, x, y, surface, &mut cells, clip);
            }
            if here && !above_solid {
                floor_site(seed, palette, biome, x, y + 1, surface, &mut cells, clip);
            }
            above_solid = here;
        }
    }
    cells
}

fn spike_material(
    palette: &Palette,
    biome: &crate::biomes::Biome,
    y: i32,
    depth: i32,
    roll: u64,
) -> MaterialId {
    if biome.snow_cover && depth < 70 {
        palette.ice
    } else if y < -380 && (roll & 0xFF) < 64 {
        palette.crystal
    } else if y < -350 {
        palette.deepstone
    } else {
        palette.stone
    }
}

#[allow(clippy::too_many_arguments)]
fn ceiling_site(
    seed: u64,
    palette: &Palette,
    biome: &crate::biomes::Biome,
    x: i32,
    top_air: i32,
    surface: i32,
    cells: &mut Vec<FeatureCell>,
    clip: &Clip,
) {
    let hash = hash2(seed, "ceiling", x, top_air);
    let depth = surface - top_air;
    if depth < VINE_MAX_DEPTH
        && biome.vine_chance > 0.0
        && ((hash & 0xFF) as f32) < biome.vine_chance * 256.0
    {
        let length = 3 + ((hash >> 8) % 8) as i32;
        for dy in 0..length {
            clip.push(cells, x, top_air - dy, palette.moss);
        }
        return;
    }
    if (hash >> 16) & 0xFF < 115 {
        let length = 2 + ((hash >> 24) % 6) as i32;
        let material = spike_material(palette, biome, top_air, depth, hash >> 32);
        for dy in 0..length {
            clip.push(cells, x, top_air - dy, material);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn floor_site(
    seed: u64,
    palette: &Palette,
    biome: &crate::biomes::Biome,
    x: i32,
    bottom_air: i32,
    surface: i32,
    cells: &mut Vec<FeatureCell>,
    clip: &Clip,
) {
    let hash = hash2(seed, "floor", x, bottom_air);
    if (hash & 0xFF) < 90 {
        let length = 2 + ((hash >> 8) % 4) as i32;
        let depth = surface - bottom_air;
        let material = spike_material(palette, biome, bottom_air, depth, hash >> 16);
        for dy in 0..length {
            clip.push(cells, x, bottom_air + dy, material);
        }
    }
}

pub fn mushrooms_for_rect(
    seed: u64,
    palette: &Palette,
    solid: &dyn Fn(i32, i32) -> bool,
    clip: &Clip,
) -> Vec<FeatureCell> {
    let grid = MUSHROOM_ANCHOR_GRID;
    let mut cells = Vec::new();
    let margin = 24;
    let anchor_min_x = (clip.min_x - margin).div_euclid(grid);
    let anchor_max_x = (clip.max_x + margin).div_euclid(grid);
    let anchor_min_y = (clip.min_y - margin)
        .div_euclid(grid)
        .max(MUSHROOM_MIN_Y.div_euclid(grid));
    let anchor_max_y = (clip.max_y + margin)
        .div_euclid(grid)
        .min(MUSHROOM_MAX_Y.div_euclid(grid));
    for anchor_y in anchor_min_y..=anchor_max_y {
        for anchor_x in anchor_min_x..=anchor_max_x {
            let hash = hash2(seed, "mushroom", anchor_x, anchor_y);
            if ((hash & 0xFF) as f32) >= MUSHROOM_CHANCE * 256.0 {
                continue;
            }
            let mut rng = Xorshift::new(hash);
            let x = anchor_x * grid + rng.range(0, grid - 1);
            let start = anchor_y * grid + rng.range(0, grid - 1);
            let mut floor_air = None;
            for y in (start - 20..=start + 20).rev() {
                if !solid(x, y) && solid(x, y - 1) {
                    floor_air = Some(y);
                    break;
                }
            }
            let Some(base) = floor_air else {
                continue;
            };
            let stem = rng.range(5, 12);
            let cap_rx = rng.range(3, 6);
            let cap_ry = rng.range(2, 3);
            let headroom = stem + cap_ry + 2;
            if (0..headroom).any(|dy| solid(x, base + dy)) {
                continue;
            }
            let wide = rng.step() & 1 == 0;
            for dy in 0..stem {
                clip.push(&mut cells, x, base + dy, palette.mushroom_stem);
                if wide {
                    clip.push(&mut cells, x + 1, base + dy, palette.mushroom_stem);
                }
            }
            let cap_center = base + stem;
            for dy in -1..=cap_ry {
                for dx in -cap_rx..=cap_rx {
                    let nx = dx as f32 / cap_rx as f32;
                    let ny = dy as f32 / (cap_ry as f32 + 1.0);
                    if nx * nx + ny * ny <= 1.0 {
                        clip.push(&mut cells, x + dx, cap_center + dy, palette.glowshroom);
                    }
                }
            }
        }
    }
    cells
}
