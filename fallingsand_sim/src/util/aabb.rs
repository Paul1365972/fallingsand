use itertools::Itertools;

pub struct AABB {
    min: (i32, i32),
    max: (i32, i32),
}

impl AABB {
    pub fn new(min: (i32, i32), max: (i32, i32)) -> AABB {
        AABB {
            min: (min.0.min(max.0), min.1.min(max.1)),
            max: (min.0.max(max.0), min.1.max(max.1)),
        }
    }

    pub fn from_point(p: (i32, i32)) -> AABB {
        Self::new(p, (p.0 + 1, p.1 + 1))
    }

    pub fn from_radius(size: i32) -> AABB {
        Self::new((-size, -size), (size + 1, size + 1))
    }

    pub fn translate(&self, p: (i32, i32)) -> AABB {
        Self::new(
            (self.min.0 + p.0, self.min.1 + p.1),
            (self.max.0 + p.0, self.max.1 + p.1),
        )
    }

    pub fn include(&mut self, p: (i32, i32)) -> AABB {
        AABB::new(
            (self.min.0.min(p.0), self.min.1.min(p.1)),
            (self.max.0.max(p.0), self.max.1.max(p.1)),
        )
    }

    pub fn grow(&mut self, v: i32) -> AABB {
        AABB::new(
            (self.min.0 - v, self.min.1 - v),
            (self.max.0 + v, self.max.1 + v),
        )
    }

    pub fn union(&self, other: &AABB) -> AABB {
        AABB::new(
            (self.min.0.min(other.min.0), self.min.1.min(other.min.1)),
            (self.max.0.max(other.max.0), self.max.1.max(other.max.1)),
        )
    }

    pub fn intersections(&self, other: &AABB) -> AABB {
        AABB::new(
            (self.min.0.max(other.min.0), self.min.1.max(other.min.1)),
            (self.max.0.min(other.max.0), self.max.1.min(other.max.1)),
        )
    }

    pub fn contains(&self, other: &AABB) -> bool {
        other.min.0 >= self.min.0
            && other.min.1 >= self.min.1
            && other.max.0 <= self.max.0
            && other.max.1 <= self.max.1
    }

    pub fn iter(&self) -> itertools::Product<std::ops::Range<i32>, std::ops::Range<i32>> {
        ((self.min.0)..(self.max.0)).cartesian_product((self.min.1)..(self.max.1))
    }
}
