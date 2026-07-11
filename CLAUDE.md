# fallingsand

Multiplayer Noita-like falling-sand game.
docs/ holds the intended design — goals, ideas, invariants, not detailed spec: start at docs/Overview.md (it indexes the rest). Docs can lag the code; reconcile deviations you spot (fix whichever is wrong).

## Commands

```
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all
```

## Principles

- Quality bar: great, not good; Noita and Celeste are the benchmarks (worldgen taste: Terraria × Noita × modern Minecraft); feel is judged by human playtest.
- Semi-realism: phase-based, semi-realistic physics; no artificial caps or clamps that distort it.
- Fix root causes: recurring sim/physics bugs get architectural reworks, not symptom patches.
- Boil-the-ocean mode: nothing is holy — rework any system, architecture, or protocol freely; no backwards compatibility or migrations, bump the version constants.

## Invariants

- Conservation of mass: cells aren't created or destroyed without a physical cause; overlaps should be impossible, or in rare cases resolve by displacement.
- Velocity is per-cell and dissipative: drag, contact friction, and restitution (0 ≤ e < 1) make all motion terminate and settle; feel and locality win over strict momentum bookkeeping.
- One cell, one owner: double occupancy by terrain, rigid bodies, or entities is an architecture bug, not a tuning problem.
- Universality: every matter-affecting system handles all matter kinds — grid cells, rigid bodies, entities — or explicitly flags the gap (a fallen tree must burn).
- Server-authoritative: clients send raw input and render replicated state — no prediction, no client-side gameplay logic.
- Idle cost: a settled world costs ~nothing — no per-tick work, no permanently-awake chunks.
- Locality & speed of light: every update stays within its 64-cell window — no sweeps, no action at a distance; longer-range effects propagate as local waves over ticks.
- Determinism: same seed + inputs → same world on one machine.
- Suspend/resume: sleep, unload, and reload preserve pending activity — in-flight processes don't freeze in time.
- Body rasterization: body flag ⇔ exactly one live body's or player's raster covers the cell; public cell writes only produce unflagged cells. The player is a grid resident — its cells are as real as any terrain.

## Architecture

- Crates and dependency direction: docs/Tech.md. Content compiles from `fallingsand_core/content/` via the `content!` proc macro; the grid kernel is monomorphized per material and integer-only.
- Scheduling: 4-phase 2×2-chunk-block scheduling, disjoint 4×4-chunk `SimWindow`s per phase. Rects: `sim` (scheduling) ⊇ `change` (replication/persistence); `sim` is honest — exactly the cells simulated next tick (docs/Simulation.md).
- Randomness: tick-seeded, stateless, no iteration-order-dependent containers in sim paths. Tuning constants are seconds-based, not per-tick.

## Workflow

- No comments: no code or doc comments (rare exceptions like `// SAFETY:`); docs are terse and standalone, no meta talk.
- Verification: no tests unless asked; clippy + build, then hand feel/UI verification to the user.
- Whole-feature builds: big features are built as one unit and playtested once at the end.
- Commits: once at the end of a task: conventional subject, no body, no co-author; don't push; leave the user's parallel WIP untouched.
