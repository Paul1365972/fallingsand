use crate::messages::PlayerId;
use serde::{Deserialize, Serialize};

pub const CHAT_MAX_CHARS: usize = 240;
pub const HISTORY_CAP: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatKind {
    Say,
    System,
    Error,
    Announce,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatEntry {
    pub kind: ChatKind,
    pub author: Option<(PlayerId, String)>,
    pub text: String,
}

impl ChatEntry {
    pub fn say(player: PlayerId, name: String, text: String) -> Self {
        Self {
            kind: ChatKind::Say,
            author: Some((player, name)),
            text,
        }
    }

    fn bare(kind: ChatKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            author: None,
            text: text.into(),
        }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::bare(ChatKind::System, text)
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self::bare(ChatKind::Error, text)
    }

    pub fn announce(text: impl Into<String>) -> Self {
        Self::bare(ChatKind::Announce, text)
    }
}

pub fn clamp_line(text: &str) -> String {
    text.trim().chars().take(CHAT_MAX_CHARS).collect()
}

pub fn push_history(history: &mut Vec<String>, line: &str) {
    if history.last().is_some_and(|last| last == line) {
        return;
    }
    history.push(line.to_string());
    if history.len() > HISTORY_CAP {
        history.drain(..history.len() - HISTORY_CAP);
    }
}
