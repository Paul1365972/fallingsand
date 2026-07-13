# Crates & Dependencies

```
fallingsand_material  # Shared leaf: MaterialId/Phase/Tag/Tags, content structs, TICK_RATE, quantization
fallingsand_content   # Host-only typed definitions, validation, quantization, and generated registries
fallingsand_macros    # Small literal-to-constant proc macros used by the simulation
fallingsand_core      # Coords, cells/chunks/regions, content module (compile-time materials/items/recipes)
fallingsand_sim       # CA kernel (per-material monomorphized), dirty rects, sleeping, physics
fallingsand_protocol  # Client↔server messages
fallingsand_net       # Transport trait: WebTransport (native + wasm), in-memory
fallingsand_worldgen  # Procedural generation
fallingsand_rng       # Tick-seeded stateless randomness (splitmix64)
fallingsand_server    # Authoritative server: library + dedicated binary
fallingsand_client    # Plain-Rust game core + bevy IO shell (game/ vs view/); builds native + WASM
```

Direction: `material ← {content, macros, core}`, `{material, rng} ← content ← core(build)`, `rng ← {macros, sim, worldgen, client}`, `macros ← sim`, `core ← {sim, worldgen, server, client}`, `core ← sim ← server`, `core ← protocol ← {server, client}`; the client reaches the sim only through the embedded server. `fallingsand_material` is the shared runtime vocabulary; `core::material` re-exports it.

- **Content is compiled in**: ordinary typed Rust definitions build an ordered host-side catalog. `fallingsand_core/build.rs` validates and quantizes it, then emits id handles, exhaustive-match accessors, and per-material `MatSpec` types — no runtime name lookup or registry object. See [WorldModel.md](WorldModel.md).
- **Client stays WASM-clean** — the browser build is join-only; rayon, storage, and the embedded server compile out for wasm. CI builds the client for `wasm32-unknown-unknown`.
- Only the client depends on Bevy; only the server depends on redb.
- One transport trait spans WebTransport and the in-memory pipe, so single player runs the real protocol, not a shortcut.
