# Physics

Players and detached terrain are real cells in the world, moved by a small custom module. Everything collides directly against the grid, so terrain changes never rebuild collision geometry. Y is up everywhere.

## Invariants

- **One cell, one owner** — terrain, one body, or one player; stamps are exclusive by construction.
- **Body raster integrity** — a body flag corresponds to exactly one live body or player raster. Transmutation keeps ownership while the product still obstructs, so a body carries the ash its own fire makes; removals and placements clear it. Every body reconciles its slots against the grid before motion.
- **Mass through motion** — exact lattice rotation maps every slot to one cell; every relocation pairs entered fluid with a vacated cell; settling only clears ownership flags.
- **Bodies are terrain** — owned cells run the same reactions, decay, combustion, digging, and material writes as terrain. Ownership suspends only independent cell movement.
- **No active welding** — an active body retains only its existing transformed slots. Reactions may split or release them, but contact with terrain or another body never adds slots or merges bodies.
- **Suspend/resume** — unload settles crossing bodies; a save naturally records any live raster as terrain because runtime flags are never persisted. There is no body persistence format.

## Player

An alive avatar is a 3×N raster of inert, body-flagged flesh cells stamped transactionally each tick — the shade pattern is the character art. Height changes at 50 rows/s, never more than one row per tick, between ducked and standing; feet stay invariant and a row grows only into free headroom. The observable pose is integer cells: collision, raster, wire, hazards, and rendering all derive from one floor-anchored footprint; sub-cell motion is an internal accumulator, snapped flush on a blocked axis.

The controller is Celeste ported to cells/s, tuned server-side (coyote time, jump buffer, variable height, corner correction, step assists) plus swimming, swept per-axis cell-by-cell against solid and powder cells — powders are walls, digging is the way through. Surfaces contribute authored grip and bounce. Submersion is estimated from the ring around the raster, drags toward the local liquid velocity, and throttles run speed.

The stamp commits the sweep's pose: liquids in newly claimed cells pair into vacated cells or surface up their column within the 64-cell window, refusing the move when no free surface is in reach. Conflicts cascade full → x-only → y-only → stay; a clobbered raster self-heals by full re-stamp; an unchanged raster writes nothing, so an idle player keeps chunks asleep. Two players are mutually solid and exchange momentum. Contact with a live body transfers the blocked controller impulse directly into that body. Death and departure unstamp to air.

## Pixel bodies

A pixel body is a list of canonical local slots, one flagged world raster, and fixed-point pose, velocity and spin. A slot is an offset plus the material last read from the grid; mass, bond group, restitution and grip all derive from that material, so the body copies no material state and owns no solver state, contact graph, owner plane, damage queue, sleep state, or persistence record. Mass is density-weighted per cell, so an ore vein leads a fall.

- **Bonds decide structure, adjacency decides carriage** — rigid materials author bond groups; the symmetric bond matrix flood-fills an island and any obstructing cell below rejects it. The same matrix splits a body's surviving slots into components, so burning through a beam snaps it. Owned matter that cannot bond joins whichever component it touches and rides along; matter touching none returns to terrain. Newly adjacent cells are never considered, so active bodies cannot weld. Landed cells are terrain again and participate in the next flood fill normally.
- **Reconciliation preserves motion** — one linear scan compares every slot against the grid. Departed cells return to terrain carrying their rigid point velocity. Survivors keep the raster bit-for-bit: shifting the pose by exactly the travel of the centre of mass leaves the rasterisation pivot unchanged, so nothing snaps to the lattice. Angular velocity is untouched, because a departing cell carries away precisely its own share of the angular momentum, and the surviving centre of mass inherits the point velocity it already had. No dynamic state round-trips through cell fields.
- **Detachment discovery is local** — a write whose support class differs from what it replaced unseats something. Withdrawing support unseats the rigid neighbours of the change, because anything around it may now be standing on nothing; adding support unseats only the written cell, the sole thing that can be newly unsupported. Velocity-only writes, which are most writes, unseat nothing. Discovery waits when the whole island margin is not simulated, then atomically flags a detached island.
- **Motion is swept and exact** — gravity, contacts and player pushes advance one combined translation-and-rotation traversal over 64 authoritative lattice orientations; continuous turn is only the accumulator between them. A traversal step crosses at most one cell per axis, one cell of rim travel, and one authoritative orientation, so no intermediate raster goes untested. The reversible integer-lattice rotation map keeps every slot unique, and a blocked combined transition is never decomposed into a path that was not swept.
- **Contacts are impulses** — rejected entering cells become cardinal contacts. Sequential impulses with per-contact accumulation resolve every normal before any friction, which is what stops a symmetric landing from crabbing sideways. Effective mass at a contact is exact integer arithmetic, and two participants combine harmonically. Off-centre support turns gravity into spin, so an overhang topples on its own with no rule saying it should.
- **One impact per tick** — restitution is a target separation speed the tick's first impact may ask for; every later contact in the same tick is inelastic. A body wedged between two surfaces therefore cannot pump itself full of energy pass after pass, which is the only reason a collapsing pile stays a pile.
- **A blocked tick keeps its budget** — the fraction of the tick left after a contact is re-swept against the post-impulse velocity, so bodies slide along walls and roll along ground instead of stopping dead at a graze. The sweep gives up when a contact can no longer change the motion, and a simulation frontier freezes the body in place.
- **Everything solid is a contact peer** — terrain has infinite mass, another body takes the equal and opposite impulse directly, and a live player raster takes its reaction as a velocity change routed back to that avatar. A plank landing on someone's head shoves them; it does not treat them as bedrock. Bodies still do not solve stacks, buoyancy, or crush damage.
- **Relocation is transactional** — only the final raster is committed. Entered liquid and gas cells pair deterministically into vacated cells, conserving matter without a spill search.
- **Rest, then terrain** — a body is a motion event. Half a second without changing a single cell writes the raster back as terrain carrying each cell's point velocity, whether the body is resting flat, wedged, or stalled mid-topple. A push or any slot change restarts the window, which is what makes a landed boulder kickable.
- **Interaction stays small** — a player push changes body velocity and spin at the contacted cell, transferring at most the body's mass. Grid writes handle digging and chemistry without body-specific paths.

There is no gameplay body protocol or renderer: flagged cells ride ordinary chunk deltas and render as terrain. The opt-in diagnostic stream sends complete live rasters for ownership outlines.

## Glossary

| Term | Meaning |
|------|---------|
| Avatar | The physical realization of an alive player: actor, raster, health, interaction, deferred physical state |
| Actor | Kinematic controller whose observable pose is its integer footprint |
| Footprint | Floor-anchored integer cell rect; collision, raster, wire, and hazards all read it |
| Subcell | Fixed-point continuous pose and per-tick motion; exact in saves, never on the wire |
| Flesh | The player's inert body material — body-flagged, undiggable, and omitted from region snapshots |
| PixelBody | Transient tumbling canonical cell shape over a flagged world raster |
| Slot | One canonical body cell: a local offset plus the material last read from the grid |
| Bond group | Authored connectivity class deciding which rigid materials hold together |
| Support class | A cell's contribution to holding rigid matter up: obstruction plus bond group |
| Unseated | A cell a write may have left standing on nothing, seeding an island check |
| Stander | A live player raster acting as a finite-mass contact peer |
| Rest window | Half a second of stillness after which a body writes itself back as terrain |
