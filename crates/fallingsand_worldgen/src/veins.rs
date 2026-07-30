use crate::biomes::{Lithology, SubBiome};
use crate::scale::{len, pitch};
use fallingsand_core::MaterialId;
use fallingsand_core::content::material;
use fallingsand_math::Hash;

const VEIN_SALT: Hash = Hash::label("worldgen.vein");

struct Deposit {
    material: MaterialId,
    hosts: &'static [Lithology],
    peak: i32,
    spread: f32,
    lattice: i32,
    chance: f32,
    radius: (i32, i32),
    elongation: f32,
    tell: Option<MaterialId>,
}

pub struct Vein {
    pub x: i32,
    pub y: i32,
    pub material: MaterialId,
    pub tell: bool,
}

fn deposits() -> Vec<Deposit> {
    vec![
        Deposit {
            material: material::FLINT,
            hosts: &[Lithology::Carbonate],
            peak: len(-80),
            spread: len(300) as f32,
            lattice: pitch(170),
            chance: 0.26,
            radius: (2, 4),
            elongation: 1.0,
            tell: None,
        },
        Deposit {
            material: material::AMBER,
            hosts: &[Lithology::Soil],
            peak: len(-60),
            spread: len(180) as f32,
            lattice: pitch(460),
            chance: 0.12,
            radius: (1, 3),
            elongation: 1.0,
            tell: None,
        },
        Deposit {
            material: material::NATIVE_COPPER,
            hosts: &[Lithology::Clastic, Lithology::Igneous, Lithology::Carbonate],
            peak: len(-180),
            spread: len(360) as f32,
            lattice: pitch(230),
            chance: 0.24,
            radius: (2, 5),
            elongation: 1.2,
            tell: Some(material::VERDIGRIS),
        },
        Deposit {
            material: material::TIN_ORE,
            hosts: &[Lithology::Crystalline, Lithology::Igneous],
            peak: len(-240),
            spread: len(340) as f32,
            lattice: pitch(380),
            chance: 0.18,
            radius: (2, 4),
            elongation: 1.4,
            tell: None,
        },
        Deposit {
            material: material::COAL,
            hosts: &[Lithology::Clastic, Lithology::Soil],
            peak: len(-500),
            spread: len(800) as f32,
            lattice: pitch(150),
            chance: 0.32,
            radius: (4, 11),
            elongation: 5.0,
            tell: None,
        },
        Deposit {
            material: material::SALTPETER,
            hosts: &[Lithology::Carbonate],
            peak: len(-560),
            spread: len(520) as f32,
            lattice: pitch(320),
            chance: 0.20,
            radius: (2, 5),
            elongation: 2.2,
            tell: None,
        },
        Deposit {
            material: material::IRON_ORE,
            hosts: &[
                Lithology::Clastic,
                Lithology::Igneous,
                Lithology::Crystalline,
            ],
            peak: len(-1500),
            spread: len(1100) as f32,
            lattice: pitch(200),
            chance: 0.26,
            radius: (3, 7),
            elongation: 1.3,
            tell: Some(material::RUST),
        },
        Deposit {
            material: material::QUARTZ,
            hosts: &[Lithology::Crystalline, Lithology::Igneous],
            peak: len(-2100),
            spread: len(1600) as f32,
            lattice: pitch(300),
            chance: 0.28,
            radius: (3, 8),
            elongation: 3.2,
            tell: None,
        },
        Deposit {
            material: material::GOLD,
            hosts: &[Lithology::Crystalline],
            peak: len(-2600),
            spread: len(1800) as f32,
            lattice: pitch(560),
            chance: 0.20,
            radius: (1, 3),
            elongation: 2.4,
            tell: Some(material::QUARTZ),
        },
        Deposit {
            material: material::SULFUR,
            hosts: &[Lithology::Igneous],
            peak: len(-4400),
            spread: len(2200) as f32,
            lattice: pitch(240),
            chance: 0.26,
            radius: (3, 7),
            elongation: 1.6,
            tell: None,
        },
        Deposit {
            material: material::LUMEN,
            hosts: &[Lithology::Crystalline, Lithology::Flesh],
            peak: len(-7200),
            spread: len(2600) as f32,
            lattice: pitch(380),
            chance: 0.24,
            radius: (3, 8),
            elongation: 1.1,
            tell: Some(material::QUARTZ),
        },
    ]
}

pub fn veins_for_rect(
    seed: u64,
    sub_at: &dyn Fn(i32, i32) -> SubBiome,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
) -> Vec<Vein> {
    let mut out = Vec::new();
    for (index, deposit) in deposits().iter().enumerate() {
        let reach = len(deposit.radius.1) * deposit.elongation.ceil() as i32 + len(4);
        let lo_x = (min_x - reach).div_euclid(deposit.lattice);
        let hi_x = (max_x + reach).div_euclid(deposit.lattice);
        let lo_y = (min_y - reach).div_euclid(deposit.lattice);
        let hi_y = (max_y + reach).div_euclid(deposit.lattice);
        for cell_y in lo_y..=hi_y {
            for cell_x in lo_x..=hi_x {
                let hash = Hash::seed(seed)
                    .salt(VEIN_SALT)
                    .add(index as u64)
                    .pos(cell_x, cell_y);
                let mut rng = hash.rng();
                if !rng.draw().chance(deposit.chance) {
                    continue;
                }
                let cx = cell_x * deposit.lattice + rng.draw().range(0, deposit.lattice - 1);
                let cy = cell_y * deposit.lattice + rng.draw().range(0, deposit.lattice - 1);

                let offset = (cy - deposit.peak) as f32 / deposit.spread;
                if !rng.draw().chance((-offset * offset).exp()) {
                    continue;
                }
                if !deposit.hosts.contains(&sub_at(cx, cy).lithology) {
                    continue;
                }

                let radius = len(rng.draw().range(deposit.radius.0, deposit.radius.1)) as f32;
                let rx = radius * deposit.elongation;
                let ry = radius;
                let tilt = (rng.draw().unit() - 0.5) * 0.7;
                for y in (cy - ry.ceil() as i32 - 1)..=(cy + ry.ceil() as i32 + 1) {
                    for x in (cx - rx.ceil() as i32 - 1)..=(cx + rx.ceil() as i32 + 1) {
                        if x < min_x || x > max_x || y < min_y || y > max_y {
                            continue;
                        }
                        let dx = (x - cx) as f32;
                        let dy = (y - cy) as f32;
                        let rotated_x = dx + dy * tilt;
                        let nx = rotated_x / rx;
                        let ny = dy / ry;
                        let wobble = Hash::seed(seed).pos(x, y).unit() * 0.22;
                        let distance = nx * nx + ny * ny;
                        if distance < 1.0 - wobble {
                            out.push(Vein {
                                x,
                                y,
                                material: deposit.material,
                                tell: false,
                            });
                        } else if deposit.tell.is_some() && distance < 1.5 - wobble {
                            out.push(Vein {
                                x,
                                y,
                                material: deposit.tell.expect("checked"),
                                tell: true,
                            });
                        }
                    }
                }
            }
        }
    }
    out
}
