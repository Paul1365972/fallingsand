# Crates & Dependencies

```
fallingsand_math      # Tick/subcell constants and tick-seeded stateless randomness
fallingsand_material  # Runtime material schema
fallingsand_content   # Host-only typed definitions, validation, quantization, codegen
fallingsand_core      # Coords, cells/chunks/regions, generated content module
fallingsand_sim       # CA kernel, dirty rects, sleeping, physics
fallingsand_protocol  # Client↔server messages
fallingsand_net       # Transport trait: WebTransport (native + wasm), in-memory
fallingsand_worldgen  # Procedural generation
fallingsand_server    # Authoritative server: library + dedicated binary
fallingsand_client    # Plain-Rust game core + bevy IO shell (game/ vs view/); native + WASM
```

Direction: `{math, material} ← content ← core(build)`, `{math, material} ← core`, `math ← {sim, worldgen, server}`, `core ← {sim, worldgen, protocol, server, client}`, `sim ← server`, `protocol ← {server, client}`; the client reaches the sim only through the embedded server.

- Content compiles in during the core build — see [Content.md](Content.md).
- The client stays WASM-clean: the browser build is join-only; rayon, storage, and the embedded server compile out. CI builds for `wasm32-unknown-unknown`.
- Only the client depends on Bevy; only the server depends on redb.
- One transport trait spans WebTransport and the in-memory pipe, so single player runs the real protocol, not a shortcut.

## Verifying cell rules

Verify behavior with a temporary example (deleted before commit) that drives the real kernel:

- Build a `CellWorld`, insert fresh chunks one chunk beyond the scenario on every side (a chunk simulates only with its full 3×3 loaded), place cells with `fill_material` / `clear_cell`, and step with `step_scoped(&mut world, &|_| true, &|_| true)` — keep the random-tick closure on, it is part of behavior.
- Measure, don't eyeball: print regions top-down (Y is up), count cells per material for conservation, track per-column tops for leveling, and check `awake_counts()` to prove settling actually sleeps.
- For realistic coverage, place the example in `fallingsand_server` and insert `WorldGenerator::generate_region` output — multiple bodies on real terrain expose scheduling and wake bugs that single-basin tubs cannot.
