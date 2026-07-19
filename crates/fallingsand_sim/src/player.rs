use crate::bodies::{Raster, commit_stamp};
use crate::physics::Footprint;
use crate::world::CellWorld;
use fallingsand_core::content::material;
use fallingsand_core::{Cell, CellPos};
use rustc_hash::FxHashSet;

pub const PLAYER_COLS: usize = 3;
pub const STAND_ROWS: usize = 9;
pub const DUCK_ROWS: usize = 5;

const ROBE: u8 = 0;
const HAIR: u8 = 1;
const SHADE: u8 = 2;
const DARK: u8 = 3;
const BELT: u8 = 4;
const BUCKLE: u8 = 5;

const ROWS_9: [[u8; PLAYER_COLS]; 9] = [
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

const ROWS_8: [[u8; PLAYER_COLS]; 8] = [
    [ROBE, HAIR, HAIR],
    [SHADE, SHADE, SHADE],
    [ROBE, ROBE, SHADE],
    [ROBE, ROBE, SHADE],
    [BELT, BELT, BUCKLE],
    [ROBE, ROBE, SHADE],
    [ROBE, ROBE, SHADE],
    [SHADE, SHADE, DARK],
];

const ROWS_7: [[u8; PLAYER_COLS]; 7] = [
    [ROBE, HAIR, HAIR],
    [SHADE, SHADE, SHADE],
    [ROBE, ROBE, SHADE],
    [ROBE, ROBE, SHADE],
    [BELT, BELT, BUCKLE],
    [ROBE, ROBE, SHADE],
    [SHADE, SHADE, DARK],
];

const ROWS_6: [[u8; PLAYER_COLS]; 6] = [
    [ROBE, HAIR, HAIR],
    [SHADE, SHADE, SHADE],
    [ROBE, ROBE, SHADE],
    [BELT, BELT, BUCKLE],
    [ROBE, ROBE, SHADE],
    [SHADE, SHADE, DARK],
];

const ROWS_5: [[u8; PLAYER_COLS]; 5] = [
    [ROBE, HAIR, HAIR],
    [SHADE, SHADE, SHADE],
    [ROBE, ROBE, SHADE],
    [BELT, BELT, BUCKLE],
    [SHADE, SHADE, DARK],
];

fn pattern(rows: usize) -> &'static [[u8; PLAYER_COLS]] {
    match rows {
        5 => &ROWS_5,
        6 => &ROWS_6,
        7 => &ROWS_7,
        8 => &ROWS_8,
        _ => &ROWS_9,
    }
}

#[derive(Debug, Default)]
pub struct PlayerStamp {
    pub(crate) raster: Option<Raster>,
    pub(crate) rows: u8,
    pub(crate) facing_left: bool,
}

impl PlayerStamp {
    pub fn is_stamped(&self) -> bool {
        self.raster.is_some()
    }

    pub fn facing_left(&self) -> bool {
        self.facing_left
    }

    pub fn own_cells(&self) -> Option<&FxHashSet<CellPos>> {
        self.raster.as_ref().map(|raster| &raster.set)
    }

    pub fn covers(&self, pos: CellPos) -> bool {
        self.raster
            .as_ref()
            .is_some_and(|raster| raster.covers(pos))
    }
}

fn shade_for(local: u16, rows: u8, facing_left: bool) -> u8 {
    let row = (local / PLAYER_COLS as u16) as usize;
    let mut col = (local % PLAYER_COLS as u16) as usize;
    if facing_left {
        col = PLAYER_COLS - 1 - col;
    }
    pattern(rows as usize)[row][col]
}

fn flesh_cell(local: u16, rows: u8, facing_left: bool) -> Cell {
    let mut cell = Cell::new(material::FLESH, shade_for(local, rows, facing_left));
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
    stamp: &mut PlayerStamp,
    fp: Footprint,
    facing_left: bool,
) -> Option<()> {
    let rows = (fp.y1 - fp.y0 + 1) as u8;
    let unchanged = stamp.rows == rows
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
                .is_some_and(|cell| cell.material == material::FLESH && cell.is_body())
        });
        if !intact {
            for &(pos, local) in &raster.cells {
                world.set_cell_raw(pos, flesh_cell(local, rows, facing_left));
            }
        }
        return Some(());
    }

    let new = player_raster(fp);
    let cell_for = |local: u16| flesh_cell(local, rows, facing_left);
    let empty = Raster::default();
    let old = stamp.raster.as_ref().unwrap_or(&empty);
    commit_stamp(world, &[], old, &new, &cell_for)?;
    stamp.raster = Some(new);
    stamp.rows = rows;
    stamp.facing_left = facing_left;
    Some(())
}

pub fn unstamp_player(world: &mut CellWorld, stamp: &mut PlayerStamp) {
    if let Some(raster) = stamp.raster.take() {
        for &(pos, _) in &raster.cells {
            world.set_cell_raw(pos, Cell::AIR);
        }
    }
}
