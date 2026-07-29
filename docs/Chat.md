# Chat & Commands

One global channel of typed entries, and a command registry that is the single source of execution, usage, and completion.

## Invariants

- **Speech and commands are separate channels** — the server never parses chat for intent; the client routes on the leading `/` and sends a distinct message. A chat line is always text.
- **One registry, one truth** — a command declares its name, aliases, summary, and typed parameters once; dispatch, usage, `/help`, and the client's completion table all derive from it.
- **Every entry is typed** — an entry carries kind and author; presentation decides color and format, the producer never formats.
- **No silent drops** — throttle, unknown command, and bad arguments each answer the caller with an error entry.

## Model

**Server** — the registry pairs a `CommandInfo` with a handler taking one context that packs the mutable world and the caller. Argument accessors fail into `Usage`, rendered from the entry's own parameters, so no handler authors a usage string; replies target the caller or broadcast. Commands run in tick step 2 for a living caller only, throttled apart from chat. What can be summoned is the species registry itself.

**Client** — a 100-entry ring plus a composer owning draft, recall, and completion. Recall and completion write the draft through the field's live editor, never by replacing it; typing flows back and invalidates completion. Both sides append to the recall ring through one shared function, so the optimistic and persisted copies agree.

**Completion** — replaces the token under the caret and cycles alternatives in place, never advancing it. A lone candidate resolves: it appends the separator and retires the session, so the next cycle lands on the next parameter. Advancing is always the typed separator, never the cycle key. Cycling an empty prompt opens a command.

**UI** — rows carry their own scrim and fade individually while closed. Open scrolls a window over the ring; closed shows recent lines and an unread count.

## Glossary

| Term | Meaning |
|------|---------|
| Entry | one chat line on the wire: kind, optional author, text |
| Kind | say / system / error / announce — decides presentation |
| Composer | client-side draft, recall, and completion state |
| Recall ring | per-player history of sent lines, persisted and resynced at handshake |
