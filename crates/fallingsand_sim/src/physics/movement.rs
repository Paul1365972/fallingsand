use super::{
    Actor, BOUNCE_MIN_SPEED, CellSource, Obstacle, OwnCells, footprint_at, rect_blocked,
    supported_at, walk_footprint,
};
use fallingsand_core::content;
use fallingsand_core::{CellPos, Subcell};
use std::ops::ControlFlow;

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
    pub dvx: f32,
    pub dvy: f32,
}

#[derive(Debug, Default)]
pub struct MoveResult {
    pub blocked: Vec<Blocked>,
    pub hit_ceiling: bool,
    pub(super) corrected_ceiling: bool,
}

impl MoveResult {
    fn record_blocked(&mut self, solids: &[CellPos], dvx: f32, dvy: f32) {
        if solids.is_empty() {
            return;
        }
        let share = 1.0 / solids.len() as f32;
        for &pos in solids {
            self.blocked.push(Blocked {
                pos,
                dvx: dvx * share,
                dvy: dvy * share,
            });
        }
    }
}

struct Blockage {
    solid: bool,
    unloaded: bool,
    solids: Vec<CellPos>,
}

impl Blockage {
    fn free(&self) -> bool {
        !self.solid
    }

    fn single_head_hit(&self, head_row: i32) -> Option<CellPos> {
        if self.unloaded {
            return None;
        }
        match self.solids.as_slice() {
            [pos] if pos.y == head_row => Some(*pos),
            _ => None,
        }
    }

    fn step_top(&self) -> Option<i32> {
        self.solids.iter().map(|pos| pos.y).max()
    }

    fn near_col(&self, dir: i32) -> Option<i32> {
        let cols = self.solids.iter().map(|pos| pos.x);
        if dir > 0 { cols.min() } else { cols.max() }
    }

    fn near_row(&self, dir: i32) -> Option<i32> {
        let rows = self.solids.iter().map(|pos| pos.y);
        if dir > 0 { rows.min() } else { rows.max() }
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
            e = e.max(content::material(cell.material).surface_bounce);
        }
    }
    e
}

fn passage<W: CellSource>(
    world: &W,
    body: &Actor,
    cx: Subcell,
    cy: Subcell,
    own: OwnCells,
) -> Blockage {
    let mut blockage = Blockage {
        solid: false,
        unloaded: false,
        solids: Vec::new(),
    };
    walk_footprint(world, body, cx, cy, own, |obstacle| {
        blockage.solid = true;
        match obstacle {
            Obstacle::Unloaded => blockage.unloaded = true,
            Obstacle::Solid(pos) => blockage.solids.push(pos),
        }
        ControlFlow::Continue(())
    });
    blockage
}

fn try_step_up<W: CellSource>(
    world: &W,
    body: &mut Actor,
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
    if rect_blocked(world, body, body.x, body.y + rise_needed, own) {
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
    body: &Actor,
    next_y: Subcell,
    vx: Subcell,
    own: OwnCells,
) -> Option<(Subcell, i32)> {
    let w = body.half_w.times(2).round_cells().max(1);
    let max_shift = w - 1;
    if max_shift < 1 {
        return None;
    }
    let head_row = footprint_at(body.x, next_y, body.half_w, body.half_h).y1;
    let sides: [i32; 2] = if vx > Subcell::ZERO { [1, -1] } else { [-1, 1] };
    for side in sides {
        for step in 1..=max_shift {
            let cand_x = body.x + Subcell::from_cells((side * step) as f32);
            if !passage(world, body, cand_x, body.y, own).free() {
                break;
            }
            let up = passage(world, body, cand_x, next_y, own);
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

pub fn move_body<W: CellSource>(
    world: &W,
    body: &mut Actor,
    submersion: f32,
    own: OwnCells,
) -> MoveResult {
    let mut result = MoveResult::default();
    let was_grounded = body.on_ground;
    body.on_ground = false;
    let w = body.half_w.times(2).round_cells().max(1);
    let h = body.half_h.times(2).round_cells().max(1);
    let (w_left, w_right) = (w / 2, w - 1 - w / 2);
    let (h_down, h_up) = (h / 2, h - 1 - h / 2);
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

    let mut climbed = false;
    if remaining_x != Subcell::ZERO {
        let dir = if remaining_x > Subcell::ZERO {
            1i32
        } else {
            -1
        };
        let target = body.x + remaining_x;
        let mut col = if dir > 0 {
            body.x.floor_cell() + w_right
        } else {
            body.x.floor_cell() - w_left
        };
        loop {
            let next_col = col + dir;
            let next_x = if dir > 0 {
                Subcell::from_cell(next_col - w_right)
            } else {
                Subcell::from_cell(next_col + w_left + 1) - Subcell::QUANTUM
            };
            let overshoots = if dir > 0 {
                next_x >= target
            } else {
                next_x <= target
            };
            if overshoots {
                let blockage = passage(world, body, target, body.y, own);
                if blockage.free() {
                    body.x = target;
                    break;
                }
                if try_step_up(world, body, &blockage, own) {
                    climbed = true;
                    continue;
                }
                let e = solids_bounce(world, &blockage.solids);
                let after = resolve_axis(body.vx, e);
                result.record_blocked(
                    &blockage.solids,
                    (body.vx - after).to_cells_per_second(),
                    0.0,
                );
                body.vx = after;
                break;
            }
            let blockage = passage(world, body, next_x, body.y, own);
            if blockage.free() {
                body.x = next_x;
                col = next_col;
                continue;
            }
            if try_step_up(world, body, &blockage, own) {
                climbed = true;
                continue;
            }
            let e = solids_bounce(world, &blockage.solids);
            let after = resolve_axis(body.vx, e);
            result.record_blocked(
                &blockage.solids,
                (body.vx - after).to_cells_per_second(),
                0.0,
            );
            let snap = blockage.near_col(dir);
            body.x = match snap {
                Some(near) if dir > 0 => Subcell::from_cell(near - w_right) - Subcell::QUANTUM,
                Some(near) => Subcell::from_cell(near + 1 + w_left),
                None if dir > 0 => Subcell::from_cell(next_col - w_right) - Subcell::QUANTUM,
                None => Subcell::from_cell(next_col + 1 + w_left),
            };
            body.vx = after;
            break;
        }
    }

    if climbed && was_grounded && body.vy <= Subcell::ZERO {
        body.on_ground = true;
    }

    if was_grounded
        && body.vy <= Subcell::ZERO
        && submersion < SNAP_DOWN_MAX_SUBMERSION
        && !supported_at(world, body, body.x, body.y, own)
    {
        for step in 1..=STEP_DOWN_CELLS {
            let next_y = body.y - Subcell::from_cells(step as f32);
            if rect_blocked(world, body, body.x, next_y, own) {
                break;
            }
            if supported_at(world, body, body.x, next_y, own) {
                body.y = next_y;
                body.on_ground = true;
                break;
            }
        }
    }

    if remaining_y != Subcell::ZERO {
        let dir = if remaining_y > Subcell::ZERO {
            1i32
        } else {
            -1
        };
        let mut target = body.y + remaining_y;
        let mut corrected = false;
        let mut row = if dir > 0 {
            body.y.floor_cell() + h_up
        } else {
            body.y.floor_cell() - h_down
        };
        loop {
            let next_row = row + dir;
            let next_y = if dir > 0 {
                Subcell::from_cell(next_row - h_up)
            } else {
                Subcell::from_cell(next_row + h_down + 1) - Subcell::QUANTUM
            };
            let overshoots = if dir > 0 {
                next_y >= target
            } else {
                next_y <= target
            };
            if overshoots {
                let blockage = passage(world, body, body.x, target, own);
                if blockage.free() {
                    body.y = target;
                } else {
                    let e = solids_bounce(world, &blockage.solids);
                    let after = resolve_axis(body.vy, e);
                    result.record_blocked(
                        &blockage.solids,
                        0.0,
                        (body.vy - after).to_cells_per_second(),
                    );
                    if dir > 0 {
                        result.hit_ceiling = true;
                    }
                    body.vy = after;
                }
                break;
            }
            let blockage = passage(world, body, body.x, next_y, own);
            if blockage.free() {
                body.y = next_y;
                row = next_row;
            } else {
                if dir > 0 {
                    let head_row = footprint_at(body.x, next_y, body.half_w, body.half_h).y1;
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
                        result.record_blocked(
                            &[contact],
                            (vx0 - body.vx).to_cells_per_second(),
                            (vy0 - body.vy).to_cells_per_second(),
                        );
                        result.hit_ceiling = true;
                        result.corrected_ceiling = true;
                        body.x = corrected_x;
                        body.y = next_y;
                        target = body.y + (target - body.y).scaled_by(0.5);
                        row = next_row;
                        corrected = true;
                        continue;
                    }
                    result.hit_ceiling = true;
                }
                let e = solids_bounce(world, &blockage.solids);
                let after = resolve_axis(body.vy, e);
                result.record_blocked(
                    &blockage.solids,
                    0.0,
                    (body.vy - after).to_cells_per_second(),
                );
                body.y = match blockage.near_row(dir) {
                    Some(near) if dir > 0 => Subcell::from_cell(near - h_up) - Subcell::QUANTUM,
                    Some(near) => Subcell::from_cell(near + 1 + h_down),
                    None if dir > 0 => Subcell::from_cell(next_row - h_up) - Subcell::QUANTUM,
                    None => Subcell::from_cell(next_row + 1 + h_down),
                };
                if dir < 0 && after <= Subcell::ZERO {
                    body.on_ground = true;
                }
                body.vy = after;
                break;
            }
        }
    }

    if body.vy <= Subcell::ZERO && !body.on_ground && supported_at(world, body, body.x, body.y, own)
    {
        body.on_ground = true;
    }
    result
}
