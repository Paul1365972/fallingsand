# Simulation Kernel

The CA (`fallingsand_sim`) collides against the cell grid directly, so terrain changes never rebuild collision geometry.

## Scheduling

- **4-phase block scheduling**: chunks group into 2×2 blocks; each tick runs 4 phases by block parity. A worker owns its block and reads/writes one chunk beyond it. Same-phase windows share no chunks, so rayon workers hold disjoint mutable access — race-free, no locking.
- **Speed of light = 64**: no update reaches farther than 64 cells or it escapes the window. Longer-range effects (explosions, placement) go through a world-edit queue applied between phases.
- Randomness is tick-seeded and stateless (`fallingsand_rng`); no RNG state in the hot path.

## Movement rules

Each has a physical cause, and each is written so a settled cell writes nothing (and sleeps).

- **Corner sealing**: a diagonal move needs an open orthogonal path — two diagonally touching solids seal the gap.
- **Drop-seeking flow**: liquids move sideways only toward a lower spot (gases mirrored); `dispersion` caps travel per event, momentum carries the rest. Flat surface → no gradient → pools go quiet.
- **Fall speed / viscosity**: free fall runs at full `fall_speed` (gravity is material-independent); sideways/diagonal/percolation moves gate on `flow_rate`. Lava oozes, water pours.
- **Momentum flag** (direction + spent bits): only descending clears spent, so motion always terminates.
- **Surface creep**: a directed liquid cell over liquid glides downhill until it rests — this levels pools regardless of width. Droplets on solids bead (surface tension); viscous fluids keep ±1 texture.
- **Pressure wake**: a vacated cell or opened passage marks a keep-alive strip so nearby sleepers re-evaluate; strips die when motion stops. Leveling is exact only within the 64-cell window — farther apart, a bead and a dimple persist as ±1 texture.
- **Condensation**: steam decays back to water so gas pockets resolve; no mass created or destroyed.

## Sleeping

Each chunk tracks the dirty rect of cells changed last tick. Everything keys off it: the sim skips empty rects (**sleeping** — the biggest optimization), replication and rendering touch only the rect. A write to a sleeping chunk or its border wakes it.

A separate **keep-alive rect** marks cells that must re-simulate without having changed (clinging fire, pending decay, reactive pairs). The sim schedules from both; replication reads only the change rect, so keep-alives cost zero bandwidth. This is why a mostly-settled world of ~2000 active chunks stays inside the tick budget.

Cell particles: cells knocked loose fly ballistically as free particles and reinsert into the grid on impact.
