use crate::{
    BOND_GROUP_COUNT, BurningDef, Catalog, EmissionDef, Error, FlammableDef, GasDef, IngredientDef,
    LiquidDef, MaterialDef, MaterialKey, OperandDef, PhaseDef, PowderDef, ProductDef, SolidDef,
};
use fallingsand_material::{
    Burning, BurningKind, Dynamics, GasDynamics, Ignition, LiquidDynamics, MaterialId, Phase,
    PowderDynamics, Reaction, SealedBurn, Tag, Tags, milli, per_tick_chance, per_tick_keep, q16,
};
use fallingsand_rng::chance_threshold;
use std::collections::HashMap;

fn phase_tag(phase: PhaseDef) -> Phase {
    match phase {
        PhaseDef::Empty => Phase::Empty,
        PhaseDef::Solid(_) => Phase::Solid,
        PhaseDef::Powder(_) => Phase::Powder,
        PhaseDef::Liquid(_) => Phase::Liquid,
        PhaseDef::Gas(_) => Phase::Gas,
    }
}

#[derive(Clone)]
struct RawMaterial {
    name: String,
    phase: PhaseDef,
    density: f32,
    colors: Vec<[u8; 4]>,
    surface_grip: f32,
    hardness: f32,
    restitution: f32,
    surface_bounce: f32,
    contact_damage: f32,
    tags: Tags,
    flammable: Option<FlammableDef>,
    burning: Option<BurningDef>,
    emission: Option<EmissionDef>,
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

const BURNING_EMISSION: EmissionDef = EmissionDef {
    color: [255, 120, 32],
    intensity: 1.4,
    flicker: 0.5,
};

fn srgb_to_linear(channel: u8) -> f32 {
    let s = channel as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn bake_emission(def: Option<EmissionDef>) -> ([f32; 3], f32) {
    match def {
        Some(emission) => (
            [
                srgb_to_linear(emission.color[0]) * emission.intensity,
                srgb_to_linear(emission.color[1]) * emission.intensity,
                srgb_to_linear(emission.color[2]) * emission.intensity,
            ],
            emission.flicker,
        ),
        None => ([0.0; 3], 0.0),
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
    pub items: Vec<ItemOut>,
    pub recipes: Vec<RecipeOut>,
    pub item_for_material: Vec<u16>,
    pub bond_masks: Vec<u32>,
}

const MATERIAL_STACK_MAX: u32 = 10_000;

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
            density: flammable.density.unwrap_or(base.density),
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
        items,
        recipes,
        item_for_material,
        bond_masks,
    })
}

fn build_items(
    catalog: &Catalog,
    materials: &[Mat],
    fuel_base: &[Option<MaterialId>],
) -> Result<(Vec<ItemOut>, Vec<u16>), Error> {
    let mut items = vec![ItemOut {
        name: "none".to_owned(),
        display: "None".to_owned(),
        stack_max: 0,
        sprite: String::new(),
        place: None,
        tool: None,
    }];

    let mut by_key: HashMap<String, u16> = HashMap::new();
    for (key, def) in &catalog.items {
        validate_ident("item key", key.as_str())?;
        let name = key.as_str().to_ascii_lowercase();
        let id = items.len() as u16;
        if by_key.insert(key.as_str().to_owned(), id).is_some() {
            return Err(fail(format!("duplicate item key `{key}`")));
        }
        items.push(ItemOut {
            display: def.display.clone(),
            stack_max: def.stack.max(1),
            sprite: name.clone(),
            place: None,
            tool: def.tool,
            name,
        });
    }

    let mut material_item = vec![0u16; materials.len()];
    for (index, mat) in materials.iter().enumerate().skip(1) {
        if mat.tags.contains(Tag::Player) {
            continue;
        }
        let id = items.len() as u16;
        items.push(ItemOut {
            name: format!("mat:{}", mat.name),
            display: pretty_name(&mat.name),
            stack_max: MATERIAL_STACK_MAX,
            sprite: format!("materials/{}", mat.name),
            place: Some(MaterialId(index as u16)),
            tool: None,
        });
        material_item[index] = id;
    }

    if items.len() > u16::MAX as usize {
        return Err(fail(format!("too many items: {}", items.len())));
    }

    let item_for_material = (0..materials.len())
        .map(|index| {
            let source = fuel_base[index].map_or(index, |base| base.0 as usize);
            material_item[source]
        })
        .collect();

    Ok((items, item_for_material))
}

fn build_recipes(
    catalog: &Catalog,
    by_name: &HashMap<String, MaterialId>,
    item_for_material: &[u16],
) -> Result<Vec<RecipeOut>, Error> {
    let by_key: HashMap<&str, u16> = catalog
        .items
        .iter()
        .enumerate()
        .map(|(index, (key, _))| (key.as_str(), index as u16 + 1))
        .collect();

    let resolve = |ingredient: &IngredientDef| -> Result<u16, Error> {
        match ingredient {
            IngredientDef::Item(key) => by_key
                .get(key.as_str())
                .copied()
                .ok_or_else(|| fail(format!("recipes: unknown item `{key}`"))),
            IngredientDef::Material(key) => {
                let mat = by_name
                    .get(key.as_str())
                    .ok_or_else(|| fail(format!("recipes: unknown material `{key}`")))?;
                let item = item_for_material[mat.0 as usize];
                if item == 0 {
                    Err(fail(format!("recipes: material `{key}` has no item form")))
                } else {
                    Ok(item)
                }
            }
        }
    };

    let mut recipes = Vec::with_capacity(catalog.recipes.len());
    for def in &catalog.recipes {
        let mut inputs = Vec::with_capacity(def.inputs.len());
        for (ingredient, count) in &def.inputs {
            inputs.push((resolve(ingredient)?, *count));
        }
        let output = (resolve(&def.output.0)?, def.output.1);
        recipes.push(RecipeOut { inputs, output });
    }
    Ok(recipes)
}

fn validate_ident(kind: &str, name: &str) -> Result<(), Error> {
    if name.is_empty()
        || !name.as_bytes()[0].is_ascii_uppercase()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(fail(format!(
            "{kind} `{name}` must be an UPPER_SNAKE_CASE Rust identifier"
        )));
    }
    Ok(())
}

fn pretty_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for (index, word) in raw.split('_').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

fn build_materials(catalog: &Catalog) -> Result<Vec<RawMaterial>, Error> {
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
            cohesion,
        }) => validate_numbers(
            &context,
            &[
                ("air_drag", air_drag),
                ("ground_friction", ground_friction),
                ("topple_start", topple_start),
                ("topple_keep", topple_keep),
                ("deflect", deflect),
                ("cohesion", cohesion),
            ],
        )?,
        PhaseDef::Liquid(LiquidDef {
            air_drag,
            ground_friction,
            deflect,
            cohesion,
            flow_rate,
        }) => validate_numbers(
            &context,
            &[
                ("air_drag", air_drag),
                ("ground_friction", ground_friction),
                ("deflect", deflect),
                ("cohesion", cohesion),
                ("flow_rate", flow_rate),
            ],
        )?,
        PhaseDef::Gas(GasDef {
            air_drag,
            cohesion,
            turbulence,
            deflect,
        }) => validate_numbers(
            &context,
            &[
                ("air_drag", air_drag),
                ("cohesion", cohesion),
                ("turbulence", turbulence),
                ("deflect", deflect),
            ],
        )?,
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

fn drag_keeps(air_drag: f32) -> (u32, u32) {
    let drag_loss = 1.0 - per_tick_keep(air_drag);
    (
        q16(1.0 - drag_loss.min(0.9)),
        q16(1.0 - (drag_loss * 6.0).min(0.9)),
    )
}

fn quantize_dynamics(raw: &RawMaterial) -> Dynamics {
    let restitution_q16 = q16(raw.restitution.clamp(0.0, 1.0));
    match raw.phase {
        PhaseDef::Empty | PhaseDef::Solid(_) => Dynamics::None,
        PhaseDef::Powder(PowderDef {
            air_drag,
            ground_friction,
            topple_start,
            topple_keep,
            deflect,
            cohesion,
        }) => {
            let (air_drag_keep_q16, submerged_drag_q16) = drag_keeps(air_drag);
            Dynamics::Powder(PowderDynamics {
                air_drag_keep_q16,
                submerged_drag_q16,
                ground_friction_keep_q16: q16(per_tick_keep(ground_friction)),
                cohesion_q16: q16(per_tick_chance(cohesion)),
                restitution_q16,
                deflect_keep_q16: q16(deflect.clamp(0.0, 1.0)),
                topple_start_threshold: chance_threshold(per_tick_chance(topple_start)),
                topple_keep_threshold: chance_threshold(per_tick_chance(topple_keep)),
            })
        }
        PhaseDef::Liquid(LiquidDef {
            air_drag,
            ground_friction,
            deflect,
            cohesion,
            flow_rate,
        }) => {
            let (air_drag_keep_q16, submerged_drag_q16) = drag_keeps(air_drag);
            Dynamics::Liquid(LiquidDynamics {
                air_drag_keep_q16,
                submerged_drag_q16,
                ground_friction_keep_q16: q16(per_tick_keep(ground_friction)),
                cohesion_q16: q16(per_tick_chance(cohesion)),
                restitution_q16,
                deflect_keep_q16: q16(deflect.clamp(0.0, 1.0)),
                flow_threshold: if flow_rate > 0.0 {
                    chance_threshold(per_tick_chance(flow_rate))
                } else {
                    u64::MAX
                },
            })
        }
        PhaseDef::Gas(GasDef {
            air_drag,
            cohesion,
            turbulence,
            deflect,
        }) => Dynamics::Gas(GasDynamics {
            air_drag_keep_q16: drag_keeps(air_drag).0,
            cohesion_q16: q16(per_tick_chance(cohesion)),
            restitution_q16,
            deflect_keep_q16: q16(deflect.clamp(0.0, 1.0)),
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
