# Overview

Multiplayer Noita-like falling-sand game. Inspirations: Noita (world sim, destruction), Don't Starve Together (drop-in co-op survival), Minecraft (chunked infinite worlds, server/client).

## Pillars

1. **The world is the simulation** — every pixel is a material that can burn, melt, flow, fall. Destruction is a mechanic, not an effect.
2. **Multiplayer-native** — always server/client; single player is an embedded local server.
3. **Performance is a feature** — sleep what can sleep, parallelize what can't.
4. **Architecture before content** — foundations the final game grows on without rewrites.

## Gameplay

Co-op survival for ~10 players in one persistent, infinite world; every mechanic routes through the material sim — you dig actual sand, a breached aquifer floods the base, fire spreads through what's flammable.

- Infinite in all directions, content around a surface band at y≈0. Depth trades better materials for worse hazards; no bedrock — depth is gated by hardness and hazard, never fiat.
- Bare-handed digging gated by per-material hardness. Dug material becomes stackable **material items** (very high stack caps); items craft in the inventory, and placeable items route back through the material sim. Items are a thin layer over materials, not a parallel economy.
- Hazards are material contact (burn, drown, crush). Per-player game modes via `/gamemode`.

## Docs

- [Tech.md](Tech.md) — crates and dependency rules
- [WorldModel.md](WorldModel.md) — cells, chunks, regions, materials
- [Simulation.md](Simulation.md) — CA kernel, scheduling, movement, sleeping
- [Physics.md](Physics.md) — character controller, pixel bodies
- [Server.md](Server.md) — tick loop, interest, persistence
- [Networking.md](Networking.md) — protocol and latency
- [Client.md](Client.md) — rendering and UI
- [Inventory.md](Inventory.md) — items, slots, crafting, trash
- [Worldgen.md](Worldgen.md) — generation pipeline
- [Deploy.md](Deploy.md) — dedicated-server networking, DNS/TLS automation
- [Glossary.md](Glossary.md) — canonical names for core types and units
- [References.md](References.md) — prior art
- [skysim.html](skysim.html) — browser sky/calendar simulator mirroring `core/celestial.rs`
