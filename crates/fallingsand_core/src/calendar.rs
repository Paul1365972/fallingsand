use crate::MOON_PHASES;

pub const DAY_UNITS: u64 = 86_400_000;
pub const AGE_PER_TICK: u64 = 4_800;

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

    pub const fn moon_phase(self) -> u32 {
        (self.day() % MOON_PHASES as u64) as u32
    }

    pub const fn minute_of_day(self) -> u32 {
        ((self.age % DAY_UNITS) / 60_000) as u32
    }

    pub fn day_fraction(self) -> f32 {
        (self.age % DAY_UNITS) as f32 / DAY_UNITS as f32
    }
}
