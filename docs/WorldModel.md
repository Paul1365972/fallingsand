# World Model

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | per-tick work: dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

Cells are 8-byte, heap-free (flat array per chunk): material, a per-cell velocity (`vx`/`vy`), visual shade, and a body-membership flag. Every cell is a particle — velocity drives all grid movement (see [Simulation.md](Simulation.md)). Fire is a material phase, not a flag; burn is probabilistic decay, not per-cell HP — a burning fuel is its own material whose slow/fast decay *is* its burn duration (see Combustion in [Simulation.md](Simulation.md)). Heavier still (e.g. temperature) would be a separate per-chunk plane most chunks skip.

Cell velocity is `Q12.4` `i16` cells/second (4 fractional bits, ±2047 cells/s), sim-only: it drives movement and persists in saves (suspend/resume) but is never sent on the wire — clients render cell-snapped and don't need it. Other continuous quantities (entity/body positions, velocities) are `Fixed` Q24.8 — exact everywhere in the infinite world, on the wire, and in saves. Storage keys use z-order region coords.

## Materials are data

`data/materials.ron` defines each material; `data/reactions.ron` defines pairwise reactions (tag operands + per-second rate, e.g. `fire + [woody] → fire + burning_wood`). The kernel switches on **phase + properties**, never material identity — a new powder is a data edit, zero engine code. The server hashes the registry (materials + reactions) to detect client mismatch. `data/items.ron` (+ auto-generated material items) and `data/recipes.ron` layer the item model on top; see [Inventory.md](Inventory.md). Region blobs (`REGION_FORMAT_VERSION`) and player records (`WORLD_FORMAT_VERSION`) carry dropped-item and per-slot inventory state respectively.

### Field units

All tunables are **seconds-based**, converted per-tick at registry build (see Tuning units in [Simulation.md](Simulation.md)):

| Field | Unit | Meaning |
|-------|------|---------|
| `density` | relative mass | buoyancy/sinking (air 1.2, water 1000, stone 2600) |
| `drag` | rate `1/s` | velocity damping through the medium (`e^(−drag·dt)` kept/tick) |
| `friction` | rate `1/s` | horizontal velocity bled while resting on a face |
| `repose` | rate `1/s` | powder angle-of-repose slide events (0 = stacks vertically) |
| `redirect_keep` | 0..1 | fraction of blocked-fall velocity redirected sideways (ledge jets; 1 = frictionless) |
| `cohesion` | rate `1/s` | pull of velocity toward like-phase neighbour mean (coherent jets) |
| `turbulence` | `cells/s·√s` | random swirl-kick intensity for rising gas/fire (per-tick kick ∝ `√dt`) |
| `flow_rate` | rate `1/s` | liquid spread across a level surface (0 = doesn't level) |
| `restitution` | 0..1 | collision bounce (inelastic) |
| `surface_grip` | 0..1 | entity-controller traction on the surface (ice ≈ 0.05) |
| `surface_bounce` | 0..1 | entity-controller bounce off the surface |
| `hardness` | scale | bare-handed dig resistance |
| `contact_damage` | HP `/s` | damage to entities in contact |
| `decay_rate` | rate `1/s` | probabilistic decay (this *is* the burn duration); `decay_into` = product |
| `emit_rate` | rate `1/s` | rate an ember `emits` fire into adjacent air |
| `smoulder` | 0..1 | readiness to ignite without adjacent oxygen (0 = surface-only) |
| `residue_chance` | 0..1 | per-decay chance to leave `residue_into` (else void/smoke) |

`friction`/`repose`/`cohesion`/`flow_rate` look large (e.g. sand `friction: 48`) because they are strong per-second rates: a rate `r` retains `e^(−r·dt)` (or fires with `1−e^(−r·dt)`) each tick, so `r ≈ 48` means near-total bleed within a few ticks at 60 Hz. Reaction `rate` (in `reactions.ron`) is likewise `1/s`, with `∞` meaning instant.
