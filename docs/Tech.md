# Crates & Dependencies

```
fallingsand_core      # Coords, cells/chunks/regions, material registry
fallingsand_sim       # CA kernel, dirty rects, sleeping, physics
fallingsand_protocol  # Client↔server messages
fallingsand_net       # Transport trait: WebTransport (native + wasm), in-memory
fallingsand_worldgen  # Procedural generation
fallingsand_rng       # Tick-seeded stateless randomness (splitmix64)
fallingsand_server    # Authoritative server: library + dedicated binary
fallingsand_client    # Bevy app; builds native + WASM
```

Direction: `core ← sim ← {server, client}`, `core ← protocol ← {server, client}`. `fallingsand_rng` is a dependency-free leaf (splitmix64) used by `core`, `sim`, and `worldgen`.

- **Client stays WASM-clean** — the browser build is join-only, so rayon, storage, and the embedded server compile out for wasm. CI builds the client for `wasm32-unknown-unknown`.
- Only the client depends on Bevy; only the server depends on redb.
- One transport trait spans WebTransport and the in-memory pipe, so single player runs the real protocol, not a shortcut.
