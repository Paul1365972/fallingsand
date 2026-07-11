use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialId(pub u16);

impl MaterialId {
    pub const AIR: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Empty,
    Solid,
    Powder,
    Liquid,
    Gas,
    Fire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
    Dissolvable,
    Hot,
    Emissive,
    Player,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tags(u32);

impl Tags {
    pub const EMPTY: Self = Self(0);

    pub const fn new(tags: &[Tag]) -> Self {
        let mut bits = 0u32;
        let mut i = 0;
        while i < tags.len() {
            bits |= 1u32 << tags[i] as u32;
            i += 1;
        }
        Self(bits)
    }

    #[inline]
    pub const fn contains(self, tag: Tag) -> bool {
        self.0 & (1u32 << tag as u32) != 0
    }

    #[inline]
    pub const fn intersects(self, other: Tags) -> bool {
        self.0 & other.0 != 0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub name: &'static str,
    pub phase: Phase,
    pub density: f32,
    pub colors: &'static [[u8; 4]],
    pub rigid_capable: bool,
    pub drag: f32,
    pub friction: f32,
    pub repose: f32,
    pub redirect_keep: f32,
    pub surface_grip: f32,
    pub cohesion: f32,
    pub turbulence: f32,
    pub flow_rate: f32,
    pub hardness: f32,
    pub restitution: f32,
    pub surface_bounce: f32,
    pub contact_damage: f32,
    pub tags: Tags,
    pub decay_rate: f32,
    pub decay_into: Option<MaterialId>,
    pub flammability: f32,
    pub burn_rate: f32,
    pub burn_emit: f32,
    pub smoulder: f32,
    pub residue_into: Option<MaterialId>,
    pub residue_chance: f32,
    pub burn_damage: f32,
}

impl Material {
    pub const DEFAULT: Self = Self {
        name: "",
        phase: Phase::Empty,
        density: 0.0,
        colors: &[],
        rigid_capable: false,
        drag: 0.0,
        friction: 0.0,
        repose: 0.0,
        redirect_keep: 1.0,
        surface_grip: 1.0,
        cohesion: 0.0,
        turbulence: 0.0,
        flow_rate: 0.0,
        hardness: 0.0,
        restitution: 0.0,
        surface_bounce: 0.0,
        contact_damage: 0.0,
        tags: Tags::EMPTY,
        decay_rate: 0.0,
        decay_into: None,
        flammability: 0.0,
        burn_rate: 0.0,
        burn_emit: 0.0,
        smoulder: 0.0,
        residue_into: None,
        residue_chance: 0.0,
        burn_damage: 0.0,
    };
}

#[derive(Debug, Clone, Copy)]
pub enum Operand {
    Material(MaterialId),
    Tag(Tag),
}

#[derive(Debug, Clone, Copy)]
pub struct ReactionDef {
    pub a: Operand,
    pub b: Operand,
    pub a_becomes: MaterialId,
    pub b_becomes: MaterialId,
    pub rate: f32,
}

pub fn per_tick_chance(rate: f32) -> f32 {
    1.0 - (-rate * crate::TICK_DT).exp()
}

pub(crate) fn per_tick_keep(rate: f32) -> f32 {
    (-rate * crate::TICK_DT).exp()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reaction {
    pub becomes: MaterialId,
    pub other_becomes: MaterialId,
    pub chance: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Burn {
    pub ignite_chance: f32,
    pub burn_chance: f32,
    pub emit_chance: f32,
    pub smoulder: f32,
    pub residue: Option<(f32, MaterialId)>,
    pub damage: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Dynamics {
    pub drag_keep: f32,
    pub friction_keep: f32,
    pub cohesion: f32,
    pub restitution: f32,
    pub turbulence: f32,
    pub slide_chance: f32,
    pub redirect_keep: f32,
    pub flow_chance: f32,
}

#[derive(Debug, Clone)]
pub struct MaterialRegistry {
    materials: Vec<Material>,
    reactions: Vec<Option<Reaction>>,
    decays: Vec<Option<(f32, MaterialId)>>,
    burns: Vec<Option<Burn>>,
    reactive: Vec<bool>,
    dynamics: Vec<Dynamics>,
}

impl MaterialRegistry {
    pub fn from_materials(materials: &[Material], reaction_defs: &[ReactionDef]) -> Self {
        let materials = materials.to_vec();
        assert!(
            materials.len() <= u16::MAX as usize,
            "too many materials: {}",
            materials.len()
        );
        match materials.first() {
            Some(first) if first.phase == Phase::Empty => {}
            first => panic!(
                "material 0 must be air with phase Empty, got {:?}",
                first.map(|m| m.name)
            ),
        }
        let len = materials.len();

        for material in materials.iter() {
            assert!(
                !material.colors.is_empty(),
                "material {:?} has no colors",
                material.name
            );
        }

        let decays: Vec<Option<(f32, MaterialId)>> = materials
            .iter()
            .map(|material| {
                (material.decay_rate > 0.0).then(|| {
                    (
                        per_tick_chance(material.decay_rate),
                        material.decay_into.unwrap_or(MaterialId::AIR),
                    )
                })
            })
            .collect();

        let burns: Vec<Option<Burn>> = materials
            .iter()
            .map(|material| {
                (material.flammability > 0.0).then(|| Burn {
                    ignite_chance: per_tick_chance(material.flammability),
                    burn_chance: per_tick_chance(material.burn_rate),
                    emit_chance: per_tick_chance(material.burn_emit),
                    smoulder: material.smoulder.clamp(0.0, 1.0),
                    residue: match (material.residue_into, material.residue_chance) {
                        (Some(id), chance) if chance > 0.0 => Some((chance.clamp(0.0, 1.0), id)),
                        _ => None,
                    },
                    damage: material.burn_damage,
                })
            })
            .collect();

        let expand = |operand: Operand| -> Vec<MaterialId> {
            match operand {
                Operand::Material(id) => vec![id],
                Operand::Tag(tag) => (0..len)
                    .filter(|&index| materials[index].tags.contains(tag))
                    .map(|index| MaterialId(index as u16))
                    .collect(),
            }
        };

        let mut table: Vec<Option<(Reaction, u8)>> = vec![None; len * len];
        for def in reaction_defs {
            let becomes_a = def.a_becomes;
            let becomes_b = def.b_becomes;
            let specificity = matches!(def.a, Operand::Material(_)) as u8
                + matches!(def.b, Operand::Material(_)) as u8;
            for a in expand(def.a) {
                for b in expand(def.b) {
                    let entries = if a == b {
                        vec![(a, b, becomes_a, becomes_b)]
                    } else {
                        vec![(a, b, becomes_a, becomes_b), (b, a, becomes_b, becomes_a)]
                    };
                    for (from, other, becomes, other_becomes) in entries {
                        let slot = &mut table[from.0 as usize * len + other.0 as usize];
                        match slot {
                            Some((_, existing)) if *existing == specificity => {
                                panic!(
                                    "ambiguous reactions for pair {:?} + {:?}",
                                    materials[from.0 as usize].name,
                                    materials[other.0 as usize].name
                                );
                            }
                            Some((_, existing)) if *existing > specificity => {}
                            _ => {
                                *slot = Some((
                                    Reaction {
                                        becomes,
                                        other_becomes,
                                        chance: per_tick_chance(def.rate),
                                    },
                                    specificity,
                                ));
                            }
                        }
                    }
                }
            }
        }
        let reactions: Vec<Option<Reaction>> = table
            .into_iter()
            .map(|slot| slot.map(|(reaction, _)| reaction))
            .collect();

        let reactive: Vec<bool> = (0..len)
            .map(|index| {
                materials[index].phase == Phase::Fire
                    || decays[index].is_some()
                    || reactions[index * len..(index + 1) * len]
                        .iter()
                        .any(|slot| slot.is_some())
            })
            .collect();

        let dynamics: Vec<Dynamics> = materials
            .iter()
            .map(|material| Dynamics {
                drag_keep: per_tick_keep(material.drag),
                friction_keep: per_tick_keep(material.friction),
                cohesion: per_tick_chance(material.cohesion),
                restitution: material.restitution.clamp(0.0, 1.0),
                turbulence: material.turbulence
                    * crate::TICK_DT.sqrt()
                    * crate::TICK_DT
                    * crate::VEL_ONE as f32,
                slide_chance: per_tick_chance(material.repose),
                redirect_keep: material.redirect_keep.clamp(0.0, 1.0),
                flow_chance: if material.flow_rate > 0.0 {
                    per_tick_chance(material.flow_rate)
                } else {
                    1.0
                },
            })
            .collect();

        Self {
            materials,
            reactions,
            decays,
            burns,
            reactive,
            dynamics,
        }
    }

    #[inline]
    pub fn get(&self, id: MaterialId) -> &Material {
        &self.materials[id.0 as usize]
    }

    #[inline]
    pub fn try_get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.0 as usize)
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (MaterialId, &Material)> {
        self.materials
            .iter()
            .enumerate()
            .map(|(i, m)| (MaterialId(i as u16), m))
    }

    #[inline]
    pub fn reaction(&self, a: MaterialId, b: MaterialId) -> Option<Reaction> {
        self.reactions[a.0 as usize * self.materials.len() + b.0 as usize]
    }

    #[inline]
    pub fn decay(&self, id: MaterialId) -> Option<(f32, MaterialId)> {
        self.decays[id.0 as usize]
    }

    #[inline]
    pub fn burn(&self, id: MaterialId) -> Option<Burn> {
        self.burns[id.0 as usize]
    }

    #[inline]
    pub fn is_flammable(&self, id: MaterialId) -> bool {
        self.burns[id.0 as usize].is_some()
    }

    #[inline]
    pub fn is_reactive(&self, id: MaterialId) -> bool {
        self.reactive[id.0 as usize]
    }

    #[inline]
    pub fn dynamics(&self, id: MaterialId) -> Dynamics {
        self.dynamics[id.0 as usize]
    }

    #[inline]
    pub fn has_tag(&self, id: MaterialId, tag: Tag) -> bool {
        self.materials[id.0 as usize].tags.contains(tag)
    }
}
