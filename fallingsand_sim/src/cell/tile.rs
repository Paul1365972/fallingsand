
#[derive(Debug, Copy, Clone)]
pub struct MyTile {
    pub variant: MyTileVariant,
    //pub clock: u8,
    //pub temperature: u16, // fix point (4bit) 0K - 4096K with 0.0625 steps (water freezes at 4371 and boils at 5971, Temp of TNT explosion 56000)
}

#[derive(Debug, Copy, Clone)]
pub enum MyTileVariant {
    //#[default]
    NIL,
    AIR,
    SAND,
    STONE,
    WATER,
}

pub struct TileEntity {}
