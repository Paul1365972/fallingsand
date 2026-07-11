# Networking

One reliable ordered stream per connection; postcard-encoded serde enums, lz4 above a size threshold. `ServerMessage` = out-of-band control/social events (hello, join/leave, chat) plus one `TickFrame` per server tick carrying `tick`, `world_age`, and every per-tick delta. The frame *is* the tick boundary and the client's clock — an idle world still sends a tiny empty frame.

Each subsystem rides the frame with its own change signal (no generic differ):

- **chunks** — `ChunkOp` `Load`/`Delta`/`Unload` from the sim's change rect against a per-session `known_chunks` set; pixel bodies and player flesh have no wire presence and ride these deltas.
- **players** — change-gated `PlayerState` snapshots — anchor only, for nametag, flames, and camera.
- **inventory + self** — per-slot deltas and private `self_state`, each sent only when changed.

Client→server: one `InputFrame` per client fixed tick — held `InputState` (latest-wins, OR-merged when frames coalesce) plus ordered one-shot `InputAction`s (never coalesced, validated server-side); held input decays to neutral after 0.5 s without frames. A new discrete input is a new `InputAction` variant, never a new message.

A wire cell is 3 bytes (material + shade flags) — no velocity or timing; the server re-derives them. Chunk payloads are paletted containers (uniform / paletted / raw, smallest wins).

`HelloAck` carries `PROTOCOL_VERSION`; the client rejects on mismatch. Any change to `core::content` bumps it.

Reliable+ordered is load-bearing: deltas always apply on top of the last state — no per-chunk versioning, no resync, no sequence numbers; packet loss costs a retransmit delay, never correctness — fine for ~10-player co-op. **Latency**: no prediction, no reconciliation, no interpolation — cell-snapped at tick arrival; the camera's exponential smoothing carries the feel. The shared `step_player` is the insertion point if latency ever forces prediction.
