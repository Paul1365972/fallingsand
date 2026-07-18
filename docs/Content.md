# Content

Content is code, compiled in: materials, reactions, items, and recipes are typed Rust definitions that execute only during the core build. The compiler validates everything, converts real-unit tuning into integer tick constants, and emits dense registries plus one monomorphized kernel spec per material. The runtime has no registry object, no name lookup, no interpretation.

## Invariants

- **Compiled content** — definitions run at build time only; both binaries consume the same generated tables. New content is a data edit, zero engine code.
- **Units** — every tunable is authored in real units (seconds, fractions, densities); the compiler quantizes to per-tick integers. No hand-authored per-tick constants.
- **Phase scoping** — tuning lives inside its phase block; a field a phase doesn't simulate is a compile error. Combustion fields are scoped the same way.
- **Synthesis over mirroring** — a flammable authors one block and receives its generated burning twin; nothing is hand-mirrored.

## Authoring

Definition functions fill one ordered build-time catalog in plain Rust with loops and helpers; typed phase builders expose only relevant tuning, and a definition may inherit an earlier material and override selected properties. A key macro provides navigable UPPER handles. Items and recipes are authored the same way: explicit items plus one auto-item per itemizable material, recipes shapeless and resolved to concrete ids at compile time.

The kernel is driven by phase + properties — a new powder is a data edit — but freely names specific materials where identity is clearest.

## Units

Rates are authored per second: rate `r` fires with `1−e^(−r·dt)` each tick (keeps `e^(−r·dt)`); accelerations integrate as `a·dt`; durations become tick counts — behavior is ~invariant to tick rate, and an outsized rate fires effectively every tick. Random-tick rates rescale by the sampling density so a seconds-rate means the same real time, clamped at certainty.

Movement tuning per phase: powders author drag, friction, topple (start, keep), and deflect; liquids author drag, friction, splash deflect, and an optional flow rate — the per-tick chance gating flow steps, viscosity in one number, ungated when unset; gases author drag, turbulence, and the same optional flow rate; rigid solids author a bond group. Top-level fields cover density, restitution, entity surface feel, hardness, and contact damage. Fuels author ignite, sealed fraction, burn rate, emit, palette, residue, burnout, and damage; hand flames author the burning side directly.

## Glossary

| Term | Meaning |
|------|---------|
| Catalog | the ordered build-time definition set the compiler consumes |
| MatSpec | generated zero-sized per-material spec the kernel monomorphizes over |
| Dynamics | per-material precomputed per-tick coefficients |
| Burning twin | synthesized burning counterpart of a flammable material |
| Bond group | authored rigid-connectivity class; a compiled symmetric matrix says which groups hold together |
