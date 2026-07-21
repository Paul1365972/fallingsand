use super::items::validate_ident;
use super::{Error, fail};
use crate::{
    BurningDef, Catalog, EmissionDef, FlammableDef, GasDef, LiquidDef, MaterialDef, PhaseDef,
    PowderDef,
};
use fallingsand_material::Tags;

#[derive(Clone)]
pub(super) struct RawMaterial {
    pub(super) name: String,
    pub(super) phase: PhaseDef,
    pub(super) density: f32,
    pub(super) colors: Vec<[u8; 4]>,
    pub(super) surface_grip: f32,
    pub(super) hardness: f32,
    pub(super) restitution: f32,
    pub(super) surface_bounce: f32,
    pub(super) contact_damage: f32,
    pub(super) tags: Tags,
    pub(super) flammable: Option<FlammableDef>,
    pub(super) burning: Option<BurningDef>,
    pub(super) emission: Option<EmissionDef>,
}

impl RawMaterial {
    fn defaults(name: String) -> Self {
        Self {
            name,
            phase: PhaseDef::Empty,
            density: 0.0,
            colors: Vec::new(),
            surface_grip: 1.0,
            hardness: 0.0,
            restitution: 0.0,
            surface_bounce: 0.0,
            contact_damage: 0.0,
            tags: Tags::EMPTY,
            flammable: None,
            burning: None,
            emission: None,
        }
    }
}

pub(super) fn build_materials(catalog: &Catalog) -> Result<Vec<RawMaterial>, Error> {
    let mut raws = Vec::with_capacity(catalog.materials.len());
    for (key, definition) in &catalog.materials {
        validate_ident("material key", key.as_str())?;
        let mut raw = match &definition.base {
            Some(base) => {
                let inherited = raws
                    .iter()
                    .find(|raw: &&RawMaterial| raw.name == base.as_str())
                    .ok_or_else(|| {
                        fail(format!(
                            "material {key}: unknown base `{base}` (bases must be defined earlier)"
                        ))
                    })?;
                RawMaterial {
                    name: key.as_str().to_owned(),
                    ..(*inherited).clone()
                }
            }
            None => RawMaterial::defaults(key.as_str().to_owned()),
        };
        apply_definition(&mut raw, definition);
        validate_material(&raw)?;
        raws.push(raw);
    }
    Ok(raws)
}

fn apply_definition(raw: &mut RawMaterial, definition: &MaterialDef) {
    if let Some(phase) = definition.phase {
        raw.phase = phase;
    }
    if let Some(value) = definition.density {
        raw.density = value;
    }
    if let Some(value) = &definition.colors {
        raw.colors.clone_from(value);
    }
    if let Some(value) = definition.surface_grip {
        raw.surface_grip = value;
    }
    if let Some(value) = definition.hardness {
        raw.hardness = value;
    }
    if let Some(value) = definition.restitution {
        raw.restitution = value;
    }
    if let Some(value) = definition.surface_bounce {
        raw.surface_bounce = value;
    }
    if let Some(value) = definition.contact_damage {
        raw.contact_damage = value;
    }
    if let Some(value) = &definition.tags {
        raw.tags = Tags::new(value);
    }
    if let Some(value) = &definition.flammable {
        raw.flammable = Some(value.clone());
    }
    if let Some(value) = &definition.burning {
        raw.burning = Some(value.clone());
    }
    if let Some(value) = definition.emission {
        raw.emission = Some(value);
    }
}

fn validate_material(raw: &RawMaterial) -> Result<(), Error> {
    let context = format!("material {}", raw.name);
    if raw.burning.is_some() && raw.flammable.is_some() {
        return Err(fail(format!(
            "{context}: a burning material cannot also be flammable"
        )));
    }
    if !matches!(raw.phase, PhaseDef::Empty | PhaseDef::Solid(_)) && raw.density <= 0.0 {
        return Err(fail(format!("{context}: moving phases need density > 0")));
    }
    for (field, value) in [
        ("density", raw.density),
        ("surface_grip", raw.surface_grip),
        ("hardness", raw.hardness),
        ("restitution", raw.restitution),
        ("surface_bounce", raw.surface_bounce),
        ("contact_damage", raw.contact_damage),
    ] {
        validate_number(&format!("{context}: {field}"), value)?;
    }
    match raw.phase {
        PhaseDef::Empty | PhaseDef::Solid(_) => {}
        PhaseDef::Powder(PowderDef {
            air_drag,
            ground_friction,
            topple_start,
            topple_keep,
            deflect,
        }) => validate_numbers(
            &context,
            &[
                ("air_drag", air_drag),
                ("ground_friction", ground_friction),
                ("topple_start", topple_start),
                ("topple_keep", topple_keep),
                ("deflect", deflect),
            ],
        )?,
        PhaseDef::Liquid(LiquidDef {
            drag,
            impact,
            flow_rate,
        }) => {
            validate_numbers(&context, &[("drag", drag), ("impact", impact)])?;
            validate_flow_rate(&context, flow_rate)?;
        }
        PhaseDef::Gas(GasDef {
            air_drag,
            turbulence,
            flow_rate,
        }) => {
            validate_numbers(
                &context,
                &[("air_drag", air_drag), ("turbulence", turbulence)],
            )?;
            validate_flow_rate(&context, flow_rate)?;
        }
    }
    if let Some(flammable) = &raw.flammable {
        if flammable.ignite <= 0.0 {
            return Err(fail(format!("{context}: flammable ignite must be > 0")));
        }
        validate_numbers(
            &context,
            &[
                ("flammable ignite", flammable.ignite),
                ("flammable sealed_burn", flammable.sealed_burn),
                ("flammable rate", flammable.rate),
                ("flammable emit", flammable.emit),
                ("flammable residue_chance", flammable.residue_chance),
                ("flammable damage", flammable.damage),
            ],
        )?;
        if flammable.residue.is_some() && flammable.residue_chance <= 0.0 {
            return Err(fail(format!(
                "{context}: flammable residue is set but residue_chance is 0 (it would never appear)"
            )));
        }
    }
    if let Some(emission) = &raw.emission {
        validate_numbers(
            &context,
            &[
                ("emission intensity", emission.intensity),
                ("emission flicker", emission.flicker),
            ],
        )?;
    }
    if let Some(burning) = &raw.burning {
        validate_numbers(
            &context,
            &[
                ("burning rate", burning.rate),
                ("burning emit", burning.emit),
                ("burning residue_chance", burning.residue_chance),
            ],
        )?;
        if burning.residue.is_some() && burning.residue_chance <= 0.0 {
            return Err(fail(format!(
                "{context}: burning residue is set but residue_chance is 0 (it would never appear)"
            )));
        }
    }
    Ok(())
}

fn validate_flow_rate(context: &str, flow_rate: Option<f32>) -> Result<(), Error> {
    if let Some(rate) = flow_rate {
        validate_number(&format!("{context}: flow_rate"), rate)?;
        if rate <= 0.0 {
            return Err(fail(format!("{context}: flow_rate must be > 0")));
        }
    }
    Ok(())
}

pub(super) fn validate_numbers(context: &str, values: &[(&str, f32)]) -> Result<(), Error> {
    for &(field, value) in values {
        validate_number(&format!("{context}: {field}"), value)?;
    }
    Ok(())
}

pub(super) fn validate_number(context: &str, value: f32) -> Result<(), Error> {
    if !value.is_finite() || value < 0.0 {
        return Err(fail(format!(
            "{context} must be a finite non-negative number"
        )));
    }
    Ok(())
}
