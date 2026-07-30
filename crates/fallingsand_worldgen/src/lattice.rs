use crate::biomes::{Biome, pick_biome, pick_sub};
use crate::noise::Field;
use crate::scale::wave;
use crate::terrain::{SEA_LEVEL, Terrain};
use fallingsand_math::Hash;

const ANCHOR: Hash = Hash::label("worldgen.anchor");
const COARSE: Hash = Hash::label("worldgen.coarse");

const CELL_WIDTH: f32 = 300.0;
const SKIN_DEPTH: f32 = 70.0;
const BIOME_SPAN: i32 = 5;
const JITTER: f32 = 0.42;
const WANDER: f32 = 190.0;
const POCKET: f32 = 0.075;
const POCKET_FADE: f32 = 420.0;
const POCKET_WAVE: f32 = 1300.0;
const ROW_SLACK: f32 = 700.0;
const CURL: f32 = 300.0;
const CURL_WAVE: f32 = 380.0;
const SWIRL_FADE: f32 = 460.0;

#[derive(Clone, Copy)]
pub struct Place {
    pub biome: u16,
    pub sub: u16,
}

pub struct Lattice {
    seed: u64,
    wander: Field,
    pocket: Field,
    curl: Field,
    swirl: Field,
}

pub struct Cells {
    min_u: i32,
    min_v: i32,
    width: i32,
    height: i32,
    anchors: Vec<(f32, f32, Place)>,
}

impl Cells {
    pub fn nearest(&self, u: f32, v: f32) -> Option<Place> {
        let base_u = u.floor() as i32;
        let base_v = v.floor() as i32;
        let mut found = None;
        let mut best = f32::MAX;
        for offset_v in -1..=1 {
            for offset_u in -1..=1 {
                let index_u = base_u + offset_u - self.min_u;
                let index_v = base_v + offset_v - self.min_v;
                if index_u < 0 || index_v < 0 || index_u >= self.width || index_v >= self.height {
                    continue;
                }
                let (anchor_u, anchor_v, place) =
                    self.anchors[(index_v * self.width + index_u) as usize];
                let distance = (anchor_u - u) * (anchor_u - u) + (anchor_v - v) * (anchor_v - v);
                if distance < best {
                    best = distance;
                    found = Some(place);
                }
            }
        }
        found
    }
}

fn row_of(above: f32) -> f32 {
    let skin = wave(SKIN_DEPTH);
    if above < skin {
        above / skin
    } else {
        1.0 + (above - skin) / wave(CELL_WIDTH)
    }
}

fn above_of(row: f32) -> f32 {
    let skin = wave(SKIN_DEPTH);
    if row < 1.0 {
        row * skin
    } else {
        skin + (row - 1.0) * wave(CELL_WIDTH)
    }
}

impl Lattice {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            wander: Field::new(seed, Hash::label("worldgen.wander"), wave(1400.0)).octaves(2),
            pocket: Field::new(seed, Hash::label("worldgen.pocket"), wave(POCKET_WAVE)).octaves(2),
            curl: Field::new(seed, Hash::label("worldgen.curl"), wave(CURL_WAVE)).octaves(2),
            swirl: Field::new(seed, Hash::label("worldgen.swirl"), wave(CURL_WAVE)).octaves(2),
        }
    }

    pub fn coords(&self, x: f32, y: f32, depth: f32) -> (f32, f32) {
        let wandered = x + self.wander.at(x, y) * wave(WANDER) + self.curl.at(x, y) * wave(CURL);
        let seated = Terrain::depth_to_above(depth);
        let fade = (seated / wave(SWIRL_FADE)).min(1.0);
        let above = seated + self.swirl.at(x, y) * wave(CURL) * fade * fade;
        (wandered / wave(CELL_WIDTH), row_of(above.max(0.0)))
    }

    fn anchor(&self, salt: Hash, span: f32, u: i32, v: i32) -> (f32, f32) {
        let mut rng = Hash::seed(self.seed).salt(salt).pos(u, v).rng();
        let offset_u = (rng.draw().unit() - 0.5) * 2.0 * JITTER;
        let offset_v = (rng.draw().unit() - 0.5) * 2.0 * JITTER;
        (
            (u as f32 + 0.5 + offset_u) * span,
            (v as f32 + 0.5 + offset_v) * span,
        )
    }

    fn sample_at(&self, terrain: &Terrain, u: f32, v: f32, shift: bool) -> crate::terrain::Params {
        let x = u * wave(CELL_WIDTH);
        let above = above_of(v).max(0.0);
        let y = SEA_LEVEL as f32 - above;
        let seated = Terrain::above_to_depth(above);
        let depth = if shift {
            let fade = (above / wave(POCKET_FADE)).min(1.0);
            (seated + self.pocket.at(x, y) * POCKET * fade * fade).clamp(0.0, 1.0)
        } else {
            seated
        };
        terrain.params(x, y, depth)
    }

    fn biome_at(&self, terrain: &Terrain, biomes: &[Biome], u: f32, v: f32) -> usize {
        let span = BIOME_SPAN as f32;
        let base_u = (u / span).floor() as i32;
        let base_v = (v / span).floor() as i32;
        let mut found = 0;
        let mut best = f32::MAX;
        for offset_v in -1..=1 {
            for offset_u in -1..=1 {
                let (anchor_u, anchor_v) =
                    self.anchor(COARSE, span, base_u + offset_u, base_v + offset_v);
                let distance = (anchor_u - u) * (anchor_u - u) + (anchor_v - v) * (anchor_v - v);
                if distance < best {
                    best = distance;
                    let params = self.sample_at(terrain, anchor_u, anchor_v, true);
                    found = pick_biome(biomes, &params, false);
                }
            }
        }
        found
    }

    fn place_of(&self, terrain: &Terrain, biomes: &[Biome], u: i32, v: i32) -> Place {
        let (anchor_u, anchor_v) = self.anchor(ANCHOR, 1.0, u, v);
        let params = self.sample_at(terrain, anchor_u, anchor_v, true);
        let biome = if above_of(anchor_v) < wave(SKIN_DEPTH) {
            pick_biome(biomes, &params, true)
        } else {
            self.biome_at(terrain, biomes, anchor_u, anchor_v)
        };
        let sub = pick_sub(&biomes[biome].members, &params);
        Place {
            biome: biome as u16,
            sub: sub as u16,
        }
    }

    pub fn place_at(
        &self,
        terrain: &Terrain,
        biomes: &[Biome],
        x: f32,
        y: f32,
        depth: f32,
    ) -> Place {
        let (u, v) = self.coords(x, y, depth);
        let base_u = u.floor() as i32;
        let base_v = v.floor() as i32;
        let mut found = Place { biome: 0, sub: 0 };
        let mut best = f32::MAX;
        for offset_v in -1..=1 {
            for offset_u in -1..=1 {
                let (cell_u, cell_v) = (base_u + offset_u, base_v + offset_v);
                let (anchor_u, anchor_v) = self.anchor(ANCHOR, 1.0, cell_u, cell_v);
                let distance = (anchor_u - u) * (anchor_u - u) + (anchor_v - v) * (anchor_v - v);
                if distance < best {
                    best = distance;
                    found = self.place_of(terrain, biomes, cell_u, cell_v);
                }
            }
        }
        found
    }

    pub fn cells(
        &self,
        terrain: &Terrain,
        biomes: &[Biome],
        min_x: i32,
        max_x: i32,
        min_above: f32,
        max_above: f32,
    ) -> Cells {
        let span = wave(CELL_WIDTH);
        let slack = wave(ROW_SLACK);
        let min_u = ((min_x as f32 - wave(WANDER) - wave(CURL)) / span).floor() as i32 - 1;
        let max_u = ((max_x as f32 + wave(WANDER) + wave(CURL)) / span).floor() as i32 + 1;
        let min_v = row_of((min_above - slack).max(0.0)).floor() as i32 - 1;
        let max_v = row_of(max_above + slack).floor() as i32 + 1;
        let width = max_u - min_u + 1;
        let height = max_v - min_v + 1;
        let mut anchors = Vec::with_capacity((width * height) as usize);
        for v in min_v..=max_v {
            for u in min_u..=max_u {
                let (anchor_u, anchor_v) = self.anchor(ANCHOR, 1.0, u, v);
                anchors.push((anchor_u, anchor_v, self.place_of(terrain, biomes, u, v)));
            }
        }
        Cells {
            min_u,
            min_v,
            width,
            height,
            anchors,
        }
    }
}
