# Overview

Multiplayer Noita-like falling-sand game. Inspirations: Noita (world sim, destruction), Don't Starve Together (drop-in co-op survival), Minecraft (chunked infinite worlds, server/client).

## Pillars

1. **The world is the simulation** — every pixel is a material that can burn, melt, flow, fall. Destruction is a mechanic, not an effect.
2. **Multiplayer-native** — always server/client; single player is an embedded local server.
3. **Performance is a feature** — sleep what can sleep, parallelize what can't.
4. **Architecture before content** — foundations the final game grows on without rewrites.

## Gameplay

Co-op survival for ~10 players in one persistent, infinite world: gather, build, defend, expand — but every mechanic routes through the material sim. You dig through actual sand and rock; bases flood if you breach an aquifer; fire spreads through what's flammable.

- Infinite in all directions, content around a surface band at y≈0. Depth trades better materials for worse hazards. No bedrock — depth is gated by hardness and hazard, never fiat.
- Digging is bare-handed, gated by per-material hardness. Dug material becomes stackable **material items** (very high stack caps, so digging stays forgiving); items are the resource. Items craft and drop into the world as physical entities — but every placeable material item still routes through the material sim when placed. Items are a thin layer over materials, not a parallel economy.
- Hazards are material contact (burn, drown, crush). Per-player game modes via chat commands (`/gamemode`).

## Docs

- [Tech.md](Tech.md) — crates and dependency rules
- [WorldModel.md](WorldModel.md) — cells, chunks, regions, materials
- [Simulation.md](Simulation.md) — CA kernel, scheduling, movement, sleeping
- [Physics.md](Physics.md) — character controller, pixel bodies
- [Server.md](Server.md) — tick loop, interest, persistence
- [Networking.md](Networking.md) — protocol and latency
- [Client.md](Client.md) — rendering and UI
- [Inventory.md](Inventory.md) — items, slots, crafting, dropped items
- [Worldgen.md](Worldgen.md) — generation pipeline
- [References.md](References.md) — prior art
