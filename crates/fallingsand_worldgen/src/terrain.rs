use crate::noise::Field;
use crate::scale::{len, wave};
use fallingsand_math::Hash;

pub const SEA_LEVEL: i32 = 0;

const DEPTH_HALF: f32 = 2200.0;
const DEPTH_WARP: i32 = 460;
const DEPTH_WARP_FADE: i32 = 200;

const SHORE: [(f32, f32); 6] = [
    (0.00, -300.0),
    (0.18, -150.0),
    (0.30, -18.0),
    (0.44, 52.0),
    (0.70, 132.0),
    (1.00, 300.0),
];

#[derive(Clone, Copy)]
pub struct Params {
    pub depth: f32,
    pub land: f32,
    pub relief: f32,
    pub rock: f32,
    pub heat: f32,
    pub wet: f32,
    pub weird: f32,
    pub variant: f32,
}

pub struct Terrain {
    shore: Field,
    even: Field,
    ridge: Field,
    detail: Field,
    grain: Field,
    massif: Field,
    terrace: Field,
    warmth: Field,
    warmth_slow: Field,
    damp: Field,
    strange: Field,
    rock: Field,
    rock_slow: Field,
    variant: Field,
    depth_warp: Field,
    overhang: Field,
    bedding: Field,
    mantle: Field,
}

impl Terrain {
    pub fn new(seed: u64) -> Self {
        Self {
            shore: Field::new(seed, Hash::label("worldgen.shore"), wave(6200.0)).octaves(2),
            even: Field::new(seed, Hash::label("worldgen.even"), wave(2600.0)).octaves(3),
            ridge: Field::new(seed, Hash::label("worldgen.ridge"), wave(1100.0))
                .octaves(5)
                .gain(0.45),
            detail: Field::new(seed, Hash::label("worldgen.detail"), wave(240.0)).octaves(2),
            grain: Field::new(seed, Hash::label("worldgen.grain"), wave(60.0)),
            massif: Field::new(seed, Hash::label("worldgen.massif"), wave(14000.0)).ridged(),
            terrace: Field::new(seed, Hash::label("worldgen.terrace"), wave(3400.0)),
            warmth: Field::new(seed, Hash::label("worldgen.warmth"), wave(1900.0))
                .octaves(3)
                .flatten(3.0),
            warmth_slow: Field::new(seed, Hash::label("worldgen.warmth_slow"), wave(12000.0)),
            damp: Field::new(seed, Hash::label("worldgen.damp"), wave(1150.0))
                .octaves(3)
                .flatten(3.0),
            strange: Field::new(seed, Hash::label("worldgen.strange"), wave(1400.0))
                .octaves(2)
                .flatten(2.5),
            rock: Field::new(seed, Hash::label("worldgen.rock"), wave(2600.0)).octaves(2),
            rock_slow: Field::new(seed, Hash::label("worldgen.rock_slow"), wave(9000.0)),
            variant: Field::new(seed, Hash::label("worldgen.variant"), wave(680.0)).octaves(2),
            depth_warp: Field::new(seed, Hash::label("worldgen.depth_warp"), wave(1400.0))
                .octaves(2),
            overhang: Field::new(seed, Hash::label("worldgen.overhang"), wave(380.0)).octaves(3),
            bedding: Field::new(seed, Hash::label("worldgen.bedding"), wave(170.0))
                .octaves(2)
                .flatten(0.09),
            mantle: Field::new(seed, Hash::label("worldgen.mantle"), wave(260.0)).octaves(2),
        }
    }

    pub fn land(&self, x: f32) -> f32 {
        (self.shore.at(x, 0.0) * 0.62 + 0.5).clamp(0.0, 1.0)
    }

    pub fn flat(&self, x: f32) -> f32 {
        (self.even.at(x, 0.0) * 0.60 + 0.5).clamp(0.0, 1.0)
    }

    pub fn height(&self, x: i32) -> i32 {
        let fx = x as f32;
        let flat = self.flat(fx);
        let rugged = 1.0 - flat;

        let base = wave(spline(self.land(fx), &SHORE));
        let amplitude = wave(18.0) + rugged * rugged * wave(240.0);
        let relief = self.ridge.at(fx, 0.0) * amplitude;
        let detail = self.detail.at(fx, 0.0) * wave(14.0) + self.grain.at(fx, 0.0) * wave(4.0);

        let raw = self.massif.at(fx, 0.0);
        let massif = if raw > 0.86 {
            let t = ((raw - 0.86) / 0.14).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t) * wave(300.0) * (0.3 + 0.7 * rugged)
        } else {
            0.0
        };

        let mut height = base + relief + detail + massif;
        let terracing = (1.0 - ((flat - 0.68) / 0.15).abs()).clamp(0.0, 1.0) * 0.9;
        if terracing > 0.0 {
            let step = wave(46.0) + (self.terrace.at(fx, 0.0) * 0.5 + 0.5) * wave(120.0);
            let phase = height / step;
            let level = phase.floor();
            let fraction = phase - level;
            let riser = fraction * fraction * fraction;
            let shaped = riser / (riser + (1.0 - fraction).powi(3));
            height = (level + fraction + (shaped - fraction) * terracing) * step;
            height += self.grain.at(fx, 0.0) * wave(4.0);
        }
        height as i32
    }

    pub fn lip(&self, x: i32, y: i32, base: f32) -> f32 {
        let rugged = 1.0 - self.flat(x as f32);
        let reach = wave(200.0);
        let fade = 1.0 - (base.abs() / reach).min(1.0);
        if fade <= 0.0 {
            return base;
        }
        base + self.overhang.at(x as f32, y as f32) * wave(30.0) * rugged * fade * fade
    }

    pub fn depth(&self, x: f32, y: f32, surface: i32) -> f32 {
        let above = (surface as f32 - y).max(0.0);
        let fade = (above / len(DEPTH_WARP_FADE) as f32).min(1.0);
        let raw =
            (above + self.depth_warp.at(x, y) * len(DEPTH_WARP) as f32 * fade * fade).max(0.0);
        raw / (raw + wave(DEPTH_HALF))
    }

    pub fn depth_to_above(depth: f32) -> f32 {
        let d = depth.clamp(0.0, 0.985);
        wave(DEPTH_HALF) * d / (1.0 - d)
    }

    pub fn above_to_depth(above: f32) -> f32 {
        let raw = above.max(0.0);
        raw / (raw + wave(DEPTH_HALF))
    }

    pub fn params(&self, x: f32, y: f32, depth: f32) -> Params {
        let spread = 1.0 + depth * 1.5;
        let warm = (self.warmth.at(x, y) * 0.42 + self.warmth_slow.at(x, 0.0) * 0.34) * spread;
        let damp = self.damp.at(x, y) * 0.54 * (1.0 + depth * 0.8);
        let tail = ((self.strange.at(x, y).abs() - 0.30) / 0.70).clamp(0.0, 1.0);
        let grain = self.rock.at(x, y) * 0.52 + self.rock_slow.at(x, y) * 0.34;
        Params {
            depth,
            land: self.land(x),
            relief: self.flat(x),
            rock: (0.5 + grain).clamp(0.0, 1.0),
            heat: (0.5 + warm + depth.powf(1.8) * 0.34).clamp(0.0, 1.0),
            wet: (0.5 + damp).clamp(0.0, 1.0),
            weird: (tail * tail + depth * depth * 0.30).clamp(0.0, 1.0),
            variant: (0.5 + self.variant.at(x, y) * 0.66).clamp(0.0, 1.0),
        }
    }

    pub fn bedded(&self, x: i32, y: i32) -> bool {
        self.bedding.at(x as f32, y as f32) > 0.36
    }

    pub fn mantle_scale(&self, x: i32) -> f32 {
        1.0 + self.mantle.at(x as f32, 0.0) * 0.45
    }
}

fn spline(t: f32, knots: &[(f32, f32)]) -> f32 {
    if t <= knots[0].0 {
        return knots[0].1;
    }
    for pair in knots.windows(2) {
        let (low, low_value) = pair[0];
        let (high, high_value) = pair[1];
        if t <= high {
            let u = (t - low) / (high - low);
            return low_value + (high_value - low_value) * u * u * (3.0 - 2.0 * u);
        }
    }
    knots[knots.len() - 1].1
}
