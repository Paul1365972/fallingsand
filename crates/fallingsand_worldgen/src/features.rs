use crate::biomes::{
    Canopy, DECOR_COLUMN_CHANCE, DECOR_SCAN_FLOOR, MUSHROOM_ANCHOR_GRID, MUSHROOM_CHANCE,
    MUSHROOM_MAX_Y, MUSHROOM_MIN_Y, TREE_MARGIN, VINE_MAX_DEPTH, WorldDef,
};
use crate::terrain::Terrain;
use fallingsand_core::MaterialId;
use fallingsand_data::material;
use fallingsand_rng::Hash;

pub(crate) struct FeatureCell {
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
pub(crate) fn trees_for_rect(
    seed: u64,
    def: &WorldDef,
    terrain: &Terrain,
    solid: &dyn Fn(i32, i32) -> bool,
    surface_of: &dyn Fn(i32) -> i32,
    water_top: &dyn Fn(i32) -> i32,
    clip: &Clip,
) -> Vec<FeatureCell> {
    let biome_count = def.biomes.len();
    let candidate = |x: i32| -> Option<Hash> {
        let biome = &def.biomes[terrain.biome_at(biome_count, x)];
        let tree = biome.tree.as_ref()?;
        let hash = Hash::seed(seed).bytes(b"tree").pos(x, 0);
        hash.chance(tree.density).then_some(hash)
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

        let mut rng = key.rng();
        let height = rng.draw().range(tree.trunk_height.0, tree.trunk_height.1);
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
                let rx = rng.draw().range(5, 9);
                let ry = rng.draw().range(4, 7);
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
                clip.push(&mut cells, cx, top + 1, material::SNOW);
            }
        }
    }
    cells
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn decorations_for_rect(
    seed: u64,
    def: &WorldDef,
    terrain: &Terrain,
    solid: &dyn Fn(i32, i32) -> bool,
    surface_of: &dyn Fn(i32) -> i32,
    clip: &Clip,
) -> Vec<FeatureCell> {
    let biome_count = def.biomes.len();
    let mut cells = Vec::new();
    for x in clip.min_x..=clip.max_x {
        if !Hash::seed(seed)
            .bytes(b"decor")
            .pos(x, 0)
            .chance(DECOR_COLUMN_CHANCE)
        {
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
                ceiling_site(seed, biome, x, y, surface, &mut cells, clip);
            }
            if here && !above_solid {
                floor_site(seed, biome, x, y + 1, surface, &mut cells, clip);
            }
            above_solid = here;
        }
    }
    cells
}

fn spike_material(biome: &crate::biomes::Biome, y: i32, depth: i32, roll: Hash) -> MaterialId {
    if biome.snow_cover && depth < 70 {
        material::ICE
    } else if y < -380 && roll.chance(0.25) {
        material::CRYSTAL
    } else if y < -350 {
        material::DEEPSTONE
    } else {
        material::STONE
    }
}

#[allow(clippy::too_many_arguments)]
fn ceiling_site(
    seed: u64,
    biome: &crate::biomes::Biome,
    x: i32,
    top_air: i32,
    surface: i32,
    cells: &mut Vec<FeatureCell>,
    clip: &Clip,
) {
    let mut rng = Hash::seed(seed).bytes(b"ceiling").pos(x, top_air).rng();
    let depth = surface - top_air;
    if depth < VINE_MAX_DEPTH && rng.draw().chance(biome.vine_chance) {
        let length = rng.draw().range(3, 10);
        for dy in 0..length {
            clip.push(cells, x, top_air - dy, material::MOSS);
        }
        return;
    }
    if rng.draw().chance(0.45) {
        let length = rng.draw().range(2, 7);
        let spike = spike_material(biome, top_air, depth, rng.draw());
        for dy in 0..length {
            clip.push(cells, x, top_air - dy, spike);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn floor_site(
    seed: u64,
    biome: &crate::biomes::Biome,
    x: i32,
    bottom_air: i32,
    surface: i32,
    cells: &mut Vec<FeatureCell>,
    clip: &Clip,
) {
    let mut rng = Hash::seed(seed).bytes(b"floor").pos(x, bottom_air).rng();
    if rng.draw().chance(0.35) {
        let length = rng.draw().range(2, 5);
        let depth = surface - bottom_air;
        let spike = spike_material(biome, bottom_air, depth, rng.draw());
        for dy in 0..length {
            clip.push(cells, x, bottom_air + dy, spike);
        }
    }
}

pub(crate) fn mushrooms_for_rect(
    seed: u64,
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
            let mut rng = Hash::seed(seed)
                .bytes(b"mushroom")
                .pos(anchor_x, anchor_y)
                .rng();
            if !rng.draw().chance(MUSHROOM_CHANCE) {
                continue;
            }
            let x = anchor_x * grid + rng.draw().range(0, grid - 1);
            let start = anchor_y * grid + rng.draw().range(0, grid - 1);
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
            if !(MUSHROOM_MIN_Y..=MUSHROOM_MAX_Y).contains(&base) {
                continue;
            }
            let stem = rng.draw().range(5, 12);
            let cap_rx = rng.draw().range(3, 6);
            let cap_ry = rng.draw().range(2, 3);
            let headroom = stem + cap_ry + 2;
            if (0..headroom).any(|dy| solid(x, base + dy)) {
                continue;
            }
            let wide = rng.draw().bit();
            for dy in 0..stem {
                clip.push(&mut cells, x, base + dy, material::MUSHROOM_STEM);
                if wide {
                    clip.push(&mut cells, x + 1, base + dy, material::MUSHROOM_STEM);
                }
            }
            let cap_center = base + stem;
            for dy in -1..=cap_ry {
                for dx in -cap_rx..=cap_rx {
                    let nx = dx as f32 / cap_rx as f32;
                    let ny = dy as f32 / (cap_ry as f32 + 1.0);
                    if nx * nx + ny * ny <= 1.0 {
                        clip.push(&mut cells, x + dx, cap_center + dy, material::GLOWSHROOM);
                    }
                }
            }
        }
    }
    cells
}
