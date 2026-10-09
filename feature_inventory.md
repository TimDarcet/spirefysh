# Observation feature inventory

Source: `src/learning/observation.rs` on main (feature version 59): `populate_domain_features`,
the row constructors and `ContinuationBuilder`. Notation: `f[i]` numeric column, `u[i]`/`s[i]`
unsigned/signed row fields, `÷ n` the scale applied.

## 1. Columns with a single meaning (no action needed)

| Domain | Columns |
|---|---|
| run (22) | ascension ÷20, act ÷3, floor ÷60, bosses visited ÷3, weak/regular/elite encounters left ÷20, HP ÷30, max HP ÷30, HP fraction, missing HP fraction, gold ÷1000, energy ÷10, draw ÷10, orb slots ÷10, shop removals ÷10, rarity offset ÷10000, potion odds ÷100, unknown combat/elite/treasure/shop weight ÷1000 |
| card (40) | cost delta ÷10, value ÷30, cost override ÷10, enchantment amount ÷30, enchantment value ÷30, replays ÷5, 16 card flags, 16 turn flags, free, cost override set |
| actor (12) | HP ÷30, max HP ÷30, block ÷30, (player only:) max energy ÷5, draw per turn ÷10, orb slots ÷10, energy ÷5, stars ÷5, turn ÷10; HP fraction, missing HP fraction, block fraction |
| power (2) | amount ÷30, value ÷30 |
| relic (1) | Pumpkin Candle counter (raw, unscaled, only that relic) |
| orb (1) | value ÷30 |
| history (29) | player row: cards, manual cards, manual plays, attacks, skills, powers, energy, exhausted, discarded, shivs, stars gained, generated, ethereal, extra drawn, doom applied, Osty attacks, block gains, block card, block card gains, HP lost ÷30, HP loss events, feral returns, last cards, orbit spent, lightning channeled, orbs channeled, poisoned, last damage ÷30, drawn (all others ÷10). Enemy move-history rows leave all 29 at 0: two row types, not overloaded columns |
| action f0–f8 | expected block ÷30, expected target damage ÷30, expected total damage ÷30, player HP delta ÷30, expected draw ÷10, discard ÷10, exhaust ÷10, energy cost ÷10, star cost ÷10 |

## 2. Columns whose meaning depends on the row's kind

### Phase (4 columns)

| Phase kind | f0 | f1 | f2 | f3 |
|---|---|---|---|---|
| combat, choice pending | picks remaining ÷10 | choice filter's signed argument ÷10 (e.g. a cost) | choice operation's signed argument ÷10 | |
| Remove / Upgrade / Transform / Choose cards | cards to pick ÷10 | | | |
| Enchant cards | enchantment amount ÷30 | cards to pick ÷10 | | |
| event Crystal Sphere | | gold cost 51–99 ÷1000 | | |
| event Luminous Choir | | event value 100–149 ÷1000 (probably a gold amount; not traced) | | |
| event Whispering Hollow | | event value 26–44 ÷1000 (not traced) | | |
| event Jungle Maze | | event value ≈150 ÷1000 (not traced) | event value ≈50 ÷1000 (not traced) | |
| event Abyssal Baths / Slippery Bridge | | repeat counter ÷30 (damage = 3 + counter) | | |
| Crystal Sphere minigame active (any phase) | | | | tools remaining ÷10 |

f0 has 3 meanings, f1 has 6, f2 has 2.

### Status (40 columns)

| Status kind | Source | f0 | f1 | f2 | f3 | f4 | f5 | f6–f39 |
|---|---|---|---|---|---|---|---|---|
| 0 | took damage this run (marker) | | | | | | | |
| 1 | nightmare card | copies ÷5 | cost delta ÷10 | value ÷30 | cost override ÷10 | ench. amount ÷30 | ench. value ÷30 | card flags |
| 2 | bomb | turns ÷10 | damage ÷30 | | | | | |
| 3 | automation | turns ÷10 | energy ÷5 | | | | | |
| 4 | panache | cards ÷10 | damage ÷30 | start ÷10 | | | | |
| 5 | rolling boulder | amount ÷30 | | | | | | |
| 6 | history-course card | (card numbers never written) | | | | | | card flags |
| 7 | enemy stunned (marker) | | | | | | | |
| 8 | enemy stored value | value ÷30 | | | | | | |
| 9 | hits this turn | hits ÷10 | | | | | | |
| 10 | enemy move repeats | repeats ÷10 | | | | | | |
| 11 | Wriggler parity | 1 or 2 ÷10 | | | | | | |

f0 has 9 meanings with 4 different scales; f1–f2 have 4–5 meanings each.

### Action f9

| Action kind | f9 |
|---|---|
| take gold reward | gold ÷1000 |
| take removal reward | removals ÷10 |
| buy | price ÷1000 |
| all others | 0 |

### Continuation (58 columns)

Three overlapping regions:
- f0–f1: the row's signed values, meaning set per kind and variant.
- f10–f15: up to six unsigned payload fields copied by `put_continuation_numbers`, with a per-variant choice of field and scale.
- f16–f23: evaluated amounts (base, multiplier, divisor, final value) for up to two amounts, written only for effects being executed (`put_evaluated_amounts`); scale 30, 100 (heal %), 1000 (gold) or 10 (draw-like) per effect.
- Card continuations instead use f1, f4–f9 and f24–f57.

Continuation kinds actually emitted: 0–5 and 7–9 (kind 6 never is; its feature code is dead). Kinds 3 and 9 come from the combat queue, not the builder methods.

**Kind 0, run effect (variant = run effect)**

| Variant | Numbers |
|---|---|
| Gold | f0 gold ÷1000 |
| RandomGold | f0 min ÷1000, f1 max ÷1000 |
| Heal, LoseHp, MaxHp, MaxHpTo | f0 value ÷30 |
| HealPercent | f10 percent ÷100 |
| AddCard, PotionRewards, TransformCards | f10 count ÷10 |
| NextRelics, RemoveCards, UpgradeCards, UpgradeRandom, UpgradeShuffled, DowngradeRandom | f10 count ÷10 |
| SkipEventRng | f10 value ÷10 |
| EventAction | f10 **action id** ÷10 (an ID encoded as a number) |
| EnchantCards | f0 amount ÷30, f10 count ÷10 |
| ChooseCommonCards, ChooseRewardCards | f10 first ÷10, f11 second ÷10 |
| DiscardPotion | slot index stored but **never featured** |
| others | no numbers |

**Kind 1, combat effect (variant = effect)**

| Variants | Numbers |
|---|---|
| Attack, OstyAttack | f10 hit count ÷10 |
| Automation, Energy, OrbSlots, MaxEnergy, SetCardCost, ReduceCardCost | f0 value ÷10 |
| DoublePower | f0 value ÷30 |
| Draw, HandDraw, DrawTo, OfferColorless, OfferCharacter(Retain), ChooseRandomDraw, ShuffleHandDraw, Discard, WhisperingEarring, DrawFiltered(Step), Exhaust, Random, EvokeAll, PassiveFirst, PassiveLast | f10 count ÷10 |
| CopyCard | f10 = **pile id** ÷10 (should be the count, which is in u12) |
| AddCard, AddUpgradedCard, AddFlaggedCard, RandomCard, AddRandom | f10 count ÷10 |
| OfferCharacterType, OfferOtherCharacter, DistinctCharacter, DistinctColorless, ReplayTagged, ExhaustAttackStep, Upgrade, Channel | f10 count ÷10 |
| AddRandomCard | f10 = **card id** ÷10, f11 first value ÷10 (second value lost) |
| TransformHand, RandomOrb, RecycleHand | f10, f11 the two values ÷10 |
| Select | f10, f11 the two values ÷10 |
| PassiveLightning | shares index 118 with Select, so its count (u11) is read from Select's slots and **lost** |
| effects with amounts (Attack, Block, Damage, ApplyPower, …) | amount base / upgraded / multiplier / divisor are stored but only featured as f16–f23 **when the effect is being executed**; queued or offered effects carry no amount |

**Other kinds**

| Kind | Numbers |
|---|---|
| 2 context | f0 X ÷5, f1 event value ÷30 |
| 3 queued auto-play | f10 plays ÷5 |
| 9 card being played | f10 energy ÷10, f11 stars ÷10, f12 plays ÷10 |
| 9 extra player turn, add random curse | markers, no numbers |
| 4 resumed phase: Rewards | f0 gold ÷1000, f1 removals ÷10 |
| 4 resumed phase: Remove / Upgrade / Choose cards | f10 count ÷10 |
| 4 resumed phase: Transform cards | f10 = **target card id + 1** ÷10 (the count is in u12) |
| 4 resumed phase: Enchant cards | f0 amount ÷30, f10 count ÷10 |
| 5 card | f1 replays ÷5, f4 cost delta ÷10, f5 value ÷30, f6 cost override ÷10, f7 ench. amount ÷30, f8 ench. value ÷30, f9 shop price ÷1000, f24–f57 card flags; f0, f2, f3 always 0 |
| 7 event option | f0 gold requirement ÷1000 or HP requirement ÷30, depending on the requirement |
| 8 payload item: shop relic, shop potion, shop removal | f0 price ÷1000 |
| 8 payload item: others (card, relic, card reward, reward relic, reward potion) | no numbers |

f0 alone has more than 15 meanings across kinds, with scales 5, 10, 30, 1000.

## 3. Overloaded categorical codes

- **Boolean namespace**: `push_boolean(field, value)` encodes field × 2 + value, but several sites push `Boolean` with a raw 0/1 value instead (run-effect RandomPotionReward flag, power continuations, many effect payload flags, the continuation leaf flag). Those collide with field 0, which elsewhere means "combat choice pending". Fields 4 and 70 are also each shared by several unrelated uses.
- **Continuation variant**: kinds 2, 5 and 7 all use variant 0; kind 8 uses variants 0–7 for card, relic, card reward, reward relic, reward potion, shop relic, shop potion, shop removal. The same code means different things per kind.
- **Effect 118**: Select and PassiveLightning share it.

## 4. Information stored on rows but never featured

- Phase row in the Rewards phase: gold, removals and card-reward count (gold and removals do reach the model through the RewardGold / RewardRemove actions).
- Status kind 6 (history-course card): cost delta, value, cost override, enchantment numbers.
- Run effect DiscardPotion: slot index.
- Effect PassiveLightning: count (index collision).
- Queued or offered effects: their amounts (only executed effects get f16–f23).
- Actor row: intent value and hits (`s[3]`, `s[4]`); they reach the model via status kinds 8 and 9 instead.

## 5. Dead code

- Continuation kind 6: feature branch for a kind that is never emitted.
- Continuation card rows: f0, f2, f3 always 0; f16–f23 always 0 except executed effects.

## 6. Status after option C (implemented in this workspace, feature version 61)

Cards, relics and potions inside continuations are no longer continuation rows. They are ordinary
card / relic / potion rows (zones 9 / 12 / 5) encoded by those domains' own encoders. Rows inside a
suspended phase or attached to an action use those scopes as before; rows belonging to a state-queue item
use scope = -3 - item, and the model adds their encoding into that item's sum. Prices stay on continuation
rows (new variant 8 = shop card price), and the Unsettling Lamp marker moved onto the queued-play row.
The continuation card layout (f0–f9, f24–f57) and the payload-item IDs above no longer exist.
