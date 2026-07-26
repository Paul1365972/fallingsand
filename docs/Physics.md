# Physics

One movement system over the grid: **shapes** in motion.
A shape is a set of real world cells that move as a unit; everything collides per cell against the grid, so terrain changes never rebuild collision geometry.
Two kinds share the substrate: **creatures** — controlled, living, never rotating, never settling — and **debris** — detached rigid matter that tumbles and returns to terrain.
Y is up everywhere.
The player is the substrate's first creature and runs on the body-id union; mobs, debris, and the contact law are design awaiting implementation.

## Substrate

- **Cells are the truth** — a shape's cells are real cells in the world raster; the shape marks positions and copies nothing.
  A cell's motion bytes are a tagged union — velocity or body id, discriminated by one flag bit and read only through that enum — so a misread is structurally impossible; in practice only members bear ids, and everything else keeps honest velocity.
  Cell→body is O(1) and the grid is the membership store; there are no sidecar structures and no separate collision geometry: per-cell shape is the sole truth, nothing is an AABB.
- **Id lifecycle is guarded** — ids are session-monotonic and never reused; every clear writes only cells still bearing the writer's id; saves scrub ids on encode and decode both, so no id ever enters the sim from disk.
- **Owned cells are terrain to the grid** — they run reactions, combustion, decay, digging, and material writes exactly as terrain; ownership suspends only independent cell movement.
- **Conservation** — cells never appear or disappear; resolution never creates energy; motion a shape cannot realize is never silently destroyed.
- **Integer anchor, quantized motion** — a body's position is its raster; fractions exist only as per-freedom motion accumulators, read by nothing but stepping.
  Velocity converts each tick to motion quanta per freedom, interleaved Bresenham-style; a quantum crosses at most one cell or orientation step, so no intermediate raster goes untested.
  A blocked quantum consumes its budget slot.
  Debris has three freedoms (turn, x, y); creatures have two.
- **One pass, one law** — whether a shape should move, whether it can, how contact redirects it, and when it rests are the same underlying logic; there are no separate settle, lean, or wake paths.
- **One law, two schedulers** — a free dynamic cell is the degenerate one-cell body under the same quantum, entry, and contact semantics; detachment is writing an id, settling is clearing it.
  The parallel kernel executes single cells; the rounds execute rigid bodies; the law is identical on both sides.
- **Forces derive from parts** — a body stores only velocity and spin; each tick the effects kernel applies the same per-cell forces a lone cell gets — weight minus displaced medium, drag when submerged, updraft — accumulated as impulse and torque on the body id.
  Buoyancy, water drag, and currents therefore emerge from the existing cell laws: a plank floats because its submerged cells individually receive buoyant force.
- **Touch exchanges momentum both ways** — every touching pair trades per-tick impulses, cells into bodies and bodies into cells, bounded by the pair's friction.
  Carriage is friction, not membership: sand stays on an accelerating cart because the cart drags it, and slips when inertia beats grip — nothing rides by attachment.
- **Weight is momentum flux** — a resting cell whose support is live (a body cell, or a support that transferred this tick) does not settle; it transfers its blocked gravity momentum into its support instead.
  Load therefore propagates one cell per tick through the existing exchange, the pile wakes upward through the moved stamp and falls asleep from the root when the support becomes terrain, and a body under a pile receives the pile's true weight and torque — a scale tips with no law naming scales.
  Transmission saturates at the grain's repose-derived strength: load beyond the cap is refused, the refused momentum stays upstream and the pile fails by toppling — a confined shaft saturates like a silo instead of pressing without bound.
  A cell losing support releases at most its cap, never a column's worth of momentum; stress is not motion, and only the saturated flux ever becomes velocity.
- **Containment is emergent** — contents are never carried and no contained-matter state exists: a container and its contents co-move under shared gravity, displacement pairing handles overlap, and sloshing, pouring, and cargo tumbling are the contents' own laws running inside a moving boundary.
- **Locality** — every rule is strictly neighbourhood-local; no column scans, no field solves, no action at a distance.
- **Transactional relocation** — displaced liquid and gas pair deterministically into vacated cells; occupancy change per committed step is a bijection.

### Simultaneity

All shapes move in the same instant; no step of the system depends on a spatial order (two interlinked C-shapes have none).
The parallel kernel is round zero: free cells take their quanta first, and the body rounds read the post-kernel world.
A quantum targeting a cell stamped moved this tick is busy — deferred, never a contact — so neither scheduler phases through the other's motion; contents see their container one round late, the same one-cell-per-tick locality as every law.
The rounds always run to completion; determinism is never traded for a budget.
Realization proceeds in synchronous rounds:

1. Every shape with remaining budget proposes its next quantum; the tested targets are its newly entered cells only.
2. A target overlapping terrain, settled matter, or any non-proposing shape is refused outright.
3. A target overlapping another proposer's pre-move cells creates a dependency: the commit needs that proposer to vacate.
   Dependencies re-validate against actual occupancy at commit time, so a retained or rotated-into cell refuses the dependent rather than double-owning.
4. Two proposers claiming the same cell: the greater closing momentum wins (stable id breaks ties); the loser is refused by the winner's new occupancy — a true adjacent contact, no frozen gap.
5. Anti-crossing: no shape may enter a cell vacated this tick by opposing motion (negative velocity dot); convoys and followers pass, head-on pairs collide instead of phase-ghosting.
6. Refusal is monotone and cascades: a refused shape's cells are static for the round, refusing dependents in turn; cascades only ever produce contacts between adjacent cells, so force chains form exactly.
7. A stalled dependency cycle commits jointly iff the union's occupancy change is injective, its newly entered cells are free, and every hand-off passes anti-crossing — interlinked C-shapes convoy, a carousel turns, a swap collides.
8. Between refusal and contact, a shape's own policy may answer first — a creature's step-up retries the quantum from an adjusted pose; only unanswered refusals become contacts.
9. When a round stalls, refused quanta have become contacts; contacts sort canonically (shape pair, then cell) and resolve; budgets recompute from post-impulse velocities; rounds continue until budgets exhaust.

### Contacts

A contact is an adjacent cell pair — face or corner — with its normal the pair's exact separation direction, one of 8; sampled normals and polygon proxies are forbidden.
Diagonal normal lengths are carried exactly, or every corner bounce gains energy.

- Blocked quanta produce contacts, and resting contact is continuous: any into-surface velocity at a cell boundary emits its blocked quantum every tick, so weight rests on support and derived support never flickers with subcell phase.
- A corner contact counts only when motion closes on both of its axes; flat sliding is never kicked upward by its own leading corner.
- Restitution and friction are per contact from the two touching materials: restitution pairs by the bouncier, friction multiplies so ice slides on everything.
- Resolution fixes every contact's separation target once from its closing speed at resolve start; accumulated sequential impulses push toward targets and never past, so a wedged shape decays instead of pumping.
  All normals resolve before any friction, so a symmetric landing cannot crab sideways.
  Effective mass is exact integer arithmetic combining harmonically, including angular response — off-centre support turns gravity into spin, so overhangs topple with no rule saying they should.
- A refused quantum whose resolve finds nothing closing is deferred: the freedom parks for the tick, velocity intact, and realizes next tick — a decomposition artifact, not lost momentum.

### Peers

Everything solid a contact touches is a peer with a mass.

- **Terrain and settled matter** — infinite mass.
- **Debris** — equal and opposite impulse, live velocities.
- **A creature** — a non-rotating peer of finite, density-weighted mass; the impulse arrives as a velocity change.
  Creature↔creature and creature↔debris resolve through the same law — an ogre shoves a player, a player kicks a critter — and creatures are mutually solid.
- **Derived support** — a peer whose own opposing contact rests on terrain is immovable in translation along that support normal only.
  Each impulse decomposes: the support-axis component routes to ground, the residual sees finite mass and true angular inertia; evaluated fresh per impulse, depth one, no flag.
  A slab rests on a grounded player's head while a sideways wedge still shoves them; an anvil on a plank's cantilevered tip still tips it.
- **Powder** — a finite-mass peer at its density, holding like terrain below its authored repose resistance and yielding above it as cell velocity; matter yields by moving, never by rule.
- **Liquid** — yields; relocation pairs it into vacated cells, and drag emerges from the momentum spent moving them.

### Materials

Shapes read only existing authored data: `density` weighs mass, `restitution` is the same contact bounce cell collisions read, `friction` is the rigid-contact tangential coefficient, and a powder's repose resistance is its topple resistance.
The reset removed `friction` and bond groups from content entirely; both are re-authored when implementation starts.
`entity_grip` and `entity_bounce` are underfoot feel, read only by creature locomotion — never by the contact law.

### Settled law

Lessons that recurred across failed architectures until a rule killed them; non-negotiable.

- Cells never leave the grid; every sidecar structure desynced from the raster and died.
- Dynamic state never round-trips through cell fields.
- Rotation is a lattice bijection or mass conservation breaks.
- Contact normals are exact cell-pair separation directions; anything sampled bleeds momentum.
- No band-aid state: rest is a fixed point, support is derived, restitution is a per-resolve target.
- Simultaneity replaces ordering; ordering cannot be defined for interlocked or horizontally moving shapes.

## Creatures

A creature is a shape with a controller: pose, velocity, cell shape, health.
It never rotates and never settles; players and mobs are the same kind, differing in controller — network or AI.

- **Locomotion splits in two** — the substrate applies the physical world identically to every creature: gravity, buoyancy, fluid drag toward local flow, grip and bounce from touched materials, submersion from the liquid the shape displaced, the quantized sweep.
  Each species' controller produces its own drives on top — no shared intent vocabulary, because a bat's hover and a player's coyote jump share nothing.
  The player's controller is the Celeste port, tuned server-side; its assists — step-up with climb debt, ceiling corner correction, snap-down — are controller policy expressed as pose adjustments on refused quanta, not substrate law.
- **Animation drives shape** — each animation frame's cell set is the shape; the raster is the art and the collision, one truth.
  A shape transition commits transactionally and only grows into free space — the duck/stand row-stepping generalized.
  The player's shape is currently the full 3×N rectangle; silhouettes with hands come with animation frames.
- **The player** — an alive avatar is a raster of inert flesh cells bearing its body id, stamped transactionally each tick.
  Height changes at 50 rows/s, one row per tick at most, feet invariant; the observable pose is integer cells, sub-cell motion an internal accumulator snapped flush on a blocked axis.
  Powders are walls to creatures — digging is the way through.
  The stamp commits the sweep's pose: entered liquids pair into vacated cells or surface up a connected fluid column stopping at the first solid, so displacement never crosses a barrier; an unreachable surface refuses the pose — motion falls back to the held shape at the swept position, height growth waits, a spawn candidate is rejected.
  The stamp records which claimed cells held liquid; the record follows partial restamps, absorbs liquid found intruding during a heal, and drains to zero over a few ticks once no liquid borders the raster — it is the creature's submersion and displaced density, with flow still sampled from the bordering ring.
  A clobbered raster self-heals by re-stamp; an unchanged raster writes nothing, keeping chunks asleep.
  Death and departure unstamp to air.
- **Interim momentum channel** — until the contact law lands, blocked sweep motion against another creature transfers the exact fixed-point velocity delta straight into the receiver's velocity at the end of the pass; no deferred state exists.
  The channel dissolves into the rounds' in-pass contact resolution.

## Debris

Debris is a transient motion event over the substrate: detached rigid matter that falls, tumbles, collides, and settles back to terrain.
Persistent state per debris is exactly slots, pose, velocity, spin — nothing else survives a tick.

### Structure

- **Membership is the id** — a debris' cells are exactly the cells bearing its id; mass, centre of mass, restitution, friction, and bonds derive from their materials, recomputed on change.
  Any member or frontier list is a rebuildable cache, never truth; there is no canonical copy to reconcile.
  Mass and centre of mass are density-weighted, so an ore vein leads the fall.
- **Bonds decide structure** — rigid materials author bond groups; a symmetric matrix flood-fills an island.
  Matter that cannot bond is released as free cells carrying their momentum share; newly adjacent cells are never added, so live debris cannot weld.
- **Membership follows writes** — a transmutation product that is no longer bondable releases with its momentum share (`v + ω×r`); a departing cell takes exactly its own share of momentum and angular momentum.
  The bond matrix splits survivors into parts.
- **Detachment is local** — a grid write unseats its rigid neighbourhood; discovery flood-fills from unseated cells and flags a detached island atomically, waiting while its margin is unsimulated.
  Id-bearing cells are flood boundaries, never candidates — splitting a live body is exclusively its own bond recheck.

### Rotation

Rotation is quantized to 64 orientations as nearest quarter-turn refined by shears — an exact lattice bijection, so mass conservation is structural.
Continuous spin is only the accumulator between orientation steps.
A turn quantum on a wide shape probes every cell its slots cross, so a felled tree cannot sweep through a wall.

### Settle

Rest is the fixed point of the pass, decided in isolation: no external impulse, post-snap velocity and spin zero, ambient force resolving through the standing contacts to zero realizable motion — then terrain the same tick.

- The snap threshold is rounding-scale, not gravity-scale, or every tipping plank freezes on tick one.
- A plank with its centre of mass past the ledge is not a fixed point — gravity becomes spin through the off-centre contact; a shape wedged in a crack is one, and settles instantly.
- Spin that changes no raster is discarded at settle, never stored.
- Settle requires every load-bearing contact to end in terrain or settled matter; a crate on a player's head never becomes terrain.
- Unload settles crossing debris before extraction, both halves; saves never record runtime state, so a live raster persists naturally as terrain — there is no debris persistence format.
- A probe into an unloaded cell parks the whole body for the tick, velocity intact — never a contact, never a bounce, never a settle candidate; creatures keep their hard frontier wall as controller policy.

### Out of scope

Explicitly deferred; nothing here may be partially implemented.

- **Kick-to-detach of settled matter** — a settled crate is terrain until a later mechanic frees it; radial contact impulses (explosions) are the natural candidate.
- **Crush damage** — hazards from contact impulses.
- **Static fluid pressure** — hydraulics, sealed presses, U-tubes under load; buoyancy needs none of it and the pressure solve stays banned.

## Feel benchmarks

Playtest criteria; human feel decides.

- A diagonal plank landing on a diagonal plank slides off sideways, never a dead vertical bounce.
- Debris on a slope rolls or slides, decided by friction; a ledge or a player's head rotates a landing naturally.
- A heavy slab lands on a grounded player and rests; a sideways wedge shoves them; a kicked light crate flies.
- Interlinked C-shapes fall as one, no gap, no phantom impulses; a falling stack stays closed at any speed.
- An anvil dropped on sand scatters it, then rests on it.
- A wooden plank floats on water and bobs to equilibrium; an anvil sinks with drag; rising floodwater lifts loose debris.
- A beam balanced on a fulcrum tips toward a sand pile poured on one pan against a player standing on the other.
- A falling cauldron keeps its water, sloshes on landing, and pours when tipped; sand on an accelerating cart is dragged along and slips when jerked.
- A landed crate is still within a tick; a teetering plank never freezes mid-tip.
- A beam burned through snaps into parts that fall separately with conserved momentum.
- The player's movement feels identical before and after the substrate: every Celeste assist survives quantization.

## Glossary

| Term | Meaning |
|------|---------|
| Body | Anything with an id owning cells and moving them as one; creatures and debris are its kinds |
| Shape | A body's set of owned world cells; the sole collision truth |
| Creature | A body with a controller: living, non-rotating, never settles; players and mobs |
| Debris | A transient rigid body: members, anchor, velocity, spin; settles back to terrain |
| Avatar | The physical realization of an alive player: creature, raster, health, interaction |
| Flesh | The player's inert material — id-bearing, undiggable, omitted from region snapshots |
| Body id | A cell's motion bytes under the tag: which body owns the cell |
| Subcell | Fixed-point motion accumulator per freedom; exact in saves, never on the wire |
| Bond group | Authored connectivity class deciding which rigid materials hold together |
| Freedom | One swept degree of a pose: turn, x, or y |
| Quantum | One step of one freedom, crossing at most one cell or orientation |
| Round | One synchronous propose/commit step of all shapes at once |
| Convoy | A dependency cycle committing jointly as a bijective occupancy change |
| Anti-crossing | No entry into a cell vacated this tick by opposing motion |
| Contact | An adjacent cell pair with the pair's exact separation direction as normal |
| Separation target | A contact's resolved-for velocity, fixed once per resolve from its closing speed |
| Derived support | Per-impulse immovability along a peer's own terrain-backed support normal |
| Deferral | Parking a refused freedom for the tick, velocity intact |
| Fixed point | The rest condition: nothing external, nothing moving, nothing realizable |
| Shove channel | Interim creature↔creature velocity transfer until the contact law lands |
