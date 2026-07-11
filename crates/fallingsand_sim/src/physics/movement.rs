use super::{
    Actor, BOUNCE_MIN_SPEED, CellSource, OwnCells, footprint_at, own_covers, rect_blocked,
    supported_at,
};
use fallingsand_core::{CellPos, Fixed, MaterialRegistry, Phase};

const LAUNCH_MIN_SPEED: Fixed = Fixed::vel_per_sec(80.0);
const LEDGE_LAUNCH_K: Fixed = Fixed::from_f32(0.35);
const STEP_UP_CELLS: i32 = 3;
const STEP_DOWN_CELLS: i32 = 3;
const UPWARD_CORNER_CORRECTION: i32 = 4;
const SNAP_DOWN_MAX_SUBMERSION: f32 = 0.5;
const CLIMB_COST: Fixed = Fixed::HALF;
const CLIMB_DRAIN: Fixed = Fixed::HALF;

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
    solids: Vec<CellPos>,
}

impl Blockage {
    fn free(&self) -> bool {
        !self.solid
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

fn resolve_axis(v: Fixed, e: f32) -> Fixed {
    if v.abs() > Fixed::vel_per_sec(BOUNCE_MIN_SPEED) {
        -v.mul(Fixed::from_f32(e))
    } else {
        Fixed::ZERO
    }
}

fn solids_bounce<W: CellSource>(world: &W, registry: &MaterialRegistry, solids: &[CellPos]) -> f32 {
    let mut e = 0.0f32;
    for &pos in solids {
        if let Some(cell) = world.cell_at(pos) {
            e = e.max(registry.get(cell.material).surface_bounce);
        }
    }
    e
}

fn passage<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
    cx: Fixed,
    cy: Fixed,
    own: OwnCells,
) -> Blockage {
    let cur = body.footprint();
    let next = footprint_at(cx, cy, body.half_w, body.half_h);
    let mut blockage = Blockage {
        solid: false,
        solids: Vec::new(),
    };
    for y in next.y0..=next.y1 {
        for x in next.x0..=next.x1 {
            let pos = CellPos::new(x, y);
            let Some(cell) = world.cell_at(pos) else {
                blockage.solid = true;
                continue;
            };
            if cur.contains(pos) || own_covers(own, pos) {
                continue;
            }
            if matches!(
                registry.get(cell.material).phase,
                Phase::Solid | Phase::Powder
            ) {
                blockage.solid = true;
                blockage.solids.push(pos);
            }
        }
    }
    blockage
}

fn try_step_up<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &mut Actor,
    blockage: &Blockage,
    own: OwnCells,
) -> bool {
    let Some(step_top) = blockage.step_top() else {
        return false;
    };
    let fp = body.footprint();
    let down = body.y.floor_cell() - fp.y0;
    let rise_needed = Fixed::from_cell(step_top + 1 + down) - body.y;
    if rise_needed <= Fixed::ZERO || rise_needed > Fixed::from_int(STEP_UP_CELLS) {
        return false;
    }
    if rect_blocked(world, registry, body, body.x, body.y + rise_needed, own) {
        return false;
    }
    body.y += rise_needed;
    body.climb_debt += rise_needed.mul(CLIMB_COST);
    if body.vx.abs() > LAUNCH_MIN_SPEED {
        body.vy = body.vy.max(body.vx.abs().mul(LEDGE_LAUNCH_K));
    }
    true
}

fn corner_correct<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
    next_y: Fixed,
    own: OwnCells,
) -> Option<Fixed> {
    let mut sides: Vec<i32> = Vec::new();
    if body.vx <= Fixed::ZERO {
        sides.push(-1);
    }
    if body.vx >= Fixed::ZERO {
        sides.push(1);
    }
    for side in sides {
        for off in 1..=UPWARD_CORNER_CORRECTION {
            let cand_x = body.x + Fixed::from_int(side * off);
            if passage(world, registry, body, cand_x, next_y, own).free() {
                return Some(cand_x);
            }
        }
    }
    None
}

pub fn move_body<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &mut Actor,
    submersion: f32,
    own: OwnCells,
) -> MoveResult {
    let mut result = MoveResult::default();
    let was_grounded = body.on_ground;
    body.on_ground = false;
    let w = body.half_w.mul_int(2).round_int().max(1);
    let h = body.half_h.mul_int(2).round_int().max(1);
    let (w_left, w_right) = (w / 2, w - 1 - w / 2);
    let (h_down, h_up) = (h / 2, h - 1 - h / 2);
    let mut remaining_x = body.vx;
    let remaining_y = body.vy;

    if remaining_x == Fixed::ZERO {
        body.climb_debt = Fixed::ZERO;
    } else {
        let drain = body.climb_debt.mul(CLIMB_DRAIN).min(remaining_x.abs());
        body.climb_debt -= drain;
        remaining_x = if remaining_x > Fixed::ZERO {
            remaining_x - drain
        } else {
            remaining_x + drain
        };
    }

    let mut climbed = false;
    if remaining_x != Fixed::ZERO {
        let dir = if remaining_x > Fixed::ZERO { 1i32 } else { -1 };
        let target = body.x + remaining_x;
        let mut col = if dir > 0 {
            body.x.floor_cell() + w_right
        } else {
            body.x.floor_cell() - w_left
        };
        loop {
            let next_col = col + dir;
            let next_x = if dir > 0 {
                Fixed::from_cell(next_col - w_right)
            } else {
                Fixed::from_cell(next_col + w_left + 1) - Fixed::SUBUNIT
            };
            let overshoots = if dir > 0 {
                next_x >= target
            } else {
                next_x <= target
            };
            if overshoots {
                let blockage = passage(world, registry, body, target, body.y, own);
                if blockage.free() {
                    body.x = target;
                    break;
                }
                if try_step_up(world, registry, body, &blockage, own) {
                    climbed = true;
                    continue;
                }
                let e = solids_bounce(world, registry, &blockage.solids);
                let after = resolve_axis(body.vx, e);
                result.record_blocked(&blockage.solids, (body.vx - after).vel_f32(), 0.0);
                body.vx = after;
                break;
            }
            let blockage = passage(world, registry, body, next_x, body.y, own);
            if blockage.free() {
                body.x = next_x;
                col = next_col;
                continue;
            }
            if try_step_up(world, registry, body, &blockage, own) {
                climbed = true;
                continue;
            }
            let e = solids_bounce(world, registry, &blockage.solids);
            let after = resolve_axis(body.vx, e);
            result.record_blocked(&blockage.solids, (body.vx - after).vel_f32(), 0.0);
            let snap = blockage.near_col(dir);
            body.x = match snap {
                Some(near) if dir > 0 => Fixed::from_cell(near - w_right) - Fixed::SUBUNIT,
                Some(near) => Fixed::from_cell(near + 1 + w_left),
                None if dir > 0 => Fixed::from_cell(next_col - w_right) - Fixed::SUBUNIT,
                None => Fixed::from_cell(next_col + 1 + w_left),
            };
            body.vx = after;
            break;
        }
    }

    if climbed && was_grounded && body.vy <= Fixed::ZERO {
        body.on_ground = true;
    }

    if was_grounded
        && body.vy <= Fixed::ZERO
        && submersion < SNAP_DOWN_MAX_SUBMERSION
        && !supported_at(world, registry, body, body.x, body.y, own)
    {
        for step in 1..=STEP_DOWN_CELLS {
            let next_y = body.y - Fixed::from_int(step);
            if rect_blocked(world, registry, body, body.x, next_y, own) {
                break;
            }
            if supported_at(world, registry, body, body.x, next_y, own) {
                body.y = next_y;
                body.on_ground = true;
                break;
            }
        }
    }

    if remaining_y != Fixed::ZERO {
        let dir = if remaining_y > Fixed::ZERO { 1i32 } else { -1 };
        let target = body.y + remaining_y;
        let mut row = if dir > 0 {
            body.y.floor_cell() + h_up
        } else {
            body.y.floor_cell() - h_down
        };
        loop {
            let next_row = row + dir;
            let next_y = if dir > 0 {
                Fixed::from_cell(next_row - h_up)
            } else {
                Fixed::from_cell(next_row + h_down + 1) - Fixed::SUBUNIT
            };
            let overshoots = if dir > 0 {
                next_y >= target
            } else {
                next_y <= target
            };
            if overshoots {
                let blockage = passage(world, registry, body, body.x, target, own);
                if blockage.free() {
                    body.y = target;
                } else {
                    let e = solids_bounce(world, registry, &blockage.solids);
                    let after = resolve_axis(body.vy, e);
                    result.record_blocked(&blockage.solids, 0.0, (body.vy - after).vel_f32());
                    if dir > 0 {
                        result.hit_ceiling = true;
                    }
                    body.vy = after;
                }
                break;
            }
            let blockage = passage(world, registry, body, body.x, next_y, own);
            if blockage.free() {
                body.y = next_y;
                row = next_row;
            } else {
                if dir > 0 {
                    if let Some(corrected_x) = corner_correct(world, registry, body, next_y, own) {
                        body.x = corrected_x;
                        body.y = next_y;
                        row = next_row;
                        continue;
                    }
                    result.hit_ceiling = true;
                }
                let e = solids_bounce(world, registry, &blockage.solids);
                let after = resolve_axis(body.vy, e);
                result.record_blocked(&blockage.solids, 0.0, (body.vy - after).vel_f32());
                body.y = match blockage.near_row(dir) {
                    Some(near) if dir > 0 => Fixed::from_cell(near - h_up) - Fixed::SUBUNIT,
                    Some(near) => Fixed::from_cell(near + 1 + h_down),
                    None if dir > 0 => Fixed::from_cell(next_row - h_up) - Fixed::SUBUNIT,
                    None => Fixed::from_cell(next_row + 1 + h_down),
                };
                if dir < 0 && after <= Fixed::ZERO {
                    body.on_ground = true;
                }
                body.vy = after;
                break;
            }
        }
    }

    if body.vy <= Fixed::ZERO
        && !body.on_ground
        && supported_at(world, registry, body, body.x, body.y, own)
    {
        body.on_ground = true;
    }
    result
}
