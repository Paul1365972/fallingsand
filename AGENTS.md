# Repository Guidelines

[docs/Overview.md](docs/Overview.md) indexes the design docs.
Each system doc states its goal, its invariants, and its vocabulary.
Read the ones the task touches; the invariants are non-negotiable design.
Docs express intent, not exact specification: when relevant code and docs disagree, establish the intended behavior and update both within task scope.

## Working Rules

- Great, not good: Human playtesting decides feel.
- Fix root causes: architectural fixes over symptom patches; rework any system, architecture, or protocol freely. No backward compatibility or migrations — bump affected version constants.
- Build features as coherent units, without duplicate paths, compatibility shims, or half-migrated abstractions.
- Write self-explanatory code with no comments; a comment means the code is not readable enough.
- Keep docs terse and standalone: goals, invariants, and vocabulary, sparse code references, no implementation-process narration.
- Only add tests when requested.

## Verification

Run `cargo fmt --all` and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

Manual gameplay verification belongs to the user.
Do not build or launch the game unless explicitly requested.

Commit once at the end of a task (separate packets only when clearly separable): conventional subject, no body, no co-author; don't push; leave the user's parallel WIP untouched.
