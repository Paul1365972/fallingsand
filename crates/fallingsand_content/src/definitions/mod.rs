mod materials;
mod reactions;

use crate::Catalog;

pub fn catalog() -> Catalog {
    let mut catalog = Catalog::new([
        [255, 190, 60, 255],
        [255, 150, 36, 255],
        [255, 220, 90, 255],
        [236, 120, 24, 255],
    ]);
    materials::define(&mut catalog);
    reactions::define(&mut catalog);
    catalog
}
