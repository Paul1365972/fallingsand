use crate::material::MaterialId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell {
    pub material: MaterialId,
    pub vx: i16,
    pub vy: i16,
    pub shade: u8,
    pub flags: u8,
    pub aux: u8,
}

impl Cell {
    pub const MOVED: u8 = 0x01;
    pub const BODY: u8 = 0x02;

    pub const AIR: Self = Self {
        material: MaterialId::AIR,
        vx: 0,
        vy: 0,
        shade: 0,
        flags: 0,
        aux: 0,
    };

    pub const fn new(material: MaterialId, shade: u8) -> Self {
        Self {
            material,
            vx: 0,
            vy: 0,
            shade: shade & 0x0F,
            flags: 0,
            aux: 0,
        }
    }

    pub const fn vel(self) -> (i32, i32) {
        (self.vx as i32, self.vy as i32)
    }

    pub fn set_vel(&mut self, vx: i32, vy: i32) {
        self.vx = vx.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        self.vy = vy.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }

    pub const fn is_body(self) -> bool {
        self.flags & Self::BODY != 0
    }

    pub fn set_body(&mut self, body: bool) {
        if body {
            self.flags |= Self::BODY;
        } else {
            self.flags &= !Self::BODY;
        }
    }

    pub const fn is_air(self) -> bool {
        self.material.0 == MaterialId::AIR.0
    }
}
