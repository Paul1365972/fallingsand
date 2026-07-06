# fallingsand

Multiplayer Noita-like falling-sand game.
docs/Idea.md is the single source of truth: read it first; flag any deviation you make.

## Commands

```
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all
cargo build -p fallingsand_client --target wasm32-unknown-unknown
cargo run -p fallingsand_client --features dev
```

## Principles

- Quality bar: great, not good; Noita and Celeste are the benchmarks (worldgen taste: Terraria × Noita × modern Minecraft), and feel is judged by human playtest.
- Semi-realism: physics is semi-realistic and phase-based; no artificial caps or clamps that distort it.
- Fix root causes: recurring sim/physics bugs get architectural reworks, not symptom patches.
- Boil-the-ocean mode: nothing is holy — rework any system, architecture, or protocol freely; no backwards compatibility or migrations, bump the version constants instead.

## Invariants

- Conservation of mass: cells aren't created or destroyed without a physical cause (a reaction, explicit consumption); overlaps and awkward states should be impossible, or in rare cases resolve by displacement.
- Conservation of momentum: conserved in every exchange; restitution is a material property (0 ≤ e < 1) — a contact returns at most the energy it received.
- One cell, one owner: double occupancy by terrain, rigid bodies, or entities is an architecture bug, not a tuning problem.
- Universality: every matter-affecting system handles all matter kinds — grid cells, powders/fluids, rigid bodies, entities — or explicitly flags the gap (a fallen tree must burn).
- Server-authoritative: clients send raw input and render interpolated state — no client prediction, no client-side gameplay logic.
- Idle cost: a settled world costs ~nothing — no per-tick work, no permanently-awake chunks, server or client.
- Speed of light: no update may reach beyond its window (= 64); longer-range effects go through the `WorldEdit` queue.
- Locality: a reaction reads and writes only its immediate neighborhood.
- Determinism: same seed + inputs → same world on one machine.
- Suspend/resume: sleep, unload, and reload preserve pending activity — in-flight processes don't freeze in time.
- Body rasterization: body flag ⇔ exactly one live body's raster covers the cell; public cell writes only produce unflagged cells.

## Architecture

- Dependency direction: `core ← sim ← {server, client}`, `core ← protocol ← {server, client}`.
- Scheduling: 4-phase 2×2-chunk-block scheduling; workers get a 4×4-chunk `SimWindow`, disjoint per phase.
- Rects: change rects (`bounds`) feed replication/persistence; keep-alive rects (`keep_bounds`) only feed sim scheduling. `window.mark` for "simulate again", `window.set` for real changes.
- Randomness: tick-seeded, no RNG state, no iteration-order-dependent containers in sim paths.
- Tuning units: constants are seconds-based, not per-tick.

## Workflow

- No comments: no code comments, no doc comments (rare exceptions like `// SAFETY:`); docs are terse and standalone, no meta talk.
- Verification: no tests unless asked; verify with clippy + build, then hand feel/UI verification to the user rather than hacking up self-verification.
- Whole-feature builds: big features are built as one unit and playtested once at the end.
- Commits: once at the end of a task (packets only when clearly separable): conventional subject, no body, no co-author; don't push; leave the user's parallel WIP untouched.
