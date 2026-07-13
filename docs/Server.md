# Server

`fallingsand_server::Server` is a library value you tick — the dedicated binary and the embedded single-player server are the same type. `bevy_ecs` (the grid is a resource), fixed 60 Hz; on overrun it slews rather than spirals.

## Tick order

1. Drain network: authenticate sessions, neutralize stale input, generation-tag queued actions, persist departures, and unstamp departing rasters
2. Apply alive-player commands, single-cell dig/place, inventory actions, and crafting
3. Recompute tickets, then load/generate/unload regions
4. Step the CA in four phases
5. Step alive players in `PlayerId` order, reconcile body damage, then step pixel bodies
6. Apply hazards and crush damage, then resolve every lethal transition and validated revive in the same tick
7. Advance the calendar and emit one `TickFrame` per session
8. Flush dirty regions, authenticated player records, and world metadata when due

Budget ~16 ms/tick, sim ≤8 ms; sleeping keeps ~2000 active chunks inside it.

## Interest

Each ticket source (player, spawn anchor, …) projects onto chunks as `Active` (sim + replicate), `Border` (sim only, so edges behave), or `Loaded` (in memory); simulation extends one margin beyond replication. Zero-ticket regions unload after a grace period; frozen chunks keep their rects until re-entered.

## Persistence

redb, server-side only: `regions` (z-order → lz4 blob), `players`, `meta`; written only when dirty, through transactions. No migrations — a format-version mismatch is rejected.

- Pixel bodies persist as their grid cells; unload settles them, load strips leftover flags — a crash degrades in-flight bodies to plain terrain. Stale flesh is voided on load; the player record keeps its `Fixed` pose.
- Each chunk saves a **resume rect** restored as a keep-alive on load, so in-flight processes continue after reload.
- Player records preserve lifecycle, pose, velocity, inventory, and the latest 100 submitted chat/command lines. A dead record reconnects dead; disconnect is not a revive.
- Spawn and revive stamp transactionally, never overwriting terrain, bodies, or player cells: `Alive` only on a complete commit, deferring while spawn chunks are unloaded, staying dead when no legal footprint exists. An alive player that cannot stamp drops to zero health and dies the same tick rather than replicating a rasterless body.
