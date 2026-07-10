use crate::bodies::{Raster, commit_stamp};
use crate::physics::Footprint;
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, MaterialId, MaterialRegistry};

pub const PLAYER_COLS: usize = 3;
pub const STAND_ROWS: usize = 9;
pub const DUCK_ROWS: usize = 5;

const ROBE: u8 = 0;
const HAIR: u8 = 1;
const SHADE: u8 = 2;
const DARK: u8 = 3;
const BELT: u8 = 4;
const BUCKLE: u8 = 5;

const STAND_PATTERN: [[u8; PLAYER_COLS]; STAND_ROWS] = [
    [ROBE, ROBE, HAIR],
    [ROBE, HAIR, HAIR],
    [SHADE, SHADE, SHADE],
    [ROBE, ROBE, SHADE],
    [ROBE, ROBE, SHADE],
    [BELT, BELT, BUCKLE],
    [ROBE, ROBE, SHADE],
    [ROBE, ROBE, SHADE],
    [SHADE, SHADE, DARK],
];

const DUCK_PATTERN: [[u8; PLAYER_COLS]; DUCK_ROWS] = [
    [ROBE, HAIR, HAIR],
    [SHADE, SHADE, SHADE],
    [ROBE, ROBE, SHADE],
    [BELT, BELT, BUCKLE],
    [SHADE, SHADE, DARK],
];

#[derive(Debug, Default)]
pub struct PlayerStamp {
    pub raster: Option<Raster>,
    pub ducked: bool,
    pub facing_left: bool,
}

fn shade_for(local: u16, ducked: bool, facing_left: bool) -> u8 {
    let row = (local / PLAYER_COLS as u16) as usize;
    let mut col = (local % PLAYER_COLS as u16) as usize;
    if facing_left {
        col = PLAYER_COLS - 1 - col;
    }
    if ducked {
        DUCK_PATTERN[row][col]
    } else {
        STAND_PATTERN[row][col]
    }
}

fn flesh_cell(flesh: MaterialId, local: u16, ducked: bool, facing_left: bool) -> Cell {
    let mut cell = Cell::new(flesh, shade_for(local, ducked, facing_left));
    cell.set_body(true);
    cell
}

pub(crate) fn player_raster(fp: Footprint) -> Raster {
    let mut raster = Raster::default();
    for y in fp.y0..=fp.y1 {
        for x in fp.x0..=fp.x1 {
            let pos = CellPos::new(x, y);
            let row = (fp.y1 - y) as u16;
            let col = (x - fp.x0) as u16;
            let local = row * PLAYER_COLS as u16 + col;
            if raster.set.insert(pos) {
                raster.cells.push((pos, local));
            }
        }
    }
    raster
}

fn covers_exactly(raster: &Raster, fp: Footprint) -> bool {
    let area = ((fp.x1 - fp.x0 + 1) * (fp.y1 - fp.y0 + 1)) as usize;
    raster.cells.len() == area
        && raster.covers(CellPos::new(fp.x0, fp.y0))
        && raster.covers(CellPos::new(fp.x1, fp.y1))
}

pub fn stamp_player(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    flesh: MaterialId,
    stamp: &mut PlayerStamp,
    fp: Footprint,
    ducked: bool,
    facing_left: bool,
) -> Option<Vec<CellPos>> {
    let unchanged = stamp.ducked == ducked
        && stamp.facing_left == facing_left
        && stamp
            .raster
            .as_ref()
            .is_some_and(|raster| covers_exactly(raster, fp));
    if unchanged {
        let raster = stamp.raster.as_ref().expect("unchanged stamp has a raster");
        let intact = raster.cells.iter().all(|&(pos, _)| {
            world
                .get_cell(pos)
                .is_some_and(|cell| cell.material == flesh && cell.is_body())
        });
        if !intact {
            for &(pos, local) in &raster.cells {
                world.set_cell_raw(pos, flesh_cell(flesh, local, ducked, facing_left));
            }
        }
        return Some(Vec::new());
    }

    let new = player_raster(fp);
    let cell_for = |local: u16| flesh_cell(flesh, local, ducked, facing_left);
    let empty = Raster::default();
    let old = stamp.raster.as_ref().unwrap_or(&empty);
    let vacated = commit_stamp(world, registry, &[], old, &new, &cell_for)?;
    stamp.raster = Some(new);
    stamp.ducked = ducked;
    stamp.facing_left = facing_left;
    Some(vacated)
}

pub fn force_stamp_player(
    world: &mut CellWorld,
    flesh: MaterialId,
    stamp: &mut PlayerStamp,
    fp: Footprint,
    ducked: bool,
    facing_left: bool,
) {
    unstamp_player(world, stamp);
    let new = player_raster(fp);
    for &(pos, local) in &new.cells {
        world.set_cell_raw(pos, flesh_cell(flesh, local, ducked, facing_left));
    }
    stamp.raster = Some(new);
    stamp.ducked = ducked;
    stamp.facing_left = facing_left;
}

pub fn unstamp_player(world: &mut CellWorld, stamp: &mut PlayerStamp) {
    if let Some(raster) = stamp.raster.take() {
        for &(pos, _) in &raster.cells {
            world.set_cell_raw(pos, Cell::AIR);
        }
    }
}
