# Crates & Dependencies

```
fallingsand_material  # Shared leaf: MaterialId/Phase/Tag/Tags, content structs, TICK_RATE, quantization
fallingsand_macros    # content! proc macro: parses content/ files, runs the registry build at compile time
fallingsand_core      # Coords, cells/chunks/regions, content module (compile-time materials/items/recipes)
fallingsand_sim       # CA kernel (per-material monomorphized), dirty rects, sleeping, physics
fallingsand_protocol  # Client↔server messages
fallingsand_net       # Transport trait: WebTransport (native + wasm), in-memory
fallingsand_worldgen  # Procedural generation
fallingsand_rng       # Tick-seeded stateless randomness (splitmix64)
fallingsand_server    # Authoritative server: library + dedicated binary
fallingsand_client    # Plain-Rust game core + bevy IO shell (game/ vs view/); builds native + WASM
```

Direction: `material ← {macros, core}`, `macros ← core ← {sim, worldgen, server, client}`, `core ← sim ← server`, `core ← protocol ← {server, client}`; the client reaches the sim only through the embedded server. `fallingsand_material` is the single source of truth the proc macro and the runtime share — the enums, the content structs, `TICK_RATE`, and the quantization math — so the macro computes with the exact types and constants the game runs on; `core::material` re-exports it. Content is compiled in: definition files under `fallingsand_core/content/` (outside `src/`, not Rust modules) feed the single `content!` invocation in `core::content`, which emits id handles (`content::material::STONE`, `content::item::STICK`), exhaustive-match accessors (`content::phase(id)`, `content::density_milli(id)` — the compiler turns them into lookup tables and mask tests), and per-material `MatSpec` types — the kernel, worldgen, and gameplay name materials directly (no runtime name lookup, no registry object). `fallingsand_rng` is a dependency-free leaf (splitmix64) used by `sim` and `worldgen`.

- **Content scales by file** — the `content!` proc macro turns declarations into typed constants. A material is one `NAME = Material { … }` entry in a domain file (`content/materials/terrain.rs`, `fluids.rs`, `fire.rs`, …); only non-default fields are written, and the dense id + UPPER handle derive from file/declaration order (the ordered file list in the invocation fixes id ranges). All validation (unknown fields, missing colors, ambiguous reactions) happens at compile time with errors naming the file and definition. Reactions read `LAVA + WATER => STONE + STEAM @ 97.0`; recipes `1 material::WOOD => 4 material::PLANKS` (declarative macros in `core::content`). Adding a material is one edit to one file; adding a domain is a new file plus one path in the invocation. Edits to content files retrigger expansion via emitted `include_str!` tracking.
- **Client stays WASM-clean** — the browser build is join-only, so rayon, storage, and the embedded server compile out for wasm. CI builds the client for `wasm32-unknown-unknown`.
- Only the client depends on Bevy; only the server depends on redb.
- One transport trait spans WebTransport and the in-memory pipe, so single player runs the real protocol, not a shortcut.
