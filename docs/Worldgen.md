# World Generation

## Invariants

- **A pure function** — generation maps (seed, region) to cells deterministically; regions generate independently in any order — there is no whole-world step.
- **Overlap, not sequence** — cross-border features use deterministic overlap generation, never sequential dependency.
- **No fiat gates** — depth is gated by hardness and hazard, never bedrock.

The benchmark is Terraria × Noita × modern Minecraft. Layout: a surface band at y≈0, infinite depth in progressive bands (biome, hazard, loot keyed on depth), infinite sky thinning above. Pipeline: base terrain (noise + domain warping) → biome → material fill → ore veins → structures → vegetation. Biomes and features are hardcoded typed Rust naming materials directly — no palette indirection or data files. A preview example renders regions to PNG for offline iteration.

Ore placement carries the early progression near spawn: coal from the surface down, iron by the early caves, gold and crystal as depth rewards.
