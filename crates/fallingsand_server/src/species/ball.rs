use super::{Frame, Species};
use fallingsand_core::MaterialId;
use fallingsand_core::content::material;
use fallingsand_sim::body::Policy;

pub const SPECIES: Species = Species {
    name: "ball",
    frame: &BALL,
    policy: Policy::BALL,
    life: None,
};

const LEGEND: &[(u8, MaterialId, u8)] = &[
    (b'r', material::RUBBER, 0),
    (b'l', material::RUBBER, 1),
    (b'd', material::RUBBER, 2),
    (b'w', material::RUBBER, 3),
];

const BALL: Frame = Frame {
    art: r"
        .lw.
        lrrd
        rrrd
        .dd.
    ",
    legend: LEGEND,
};
