# Client

Display, input, and UI only — the client never drives game logic. Bevy is the IO shell; game state is plain Rust with no engine machinery in it.

## Invariants

- **Render replicated state** — every gameplay decision is server-side; the client presents, paces input, and smooths the camera.
- **Core owns no engine handles** — view modules must render any core state from scratch, and dropping the in-game state is the entire cleanup path. This is what makes reconnect and quit-to-menu clean.
- **Perceptual continuity** — a visual field derived from world state accounts for every contributing cell and follows changes as they happen: no source cap, sampling stride, or wall-clock refresh that drops or delays visible change. A named transition ramp — the fog reveal fade — is presentation, not a dropped update: it starts on the frame the change arrives and never lags a later one.
- **Pixel-perfect** — every layer renders at native resolution (1 texel = 1 cell) and upscales by an integer factor offset by its snap remainder: smooth camera, crisp cells.

## Architecture

One plain game struct owns everything; a flow enum separates menu from in-game, and in-game owns all session state. One update entry point runs fixed stages per frame: UI events → input → net → input flush → timers. The shell holds that struct as a single resource: one driver system feeds it an IO frame and executes returned effects; every other system reconciles engine entities, assets, and UI from core state. Cheap values recompute every frame behind if-different guards; expensive updates gate on a frame-scoped change signal; spawns bake correct initial values. Session loss keeps the in-game state alive: replicated state clears and the supervisor redials.

## Rendering

- **Renderer** — one gameplay-render plugin owns staging and a renderer-owned command sequence that draws all gameplay before UI. Code, shader contracts, GPU resources, and command encoding are grouped by raster, light-field, and composite pass; gameplay never creates offscreen cameras or per-object materials.
- **Chunks** — one growable GPU atlas stores all cell data with dirty sub-rect uploads and resets with the session; material + shade resolve through shared palette textures. Two instanced draws render every live chunk into the world and emission targets. Players have no sprite: their flesh cells render as world cells; nametags and camera derive from the replicated pose, cell-snapped, with exponential camera smoothing keeping whole-cell steps readable.
- **Layers** — world and emission render at native cell resolution. Procedural sky and parallax evaluate directly on their native logical grids while building the physical HDR scene. Bloom and tonemapping are centralized in the renderer.
- **Lighting** — ambient is per-cell and shaped by air: a normalized-blurred air-coverage mask scaled by the sky's day/night level lights surfaces facing open space while sealed cells stay dark at any hour — buried ore is invisible until air reaches it. Emissive materials feed a glow field (every emissive cell contributes, tinted by its own color, fire self-flickering) plus every player and burning point light through a growable buffer. The emissive pass renders over the viewport expanded by the blur margin, collapses through a downsample chain into one quarter-resolution light field, and blurs once separably — energy-preserving for glow, normalized for air — so light and darkness from just-off-screen cells resolve instead of clamping at the frame edge. Every offscreen pass goes inactive outside gameplay.
- **Fog of war** — the replicated explored mask ([Server.md](Server.md)) reaches the GPU as a fog atlas sharing the chunk atlas's slot grid at one texel per four cells, holding a reveal value the client ramps per texel so newly explored ground melts open while re-entered ground is already solid. Fog multiplies air and emission *in the emission pass*, so an unvisited pocket contributes no coverage and no glow — its air never lights the rock in front of it. The same pass writes fog to a second attachment, blurred separably at cell resolution into a screen-space field. The composite picks the dark end of the lighting ramp from it — unexplored resolves to the cave tone for rock and backdrop alike, so no silhouette survives; explored but unlit resolves to a desaturated, cool, dimmed version of the live cell, legible without a light source. The dimming eases off as a material approaches black, because a fraction of nearly nothing is nothing — that keeps the darkest stone visible and, by construction, never lets a remembered cell out-shine its own lit colour. The atlas fetch is bilinear within the chunk's own slot, so the source is a continuous field rather than blocks before the blur ever runs, and world-anchored value noise breaks the remaining grid so the boundary reads organic and never crawls against scrolling terrain. One floor value forces full visibility through both passes — that is the whole of the F3+F debug toggle, and it never touches replicated state.
- **Particles** — dig spray is server-authoritative spawn events; the client integrates velocity, gravity, and fade in one dense visual buffer and clears it on leaving.
- **Cave backdrop** — the wall behind open space is procedural rock: folded strata from a vertically-warped anisotropic noise, quantised to steps that swing warm as they brighten so texture reads as hue and not only as value, with thin fissures where the field crosses a level. Depth is its parallax ratio alone — feature size is authored, never derived from it, because every layer shares one native cell grid. It carries the figure-ground contrast underground, where terrain itself is nearly black, so it is deliberately brighter than unlit terrain. Its day/night response fades out with depth: a cave mouth still darkens at dusk, a deep cavern does not care what time it is.
- **Parallax & sky** — client-only procedural silhouettes made visible purely by airlight, so aerial perspective tracks day, dusk, and moon; and one deterministic celestial model shared with the core calendar — shader-evaluated sun and moon discs on their logical pixel grids, seasonal orbit, on-screen overlap equals eclipse math by construction. Explore with [skysim.html](skysim.html).
- **Camera** — screen pixels per cell derive from window size (~424×242 cells visible); zoom steps by whole pixels within half to four times base. Interest budgets are sized for default zoom, so zooming out never grows server obligations.

Per-cell visual changes remain dirty sub-rectangle uploads. New repeated visuals use instance or storage buffers, and transient pixel particles never become entities. Fullscreen effects belong to the centralized renderer and reuse or deliberately extend its target set. UI and diagnostics stay separate from gameplay rendering; diagnostic geometry owns a final overlay pass rather than sharing world-raster resources, and diagnostics update truthfully every enabled frame.

## UI & input

- One context stack derives from flow, session readiness, life state, the active panel, and settings — priority-ordered, chords beat plain presses, world input suppressed under overlays; the shell only snapshots raw keys and pointer state. UI depths mirror the same order.
- The game menu opens above death and connection status; embedded single player pauses on it, remote play keeps running.
- Settings (fullscreen, vsync, render and cursor modes, UI scale) persist as JSON on native; the wasm build resets each session; invalid files fall back to defaults.
- On-screen text is ASCII — the default font is a subset; anything outside it renders as tofu. Chat and command completion — see [Chat.md](Chat.md).
- Debug overlay: world/player context, chunk activity, fps, render-pass, and server-tick timings — see [Tech.md](Tech.md). Inventory UI — see [Inventory.md](Inventory.md).

## Glossary

| Term | Meaning |
|------|---------|
| Composite | the physical-pixel target native layers upscale onto |
| Air mask | normalized blurred air coverage driving ambient light |
| Glow field | energy-summing blurred emissive contribution |
| Light field | quarter-resolution texture holding glow in color, air in alpha |
| Fog atlas | per-chunk reveal values on the chunk atlas's slot grid, one texel per 4×4 cells |
| Fog field | blurred screen-space explored mask; picks the dark end of the lighting ramp |
| Memory tier | explored but unlit: the live cell desaturated, cooled and dimmed |
| Anchor | coarse camera target replicated while no avatar exists |
