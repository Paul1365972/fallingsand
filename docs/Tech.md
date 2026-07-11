# Crates & Dependencies

```
fallingsand_core      # Coords, cells/chunks/regions, material/item registry types + builders
fallingsand_data      # Hardcoded content: materials, reactions, items, recipes + id/tag constants
fallingsand_sim       # CA kernel, dirty rects, sleeping, physics
fallingsand_protocol  # Client↔server messages
fallingsand_net       # Transport trait: WebTransport (native + wasm), in-memory
fallingsand_worldgen  # Procedural generation
fallingsand_rng       # Tick-seeded stateless randomness (splitmix64)
fallingsand_server    # Authoritative server: library + dedicated binary
fallingsand_client    # Plain-Rust game core + bevy IO shell (game/ vs view/); builds native + WASM
```

Direction: `core ← data ← {sim, worldgen, server, client}`, `core ← sim ← server`, `core ← protocol ← {server, client}`; the client reaches the sim only through the embedded server. `fallingsand_data` is the single compiled-in content set — `core` holds the engine types and registry builders, `data` holds the definitions plus `material::*`/`item::*` id handles that let the kernel, worldgen, and gameplay name materials directly (no runtime name lookup, no data file). `fallingsand_rng` is a dependency-free leaf (splitmix64) used by `sim` and `worldgen`.

- **Content scales by file** — `fallingsand_data`'s `materials!`, `reactions!`, `items!`, `recipes!` macros (in `macros.rs`) turn declarations into typed data. A material is one `NAME = Material { … }` entry in a domain file (`material/terrain.rs`, `fluids.rs`, `fire.rs`, …); the `materials!` macro injects the `name` and `..Material::DEFAULT`, and derives the dense id + UPPER handle (`material::STONE`) from declaration order — no `..DEFAULT` boilerplate, no separate roster, no hand-written id. The `domains!` list in `material/mod.rs` is the single ordered place that fixes id ranges and assembles the registry (arithmetic base offsets, no name scan). Reactions read `lava + water => stone + steam @ 97.0`; recipes `1 material::WOOD => 4 material::PLANKS`. Adding a material is one edit to one file; adding a domain is a new file plus one token in `domains!`.
- **Client stays WASM-clean** — the browser build is join-only, so rayon, storage, and the embedded server compile out for wasm. CI builds the client for `wasm32-unknown-unknown`.
- Only the client depends on Bevy; only the server depends on redb.
- One transport trait spans WebTransport and the in-memory pipe, so single player runs the real protocol, not a shortcut.
