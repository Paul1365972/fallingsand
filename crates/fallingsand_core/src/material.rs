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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub phase: Phase,
    pub density: f32,
    pub colors: Vec<[u8; 4]>,
    #[serde(default)]
    pub flammability: f32,
    #[serde(default)]
    pub rigid_capable: bool,
    #[serde(default)]
    pub dispersion: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialFile {
    pub materials: Vec<Material>,
}

#[derive(Debug, Clone)]
pub struct MaterialRegistry {
    materials: Vec<Material>,
    by_name: HashMap<String, MaterialId>,
    hash: u64,
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
}

impl MaterialRegistry {
    pub fn from_ron(source: &str) -> Result<Self, RegistryError> {
        let file: MaterialFile = ron::from_str(source)?;
        Self::from_materials(file.materials)
    }

    pub fn from_materials(materials: Vec<Material>) -> Result<Self, RegistryError> {
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
        let hash = registry_hash(&materials);
        Ok(Self {
            materials,
            by_name,
            hash,
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
}

fn registry_hash(materials: &[Material]) -> u64 {
    let bytes = postcard::to_allocvec(materials).expect("materials serialize");
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
