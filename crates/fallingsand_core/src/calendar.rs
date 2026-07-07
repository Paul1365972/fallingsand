use crate::MOON_PHASES;
use std::f32::consts::TAU;

pub const DAY_UNITS: u64 = 86_400_000;
pub const AGE_PER_TICK: u64 = 4_800;
pub const SYNODIC_UNITS: u64 = 124 * DAY_UNITS / 10;
pub const DRACONIC_UNITS: u64 = 109 * DAY_UNITS / 10;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Calendar {
    pub age: u64,
}

impl Calendar {
    pub const fn new(age: u64) -> Self {
        Self { age }
    }

    pub fn advance(&mut self) {
        self.age = self.age.saturating_add(AGE_PER_TICK);
    }

    pub const fn day(self) -> u64 {
        self.age / DAY_UNITS
    }

    pub const fn minute_of_day(self) -> u32 {
        ((self.age % DAY_UNITS) / 60_000) as u32
    }

    pub fn day_fraction(self) -> f32 {
        (self.age % DAY_UNITS) as f32 / DAY_UNITS as f32
    }

    pub fn synodic_fraction(self) -> f32 {
        (self.age % SYNODIC_UNITS) as f32 / SYNODIC_UNITS as f32
    }

    pub fn draconic_fraction(self) -> f32 {
        (self.age % DRACONIC_UNITS) as f32 / DRACONIC_UNITS as f32
    }

    pub fn elongation(self) -> f32 {
        self.synodic_fraction() * TAU
    }

    pub fn moon_illumination(self) -> f32 {
        (1.0 - self.elongation().cos()) / 2.0
    }

    pub fn ecliptic_latitude(self) -> f32 {
        (self.draconic_fraction() * TAU).sin()
    }

    pub fn moon_phase(self) -> u32 {
        ((self.synodic_fraction() * MOON_PHASES as f32).round() as u32) % MOON_PHASES
    }
}
