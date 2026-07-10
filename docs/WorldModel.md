# World Model

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | per-tick work: dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

Cells are 8-byte, heap-free (flat array per chunk): material, per-cell velocity (`vx`/`vy`), shade, body flag, and an `updated` tick byte (a cell moves at most once per tick). Every cell is a particle — velocity drives all grid movement (see [Simulation.md](Simulation.md)). Fire is a material phase, not a flag; a burning fuel is its own material whose probabilistic decay *is* its burn duration — no per-cell HP. Heavier per-cell state (e.g. temperature) would be a separate per-chunk plane most chunks skip.

Cell velocity is Q11.4 `i16` cells/s (clamped in-flow to ±2000 = `VEL_MAX`), sim-only: it persists in saves but never goes on the wire — clients render cell-snapped. Other continuous quantities (actor/body pose, velocity) are `Fixed` Q24.8, exact on the wire and in saves. Cell coordinates are `i32`, but `Fixed` pose caps the continuously addressable world at ~±8.3M cells. Storage keys use z-order region coords.

## Materials are data

`data/materials.ron` defines each material; `data/reactions.ron` defines pairwise reactions (tag operands + per-second rate, e.g. `fire + [woody] → fire + burning_wood`). A product of `@burnout` resolves to the reacting cell's own burnout — its `residue_into` at `residue_chance`, else its decay product — so one tag rule can end many materials each on their own terms. The kernel switches on **phase + properties**, never material identity — a new powder is a data edit, zero engine code. The server hashes the registry to detect client mismatch. `data/items.ron` (+ auto-generated material items) and `data/recipes.ron` layer the item model on top; see [Inventory.md](Inventory.md).

`flesh` is the player's body material: Solid, slightly denser than water, `player`-tagged — inert (no reactions reference it), undiggable (both dig paths skip the tag), never auto-itemized, and voided on region load as a crash artifact. Its 16-shade palette is the pixel-person pattern authored in `sim::player`.

### Field units

All tunables are **seconds-based**, converted per-tick at registry build (see Tuning units in [Simulation.md](Simulation.md)):

| Field | Unit | Meaning |
|-------|------|---------|
| `density` | relative mass | buoyancy/sinking (air 1.2, water 1000, stone 2600) |
| `drag` | rate `1/s` | velocity damping through the medium |
| `friction` | rate `1/s` | horizontal velocity bled while resting on a face |
| `repose` | rate `1/s` | powder angle-of-repose slide events (0 = stacks vertically) |
| `redirect_keep` | 0..1 | fraction of blocked-fall velocity redirected sideways (1 = frictionless) |
| `cohesion` | rate `1/s` | pull of velocity toward like-phase neighbour mean |
| `turbulence` | `cells/s·√s` | random swirl-kick intensity for rising gas/fire |
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

Rates are strong per-second values: rate `r` retains `e^(−r·dt)` (or fires with `1−e^(−r·dt)`) each tick, so sand's `friction: 48` bleeds within a few ticks at 60 Hz. Reaction `rate` in `reactions.ron` is likewise `1/s`; `∞` = instant.
