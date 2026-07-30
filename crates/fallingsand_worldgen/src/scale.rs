pub const PLAYER_WIDTH: i32 = 3;

const REFERENCE_WIDTH: i32 = 3;

pub const fn len(reference: i32) -> i32 {
    let scaled = reference * PLAYER_WIDTH / REFERENCE_WIDTH;
    if reference > 0 && scaled < 1 {
        1
    } else if reference < 0 && scaled > -1 {
        -1
    } else {
        scaled
    }
}

pub fn wave(reference: f32) -> f32 {
    reference * PLAYER_WIDTH as f32 / REFERENCE_WIDTH as f32
}

pub const fn pitch(reference: i32) -> i32 {
    let scaled = len(reference);
    let snapped = (scaled + 4) / 8 * 8;
    if snapped < 8 { 8 } else { snapped }
}
