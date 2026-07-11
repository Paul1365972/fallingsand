# Physics

A small custom module in `fallingsand_sim` — everything collides against the cell grid, so terrain changes never rebuild collision geometry; that coupling is why there's no general-purpose engine.

**Axes:** y is up everywhere — falling is negative `vy`. Three gravities: `GRID_GRAVITY` (cells) and `PlayerParams.gravity` (controller) are positive magnitudes applied downward; `BODY_GRAVITY` (pixel bodies) is stored already-signed (negative). Buoyancy samples cells below and beside a raster, never above.

## Player: a grid-resident controller

The player is real cells at all times: a 3×9 (ducked 3×5) raster of inert `flesh` cells — body-flagged, authored shade pattern (the pixel person), mirrored by facing — stamped transactionally each tick. The observable pose is integer cells: collision footprint, raster, wire state, hazards, and rendering all derive from one floor-anchored footprint, a pure function of `(floor(x), floor(y), ducking)`. The controller keeps full `Fixed` sub-cell math internally as a motion accumulator; a blocked axis snaps the accumulator flush against the wall.

The controller is Celeste ported to cells/s, tuned server-side (coyote time, jump buffer, variable height, corner correction, step assists) plus Minecraft-flavored swimming (drag-limited swim, idle sink, bank vault, wade drag), swept per-axis cell-by-cell against solid *and powder* cells — powders are walls, digging is the way through. A blocked axis reflects by the surface material's `surface_bounce` (default 0 — terrain dead-stops, only springy materials bounce); tangential velocity survives. Ground reads per-material `surface_grip` (ice glides); both are distinct from the cell-vs-cell `restitution`/`friction` driving the CA and pixel bodies. A fast step-up converts horizontal speed into an upward launch. Submersion is estimated from the 1-cell ring around the raster (liquid bears from below/beside, never above); submerged players are dragged toward the local liquid velocity, not zero, and submersion throttles horizontal run speed (wade) whether wading or swimming. External impulses (pixel bodies, blocked-into-player shoves) feed one per-entity impulse queue applied next tick.

The stamp commits the sweep's pose: liquids in newly claimed cells pair into vacated cells (a pure translation pairs exactly; unduck/spawn spill relocates nearby or surfaces the displaced liquid up its column — never deleted — refusing the move only when no free surface exists, so a sealed flooded pocket can still keep you ducked). Conflicts cascade full → x-only → y-only → stay, zeroing the aborted axis. An unchanged raster writes nothing, so an idle player keeps chunks asleep; a clobbered raster cell self-heals by a full re-stamp. Two players are mutually solid — stamps are exclusive by construction; bumping transfers momentum through the blocked contact. Spawn/respawn stamps in with an upward clearance search; despawn/death unstamps to air; region load voids stale flesh (crash artifacts) like leftover body flags.

There is no entity obstacle mask — every mover in the sim is real cells (players stamped, pixel bodies rasterized) and needs none.

## Pixel bodies

Rigid bodies made of cells, rasterized in the grid at all times: a motion record (local cell buffer, `Fixed` pose/velocity, spin, mass, inertia) over real world cells carrying the body flag — one cell, one owner.

- **Registration**: flood-fill finds disconnected solid islands (`rigid_capable` materials) and flags them in place — cells never leave the grid. Anything removing support feeds one structural-notification queue that seeds the check.
- **Dynamics**: impulse-based, substepped; one transactional re-stamp per tick moves the footprint — plan-then-commit, conflicts fall back translation-only → rotation-only → damped abort; displaced fluid pairs into vacated cells or surfaces up its column, never deleted. An unchanged raster writes nothing, so resting bodies keep chunks asleep. Hidden overlap mass stays in the buffer and reappears — matter is conserved.
- **Buoyancy** from liquid bearing on the footprint below or beside, plus drag — wood floats, stone sinks, no special cases.
- **Bodies are terrain**: the CA runs reactions and decay on them (a fallen tree burns). A write unflagging a body cell feeds a damage queue; the bodies pass reconciles it — solid products re-adopted, bodies split by connectivity or despawn when empty.
- **Resting is free**: a resting body sleeps as a kickable body forever; only region unload settles it into terrain (motion lost). Kicks, weight, damage, undermining, or fluid on top wake it.
- **No body protocol or renderer** — body cells ride ordinary chunk deltas and render as terrain, cell-snapped.
