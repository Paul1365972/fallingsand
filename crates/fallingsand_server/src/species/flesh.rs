use super::Frame;
use fallingsand_core::content::material;
use fallingsand_core::{CellPos, CellRect, MaterialId};

pub const COLS: i32 = 3;
pub const STAND_ROWS: i32 = 9;
pub const DUCK_ROWS: i32 = 5;

const LEGEND: &[(u8, MaterialId, u8)] = &[
    (b'r', material::BODY, 0),
    (b'h', material::BODY, 1),
    (b's', material::BODY, 2),
    (b'd', material::BODY, 3),
    (b'b', material::BODY, 4),
    (b'k', material::BODY, 5),
];

const ROWS_9: Frame = Frame {
    art: r"
        rrh
        rhh
        sss
        rrs
        rrs
        bbk
        rrs
        rrs
        ssd
    ",
    legend: LEGEND,
};

const ROWS_8: Frame = Frame {
    art: r"
        rhh
        sss
        rrs
        rrs
        bbk
        rrs
        rrs
        ssd
    ",
    legend: LEGEND,
};

const ROWS_7: Frame = Frame {
    art: r"
        rhh
        sss
        rrs
        rrs
        bbk
        rrs
        ssd
    ",
    legend: LEGEND,
};

const ROWS_6: Frame = Frame {
    art: r"
        rhh
        sss
        rrs
        bbk
        rrs
        ssd
    ",
    legend: LEGEND,
};

const ROWS_5: Frame = Frame {
    art: r"
        rhh
        sss
        rrs
        bbk
        ssd
    ",
    legend: LEGEND,
};

pub fn frame(rows: i32) -> &'static Frame {
    match rows {
        5 => &ROWS_5,
        6 => &ROWS_6,
        7 => &ROWS_7,
        8 => &ROWS_8,
        _ => &ROWS_9,
    }
}

pub fn corner(stand: CellPos) -> CellPos {
    CellPos::new(stand.x - COLS / 2, stand.y)
}

pub fn standing(corner: CellPos) -> CellPos {
    CellPos::new(corner.x + COLS / 2, corner.y)
}

pub fn rect(corner: CellPos, rows: i32) -> CellRect {
    CellRect::spanning(corner, COLS, rows)
}

pub fn cells(corner: CellPos, rows: i32, facing_left: bool) -> Vec<(CellPos, MaterialId, u8)> {
    frame(rows).cells(corner, facing_left)
}
