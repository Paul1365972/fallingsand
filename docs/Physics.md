# Physics

Everything physical is grid-resident: players and rigid bodies are real cells in the world, moved by a small custom module — no general-purpose engine, because everything collides against the cell grid and terrain changes never rebuild collision geometry. Y is up everywhere.

## Invariants

- **One cell, one owner** — terrain, one body, or one player; stamps are exclusive by construction.
- **Body raster integrity** — a body flag corresponds to exactly one live body or player raster; public cell writes create only unflagged cells; players are grid residents, not collision overlays. There is no entity obstacle mask — every live mover is real cells and needs none.
- **Universality** — bodies are terrain: reactions, decay, and combustion run on them (a fallen tree burns).
- **Mass through motion** — rotation maps every owned member onto the grid bijectively, so it can neither hide nor lose cells; settling writes exactly one terrain cell per member; displaced fluid pairs into vacated cells or surfaces up its column, never deleted.
- **Suspend/resume** — saves encode body cells as terrain with their velocities and persist only the continuous pose keyed by its pivot. Reload re-registers the containing bonded island and re-derives its dynamics.
- **Single-authority contact** — player↔body momentum transfers in exactly one place, the body contact solver; the accumulated result lands as one impulse per player per tick.

## Player

An alive avatar is a 3×N raster of inert, body-flagged flesh cells stamped transactionally each tick — the shade pattern is the character art. Height changes at 50 rows/s, never more than one row per tick, between ducked and standing; feet stay invariant and a row grows only into free headroom. The observable pose is integer cells: collision, raster, wire, hazards, and rendering all derive from one floor-anchored footprint; sub-cell motion is an internal accumulator, snapped flush on a blocked axis.

The controller is Celeste ported to cells/s, tuned server-side (coyote time, jump buffer, variable height, corner correction, step assists) plus swimming, swept per-axis cell-by-cell against solid *and powder* cells — powders are walls, digging is the way through. Surfaces contribute authored grip and bounce, distinct from the CA's cell-vs-cell restitution and friction. Submersion is estimated from the ring around the raster, drags toward the local liquid velocity, and throttles run speed.

The stamp commits the sweep's pose: liquids in newly claimed cells pair into vacated cells or surface up their column within the 64-cell window, refusing the move when no free surface is in reach. Conflicts cascade full → x-only → y-only → stay; a clobbered raster self-heals by full re-stamp; an unchanged raster writes nothing, so an idle player keeps chunks asleep. Two players are mutually solid; bumping transfers momentum. Death and departure unstamp to air and wake affected bodies.

## Pixel bodies

One body set owns every live body and solver buffer. Each body directly owns its member positions; there is no parallel owner map or ownership plane. Registration, splitting, removal, and settling update body membership transactionally.

A rigid body persistently holds membership, continuous pose, and lifecycle state over real world cells carrying the body flag. Each awake tick flood-fills its bonded membership from its pivot, splits disconnected fragments, and re-derives material cells, mass, linear momentum, angular momentum, inertia, restitution, and perimeter from the grid. Resting bodies skip that work.

- **Bonds decide connectivity** — rigid materials author a bond group (mineral, wood, foliage, ice); a compiled symmetric matrix says which groups hold together. Island detection walks the terrain lattice and live-body splitting walks the inverse-rotated local lattice with the same cardinal boolean predicate, so rotation cannot sever a bond. Stone never fuses to a tree trunk by touching it, while wood carries its leaves. No per-edge bond state: moving bodies never merge on collision, and a settled body is terrain again and bonds like it.
- **Registration** — ordinary cell writes centrally note nearby rigid cells; flood fill rejects externally supported bonded islands and flags detached ones in place. Creative placement alone suppresses its initial note without changing the placed cells, so a later impact wakes the whole bonded construction through the normal path. Detection waiting on unsimulated territory is runtime-only.
- **Rotation is bijective** — the continuous angle integrates smoothly, but each tick's raster snaps it to a discrete step and maps the freshly derived local members through an exact reversible permutation of the cell lattice.
- **Dynamics** — impulse-based, substepped; one transactional re-stamp per tick, plan-then-commit; conflicts fall back translation-only → rotation-only → damped abort.
- **Contacts** — found against the exact proposed member cells, with cardinal normals off the true obstruction — never a sampled vote or constant fallback. A sequential-impulse solver accumulates clamped equal-and-opposite impulses reading each partner's live velocity, and a final writeback preserves impulses regardless of processing order. Valid adjacent support keeps the proposed flush pose instead of rolling back one cell. Dissipation comes only from authored restitution and friction plus the resting snap.
- **Players couple through the solver** — the player's own sweep only blocks its position against body cells; entity velocity state is shared across every body in the tick, so several touching bodies can never each re-apply the same reaction.
- **Cell momentum** — grid interactions transfer momentum into individual member cells. Derivation fits the rigid field around the mass center, then writeback stores one exactly representable Q5.10 rigid field, preventing quantization drift across unchanged ticks.
- **Buoyancy** — vertical force and torque come from the displaced-liquid work of the same transactional relocation rules used by stamping, plus drag: wood floats, stone sinks, no material special cases.
- **Damage** — any write unflagging a body cell feeds a damage queue reconciled before stepping: solid products re-adopted (a moving log keeps its fire), bodies split by bond connectivity or despawned when empty; every fragment derives its motion from its own cells.
- **Lifecycle: active → resting → settled** — a slow body in a terrain-anchored cardinal contact network rests; a liquid-anchored contact network also rests after remaining below the same motion thresholds, while a sinking assembly stays active. A standing player neither anchors the network nor wakes it. Any nearby material write notes body cells, so collision, damage, support, player stamping, and fluid changes share the same wake path. After ~half a second at rest—or immediately when a crossed region unloads—the live raster becomes ordinary terrain in place. The largest live fragment inherits the continuous pose after a split, with the pivot fragment breaking size ties; new fragments start at the parent angle. Settled terrain reclaims a pose by containing its pivot, while a merge containing several pose records restarts with a fresh pose.

There is no gameplay body protocol or renderer: body cells ride ordinary chunk deltas and render as terrain. The opt-in debug stream exposes body ownership only for outlines.

## Glossary

| Term | Meaning |
|------|---------|
| Avatar | the physical realization of an alive player: actor, raster, health, interaction, deferred physical state |
| Actor | kinematic controller whose observable pose is its integer footprint |
| Footprint | floor-anchored integer cell rect; collision, raster, wire, and hazards all read it |
| Subcell | fixed-point continuous pose and per-tick motion; exact in saves, never on the wire |
| Flesh | the player's inert body material — body-flagged, undiggable, and omitted from region snapshots |
| PixelBody | rigid body made of cells: owned membership + continuous pose over flagged world cells; dynamics derive from those cells |
| Bond group | authored connectivity class deciding which rigid materials hold together |
| Structural note | tick-local notification derived centrally from a material write |
