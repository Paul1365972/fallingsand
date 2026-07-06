# fallingsand

Multiplayer Noita-like falling-sand game.
docs/Idea.md is the single source of truth: read it first; flag any deviation you make.
Old pre-rewrite code lives on the `legacy` branch: reference only, never copy forward.

## Commands

```
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all
cargo build -p fallingsand_client --target wasm32-unknown-unknown
cargo run -p fallingsand_client --features dev
```

## Ideology

- Conservation of mass: cells are never created or destroyed without a physical cause (a reaction, explicit consumption); overlaps and awkward states resolve by displacement, never by deleting whichever cell is inconvenient.
- Momentum is conserved in every exchange; restitution is a material property (0 ≤ e < 1) — a contact returns at most the energy it received, never more.
- One cell, one owner: double occupancy by terrain, rigid bodies, or entities is an architecture bug, not a tuning problem.
- Every matter-affecting system handles all matter kinds — grid cells, powders/fluids, rigid bodies, entities — or explicitly flags the gap (a fallen tree must burn).
- Physics is semi-realistic and phase-based; no artificial caps or clamps that distort it.
- Fix root causes: recurring sim/physics bugs get architectural reworks, not symptom patches.
- Boil-the-ocean mode: nothing is holy — rework any system, architecture, or protocol freely; no backwards compatibility or migrations, bump the version constants instead.
- Server-authoritative everything: clients send raw input and render interpolated state — no client prediction, no client-side gameplay logic.
- Every system needs an idle story: a settled world costs ~nothing — no per-tick work, no permanently-awake chunks, server or client.
- We want great, not good; Noita and Celeste are the benchmarks (worldgen taste: Terraria × Noita × modern Minecraft), and feel is judged by human playtest.

## Rules

- Dependency direction: `core ← sim ← {server, client}`, `core ← protocol ← {server, client}`.
- Only `fallingsand_client` may depend on Bevy; only `fallingsand_server` on redb; tokio: server plus the client's native-only target.
- No code comments, no doc comments (rare exceptions like `// SAFETY:`); docs are terse and standalone, no meta talk.
- Material data is RON in `data/materials.ron`; the sim dispatches on phase + properties, never material identity. Worldgen (biomes, bands, ores) is hardcoded Rust in `fallingsand_worldgen` until its design stabilizes.
- No tests unless asked; verify with clippy + build, then hand feel/UI verification to the user — never hack up self-verification.
- Big features are built as one unit and playtested once at the end — no placeholder milestones or demo scaffolding.
- Commit once at the end of a task (packets only when clearly separable): conventional subject, no body, no co-author; never push; leave the user's parallel WIP untouched.

## Sim invariants

- 4-phase 2×2-chunk-block scheduling; workers get a 4×4-chunk `SimWindow`, disjoint per phase.
- No update may reach beyond the window (speed of light = 64); longer effects use the `WorldEdit` queue.
- Change rects (`bounds`) feed replication/persistence; keep-alive rects (`keep_bounds`) only feed sim scheduling. `window.mark` for "simulate again", `window.set` for real changes.
- Sleep, unload, and reload must preserve pending activity — in-flight processes never freeze in time.
- Same seed + inputs → same world on one machine; randomness is tick-seeded FxHash, no RNG state, no iteration-order-dependent containers in sim paths.
- Pixel bodies are always rasterized: body flag ⇔ exactly one live body's raster covers the cell. The bodies pass is a serial post-CA stage using raw writes; every re-stamp is a plan-then-commit transaction that never half-fails. Public cell writes only produce unflagged cells and notify the owning body via the damage queue.
- Tuning constants are seconds-based, never per-tick.
