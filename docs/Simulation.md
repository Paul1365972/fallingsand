# Simulation Kernel

The CA (`fallingsand_sim`) collides against the cell grid directly, so terrain changes never rebuild collision geometry.

## Scheduling

- **4-phase block scheduling**: chunks group into 2×2 blocks; each tick runs 4 phases by block parity. A worker owns its block and reads/writes one chunk beyond it. Same-phase windows share no chunks, so rayon workers hold disjoint mutable access — race-free, no locking.
- **Speed of light = 64**: no update reaches farther than 64 cells or it escapes the window. Longer-range effects (explosions, placement) go through a world-edit queue applied between phases.
- Randomness is tick-seeded and stateless (`fallingsand_rng`); no RNG state in the hot path.

## Movement rules

Every cell carries a velocity (`vx`/`vy`, Q8.8 cells/tick). Movement is that velocity integrated locally each tick — no phase-specific heuristics, no sweeps. Each step is written so a settled cell writes nothing (and sleeps). Per moving cell, in order:

- **Accelerate**: gravity (global `GRAVITY`; powders/liquids fall, gases/fire rise), reduced by buoyancy from the fluid being displaced (a dense grain sinks slowly through water, snow floats), then `drag` — amplified in a dense medium, so things settle slowly underwater instead of dropping straight through. Crucially the *driving* force can go to zero (a same-density parcel is neutrally buoyant) without stalling flow: leveling is driven by **redirect** below, not by velocity. A lighter liquid trapped under a denser one floats up by a direct swap.
- **Contact friction**: while resting on a blocked face, horizontal velocity bleeds by `friction`. Low-friction water keeps its momentum and shoots off a ledge; high-friction sand loses it and dribbles 1–2 cells.
- **Cohesion**: velocity is pulled toward the mean of like-phase neighbours (`cohesion`, read-only) — a fast stream drags its neighbours into a coherent jet; powders barely couple.
- **Traverse**: step cell-by-cell along the velocity (fractional speed resolved by a tick-seeded chance, capped at 8 cells/tick). Steps are cardinal, so a diagonal needs an open orthogonal cell — corners seal for free.
- **Collide & redirect**: a blocked face reflects that velocity component by `restitution` (near-inelastic, so things settle). A cell that can't advance but can descend diagonally does so, converting blocked fall to sideways velocity scaled by `(1 − friction)` — ledges and jets for liquids, the angle-of-repose slide for powders (gated per-grain by `friction` with RNG jitter, so piles are irregular and each powder stacks differently). A liquid that can't descend also spreads one cell across a level surface — no velocity gain, so it injects no energy — which is what collapses a liquid to a flat top instead of a powder-like pile.
- **Settle**: velocity into a blocked face is killed and sub-threshold velocity snaps to zero, so a supported cell nets no change and its chunk sleeps.

Leveling, spreading, and pressure all propagate as local waves over successive ticks — a dirty cell wakes its immediate neighbours via the change-rect spill, never a scan. **Condensation** still closes the loop: steam decays back to water so gas pockets resolve, no mass created or destroyed.

## Sleeping

Each chunk tracks the dirty rect of cells changed last tick. Everything keys off it: the sim skips empty rects (**sleeping** — the biggest optimization), replication and rendering touch only the rect. A write to a sleeping chunk or its border wakes it.

A separate **keep-alive rect** marks cells that must re-simulate without having changed (clinging fire, pending decay, reactive pairs). The sim schedules from both; replication reads only the change rect, so keep-alives cost zero bandwidth. This is why a mostly-settled world of ~2000 active chunks stays inside the tick budget.

Cell particles: cells knocked loose fly ballistically as free particles and reinsert into the grid on impact.

## Combustion

Burning is three material-driven stages, all local, no per-cell burn timer — state lives in the material id. **Ignite**: a flame or ember reacts with adjacent fuel at that fuel's own rate (`fire + oil` near-instant, `fire + coal` slow), turning the fuel into its `burning_*` variant. **Burn**: the ember stays in place, spreads through adjacent like fuel (`burning_coal + coal`), glows (`emissive`), burns entities (`hot`), and is consumed by its own `decay` into ash or smoke — a tiny rate is a long life (coal smoulders for ~30s), a huge rate is gone instantly (foliage). Probabilistic decay *is* the burn duration. **Flame**: an ember `emits` short-lived `fire` into an adjacent air cell (the licking flames + smoke plume); fire is `sustained_by` embers so it clings while fuel remains, then decays to smoke. Water quenches embers to ash. Fuels sleep until a hot neighbour wakes them, so an unlit forest costs nothing.
