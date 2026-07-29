pub mod fire;
pub mod flora;
pub mod fluids;
pub mod ores;
pub mod special;
pub mod terrain;
pub mod toys;

use crate::Catalog;

pub fn define(catalog: &mut Catalog) {
    special::define(catalog);
    terrain::define(catalog);
    ores::define(catalog);
    fluids::define(catalog);
    flora::define(catalog);
    fire::define(catalog);
    toys::define(catalog);
}
