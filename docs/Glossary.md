# Glossary

Canonical names for the core domain vocabulary. One concept, one name.

| Term | Meaning |
|------|---------|
| **Cell / Chunk / Region** | 8-byte grid unit / 64×64 cells (dirty-tracking, sleeping, replication, rendering) / 8×8 chunks (generation, storage, load/unload) |
| **CellPos / ChunkPos / RegionPos** | `i32` x,y coordinates at each granularity |
| **sim / change** | per-chunk rects: `sim` (cells to re-sim next tick, feeds scheduling) ⊇ `change` (changed cells, feeds replication + persistence); double-buffered (`prev_`) |
| **Fixed** | Q54.10 `i64` fixed-point for continuous pose/velocity; saves only, never on the wire |
| **Cell velocity** | `i16` Q10 cells/tick, sim-only, persisted, never on the wire; clamped in-flow to ±31 cells/tick (`VEL_MAX`) |
| **Actor** | kinematic controller: players (creatures later) — `Fixed` accumulator pose; observable pose is its integer `Footprint` |
| **Footprint** | floor-anchored integer cell rect, pure function of `(floor(x), floor(y), extents)`; collision, raster, wire, hazards all read it |
| **PixelBody** | rigid body made of cells (pose + angle + spin + mass + raster); the only "Body" |
| **PlayerRaster / flesh** | the player's stamped grid presence: body-flagged inert `flesh` cells |
| **TickFrame** | the one frame sent per server tick: `tick`, `world_age`, chunks, players, inventory/self/debug |
| **ChunkOp** | per-chunk wire delta: `Load` / `Delta` / `Unload` |
| **PlayerState** | wire snapshot (integer cell pose, height, burning, facing) — anchor only; the body rides chunk deltas |
| **InputFrame** | per-client-tick input: held `InputState` (merged) + ordered one-shot `InputAction`s (never lost) |
| **PlayerId / PlayerUuid** | session player id / persistent account id |
| **tick / world_age** | monotonic sim tick / calendar clock (60-day year; `season()`/`day_of_year()` are integer math) |
| **SimWindow** | a worker's 4×4-chunk window: simulates the inner 2×2 block, reads one chunk beyond |
| **SPEED_OF_LIGHT** | max reach of one update = `CHUNK_SIZE` = 64 cells |
| **Dynamics** | per-material precomputed per-tick sim coefficients |
