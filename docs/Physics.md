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

A pixel body is canonical local slots, one flagged world raster, and fixed-point pose, velocity and spin. A slot is an offset plus the material last read from the grid; mass, bond group, restitution and grip derive from it, so a body copies no material state and owns no solver state, contact graph, owner plane, damage queue, sleep state, or persistence record. Mass is density-weighted, so an ore vein leads a fall.

- **Bonds decide structure, adjacency decides carriage** — rigid materials author bond groups; the symmetric matrix flood-fills an island, and any obstructing cell below rejects it. The same matrix splits surviving slots, so burning through a beam snaps it. Owned matter that cannot bond joins whichever part it touches; matter touching none returns to terrain. Newly adjacent cells are never considered, so active bodies cannot weld.
- **Reconciliation preserves motion** — one linear scan compares every slot against the grid. Survivors keep the raster bit-for-bit: shifting the pose by the travel of the centre of mass leaves the rasterisation pivot unchanged. Angular velocity carries over untouched, because a departing cell takes exactly its own share of the angular momentum. No dynamic state round-trips through cell fields.
- **Detachment discovery is local** — withdrawing support unseats the rigid neighbours of the write; adding it unseats only the written cell. Velocity-only writes unseat nothing. Discovery waits when the island margin is not simulated, then atomically flags a detached island.
- **Motion is swept per freedom** — turn, x and y each advance alone, one quantum at a time, across 64 authoritative lattice orientations; continuous turn is only the accumulator between them. A quantum crosses at most one cell or one orientation, so no intermediate raster goes untested and a floor that refuses the fall never vetoes the turn that tips a body over its edge. The reversible lattice rotation keeps every slot unique.
- **Contacts are impulses** — rejected entering cells become cardinal contacts, and a body whose motion changed no cell leans one cell into gravity to ask what holds it up, so weight rests on its contacts every tick instead of quietly free-falling between raster crossings. Accumulated sequential impulses resolve every normal before any friction, which is what stops a symmetric landing crabbing sideways. Effective mass is exact integer arithmetic and combines harmonically. Off-centre support turns gravity into spin, so an overhang topples with no rule saying it should.
- **One impact per tick** — restitution is a target separation speed only the tick's first impact may ask for; later contacts are inelastic. A body wedged between two surfaces therefore cannot pump itself full of energy.
- **A blocked tick keeps its budget** — the unspent fraction is re-swept against the post-impulse velocity, so bodies slide and roll instead of stopping at a graze. A freedom whose refusal the impulse cannot answer is absorbed, because an orientation snap can drive a cell where no linear impulse opposes it; motion a body cannot realize never survives its tick, so a jam cannot hoard weight and release it as a launch. A simulation frontier freezes the body.
- **Everything solid is a peer** — terrain has infinite mass, another body takes the equal and opposite impulse, and a live player raster takes its reaction as a velocity change routed to that avatar. A player carries one shared velocity for the whole tick, so several contacts cannot each re-apply the same reaction, and a grounded one is braced: an impulse pressing them into their footing goes to the ground. A slab lands on you; it does not launch you. Bodies do not solve stacks, buoyancy, or crush damage.
- **Relocation is transactional** — only the final raster is committed. Entered liquid and gas pair deterministically into vacated cells, conserving matter without a spill search.
- **Rest, then terrain** — a body is a motion event. A second and a half of touching something without changing a cell writes the raster back as terrain; a body with nothing under it never counts, so an unsupported fragment cannot freeze in mid-air. Losing cells to fire does not move a body, so a burning pile still settles.
- **Interaction stays small** — a player push changes velocity and spin at the contacted cell, transferring at most the body's mass. Grid writes handle digging and chemistry without body-specific paths.

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
| Support class | What a cell contributes to holding rigid matter up: obstruction plus bond group |
| Unseated | A cell a write may have left standing on nothing |
| Stander | A live player raster as a contact peer: finite mass, braced when grounded |
| Freedom | One swept degree of a body's pose: turn, x, or y |
| Rest window | 1.5s supported and unmoved, after which a body writes itself back as terrain |
