# Inventory

Item-centric inventory over the material sim. Items are the resource; materials are one kind of item — a thin layer, not a parallel economy.

- **Item model** (`core::item`): `ItemStack { item, count }`; registry = named items from `core::content` plus one auto-generated material item per material (very high stack cap, swatch icon, places its material). Named data items aren't placeable; tools are `stack_max = 1` stubs (no durability in v1). Recipes are shapeless, count-based.
- **Slots**: player = 36 (hotbar 0..9, main); the server holds the cursor-held stack.
- **Dig / place**: selected slot and brush size are server-side per-player fields set via input actions, clamped and validated. Survival dig yields the material's item; a cell whose yield doesn't fit is refused and stays undug — a full stack never voids material. Place stamps the slot's `place` material across the brush (survival decrements per cell).
- **Slot actions**: server-authoritative and intent-based — the client resolves keybinds to intents, no raw modifiers cross the wire: left/right click, quick-move, trash, craft (once or all; a craft whose output wouldn't fit is refused — inputs consume against a trial copy, so crafts that free their own space work), creative grab. The server re-validates everything. Inventory rides the `TickFrame`: full state on first frame, then per-slot diffs.
- **Trash**: the one sanctioned mass-deletion affordance (items never drop into the world). A single trash slot, invisible to insertion, counting, and crafting by construction: cursor non-empty → previous trash contents are destroyed and the cursor stack moves in; cursor empty → the trashed stack returns (recoverable until replaced). Destroying is a single deliberate gesture.
- **Client UI**: `E` toggles a full-screen overlay (grid + hotbar + trash; side panel = crafting / creative palette); drag & drop via the authoritative cursor, no prediction; the hotbar is always visible.
- **Persistence**: `PlayerRecord` stores per-slot `(item_name, count)` + cursor + trash. No migrations.
