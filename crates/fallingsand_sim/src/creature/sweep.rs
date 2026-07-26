use super::{BOUNCE_MIN_SPEED, Creature};
use crate::shape::{Blockage, CellSource, Dir, OwnCells};
use fallingsand_core::content;
use fallingsand_core::{CellPos, Subcell};

const LAUNCH_MIN_SPEED: Subcell = Subcell::from_cells_per_second(80.0);
const LEDGE_LAUNCH_FACTOR: f32 = 0.35;
const STEP_UP_CELLS: i32 = 3;
const STEP_DOWN_CELLS: i32 = 3;
const CEILING_VY_DAMP: f32 = 0.5;
const CEILING_VX_REDIRECT: f32 = 0.25;
const SNAP_DOWN_MAX_SUBMERSION: f32 = 0.5;
const CLIMB_COST: f32 = 0.5;
const CLIMB_DRAIN: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blocked {
    pub pos: CellPos,
    pub velocity_delta_x: Subcell,
    pub velocity_delta_y: Subcell,
}

#[derive(Debug, Default)]
pub struct MoveResult {
    pub blocked: Vec<Blocked>,
    pub hit_ceiling: bool,
    pub(super) corrected_ceiling: bool,
}

impl MoveResult {
    fn record_blocked(
        &mut self,
        solids: &[CellPos],
        velocity_delta_x: Subcell,
        velocity_delta_y: Subcell,
    ) {
        if solids.is_empty() {
            return;
        }
        let count = solids.len() as u32;
        for &pos in solids {
            self.blocked.push(Blocked {
                pos,
                velocity_delta_x: velocity_delta_x.per_substep(count),
                velocity_delta_y: velocity_delta_y.per_substep(count),
            });
        }
    }
}

fn resolve_axis(v: Subcell, e: f32) -> Subcell {
    if v.abs() > Subcell::from_cells_per_second(BOUNCE_MIN_SPEED) {
        -v.scaled_by(e)
    } else {
        Subcell::ZERO
    }
}

fn solids_bounce<W: CellSource>(world: &W, solids: &[CellPos]) -> f32 {
    let mut e = 0.0f32;
    for &pos in solids {
        if let Some(cell) = world.cell_at(pos) {
            e = e.max(content::material(cell.material).entity_bounce);
        }
    }
    e
}

fn quanta_toward(pose: Subcell, target: Subcell) -> i32 {
    let start = pose.floor_cell();
    let end = target.floor_cell();
    if target > pose {
        if Subcell::from_cell(end) < target {
            end - start
        } else {
            end - start - 1
        }
    } else if target < pose {
        if Subcell::from_cell(end + 1) - Subcell::QUANTUM > target {
            start - end
        } else {
            start - end - 1
        }
    } else {
        0
    }
}

fn quantum_end(pose: Subcell, dir: i32, quanta: i32) -> Subcell {
    if quanta == 0 {
        pose
    } else if dir > 0 {
        Subcell::from_cell(pose.floor_cell() + quanta)
    } else {
        Subcell::from_cell(pose.floor_cell() - quanta + 1) - Subcell::QUANTUM
    }
}

fn snapped(dir: i32, blocking: i32, span: i32, anchor_off: i32) -> Subcell {
    if dir > 0 {
        Subcell::from_cell(blocking - span + anchor_off + 1) - Subcell::QUANTUM
    } else {
        Subcell::from_cell(blocking + 1 + anchor_off)
    }
}

fn try_step_up<W: CellSource>(
    world: &W,
    body: &mut Creature,
    blockage: &Blockage,
    own: OwnCells,
) -> bool {
    let Some(step_top) = blockage.step_top() else {
        return false;
    };
    let fp = body.footprint();
    let down = body.y.floor_cell() - fp.y0;
    let rise_needed = Subcell::from_cell(step_top + 1 + down) - body.y;
    if rise_needed <= Subcell::ZERO || rise_needed > Subcell::from_cells(STEP_UP_CELLS as f32) {
        return false;
    }
    let raised = body.shape.origin(body.x, body.y + rise_needed);
    if body.shape.blocked_at(world, own, body.origin(), raised) {
        return false;
    }
    body.y += rise_needed;
    body.climb_debt += rise_needed.scaled_by(CLIMB_COST);
    if body.vx.abs() > LAUNCH_MIN_SPEED {
        body.vy = body.vy.max(body.vx.abs().scaled_by(LEDGE_LAUNCH_FACTOR));
    }
    true
}

fn ceiling_correct<W: CellSource>(
    world: &W,
    body: &Creature,
    next_y: Subcell,
    vx: Subcell,
    own: OwnCells,
) -> Option<(Subcell, i32)> {
    let max_shift = body.shape.w() - 1;
    if max_shift < 1 {
        return None;
    }
    let head_row = body.shape.footprint(body.x, next_y).y1;
    let origin = body.origin();
    let sides: [i32; 2] = if vx > Subcell::ZERO { [1, -1] } else { [-1, 1] };
    for side in sides {
        for step in 1..=max_shift {
            let cand_x = body.x + Subcell::from_cells((side * step) as f32);
            let beside = body.shape.origin(cand_x, body.y);
            if body.shape.blocked_at(world, own, origin, beside) {
                break;
            }
            let above = body.shape.origin(cand_x, next_y);
            let up = body.shape.blockage_at(world, own, origin, above);
            if up.free() {
                return Some((cand_x, side));
            }
            if up.single_head_hit(head_row).is_none() {
                break;
            }
        }
    }
    None
}

struct AxisState {
    dir: i32,
    tail: Subcell,
    blocked: bool,
}

pub fn move_body<W: CellSource>(
    world: &W,
    body: &mut Creature,
    submersion: f32,
    own: OwnCells,
) -> MoveResult {
    let mut result = MoveResult::default();
    let was_grounded = body.on_ground;
    body.on_ground = false;

    let mut remaining_x = body.vx;
    let remaining_y = body.vy;
    if remaining_x == Subcell::ZERO {
        body.climb_debt = Subcell::ZERO;
    } else {
        let drain = body
            .climb_debt
            .scaled_by(CLIMB_DRAIN)
            .min(remaining_x.abs());
        body.climb_debt -= drain;
        remaining_x = if remaining_x > Subcell::ZERO {
            remaining_x - drain
        } else {
            remaining_x + drain
        };
    }

    let target_x = body.x + remaining_x;
    let mut target_y = body.y + remaining_y;
    let dir_x = if remaining_x > Subcell::ZERO {
        1i32
    } else {
        -1
    };
    let dir_y = if remaining_y > Subcell::ZERO {
        1i32
    } else {
        -1
    };
    let mut distance = [
        quanta_toward(body.x, target_x),
        quanta_toward(body.y, target_y),
    ];
    let mut done = [0i32, 0];
    let mut x_state = AxisState {
        dir: dir_x,
        tail: target_x - quantum_end(body.x, dir_x, distance[0]),
        blocked: false,
    };
    let mut y_state = AxisState {
        dir: dir_y,
        tail: target_y - quantum_end(body.y, dir_y, distance[1]),
        blocked: false,
    };

    let mut climbed = false;
    let mut corrected = false;
    while done != distance {
        let axis = if done[0] == distance[0] {
            1
        } else if done[1] == distance[1] || done[0] * distance[1] <= done[1] * distance[0] {
            0
        } else {
            1
        };
        let (ox, oy) = body.origin();
        if axis == 0 {
            let dir = x_state.dir;
            let cand = (ox + dir, oy);
            let step_dir = if dir > 0 { Dir::PosX } else { Dir::NegX };
            let blockage = body.shape.step_blockage(world, own, cand, step_dir);
            if blockage.free() {
                let anchor = body.x.floor_cell();
                body.x = if dir > 0 {
                    Subcell::from_cell(anchor + 1)
                } else {
                    Subcell::from_cell(anchor) - Subcell::QUANTUM
                };
                done[0] += 1;
                continue;
            }
            if try_step_up(world, body, &blockage, own) {
                climbed = true;
                continue;
            }
            let e = solids_bounce(world, &blockage.solids);
            let after = resolve_axis(body.vx, e);
            result.record_blocked(&blockage.solids, body.vx - after, Subcell::ZERO);
            let leading = if dir > 0 {
                cand.0 + body.shape.w() - 1
            } else {
                cand.0
            };
            let col = blockage.near_col(dir).unwrap_or(leading);
            body.x = snapped(dir, col, body.shape.w(), body.shape.w() / 2);
            body.vx = after;
            x_state.blocked = true;
            done[0] = distance[0];
        } else {
            let dir = y_state.dir;
            let cand = (ox, oy + dir);
            let step_dir = if dir > 0 { Dir::PosY } else { Dir::NegY };
            let blockage = body.shape.step_blockage(world, own, cand, step_dir);
            let anchor = body.y.floor_cell();
            let next_y = if dir > 0 {
                Subcell::from_cell(anchor + 1)
            } else {
                Subcell::from_cell(anchor) - Subcell::QUANTUM
            };
            if blockage.free() {
                body.y = next_y;
                done[1] += 1;
                continue;
            }
            if dir > 0 {
                let head_row = cand.1 + body.shape.h() - 1;
                if !corrected
                    && let Some(contact) = blockage.single_head_hit(head_row)
                    && let Some((corrected_x, side)) =
                        ceiling_correct(world, body, next_y, body.vx, own)
                {
                    let (vx0, vy0) = (body.vx, body.vy);
                    let removed = vy0.scaled_by(CEILING_VY_DAMP);
                    body.vy = vy0 - removed;
                    let redirect = removed.scaled_by(CEILING_VX_REDIRECT).min(body.vy);
                    body.vx += redirect.times(side);
                    result.record_blocked(&[contact], vx0 - body.vx, vy0 - body.vy);
                    result.hit_ceiling = true;
                    result.corrected_ceiling = true;
                    body.x = corrected_x;
                    body.y = next_y;
                    done[1] += 1;
                    target_y = body.y + (target_y - body.y).scaled_by(0.5);
                    distance[1] = done[1] + quanta_toward(body.y, target_y);
                    y_state.tail = target_y - quantum_end(body.y, dir, distance[1] - done[1]);
                    corrected = true;
                    continue;
                }
                result.hit_ceiling = true;
            }
            let e = solids_bounce(world, &blockage.solids);
            let after = resolve_axis(body.vy, e);
            result.record_blocked(&blockage.solids, Subcell::ZERO, body.vy - after);
            let leading = if dir > 0 {
                cand.1 + body.shape.h() - 1
            } else {
                cand.1
            };
            let row = blockage.near_row(dir).unwrap_or(leading);
            body.y = snapped(dir, row, body.shape.h(), body.shape.h() / 2);
            if dir < 0 && after <= Subcell::ZERO {
                body.on_ground = true;
            }
            body.vy = after;
            y_state.blocked = true;
            done[1] = distance[1];
        }
    }

    if !x_state.blocked && x_state.tail != Subcell::ZERO {
        loop {
            let next_x = body.x + x_state.tail;
            if next_x.floor_cell() == body.x.floor_cell() {
                body.x = next_x;
                break;
            }
            let cand = body.shape.origin(next_x, body.y);
            let step_dir = if x_state.dir > 0 {
                Dir::PosX
            } else {
                Dir::NegX
            };
            let blockage = body.shape.step_blockage(world, own, cand, step_dir);
            if blockage.free() {
                body.x = next_x;
                break;
            }
            if try_step_up(world, body, &blockage, own) {
                climbed = true;
                continue;
            }
            let e = solids_bounce(world, &blockage.solids);
            let after = resolve_axis(body.vx, e);
            result.record_blocked(&blockage.solids, body.vx - after, Subcell::ZERO);
            body.vx = after;
            break;
        }
    }

    if !y_state.blocked && y_state.tail != Subcell::ZERO {
        let next_y = body.y + y_state.tail;
        if next_y.floor_cell() == body.y.floor_cell() {
            body.y = next_y;
        } else {
            let cand = body.shape.origin(body.x, next_y);
            let step_dir = if y_state.dir > 0 {
                Dir::PosY
            } else {
                Dir::NegY
            };
            let blockage = body.shape.step_blockage(world, own, cand, step_dir);
            if blockage.free() {
                body.y = next_y;
            } else {
                let e = solids_bounce(world, &blockage.solids);
                let after = resolve_axis(body.vy, e);
                result.record_blocked(&blockage.solids, Subcell::ZERO, body.vy - after);
                if y_state.dir > 0 {
                    result.hit_ceiling = true;
                }
                body.vy = after;
            }
        }
    }

    if was_grounded
        && body.vy <= Subcell::ZERO
        && submersion < SNAP_DOWN_MAX_SUBMERSION
        && !body.shape.supported_at(world, own, body.origin())
    {
        for step in 1..=STEP_DOWN_CELLS {
            let next_y = body.y - Subcell::from_cells(step as f32);
            let cand = body.shape.origin(body.x, next_y);
            if body.shape.blocked_at(world, own, body.origin(), cand) {
                break;
            }
            if body.shape.supported_at(world, own, cand) {
                body.y = next_y;
                body.on_ground = true;
                break;
            }
        }
    }

    if climbed && was_grounded && body.vy <= Subcell::ZERO {
        body.on_ground = true;
    }

    if body.vy <= Subcell::ZERO
        && !body.on_ground
        && body.shape.supported_at(world, own, body.origin())
    {
        body.on_ground = true;
    }
    result
}
