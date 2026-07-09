# fallingsand

Multiplayer Noita-like falling-sand game.
docs/ is the source of truth for intended design: start at docs/Overview.md (it indexes the rest). It can lag the code, so read the relevant doc first and reconcile any deviation you spot (fix the doc or the code, whichever is wrong).

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
- Velocity is per-cell and dissipative: every cell carries a velocity; motion is bled by drag, contact friction, and restitution (0 ≤ e < 1) so it always terminates and settles. Momentum is not strictly conserved across exchanges — feel and locality win over bookkeeping.
- One cell, one owner: double occupancy by terrain, rigid bodies, or entities is an architecture bug, not a tuning problem.
- Universality: every matter-affecting system handles all matter kinds — grid cells, powders/fluids, rigid bodies, entities — or explicitly flags the gap (a fallen tree must burn).
- Server-authoritative: clients send raw input and render interpolated state — no client prediction, no client-side gameplay logic.
- Idle cost: a settled world costs ~nothing — no per-tick work, no permanently-awake chunks, server or client.
- Speed of light: no update may reach beyond its window (= 64); longer-range effects go through the `WorldEdit` queue.
- Locality: every update — reaction or movement — reads and writes only its immediate neighborhood. No sweeps, no scanning for distant targets, no action at a distance; long-range effects propagate as local waves over ticks or go through the `WorldEdit` queue.
- Determinism: same seed + inputs → same world on one machine.
- Suspend/resume: sleep, unload, and reload preserve pending activity — in-flight processes don't freeze in time.
- Body rasterization: body flag ⇔ exactly one live body's raster covers the cell; public cell writes only produce unflagged cells.

## Architecture

- Dependency direction: `core ← sim ← {server, client}`, `core ← protocol ← {server, client}`.
- Scheduling: 4-phase 2×2-chunk-block scheduling; workers get a 4×4-chunk `SimWindow`, disjoint per phase.
- Rects: `sim` (feeds scheduling) ⊇ `change` (feeds replication/persistence); `set` marks `change` tight + `sim` as the changed cell's 3×3 Moore neighbourhood (across chunk borders), `mark_keep` marks `sim` 1×1 only. `sim` is honest — the exact cells simulated next tick, no read-time dilation. Double-buffered (`prev_*`, `swap_rects`). `window.mark` for "simulate again", `window.set` for real changes.
- Randomness: tick-seeded, no RNG state, no iteration-order-dependent containers in sim paths.
- Tuning units: constants are seconds-based, not per-tick.

## Workflow

- No comments: no code comments, no doc comments (rare exceptions like `// SAFETY:`); docs are terse and standalone, no meta talk.
- Verification: no tests unless asked; verify with clippy + build, then hand feel/UI verification to the user rather than hacking up self-verification.
- Whole-feature builds: big features are built as one unit and playtested once at the end.
- Commits: once at the end of a task (packets only when clearly separable): conventional subject, no body, no co-author; don't push; leave the user's parallel WIP untouched.
