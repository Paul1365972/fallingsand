# fallingsand

A multiplayer Noita-like falling-sand game.
See [docs/Overview.md](docs/Overview.md) for the full design.

Play the latest web build at [paul1365972.github.io/fallingsand](https://paul1365972.github.io/fallingsand/).

## Workspace

```
assets/                   # Client assets
docs/                     # Design docs
crates/
├── fallingsand_math      # Tick/subcell constants and tick-seeded stateless randomness
├── fallingsand_material  # Runtime material schema
├── fallingsand_content   # Host-only typed content (materials, reactions, items, recipes, tuning); build-time codegen
├── fallingsand_core      # Shared foundation: coords, cells/chunks/regions, compile-time content
├── fallingsand_sim       # Simulation kernel: per-material CA passes, dirty rects, sleeping, physics
├── fallingsand_protocol  # All client↔server messages: serde types, framing, versioning
├── fallingsand_net       # Transport trait; backends: WebTransport (native + wasm), in-memory
├── fallingsand_worldgen  # Deterministic procedural generation
├── fallingsand_server    # Authoritative server library
├── fallingsand_dedicated # Headless `fallingsand_server` binary: CLI, certs, tracing
└── fallingsand_client    # Bevy app; builds the `fallingsand` binary (native + WASM)
```

## Development

```
cargo dev                                  # native client (dev mode)
cargo dev-server                           # dedicated headless server
cargo profile                              # native client, tracy-instrumented
cargo dist                                 # stripped, fat-LTO release binaries
bevy run --no-default-features -p fallingsand_client web   # web client, needs the bevy CLI
cargo run -p fallingsand_core --example gen_icons          # regenerate item/material icons
```

The client's default features are the dev set: `bevy/dynamic_linking`, `bevy/file_watcher`,
`bevy/debug`. Release, `dist`, and web builds drop them with `--no-default-features`.

Bevy CLI install:

```
cargo binstall --git https://github.com/TheBevyFlock/bevy_cli --version 0.1.0-alpha.2 --locked bevy_cli
```
