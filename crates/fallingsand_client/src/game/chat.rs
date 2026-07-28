use fallingsand_protocol::{ChatEntry, ClientMessage, CommandInfo, ParamKind, push_history};
use std::collections::VecDeque;

const LOG_CAP: usize = 100;
pub const OPEN_ROWS: usize = 12;
pub const CLOSED_ROWS: usize = 8;

#[derive(Default)]
pub struct Chat {
    pub log: Log,
    pub composer: Composer,
}

impl Chat {
    pub(super) fn open(&mut self) {
        self.log.scroll = 0;
        self.log.unread = 0;
        self.composer.open();
    }
}

#[derive(Default)]
pub struct Log {
    entries: VecDeque<(ChatEntry, f32)>,
    scroll: usize,
    pub unread: usize,
}

impl Log {
    pub(super) fn push(&mut self, entry: ChatEntry, now: f32, open: bool) {
        if self.entries.len() == LOG_CAP {
            self.entries.pop_front();
        }
        self.entries.push_back((entry, now));
        match open {
            true => self.scroll = self.scroll.min(self.depth()),
            false => self.unread += 1,
        }
    }

    fn depth(&self) -> usize {
        self.entries.len().saturating_sub(OPEN_ROWS)
    }

    pub fn scroll_by(&mut self, delta: isize) -> bool {
        let target = (self.scroll as isize + delta).clamp(0, self.depth() as isize) as usize;
        let moved = target != self.scroll;
        self.scroll = target;
        moved
    }

    pub fn visible(&self, open: bool) -> impl Iterator<Item = &(ChatEntry, f32)> {
        let (rows, back) = match open {
            true => (OPEN_ROWS, self.scroll),
            false => (CLOSED_ROWS, 0),
        };
        let end = self.entries.len().saturating_sub(back);
        self.entries.range(end.saturating_sub(rows)..end)
    }
}

pub struct Suggestion {
    pub line: Vec<(String, bool)>,
    pub candidates: Vec<(String, bool)>,
    pub error: bool,
}

struct Completion {
    active: usize,
    candidates: Vec<String>,
    index: usize,
}

struct Scan {
    active: usize,
    candidates: Vec<String>,
    command: Option<usize>,
}

#[derive(Default)]
pub struct Composer {
    pub draft: String,
    pub commands: Vec<CommandInfo>,
    history: Vec<String>,
    recall: Option<usize>,
    stash: String,
    completion: Option<Completion>,
}

impl Composer {
    fn open(&mut self) {
        self.draft.clear();
        self.stash.clear();
        self.recall = None;
        self.completion = None;
    }

    pub(super) fn set_history(&mut self, entries: Vec<String>) {
        self.history = entries;
    }

    pub(super) fn observe(&mut self, text: &str) -> bool {
        if self.draft == text {
            return false;
        }
        self.draft = text.to_string();
        self.completion = None;
        true
    }

    pub fn submit(&mut self) -> Option<ClientMessage> {
        let text = self.draft.trim().to_string();
        if text.is_empty() {
            return None;
        }
        push_history(&mut self.history, &text);
        self.recall = None;
        if let Some(line) = text.strip_prefix('/') {
            let line = line.trim().to_string();
            return (!line.is_empty()).then_some(ClientMessage::Command { line });
        }
        Some(ClientMessage::Chat { text })
    }

    pub fn recall_prev(&mut self) -> bool {
        if self.history.is_empty() {
            return false;
        }
        let index = match self.recall {
            Some(index) => index.saturating_sub(1),
            None => {
                self.stash = self.draft.clone();
                self.history.len() - 1
            }
        };
        self.recall = Some(index);
        self.rewrite(self.history[index].clone());
        true
    }

    pub fn recall_next(&mut self) -> bool {
        let Some(index) = self.recall else {
            return false;
        };
        match self.history.get(index + 1) {
            Some(entry) => {
                self.recall = Some(index + 1);
                self.rewrite(entry.clone());
            }
            None => {
                self.recall = None;
                self.rewrite(self.stash.clone());
            }
        }
        true
    }

    fn rewrite(&mut self, text: String) {
        self.draft = text;
        self.completion = None;
    }

    pub fn complete(&mut self) -> bool {
        if self.draft.is_empty() {
            self.draft.push('/');
        }
        match &mut self.completion {
            Some(completion) => {
                completion.index = (completion.index + 1) % completion.candidates.len();
            }
            None => {
                let scan = self.scan().filter(|scan| !scan.candidates.is_empty());
                let Some(scan) = scan else {
                    return false;
                };
                self.completion = Some(Completion {
                    active: scan.active,
                    candidates: scan.candidates,
                    index: 0,
                });
            }
        }
        let (draft, resolved) = {
            let completion = self.completion.as_ref().expect("completion present");
            let mut parts: Vec<&str> = tokens(&self.draft).take(completion.active).collect();
            parts.push(&completion.candidates[completion.index]);
            let resolved = completion.candidates.len() == 1;
            let separator = if resolved { " " } else { "" };
            (format!("/{}{separator}", parts.join(" ")), resolved)
        };
        self.draft = draft;
        if resolved {
            self.completion = None;
        }
        true
    }

    pub fn suggestion(&self) -> Option<Suggestion> {
        let scan = self.scan()?;
        let (active, pool, selected) = match &self.completion {
            Some(completion) => (
                completion.active,
                &completion.candidates,
                Some(completion.index),
            ),
            None => (scan.active, &scan.candidates, None),
        };
        let candidates = pool
            .iter()
            .enumerate()
            .map(|(index, value)| (value.clone(), Some(index) == selected))
            .collect();
        let unknown = scan.command.is_none() && pool.is_empty();
        let line = match scan.command {
            Some(index) => self.commands[index]
                .segments()
                .enumerate()
                .map(|(index, text)| (text, index == active))
                .collect(),
            None if unknown => {
                let name = self.draft.split_whitespace().next().unwrap_or("/");
                vec![(format!("unknown command: {name}"), true)]
            }
            None => Vec::new(),
        };
        Some(Suggestion {
            line,
            candidates,
            error: unknown,
        })
    }

    fn scan(&self) -> Option<Scan> {
        let line = self.draft.strip_prefix('/')?;
        let parts: Vec<&str> = tokens(&self.draft).collect();
        let active = match line.ends_with(char::is_whitespace) {
            true => parts.len(),
            false => parts.len().saturating_sub(1),
        };
        let prefix = parts.get(active).copied().unwrap_or_default();
        let command = parts
            .first()
            .and_then(|name| self.commands.iter().position(|info| info.matches(name)));
        let pool: Vec<&str> = match active.checked_sub(1) {
            None => self.names(),
            Some(param) => match command.and_then(|index| self.commands[index].params.get(param)) {
                Some(param) if param.kind == ParamKind::Command => self.names(),
                Some(param) => param.choices.iter().map(String::as_str).collect(),
                None => Vec::new(),
            },
        };
        Some(Scan {
            active,
            candidates: pool
                .into_iter()
                .filter(|value| value.starts_with(prefix))
                .map(str::to_string)
                .collect(),
            command,
        })
    }

    fn names(&self) -> Vec<&str> {
        self.commands
            .iter()
            .map(|info| info.name.as_str())
            .collect()
    }
}

fn tokens(draft: &str) -> impl Iterator<Item = &str> {
    draft
        .strip_prefix('/')
        .unwrap_or_default()
        .split_whitespace()
}
