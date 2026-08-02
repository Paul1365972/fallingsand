# Crates & Dependencies

```
fallingsand_math      # Tick/subcell constants and tick-seeded stateless randomness
fallingsand_material  # Runtime material schema
fallingsand_content   # Host-only typed definitions, validation, quantization, codegen
fallingsand_core      # Coords, cells/chunks/regions, generated content module
fallingsand_sim       # CA kernel, dirty rects, sleeping, physics
fallingsand_protocol  # Client↔server messages
fallingsand_net       # Transport trait: WebTransport (native + wasm), in-memory
fallingsand_worldgen  # Procedural generation
fallingsand_server    # Authoritative server library
fallingsand_dedicated # Headless binary: CLI, ACME certs, tracing
fallingsand_client    # Plain-Rust game core + bevy IO shell (game/ vs view/); native + WASM
```

Direction: `{math, material} ← content ← core(build)`, `{math, material} ← core`, `math ← {sim, worldgen, server}`, `core ← {sim, worldgen, protocol, server, client}`, `sim ← server`, `protocol ← {server, client}`, `server ← {dedicated, client}`; the client reaches the sim only through the embedded server.

- Content compiles in during the core build — see [Content.md](Content.md).
- The client stays WASM-clean: the browser build is join-only; rayon, storage, and the embedded server compile out. CI builds for `wasm32-unknown-unknown`.
- Only the client depends on Bevy; only the server depends on redb; only the dedicated binary depends on the HTTP and certificate stack.
- One transport trait spans WebTransport and the in-memory pipe, so single player runs the real protocol, not a shortcut.

## Profiling

Timings are only meaningful in an optimized build. `[profile.dev]` puts workspace crates at `opt-level = 1` and leaves `debug_assert!` live in the innermost cell accessors, so `cargo dev` numbers are not a measurement of anything. Use `cargo profile` (release + symbols + tracy) or `--profile perf`; `cargo profile-server` does the same for the dedicated binary.

The debug overlay is the first instrument; tracy is the second. What the overlay states:

- `frame` with its min/max spread separates a steady cost from a hitch. The embedded server owns its own thread and the kernel fans out over rayon, so a client hitch can be sim contention rather than render cost.
- `draw` ranks GPU passes, the game's own (`raster`, `light_field`, `fog_field`, `composite`) alongside Bevy's post-process chain. `composite` and everything after it run at window resolution, not at native cell resolution.
- `cpu net` / `cpu atlas` are the client's two per-frame costs that scale with world churn: wire decode plus `WorldView::apply`, and dirty-rect packing into the chunk atlas.
- `tick`/`sim` carry a peak alongside the average, and `peak` names the worst phases over the same window. Averages hide work that runs on a few ticks out of hundreds — region integration, autosave — which is exactly the work a frame spike is made of.
- `chunks` counts what is loaded, simulated (active plus border), and awake; `cells ~N active` sums *sim rect areas*, not moving cells. Approaching `awake chunks × 4096` means the rects have degenerated to whole chunks.
- `delta … written … visible … sent` separates the three quantities a dirty rect conflates per tick: cell writes, writes that changed the wire representation (material and shade), and cells actually replicated. `sent ≫ written` is rect over-approximation; `written ≫ visible` is invisible state churning the replication rect.

## Verifying body rules

`fallingsand_server --example burning_tree` is the standing physics harness: it drives the real
kernel and the real body phase over generated terrain and audits the result.

- `body::journal` is an opt-in per-tick record of every body interaction — detach, split, release,
  strike, load, quantum outcome, contact, entrainment, carriage, settle and every refused settle
  with its verdict and residual. Off by default and branch-cheap; the sim never reads it back.
- `Bodies::states` is the matching snapshot: mass, motion, accumulators, bounds, policy.
- Scenes: `tree` burns a generated tree and audits the debris, `stack` drops small debris on a
  floor, `gas` walks a body through smoke, `survey` counts what worldgen leaves falling on load.
  `--events`, `--trace <id>` and `--frames <n>` turn the journal into a readable trace.
- Judge by the totals, not by eye: settle-refusal verdicts, quantum outcomes, peak speed per body
  and settled-versus-detached cells localize a regression faster than any single frame.

## Verifying cell rules

Verify behavior with a temporary example (deleted before commit) that drives the real kernel:

- Build a `CellWorld`, insert fresh chunks one chunk beyond the scenario on every side (a chunk simulates only with its full 3×3 loaded), place and remove cells with `set_material`, and step with `step_scoped(&mut world, &|_| true, &|_| true)` — keep the random-tick closure on, it is part of behavior.
- Measure, don't eyeball: print regions top-down (Y is up), count cells per material for conservation, track per-column tops for leveling, and check `awake_counts()` to prove settling actually sleeps.
- For realistic coverage, place the example in `fallingsand_server` and insert `WorldGenerator::generate_region` output — multiple bodies on real terrain expose scheduling and wake bugs that single-basin tubs cannot.
