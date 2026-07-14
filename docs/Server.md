# Server

`fallingsand_server::Server` is a library value you tick; the dedicated binary and the embedded single-player server are the same type. A plain `ServerState` owns the simulation, sessions, players, bodies, regions, registries, persistence, and replication. The server has no ECS: its small fixed set of domain collections and explicit tick pipeline make ownership and order visible.

## Player ownership

- A `Session` is one transport connection plus its handshake and replication baselines. `SessionId` never leaves the server.
- A `Player` is one authenticated person currently present in the server. `PlayerId` is stable through connection takeover, death, and revive, but a completed departure removes it; a later join receives a new ID. `PlayerUuid`, derived from the authenticated public key, is the durable storage key.
- `PlayerProfile` owns state that survives avatars: game mode, selected slot, inventory, and history.
- `PlayerControl` owns ephemeral accepted input and queued intents. Takeover, death, materialization, and departure reset it, so work from an old connection cannot leak into a new incarnation.
- `PlayerLife` is the exclusive lifecycle: `Entering`, `Alive(Avatar)`, `Dead`, or `Reviving`. Only `Alive` owns an `Avatar`; the avatar owns every physical and deferred physical value, including actor, controller, raster stamp, health, air, burning, dig progress, impulses, and crush response.

`Sessions` maintains the authoritative `PlayerId -> SessionId` relation. A successful takeover rebinds that relation before closing the old session, so cleanup of the superseded connection cannot remove the player. Network draining reports true departures; `ServerState` then snapshots the player, unstamps its raster, wakes affected bodies, and removes the runtime player before gameplay advances.

## Tick order

1. Drain network: authenticate or take over sessions, adopt each session's latest held input, neutralize stale input, and complete departures
2. Apply alive-player commands, dig/place, inventory actions, then begin requested revives
3. Recompute view and materialization-search tickets, then load/generate/unload regions
4. Step the CA in four phases
5. Step alive avatars in `PlayerId` order, reconcile body damage, then step pixel bodies
6. Apply hazards and crush damage, resolve lethal transitions, then advance entering/revive materialization searches
7. Advance the calendar and emit one `TickFrame` per active session
8. Flush dirty regions, snapshots of every active player, and world metadata when due

Budget ~16 ms/tick, sim <=8 ms; sleeping keeps ~2000 active chunks inside it.

## Interest and materialization

Each view projects onto chunks as `Active` (sim + replicate), `Border` (sim only, so edges behave), or loaded through its containing region; simulation extends one margin beyond replication. Random ticks (see Simulation.md) run only on each player's `Active` chunks, a bounded range that always sits inside the loaded `Active ∪ Border` set, so its 3×3 window-scheduler neighbourhood is always available; the spawn keep-alive and materialization search windows load and simulate but are never random-ticked. Zero-ticket regions unload after a grace period, and frozen chunks retain their pending rects until re-entered.

Entering and revive use the same deterministic Manhattan-ring search. Search work advances over ticks in batches of 64 candidates and only examines a loaded 64x64-cell window. Crossing that window moves its tickets and waits for loading before continuing. A candidate becomes `Alive` only after one complete transactional stamp; terrain, bodies, and other players are never overwritten. Dead players keep their camera interest at the death location while revive searches around the world spawn.

## Persistence

`Persistence` owns both redb and the in-memory pending records used between the live world and storage. The disk tables are `regions` (z-order -> lz4 blob), `players`, and `meta`, written through transactions. A server without a save path uses the same pending maps as its memory backing, so region unload still preserves the world for the process lifetime. No migrations: a format-version mismatch is rejected.

- Pixel bodies persist as grid cells; unload settles them, and load strips leftover flags. A crash degrades in-flight bodies to plain terrain. Stale flesh is voided on load.
- Each chunk saves a resume rect restored as a keep-alive on load, so in-flight processes continue after reload.
- `PlayerRecord` and `AvatarRecord` are storage-only DTOs converted at the persistence boundary to `RestoredPlayer`, `ResumeSnapshot`, and `AvatarSnapshot`; gameplay and physics never depend on a database record type. Alive records preserve pose, velocity, health/regen delay, air, burning, and flight. Runtime-only `Entering` persists its materialization template; runtime-only `Reviving` persists as dead, so an interrupted revive restarts from an explicit player request.
- Dirty region snapshots and every active player snapshot enter pending maps before a transaction. Failed writes retain them, and unloaded regions are never discarded on a write failure. A missing region or player is generated only after a successful `None` load. Any region read or decode error is a fatal world-load error: the server exits immediately without retrying, generating a replacement, or running its normal final save.
