# Physics

Everything physical is grid-resident: players and rigid bodies are real cells in the world, moved by a small custom module — no general-purpose engine, because everything collides against the cell grid and terrain changes never rebuild collision geometry. Y is up everywhere.

## Invariants

- **One cell, one owner** — terrain, one body, or one player; stamps are exclusive by construction.
- **Body raster integrity** — a body flag corresponds to exactly one live body or player raster; public cell writes create only unflagged cells; players are grid residents, not collision overlays. There is no entity obstacle mask — every live mover is real cells and needs none.
- **Universality** — bodies are terrain: reactions, decay, and combustion run on them (a fallen tree burns).
- **Mass through motion** — rotation maps the body buffer onto the grid bijectively, so it can neither hide nor lose cells; settling writes exactly one terrain cell per buffer cell; displaced fluid pairs into vacated cells or surfaces up its column, never deleted.
- **Runtime bodies** — saves encode a body's current raster as ordinary terrain and region unload settles it in place. Pose, velocity, spin, and pending registration are deliberately not persistent.
- **Single-authority contact** — player↔body momentum transfers in exactly one place, the body contact solver; the accumulated result lands as one impulse per player per tick.

## Player

An alive avatar is a 3×N raster of inert, body-flagged flesh cells stamped transactionally each tick — the shade pattern is the character art. Height changes at 50 rows/s, never more than one row per tick, between ducked and standing; feet stay invariant and a row grows only into free headroom. The observable pose is integer cells: collision, raster, wire, hazards, and rendering all derive from one floor-anchored footprint; sub-cell motion is an internal accumulator, snapped flush on a blocked axis.

The controller is Celeste ported to cells/s, tuned server-side (coyote time, jump buffer, variable height, corner correction, step assists) plus swimming, swept per-axis cell-by-cell against solid *and powder* cells — powders are walls, digging is the way through. Surfaces contribute authored grip and bounce, distinct from the CA's cell-vs-cell restitution and friction. Submersion is estimated from the ring around the raster, drags toward the local liquid velocity, and throttles run speed.

The stamp commits the sweep's pose: liquids in newly claimed cells pair into vacated cells or surface up their column within the 64-cell window, refusing the move when no free surface is in reach. Conflicts cascade full → x-only → y-only → stay; a clobbered raster self-heals by full re-stamp; an unchanged raster writes nothing, so an idle player keeps chunks asleep. Two players are mutually solid; bumping transfers momentum. Death and departure unstamp to air and wake affected bodies.

## Pixel bodies

One body set owns every live body, body id, raster owner entry, and solver buffer. Registration, splitting, removal, and settling update that aggregate transactionally; callers cannot mutate the body list separately from its owner index.

A rigid body is a motion record — cell buffer, pose, velocity, spin, mass, inertia — over real world cells carrying the body flag; resolving which body owns a flagged cell is constant-time.

- **Bonds decide connectivity** — rigid materials author a bond group (mineral, wood, foliage, ice); a compiled symmetric matrix says which groups hold together. Island detection and damage splitting walk the same cardinal boolean predicate — stone never fuses to a tree trunk by touching it, wood carries its leaves. No per-edge bond state: moving bodies never merge on collision, and a settled body is terrain again and bonds like it.
- **Registration** — ordinary cell writes centrally note nearby rigid cells; flood fill rejects externally supported bonded islands and flags detached ones in place. Creative placement alone suppresses its initial note without changing the placed cells, so a later impact wakes the whole bonded construction through the normal path. Detection waiting on unsimulated territory is runtime-only.
- **Rotation is bijective** — the continuous angle integrates smoothly, but each tick's raster snaps it to a discrete step and maps the canonical buffer through an exact permutation of the cell lattice, recomputed fresh from canonical.
- **Dynamics** — impulse-based, substepped; one transactional re-stamp per tick, plan-then-commit; conflicts fall back translation-only → rotation-only → damped abort.
- **Contacts** — found against the exact cells the body occupies, with cardinal normals off the true obstruction — never a sampled vote or constant fallback. A sequential-impulse solver accumulates clamped equal-and-opposite impulses reading each partner's live velocity. Dissipation comes only from authored restitution and friction plus the resting snap — no blanket contact damping.
- **Players couple through the solver** — the player's own sweep only blocks its position against body cells; entity velocity state is shared across every body in the tick, so several touching bodies can never each re-apply the same reaction.
- **Buoyancy** — from liquid bearing on the raster, plus drag: wood floats, stone sinks, no special cases.
- **Damage** — any write unflagging a body cell feeds a damage queue reconciled before stepping: solid products re-adopted (a moving log keeps its fire), bodies split by bond connectivity or despawned when empty; every fragment inherits the parent's point velocity at its new center.
- **Lifecycle: active → resting → settled** — a slow body supported by static ground (or a resting body) rests; a standing player neither blocks rest nor wakes it. Any nearby material write notes body cells, so collision, damage, support, player stamping, and fluid changes share the same wake path. After ~half a second at rest—or immediately when a crossed region unloads—the live raster becomes ordinary terrain in place; snapshots independently encode it as terrain. Later interaction can register it again.

There is no gameplay body protocol or renderer: body cells ride ordinary chunk deltas and render as terrain. The opt-in debug stream exposes body ownership only for outlines.

## Glossary

| Term | Meaning |
|------|---------|
| Avatar | the physical realization of an alive player: actor, raster, health, interaction, deferred physical state |
| Actor | kinematic controller whose observable pose is its integer footprint |
| Footprint | floor-anchored integer cell rect; collision, raster, wire, and hazards all read it |
| Subcell | fixed-point continuous pose and per-tick motion; exact in saves, never on the wire |
| Flesh | the player's inert body material — body-flagged, undiggable, and omitted from region snapshots |
| PixelBody | rigid body made of cells: buffer + pose + spin + mass over flagged world cells |
| Bond group | authored connectivity class deciding which rigid materials hold together |
| Structural note | tick-local notification derived centrally from a material write |
