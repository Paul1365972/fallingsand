mod biomes;
mod caves;
mod flora;
mod lattice;
mod mines;
mod noise;
mod remnants;
mod scale;
mod setpieces;
mod structures;
mod terrain;
mod veins;

use biomes::{Biome, SubBiome, biomes};
use caves::{Carve, Caves};
use fallingsand_core::content::material;
use fallingsand_core::{
    CHUNK_SIZE, Cell, CellOffset, CellPos, ChunkOffset, DirtyRect, FOG_TEXEL_CELLS, FogMask,
    FogPos, MaterialId, Phase, REGION_SIZE_CELLS, REGION_SIZE_CHUNKS, Region, RegionPos, content,
};
use fallingsand_math::Hash;
use lattice::{Cells, Lattice, Place};
use scale::len;
use structures::{Build, Site};
use terrain::{SEA_LEVEL, Terrain};
use veins::veins_for_rect;

const CELL_SHADE: Hash = Hash::label("worldgen.cell_shade");

const MARGIN: i32 = len(168);
const SURFACE_REVEAL_DEPTH: i32 = 28;

fn region_index(base_x: i32, base_y: i32, x: i32, y: i32) -> (ChunkOffset, CellOffset) {
    let local = CellPos::new(x - base_x, y - base_y);
    (local.chunk().offset(), local.offset())
}

fn region_get(region: &Region, base_x: i32, base_y: i32, x: i32, y: i32) -> Cell {
    let (chunk, cell) = region_index(base_x, base_y, x, y);
    region.chunks()[chunk.index()].cells()[cell.index()]
}

fn region_set(region: &mut Region, base_x: i32, base_y: i32, x: i32, y: i32, cell: Cell) {
    let (chunk, offset) = region_index(base_x, base_y, x, y);
    region.chunk_mut(chunk).cells_mut()[offset.index()] = cell;
}

pub struct WorldGenerator {
    seed: u64,
    terrain: Terrain,
    lattice: Lattice,
    biomes: Vec<Biome>,
    caves: Caves,
}

struct Sample<'g> {
    generator: &'g WorldGenerator,
    min_x: i32,
    columns: Vec<i32>,
    cells: Cells,
}

impl Sample<'_> {
    fn surface(&self, x: i32) -> i32 {
        let index = x - self.min_x;
        if index >= 0 && (index as usize) < self.columns.len() {
            self.columns[index as usize]
        } else {
            self.generator.terrain.height(x)
        }
    }

    fn depth(&self, x: i32, y: i32) -> f32 {
        self.generator
            .terrain
            .depth(x as f32, y as f32, self.surface(x))
    }

    fn place(&self, x: i32, y: i32) -> Place {
        let generator = self.generator;
        let depth = self.depth(x, y);
        let (u, v) = generator.lattice.coords(x as f32, y as f32, depth);
        self.cells.nearest(u, v).unwrap_or_else(|| {
            generator.lattice.place_at(
                &generator.terrain,
                &generator.biomes,
                x as f32,
                y as f32,
                depth,
            )
        })
    }

    fn sub(&self, x: i32, y: i32) -> &SubBiome {
        let place = self.place(x, y);
        &self.generator.biomes[place.biome as usize].members[place.sub as usize]
    }
}

struct Ctx<'g> {
    sample: Sample<'g>,
    carve: Carve,
}

impl Ctx<'_> {
    fn below(&self, x: i32, y: i32) -> f32 {
        let base = (self.sample.surface(x) - y) as f32;
        self.sample.generator.terrain.lip(x, y, base)
    }

    fn is_rock(&self, x: i32, y: i32) -> bool {
        self.below(x, y) > 0.0 && !self.carve.is_open(x, y)
    }

    fn ground_at(&self, x: i32) -> Option<i32> {
        let surface = self.sample.surface(x);
        let mut y = surface + len(30);
        while y >= surface - len(16) {
            if self.is_rock(x, y) {
                return Some(y);
            }
            y -= 1;
        }
        None
    }

    fn undercut(&self, x: i32, y: i32) -> bool {
        self.carve.is_open(x, y - 1) || self.carve.is_open(x + 1, y) || self.carve.is_open(x - 1, y)
    }

    fn strata(&self, sub: &SubBiome, x: i32, y: i32, below: f32) -> MaterialId {
        let terrain = &self.sample.generator.terrain;
        if let Some(skin) = sub.skin {
            let scale = terrain.mantle_scale(x);
            let cover = len(skin.cover_depth) as f32 * scale;
            let soil = cover + len(skin.soil_depth) as f32 * scale;
            let subsoil = soil + len(skin.subsoil_depth) as f32 * scale;
            if below <= cover {
                return skin.cover;
            }
            if below <= soil {
                return skin.soil;
            }
            if below <= subsoil {
                return skin.subsoil;
            }
        }
        if terrain.bedded(x, y) {
            sub.streak
        } else {
            sub.stone
        }
    }

    fn above_floor(&self, x: i32, y: i32, cap: i32) -> Option<i32> {
        for step in 1..=cap {
            if self.is_rock(x, y - step) {
                return Some(step - 1);
            }
        }
        None
    }

    fn below_ceiling(&self, x: i32, y: i32, cap: i32) -> Option<i32> {
        for step in 1..=cap {
            if self.is_rock(x, y + step) {
                return Some(step - 1);
            }
        }
        None
    }

    fn cell_at(&self, x: i32, y: i32) -> Cell {
        let generator = self.sample.generator;
        let below = self.below(x, y);

        if below <= 0.0 {
            if y <= SEA_LEVEL {
                return generator.shaded(material::WATER, x, y);
            }
            return Cell::AIR;
        }

        let sub = self.sample.sub(x, y);

        if !self.carve.is_open(x, y) {
            let chosen = self.strata(sub, x, y, below);
            if content::phase(chosen) == Phase::Powder && self.undercut(x, y) {
                return generator.shaded(sub.stone, x, y);
            }
            return generator.shaded(chosen, x, y);
        }

        if let Some(fluid) = sub.fluid
            && y <= sub.fluid_level
        {
            return generator.shaded(fluid, x, y);
        }

        let sediment = generator.caves.sediment_depth(x, y);
        if sediment > 0
            && let Some(height) = self.above_floor(x, y, sediment)
            && height < sediment
        {
            return generator.shaded(sub.sediment, x, y);
        }

        if let Some(gas) = sub.gas
            && generator.caves.gas_pocket(x, y, sub.gas_chance)
        {
            let heavy = fallingsand_core::content::density_milli(gas) > 1200;
            let near = if heavy {
                self.above_floor(x, y, len(14))
            } else {
                self.below_ceiling(x, y, len(14))
            };
            if near.is_some() {
                return generator.shaded(gas, x, y);
            }
        }

        Cell::AIR
    }
}

impl WorldGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            terrain: Terrain::new(seed),
            lattice: Lattice::new(seed),
            biomes: biomes(),
            caves: Caves::new(seed),
        }
    }

    pub fn surface_height(&self, x: i32) -> i32 {
        self.terrain.height(x)
    }

    pub fn location_names(&self, x: i32, y: i32) -> (&'static str, &'static str) {
        let depth = self
            .terrain
            .depth(x as f32, y as f32, self.terrain.height(x));
        let place = self
            .lattice
            .place_at(&self.terrain, &self.biomes, x as f32, y as f32, depth);
        let biome = &self.biomes[place.biome as usize];
        (biome.name, biome.members[place.sub as usize].name)
    }

    fn shaded(&self, chosen: MaterialId, x: i32, y: i32) -> Cell {
        let shade = Hash::seed(self.seed).salt(CELL_SHADE).pos(x, y).bits(4) as u8;
        Cell::new(chosen, shade)
    }

    pub fn generate_region(&self, pos: RegionPos) -> Region {
        let mut region = Region::new();
        let base = pos.base_chunk().base_cell();
        let size = REGION_SIZE_CELLS as i32;
        let min_x = base.x - MARGIN;
        let min_y = base.y - MARGIN;
        let max_x = base.x + size + MARGIN;
        let max_y = base.y + size + MARGIN;

        let mut columns = Vec::with_capacity((max_x - min_x) as usize);
        for x in min_x..max_x {
            columns.push(self.terrain.height(x));
        }
        let highest = *columns.iter().max().unwrap();
        let lowest = *columns.iter().min().unwrap();
        let cells = self.lattice.cells(
            &self.terrain,
            &self.biomes,
            min_x,
            max_x,
            (lowest - max_y) as f32,
            (highest - min_y) as f32,
        );
        let sample = Sample {
            generator: self,
            min_x,
            columns,
            cells,
        };

        let quarries = mines::plan(
            self.seed,
            min_x,
            min_y,
            max_x,
            max_y,
            &|x, y| *sample.sub(x, y),
            &|x| sample.surface(x),
            &|x, y| sample.depth(x, y),
        );
        let mut carve = self.caves.build(
            min_x,
            min_y,
            max_x,
            max_y,
            &|x, y| *sample.sub(x, y),
            &|x| sample.surface(x),
        );
        for quarry in &quarries {
            quarry.carve(&mut carve);
        }
        let ctx = Ctx { sample, carve };

        for chunk_index in 0..REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS {
            let offset = ChunkOffset::from_index(chunk_index);
            let chunk = region.chunk_mut(offset);
            let chunk_base_x = base.x + offset.x as i32 * CHUNK_SIZE as i32;
            let chunk_base_y = base.y + offset.y as i32 * CHUNK_SIZE as i32;
            for local_y in 0..CHUNK_SIZE as u8 {
                let y = chunk_base_y + local_y as i32;
                for local_x in 0..CHUNK_SIZE as u8 {
                    let x = chunk_base_x + local_x as i32;
                    let cell = ctx.cell_at(x, y);
                    if cell.material != MaterialId::AIR {
                        chunk.cells_mut()[CellOffset::new(local_x, local_y).index()] = cell;
                    }
                }
            }
            chunk.change = DirtyRect::FULL;
            chunk.sim = DirtyRect::FULL;
        }

        let inside =
            |x: i32, y: i32| x >= base.x && x < base.x + size && y >= base.y && y < base.y + size;

        for growth in flora::growth_for_rect(
            self.seed,
            &|x| ctx.sample.sub(x, ctx.sample.surface(x)).skin,
            &|x| ctx.ground_at(x),
            min_x,
            max_x,
        ) {
            if !inside(growth.x, growth.y) {
                continue;
            }
            let present = region_get(&region, base.x, base.y, growth.x, growth.y).material;
            if present == MaterialId::AIR || present == material::WATER {
                region_set(
                    &mut region,
                    base.x,
                    base.y,
                    growth.x,
                    growth.y,
                    self.shaded(growth.material, growth.x, growth.y),
                );
            }
        }

        for vein in veins_for_rect(
            self.seed,
            &|x, y| *ctx.sample.sub(x, y),
            base.x,
            base.y,
            base.x + size - 1,
            base.y + size - 1,
        ) {
            let current = region_get(&region, base.x, base.y, vein.x, vein.y);
            if !ctx.sample.sub(vein.x, vein.y).hosts(current.material) {
                continue;
            }
            if vein.tell && current.material == vein.material {
                continue;
            }
            region_set(
                &mut region,
                base.x,
                base.y,
                vein.x,
                vein.y,
                self.shaded(vein.material, vein.x, vein.y),
            );
        }

        let mut builds: Vec<Build> = Vec::new();
        for quarry in &quarries {
            quarry.furnish(self.seed, &mut builds);
        }
        let buried = |x: i32, y: i32| ctx.below(x, y) > 0.0;
        let site = Site::new(&ctx.carve, &buried);
        remnants::remnants_for_rect(
            self.seed,
            &site,
            &|x, y| *ctx.sample.sub(x, y),
            &|x, y| ctx.sample.depth(x, y),
            min_x,
            min_y,
            max_x,
            max_y,
            &mut builds,
        );
        setpieces::pieces_for_rect(
            self.seed,
            &site,
            &|x, y| *ctx.sample.sub(x, y),
            &|x, y| ctx.sample.depth(x, y),
            min_x,
            min_y,
            max_x,
            max_y,
            &mut builds,
        );
        for build in builds {
            if !inside(build.x, build.y) {
                continue;
            }
            let cell = if build.material == MaterialId::AIR {
                Cell::AIR
            } else {
                self.shaded(build.material, build.x, build.y)
            };
            region_set(&mut region, base.x, base.y, build.x, build.y, cell);
        }

        reveal_surface(&mut region, pos, &|x| ctx.sample.surface(x));

        region
    }
}

fn reveal_surface(region: &mut Region, pos: RegionPos, surface: &dyn Fn(i32) -> i32) {
    for (offset, chunk_pos) in pos.chunk_positions() {
        let mut fog = FogMask::EMPTY;
        for (index, texel) in FogPos::in_chunk(chunk_pos) {
            let base = texel.base_cell();
            let daylight = (0..FOG_TEXEL_CELLS as i32)
                .map(|dx| surface(base.x + dx))
                .min()
                .expect("a fog texel spans at least one column");
            let deepest = daylight - SURFACE_REVEAL_DEPTH;
            if base.y + FOG_TEXEL_CELLS as i32 > deepest {
                fog.set(index);
            }
        }
        region.chunk_mut(offset).reveal(&fog);
    }
}
