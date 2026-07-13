# Physics

A small custom module in `fallingsand_sim` — everything collides against the cell grid, so terrain changes never rebuild collision geometry; that coupling is why there's no general-purpose engine. Y is up everywhere — falling is negative `vy`; buoyancy samples below and beside a raster, never above.

## Player: a grid-resident controller

An alive avatar is real cells: a 3×N raster of inert `flesh` cells — body-flagged, the authored pixel-person pattern — stamped transactionally each tick. Dead, entering, reviving, and disconnected players have no raster. N steps at most one row per tick between 9 (standing) and 5 (ducked), feet-row invariant; a row only grows when the cells above the head are free, so rising under a low ceiling stalls until headroom clears. The observable pose is integer cells: collision, raster, wire, hazards, and rendering all derive from one floor-anchored footprint, a pure function of `(floor(x), floor(y), rows)`; the controller keeps `Fixed` sub-cell math internally as a motion accumulator, snapped flush on a blocked axis.

The controller is Celeste ported to cells/s, tuned server-side (coyote time, jump buffer, variable height, corner correction, step assists) plus Minecraft-flavored swimming, swept per-axis cell-by-cell against solid *and powder* cells — powders are walls, digging is the way through. A blocked axis reflects by the surface's `surface_bounce` (default 0); ground reads `surface_grip` (ice glides) — both distinct from the CA's cell-vs-cell `restitution`/`friction`. Submersion is estimated from the 1-cell ring around the raster; submerged players drag toward the local liquid velocity, and submersion throttles run speed. External impulses and pending crush response are owned by the avatar and applied on the next relevant phase.

The stamp commits the sweep's pose: liquids in newly claimed cells pair into vacated cells or surface up their column (probed ≤ 64 cells, the speed of light) — never deleted — refusing the move when no free surface is in reach. Conflicts cascade full → x-only → y-only → stay. An unchanged raster writes nothing (an idle player keeps chunks asleep); a clobbered raster self-heals by full re-stamp. Two players are mutually solid — stamps are exclusive by construction; bumping transfers momentum. Enter and revive search the nearest legal footprint in deterministic Manhattan order over ticks and stamp only after its 64-cell window is loaded. Death and departure unstamp to air and wake affected bodies; region load voids stale flesh. There is no entity obstacle mask — every live mover is real cells and needs none.

## Pixel bodies

Rigid bodies made of cells, rasterized at all times: a motion record (cell buffer, `Fixed` pose/velocity, spin, mass, inertia) over real world cells carrying the body flag — one cell, one owner.

- **Registration**: flood-fill finds disconnected solid islands (`rigid_capable`) and flags them in place; anything removing support feeds one structural-notification queue.
- **Dynamics**: impulse-based, substepped; one transactional re-stamp per tick — plan-then-commit, conflicts fall back translation-only → rotation-only → damped abort; displaced fluid pairs into vacated cells or surfaces up its column. Hidden overlap mass stays in the buffer and reappears — matter is conserved.
- **Buoyancy** from liquid bearing on the footprint, plus drag — wood floats, stone sinks, no special cases.
- **Bodies are terrain**: the CA runs reactions, decay, and combustion on them (a fallen tree burns). Any write unflagging a body cell feeds a damage queue reconciled before stepping — solid products re-adopted (a moving log keeps its fire), bodies split by connectivity or despawn when empty.
- **Resting is free**: a resting body sleeps as a kickable body forever; only region unload settles it into terrain. Kicks, weight, damage, undermining, or fluid on top wake it.
- **No body protocol or renderer** — body cells ride ordinary chunk deltas and render as terrain.
