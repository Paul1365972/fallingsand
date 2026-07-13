mod author;
mod definitions;
mod emit;
mod model;

pub use author::*;

pub fn compile() -> Result<String, Error> {
    let catalog = definitions::catalog();
    let content = model::build(&catalog)?;
    Ok(emit::emit(&content).to_string())
}

#[derive(Debug, Clone)]
pub struct Error(String);

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for Error {}
