#[derive(Debug, Copy, Clone, Default)]
pub struct Tile {
    pub variant: Variant,
    pub clock: u8,
    pub temperature: u16, // fix point (4bit) 0K - 4096K with 0.0625 steps (water freezes at 4371 and boils at 5971, Temp of TNT explosion 56000)
}

#[derive(Debug, Copy, Clone, Default)]
pub enum Variant {
    #[default]
    NIL,
    AIR,
    SAND,
    STONE,
    WATER,
}
