# Simulation Kernel

The CA (`fallingsand_sim`) collides against the cell grid directly — terrain changes never rebuild collision geometry.

## Scheduling

- **4-phase block scheduling**: chunks group into 2×2 blocks, run in 4 phases by block parity; a worker owns its block plus a one-chunk halo, and same-phase windows share no chunks — race-free, no locking.
- **Neighbourhood-complete**: a chunk simulates only when its whole 3×3 chunk neighbourhood is loaded; frontier chunks defer, keeping their rects.
- **Speed of light = 64**: no update reaches farther than 64 cells; longer-range effects propagate as local waves over ticks. A queued between-tick world-event list is the sanctioned escape hatch for true long-range effects — none exists today.
- Randomness is tick-seeded and stateless (`fallingsand_rng`).
- **Per-material kernels**: the kernel monomorphizes per material (`MatSpec`), so a cell's own properties are immediate constants and dead rule branches vanish (an inert solid's kernel is nearly empty); neighbours read generated exhaustive-match accessors. One movement kernel per phase, each taking exactly its phase's coefficients — a liquid kernel cannot read a repose angle. Integer-only: precomputed RNG thresholds, Q16 multipliers — grid determinism is independent of float semantics.

## Movement rules

Every cell carries a velocity, integrated locally each tick — no phase heuristics, no sweeps; a settled cell writes nothing. Per moving cell, in order:

- **Accelerate**: gravity (gases/fire rise) minus buoyancy from the displaced fluid, then `drag`; a lighter liquid under a denser one swaps up directly; rising gases get a mean-reverting `turbulence` sway.
- **Contact friction**: resting on a blocked face bleeds horizontal velocity by `friction`.
- **Cohesion**: velocity pulls toward the mean of like-phase neighbours — streams form coherent jets.
- **Traverse**: step cell-by-cell along the velocity (fractional speed by tick-seeded chance; capped at `MAX_STEP` = 31, keeping reach ≤ 64). Steps are cardinal — a diagonal needs an open orthogonal cell, so corners seal for free.
- **Collide & redirect**: a blocked face reflects by `restitution` (near-inelastic); blocked fall that can descend diagonally converts to sideways velocity by `redirect_keep` — ledge jets for liquids, angle-of-repose slides for powders (`repose`). A liquid that can't descend spreads one cell across a level surface with no velocity gain — flattening without injecting energy.
- **Settle**: velocity into a blocked face dies and sub-threshold velocity snaps to zero, so a supported cell nets no change and its chunk sleeps.

Leveling and pressure propagate as local waves over ticks. Steam condenses back to water so gas pockets resolve — no mass created or destroyed.

## Sleeping

Each chunk tracks a **sim rect** of cells to re-simulate; an empty rect **sleeps** (the biggest optimization), and a write to a sleeping chunk or its border wakes it. The rect is honest: exactly the cells iterated next tick. The **change rect** (⊆ `sim`) holds actual value changes: a write marks `change` tight and `sim` as the 3×3 Moore neighbourhood (dilating across chunk borders); a **keep-alive** mark (clinging fire, pending decay) extends `sim` by 1×1 only. Scheduling reads `sim`; replication reads `change`, so keep-alives cost zero bandwidth.

Cell particles (aspirational, not built): cells knocked loose fly ballistically as free particles and reinsert on impact.

## Tuning units

Constants are seconds-based, converted per-tick from `TICK_DT`, so behaviour is ~invariant to tick rate: rate `r` fires with `1−e^(−r·dt)` (keeps `e^(−r·dt)`), accelerations integrate as `a·dt`, durations become tick counts. The content compiler and `per_tick_threshold!` quantize them during compilation.

## Combustion

Burning is an **ember material**: each flammable fuel authors one `burn_variant: Burning { … }` block and gets a synthesized `burning_*` twin (same phase and dynamics, its own palette, `hot`+`emissive`) — nothing hand-mirrored, no per-cell state beyond the id. Three local stages:

- **Ignite**: any `hot` cell transmutes adjacent flammables into their embers at their `ignite` rate, keeping velocity and shade (igniting oil keeps flowing); ember spread into a sealed neighbour (no adjacent oxygen) scales by `smoulder` — 0 is surface-only (oil), higher burns through a sealed lump (coal) — while open flames (lava, `fire`) ignite sealed fuel at the full rate.
- **Burn**: an ember damages entities, emits `fire` into adjacent air at `emit`, and burns out at `rate` — that rate *is* the burn duration; `residue`/`residue_chance` leave ash, otherwise burnout resolves to `burnout` (smoke) so the front self-exposes to oxygen.
- **Flame**: `fire` is a hand-authored ember with no base fuel — a `hot` gas persisting beside fuel, burning out into `smoke`. One pipeline covers fuel and flame; there is no fire phase.

A `water` neighbour quenches: a flame just dies to steam, keeping the water; a fuel ember resolves to its residue (charring is never restored) and the water flashes to steam — dousing *spends* the water, so a puddle can only smother so much. Fuels sleep until lit, so an unlit forest costs nothing.
