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

pub use memory::{MemoryConnection, MemoryDialer, MemoryListener, memory_listener, memory_pair};

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
