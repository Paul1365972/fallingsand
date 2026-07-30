use crate::player::Players;
use fallingsand_core::{
    CellOffset, Chunk, ChunkPos, FOG_CHUNK_SIDE, FOG_TEXEL_BITS, FOG_TEXEL_CELLS, FogMask, FogPos,
    MaterialId, content,
};
use fallingsand_protocol::PlayerId;
use fallingsand_sim::CellWorld;
use fallingsand_sim::body::Bodies;
use rustc_hash::FxHashMap;

pub const REVEAL_RADIUS_CELLS: i32 = 160;
const RADIUS: i32 = REVEAL_RADIUS_CELLS >> FOG_TEXEL_BITS;
const SIDE: i32 = 2 * RADIUS + 1;
const SIGHT_OPAQUE_CELLS: u32 = 8;

struct EyeMemo {
    eye: FogPos,
    sight_hash: u64,
}

struct View {
    eye: FogPos,
    sight: Vec<bool>,
    visible: Vec<bool>,
    seen: Option<(i32, i32, i32, i32)>,
}

impl View {
    fn new() -> Self {
        let area = (SIDE * SIDE) as usize;
        Self {
            eye: FogPos::new(0, 0),
            sight: vec![true; area],
            visible: vec![false; area],
            seen: None,
        }
    }

    fn aim(&mut self, eye: FogPos) {
        self.eye = eye;
        self.sight.fill(true);
        self.visible.fill(false);
        self.seen = None;
    }

    #[inline]
    fn offset(dx: i32, dy: i32) -> Option<usize> {
        let (x, y) = (RADIUS + dx, RADIUS + dy);
        (x >= 0 && y >= 0 && x < SIDE && y < SIDE).then(|| (y * SIDE + x) as usize)
    }

    #[inline]
    fn slot(&self, texel: FogPos) -> Option<usize> {
        Self::offset(texel.x - self.eye.x, texel.y - self.eye.y)
    }

    fn window(&self) -> (ChunkPos, ChunkPos) {
        (
            self.eye.translated(-RADIUS, -RADIUS).chunk(),
            self.eye.translated(RADIUS, RADIUS).chunk(),
        )
    }

    fn revealed(&self) -> Option<(ChunkPos, ChunkPos)> {
        self.seen.map(|(min_x, min_y, max_x, max_y)| {
            (
                self.eye.translated(min_x, min_y).chunk(),
                self.eye.translated(max_x, max_y).chunk(),
            )
        })
    }

    fn blocked(&self, dx: i32, dy: i32) -> bool {
        Self::offset(dx, dy).is_none_or(|slot| self.sight[slot])
    }

    fn mark(&mut self, dx: i32, dy: i32) {
        if dx * dx + dy * dy > RADIUS * RADIUS {
            return;
        }
        let Some(slot) = Self::offset(dx, dy) else {
            return;
        };
        self.visible[slot] = true;
        self.seen = Some(match self.seen {
            None => (dx, dy, dx, dy),
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(dx), min_y.min(dy), max_x.max(dx), max_y.max(dy))
            }
        });
    }
}

pub struct FogState {
    opaque: Vec<bool>,
    memos: FxHashMap<PlayerId, EyeMemo>,
    view: View,
}

impl Default for FogState {
    fn default() -> Self {
        Self::new()
    }
}

impl FogState {
    pub fn new() -> Self {
        Self {
            opaque: (0..content::MATERIAL_COUNT)
                .map(|index| opaque_material(MaterialId(index as u16)))
                .collect(),
            memos: FxHashMap::default(),
            view: View::new(),
        }
    }
}

fn chunk_span((min, max): (ChunkPos, ChunkPos)) -> impl Iterator<Item = ChunkPos> {
    (min.y..=max.y).flat_map(move |y| (min.x..=max.x).map(move |x| ChunkPos::new(x, y)))
}

fn opaque_material(material: MaterialId) -> bool {
    content::material(material)
        .colors
        .iter()
        .all(|color| color[3] == u8::MAX)
}

pub fn reveal(sim: &mut CellWorld, bodies: &Bodies, players: &Players, state: &mut FogState) {
    let eyes: Vec<(PlayerId, FogPos)> = players
        .iter()
        .filter(|(_, player)| player.is_alive())
        .map(|(&id, player)| (id, FogPos::of(player.view_anchor(bodies))))
        .collect();
    state
        .memos
        .retain(|id, _| eyes.iter().any(|(present, _)| present == id));
    for (id, eye) in eyes {
        reveal_from(sim, state, id, eye);
    }
}

fn reveal_from(sim: &mut CellWorld, state: &mut FogState, id: PlayerId, eye: FogPos) {
    state.view.eye = eye;
    let window = state.view.window();
    let mut hash = 0u64;
    for pos in chunk_span(window) {
        let rev = match sim.chunk_mut(pos) {
            Some(chunk) => {
                refresh_sight(chunk, &state.opaque);
                chunk.sight_rev()
            }
            None => u32::MAX,
        };
        hash = hash
            .wrapping_mul(0x0100_0000_01b3)
            .wrapping_add(u64::from(rev));
    }
    let memo = EyeMemo {
        eye,
        sight_hash: hash,
    };
    if state
        .memos
        .get(&id)
        .is_some_and(|seen| seen.eye == memo.eye && seen.sight_hash == memo.sight_hash)
    {
        return;
    }
    state.memos.insert(id, memo);

    state.view.aim(eye);
    for pos in chunk_span(window) {
        let Some(chunk) = sim.chunk(pos) else {
            continue;
        };
        for (index, texel) in FogPos::in_chunk(pos) {
            if let Some(slot) = state.view.slot(texel) {
                state.view.sight[slot] = chunk.sight().get(index);
            }
        }
    }

    cast(&mut state.view);

    let Some(revealed_span) = state.view.revealed() else {
        return;
    };
    for pos in chunk_span(revealed_span) {
        let Some(chunk) = sim.chunk_mut(pos) else {
            continue;
        };
        if chunk.fog().is_full() {
            continue;
        }
        let mut revealed = FogMask::EMPTY;
        for (index, texel) in FogPos::in_chunk(pos) {
            if state
                .view
                .slot(texel)
                .is_some_and(|slot| state.view.visible[slot])
            {
                revealed.set(index);
            }
        }
        chunk.reveal(&revealed);
    }
}

fn refresh_sight(chunk: &mut Chunk, opaque: &[bool]) {
    let rect = chunk.take_sight_dirty();
    if rect.is_empty() {
        return;
    }
    let mut sight = *chunk.sight();
    for ty in (rect.min_y >> FOG_TEXEL_BITS)..=(rect.max_y >> FOG_TEXEL_BITS) {
        for tx in (rect.min_x >> FOG_TEXEL_BITS)..=(rect.max_x >> FOG_TEXEL_BITS) {
            let index = ty as usize * FOG_CHUNK_SIDE + tx as usize;
            sight.assign(index, opaque_texel(chunk, tx, ty, opaque));
        }
    }
    chunk.store_sight(sight);
}

fn opaque_texel(chunk: &Chunk, tx: u8, ty: u8, opaque: &[bool]) -> bool {
    let cells = FOG_TEXEL_CELLS as u8;
    let mut count = 0;
    for cy in 0..cells {
        for cx in 0..cells {
            let cell = chunk.get(CellOffset::new(tx * cells + cx, ty * cells + cy));
            if cell.body_id().is_none() && opaque[cell.material.0 as usize] {
                count += 1;
            }
        }
    }
    count >= SIGHT_OPAQUE_CELLS
}

#[derive(Clone, Copy)]
struct Slope {
    num: i64,
    den: i64,
}

impl Slope {
    const fn new(num: i64, den: i64) -> Self {
        Self { num, den }
    }

    fn min_column(self, depth: i64) -> i64 {
        (2 * depth * self.num + self.den).div_euclid(2 * self.den)
    }

    fn max_column(self, depth: i64) -> i64 {
        -(self.den - 2 * depth * self.num).div_euclid(2 * self.den)
    }
}

fn transform(quadrant: u8, depth: i64, column: i64) -> (i32, i32) {
    let depth = depth as i32;
    let column = column as i32;
    match quadrant {
        0 => (column, depth),
        1 => (column, -depth),
        2 => (depth, column),
        _ => (-depth, column),
    }
}

fn cast(view: &mut View) {
    view.mark(0, 0);
    for quadrant in 0..4 {
        scan(view, quadrant, 1, Slope::new(-1, 1), Slope::new(1, 1));
    }
}

fn scan(view: &mut View, quadrant: u8, depth: i64, mut start: Slope, end: Slope) {
    if depth > i64::from(RADIUS) {
        return;
    }
    let mut previous_wall = true;
    for column in start.min_column(depth)..=end.max_column(depth) {
        let (dx, dy) = transform(quadrant, depth, column);
        let wall = view.blocked(dx, dy);
        let symmetric =
            column * start.den >= depth * start.num && column * end.den <= depth * end.num;
        if wall || symmetric {
            view.mark(dx, dy);
        }
        let edge = Slope::new(2 * column - 1, 2 * depth);
        if previous_wall && !wall {
            start = edge;
        }
        if !previous_wall && wall {
            scan(view, quadrant, depth + 1, start, edge);
        }
        previous_wall = wall;
    }
    if !previous_wall {
        scan(view, quadrant, depth + 1, start, end);
    }
}
