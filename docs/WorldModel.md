# World Model

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | per-tick work: dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

Cells are 8-byte, heap-free (flat array per chunk): material, per-cell velocity, shade, a body flag, and an `updated` tick byte (a cell moves at most once per tick). Every cell is a particle — velocity drives all grid movement. Burning is a material, not a flag: a lit fuel transmutes to its synthesized `burning_*` ember, and probabilistic burnout *is* the burn duration — no per-cell HP. Heavier per-cell state (e.g. temperature) would be a separate per-chunk plane most chunks skip.

Cell velocity is Q10 `i16` cells/tick, sim-only: persisted, never on the wire — clients render cell-snapped. Other continuous quantities are `Fixed`, Q10 over `i64`, exact in saves and never on the wire. Cell coordinates are `i32`; storage keys use z-order region coords.

## Materials are data

Content lives in `fallingsand_core/content/` as plain `NAME = Material { … }` definition files (one per domain), compiled by the `content!` proc macro: dense ids and UPPER handles (`content::material::STONE`) from declaration order, per-second rates converted to per-tick integer constants, every check a compile error naming the offending definition. Adding a material is one edit to one file; a definition can extend an earlier one with struct-update syntax (`..WOOD`).

`content/reactions.rs` holds every transmutation: pairwise `LAVA + WATER => STONE + STEAM @ 97.0` (material/tag operands, per-second rate) and decays `STEAM => WATER @ 0.1`. Combustion is **not** a reaction: a flammable material authors a *burn profile* and the macro synthesizes its ember twin (see [Simulation.md](Simulation.md)).

The kernel is driven by **phase + properties** (a new powder is a data edit, zero engine code) but freely names specific materials where identity is clearest. The macro emits one zero-sized spec type per material; the kernel monomorphizes over these. Content is compiled into both binaries, identical by construction; `PROTOCOL_VERSION` gates compatibility. Items and recipes layer on top; see [Inventory.md](Inventory.md).

`FLESH` is the player's body material: inert, undiggable, never auto-itemized, voided on region load as a crash artifact; its shade palette is the pixel-person pattern.

### Field units

All tunables are seconds-based, converted per-tick and quantized to integers at compile time. Movement knobs live inside the phase block — `Powder { drag, friction, repose, redirect_keep, cohesion }`, `Liquid { … flow_rate }`, `Gas { … turbulence }`, `Solid { rigid_capable }` — so a field a phase doesn't simulate is a compile error. Top-level fields cover `density`, `restitution`, entity surface feel (`surface_grip`/`surface_bounce`), `hardness`, `contact_damage`, and the burn profile (`flammability`, `burn_rate`, `burn_emit`, `burn_colors`, `smoulder`, `residue_into`/`residue_chance`, `burnout_into`, `burn_damage`). Rates are strong per-second values: rate `r` fires with `1−e^(−r·dt)` each tick; an outsized rate (`1e9`) fires effectively every tick.
