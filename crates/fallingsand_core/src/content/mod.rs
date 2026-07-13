pub mod api;
pub mod spec;

pub use api::*;
pub use spec::MatSpec;

include!(concat!(env!("OUT_DIR"), "/content.rs"));
