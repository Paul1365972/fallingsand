use crate::material::MaterialId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct Cell {
    pub material: MaterialId,
    pub shade_flags: u8,
    pub updated: u8,
}

const _: () = assert!(size_of::<Cell>() == 4);

impl Cell {
    pub const AIR: Self = Self {
        material: MaterialId::AIR,
        shade_flags: 0,
        updated: 0,
    };

    pub const fn new(material: MaterialId, shade: u8) -> Self {
        Self {
            material,
            shade_flags: shade << 4,
            updated: 0,
        }
    }

    pub const fn shade(self) -> u8 {
        self.shade_flags >> 4
    }

    pub fn set_shade(&mut self, shade: u8) {
        self.shade_flags = (self.shade_flags & 0x0F) | (shade << 4);
    }

    pub const FLOW_NONE: u8 = 0;
    pub const FLOW_LEFT: u8 = 1;
    pub const FLOW_RIGHT: u8 = 2;

    pub const fn flow_state(self) -> u8 {
        self.shade_flags & 0x03
    }

    pub fn set_flow_state(&mut self, state: u8) {
        self.shade_flags = (self.shade_flags & !0x03) | (state & 0x03);
    }

    pub const fn flow_spent(self) -> bool {
        self.shade_flags & 0x04 != 0
    }

    pub fn set_flow_spent(&mut self, spent: bool) {
        if spent {
            self.shade_flags |= 0x04;
        } else {
            self.shade_flags &= !0x04;
        }
    }

    pub const fn is_air(self) -> bool {
        self.material.0 == MaterialId::AIR.0
    }
}
