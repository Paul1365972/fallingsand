# Server

`fallingsand_server::Server` is a library value you tick — the dedicated binary and the client's embedded single-player server are the same type. It runs on `bevy_ecs` (entities are ECS entities, the grid is a resource) at a fixed 60 Hz; on overrun it slews rather than spirals. Sim and replication rates are independent knobs.

## Tick order

1. Drain network (sessions, inputs, commands)
2. Apply inputs → world edits
3. Load/generate/unload regions per ticket changes
4. Step CA (4 phases, rayon) + deferred world edits
5. Step physics (entities, then dropped-item `step_items`, then the serial bodies pass: damage → island registration → dynamics + re-stamp → sleep)
6. Game logic (health, hazards, dig/place, slot actions + crafting)
7. Build and send one `Tick` frame per session (chunks, players, items, inventory, self-state, `tick`+`age`)
8. Periodic persistence flush + autosave

Budget ~16 ms/tick, sim ≤8 ms; sleeping is what keeps ~2000 active chunks inside it.

## Interest

Each ticket source (player, spawn anchor, …) projects onto chunks as `Active` (sim + replicate), `Border` (sim only, so edges behave), or `Loaded` (in memory). Sim and replication distance are separate knobs. Zero-ticket regions unload after a grace period; frozen chunks preserve their rects until re-entered.

## Persistence

redb, server-side only: `regions` (z-order → lz4 blob, versioned), `players`, `meta`. Written only when dirty, through transactions. Format carries a version byte; migrations are a function table.

- Pixel bodies persist as their grid cells (not separately); unload settles them (motion lost), load strips leftover flags — a crash degrades in-flight bodies to plain terrain.
- Each chunk saves a **resume rect** (union of change + keep-alive) restored as a keep-alive on load, so in-flight processes continue after reload at zero replication cost.
- Dropped items ride the owning region blob (`RegionExtras`); re-spawned on region load, gathered on unload/autosave. A per-region item signature gates re-saves so a region is rewritten exactly when its item set changed (clearing stale blobs on pickup/boundary drift, skipping idle piles). Player inventories are per-slot in the player record. See [Inventory.md](Inventory.md).
