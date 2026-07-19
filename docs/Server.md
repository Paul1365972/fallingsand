# Server

The server is authoritative for every gameplay rule; clients send raw input and render replicated state. Single player embeds the same server and speaks the real protocol. The server is a library value you tick — no ECS: a small fixed set of domain collections and an explicit tick pipeline keep ownership and order visible.

## Invariants

- **Server authority** — gameplay rules live here, including single player through the embedded server.
- **Exclusive lifecycle** — exactly one life state per player (entering, alive, dead, reviving); only alive owns an avatar, and the avatar owns every physical and deferred-physical value. Input or queued work from an old connection never leaks into a new incarnation.
- **Persistence is faithful** — pending state survives failed writes; a region is generated only after a confirmed missing read; a read or decode error is fatal, never papered over. No migrations: a format-version mismatch is rejected.
- **Suspend/resume** — loaded chunks wake fully for one tick; rigid bodies are intentionally reduced to terrain at their current raster.

## Players

A session is one transport connection plus handshake state and replication baselines. A player is one authenticated person currently present: durable identity derives from their key; the runtime id is stable through connection takeover, death, and revive, and retired on completed departure. Profile state (game mode, inventory, history) survives avatars; control state (accepted input, queued intents) resets on every incarnation boundary. Takeover rebinds player→session before closing the old session, so cleanup of the superseded connection cannot remove the player; a true departure snapshots the player, unstamps its raster, and wakes affected bodies before gameplay advances.

## Tick order

1. Drain network: authenticate or take over, adopt latest held input, neutralize stale input, complete departures
2. Apply commands, dig/place, inventory actions; begin requested revives
3. Recompute interest tickets; load/generate/unload regions
4. Step the CA in four phases
5. Step avatars in deterministic order, reconcile body damage, step pixel bodies
6. Apply hazards and crush, resolve lethal transitions, advance materialization searches
7. Advance the calendar and emit one frame per active session
8. Flush dirty regions, player snapshots, and world metadata when due

Budget ~16 ms/tick, sim ≤8 ms; sleeping keeps the active-chunk set inside it.

## Interest

Each view projects onto chunks as active (simulate + replicate) or border (simulate only, so edges behave), loaded through their containing region; simulation extends one margin beyond replication. Random ticks run only on each player's active chunks. Zero-ticket regions unload after a grace period; frozen chunks retain their pending rects until re-entered.

Entering and revive share one deterministic ring search advancing over ticks, examining only loaded windows, becoming alive only after one complete transactional stamp — terrain, bodies, and other players are never overwritten. Dead players keep camera interest at the death location while revive searches around spawn.

## Persistence

One store owns the disk tables (regions, players, meta) and the in-memory pending records between the live world and storage; a server without a save path uses the pending maps as its memory backing, so unload still preserves the world for the process lifetime. Region blobs contain only chunks: pixel-body flags are stripped into ordinary terrain cells, while player flesh is omitted. Unloading any crossed region settles the body at its current raster before extracting chunks. Storage records are DTOs validated and converted at the persistence boundary; malformed cells, rectangles, coordinates, values, or identifiers are fatal, while failed writes retain their pending records. Gameplay never depends on a database type. An interrupted player revive persists as dead and restarts from an explicit request.

## Glossary

| Term | Meaning |
|------|---------|
| Session | one connection: handshake state and replication baselines |
| Player | one authenticated person present: identity, profile, control, one life state |
| SessionId / PlayerId / PlayerUuid | connection id / runtime presence id / durable key-derived identity |
| Avatar | owned only by the alive state; every physical and deferred-physical value |
| Ticket | a chunk's reason to be loaded, simulated, or replicated |
| tick / world_age | monotonic sim tick / calendar clock |
