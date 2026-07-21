use super::{MaterialKey, Tag};

#[derive(Debug, Clone)]
pub enum OperandDef {
    Material(MaterialKey),
    Tag(Tag),
}

impl From<MaterialKey> for OperandDef {
    fn from(value: MaterialKey) -> Self {
        Self::Material(value)
    }
}

pub fn tagged(tag: Tag) -> OperandDef {
    OperandDef::Tag(tag)
}

#[derive(Debug, Clone)]
pub enum ProductDef {
    Material(MaterialKey),
    Same(Tag),
}

impl From<MaterialKey> for ProductDef {
    fn from(value: MaterialKey) -> Self {
        Self::Material(value)
    }
}

pub fn same(tag: Tag) -> ProductDef {
    ProductDef::Same(tag)
}

#[derive(Debug, Clone)]
pub struct ReactionDef {
    pub(crate) a: OperandDef,
    pub(crate) b: OperandDef,
    pub(crate) a_becomes: ProductDef,
    pub(crate) b_becomes: ProductDef,
    pub(crate) rate: f32,
}

pub struct ReactionBuilder {
    a: OperandDef,
    b: OperandDef,
}

impl ReactionBuilder {
    pub fn becomes(
        self,
        a: impl Into<ProductDef>,
        b: impl Into<ProductDef>,
    ) -> ReactionProductsBuilder {
        ReactionProductsBuilder {
            a: self.a,
            b: self.b,
            a_becomes: a.into(),
            b_becomes: b.into(),
        }
    }
}

pub struct ReactionProductsBuilder {
    a: OperandDef,
    b: OperandDef,
    a_becomes: ProductDef,
    b_becomes: ProductDef,
}

impl ReactionProductsBuilder {
    pub fn rate(self, rate: f32) -> ReactionDef {
        ReactionDef {
            a: self.a,
            b: self.b,
            a_becomes: self.a_becomes,
            b_becomes: self.b_becomes,
            rate,
        }
    }
}

pub fn reaction(a: impl Into<OperandDef>, b: impl Into<OperandDef>) -> ReactionBuilder {
    ReactionBuilder {
        a: a.into(),
        b: b.into(),
    }
}

#[derive(Debug, Clone)]
pub struct DecayDef {
    pub(crate) from: MaterialKey,
    pub(crate) into: MaterialKey,
    pub(crate) rate: f32,
}
