const LOG_CAP: usize = 8;

#[derive(Default)]
pub struct Chat {
    pub log: Vec<(String, f32)>,
}

impl Chat {
    pub(super) fn push(&mut self, line: String, now: f32) {
        self.log.push((line, now));
        if self.log.len() > LOG_CAP {
            let excess = self.log.len() - LOG_CAP;
            self.log.drain(..excess);
        }
    }
}
