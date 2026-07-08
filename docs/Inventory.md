# Inventory

Item-centric inventory over the material sim. Items are the resource; materials are one kind of item.

## Item model (`core::item`)

- `ItemId(u16)` (0 = none). `ItemStack { item, count: u32 }` — 8-byte `Copy`.
- `ItemRegistry` from `data/items.ron` + **one auto-generated material item per non-empty material**
  (`"mat:<name>"`, `stack_max = 10_000`, swatch icon = `material.colors[0]`, `place = id`).
- `ItemDef`: category, `stack_max`, icon (`MaterialSwatch` or atlas index), tags, optional `place`
  (material id). Named data items aren't placeable; tools are `stack_max = 1` stubs (no durability/use
  in v1).
- Registry hash folds the material hash + `items.ron`; sent in `HelloAck` as `item_registry_hash`.
- `RecipeRegistry` from `data/recipes.ron` — shapeless, count-based (`inputs → output`).

## Slots

`core::Inventory { slots: Vec<Option<ItemStack>> }`. Player = 36 slots (hotbar = 0..9, main = 9..36).
Ops: `insert_first_fit` (fill matches then empties, returns overflow), `left_click` /
`right_click` (cursor semantics), `remove_item`, `count_item`. Server holds the cursor-held stack.

## Dig / place (server `systems.rs`)

- Selected hotbar slot is `PlayerInput.selected_slot`; brush size is `PlayerInput.brush_radius`
  (0..=6, `[`/`]` or `-`/`=`; scroll cycles the hotbar).
- Survival dig → `item_for_material` into the inventory; overflow spawns a dropped item at the cell.
- Place reads the selected slot's `place` material and stamps it across the brush (survival decrements
  per cell).

## Slot actions (`ClientMessage::Slot(SlotAction)` → `apply_slot_actions`)

Server-authoritative: `LeftClick/RightClick` (cursor), `QuickMove` (shift; hotbar↔main),
`DropSlot`/`DropCursor` (throw into world), `Craft { recipe, times }`, `CreativeGrab` (creative:
infinite stack onto cursor). Inventory syncs via `ServerMessage::Inventory { slots, cursor }` when
dirty.

## Dropped items (Terraria-style)

`DroppedItem { stack }` + `ItemBody(Body)` — small AABB reusing `move_body`. `step_items`: gravity +
grid sweep + ground friction; same-item merge within a chunk; magnetic pull toward a nearby player with
room, absorbed within pickup range (thrown items have a short pickup delay); per-chunk cap bounds count.
Client renders a swatch sprite that bobs and interpolates. Replicated interest-filtered as
`ServerMessage::ItemEntities`, sent only when a viewer has items in interest (plus one clearing
message on the non-empty→empty edge, so an idle world sends nothing); persisted in the region blob.

## Client UI

`E` toggles a full-screen overlay (player grid + hotbar; side panel = crafting / creative
palette). Drag & drop via the authoritative cursor (no prediction): left = pick/place/swap, right =
half/one, shift-left = quick-move, click backdrop = drop to world. Tooltips on hover; hotbar shows
slots 0..9. World input is suppressed while the overlay (or chat) is open.

## Persistence

`WORLD_FORMAT_VERSION = 9`, `REGION_FORMAT_VERSION = 5` (no migrations). `PlayerRecord` stores per-slot
`(item_name, count)` + cursor. Region blobs append `RegionExtras { items }` by item name;
re-spawned on region load, gathered on unload/autosave.
