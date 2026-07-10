# Server

`fallingsand_server::Server` is a library value you tick — the dedicated binary and the client's embedded single-player server are the same type. It runs on `bevy_ecs` (entities are ECS entities, the grid is a resource) at a fixed 60 Hz; on overrun it slews rather than spirals. Sim and replication rates are independent knobs.

## Tick order

1. Drain network — sessions, input frames (state merged, actions folded in order, stale input decays), chat, commands; departing players unstamp their raster
2. Apply inputs → world edits: run chat commands, dig/place, slot actions + crafting
3. Load/generate/unload regions per ticket changes
4. Step CA (4 phases, rayon), applying deferred world edits after the 4 phases
5. Step physics: players in `PlayerId` order (controller sweep, then raster stamp — so later players see earlier stamps), then the serial bodies pass (damage → island registration → dynamics + re-stamp → sleep)
6. Game logic: health, hazards, crush; advance the world clock
7. Build and send one `TickFrame` per session (chunks, players, inventory, self-state, `tick`+`world_age`)
8. Periodic persistence flush + autosave

Budget ~16 ms/tick, sim ≤8 ms; sleeping is what keeps ~2000 active chunks inside it.

## Interest

Each ticket source (player, spawn anchor, …) projects onto chunks as `Active` (sim + replicate), `Border` (sim only, so edges behave), or `Loaded` (in memory). Both distances derive from `INTEREST_RADIUS_X/Y`: replication covers the interest rect, and simulation extends one `BORDER_MARGIN` beyond it. Zero-ticket regions unload after a grace period; frozen chunks preserve their rects until re-entered.

## Persistence

redb, server-side only: `regions` (z-order → lz4 blob, versioned), `players`, `meta`. Written only when dirty, through transactions. Each blob carries a format version (`REGION_FORMAT_VERSION`, `WORLD_FORMAT_VERSION`); there are no migrations — a version mismatch is rejected (pre-release worlds are deleted and regenerated).

- Pixel bodies persist as their grid cells (not separately); unload settles them (motion lost), load strips leftover flags — a crash degrades in-flight bodies to plain terrain. Player flesh cells saved mid-play are voided on load (a crash leaves a hole that refills), and the player record keeps its `Fixed` pose.
- Each chunk saves a **resume rect** (union of change + keep-alive) restored as a keep-alive on load, so in-flight processes continue after reload at zero replication cost.
- Player inventories are per-slot in the player record (plus cursor and trash). See [Inventory.md](Inventory.md).
