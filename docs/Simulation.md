# Simulation Kernel

The CA (`fallingsand_sim`) collides against the cell grid directly, so terrain changes never rebuild collision geometry.

## Scheduling

- **4-phase block scheduling**: chunks group into 2×2 blocks; each tick runs 4 phases by block parity. A worker owns its block and reads/writes one chunk beyond it. Same-phase windows share no chunks, so rayon workers hold disjoint mutable access — race-free, no locking.
- **Neighbourhood-complete**: a chunk simulates only when its whole 3×3 chunk neighbourhood is loaded; one at the loaded frontier defers (keeping its rects, like a frozen chunk) until its neighbours arrive, so the kernel never reads unloaded cells.
- **Speed of light = 64**: no update reaches farther than 64 cells or it escapes the window. Longer-range effects (explosions, placement) go through a world-edit queue applied once after the four phases.
- Randomness is tick-seeded and stateless (`fallingsand_rng`); no RNG state in the hot path.

## Movement rules

Every cell carries a velocity (`vx`/`vy`, Q11.4 cells/second). Movement is that velocity integrated locally each tick (per-tick displacement = velocity ÷ tick rate) — no phase-specific heuristics, no sweeps. Each step is written so a settled cell writes nothing (and sleeps). Per moving cell, in order:

- **Accelerate**: gravity (global `GRID_GRAVITY`; powders/liquids fall, gases/fire rise), reduced by buoyancy from the fluid being displaced (a dense grain sinks slowly through water, snow floats), then `drag` — amplified in a dense medium, so things settle slowly underwater instead of dropping straight through. Crucially the *driving* force can go to zero (a same-density parcel is neutrally buoyant) without stalling flow: leveling is driven by **redirect** below, not by velocity. A lighter liquid trapped under a denser one floats up by a direct swap. Rising gases and fire also carry a per-material `turbulence`: a tick-seeded horizontal velocity kick, bled by drag into a gentle mean-reverting sway, so smoke wisps and curls instead of climbing in a rigid column.
- **Contact friction**: while resting on a blocked face, horizontal velocity bleeds by `friction`. Low-friction water keeps its momentum and shoots off a ledge; high-friction sand loses it and dribbles 1–2 cells.
- **Cohesion**: velocity is pulled toward the mean of like-phase neighbours (`cohesion`, read-only) — a fast stream drags its neighbours into a coherent jet; powders barely couple.
- **Traverse**: step cell-by-cell along the velocity (fractional speed resolved by a tick-seeded chance, per-tick displacement capped at `MAX_STEP` = 32 cells). That cap is the speed-of-light budget: a cell update reaches `MAX_STEP` + neighbour reads ≤ 64 (one chunk halo), so it can never exceed ~62. Steps are cardinal, so a diagonal needs an open orthogonal cell — corners seal for free.
- **Collide & redirect**: a blocked face reflects that velocity component by `restitution` (near-inelastic, so things settle). A cell that can't advance but can descend diagonally does so, converting blocked fall to sideways velocity scaled by `redirect_keep` — ledges and jets for liquids, the angle-of-repose slide for powders (gated per-grain by `repose` with RNG jitter, so piles are irregular and each powder stacks differently). A liquid that can't descend also spreads one cell across a level surface — no velocity gain, so it injects no energy — which is what collapses a liquid to a flat top instead of a powder-like pile.
- **Settle**: velocity into a blocked face is killed and sub-threshold velocity snaps to zero, so a supported cell nets no change and its chunk sleeps.

Leveling, spreading, and pressure all propagate as local waves over successive ticks — a dirty cell wakes its immediate neighbours via the change-rect spill, never a scan. **Condensation** still closes the loop: steam decays back to water so gas pockets resolve, no mass created or destroyed.

## Sleeping

Each chunk tracks a **sim rect** of cells to re-simulate; the sim skips empty rects (**sleeping** — the biggest optimization). A write to a sleeping chunk or its border wakes it.

The **change rect** (`change` ⊆ `sim`) holds cells whose value changed. A write marks both; a **keep-alive** mark (clinging fire, pending decay, reactive pairs) extends `sim` only. Scheduling reads `sim`; replication reads `change`, so keep-alives cost zero bandwidth — a mostly-settled world of ~2000 active chunks stays inside the tick budget.

Cell particles (aspirational): cells knocked loose would fly ballistically as free particles and reinsert on impact. Not yet built — a future store must carry `Fixed` cells/s velocity, not the grid's `Q11.4` `i16` (whose ±2047 cells/s storage range, clamped to ±2000 in flow, is for in-grid movement).

## Tuning units

Constants are seconds-based; a small vocabulary converts each kind to per-tick from `TICK_DT`, so behaviour is ~invariant to tick rate. Rates (`1/s`): `per_tick_chance` = `1−e^(−r·dt)` (reactions, decay, emit, flow, powder repose slide, flicker, and the cohesion blend); `per_tick_keep` = `e^(−r·dt)` (drag, contact-friction bleed). Accelerations (`cells/s²`, e.g. `GRID_GRAVITY`) integrate as `a·dt` into cells/s velocity. Turbulence is a random-walk kick intensity scaled by `√dt`. Durations (`s`) become tick counts via `s·TICK_RATE`. Restitution, density ratios, and redirect gain are dimensionless — no conversion.

## Combustion

Burning is three material-driven stages, all local, no per-cell burn timer — state lives in the material id. **Ignite**: a flame or ember reacts with adjacent fuel at that fuel's own rate (`fire + oil` near-instant, `fire + coal` slow), turning the fuel into its `burning_*` variant. Ignition needs oxygen — a neighbouring air, gas, or fire cell — but each fuel's `smoulder` (0..1) scales how readily it lights *without* one: `0` is surface-only (oil burns only where its pool meets air), higher values let a sealed lump light from the inside at a reduced rate (coal burns clean through). **Burn**: the ember stays in place, spreads through adjacent like fuel (`burning_coal + coal`), glows (`emissive`), burns entities (`hot`), and is consumed by its own `decay` — a tiny rate is a long life (coal smoulders for ~30s), a huge rate is gone instantly (foliage). Probabilistic decay *is* the burn duration. Consumption mostly gasifies the fuel, leaving a void (or smoke) so the burn front self-exposes the next cell to oxygen; only a fraction (`residue_chance`) leaves solid `ash`, so combustion no longer chokes on its own residue. **Flame**: an ember `emits` short-lived `fire` into an adjacent air cell (the licking flames + smoke plume); fire is `sustained_by` embers so it clings while fuel remains, then decays to smoke. Water quenches embers to ash. Fuels sleep until a hot neighbour wakes them, so an unlit forest costs nothing.
