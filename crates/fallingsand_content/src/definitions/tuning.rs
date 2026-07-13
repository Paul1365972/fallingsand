use crate::Catalog;

pub fn define(catalog: &mut Catalog) {
    catalog.threshold("FLICKER_THRESHOLD", 18.0);
}
