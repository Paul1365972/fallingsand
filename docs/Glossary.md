# Glossary

Canonical names for the core domain vocabulary. One concept, one name.

| Term | Meaning |
|------|---------|
| **Cell / Chunk / Region** | 8-byte grid unit / 64x64 cells (dirty tracking, sleeping, replication, rendering) / 8x8 chunks (generation, storage, load/unload) |
| **CellPos / ChunkPos / RegionPos** | `i32` x,y coordinates at each granularity |
| **sim / change** | per-chunk rects: `sim` (cells to re-sim next tick, feeds scheduling) contains `change` (changed cells, feeds replication + persistence); double-buffered (`prev_`) |
| **Fixed** | Q53.10 `i64` fixed-point (10 fractional bits) for continuous pose/velocity; saves only, never on the wire |
| **Cell velocity** | `i16` Q10 cells/tick, sim-only, persisted, never on the wire; clamped in-flow to +/-31 cells/tick (`VEL_MAX`) |
| **Session** | one connection, handshake state, and replication baselines; server-local `SessionId` |
| **Player** | one authenticated person currently present: identity, profile, control, and exactly one lifecycle state |
| **PlayerProfile / PlayerControl** | avatar-independent durable properties / ephemeral accepted input and queued intents |
| **Avatar** | the physical realization owned only by `PlayerLife::Alive`: actor, raster, health, interaction, and deferred physical state |
| **AvatarSnapshot** | domain snapshot used to materialize or resume an avatar; storage records convert to it at the persistence boundary |
| **Actor** | kinematic controller for an avatar (creatures later); `Fixed` accumulator pose whose observable pose is its integer `Footprint` |
| **Footprint** | floor-anchored integer cell rect, pure function of `(floor(x), floor(y), extents)`; collision, raster, wire, hazards all read it |
| **PixelBody** | rigid body made of cells (pose + angle + spin + mass + raster); the only `Body` |
| **PlayerRaster / flesh** | a live avatar's stamped grid presence: body-flagged inert `flesh` cells |
| **TickFrame** | the one frame sent per server tick: `tick`, `world_age`, chunks, optional avatar states, inventory/self/debug |
| **ChunkOp** | per-chunk wire delta: `Load` / `Delta` / `Unload` |
| **PlayerState** | public wire state: `PlayerId` plus an optional live-avatar anchor; the raster rides chunk deltas |
| **SelfLife** | private wire lifecycle: `Entering`, `Alive(SelfAvatarState)`, `Dead`, or `Reviving` |
| **InputFrame** | per-client-tick input: held `InputState` (merged) + ordered one-shot `InputAction`s |
| **SessionId / PlayerId / PlayerUuid** | connection ID / current runtime-presence ID / durable authenticated identity and storage key |
| **tick / world_age** | monotonic sim tick / calendar clock (60-day year; `season()`/`day_of_year()` are integer math) |
| **SimWindow** | a worker's 4x4-chunk window: simulates the inner 2x2 block, reads one chunk beyond |
| **SPEED_OF_LIGHT** | max reach of one update = `CHUNK_SIZE` = 64 cells |
| **Dynamics** | per-material precomputed per-tick sim coefficients |
