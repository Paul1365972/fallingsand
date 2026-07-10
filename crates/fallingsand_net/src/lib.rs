#[cfg(any(
    all(feature = "wt-native", not(target_family = "wasm")),
    all(feature = "wt-wasm", target_family = "wasm", target_os = "unknown")
))]
mod framing;
pub mod memory;
#[cfg(all(feature = "wt-native", not(target_family = "wasm")))]
pub mod wt_native;
#[cfg(all(feature = "wt-wasm", target_family = "wasm", target_os = "unknown"))]
pub mod wt_wasm;

pub use memory::{MemoryDialer, MemoryListener, memory_listener};

pub const DEFAULT_PORT: u16 = 4433;

#[cfg(any(feature = "wt-native", feature = "wt-wasm"))]
pub(crate) fn normalize_server_url(input: &str) -> Result<url::Url, url::ParseError> {
    let input = input.trim();
    let with_scheme = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let mut url: url::Url = with_scheme.parse()?;
    if url.port().is_none() && !authority_has_port(&with_scheme) {
        let _ = url.set_port(Some(DEFAULT_PORT));
    }
    Ok(url)
}

#[cfg(any(feature = "wt-native", feature = "wt-wasm"))]
fn authority_has_port(with_scheme: &str) -> bool {
    let Some((_, rest)) = with_scheme.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    match authority.rfind(']') {
        Some(bracket_end) => authority[bracket_end..].contains(':'),
        None => authority.contains(':'),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Closed { reason: String },
}

pub trait Connection: Send + Sync {
    fn send(&mut self, message: Vec<u8>);
    fn poll(&mut self) -> Option<Vec<u8>>;
    fn status(&self) -> ConnectionStatus;
    fn close(&mut self, reason: &str);
}

pub trait Listener: Send + Sync {
    fn poll_accept(&mut self) -> Option<Box<dyn Connection>>;
}
