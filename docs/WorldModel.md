# World Model

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | per-tick work: dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

Cells are 8-byte, heap-free (flat array per chunk): material, per-cell velocity, shade, a body flag, and an `updated` tick byte (a cell moves at most once per tick). Every cell is a particle — velocity drives all grid movement. Burning is a material, not a flag: a lit fuel transmutes to its synthesized `burning_*` twin, and probabilistic burnout *is* the burn duration — no per-cell HP. Heavier per-cell state (e.g. temperature) would be a separate per-chunk plane most chunks skip.

Cell velocity is Q5.10 `i16` cells/tick (10 fractional bits), sim-only: persisted, never on the wire — clients render cell-snapped. Other continuous quantities are `Fixed`, Q53.10 over `i64` (10 fractional bits), exact in saves and never on the wire. Cell coordinates are `i32`; storage keys use z-order region coords.

## Materials are data

Content lives in `fallingsand_content` as ordinary typed Rust grouped by domain. Definition functions fill one ordered build-time `Catalog`; they may use loops and helpers, while typed phase builders expose only relevant tuning. A tiny key macro supplies navigable UPPER symbols. Definitions can inherit an earlier material and override selected properties.

The host-only compiler validates names, references, inheritance, reactions, and units; synthesizes burning twins; converts per-second tuning to integer tick constants; and generates dense ids, UPPER runtime handles, exhaustive accessors, reaction rows, item sources, and one `MatSpec` per material. Combustion is not a reaction: a flammable definition authors one `flammable` block and receives a synthesized `burning` twin (see [Simulation.md](Simulation.md)).

Items and recipes are authored the same way in the same crate: typed builders fill the catalog, and the compiler assigns the whole item-id space (explicit items then one auto-item per itemizable material), emits static `ItemInfo`/`Recipe` tables and `item`/`item_for_material`/`item_id_of` accessors, and resolves recipes to concrete ids. There is no runtime registry object; item queries are plain generated functions like materials.

The kernel is driven by **phase + properties** (a new powder is a data edit, zero engine code) but freely names specific materials where identity is clearest. The generated zero-sized spec types keep kernels monomorphized. The authoring crate is not linked into either binary; both consume the same generated core content. See [Inventory.md](Inventory.md).

`FLESH` is the player's body material: inert, undiggable, never auto-itemized, voided on region load as a crash artifact; its shade palette is the pixel-person pattern.

### Field units

All tunables are seconds-based, converted per-tick and quantized to integers at compile time. Movement knobs live inside the phase block — `Powder { drag, friction, repose(start, keep), redirect_keep, cohesion }`, `Liquid { … flow_rate }`, `Gas { … turbulence }`, `Solid { rigid(bond_group) }` — so a field a phase doesn't simulate is a compile error. Top-level fields cover `density`, `restitution`, entity surface feel (`surface_grip`/`surface_bounce`), `hardness`, and `contact_damage`. Combustion is scoped the same way: fuels author `flammable: Flammable { ignite, sealed_burn, rate, emit, colors, residue, residue_chance, burnout, damage }`, hand flames (fire) author `burning: Burning { rate, emit, residue, residue_chance, burnout }` — a burn field outside its block is a compile error. Rates are strong per-second values: rate `r` fires with `1−e^(−r·dt)` each tick; an outsized rate (`1e9`) fires effectively every tick.
