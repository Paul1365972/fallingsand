# Physics

A small custom module in `fallingsand_sim` — everything collides against the cell grid, so terrain changes never rebuild collision geometry. That coupling is why there's no general-purpose engine.

**Axes:** y is up everywhere — falling is negative `vy`. Three separate gravities apply it: `GRID_GRAVITY` (cells) and `PlayerParams.gravity` (controller) are positive magnitudes applied downward, while `BODY_GRAVITY` (pixel bodies) is stored already-signed (negative). Buoyancy bearing samples the cells below and beside a body, never above.

## Entities & controller

Entities (players, items, creatures) are kinematic AABB/capsule bodies, swept against solid cells, with material-aware drag and sinking. The controller is Celeste ported to cells/s and tuned server-side (coyote time, jump buffer, variable height, corner correction, step assists) plus Minecraft-flavored swimming (drag-limited swim, idle sink, treading bob, bank vault, wade drag). Dropped items reuse the same `move_body` sweep with gravity + ground friction and settle when at rest (idle-free); pickup/merge are strictly local (see [Inventory.md](Inventory.md)).

Contact conserves momentum like the rest of the sim. A blocked axis reflects by the surface material's `surface_bounce` — an opt-in entity-vs-terrain restitution (default 0), distinct from the cell `restitution` that drives the CA and pixel bodies, so ordinary terrain dead-stops you (Celeste-crisp) and only springy materials (mushrooms) bounce; tangential velocity survives (a wall keeps your fall, a ceiling keeps your run). Ground handling likewise reads a per-material `surface_grip` — distinct from cell-vs-cell `friction` — so ice glides and turns sluggishly while normal terrain is unchanged. A fast step-up converts horizontal speed into an upward launch, so momentum flings you off a lip. Submerged entities are dragged toward the local liquid velocity, not toward zero — currents carry you. External impulses (pixel bodies, entity-vs-entity shove, radial knockback) feed one per-entity impulse queue.

## Solidity & overlaps

Entity AABBs rasterize into an entity-only obstacle mask each tick: powders treat masked cells as ground (sand piles on your head), liquids/gases pass through (swimming works at cell scale). Pixel-body cells need no mask — they're real solid cells.

Overlaps exchange momentum instead of blocking: cells already inside your hitbox never obstruct you (you can move *out* of an overlap, never deeper into fresh cells), so rasterized debris can't lock you. Both sides carry mass; restitution is a material property (0 ≤ e < 1, inelastic below a small speed so things settle).

## Pixel bodies

Rigid bodies made of cells, rasterized in the grid at all times. A body is a motion record (local cell buffer, `Fixed` pose/velocity, spin, mass, inertia) over real world cells carrying the body flag — one cell, one owner.

- **Registration**: flood-fill finds disconnected solid islands (materials tagged `rigid_capable`) and flags them in place — cells never leave the grid. Anything that removes support (digs, reactions, powder draining out) feeds one structural-notification queue that seeds the check.
- **Dynamics** are impulse-based and substepped, then one transactional re-stamp per tick moves the footprint: plan-then-commit, conflicts fall back to translation-only → rotation-only → damped abort; displaced fluid pairs into vacated cells. Nothing is half-written; an unchanged raster writes nothing, so resting bodies keep chunks asleep. Hidden overlap mass stays in the buffer and reappears — matter is conserved.
- **Buoyancy** from liquid bearing on the footprint from below or beside (water on top bears nothing) plus drag — wood floats, stone sinks, no special cases.
- **Bodies are terrain**: the CA runs reactions and decay on them (a fallen tree burns). Any write unflagging a body cell feeds a damage queue; the bodies pass reconciles it — solid products re-adopted, the rest leaves, bodies split by connectivity or despawn when empty.
- **Resting is free**: a body at rest sleeps as a kickable body forever and never settles into terrain during play. Only region unload settles it in place (motion lost). Kicks, weight, damage, undermining, or fluid on top wake it.
- **No body protocol or renderer** — body cells ride ordinary chunk deltas and render as terrain, cell-snapped at tick rate.
