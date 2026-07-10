# Simulation Kernel

The CA (`fallingsand_sim`) collides against the cell grid directly — terrain changes never rebuild collision geometry.

## Scheduling

- **4-phase block scheduling**: chunks group into 2×2 blocks; each tick runs 4 phases by block parity. A worker owns its block and reads/writes one chunk beyond it; same-phase windows share no chunks, so rayon workers hold disjoint mutable access — race-free, no locking.
- **Neighbourhood-complete**: a chunk simulates only when its whole 3×3 chunk neighbourhood is loaded; a chunk at the loaded frontier defers (keeping its rects) until its neighbours arrive, so the kernel never reads unloaded cells.
- **Speed of light = 64**: no update reaches farther than 64 cells. Longer-range effects propagate as local waves over ticks; a true long-range effect (teleport, scripted event) would go through a queued world-event list applied between ticks — none exists today.
- Randomness is tick-seeded and stateless (`fallingsand_rng`).

## Movement rules

Every cell carries a velocity (Q11.4 cells/s), integrated locally each tick — no phase-specific heuristics, no sweeps; a settled cell writes nothing. Per moving cell, in order:

- **Accelerate**: gravity (`GRID_GRAVITY`; powders/liquids fall, gases/fire rise) reduced by buoyancy from the fluid being displaced, then `drag` — amplified in a dense medium. The driving force can reach zero (neutral buoyancy) without stalling flow: leveling is driven by redirect, not velocity. A lighter liquid trapped under a denser one swaps up directly. Rising gases/fire also get a `turbulence` kick — tick-seeded horizontal velocity bled by drag into a mean-reverting sway.
- **Contact friction**: while resting on a blocked face, horizontal velocity bleeds by `friction`.
- **Cohesion**: velocity is pulled toward the mean of like-phase neighbours (read-only) — a fast stream drags its neighbours into a coherent jet.
- **Traverse**: step cell-by-cell along the velocity (fractional speed via tick-seeded chance; per-tick displacement capped at `MAX_STEP` = 32, keeping update reach ≤ 64 = one chunk halo). Steps are cardinal — a diagonal needs an open orthogonal cell, so corners seal for free.
- **Collide & redirect**: a blocked face reflects that velocity component by `restitution` (near-inelastic). A cell that can't advance but can descend diagonally converts blocked fall to sideways velocity scaled by `redirect_keep` — ledge jets for liquids, the angle-of-repose slide for powders (gated per grain by `repose` with RNG jitter). A liquid that can't descend also spreads one cell across a level surface with no velocity gain — this flattens liquids without injecting energy.
- **Settle**: velocity into a blocked face is killed and sub-threshold velocity snaps to zero, so a supported cell nets no change and its chunk sleeps.

Leveling, spreading, and pressure propagate as local waves over successive ticks — a change marks its 3×3 neighbourhood into the sim rect, never a scan. Condensation closes the loop: steam decays back to water so gas pockets resolve, no mass created or destroyed.

## Sleeping

Each chunk tracks a **sim rect** of cells to re-simulate; the sim skips empty rects (**sleeping** — the biggest optimization). A write to a sleeping chunk or its border wakes it. The sim rect is honest: exactly the cells the kernel iterates next tick, no read-time dilation.

The **change rect** (`change` ⊆ `sim`) holds cells whose value changed. A write marks `change` tight and `sim` as the 3×3 Moore neighbourhood (dilating across chunk borders). A **keep-alive** mark (clinging fire, pending decay, reactive pairs) extends `sim` by 1×1 only. Scheduling reads `sim`; replication reads `change`, so keep-alives cost zero bandwidth.

Cell particles (aspirational): cells knocked loose fly ballistically as free particles and reinsert on impact. Not built — a future store must carry `Fixed` velocity, not the grid's Q11.4.

## Tuning units

Constants are seconds-based, converted per-tick from `TICK_DT`, so behaviour is ~invariant to tick rate. Rates (`1/s`): `per_tick_chance` = `1−e^(−r·dt)`; `per_tick_keep` = `e^(−r·dt)`. Accelerations (`cells/s²`) integrate as `a·dt`. Turbulence scales by `√dt`. Durations (`s`) become tick counts. Restitution, density ratios, and redirect gain are dimensionless.

## Combustion

Three material-driven stages, all local — state lives in the material id, no per-cell burn timer.

- **Ignite**: a flame or ember reacts with adjacent fuel at that fuel's own rate (`fire + oil` near-instant, `fire + coal` slow), turning it into its `burning_*` variant. Ignition needs oxygen (an adjacent air/gas/fire cell); each fuel's `smoulder` (0..1) scales ignition *without* one — 0 is surface-only (oil), higher lets a sealed lump burn through (coal).
- **Burn**: the ember stays in place, spreads through adjacent like fuel, glows (`emissive`), burns entities (`hot`), and is consumed by its own `decay` — the decay rate *is* the burn duration. Consumption mostly gasifies the fuel (void/smoke) so the burn front self-exposes to oxygen; only `residue_chance` leaves solid `ash`.
- **Flame**: an ember `emits` short-lived `fire` into adjacent air (licking flames + smoke plume); fire is `sustained_by` embers, then decays to smoke.

Water quenches an adjacent ember: the water flashes to steam and the ember burns out early (`@burnout`, ash at its own `residue_chance`) — dousing a big fire spends water. Fuels sleep until a hot neighbour wakes them, so an unlit forest costs nothing.
