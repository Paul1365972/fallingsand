# Simulation

The world is one cellular automaton: every pixel is matter. Physics is phase-based and semi-realistic — natural behavior over artificial caps and clamps. Players and rigid bodies live in the same grid and everything collides against it directly, so terrain changes never rebuild collision geometry.

## Invariants

- **Conservation of mass** — cells are created or destroyed only by a physical cause.
- **One cell, one owner** — a cell belongs to terrain, one rigid body, or one player raster; double occupancy is an architecture bug, not a tuning problem.
- **Determinism** — the same seed and inputs produce the same result on one machine; every random domain has a compile-time string-labeled salt and simulation paths avoid iteration-order-dependent collections. Duration processes use tick-seeded stateless rolls and stay awake while a roll is possible; fixed-point responses derive resistance from cell state, so evaluating an unchanged state repeats the same result.
- **Locality (speed of light)** — no update reaches farther than 64 cells in one tick; longer-range behavior propagates locally over ticks. A queued between-tick world-event list is the sanctioned escape hatch — none exists today.
- **Idle cost** — unloaded chunks cost nothing; a settled ticketed chunk does no movement work, paying only a bounded random-tick sample. No unbounded or growing per-tick cost.
- **Sleeping is sound** — a slept world is a fixed point: force-evaluating every cell of a quiescent world changes nothing. Marks may defer a cascade one tick versus evaluating everything — they never lose work. A rule whose outcome depends on anything outside its marked footprint, or pending stochastic work that goes quiet without a keep-alive, is a bug.
- **Suspend/resume** — loaded chunks wake fully once, conservatively resuming every grid process without persisting scheduling state. Runtime-only rigid motion snapshots as ordinary terrain at its current raster.

## The grid

| Level | Size | Unit of |
|-------|------|---------|
| Cell | 1×1 | one material instance |
| Chunk | 64×64 cells | dirty tracking, sleeping, replication, rendering |
| Region | 8×8 chunks | generation, storage, load/unload |

A cell is a compact heap-free value: material, velocity, shade, a runtime flags byte — the tick-local moved stamp and body ownership, never persisted — and a persistent per-material aux byte. Every cell is a particle — velocity drives all grid movement. Burning is a material, not a flag: a lit fuel transmutes into its synthesized burning twin and probabilistic burnout *is* the burn duration; there is no per-cell HP.

## Scheduling

- Chunks group into 2×2 blocks run in four phases by block parity; a worker owns its block plus a one-chunk halo, and same-phase windows share no chunks — race-free without locks. A chunk simulates only when its whole 3×3 neighbourhood is loaded; frontier chunks defer, keeping their rects.
- Each tick runs two full passes over awake cells: **effects** then **movement**. Effects apply everything except position changes — forces write velocity, combustion and reactions transmute in place, liquids relax pressure heads. Movement is the only pass that moves matter: a cell with velocity integrates it kinematically; a settled liquid or capped gas takes a discrete flow step.
- Every moved stamp is clear when the first pass begins: each chunk starts the tick by clearing them inside its sim rect, then ready chunks roll their rects. Movement swaps stamp both cells and a collision impulse stamps its receiver; every stamp is a write, so stamped cells always lie inside that rect and no stale tick-local state survives awake, frontier, or freshly loaded.
- Rows scan bottom-up so a faller vacates space the cell above enters the same tick; the horizontal direction is tick-hashed per world row and the four phases run in a tick-hashed order to cancel scan bias.
- Random ticks are a third, sleep-independent pass scoped to a bounded chunk range around each player: each chunk samples a few tick-seeded cells for ambient processes. Reserved infrastructure for plant growth and decay; nothing uses it today.
- The effect kernel monomorphizes per material, so a cell's own properties are constants and dead branches vanish. Integer-only — grid determinism is independent of float semantics. Tuning is authored in real units and compiled to integers; see [Content.md](Content.md).

## Effects

Effects shape velocity and matter, never position. A settled cell nets zero force and writes nothing.

- **Weight** — an unsupported cell takes gravity minus buoyancy from the medium it displaces; a cell under a denser liquid takes the buoyant rise instead, so stratification, sinking sediment, and rising gases are all one rule. Support means the cell below does not admit the mover — a normal force, not a clamp.
- **Drag & friction** — drag by the medium above, ground friction when supported; sub-settle velocity snaps to zero at rest. Gas turbulence derives from cell state, so unchanged enclosed gas is a true fixed point rather than a sleeping lottery.
- **Topple** — a grounded grain on an open slope converts fall energy into sideways velocity. Static load tests a deterministic per-grain resistance; a moving grain tests kinetic resistance while its velocity keeps it awake. Impacts propagate as transferred momentum, never as an observed agitation condition. Redirection is a force, never a swap.
- **Pressure** — a liquid relaxes its aux byte: *depth*, hydrostatic depth below its own surface (+1 per cell downward, decaying through covered passages), and two one-way **excess flows**, one carried only rightward and one only leftward, both climbing by the hydrostatic step. Each tick a row refreshes the flow matching its scan direction from just-relaxed upstream cells — the matching flow re-derives across a whole row in one tick — while the counter flow only checks for pending work and keeps the cell awake until its own direction comes up. A flow can never circle back toward its source, so no cycle can sustain stale pressure. *Excess* is the larger flow, saturating at 3: it reports whether the connected surface stands at least two cells above this column; open pools level fully, while a shaft rises only a few cells above a long covered passage. Actuation fires only into calm, converged water, at a cadence below the field's own pace: a surface cell with excess ≥ 2 wells gently upward — a shaft rises toward balance; a submerged cell beside open air at head ≥ 3 jets sideways — a breached wall gushes by depth. Movement resets a traveling cell's pressure, so jets never feed on their own splash; a balanced body is silent.
- **Splash** — a hard plunge into liquid converts into lateral splash velocity; trapped voids resolve by plain gravity, water falling in as the air works up and out.

## Movement

A cell with velocity integrates it kinematically: cardinal steps along the velocity vector, fractional speed by tick-seeded chance, reach capped inside the 64-cell window. A swap marks both cells moved: moved matter cannot be displaced again that tick, moved air still admits traversal, and a mover refused by a moved cell holds its velocity and retries. A target admits a mover only if it is less dense, or denser when moving up (buoyant exchange), or air sideways — two solids can never swap, and equal densities never interpenetrate. Contact with another dynamic cell transfers a density-weighted normal impulse and marks the receiver moved, so momentum advances at most one interaction per tick; static matter reflects by restitution. Settling snaps sub-threshold velocity to zero against support.

A settled liquid takes at most one flow step per tick, gated by its flow chance (viscosity): fall into a fresh opening below, else slide diagonally into an open drop, else spread one cell sideways — onto foreign support when part of a puddle, or onto its own liquid's surface where that column's excess is at least two and exceeds its own. The pressure-gated hop is what levels: transport runs strictly down the excess gradient, throttled below the field's pace so actuation never outruns information, and a connected pool at rest is level to within a cell. A settled capped gas mirrors the fall upward, rising into fresh openings and seeping along ceilings into open pockets; steam condenses back to water so gas pockets resolve. Velocity handles the drama — falls, splashes, jets — flow steps handle the leveling; the two never fight because flow runs only at zero velocity and pressure only actuates into still water.

## Sleeping

Each chunk tracks two rects. The **sim rect** is honest: exactly the cells re-simulated next tick — an empty rect skips the chunk. The **change rect** (⊆ sim) holds actual value changes and feeds replication and persistence. A write marks change tight and sim as the 3×3 neighbourhood, dilating across chunk borders; a **keep-alive** mark (burning fuel, a pending stochastic interaction) extends sim by a single cell and costs zero bandwidth. Every CA rule reads only its 3×3 neighbourhood, so marks alone carry every CA wake: a distant imbalance arrives as a pressure wave that wakes the cells it passes. Structural registration is interaction-driven instead: every ordinary write centrally notes nearby rigid and body cells. Creative placement suppresses only that initial note; it stores an ordinary cell, and the next nearby interaction follows the universal path.

## Combustion

Each flammable fuel authors one flammable block and receives a synthesized burning twin — same phase and dynamics, its own palette, hot with baked emission. Three local stages:

- **Ignite** — a flammable beside a hot cell (flame, burning fuel, lava) transmutes *itself* into its burning twin at its ignite rate, keeping velocity and shade — igniting oil keeps flowing. The fuel reads only its own neighbourhood: oxygen is judged at the fuel's surface, and the fuel owns the pending-work keep-alive whenever a roll is possible, so a settled lava shore still catches a log that lands on it while a fuel-less lava lake — and a fully sealed fuel — sleeps for free. Burning fuel igniting its own material is the propagation front; free flames add plume and secondary ignition above.
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
| Flags byte | runtime per-cell flags — the tick-local moved stamp and body ownership; never persisted |
| Aux byte | persistent per-material per-cell state; liquids store depth and the two excess flows, others zero |
| Head | depth = hydrostatic depth below the surface, head = depth + excess; excess = the larger one-way flow |
| Excess flows | the rightward and leftward one-way excess relaxations; each refreshes end-to-end on ticks whose row scan matches its direction |
| Flow step | the single discrete swap a settled liquid (fall, slide, spread, pressure hop) or capped gas (rise, seep) takes per tick |
| Tick start | the per-chunk begin-tick sweep clearing moved stamps in the sim rect, then rolling rects for ready chunks |
| Keep-alive | a sim-rect mark without a change: pending work that must be re-evaluated |
| Burning twin | the synthesized burning material of a flammable fuel |
| Random tick | bounded tick-seeded per-chunk ambient sample, independent of sleep |
| Displacement budget | a swapped cell or collision receiver is marked moved and can't be displaced again that tick; moved air still admits traversal |
