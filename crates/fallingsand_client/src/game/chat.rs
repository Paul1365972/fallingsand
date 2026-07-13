const LOG_CAP: usize = 8;

#[derive(Default)]
pub struct Chat {
    pub log: Vec<(String, f32)>,
    pub history: Vec<String>,
    cursor: Option<usize>,
    draft: String,
    pub recall: String,
}

impl Chat {
    pub fn begin_history(&mut self) {
        self.cursor = None;
        self.draft.clear();
        self.recall.clear();
    }

    pub fn record(&mut self, text: &str) {
        if self.history.last().is_none_or(|entry| entry != text) {
            self.history.push(text.to_string());
        }
        self.cursor = None;
    }

    pub fn previous(&mut self, current: &str) -> bool {
        if self.history.is_empty() {
            return false;
        }
        let index = match self.cursor {
            Some(index) => index.saturating_sub(1),
            None => {
                self.draft = current.to_string();
                self.history.len() - 1
            }
        };
        self.cursor = Some(index);
        self.recall = self.history[index].clone();
        true
    }

    pub fn next(&mut self) -> bool {
        let Some(index) = self.cursor else {
            return false;
        };
        if index + 1 < self.history.len() {
            self.cursor = Some(index + 1);
            self.recall = self.history[index + 1].clone();
        } else {
            self.cursor = None;
            self.recall = self.draft.clone();
        }
        true
    }
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
