use crate::caves::Carve;
use fallingsand_core::MaterialId;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    Sky,
    Cave,
    Stone,
}

pub struct Build {
    pub x: i32,
    pub y: i32,
    pub material: MaterialId,
}

pub struct Site<'a> {
    carve: &'a Carve,
    buried: &'a dyn Fn(i32, i32) -> bool,
}

impl<'a> Site<'a> {
    pub fn new(carve: &'a Carve, buried: &'a dyn Fn(i32, i32) -> bool) -> Self {
        Self { carve, buried }
    }

    pub fn at(&self, x: i32, y: i32) -> Ground {
        if !(self.buried)(x, y) {
            Ground::Sky
        } else if self.carve.is_open(x, y) {
            Ground::Cave
        } else {
            Ground::Stone
        }
    }

    pub fn hollow_above(&self, x: i32, y: i32, need: i32) -> bool {
        (1..=need).all(|step| self.at(x, y + step) == Ground::Cave)
    }

    pub fn floor_near(&self, x: i32, y: i32, span: i32, headroom: i32) -> Option<i32> {
        let mut level = y + span;
        while level >= y - span {
            if self.at(x, level) == Ground::Stone && self.hollow_above(x, level, headroom) {
                return Some(level);
            }
            level -= 1;
        }
        None
    }

    pub fn ceiling_near(&self, x: i32, y: i32, span: i32, drop: i32) -> Option<i32> {
        let mut level = y - span;
        while level <= y + span {
            if self.at(x, level) == Ground::Stone
                && (1..=drop).all(|step| self.at(x, level - step) == Ground::Cave)
            {
                return Some(level);
            }
            level += 1;
        }
        None
    }

    pub fn bench(&self, x: i32, y: i32, half: i32, headroom: i32) -> bool {
        (-half..=half).all(|offset| {
            self.at(x + offset, y) == Ground::Stone && self.hollow_above(x + offset, y, headroom)
        })
    }
}
