# Overview

Multiplayer Noita-like falling-sand game. Inspirations: Noita (world sim, destruction), Don't Starve Together (drop-in co-op survival), Minecraft (chunked infinite worlds, server/client).

## Pillars

1. **The world is the simulation** — every pixel is a material that can burn, melt, flow, fall. Destruction is a mechanic, not an effect.
2. **Multiplayer-native** — always server/client; single player is an embedded local server.
3. **Performance is a feature** — sleep what can sleep, parallelize what can't.
4. **Architecture before content** — foundations the final game grows on without rewrites.

Cross-cutting principles: physics is phase-based and semi-realistic — natural behavior over artificial caps and clamps; **universality** — every matter-affecting system handles grid cells, rigid bodies, and entities, or explicitly flags the gap; the quality bar is great-not-good, with Noita and Celeste as benchmarks and human playtesting deciding feel.

## Gameplay

Co-op survival for ~10 players in one persistent, infinite world; every mechanic routes through the material sim — you dig actual sand, a breached aquifer floods the base, fire spreads through what's flammable. Infinite in all directions around a surface band; depth trades better materials for worse hazards. Digging is gated by per-material hardness; dug matter becomes stackable items that craft and place back through the sim. Hazards are material contact: burn, drown, crush. Per-player game modes.

## Docs

Each system doc states its goal, its invariants, and its vocabulary; the invariants are non-negotiable design.

- [Tech.md](Tech.md) — crates, dependency rules, profiling
- [Content.md](Content.md) — compiled content and units
- [Simulation.md](Simulation.md) — the grid, scheduling, movement, sleeping, combustion
- [Physics.md](Physics.md) — players and pixel bodies
- [Server.md](Server.md) — authority, tick, interest, persistence
- [Networking.md](Networking.md) — protocol
- [Client.md](Client.md) — rendering and UI
- [Inventory.md](Inventory.md) — items, dig/place, crafting
- [Worldgen.md](Worldgen.md) — generation
- [Deploy.md](Deploy.md) — dedicated-server operations
- [References.md](References.md) — prior art
- [skysim.html](skysim.html) — browser sky/calendar simulator
