# Crates & Dependencies

```
fallingsand_material  # Shared leaf: material vocabulary, tick constants, quantization
fallingsand_content   # Host-only typed definitions, validation, quantization, codegen
fallingsand_core      # Coords, cells/chunks/regions, generated content module
fallingsand_sim       # CA kernel, dirty rects, sleeping, physics
fallingsand_protocol  # Client↔server messages
fallingsand_net       # Transport trait: WebTransport (native + wasm), in-memory
fallingsand_worldgen  # Procedural generation
fallingsand_rng       # Tick-seeded stateless randomness
fallingsand_server    # Authoritative server: library + dedicated binary
fallingsand_client    # Plain-Rust game core + bevy IO shell (game/ vs view/); native + WASM
```

Direction: `material ← {content, core}`, `{material, rng} ← content ← core(build)`, `rng ← {sim, worldgen, client}`, `core ← {sim, worldgen, server, client}`, `core ← sim ← server`, `core ← protocol ← {server, client}`; the client reaches the sim only through the embedded server.

- Content compiles in during the core build — see [Content.md](Content.md).
- The client stays WASM-clean: the browser build is join-only; rayon, storage, and the embedded server compile out. CI builds for `wasm32-unknown-unknown`.
- Only the client depends on Bevy; only the server depends on redb.
- One transport trait spans WebTransport and the in-memory pipe, so single player runs the real protocol, not a shortcut.

## Profiling

- **Every build:** the server times each tick phase; the F3 overlay shows it (embedded), the dedicated server logs sim/tick times.
- **Dev + profiling builds:** per-render-pass CPU/GPU timings feed the overlay (CPU-only on WebGPU).
- **Tracy (native):** `cargo profile` / `cargo profile-server` build the perf profile (release + symbols) and stream tracing spans to the Tracy GUI; `dist` compiles all spans out. samply/Superluminal also work on any perf binary.
