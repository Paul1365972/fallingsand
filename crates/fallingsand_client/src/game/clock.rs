use fallingsand_core::Calendar;

#[derive(Default, Clone, Copy)]
pub struct WorldClock {
    pub calendar: Calendar,
    pub synced: bool,
}

impl WorldClock {
    pub fn moon_phase(&self) -> u32 {
        self.calendar.moon_phase()
    }

    pub(super) fn apply(&mut self, world_age: u64) {
        self.calendar.age = world_age;
        self.synced = true;
    }
}
