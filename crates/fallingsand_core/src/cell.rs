use crate::material::MaterialId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    Velocity(i32, i32),
    Body(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell {
    pub material: MaterialId,
    motion: [u8; 4],
    pub shade: u8,
    flags: u8,
}

impl Cell {
    const MOVED: u8 = 0x01;
    const BODY: u8 = 0x02;
    const STRESS: u8 = 0x04;

    pub const AIR: Self = Self {
        material: MaterialId::AIR,
        motion: [0; 4],
        shade: 0,
        flags: 0,
    };

    pub const fn new(material: MaterialId, shade: u8) -> Self {
        Self {
            material,
            motion: [0; 4],
            shade: shade & 0x0F,
            flags: 0,
        }
    }

    const fn pack(vx: i16, vy: i16) -> [u8; 4] {
        let x = vx.to_le_bytes();
        let y = vy.to_le_bytes();
        [x[0], x[1], y[0], y[1]]
    }

    const fn unpack(self) -> (i32, i32) {
        let vx = i16::from_le_bytes([self.motion[0], self.motion[1]]);
        let vy = i16::from_le_bytes([self.motion[2], self.motion[3]]);
        (vx as i32, vy as i32)
    }

    pub const fn motion(self) -> Motion {
        if self.flags & Self::BODY != 0 {
            Motion::Body(u32::from_le_bytes(self.motion))
        } else {
            let (vx, vy) = self.unpack();
            Motion::Velocity(vx, vy)
        }
    }

    pub const fn body_id(self) -> Option<u32> {
        match self.motion() {
            Motion::Body(id) => Some(id),
            Motion::Velocity(..) => None,
        }
    }

    pub const fn vel(self) -> (i32, i32) {
        debug_assert!(
            self.flags & Self::BODY == 0,
            "velocity read on a body-tagged cell"
        );
        self.unpack()
    }

    pub fn set_vel(&mut self, vx: i32, vy: i32) {
        debug_assert!(
            self.flags & Self::BODY == 0,
            "velocity write on a body-tagged cell"
        );
        self.motion = Self::pack(
            vx.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            vy.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        );
    }

    pub fn set_body(&mut self, id: u32) {
        self.motion = id.to_le_bytes();
        self.flags |= Self::BODY;
    }

    pub fn clear_body(&mut self) {
        if self.flags & Self::BODY != 0 {
            self.flags &= !Self::BODY;
            self.motion = [0; 4];
        }
    }

    pub const fn is_moved(self) -> bool {
        self.flags & Self::MOVED != 0
    }

    pub fn set_moved(&mut self) {
        self.flags |= Self::MOVED;
    }

    pub fn clear_moved(&mut self) {
        self.flags &= !Self::MOVED;
    }

    pub const fn is_stressed(self) -> bool {
        self.flags & Self::STRESS != 0
    }

    pub fn set_stressed(&mut self) {
        self.flags |= Self::STRESS;
    }

    pub fn clear_stressed(&mut self) {
        self.flags &= !Self::STRESS;
    }

    pub const fn is_air(self) -> bool {
        self.material.0 == MaterialId::AIR.0
    }
}
