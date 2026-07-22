# Networking

One reliable ordered stream per connection; compact binary frames, compressed above a size threshold. The one frame per server tick is the tick boundary and the client's clock; an idle world still sends a tiny empty frame.

## Invariants

- **Reliable + ordered is load-bearing** — every delta applies on top of the last state; no per-chunk versioning, resync, or sequence numbers. Packet loss costs retransmit delay, never correctness — acceptable for ~10-player co-op. No prediction, reconciliation, or interpolation; state is cell-snapped at tick arrival and camera smoothing carries the feel.
- **Raw input only** — the client sends a latest-wins held snapshot plus ordered one-shot actions; edge-sensitive intent always rides the action channel. A new discrete input is a new action variant, never a new message.
- **Identity is a keypair** — each connection signs a server challenge; durable identity derives from the public key, never client-asserted. The private key stays client-side.
- **Version-gated** — any wire change or compiled-content change bumps the protocol version; mismatch rejects at handshake.

## Server → client

Roster and physical presence are separate: roster messages maintain connected names, while the tick frame carries change-gated per-subsystem state — chunk load/delta/unload from the sim's change rect (bodies and player flesh ride these deltas), optional avatar anchors for presentation, per-slot inventory diffs, a private self state whose lifecycle carries health/air/interaction exactly while alive and a camera anchor while not, and dig-spray particle spawn events (the server decides when and where; the client only integrates and fades them).

A wire cell is 3 bytes — material and shade, no velocity or timing; the server re-derives them. Chunk payloads are paletted containers, smallest encoding wins.

The opt-in debug stream adds per-chunk sim/change rects and live body ownership offsets for diagnostic outlines; neither participates in gameplay state.

## Client → server

One input frame per client fixed tick: the held snapshot (coalescing frames replace it wholesale) plus ordered actions. Dig and place ride the action channel as use events paced client-side: one immediate event on press — a press+release inside one flush window still lands exactly one action — then repeat mode re-emits on an interval and emits every cell traversed between aim samples along a four-connected line, so dragged strokes are gapless and diagonal-free by construction. The server validates and executes each event in order; held state drives only survival dig progress and previews.

A client transition that cancels held control (menu opened, avatar incapacitated) emits one immediate neutral frame; otherwise held input decays to neutral after half a second without frames. Sessions, handshake lifetime, frame size, drain rate, and actions per frame are bounded; the client carries excess actions into later frames and the server rejects an over-limit frame rather than silently dropping actions. The authoritative session binding rejects input from a superseded connection; takeover and lifecycle transitions clear queued work.

The client resolves one identity at startup and holds it for the session: key by override, then storage, then generation; name by storage first, then override, then generation — a name edit persists with the same key, never minting a new identity, and an externally supplied key is never written back.

## Glossary

| Term | Meaning |
|------|---------|
| TickFrame | the one frame per server tick: chunks, players, inventory, self, particles |
| ChunkOp | per-chunk wire delta: load / delta / unload |
| InputFrame | per-client-tick input: held snapshot + ordered one-shot actions |
| InputState / InputAction | latest-wins held controls / edge-sensitive intent, including use events |
| SelfLife | private lifecycle on the wire: entering, alive, dead, reviving |
| PlayerState | public per-player state: id plus an optional live-avatar anchor |
