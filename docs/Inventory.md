# Inventory

Items are the resource; materials are one kind of item — a thin layer over the material sim, not a parallel economy.

## Invariants

- **Matter is never silently voided** — every diggable cell yields an item; a cell with no item refuses the dig; a full inventory refuses before matter is removed. Trash is the one sanctioned mass-deletion affordance; items never drop into the world.
- **The dig gate owns obtainability** — item generation encodes no survival rules; phase, hardness, tool, and inventory space decide what normal play yields.
- **Server-authoritative intents** — the client resolves keybinds to slot intents; no raw modifiers cross the wire; the server re-validates everything.

## Model

The compiled item table holds named items plus one auto-item per itemizable material — air and player flesh share the empty item; burning variants do get items. Recipes are shapeless, count-based, and compiled to concrete ids. Player inventory is 30 slots with a 10-slot hotbar; the server holds the cursor stack. Inventory rides the tick frame: full state once, then per-slot diffs.

**Dig / place** — each use event addresses one server-selected cell bounded to authoritative reach, so raw aim never sets the work volume. Smart mode sweeps from the avatar toward the cursor and acts on the last open cell before obstruction; precise mode targets the cursor cell directly, including through walls — reach is the only spatial gate. Placement and creative digging execute once per event; survival digging accrues time-gated progress while held (a lone tap counts as one tick of work), binds progress to target, material, and mining method, and replicates target, validity, and progress. Survival digs only solids and powders; creative destroys any non-player matter, liquids and gases included, as a sanctioned creator sink. Burning fuel yields its base material. Mining tiers derive from material hardness at compile time; bare hands are tier 0 and slow, pickaxes raise tier and speed. Creative dig and place are explicit creator source/sink causes: they bypass inventory and tiers but keep public unflagged writes, ownership checks, body damage, and the authoritative target.

**Slot actions** — click, quick-move, trash, craft once or all (a craft whose output wouldn't fit is refused; inputs consume against a trial copy, so crafts that free their own space work), creative grab. The trash slot is invisible to insertion, counting, and crafting; trashed contents are recoverable until replaced.

**UI** — `E` toggles the overlay (grid, hotbar, trash, crafting or creative palette); drag & drop rides the authoritative cursor with no prediction; the hotbar is always visible.

Progression: wood → planks → sticks → wooden pickaxe → shallow coal/iron → ingots → stone and iron pickaxes. A furnace is deferred until fuel, timed processing, UI, persistence, and suspend/resume can ship as one feature.
