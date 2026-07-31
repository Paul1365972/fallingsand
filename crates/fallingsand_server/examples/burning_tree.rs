//! Physics harness: grow a tree on real terrain, set it alight, and audit what the
//! body solver does with the debris. Every observation comes from the real kernel.
//!
//! Usage: cargo run --example burning_tree -- [--scene tree|stack|gas|survey] [flags]

use fallingsand_core::{
    CHUNK_SIZE, CellPos, CellRect, ChunkPos, MaterialId, Phase, REGION_SIZE_CELLS, RegionPos,
    Subcell, content,
};
use fallingsand_sim::body::journal::{Event, Outcome, Peer, Verdict};
use fallingsand_sim::body::{Bodies, BodyState, Policy};
use fallingsand_sim::{CellWorld, Simulator};
use fallingsand_worldgen::WorldGenerator;
use rustc_hash::{FxHashMap, FxHashSet};

const SUBCELL: i64 = 1024;

fn main() {
    fallingsand_core::install_panic_hook();
    let args = Args::parse();
    match args.scene {
        Scene::Tree => scene_tree(&args),
        Scene::Stack => scene_stack(&args),
        Scene::Gas => scene_gas(&args),
        Scene::Survey => scene_survey(&args),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scene {
    Tree,
    Stack,
    Gas,
    Survey,
}

struct Args {
    scene: Scene,
    seed: u64,
    ticks: u64,
    origin: i32,
    events: bool,
    frames: u64,
    trace: Option<u32>,
}

impl Args {
    fn parse() -> Self {
        let mut args = Self {
            scene: Scene::Tree,
            seed: 42,
            ticks: 900,
            origin: 0,
            events: false,
            frames: 0,
            trace: None,
        };
        let mut iter = std::env::args().skip(1);
        while let Some(arg) = iter.next() {
            let mut value = || iter.next().expect("missing argument value");
            match arg.as_str() {
                "--scene" => {
                    args.scene = match value().as_str() {
                        "tree" => Scene::Tree,
                        "stack" => Scene::Stack,
                        "gas" => Scene::Gas,
                        "survey" => Scene::Survey,
                        other => panic!("unknown scene {other:?}"),
                    }
                }
                "--seed" => args.seed = value().parse().expect("seed"),
                "--ticks" => args.ticks = value().parse().expect("ticks"),
                "--origin" => args.origin = value().parse().expect("origin"),
                "--events" => args.events = true,
                "--frames" => args.frames = value().parse().expect("frames"),
                "--trace" => args.trace = Some(value().parse().expect("trace")),
                other => panic!("unknown argument {other:?}"),
            }
        }
        args
    }
}

struct Harness {
    world: CellWorld,
    simulator: Simulator,
    bodies: Bodies,
    simulated: FxHashSet<ChunkPos>,
    next_id: u32,
}

impl Harness {
    fn new() -> Self {
        let mut bodies = Bodies::default();
        bodies.journal().set_enabled(true);
        Self {
            world: CellWorld::new(),
            simulator: Simulator::new(),
            bodies,
            simulated: FxHashSet::default(),
            next_id: 0,
        }
    }

    fn load(&mut self, generator: &WorldGenerator, center: RegionPos, radius: i32) {
        let mut loaded = FxHashSet::default();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let pos = RegionPos::new(center.x + dx, center.y + dy);
                let region = generator.generate_region(pos);
                for ((_, chunk_pos), chunk) in pos.chunk_positions().zip(*region.into_chunks()) {
                    self.world.insert_chunk(chunk_pos, chunk);
                    loaded.insert(chunk_pos);
                }
            }
        }
        self.scope(loaded);
        let seeds: Vec<ChunkPos> = self.simulated.iter().copied().collect();
        self.bodies.unseat_exposed(&self.world, seeds);
    }

    fn load_flat(&mut self, radius: i32, ground: i32, material: MaterialId) {
        let mut loaded = FxHashSet::default();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let pos = ChunkPos::new(dx, dy);
                self.world.insert_chunk(pos, fallingsand_core::Chunk::new());
                loaded.insert(pos);
            }
        }
        self.scope(loaded);
        let span = radius * CHUNK_SIZE as i32;
        for y in -span..ground {
            for x in -span..span {
                self.world.set_material(CellPos::new(x, y), material);
            }
        }
    }

    fn scope(&mut self, loaded: FxHashSet<ChunkPos>) {
        self.simulated = loaded
            .iter()
            .copied()
            .filter(|pos| {
                (-1..=1).all(|dy| (-1..=1).all(|dx| loaded.contains(&pos.translated(dx, dy))))
            })
            .collect();
    }

    fn step(&mut self) -> Vec<Event> {
        let simulated = std::mem::take(&mut self.simulated);
        let scope = |pos: ChunkPos| simulated.contains(&pos);
        let (_, effects) = self
            .simulator
            .step_scoped(&mut self.world, &scope, &|_| true);
        let mut seeds = self.world.drain_unseated();
        seeds.extend(effects.unseated.iter().copied());
        self.bodies.unseat(seeds);
        let mut next = self.next_id;
        self.bodies
            .integrate(&mut self.world, &effects.impulses, &scope, &mut || {
                next += 1;
                next
            });
        self.next_id = next;
        self.bodies.drain_fractures();
        self.bodies.advance(&mut self.world, &scope);
        self.simulated = simulated;
        self.bodies.journal().drain()
    }

    fn census(&self) -> Census {
        let mut census = Census::default();
        for (_, chunk) in self.world.chunks() {
            for cell in chunk.cells() {
                if cell.is_air() {
                    continue;
                }
                census.cells += 1;
                match content::phase(cell.material) {
                    Phase::Solid => census.solid += 1,
                    Phase::Powder => census.powder += 1,
                    Phase::Liquid => census.liquid += 1,
                    Phase::Gas => census.gas += 1,
                    Phase::Empty => {}
                }
                if cell.body_id().is_some() {
                    census.owned += 1;
                }
            }
        }
        census
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct Census {
    cells: u64,
    solid: u64,
    powder: u64,
    liquid: u64,
    gas: u64,
    owned: u64,
}

#[derive(Default)]
struct Tracker {
    seen: FxHashMap<u32, Track>,
    findings: Vec<String>,
    tick: u64,
}

struct Track {
    born: u64,
    cells: usize,
    last_bounds: CellRect,
    still_since: u64,
    peak_speed: i64,
    peak_spin: i64,
    verdicts: FxHashMap<&'static str, u32>,
    alive: bool,
}

const FAST: i64 = 6 * SUBCELL;
const STORED: i64 = SUBCELL / 2;
const STUCK_TICKS: u64 = 120;

impl Tracker {
    fn observe(&mut self, tick: u64, bodies: &Bodies, events: &[Event]) {
        self.tick = tick;
        let mut live = FxHashSet::default();
        for state in bodies.states() {
            live.insert(state.id);
            let speed = magnitude(state.velocity);
            let track = self.seen.entry(state.id).or_insert_with(|| Track {
                born: tick,
                cells: state.cells,
                last_bounds: state.bounds,
                still_since: tick,
                peak_speed: 0,
                peak_spin: 0,
                verdicts: FxHashMap::default(),
                alive: true,
            });
            track.alive = true;
            track.cells = state.cells;
            if track.last_bounds != state.bounds {
                track.last_bounds = state.bounds;
                track.still_since = tick;
            }
            if speed > track.peak_speed {
                track.peak_speed = speed;
                if speed >= FAST {
                    self.findings.push(format!(
                        "t{tick} body {} ({} cells) reached {} cells/tick — {}",
                        state.id,
                        state.cells,
                        fixed(speed),
                        describe(&state)
                    ));
                }
            }
            track.peak_spin = track.peak_spin.max(state.spin.abs());
            if tick - track.still_since == STUCK_TICKS && state.settles {
                self.findings.push(format!(
                    "t{tick} body {} ({} cells) held position {STUCK_TICKS} ticks without settling — {}",
                    state.id,
                    state.cells,
                    describe(&state)
                ));
            }
            if tick - track.still_since >= STUCK_TICKS && speed > STORED && !state.parked {
                self.findings.push(format!(
                    "t{tick} body {} carries {} cells/tick while pinned in place — {}",
                    state.id,
                    fixed(speed),
                    describe(&state)
                ));
                track.still_since = tick;
            }
        }
        for (id, track) in self.seen.iter_mut() {
            if track.alive && !live.contains(id) {
                track.alive = false;
            }
        }
        for event in events {
            if let Event::Restless { id, verdict, .. } = event
                && let Some(track) = self.seen.get_mut(id)
            {
                *track.verdicts.entry(verdict_name(*verdict)).or_default() += 1;
            }
        }
    }

    fn report(&self) {
        println!("\n== anomalies ==");
        if self.findings.is_empty() {
            println!("  none");
        }
        for finding in &self.findings {
            println!("  {finding}");
        }

        let mut restless: Vec<(&u32, &Track)> = self
            .seen
            .iter()
            .filter(|(_, track)| track.alive && !track.verdicts.is_empty())
            .collect();
        restless.sort_unstable_by_key(|(id, _)| **id);
        if !restless.is_empty() {
            println!("\n== bodies still awake at t{} ==", self.tick);
        }
        for (id, track) in restless {
            let mut reasons: Vec<(&&str, &u32)> = track.verdicts.iter().collect();
            reasons.sort_unstable_by_key(|(_, count)| std::cmp::Reverse(**count));
            let reasons: Vec<String> = reasons
                .iter()
                .map(|(name, count)| format!("{name} x{count}"))
                .collect();
            println!(
                "  body {id}: {} cells, alive {} ticks, peak {} cells/tick, refused settle: {}",
                track.cells,
                self.tick - track.born,
                fixed(track.peak_speed),
                reasons.join(", ")
            );
        }
    }
}

fn describe(state: &BodyState) -> String {
    format!(
        "v=({},{}) spin={} acc=({},{},{}) mass={} weight={} bounds={}..{} {}{}",
        fixed(state.velocity.0),
        fixed(state.velocity.1),
        state.spin,
        state.accumulators.0,
        state.accumulators.1,
        state.accumulators.2,
        state.mass,
        state.weight,
        point(state.bounds.min),
        point(state.bounds.max),
        if state.settles { "settles " } else { "" },
        if state.parked { "parked" } else { "" },
    )
}

fn magnitude(velocity: (i64, i64)) -> i64 {
    (velocity.0 * velocity.0 + velocity.1 * velocity.1).isqrt()
}

fn fixed(value: i64) -> String {
    format!("{:.3}", value as f64 / SUBCELL as f64)
}

fn point(pos: CellPos) -> String {
    format!("({},{})", pos.x, pos.y)
}

#[derive(Default)]
struct Digest {
    counts: FxHashMap<&'static str, u64>,
    contacts: FxHashMap<&'static str, u64>,
    quanta: FxHashMap<&'static str, u64>,
    verdicts: FxHashMap<&'static str, u64>,
    detached: u64,
    detached_cells: u64,
    settled: u64,
    settled_cells: u64,
    splits: u64,
    released: u64,
}

impl Digest {
    fn absorb(&mut self, events: &[Event]) {
        for event in events {
            *self.counts.entry(event_name(event)).or_default() += 1;
            match event {
                Event::Detached { cells, .. } => {
                    self.detached += 1;
                    self.detached_cells += *cells as u64;
                }
                Event::Settled { cells, .. } => {
                    self.settled += 1;
                    self.settled_cells += *cells as u64;
                }
                Event::Split { .. } => self.splits += 1,
                Event::Released { .. } => self.released += 1,
                Event::Contact { peer, .. } => {
                    *self.contacts.entry(peer_name(*peer)).or_default() += 1;
                }
                Event::Quantum { outcome, .. } => {
                    *self.quanta.entry(outcome_name(*outcome)).or_default() += 1;
                }
                Event::Restless { verdict, .. } => {
                    *self.verdicts.entry(verdict_name(*verdict)).or_default() += 1;
                }
                _ => {}
            }
        }
    }

    fn report(&self) {
        println!("\n== journal totals ==");
        print_map("  events", &self.counts);
        print_map("  contacts by peer", &self.contacts);
        print_map("  quanta by outcome", &self.quanta);
        print_map("  settle refusals", &self.verdicts);
        println!(
            "  {} islands detached ({} cells), {} bodies settled ({} cells), {} splits, {} cells released",
            self.detached,
            self.detached_cells,
            self.settled,
            self.settled_cells,
            self.splits,
            self.released
        );
    }
}

fn print_map(title: &str, map: &FxHashMap<&'static str, u64>) {
    if map.is_empty() {
        return;
    }
    let mut entries: Vec<(&&str, &u64)> = map.iter().collect();
    entries.sort_unstable_by_key(|(name, count)| (std::cmp::Reverse(**count), **name));
    let line: Vec<String> = entries
        .iter()
        .map(|(name, count)| format!("{name} {count}"))
        .collect();
    println!("{title}: {}", line.join(", "));
}

fn event_name(event: &Event) -> &'static str {
    match event {
        Event::Detached { .. } => "detached",
        Event::Split { .. } => "split",
        Event::Dissolved { .. } => "dissolved",
        Event::Released { .. } => "released",
        Event::Struck { .. } => "struck",
        Event::Loaded { .. } => "loaded",
        Event::Parked { .. } => "parked",
        Event::Woke { .. } => "woke",
        Event::Quantum { .. } => "quantum",
        Event::Contact { .. } => "contact",
        Event::Entrained { .. } => "entrained",
        Event::Carried { .. } => "carried",
        Event::Settled { .. } => "settled",
        Event::Restless { .. } => "restless",
    }
}

fn peer_name(peer: Peer) -> &'static str {
    match peer {
        Peer::Terrain => "terrain",
        Peer::Body(_) => "body",
        Peer::Grain(material) => content::material(material).name,
    }
}

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Committed => "committed",
        Outcome::Rebounded => "rebounded",
        Outcome::Parked => "parked",
        Outcome::Unloaded => "unloaded",
    }
}

fn verdict_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Rests => "rests",
        Verdict::Fast => "fast",
        Verdict::Spinning => "spinning",
        Verdict::Unloaded => "unloaded",
        Verdict::OnBody => "on-body",
        Verdict::OnPowder => "on-powder",
        Verdict::Unsupported => "unsupported",
        Verdict::Restless => "restless",
    }
}

fn body_of(event: &Event) -> u32 {
    match event {
        Event::Detached { id, .. }
        | Event::Dissolved { id, .. }
        | Event::Released { id, .. }
        | Event::Struck { id, .. }
        | Event::Loaded { id, .. }
        | Event::Parked { id, .. }
        | Event::Woke { id }
        | Event::Quantum { id, .. }
        | Event::Contact { id, .. }
        | Event::Entrained { id, .. }
        | Event::Carried { id, .. }
        | Event::Settled { id, .. }
        | Event::Restless { id, .. } => *id,
        Event::Split { source, .. } => *source,
    }
}

fn render_event(event: &Event) -> String {
    match event {
        Event::Detached {
            id,
            cells,
            span,
            at,
            material,
        } => format!(
            "detached {id}: {cells} cells of {}, {}x{} at {}",
            content::material(*material).name,
            span.0,
            span.1,
            point(*at)
        ),
        Event::Split { source, parts, at } => {
            format!("split {source} -> {parts:?} at {}", point(*at))
        }
        Event::Dissolved { id, at } => format!("dissolved {id} at {}", point(*at)),
        Event::Released { id, at, material } => format!(
            "released {} from {id} at {}",
            content::material(*material).name,
            point(*at)
        ),
        Event::Struck { id, at, push, drag } => {
            format!("struck {id} at {} push={push} drag={drag}", point(*at))
        }
        Event::Loaded { id, at, jy } => format!("loaded {id} at {} j={jy}", point(*at)),
        Event::Parked { id, blocker } => format!("parked {id} behind chunk {blocker:?}"),
        Event::Woke { id } => format!("woke {id}"),
        Event::Quantum {
            id,
            freedom,
            sign,
            outcome,
        } => format!(
            "quantum {id} {freedom:?}{} {}",
            if *sign < 0 { "-" } else { "+" },
            outcome_name(*outcome)
        ),
        Event::Contact {
            id,
            at,
            normal,
            peer,
            push,
            drag,
        } => format!(
            "contact {id} at {} n=({},{}) vs {} push={push} drag={drag}",
            point(*at),
            normal.0,
            normal.1,
            peer_name(*peer)
        ),
        Event::Entrained {
            id,
            at,
            material,
            jx,
            jy,
        } => format!(
            "entrained {} by {id} at {} j=({jx},{jy})",
            content::material(*material).name,
            point(*at)
        ),
        Event::Carried {
            id,
            at,
            material,
            impulse,
        } => format!(
            "carried {} by {id} at {} j={impulse}",
            content::material(*material).name,
            point(*at)
        ),
        Event::Settled { id, cells, at } => {
            format!("settled {id}: {cells} cells at {}", point(*at))
        }
        Event::Restless {
            id,
            verdict,
            residual,
        } => format!(
            "restless {id}: {} residual v=({},{}) spin={}",
            verdict_name(*verdict),
            fixed(residual.0),
            fixed(residual.1),
            residual.2
        ),
    }
}

fn paint(harness: &Harness, window: CellRect, columns: usize) {
    let width = (window.max.x - window.min.x + 1) as usize;
    let step = width.div_ceil(columns.max(1)).max(1) as i32;
    let mut ids: Vec<u32> = harness.bodies.states().map(|state| state.id).collect();
    ids.sort_unstable();
    let mut y = window.max.y;
    while y >= window.min.y {
        let mut line = String::new();
        let mut x = window.min.x;
        while x <= window.max.x {
            line.push(glyph(harness, CellPos::new(x, y), &ids));
            x += step;
        }
        println!("  {line}");
        y -= step;
    }
}

fn glyph(harness: &Harness, pos: CellPos, ids: &[u32]) -> char {
    const BODY_MARKS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let Some(cell) = harness.world.get_cell(pos) else {
        return ' ';
    };
    if let Some(id) = cell.body_id() {
        let index = ids.iter().position(|&other| other == id).unwrap_or(0);
        return BODY_MARKS[index % BODY_MARKS.len()] as char;
    }
    if cell.is_air() {
        return '.';
    }
    let name = content::material(cell.material).name;
    match content::phase(cell.material) {
        Phase::Gas => '"',
        Phase::Liquid => '~',
        _ => name.chars().next().unwrap_or('?'),
    }
}

fn scene_tree(args: &Args) {
    let generator = WorldGenerator::new(args.seed);
    let mut harness = Harness::new();
    let surface = generator.surface_height(args.origin);
    harness.load(&generator, region_of(args.origin, surface), 1);

    let Some(trunk) = find_trunk(&harness, args.origin) else {
        println!(
            "no tree within reach of x={}; try another --origin",
            args.origin
        );
        return;
    };
    let crown = tree_extent(&harness, trunk);
    println!(
        "seed {} tree at {} — {} wood/leaf cells spanning {}..{}",
        args.seed,
        point(trunk),
        crown.1,
        point(crown.0.min),
        point(crown.0.max)
    );
    println!("  standing on: {}", footing_report(&harness, trunk));

    let settle_ticks = 30;
    println!("\n== settling the freshly generated world for {settle_ticks} ticks ==");
    let mut digest = Digest::default();
    let mut tracker = Tracker::default();
    for tick in 0..settle_ticks {
        let events = harness.step();
        digest.absorb(&events);
        tracker.observe(tick, &harness.bodies, &events);
        report_tick(args, tick, &events, &harness);
    }
    if harness.bodies.states().count() > 0 {
        println!(
            "  worldgen left {} loose bodies before anything was touched:",
            harness.bodies.states().count()
        );
        for state in harness.bodies.states() {
            println!("    body {}: {}", state.id, describe(&state));
        }
    }

    let mut lit = 0;
    for dy in 0..8 {
        for dx in -6..=6 {
            let pos = trunk.translated(dx, dy);
            if harness
                .world
                .set_material(pos, content::material::FIRE)
                .then_some(())
                .is_some()
                && harness
                    .world
                    .get_cell(pos)
                    .is_some_and(|cell| cell.material == content::material::FIRE)
            {
                lit += 1;
            }
        }
    }
    println!(
        "\n== set {lit} fire cells against the trunk, running {} ticks ==",
        args.ticks
    );

    let before = harness.census();
    for tick in settle_ticks..settle_ticks + args.ticks {
        let events = harness.step();
        digest.absorb(&events);
        tracker.observe(tick, &harness.bodies, &events);
        report_tick(args, tick, &events, &harness);
        if args.frames > 0 && tick.is_multiple_of(args.frames) {
            println!("\n-- t{tick} --");
            paint(&harness, crown.0.grown(8), 110);
        }
    }
    let after = harness.census();

    digest.report();
    tracker.report();
    println!("\n== matter ==");
    println!(
        "  before: {} cells ({} solid, {} powder, {} liquid, {} gas, {} owned)",
        before.cells, before.solid, before.powder, before.liquid, before.gas, before.owned
    );
    println!(
        "  after:  {} cells ({} solid, {} powder, {} liquid, {} gas, {} owned)",
        after.cells, after.solid, after.powder, after.liquid, after.gas, after.owned
    );
    println!("\n== final frame ==");
    paint(&harness, crown.0.grown(8), 110);
}

fn find_trunk(harness: &Harness, near: i32) -> Option<CellPos> {
    let mut best: Option<((i32, i32), CellPos)> = None;
    for (pos, chunk) in harness.world.chunks() {
        if !harness.simulated.contains(&pos) {
            continue;
        }
        let base = pos.base_cell();
        for local_y in 0..CHUNK_SIZE as i32 {
            for local_x in 0..CHUNK_SIZE as i32 {
                let at = base.translated(local_x, local_y);
                if chunk.get(at.offset()).material != content::material::WOOD {
                    continue;
                }
                let score = ((at.x - near).abs() / 64, at.y);
                if best.is_none_or(|(current, _)| score < current) {
                    best = Some((score, at));
                }
            }
        }
    }
    best.map(|(_, pos)| pos)
}

fn footing_report(harness: &Harness, trunk: CellPos) -> String {
    let mut column = Vec::new();
    for dy in 1..=4 {
        let pos = trunk.translated(0, -dy);
        let name = harness
            .world
            .get_cell(pos)
            .map_or("unloaded", |cell| content::material(cell.material).name);
        let info = harness
            .world
            .get_cell(pos)
            .map(|cell| content::material(cell.material));
        column.push(match info {
            Some(info) => format!("{name}(hardness {:.2}, {:?})", info.hardness, {
                harness
                    .world
                    .get_cell(pos)
                    .map(|cell| content::phase(cell.material))
                    .expect("loaded")
            }),
            None => name.to_string(),
        });
    }
    column.join(" / ")
}

fn tree_extent(harness: &Harness, trunk: CellPos) -> (CellRect, usize) {
    let mut seen = FxHashSet::default();
    let mut stack = vec![trunk];
    seen.insert(trunk);
    let mut min = trunk;
    let mut max = trunk;
    while let Some(pos) = stack.pop() {
        min = CellPos::new(min.x.min(pos.x), min.y.min(pos.y));
        max = CellPos::new(max.x.max(pos.x), max.y.max(pos.y));
        for dy in -1..=1 {
            for dx in -1..=1 {
                let next = pos.translated(dx, dy);
                if seen.contains(&next) {
                    continue;
                }
                let bonded = harness.world.get_cell(next).is_some_and(|cell| {
                    content::bond_group(cell.material)
                        == content::bond_group(content::material::WOOD)
                });
                if bonded {
                    seen.insert(next);
                    stack.push(next);
                }
            }
        }
    }
    (CellRect::new(min, max), seen.len())
}

fn scene_stack(args: &Args) {
    let mut harness = Harness::new();
    harness.load_flat(2, -60, content::material::GRANITE);

    let shapes: [&[(i32, i32)]; 6] = [
        &[(0, 0)],
        &[(0, 0), (1, 0)],
        &[(0, 0), (0, 1)],
        &[(0, 0), (1, 0), (0, 1)],
        &[(0, 0), (1, 0), (0, 1), (1, 1)],
        &[(0, 0), (1, 0), (2, 0), (1, 1)],
    ];
    let mut spawned = 0;
    for (index, shape) in shapes.iter().cycle().take(24).enumerate() {
        let cells: Vec<(CellPos, MaterialId, u8)> = shape
            .iter()
            .map(|&(dx, dy)| {
                (
                    CellPos::new(
                        -20 + (index as i32 % 6) * 7 + dx,
                        -40 + (index as i32 / 6) * 9 + dy,
                    ),
                    content::material::WOOD,
                    0,
                )
            })
            .collect();
        if harness.bodies.spawn(
            &mut harness.world,
            1000 + index as u32,
            &cells,
            Policy::DEBRIS,
        ) {
            spawned += 1;
        }
    }
    println!("spawned {spawned} debris bodies over a granite floor");

    let mut digest = Digest::default();
    let mut tracker = Tracker::default();
    for tick in 0..args.ticks {
        let events = harness.step();
        digest.absorb(&events);
        tracker.observe(tick, &harness.bodies, &events);
        report_tick(args, tick, &events, &harness);
    }
    digest.report();
    tracker.report();
    println!(
        "\n{} of {spawned} bodies never settled after {} ticks",
        harness.bodies.states().count(),
        args.ticks
    );
    paint(
        &harness,
        CellRect::new(CellPos::new(-24, -62), CellPos::new(24, -30)),
        110,
    );
}

fn scene_gas(args: &Args) {
    let mut harness = Harness::new();
    harness.load_flat(2, -60, content::material::GRANITE);
    let walker = 900;
    let cells: Vec<(CellPos, MaterialId, u8)> = (0..3)
        .flat_map(|dx| {
            (0..5).map(move |dy| (CellPos::new(-28 + dx, -60 + dy), content::material::BODY, 0))
        })
        .collect();
    assert!(
        harness
            .bodies
            .spawn(&mut harness.world, walker, &cells, Policy::PLAYER),
        "walker spawns into open air"
    );
    println!(
        "driving a player-policy body east at exactly one cell per tick through a maintained\n\
         smoke bank spanning x=-20..80; every tick without progress is the gas obstructing it"
    );

    let mut travelled = 0;
    let mut inside = 0;
    let mut blocked = 0;
    let mut digest = Digest::default();
    for tick in 0..args.ticks {
        for y in -60..-40 {
            for x in -20..80 {
                harness
                    .world
                    .set_material(CellPos::new(x, y), content::material::SMOKE);
            }
        }
        let before = harness.bodies.footing(walker).expect("walker alive");
        let vy = harness.bodies.velocity(walker).expect("walker alive").1;
        harness.bodies.drive(walker, Subcell::from_raw(SUBCELL), vy);
        let events = harness.step();
        digest.absorb(&events);
        report_tick(args, tick, &events, &harness);
        let after = harness.bodies.footing(walker).expect("walker alive");
        travelled += after.x - before.x;
        if after.x >= -20 {
            inside += 1;
            if after.x == before.x {
                blocked += 1;
            }
        }
    }
    digest.report();
    println!(
        "\nwalker advanced {travelled} cells in {} ticks; inside the bank {blocked} of {inside} ticks made no progress",
        args.ticks
    );
    paint(
        &harness,
        CellRect::new(CellPos::new(-32, -61), CellPos::new(32, -38)),
        110,
    );
}

fn scene_survey(args: &Args) {
    let generator = WorldGenerator::new(args.seed);
    println!("scanning worldgen output for trees that fall on load");
    let mut sampled = 0;
    let mut woody = 0;
    let mut detached = 0;
    let mut detached_cells = 0;
    for region_x in -6..=6 {
        let mut harness = Harness::new();
        let origin = region_x * REGION_SIZE_CELLS as i32;
        let region = region_of(origin, generator.surface_height(origin));
        harness.load(&generator, region, 1);
        sampled += 1;
        if count_material(&harness, content::material::WOOD) == 0 {
            continue;
        }
        woody += 1;
        let mut by_material: FxHashMap<&'static str, (u64, u64)> = FxHashMap::default();
        for tick in 0..8 {
            let events = harness.step();
            for event in &events {
                if let Event::Detached {
                    cells, material, ..
                } = event
                {
                    detached += 1;
                    detached_cells += *cells as u64;
                    let entry = by_material
                        .entry(content::material(*material).name)
                        .or_default();
                    entry.0 += 1;
                    entry.1 += *cells as u64;
                }
            }
            if tick == 0 && !by_material.is_empty() {
                let mut kinds: Vec<_> = by_material.iter().collect();
                kinds.sort_unstable_by_key(|(_, (_, cells))| std::cmp::Reverse(*cells));
                let summary: Vec<String> = kinds
                    .iter()
                    .map(|(name, (count, cells))| format!("{name} x{count} ({cells} cells)"))
                    .collect();
                println!(
                    "  region {},{}: {} islands loose on the first tick — {}",
                    region.x,
                    region.y,
                    by_material.values().map(|(count, _)| count).sum::<u64>(),
                    summary.join(", ")
                );
            }
        }
    }
    println!(
        "\n{sampled} regions sampled, {woody} with woody growth, {detached} islands detached on load ({detached_cells} cells)"
    );
}

fn region_of(x: i32, y: i32) -> RegionPos {
    RegionPos::new(
        x.div_euclid(REGION_SIZE_CELLS as i32),
        y.div_euclid(REGION_SIZE_CELLS as i32),
    )
}

fn count_material(harness: &Harness, material: MaterialId) -> u64 {
    let mut count = 0;
    for (pos, chunk) in harness.world.chunks() {
        if !harness.simulated.contains(&pos) {
            continue;
        }
        for cell in chunk.cells() {
            if cell.material == material {
                count += 1;
            }
        }
    }
    count
}

fn report_tick(args: &Args, tick: u64, events: &[Event], harness: &Harness) {
    if !args.events && args.trace.is_none() {
        return;
    }
    let shown: Vec<&Event> = events
        .iter()
        .filter(|event| args.trace.is_none_or(|id| body_of(event) == id))
        .filter(|event| {
            args.events || !matches!(event, Event::Quantum { .. } | Event::Contact { .. })
        })
        .collect();
    if shown.is_empty() {
        return;
    }
    println!("t{tick}:");
    for event in shown {
        println!("    {}", render_event(event));
    }
    if let Some(id) = args.trace
        && let Some(state) = harness.bodies.states().find(|state| state.id == id)
    {
        println!("    state: {}", describe(&state));
    }
}
