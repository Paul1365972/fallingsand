# Server

The server is authoritative for every gameplay rule; clients send raw input and render replicated state. Single player embeds the same server and speaks the real protocol. The server is a library value you tick — no ECS: a small fixed set of domain collections and an explicit tick pipeline keep ownership and order visible.

## Invariants

- **Server authority** — gameplay rules live here, including single player through the embedded server.
- **Exclusive lifecycle** — exactly one life state per player (entering, alive, dead, reviving); only alive owns an avatar, and the avatar owns every physical and deferred-physical value. Input or queued work from an old connection never leaks into a new incarnation.
- **Persistence is faithful** — pending state survives failed writes; a region is generated only after a confirmed missing read; a read or decode error is fatal, never papered over. No migrations: a format-version mismatch is rejected.
- **Suspend/resume** — loaded chunks wake fully for one tick.

## Players

A session is one transport connection plus handshake state and replication baselines. A player is one authenticated person currently present: durable identity derives from their key; the runtime id is stable through connection takeover, death, and revive, and retired on completed departure. Profile state (game mode, inventory, history) survives avatars; inbox state (accepted input, queued intents) resets on every incarnation boundary. Takeover rebinds player→session before closing the old session, so cleanup of the superseded connection cannot remove the player; a true departure snapshots the player and despawns its body before gameplay advances.

## Tick order

1. Drain network: authenticate or take over, adopt latest held input, neutralize stale input, complete departures
2. Dispatch queued commands through the registry, dig/place, inventory actions; begin requested revives
3. Recompute interest tickets; integrate completed region requests; request and unload regions
4. Step the CA in four phases
5. Integrate body forces, run every controller in deterministic order, then advance the bodies
6. Apply hazards, resolve lethal transitions, advance materialization searches
7. Reveal fog around every alive player
8. Advance the calendar and emit one frame per active session
9. Enqueue the ten-second world snapshot when due

Budget ~16 ms/tick, sim ≤8 ms; sleeping keeps the active-chunk set inside it.

## Interest

Each view projects onto chunks as active (simulate + replicate) or border (simulate only, so edges behave), loaded through their containing region; simulation extends one margin beyond replication. Random ticks run only on each player's active chunks. Zero-ticket regions unload after a grace period; frozen chunks retain their pending rects until re-entered.

Entering and revive share one deterministic ring search advancing over ticks, examining only loaded windows, becoming alive only once a whole avatar body spawns into free cells — terrain and other players are never overwritten. A saved avatar is an anchor cell plus velocity and vitals; its body is re-authored around that. Dead players keep camera interest at the death location while revive searches around spawn.

## Fog of war

Exploration is world state, not player state: one shared mask per chunk, one bit per 4×4 cells, revealed by anyone and never re-closed. Monotone accumulation is what makes it cheap everywhere — merge is OR, so replication needs no versioning and a lost delta costs only latency.

- **Sight is coarse and lazily rebuilt** — a fog texel blocks sight when most of its cells are fully opaque. Anything you can see through sees through: every liquid and gas is authored below full alpha, so water and glass carry sight instead of casting permanent shadow, and body-owned cells never block so a player is not their own wall. Cell writes only mark a dirty rect; the mask is recomputed when a reveal actually reads that chunk, so chunks nobody looks at cost nothing.
- **Reveal is line of sight, every tick, for every alive player** — symmetric shadowcasting over a flat local copy of the sight grid. Cost is bounded by memoization, not by cadence: an eye that has not crossed a texel boundary and sees no changed sight mask cannot produce a different result, and skips. The `fog` tick phase reports its own timing.
- **Daylight starts revealed** — generation pre-reveals the band from open sky down a shallow depth ([Worldgen.md](Worldgen.md)), so the surface reads as landscape rather than as a bubble around the player and the reveal radius only has to carry the near field.
- **Replication rides chunk interest** — a load carries the mask, a change sends it again; the session baseline is the mask it last sent. The server never lies about fog: hiding it is a client-side render toggle.

## Persistence

Every ten seconds, one transaction saves every loaded or pending region, every present player, and world metadata. Unload and departure only replace pending snapshots; persisted unloaded regions remain valid. Graceful shutdown takes one final snapshot, finishes any in-flight batch, then commits the final batch. A crash or forced termination recovers from the latest completed autosave. Without a save path, pending snapshots retain unloaded state in memory.

The worker owns reads, confirmed-missing generation, encoding, compression, and writes; ready regions integrate deterministically at one per tick. Success drops the immutable batch, failure restores entries without newer replacements, and read or decode errors are fatal. Region blobs omit player flesh and runtime flags. Validated DTOs isolate gameplay from storage. Interrupted revives persist as dead.

## Glossary

| Term | Meaning |
|------|---------|
| Session | one connection: handshake state and replication baselines |
| Player | one authenticated person present: identity, profile, inbox, one life state |
| SessionId / PlayerId / PlayerUuid | connection id / runtime presence id / durable key-derived identity |
| Avatar | owned only by the alive state; every physical and deferred-physical value |
| Ticket | a chunk's reason to be loaded, simulated, or replicated |
| Fog texel | 4×4 cells: the unit of explored state, 32 bytes per chunk |
| Sight mask | per-chunk coarse opacity driving line of sight; runtime only |
| tick / world_age | monotonic sim tick / calendar clock |
