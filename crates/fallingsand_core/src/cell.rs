use crate::material::MaterialId;
use serde::{Deserialize, Serialize};

pub use fallingsand_material::VEL_ONE;
const _: () = assert!(VEL_ONE == crate::Fixed::ONE.raw() as i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct Cell {
    pub material: MaterialId,
    pub vx: i16,
    pub vy: i16,
    pub shade_flags: u8,
    pub updated: u8,
}

const _: () = assert!(size_of::<Cell>() == 8);

impl Cell {
    pub const AIR: Self = Self {
        material: MaterialId::AIR,
        vx: 0,
        vy: 0,
        shade_flags: 0,
        updated: 0,
    };

    pub const fn new(material: MaterialId, shade: u8) -> Self {
        Self {
            material,
            vx: 0,
            vy: 0,
            shade_flags: (shade & 0x0F) << 4,
            updated: 0,
        }
    }

    pub const fn shade(self) -> u8 {
        self.shade_flags >> 4
    }

    pub fn set_shade(&mut self, shade: u8) {
        self.shade_flags = (self.shade_flags & 0x0F) | ((shade & 0x0F) << 4);
    }

    pub const fn vel(self) -> (i32, i32) {
        (self.vx as i32, self.vy as i32)
    }

    pub fn set_vel(&mut self, vx: i32, vy: i32) {
        self.vx = vx.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        self.vy = vy.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }

    pub const fn is_body(self) -> bool {
        self.shade_flags & 0x08 != 0
    }

    pub fn set_body(&mut self, body: bool) {
        if body {
            self.shade_flags |= 0x08;
        } else {
            self.shade_flags &= !0x08;
        }
    }

    pub const fn is_air(self) -> bool {
        self.material.0 == MaterialId::AIR.0
    }
}
