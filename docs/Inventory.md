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

- Selected hotbar slot is `PlayerInput.selected_slot`; brush size is `PlayerInput.brush_radius`
  (0..=6, `[`/`]` or `-`/`=`; scroll cycles the hotbar). The server clamps `brush_radius` to
  `MAX_BRUSH` and ignores a `selected_slot` outside the hotbar before use — slot eligibility is
  server-authoritative.
- Survival dig → `item_for_material` into the inventory; overflow spawns a dropped item at the cell.
- Place reads the selected slot's `place` material and stamps it across the brush (survival decrements
  per cell).

## Slot actions (`ClientMessage::Slot(SlotAction)` → `apply_slot_actions`)

Server-authoritative and intent-based — the client resolves its keybinds to intents, no raw modifiers
cross the wire: `LeftClick`/`RightClick` (cursor), `QuickMove` (hotbar↔main), `DropSlot`/`DropCursor`
(throw into world), `Craft { recipe, all }` (server crafts once, or repeatedly until inputs run out),
`CreativeGrab` (creative: infinite stack onto cursor). The server holds the cursor and re-validates
every action against authoritative state. Inventory rides the `TickFrame`: all slots + cursor on a
session's first frame, then per-slot `(slot, stack)` diffs (plus the cursor when it changes) while
dirty — there is no standalone inventory message.

## Dropped items (Terraria-style)

`DroppedItem` + `ItemActor(Actor)` — small AABB reusing `move_body`. `step_items`: gravity (capped at a
speed-of-light-safe fall clamp) + grid sweep + seconds-based ground/air drag; local same-item touch-merge
(overlapping stacks drain together, never deletes — mass is conserved, only emptied entities despawn);
magnetic pull toward a nearby player with room, absorbed within pickup range (thrown items have a short
pickup delay).
Items **sleep** once settled on the ground with no player in grab range — skipping physics, merge, and
replication until a player nears — so a resting pile costs ~nothing. Client renders a swatch sprite that
bobs and interpolates. Replicated as interest-filtered `ServerMessage::ItemDelta { spawned, moved,
despawned }` against a per-session known-item set: only awake (moving) items send positions, so an idle
world sends nothing; persisted in the owning region blob.

## Client UI

`E` toggles a full-screen overlay (player grid + hotbar; side panel = crafting / creative
palette). Drag & drop via the authoritative cursor (no prediction): left = pick/place/swap, right =
half/one, shift-left = quick-move, click backdrop = drop to world. Tooltips on hover; hotbar shows
slots 0..9. World input is suppressed while the overlay (or chat) is open.

## Persistence

`WORLD_FORMAT_VERSION = 10`, `REGION_FORMAT_VERSION = 7` (no migrations). `PlayerRecord` stores per-slot
`(item_name, count)` + cursor. Region blobs append `RegionExtras { items }` (item name, position,
velocity, age, pickup delay); re-spawned on region load, gathered on unload/autosave. Active items mark
their region dirty each tick (both the region they leave and the one they enter, so boundary crossings
clear the stale blob); asleep items mark nothing, so an idle region — terrain and items alike — is never
re-saved. This closes the pickup/boundary dupe and keeps conservation of mass across save/reload.
