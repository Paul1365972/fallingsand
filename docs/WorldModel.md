# World Model

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | per-tick work: dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

Cells are 8-byte, heap-free (flat array per chunk): material, a per-cell velocity (`vx`/`vy`), visual shade, and a body-membership flag. Every cell is a particle — velocity drives all grid movement (see [Simulation.md](Simulation.md)). Fire is a material phase, not a flag; burn is probabilistic decay, not per-cell HP — a burning fuel is its own material whose slow/fast decay *is* its burn duration (see Combustion in [Simulation.md](Simulation.md)). Heavier still (e.g. temperature) would be a separate per-chunk plane most chunks skip.

Cell velocity is `Q8.8` `i16` cells/tick, sim-only: it drives movement and persists in saves (suspend/resume) but is never sent on the wire — clients render cell-snapped and don't need it. Other continuous quantities (entity/body positions, velocities) are `Fixed` Q24.8 — exact everywhere in the infinite world, on the wire, and in saves. Storage keys use z-order region coords.

## Materials are data

`data/materials.ron` defines phase, density, restitution, palette, tags, decay, emission, and reactions (per-pair probability + tag operands, e.g. `fire + [woody] → fire + burning_wood`). The kernel switches on **phase + properties**, never material identity — a new powder is a data edit, zero engine code. The server hashes the registry to detect client mismatch.
