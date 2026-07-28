use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamKind {
    Choice,
    Command,
    Free,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamSpec {
    pub name: String,
    pub required: bool,
    pub kind: ParamKind,
    pub choices: Vec<String>,
}

impl ParamSpec {
    pub fn new(name: &str, kind: ParamKind, choices: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            required: true,
            kind,
            choices: choices.iter().map(|choice| choice.to_string()).collect(),
        }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    fn render(&self) -> String {
        let body = match self.kind {
            ParamKind::Choice => self.choices.join("|"),
            ParamKind::Command | ParamKind::Free => self.name.clone(),
        };
        match self.required {
            true => format!("<{body}>"),
            false => format!("[{body}]"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandInfo {
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: String,
    pub params: Vec<ParamSpec>,
}

impl CommandInfo {
    pub fn matches(&self, name: &str) -> bool {
        self.name == name || self.aliases.iter().any(|alias| alias == name)
    }

    pub fn segments(&self) -> impl Iterator<Item = String> {
        std::iter::once(format!("/{}", self.name)).chain(self.params.iter().map(ParamSpec::render))
    }

    pub fn usage(&self) -> String {
        self.segments().collect::<Vec<_>>().join(" ")
    }
}
