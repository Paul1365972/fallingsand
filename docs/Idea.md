# Fallingsand — Design & Architecture

A multiplayer Noita-like falling-sand game.
Main inspirations: **Noita** (world simulation, destruction), **Don't Starve Together** (drop-in multiplayer survival), **Minecraft** (chunked infinite worlds, server/client model).

## Vision & Pillars

1. **The world is the simulation.**
   Every pixel is a material that can burn, melt, flow, or fall.
   Terrain destruction is a first-class mechanic, not an effect.
2. **Multiplayer-native.**
   The game is always server/client — single player runs an embedded local server.
   No separate code path for single player.
3. **Performance is a feature.**
   A big, live, multi-player world only works if the sim is ruthlessly efficient: sleep everything that can sleep, parallelize everything that can't.
4. **Architecture before content.**
   The first iteration ships few features, but each one on load-bearing foundations that the final game can grow on without rewrites.

## Gameplay

**Co-op survival in a fully simulated world**: up to ~10 players share one persistent, infinite world.
The loop is DST-flavored — gather, build, defend, expand — but every mechanic routes through the material simulation: you don't "mine a block", you dig through actual sand and rock; your base floods if you breach an aquifer; fire spreads through what's actually flammable.

- **Persistent shared world**: the server (dedicated or friend-hosted) keeps the world alive; players drop in and out.
  Death costs you your stuff and a trek back, never the world.
- **Infinite world in all directions**: content is anchored around a surface band at y≈0.
  The surface is home — buildable, farmable, survivable.
  Depth is the risk/reward axis: better materials, worse hazards.
  The sky thins into emptiness and is a late-game frontier, not a wall.
- **First iteration gameplay** is deliberately thin: move, dig, place, a few materials, basic health.
  The world simulation *is* the early content; survival systems layer on in later milestones once the foundations are proven.

## Technology Choices

| Area | Choice | Rationale |
|------|--------|-----------|
| Platforms | **Native (Win/Linux/macOS) + Web client** | Server and single player are native-only. The browser build is a *client only* that joins remote servers. This constrains the client crate (WASM-compatible deps, no threads assumed, no filesystem) but not the server. |
| Network authority | **Authoritative server + delta replication** | Server owns the world. Clients receive delta-compressed dirty-region updates for chunks in their interest area. No determinism requirement across machines; robust to packet loss and late joins. Bandwidth is the engineering cost — paid via dirty rects + compression. |
| Transport | **WebTransport (QUIC)** via the `web-transport` crate family (`web-transport-quinn` native, `web-transport-wasm` browser); in-memory channels for the embedded local server | One reliable ordered stream per connection carries the whole protocol — no datagram machinery, no loss handling above the transport. Works in browsers, one `Session` API across server, native client, and wasm client. Actively maintained (MoQ project); pin versions — roughly one breaking release per quarter. Single player bypasses the network entirely through the same transport trait. |
| Terrain physics | **Rigid bodies built into the architecture** | Pixel-terrain ↔ rigid-body conversion (flood-fill → pixel bodies) is part of the world model, protocol, and renderer even though the first playable ships only cellular automata + entity collision. Retrofitting this is what kills falling-sand engines. |
| Physics engine | **Custom, purpose-built** | Physics here is inseparable from the cell grid: bodies are made of cells, terrain changes every tick, and collision must be pixel-perfect. A general-purpose engine fights that coupling; a small bespoke solver (kinematic character movement + impulse-based pixel bodies) covers exactly what the game needs and nothing more. |
| Server core | **`bevy_ecs` standalone** | Just the ECS crate — no Bevy app, windowing, or render on the server. Battle-tested parallel scheduler; entity component types are shared with the client naturally. |
| Client engine | **Bevy** (display/input/UI layer only) | Bevy never drives game logic. It renders world state, plays audio, and collects input. |
| Persistence | **redb** (pure-Rust embedded ACID KV store) | Server-side only. Regions stored as compressed blobs keyed by z-order region coords, plus tables for entities, players, and world metadata. No monolithic world blob. |
| Serialization | **serde + postcard** for protocol & storage, versioned | Compact varint encoding, written wire-format spec that is stable across versions (safe for saves), pure Rust, WASM-clean, sane on hostile input. One protocol crate defines every message; nothing hand-rolls byte buffers. |
| Compression | **lz4_flex** | Pure Rust, extremely fast, WASM-compatible (the web client must decompress chunk data). Zstd can be revisited later for cold storage on the server. |

## Workspace Layout

```
crates/
├── fallingsand_core      # Shared foundation: coords, cells/chunks/regions, material registry
├── fallingsand_sim       # Simulation kernel: CA passes, dirty rects, sleeping, physics
├── fallingsand_protocol  # All client↔server messages: serde types, framing, versioning
├── fallingsand_net       # Transport trait; backends: WebTransport (native + wasm), in-memory
├── fallingsand_worldgen  # Deterministic procedural generation
├── fallingsand_server    # Authoritative server: library + dedicated headless binary
└── fallingsand_client    # Bevy app; builds the `fallingsand` binary (native + trunk WASM)
assets/                   # Client assets
data/                     # Hand-authored definitions (materials.ron, biomes.ron, ...)
docs/                     # Design docs
```

Strict dependency direction: `core ← sim ← {server, client}` and `core ← protocol ← {server, client}`.
Only `fallingsand_client` may depend on Bevy; only `fallingsand_server` may depend on tokio and redb.
CI enforces that `fallingsand_client` builds for `wasm32-unknown-unknown`.

## World Model

| Level | Size | Purpose |
|-------|------|---------|
| Cell | 1×1 | One material instance |
| Chunk | 64×64 cells | Unit of per-tick work: dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks (512×512 cells) | Unit of generation, storage, and load/unload |

"Cell" always means this atomic unit — the parallel-scheduling work item (a 2×2-chunk block plus its halo) is not a named concept.

Coordinates: global positions `CellPos`, `ChunkPos`, `RegionPos` (i32 per axis) and local offsets `CellOffset` (within chunk), `ChunkOffset` (within region); conversions are bit shifts and masks.
Storage keys use z-order (Morton) encoding of `RegionPos` for locality.

### Cell representation

Fixed-size, cache-friendly, no heap indirection.
Target **4 bytes/cell**:

```rust
struct Cell {
    material: MaterialId,  // u16 — index into the material registry
    shade_flags: u8,       // high nibble: color variation (visual only),
                           // low nibble: flag bits (burning, electrified, ...)
    updated: u8,           // last-updated tick (low byte)
}
```

If temperature simulation lands later it becomes a *separate* per-chunk plane (SoA), not a fatter Cell — most chunks won't need it and it should be skippable wholesale.

`updated` wraps every 256 ticks, which could skip one update in a chunk waking from sleep.
Rule: when a chunk transitions sleep→awake, normalize all `updated` bytes.
Cheap, happens rarely, kills the bug class.

### Materials are data, not code variants

A hardcoded material enum doesn't scale to a real material catalogue.
Instead:

- `data/materials.ron` defines materials: id, name, phase (solid/powder/liquid/gas), density, color palette, flammability, reactions (e.g. water + lava → stone + steam), etc.
- Loaded into a `MaterialRegistry` at startup.
  The server sends the client a hash to detect mismatch; later this enables server-side custom materials.
- The sim kernel switches on **phase + properties**, not on material identity.
  Adding a new powder must require zero engine code.

### Simulation kernel

- **4-phase block scheduling**: chunks are grouped into 2×2-chunk blocks; each tick runs 4 phases, selecting awake blocks by parity (`block.x mod 2, block.y mod 2`).
  A worker simulates only the cells of its own block, with read/write access one chunk beyond it (a 4×4-chunk window).
  Same-phase windows share no chunks, so workers hold disjoint mutable chunk access in parallel (rayon) — race-free by construction, no locking.
- **Speed of light = 64**: no update may reach farther than 64 cells from its origin, or it could escape the window; the kernel API enforces this.
- **Deferred world edits**: anything that reaches farther — explosions, structure placement — goes into a world-edit queue applied between phases instead of writing in-pass.
- **Alternating raster order** per row + tick-seeded FxHash for left/right tie-breaking: cheap, stateless, reproducible randomness.
- **Bottom-up scan** for gravity-dominated updates; per-material update rules dispatch on phase.
- **Dirty rects per chunk**: each chunk tracks the bounding rect of cells changed last tick (double-buffered: `bounds` / `old_bounds`).
  Every consumer keys off this:
  - the sim skips chunks with empty dirty rects (**chunk sleeping**),
  - replication sends only dirty rects,
  - the renderer re-uploads only dirty rects.
  Waking rules: a write to a sleeping chunk (or its border) wakes it; neighbors of a dirty border wake too.
- **Cell particles**: cells knocked loose (explosions, splashes, digging spray) leave the grid and fly ballistically as lightweight free particles, reinserting into the grid on impact.
  Server-simulated and replicated as spawn events (clients integrate the trajectories themselves); purely cosmetic spray can additionally be spawned client-side.
- **Determinism *within* the server**: same seed + same inputs → same world on one machine (useful for tests/replays).
  Cross-machine determinism is *not* required; the authoritative-server model frees us from that.

### Physics

Physics is a small custom module inside `fallingsand_sim` — no general-purpose physics engine.
Everything collides against the cell grid directly, so changing terrain never requires rebuilding collision geometry.

- **Entities** (players, items, creatures) are kinematic bodies: AABB/capsule shapes moved with swept collision tests against solid cells, with material-aware effects (drag in liquids, sinking in powders).
- **The character controller is feel-first** (Celeste-style forgiveness, tuned server-side since the server is authoritative): coyote time and jump buffering, variable jump height (release to cut), reduced gravity at the jump apex, heavier falling gravity, run acceleration with harder deceleration and turn assist, ceiling corner correction, and step-up/step-down assists that preserve momentum over rough terrain.
- **Players wade through small amounts of granular matter**: movement blocked only by a few powder cells pushes through them with a speed penalty per grain, displacing the grains to nearby free cells (conserved, not destroyed).
  Loose sand can slow, bury, and trap you — but you can always struggle out; only true solids (rock) pin you permanently.
- **Solidity is phase-based**: every tick, entity AABBs and in-flight pixel-body cells rasterize into an obstacle mask over the grid.
  Powders treat masked cells as ground — sand piles on a player's head and on flying debris instead of passing through.
  Liquids and gases ignore the mask: at cell scale, water flowing *around* a body and *through* its cells are the same thing, which keeps swimming and submersion trivially correct.
  When a mask moves, vacated cells with grains resting on or above them are dirty-marked so piles wake and collapse.
- Entities collide against in-flight pixel-body cells (stand on a falling platform, blocked by a flying slab); pixel bodies gain contacts from entity boxes and never settle back into the grid while overlapping an entity, so nobody gets stamped into stone.
- **Pixel bodies** are rigid bodies made of cells.
  Solid materials marked `rigid_capable` in the registry participate.
  On terrain damage, flood-fill detects disconnected solid islands; the island's cells are lifted out of the grid into a pixel body (a small cell buffer + position/rotation/velocity).
- Pixel-body integration is impulse-based: collision detection samples the body's perimeter cells against the grid, contacts get impulse response plus positional correction.
  Body-vs-body collision starts coarse (AABB/circle approximation) and is refined only if gameplay demands it.
- When a pixel body comes to rest it is stamped back into the grid and becomes terrain again.
- Rendering: pixel bodies are just small textures with a transform.
- The first playable may cap pixel-body count aggressively; the important part is that the world model, protocol, and renderer all know pixel bodies exist from day one.

## Server Architecture

A `fallingsand_server::Server` is a library value you construct and tick — the dedicated binary and the client's embedded single-player server use the identical type.

- **`bevy_ecs` World + Schedule** drives everything: entities are ECS entities, systems run on the ECS parallel scheduler.
  The cell grid is a resource, not entities.
- **Fixed tick rate: 60 Hz** target; the tick budget breakdown lives in [Performance](#performance-strategy).
  If a tick overruns, the server slews rather than spiraling.
  Simulation rate and replication rate are independent knobs; both start at 60 Hz, and deltas are tick-tagged so replication can drop to a lower cadence later without redesign.
- **Tick order**:
  1. Drain network: sessions, player inputs, commands
  2. Apply player inputs (movement intents, actions → world edits)
  3. Load/generate/unload regions per chunk-ticket changes
  4. Step cellular automata (4 block phases, rayon), apply deferred world edits
  5. Step physics (entities + pixel bodies), settle resting pixel bodies back into grid
  6. Run game logic systems (health, interactions, inventory…)
  7. Snapshot dirty state → replication → send
  8. Periodic: persistence flush (dirty regions → redb), autosave entities

### Interest management: chunk tickets

- Every **ticket source** (player, spawn anchor, scripted camera) projects tickets onto chunks: `Active` (simulate + replicate), `Border` (loaded, simulated so edges behave, not replicated), `Loaded` (in memory only).
- Regions with zero tickets get persisted and unloaded after a grace period.
- Replication distance and simulation distance are separate knobs per source.

### Persistence

- redb tables: `regions` (z-order key → lz4 blob, format-versioned), `entities` (region key → entity set), `players` (uuid → player data), `meta` (seed, world version, name).
- Regions are written **only when dirty**, on unload and on periodic autosave.
  Writes go through redb transactions so a crash never corrupts the world.
- The save format carries an explicit version byte from day one; migrations are a function table.

## Networking

### Protocol (`fallingsand_protocol`)

All messages are serde enums, postcard-encoded, lz4-compressed above a size threshold.
Everything flows over **one reliable ordered stream** per connection — handshake, chunk loads/unloads, per-tick dirty-rect deltas, entity and pixel-body state, player input.
Because delivery is reliable and ordered, deltas always apply cleanly on top of the last state: no per-chunk versioning, no resync protocol, no input sequence numbers.
Packet loss costs a retransmit delay (head-of-line blocking for roughly one RTT), never correctness — an acceptable trade for a ~10-player co-op game, and the reason the protocol stays radically simple.
If profiling ever shows loss-induced stutter matters, moving hot state back to datagrams is a contained change behind the transport trait.

### Client-side latency handling

- **Interpolate everything** (players, remote entities, pixel bodies) between the last two received states — plain lerp, no prediction, no reconciliation.
  Local-player prediction is a deliberate non-feature until latency demands it; the shared `step_player` in `fallingsand_sim` keeps the insertion point ready.
- **Do not predict the sand.**
  Cell deltas apply as they arrive; at 60 Hz server ticks the world feels live.
  Cosmetic client-side sim prediction of visible chunks is a later optimization with a clear insertion point (the client already ships `fallingsand_sim` for the embedded server, so the machinery exists).

### Local single player

The embedded server runs in a background thread of the client process.
`fallingsand_net`'s in-memory duplex transport connects them with the same `Connection` trait as WebTransport.
Zero serialization shortcuts — the local pipe still moves `fallingsand_protocol` messages, so single player constantly exercises the multiplayer path (frame-copy cost is negligible; revisit only if profiling says otherwise).

## Client Architecture (`fallingsand_client`)

- **Bevy app** with states: `MainMenu → Connecting → InGame (→ Paused overlay)`.
- **World rendering**: one GPU texture per loaded chunk (64×64), re-uploaded only for dirty rects; chunks drawn as instanced quads.
  Material id + shade resolve to color via a palette texture in a fragment shader — no per-pixel CPU loop.
- **Camera & scale**: Noita-like virtual resolution of ~424×242 cells, integer-scaled to the window (~4–5 px per cell at 1080p) so grains stay readable.
  A modest zoom range is allowed, but replication/interest budgets are sized for the default zoom — zooming out never expands the server's obligations.
- **Pixel bodies & entities**: sprites/textures with transforms, interpolated.
- **UI**: Bevy UI for the first iteration (menu, HUD, chat, debug overlay).
  The main menu is a real deliverable: world list (create/load/delete), server browser/direct-connect, settings.
- **Debug tooling from day one**: F3-style overlay (tick time, chunk counts, dirty stats, bandwidth), chunk-boundary/dirty-rect visualizer, material inspector under cursor.
  This pays for itself immediately when tuning the sim.
- **WASM build** (trunk): same crate, `fallingsand_sim`'s rayon behind a feature, storage/embedded server compiled out.
  The web client is join-only.

## World Generation (`fallingsand_worldgen`)

- Deterministic pure function: `(seed, RegionCoords) → Region`.
  Regions must be generatable independently and in any order, so features that cross region borders use deterministic overlap generation rather than sequential dependency.
  This is mandatory for an unbounded world: there is no "generate the whole world" step.
- **Infinite-world layout**: a surface band at y≈0 (heightmap via noise), infinite depth below organized in progressive depth bands (biome/hazard/loot tiers keyed on Y), infinite sky above thinning toward emptiness.
  Horizontal variety comes from biome noise along X; vertical variety from depth bands — the two compose into a 2D biome lookup.
- Layered pipeline: base terrain (noise: heights/caves via fBm + domain warping) → biome assignment (X-noise × Y-band) → material fill → features/structures → post-passes (ore veins, vegetation).
- Biome and feature definitions are data-driven (`data/biomes.ron`) like materials.
- First iteration: surface band with height variation, a cave layer, 2–3 biomes, a handful of materials.
  Enough to prove the pipeline shape, not content-complete.

## Performance Strategy

Budgets first, so regressions are visible in the in-game debug overlay:

- **Server tick budget @ 60 Hz: ~16 ms**, sim target **≤ 8 ms** at the worst case: ~10 players in fully disjoint interest areas ≈ **~2,000 active chunks** (~200 per player).
  Noita runs ~768 active chunks for one player, so this demands sleeping to actually work — settled chunks must cost ~nothing, and simulation distance throttles before tick time spirals.
- **Per-player replication caps**: dirty-rect bytes per tick per client are budgeted and degrade gracefully (nearest chunks win; far dirty chunks coalesce into periodic resyncs).
- Key levers, in priority order:
  1. **Sleeping** — a settled world should cost near zero.
     Dirty-rect-driven skipping is the single most important optimization; it's wired into the design, not bolted on.
  2. **Parallelism** — 4-phase block scheduling over rayon (server), ECS parallel systems.
  3. **Locality** — SoA where it matters, z-order storage, fixed-size cells, no per-cell heap.
  4. **Bandwidth** — dirty rects + postcard + lz4; measured in the debug overlay from day one.
- GPU compute for the sim is explicitly **out of scope**: authority lives on a headless server, and CPU sim with sleeping is proven sufficient at Noita scale.

## Open Questions

Not blocking the milestones below; each gets its own decision when it becomes load-bearing.

- **Lighting**: Terraria-style flood lighting vs. raycast; likely a client-side concern — decide before the "beautiful" pass.
- **Fluids beyond cellular automata**: pressure/velocity fields for large liquid bodies (Noita does a hybrid); revisit after CA liquids exist.
- **Temperature/fire spread plane**: separate SoA plane per chunk — design sketch exists (see Cell representation), scheduling TBD.
- **Modding surface**: data-driven materials/biomes are the first step; scripting is deliberately unplanned.
- **Auth/identity for public servers**: fine with join-by-address + name for now.
- **Browser certificate distribution**: WebTransport in browsers requires either a CA-trusted cert or `serverCertificateHashes` with ECDSA certs of ≤14-day validity, so community-hosted servers need cert rotation plus an out-of-band hash channel (e.g. a lobby HTTPS API) — or a WebSocket fallback for browsers.
  Decide by M4; native clients are unaffected.
- **Environmental pressure design** (day/night, seasons, weather à la DST): which cycles exist and how they interact with the material sim (rain refills aquifers? winter freezes water?) — decide when the survival layer lands (M6+).

## Milestones

Each milestone is playable/demoable and merges only with its tests.

- **M0 — Skeleton**: workspace scaffolding, CI (fmt, clippy, tests, wasm client build), empty crates with the dependency rules enforced.
- **M1 — Kernel**: `fallingsand_core` + `fallingsand_sim`: materials from RON, sand/water/gas behavior, dirty rects + sleeping working.
- **M2 — See it**: `fallingsand_client` renders a local `fallingsand_server` (embedded, in-memory transport): chunk textures with dirty-rect uploads, pan/zoom camera, a controllable player with grid collision, digging/placing materials.
  First playable.
- **M3 — Persist & generate**: `fallingsand_worldgen` pipeline + redb persistence; create/load/save worlds; region streaming via chunk tickets as the player moves.
- **M4 — Multiplayer**: WebTransport transport, protocol handshake, delta replication, prediction/interpolation, dedicated server binary, web client joins a native server.
- **M5 — Break it**: pixel bodies — flood-fill island detection, impulse-based dynamics with pixel-perfect terrain collision, settle-back, replicated to clients.
  The Noita moment.
- **M6 — Feel like a game**: main menu (worlds, server browser, settings), HUD, basic survival loop (health, a few tools/items), polish pass on rendering (palette work, particles for impacts).
