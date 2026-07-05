# fallingsand

Multiplayer Noita-like falling-sand game.
docs/Idea.md is the single source of truth: read it first; flag any deviation you make.
Old pre-rewrite code lives on the `legacy` branch: reference only, never copy forward.

## Commands

```
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all
cargo build -p fallingsand_client --target wasm32-unknown-unknown
cargo run -p fallingsand_client --features dev
```

## Rules

- Dependency direction: `core ← sim ← {server, client}`, `core ← protocol ← {server, client}`.
- Only `fallingsand_client` may depend on Bevy; only `fallingsand_server` on redb. tokio: server, plus the client's native-only target (WebTransport dialer runtime).
- No code comments, no doc comments; only very few exceptions like `// SAFETY:` on unsafe blocks for example.
- Conventional commits.
- Game data is RON in `data/`; the sim dispatches on phase + properties, never material identity.

## Sim invariants

- 4-phase 2×2-chunk-block scheduling; workers get a 4×4-chunk `SimWindow`, disjoint per phase.
- No update may reach beyond the window (speed of light = 64); longer effects use the `WorldEdit` queue.
- Chunks sleep via double-buffered dirty rects; writes to sleeping chunks must `normalize_updated` first (helpers do this).
- Change rects (`bounds`) feed replication/persistence; keep-alive rects (`keep_bounds`) only feed sim scheduling. `window.mark` for "simulate again", `window.set` for real changes — never mark `bounds` without a cell write.
- `Cell` stays exactly 4 bytes; extra per-cell state becomes separate SoA planes later.
- Same seed + inputs → same world on one machine; randomness is tick-seeded FxHash, no RNG state.

## Ecosystem facts (verified 2026-07, training data is stale)

- Bevy 0.19: required components (no bundles), `Message`/`MessageReader` for buffered events, `On<E>` observers, Resource is a Component, `2d`/`ui`/`audio` feature collections, `web` feature for wasm.
- Standalone bevy_ecs needs the `multi_threaded` feature for the parallel executor.
- Web builds use Bevy CLI (`bevy build web --bundle`), not Trunk for now.

