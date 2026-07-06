# World Model

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | per-tick work: dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

Cells are small and heap-free (flat array per chunk). A cell holds its material plus visual shade and a few sim flags (flow direction, flow-spent, body membership). Fire is a material phase, not a flag; burn is probabilistic decay, not per-cell HP. Anything heavier (e.g. temperature) would be a separate per-chunk plane most chunks skip, never a fatter cell.

Continuous quantities (entity/body positions, velocities) are `Fixed` Q24.8 — exact everywhere in the infinite world, on the wire, and in saves. Storage keys use z-order region coords.

## Materials are data

`data/materials.ron` defines phase, density, restitution, palette, tags, decay, and reactions (per-pair probability + tag operands, e.g. `fire + [burnable] → fire + fire`). The kernel switches on **phase + properties**, never material identity — a new powder is a data edit, zero engine code. The server hashes the registry to detect client mismatch.
