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

Direction: `material ← {macros, core}`, `macros ← core ← {sim, worldgen, server, client}`, `core ← sim ← server`, `core ← protocol ← {server, client}`; the client reaches the sim only through the embedded server. `fallingsand_material` is the single source of truth the proc macro and the runtime share (enums, content structs, `TICK_RATE`, quantization); `core::material` re-exports it.

- **Content is compiled in**: definition files under `fallingsand_core/content/` feed the single `content!` invocation in `core::content`, which emits id handles, exhaustive-match accessors, and per-material `MatSpec` types — no runtime name lookup, no registry object. See [WorldModel.md](WorldModel.md).
- **Client stays WASM-clean** — the browser build is join-only; rayon, storage, and the embedded server compile out for wasm. CI builds the client for `wasm32-unknown-unknown`.
- Only the client depends on Bevy; only the server depends on redb.
- One transport trait spans WebTransport and the in-memory pipe, so single player runs the real protocol, not a shortcut.
