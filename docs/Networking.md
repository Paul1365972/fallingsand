# Networking

Everything flows over **one reliable ordered stream** per connection; messages are postcard-encoded
serde enums, lz4 above a size threshold. `ServerMessage` splits into out-of-band control/social events
(`HelloAck`, `Reject`, `PlayerJoined`, `PlayerLeft`, `Chat`, `System`) and **one `TickFrame` per server
tick** carrying `tick`, `world_age`, and every per-tick delta. The frame *is* the tick boundary — no
end-of-tick sentinel — and its arrival is the client's clock, so an idle world still sends a tiny empty
frame each tick.

The frame bundles a delta per subsystem, each with its own change signal (no generic differ):

- **chunks** — `ChunkOp` `Load`/`Delta`/`Unload` from the sim's dirty rect against a per-session
  `known_chunks` set; pixel bodies have no wire presence and ride these deltas.
- **players** — a change-gated `PlayerState` snapshot (pose, ducking, burning), despawned via
  `PlayerLeft`, with a full roster on a session's first frame.
- **items** — dropped items, the one explicitly-replicated non-player entity: interest-filtered
  `ItemDelta` (spawn/move/despawn) against a per-session `known_items` set, interpolated like players.
- **inventory + self** — a per-slot inventory delta (all slots on first send) and private `self_state`
  (hp, air, mode), each sent only when changed; debug rects only while subscribed.

The client demultiplexes the frame once; no system re-scans a message union.
A cell's wire state is 3 bytes (material + shade flags), dropping per-cell velocity and timing — the
client renders from streamed positions and the server re-derives them each tick. Each `ChunkOp` cell
payload is a Minecraft-style paletted container over that 3-byte state (cell count comes from context —
chunk area or delta rect): **uniform** (one entry, zero index bits), **paletted** (≤256 first-occurrence
entries + `ceil(log2(n))`-bit LSB-first indices, no minimum width, no word padding), or **raw** 3-byte
cells — the encoder picks whichever is smallest. Water costs ~5 bits/cell (materials × 16 random shade
nibbles), of which the shade noise is the irreducible 4; frame-level lz4 then catches cross-chunk
palette repetition. IDs: `PlayerId` (session),
`EntityId` (replicated non-player entities; today only dropped items), `PlayerUuid` (account).
`HelloAck` carries the server's protocol version; the client rejects the connection on a mismatch.
`PROTOCOL_VERSION` gates content compatibility too — any change to `materials.ron`/`items.ron` bumps it.

The single-stream choice is load-bearing: reliable+ordered means deltas always apply on top of the last
state, so there's no per-chunk versioning, no resync, no input sequence numbers. Packet loss costs a
retransmit delay, never correctness — fine for ~10-player co-op. Moving hot state to datagrams stays a
contained change behind the transport trait.

**Latency**: interpolate players and remote entities between the last two states — no prediction, no
reconciliation. Pixel bodies are cell-snapped and uninterpolated. Client-side prediction is a deliberate
non-feature — the shared `step_player` is the insertion point if latency ever forces it.
