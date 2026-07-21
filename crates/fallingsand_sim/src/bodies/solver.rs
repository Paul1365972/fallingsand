use super::PixelBody;
use super::contact::Contact;

const FRICTION: f32 = 0.4;

#[derive(Clone, Copy)]
pub(super) struct PointState {
    pub(super) vx: f32,
    pub(super) vy: f32,
    pub(super) spin: f32,
    pub(super) inv_mass: f32,
    pub(super) inv_inertia: f32,
}

impl PointState {
    fn point_vel(&self, rx: f32, ry: f32) -> (f32, f32) {
        (self.vx - self.spin * ry, self.vy + self.spin * rx)
    }

    fn apply(&mut self, rx: f32, ry: f32, jx: f32, jy: f32) {
        self.vx += jx * self.inv_mass;
        self.vy += jy * self.inv_mass;
        self.spin += (rx * jy - ry * jx) * self.inv_inertia;
    }
}

pub(super) enum Partner {
    Static,
    Body { slot: usize, rx: f32, ry: f32 },
    Entity { slot: usize },
}

pub(super) struct SolverContact {
    pub(super) rx: f32,
    pub(super) ry: f32,
    pub(super) nx: f32,
    pub(super) ny: f32,
    pub(super) restitution: f32,
    pub(super) partner: Partner,
    pub(super) bias: f32,
    pub(super) acc_n: f32,
    pub(super) acc_t: f32,
}

#[derive(Default)]
pub(super) struct SolverScratch {
    pub(super) contacts: Vec<Contact>,
    pub(super) points: Vec<PointState>,
    pub(super) body_slots: Vec<(usize, usize)>,
    pub(super) solver: Vec<SolverContact>,
}

pub(super) fn state_of(body: &PixelBody) -> PointState {
    PointState {
        vx: body.vx.to_cells_per_second(),
        vy: body.vy.to_cells_per_second(),
        spin: body.spin,
        inv_mass: body.inv_mass,
        inv_inertia: body.inv_inertia,
    }
}

pub(super) fn slot_for(
    states: &mut Vec<PointState>,
    map: &mut Vec<(usize, usize)>,
    key: usize,
    make: impl FnOnce() -> PointState,
) -> usize {
    if let Some(&(_, slot)) = map.iter().find(|&&(k, _)| k == key) {
        return slot;
    }
    let slot = states.len();
    states.push(make());
    map.push((key, slot));
    slot
}

pub(super) fn relative_vn(
    points: &[PointState],
    entities: &[PointState],
    sc: &SolverContact,
) -> f32 {
    let (ax, ay) = points[0].point_vel(sc.rx, sc.ry);
    let (bx, by) = partner_point_vel(points, entities, sc);
    (ax - bx) * sc.nx + (ay - by) * sc.ny
}

fn partner_point_vel(
    points: &[PointState],
    entities: &[PointState],
    sc: &SolverContact,
) -> (f32, f32) {
    match sc.partner {
        Partner::Static => (0.0, 0.0),
        Partner::Body { slot, rx, ry } => points[slot].point_vel(rx, ry),
        Partner::Entity { slot } => (entities[slot].vx, entities[slot].vy),
    }
}

fn partner_effective(
    points: &[PointState],
    entities: &[PointState],
    sc: &SolverContact,
) -> (f32, f32, f32, f32) {
    match sc.partner {
        Partner::Static => (0.0, 0.0, 0.0, 0.0),
        Partner::Body { slot, rx, ry } => (points[slot].inv_mass, points[slot].inv_inertia, rx, ry),
        Partner::Entity { slot } => (entities[slot].inv_mass, 0.0, 0.0, 0.0),
    }
}

fn apply_partner(
    points: &mut [PointState],
    entities: &mut [PointState],
    sc: &SolverContact,
    jx: f32,
    jy: f32,
) {
    match sc.partner {
        Partner::Static => {}
        Partner::Body { slot, rx, ry } => points[slot].apply(rx, ry, -jx, -jy),
        Partner::Entity { slot } => {
            entities[slot].vx -= jx * entities[slot].inv_mass;
            entities[slot].vy -= jy * entities[slot].inv_mass;
        }
    }
}

pub(super) fn solve_contact(
    solver: &mut [SolverContact],
    points: &mut [PointState],
    entities: &mut [PointState],
    i: usize,
) {
    let (rx, ry, nx, ny, bias) = {
        let sc = &solver[i];
        (sc.rx, sc.ry, sc.nx, sc.ny, sc.bias)
    };
    let (other_inv_mass, other_inv_inertia, r2x, r2y) =
        partner_effective(points, entities, &solver[i]);

    let (ax, ay) = points[0].point_vel(rx, ry);
    let (bx, by) = partner_point_vel(points, entities, &solver[i]);
    let vn = (ax - bx) * nx + (ay - by) * ny;
    let r_cross_n = rx * ny - ry * nx;
    let r2_cross_n = r2x * ny - r2y * nx;
    let kn = points[0].inv_mass
        + other_inv_mass
        + r_cross_n * r_cross_n * points[0].inv_inertia
        + r2_cross_n * r2_cross_n * other_inv_inertia;
    let jn_target = -(vn + bias) / kn;
    let old_n = solver[i].acc_n;
    let new_n = (old_n + jn_target).max(0.0);
    let dn = new_n - old_n;
    solver[i].acc_n = new_n;
    points[0].apply(rx, ry, dn * nx, dn * ny);
    apply_partner(points, entities, &solver[i], dn * nx, dn * ny);

    let (tx, ty) = (-ny, nx);
    let (ax, ay) = points[0].point_vel(rx, ry);
    let (bx, by) = partner_point_vel(points, entities, &solver[i]);
    let vt = (ax - bx) * tx + (ay - by) * ty;
    let r_cross_t = rx * ty - ry * tx;
    let r2_cross_t = r2x * ty - r2y * tx;
    let kt = points[0].inv_mass
        + other_inv_mass
        + r_cross_t * r_cross_t * points[0].inv_inertia
        + r2_cross_t * r2_cross_t * other_inv_inertia;
    let jt_target = -vt / kt;
    let limit = FRICTION * solver[i].acc_n;
    let old_t = solver[i].acc_t;
    let new_t = (old_t + jt_target).clamp(-limit, limit);
    let dt = new_t - old_t;
    solver[i].acc_t = new_t;
    points[0].apply(rx, ry, dt * tx, dt * ty);
    apply_partner(points, entities, &solver[i], dt * tx, dt * ty);
}
