# Glossary

Canonical names for the core domain vocabulary. One concept, one name.

## Grid & coordinates

| Term | Meaning |
|------|---------|
| **Cell** | 8-byte grid unit (`core::cell::Cell`): `material: MaterialId`, `vx`/`vy: i16` (Q11.4 cells/s), `shade_flags`, `updated` |
| **Chunk** | 64×64 cells; unit of dirty-tracking, sleeping, replication, rendering |
| **Region** | 8×8 chunks; unit of generation, storage, load/unload |
| **CellPos / ChunkPos / RegionPos** | `i32` x,y coordinates at cell / chunk / region granularity |
| **CellOffset / ChunkOffset** | `u8` in-parent index (cell-in-chunk, chunk-in-region) |
| **sim / change** | per-chunk rects: `sim` (exact cells to re-sim next tick = 3×3 Moore neighbourhood of each change + 1×1 keep-alives, feeds scheduling) ⊇ `change` (changed cells, feeds replication + persistence); double-buffered (`prev_`) |

## Numbers & units

| Term | Meaning |
|------|---------|
| **Fixed** | Q24.8 `i32` fixed-point for continuous pose/velocity (actors, bodies); on the wire and in saves |
| **Cell velocity** | `Cell.vx/vy` = `i16` Q11.4 **cells/s**, sim-only, never sent on the wire |
| **VEL_MAX** | in-flow clamp on cell velocity: ±2000 cells/s (storage range is ±2047) |
| **GRID_GRAVITY / BODY_GRAVITY / PlayerParams.gravity** | grid-CA / pixel-body / player-controller gravity; y is **up** (falling = negative vy) |

## Movers

| Term | Meaning |
|------|---------|
| **Actor** | kinematic controller (`sim::physics::Actor`): players (creatures later) — `Fixed` accumulator pose, velocity, half-extents, `on_ground`; observable pose is its integer `Footprint` |
| **Footprint** | floor-anchored integer cell rect of an actor — pure function of `(floor(x), floor(y), extents)`; collision, raster, wire, hazards all read it |
| **PixelBody** | rigid body made of cells (pose + angle + spin + mass + raster); the only "Body" |
| **PlayerRaster / flesh** | the player's stamped grid presence: a `PlayerStamp` (rect raster + duck + facing) of body-flagged inert `flesh` cells |
| **PlayerActor** | server ECS component wrapping an `Actor` |
| **ActorAabb / ActorDynamics** | actor collision proxies handed to the pixel-body pass |

## Protocol & ids

| Term | Meaning |
|------|---------|
| **TickFrame** | the one frame sent per server tick: `tick`, `world_age`, `chunks`, `players`, inventory/cursor/trash/self/debug |
| **ChunkOp** | per-chunk wire delta inside a `TickFrame`: `Load` / `Delta` / `Unload` |
| **PlayerState** | wire snapshot of a player (integer cell pose, ducking, burning, facing) — anchor only; the body rides chunk deltas |
| **InputFrame** | per-client-tick input message: held `InputState` (latest-wins, merged) + ordered one-shot `InputAction`s (never lost) |
| **PlayerId / PlayerUuid** | session player id / persistent account id |
| **tick / world_age** | monotonic sim tick number / calendar clock (DAY_UNITS; YEAR_UNITS = 60 days, `season()`/`day_of_year()` are integer-math accessors) |

## Sim internals

| Term | Meaning |
|------|---------|
| **SimWindow** | a worker's 4×4-chunk window; simulates the inner 2×2 block and reads one chunk beyond |
| **SPEED_OF_LIGHT** | max reach of one update = `CHUNK_SIZE` = 64 cells |
| **Dynamics** | per-material precomputed per-tick sim coefficients (drag_keep, friction_keep, cohesion, restitution, redirect_keep, …) |
