mod contact;
mod island;
mod rotation;
mod rounds;
mod state;

use crate::window::BodyImpulse;
use crate::world::CellWorld;
use fallingsand_core::{
    CARDINAL_NEIGHBORS, CHUNK_SIZE, Cell, CellOffset, CellPos, CellRect, ChunkPos, MaterialId,
    Phase, Q16, RegionPos, Subcell, content,
};
use rustc_hash::FxHashMap;
pub use state::Policy;

use state::{Body, Slot, bondable, capture, release};
use std::collections::VecDeque;

const SURFACE_PROBE: i32 = 64;

#[derive(Default)]
pub struct Bodies {
    bodies: Vec<Body>,
    by_id: FxHashMap<u32, usize>,
    pending: Vec<CellPos>,
    parked_seeds: FxHashMap<ChunkPos, Vec<CellPos>>,
    fractures: Vec<Fracture>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fracture {
    pub source: u32,
    pub anchor: CellPos,
    pub parts: Vec<u32>,
}

impl Bodies {
    pub fn rasters(&self) -> impl Iterator<Item = (u32, &[CellPos])> {
        self.bodies
            .iter()
            .map(|body| (body.id, body.raster.as_slice()))
    }

    pub fn unseat(&mut self, seeds: impl IntoIterator<Item = CellPos>) {
        self.pending.extend(seeds);
    }

    pub fn drain_fractures(&mut self) -> Vec<Fracture> {
        std::mem::take(&mut self.fractures)
    }

    pub fn spawn(
        &mut self,
        world: &mut CellWorld,
        id: u32,
        cells: &[(CellPos, MaterialId, u8)],
        policy: Policy,
    ) -> bool {
        let positions: rustc_hash::FxHashSet<CellPos> =
            cells.iter().map(|&(pos, _, _)| pos).collect();
        if cells.is_empty()
            || self.by_id.contains_key(&id)
            || positions.len() != cells.len()
            || cells.iter().any(|&(_, material, _)| !bondable(material))
            || !cells
                .iter()
                .all(|&(pos, _, _)| world.get_cell(pos).is_some_and(|cell| cell.is_air()))
        {
            return false;
        }
        for &(pos, material, shade) in cells {
            world.place(pos, material, shade);
        }
        let mut body = capture(world, id, cells.iter().map(|&(pos, _, _)| pos).collect());
        body.apply(policy);
        self.bodies.push(body);
        self.rebuild_index();
        true
    }

    pub fn cell(&self, id: u32) -> Option<CellPos> {
        self.by_id.get(&id).map(|&index| self.bodies[index].anchor)
    }

    pub fn velocity(&self, id: u32) -> Option<(Subcell, Subcell)> {
        let &index = self.by_id.get(&id)?;
        let body = &self.bodies[index];
        Some((Subcell::from_raw(body.vx), Subcell::from_raw(body.vy)))
    }

    pub fn drive(&mut self, id: u32, vx: Subcell, vy: Subcell) {
        if let Some(&index) = self.by_id.get(&id) {
            self.bodies[index].vx = vx.raw();
            self.bodies[index].vy = vy.raw();
        }
    }

    pub fn weight(&self, id: u32) -> Subcell {
        self.by_id.get(&id).map_or(Subcell::ZERO, |&index| {
            Subcell::from_raw(self.bodies[index].weight)
        })
    }

    pub fn touching(&self, world: &CellWorld, id: u32, dx: i32, dy: i32) -> bool {
        self.by_id.get(&id).is_some_and(|&index| {
            let body = &self.bodies[index];
            body.raster.iter().any(|&pos| {
                world.get_cell(pos.translated(dx, dy)).is_some_and(|cell| {
                    cell.body_id() != Some(body.id)
                        && matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
                })
            })
        })
    }

    pub fn supported(&self, world: &CellWorld, id: u32) -> bool {
        self.touching(world, id, 0, -1)
    }

    pub fn bounds(&self, id: u32) -> Option<CellRect> {
        let &index = self.by_id.get(&id)?;
        let body = &self.bodies[index];
        let mut min = (i32::MAX, i32::MAX);
        let mut max = (i32::MIN, i32::MIN);
        for &pos in &body.raster {
            min = (min.0.min(pos.x), min.1.min(pos.y));
            max = (max.0.max(pos.x), max.1.max(pos.y));
        }
        Some(CellRect::new(
            CellPos::new(min.0, min.1),
            CellPos::new(max.0, max.1),
        ))
    }

    pub fn support_traction(&self, world: &CellWorld, id: u32) -> Option<f32> {
        let &index = self.by_id.get(&id)?;
        let body = &self.bodies[index];
        let mut best: Option<f32> = None;
        for &pos in &body.raster {
            let below = pos.translated(0, -1);
            let Some(cell) = world.get_cell(below) else {
                continue;
            };
            if cell.body_id() == Some(body.id)
                || !matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
            {
                continue;
            }
            let traction = content::material(cell.material).traction;
            best = Some(best.map_or(traction, |found: f32| found.max(traction)));
        }
        best
    }

    pub fn submersion(&self, world: &CellWorld, id: u32) -> f32 {
        let Some(&index) = self.by_id.get(&id) else {
            return 0.0;
        };
        let body = &self.bodies[index];
        let mut liquid = 0u32;
        let mut total = 0u32;
        for &pos in &body.raster {
            for (dx, dy) in CARDINAL_NEIGHBORS {
                let Some(cell) = world.get_cell(pos.translated(dx, dy)) else {
                    continue;
                };
                if cell.body_id().is_some() {
                    continue;
                }
                total += 1;
                if content::phase(cell.material) == Phase::Liquid {
                    liquid += 1;
                }
            }
        }
        if total == 0 {
            return 0.0;
        }
        liquid as f32 / total as f32
    }

    pub fn repaint(&mut self, world: &mut CellWorld, id: u32, shade: impl Fn(i32, i32) -> u8) {
        let Some(&index) = self.by_id.get(&id) else {
            return;
        };
        let body = &self.bodies[index];
        let corner = body.slots.iter().fold((i32::MAX, i32::MAX), |min, slot| {
            (min.0.min(slot.local.0), min.1.min(slot.local.1))
        });
        for (slot, &pos) in body.slots.iter().zip(&body.raster) {
            let Some(cell) = world.get_cell(pos) else {
                continue;
            };
            if cell.body_id() != Some(body.id) {
                continue;
            }
            let mut next = Cell::new(
                cell.material,
                shade(slot.local.0 - corner.0, slot.local.1 - corner.1),
            );
            next.set_body(body.id);
            if world.get_cell(pos) != Some(next) {
                world.set(pos, next);
            }
        }
    }

    pub fn reshape(
        &mut self,
        world: &mut CellWorld,
        id: u32,
        cells: &[(CellPos, MaterialId, u8)],
    ) -> bool {
        let Some(&index) = self.by_id.get(&id) else {
            return false;
        };
        let claimed: rustc_hash::FxHashSet<CellPos> =
            cells.iter().map(|&(pos, _, _)| pos).collect();
        if cells.is_empty()
            || claimed.len() != cells.len()
            || cells.iter().any(|&(_, material, _)| !bondable(material))
        {
            return false;
        }
        let held: rustc_hash::FxHashSet<CellPos> =
            self.bodies[index].raster.iter().copied().collect();
        let mut evicted = Vec::new();
        for &pos in &claimed {
            if held.contains(&pos) {
                continue;
            }
            let Some(cell) = world.get_cell(pos) else {
                return false;
            };
            if cell.body_id().is_some() {
                return false;
            }
            match content::phase(cell.material) {
                Phase::Solid | Phase::Powder => return false,
                Phase::Empty => {}
                Phase::Liquid | Phase::Gas => evicted.push((pos, cell)),
            }
        }
        let mut vacated: Vec<CellPos> = held.difference(&claimed).copied().collect();
        vacated.sort_unstable_by_key(|pos| (pos.y, pos.x));
        evicted.sort_unstable_by_key(|&(pos, cell)| {
            (
                std::cmp::Reverse(content::density_milli(cell.material)),
                pos.y,
                pos.x,
            )
        });
        let mut homes = Vec::with_capacity(evicted.len());
        let mut taken = vacated.clone();
        let mut receptacles = vacated.iter().copied();
        for &(pos, _) in &evicted {
            match receptacles.next() {
                Some(target) => homes.push(target),
                None => {
                    let Some(spot) = surface_spot(world, &claimed, &taken, pos) else {
                        return false;
                    };
                    taken.push(spot);
                    homes.push(spot);
                }
            }
        }
        let leftovers: Vec<CellPos> = receptacles.collect();

        for &pos in &held {
            if world
                .get_cell(pos)
                .is_some_and(|cell| cell.body_id() == Some(id))
            {
                world.set(pos, Cell::AIR);
            }
        }
        for (&(_, cell), &target) in evicted.iter().zip(&homes) {
            world.set(target, cell);
        }
        for target in leftovers {
            world.set(target, Cell::AIR);
        }
        for &(pos, material, shade) in cells {
            world.set(pos, Cell::new(material, shade));
        }

        let body = self.bodies.remove(index);
        let mut next = capture(world, id, cells.iter().map(|&(pos, _, _)| pos).collect());
        next.freedoms = body.freedoms;
        next.settles = body.settles;
        next.assists = body.assists;
        next.parked = body.parked;
        next.weight = body.weight;
        next.vx = body.vx;
        next.vy = body.vy;
        next.spin = body.spin;
        next.acc_x = body.acc_x;
        next.acc_y = body.acc_y;
        next.acc_turn = body.acc_turn;
        self.bodies.push(next);
        self.rebuild_index();
        true
    }

    pub fn recast(&mut self, world: &mut CellWorld, id: u32, material: MaterialId) {
        let Some(&index) = self.by_id.get(&id) else {
            return;
        };
        let body = &mut self.bodies[index];
        for (slot, &pos) in body.slots.iter_mut().zip(&body.raster) {
            let Some(cell) = world.get_cell(pos) else {
                continue;
            };
            if cell.body_id() != Some(body.id) {
                continue;
            }
            let mut next = Cell::new(material, cell.shade);
            next.set_body(body.id);
            world.set(pos, next);
            slot.material = material;
        }
        body.rebase_reference_pose();
    }

    pub fn die(&mut self, id: u32) {
        if let Some(&index) = self.by_id.get(&id) {
            self.bodies[index].apply(Policy::DEBRIS);
        }
    }

    pub fn despawn(&mut self, world: &mut CellWorld, id: u32) {
        let Some(&index) = self.by_id.get(&id) else {
            return;
        };
        let body = self.bodies.remove(index);
        for &pos in &body.raster {
            if world
                .get_cell(pos)
                .is_some_and(|cell| cell.body_id() == Some(body.id))
            {
                world.set(pos, Cell::AIR);
            }
        }
        self.rebuild_index();
    }

    pub fn wake_chunks(&mut self, chunks: impl IntoIterator<Item = ChunkPos>) {
        for chunk in chunks {
            if let Some(seeds) = self.parked_seeds.remove(&chunk) {
                self.pending.extend(seeds);
            }
        }
    }

    pub fn unseat_exposed(
        &mut self,
        world: &CellWorld,
        chunks: impl IntoIterator<Item = ChunkPos>,
    ) {
        for chunk_pos in chunks {
            let Some(chunk) = world.chunk(chunk_pos) else {
                continue;
            };
            let base = chunk_pos.base_cell();
            for y in 0..CHUNK_SIZE as i32 {
                for x in 0..CHUNK_SIZE as i32 {
                    let cell = chunk.get(CellOffset::new(x as u8, y as u8));
                    if !bondable(cell.material) {
                        continue;
                    }
                    let pos = base.translated(x, y);
                    let exposed = CARDINAL_NEIGHBORS.iter().any(|&(dx, dy)| {
                        world.get_cell(pos.translated(dx, dy)).is_none_or(|near| {
                            matches!(
                                content::phase(near.material),
                                Phase::Empty | Phase::Liquid | Phase::Gas
                            )
                        })
                    });
                    if exposed {
                        self.pending.push(pos);
                    }
                }
            }
        }
    }

    pub fn integrate(
        &mut self,
        world: &mut CellWorld,
        impulses: &[BodyImpulse],
        simulated: &dyn Fn(ChunkPos) -> bool,
        allocate: &mut dyn FnMut() -> u32,
    ) {
        for body in &mut self.bodies {
            if body.parked {
                body.parked = island_blocker(world, simulated, &body.raster).is_some();
            }
        }
        self.reconcile(world, allocate);
        self.detach(world, simulated, allocate);
        self.rebuild_index();

        for impulse in impulses {
            if let Some(&index) = self.by_id.get(&impulse.id) {
                let body = &mut self.bodies[index];
                if body.parked {
                    continue;
                }
                let com = body.com();
                body.apply_impulse(com, impulse.pos, impulse.jx, impulse.jy);
            }
        }
        for body in &mut self.bodies {
            if !body.parked {
                rounds::integrate_forces(world, body);
            }
        }
    }

    pub fn advance(&mut self, world: &mut CellWorld, simulated: &dyn Fn(ChunkPos) -> bool) {
        let mut cells = FxHashMap::default();
        rounds::run_rounds(world, &mut self.bodies, &self.by_id, &simulated, &mut cells);

        let mut yielded: Vec<_> = cells.into_iter().collect();
        yielded.sort_unstable_by_key(|(pos, _)| (pos.y, pos.x));
        for (pos, state) in yielded {
            if (state.vx, state.vy) == (state.start_vx, state.start_vy) {
                continue;
            }
            let Some(mut cell) = world.get_cell(pos) else {
                continue;
            };
            if cell.body_id().is_some() {
                continue;
            }
            cell.set_vel(state.vx as i32, state.vy as i32);
            world.set(pos, cell);
        }

        for body in &mut self.bodies {
            rounds::carriage(world, body);
        }
        let mut index = 0;
        while index < self.bodies.len() {
            if rounds::try_settle(world, &mut self.bodies[index]) {
                let body = self.bodies.remove(index);
                settle_body(world, &body);
            } else {
                index += 1;
            }
        }
        self.rebuild_index();
    }

    pub fn settle_regions(&mut self, world: &mut CellWorld, regions: &[RegionPos]) {
        let mut index = 0;
        while index < self.bodies.len() {
            let crossing = self.bodies[index]
                .raster
                .iter()
                .any(|pos| regions.iter().any(|&region| pos.chunk().region() == region));
            if crossing {
                let body = self.bodies.remove(index);
                settle_body(world, &body);
            } else {
                index += 1;
            }
        }
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.by_id.clear();
        for (index, body) in self.bodies.iter().enumerate() {
            self.by_id.insert(body.id, index);
        }
    }

    fn reconcile(&mut self, world: &mut CellWorld, allocate: &mut dyn FnMut() -> u32) {
        let mut index = 0;
        while index < self.bodies.len() {
            match reconcile_body(world, &self.bodies[index], allocate) {
                Reconciled::Intact => index += 1,
                Reconciled::Parked => {
                    self.bodies[index].parked = true;
                    index += 1;
                }
                Reconciled::Gone => {
                    let body = &self.bodies[index];
                    self.fractures.push(Fracture {
                        source: body.id,
                        anchor: body.anchor,
                        parts: Vec::new(),
                    });
                    self.bodies.remove(index);
                }
                Reconciled::Parts(mut parts) => {
                    let retained = self.bodies[index].id;
                    self.fractures.push(Fracture {
                        source: retained,
                        anchor: self.bodies[index].anchor,
                        parts: parts.iter().map(|part| part.id).collect(),
                    });
                    self.bodies.remove(index);
                    let mut offset = 0;
                    for part in parts.drain(..) {
                        if part.id == retained {
                            self.bodies.insert(index, part);
                            offset = 1;
                        } else {
                            for &pos in &part.raster {
                                if let Some(mut cell) = world.get_cell(pos)
                                    && cell.body_id() == Some(retained)
                                {
                                    cell.set_body(part.id);
                                    world.set(pos, cell);
                                }
                            }
                            self.bodies.push(part);
                        }
                    }
                    index += offset;
                }
            }
        }
    }

    fn detach(
        &mut self,
        world: &mut CellWorld,
        simulated: &dyn Fn(ChunkPos) -> bool,
        allocate: &mut dyn FnMut() -> u32,
    ) {
        let mut seeds = std::mem::take(&mut self.pending);
        seeds.sort_unstable_by_key(|pos| (pos.y, pos.x));
        seeds.dedup();
        let mut scanned = rustc_hash::FxHashSet::default();
        for seed in seeds {
            let mut candidates = vec![seed];
            candidates.extend(
                CARDINAL_NEIGHBORS
                    .iter()
                    .map(|&(dx, dy)| seed.translated(dx, dy)),
            );
            for candidate in candidates {
                if scanned.contains(&candidate) {
                    continue;
                }
                if world.get_cell(candidate).is_none() {
                    self.parked_seeds
                        .entry(candidate.chunk())
                        .or_default()
                        .push(candidate);
                    continue;
                }
                let Some(island) = island::detect_detached_island(world, candidate, &mut scanned)
                else {
                    continue;
                };
                match island_blocker(world, simulated, &island) {
                    None => {
                        let id = allocate();
                        self.bodies.push(capture(world, id, island));
                    }
                    Some(chunk) => {
                        self.parked_seeds.entry(chunk).or_default().push(candidate);
                    }
                }
            }
        }
    }
}

enum Reconciled {
    Intact,
    Parked,
    Gone,
    Parts(Vec<Body>),
}

fn reconcile_body(
    world: &mut CellWorld,
    body: &Body,
    allocate: &mut dyn FnMut() -> u32,
) -> Reconciled {
    let mut changed = false;
    let mut survivors: Vec<Slot> = Vec::with_capacity(body.slots.len());
    let mut positions: Vec<CellPos> = Vec::with_capacity(body.raster.len());
    for (slot, &pos) in body.slots.iter().zip(&body.raster) {
        let Some(cell) = world.get_cell(pos) else {
            return Reconciled::Parked;
        };
        if cell.body_id() != Some(body.id) {
            changed = true;
            continue;
        }
        if !bondable(cell.material) {
            release(world, body, pos);
            changed = true;
            continue;
        }
        if cell.material != slot.material {
            changed = true;
        }
        survivors.push(Slot {
            local: slot.local,
            material: cell.material,
        });
        positions.push(pos);
    }
    if survivors.is_empty() {
        return Reconciled::Gone;
    }
    if !changed {
        return Reconciled::Intact;
    }

    let by_local: FxHashMap<(i32, i32), u32> = survivors
        .iter()
        .enumerate()
        .map(|(slot, entry)| (entry.local, slot as u32))
        .collect();
    const UNCLAIMED: u32 = u32::MAX;
    let mut part_of = vec![UNCLAIMED; survivors.len()];
    let mut parts = 0u32;
    let mut queue = VecDeque::new();
    for start in 0..survivors.len() {
        if part_of[start] != UNCLAIMED {
            continue;
        }
        part_of[start] = parts;
        queue.push_back(start as u32);
        while let Some(current) = queue.pop_front() {
            let entry = survivors[current as usize];
            for (dx, dy) in CARDINAL_NEIGHBORS {
                let Some(&next) = by_local.get(&(entry.local.0 + dx, entry.local.1 + dy)) else {
                    continue;
                };
                if part_of[next as usize] == UNCLAIMED
                    && content::bonds(entry.material, survivors[next as usize].material)
                {
                    part_of[next as usize] = parts;
                    queue.push_back(next);
                }
            }
        }
        parts += 1;
    }

    let mut out = Vec::with_capacity(parts as usize);
    for part in 0..parts {
        let mut slots = Vec::new();
        let mut raster = Vec::new();
        for (slot, &assigned) in part_of.iter().enumerate() {
            if assigned == part {
                slots.push(survivors[slot]);
                raster.push(positions[slot]);
            }
        }
        let id = if part == 0 { body.id } else { allocate() };
        out.push(derive_part(body, body.com(), id, slots, raster));
    }
    Reconciled::Parts(out)
}

fn derive_part(
    source: &Body,
    com: (i64, i64),
    id: u32,
    slots: Vec<Slot>,
    raster: Vec<CellPos>,
) -> Body {
    let mut body = Body {
        id,
        slots,
        raster,
        anchor: source.anchor,
        step: source.step,
        vx: source.vx,
        vy: source.vy,
        spin: source.spin,
        acc_x: if id == source.id { source.acc_x } else { 0 },
        acc_y: if id == source.id { source.acc_y } else { 0 },
        acc_turn: if id == source.id { source.acc_turn } else { 0 },
        mass: 0,
        local_com: (0, 0),
        moment: 0,
        radius: 0,
        restitution: Q16::from_raw(0),
        friction: Q16::from_raw(0),
        weight: source.weight,
        freedoms: source.freedoms,
        settles: source.settles,
        assists: source.assists,
        parked: false,
    };
    body.rebase_reference_pose();
    let part_com = body.com();
    body.vx = source.vx - source.spin.speed_at(part_com.1 - com.1);
    body.vy = source.vy + source.spin.speed_at(part_com.0 - com.0);
    body
}

fn surface_spot(
    world: &CellWorld,
    claimed: &rustc_hash::FxHashSet<CellPos>,
    taken: &[CellPos],
    from: CellPos,
) -> Option<CellPos> {
    let mut pos = from;
    for _ in 0..SURFACE_PROBE {
        pos = pos.translated(0, 1);
        if claimed.contains(&pos) {
            continue;
        }
        let phase = world
            .get_cell(pos)
            .filter(|cell| cell.body_id().is_none())
            .map(|cell| content::phase(cell.material))?;
        match phase {
            Phase::Empty if !taken.contains(&pos) => return Some(pos),
            Phase::Empty | Phase::Liquid | Phase::Gas => {}
            Phase::Solid | Phase::Powder => return None,
        }
    }
    None
}

fn settle_body(world: &mut CellWorld, body: &Body) {
    for &pos in &body.raster {
        let Some(mut cell) = world.get_cell(pos) else {
            continue;
        };
        if cell.body_id() != Some(body.id) {
            continue;
        }
        cell.clear_body();
        cell.set_vel(0, 0);
        world.set(pos, cell);
    }
}

fn island_blocker(
    world: &CellWorld,
    simulated: &dyn Fn(ChunkPos) -> bool,
    island: &[CellPos],
) -> Option<ChunkPos> {
    let min_x = island.iter().map(|pos| pos.x).min().unwrap();
    let max_x = island.iter().map(|pos| pos.x).max().unwrap();
    let min_y = island.iter().map(|pos| pos.y).min().unwrap();
    let max_y = island.iter().map(|pos| pos.y).max().unwrap();
    let min = CellPos::new(min_x - 1, min_y - 1).chunk();
    let max = CellPos::new(max_x + 1, max_y + 1).chunk();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let pos = ChunkPos::new(x, y);
            if world.chunk(pos).is_none() || !simulated(pos) {
                return Some(pos);
            }
        }
    }
    None
}
