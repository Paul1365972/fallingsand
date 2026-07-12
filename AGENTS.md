# Repository Guidelines

## Design Context

`docs/Overview.md` indexes the design documents; read only the documents relevant to the task.
The docs express intended goals and invariants, not an exact specification.
When relevant code and docs disagree, establish the intended behavior and update both within task scope.

## Working Rules

- **Quality bar:** Great, not good; Noita and Celeste are the benchmarks, with Terraria x Noita x modern Minecraft for world generation. Human playtesting decides feel.
- **Semi-realism:** Physics is phase-based and semi-realistic. Natural behavior takes priority over artificial caps and clamps.
- **Fix root causes:** Use architectural fixes for recurring simulation or physics bugs rather than patching symptoms.
- **Boil-the-ocean mode:** Nothing is holy; rework any system, architecture, or protocol freely. No backward compatibility or migrations; bump affected version constants.
- Build features as coherent units, without duplicate paths, compatibility shims, or half-migrated abstractions.
- Prefer self-explanatory code. Reserve comments for indispensable rationale such as `// SAFETY:`. Keep project docs terse, standalone, and free of implementation-process narration.
- Only add tests when requested.

## Non-Negotiable Design

- **Server authority:** Clients send raw input and render replicated state. Gameplay rules live on the server, including single player through the embedded server and real protocol.
- **Conservation of mass:** Cells are created or destroyed only by a physical cause.
- **One cell, one owner:** Double occupancy by terrain, rigid bodies, or entities is an architecture bug, not a tuning problem.
- **Universality:** Every matter-affecting system handles grid cells, rigid bodies, and entities, or explicitly flags the gap.
- **Body raster integrity:** A body flag corresponds to exactly one live body or player raster. Public cell writes create only unflagged cells. Players are grid residents, not collision overlays.
- **Idle cost:** A settled world costs approximately nothing, with no per-tick work or permanently awake chunks.
- **Locality and speed of light:** Simulation work remains within a 64-cell window; longer-range behavior propagates locally over ticks.
- **Suspend/resume:** Sleep, unload, and reload preserve pending activity; in-flight processes do not freeze in time.
- **Determinism:** The same seed and inputs produce the same result on one machine. Simulation randomness is tick-seeded and stateless; avoid iteration-order-dependent collections in simulation paths.
- **Scheduling:** Four-phase 2x2-chunk-block scheduling produces disjoint 4x4-chunk `SimWindow`s. `sim` is exactly the area evaluated next tick and contains `change`; replication and persistence consume `change`.
- **Compiled content:** Material definitions under `fallingsand_core/content/` compile through `content!`. Kernels remain monomorphized per material and integer-only.
- **Units:** Author tuning in seconds, not per-tick constants. Quantize at compile time.

## Verification

Run `cargo fmt --all` and `cargo clippy --workspace --all-targets --locked -- -D warnings`.
