# Inventory

Item-centric inventory over the material sim. Items are the resource; materials are one kind of item.

## Item model (`core::item`)

- `ItemId(u16)` (0 = none). `ItemStack { item, count: u32 }` — 8-byte `Copy`.
- `ItemRegistry` from `fallingsand_data`'s named items (`item::*`) + one auto-generated material item per non-empty material (`"mat:<name>"`, `stack_max = 10_000`, swatch icon, `place = id`).
- `ItemDef`: category, `stack_max`, icon (`MaterialSwatch` or atlas index), optional `place` (material id). Named data items aren't placeable; tools are `stack_max = 1` stubs (no durability/use in v1).
- `RecipeRegistry` from `fallingsand_data`'s recipes — shapeless, count-based (`inputs → output`).

## Slots

`core::Inventory { slots: Vec<Option<ItemStack>> }`. Player = 36 slots (hotbar = 0..9, main = 9..36). Ops: `insert_first_fit` (fill matches then empties, returns overflow), `left_click` / `right_click` (cursor semantics), `remove_item`, `count_item`. The server holds the cursor-held stack.

## Dig / place (server `dig.rs`)

- Selected hotbar slot and brush size (0..=6) are server-side per-player fields set via `InputAction::SelectSlot`/`SetBrush`; the server clamps the brush to `MAX_BRUSH` and ignores out-of-hotbar slots.
- Survival dig → `item_for_material` into the inventory; a cell whose yield doesn't fit is refused and stays undug (no budget spent), so a full stack never voids material and never blocks digging other materials in the same brush.
- Place reads the selected slot's `place` material and stamps it across the brush (survival decrements per cell).

## Slot actions (`InputAction::Slot(SlotAction)` → `apply_slot_actions`)

Server-authoritative and intent-based — the client resolves keybinds to intents, no raw modifiers cross the wire: `LeftClick`/`RightClick` (cursor), `QuickMove` (hotbar↔main), `Trash`, `Craft { recipe, all }` (once, or until inputs run out; a craft whose output wouldn't fit is refused — inputs are consumed against a trial copy first, so crafts that free their own space work, and `all` stops at capacity), `CreativeGrab` (infinite stack onto cursor). The server re-validates every action. Inventory rides the `TickFrame`: all slots + cursor + trash on first frame, then per-slot diffs while dirty — no standalone inventory message.

## Trash

The one sanctioned mass-deletion affordance (items never drop into the world). A cursor-pattern `trash: Option<ItemStack>` on the server `Inventory`, invisible to `insert_first_fit`, `count_item`, `remove_item`, and crafting by construction. One payload-less `SlotAction::Trash`: cursor non-empty → previous trash contents are destroyed and the cursor stack moves in; cursor empty → the trashed stack returns to the cursor (recoverable until replaced). No merge, no half-stack, no quick-move — destroying is a single deliberate gesture.

## Client UI

`E` toggles a full-screen overlay (player grid + hotbar + trash slot; side panel = crafting list / creative palette). Drag & drop via the authoritative cursor, no prediction: left = pick/place/swap, right = half/one, shift-left = quick-move, left on the red-bordered trash slot = trash/recover. Tooltips on hover; the hotbar (slots 0..9) is always visible — digits/scroll select, `[`/`]` size the brush.

## Persistence

`PlayerRecord` (`WORLD_FORMAT_VERSION`) stores per-slot `(item_name, count)` + cursor + trash. Region blobs (`REGION_FORMAT_VERSION`) are cells only. No migrations.
