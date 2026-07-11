use crate::dsl::{self, Header, MaterialAst, OperandAst, Sources};
use fallingsand_material::{
    Dynamics, Ember, GasDynamics, Ignition, LiquidDynamics, MaterialId, Phase, PowderDynamics,
    Reaction, Tag, Tags, chance_threshold, milli, per_tick_chance, per_tick_keep, q16,
};
use std::collections::HashMap;
use syn::Expr;

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
            RawPhase::Empty => Phase::Empty,
            RawPhase::Solid { .. } => Phase::Solid,
            RawPhase::Powder { .. } => Phase::Powder,
            RawPhase::Liquid { .. } => Phase::Liquid,
            RawPhase::Gas { .. } => Phase::Gas,
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
    ember: bool,
    flammability: f32,
    burn_rate: f32,
    burn_emit: f32,
    burn_colors: Vec<[u8; 4]>,
    smoulder: f32,
    residue_into: Option<String>,
    residue_chance: f32,
    burn_damage: f32,
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
            ember: false,
            flammability: 0.0,
            burn_rate: 0.0,
            burn_emit: 0.0,
            burn_colors: Vec::new(),
            smoulder: 0.0,
            residue_into: None,
            residue_chance: 0.0,
            burn_damage: 0.0,
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
    pub is_flammable: bool,
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
}

pub fn build(header: &Header, sources: &Sources) -> syn::Result<Content> {
    let mut raws = parse_materials(sources)?;

    match raws.first() {
        Some(first) if matches!(first.phase, RawPhase::Empty) => {}
        Some(first) => {
            return Err(dsl::fail(format!(
                "material 0 must be air with phase Empty, got {}",
                first.name
            )));
        }
        None => return Err(dsl::fail("no materials defined")),
    }

    let hand_len = raws.len();
    let mut ignitions: Vec<Option<Ignition>> = vec![None; hand_len];
    for index in 0..hand_len {
        if raws[index].ember || raws[index].flammability <= 0.0 {
            continue;
        }
        let base = raws[index].clone();
        let chance = per_tick_chance(base.flammability);
        let smoulder = base.smoulder.clamp(0.0, 1.0);
        ignitions[index] = Some(Ignition {
            into: MaterialId(raws.len() as u16),
            open: chance_threshold(chance),
            sealed: chance_threshold(chance * smoulder),
        });
        raws.push(RawMaterial {
            name: format!("burning_{}", base.name),
            colors: if base.burn_colors.is_empty() {
                header.ember_colors.clone()
            } else {
                base.burn_colors.clone()
            },
            contact_damage: base.burn_damage.max(base.contact_damage),
            tags: base.tags.union(Tags::new(&[Tag::Hot, Tag::Emissive])),
            ember: true,
            flammability: 0.0,
            smoulder: 0.0,
            burn_colors: Vec::new(),
            ..base
        });
    }
    let len = raws.len();
    if len > u16::MAX as usize {
        return Err(dsl::fail(format!("too many materials: {len}")));
    }
    ignitions.resize(len, None);

    let mut ember_bases: Vec<Option<MaterialId>> = vec![None; len];
    for (base_index, ignition) in ignitions.iter().take(hand_len).enumerate() {
        if let Some(ignition) = ignition {
            ember_bases[ignition.into.0 as usize] = Some(MaterialId(base_index as u16));
        }
    }

    for raw in &raws {
        if raw.colors.is_empty() {
            return Err(dsl::fail(format!("material {} has no colors", raw.name)));
        }
    }

    let mut by_name: HashMap<String, MaterialId> = HashMap::new();
    for (index, raw) in raws.iter().enumerate() {
        let const_name = raw.name.to_ascii_uppercase();
        if by_name
            .insert(const_name, MaterialId(index as u16))
            .is_some()
        {
            return Err(dsl::fail(format!("duplicate material name {}", raw.name)));
        }
    }
    let resolve = |handle: &Option<String>, owner: &str| -> syn::Result<Option<MaterialId>> {
        match handle {
            None => Ok(None),
            Some(name) => by_name
                .get(name.as_str())
                .copied()
                .map(Some)
                .ok_or_else(|| dsl::fail(format!("material {owner}: unknown material `{name}`"))),
        }
    };

    let reactions = expand_reactions(sources, &raws, &by_name)?;

    let mut decays: Vec<Option<(u64, MaterialId)>> = vec![None; len];
    for def in &sources.decays {
        let from = def.from.to_string();
        let Some(from) = by_name.get(from.as_str()) else {
            return Err(dsl::fail(format!("reactions: unknown material `{from}`")));
        };
        let into = def.into.to_string();
        let Some(into) = by_name.get(into.as_str()) else {
            return Err(dsl::fail(format!("reactions: unknown material `{into}`")));
        };
        let slot = &mut decays[from.0 as usize];
        if slot.is_some() {
            return Err(dsl::fail(format!(
                "duplicate decay for {}",
                raws[from.0 as usize].name
            )));
        }
        *slot = Some((chance_threshold(per_tick_chance(def.rate)), *into));
    }

    let mut materials = Vec::with_capacity(len);
    for (index, raw) in raws.iter().enumerate() {
        let decay = decays[index];

        let ember = if raw.ember {
            let residue = match (&raw.residue_into, raw.residue_chance) {
                (Some(_), chance) if chance > 0.0 => {
                    let id = resolve(&raw.residue_into, &raw.name)?.unwrap_or(MaterialId::AIR);
                    Some((chance_threshold(chance.clamp(0.0, 1.0)), id))
                }
                _ => None,
            };
            Some(Ember {
                burn: chance_threshold(per_tick_chance(raw.burn_rate)),
                emit: chance_threshold(per_tick_chance(raw.burn_emit)),
                residue,
                base: ember_bases[index],
            })
        } else {
            None
        };

        let reactive = decay.is_some()
            || reactions[index * len..(index + 1) * len]
                .iter()
                .any(Option::is_some);

        let const_name = raw.name.to_ascii_uppercase();
        materials.push(Mat {
            spec_name: camel_case(&const_name),
            const_name,
            name: raw.name.clone(),
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
            is_flammable: ignitions[index].is_some(),
            is_fuel_ember: raw.ember && ember_bases[index].is_some(),
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

    Ok(Content {
        materials,
        ignitions,
        reactions,
    })
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

fn parse_materials(sources: &Sources) -> syn::Result<Vec<RawMaterial>> {
    let mut raws: Vec<RawMaterial> = Vec::new();
    for (file, defs) in &sources.materials {
        for def in defs {
            let raw = parse_material(file, def, &raws)?;
            raws.push(raw);
        }
    }
    Ok(raws)
}

fn parse_phase(file: &str, name: &str, value: &Expr) -> syn::Result<RawPhase> {
    let context = format!("material {name}: field `phase`");
    let (phase_name, fields) = dsl::expr_phase(value, file, &context)?;
    let mut phase = match phase_name.as_str() {
        "Empty" => RawPhase::Empty,
        "Solid" => RawPhase::Solid {
            rigid_capable: false,
        },
        "Powder" => RawPhase::Powder {
            drag: 0.0,
            friction: 0.0,
            repose: 0.0,
            redirect_keep: 1.0,
            cohesion: 0.0,
        },
        "Liquid" => RawPhase::Liquid {
            drag: 0.0,
            friction: 0.0,
            redirect_keep: 1.0,
            cohesion: 0.0,
            flow_rate: 0.0,
        },
        "Gas" => RawPhase::Gas {
            drag: 0.0,
            cohesion: 0.0,
            turbulence: 0.0,
            redirect_keep: 1.0,
        },
        other => {
            return Err(dsl::fail(format!(
                "{file}: {context}: unknown phase `{other}`"
            )));
        }
    };
    for (field, value) in &fields {
        let context = format!("material {name}: {phase_name} field `{field}`");
        let number = || dsl::expr_f32(value, file, &context);
        match (&mut phase, field.to_string().as_str()) {
            (RawPhase::Solid { rigid_capable }, "rigid_capable") => {
                *rigid_capable = dsl::expr_bool(value, file, &context)?;
            }
            (
                RawPhase::Powder { drag, .. }
                | RawPhase::Liquid { drag, .. }
                | RawPhase::Gas { drag, .. },
                "drag",
            ) => *drag = number()?,
            (RawPhase::Powder { friction, .. } | RawPhase::Liquid { friction, .. }, "friction") => {
                *friction = number()?
            }
            (RawPhase::Powder { repose, .. }, "repose") => *repose = number()?,
            (
                RawPhase::Powder { redirect_keep, .. }
                | RawPhase::Liquid { redirect_keep, .. }
                | RawPhase::Gas { redirect_keep, .. },
                "redirect_keep",
            ) => *redirect_keep = number()?,
            (
                RawPhase::Powder { cohesion, .. }
                | RawPhase::Liquid { cohesion, .. }
                | RawPhase::Gas { cohesion, .. },
                "cohesion",
            ) => *cohesion = number()?,
            (RawPhase::Liquid { flow_rate, .. }, "flow_rate") => *flow_rate = number()?,
            (RawPhase::Gas { turbulence, .. }, "turbulence") => *turbulence = number()?,
            (_, other) => {
                return Err(dsl::fail(format!(
                    "{file}: material {name}: {phase_name} has no field `{other}`"
                )));
            }
        }
    }
    Ok(phase)
}

fn parse_material(file: &str, ast: &MaterialAst, done: &[RawMaterial]) -> syn::Result<RawMaterial> {
    let name = ast.name.to_string();
    let mut raw = match &ast.base {
        Some(base) => {
            let base_name = base.to_string();
            let base = done.iter().find(|raw| raw.name == base_name).ok_or_else(|| {
                dsl::fail(format!(
                    "{file}: material {name}: unknown base `{base_name}` (bases must be defined earlier)"
                ))
            })?;
            RawMaterial {
                name: name.clone(),
                ..base.clone()
            }
        }
        None => RawMaterial::defaults(name.clone()),
    };
    for (field, value) in &ast.fields {
        let context = format!("material {name}: field `{field}`");
        match field.to_string().as_str() {
            "phase" => raw.phase = parse_phase(file, &name, value)?,
            "density" => raw.density = dsl::expr_f32(value, file, &context)?,
            "colors" => raw.colors = dsl::expr_colors(value, file, &context)?,
            "surface_grip" => raw.surface_grip = dsl::expr_f32(value, file, &context)?,
            "hardness" => raw.hardness = dsl::expr_f32(value, file, &context)?,
            "restitution" => raw.restitution = dsl::expr_f32(value, file, &context)?,
            "surface_bounce" => raw.surface_bounce = dsl::expr_f32(value, file, &context)?,
            "contact_damage" => raw.contact_damage = dsl::expr_f32(value, file, &context)?,
            "tags" => {
                let mut tags = Tags::EMPTY;
                for tag in dsl::expr_tags(value, file, &context)? {
                    let tag = Tag::parse(&tag).ok_or_else(|| {
                        dsl::fail(format!("{file}: {context}: unknown tag `{tag}`"))
                    })?;
                    tags = tags.union(Tags::new(&[tag]));
                }
                raw.tags = tags;
            }
            "ember" => raw.ember = dsl::expr_bool(value, file, &context)?,
            "flammability" => raw.flammability = dsl::expr_f32(value, file, &context)?,
            "burn_rate" => raw.burn_rate = dsl::expr_f32(value, file, &context)?,
            "burn_emit" => raw.burn_emit = dsl::expr_f32(value, file, &context)?,
            "burn_colors" => raw.burn_colors = dsl::expr_colors(value, file, &context)?,
            "smoulder" => raw.smoulder = dsl::expr_f32(value, file, &context)?,
            "residue_into" => raw.residue_into = Some(dsl::expr_handle(value, file, &context)?),
            "residue_chance" => raw.residue_chance = dsl::expr_f32(value, file, &context)?,
            "burn_damage" => raw.burn_damage = dsl::expr_f32(value, file, &context)?,
            other @ ("drag" | "friction" | "repose" | "redirect_keep" | "cohesion"
            | "turbulence" | "flow_rate" | "rigid_capable") => {
                return Err(dsl::fail(format!(
                    "{file}: material {name}: `{other}` belongs in the phase block"
                )));
            }
            other @ ("decay_rate" | "decay_into") => {
                return Err(dsl::fail(format!(
                    "{file}: material {name}: `{other}` is gone; declare decay in reactions.rs (`STEAM => WATER @ 0.1;`)"
                )));
            }
            other => {
                return Err(dsl::fail(format!(
                    "{file}: material {name}: unknown field `{other}`"
                )));
            }
        }
    }
    validate(file, &raw, ast)?;
    Ok(raw)
}

fn validate(file: &str, raw: &RawMaterial, ast: &MaterialAst) -> syn::Result<()> {
    let name = &raw.name;
    let fuel = raw.flammability > 0.0;
    if raw.ember && fuel {
        return Err(dsl::fail(format!(
            "{file}: material {name}: an ember cannot be flammable (set `flammability: 0.0`)"
        )));
    }
    for (field, _) in &ast.fields {
        let complaint = match field.to_string().as_str() {
            "smoulder" | "burn_colors" | "burn_damage" if !fuel => "needs flammability",
            "burn_rate" | "burn_emit" | "residue_into" | "residue_chance"
                if !(fuel || raw.ember) =>
            {
                "needs flammability or ember"
            }
            _ => continue,
        };
        return Err(dsl::fail(format!(
            "{file}: material {name}: `{field}` {complaint}"
        )));
    }
    if !matches!(raw.phase, RawPhase::Empty | RawPhase::Solid { .. }) && raw.density <= 0.0 {
        return Err(dsl::fail(format!(
            "{file}: material {name}: moving phases need density > 0"
        )));
    }
    Ok(())
}

enum Operand {
    Material(MaterialId),
    Tag(Tag),
}

fn expand_reactions(
    sources: &Sources,
    raws: &[RawMaterial],
    by_name: &HashMap<String, MaterialId>,
) -> syn::Result<Vec<Option<Reaction>>> {
    let len = raws.len();
    let resolve_operand = |ast: &OperandAst| -> syn::Result<Operand> {
        match ast {
            OperandAst::Material(ident) => {
                let name = ident.to_string();
                by_name
                    .get(name.as_str())
                    .copied()
                    .map(Operand::Material)
                    .ok_or_else(|| dsl::fail(format!("reactions: unknown material `{name}`")))
            }
            OperandAst::Tag(ident) => {
                let name = ident.to_string();
                Tag::parse(&name)
                    .map(Operand::Tag)
                    .ok_or_else(|| dsl::fail(format!("reactions: unknown tag `{name}`")))
            }
        }
    };
    let resolve_product = |ident: &syn::Ident| -> syn::Result<MaterialId> {
        let name = ident.to_string();
        by_name
            .get(name.as_str())
            .copied()
            .ok_or_else(|| dsl::fail(format!("reactions: unknown material `{name}`")))
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
    for def in &sources.reactions {
        let a = resolve_operand(&def.a)?;
        let b = resolve_operand(&def.b)?;
        let becomes_a = resolve_product(&def.a_becomes)?;
        let becomes_b = resolve_product(&def.b_becomes)?;
        let threshold = chance_threshold(per_tick_chance(def.rate));
        let specificity =
            matches!(a, Operand::Material(_)) as u8 + matches!(b, Operand::Material(_)) as u8;
        for a_id in expand(&a) {
            for b_id in expand(&b) {
                let entries = if a_id == b_id {
                    vec![(a_id, b_id, becomes_a, becomes_b)]
                } else {
                    vec![
                        (a_id, b_id, becomes_a, becomes_b),
                        (b_id, a_id, becomes_b, becomes_a),
                    ]
                };
                for (from, other, becomes, other_becomes) in entries {
                    let slot = &mut table[from.0 as usize * len + other.0 as usize];
                    match slot {
                        Some((_, existing)) if *existing == specificity => {
                            return Err(dsl::fail(format!(
                                "ambiguous reactions for pair {} + {}",
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
