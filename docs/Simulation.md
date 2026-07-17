# Simulation

The world is one cellular automaton: every pixel is matter. Physics is phase-based and semi-realistic — natural behavior over artificial caps and clamps. Players and rigid bodies live in the same grid and everything collides against it directly, so terrain changes never rebuild collision geometry.

## Invariants

- **Conservation of mass** — cells are created or destroyed only by a physical cause.
- **One cell, one owner** — a cell belongs to terrain, one rigid body, or one player raster; double occupancy is an architecture bug, not a tuning problem.
- **Determinism** — the same seed and inputs produce the same result on one machine; simulation randomness is tick-seeded and stateless, and simulation paths avoid iteration-order-dependent collections.
- **Locality (speed of light)** — no update reaches farther than 64 cells in one tick; longer-range behavior propagates locally over ticks. A queued between-tick world-event list is the sanctioned escape hatch — none exists today.
- **Idle cost** — unloaded chunks cost nothing; a settled ticketed chunk does no movement work, paying only a bounded random-tick sample. No unbounded or growing per-tick cost.
- **Sleeping is a pure optimization** — evaluating a chunk's full area and evaluating only its sim rect must produce identical results. A rule whose outcome depends on anything outside its marked neighbourhood, or pending stochastic work that goes quiet without a keep-alive, is a bug.
- **Suspend/resume** — sleep, unload, and reload preserve pending activity; in-flight processes never freeze in time.

## The grid

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

A cell is eight heap-free bytes: material, velocity, shade, a body flag, and an updated-this-tick byte. Every cell is a particle — velocity drives all grid movement. Burning is a material, not a flag: a lit fuel transmutes into its synthesized burning twin and probabilistic burnout *is* the burn duration; there is no per-cell HP. Heavier per-cell state (temperature, say) would be a separate per-chunk plane most chunks skip.

## Scheduling

- Chunks group into 2×2 blocks run in four phases by block parity; a worker owns its block plus a one-chunk halo, and same-phase windows share no chunks — race-free without locks. A chunk simulates only when its whole 3×3 neighbourhood is loaded; frontier chunks defer, keeping their rects.
- Rows scan bottom-up so a faller vacates space the cell above enters the same tick; the horizontal direction alternates per row and tick to cancel scan bias.
- Random ticks are a second, sleep-independent pass scoped to a bounded chunk range around each player: each chunk samples a few tick-seeded cells for ambient processes. Reserved infrastructure for plant growth and decay; nothing uses it today.
- The kernel monomorphizes per material, so a cell's own properties are constants and dead branches vanish; one movement kernel per phase, taking exactly its phase's coefficients. Integer-only — grid determinism is independent of float semantics. Tuning is authored in real units and compiled to integers; see [Content.md](Content.md).

## Movement

Every moving cell integrates its velocity locally each tick; a settled cell writes nothing. In order:

- **Accelerate** — gravity (gases and fire rise) minus buoyancy from the displaced fluid, then drag by the medium above; a lighter liquid under a denser one swaps up directly; a submerged liquid dives diagonally into adjacent air pockets, so bubbles wander as they rise; rising gases sway by turbulence.
- **Contact friction** — resting on a blocked face bleeds horizontal velocity by ground friction.
- **Cohesion** — liquid and gas velocity pulls toward the mean of like-phase neighbours, forming coherent jets.
- **Traverse** — step cell-by-cell along the velocity, fractional speed by tick-seeded chance, reach capped inside the 64-cell window. Steps are cardinal: a diagonal needs an open orthogonal cell, so corners seal for free. A swap stamps both cells: stamped matter cannot be displaced again that tick, stamped air still admits velocity-backed traversal, and a mover refused by a stamp holds its velocity and retries instead of reflecting.
- **Collide & redirect** — a blocked face reflects by restitution (near-inelastic); a blocked fall that can descend diagonally converts to sideways velocity by deflect — ledge jets for liquids, repose slides for powders. Powder topple is static/kinetic friction: a stationary grain holds any slope until a moving powder or liquid neighbour agitates it (start rate); a moving grain slides readily (keep rate) and agitates its own neighbours, so avalanches propagate through motion and die with it — never through sleep accidents. One exception: a grain loaded from above with an open slide path is pending work and keeps rolling its start rate, so a dug-out face collapses instead of standing on friction. A liquid that can't descend spreads one cell across a level surface with no velocity gain. Redirects yield right of way to a denser faller directly above the target, so gaps fill from above, not from the sides.
- **Settle** — velocity into a blocked face dies and sub-threshold velocity snaps to zero, so a supported cell nets no change and its chunk sleeps.

Leveling and pressure propagate as local waves over ticks. Steam condenses back to water so gas pockets resolve.

## Sleeping

Each chunk tracks two rects. The **sim rect** is honest: exactly the cells re-simulated next tick — an empty rect skips the chunk. The **change rect** (⊆ sim) holds actual value changes and feeds replication and persistence. A write marks change tight and sim as the 3×3 neighbourhood, dilating across chunk borders; a **keep-alive** mark (burning fuel, a pending stochastic interaction) extends sim by a single cell and costs zero bandwidth.

## Combustion

Each flammable fuel authors one flammable block and receives a synthesized burning twin — same phase and dynamics, its own palette, hot with baked emission. Three local stages:

- **Ignite** — every hot cell (flame, burning fuel, lava) transmutes adjacent flammables into their burning twins at the *fuel's* ignite rate, keeping velocity and shade — igniting oil keeps flowing. A hot↔flammable adjacency is pending work and keeps itself marked, so a settled lava shore still catches a log that lands on it while a fuel-less lava lake sleeps for free. Burning fuel igniting its own material is the propagation front; free flames add plume and secondary ignition above.
- **Burn** — a burning cell damages entities, emits fire into adjacent air, and burns out at its rate — that rate *is* the burn duration; residue leaves ash, otherwise burnout resolves to smoke so the front self-exposes to oxygen.
- **Sealed** — without adjacent oxygen, ignition and burning scale by the fuel's sealed fraction, monotonically: a positive fraction smoulders; zero means the fuel needs air — it never catches sealed, and a sealed burning cell snuffs back to its unburned material. A sealed free flame burns out to smoke.

A water neighbour quenches: a flame dies to steam keeping the water; a burning fuel resolves to its residue (charring is never restored) and the water flashes to steam — dousing *spends* the water. Fuels sleep until lit, so an unlit forest costs nothing.

## Glossary

| Term | Meaning |
|------|---------|
| Cell / Chunk / Region | 1×1 material instance / 64×64 cells / 8×8 chunks |
| CellPos / ChunkPos / RegionPos | integer x,y coordinates at each granularity |
| sim / change | per-chunk rects: cells to re-simulate next tick ⊇ cells changed this tick; double-buffered |
| SimWindow | a worker's 4×4-chunk view: simulates the inner 2×2 block, reads one chunk beyond |
| Speed of light | max reach of one update = one chunk = 64 cells |
| Cell velocity | per-cell fixed-point cells/tick; sim-only, persisted, never on the wire |
| Keep-alive | a sim-rect mark without a change: pending work that must be re-evaluated |
| Burning twin | the synthesized burning material of a flammable fuel |
| Random tick | bounded tick-seeded per-chunk ambient sample, independent of sleep |
| Displacement budget | a swapped cell is stamped and can't be displaced again that tick; stamped air still admits velocity-backed traversal |
| Right of way | sideways redirects refuse a gap with a denser faller directly above it; the faller fills it |
