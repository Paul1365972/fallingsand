# Simulation Kernel

The CA (`fallingsand_sim`) collides against the cell grid directly — terrain changes never rebuild collision geometry.

## Scheduling

- **4-phase block scheduling**: chunks group into 2×2 blocks, run in 4 phases by block parity; a worker owns its block plus a one-chunk halo, and same-phase windows share no chunks — race-free, no locking.
- **Neighbourhood-complete**: a chunk simulates only when its whole 3×3 chunk neighbourhood is loaded; frontier chunks defer, keeping their rects.
- **Speed of light = 64**: no update reaches farther than 64 cells; longer-range effects propagate as local waves over ticks. A queued between-tick world-event list is the sanctioned escape hatch for true long-range effects — none exists today.
- **Random ticks**: a second simulation, its own 4-phase pass after the interaction sim, scoped to a bounded chunk range around each player rather than the full loaded/ticketed world. Each chunk in range samples `RANDOM_TICKS_PER_CHUNK` cells (seeded by tick and chunk) and runs the ambient kernel on each. It is reserved infrastructure for future sleep-independent processes (plant growth, decay); nothing uses it today — ignition lives entirely in the interaction sim.
- Randomness is tick-seeded and stateless (`fallingsand_rng`).
- **Per-material kernels**: the kernel monomorphizes per material (`MatSpec`), so a cell's own properties are immediate constants and dead rule branches vanish (an inert solid's kernel is nearly empty); neighbours read generated exhaustive-match accessors. One movement kernel per phase, each taking exactly its phase's coefficients — a liquid kernel cannot read a repose angle. Integer-only: precomputed RNG thresholds, Q16 multipliers — grid determinism is independent of float semantics.

## Movement rules

Every cell carries a velocity, integrated locally each tick — no phase heuristics, no sweeps; a settled cell writes nothing. Per moving cell, in order:

- **Accelerate**: gravity (gases/fire rise) minus buoyancy from the displaced fluid, then `drag`; a lighter liquid under a denser one swaps up directly; rising gases get a mean-reverting `turbulence` sway.
- **Contact friction**: resting on a blocked face bleeds horizontal velocity by `friction`.
- **Cohesion**: velocity pulls toward the mean of like-phase neighbours — streams form coherent jets.
- **Traverse**: step cell-by-cell along the velocity (fractional speed by tick-seeded chance; capped at `MAX_STEP` = 31, keeping reach ≤ 64). Steps are cardinal — a diagonal needs an open orthogonal cell, so corners seal for free.
- **Collide & redirect**: a blocked face reflects by `restitution` (near-inelastic); blocked fall that can descend diagonally converts to sideways velocity by `redirect_keep` — ledge jets for liquids, angle-of-repose slides for powders. Powder repose is a static/kinetic friction pair: a stationary grain holds any slope until a moving powder or liquid neighbour agitates it (start rate), while a moving grain keeps sliding readily (keep rate) and carries enough velocity to agitate its own neighbours — avalanches propagate through motion and die out as motion does, never through sleep accidents. A liquid that can't descend spreads one cell across a level surface with no velocity gain — flattening without injecting energy.
- **Settle**: velocity into a blocked face dies and sub-threshold velocity snaps to zero, so a supported cell nets no change and its chunk sleeps.

Leveling and pressure propagate as local waves over ticks. Steam condenses back to water so gas pockets resolve — no mass created or destroyed.

## Sleeping

Each chunk tracks a **sim rect** of cells to re-simulate; a write to a chunk or its border marks it. Sleep gates the interaction sim: an empty sim rect skips the chunk, so a settled world does no movement work; the random-tick sim ignores sleep. The rect is honest: exactly the cells iterated next tick. The **change rect** (⊆ `sim`) holds actual value changes: a write marks `change` tight and `sim` as the 3×3 Moore neighbourhood (dilating across chunk borders); a **keep-alive** mark (burning fuel, pending decay, a same-tick update skip) extends `sim` by 1×1 only. Scheduling reads `sim`; replication reads `change`, so keep-alives cost zero bandwidth.

Sleeping is a pure optimization: evaluating a chunk's full area and evaluating only its sim rect must produce identical results. A rule whose outcome depends on anything but its marked neighbourhood, or pending stochastic work that goes quiet without a keep-alive, is a bug — stationary cells act only on deterministic conditions of their neighbours, and every state that can still act re-marks itself until resolved.

Cell particles (aspirational, not built): cells knocked loose fly ballistically as free particles and reinsert on impact.

## Tuning units

Constants are seconds-based, converted per-tick from `TICK_DT`, so behaviour is ~invariant to tick rate: rate `r` fires with `1−e^(−r·dt)` (keeps `e^(−r·dt)`), accelerations integrate as `a·dt`, durations become tick counts. The content compiler quantizes authored seconds into integer tick constants during compilation. Random-tick rates scale by `CHUNK_AREA / RANDOM_TICKS_PER_CHUNK` (`per_random_tick_chance`) so a seconds-rate means the same real time under random ticks, clamped at 1.0.

## Combustion

Burning is a material, not a flag: each flammable fuel authors one `flammable: Flammable { … }` block and gets a synthesized `burning_*` twin (same phase and dynamics, its own palette, `hot` with baked emission) — nothing hand-mirrored, no per-cell state beyond the id. Three local stages:

- **Ignite**: every `hot` cell — flame, burning fuel, or lava — transmutes adjacent flammables into their burning twins at their `ignite` rate in the interaction sim, keeping velocity and shade (igniting oil keeps flowing). A hot↔flammable adjacency is pending work and keeps itself marked until it resolves, so a settled lava shore still catches a log that lands on it while a lava lake with no fuel beside it sleeps for free. Burning fuel igniting its own material is the propagation front; free flames add plume and secondary ignition above.
- **Burn**: a burning cell damages entities, emits `fire` into adjacent air at `emit`, and burns out at `rate` — that rate *is* the burn duration; `residue`/`residue_chance` leave ash, otherwise burnout resolves to `burnout` (smoke) so the front self-exposes to oxygen.
- **Sealed**: without adjacent oxygen, ignition scales by the fuel's `sealed_burn` fraction and burning follows it monotonically — `sealed_burn > 0` smoulders at the scaled rate; `sealed_burn = 0` means the fuel needs air: it never catches while sealed and a sealed burning cell snuffs back to its unburned material (oil). A sealed free flame burns out into `smoke`.

A `water` neighbour quenches: a flame just dies to steam, keeping the water; a burning fuel resolves to its residue (charring is never restored) and the water flashes to steam — dousing *spends* the water, so a puddle can only smother so much. Fuels sleep until lit, so an unlit forest costs nothing.
