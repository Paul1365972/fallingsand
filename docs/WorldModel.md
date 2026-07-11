# World Model

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | per-tick work: dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

Cells are 8-byte, heap-free (flat array per chunk): material, per-cell velocity (`vx`/`vy`), shade, a body flag, and an `updated` tick byte (a cell moves at most once per tick). Every cell is a particle — velocity drives all grid movement (see [Simulation.md](Simulation.md)). Burning is a material, not a flag: a lit fuel transmutes to its synthesized `burning_*` ember (same phase and dynamics), consumed by its burn profile — that probabilistic burnout *is* its burn duration, no per-cell HP. Heavier per-cell state (e.g. temperature) would be a separate per-chunk plane most chunks skip.

Cell velocity is Q11.4 `i16` cells/s (clamped in-flow to ±2000 = `VEL_MAX`), sim-only: it persists in saves but never goes on the wire — clients render cell-snapped. Other continuous quantities (actor/body pose, velocity) are `Fixed` Q24.8, exact on the wire and in saves. Cell coordinates are `i32`, but `Fixed` pose caps the continuously addressable world at ~±8.3M cells. Storage keys use z-order region coords.

## Materials are data

`fallingsand_data` defines each material as a plain `Material { … }` struct literal through the `materials!` macro, which injects the `name` and `..Material::DEFAULT` so only non-default fields are written — no `..DEFAULT` boilerplate, no hand-written id. Domain files (`material/terrain.rs`, `fluids.rs`, `fire.rs`, …) are ordered by the one-line `domains!` list in `material/mod.rs`, which fixes each domain's id range; a material's dense `MaterialId` and its idiomatic UPPER handle (`material::STONE`) come from declaration order — adding a material is a single edit to one file, adding a domain is a file plus one token in `domains!`. Reactions are pairwise material↔material transmutations written `lava + water => stone + steam @ 97.0` (material/tag operands, per-second rate). Combustion is **not** a reaction: a flammable material carries a *burn profile* and the registry synthesizes its `burning_*` ember twin (same phase/dynamics, `burn_colors` palette, `hot`+`emissive` tags, no auto-item) — the kernel ignites fuel into its ember (per `flammability`, gated by `smoulder`/oxygen), emits `fire` + burns out to `residue_into`/air (per `burn_rate`), and quenches to residue on a `water` neighbour — one authored profile combusts every fuel on its own terms with no hand-written mirror materials. `fire` is the one hand-authored ember (no base fuel): a `hot` gas sustained beside fuel, burning out into `smoke`. The kernel is driven by **phase + properties** (a new powder is a data edit, zero engine code) but freely names a specific material via `fallingsand_data::material::*` where identity is clearest — there is no material-identity-agnostic rule. Content is compiled into both binaries and identical by construction; `PROTOCOL_VERSION` gates client/server compatibility. Named items and recipes layer the item model on top; see [Inventory.md](Inventory.md).

`FLESH` is the player's body material: Solid, slightly denser than water, `player`-tagged — inert (no reactions reference it), undiggable (both dig paths skip the tag), never auto-itemized, and voided on region load as a crash artifact. Its 16-shade palette is the pixel-person pattern authored in `sim::player`.

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
| `decay_rate` | rate `1/s` | probabilistic decay to `decay_into` (smoke/steam dissipate) |
| `ember` | bool | this material *is* combustion state (`fire`; set implicitly on synthesized `burning_*`) |
| `flammability` | rate `1/s` | ignition rate when touched by a `hot` cell (0 = inert; > 0 synthesizes the ember) |
| `burn_rate` | rate `1/s` | ember burnout (this *is* the burn duration) |
| `burn_emit` | rate `1/s` | rate the ember emits `fire` into adjacent air |
| `burn_colors` | palette | the ember's colors (default: shared flame ramp) |
| `smoulder` | 0..1 | readiness to ignite without adjacent oxygen (0 = surface-only) |
| `residue_into` / `residue_chance` | id / 0..1 | per-burnout chance to leave a residue (else air) |
| `burn_damage` | HP `/s` | the ember's `contact_damage` |

Rates are strong per-second values: rate `r` retains `e^(−r·dt)` (or fires with `1−e^(−r·dt)`) each tick, so sand's `friction: 48` bleeds within a few ticks at 60 Hz. Reaction `rate` in `fallingsand_data` is likewise `1/s`; `∞` = instant.
