# Client

Bevy app, display/input/UI only — it never drives game logic. States: `MainMenu → InGame`, with `Connecting → Playing` and a connection screen that clears once the first full tick batch applies; mid-game loss reuses it as a reconnecting overlay.

- **Rendering**: one GPU texture per chunk, re-uploaded only for dirty sub-rects, one quad each. Material id + shade resolve to color via a palette texture in a fragment shader — no per-pixel CPU loop.
- **Camera**: Noita-like virtual resolution ~424×242 cells with clamped zoom. Interest budgets are sized for the default zoom, so zooming out never grows the server's obligations.
- **Debug overlay** (tick time, chunk/ticket counts, dirty stats, bandwidth; boundary/delta-rect visualizer; material inspector).
