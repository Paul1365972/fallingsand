# Server

`fallingsand_server::Server` is a library value you tick — the dedicated binary and the embedded single-player server are the same type. `bevy_ecs` (the grid is a resource), fixed 60 Hz; on overrun it slews rather than spirals.

## Tick order

1. Drain network — sessions, inputs, chat, commands; departing players unstamp
2. Apply inputs → world edits: commands, dig/place, slot actions + crafting
3. Load/generate/unload regions per ticket changes
4. Step CA (4 phases, rayon)
5. Step physics: players in `PlayerId` order (sweep, then stamp), then the serial bodies pass
6. Game logic: health, hazards, crush; advance the world clock
7. One `TickFrame` per session
8. Periodic persistence flush + autosave

Budget ~16 ms/tick, sim ≤8 ms; sleeping keeps ~2000 active chunks inside it.

## Interest

Each ticket source (player, spawn anchor, …) projects onto chunks as `Active` (sim + replicate), `Border` (sim only, so edges behave), or `Loaded` (in memory); simulation extends one margin beyond replication. Zero-ticket regions unload after a grace period; frozen chunks keep their rects until re-entered.

## Persistence

redb, server-side only: `regions` (z-order → lz4 blob), `players`, `meta`; written only when dirty, through transactions. No migrations — a format-version mismatch is rejected.

- Pixel bodies persist as their grid cells; unload settles them, load strips leftover flags — a crash degrades in-flight bodies to plain terrain. Stale flesh is voided on load; the player record keeps its `Fixed` pose.
- Each chunk saves a **resume rect** restored as a keep-alive on load, so in-flight processes continue after reload.
- Inventories are per-slot in the player record ([Inventory.md](Inventory.md)).
