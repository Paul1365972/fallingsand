use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialId(pub u16);

impl MaterialId {
    pub const AIR: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    Empty,
    Solid,
    Powder,
    Liquid,
    Gas,
    Fire,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub phase: Phase,
    pub density: f32,
    pub colors: Vec<[u8; 4]>,
    #[serde(default)]
    pub rigid_capable: bool,
    #[serde(default)]
    pub drag: f32,
    #[serde(default)]
    pub friction: f32,
    #[serde(default)]
    pub cohesion: f32,
    #[serde(default)]
    pub flow_rate: f32,
    #[serde(default)]
    pub hardness: f32,
    #[serde(default)]
    pub restitution: f32,
    #[serde(default)]
    pub contact_damage: f32,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub decay_rate: f32,
    #[serde(default)]
    pub decay_into: Option<String>,
    #[serde(default)]
    pub sustained_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReactionDef {
    pub a: String,
    pub b: String,
    pub a_becomes: String,
    pub b_becomes: String,
    #[serde(default = "default_rate")]
    pub rate: f32,
}

fn default_rate() -> f32 {
    f32::INFINITY
}

pub fn per_tick_chance(rate: f32) -> f32 {
    1.0 - (-rate * crate::TICK_DT).exp()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialFile {
    pub materials: Vec<Material>,
    #[serde(default)]
    pub reactions: Vec<ReactionDef>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reaction {
    pub becomes: MaterialId,
    pub other_becomes: MaterialId,
    pub chance: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Dynamics {
    pub drag_keep: f32,
    pub friction: f32,
    pub cohesion: f32,
    pub restitution: f32,
    pub flow_chance: f32,
}

#[derive(Debug, Clone)]
pub struct MaterialRegistry {
    materials: Vec<Material>,
    by_name: HashMap<String, MaterialId>,
    hash: u64,
    tag_index: HashMap<String, u32>,
    tag_bits: Vec<u32>,
    sustain_bits: Vec<u32>,
    reactions: Vec<Option<Reaction>>,
    decays: Vec<Option<(f32, MaterialId)>>,
    reactive: Vec<bool>,
    dynamics: Vec<Dynamics>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("failed to parse materials: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("material 0 must be named `air` with phase Empty, got {0:?}")]
    BadAir(String),
    #[error("duplicate material name {0:?}")]
    DuplicateName(String),
    #[error("too many materials: {0} (max {max})", max = u16::MAX)]
    TooMany(usize),
    #[error("material {0:?} has no colors")]
    NoColors(String),
    #[error("too many distinct tags: {0} (max 32)")]
    TooManyTags(usize),
    #[error("unknown material {0:?}")]
    UnknownMaterial(String),
    #[error("unknown tag {0:?}")]
    UnknownTag(String),
    #[error("ambiguous reactions for pair {0:?} + {1:?}")]
    AmbiguousReaction(String, String),
}

enum Operand {
    Name(MaterialId),
    Tag(u32),
}

impl MaterialRegistry {
    pub fn from_ron(source: &str) -> Result<Self, RegistryError> {
        let file: MaterialFile = ron::from_str(source)?;
        Self::from_materials(file.materials, file.reactions)
    }

    pub fn from_materials(
        materials: Vec<Material>,
        reaction_defs: Vec<ReactionDef>,
    ) -> Result<Self, RegistryError> {
        if materials.len() > u16::MAX as usize {
            return Err(RegistryError::TooMany(materials.len()));
        }
        match materials.first() {
            Some(first) if first.name == "air" && first.phase == Phase::Empty => {}
            first => {
                return Err(RegistryError::BadAir(
                    first.map(|m| m.name.clone()).unwrap_or_default(),
                ));
            }
        }
        let mut by_name = HashMap::new();
        for (index, material) in materials.iter().enumerate() {
            if material.colors.is_empty() {
                return Err(RegistryError::NoColors(material.name.clone()));
            }
            if by_name
                .insert(material.name.clone(), MaterialId(index as u16))
                .is_some()
            {
                return Err(RegistryError::DuplicateName(material.name.clone()));
            }
        }

        let mut tag_index: HashMap<&str, u32> = HashMap::new();
        for material in &materials {
            for tag in &material.tags {
                let next = tag_index.len() as u32;
                tag_index.entry(tag.as_str()).or_insert(next);
            }
        }
        if tag_index.len() > 32 {
            return Err(RegistryError::TooManyTags(tag_index.len()));
        }
        let tag_bits: Vec<u32> = materials
            .iter()
            .map(|material| {
                material
                    .tags
                    .iter()
                    .map(|tag| 1u32 << tag_index[tag.as_str()])
                    .fold(0, |bits, bit| bits | bit)
            })
            .collect();
        let sustain_bits: Vec<u32> = materials
            .iter()
            .map(|material| {
                material
                    .sustained_by
                    .iter()
                    .map(|tag| {
                        tag_index
                            .get(tag.as_str())
                            .map(|&index| 1u32 << index)
                            .ok_or_else(|| RegistryError::UnknownTag(tag.clone()))
                    })
                    .try_fold(0u32, |bits, bit| bit.map(|bit| bits | bit))
            })
            .collect::<Result<_, _>>()?;

        let decays: Vec<Option<(f32, MaterialId)>> = materials
            .iter()
            .map(|material| {
                if material.decay_rate <= 0.0 {
                    return Ok(None);
                }
                let product = match &material.decay_into {
                    Some(name) => *by_name
                        .get(name)
                        .ok_or_else(|| RegistryError::UnknownMaterial(name.clone()))?,
                    None => MaterialId::AIR,
                };
                Ok(Some((per_tick_chance(material.decay_rate), product)))
            })
            .collect::<Result<_, RegistryError>>()?;

        let len = materials.len();
        let resolve = |operand: &str| -> Result<Operand, RegistryError> {
            if let Some(tag) = operand.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                let &index = tag_index
                    .get(tag)
                    .ok_or_else(|| RegistryError::UnknownTag(tag.to_string()))?;
                return Ok(Operand::Tag(1u32 << index));
            }
            by_name
                .get(operand)
                .copied()
                .map(Operand::Name)
                .ok_or_else(|| RegistryError::UnknownMaterial(operand.to_string()))
        };
        let expand = |operand: &Operand| -> Vec<MaterialId> {
            match operand {
                Operand::Name(id) => vec![*id],
                Operand::Tag(bit) => (0..len)
                    .filter(|&index| tag_bits[index] & bit != 0)
                    .map(|index| MaterialId(index as u16))
                    .collect(),
            }
        };

        let mut table: Vec<Option<(Reaction, u8)>> = vec![None; len * len];
        for def in &reaction_defs {
            let op_a = resolve(&def.a)?;
            let op_b = resolve(&def.b)?;
            let becomes_a = match resolve(&def.a_becomes)? {
                Operand::Name(id) => id,
                Operand::Tag(_) => {
                    return Err(RegistryError::UnknownMaterial(def.a_becomes.clone()));
                }
            };
            let becomes_b = match resolve(&def.b_becomes)? {
                Operand::Name(id) => id,
                Operand::Tag(_) => {
                    return Err(RegistryError::UnknownMaterial(def.b_becomes.clone()));
                }
            };
            let specificity =
                matches!(op_a, Operand::Name(_)) as u8 + matches!(op_b, Operand::Name(_)) as u8;
            for a in expand(&op_a) {
                for b in expand(&op_b) {
                    let entries = if a == b {
                        vec![(a, b, becomes_a, becomes_b)]
                    } else {
                        vec![(a, b, becomes_a, becomes_b), (b, a, becomes_b, becomes_a)]
                    };
                    for (from, other, becomes, other_becomes) in entries {
                        let slot = &mut table[from.0 as usize * len + other.0 as usize];
                        match slot {
                            Some((_, existing)) if *existing == specificity => {
                                return Err(RegistryError::AmbiguousReaction(
                                    materials[from.0 as usize].name.clone(),
                                    materials[other.0 as usize].name.clone(),
                                ));
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
                drag_keep: (1.0 - material.drag * crate::TICK_DT).clamp(0.0, 1.0),
                friction: material.friction.clamp(0.0, 1.0),
                cohesion: (material.cohesion * crate::TICK_DT).clamp(0.0, 1.0),
                restitution: material.restitution.clamp(0.0, 1.0),
                flow_chance: if material.flow_rate > 0.0 {
                    per_tick_chance(material.flow_rate)
                } else {
                    1.0
                },
            })
            .collect();

        let hash = registry_hash(&materials, &reaction_defs);
        let tag_index = tag_index
            .into_iter()
            .map(|(tag, index)| (tag.to_string(), index))
            .collect();
        Ok(Self {
            materials,
            by_name,
            hash,
            tag_index,
            tag_bits,
            sustain_bits,
            reactions,
            decays,
            reactive,
            dynamics,
        })
    }

    #[inline]
    pub fn get(&self, id: MaterialId) -> &Material {
        &self.materials[id.0 as usize]
    }

    #[inline]
    pub fn try_get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.0 as usize)
    }

    pub fn id_of(&self, name: &str) -> Option<MaterialId> {
        self.by_name.get(name).copied()
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

    pub const fn hash(&self) -> u64 {
        self.hash
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
    pub fn is_reactive(&self, id: MaterialId) -> bool {
        self.reactive[id.0 as usize]
    }

    #[inline]
    pub fn dynamics(&self, id: MaterialId) -> Dynamics {
        self.dynamics[id.0 as usize]
    }

    #[inline]
    pub fn sustains(&self, fire: MaterialId, neighbor: MaterialId) -> bool {
        self.sustain_bits[fire.0 as usize] & self.tag_bits[neighbor.0 as usize] != 0
    }

    pub fn tag_mask(&self, tag: &str) -> u32 {
        self.tag_index
            .get(tag)
            .map(|&index| 1u32 << index)
            .unwrap_or(0)
    }

    #[inline]
    pub fn has_tag(&self, id: MaterialId, mask: u32) -> bool {
        self.tag_bits[id.0 as usize] & mask != 0
    }
}

fn registry_hash(materials: &[Material], reactions: &[ReactionDef]) -> u64 {
    let bytes = postcard::to_allocvec(&(materials, reactions)).expect("materials serialize");
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
