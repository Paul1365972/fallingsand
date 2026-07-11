use crate::TICK_RATE;
use std::f32::consts::TAU;

pub const DAY_UNITS: u64 = 86_400_000;
const AGE_PER_SEC: u64 = 288_000;
const AGE_PER_TICK: u64 = AGE_PER_SEC / TICK_RATE as u64;
const YEAR_DAYS: u64 = 60;
const YEAR_UNITS: u64 = YEAR_DAYS * DAY_UNITS;
pub const SEASON_DAYS: u64 = 15;
const SYNODIC_UNITS: u64 = 643 * DAY_UNITS / 100;
const ECCENTRE_UNITS: u64 = 4652 * DAY_UNITS / 1000;
const ANOMALISTIC_UNITS: u64 = 587 * DAY_UNITS / 100;
const SYNODIC_EPOCH: u64 = 29 * SYNODIC_UNITS / 100;
const ECCENTRE_EPOCH: u64 = 61 * ECCENTRE_UNITS / 100;
const ANOMALISTIC_EPOCH: u64 = 43 * ANOMALISTIC_UNITS / 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const fn label(self) -> &'static str {
        match self {
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Autumn => "autumn",
            Season::Winter => "winter",
        }
    }
}

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

    pub const fn day_of_year(self) -> u32 {
        ((self.age % YEAR_UNITS) / DAY_UNITS) as u32
    }

    pub const fn season(self) -> Season {
        match self.day_of_year() as u64 / SEASON_DAYS {
            0 => Season::Spring,
            1 => Season::Summer,
            2 => Season::Autumn,
            _ => Season::Winter,
        }
    }

    pub fn day_fraction(self) -> f32 {
        (self.age % DAY_UNITS) as f32 / DAY_UNITS as f32
    }

    pub fn year_fraction(self) -> f32 {
        (self.age % YEAR_UNITS) as f32 / YEAR_UNITS as f32
    }

    pub fn sidereal(self) -> f32 {
        (self.day_fraction() + self.year_fraction()).fract()
    }

    pub fn synodic_fraction(self) -> f32 {
        ((self.age + SYNODIC_EPOCH) % SYNODIC_UNITS) as f32 / SYNODIC_UNITS as f32
    }

    pub fn eccentre_fraction(self) -> f32 {
        ((self.age + ECCENTRE_EPOCH) % ECCENTRE_UNITS) as f32 / ECCENTRE_UNITS as f32
    }

    pub fn anomalistic_fraction(self) -> f32 {
        ((self.age + ANOMALISTIC_EPOCH) % ANOMALISTIC_UNITS) as f32 / ANOMALISTIC_UNITS as f32
    }

    pub fn elongation(self) -> f32 {
        self.synodic_fraction() * TAU
    }
}
