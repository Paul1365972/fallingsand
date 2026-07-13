use crate::{
    BurningDef, Catalog, EmberDef, Error, MaterialDef, MaterialKey, OperandDef, PhaseDef,
    ProductDef,
};
use fallingsand_material::{
    Dynamics, Ember, EmberKind, GasDynamics, Ignition, LiquidDynamics, MaterialId, Phase,
    PowderDynamics, Reaction, Tag, Tags, milli, per_tick_chance, per_tick_keep, q16,
};
use fallingsand_rng::chance_threshold;
use std::collections::HashMap;

#[derive(Clone)]
enum RawPhase {
    Empty,
    Solid {
        rigid_capable: bool,
    },
    Powder {
        drag: f32,
        friction: f32,
        repose: f32,
        redirect_keep: f32,
        cohesion: f32,
    },
    Liquid {
        drag: f32,
        friction: f32,
        redirect_keep: f32,
        cohesion: f32,
        flow_rate: f32,
    },
    Gas {
        drag: f32,
        cohesion: f32,
        turbulence: f32,
        redirect_keep: f32,
    },
}

impl RawPhase {
    fn tag(&self) -> Phase {
        match self {
            Self::Empty => Phase::Empty,
            Self::Solid { .. } => Phase::Solid,
            Self::Powder { .. } => Phase::Powder,
            Self::Liquid { .. } => Phase::Liquid,
            Self::Gas { .. } => Phase::Gas,
        }
    }
}

impl From<PhaseDef> for RawPhase {
    fn from(value: PhaseDef) -> Self {
        match value {
            PhaseDef::Empty => Self::Empty,
            PhaseDef::Solid(def) => Self::Solid {
                rigid_capable: def.rigid_capable,
            },
            PhaseDef::Powder(def) => Self::Powder {
                drag: def.drag,
                friction: def.friction,
                repose: def.repose,
                redirect_keep: def.redirect_keep,
                cohesion: def.cohesion,
            },
            PhaseDef::Liquid(def) => Self::Liquid {
                drag: def.drag,
                friction: def.friction,
                redirect_keep: def.redirect_keep,
                cohesion: def.cohesion,
                flow_rate: def.flow_rate,
            },
            PhaseDef::Gas(def) => Self::Gas {
                drag: def.drag,
                cohesion: def.cohesion,
                turbulence: def.turbulence,
                redirect_keep: def.redirect_keep,
            },
        }
    }
}

#[derive(Clone, Default)]
struct RawBurn {
    ignite: f32,
    smoulder: f32,
    rate: f32,
    emit: f32,
    colors: Vec<[u8; 4]>,
    residue: Option<MaterialKey>,
    residue_chance: f32,
    burnout: Option<MaterialKey>,
    damage: f32,
}

impl From<BurningDef> for RawBurn {
    fn from(value: BurningDef) -> Self {
        Self {
            ignite: value.ignite,
            smoulder: value.smoulder,
            rate: value.rate,
            emit: value.emit,
            colors: value.colors,
            residue: value.residue,
            residue_chance: value.residue_chance,
            burnout: value.burnout,
            damage: value.damage,
        }
    }
}

#[derive(Clone, Default)]
struct RawEmber {
    rate: f32,
    emit: f32,
    residue: Option<MaterialKey>,
    residue_chance: f32,
    burnout: Option<MaterialKey>,
}

impl From<EmberDef> for RawEmber {
    fn from(value: EmberDef) -> Self {
        Self {
            rate: value.rate,
            emit: value.emit,
            residue: value.residue,
            residue_chance: value.residue_chance,
            burnout: value.burnout,
        }
    }
}

#[derive(Clone)]
struct RawMaterial {
    name: String,
    phase: RawPhase,
    density: f32,
    colors: Vec<[u8; 4]>,
    surface_grip: f32,
    hardness: f32,
    restitution: f32,
    surface_bounce: f32,
    contact_damage: f32,
    tags: Tags,
    burn: Option<RawBurn>,
    ember: Option<RawEmber>,
}

impl RawMaterial {
    fn defaults(name: String) -> Self {
        Self {
            name,
            phase: RawPhase::Empty,
            density: 0.0,
            colors: Vec::new(),
            surface_grip: 1.0,
            hardness: 0.0,
            restitution: 0.0,
            surface_bounce: 0.0,
            contact_damage: 0.0,
            tags: Tags::EMPTY,
            burn: None,
            ember: None,
        }
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
    pub is_fuel_ember: bool,
    pub hardness: f32,
    pub restitution: f32,
    pub surface_grip: f32,
    pub surface_bounce: f32,
    pub contact_damage: f32,
    pub ember: Option<Ember>,
    pub decay: Option<(u64, MaterialId)>,
    pub reactive: bool,
    pub dynamics: Dynamics,
}

pub struct Content {
    pub materials: Vec<Mat>,
    pub ignitions: Vec<Option<Ignition>>,
    pub reactions: Vec<Option<Reaction>>,
    pub item_source: Vec<Option<MaterialId>>,
}

pub fn build(catalog: &Catalog) -> Result<Content, Error> {
    let mut raws = build_materials(catalog)?;

    match raws.first() {
        Some(first) if matches!(first.phase, RawPhase::Empty) => {}
        Some(first) => {
            return Err(fail(format!(
                "material 0 must be air with phase Empty, got {}",
                first.name
            )));
        }
        None => return Err(fail("no materials defined")),
    }

    let hand_len = raws.len();
    let mut ignitions: Vec<Option<Ignition>> = vec![None; hand_len];
    for index in 0..hand_len {
        let Some(burn) = raws[index].burn.clone() else {
            continue;
        };
        let base = raws[index].clone();
        let chance = per_tick_chance(burn.ignite);
        let smoulder = burn.smoulder.clamp(0.0, 1.0);
        ignitions[index] = Some(Ignition {
            into: MaterialId(raws.len() as u16),
            open: chance_threshold(chance),
            sealed: chance_threshold(chance * smoulder),
        });
        raws.push(RawMaterial {
            name: format!("burning_{}", base.name),
            colors: if burn.colors.is_empty() {
                catalog.ember_colors.clone()
            } else {
                burn.colors.clone()
            },
            contact_damage: burn.damage.max(base.contact_damage),
            tags: base.tags.union(Tags::new(&[Tag::Hot, Tag::Emissive])),
            ember: Some(RawEmber {
                rate: burn.rate,
                emit: burn.emit,
                residue: burn.residue.clone(),
                residue_chance: burn.residue_chance,
                burnout: burn.burnout.clone(),
            }),
            burn: None,
            ..base
        });
    }

    let len = raws.len();
    if len > u16::MAX as usize {
        return Err(fail(format!("too many materials: {len}")));
    }
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
        let ember = match &raw.ember {
            Some(raw_ember) => {
                let residue = match (&raw_ember.residue, raw_ember.residue_chance) {
                    (Some(_), chance) if chance > 0.0 => {
                        let id = resolve(&raw_ember.residue, &raw.name)?.unwrap_or(MaterialId::AIR);
                        Some((chance_threshold(chance.clamp(0.0, 1.0)), id))
                    }
                    _ => None,
                };
                Some(Ember {
                    burn: chance_threshold(per_tick_chance(raw_ember.rate)),
                    emit: chance_threshold(per_tick_chance(raw_ember.emit)),
                    residue,
                    burnout: resolve(&raw_ember.burnout, &raw.name)?.unwrap_or(MaterialId::AIR),
                    kind: if index < hand_len {
                        EmberKind::Flame
                    } else {
                        EmberKind::Fuel
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
        materials.push(Mat {
            spec_name: camel_case(&const_name),
            name: raw.name.to_ascii_lowercase(),
            const_name,
            phase: raw.phase.tag(),
            density_milli: milli(raw.density),
            colors: raw.colors.clone(),
            tags: raw.tags,
            rigid_capable: matches!(
                raw.phase,
                RawPhase::Solid {
                    rigid_capable: true
                }
            ),
            is_fuel_ember: raw.ember.is_some() && index >= hand_len,
            hardness: raw.hardness,
            restitution: raw.restitution,
            surface_grip: raw.surface_grip,
            surface_bounce: raw.surface_bounce,
            contact_damage: raw.contact_damage,
            ember,
            decay,
            reactive,
            dynamics: quantize_dynamics(raw),
        });
    }

    let mut ember_base = vec![None; len];
    for (base, ignition) in ignitions.iter().enumerate() {
        if let Some(ignition) = ignition {
            ember_base[ignition.into.0 as usize] = Some(MaterialId(base as u16));
        }
    }
    let item_source = materials
        .iter()
        .enumerate()
        .map(|(index, mat)| {
            if mat.is_fuel_ember {
                ember_base[index]
            } else if matches!(mat.phase, Phase::Empty) || mat.tags.contains(Tag::Player) {
                None
            } else {
                Some(MaterialId(index as u16))
            }
        })
        .collect();

    Ok(Content {
        materials,
        ignitions,
        reactions,
        item_source,
    })
}

fn build_materials(catalog: &Catalog) -> Result<Vec<RawMaterial>, Error> {
    let mut raws = Vec::with_capacity(catalog.materials.len());
    for (key, definition) in &catalog.materials {
        validate_key(key)?;
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
        raw.phase = phase.into();
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
    if let Some(value) = &definition.burn {
        raw.burn = Some(value.clone().into());
    }
    if let Some(value) = &definition.ember {
        raw.ember = Some(value.clone().into());
    }
}

fn validate_key(key: &MaterialKey) -> Result<(), Error> {
    let name = key.as_str();
    if name.is_empty()
        || !name.as_bytes()[0].is_ascii_uppercase()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(fail(format!(
            "material key `{key}` must be an UPPER_SNAKE_CASE Rust identifier"
        )));
    }
    Ok(())
}

fn validate_material(raw: &RawMaterial) -> Result<(), Error> {
    let context = format!("material {}", raw.name);
    if raw.ember.is_some() && raw.burn.is_some() {
        return Err(fail(format!(
            "{context}: an ember cannot author a burning variant"
        )));
    }
    if !matches!(raw.phase, RawPhase::Empty | RawPhase::Solid { .. }) && raw.density <= 0.0 {
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
        RawPhase::Empty | RawPhase::Solid { .. } => {}
        RawPhase::Powder {
            drag,
            friction,
            repose,
            redirect_keep,
            cohesion,
        } => validate_numbers(
            &context,
            &[
                ("drag", drag),
                ("friction", friction),
                ("repose", repose),
                ("redirect_keep", redirect_keep),
                ("cohesion", cohesion),
            ],
        )?,
        RawPhase::Liquid {
            drag,
            friction,
            redirect_keep,
            cohesion,
            flow_rate,
        } => validate_numbers(
            &context,
            &[
                ("drag", drag),
                ("friction", friction),
                ("redirect_keep", redirect_keep),
                ("cohesion", cohesion),
                ("flow_rate", flow_rate),
            ],
        )?,
        RawPhase::Gas {
            drag,
            cohesion,
            turbulence,
            redirect_keep,
        } => validate_numbers(
            &context,
            &[
                ("drag", drag),
                ("cohesion", cohesion),
                ("turbulence", turbulence),
                ("redirect_keep", redirect_keep),
            ],
        )?,
    }
    if let Some(burn) = &raw.burn {
        if burn.ignite <= 0.0 {
            return Err(fail(format!("{context}: burning ignite must be > 0")));
        }
        validate_numbers(
            &context,
            &[
                ("burning ignite", burn.ignite),
                ("burning smoulder", burn.smoulder),
                ("burning rate", burn.rate),
                ("burning emit", burn.emit),
                ("burning residue_chance", burn.residue_chance),
                ("burning damage", burn.damage),
            ],
        )?;
    }
    if let Some(ember) = &raw.ember {
        validate_numbers(
            &context,
            &[
                ("ember rate", ember.rate),
                ("ember emit", ember.emit),
                ("ember residue_chance", ember.residue_chance),
            ],
        )?;
    }
    Ok(())
}

fn validate_numbers(context: &str, values: &[(&str, f32)]) -> Result<(), Error> {
    for &(field, value) in values {
        validate_number(&format!("{context}: {field}"), value)?;
    }
    Ok(())
}

fn validate_number(context: &str, value: f32) -> Result<(), Error> {
    if !value.is_finite() || value < 0.0 {
        return Err(fail(format!(
            "{context} must be a finite non-negative number"
        )));
    }
    Ok(())
}

fn drag_keeps(drag: f32) -> (u32, u32) {
    let drag_loss = 1.0 - per_tick_keep(drag);
    (
        q16(1.0 - drag_loss.min(0.9)),
        q16(1.0 - (drag_loss * 6.0).min(0.9)),
    )
}

fn quantize_dynamics(raw: &RawMaterial) -> Dynamics {
    let restitution_q16 = q16(raw.restitution.clamp(0.0, 1.0));
    match raw.phase {
        RawPhase::Empty | RawPhase::Solid { .. } => Dynamics::None,
        RawPhase::Powder {
            drag,
            friction,
            repose,
            redirect_keep,
            cohesion,
        } => {
            let (drag_keep_q16, drag_keep_submerged_q16) = drag_keeps(drag);
            Dynamics::Powder(PowderDynamics {
                drag_keep_q16,
                drag_keep_submerged_q16,
                friction_keep_q16: q16(per_tick_keep(friction)),
                cohesion_q16: q16(per_tick_chance(cohesion)),
                restitution_q16,
                redirect_keep_q16: q16(redirect_keep.clamp(0.0, 1.0)),
                slide_threshold: chance_threshold(per_tick_chance(repose)),
            })
        }
        RawPhase::Liquid {
            drag,
            friction,
            redirect_keep,
            cohesion,
            flow_rate,
        } => {
            let (drag_keep_q16, drag_keep_submerged_q16) = drag_keeps(drag);
            Dynamics::Liquid(LiquidDynamics {
                drag_keep_q16,
                drag_keep_submerged_q16,
                friction_keep_q16: q16(per_tick_keep(friction)),
                cohesion_q16: q16(per_tick_chance(cohesion)),
                restitution_q16,
                redirect_keep_q16: q16(redirect_keep.clamp(0.0, 1.0)),
                flow_threshold: if flow_rate > 0.0 {
                    chance_threshold(per_tick_chance(flow_rate))
                } else {
                    u64::MAX
                },
            })
        }
        RawPhase::Gas {
            drag,
            cohesion,
            turbulence,
            redirect_keep,
        } => Dynamics::Gas(GasDynamics {
            drag_keep_q16: drag_keeps(drag).0,
            cohesion_q16: q16(per_tick_chance(cohesion)),
            restitution_q16,
            redirect_keep_q16: q16(redirect_keep.clamp(0.0, 1.0)),
            turbulence_q16: {
                let dt = 1.0f32 / fallingsand_material::TICK_RATE as f32;
                q16(turbulence * dt.sqrt() * dt * fallingsand_material::VEL_ONE as f32)
            },
        }),
    }
}

enum Operand {
    Material(MaterialId),
    Tag(Tag),
}

#[derive(Clone, Copy)]
enum Product {
    Fixed(MaterialId),
    Same,
}

impl Product {
    fn resolve(self, operand_id: MaterialId) -> MaterialId {
        match self {
            Self::Fixed(id) => id,
            Self::Same => operand_id,
        }
    }
}

fn expand_reactions(
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
                    .map(Product::Fixed)
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

fn fail(message: impl Into<String>) -> Error {
    Error::new(message)
}
