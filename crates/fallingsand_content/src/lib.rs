mod author;
mod compiler;
mod definitions;
mod emit;

pub use author::*;

pub fn compile() -> Result<String, Error> {
    let catalog = definitions::catalog();
    let content = compiler::build(&catalog)?;
    let tokens = emit::emit(&content);
    let file = syn::parse2::<syn::File>(tokens)
        .map_err(|err| Error::new(format!("generated content is not valid Rust: {err}")))?;
    Ok(prettyplease::unparse(&file))
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
