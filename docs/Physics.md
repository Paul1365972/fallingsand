# Physics

One movement system over the grid: **bodies** in motion.
A body is a set of real world cells that move as a unit; everything collides per cell against the grid, so terrain changes never rebuild collision geometry.
There is one body kind — a player, a frog, a ball, a corpse and a boulder differ only by policy fields, all mutable at runtime.
Y is up everywhere.

## Substrate

- **Cells are the truth** — a body's cells are real cells in the world raster; the body marks positions and copies nothing.
  A cell's motion bytes are a tagged union — velocity or body id — so a misread is structurally impossible.
  Cell→body is O(1); there are no sidecar structures and no separate collision geometry, nothing is an AABB.
- **Id lifecycle is guarded** — ids are session-monotonic and never reused; every clear writes only cells still bearing the writer's id; saves scrub ids on encode and decode.
- **Owned cells are terrain to the grid** — they run reactions, combustion, decay, digging and material writes exactly as terrain; ownership suspends only independent cell movement.
- **Conservation** — cells never appear or disappear; resolution never creates energy; motion a body cannot realize is never silently destroyed.
- **Integer anchor, quantized motion** — position is the raster; fractions are per-freedom accumulators read by nothing but stepping.
  A quantum crosses at most one cell or orientation step, interleaved Bresenham-style, so no intermediate raster goes untested; a blocked quantum consumes its budget slot.
  Sub-floor velocity accrues no quanta and its accumulator drains, so equilibrium jitter never writes the world.
- **One pass, one law** — whether a body should move, whether it can, how contact redirects it and when it rests are the same logic; there are no separate settle, lean or wake paths.
- **One law, two schedulers** — a free cell is the degenerate one-cell body under the same quantum, entry and contact semantics; detachment is writing an id, settling is clearing it.
- **Forces derive from parts** — each tick the effects kernel applies the same per-cell forces a lone cell gets — weight minus displaced medium, drag, updraft — as impulse and torque on the body id.
  Weight is per cell, so an ore vein leads the fall and off-centre mass turns gravity into spin.
- **Displacement is volume, drag is surface** — buoyancy is the weight of the medium a body displaces and scales with cell count; only drag scales with the touched boundary.
  Getting this backwards makes buoyancy a surface-area law: thin planks float and thick ones sink.
- **Displaced medium is per cell** — every cell is buoyed by the medium at its own height, sampled body-locally; interior cells inherit their row. Read through one accessor, so refining the sample never touches force integration.
- **The waterline falls between rows** — a body whose lift would reverse one row higher carries no vertical force at all, and rests within a cell of its draught. Without that fixed point buoyancy is bang-bang: floaters jackhammer and pump waves.
- **Touch exchanges momentum both ways** — every touching pair trades per-tick impulses, cells into bodies and bodies into cells, bounded by the pair's friction.
  Carriage is friction, not membership: sand rides an accelerating cart by grip and slips when inertia beats it.
- **Weight is momentum flux** — a resting cell on live support transfers its blocked gravity momentum into that support instead of settling.
  Stress rides in the velocity bytes under its own tag, so load can never be impersonated by impact momentum; it propagates one cell per tick and saturates at the grain's repose-derived strength, so a confined shaft silos instead of pressing without bound.
- **Containment is emergent** — no contained-matter state exists; a container and its contents co-move under shared gravity and the contents' own laws run inside a moving boundary.
- **Locality** — every rule is neighbourhood-local, or at worst body-local: a body may read its own raster, never the world beyond it. No column scans, no field solves.
- **Transactional relocation** — displaced liquid and gas pair deterministically into vacated cells; occupancy change per committed step is a bijection.

### Simultaneity

All bodies move in the same instant; no step depends on a spatial order (two interlinked C-shapes have none).
Sequential movers are the failure mode this replaces: whether two bodies touch at all would depend on their ids.
The parallel kernel is round zero; a quantum targeting a cell stamped moved this tick is busy — deferred, never a contact.
Rounds always run to completion; determinism is never traded for a budget.

1. Every body with budget proposes its next quantum; only newly entered cells are tested.
2. A target overlapping terrain, settled matter or any non-proposing body is refused.
3. Overlapping a proposer's pre-move cells creates a dependency, re-validated against actual occupancy at commit.
4. Two proposers claiming one cell: greater closing momentum wins, stable id breaks ties; the loser is refused into a true adjacent contact.
5. Anti-crossing: no entry into a cell vacated this tick by opposing motion — convoys pass, head-on pairs collide.
6. Refusal is monotone and cascades, producing contacts only between adjacent cells, so force chains form exactly.
7. A stalled dependency cycle commits jointly iff the union's occupancy change is injective and every hand-off passes anti-crossing.
8. Between refusal and contact the body's assist policy may answer first — the player's step-up retries from an adjusted pose; only unanswered refusals become contacts.
9. Contacts sort canonically and resolve; budgets recompute from post-impulse velocities; rounds continue until budgets exhaust.

### Contacts

A contact is an adjacent cell pair — face or corner — with its normal the pair's exact separation direction, one of 8; sampled normals and polygon proxies are forbidden, and diagonal lengths are carried exactly or every corner bounce gains energy.

- Resting contact is continuous: any into-surface velocity emits its blocked quantum every tick, so weight rests on support without flickering.
- Restitution belongs to impacts: a refused quantum bounces, a resting probe resolves without it.
- A corner contact counts only when motion closes on both axes; flat sliding is never kicked up by its own leading corner.
- **Impulses are relative** — every contact resolves from the pair's closing velocity, never one side's world-frame speed.
  Reflecting against an assumed infinite mass and refunding a share is not Galilean: it creates energy for any pair closing at less than one side's speed, worst of all for a matched-speed convoy.
- Restitution and friction are per contact from the two materials: restitution pairs by the bouncier, friction multiplies so ice slides on everything.
- Separation targets are fixed once per resolve from closing speed; accumulated impulses push toward them and never past, so a wedged body decays instead of pumping.
  All normals resolve before any friction. Effective mass is exact integer arithmetic combining harmonically, including angular response.
- A refused quantum whose resolve finds nothing closing is deferred: the freedom parks, velocity intact.

### Peers

- **Terrain and settled matter** — infinite mass.
- **A body** — equal and opposite impulse, live velocities, angular response where it holds the turn freedom.
  There is no separate creature peer: an ogre shoving a player, a player kicking a crate and two boulders colliding are one path.
- **Derived support** — a peer whose own opposing contact rests on terrain is immovable along that support normal only; each impulse decomposes, evaluated fresh, depth one, no flag.
- **Powder** — a finite-mass peer at its density, holding like terrain below its authored repose resistance and yielding above it; matter yields by moving, never by rule.
- **Liquid** — yields; relocation pairs it into vacated cells and drag emerges from the momentum spent.
  Surface leveling is momentum along each row's hashed drain direction; a parked cell's drain side is the matter that stopped it, so it can never relaunch backward.
  Displacement is an inelastic exchange at the meeting point's effective mass, so energy only ever dissipates.

### Materials

| Field | Read by | Meaning |
|-------|---------|---------|
| `density` | forces | mass and displaced medium |
| `restitution` | contact law | bounce, paired by the bouncier side |
| `friction` | contact law | tangential coefficient, multiplied across the pair |
| `traction` | locomotion drive | how well a driven body pushes against this surface |
| `cohesion` | powder | holds an open downhill diagonal against topple |
| `bond_group` | membership | which rigid materials hold together |

`traction` and `friction` are deliberately separate: control feel and physics fun are different tunings of one surface.
Grass grippy in `traction` and slick in `friction` gives a live frog footing and lets its corpse slide over the same cells — no body-type check anywhere.
The contact law never reads `traction`; the drive never reads `friction`.

### Settled law

Lessons that recurred across failed architectures until a rule killed them; non-negotiable.

- Cells never leave the grid; every sidecar structure desynced and died.
- Dynamic state never round-trips through cell fields.
- Rotation is a lattice bijection or mass conservation breaks.
- Contact normals are exact cell-pair separation directions; anything sampled bleeds momentum.
- Contact impulses are relative to the pair; world-frame reflection plus refund creates energy.
- No band-aid state: rest is a fixed point, support is derived, restitution is a per-resolve target.
- Simultaneity replaces ordering; ordering cannot be defined for interlocked or horizontally moving bodies.
- One substrate, one implementation. A second mover for a second body kind diverges, and its bugs are unreachable from the first one's tests.

## Bodies

An id, its owned cells, a pose, a velocity, inertia derived from its materials, and three mutable policy fields.

| Field | Meaning |
|-------|---------|
| `turns` | may take turn quanta; without it a body never gains spin |
| `settles` | may become terrain at its fixed point |
| `assists` | what may answer a refused quantum before it becomes a contact |

| | turns | settles | assists |
|---|---|---|---|
| debris | yes | yes | — |
| ball | yes | **no** | — |
| mob | no | no | — |
| player | no | no | step-up, snap-down |
| corpse | yes | yes | — |

There is no form distinction: a body's cells are exactly the cells bearing its id, and an authored cell-set swap is available to any body.
Species are initializers, not types; death and destruction are transitions that write these fields.
The ball is debris that never settles — that is the whole of its species, and why it stays forever kickable. Its shading rotates with it, so rolling is what the turn freedom looks like.

- **Pose is a proposal** — rotating a body and swapping in an authored cell set are one law: propose a new cell set, commit transactionally iff it is a lattice bijection into free space.
  Entered liquids pair into vacated cells or surface up a connected fluid column stopping at the first solid; an unreachable surface refuses the pose, so ducking under water is reversible.
- **Species are content** — one registry entry: a name, frame art whose marks paint materials and shades, initial policy, and for the living a mind driver, hit points and a corpse material.
  Summoning, painting, hazards and death all read that one unit, so a new species is one definition, never a new code path. The frame raster is the collision and the sprite, one truth.
  Locomotion tuning lives with the species; world physics never does. A species may scale the world's own weight, never invent a second gravity.
- **Controllers are not physics** — a controller reads support, contact, weight and submersion state and writes drives; it never moves the raster.
  Drives land between force integration and the rounds, so what a controller writes is what moves. Physics owns bodies, gameplay owns minds.
- **Death is a transition** — `die` adds turn and sets settles, dropping the assists and the controller.
  Id and motion stay continuous, so a frog shot mid-hop tumbles from the velocity it had. It is a field write, not a respawn, because bodies are indexed positionally inside a round and death arrives from contact impulses mid-round.
  Every death recasts flesh into the species' corpse matter — it decays to smoke and salvages to nothing, so no corpse outlives its interest; a player respawns as a new body.
- **Membership follows the grid** — a body's cells are exactly the cells bearing its id, reconciled every tick; it re-derives from bonds and splits into parts. Its matter digs, burns and reacts as that material does in terrain, and losing coherence that way is the same `die`.
- **Bonds decide structure** — rigid materials author bond groups; a symmetric matrix flood-fills an island. Unbondable matter releases as free cells with its momentum share; newly adjacent cells are never added, so live bodies cannot weld.
- **Detachment is local** — a grid write unseats its rigid neighbourhood; discovery flood-fills and flags an island atomically, parking under an unsimulated chunk and waking with it. Id-bearing cells are flood boundaries, never candidates.
  A region load is a detachment event: every exposed bonded cell reseeds discovery, so matter settled by an unload resumes as a body when its region returns.
- **Anchoring is adhesion** — an island holds while any member touches a foreign structural solid, or rests on powder from below; weak matter below a minimal hardness never anchors.
- **Rotation** is quantized to 256 orientations as nearest quarter-turn refined by shears — an exact lattice bijection. A turn quantum probes every cell its slots cross, so a felled tree cannot sweep through a wall.
- Species flesh saves as air while alive; a settled corpse is ordinary matter and persists as terrain.

### Settle

Rest is the fixed point of the pass, decided in isolation: no external impulse, post-snap velocity and spin zero, ambient force resolving to zero realizable motion — then terrain the same tick. Only bodies whose `settles` is set are candidates.
Support lies in the direction the net ambient force presses, so a buoyant body settles against a ceiling exactly as a heavy one settles onto a floor.

- The snap threshold is rounding-scale, not gravity-scale, or every tipping plank freezes on tick one.
- A plank with its centre of mass past the ledge is not a fixed point; a body wedged in a crack is one.
- Settle requires every load-bearing contact to end in terrain; a crate on a player's head never becomes terrain.
- A probe into an unloaded cell parks the body, velocity intact — never a contact, never a settle candidate — and a parked body accrues no forces until its whole neighbourhood simulates again.
- An unloading region despawns its minded bodies and settles the rest into terrain, so cells never leave the grid.

### Out of scope

Explicitly deferred; nothing here may be partially implemented.

- **Enclosed-void buoyancy** — a hull floats on its own cells, not the air it encloses; finding an enclosed void is a flood fill and is not local.
- **Static fluid pressure** — hydraulics, sealed presses, U-tubes; buoyancy needs none of it.
- **Kick-to-detach of settled matter** — radial contact impulses are the natural candidate.
- **Crush damage** — hazards from contact impulses.

## Feel benchmarks

Playtest criteria; human feel decides.

- The player's movement feels identical before and after the substrate: every Celeste assist survives quantization.
- A heavy slab lands on a grounded player and rests; a sideways wedge shoves them; a kicked light crate flies.
- A ball rolls over a one-cell bump instead of reversing off it; a row of balls at matched speed drifts without shoving itself apart.
- A diagonal plank landing on a diagonal plank slides off sideways; debris on a slope rolls or slides by friction.
- Interlinked C-shapes fall as one, no gap, no phantom impulses; a falling stack stays closed at any speed.
- An anvil dropped on sand scatters it, then rests on it; a beam on a fulcrum tips toward a poured pile.
- A wooden plank bobs as it lands, then lies dead still at its draught on a flat pond; a thick block floats too; an anvil sinks; rising floodwater lifts loose debris.
- A balloon rises at balloon pace with its string swinging it upright, and settles against a ceiling.
- A falling cauldron keeps its water, sloshes on landing, and pours when tipped.
- A landed crate is still within a tick; a teetering plank never freezes mid-tip.
- A beam burned through snaps into parts that fall separately with conserved momentum.
- A frog dies mid-hop and tumbles from the motion it had; its corpse slides on grass its live self gripped.
- A killed player leaves a corpse that tumbles from the motion it had and smokes away while the new avatar walks back to it.

## Glossary

| Term | Meaning |
|------|---------|
| Body | An id owning cells and moving them as one; the only kind |
| Freedoms | Which of turn, x, y a body may take quanta on |
| Assist | What a body may do with a refused quantum before it becomes a contact |
| Species | One registry entry: name, frame art, policy, and for the living hit points, corpse matter, mind driver |
| Frame | One authored art grid of a species; each mark paints a material and shade, the raster is collision and sprite |
| Mind | The per-mob state a species driver reads and writes |
| Supported | Solid or powder under any cell, own cells excluded — another body counts; only terrain arms snap-down and derived support |
| Controller | The mind driving a body; never moves the raster |
| Drive | A controller's per-tick velocity intent, written after forces and before the rounds |
| Weight | The per-tick velocity change gravity and buoyancy give a body; a controller reads it, never recomputes it |
| Avatar | The physical realization of an alive player |
| Flesh | The player's inert material — id-bearing, undiggable, omitted from snapshots |
| Corpse matter | What death recasts flesh into: ordinary settling matter that decays to smoke and salvages to nothing |
| Subcell | Fixed-point motion accumulator per freedom |
| Freedom | One swept degree of a pose: turn, x, or y |
| Quantum | One step of one freedom, crossing at most one cell or orientation |
| Round | One synchronous propose/commit step of all bodies at once |
| Convoy | A dependency cycle committing jointly as a bijective occupancy change |
| Anti-crossing | No entry into a cell vacated this tick by opposing motion |
| Contact | An adjacent cell pair with the pair's exact separation direction as normal |
| Closing velocity | The pair-relative speed along a contact normal; the only input to its impulse |
| Derived support | Per-impulse immovability along a peer's own terrain-backed support normal |
| Displaced medium | The medium a body's volume stands in; the buoyancy term |
| Traction | Authored surface grip read only by locomotion drive |
| Fixed point | The rest condition: nothing external, nothing moving, nothing realizable |
| Stress | Tagged load momentum in a resting grain's velocity bytes; never motion |
