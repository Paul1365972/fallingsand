mod items;
mod materials;
mod quantize;
mod reactions;

use crate::{
    BOND_GROUP_COUNT, BurningDef, Catalog, EmissionDef, Error, MaterialKey, PhaseDef, SolidDef,
};
use fallingsand_material::{
    Burning, BurningKind, Dynamics, Ignition, MaterialId, Phase, Reaction, SealedBurn, Tag, Tags,
};
use fallingsand_math::chance_threshold;
use std::collections::HashMap;

use items::{build_items, build_recipes};
use materials::{RawMaterial, build_materials, validate_number};
use quantize::{
    bake_emission, compile_liquid_exchange_thresholds, flow_threshold, milli, per_tick_chance,
    phase_tag, quantize_dynamics,
};
use reactions::expand_reactions;

const BURNING_EMISSION: EmissionDef = EmissionDef {
    color: [255, 120, 32],
    intensity: 1.4,
    flicker: 0.5,
};

pub(crate) const fn mining_tier_from_hardness(hardness: f32) -> u8 {
    if hardness <= 0.35 {
        0
    } else if hardness <= 1.0 {
        1
    } else if hardness <= 2.0 {
        2
    } else {
        3
    }
}

pub struct Mat {
    pub name: String,
    pub const_name: String,
    pub spec_name: String,
    pub phase: Phase,
    pub density_milli: i32,
    pub colors: Vec<[u8; 4]>,
    pub tags: Tags,
    pub rigid_capable: bool,
    pub bond_group: Option<u8>,
    pub hardness: f32,
    pub restitution: f32,
    pub surface_grip: f32,
    pub surface_bounce: f32,
    pub contact_damage: f32,
    pub emission: [f32; 3],
    pub flicker: f32,
    pub burning: Option<Burning>,
    pub decay: Option<(u64, MaterialId)>,
    pub reactive: bool,
    pub dynamics: Dynamics,
    pub flow_threshold: u64,
}

pub struct ItemOut {
    pub name: String,
    pub display: String,
    pub stack_max: u32,
    pub sprite: String,
    pub place: Option<MaterialId>,
    pub tool: Option<(u8, f32)>,
}

pub struct RecipeOut {
    pub inputs: Vec<(u16, u32)>,
    pub output: (u16, u32),
}

pub struct Content {
    pub materials: Vec<Mat>,
    pub ignitions: Vec<Option<Ignition>>,
    pub reactions: Vec<Option<Reaction>>,
    pub liquid_exchange_thresholds: Vec<u64>,
    pub items: Vec<ItemOut>,
    pub recipes: Vec<RecipeOut>,
    pub item_for_material: Vec<u16>,
    pub bond_masks: Vec<u32>,
}

pub fn build(catalog: &Catalog) -> Result<Content, Error> {
    let mut raws = build_materials(catalog)?;

    match raws.first() {
        Some(first) if matches!(first.phase, PhaseDef::Empty) => {}
        Some(first) => {
            return Err(fail(format!(
                "material 0 must be air with phase Empty, got {}",
                first.name
            )));
        }
        None => return Err(fail("no materials defined")),
    }

    let hand_len = raws.len();
    let flammable_count = raws.iter().filter(|raw| raw.flammable.is_some()).count();
    if hand_len + flammable_count > u16::MAX as usize {
        return Err(fail(format!(
            "too many materials: {}",
            hand_len + flammable_count
        )));
    }
    let mut ignitions: Vec<Option<Ignition>> = vec![None; hand_len];
    for index in 0..hand_len {
        let Some(flammable) = raws[index].flammable.clone() else {
            continue;
        };
        let base = raws[index].clone();
        let sealed_keep = flammable.sealed_burn.clamp(0.0, 1.0);
        ignitions[index] = Some(Ignition {
            into: MaterialId(raws.len() as u16),
            open: chance_threshold(per_tick_chance(flammable.ignite)),
            sealed: chance_threshold(per_tick_chance(flammable.ignite * sealed_keep)),
        });
        raws.push(RawMaterial {
            name: format!("burning_{}", base.name),
            colors: if flammable.colors.is_empty() {
                catalog.burning_colors.clone()
            } else {
                flammable.colors.clone()
            },
            contact_damage: flammable.damage.max(base.contact_damage),
            tags: base.tags.union(Tags::new(&[Tag::Hot])),
            burning: Some(BurningDef {
                rate: flammable.rate,
                sealed_burn: flammable.sealed_burn,
                emit: flammable.emit,
                residue: flammable.residue.clone(),
                residue_chance: flammable.residue_chance,
                burnout: flammable.burnout.clone(),
                base: Some(MaterialId(index as u16)),
            }),
            flammable: None,
            emission: Some(BURNING_EMISSION),
            ..base
        });
    }

    let len = raws.len();
    ignitions.resize(len, None);

    for raw in &raws {
        if raw.colors.is_empty() {
            return Err(fail(format!("material {} has no colors", raw.name)));
        }
    }

    let mut by_name = HashMap::new();
    for (index, raw) in raws.iter().enumerate() {
        let const_name = raw.name.to_ascii_uppercase();
        if by_name
            .insert(const_name, MaterialId(index as u16))
            .is_some()
        {
            return Err(fail(format!("duplicate material name {}", raw.name)));
        }
    }

    let resolve =
        |handle: &Option<MaterialKey>, owner: &str| -> Result<Option<MaterialId>, Error> {
            match handle {
                None => Ok(None),
                Some(key) => by_name
                    .get(key.as_str())
                    .copied()
                    .map(Some)
                    .ok_or_else(|| fail(format!("material {owner}: unknown material `{key}`"))),
            }
        };

    let reactions = expand_reactions(catalog, &raws, &by_name)?;
    let liquid_exchange_thresholds = compile_liquid_exchange_thresholds(&raws);
    let mut decays: Vec<Option<(u64, MaterialId)>> = vec![None; len];
    for def in &catalog.decays {
        let Some(from) = by_name.get(def.from.as_str()) else {
            return Err(fail(format!("reactions: unknown material `{}`", def.from)));
        };
        let Some(into) = by_name.get(def.into.as_str()) else {
            return Err(fail(format!("reactions: unknown material `{}`", def.into)));
        };
        let slot = &mut decays[from.0 as usize];
        if slot.is_some() {
            return Err(fail(format!(
                "reactions: duplicate decay for {}",
                raws[from.0 as usize].name
            )));
        }
        validate_number("reactions: decay rate", def.rate)?;
        *slot = Some((chance_threshold(per_tick_chance(def.rate)), *into));
    }

    let mut materials = Vec::with_capacity(len);
    for (index, raw) in raws.iter().enumerate() {
        let decay = decays[index];
        let burning = match &raw.burning {
            Some(raw_burning) => {
                let residue = match (&raw_burning.residue, raw_burning.residue_chance) {
                    (Some(_), chance) if chance > 0.0 => {
                        let id =
                            resolve(&raw_burning.residue, &raw.name)?.unwrap_or(MaterialId::AIR);
                        Some((chance_threshold(chance.clamp(0.0, 1.0)), id))
                    }
                    _ => None,
                };
                let sealed_keep = raw_burning.sealed_burn.clamp(0.0, 1.0);
                Some(Burning {
                    burn: chance_threshold(per_tick_chance(raw_burning.rate)),
                    sealed: if sealed_keep > 0.0 {
                        SealedBurn::Smoulder(chance_threshold(per_tick_chance(
                            raw_burning.rate * sealed_keep,
                        )))
                    } else {
                        let out =
                            resolve(&raw_burning.burnout, &raw.name)?.unwrap_or(MaterialId::AIR);
                        SealedBurn::Becomes(raw_burning.base.unwrap_or(out))
                    },
                    emit: chance_threshold(per_tick_chance(raw_burning.emit)),
                    residue,
                    burnout: resolve(&raw_burning.burnout, &raw.name)?.unwrap_or(MaterialId::AIR),
                    kind: if index < hand_len {
                        BurningKind::Flame
                    } else {
                        BurningKind::Fuel
                    },
                })
            }
            None => None,
        };

        let reactive = decay.is_some()
            || reactions[index * len..(index + 1) * len]
                .iter()
                .any(Option::is_some);
        let const_name = raw.name.to_ascii_uppercase();
        let (emission, flicker) = bake_emission(raw.emission);
        let bond_group = match raw.phase {
            PhaseDef::Solid(SolidDef { bond: Some(group) }) => Some(group as u8),
            _ => None,
        };
        materials.push(Mat {
            spec_name: camel_case(&const_name),
            name: raw.name.to_ascii_lowercase(),
            const_name,
            phase: phase_tag(raw.phase),
            density_milli: milli(raw.density),
            colors: raw.colors.clone(),
            emission,
            flicker,
            tags: raw.tags,
            rigid_capable: bond_group.is_some(),
            bond_group,
            hardness: raw.hardness,
            restitution: raw.restitution,
            surface_grip: raw.surface_grip,
            surface_bounce: raw.surface_bounce,
            contact_damage: raw.contact_damage,
            burning,
            decay,
            reactive,
            dynamics: quantize_dynamics(raw),
            flow_threshold: flow_threshold(raw),
        });
    }

    let mut fuel_base = vec![None; len];
    for (base, ignition) in ignitions.iter().enumerate() {
        if let Some(ignition) = ignition {
            fuel_base[ignition.into.0 as usize] = Some(MaterialId(base as u16));
        }
    }

    let (items, item_for_material) = build_items(catalog, &materials, &fuel_base)?;
    let recipes = build_recipes(catalog, &by_name, &item_for_material)?;

    let mut bond_masks = vec![0u32; BOND_GROUP_COUNT];
    for (group, mask) in bond_masks.iter_mut().enumerate() {
        *mask |= 1 << group;
    }
    for &(a, b) in &catalog.bonds {
        bond_masks[a as usize] |= 1 << (b as usize);
        bond_masks[b as usize] |= 1 << (a as usize);
    }

    Ok(Content {
        materials,
        ignitions,
        reactions,
        liquid_exchange_thresholds,
        items,
        recipes,
        item_for_material,
        bond_masks,
    })
}

fn camel_case(const_name: &str) -> String {
    let mut out = String::new();
    for word in const_name.split('_') {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(&chars.as_str().to_ascii_lowercase());
        }
    }
    out
}

pub(super) fn fail(message: impl Into<String>) -> Error {
    Error::new(message)
}
