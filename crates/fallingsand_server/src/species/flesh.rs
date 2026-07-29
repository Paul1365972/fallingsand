use super::Frame;
use fallingsand_core::content::material;
use fallingsand_core::{CellPos, CellRect, MaterialId};

pub const COLS: i32 = 3;
pub const STAND_ROWS: i32 = 9;
pub const DUCK_ROWS: i32 = 5;

const LEGEND: &[(u8, MaterialId, u8)] = &[
    (b'r', material::FLESH, 0),
    (b'h', material::FLESH, 1),
    (b's', material::FLESH, 2),
    (b'd', material::FLESH, 3),
    (b'b', material::FLESH, 4),
    (b'k', material::FLESH, 5),
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

pub fn rect(anchor: CellPos, rows: i32) -> CellRect {
    CellRect::spanning(
        CellPos::new(anchor.x - COLS / 2, anchor.y - rows / 2),
        COLS,
        rows,
    )
}

pub fn feet(anchor: CellPos, rows: i32) -> i32 {
    anchor.y - rows / 2
}

pub fn anchor(x: i32, feet: i32, rows: i32) -> CellPos {
    CellPos::new(x, feet + rows / 2)
}

pub fn cells(anchor: CellPos, rows: i32, facing_left: bool) -> Vec<(CellPos, MaterialId, u8)> {
    frame(rows).cells(rect(anchor, rows).min, facing_left)
}
