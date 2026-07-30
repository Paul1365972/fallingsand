# World Generation

## Invariants

- **A pure function** — generation maps (seed, region) to cells deterministically; regions generate independently in any order — there is no whole-world step.
- **Overlap, not sequence** — cross-border features use deterministic overlap generation, never sequential dependency. Every feature is anchored on a hashed lattice and clamped to a bounded reach, so each overlapping region re-derives the identical feature and keeps only its own intersection.
- **Place is two-dimensional** — a place is named by a *biome* and a *sub-biome*, both functions of x **and** y. There are no depth layers, no tiers, and nothing anywhere reports a band of y. A world built from a biome-of-x crossed with a band-of-y is a grid, and a grid reads as vertical stripes and horizontal rules no matter how the boundaries are warped.
- **Depth is an axis, not a ladder** — depth is one input among eight, and what a place is *made of* is carried by its own 2D axis instead. Descending changes the odds rather than stepping through an ordered set of tiers. A biome that fades out with depth must do so by losing the scoring contest, never by being switched off at a threshold.
- **Terrain shape is never a biome property** — the silhouette comes from the terrain fields alone, and biomes are chosen from those same fields. This breaks the circular dependency, deletes the blend band, and means a biome *describes* its landform instead of dictating it.
- **No fiat gates** — depth is gated by hazard and preparation, never by bedrock and never by tool tier. Hardness is dig *time* only.
- **One scale knob** — every dimension is authored relative to the player avatar. `scale::PLAYER_WIDTH` is the only edit needed to retune the whole world.
- **Nothing generated may be born falling** — the sim promotes any detached cluster of bonded solids into a rigid body, so a placed structure that is not anchored becomes debris the moment the chunk wakes. See *Standing up*.
- **Foliage is one connected body** — a crown is flood-filled from the trunk column and anything unreachable is dropped, so no leaf ever hangs in open air. A per-cell stipple alone always leaves orphans.

The benchmark is Terraria × Noita × modern Minecraft. `examples/preview.rs` renders regions to PNG (`--step` downsamples for kilometre-wide views, `PROBE=1` prints terrain statistics, per-depth biome and sub-biome histograms and air fractions) and is the tuning loop. Histograms are the guard against a biome that reads well but never occurs, or one that quietly swallows its neighbours.

## Scale

A cell is roughly 20 cm, which makes the reference 3×9 avatar a 0.6 × 1.8 m human — but metres are only a plausibility check, never a generator input. Three helpers carry the rule set:

| Helper | Scales | Used for |
|---|---|---|
| `len(n)` | linearly | depths, radii, thicknesses, clearances |
| `wave(n)` | linearly | noise **wavelengths in cells**, divided into the sample coordinate |
| `pitch(n)` | linearly, snapped to 8 | anchor lattice spacings |

Feature counts are never authored: a lattice of pitch `p` yields `(512/p)²` anchors per region, so per-area density falls off as 1/S² by construction. Thresholds, probabilities, densities and aspect ratios are dimensionless and never scale. Depths are authored negative, and `len` floors magnitude rather than sign.
## Parameters

Eight dimensionless axes, all in 0..1, describe every point in the world:

| Axis | Source | Meaning |
|---|---|---|
| `land` | f(x), long wavelength | ocean basin … shore … lowland … upland |
| `relief` | f(x) | erosion: rugged … smoothed |
| `rock` | f(x, y), long wavelength | what the country is *made of*: carbonate … clastic … crystalline … igneous |
| `heat` | f(x, y) + geothermal ramp | the deep world is hot on average, and more *variable* with depth, so cold pockets exist at any depth |
| `wet` | f(x, y) | drowned … bone dry, likewise widening with depth |
| `weird` | f(x, y), squared upper tail, plus a depth ramp | the anomaly axis |
| `depth` | (surface − y), warped, then `raw/(raw+h)` | a saturating coordinate: 0 at the surface, no bottom |
| `variant` | f(x, y), short wavelength | picks between sub-biomes inside one biome; means nothing on its own |

`land` and `relief` also drive the height function, so a rugged wet upland is where the cloudforest *is* rather than something the cloudforest causes.

**`rock` is the axis that keeps depth honest.** Lithology used to be implied by how deep you were — soil, then shale, then granite, then void — which is a ladder no amount of boundary warping disguises. Making it its own two-dimensional field means limestone country and granite country interleave at *every* depth, and `depth` is left to control only what it should: how open the caves are, how rich the ore, and how strange the world gets.

## Materials

The palette is weighted the way a falling-sand game plays, not the way a geology textbook reads: a handful of structural rocks, and everything else reactive. Inert stone that differs only in colour is dead weight — it costs a material slot, a colour ramp and a biome window, and the player can do nothing with it. A rock earns its place by being the wall of somewhere, and anything else earns it by burning, dissolving, flowing, choking or setting.

**Every buried biome is defined by its hazard, not by its stone.** Coal Pits is not "the dark grey rock" — it is the place where firedamp collects and everything burns at once. The Sludgeworks is toxic sludge and the gas coming off it. Acid Caves eat their own walls. Drowned Halls flood. That is the axis the player actually feels, and it is what the roster is authored against.

Reactions are the point, so a new fluid without one is decoration: acid dissolves anything tagged, lava quenches to obsidian, oil and alcohol float and burn, cement sets into concrete, sulfur and sludge cook off into toxic gas, and toxic gas over steam condenses back to acid.

## Biomes and sub-biomes

A **biome** is the coarse 2D region — the name a player uses for where they are. It declares a window on every axis and owns nothing else. A **sub-biome** is a patch inside one biome, and owns everything material: stone and its bedding companion, sediment, fluid and fluid level, gas and pocket density, cave character, ore lithology and prize, and — only above ground — a `skin` giving the soil stack, trees, ground cover and boulders.

Biomes are either **daylight** or **buried**, and both come from the same **jittered anchor lattice** in warped (x, above-surface) space. The top row of that lattice is the soil skin: an anchor sitting inside it picks from the daylight biomes, an anchor below it picks from the buried ones. Every row is otherwise square, which matters — cells much wider than they are tall put every boundary within reach of a horizontal walk and none within a vertical one, and that is indistinguishable from a function of x.

**There is no separate per-column path for the surface, and there must never be one.** A lookup keyed on x alone makes every biome boundary a perfectly vertical cut through the whole soil column, and no amount of dithering or fringe noise hides it — it only turns one clean cut into several. The skin is a row of the lattice like any other, so its boundaries are warped and ragged for the same reason every other boundary is. `PROBE=1` reports how many sub-biome seams run straight for 40+ cells; that number is the guard, and it belongs near zero.

**The lattice domain is `above >= 0`.** `Lattice::coords` clamps every query to the surface, so open sky resolves to the skin row of the column beneath it and a region of pure air is generated from the same rows as the ground below. Anything that gives the sky a place of its own extends the row space to negative rows — `row_of` and `above_of` are already inverses there — rather than adding a second lookup beside the lattice.

Sub-biomes are picked from their biome's members on a finer lattice, so one biome breaks into patches instead of being uniform. A sub-biome is indivisible within its cell: a boundary is a single clean transition and a sub-biome can never flicker back and forth across one.

Selection is a single scored match: squared distance to each declared window, summed, `depth` and `rock` weighted above the rest. Nothing can fall through — the nearest biome wins when no window matches. Three rules keep the roster honest:

- **The `rock` axis must tile.** Every value in 0..1 has to fall inside some buried biome's window, or points in the gap get captured by whatever is nearest on an unrelated axis.
- **Every biome needs a generic sub-biome** whose windows are all wide open, or a point that matches no member gets an arbitrary one.
- **Every non-generic sub-biome needs a priority above zero.** Equal scores break by list order, so a generic listed first silently erases the specials that overlap it.

## Caves

In 2D the zero set of a smooth field is a *curve*, and thresholded noise below ~50 % air percolates into isolated pockets rather than a connected system. Connectivity therefore comes from explicit structure, never from tuning a threshold — the noise-carver approach is what produces uniform unwalkable mush.

- **Worms** are the skeleton and the only guarantee of connectivity: parametric polylines rasterised as capsule chains on two lattices, local and trunk. Step length is derived from the radius, never authored — a step longer than the radius leaves gaps and the tunnel comes out as a dotted line. Curvature is applied per cell travelled, so thin worms do not spin into scribbles, and trunks sway far less than locals so they read as through-routes rather than more wandering. Radius is drawn through a bimodal remap so corridors are crawl-sized or gallery-sized and rarely the featureless middle, and both modes are floored well above the avatar: **a passage narrower than the player is not a cave, it is a crack**. Branch points are chosen up front and bounded to one generation; a per-step branch probability compounds into an explosion, and a second generation of branches is just dead arms. Prefer few fat long worms over many thin ones — the same air budget spent on width instead of count is the difference between a cave system and a root ball.
- **Placement is one worm per lattice cell, not a low chance on a fine lattice.** Both give the same mean, but a sparse Bernoulli process has Poisson variance, so at a few anchors per region some screens come out solid and others honeycombed. A coarse lattice fired near-certainly spreads the same air evenly; the sub-biome's `worms` factor and the elevation falloff then suppress it where it should be absent.
- **Loose matter must not line a passage.** A powder with open space below or beside it avalanches the moment the chunk wakes, so a tunnel cut through sand or soil refills itself. Powder touching a cavity from above or the side resolves to the sub-biome's stone instead, leaving powder deeper in the strata where digging into it still pours. Floor sediment is exempt — it is already at rest — but must stay shallow relative to the smallest corridor, or it silts them shut.
- **Every capsule is truncated below its centre**, leaving the lowest slice solid. That is the cheapest walkable floor there is, and loose matter settling into it makes the floor flatter as the sim runs.
- **Chambers** are superellipses, not ellipses: near exponent 3 a room has a broad flat floor and ceiling with rounded corners, which is the difference between a room and a bubble. Past about 4 it is a box, which reads worse than either. Pillars are re-solidified afterwards so they survive.
- **Galleries** give each level a ceiling and floor from two *separate* fields, the floor's amplitude deliberately much smaller. A rough roof reads as rock while a nearly flat floor is somewhere a body can run. Their centre line wanders over a long wavelength, and clearance fades to nothing as the presence field crosses its threshold — a gallery that stops at full height leaves a vertical wall no geology would cut.
- **Porosity** varies over a few screens and gates the noise-driven detail, but never the worms or chambers — that is what lets a region swing from near-solid massif to honeycomb without ever sealing off. Its floor is the sub-biome's `solidity`.
- **Worm and chamber density are per-sub-biome**, so "the karst is riddled and the granite is nearly solid" is content rather than a global field. Anchor probability additionally falls off cubically with elevation above sea level, or a tall mountain gets its entire interior threaded.
- **Cave mouths** are guaranteed on a 1D lattice along the surface, and a plunging shaft is guaranteed per region-sized area, so a route in and a route down exist by lattice arithmetic rather than by luck.

Carved cells resolve to the sub-biome's sediment near the floor (quantized so walking never stutters), to its fluid below its fluid level, and to its gas in pockets — heavy damps find floors and light ones find ceilings under their own density, so generation only seeds them.

## Terrain

Height is a spline on `land` — deep basin through a broad shore shelf to upland — plus ridged relief whose amplitude is governed by `flat`, plus fine detail, plus sparse massifs for a skyline worth navigating by. Terracing switches on across a mid band of `flat`. A pure height function can only produce a graph, so a near-surface 2D solidity carve adds overhangs, notches and shelters.

Terracing must be a **continuous** shaping of height, never a snap to the nearest step. Rounding to a multiple and blending toward it flips discontinuously wherever the raw height crosses a midpoint, which puts a sheer vertical wall of a fraction of the step height at that column — a ruler-straight cliff in otherwise smooth ground. Shaping the fractional part through a smooth sigmoid gives the same plateaus and risers with no jump anywhere.

The soil stack is measured from the surface and its thickness is scaled by a slow field, so the cover/soil/subsoil interfaces undulate instead of ruling three lines under the terrain. Where the stack is thicker than its own lattice cell it is simply cut off by the buried biome below, which is coherent by construction: the shallow rock under a desert is dry and hot for the same reason the desert is.

Within a sub-biome, `bedding` is a strongly y-compressed field selecting its companion rock. Horizontal and thin, it reads as strata; isotropic and broad, it reads as camouflage.

## Flora

Nothing grows from the height function. `height(x)` is where rock *starts*, not where its top cell is, and the overhang carve moves the real interface by tens of cells — so planting at `height(x)` leaves everything floating a few cells up, which is what the whole surface looked like. Every plant, boulder and ground tuft is seated on the **topmost solid cell found by probing**, and a probe that finds nothing within a bounded window plants nothing: that alone stops flora hanging over cave mouths and sinkholes.

Water is a placement input, not an afterthought. A skin declares whether its ground cover belongs *below* sea level, so kelp only grows submerged and grass only grows dry, and a `wade` depth lets mangroves root in shallow water while nothing else does.

Canopies are shapes, not palette swaps. A broad crown is a stippled ellipse centred on the **leaning** trunk top — centring it on the trunk's base leaves the crown visibly beside the tree on any lean. A conifer is a tiered cone whose width pulses so it reads as layered branches. A mushroom is a bridged dome with glowing gills hanging from its underside, which is a different silhouette rather than the same tree drawn in fungus.

## Standing up

`body::island` walks **cardinal** neighbours only, links cells that share a `bond_group`, and calls a cell supported when a cardinal neighbour is a solid harder than `SUPPORT_MIN_HARDNESS`. Three consequences bind every generator that places bonded matter — wood, timber, rope, bone, ice, salt, shale, flesh, metal, brick, glass, shroom:

- **Diagonal chains are not chains.** A line drawn by stepping x and y together is a string of unlinked single cells, each its own island, each a falling body. Any sloped run must emit the whole vertical span at each new column; that shares a row with the previous column and the cardinal link exists by construction. A tree with a dozen diagonally-drawn branches becomes a dozen bodies per tree, which is hundreds of bodies per screen and tens of milliseconds of physics.
- **Soft matter does not hold anything up.** Leaves, ground cover and rope are below the support threshold, so a plank resting on a rope falls and a branch surrounded only by foliage falls. Ladders are built entirely from timber for this reason; the rope beside them hangs from rock or from a beam, never the reverse.
- **Non-bonded solids are free.** Foliage, powders and liquids never form islands, so they can be scattered without connectivity care. That is why crowns can be a sparse stipple while trunks cannot.

The cheap check when authoring a structure: trace the path from every cell to rock. If it does not exist through cardinal, same-group steps, the piece is debris.

## Mineshafts

Mines are the counterweight to the caves: straight, level, regular, unmistakably cut. They are what the natural network lacks — a spine you can follow, orient by, and return along. A complex is one to three stacked drifts on a lattice, connected by risers, with an **adit** climbing to daylight whenever the surface is close enough, so the whole system is findable from above rather than only by luck.

Their geometry is fully analytic: nothing scans the carve, so every overlapping region re-derives the identical mine. Cut before the material pass, furnished after it, which is what lets timber sit inside a corridor that the strata pass has already filled.

- **A corridor must stay walkable after every later pass.** Sediment settles on cave floors, gas collects, fluid fills below the water table — so mines re-clear their own walking envelope and leave the upper corridor alone, keeping firedamp under the roof where it belongs.
- **Timbering is read from the side.** A post from floor to ceiling would block a 2D corridor, so sets are a ceiling course with short hanging stubs and sleepers on the floor: unmistakable at a glance, never in the way.
- **Collapses are powder.** A rubble-filled span reads as failure and digs out, which is the difference between a barrier and a wall.
- **Spurs end in a dig face** — the vein someone was chasing, exposed. Following a side passage should pay.

Mines only occur in diggable lithologies and only in the band where someone plausibly worked; below that the world is untouched, and that contrast is the point.

## Remnants

Fourteen small marks — a firepit, a spoil cone, a snapped rope, tally scratches, bones, a dropped pack, a cache, the last torch — on a dense lattice, each placed against a real surface found by probing the carve. They cost almost nothing and they are most of what makes a cave feel visited.

Probing the carve makes them **region-dependent**, so the reach rule is strict: a feature's own extent plus its probe distance must fit inside `MARGIN`. Then every region that writes any of its cells has the whole probe window in its buffer and agrees; regions that merely see the anchor compute it and clip everything away.

**Density is measured against caves, not against area.** Anything that seats on a floor or a ceiling can only land in the few percent of underground volume that is open, so an anchor lattice tuned by area throws almost every anchor away and the world reads as empty. The probe window must be a large fraction of the lattice pitch, and the marker histogram — how many sampled regions contain any artificial material at all — is the number to tune against, not the cell count.

## Set pieces

Nine authored hazards, each a sealed volume behind a thin skin with a **tell** on the outside: a water lens over a clay membrane with flowstone staining and drips beneath, a sand lens with grains already on the floor, firedamp behind a sulfur rind, lava behind an obsidian crust, tar, an acid sump inside a gypsum rim, an ice plug, a sand hopper stoppered with planks, a powder keg with a torch left burning nearby.

They are material writes over solid rock, never carve edits, so they compose with everything upstream. Two rules keep them honest:

- **Choose among what fits, not fit what was chosen.** Rolling a kind and then testing its lithology and depth discards most anchors and makes every hazard rare everywhere. Collect the candidates valid at the point, then pick — each country then reliably carries its own dangers.
- **The tell is the feature.** A hazard with no readable surface is a random death; the rind, the stain, the grains on the floor are what turn it into knowledge.

## Ores

Every deposit declares the **lithologies** it occurs in, and every sub-biome declares one, so limestone country and granite country carry genuinely different ores. Abundance is a smooth falloff around a peak depth, so ores fade in and out rather than switching on at a line. Deposits carry a **tell**: a rind of a second material on the deposit's shell — verdigris beside copper, rust around iron, quartz around gold. Reading walls is the skill. Ore is written after the strata pass and only over the sub-biome's own rock, so it can never hang in a cave or float in soil.

## Glossary

| Term | Meaning |
|------|---------|
| Biome | the coarse 2D region and the name a player uses; declares windows, owns no material |
| Sub-biome | a patch inside one biome, owning stone, fluid, gas, cave character, ore and skin |
| Daylight | whether a biome reaches air; daylight biomes are chosen per column, buried ones on the lattice |
| Skin | the soil stack, trees, ground cover and boulders a daylight sub-biome puts on its column |
| Pocket | the field that displaces an anchor own depth so biomes interpenetrate instead of stacking |
| Build | one deferred material write, applied after strata, ore and flora |
| Site | the probe over a finished carve that finds floors, ceilings and walls |
| Adit | the shaft that climbs from a mine's top drift to daylight |
| Tell | a visible crust on a deposit's or set piece's shell that betrays what is behind it |
| Parameter | one of the eight dimensionless axes a biome declares windows in |
| Anchor | a jittered lattice site in (x, above-surface) space carrying one sub-biome |
| Carve | the per-region open/solid buffer the cave pipeline composites into |
| Worm | a parametric polyline rasterised as a chain of truncated capsules |
| Gallery | a horizontal cave level with independent ceiling and floor fields |
| Porosity | the local cave-density field; gates noise detail, never worms |
| Bedding | the y-compressed field that streaks a sub-biome companion rock through its stone |
| Reach | a feature's bounded maximum extent from its anchor |
