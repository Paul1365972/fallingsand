pub mod api;
mod items;
mod macros;
mod recipes;
pub mod spec;

pub use api::*;
pub use items::item;
pub use spec::MatSpec;

include!(concat!(env!("OUT_DIR"), "/content.rs"));
