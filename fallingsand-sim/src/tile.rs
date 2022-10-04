pub struct Tile {
    variant: Variant, 
    temperature: f32,
}

pub enum Variant {
    NIL,
    AIR,
    SAND,
    STONE,
    WATER,
}
