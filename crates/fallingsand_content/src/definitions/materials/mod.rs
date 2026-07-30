pub mod crafted;
pub mod fire;
pub mod flora;
pub mod fluids;
pub mod gases;
pub mod ores;
pub mod soil;
pub mod special;
pub mod stone;
pub mod toys;

use crate::Catalog;

pub fn define(catalog: &mut Catalog) {
    special::define(catalog);
    soil::define(catalog);
    stone::define(catalog);
    ores::define(catalog);
    fluids::define(catalog);
    gases::define(catalog);
    flora::define(catalog);
    fire::define(catalog);
    crafted::define(catalog);
    toys::define(catalog);
}
