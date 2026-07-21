use super::materials::validate_number;
use super::quantize::per_tick_chance;
use super::{Catalog, Error, HashMap, MaterialId, RawMaterial, Reaction, fail};
use crate::{OperandDef, ProductDef};
use fallingsand_material::Tag;
use fallingsand_math::chance_threshold;

enum Operand {
    Material(MaterialId),
    Tag(Tag),
}

#[derive(Clone, Copy)]
enum Product {
    Material(MaterialId),
    Same,
}

impl Product {
    fn resolve(self, operand_id: MaterialId) -> MaterialId {
        match self {
            Self::Material(id) => id,
            Self::Same => operand_id,
        }
    }
}

pub(super) fn expand_reactions(
    catalog: &Catalog,
    raws: &[RawMaterial],
    by_name: &HashMap<String, MaterialId>,
) -> Result<Vec<Option<Reaction>>, Error> {
    let len = raws.len();
    let resolve_operand = |definition: &OperandDef| -> Result<Operand, Error> {
        match definition {
            OperandDef::Material(key) => by_name
                .get(key.as_str())
                .copied()
                .map(Operand::Material)
                .ok_or_else(|| fail(format!("reactions: unknown material `{key}`"))),
            OperandDef::Tag(tag) => Ok(Operand::Tag(*tag)),
        }
    };
    let resolve_product =
        |definition: &ProductDef, operand: &OperandDef| -> Result<Product, Error> {
            match definition {
                ProductDef::Material(key) => by_name
                    .get(key.as_str())
                    .copied()
                    .map(Product::Material)
                    .ok_or_else(|| fail(format!("reactions: unknown material `{key}`"))),
                ProductDef::Same(tag) => match operand {
                    OperandDef::Tag(operand_tag) if operand_tag == tag => Ok(Product::Same),
                    _ => Err(fail(format!(
                        "reactions: same({tag:?}) must repeat the tag operand on its side"
                    ))),
                },
            }
        };
    let expand = |operand: &Operand| -> Vec<MaterialId> {
        match operand {
            Operand::Material(id) => vec![*id],
            Operand::Tag(tag) => (0..len)
                .filter(|&index| raws[index].tags.contains(*tag))
                .map(|index| MaterialId(index as u16))
                .collect(),
        }
    };

    let mut table: Vec<Option<(Reaction, u8)>> = vec![None; len * len];
    for definition in &catalog.reactions {
        validate_number("reactions: rate", definition.rate)?;
        let a = resolve_operand(&definition.a)?;
        let b = resolve_operand(&definition.b)?;
        let becomes_a = resolve_product(&definition.a_becomes, &definition.a)?;
        let becomes_b = resolve_product(&definition.b_becomes, &definition.b)?;
        let threshold = chance_threshold(per_tick_chance(definition.rate));
        let specificity =
            matches!(a, Operand::Material(_)) as u8 + matches!(b, Operand::Material(_)) as u8;
        for a_id in expand(&a) {
            for b_id in expand(&b) {
                let out_a = becomes_a.resolve(a_id);
                let out_b = becomes_b.resolve(b_id);
                let entries = if a_id == b_id {
                    vec![(a_id, b_id, out_a, out_b)]
                } else {
                    vec![(a_id, b_id, out_a, out_b), (b_id, a_id, out_b, out_a)]
                };
                for (from, other, becomes, other_becomes) in entries {
                    let slot = &mut table[from.0 as usize * len + other.0 as usize];
                    match slot {
                        Some((_, existing)) if *existing == specificity => {
                            return Err(fail(format!(
                                "reactions: ambiguous reactions for pair {} + {}",
                                raws[from.0 as usize].name, raws[other.0 as usize].name
                            )));
                        }
                        Some((_, existing)) if *existing > specificity => {}
                        _ => {
                            *slot = Some((
                                Reaction {
                                    becomes,
                                    other_becomes,
                                    threshold,
                                },
                                specificity,
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(table
        .into_iter()
        .map(|slot| slot.map(|(reaction, _)| reaction))
        .collect())
}
