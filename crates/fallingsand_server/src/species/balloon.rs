use super::{Frame, Species};
use fallingsand_core::MaterialId;
use fallingsand_core::content::material;
use fallingsand_sim::body::Policy;

pub const SPECIES: Species = Species {
    name: "balloon",
    frame: &BALLOON,
    policy: Policy::DEBRIS,
    life: None,
};

const LEGEND: &[(u8, MaterialId, u8)] = &[
    (b'o', material::BALLOON, 0),
    (b'l', material::BALLOON, 1),
    (b'd', material::BALLOON, 2),
    (b's', material::BALLOON_STRING, 0),
    (b'h', material::BALLOON_STRING, 1),
    (b'k', material::BALLOON_STRING, 2),
];

const BALLOON: Frame = Frame {
    art: r"
        .loo.
        llooo
        ooooo
        ooodd
        .ood.
        ..h..
        ..s..
        ..k..
    ",
    legend: LEGEND,
};
