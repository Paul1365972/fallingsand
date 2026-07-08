# World Generation

A deterministic pure function `(seed, RegionCoords) → Region`. Regions generate independently in any order — no "generate the whole world" step — so cross-border features use deterministic overlap generation, never sequential dependency.

Layout: a surface band at y≈0 (noise heightmap), infinite depth in progressive bands (biome/hazard/loot keyed on Y), infinite sky thinning above. Pipeline: base terrain (fBm + domain warping) → biome → material fill → ore veins → structures (ruins, mineshafts, islands) → vegetation (decorations, mushrooms, trees).

Biome and feature definitions are hardcoded Rust, not data files. `examples/preview.rs` renders to PNG so generation is iterated offline.
