use super::{DriveCtx, Frame, Life, Species};
use fallingsand_core::content::material;
use fallingsand_core::{MaterialId, Subcell};
use fallingsand_sim::body::Policy;

pub const SPECIES: Species = Species {
    name: "frog",
    frame: &SIT,
    policy: Policy::MOB,
    life: Some(Life {
        max_hp: 10.0,
        corpse: material::CORPSE,
        drive,
    }),
};

const LEGEND: &[(u8, MaterialId, u8)] = &[
    (b'g', material::FROG, 0),
    (b'd', material::FROG, 1),
    (b'b', material::FROG, 2),
    (b'e', material::FROG, 3),
];

const SIT: Frame = Frame {
    art: r"
        dge
        dbb
    ",
    legend: LEGEND,
};

const STRETCH: Frame = Frame {
    art: r"
        gge
        dbb
    ",
    legend: LEGEND,
};

struct Leap {
    vx: Subcell,
    vy: Subcell,
}

const SWIM_MIN_SUBMERSION: f32 = 0.5;
const GROUND_FRICTION: Subcell = Subcell::from_cells_per_second_squared(600);
const HOP: Leap = Leap {
    vx: Subcell::from_cells_per_second(30.0),
    vy: Subcell::from_cells_per_second(65.0),
};
const FLEE: Leap = Leap {
    vx: Subcell::from_cells_per_second(50.0),
    vy: Subcell::from_cells_per_second(85.0),
};
const PADDLE: Leap = Leap {
    vx: Subcell::from_cells_per_second(20.0),
    vy: Subcell::from_cells_per_second(40.0),
};
const REST_TICKS: (i32, i32) = (45, 300);
const FLEE_REST_TICKS: (i32, i32) = (10, 30);
const PADDLE_TICKS: (i32, i32) = (15, 35);

fn drive(ctx: &mut DriveCtx) {
    let (mut vx, mut vy) = ctx.velocity();
    let supported = ctx.supported();
    let swimming = !supported && ctx.submersion() >= SWIM_MIN_SUBMERSION;
    let threat = ctx.threat;
    let mind = &mut *ctx.mind;
    if threat.is_some() {
        let (_, max) = FLEE_REST_TICKS;
        mind.rest = mind.rest.min(max as u16);
    }
    mind.rest = mind.rest.saturating_sub(1);
    if supported {
        vx = vx.approach(Subcell::ZERO, GROUND_FRICTION);
        if mind.rest == 0 {
            mind.facing = match threat {
                Some(dir) if dir != 0 => dir,
                Some(_) => -mind.facing,
                None if ctx.rng.draw().bit() => 1,
                None => -1,
            };
            let (leap, (min, max)) = if threat.is_some() {
                (FLEE, FLEE_REST_TICKS)
            } else {
                (HOP, REST_TICKS)
            };
            vx = leap.vx.times(mind.facing);
            vy = leap.vy;
            mind.rest = ctx.rng.draw().range(min, max) as u16;
        }
    } else if swimming {
        let (min, max) = PADDLE_TICKS;
        mind.rest = mind.rest.min(max as u16);
        if mind.rest == 0 {
            mind.facing = threat.filter(|&dir| dir != 0).unwrap_or(mind.facing);
            vx += PADDLE.vx.times(mind.facing);
            vy += PADDLE.vy;
            mind.rest = ctx.rng.draw().range(min, max) as u16;
        }
    }
    let facing_left = mind.facing < 0;
    ctx.drive(vx, vy);
    ctx.paint(if supported { &SIT } else { &STRETCH }, facing_left);
}
