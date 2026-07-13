# World Generation

A deterministic pure function `(seed, RegionCoords) → Region`. Regions generate independently in any order — no "generate the whole world" step — so cross-border features use deterministic overlap generation, never sequential dependency.

Layout: a surface band at y≈0 (noise heightmap), infinite depth in progressive bands (biome/hazard/loot keyed on Y), infinite sky thinning above. Pipeline: base terrain (fBm + domain warping) → biome → material fill → ore veins → structures (ruins, mineshafts, islands) → vegetation (decorations, mushrooms, trees).

Biome and feature definitions are hardcoded Rust, not data files, and name materials directly through `fallingsand_core::content::material::*` (UPPER handles like `material::STONE`) — no palette indirection. `examples/preview.rs` renders to PNG so generation is iterated offline.

Coal occurs from the surface through the shallow crust; iron overlaps the early cave layer and becomes denser before deepstone. Gold and crystal remain depth rewards. This distribution supports the wood → first pickaxe → coal/iron → next tool loop near spawn without making rare deep resources surface loot.
