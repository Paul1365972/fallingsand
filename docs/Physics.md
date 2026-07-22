# Physics

Players and detached terrain are real cells in the world, moved by a small custom module. Everything collides directly against the grid, so terrain changes never rebuild collision geometry. Y is up everywhere.

## Invariants

- **One cell, one owner** — terrain, one body, or one player; stamps are exclusive by construction.
- **Body raster integrity** — a body flag corresponds to exactly one live body or player raster; public cell writes create only unflagged cells.
- **Mass through motion** — exact lattice rotation maps every member to one cell; every relocation pairs entered fluid with a vacated cell; settling only clears ownership flags.
- **Transient bodies** — a detached island is a short-lived motion event. Its cells are inert and undiggable while moving, then regain every terrain rule the instant they land.
- **Suspend/resume** — unload settles crossing bodies; a save naturally records any live raster as terrain because runtime flags are never persisted. There is no body persistence format.

## Player

An alive avatar is a 3×N raster of inert, body-flagged flesh cells stamped transactionally each tick — the shade pattern is the character art. Height changes at 50 rows/s, never more than one row per tick, between ducked and standing; feet stay invariant and a row grows only into free headroom. The observable pose is integer cells: collision, raster, wire, hazards, and rendering all derive from one floor-anchored footprint; sub-cell motion is an internal accumulator, snapped flush on a blocked axis.

The controller is Celeste ported to cells/s, tuned server-side (coyote time, jump buffer, variable height, corner correction, step assists) plus swimming, swept per-axis cell-by-cell against solid and powder cells — powders are walls, digging is the way through. Surfaces contribute authored grip and bounce. Submersion is estimated from the ring around the raster, drags toward the local liquid velocity, and throttles run speed.

The stamp commits the sweep's pose: liquids in newly claimed cells pair into vacated cells or surface up their column within the 64-cell window, refusing the move when no free surface is in reach. Conflicts cascade full → x-only → y-only → stay; a clobbered raster self-heals by full re-stamp; an unchanged raster writes nothing, so an idle player keeps chunks asleep. Two players are mutually solid and exchange momentum. Contact with a live body transfers the blocked controller impulse directly into that body. Death and departure unstamp to air.

## Pixel bodies

A pixel body is an immutable canonical cell shape, fixed-point motion and rotation accumulators, and one flagged world raster. It has no solver state, contact graph, owner plane, damage queue, sleep state, or persistence record.

- **Bonds decide detachment** — rigid materials author bond groups; the symmetric bond matrix flood-fills an island and any solid or powder support rejects it. Moving bodies never merge. Landed cells are terrain again and participate in the next flood fill normally.
- **Detachment discovery is local** — ordinary writes queue nearby rigid cells for support checks. Discovery waits when the whole island margin is not simulated, then atomically flags a detached island.
- **Motion is swept and exact** — gravity and player pushes advance one combined translation-and-rotation traversal over 64 authoritative lattice orientations; continuous turn is only the accumulator between them. Fixed-point traversal steps bound requested point travel and cross at most one orientation per step, with every proposal compared against the immediately preceding valid raster. The reversible integer-lattice rotation map keeps every member unique, and a blocked combined transition is never decomposed into a path that was not swept.
- **Blocked transitions redirect motion** — obstructed entering cell faces propose temporary bounce, slide, and spin responses in the mass-and-inertia metric; redundant faces collapse to their extreme lever arms, and parallel extremes share one whole-body response so broad faces bounce together. Incoming motion uses an impact response, while gravity-only support may sweep around its contact anchor so an overhanging body can tip without penetration or persistent contact state. Each proposal must sweep without crossing a blocked raster boundary before it can become motion; contact-only angle phase continues only when its next authoritative orientation is reachable. Only committed pose displacement contributes gravitational work. The most elastic body or struck cell controls restitution above a small body-wide minimum, tangential projection dissipates friction, and the absence of a valid nonzero successor settles immediately. A simulation frontier freezes the body in place.
- **Relocation is transactional** — only the final raster is committed. Entered liquid and gas cells pair deterministically into vacated cells, conserving matter without a spill search.
- **Interaction stays small** — a player push changes body velocity and torque at the contacted cell. Transfer uses at most the body's mass, so a one-cell body gains the blocked player speed instead of the player's full momentum divided by one cell. Bodies do not solve body–body stacks, buoyancy, crush damage, or in-flight reactions.

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
| Bond group | Authored connectivity class deciding which rigid materials hold together |
| Detachment check | Tick-local request to reconsider the support of a nearby rigid cell |
