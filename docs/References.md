# References

Prior art, research links, and measurements worth keeping around.

## Falling-sand engines & write-ups

- https://www.youtube.com/@Zicore47
- https://github.com/cakeng/falling_sand_sim
- https://github.com/abnerpalmeira/MagicPixelEngine
- https://github.com/PieKing1215/FallingSandEngine
- https://github.com/PieKing1215/FallingSandSurvival/issues/4
- https://github.com/spicylobstergames/astratomic
- https://blog.macuyiko.com/post/2020/an-exploration-of-cellular-automata-and-graph-based-game-systems-part-4.html

## Noita measurements

From the GDC talk and community reverse engineering:

- 1 tile is one color.
- Virtual resolution: 424×242 (~4.5 screen pixels per tile).
- Simulation chunk size: 64×64.
- Region (biome) chunk size: 512×512 (8×8 sim chunks).
- World size: 35,840×73,728 (70×144 region chunks).
- 12 region chunks loaded at a time → 3,145,728 "active" tiles (768 sim chunks).
- Simulation runs in 4 checkerboard passes, ~196 chunks per pass.

## Fluid / particle simulation research

- APIC: 4–16 particles per cell simulated.
- Position Based Dynamics: https://matthias-research.github.io/pages/publications/posBasedDyn.pdf
  (see §3.5 — group particles and equalize their velocities to dampen local movement, with angular momentum handling).
