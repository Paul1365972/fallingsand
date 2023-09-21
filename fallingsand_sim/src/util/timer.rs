use std::time::{Duration, Instant};

pub struct StepTimer {
    period: Duration,
    last_time: Instant,
}

impl StepTimer {
    pub fn new(period: Duration) -> Self {
        Self {
            period,
            last_time: Instant::now(),
        }
    }

    pub fn sleep(&mut self) {
        let now = Instant::now();
        let passed = now.duration_since(self.last_time);
        let remaining = self.period.saturating_sub(passed);
        //println!("Passed: {:?}, Extra Time: {:?}", passed, remaining);
        spin_sleep::sleep(remaining);
        self.last_time = Instant::now();
    }
}
