# Physics

A small custom module in `fallingsand_sim` — everything collides against the cell grid, so terrain changes never rebuild collision geometry; that coupling is why there's no general-purpose engine.

**Axes:** y is up everywhere — falling is negative `vy`. Three gravities: `GRID_GRAVITY` (cells) and `PlayerParams.gravity` (controller) are positive magnitudes applied downward; `BODY_GRAVITY` (pixel bodies) is stored already-signed (negative). Buoyancy samples cells below and beside a body, never above.

## Entities & controller

Entities (players, creatures later) are kinematic AABB bodies swept against solid cells, with material-aware drag and sinking. The controller is Celeste ported to cells/s, tuned server-side (coyote time, jump buffer, variable height, corner correction, step assists) plus Minecraft-flavored swimming (drag-limited swim, idle sink, treading bob, bank vault, wade drag).

A blocked axis reflects by the surface material's `surface_bounce` — entity-vs-terrain restitution, default 0, so terrain dead-stops and only springy materials bounce; tangential velocity survives (a wall keeps your fall). Ground handling reads per-material `surface_grip` (ice glides). Both are distinct from the cell-vs-cell `restitution`/`friction` that drive the CA and pixel bodies. A fast step-up converts horizontal speed into an upward launch. Submerged entities are dragged toward the local liquid velocity, not zero — currents carry. External impulses (pixel bodies, entity shove, radial knockback) feed one per-entity impulse queue.

## Solidity & overlaps

Entity AABBs rasterize into an entity-only obstacle mask each tick: powders treat masked cells as ground, liquids/gases pass through. Pixel-body cells need no mask — they're real solid cells.

Overlaps exchange momentum instead of blocking: cells already inside a hitbox never obstruct (you can move *out* of an overlap, never deeper into fresh cells), so rasterized debris can't lock you. Both sides carry mass; restitution is a material property (0 ≤ e < 1, inelastic below a small speed).

## Pixel bodies

Rigid bodies made of cells, rasterized in the grid at all times: a motion record (local cell buffer, `Fixed` pose/velocity, spin, mass, inertia) over real world cells carrying the body flag — one cell, one owner.

- **Registration**: flood-fill finds disconnected solid islands (`rigid_capable` materials) and flags them in place — cells never leave the grid. Anything removing support feeds one structural-notification queue that seeds the check.
- **Dynamics**: impulse-based, substepped; one transactional re-stamp per tick moves the footprint — plan-then-commit, conflicts fall back translation-only → rotation-only → damped abort; displaced fluid pairs into vacated cells. An unchanged raster writes nothing, so resting bodies keep chunks asleep. Hidden overlap mass stays in the buffer and reappears — matter is conserved.
- **Buoyancy** from liquid bearing on the footprint below or beside, plus drag — wood floats, stone sinks, no special cases.
- **Bodies are terrain**: the CA runs reactions and decay on them (a fallen tree burns). A write unflagging a body cell feeds a damage queue; the bodies pass reconciles it — solid products re-adopted, bodies split by connectivity or despawn when empty.
- **Resting is free**: a resting body sleeps as a kickable body forever; only region unload settles it into terrain (motion lost). Kicks, weight, damage, undermining, or fluid on top wake it.
- **No body protocol or renderer** — body cells ride ordinary chunk deltas and render as terrain, cell-snapped.
