# Observation feature redesign: proposed decisions

Companion to `feature_inventory.md`. Each decision lists the recommendation first.

## Principles (apply everywhere)

- P1. One numeric column = one physical quantity with one scale, identical in every row of its domain.
  What is done with the quantity (heal vs lose HP, …) comes from the row's categorical kind, not from
  reusing columns.
- P2. IDs are never numbers. Pile, card, event-action and similar IDs are categorical codes only.
- P3. Every boolean has its own named code. No raw 0/1 pushes into the shared Boolean namespace.
- P4. Each domain declares its numeric fields in Rust as a table of (name, scale); values are written only
  through a helper taking (field, raw value). The simulator exports names and scales; the Python name
  table goes away.
- P5. Enforcement: a Rust test walks many simulated states (all phases, events, Crystal Sphere) and checks
  that every non-zero column is a field declared for that row's kind, that no raw `row.f[i] =` writes
  remain, and that values stay within a scale bound (|x| ≤ 5).

## D1. Continuation

Decided: option C (cards, relics and potions inside continuations reuse their own domains' encoders), implemented; see feature_inventory.md section 6. The remaining kinds (A: per-kind column blocks vs B: separate domains) are still open.

### Original proposal: split into separate token types

The kinds share almost nothing, so each becomes its own domain with a fixed schema:

| New type | From | Numeric fields | Categorical |
|---|---|---|---|
| run effect | kind 0 | gold amount, gold min, gold max (÷1000); HP amount (÷30); heal percent (÷100); count (÷10); second count (÷10); enchantment amount (÷30) | run effect kind, card, relic, potion, rarity, tags, enchantment, card type, event action (new namespace), potion slot |
| combat effect | kind 1 | amount 1 and amount 2, each: base, upgraded (÷30, or the effect's scale), multiplier, divisor (÷10), evaluated value; count (÷10); second count (÷10); signed value (÷10) | effect kind, target, pile, card, power, orb, filter, operation, condition, scale kinds, flags |
| execution context | kind 2 | X (÷5), event value (÷30) | source, target, card, orb |
| event option | kind 7 | gold requirement (÷1000), HP requirement (÷30) | requirement kind |
| card reward descriptor | kind 8 variant 2 | none | reward kind, room, card, rarity |

The rest folds into existing domains instead of new types:
- card continuations (kind 5) become card-domain rows with a new zone (reward, shop, autoplay payload); the
  card domain gains a "price" field (÷1000);
- relic and potion payload items (kind 8) become relic- and potion-domain rows with reward/shop zones;
  those domains gain "price";
- shop removal becomes a card-removal row (or a phase field, see D4);
- resumed phase (kind 4) folds into the phase domain, which already has a suspended-phase row (see D4).

Alternative: one wide continuation domain holding the union of all fields (simpler wiring, ~40 sparse
columns, one encoder shared by unrelated things).

## D2. Combat-effect amounts: always encoded (recommended)

Today amounts are only featured while an effect is being executed, so queued and offered effects carry no
numbers. Proposal: always write base, upgraded, multiplier and divisor; write the evaluated value when a
context exists, with a presence flag for "evaluated".

## D3. Combat-effect parameters: semantic fields (recommended)

Map each of the ~119 effects' parameters onto a small set of shared fields (amount 1/2, count, second
count, signed value), with the effect kind saying how they are used. Alternative: dedicated columns per
effect parameter (hundreds of columns, almost all zero). The per-effect mapping table will be produced
during implementation for review.

## D4. Phase: union of named fields (recommended)

| Field | Scale | Set by |
|---|---|---|
| cards to pick | ÷10 | combat choice (picks remaining), remove / upgrade / transform / choose / enchant phases |
| filter cost | ÷10 | combat choice whose filter is Cost or PlayableCost |
| operation cost | ÷10 | combat choice whose operation is Cost or SetCost |
| enchantment amount | ÷30 | enchant phase |
| event gold cost | ÷1000 | Crystal Sphere (and any event whose paid option has a gold cost) |
| event counter | ÷30 | Abyssal Baths, Slippery Bridge |
| per-event value fields | per event | Luminous Choir, Whispering Hollow, Jungle Maze ×2 (meaning to be traced first) |
| Crystal Sphere tools remaining | ÷10 | minigame active |
| reward gold, reward removals | ÷1000, ÷10 | Rewards phase (decision D8) |

The suspended phase keeps its own phase row (already the case); the duplicate resumed-phase continuation is
dropped.

Fixes: the TypeWithoutTurnFlag filter currently also writes its flag ID as a number (P2).

## D5. Status: move entity properties out, keep a small union (recommended)

Implemented (feature version 61): nightmare and history-course cards are card rows (zones 6 / 7, tagged
with status kind 1 / 6) pooled into the player's effects; enemy stored value, hits this turn and move
repeats are actor f12–f14 and Wriggler parity is an actor flag. "Stunned" was already encoded by the
enemy's move token, so its status row is simply dropped.

- Card statuses (nightmare, history course) become card-domain rows with new zones; this also restores the
  history-course card's missing numbers.
- Enemy properties become actor fields: stored value (÷30), hits this turn (÷10), move repeats (÷10),
  Wriggler parity; "stunned" becomes an actor flag.
- Status keeps combat-wide effects with one field each: bomb turns, bomb damage, automation turns,
  automation energy, panache cards, panache damage, panache start, boulder damage; plus the
  "took damage" marker as a categorical kind.

Alternative: keep everything in status as one union (~20 fields + 40 card columns).

## D6. Action column 9: three fields (recommended)

reward gold (÷1000), removal reward count (÷10), purchase price (÷1000).

## D7. Presence flags: only where a field can be absent within one kind (recommended)

The row's kind one-hot already says which fields apply, so no flag is needed in general. Flags only where
the same kind may or may not have the value (e.g. "evaluated amount available").

## D8. Rewards-phase gold / removals / card-reward count: add to the phase row?

They currently reach the model only through the RewardGold / RewardRemove actions. Recommendation: add them
(cheap, and the information is visible even when those actions are not legal).

## D9. Scales: keep current scales, documented in the schema (recommended)

Review outliers during implementation (card rarity offset ÷10000, the raw Pumpkin Candle counter) and
enforce the |x| ≤ 5 bound from P5.

## D10. Other fixes included

Implemented (feature version 61), except the DiscardPotion slot: no content constructs
RunEffect::DiscardPotion, so there is nothing to restore.

- Effect index collision: PassiveLightning gets its own index.
- IDs as numbers removed: CopyCard pile, AddRandomCard card, resumed TransformCards target, event action.
- Lost values restored: DiscardPotion slot, PassiveLightning count, AddRandomCard second value.
- Dead code removed: continuation kinds 3, 6, 9.
- Boolean: one named code per meaning (P3); continuation-variant namespace replaced by per-type kinds.

## D11. Rollout

Rebased on main, which had independently taken feature version 60 / model version 78; this workspace is
feature version 61 / model version 79. New observations mean fresh training runs; old checkpoints do
not load. The Python/Rust parity test, the P5 schema test and the viewer's names (now exported) cover it.
