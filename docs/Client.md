# Client

Display/input/UI only — never drives game logic. Bevy is the IO engine; game state is plain Rust with no bevy machinery in it.

## Architecture

- **`game/` — the core.** One plain `ClientGame` struct owns everything, with a `Flow` enum (`Menu | InGame`); `InGame` owns all session state, and dropping it (leaving to menu) is the entire cleanup path. Session loss keeps `InGame` alive: replicated state clears and the supervisor redials.
- **One update entry point.** `ClientGame::update(&mut self, io: &mut IoFrame)` runs fixed stages: UI events → input → net (demultiplex each message once) → flush one `InputFrame` per tick (edges latched so they survive aliasing) → timers. No bevy messages, states, change detection, or `FixedUpdate` in client logic.
- **`view/` — the bevy shell.** `Game(ClientGame)` is a single resource; one driver system builds the `IoFrame`, calls `update`, executes returned `Effect`s; every other system reconciles bevy entities/assets/UI from core state.
- **Update triggering.** Cheap values recompute every frame behind if-different guards; expensive updates gate on `Changes`, a frame-scoped signal struct. Spawns bake correct initial values — no first-frame patch.
- **Entity policy.** The core owns zero `Entity` ids and zero `Handle`s; view modules must render any core state from scratch — this is what makes reconnect and quit-to-menu clean.

## Rendering

- **Chunks**: one GPU texture per chunk, dirty sub-rect uploads; material + shade resolve to color via a palette texture in the shader. Players have no sprite — their flesh cells render as world cells (the shade pattern is the character art); nametags, flames, and camera derive from the replicated pose, cell-snapped, the camera's exponential smoothing keeping whole-cell steps readable.
- **Pixel-perfect layers**: every layer (world, sky, parallax) renders at native cell resolution (1 texel = 1 cell) into its own small HDR target, then upscales ×k onto a physical-pixel composite offset by its snap remainder — smooth camera, crisp k×k cells. Data-driven from one `LAYERS` table. Night darkness + emissive lights multiply at native texel centers (alpha-preserving); targets hold premultiplied color and composite materials must match. Bloom/tonemapping live on the composite.
- **Camera**: k = round(window_px/424) screen pixels per cell (~424×242 cells visible); `Ctrl`+scroll steps k by whole pixels. Interest budgets are sized for the default zoom, so zooming out never grows server obligations.
- **Parallax**: client-only procedural layers keyed by ratio (0 = gameplay, 1 = sky): two mountain silhouettes — opaque black, made visible purely by airlight, so aerial perspective tracks day/dusk/moon brightness — and a dim cave wall behind dug-out caves.
- **Sky**: `Calendar::celestial()` in core is the single deterministic source — a native-2D "great wheel" cosmology where the rendered plane *is* the physics: on-screen overlap equals eclipse math by construction. An elliptical sun track behind the flat world gives night, day length (8.1–15.9 h), and seasons over the 60-day year; the moon laps the sun and drifts between rails, so solar eclipses (total/annular/partial as covered-area fraction) fall out of geometry; a lunar eclipse is the moon caught behind the invisible **Shade**, turning blood-red. `CelestialState` is f32 and client-visual only — server gameplay uses integer calendar math. The starfield is its own layer anchored in world-cell space, turning once per sidereal day from the replicated calendar. Explore with [skysim.html](skysim.html).

## UI & input

- **Debug overlay** (F3): world/player context left, performance/system right; F3+G rect visualizer, F3+N game mode.
- **Inventory UI**: see [Inventory.md](Inventory.md) — `E` overlay, server-authoritative drag & drop, always-visible hotbar; world input is suppressed while an overlay is open.
- **Input**: `game/input/` owns the whole layer — `IoFrame` carries a `RawInput` (a `Button` vocabulary over bevy's plain `KeyCode`/`MouseButton` data enums, edges + held set, scroll as pulse buttons) and the shell's only input job is snapshotting bevy state into it. A declarative per-context binding table maps triggers (press, chord like F3+G, tap-release, double-tap, hold) to semantic `Action`s — gestures are binding kinds available to any key, and rebinding later means swapping table entries. Contexts form an explicit stack (Menu | Connecting | Gameplay base + Inventory/Chat/Paused overlays on `InGame`); resolution walks the always-visible global layer then the stack top-down, stopping at opaque layers, chords beating plain presses on the same button. Held gameplay state samples from the same table and flushes as one `InputFrame` per fixed tick.
