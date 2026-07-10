mod biomes;
mod caves;
mod features;
mod noise;
mod ores;
mod structures;
mod terrain;
mod water;

use biomes::{
    BEACH_DEPTH, BEACH_RANGE, Band, Beach, MAX_OVERHANG, MOSS_CHANCE, MOSS_MAX_DEPTH,
    OVERHANG_AMPLITUDE, OVERHANG_FADE, Palette, SNOW_COVER_DEPTH, SNOW_LINE, WorldDef, world_def,
};
use caves::Caves;
use fallingsand_core::{
    Cell, CellOffset, ChunkOffset, DirtyRect, MaterialId, MaterialRegistry, REGION_SIZE_CELLS,
    REGION_SIZE_CHUNKS, Region, RegionPos,
};
use fallingsand_rng::Hash;
use noise::Cached;
use terrain::Terrain;
use water::Waters;

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("worldgen references unknown material {0:?}")]
    UnknownMaterial(String),
}

pub struct WorldGenerator {
    seed: u64,
    def: WorldDef,
    palette: Palette,
    terrain: Terrain,
    caves: Caves,
    waters: Waters,
}

const MARGIN: i32 = 64;

#[derive(Clone, Copy)]
struct Column {
    surface: i32,
    biome: usize,
    rugged: f32,
    pond_level: Option<i32>,
    tuft_height: i32,
    shaft: f32,
    canyon: f32,
    beach: bool,
}

struct Ctx<'g> {
    generator: &'g WorldGenerator,
    min_x: i32,
    columns: Vec<Column>,
    shape: Cached<'g>,
    tunnel_a: Cached<'g>,
    tunnel_b: Cached<'g>,
    cheese: Cached<'g>,
    rarity: Cached<'g>,
    aquifer: Cached<'g>,
    lava: Cached<'g>,
}

impl Ctx<'_> {
    fn column(&self, x: i32) -> Column {
        let index = x - self.min_x;
        if index >= 0 && (index as usize) < self.columns.len() {
            self.columns[index as usize]
        } else {
            self.generator.build_column(x)
        }
    }

    fn solid_depth(&self, column: &Column, x: i32, y: i32) -> f32 {
        let base = (column.surface - y) as f32;
        let over = base.abs() / MAX_OVERHANG as f32;
        if over >= 1.0 {
            return base;
        }
        let falloff = ((1.0 - over) / OVERHANG_FADE).min(1.0);
        base + self.shape.at(x, y) * OVERHANG_AMPLITUDE * column.rugged * falloff
    }

    fn cave_sample(&self, x: i32, y: i32) -> caves::CaveSample {
        caves::CaveSample {
            tunnel_a: self.tunnel_a.at(x, y),
            tunnel_b: self.tunnel_b.at(x, y),
            cheese: self.cheese.at(x, y),
            rarity: self.rarity.at(x, y),
        }
    }

    fn carved(&self, column: &Column, band: &Band, x: i32, y: i32, depth: f32) -> bool {
        caves::carved(
            self.cave_sample(x, y),
            depth,
            band,
            column.shaft,
            column.canyon,
        )
    }

    fn solid_at(&self, x: i32, y: i32) -> bool {
        let column = self.column(x);
        let depth = self.solid_depth(&column, x, y);
        if depth <= 0.0 {
            return false;
        }
        let band = self.generator.band_at(x, y);
        !self.carved(&column, band, x, y, depth)
    }

    fn cell_for(&self, x: i32, y: i32) -> Cell {
        let generator = self.generator;
        let def = &generator.def;
        let palette = &generator.palette;
        let column = self.column(x);
        let column = &column;
        let biome = &def.biomes[column.biome];
        let depth = self.solid_depth(column, x, y);

        if depth <= 0.0 {
            if y <= def.sea_level {
                if biome.beach == Beach::Ice && y >= def.sea_level - 1 {
                    return shaded(generator.seed, palette.ice, x, y);
                }
                return shaded(generator.seed, palette.water, x, y);
            }
            if let Some(level) = column.pond_level
                && y <= level
            {
                if biome.beach == Beach::Ice && y == level {
                    return shaded(generator.seed, palette.ice, x, y);
                }
                return shaded(generator.seed, palette.water, x, y);
            }
            if (biome.snow_cover || column.surface > SNOW_LINE)
                && y <= column.surface + SNOW_COVER_DEPTH
                && depth > -(SNOW_COVER_DEPTH as f32 + 1.0)
            {
                return shaded(generator.seed, palette.snow, x, y);
            }
            if column.tuft_height > 0 && y <= column.surface + column.tuft_height {
                return shaded(generator.seed, biome.surface, x, y);
            }
            return Cell::AIR;
        }

        let band = generator.band_at(x, y);
        if self.carved(column, band, x, y, depth) {
            let filled = water::cave_fill(
                biome,
                band,
                palette.lava,
                palette.water,
                self.aquifer.at(x, y),
                self.lava.at(x, y),
                y,
                depth,
            );
            return match filled {
                Some(material) => shaded(generator.seed, material, x, y),
                None => Cell::AIR,
            };
        }

        if band.aquifers
            && depth < MOSS_MAX_DEPTH as f32
            && depth > 8.0
            && Hash::seed(generator.seed)
                .bytes(b"moss")
                .pos(x, y)
                .chance(MOSS_CHANCE)
        {
            let below = self.solid_depth(column, x, y - 1);
            let above = self.solid_depth(column, x, y + 1);
            if (below > 0.0 && self.carved(column, band, x, y - 1, below))
                || (above > 0.0 && self.carved(column, band, x, y + 1, above))
            {
                return shaded(generator.seed, palette.moss, x, y);
            }
        }

        let jitter = Hash::seed(generator.seed).bytes(b"soil").pos(x, y).bits(2) as f32
            * biomes::SOIL_TRANSITION_JITTER
            * 0.5;
        let layered = depth + jitter;
        let material =
            if column.beach && biome.beach == Beach::Sand && layered <= BEACH_DEPTH as f32 {
                palette.sand
            } else if column.surface > SNOW_LINE && layered <= 3.0 {
                palette.snow
            } else if layered <= 1.5 {
                if column.surface < def.sea_level {
                    biome.soil
                } else {
                    biome.surface
                }
            } else if layered <= biome.soil_depth as f32 {
                biome.soil
            } else if let Some((under, under_depth)) = biome.underlayer
                && layered <= (biome.soil_depth + under_depth) as f32
            {
                under
            } else {
                band.stone
            };
        shaded(generator.seed, material, x, y)
    }
}

impl WorldGenerator {
    pub fn new(seed: u64, registry: &MaterialRegistry) -> Result<Self, GenError> {
        let palette = Palette::resolve(registry)?;
        let def = world_def(&palette);
        Ok(Self {
            seed,
            terrain: Terrain::new(seed),
            caves: Caves::new(seed),
            waters: Waters::new(seed),
            def,
            palette,
        })
    }

    pub fn surface_height(&self, x: i32) -> i32 {
        self.terrain.surface_height(&self.def, x)
    }

    fn band_at(&self, x: i32, y: i32) -> &Band {
        for (index, band) in self.def.bands.iter().enumerate() {
            let Some(floor) = band.floor else {
                return band;
            };
            let jitter = self.terrain.band_jitter(x, index);
            if y as f32 >= floor as f32 + jitter {
                return band;
            }
        }
        self.def.bands.last().expect("bands are non-empty")
    }

    fn build_column(&self, x: i32) -> Column {
        let biome_index = self.terrain.biome_at(self.def.biomes.len(), x);
        let biome = &self.def.biomes[biome_index];
        let base_surface = self.terrain.surface_height(&self.def, x);
        let pond = self.terrain.pond(&self.def, x, biome_index);
        let (surface, pond_level) = match pond {
            Some((floor, level)) => (base_surface.min(floor), Some(level)),
            None => (base_surface, None),
        };
        let mut tuft_rng = Hash::seed(self.seed).bytes(b"tuft").pos(x, 0).rng();
        let tuft_height = if surface > self.def.sea_level + 2
            && pond_level.is_none_or(|level| surface > level)
            && tuft_rng.draw().chance(biome.tuft_chance)
        {
            tuft_rng.draw().range(1, 2)
        } else {
            0
        };
        Column {
            surface,
            biome: biome_index,
            rugged: self.terrain.ruggedness(&self.def, x),
            pond_level,
            tuft_height,
            shaft: self.caves.shaft_factor(x),
            canyon: self.terrain.canyon_factor(x),
            beach: (base_surface - self.def.sea_level).abs() <= BEACH_RANGE,
        }
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
            columns.push(self.build_column(x));
        }
        let ctx = Ctx {
            generator: self,
            min_x,
            columns,
            shape: Cached::build(&self.terrain.shape, min_x, min_y, max_x, max_y),
            tunnel_a: Cached::build(&self.caves.tunnel_a, min_x, min_y, max_x, max_y),
            tunnel_b: Cached::build(&self.caves.tunnel_b, min_x, min_y, max_x, max_y),
            cheese: Cached::build(&self.caves.cheese, min_x, min_y, max_x, max_y),
            rarity: Cached::build(&self.caves.rarity, min_x, min_y, max_x, max_y),
            aquifer: Cached::build(&self.waters.aquifer, min_x, min_y, max_x, max_y),
            lava: Cached::build(&self.waters.lava, min_x, min_y, max_x, max_y),
        };

        for chunk_index in 0..REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS {
            let offset = ChunkOffset::from_index(chunk_index);
            let chunk = region.chunk_mut(offset);
            let chunk_base_x = base.x + offset.x as i32 * 64;
            let chunk_base_y = base.y + offset.y as i32 * 64;
            for local_y in 0..64u8 {
                let y = chunk_base_y + local_y as i32;
                for local_x in 0..64u8 {
                    let x = chunk_base_x + local_x as i32;
                    let cell = ctx.cell_for(x, y);
                    if cell.material != MaterialId::AIR {
                        chunk.cells_mut()[CellOffset::new(local_x, local_y).index()] = cell;
                    }
                }
            }
            chunk.change = DirtyRect::FULL;
            chunk.sim = DirtyRect::FULL;
        }

        let stones = [
            self.palette.stone,
            self.palette.sandstone,
            self.palette.deepstone,
            self.palette.basalt,
        ];
        for vein in ores::veins_for_rect(
            self.seed,
            &self.def,
            base.x,
            base.y,
            base.x + size - 1,
            base.y + size - 1,
        ) {
            let current = region_get(&region, base.x, base.y, vein.x, vein.y);
            if stones.contains(&current.material) {
                region_set(
                    &mut region,
                    base.x,
                    base.y,
                    vein.x,
                    vein.y,
                    shaded(self.seed, vein.material, vein.x, vein.y),
                );
            }
        }

        let ctx_ref = &ctx;
        let solid = |x: i32, y: i32| ctx_ref.solid_at(x, y);
        let surface_of = |x: i32| ctx_ref.column(x).surface;
        let sea_level = self.def.sea_level;
        let water_top = |x: i32| {
            ctx_ref
                .column(x)
                .pond_level
                .unwrap_or(i32::MIN)
                .max(sea_level + 2)
        };
        let covered = |x: i32, y: i32| {
            let column = ctx_ref.column(x);
            ctx_ref.solid_depth(&column, x, y) > 10.0
        };
        let clip = features::Clip {
            min_x: base.x,
            min_y: base.y,
            max_x: base.x + size - 1,
            max_y: base.y + size - 1,
        };

        for cell in structures::structures_for_rect(
            self.seed,
            &self.palette,
            &solid,
            &surface_of,
            &water_top,
            &covered,
            &clip,
        ) {
            if cell.replace {
                let value = if cell.material == MaterialId::AIR {
                    Cell::AIR
                } else {
                    shaded(self.seed, cell.material, cell.x, cell.y)
                };
                region_set(&mut region, base.x, base.y, cell.x, cell.y, value);
            } else if cell.material != MaterialId::AIR
                && region_get(&region, base.x, base.y, cell.x, cell.y).material == MaterialId::AIR
            {
                region_set(
                    &mut region,
                    base.x,
                    base.y,
                    cell.x,
                    cell.y,
                    shaded(self.seed, cell.material, cell.x, cell.y),
                );
            }
        }

        let mut into_air = Vec::new();
        into_air.extend(features::decorations_for_rect(
            self.seed,
            &self.def,
            &self.palette,
            &self.terrain,
            &solid,
            &surface_of,
            &clip,
        ));
        into_air.extend(features::mushrooms_for_rect(
            self.seed,
            &self.palette,
            &solid,
            &clip,
        ));
        into_air.extend(features::trees_for_rect(
            self.seed,
            &self.def,
            &self.palette,
            &self.terrain,
            &solid,
            &surface_of,
            &water_top,
            &clip,
        ));
        for cell in into_air {
            if region_get(&region, base.x, base.y, cell.x, cell.y).material == MaterialId::AIR {
                region_set(
                    &mut region,
                    base.x,
                    base.y,
                    cell.x,
                    cell.y,
                    shaded(self.seed, cell.material, cell.x, cell.y),
                );
            }
        }

        region
    }
}

fn region_index(base_x: i32, base_y: i32, x: i32, y: i32) -> (ChunkOffset, CellOffset) {
    let local_x = x - base_x;
    let local_y = y - base_y;
    (
        ChunkOffset::new((local_x / 64) as u8, (local_y / 64) as u8),
        CellOffset::new((local_x % 64) as u8, (local_y % 64) as u8),
    )
}

fn region_get(region: &Region, base_x: i32, base_y: i32, x: i32, y: i32) -> Cell {
    let (chunk, cell) = region_index(base_x, base_y, x, y);
    region.chunks()[chunk.index()].cells()[cell.index()]
}

fn region_set(region: &mut Region, base_x: i32, base_y: i32, x: i32, y: i32, cell: Cell) {
    let (chunk, offset) = region_index(base_x, base_y, x, y);
    region.chunk_mut(chunk).cells_mut()[offset.index()] = cell;
}

fn shaded(seed: u64, material: MaterialId, x: i32, y: i32) -> Cell {
    Cell::new(material, Hash::seed(seed).pos(x, y).bits(4) as u8)
}
