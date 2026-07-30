use crate::biomes::SubBiome;
use crate::noise::Field;
use crate::scale::{len, pitch, wave};
use crate::terrain::SEA_LEVEL;
use fallingsand_math::Hash;

const WORM_SALT: Hash = Hash::label("worldgen.worm");
const TRUNK_SALT: Hash = Hash::label("worldgen.trunk");
const SHAFT_SALT: Hash = Hash::label("worldgen.shaft");
const CHAMBER_SALT: Hash = Hash::label("worldgen.chamber");
const MOUTH_SALT: Hash = Hash::label("worldgen.mouth");
const BRANCH_SALT: Hash = Hash::label("worldgen.branch");

const LOCAL_PITCH: i32 = 1060;
const TRUNK_PITCH: i32 = 2830;
const CHAMBER_PITCH: i32 = 900;
const SHAFT_PITCH: i32 = 640;
const MOUTH_PITCH: i32 = 1536;

const LOCAL_REACH: i32 = 900;
const TRUNK_REACH: i32 = 3600;
const CHAMBER_REACH: i32 = 1200;
const SHAFT_REACH: i32 = 460;
const MOUTH_REACH: i32 = 380;

pub struct Caves {
    seed: u64,
    density: Field,
    rough: Field,
    drift: Field,
    layer_ceiling: Field,
    layer_floor: Field,
    layer_presence: Field,
    sediment: Field,
    gas: Field,
}

pub struct Carve {
    min_x: i32,
    min_y: i32,
    width: i32,
    height: i32,
    open: Vec<bool>,
}

impl Carve {
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let ix = x - self.min_x;
        let iy = y - self.min_y;
        if ix < 0 || iy < 0 || ix >= self.width || iy >= self.height {
            return None;
        }
        Some((iy * self.width + ix) as usize)
    }

    pub fn is_open(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_some_and(|index| self.open[index])
    }

    pub fn open(&mut self, x: i32, y: i32) {
        if let Some(index) = self.index(x, y) {
            self.open[index] = true;
        }
    }

    pub fn seal(&mut self, x: i32, y: i32) {
        if let Some(index) = self.index(x, y) {
            self.open[index] = false;
        }
    }
}

struct Worm {
    x: f32,
    y: f32,
    angle: f32,
    radius: f32,
    length: f32,
    squash: f32,
    sway: f32,
    tag: u64,
}

impl Caves {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            density: Field::new(seed, Hash::label("worldgen.density"), wave(1800.0)).octaves(3),
            rough: Field::new(seed, Hash::label("worldgen.rough"), wave(60.0)).octaves(3),
            drift: Field::new(seed, Hash::label("worldgen.drift"), wave(190.0)).octaves(2),
            layer_ceiling: Field::new(seed, Hash::label("worldgen.layer_ceiling"), wave(220.0))
                .octaves(2),
            layer_floor: Field::new(seed, Hash::label("worldgen.layer_floor"), wave(150.0)),
            layer_presence: Field::new(seed, Hash::label("worldgen.layer_presence"), wave(900.0)),
            sediment: Field::new(seed, Hash::label("worldgen.sediment"), wave(90.0)),
            gas: Field::new(seed, Hash::label("worldgen.gas"), wave(320.0)).octaves(2),
        }
    }

    pub fn gas_pocket(&self, x: i32, y: i32, chance: f32) -> bool {
        if chance <= 0.0 {
            return false;
        }
        let raw = self.gas.at(x as f32, y as f32) * 0.5 + 0.5;
        raw > 1.0 - chance
    }

    fn porosity(&self, x: f32, y: f32, sub: &SubBiome) -> f32 {
        let raw = self.density.at(x, y) * 0.5 + 0.5;
        let low = sub.solidity;
        ((raw - low) / (1.0 - low)).clamp(0.0, 1.0)
    }

    pub fn build(
        &self,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        sub_at: &dyn Fn(i32, i32) -> SubBiome,
        surface_at: &dyn Fn(i32) -> i32,
    ) -> Carve {
        let width = max_x - min_x;
        let height = max_y - min_y;
        let mut carve = Carve {
            min_x,
            min_y,
            width,
            height,
            open: vec![false; (width * height) as usize],
        };

        self.carve_worms(&mut carve, sub_at);
        self.carve_chambers(&mut carve, sub_at, surface_at);
        self.carve_mouths(&mut carve, surface_at);

        self.carve_layers(&mut carve, sub_at);
        carve
    }

    fn carve_layers(&self, carve: &mut Carve, sub_at: &dyn Fn(i32, i32) -> SubBiome) {
        let resolution = len(46);
        let first = carve.min_y.div_euclid(resolution) * resolution;
        let mut level = first;
        while level < carve.min_y + carve.height + resolution {
            let hash = Hash::seed(self.seed)
                .salt(Hash::label("worldgen.layer"))
                .pos(0, level);
            if !hash.chance(0.18) {
                level += resolution;
                continue;
            }
            let mut rng = hash.rng();
            let ceiling_amp = len(rng.draw().range(6, 18)) as f32;
            let floor_amp = len(rng.draw().range(1, 5)) as f32;
            let clearance = len(rng.draw().range(12, 28)) as f32;
            let phase = rng.draw().range(0, 8192) as f32;

            for x in carve.min_x..carve.min_x + carve.width {
                let fx = x as f32 + phase;
                let sub = sub_at(x, level);
                if sub.galleries <= 0.0 {
                    continue;
                }
                let porosity = self.porosity(x as f32, level as f32, &sub);
                let presence = self.layer_presence.at(fx, level as f32) * 0.5 + 0.5;
                let onset = 1.0 - 0.44 * sub.galleries;
                if presence < onset || porosity < 0.30 {
                    continue;
                }
                let reveal = ((presence - onset) / (1.0 - onset).max(0.05))
                    .clamp(0.0, 1.0)
                    .sqrt();
                let wander = self.layer_ceiling.at(fx * 0.6, level as f32 + 4096.0) * wave(52.0);
                let floor =
                    level as f32 + wander + self.layer_floor.at(fx, level as f32) * floor_amp;
                let ceiling = floor
                    + (clearance
                        + (self.layer_ceiling.at(fx, level as f32) * 0.5 + 0.5) * ceiling_amp)
                        * reveal;
                let mut y = floor.ceil() as i32;
                let top = ceiling as i32;
                while y <= top {
                    carve.open(x, y);
                    y += 1;
                }
            }
            level += resolution;
        }
    }

    fn carve_worms(&self, carve: &mut Carve, sub_at: &dyn Fn(i32, i32) -> SubBiome) {
        for (salt, lattice, reach, trunk) in [
            (WORM_SALT, pitch(LOCAL_PITCH), len(LOCAL_REACH), false),
            (TRUNK_SALT, pitch(TRUNK_PITCH), len(TRUNK_REACH), true),
        ] {
            let lo_x = (carve.min_x - reach).div_euclid(lattice);
            let hi_x = (carve.min_x + carve.width + reach).div_euclid(lattice);
            let lo_y = (carve.min_y - reach).div_euclid(lattice);
            let hi_y = (carve.min_y + carve.height + reach).div_euclid(lattice);
            for cell_y in lo_y..=hi_y {
                for cell_x in lo_x..=hi_x {
                    let hash = Hash::seed(self.seed).salt(salt).pos(cell_x, cell_y);
                    let mut rng = hash.rng();
                    let origin_x = cell_x * lattice + rng.draw().range(0, lattice - 1);
                    let origin_y = cell_y * lattice + rng.draw().range(0, lattice - 1);
                    let above = (origin_y - SEA_LEVEL).max(0) as f32 / len(300) as f32;
                    let sub = sub_at(origin_x, origin_y);
                    let chance = sub.worms.min(1.0) / (1.0 + above * above * above);
                    if !rng.draw().chance(chance) {
                        continue;
                    }

                    let roll = rng.draw().unit();
                    let radius = if roll < 0.45 {
                        lerp(wave(7.0), wave(9.5), roll / 0.45)
                    } else {
                        lerp(wave(12.0), wave(22.0), (roll - 0.45) / 0.55)
                    };
                    let plunging = rng.draw().chance(if trunk { 0.22 } else { 0.16 });
                    let angle = if plunging {
                        -std::f32::consts::FRAC_PI_2 + (rng.draw().unit() - 0.5) * 1.1
                    } else {
                        (rng.draw().unit() - 0.5) * 0.9
                            + if rng.draw().bit() {
                                std::f32::consts::PI
                            } else {
                                0.0
                            }
                    };
                    let length = if trunk {
                        wave(rng.draw().range(4200, 9000) as f32)
                    } else {
                        wave(rng.draw().range(700, 1900) as f32)
                    };
                    let worm = Worm {
                        x: origin_x as f32,
                        y: origin_y as f32,
                        angle,
                        radius,
                        length,
                        squash: if plunging { 0.9 } else { 1.4 },
                        sway: if trunk { 0.008 } else { 0.019 },
                        tag: 0,
                    };
                    self.run_worm(carve, worm, reach, origin_x, origin_y);
                }
            }
        }
    }

    fn run_worm(&self, carve: &mut Carve, worm: Worm, reach: i32, root_x: i32, root_y: i32) {
        self.run_branch(carve, worm, reach, root_x, root_y, 1);
    }

    fn run_branch(
        &self,
        carve: &mut Carve,
        mut worm: Worm,
        reach: i32,
        root_x: i32,
        root_y: i32,
        depth: u32,
    ) {
        let reach_cells = reach;
        let reach = reach as f32;
        let step_len = (worm.radius * 0.4).max(1.0);
        let steps = (worm.length / step_len).ceil() as i32;

        let mut branch_rng = Hash::seed(self.seed)
            .salt(BRANCH_SALT)
            .pos(root_x, root_y)
            .add(worm.tag)
            .rng();
        let branch_count = if depth > 0 && steps > 12 {
            branch_rng.draw().range(1, 3)
        } else {
            0
        };
        let mut branch_at = [-1i32; 4];
        for slot in branch_at.iter_mut().take(branch_count as usize) {
            *slot = branch_rng.draw().range(steps / 6, steps * 4 / 5);
        }

        for step in 0..steps {
            for (index, &at) in branch_at.iter().take(branch_count as usize).enumerate() {
                if at != step {
                    continue;
                }
                let mut rng = Hash::seed(self.seed)
                    .salt(BRANCH_SALT)
                    .pos(root_x, root_y)
                    .add(worm.tag)
                    .add(index as u64 + 1)
                    .rng();
                let side = if rng.draw().bit() { 1.0 } else { -1.0 };
                let child = Worm {
                    x: worm.x,
                    y: worm.y,
                    angle: worm.angle + side * lerp(0.6, 1.4, rng.draw().unit()),
                    radius: (worm.radius * lerp(0.52, 0.74, rng.draw().unit())).max(wave(5.5)),
                    length: worm.length * lerp(0.35, 0.62, rng.draw().unit()),
                    squash: worm.squash,
                    sway: worm.sway * 1.4,
                    tag: worm.tag * 5 + index as u64 + 1,
                };
                self.run_branch(carve, child, reach_cells, root_x, root_y, depth - 1);
            }
            worm.angle += self.drift.at(worm.x, worm.y) * worm.sway * step_len;
            worm.x += worm.angle.cos() * step_len;
            worm.y += worm.angle.sin() * step_len;

            if (worm.x - root_x as f32).abs() > reach || (worm.y - root_y as f32).abs() > reach {
                return;
            }
            if worm.x + worm.radius < carve.min_x as f32
                || worm.x - worm.radius > (carve.min_x + carve.width) as f32
                || worm.y + worm.radius * 2.0 < carve.min_y as f32
                || worm.y - worm.radius * 2.0 > (carve.min_y + carve.height) as f32
            {
                continue;
            }

            let pulse = 0.84 + 0.32 * (self.rough.at(worm.x, worm.y) * 0.5 + 0.5);
            let mouth = ((step as f32 / steps as f32) * 6.0).min(1.0);
            let radius = worm.radius * pulse * (0.72 + 0.28 * mouth);
            self.stamp(carve, worm.x, worm.y, radius, worm.squash);
        }
    }

    fn stamp(&self, carve: &mut Carve, cx: f32, cy: f32, radius: f32, squash: f32) {
        let rx = radius;
        let ry = (radius / squash).max(1.0);
        let min_x = (cx - rx).floor() as i32;
        let max_x = (cx + rx).ceil() as i32;
        let min_y = (cy - ry).floor() as i32;
        let max_y = (cy + ry).ceil() as i32;
        for y in min_y..=max_y {
            let dy = (y as f32 - cy) / ry;
            if dy < -0.62 {
                continue;
            }
            for x in min_x..=max_x {
                let dx = (x as f32 - cx) / rx;
                let wobble = self.rough.at(x as f32, y as f32) * 0.18;
                if dx * dx + dy * dy < 1.0 + wobble {
                    carve.open(x, y);
                }
            }
        }
    }

    fn carve_chambers(
        &self,
        carve: &mut Carve,
        sub_at: &dyn Fn(i32, i32) -> SubBiome,
        surface_at: &dyn Fn(i32) -> i32,
    ) {
        let lattice = pitch(CHAMBER_PITCH);
        let reach = len(CHAMBER_REACH);
        let lo_x = (carve.min_x - reach).div_euclid(lattice);
        let hi_x = (carve.min_x + carve.width + reach).div_euclid(lattice);
        let lo_y = (carve.min_y - reach).div_euclid(lattice);
        let hi_y = (carve.min_y + carve.height + reach).div_euclid(lattice);
        for cell_y in lo_y..=hi_y {
            for cell_x in lo_x..=hi_x {
                let hash = Hash::seed(self.seed).salt(CHAMBER_SALT).pos(cell_x, cell_y);
                let mut rng = hash.rng();
                let seat_x = cell_x * lattice + rng.draw().range(0, lattice - 1);
                let seat_y = cell_y * lattice + rng.draw().range(0, lattice - 1);
                if seat_y > surface_at(seat_x) - len(90) {
                    continue;
                }
                let sub = sub_at(seat_x, seat_y);
                if !rng.draw().chance(0.30 * sub.chambers) {
                    continue;
                }
                let cx = seat_x as f32;
                let cy = seat_y as f32;
                let rx = len(rng.draw().range(60, 220)) as f32;
                let ry = (rx * lerp(0.34, 0.62, rng.draw().unit())).max(len(26) as f32);
                let exponent = lerp(2.1, 3.0, rng.draw().unit());

                let min_x = (cx - rx - 4.0) as i32;
                let max_x = (cx + rx + 4.0) as i32;
                let min_y = (cy - ry - 4.0) as i32;
                let max_y = (cy + ry + 4.0) as i32;
                for y in min_y.max(carve.min_y)..=max_y.min(carve.min_y + carve.height - 1) {
                    for x in min_x.max(carve.min_x)..=max_x.min(carve.min_x + carve.width - 1) {
                        let dx = ((x as f32 - cx) / rx).abs();
                        let dy = ((y as f32 - cy) / ry).abs();
                        let edge = 1.0 + 0.45 * self.rough.at(x as f32, y as f32);
                        if dx.powf(exponent) + dy.powf(exponent) < edge.powf(exponent) {
                            carve.open(x, y);
                        }
                    }
                }
                self.raise_pillars(carve, &mut rng, cx, cy, rx, ry);
                self.open_exits(carve, &mut rng, seat_x, seat_y, reach);
            }
        }
    }

    fn open_exits(
        &self,
        carve: &mut Carve,
        rng: &mut fallingsand_math::Rng,
        seat_x: i32,
        seat_y: i32,
        reach: i32,
    ) {
        let count = rng.draw().range(2, 4);
        for index in 0..count {
            let worm = Worm {
                x: seat_x as f32,
                y: seat_y as f32,
                angle: rng.draw().unit() * std::f32::consts::TAU,
                radius: wave(rng.draw().range(7, 12) as f32),
                length: wave(rng.draw().range(400, 900) as f32),
                squash: 1.25,
                sway: 0.022,
                tag: 0x9e37 + index as u64,
            };
            self.run_branch(carve, worm, reach, seat_x, seat_y, 1);
        }
    }

    fn raise_pillars(
        &self,
        carve: &mut Carve,
        rng: &mut fallingsand_math::Rng,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    ) {
        if ry < len(40) as f32 {
            return;
        }
        let count = rng.draw().range(1, 4);
        for _ in 0..count {
            let px = cx + (rng.draw().unit() - 0.5) * rx * 1.5;
            let radius = len(rng.draw().range(4, 11)) as f32;
            let min_y = (cy - ry - 4.0) as i32;
            let max_y = (cy + ry + 4.0) as i32;
            for y in min_y..=max_y {
                let taper = 1.0 + 0.5 * ((y as f32 - cy) / ry).abs().powi(3);
                let gnarl = 0.72 + 0.56 * (self.rough.at(px, y as f32) * 0.5 + 0.5);
                let half = radius * taper * gnarl;
                let lo = (px - half) as i32;
                let hi = (px + half) as i32;
                for x in lo..=hi {
                    carve.seal(x, y);
                }
            }
        }
    }

    fn carve_mouths(&self, carve: &mut Carve, surface_at: &dyn Fn(i32) -> i32) {
        let lattice = pitch(MOUTH_PITCH);
        let reach = len(MOUTH_REACH);
        let lo = (carve.min_x - reach).div_euclid(lattice);
        let hi = (carve.min_x + carve.width + reach).div_euclid(lattice);
        for cell in lo..=hi {
            let hash = Hash::seed(self.seed).salt(MOUTH_SALT).pos(cell, 0);
            let mut rng = hash.rng();
            let x = cell * lattice + rng.draw().range(0, lattice - 1);
            let top = surface_at(x);
            let radius = wave(rng.draw().range(7, 13) as f32);
            let worm = Worm {
                x: x as f32,
                y: top as f32 + radius,
                angle: -std::f32::consts::FRAC_PI_2 + (rng.draw().unit() - 0.5) * 0.8,
                radius,
                length: wave(rng.draw().range(220, 460) as f32),
                squash: 0.9,
                sway: 0.020,
                tag: 0,
            };
            self.run_worm(carve, worm, reach, x, top);
        }
        let shaft_lattice = pitch(SHAFT_PITCH);
        let shaft_reach = len(SHAFT_REACH);
        let sx_lo = (carve.min_x - shaft_reach).div_euclid(shaft_lattice);
        let sx_hi = (carve.min_x + carve.width + shaft_reach).div_euclid(shaft_lattice);
        let sy_lo = (carve.min_y - shaft_reach).div_euclid(shaft_lattice);
        let sy_hi = (carve.min_y + carve.height + shaft_reach).div_euclid(shaft_lattice);
        for cell_y in sy_lo..=sy_hi {
            for cell_x in sx_lo..=sx_hi {
                let hash = Hash::seed(self.seed).salt(SHAFT_SALT).pos(cell_x, cell_y);
                let mut rng = hash.rng();
                let x = cell_x * shaft_lattice + rng.draw().range(0, shaft_lattice - 1);
                let y = cell_y * shaft_lattice + rng.draw().range(0, shaft_lattice - 1);
                if y > surface_at(x) - len(40) {
                    continue;
                }
                let worm = Worm {
                    x: x as f32,
                    y: y as f32,
                    angle: -std::f32::consts::FRAC_PI_2 + (rng.draw().unit() - 0.5) * 0.5,
                    radius: wave(rng.draw().range(4, 7) as f32),
                    length: wave(rng.draw().range(240, 420) as f32),
                    squash: 0.75,
                    sway: 0.012,
                    tag: 0,
                };
                self.run_worm(carve, worm, shaft_reach, x, y);
            }
        }
    }

    pub fn sediment_depth(&self, x: i32, y: i32) -> i32 {
        let raw = self.sediment.at(x as f32, y as f32) * 0.5 + 0.5;
        (raw * 4.0) as i32 * len(2)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}
