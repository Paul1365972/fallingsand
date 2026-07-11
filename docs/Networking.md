# Networking

One reliable ordered stream per connection; messages are postcard-encoded serde enums, lz4 above a size threshold. `ServerMessage` splits into out-of-band control/social events (`HelloAck`, `Reject`, `PlayerJoined`, `PlayerLeft`, `Chat`, `System`) and one `TickFrame` per server tick carrying `tick`, `world_age`, and every per-tick delta. The frame *is* the tick boundary and the client's clock — an idle world still sends a tiny empty frame each tick. The client demultiplexes the frame once.

Each subsystem rides the frame with its own change signal (no generic differ):

- **chunks** — `ChunkOp` `Load`/`Delta`/`Unload` from the sim's change rect against a per-session `known_chunks` set; pixel bodies have no wire presence and ride these deltas.
- **players** — change-gated `PlayerState` snapshots (integer cell pose, height, burning, facing); full roster on a session's first frame, despawn via `PlayerLeft`. The player's *body* has no wire presence — flesh cells ride chunk deltas like pixel bodies; the snapshot is only the anchor for nametag, burning flames, and camera.
- **inventory + self** — per-slot inventory delta (all slots + cursor + trash on first send) and private `self_state` (hp, air, mode), each sent only when changed; debug rects only while subscribed.

Client→server: one `InputFrame` per client fixed tick — held `InputState` (latest-wins, OR-merged when frames coalesce into one server tick) plus ordered one-shot `InputAction`s (never coalesced, validated server-side). Held input decays to neutral after 0.5 s without frames. A new discrete input is a new `InputAction` variant, never a new message.

A wire cell is 3 bytes (material + shade flags) — no velocity or timing; the server re-derives them. Each `ChunkOp` cell payload is a paletted container over that state (cell count from context): **uniform**, **paletted** (≤256 first-occurrence entries, `ceil(log2(n))`-bit LSB-first indices, no padding), or **raw** — the encoder picks the smallest. Frame-level lz4 catches cross-chunk palette repetition.

`HelloAck` carries `PROTOCOL_VERSION`; the client rejects on mismatch. The version gates content compatibility too — any change to `core::content` (materials, items, …) bumps it.

Reliable+ordered is load-bearing: deltas always apply on top of the last state — no per-chunk versioning, no resync, no input sequence numbers. Packet loss costs a retransmit delay, never correctness — fine for ~10-player co-op. Moving hot state to datagrams stays a contained change behind the transport trait.

**Latency**: no prediction, no reconciliation, and no interpolation — players are cell-snapped and applied at tick arrival like pixel bodies (their cells *are* the visual); the camera's exponential smoothing carries the feel. The shared `step_player` is the insertion point if latency ever forces prediction.
