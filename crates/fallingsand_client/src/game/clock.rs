use fallingsand_core::Calendar;

#[derive(Default, Clone, Copy)]
pub struct WorldClock {
    pub calendar: Calendar,
    pub synced: bool,
}

impl WorldClock {
    pub(super) fn apply(&mut self, world_age: u64) {
        self.calendar.age = world_age;
        self.synced = true;
    }
}
