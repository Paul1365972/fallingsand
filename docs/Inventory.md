# Inventory

Item-centric inventory over the material sim. Items are the resource; materials are one kind of item.

## Item model (`core::item`)

- `ItemId(u16)` (0 = none). `ItemStack { item, count: u32 }` — 8-byte `Copy`.
- `ItemRegistry` from `data/items.ron` + **one auto-generated material item per non-empty material**
  (`"mat:<name>"`, `stack_max = 10_000`, swatch icon = `material.colors[0]`, `place = id`).
- `ItemDef`: category, `stack_max`, icon (`MaterialSwatch` or atlas index), tags, optional `place`
  (material id). Named data items aren't placeable; tools are `stack_max = 1` stubs (no durability/use
  in v1).
- `RecipeRegistry` from `data/recipes.ron` — shapeless, count-based (`inputs → output`).

## Slots

`core::Inventory { slots: Vec<Option<ItemStack>> }`. Player = 36 slots (hotbar = 0..9, main = 9..36).
Ops: `insert_first_fit` (fill matches then empties, returns overflow), `left_click` /
`right_click` (cursor semantics), `remove_item`, `count_item`. Server holds the cursor-held stack.

## Dig / place (server `systems.rs`)

- Selected hotbar slot and brush size (0..=6, `[`/`]` or `-`/`=`; scroll cycles the hotbar) are
  server-side per-player fields set via `InputAction::SelectSlot`/`SetBrush`. The server clamps
  the brush to `MAX_BRUSH` and ignores a slot outside the hotbar — slot eligibility is
  server-authoritative.
- Survival dig → `item_for_material` into the inventory; a cell whose yield doesn't fit is refused —
  it stays undug in the world (no dig budget spent on it), so a full stack never voids material and
  never blocks digging other materials in the same brush.
- Place reads the selected slot's `place` material and stamps it across the brush (survival decrements
  per cell).

## Slot actions (`InputAction::Slot(SlotAction)` → `apply_slot_actions`)

Server-authoritative and intent-based — the client resolves its keybinds to intents, no raw modifiers
cross the wire: `LeftClick`/`RightClick` (cursor), `QuickMove` (hotbar↔main), `Trash` (see below),
`Craft { recipe, all }` (server crafts once, or repeatedly until inputs run out; a craft whose output
wouldn't fit is refused — inputs are consumed against a trial copy first, so crafts that free their own
space still work, and `all` stops at capacity), `CreativeGrab` (creative: infinite stack onto cursor).
The server holds the cursor and re-validates every action against authoritative state. Inventory rides
the `TickFrame`: all slots + cursor + trash on a session's first frame, then per-slot `(slot, stack)`
diffs (plus the cursor / trash when they change) while dirty — there is no standalone inventory message.

## Trash

The one sanctioned mass-deletion affordance (items never drop into the world). A cursor-pattern
`trash: Option<ItemStack>` beside the cursor on the server `Inventory` — invisible to `insert_first_fit`,
`count_item`, `remove_item`, and crafting by construction. One payload-less `SlotAction::Trash`:
cursor non-empty → the previous trash contents are destroyed and the cursor stack moves in; cursor
empty → the trashed stack returns to the cursor (recoverable until replaced). No merge, no half-stack,
no quick-move — destroying is a single deliberate gesture.

## Client UI

`E` toggles a full-screen overlay (player grid + hotbar + trash slot; side panel = crafting / creative
palette). Drag & drop via the authoritative cursor (no prediction): left = pick/place/swap, right =
half/one, shift-left = quick-move, left on the red-bordered trash slot = trash/recover. Tooltips on
hover; hotbar shows slots 0..9. World input is suppressed while the overlay (or chat) is open.

## Persistence

`WORLD_FORMAT_VERSION = 11`, `REGION_FORMAT_VERSION = 8` (no migrations). `PlayerRecord` stores per-slot
`(item_name, count)` + cursor + trash. Region blobs are version byte + lz4 cell payload — nothing else
rides them.
