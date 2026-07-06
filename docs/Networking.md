# Networking

Everything flows over **one reliable ordered stream** per connection — handshake, chunk load/unload, dirty-rect deltas, entity state, input. Messages are postcard-encoded serde enums, lz4 above a size threshold. Pixel bodies have no wire presence; their cells ride the chunk deltas.

The single-stream choice is load-bearing: reliable+ordered means deltas always apply on top of the last state, so there's no per-chunk versioning, no resync, no input sequence numbers. Packet loss costs a retransmit delay, never correctness — fine for ~10-player co-op, and why the protocol stays simple. Moving hot state to datagrams stays a contained change behind the transport trait.

**Latency**: interpolate players and remote entities between the last two states — no prediction, no reconciliation. Pixel bodies are cell-snapped and uninterpolated. Client-side prediction is a deliberate non-feature — the shared `step_player` is the insertion point if latency ever forces it.
