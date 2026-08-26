#![cfg_attr(not(feature = "python"), allow(dead_code))]

use crate::*;
use std::{
    fs,
    hash::{Hash, Hasher},
    io,
    path::Path,
};

const MAGIC: &[u8; 8] = b"STSVALUE";
const VERSION: u32 = 37;
const VALUE_MODEL_VERSION: u32 = 39;
const TOKEN_CATEGORICAL: usize = 8;
const TOKEN_NUMERIC: usize = 24;
const TOKEN_VALUES: usize = TOKEN_CATEGORICAL + TOKEN_NUMERIC;
const TOKEN_ACTION_VALUES: usize = ACTION_KINDS + ACTION_VALUES + 9;
const ENEMY_COLLECTION: usize = 1;
const POWER_COLLECTION: usize = 2;
const DELAYED_COLLECTION: usize = 3;
const RELIC_COLLECTION: usize = 4;
const POTION_COLLECTION: usize = 5;
const CONTINUATION_COLLECTION: usize = 6;
const MAP_COLLECTION: usize = 7;
const CRYSTAL_COLLECTION: usize = 8;
const ENCOUNTER_COLLECTION: usize = 9;
const STATE_COLLECTION: usize = 10;
const CARD_COLLECTION: usize = 11;
const ACTION_CARD_COLLECTION: usize = 12;
const PARASOL_CARD_KIND: usize = 6;
const ANCIENT_RELIC_KIND: usize = 16;
const NIGHTMARE_CARD_KIND: usize = 17;
const TOY_BOX_RELIC_KIND: usize = 18;
const DAMPENED_CARD_KIND: usize = 18;
const QUEUED_CARD_KIND: usize = 19;
const RESUME_CARD_KIND: usize = 20;
type Token = [f32; TOKEN_VALUES];

fn token(collection: usize, kind: usize, id: usize, owner: usize, position: usize) -> Token {
    let mut token = [0.0; TOKEN_VALUES];
    token[..5].copy_from_slice(&[
        collection as f32,
        kind as f32,
        id.min(u16::MAX as usize + 1) as f32,
        owner.min(u16::MAX as usize + 1) as f32,
        position.min(u16::MAX as usize + 1) as f32,
    ]);
    token
}
const SCALARS: usize = 128;
const PHASES: usize = 14;
const ROOMS: usize = 8;
const CARD_ZONES: usize = 10;
const CARD_SLOTS: usize = 512;
const CARD_FIELDS: usize = 13;
const CARD_TOKEN_VALUES: usize = CARD_FIELDS + 5;
const CARD_VALUES: usize = 12;
const ENCHANTMENTS: usize = 22;
const ENEMY_SLOTS: usize = 8;
const ENEMY_VALUES: usize = 9;
const ENEMY_HISTORY_VALUES: usize = 23;
const OVERFLOW_ENEMY_GROUPS: usize = 256;
const ENEMY_TOKEN_VALUES: usize = 1 + ENEMY_VALUES + ENEMY_HISTORY_VALUES;
const OVERFLOW_ENEMY_VALUES: usize = 2 + ENEMY_TOKEN_VALUES;
const OVERFLOW_POWER_SLOTS: usize = 2048;
const OVERFLOW_POWER_VALUES: usize = 6;
const COMBAT_VALUES: usize = 14;
const POWER_SLOTS: usize = 256;
const POWER_VALUES: usize = 4;
const SNAPSHOT_ACTORS: usize = 1 + ENEMY_SLOTS;
const ORB_SLOTS: usize = 10;
const KNOWN_DRAW_SLOTS: usize = u8::MAX as usize;
const KNOWN_CARD_VALUES: usize = CARD_TOKEN_VALUES + 1;
const DELAYED_SLOTS: usize = 16;
const DELAYED_VALUES: usize = DELAYED_SLOTS * 9 + 10;
const RUN_VALUES: usize = 10;
const MAP_FLOORS: usize = 18;
const MAP_LANES: usize = 8;
const MAP_VALUES: usize = 4 + ROOMS + MAP_LANES;
const ACTION_KINDS: usize = 32;
const ACTION_VALUES: usize = 16;
const EVENT_REQUIREMENTS: usize = 4;
const EVENT_EFFECTS: usize = 36;
const EVENT_VALUES: usize = 8;
const EVENT_STATE_VALUES: usize = 4;
const PARASOL_SLOTS: usize = 14;
const PARASOL_VALUES: usize = 4 + CARD_FIELDS + 2;
const FAKE_MERCHANT_SLOTS: usize = 6;
const GOLD_PART_SLOTS: usize = 121;
const CONTINUATION_BYTES: usize = 16_384;
const CONTINUATION_VALUES: usize = 1 + CONTINUATION_BYTES.div_ceil(2);

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct Layout {
    characters: usize,
    cards: usize,
    powers: usize,
    relics: usize,
    potions: usize,
    enemies: usize,
    orbs: usize,
    events: usize,
    encounters: usize,
    acts: usize,
    tinker_time: Option<Id>,
    slippery_bridge: Option<Id>,
    relic_trader: Option<Id>,
    ancient_events: [Option<Id>; 8],
    stone_of_all_time: Option<Id>,
    future_of_potions: Option<Id>,
    shared_relics: &'static [Id],
    character: usize,
    act: usize,
    phase: usize,
    room: usize,
    event: usize,
    visited_event: usize,
    event_requirement: usize,
    event_effect: usize,
    event_value: usize,
    event_state: usize,
    encounter: usize,
    card: usize,
    enchantment: usize,
    known_draw: usize,
    nightmare: usize,
    dampened: usize,
    relic: usize,
    relic_trade: usize,
    relic_bag: usize,
    potion: usize,
    power: usize,
    power_snapshot: usize,
    enemy: usize,
    enemy_value: usize,
    enemy_history: usize,
    enemy_overflow: usize,
    enemy_overflow_power: usize,
    orb: usize,
    combat: usize,
    unsettling_lamp: usize,
    delayed: usize,
    run_state: usize,
    shop_card_price: usize,
    shop_relic_price: usize,
    shop_potion_price: usize,
    parasol: usize,
    fake_merchant: usize,
    reward_gold_parts: usize,
    crystal: usize,
    map: usize,
    continuation: usize,
    decision: usize,
    state_len: usize,
    action_card: usize,
    action_enchantment: usize,
    action_card_state: usize,
    action_relic: usize,
    action_potion: usize,
    action_enemy: usize,
    action_room: usize,
    action_requirement: usize,
    action_effect: usize,
    action_value: usize,
    action_len: usize,
}

impl Layout {
    fn new(content: &Content) -> Self {
        let (characters, cards, powers, relics, potions, enemies, orbs, events, encounters) = (
            content.characters.len(),
            content.cards.len(),
            content.powers.len(),
            content.relics.len(),
            content.potions.len(),
            content.enemies.len(),
            content.orbs.len(),
            content.events.len(),
            content.encounters.len(),
        );
        let acts = content.acts.len();
        let tinker_time = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.TINKER_TIME")
            .map(|id| id as Id);
        let slippery_bridge = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.SLIPPERY_BRIDGE")
            .map(|id| id as Id);
        let relic_trader = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.RELIC_TRADER")
            .map(|id| id as Id);
        let ancient_events = [
            "EVENT.NEOW",
            "EVENT.DARV",
            "EVENT.NONUPEIPE",
            "EVENT.OROBAS",
            "EVENT.TANX",
            "EVENT.TEZCATARA",
            "EVENT.PAEL",
            "EVENT.VAKUU",
        ]
        .map(|name| {
            content
                .events
                .iter()
                .position(|event| event.id == name)
                .map(|id| id as Id)
        });
        let stone_of_all_time = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.STONE_OF_ALL_TIME")
            .map(|id| id as Id);
        let future_of_potions = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.THE_FUTURE_OF_POTIONS")
            .map(|id| id as Id);
        let shared_relics = content.acts.first().map_or(&[][..], |act| act.relics);
        let mut next = SCALARS;
        let character = take(&mut next, characters);
        let act = take(&mut next, acts);
        let phase = take(&mut next, PHASES);
        let room = take(&mut next, ROOMS);
        let event = take(&mut next, events);
        let visited_event = take(&mut next, events);
        let event_requirement = take(&mut next, EVENT_REQUIREMENTS);
        let event_effect = take(&mut next, EVENT_EFFECTS);
        let event_value = take(&mut next, EVENT_VALUES);
        let event_state = take(&mut next, EVENT_STATE_VALUES);
        let encounter = take(&mut next, 5 * encounters);
        let card = take(&mut next, CARD_ZONES * CARD_SLOTS * CARD_TOKEN_VALUES);
        let enchantment = take(&mut next, ENCHANTMENTS);
        let known_draw = take(&mut next, 2 * KNOWN_DRAW_SLOTS * KNOWN_CARD_VALUES);
        let nightmare = take(
            &mut next,
            DELAYED_SLOTS * (cards + CARD_VALUES + ENCHANTMENTS + 1),
        );
        let dampened = take(
            &mut next,
            DELAYED_SLOTS * (4 + cards + CARD_VALUES + ENCHANTMENTS + 1),
        );
        let relic = take(&mut next, 6 * relics);
        let relic_trade = take(&mut next, 6 * relics);
        let relic_bag = take(&mut next, 8 * relics);
        let potion = take(&mut next, 2 * potions);
        let power = take(&mut next, (2 + ENEMY_SLOTS) * POWER_SLOTS * POWER_VALUES);
        let power_snapshot = take(&mut next, SNAPSHOT_ACTORS * POWER_SLOTS * POWER_VALUES);
        let enemy = take(&mut next, ENEMY_SLOTS * enemies);
        let enemy_value = take(&mut next, ENEMY_SLOTS * ENEMY_VALUES);
        let enemy_history = take(&mut next, ENEMY_SLOTS * ENEMY_HISTORY_VALUES);
        let enemy_overflow = take(&mut next, OVERFLOW_ENEMY_GROUPS * OVERFLOW_ENEMY_VALUES);
        let enemy_overflow_power = take(&mut next, OVERFLOW_POWER_SLOTS * OVERFLOW_POWER_VALUES);
        let orb = take(&mut next, ORB_SLOTS * (orbs + 1));
        let combat = take(&mut next, COMBAT_VALUES);
        let unsettling_lamp = take(&mut next, cards);
        let delayed = take(&mut next, DELAYED_VALUES);
        let run_state = take(&mut next, RUN_VALUES);
        let shop_card_price = take(&mut next, cards);
        let shop_relic_price = take(&mut next, relics);
        let shop_potion_price = take(&mut next, potions);
        let parasol = take(&mut next, PARASOL_SLOTS * PARASOL_VALUES);
        let fake_merchant = take(&mut next, FAKE_MERCHANT_SLOTS);
        let reward_gold_parts = take(&mut next, GOLD_PART_SLOTS);
        let crystal = take(&mut next, 121 * 10);
        let map = take(&mut next, MAP_FLOORS * MAP_LANES * MAP_VALUES);
        let continuation = take(&mut next, CONTINUATION_VALUES);
        let decision = take(&mut next, 8);
        let state_len = next;
        let mut next = ACTION_KINDS;
        let action_card = take(&mut next, cards);
        let action_enchantment = take(&mut next, ENCHANTMENTS);
        let action_card_state = take(&mut next, 2);
        let action_relic = take(&mut next, relics);
        let action_potion = take(&mut next, potions);
        let action_enemy = take(&mut next, enemies);
        let action_room = take(&mut next, ROOMS);
        let action_requirement = take(&mut next, EVENT_REQUIREMENTS);
        let action_effect = take(&mut next, EVENT_EFFECTS);
        let action_value = take(&mut next, ACTION_VALUES);
        Self {
            characters,
            cards,
            powers,
            relics,
            potions,
            enemies,
            orbs,
            events,
            encounters,
            acts,
            tinker_time,
            slippery_bridge,
            relic_trader,
            ancient_events,
            stone_of_all_time,
            future_of_potions,
            shared_relics,
            character,
            act,
            phase,
            room,
            event,
            visited_event,
            event_requirement,
            event_effect,
            event_value,
            event_state,
            encounter,
            card,
            enchantment,
            known_draw,
            nightmare,
            dampened,
            relic,
            relic_trade,
            relic_bag,
            potion,
            power,
            power_snapshot,
            enemy,
            enemy_value,
            enemy_history,
            enemy_overflow,
            enemy_overflow_power,
            orb,
            combat,
            unsettling_lamp,
            delayed,
            run_state,
            shop_card_price,
            shop_relic_price,
            shop_potion_price,
            parasol,
            fake_merchant,
            reward_gold_parts,
            crystal,
            map,
            continuation,
            decision,
            state_len,
            action_card,
            action_enchantment,
            action_card_state,
            action_relic,
            action_potion,
            action_enemy,
            action_room,
            action_requirement,
            action_effect,
            action_value,
            action_len: next,
        }
    }
}

fn take(next: &mut usize, len: usize) -> usize {
    let offset = *next;
    *next += len;
    offset
}

fn room_index(room: Room) -> usize {
    match room {
        Room::Combat => 0,
        Room::Elite => 1,
        Room::Boss => 2,
        Room::Unknown => 3,
        Room::Event => 4,
        Room::Shop => 5,
        Room::Rest => 6,
        Room::Treasure => 7,
    }
}

fn phase_index(phase: &Phase) -> usize {
    match phase {
        Phase::Map => 0,
        Phase::Combat(_) => 1,
        Phase::Rewards(_) => 2,
        Phase::Shop(_) => 3,
        Phase::Rest => 4,
        Phase::Event(..) => 5,
        Phase::RemoveCards(..) => 6,
        Phase::UpgradeCards(..) => 7,
        Phase::TransformCards(..) => 8,
        Phase::EnchantCards(..) => 9,
        Phase::ChooseCards(..) => 10,
        Phase::ChooseBundles(..) => 11,
        Phase::Won => 12,
        Phase::Dead => 13,
    }
}

fn card_type_index(card_type: CardType) -> usize {
    match card_type {
        CardType::Attack => 0,
        CardType::Skill => 1,
        CardType::Power => 2,
        CardType::Status => 3,
        CardType::Curse => 4,
        CardType::Quest => 5,
    }
}

fn pile_index(pile: Pile) -> usize {
    match pile {
        Pile::Draw => 0,
        Pile::Hand => 1,
        Pile::Discard => 2,
        Pile::Exhaust => 3,
        Pile::Offer => 4,
    }
}

fn filter_features(filter: CardFilter) -> (usize, usize) {
    match filter {
        CardFilter::Any => (0, 0),
        CardFilter::Type(kind) => (1, card_type_index(kind)),
        CardFilter::AttackOrPower => (2, 0),
        CardFilter::TypeWithoutTurnFlag(kind, flag) => (
            3,
            card_type_index(kind) * (u16::MAX as usize + 1) + flag as usize,
        ),
        CardFilter::WithoutFlag(flag) => (4, flag as usize),
        CardFilter::NotType(kind) => (5, card_type_index(kind)),
        CardFilter::Id(id) => (6, id as usize),
        CardFilter::Cost(cost) => (7, (cost as i16 + 128) as usize),
        CardFilter::PlayableCost(cost) => (8, (cost as i16 + 128) as usize),
        CardFilter::Flag(flag) => (9, flag as usize),
        CardFilter::Upgradable => (10, 0),
        CardFilter::Colorless => (11, 0),
        CardFilter::Rare => (12, 0),
        CardFilter::NoReplay => (13, 0),
        CardFilter::PlayableOrAny => (14, 0),
        CardFilter::CostsResource => (15, 0),
    }
}

fn op_features(op: CardOp) -> (usize, usize) {
    match op {
        CardOp::Move(pile) => (0, pile_index(pile)),
        CardOp::Upgrade => (1, 0),
        CardOp::Cost(cost) => (2, (cost as i16 + 128) as usize),
        CardOp::SetCost(cost) => (3, (cost as i16 + 128) as usize),
        CardOp::Flag(flag) => (4, flag as usize),
        CardOp::TurnFlag(flag) => (5, flag as usize),
        CardOp::CopyNextTurn(count) => (6, count as usize),
        CardOp::Transform(id, upgrades) => (7, id as usize * 256 + upgrades as usize),
        CardOp::AutoPlay(count) => (8, count as usize),
        CardOp::CopySelected(count) => (9, count as usize),
        CardOp::TakeOffer => (10, 0),
        CardOp::Transfigure => (11, 0),
        CardOp::Replay(count) => (12, count as usize),
        CardOp::TakeFetched => (13, 0),
        CardOp::TransformRandom => (14, 0),
        CardOp::DiscardDraw => (15, 0),
        CardOp::MoveFree(pile) => (16, pile_index(pile)),
        CardOp::FreeCombat => (17, 0),
    }
}

fn scalar(out: &mut [f32], index: &mut usize, value: impl Into<f64>) {
    assert!(*index < SCALARS);
    out[*index] = value.into() as f32;
    *index += 1;
}

fn card_values(card: Card) -> [f32; CARD_VALUES] {
    [
        0.1,
        card.upgrades as f32 / 10.0,
        card.cost_delta as f32 / 10.0,
        card.value as f32 / 100.0,
        card.free as u8 as f32 * 0.1,
        card.enchantment_amount as f32 / 100.0,
        card.replays as f32 / 10.0,
        card.cost_override.map_or(0, |cost| cost as i16 + 129) as f32 / 256.0,
        card.flags as f32 / u16::MAX as f32,
        card.turn_flags as f32 / u16::MAX as f32,
        card.enchantment_value as f32 / 100.0,
        card.variant as f32 / 10.0,
    ]
}

fn card_fields(card: Card) -> [f32; CARD_FIELDS] {
    [
        (card.id as u32 + 1) as f32 / (u16::MAX as f32 + 1.0),
        card.upgrades as f32 / 255.0,
        card.cost_delta as f32 / 128.0,
        card.flags as f32 / (u16::MAX as f32 + 1.0),
        card.turn_flags as f32 / (u16::MAX as f32 + 1.0),
        card.value as f32 / 32768.0,
        card.replays as f32 / 255.0,
        card.free as u8 as f32,
        card.cost_override.map_or(0, |cost| cost as i16 + 129) as f32 / 256.0,
        card.enchantment.map_or(0, |value| value as u8 + 1) as f32 / ENCHANTMENTS as f32,
        card.enchantment_amount as f32 / 32768.0,
        card.enchantment_value as f32 / 32768.0,
        card.variant as f32 / 255.0,
    ]
}

fn token_cmp(left: &[f32], right: &[f32]) -> std::cmp::Ordering {
    left.iter()
        .zip(right)
        .find_map(|(left, right)| (left != right).then(|| left.total_cmp(right)))
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn master_cards(deck: &[Card]) -> Vec<[f32; CARD_FIELDS]> {
    let mut cards = deck.iter().copied().map(card_fields).collect::<Vec<_>>();
    cards.sort_by(|left, right| token_cmp(left, right));
    cards.dedup();
    cards
}

fn card_state(
    card: Card,
    deck: &[Card],
    masters: &[[f32; CARD_FIELDS]],
    dampened: &[(u32, u8)],
) -> [f32; 3] {
    let (master, upgrades) = card_relation(card, deck, masters, dampened);
    [
        master as f32 / (CARD_SLOTS + 1) as f32,
        upgrades as f32 / 256.0,
        0.0,
    ]
}

fn card_relation(
    card: Card,
    deck: &[Card],
    masters: &[[f32; CARD_FIELDS]],
    dampened: &[(u32, u8)],
) -> (usize, u16) {
    let master = (card.instance != 0)
        .then(|| deck.iter().find(|master| master.instance == card.instance))
        .flatten()
        .map(|card| card_fields(*card))
        .map(|master| {
            masters
                .binary_search_by(|candidate| token_cmp(candidate, &master))
                .unwrap()
                + 1
        })
        .unwrap_or(0);
    let upgrades = dampened
        .iter()
        .find(|(instance, _)| *instance == card.instance)
        .map_or(0, |(_, upgrades)| *upgrades as u16 + 1);
    (master, upgrades)
}

fn card_token(card: Card, state: [f32; 3]) -> [f32; CARD_TOKEN_VALUES] {
    let mut token = [0.0; CARD_TOKEN_VALUES];
    token[..CARD_FIELDS].copy_from_slice(&card_fields(card));
    token[CARD_FIELDS..CARD_FIELDS + 3].copy_from_slice(&state);
    token[CARD_TOKEN_VALUES - 2] = 1.0 / u16::MAX as f32;
    token
}

fn add_card_state(out: &mut [f32], layout: Layout, zone: usize, card: Card, state: [f32; 3]) {
    assert!(zone < CARD_ZONES && (card.id as usize) < layout.cards);
    let width = CARD_SLOTS * CARD_TOKEN_VALUES;
    let cards = &mut out[layout.card + zone * width..layout.card + (zone + 1) * width];
    let used = (0..CARD_SLOTS)
        .position(|slot| cards[slot * CARD_TOKEN_VALUES] == 0.0)
        .unwrap_or(CARD_SLOTS);
    let token = card_token(card, state);
    if let Some(slot) = (0..used).find(|slot| {
        token_cmp(
            &cards[slot * CARD_TOKEN_VALUES..(slot + 1) * CARD_TOKEN_VALUES - 2],
            &token[..CARD_TOKEN_VALUES - 2],
        )
        .is_eq()
    }) {
        let offset = slot * CARD_TOKEN_VALUES + CARD_TOKEN_VALUES - 2;
        let low = (cards[offset] * u16::MAX as f32).round() as u32;
        let high = (cards[offset + 1] * u16::MAX as f32).round() as u32;
        let count = low | high << 16;
        assert!(
            count < u32::MAX,
            "card multiplicity overflow in zone {zone}"
        );
        let count = count + 1;
        cards[offset] = (count & u16::MAX as u32) as f32 / u16::MAX as f32;
        cards[offset + 1] = (count >> 16) as f32 / u16::MAX as f32;
        return;
    }
    assert!(
        used < CARD_SLOTS,
        "card zone {zone} exceeds {CARD_SLOTS} distinct public card states"
    );
    let at = (0..used)
        .position(|slot| {
            token_cmp(
                &cards[slot * CARD_TOKEN_VALUES..(slot + 1) * CARD_TOKEN_VALUES - 2],
                &token[..CARD_TOKEN_VALUES - 2],
            )
            .is_gt()
        })
        .unwrap_or(used);
    cards.copy_within(
        at * CARD_TOKEN_VALUES..used * CARD_TOKEN_VALUES,
        (at + 1) * CARD_TOKEN_VALUES,
    );
    cards[at * CARD_TOKEN_VALUES..(at + 1) * CARD_TOKEN_VALUES].copy_from_slice(&token);
}

fn add_card(out: &mut [f32], layout: Layout, zone: usize, card: Card) {
    add_card_state(out, layout, zone, card, [0.0; 3]);
}

fn add_cards(out: &mut [f32], layout: Layout, zone: usize, cards: &[Card]) {
    for &card in cards {
        add_card(out, layout, zone, card);
    }
}

fn add_combat_cards(
    out: &mut [f32],
    layout: Layout,
    zone: usize,
    cards: &[Card],
    deck: &[Card],
    masters: &[[f32; CARD_FIELDS]],
    dampened: &[(u32, u8)],
) {
    for &card in cards {
        add_card_state(
            out,
            layout,
            zone,
            card,
            card_state(card, deck, masters, dampened),
        );
    }
}

#[derive(Default)]
struct CardPreview {
    block: i16,
    hits: Vec<PreviewHit>,
    draw: i16,
    discard: i16,
    exhaust: i16,
    vigor: i16,
    gigantification: bool,
    statuses: i16,
    costs: Option<(i16, i16)>,
    hand: i16,
    draw_available: i16,
    draw_pile: i16,
    known_drawn: usize,
    doom: i16,
    orbs: Vec<Orb>,
    orb_slots: usize,
    orbs_channeled: u8,
    playing_removed: bool,
    last_damage: i16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewTarget {
    Chosen,
    Random,
    Lowest,
    All,
    Other,
    Fixed(usize),
}

#[derive(Clone, Copy)]
enum PreviewDamage {
    Attack(Actor),
    Move(Actor),
    Unpowered,
    LoseHp,
    Doom(i16),
}

#[derive(Clone, Copy)]
struct PreviewHit {
    target: PreviewTarget,
    damage: PreviewDamage,
    amount: i16,
    count: i16,
    card: Option<Card>,
    pen_nib: bool,
    vigor: i16,
    lethality: bool,
    gigantification: bool,
}

impl CardPreview {
    fn new(game: &Game, content: &Content) -> Self {
        let Some(combat) = game.combat() else {
            return Self::default();
        };
        Self {
            vigor: combat.player.power(power_id::VIGOR),
            gigantification: combat.player.power(power_id::GIGANTIFICATION) > 0,
            statuses: combat
                .draw
                .iter()
                .chain(&combat.hand)
                .chain(&combat.discard)
                .filter(|card| content.cards[card.id as usize].card_type == CardType::Status)
                .count()
                .min(i16::MAX as usize) as i16,
            hand: combat.hand.len().min(i16::MAX as usize) as i16,
            draw_available: combat
                .draw
                .len()
                .saturating_add(combat.discard.len())
                .min(i16::MAX as usize) as i16,
            draw_pile: combat.draw.len().min(i16::MAX as usize) as i16,
            orbs: combat.orbs.clone(),
            orb_slots: combat.orb_slots as usize,
            orbs_channeled: combat.orbs_channeled,
            last_damage: combat.last_damage,
            ..Self::default()
        }
    }
}

fn preview_amount(game: &Game, content: &Content, context: Context, amount: Amount) -> i16 {
    if game.combat().is_some() {
        game.amount(content, context, amount)
    } else if context.upgraded {
        amount.upgraded
    } else {
        amount.base
    }
}

fn preview_effect_amount(
    game: &Game,
    content: &Content,
    context: Context,
    amount: Amount,
    preview: &CardPreview,
) -> i16 {
    if amount.scale != Scale::LastDamage {
        return preview_amount(game, content, context, amount);
    }
    let base = if context.upgraded
        || matches!(context.source, Actor::Enemy(_))
            && amount.ascension > 0
            && game.run.ascension >= amount.ascension
    {
        amount.upgraded
    } else {
        amount.base
    };
    base.saturating_add(preview.last_damage.saturating_mul(amount.multiplier))
        / amount.divisor.max(1)
}

fn preview_attack(game: &Game, _content: &Content, source: Actor, base: i16) -> i16 {
    let Some(combat) = game.combat() else {
        return base.max(0);
    };
    let source = match source {
        Actor::Osty => &combat.osty,
        _ => &combat.player,
    };
    if source.hp <= 0 {
        return 0;
    }
    base.max(0)
}

fn weak_damage(game: &Game, content: &Content, source: Actor, damage: i16) -> i16 {
    let Some(combat) = game.combat() else {
        return damage;
    };
    let source = match source {
        Actor::Osty => &combat.osty,
        _ => &combat.player,
    };
    if source.kind(content, PowerKind::Weak) > 0 {
        damage.saturating_mul(if source.power(power_id::DEBILITATE) > 0 {
            50
        } else {
            75
        }) / 100
    } else {
        damage
    }
}

fn preview_block(
    game: &Game,
    content: &Content,
    context: Context,
    base: i16,
    powered: bool,
) -> i16 {
    let Some(combat) = game.combat() else {
        return base.max(0);
    };
    if context.card.is_some() && combat.player.power(power_id::NO_BLOCK) > 0 {
        return 0;
    }
    let relic = |id| {
        game.run
            .relics
            .iter()
            .any(|&relic| content.relics[relic as usize].id == id)
    };
    let fasten = context
        .card
        .filter(|&id| content.cards[id as usize].tags & DEFEND_TAG != 0)
        .map_or(0, |_| combat.player.power(power_id::FASTEN));
    let enchantment = context
        .card
        .and_then(|id| {
            combat
                .auto_plays
                .last()
                .map(|play| play.card)
                .or(combat.playing)
                .filter(|card| card.id == id)
        })
        .map_or(0, |card| match card.enchantment {
            Some(Enchantment::Nimble) => card.enchantment_amount,
            Some(Enchantment::Goopy) => card.enchantment_amount.saturating_sub(1),
            _ => 0,
        });
    let mut amount = game.modified_block(
        content,
        Actor::Player,
        base.saturating_add(fasten).saturating_add(enchantment),
        powered,
    );
    if context
        .card
        .is_some_and(|id| content.cards[id as usize].tags & MINION_TAG != 0)
        && relic("RELIC.VITRUVIAN_MINION")
    {
        amount = amount.saturating_mul(2);
    }
    let prior_card_blocks = combat.history.block_gains
        - if combat.history.block_card == combat.history.cards {
            combat.history.block_card_gains
        } else {
            0
        };
    if context.card.is_some() && prior_card_blocks == 0 && relic("RELIC.VAMBRACE") {
        amount = amount.saturating_mul(2);
    }
    if context.card.is_some() && combat.paels_legion == 0 && relic("RELIC.PAELS_LEGION") {
        amount = amount.saturating_mul(2);
    }
    amount
}

fn preview_gain_block(
    game: &Game,
    content: &Content,
    preview: &mut CardPreview,
    amount: i16,
    events: i16,
) {
    if amount > 0 && events > 0 {
        preview.block = preview.block.saturating_add(amount.saturating_mul(events));
        let powers = game
            .combat()
            .map_or_else(Vec::new, |combat| combat.player.powers.clone());
        for _ in 0..events {
            preview_hooks(
                game,
                content,
                Trigger::BlockGained,
                Actor::Player,
                Context::player(),
                &powers,
                preview,
            );
        }
    }
}

fn preview_target(target: Target, context: Context) -> Option<PreviewTarget> {
    match target {
        Target::Source => match context.source {
            Actor::Enemy(target) => Some(PreviewTarget::Fixed(target)),
            _ => None,
        },
        Target::ChosenEnemy | Target::ChosenEnemyOrDead => Some(PreviewTarget::Chosen),
        Target::RandomEnemy => Some(PreviewTarget::Random),
        Target::LowestHpEnemy => Some(PreviewTarget::Lowest),
        Target::AllEnemies => Some(PreviewTarget::All),
        Target::OtherEnemies => Some(PreviewTarget::Other),
        _ => None,
    }
}

fn push_preview_hit(
    game: &Game,
    context: Context,
    mut target: PreviewTarget,
    damage: PreviewDamage,
    amount: i16,
    count: i16,
    preview: &mut CardPreview,
) {
    if target == PreviewTarget::Chosen
        && context.card == Some(card_id::SOVEREIGN_BLADE)
        && game
            .combat()
            .is_some_and(|combat| combat.player.power(power_id::SEEKING_EDGE) > 0)
    {
        target = PreviewTarget::All;
    }
    let card = game.combat().and_then(|combat| {
        combat
            .auto_plays
            .last()
            .map(|play| play.card)
            .or(combat.playing)
            .filter(|card| Some(card.id) == context.card)
    });
    let player_attack = matches!(damage, PreviewDamage::Attack(Actor::Player));
    let vigor = if player_attack { preview.vigor } else { 0 };
    let gigantification = player_attack && preview.gigantification;
    preview.hits.push(PreviewHit {
        target,
        damage,
        amount,
        count,
        card,
        pen_nib: context.pen_nib,
        vigor,
        lethality: player_attack
            && game
                .combat()
                .is_some_and(|combat| combat.history.attacks == 1),
        gigantification,
    });
    if player_attack {
        preview.vigor = 0;
        preview.gigantification = false;
    }
}

fn draw_forbidden(game: &Game, content: &Content, hand_draw: bool) -> bool {
    game.combat().is_some_and(|combat| {
        combat.player.kind(content, PowerKind::NoDraw) > 0
            || !hand_draw
                && !combat.enemy_turn
                && game
                    .run
                    .relics
                    .iter()
                    .any(|&id| content.relics[id as usize].id == "RELIC.FIDDLE")
    })
}

fn draw_blocked(game: &Game, content: &Content, hand_draw: bool) -> bool {
    game.combat().is_some_and(|combat| combat.hand.len() >= 10)
        || draw_forbidden(game, content, hand_draw)
}

fn preview_draw_count(
    game: &Game,
    content: &Content,
    requested: i16,
    hand_draw: bool,
    preview: &mut CardPreview,
) -> Option<i16> {
    if game.combat().is_none() {
        return Some(requested.max(0));
    }
    if draw_forbidden(game, content, hand_draw) {
        return Some(0);
    }
    let wanted = requested.max(0).min(preview.draw_available);
    if wanted > (10 - preview.hand).max(0)
        && game
            .combat()
            .is_some_and(|combat| combat.player.power(power_id::HELLRAISER) > 0)
    {
        return None;
    }
    let powers = game.combat().unwrap().player.powers.clone();
    let mut drawn = 0;
    for _ in 0..wanted {
        if preview.hand >= 10 || preview.draw_available <= 0 {
            break;
        }
        let card = game.combat().and_then(|combat| {
            (preview.draw_pile > 0 && preview.known_drawn < combat.known_draw_top)
                .then(|| {
                    combat
                        .draw
                        .get(combat.draw.len() - 1 - preview.known_drawn)
                        .copied()
                })
                .flatten()
        });
        if card.is_some() {
            preview.known_drawn += 1;
        }
        preview.draw_available -= 1;
        if preview.draw_pile > 0 {
            preview.draw_pile -= 1;
        } else {
            preview.draw_pile = preview.draw_available;
        }
        preview.hand += 1;
        drawn += 1;
        let context = card.map_or_else(Context::player, |card| Context {
            event: 1,
            ..Context::card(card)
        });
        preview_hooks(
            game,
            content,
            Trigger::CardDrawn,
            Actor::Player,
            context,
            &powers,
            preview,
        );
        if let Some(card) = card {
            for hook in content.cards[card.id as usize]
                .hooks
                .iter()
                .filter(|hook| hook.trigger == Trigger::CardDrawn)
            {
                preview_effects(game, content, context, hook.effects, 1, preview);
            }
        }
    }
    Some(drawn)
}

fn preview_exhaust(game: &Game, content: &Content, count: i16, preview: &mut CardPreview) {
    let powers = game
        .combat()
        .map_or_else(Vec::new, |combat| combat.player.powers.clone());
    for index in 0..count.max(0) {
        preview.exhaust = preview.exhaust.saturating_add(1);
        preview_hooks(
            game,
            content,
            Trigger::CardExhausted,
            Actor::Player,
            Context::player(),
            &powers,
            preview,
        );
        if game
            .run
            .relics
            .iter()
            .any(|&id| content.relics[id as usize].id == "RELIC.JOSS_PAPER")
            && (game.joss_paper as i16 + index + 1) % 5 == 0
            && let Some(drawn) = preview_draw_count(game, content, 1, false, preview)
        {
            preview.draw = preview.draw.saturating_add(drawn);
        }
    }
}

fn exact_draw_until_not(game: &Game, content: &Content, kind: CardType) -> Option<i16> {
    let combat = game.combat()?;
    if draw_blocked(game, content, false) {
        return Some(0);
    }
    let mut count = 0i16;
    let mut hand = combat.hand.len();
    let hellraiser = combat.player.power(power_id::HELLRAISER) > 0;
    for card in combat.draw.iter().rev().take(combat.known_draw_top) {
        if hand >= 10 {
            return Some(count);
        }
        count += 1;
        if !(hellraiser && content.cards[card.id as usize].tags & STRIKE_TAG != 0) {
            hand += 1;
        }
        if content.cards[card.id as usize].card_type != kind {
            return Some(count);
        }
    }
    (combat.known_draw_top == combat.draw.len() && combat.discard.is_empty()).then_some(count)
}

fn preview_move_to_hand(from: Pile, count: i16, preview: &mut CardPreview) {
    let count = count.max(0);
    let entered = count.min((10 - preview.hand).max(0));
    preview.draw = preview.draw.saturating_add(entered);
    preview.hand = preview.hand.saturating_add(entered);
    if matches!(from, Pile::Draw | Pile::Discard) {
        preview.draw_available = preview.draw_available.saturating_sub(entered);
    }
    if from == Pile::Draw {
        preview.draw_pile = preview.draw_pile.saturating_sub(count);
    }
}

fn preview_evoke(
    game: &Game,
    content: &Content,
    index: usize,
    remove: bool,
    preview: &mut CardPreview,
) {
    let Some(orb) = preview.orbs.get(index).copied() else {
        return;
    };
    if remove {
        preview.orbs.remove(index);
    }
    preview_effects(
        game,
        content,
        Context {
            event: orb.value,
            orb: true,
            orb_id: Some(orb.id),
            ..Context::player()
        },
        content.orbs[orb.id as usize].evoke,
        1,
        preview,
    );
}

fn preview_channel(game: &Game, content: &Content, id: Id, preview: &mut CardPreview) {
    if preview.orb_slots == 0 {
        preview.orb_slots = 1;
    }
    if preview.orbs.len() == preview.orb_slots {
        preview_evoke(game, content, 0, true, preview);
    }
    preview.orbs.push(Orb {
        id,
        value: content.orbs[id as usize].initial_value,
    });
    preview.orbs_channeled = preview.orbs_channeled.saturating_add(1);
    if preview.orbs_channeled == 7
        && game
            .run
            .relics
            .iter()
            .any(|&id| content.relics[id as usize].id == "RELIC.METRONOME")
    {
        push_preview_hit(
            game,
            Context::player(),
            PreviewTarget::All,
            PreviewDamage::Unpowered,
            30,
            1,
            preview,
        );
    }
}

fn preview_passive(game: &Game, content: &Content, index: usize, preview: &mut CardPreview) {
    let Some(orb) = preview.orbs.get(index).copied() else {
        return;
    };
    let def = content.orbs[orb.id as usize];
    let focus = game
        .combat()
        .map_or(0, |combat| combat.player.kind(content, PowerKind::Focus));
    let triggers = 1
        + (index == 0
            && game
                .run
                .relics
                .iter()
                .any(|&id| content.relics[id as usize].id == "RELIC.GOLD_PLATED_CABLES"))
            as usize;
    let mut contexts = Vec::with_capacity(triggers);
    for _ in 0..triggers {
        preview.orbs[index].value = preview.orbs[index]
            .value
            .saturating_add((def.passive_value + (def.focus_passive as i16 * focus)).max(0));
        let event = preview.orbs[index].value;
        if def.passive_decay == 0 || event.saturating_add(focus) > 0 {
            preview.orbs[index].value = event.saturating_sub(def.passive_decay).max(0);
        }
        contexts.push(Context {
            event,
            orb_id: Some(orb.id),
            ..Context::player()
        });
    }
    for context in contexts.into_iter().rev() {
        preview_effects(game, content, context, def.passive, 1, preview);
    }
}

fn preview_attack_total(
    game: &Game,
    content: &Content,
    context: Context,
    hit: PreviewHit,
) -> Option<i16> {
    let combat = game.combat()?;
    let targets = match hit.target {
        PreviewTarget::Chosen => vec![context.target?],
        PreviewTarget::Lowest => combat
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| enemy.creature.hp > 0)
            .min_by_key(|(_, enemy)| enemy.creature.hp)
            .map(|(target, _)| vec![target])
            .unwrap_or_default(),
        PreviewTarget::All => combat
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| enemy.creature.hp > 0)
            .map(|(target, _)| target)
            .collect(),
        PreviewTarget::Other => combat
            .enemies
            .iter()
            .enumerate()
            .filter(|(target, enemy)| Some(*target) != context.target && enemy.creature.hp > 0)
            .map(|(target, _)| target)
            .collect(),
        PreviewTarget::Fixed(target) => vec![target],
        PreviewTarget::Random => {
            let targets = combat
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, enemy)| enemy.creature.hp > 0)
                .map(|(target, _)| target)
                .collect::<Vec<_>>();
            if targets.len() != 1 {
                return None;
            }
            targets
        }
    };
    Some(targets.into_iter().fold(0i16, |total, target| {
        total.saturating_add(
            modified_preview_damage(game, content, hit, Some(target), false)
                .0
                .saturating_mul(hit.count.max(0)),
        )
    }))
}

fn preview_effects(
    game: &Game,
    content: &Content,
    context: Context,
    effects: &[Effect],
    repeats: i16,
    preview: &mut CardPreview,
) {
    for effect in effects {
        match *effect {
            Effect::Attack(target, amount, hits) => {
                if let Some(target) = preview_target(target, context) {
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::Attack(Actor::Player),
                        preview_attack(
                            game,
                            content,
                            Actor::Player,
                            preview_amount(game, content, context, amount),
                        ),
                        (hits as i16).saturating_mul(repeats),
                        preview,
                    );
                    if let Some(total) =
                        preview_attack_total(game, content, context, *preview.hits.last().unwrap())
                    {
                        preview.last_damage = total;
                    }
                }
            }
            Effect::AttackMany(target, amount, hits) => {
                if let Some(target) = preview_target(target, context) {
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::Attack(Actor::Player),
                        preview_attack(
                            game,
                            content,
                            Actor::Player,
                            preview_amount(game, content, context, amount),
                        ),
                        preview_amount(game, content, context, hits)
                            .max(0)
                            .saturating_mul(repeats),
                        preview,
                    );
                    if let Some(total) =
                        preview_attack_total(game, content, context, *preview.hits.last().unwrap())
                    {
                        preview.last_damage = total;
                    }
                }
            }
            Effect::OstyAttack(target, amount, hits) => {
                if let Some(target) = preview_target(target, context) {
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::Attack(Actor::Osty),
                        preview_attack(
                            game,
                            content,
                            Actor::Osty,
                            preview_amount(game, content, context, amount),
                        ),
                        (hits as i16).saturating_mul(repeats),
                        preview,
                    );
                }
            }
            Effect::OstyAttackMany(target, amount, hits) => {
                if let Some(target) = preview_target(target, context) {
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::Attack(Actor::Osty),
                        preview_attack(
                            game,
                            content,
                            Actor::Osty,
                            preview_amount(game, content, context, amount),
                        ),
                        preview_amount(game, content, context, hits)
                            .max(0)
                            .saturating_mul(repeats),
                        preview,
                    );
                }
            }
            Effect::Block(Target::Player, amount) | Effect::Block(Target::Source, amount)
                if context.source == Actor::Player =>
            {
                let base = preview_effect_amount(game, content, context, amount, preview);
                preview_gain_block(
                    game,
                    content,
                    preview,
                    preview_block(game, content, context, base, true),
                    repeats,
                );
            }
            Effect::RawBlock(Target::Player, amount) | Effect::RawBlock(Target::Source, amount)
                if context.source == Actor::Player =>
            {
                preview_gain_block(
                    game,
                    content,
                    preview,
                    preview_amount(game, content, context, amount).max(0),
                    repeats,
                )
            }
            Effect::DodgeRoll(amount) => {
                let base = preview_amount(game, content, context, amount);
                preview_gain_block(
                    game,
                    content,
                    preview,
                    preview_block(game, content, context, base, true),
                    repeats,
                );
            }
            Effect::DrawBlockIf(kind, amount) => {
                let drawn = preview_draw_count(game, content, 1, false, preview).unwrap_or(0);
                preview.draw = preview.draw.saturating_add(drawn.saturating_mul(repeats));
                let known = game.combat().and_then(|combat| {
                    if combat.known_draw_top > 0 {
                        combat.draw.last().copied()
                    } else if combat.draw.is_empty()
                        && combat.discard.first().is_some_and(|first| {
                            combat.discard.iter().all(|card| {
                                content.cards[card.id as usize].card_type
                                    == content.cards[first.id as usize].card_type
                            })
                        })
                    {
                        combat.discard.first().copied()
                    } else {
                        None
                    }
                });
                if drawn == 1
                    && known.is_some_and(|card| content.cards[card.id as usize].card_type == kind)
                {
                    let base = preview_amount(game, content, context, amount);
                    preview_gain_block(
                        game,
                        content,
                        preview,
                        preview_block(game, content, context, base, true),
                        repeats,
                    );
                }
            }
            Effect::Draw(count) | Effect::HandDraw(count) => {
                let hand_draw = matches!(*effect, Effect::HandDraw(_));
                if let Some(count) =
                    preview_draw_count(game, content, count as i16, hand_draw, preview)
                {
                    preview.draw = preview.draw.saturating_add(count.saturating_mul(repeats));
                }
            }
            Effect::ChooseRandomDraw(count) => {
                let selected = if count > 0 && game.combat().is_some() {
                    repeats.max(0).min(preview.draw_pile)
                } else {
                    0
                };
                let drawn = selected.min((10 - preview.hand).max(0));
                preview.draw = preview.draw.saturating_add(drawn);
                preview.discard = preview.discard.saturating_add(selected - drawn);
                preview.hand = preview.hand.saturating_add(drawn);
                preview.draw_pile -= selected;
                preview.draw_available = preview.draw_available.saturating_sub(drawn);
            }
            Effect::DrawAmount(amount) | Effect::ChooseDraw(amount) => {
                let count = preview_amount(game, content, context, amount).max(0);
                if let Some(count) = preview_draw_count(game, content, count, false, preview) {
                    preview.draw = preview.draw.saturating_add(count.saturating_mul(repeats));
                }
            }
            Effect::DrawTo(count) => {
                let count = (count[context.upgraded as usize] as i16 - preview.hand).max(0);
                if let Some(count) = preview_draw_count(game, content, count, false, preview) {
                    preview.draw = preview.draw.saturating_add(count.saturating_mul(repeats));
                }
            }
            Effect::DrawUntilNot(kind) => {
                if let Some(count) = exact_draw_until_not(game, content, kind) {
                    if let Some(drawn) = preview_draw_count(
                        game,
                        content,
                        count.saturating_mul(repeats),
                        false,
                        preview,
                    ) {
                        preview.draw = preview.draw.saturating_add(drawn);
                    }
                }
            }
            Effect::DrawFiltered(count, filter) => {
                if let Some(count) = preview_draw_count(game, content, count as i16, false, preview)
                {
                    preview.draw = preview.draw.saturating_add(count.saturating_mul(repeats));
                    if let Some(combat) = game.combat()
                        && count as usize <= combat.known_draw_top
                    {
                        let hellraiser = combat.player.power(power_id::HELLRAISER) > 0;
                        let discarded = combat
                            .draw
                            .iter()
                            .rev()
                            .take(count as usize)
                            .filter(|card| {
                                if hellraiser
                                    && content.cards[card.id as usize].tags & STRIKE_TAG != 0
                                {
                                    return false;
                                }
                                let mut marked = **card;
                                marked.turn_flags |= FILTERED_DRAW;
                                !crate::game::eligible(
                                    content,
                                    &&marked,
                                    filter,
                                    CardOp::Move(Pile::Discard),
                                )
                            })
                            .count() as i16;
                        preview.discard = preview
                            .discard
                            .saturating_add(discarded.saturating_mul(repeats));
                        preview.hand = preview
                            .hand
                            .saturating_sub(discarded.saturating_mul(repeats));
                    }
                }
            }
            Effect::Discard(count, _) => {
                let count = (count as i16).min(preview.hand).saturating_mul(repeats);
                preview.discard = preview.discard.saturating_add(count);
                preview.hand = preview.hand.saturating_sub(count);
            }
            Effect::DiscardHandDraw => {
                let count = preview.hand;
                preview.discard = preview.discard.saturating_add(count);
                preview.hand = 0;
                preview.draw_available = preview.draw_available.saturating_add(count);
                if let Some(drawn) = preview_draw_count(game, content, count, false, preview) {
                    preview.draw = preview.draw.saturating_add(drawn);
                }
            }
            Effect::DiscardHandAdd(_) => {
                preview.discard = preview.discard.saturating_add(preview.hand * repeats);
                preview.hand = 0;
            }
            Effect::Exhaust(count, _) => {
                let count = (count as i16).min(preview.hand).saturating_mul(repeats);
                preview_exhaust(game, content, count, preview);
                preview.hand = preview.hand.saturating_sub(count);
            }
            Effect::ExhaustForBlock(amount) => {
                let count = game.combat().map_or(0, |combat| {
                    combat
                        .hand
                        .iter()
                        .filter(|card| {
                            content.cards[card.id as usize].card_type != CardType::Attack
                        })
                        .count()
                        .saturating_sub(
                            (!preview.playing_removed
                                && context.card.is_some_and(|id| {
                                    content.cards[id as usize].card_type != CardType::Attack
                                })) as usize,
                        ) as i16
                });
                let base = preview_amount(game, content, context, amount);
                let block = preview_block(game, content, context, base, true);
                preview_gain_block(game, content, preview, block, count.saturating_mul(repeats));
                preview_exhaust(game, content, count.saturating_mul(repeats), preview);
                preview.hand = preview.hand.saturating_sub(count.saturating_mul(repeats));
            }
            Effect::ExhaustForAttack(target, amount) => {
                let hand = preview.hand;
                if let Some(target) = preview_target(target, context) {
                    for _ in 0..hand.saturating_mul(repeats) {
                        push_preview_hit(
                            game,
                            context,
                            target,
                            PreviewDamage::Attack(Actor::Player),
                            preview_attack(
                                game,
                                content,
                                Actor::Player,
                                preview_amount(game, content, context, amount),
                            ),
                            1,
                            preview,
                        );
                    }
                }
                preview_exhaust(game, content, hand.saturating_mul(repeats), preview);
                preview.hand = 0;
            }
            Effect::RecycleHand(draw) => {
                if !draw_forbidden(game, content, false) {
                    preview.draw_available = preview.draw_available.saturating_add(preview.hand);
                    preview.hand = 0;
                    if let Some(drawn) = preview_draw_count(
                        game,
                        content,
                        draw[context.upgraded as usize] as i16,
                        false,
                        preview,
                    ) {
                        preview.draw = preview.draw.saturating_add(drawn.saturating_mul(repeats));
                    }
                }
            }
            Effect::ShuffleHandDraw(draw) => {
                if !draw_forbidden(game, content, false) {
                    preview.draw_available = preview.draw_available.saturating_add(preview.hand);
                    preview.hand = 0;
                    if let Some(drawn) =
                        preview_draw_count(game, content, draw as i16, false, preview)
                    {
                        preview.draw = preview.draw.saturating_add(drawn.saturating_mul(repeats));
                    }
                }
            }
            Effect::AutoPlayDraw(amount, _) => {
                let available = game.combat().map_or(0, |combat| {
                    combat
                        .draw
                        .len()
                        .saturating_add(combat.discard.len())
                        .min(i16::MAX as usize) as i16
                });
                let drawn = preview_amount(game, content, context, amount)
                    .max(0)
                    .min(available)
                    .min(preview.draw_available)
                    .saturating_mul(repeats);
                preview.draw = preview.draw.saturating_add(drawn);
                preview.draw_available = preview.draw_available.saturating_sub(drawn);
            }
            Effect::FlakCannon(amount) => {
                let count = std::mem::take(&mut preview.statuses);
                preview_exhaust(game, content, count, preview);
                if count > 0 {
                    push_preview_hit(
                        game,
                        context,
                        PreviewTarget::Random,
                        PreviewDamage::Attack(Actor::Player),
                        preview_attack(
                            game,
                            content,
                            Actor::Player,
                            preview_amount(game, content, context, amount),
                        ),
                        count,
                        preview,
                    );
                }
            }
            Effect::MoveDamage(target, amount) => {
                if let Some(target) = preview_target(target, context) {
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::Move(Actor::Player),
                        preview_attack(
                            game,
                            content,
                            Actor::Player,
                            preview_amount(game, content, context, amount),
                        ),
                        repeats,
                        preview,
                    );
                }
            }
            Effect::Damage(target, amount) => {
                if let Some(target) = preview_target(target, context) {
                    let amount =
                        preview_amount(game, content, context, amount).saturating_add(
                            (context.orb_id == Some(orb_id::LIGHTNING)
                                && game.run.relics.iter().any(|&id| {
                                    content.relics[id as usize].id == "RELIC.INFUSED_CORE"
                                })) as i16,
                        );
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::Unpowered,
                        amount.max(0),
                        repeats,
                        preview,
                    );
                }
            }
            Effect::Stars(amount) => {
                let stars = preview_amount(game, content, context, amount);
                let damage = game
                    .combat()
                    .map_or(0, |combat| combat.player.power(power_id::BLACK_HOLE));
                if stars > 0 && damage > 0 {
                    push_preview_hit(
                        game,
                        context,
                        PreviewTarget::All,
                        PreviewDamage::Unpowered,
                        damage,
                        repeats,
                        preview,
                    );
                }
            }
            Effect::ApplyPower(Target::AllEnemies, id, amount) if id == power_id::DOOM => {
                preview.doom = preview.doom.saturating_add(
                    preview_amount(game, content, context, amount).saturating_mul(repeats),
                );
            }
            Effect::DoomKill => push_preview_hit(
                game,
                context,
                PreviewTarget::All,
                PreviewDamage::Doom(preview.doom),
                0,
                repeats,
                preview,
            ),
            Effect::Misery(amount) => {
                if let Some(target) = preview_target(Target::ChosenEnemy, context) {
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::Attack(Actor::Player),
                        preview_attack(
                            game,
                            content,
                            Actor::Player,
                            preview_amount(game, content, context, amount),
                        ),
                        repeats,
                        preview,
                    );
                }
            }
            Effect::LoseHp(target, amount) => {
                if let Some(target) = preview_target(target, context) {
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::LoseHp,
                        preview_amount(game, content, context, amount).max(0),
                        repeats,
                        preview,
                    );
                }
            }
            Effect::MoveAll(pile, filter, to) => {
                let count = game
                    .combat()
                    .map_or(0, |combat| {
                        let cards = match pile {
                            Pile::Draw => &combat.draw,
                            Pile::Hand => &combat.hand,
                            Pile::Discard => &combat.discard,
                            Pile::Exhaust => &combat.exhaust,
                            Pile::Offer => &combat.offer,
                        };
                        cards
                            .iter()
                            .filter(|card| {
                                crate::game::eligible(content, card, filter, CardOp::Move(to))
                            })
                            .count()
                            .saturating_sub(
                                (pile == Pile::Hand
                                    && !preview.playing_removed
                                    && context.card.is_some_and(|id| {
                                        cards.iter().any(|card| {
                                            card.id == id
                                                && crate::game::eligible(
                                                    content,
                                                    &card,
                                                    filter,
                                                    CardOp::Move(to),
                                                )
                                        })
                                    })) as usize,
                            ) as i16
                    })
                    .saturating_mul(repeats);
                match to {
                    Pile::Hand => preview_move_to_hand(pile, count, preview),
                    Pile::Discard => preview.discard = preview.discard.saturating_add(count),
                    Pile::Exhaust => preview_exhaust(game, content, count, preview),
                    Pile::Draw | Pile::Offer => {}
                }
            }
            Effect::Select(pile, filter, count, _, _, op) => {
                let available = game.combat().map_or(0, |combat| {
                    let cards = match pile {
                        Pile::Draw => &combat.draw,
                        Pile::Hand => &combat.hand,
                        Pile::Discard => &combat.discard,
                        Pile::Exhaust => &combat.exhaust,
                        Pile::Offer => &combat.offer,
                    };
                    cards
                        .iter()
                        .filter(|card| crate::game::eligible(content, card, filter, op))
                        .count()
                        .saturating_sub(
                            (pile == Pile::Hand
                                && !preview.playing_removed
                                && context.card.is_some_and(|id| {
                                    cards.iter().any(|card| {
                                        card.id == id
                                            && crate::game::eligible(content, &card, filter, op)
                                    })
                                })) as usize,
                        ) as i16
                });
                let count = (count[context.upgraded as usize] as i16)
                    .min(available)
                    .saturating_mul(repeats);
                match op {
                    CardOp::Move(Pile::Hand) => preview_move_to_hand(pile, count, preview),
                    CardOp::Move(Pile::Discard) => {
                        preview.discard = preview.discard.saturating_add(count)
                    }
                    CardOp::Move(Pile::Exhaust) => preview_exhaust(game, content, count, preview),
                    _ => {}
                }
            }
            Effect::SelectAmount(pile, filter, amount, _, op) => {
                let available = game.combat().map_or(0, |combat| {
                    let cards = match pile {
                        Pile::Draw => &combat.draw,
                        Pile::Hand => &combat.hand,
                        Pile::Discard => &combat.discard,
                        Pile::Exhaust => &combat.exhaust,
                        Pile::Offer => &combat.offer,
                    };
                    cards
                        .iter()
                        .filter(|card| crate::game::eligible(content, card, filter, op))
                        .count() as i16
                });
                let count = preview_amount(game, content, context, amount)
                    .max(0)
                    .min(available)
                    .saturating_mul(repeats);
                match op {
                    CardOp::Move(Pile::Hand) => preview_move_to_hand(pile, count, preview),
                    CardOp::Move(Pile::Discard) => {
                        preview.discard = preview.discard.saturating_add(count)
                    }
                    CardOp::Move(Pile::Exhaust) => preview_exhaust(game, content, count, preview),
                    _ => {}
                }
            }
            Effect::If(condition, yes, no) => {
                let applies = match condition {
                    Condition::TargetDead => context.target.is_some_and(|target| {
                        game.combat().is_some_and(|combat| {
                            combat.enemies.get(target).is_some_and(|enemy| {
                                enemy.creature.hp <= 0
                                    || resolved_hits(
                                        game,
                                        content,
                                        target,
                                        Some(target),
                                        false,
                                        false,
                                        &preview.hits,
                                    )
                                    .2 >= enemy.creature.hp
                            })
                        })
                    }),
                    _ => game.combat().map_or(
                        matches!(condition, Condition::Always)
                            || matches!(condition, Condition::Upgraded) && context.upgraded,
                        |_| game.condition(content, context, condition),
                    ),
                };
                preview_effects(
                    game,
                    content,
                    context,
                    if applies { yes } else { no },
                    repeats,
                    preview,
                );
            }
            Effect::Repeat(amount, nested) => {
                for _ in
                    0..repeats.saturating_mul(preview_amount(game, content, context, amount).max(0))
                {
                    preview_effects(game, content, context, nested, 1, preview);
                }
            }
            Effect::ChannelSlots(id) => {
                for _ in 0..preview.orb_slots {
                    preview_channel(game, content, id, preview);
                }
            }
            Effect::Channel(id, count) => {
                for _ in 0..(count as i16).saturating_mul(repeats).max(0) {
                    preview_channel(game, content, id, preview);
                }
            }
            Effect::Evoke(dequeue) => {
                for _ in 0..repeats.max(0) {
                    preview_evoke(game, content, 0, dequeue, preview);
                }
            }
            Effect::EvokeMany(amount) => {
                for _ in 0..repeats.max(0) {
                    let count = preview_amount(game, content, context, amount).max(0);
                    for index in 0..count {
                        preview_evoke(game, content, 0, index == count - 1, preview);
                    }
                }
            }
            Effect::EvokeLast(amount) => {
                for _ in
                    0..repeats.saturating_mul(preview_amount(game, content, context, amount).max(0))
                {
                    let Some(last) = preview.orbs.len().checked_sub(1) else {
                        break;
                    };
                    preview_evoke(game, content, last, true, preview);
                }
            }
            Effect::EvokeAll(times) => {
                if times == 0 {
                    continue;
                }
                for _ in 0..repeats.max(0) {
                    while !preview.orbs.is_empty() {
                        for index in 0..times {
                            preview_evoke(game, content, 0, index == times - 1, preview);
                        }
                    }
                }
            }
            Effect::PassiveFirst(count) => {
                for _ in 0..(count as i16).saturating_mul(repeats).max(0) {
                    preview_passive(game, content, 0, preview);
                }
            }
            Effect::PassiveLast(count) => {
                for _ in 0..(count as i16).saturating_mul(repeats).max(0) {
                    let Some(last) = preview.orbs.len().checked_sub(1) else {
                        break;
                    };
                    preview_passive(game, content, last, preview);
                }
            }
            Effect::PassiveAll => {
                for _ in 0..repeats.max(0) {
                    for index in (0..preview.orbs.len()).rev() {
                        preview_passive(game, content, index, preview);
                    }
                }
            }
            Effect::OrbSlots(amount) => {
                preview.orb_slots =
                    (preview.orb_slots as i16 + amount as i16).clamp(0, 10) as usize;
                while preview.orbs.len() > preview.orb_slots {
                    preview_evoke(game, content, 0, true, preview);
                }
            }
            _ => {}
        }
    }
}

fn preview_hooks(
    game: &Game,
    content: &Content,
    trigger: Trigger,
    owner: Actor,
    mut context: Context,
    powers: &[Power],
    preview: &mut CardPreview,
) {
    if owner == Actor::Player {
        for (position, &id) in game.run.relics.iter().enumerate() {
            if game.melted_relics.contains(&position) {
                continue;
            }
            for hook in content.relics[id as usize]
                .hooks
                .iter()
                .filter(|hook| hook.trigger == trigger)
            {
                preview_effects(game, content, context, hook.effects, 1, preview);
            }
        }
    }
    for power in powers {
        if matches!(
            trigger,
            Trigger::PowerPlayed | Trigger::CardDrawn | Trigger::CardGenerated
        ) {
            context.event = power.amount;
        }
        for hook in content.powers[power.id as usize]
            .hooks
            .iter()
            .filter(|hook| hook.trigger == trigger)
        {
            preview_effects(game, content, context, hook.effects, 1, preview);
        }
    }
}

fn preview_card_effects(
    game: &Game,
    content: &Content,
    card: Card,
    contexts: &[Context],
    mut preview: CardPreview,
) -> CardPreview {
    let def = content.cards[card.id as usize];
    let mut dynamic;
    let effects = if def.id == "CARD.SHIV"
        && game
            .combat()
            .is_some_and(|combat| combat.player.power(power_id::FAN_OF_KNIVES) > 0)
    {
        dynamic = vec![Effect::Attack(Target::AllEnemies, Amount::fixed(4, 6), 1)];
        dynamic.as_slice()
    } else if def.id == "CARD.MAD_SCIENCE" {
        let rider = card.variant % 10;
        dynamic = match card.card_type(def) {
            CardType::Attack => vec![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(12, 12),
                if rider == 2 { 3 } else { 1 },
            )],
            CardType::Skill => vec![Effect::Block(Target::Player, Amount::fixed(8, 8))],
            _ => vec![],
        };
        if rider == 5 {
            dynamic.push(Effect::Draw(3));
        }
        dynamic.as_slice()
    } else {
        def.effects
    };
    for &context in contexts {
        preview_effects(game, content, context, effects, 1, &mut preview);
    }
    preview
}

fn card_preview(game: &Game, content: &Content, card: Card, target: Option<usize>) -> CardPreview {
    let def = content.cards[card.id as usize];
    let (x, stars) = game.combat().map_or((0, 0), |combat| {
        (
            if def.cost[card.upgrades.min(1) as usize] < 0 {
                combat.energy
            } else {
                0
            },
            crate::game::star_cost(combat, card, def),
        )
    });
    preview_card_effects(
        game,
        content,
        card,
        &[Context {
            target,
            x,
            event: stars,
            ..Context::card(card)
        }],
        CardPreview::new(game, content),
    )
}

fn played_card_preview(
    game: &Game,
    content: &Content,
    card: Card,
    target: Option<usize>,
) -> CardPreview {
    let Some(original) = game.combat() else {
        return card_preview(game, content, card, target);
    };
    let def = content.cards[card.id as usize];
    let card_type = card.card_type(def);
    let relic = |id| {
        game.run
            .relics
            .iter()
            .any(|&relic| content.relics[relic as usize].id == id)
    };
    let chemical_x = relic("RELIC.CHEMICAL_X");
    let gauntlets = relic("RELIC.SPIKED_GAUNTLETS");
    let throwing_axe = relic("RELIC.THROWING_AXE") && !original.throwing_axe;
    let helmet = relic("RELIC.INTIMIDATING_HELMET");
    let scarf = relic("RELIC.BRILLIANT_SCARF") && original.history.manual_plays == 4;
    let mut projected = game.clone();
    let Phase::Combat(combat) = &mut projected.phase else {
        unreachable!()
    };
    if let Some(index) = combat.hand.iter().position(|&candidate| candidate == card) {
        combat.hand.remove(index);
    }
    combat.playing = Some(card);
    let void_free = combat.history.manual_cards < combat.player.power(power_id::VOID_FORM);
    let free = match card_type {
        _ if void_free => None,
        CardType::Power if combat.player.power(power_id::FREE_POWER) > 0 => {
            Some(power_id::FREE_POWER)
        }
        CardType::Skill if combat.player.power(power_id::FREE_SKILL) > 0 => {
            Some(power_id::FREE_SKILL)
        }
        _ => None,
    };
    let cost = if scarf {
        0
    } else {
        crate::game::energy_cost(combat, card, def, gauntlets)
    };
    if let Some(power) = free {
        combat.player.consume_power(power);
    }
    if card.flags(def) & ETHEREAL != 0 && combat.player.power(power_id::VEILPIERCER) > 0 {
        combat.player.consume_power(power_id::VEILPIERCER);
    }
    if card_type == CardType::Attack && combat.player.power(power_id::FREE_ATTACK) > 0 {
        combat.player.consume_power(power_id::FREE_ATTACK);
    }
    let x_cost = def.cost[card.upgrades.min(1) as usize] < 0;
    let x = (if !void_free && x_cost {
        combat.energy
    } else {
        0
    }) + 2 * (x_cost && chemical_x) as i16;
    combat.energy -= cost;
    crate::game::trigger_orbit(combat, cost);
    let throne = combat.player.power(power_id::THE_SEALED_THRONE);
    if throne > 0 {
        combat.stars = combat.stars.saturating_add(throne);
        combat.history.stars_gained = combat.history.stars_gained.saturating_add(throne);
    }
    let stars = if scarf {
        0
    } else {
        crate::game::star_cost(combat, card, def)
    };
    combat.stars -= stars;
    let burst = card_type == CardType::Skill && combat.player.power(power_id::BURST) > 0;
    let punch = card_type == CardType::Attack && combat.player.power(power_id::ONE_TWO_PUNCH) > 0;
    let duplicate = (combat.player.power(power_id::DUPLICATION) > 0) as u8;
    let plays = 1
        + throwing_axe as u8
        + card.replays
        + matches!(card.enchantment, Some(Enchantment::Spiral)) as u8
        + (card.enchantment == Some(Enchantment::Glam) && card.enchantment_value > 0) as u8
        + (combat.player.power(power_id::ECHO_FORM) > 0) as u8
        + (combat.player.power(power_id::SIGNAL_BOOST) > 0) as u8
        + burst as u8
        + punch as u8
        + duplicate
        + if card.id == card_id::SOVEREIGN_BLADE {
            combat.player.power(power_id::SWORD_SAGE).max(0) as u8
        } else {
            0
        };
    for power in [
        burst.then_some(power_id::BURST),
        punch.then_some(power_id::ONE_TWO_PUNCH),
        (duplicate > 0).then_some(power_id::DUPLICATION),
        (combat.player.power(power_id::SIGNAL_BOOST) > 0).then_some(power_id::SIGNAL_BOOST),
    ]
    .into_iter()
    .flatten()
    {
        combat.player.consume_power(power);
    }
    combat.card_energy = cost;
    combat.card_stars = stars;
    combat.card_plays = plays;
    combat.history.energy += cost;
    combat.history.cards += plays as i16;
    combat.history.manual_cards += 1;
    combat.history.manual_plays += plays as i16;
    match card_type {
        CardType::Attack => combat.history.attacks += plays as i16,
        CardType::Skill => combat.history.skills += plays as i16,
        CardType::Power => combat.history.powers += plays as i16,
        _ => {}
    }
    if card.enchantment == Some(Enchantment::Sown) && card.enchantment_value > 0 {
        combat.energy = combat.energy.saturating_add(card.enchantment_amount);
        combat.playing.as_mut().unwrap().enchantment_value = 0;
    }
    let pen_nib = card_type == CardType::Attack && relic("RELIC.PEN_NIB");
    let mut counter = game.pen_nib;
    let contexts = (0..plays)
        .map(|_| {
            if pen_nib {
                counter = (counter + 1) % 10;
            }
            Context {
                target,
                x,
                event: stars,
                pen_nib: pen_nib && counter == 0,
                ..Context::card(card)
            }
        })
        .collect::<Vec<_>>();
    let child = combat.player.power(power_id::CHILD_OF_THE_STARS);
    let black_hole = combat.player.power(power_id::BLACK_HOLE);
    let danse = (cost >= 2)
        .then(|| combat.player.power(power_id::DANSE_MACABRE))
        .unwrap_or(0);
    let ash = (card.flags(def) & ETHEREAL != 0)
        .then(|| combat.player.power(power_id::SPIRIT_OF_ASH))
        .unwrap_or(0);
    let hit = PreviewHit {
        target: PreviewTarget::All,
        damage: PreviewDamage::Unpowered,
        amount: black_hole,
        count: 1,
        card: Some(card),
        pen_nib: false,
        vigor: 0,
        lethality: false,
        gigantification: false,
    };
    let mut preview = CardPreview::new(&projected, content);
    preview.playing_removed = true;
    preview_gain_block(
        &projected,
        content,
        &mut preview,
        4,
        plays as i16 * (cost >= 2 && helmet) as i16,
    );
    preview_gain_block(&projected, content, &mut preview, ash, 1);
    if danse > 0 {
        let block = preview_block(&projected, content, contexts[0], danse, true);
        preview_gain_block(&projected, content, &mut preview, block, 1);
    }
    if throne > 0 && black_hole > 0 {
        preview.hits.push(hit);
    }
    match card.enchantment {
        Some(Enchantment::Adroit) => {
            let block = preview_block(
                &projected,
                content,
                contexts[0],
                card.enchantment_amount,
                true,
            );
            preview_gain_block(&projected, content, &mut preview, block, 1);
        }
        Some(Enchantment::Swift) if card.enchantment_value > 0 => {
            if let Some(draw) = preview_draw_count(
                &projected,
                content,
                card.enchantment_amount.max(0),
                false,
                &mut preview,
            ) {
                preview.draw = preview.draw.saturating_add(draw);
            }
        }
        _ => {}
    }
    let mut preview = preview_card_effects(&projected, content, card, &contexts, preview);
    preview_gain_block(
        &projected,
        content,
        &mut preview,
        child.saturating_mul(stars),
        1,
    );
    if stars > 0 && black_hole > 0 {
        preview.hits.push(hit);
    }
    let old_attacks = original.history.attacks;
    let old_skills = original.history.skills;
    let attack_triggers = (card_type == CardType::Attack)
        .then_some((old_attacks + plays as i16) / 3 - old_attacks / 3)
        .unwrap_or(0);
    let skill_triggers = (card_type == CardType::Skill)
        .then_some((old_skills + plays as i16) / 3 - old_skills / 3)
        .unwrap_or(0);
    if relic("RELIC.ORNAMENTAL_FAN") {
        preview_gain_block(&projected, content, &mut preview, 4, attack_triggers);
    }
    let kusarigama = (card_type == CardType::Attack && relic("RELIC.KUSARIGAMA"))
        .then_some((original.kusarigama as i16 + plays as i16) / 3)
        .unwrap_or(0);
    if kusarigama > 0 {
        push_preview_hit(
            &projected,
            Context::player(),
            PreviewTarget::Random,
            PreviewDamage::Unpowered,
            6,
            kusarigama,
            &mut preview,
        );
    }
    if skill_triggers > 0 && relic("RELIC.LETTER_OPENER") {
        push_preview_hit(
            &projected,
            Context::player(),
            PreviewTarget::All,
            PreviewDamage::Unpowered,
            5,
            skill_triggers,
            &mut preview,
        );
    }
    if relic("RELIC.IRON_CLUB") {
        let count = (game.iron_club as i16 + plays as i16) / 4;
        if let Some(drawn) = preview_draw_count(&projected, content, count, false, &mut preview) {
            preview.draw = preview.draw.saturating_add(drawn);
        }
    }
    if card_type == CardType::Skill && relic("RELIC.TUNING_FORK") {
        let triggers = (game.tuning_fork as i16 + plays as i16) / 10;
        preview_gain_block(
            &projected,
            content,
            &mut preview,
            projected.modified_block(content, Actor::Player, 7, false),
            triggers,
        );
    }
    let trigger = match card_type {
        CardType::Attack => Trigger::AttackPlayed,
        CardType::Skill => Trigger::SkillPlayed,
        CardType::Power => Trigger::PowerPlayed,
        _ => Trigger::CardPlayed,
    };
    let context = Context {
        event: 1,
        ..Context::card(card)
    };
    for _ in contexts.iter().rev() {
        for (target, enemy) in original.enemies.iter().enumerate().rev() {
            preview_hooks(
                &projected,
                content,
                Trigger::CardPlayed,
                Actor::Enemy(target),
                Context {
                    source: Actor::Enemy(target),
                    target: Some(target),
                    ..context
                },
                &enemy.creature.powers,
                &mut preview,
            );
        }
        preview_hooks(
            &projected,
            content,
            Trigger::CardPlayed,
            Actor::Player,
            context,
            &original.player.powers,
            &mut preview,
        );
        if trigger != Trigger::CardPlayed {
            preview_hooks(
                &projected,
                content,
                trigger,
                Actor::Player,
                context,
                &original.player.powers,
                &mut preview,
            );
        }
    }
    let mut flags = card.flags(def);
    if matches!(card_type, CardType::Attack | CardType::Skill)
        && original.history.attacks + original.history.skills
            < original.player.power(power_id::NOSTALGIA)
    {
        flags |= RETURN_TO_DRAW;
    }
    let returned = card_type == CardType::Attack
        && cost == 0
        && original.history.feral_returns < original.player.power(power_id::FERAL)
        || flags & (RETURN_TO_HAND | RETURN_TO_DRAW) != 0;
    if !returned
        && (flags & EXHAUST != 0
            || card_type == CardType::Skill && original.player.power(power_id::CORRUPTION) > 0)
    {
        preview_exhaust(&projected, content, 1, &mut preview);
    }
    preview.costs = Some((cost, stars));
    preview
}

fn modified_preview_damage(
    game: &Game,
    content: &Content,
    hit: PreviewHit,
    target: Option<usize>,
    flutter: bool,
) -> (i16, bool, i16) {
    let attack = matches!(hit.damage, PreviewDamage::Attack(_));
    let powered = matches!(
        hit.damage,
        PreviewDamage::Attack(_) | PreviewDamage::Move(_)
    );
    let source = match hit.damage {
        PreviewDamage::Attack(source) | PreviewDamage::Move(source) => Some(source),
        _ => None,
    };
    let Some(combat) = game.combat() else {
        return (hit.amount.max(0), attack, 0);
    };
    let relic = |id| {
        game.run
            .relics
            .iter()
            .any(|&relic| content.relics[relic as usize].id == id)
    };
    let mut damage = hit.amount.max(0);
    if powered && source == Some(Actor::Player) {
        match hit
            .card
            .and_then(|card| card.enchantment.map(|x| (card, x)))
        {
            Some((_, Enchantment::Corrupted)) => damage = damage.saturating_mul(3) / 2,
            Some((_, Enchantment::Instinct)) => damage = damage.saturating_mul(2),
            Some((card, Enchantment::Sharp)) => {
                damage = damage.saturating_add(card.enchantment_amount)
            }
            Some((card, Enchantment::Vigorous)) if card.enchantment_value > 0 => {
                damage = damage.saturating_add(card.enchantment_amount)
            }
            Some((card, Enchantment::Momentum)) => {
                damage = damage.saturating_add(card.enchantment_value)
            }
            Some((_, Enchantment::TezcatarasEmber)) => damage = damage.saturating_add(3),
            _ => {}
        }
        if let Some(card) = hit.card {
            damage = damage
                .saturating_add(3 * (card.upgrades > 0 && relic("RELIC.MINIATURE_CANNON")) as i16)
                .saturating_add(
                    9 * (card.enchantment.is_some() && relic("RELIC.MYSTIC_LIGHTER")) as i16,
                );
        }
    }
    let source_creature = source.map(|source| match source {
        Actor::Osty => &combat.osty,
        _ => &combat.player,
    });
    if attack {
        if source == Some(Actor::Player)
            && hit
                .card
                .is_some_and(|card| content.cards[card.id as usize].tags & SHIV_TAG != 0)
        {
            damage = damage.saturating_add(combat.player.power(power_id::ACCURACY));
            if combat.history.shivs == 0 {
                damage = damage.saturating_add(combat.player.power(power_id::PHANTOM_BLADES));
            }
        }
        if source == Some(Actor::Player)
            && hit
                .card
                .is_some_and(|card| card.flags(content.cards[card.id as usize]) & INKY != 0)
        {
            damage = damage.saturating_add(1);
        }
        if source == Some(Actor::Player) {
            damage = damage.saturating_add(hit.vigor);
        }
        if let Some(target) = target
            && hit.card.is_some_and(|card| card.id == card_id::HANG)
        {
            damage =
                damage.saturating_mul(combat.enemies[target].creature.power(power_id::HANG).max(1));
        }
        if hit.lethality {
            damage = damage.saturating_mul(100 + combat.player.power(power_id::LETHALITY)) / 100;
        }
        if source == Some(Actor::Osty) {
            damage = damage.saturating_add(combat.player.power(power_id::CALCIFY));
        }
    }
    if let Some(source) = source_creature
        && powered
    {
        if attack
            && matches!(
                hit.damage,
                PreviewDamage::Attack(Actor::Player | Actor::Osty)
            )
            && hit
                .card
                .is_some_and(|card| content.cards[card.id as usize].tags & STRIKE_TAG != 0)
        {
            damage = damage
                .saturating_add(3 * relic("RELIC.STRIKE_DUMMY") as i16)
                .saturating_add(relic("RELIC.FAKE_STRIKE_DUMMY") as i16);
        }
        damage = damage.saturating_add(source.kind(content, PowerKind::Strength));
        if matches!(
            hit.damage,
            PreviewDamage::Attack(Actor::Player | Actor::Osty)
        ) && hit
            .card
            .is_some_and(|card| content.cards[card.id as usize].tags & MINION_TAG != 0)
            && relic("RELIC.VITRUVIAN_MINION")
        {
            damage = damage.saturating_mul(2);
        }
        if attack && hit.pen_nib {
            damage = damage.saturating_mul(2);
        }
        if let Some(target) = target {
            let enemy = &combat.enemies[target].creature;
            if attack {
                damage = damage.saturating_add(enemy.power(power_id::TAINTED));
            }
            damage = vulnerable_damage(game, content, target, damage);
        }
        damage = weak_damage(
            game,
            content,
            match hit.damage {
                PreviewDamage::Attack(source) | PreviewDamage::Move(source) => source,
                _ => unreachable!(),
            },
            damage,
        );
        if let Some(target) = target {
            damage = damage
                .saturating_mul(100 + combat.enemies[target].creature.power(power_id::SLOW))
                / 100;
        }
        for _ in 0..source.power(power_id::DOUBLE_DAMAGE).max(0) {
            damage = damage.saturating_mul(2);
        }
        if source.power(power_id::SHRINK) != 0 {
            damage = damage.saturating_mul(70) / 100;
        }
    }
    let mut cap = 0;
    if let Some(target) = target {
        let enemy = &combat.enemies[target].creature;
        if attack && flutter {
            damage /= 2;
        }
        let hard = enemy.power(power_id::HARD_TO_KILL).max(0);
        if hard > 0 {
            damage = damage.min(hard);
        }
        if attack && hit.card.is_some() && enemy.power(power_id::SOAR) > 0 {
            damage /= 2;
        }
        if attack && source == Some(Actor::Player) && enemy.kind(content, PowerKind::Weak) > 0 {
            damage = damage.saturating_mul(combat.player.power(power_id::TRACKING).max(1));
        }
        if attack
            && source == Some(Actor::Player)
            && hit
                .card
                .is_some_and(|card| card.id == card_id::SOVEREIGN_BLADE)
            && enemy.power(power_id::CONQUEROR) > 0
        {
            damage = damage.saturating_mul(2);
        }
        if hit.gigantification {
            damage = damage.saturating_mul(3);
        }
        let intangible = (enemy.kind(content, PowerKind::Intangible) > 0) as i16;
        if intangible > 0 {
            damage = damage.min(1);
        }
        cap = if hard > 0 && intangible > 0 {
            hard.min(1)
        } else {
            hard.max(intangible)
        };
    } else if hit.gigantification {
        damage = damage.saturating_mul(3);
    }
    (damage.max(0), attack, cap)
}

fn preview_damage(game: &Game, content: &Content, hit: PreviewHit) -> i16 {
    modified_preview_damage(game, content, hit, None, false)
        .0
        .saturating_mul(hit.count)
}

fn card_token_v19(
    game: &Game,
    content: &Content,
    collection: usize,
    zone: usize,
    position: usize,
    card: Card,
    state: (usize, u16, i32),
    target: Option<usize>,
    played_preview: Option<&CardPreview>,
) -> Token {
    let def = content.cards[card.id as usize];
    let card_type = card.card_type(def);
    let mut out = token(collection, zone + 1, card.id as usize + 1, 1, position);
    out[5] = card_type as usize as f32 + 1.0;
    out[6] = card.enchantment.map_or(0, |value| value as usize + 1) as f32;
    out[7] = match card_type {
        CardType::Status => 1.0,
        CardType::Curse => 2.0,
        CardType::Quest => 3.0,
        _ => 0.0,
    };
    let combat = game.combat();
    let energy = combat.map_or(game.run.energy as i16, |combat| combat.energy);
    let effective_cost = combat.map_or_else(
        || crate::game::card_cost(card, def, energy),
        |combat| {
            let gauntlets = game
                .run
                .relics
                .iter()
                .any(|&id| content.relics[id as usize].id == "RELIC.SPIKED_GAUNTLETS");
            let scarf = game
                .run
                .relics
                .iter()
                .any(|&id| content.relics[id as usize].id == "RELIC.BRILLIANT_SCARF")
                && combat.history.manual_plays == 4;
            if scarf {
                0
            } else {
                crate::game::energy_cost(combat, card, def, gauntlets)
            }
        },
    );
    let fallback;
    let preview = if let Some(preview) = played_preview {
        preview
    } else {
        fallback = card_preview(game, content, card, target);
        &fallback
    };
    let costs = preview.costs.unwrap_or_else(|| {
        (
            effective_cost,
            combat.map_or(0, |combat| crate::game::star_cost(combat, card, def)),
        )
    });
    out[TOKEN_CATEGORICAL..].copy_from_slice(&[
        card.upgrades as f32,
        card.enchantment_amount as f32,
        card.enchantment_value as f32,
        card.variant as f32,
        card.flags as f32,
        card.turn_flags as f32,
        card.value as f32,
        card.replays as f32,
        card.free as u8 as f32,
        def.cost[card.upgrades.min(1) as usize] as f32,
        def.star_cost[card.upgrades.min(1) as usize] as f32,
        card.cost_delta as f32,
        card.cost_override.map_or(0, |cost| cost as i16 + 129) as f32,
        state.0 as f32,
        state.1 as f32,
        state.2 as f32,
        costs.0 as f32,
        costs.1 as f32,
        preview.block as f32,
        preview
            .hits
            .iter()
            .filter(|hit| {
                matches!(
                    hit.target,
                    PreviewTarget::Chosen | PreviewTarget::Random | PreviewTarget::Lowest
                )
            })
            .fold(0i16, |sum, &hit| {
                sum.saturating_add(preview_damage(game, content, hit))
            }) as f32,
        preview
            .hits
            .iter()
            .filter(|hit| matches!(hit.target, PreviewTarget::All | PreviewTarget::Other))
            .fold(0i16, |sum, &hit| {
                sum.saturating_add(preview_damage(game, content, hit))
            }) as f32,
        preview.draw as f32,
        preview.discard as f32,
        preview.exhaust as f32,
    ]);
    out
}

fn extend_card_tokens(
    game: &Game,
    content: &Content,
    out: &mut Vec<Token>,
    zone: usize,
    cards: &[Card],
    positions: impl Fn(usize) -> usize,
    context: i32,
    dampened: &[(u32, u8)],
) {
    let masters = master_cards(&game.run.deck);
    let mut tokens = cards
        .iter()
        .enumerate()
        .map(|(index, &card)| {
            let (master, upgrades) = card_relation(card, &game.run.deck, &masters, dampened);
            let preview = (zone == 1).then(|| played_card_preview(game, content, card, None));
            card_token_v19(
                game,
                content,
                CARD_COLLECTION,
                zone,
                positions(index),
                card,
                (master, upgrades, context),
                None,
                preview.as_ref(),
            )
        })
        .collect::<Vec<_>>();
    tokens.sort_by(|left, right| token_cmp(left, right));
    out.extend(tokens);
}

fn state_card_tokens(game: &Game, content: &Content, out: &mut Vec<Token>) {
    extend_card_tokens(game, content, out, 0, &game.run.deck, |_| 0, 0, &[]);
    extend_card_tokens(game, content, out, 6, &game.paels_cards, |_| 0, 0, &[]);
    match &game.phase {
        Phase::Combat(combat) => {
            extend_card_tokens(
                game,
                content,
                out,
                1,
                &combat.hand,
                |_| 0,
                0,
                &combat.dampened,
            );
            extend_card_tokens(
                game,
                content,
                out,
                2,
                &combat.draw,
                |index| {
                    if index < combat.known_draw_bottom {
                        KNOWN_DRAW_SLOTS + index + 1
                    } else if index >= combat.draw.len() - combat.known_draw_top {
                        combat.draw.len() - index
                    } else {
                        0
                    }
                },
                0,
                &combat.dampened,
            );
            for (cards, zone) in [(&combat.discard, 3), (&combat.exhaust, 4)] {
                extend_card_tokens(game, content, out, zone, cards, |_| 0, 0, &combat.dampened);
            }
            if let Some(card) = combat.history_course {
                extend_card_tokens(game, content, out, 5, &[card], |_| 0, 0, &combat.dampened);
            }
            if let Some(card) = combat.playing {
                extend_card_tokens(game, content, out, 8, &[card], |_| 0, 0, &combat.dampened);
            }
            let auto_plays = combat
                .auto_plays
                .iter()
                .map(|play| play.card)
                .collect::<Vec<_>>();
            extend_card_tokens(
                game,
                content,
                out,
                9,
                &auto_plays,
                |_| 0,
                0,
                &combat.dampened,
            );
        }
        Phase::Event(id, _) if content.events[*id as usize].id == "EVENT.SLIPPERY_BRIDGE" => {
            for &instance in &game.event_cards {
                if let Some(&card) = game.run.deck.iter().find(|card| card.instance == instance) {
                    extend_card_tokens(game, content, out, 7, &[card], |_| 0, 0, &[]);
                }
            }
        }
        _ => {}
    }
}

fn push_action_card_token(
    game: &Game,
    content: &Content,
    out: &mut Vec<Token>,
    zone: usize,
    position: usize,
    context: i32,
    card: Card,
    target: Option<usize>,
    preview: Option<&CardPreview>,
) {
    let masters = master_cards(&game.run.deck);
    let dampened = game
        .combat()
        .map_or(&[][..], |combat| combat.dampened.as_slice());
    let (master, upgrades) = card_relation(card, &game.run.deck, &masters, dampened);
    out.push(card_token_v19(
        game,
        content,
        ACTION_CARD_COLLECTION,
        zone,
        position,
        card,
        (master, upgrades, context),
        target,
        preview,
    ));
}

fn vulnerable_damage(game: &Game, content: &Content, target: usize, damage: i16) -> i16 {
    let combat = game.combat().unwrap();
    let enemy = &combat.enemies[target].creature;
    if enemy.kind(content, PowerKind::Vulnerable) == 0 {
        return damage;
    }
    let base = if enemy.power(power_id::DEBILITATE) > 0 {
        200
    } else {
        150
    };
    let cruelty = combat.player.power(power_id::CRUELTY);
    let phrog = 25
        * game
            .run
            .relics
            .iter()
            .any(|&id| content.relics[id as usize].id == "RELIC.PAPER_PHROG") as i16;
    damage.saturating_mul(base + cruelty + phrog) / 100
}

fn resolved_hits(
    game: &Game,
    content: &Content,
    target: usize,
    selected: Option<usize>,
    aoe_only: bool,
    random: bool,
    hits: &[PreviewHit],
) -> (i16, i16, i16) {
    let enemy = &game.combat().unwrap().enemies[target].creature;
    let mut total = 0i16;
    let mut cap = 0;
    let mut actual = 0i16;
    let mut hp_lost = 0i16;
    let mut block = enemy.block.max(0);
    let shell = enemy.power(power_id::HARDENED_SHELL).max(0);
    let mut shell_received = enemy
        .powers
        .iter()
        .find(|power| power.id == power_id::HARDENED_SHELL)
        .map_or(0, |power| power.value.max(0));
    let mut buffer = enemy.kind(content, PowerKind::Buffer).max(0);
    let mut flutter = enemy.power(power_id::FLUTTER).max(0);
    let mut slippery = enemy.power(power_id::SLIPPERY).max(0);
    for &hit in hits {
        let applies = match hit.target {
            PreviewTarget::Chosen => selected == Some(target),
            PreviewTarget::Random => random,
            PreviewTarget::Lowest => game
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, enemy)| enemy.creature.hp > 0)
                .min_by_key(|(_, enemy)| enemy.creature.hp)
                .is_some_and(|(lowest, _)| lowest == target),
            PreviewTarget::All => true,
            PreviewTarget::Other => selected != Some(target),
            PreviewTarget::Fixed(fixed) => fixed == target,
        };
        if !applies {
            continue;
        }
        let counted = !aoe_only || matches!(hit.target, PreviewTarget::All | PreviewTarget::Other);
        for _ in 0..hit.count.max(0) {
            if hp_lost >= enemy.hp.max(0) {
                break;
            }
            if matches!(hit.damage, PreviewDamage::LoseHp) {
                let lost = hit.amount.min(enemy.hp.max(0) - hp_lost);
                hp_lost = hp_lost.saturating_add(lost);
                if counted {
                    total = total.saturating_add(hit.amount);
                    actual = actual.saturating_add(lost);
                }
                continue;
            }
            if let PreviewDamage::Doom(extra) = hit.damage {
                let remaining = enemy.hp.max(0) - hp_lost;
                let lost = if remaining <= enemy.power(power_id::DOOM).saturating_add(extra) {
                    remaining
                } else {
                    0
                };
                hp_lost = hp_lost.saturating_add(lost);
                if counted {
                    total = total.saturating_add(lost);
                    actual = actual.saturating_add(lost);
                }
                continue;
            }
            let (damage, attack, hit_cap) =
                modified_preview_damage(game, content, hit, Some(target), flutter > 0);
            if counted {
                cap = cap.max(hit_cap);
                total = total.saturating_add(damage);
            }
            let blocked = damage.min(block);
            block -= blocked;
            let mut lost = damage - blocked;
            if shell > 0 {
                lost = lost.min((shell - shell_received).max(0));
            }
            if lost > 0 && slippery > 0 {
                lost = 1;
            }
            if lost > 0 && buffer > 0 {
                buffer -= 1;
                lost = 0;
            }
            if attack
                && (1..5).contains(&lost)
                && game
                    .run
                    .relics
                    .iter()
                    .any(|&id| content.relics[id as usize].id == "RELIC.THE_BOOT")
            {
                lost = 5;
            }
            if lost > 0 {
                if slippery > 0 {
                    slippery -= 1;
                }
                if attack && flutter > 0 {
                    flutter -= 1;
                }
                shell_received = shell_received.saturating_add(lost);
                let lost = lost.min(enemy.hp.max(0) - hp_lost);
                hp_lost = hp_lost.saturating_add(lost);
                if counted {
                    actual = actual.saturating_add(lost);
                }
            }
        }
    }
    (total, cap, actual)
}

fn action_preview(game: &Game, content: &Content, action: &Action) -> Option<CardPreview> {
    match action {
        Action::Play { hand, target } => game
            .combat()?
            .hand
            .get(*hand)
            .copied()
            .map(|card| played_card_preview(game, content, card, *target)),
        Action::Potion { slot, target } => {
            let id = game.run.potions.get(*slot).copied().flatten()?;
            let mut preview = CardPreview::new(game, content);
            preview_effects(
                game,
                content,
                Context {
                    target: *target,
                    ..Context::player()
                },
                content.potions[id as usize].effects,
                1,
                &mut preview,
            );
            Some(preview)
        }
        _ => None,
    }
}

fn action_card_tokens(
    game: &Game,
    content: &Content,
    action: &Action,
    values: &mut [f32; TOKEN_ACTION_VALUES],
    out: &mut Vec<Token>,
) {
    let preview = action_preview(game, content, action);
    let target = match action {
        Action::Play { target, .. } | Action::Potion { target, .. } => *target,
        _ => None,
    };
    match action {
        Action::Play { hand, .. } => {
            if let Some(&card) = game.combat().and_then(|combat| combat.hand.get(*hand)) {
                push_action_card_token(game, content, out, 1, 0, 0, card, target, preview.as_ref());
            }
        }
        Action::Choose(index) => {
            if let Some(combat) = game.combat()
                && let Some(choice) = combat.choice
                && let Some(&card) = match choice.pile {
                    Pile::Draw => &combat.draw,
                    Pile::Hand => &combat.hand,
                    Pile::Discard => &combat.discard,
                    Pile::Exhaust => &combat.exhaust,
                    Pile::Offer => &combat.offer,
                }
                .get(*index)
            {
                let zone = match choice.pile {
                    Pile::Hand => 1,
                    Pile::Draw => 2,
                    Pile::Discard => 3,
                    Pile::Exhaust => 4,
                    Pile::Offer => 5,
                };
                let position = if zone == 2 && *index < combat.known_draw_bottom {
                    KNOWN_DRAW_SLOTS + index + 1
                } else if zone == 2 && *index >= combat.draw.len() - combat.known_draw_top {
                    combat.draw.len() - index
                } else {
                    0
                };
                push_action_card_token(game, content, out, zone, position, 0, card, None, None);
            } else {
                match &game.phase {
                    Phase::ChooseCards(cards, ..) => {
                        if let Some(&card) = cards.get(*index) {
                            push_action_card_token(game, content, out, 6, 0, 0, card, None, None);
                        }
                    }
                    Phase::ChooseBundles(bundles) => {
                        if let Some(cards) = bundles.get(*index) {
                            for &card in cards {
                                push_action_card_token(
                                    game,
                                    content,
                                    out,
                                    6,
                                    0,
                                    *index as i32 + 1,
                                    card,
                                    None,
                                    None,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Action::RewardCard(index) => {
            if let Phase::Rewards(rewards) = &game.phase
                && let Some(&card) = rewards.cards.get(*index)
            {
                push_action_card_token(game, content, out, 6, 0, 0, card, None, None);
            }
        }
        Action::Buy(index) => {
            if let Phase::Shop(items) = &game.phase
                && let Some(ShopItem::Card(card, price)) = items.get(*index)
            {
                push_action_card_token(game, content, out, 6, 0, *price, *card, None, None);
            }
        }
        Action::Smith(index) | Action::Enchant(index) | Action::RemoveCard(index) => {
            if let Some(&card) = game.run.deck.get(*index) {
                push_action_card_token(game, content, out, 0, 0, 0, card, None, None);
            }
        }
        Action::EventCard(index, card) => push_action_card_token(
            game,
            content,
            out,
            6,
            0,
            *index as i32 + 1,
            *card,
            None,
            None,
        ),
        Action::Event(_) => {
            if let Phase::Event(id, _) = &game.phase
                && content.events[*id as usize].id == "EVENT.SLIPPERY_BRIDGE"
                && let Some(&card) = game
                    .run
                    .deck
                    .iter()
                    .find(|card| card.instance == game.event_data[1] as u32)
            {
                push_action_card_token(game, content, out, 9, 0, 0, card, None, None);
            }
        }
        _ => {}
    }
    let Some(preview) = preview else {
        out.sort_by(|left, right| token_cmp(left, right));
        return;
    };
    let Some(combat) = game.combat() else {
        out.sort_by(|left, right| token_cmp(left, right));
        return;
    };
    let alive = combat
        .enemies
        .iter()
        .filter(|enemy| enemy.creature.hp > 0)
        .count();
    for (hit_index, &hit) in preview
        .hits
        .iter()
        .filter(|hit| hit.target == PreviewTarget::Random)
        .enumerate()
    {
        for (target, enemy) in combat
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| enemy.creature.hp > 0)
        {
            let (damage, cap, actual) =
                resolved_hits(game, content, target, None, false, true, &[hit]);
            let mut row = token(
                ENEMY_COLLECTION,
                3,
                enemy.creature.id as usize + 1,
                hit_index + 1,
                target + 1,
            );
            row[8..17].copy_from_slice(&[
                enemy.creature.kind(content, PowerKind::Vulnerable) as f32,
                enemy.creature.block as f32,
                cap as f32,
                damage as f32,
                actual as f32,
                0.0,
                (actual >= enemy.creature.hp) as u8 as f32,
                enemy.creature.hp as f32,
                1.0 / alive.max(1) as f32,
            ]);
            out.push(row);
        }
    }
    if target.is_none()
        && preview.hits.iter().any(|hit| {
            matches!(
                hit.target,
                PreviewTarget::Lowest
                    | PreviewTarget::All
                    | PreviewTarget::Other
                    | PreviewTarget::Fixed(_)
            )
        })
    {
        for (target, enemy) in combat
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| enemy.creature.hp > 0)
        {
            let (damage, cap, actual) =
                resolved_hits(game, content, target, None, false, false, &preview.hits);
            let mut row = token(
                ENEMY_COLLECTION,
                4,
                enemy.creature.id as usize + 1,
                0,
                target + 1,
            );
            row[8..17].copy_from_slice(&[
                enemy.creature.kind(content, PowerKind::Vulnerable) as f32,
                enemy.creature.block as f32,
                cap as f32,
                damage as f32,
                actual as f32,
                0.0,
                (actual >= enemy.creature.hp) as u8 as f32,
                enemy.creature.hp as f32,
                1.0,
            ]);
            out.push(row);
        }
    }
    let Some(target) = target else {
        values[53] = combat
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| enemy.creature.hp > 0)
            .map(|(target, enemy)| {
                resolved_hits(game, content, target, None, true, false, &preview.hits)
                    .2
                    .min(enemy.creature.hp)
            })
            .fold(0i16, i16::saturating_add) as f32;
        out.sort_by(|left, right| token_cmp(left, right));
        return;
    };
    let selected = target;
    let Some(enemy) = combat.enemies.get(target) else {
        out.sort_by(|left, right| token_cmp(left, right));
        return;
    };
    let (damage, cap, actual) = resolved_hits(
        game,
        content,
        target,
        Some(selected),
        false,
        false,
        &preview.hits,
    );
    values[48..56].copy_from_slice(&[
        enemy.creature.kind(content, PowerKind::Vulnerable) as f32,
        enemy.creature.block as f32,
        cap as f32,
        damage as f32,
        actual as f32,
        combat
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| enemy.creature.hp > 0)
            .map(|(target, enemy)| {
                resolved_hits(
                    game,
                    content,
                    target,
                    Some(selected),
                    true,
                    false,
                    &preview.hits,
                )
                .2
                .min(enemy.creature.hp)
            })
            .fold(0i16, i16::saturating_add) as f32,
        (enemy.creature.hp > 0 && actual >= enemy.creature.hp) as u8 as f32,
        enemy.creature.hp.max(0) as f32,
    ]);
    out.sort_by(|left, right| token_cmp(left, right));
}

fn public_card(
    mut card: Card,
    deck: &[Card],
    masters: &[[f32; CARD_FIELDS]],
    dampened: &[(u32, u8)],
) -> Card {
    let (master, upgrades) = card_relation(card, deck, masters, dampened);
    card.instance = master as u32 * 257 + upgrades as u32;
    card
}

fn continuation(game: &Game) -> String {
    let combat = game.combat();
    if game.resume.is_none()
        && game.run_queue.is_empty()
        && combat.is_none_or(|combat| {
            combat.queue.is_empty() && combat.playing.is_none() && combat.auto_plays.is_empty()
        })
    {
        return String::new();
    }
    let mut resume = game.resume.clone();
    if let Some(phase) = &mut resume {
        let masters = master_cards(&game.run.deck);
        let normalize = |card| public_card(card, &game.run.deck, &masters, &[]);
        match phase {
            Phase::Rewards(rewards) => rewards
                .cards
                .iter_mut()
                .for_each(|card| *card = normalize(*card)),
            Phase::Shop(items) => items.iter_mut().for_each(|item| {
                if let ShopItem::Card(card, _) = item {
                    *card = normalize(*card);
                }
            }),
            Phase::ChooseCards(cards, ..) => {
                cards.iter_mut().for_each(|card| *card = normalize(*card))
            }
            Phase::ChooseBundles(bundles) => bundles.iter_mut().flatten().for_each(|card| {
                *card = normalize(*card);
            }),
            Phase::Combat(_) => unreachable!("combat cannot be a resume phase"),
            _ => {}
        }
    }
    if let Some(combat) = combat {
        let masters = master_cards(&game.run.deck);
        let normalize = |card| public_card(card, &game.run.deck, &masters, &combat.dampened);
        let mut queue = combat.queue.clone();
        for pending in &mut queue {
            if let Effect::AutoPlay(card) = pending.effect {
                pending.effect = Effect::AutoPlay(normalize(card));
            }
        }
        let playing = combat.playing.map(normalize);
        let mut auto_plays = combat.auto_plays.clone();
        for play in &mut auto_plays {
            play.card = normalize(play.card);
        }
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}",
            game.run_queue, queue, playing, auto_plays, resume,
        )
    } else {
        format!("{:?}|{:?}", game.run_queue, resume)
    }
}

fn add_continuation(out: &mut [f32], layout: Layout, game: &Game) {
    let continuation = continuation(game);
    let bytes = continuation.as_bytes();
    let combat = game.combat();
    assert!(
        bytes.len() <= CONTINUATION_BYTES,
        "continuation exceeds {CONTINUATION_BYTES} bytes: total={} run={} queue={} playing={} auto={} resume={}",
        bytes.len(),
        format!("{:?}", game.run_queue).len(),
        combat.map_or(0, |combat| format!("{:?}", combat.queue).len()),
        combat.map_or(0, |combat| format!("{:?}", combat.playing).len()),
        combat.map_or(0, |combat| format!("{:?}", combat.auto_plays).len()),
        format!("{:?}", game.resume).len(),
    );
    out[layout.continuation] = bytes.len() as f32 / CONTINUATION_BYTES as f32;
    for (index, bytes) in bytes.chunks(2).enumerate() {
        let value = u16::from_be_bytes([bytes[0], bytes.get(1).copied().unwrap_or(0)]);
        out[layout.continuation + 1 + index] = value as f32 / u16::MAX as f32;
    }
}

fn add_known_draw(
    out: &mut [f32],
    layout: Layout,
    side: usize,
    slot: usize,
    card: Card,
    state: [f32; 3],
) {
    let id = card.id as usize;
    assert!(slot < KNOWN_DRAW_SLOTS);
    if id >= layout.cards {
        return;
    }
    let offset = layout.known_draw + (side * KNOWN_DRAW_SLOTS + slot) * KNOWN_CARD_VALUES;
    out[offset] = 1.0;
    out[offset + 1..offset + KNOWN_CARD_VALUES].copy_from_slice(&card_token(card, state));
}

fn add_nightmare(out: &mut [f32], layout: Layout, slot: usize, card: Card, count: u8) {
    let id = card.id as usize;
    if id >= layout.cards || slot >= DELAYED_SLOTS {
        return;
    }
    let width = layout.cards + CARD_VALUES + ENCHANTMENTS + 1;
    let offset = layout.nightmare + slot * width;
    out[offset + id] = 1.0;
    out[offset + layout.cards..offset + layout.cards + CARD_VALUES]
        .copy_from_slice(&card_values(card));
    if let Some(enchantment) = card.enchantment {
        out[offset + layout.cards + CARD_VALUES + enchantment as usize] = 1.0;
    }
    out[offset + width - 1] = count as f32 / 10.0;
}

fn add_dampened(
    out: &mut [f32],
    layout: Layout,
    slot: usize,
    zone: usize,
    card: Card,
    upgrades: u8,
) {
    let id = card.id as usize;
    if id >= layout.cards || slot >= DELAYED_SLOTS {
        return;
    }
    let width = 4 + layout.cards + CARD_VALUES + ENCHANTMENTS + 1;
    let offset = layout.dampened + slot * width;
    out[offset + zone] = 1.0;
    out[offset + 4 + id] = 1.0;
    out[offset + 4 + layout.cards..offset + 4 + layout.cards + CARD_VALUES]
        .copy_from_slice(&card_values(card));
    if let Some(enchantment) = card.enchantment {
        out[offset + 4 + layout.cards + CARD_VALUES + enchantment as usize] = 1.0;
    }
    out[offset + width - 1] = upgrades as f32 / 10.0;
}

fn power_token(power: Power) -> [f32; POWER_VALUES] {
    [
        (power.id as u32 + 1) as f32 / (u16::MAX as f32 + 1.0),
        power.amount as f32 / 32768.0,
        power.value as f32 / 32768.0,
        power.skip_duration as u8 as f32,
    ]
}

fn add_power(out: &mut [f32], base: usize, actors: usize, actor: usize, power: Power) {
    assert!(actor < actors);
    let width = POWER_SLOTS * POWER_VALUES;
    let powers = &mut out[base + actor * width..base + (actor + 1) * width];
    let used = (0..POWER_SLOTS)
        .position(|slot| powers[slot * POWER_VALUES] == 0.0)
        .unwrap_or(POWER_SLOTS);
    assert!(used < POWER_SLOTS, "actor exceeds {POWER_SLOTS} powers");
    let token = power_token(power);
    let at = (0..used)
        .position(|slot| {
            token_cmp(
                &powers[slot * POWER_VALUES..(slot + 1) * POWER_VALUES],
                &token,
            )
            .is_gt()
        })
        .unwrap_or(used);
    powers.copy_within(
        at * POWER_VALUES..used * POWER_VALUES,
        (at + 1) * POWER_VALUES,
    );
    powers[at * POWER_VALUES..(at + 1) * POWER_VALUES].copy_from_slice(&token);
}

fn enemy_token(enemy: &Enemy, hit: i16) -> [f32; ENEMY_TOKEN_VALUES] {
    let mut token = [0.0; ENEMY_TOKEN_VALUES];
    token[0] = (enemy.creature.id as u32 + 1) as f32 / (u16::MAX as f32 + 1.0);
    token[1] = enemy.creature.hp.max(0) as f32 / enemy.creature.max_hp.max(1) as f32;
    token[2] = enemy.creature.max_hp as f32 / 500.0;
    token[3] = enemy.creature.block as f32 / 100.0;
    token[4] = enemy.move_index.min(20) as f32 / 20.0;
    token[5] = if enemy.last_move == usize::MAX {
        -0.05
    } else {
        enemy.last_move.min(20) as f32 / 20.0
    };
    token[6] = enemy.repeats as f32 / 10.0;
    token[7] = enemy.stunned as u8 as f32;
    token[8] = enemy.value as f32 / 100.0;
    token[9] = hit as f32 / 20.0;
    for &prior in &enemy.move_history {
        if prior < 20 {
            token[10 + prior] += 0.1;
        }
    }
    for (position, &prior) in enemy.move_history.iter().rev().take(3).enumerate() {
        token[30 + position] = (prior + 1) as f32 / 20.0;
    }
    token
}

fn shop_item_token(item: &ShopItem) -> [f32; PARASOL_VALUES] {
    let mut token = [0.0; PARASOL_VALUES];
    match item {
        ShopItem::Card(card, _) => {
            token[0] = 1.0;
            token[4..4 + CARD_FIELDS].copy_from_slice(&card_fields(*card));
        }
        ShopItem::Relic(id, _) => {
            token[1] = 1.0;
            token[4 + CARD_FIELDS] = (*id as u32 + 1) as f32 / (u16::MAX as f32 + 1.0);
        }
        ShopItem::Potion(id, _) => {
            token[2] = 1.0;
            token[5 + CARD_FIELDS] = (*id as u32 + 1) as f32 / (u16::MAX as f32 + 1.0);
        }
        ShopItem::Remove(_) => token[3] = 1.0,
    }
    token
}

fn semantic_card_token(
    game: &Game,
    content: &Content,
    collection: usize,
    kind: usize,
    owner: usize,
    card: Card,
    context: i32,
) -> Token {
    let masters = master_cards(&game.run.deck);
    let dampened = game
        .combat()
        .map_or(&[][..], |combat| combat.dampened.as_slice());
    let relation = card_relation(card, &game.run.deck, &masters, dampened);
    let mut row = card_token_v19(
        game,
        content,
        collection,
        kind - 1,
        0,
        card,
        (relation.0, relation.1, context),
        None,
        None,
    );
    row[3] = owner as f32;
    row
}

fn phase_card_tokens(game: &Game, content: &Content, phase: &Phase, out: &mut Vec<Token>) {
    let mut push = |owner, card, context| {
        out.push(semantic_card_token(
            game,
            content,
            CONTINUATION_COLLECTION,
            RESUME_CARD_KIND,
            owner,
            card,
            context,
        ))
    };
    match phase {
        Phase::Rewards(rewards) => rewards
            .cards
            .iter()
            .enumerate()
            .for_each(|(position, &card)| push(position + 1, card, 0)),
        Phase::Shop(items) => items.iter().enumerate().for_each(|(position, item)| {
            if let ShopItem::Card(card, price) = item {
                push(position + 1, *card, *price);
            }
        }),
        Phase::ChooseCards(cards, ..) => cards
            .iter()
            .enumerate()
            .for_each(|(position, &card)| push(position + 1, card, 0)),
        Phase::ChooseBundles(bundles) => {
            let mut position = 0;
            for (bundle, cards) in bundles.iter().enumerate() {
                for &card in cards {
                    position += 1;
                    push(position, card, bundle as i32 + 1);
                }
            }
        }
        _ => {}
    }
}

fn state_summary_tokens(game: &Game, content: &Content, bonuses: (i16, i16), out: &mut Vec<Token>) {
    let run = &game.run;
    let mut row = token(
        STATE_COLLECTION,
        1,
        run.character as usize + 1,
        game.act as usize + 1,
        phase_index(&game.phase) + 1,
    );
    row[8..27].copy_from_slice(&[
        run.hp.max(0) as f32 / run.max_hp.max(1) as f32,
        run.max_hp as f32 / 100.0,
        run.gold as f32 / 500.0,
        run.act as f32 / 3.0,
        run.floor as f32 / 18.0,
        run.energy as f32 / 10.0,
        run.draw as f32 / 10.0,
        run.orb_slots as f32 / 10.0,
        run.card_shop_removals as f32 / 10.0,
        run.ascension as f32 / 10.0,
        room_index(game.room) as f32 / ROOMS as f32,
        bonuses.0 as f32 / 32.0,
        bonuses.1 as f32 / 32.0,
        game.rarity_offset as f32 / 10.0,
        game.potion_odds as f32 / 100.0,
        game.bosses_visited as f32 / 2.0,
        game.event_combat as f32 / 10.0,
        game.removal_price as f32 / 500.0,
        game.fishing_rod as f32 / 3.0,
    ]);
    out.push(row);

    let mut row = token(STATE_COLLECTION, 2, 0, 0, 0);
    row[8..29].copy_from_slice(&[
        game.happy_flower as f32 / 3.0,
        game.tea_set as u8 as f32,
        game.lasting_candy as f32 / 3.0,
        game.paels_wing as f32 / 3.0,
        game.silver_crucible as f32 / 10.0,
        game.silver_treasures as f32 / 10.0,
        game.winged_boots as f32 / 3.0,
        game.rest_used as f32 / 255.0,
        game.girya as f32 / 3.0,
        game.pumpkin_candle as f32 / 10.0,
        game.toy_box_combats as f32 / 10.0,
        game.nunchaku as f32 / 10.0,
        game.pendulum as f32 / 10.0,
        game.pen_nib as f32 / 10.0,
        game.iron_club as f32 / 10.0,
        game.joss_paper as f32 / 10.0,
        game.tuning_fork as f32 / 10.0,
        game.galactic_dust as f32 / 10.0,
        game.book_of_five_rings as f32 / 10.0,
        game.ember_tea as f32 / 10.0,
        game.sword_of_stone as f32 / 10.0,
    ]);
    out.push(row);

    let mut row = token(STATE_COLLECTION, 3, 0, 0, 0);
    row[8..32].copy_from_slice(&[
        game.damage_taken as u8 as f32,
        game.maw_bank as u8 as f32,
        game.lizard_tail as u8 as f32,
        game.cooking as u8 as f32,
        game.unknown_odds[0] as f32 / 100.0,
        game.unknown_odds[1] as f32 / 100.0,
        game.unknown_odds[2] as f32 / 100.0,
        game.unknown_odds[3] as f32 / 100.0,
        game.fake_happy_flower as f32 / 3.0,
        game.pollinous_core as f32 / 10.0,
        game.silken_tress as u8 as f32,
        game.bone_tea as u8 as f32,
        game.tea_of_discourtesy as u8 as f32,
        game.replacing_potion as u8 as f32,
        game.rerolled_cards as u8 as f32,
        game.weak_encounters_left as f32 / 15.0,
        game.regular_encounters_left as f32 / 15.0,
        game.elite_encounters_left as f32 / 15.0,
        game.pending_curse as u8 as f32,
        game.wongo_combats.is_some() as u8 as f32,
        game.wongo_combats.unwrap_or_default() as f32 / 5.0,
        (game.golden_compass == Some(run.act)) as u8 as f32,
        game.astrolabe as u8 as f32,
        game.transform_niche as u8 as f32,
    ]);
    out.push(row);

    let mut row = token(STATE_COLLECTION, 4, 0, 0, 0);
    row[8..14].copy_from_slice(&[
        game.paels_tooth as u8 as f32,
        game.parasol_removal as u8 as f32,
        game.conveyor as u8 as f32,
        game.fake_shop as u8 as f32,
        game.event_rng.is_some() as u8 as f32,
        game.replaying as u8 as f32,
    ]);
    out.push(row);

    for &id in &game.visited_events {
        out.push(token(STATE_COLLECTION, 5, id as usize + 1, 0, 0));
    }
    if let Some(crystal) = &game.crystal {
        let mut row = token(STATE_COLLECTION, 6, 0, 0, 0);
        row[8..12].copy_from_slice(&[
            crystal.remaining as f32 / 10.0,
            crystal.big as u8 as f32,
            crystal.clear.iter().filter(|&&clear| clear).count() as f32 / 25.0,
            crystal.revealed.len() as f32 / 25.0,
        ]);
        out.push(row);
    }

    let mut phase = token(STATE_COLLECTION, 7, 0, 0, 0);
    match &game.phase {
        Phase::Combat(combat) => {
            let mut row = token(STATE_COLLECTION, 8, 0, 0, 0);
            row[8..27].copy_from_slice(&[
                combat.energy as f32 / 10.0,
                combat.max_energy as f32 / 10.0,
                combat.draw_per_turn as f32 / 10.0,
                combat.stars as f32 / 10.0,
                combat.turn as f32 / 20.0,
                combat.orb_slots as f32 / 10.0,
                combat.player.hp.max(0) as f32 / combat.player.max_hp.max(1) as f32,
                combat.player.max_hp as f32 / 500.0,
                combat.player.block as f32 / 100.0,
                combat.osty.hp.max(0) as f32 / combat.osty.max_hp.max(1) as f32,
                combat.osty.max_hp as f32 / 500.0,
                combat.osty.block as f32 / 100.0,
                combat.card_energy as f32 / 20.0,
                combat.card_stars as f32 / 20.0,
                combat.card_plays as f32 / 20.0,
                combat.enemy_turn as u8 as f32,
                combat.ending as u8 as f32,
                combat.force_end as u8 as f32,
                combat.paels_tears as u8 as f32,
            ]);
            out.push(row);
            let history = combat.history;
            let mut row = token(STATE_COLLECTION, 9, 0, 0, 0);
            row[8..30].copy_from_slice(&[
                history.cards,
                history.manual_cards,
                history.manual_plays,
                history.attacks,
                history.skills,
                history.powers,
                history.energy,
                history.exhausted,
                history.discarded,
                history.shivs,
                history.stars_gained,
                history.generated,
                history.ethereal,
                history.extra_drawn,
                history.doom_applied,
                history.osty_attacks,
                history.block_gains,
                history.block_card,
                history.block_card_gains,
                history.hp_lost,
                history.hp_loss_events,
                history.feral_returns,
            ].map(|value| value as f32 / 20.0));
            out.push(row);
            let mut row = token(STATE_COLLECTION, 10, 0, 0, 0);
            row[8..32].copy_from_slice(&[
                combat.last_cards as i16,
                combat.orbit_spent,
                combat.last_damage,
                combat.drawn,
                combat.lightning_channeled,
                combat.orbs_channeled as i16,
                combat.poisoned as i16,
                combat.centennial_puzzle as i16,
                combat.demon_tongue as i16,
                combat.permafrost as i16,
                combat.pen_nib as i16,
                combat.ruined_helmet as i16,
                combat.music_box as i16,
                combat.mini_regent as i16,
                combat.rainbow_ring as i16,
                combat.kusarigama as i16,
                combat.unsettling_used as i16,
                combat.diamond_diadem as i16,
                combat.burning_sticks as i16,
                combat.throwing_axe as i16,
                combat.paels_eye as i16,
                combat.paels_eye_extra as i16,
                combat.paels_legion as i16,
                combat.belt_buckle as i16,
            ].map(|value| value as f32 / 20.0));
            out.push(row);
            for slot in 0..combat.orb_slots as usize {
                let orb = combat.orbs.get(slot);
                let mut row = token(
                    STATE_COLLECTION,
                    11,
                    orb.map_or(0, |orb| orb.id as usize + 1),
                    0,
                    slot + 1,
                );
                row[8] = orb.map_or(0.0, |orb| orb.value as f32 / 100.0);
                out.push(row);
            }
            if let Some(choice) = combat.choice {
                let (filter, filter_value) = filter_features(choice.filter);
                let (op, op_value) = op_features(choice.op);
                let mut row = token(
                    STATE_COLLECTION,
                    12,
                    pile_index(choice.pile) + 1,
                    filter + 1,
                    op + 1,
                );
                row[8..15].copy_from_slice(&[
                    filter_value as f32 / 1_000_000.0,
                    op_value as f32 / 1_000_000.0,
                    choice.remaining as f32 / 10.0,
                    choice.optional as u8 as f32,
                    combat.enemy_turn as u8 as f32,
                    combat.ending as u8 as f32,
                    combat.force_end as u8 as f32,
                ]);
                out.push(row);
            }
        }
        Phase::Rewards(rewards) => {
            phase[8] = rewards.gold as f32 / 500.0;
            phase[9] = rewards.removals as f32 / 10.0;
            for (position, reward) in rewards.card_rewards.iter().enumerate() {
                let (kind, id) = match reward {
                    CardReward::Standard(room) => (1, room_index(*room) + 1),
                    CardReward::Fixed(character, rarity) => {
                        (2, *character as usize * 8 + *rarity as usize + 1)
                    }
                    CardReward::Kaleidoscope => (3, 0),
                    CardReward::Crystal(rarity) => (4, *rarity as usize + 1),
                };
                out.push(token(STATE_COLLECTION, 13, id, kind, position + 1));
            }
        }
        Phase::Event(id, _) => {
            phase[2] = *id as f32 + 1.0;
            let name = content.events[*id as usize].id;
            if name == "EVENT.SLIPPERY_BRIDGE" {
                phase[8] = game.event_data[0] as f32 / 1_000.0;
            } else if name == "EVENT.TINKER_TIME" {
                phase[8] = game.event_data[3] as f32 / 3.0;
            } else if !matches!(
                name,
                "EVENT.STONE_OF_ALL_TIME"
                    | "EVENT.THE_FUTURE_OF_POTIONS"
                    | "EVENT.NEOW"
                    | "EVENT.DARV"
                    | "EVENT.NONUPEIPE"
                    | "EVENT.OROBAS"
                    | "EVENT.TANX"
                    | "EVENT.TEZCATARA"
                    | "EVENT.PAEL"
                    | "EVENT.VAKUU"
            ) {
                for (slot, value) in game.event_data.into_iter().enumerate() {
                    phase[8 + slot] = value as f32 / 1_000.0;
                }
            }
        }
        Phase::RemoveCards(count, price, optional) => {
            phase[8..11].copy_from_slice(&[
                *count as f32 / 10.0,
                *price as f32 / 500.0,
                *optional as u8 as f32,
            ]);
        }
        Phase::UpgradeCards(count, optional) => {
            phase[8] = *count as f32 / 10.0;
            phase[9] = *optional as u8 as f32;
        }
        Phase::TransformCards(target, count, optional) => {
            phase[2] = target.map_or(0, |id| id as usize + 1) as f32;
            phase[8] = *count as f32 / 10.0;
            phase[9] = *optional as u8 as f32;
        }
        Phase::EnchantCards(enchantment, amount, count, card_type, optional) => {
            phase[2] = *enchantment as usize as f32 + 1.0;
            phase[3] = card_type.map_or(0, |kind| kind as usize + 1) as f32;
            phase[8..11].copy_from_slice(&[
                *amount as f32 / 100.0,
                *count as f32 / 10.0,
                *optional as u8 as f32,
            ]);
        }
        Phase::ChooseCards(_, count, reward) => {
            phase[8] = *count as f32 / 10.0;
            phase[9] = *reward as u8 as f32;
        }
        _ => {}
    }
    out.push(phase);
}

fn push_power_tokens(out: &mut Vec<Token>, kind: usize, owner: usize, powers: &[Power]) {
    for (position, &power) in powers.iter().enumerate() {
        let mut row = token(
            POWER_COLLECTION,
            kind,
            power.id as usize + 1,
            owner,
            position + 1,
        );
        row[8] = power.amount as f32 / 32768.0;
        row[9] = power.value as f32 / 32768.0;
        row[10] = power.skip_duration as u8 as f32;
        out.push(row);
    }
}

fn encounter_vocabs(content: &Content) -> [Vec<Id>; 3] {
    let mut out = [Vec::new(), Vec::new(), Vec::new()];
    for act in &content.acts {
        for (kind, ids) in [act.encounters, act.elites, act.bosses]
            .into_iter()
            .enumerate()
        {
            for &id in ids {
                if !out[kind].contains(&id) {
                    out[kind].push(id);
                }
            }
        }
    }
    assert_eq!(out.iter().map(Vec::len).collect::<Vec<_>>(), [57, 13, 13]);
    out
}

fn encounter_id(vocab: &[Id], id: Id) -> usize {
    vocab.iter().position(|&x| x == id).unwrap() + 1
}

fn ancient_offer(game: &Game, content: &Content, index: usize) -> Option<Id> {
    let Phase::Event(id, options) = &game.phase else {
        return None;
    };
    if index >= options.len()
        || !matches!(
            content.events[*id as usize].id,
            "EVENT.NEOW"
                | "EVENT.DARV"
                | "EVENT.NONUPEIPE"
                | "EVENT.OROBAS"
                | "EVENT.TANX"
                | "EVENT.TEZCATARA"
                | "EVENT.PAEL"
                | "EVENT.VAKUU"
        )
    {
        return None;
    }
    game.event_data
        .get(index)
        .copied()
        .filter(|&id| id > 0 && id as usize <= content.relics.len())
        .map(|id| id as Id - 1)
}

fn hashed_token<T: Hash>(kind: usize, position: usize, value: &T) -> Token {
    let mut hasher = ContentHasher(0xcbf29ce484222325);
    value.hash(&mut hasher);
    let hash = hasher.finish();
    let mut row = token(
        CONTINUATION_COLLECTION,
        kind,
        (hash as u16) as usize + 1,
        ((hash >> 16) as u16) as usize + 1,
        position + 1,
    );
    row[8] = ((hash >> 32) as u16) as f32;
    row[9] = ((hash >> 48) as u16) as f32;
    row
}

fn continuation_tokens(game: &Game, content: &Content, out: &mut Vec<Token>) {
    for (position, effect) in game.run_queue.iter().rev().enumerate() {
        let mut row = hashed_token(7, position, effect);
        row[10] = event_effect_index(effect) as f32 + 1.0;
        out.push(row);
    }
    if let Some(combat) = game.combat() {
        for (position, pending) in combat.queue.iter().rev().enumerate() {
            out.push(hashed_token(8, position, pending));
        }
        for (position, play) in combat.auto_plays.iter().rev().enumerate() {
            out.push(hashed_token(9, position, play));
        }
    }
    let Some(phase) = &game.resume else {
        return;
    };
    let mut row = token(
        CONTINUATION_COLLECTION,
        10,
        phase_index(phase) + 1,
        0,
        0,
    );
    match phase {
        Phase::Rewards(rewards) => {
            row[8] = rewards.gold as f32 / 500.0;
            row[9] = rewards.removals as f32 / 10.0;
            for (position, &id) in rewards.relics.iter().enumerate() {
                out.push(token(
                    CONTINUATION_COLLECTION,
                    11,
                    id as usize + 1,
                    0,
                    position + 1,
                ));
            }
            for (position, &id) in rewards.potions.iter().enumerate() {
                out.push(token(
                    CONTINUATION_COLLECTION,
                    12,
                    id as usize + 1,
                    0,
                    position + 1,
                ));
            }
            for (position, reward) in rewards.card_rewards.iter().enumerate() {
                out.push(hashed_token(13, position, reward));
            }
        }
        Phase::Shop(items) => {
            for (position, item) in items.iter().enumerate() {
                match item {
                    ShopItem::Relic(id, price) => {
                        let mut item = token(
                            CONTINUATION_COLLECTION,
                            14,
                            *id as usize + 1,
                            0,
                            position + 1,
                        );
                        item[8] = *price as f32 / 500.0;
                        out.push(item);
                    }
                    ShopItem::Potion(id, price) => {
                        let mut item = token(
                            CONTINUATION_COLLECTION,
                            15,
                            *id as usize + 1,
                            0,
                            position + 1,
                        );
                        item[8] = *price as f32 / 500.0;
                        out.push(item);
                    }
                    ShopItem::Remove(price) => {
                        let mut item = token(CONTINUATION_COLLECTION, 16, 0, 0, position + 1);
                        item[8] = *price as f32 / 500.0;
                        out.push(item);
                    }
                    ShopItem::Card(..) => {}
                }
            }
        }
        Phase::Event(id, options) => {
            row[2] = *id as f32 + 1.0;
            for (position, option) in options.iter().enumerate() {
                out.push(hashed_token(17, position, option));
            }
        }
        Phase::RemoveCards(count, tag, optional) => {
            row[8..11].copy_from_slice(&[
                *count as f32 / 10.0,
                *tag as f32 / u16::MAX as f32,
                *optional as u8 as f32,
            ]);
        }
        Phase::UpgradeCards(count, optional) => {
            row[8] = *count as f32 / 10.0;
            row[9] = *optional as u8 as f32;
        }
        Phase::TransformCards(target, count, optional) => {
            row[2] = target.map_or(0, |id| id as usize + 1) as f32;
            row[8] = *count as f32 / 10.0;
            row[9] = *optional as u8 as f32;
        }
        Phase::EnchantCards(enchantment, amount, count, card_type, optional) => {
            row[2] = *enchantment as usize as f32 + 1.0;
            row[3] = card_type.map_or(0, |kind| kind as usize + 1) as f32;
            row[8..11].copy_from_slice(&[
                *amount as f32 / 100.0,
                *count as f32 / 10.0,
                *optional as u8 as f32,
            ]);
        }
        Phase::ChooseCards(_, count, reward) => {
            row[8] = *count as f32 / 10.0;
            row[9] = *reward as u8 as f32;
        }
        Phase::Combat(_) => unreachable!("combat cannot be a resume phase"),
        _ => {}
    }
    out.push(row);
    phase_card_tokens(game, content, phase, out);
}

fn state_entity_tokens(game: &Game, content: &Content, layout: Layout) -> Vec<Token> {
    let mut out = Vec::new();
    if let Some(combat) = game.combat() {
        let mut next_wriggler = 2;
        while combat
            .enemies
            .iter()
            .any(|enemy| enemy.instance == next_wriggler)
        {
            next_wriggler += 1;
        }
        out.push(token(
            ENEMY_COLLECTION,
            2,
            1 + next_wriggler as usize % 2,
            0,
            0,
        ));
        for (position, enemy) in combat.enemies.iter().enumerate() {
            let mut row = token(
                ENEMY_COLLECTION,
                0,
                enemy.creature.id as usize + 1,
                0,
                position + 1,
            );
            row[8] = if content.enemies[enemy.creature.id as usize].id == "MONSTER.WRIGGLER" {
                (enemy.instance.saturating_sub(1) % 2 + 1) as f32
            } else {
                0.0
            };
            row[10] = enemy.creature.hp as f32 / 500.0;
            row[11] = enemy.creature.max_hp as f32 / 500.0;
            row[12] = enemy.creature.block as f32 / 100.0;
            row[13] = enemy.move_index as f32 / 20.0;
            row[14] = if enemy.last_move == usize::MAX {
                -1.0
            } else {
                enemy.last_move as f32 / 20.0
            };
            row[15] = enemy.repeats as f32 / 10.0;
            row[16] = enemy.stunned as u8 as f32;
            row[17] = enemy.value as f32 / 100.0;
            row[18] = combat.hits.get(position).copied().unwrap_or_default() as f32 / 20.0;
            out.push(row);
            for (history, &prior) in enemy.move_history.iter().enumerate() {
                out.push(token(
                    ENEMY_COLLECTION,
                    1,
                    prior + 1,
                    position + 1,
                    history + 1,
                ));
            }
            push_power_tokens(&mut out, 0, position + 3, &enemy.creature.powers);
            if let Some(powers) = combat.enemy_power_snapshot.get(position) {
                push_power_tokens(&mut out, 1, position + 3, powers);
            }
        }
        push_power_tokens(&mut out, 0, 1, &combat.player.powers);
        push_power_tokens(&mut out, 0, 2, &combat.osty.powers);
        push_power_tokens(&mut out, 1, 1, &combat.power_snapshot);
        for (position, &(card, count)) in combat.nightmares.iter().enumerate() {
            out.push(semantic_card_token(
                game,
                content,
                DELAYED_COLLECTION,
                NIGHTMARE_CARD_KIND,
                position + 1,
                card,
                count as i32,
            ));
        }
        let piles = [&combat.draw, &combat.hand, &combat.discard, &combat.exhaust];
        for &(instance, upgrades) in &combat.dampened {
            if let Some((zone, &card)) = piles.iter().enumerate().find_map(|(zone, pile)| {
                pile.iter()
                    .find(|x| x.instance == instance)
                    .map(|x| (zone, x))
            }) {
                let mut row = semantic_card_token(
                    game,
                    content,
                    DELAYED_COLLECTION,
                    DAMPENED_CARD_KIND,
                    zone + 1,
                    card,
                    0,
                );
                row[22] = upgrades as f32 + 1.0;
                out.push(row);
            }
        }
        for (position, pending) in combat.queue.iter().enumerate() {
            if let Effect::AutoPlay(card) = pending.effect {
                out.push(semantic_card_token(
                    game,
                    content,
                    CONTINUATION_COLLECTION,
                    QUEUED_CARD_KIND,
                    position + 1,
                    card,
                    0,
                ));
            }
        }
        for (position, &(turns, amount)) in combat.bombs.iter().enumerate() {
            let mut row = token(DELAYED_COLLECTION, 2, 0, 0, position + 1);
            row[8] = turns as f32 / 3.0;
            row[9] = amount as f32 / 100.0;
            out.push(row);
        }
        for (position, &(turns, energy)) in combat.automation.iter().enumerate() {
            let mut row = token(DELAYED_COLLECTION, 3, 0, 0, position + 1);
            row[8] = turns as f32 / 10.0;
            row[9] = energy as f32 / 10.0;
            out.push(row);
        }
        for (position, &(cards, damage, start)) in combat.panache.iter().enumerate() {
            let mut row = token(DELAYED_COLLECTION, 4, 0, 0, position + 1);
            row[8] = cards as f32 / 5.0;
            row[9] = damage as f32 / 20.0;
            row[10] = start as f32 / 20.0;
            out.push(row);
        }
        for (position, &amount) in combat.boulders.iter().enumerate() {
            let mut row = token(DELAYED_COLLECTION, 5, 0, 0, position + 1);
            row[8] = amount as f32 / 100.0;
            out.push(row);
        }
        if let Some(id) = combat.unsettling_lamp {
            out.push(token(DELAYED_COLLECTION, 6, id as usize + 1, 0, 0));
        }
    }
    for (position, &id) in game.run.relics.iter().enumerate() {
        let mut row = token(RELIC_COLLECTION, 0, id as usize + 1, 0, position + 1);
        row[8] = game.melted_relics.contains(&position) as u8 as f32;
        row[9] = game
            .wax_relics
            .iter()
            .filter(|wax| !game.melted_relics.contains(wax))
            .position(|&wax| wax == position)
            .map_or(0, |x| x + 1) as f32;
        out.push(row);
    }
    let trader = matches!(game.phase, Phase::Event(id, _) if Some(id) == layout.relic_trader);
    let mut bags = [game.relic_deques.clone(), game.shared_relic_deques.clone()];
    if !trader || game.event_relic.is_some() {
        let mut hidden = if trader {
            vec![]
        } else {
            game.relic_queue.clone()
        };
        hidden.extend(game.event_relic);
        hidden.sort_unstable();
        hidden.dedup();
        for id in hidden {
            if game.run.relics.contains(&id) {
                continue;
            }
            if let Some(rarity) = crate::game::relic_group(id) {
                if !bags[0][rarity].contains(&id) {
                    bags[0][rarity].push(id);
                }
                if content.acts[0].relics.contains(&id) && !bags[1][rarity].contains(&id) {
                    bags[1][rarity].push(id);
                }
            }
        }
    }
    for (shared, deques) in bags.iter().enumerate() {
        for (rarity, relics) in deques.iter().enumerate() {
            for &id in relics {
                out.push(token(
                    RELIC_COLLECTION,
                    1 + shared,
                    id as usize + 1,
                    rarity + 1,
                    0,
                ));
            }
        }
    }
    let parasol_masters = master_cards(&game.run.deck);
    for (position, item) in game.parasol.iter().enumerate() {
        match item {
            ShopItem::Card(card, price) => {
                let relation = card_relation(*card, &game.run.deck, &parasol_masters, &[]);
                let mut row = card_token_v19(
                    game,
                    content,
                    CONTINUATION_COLLECTION,
                    PARASOL_CARD_KIND - 1,
                    0,
                    *card,
                    (relation.0, relation.1, *price),
                    None,
                    None,
                );
                row[3] = (position + 1) as f32;
                out.push(row);
            }
            ShopItem::Relic(id, price) => {
                let mut row = token(RELIC_COLLECTION, 4, *id as usize + 1, 0, position + 1);
                row[8] = *price as f32 / 500.0;
                out.push(row);
            }
            ShopItem::Potion(id, price) => {
                let mut row = token(POTION_COLLECTION, 4, *id as usize + 1, 0, position + 1);
                row[8] = *price as f32 / 500.0;
                out.push(row);
            }
            ShopItem::Remove(price) => {
                let mut row = token(CONTINUATION_COLLECTION, 2, 0, 0, position + 1);
                row[8] = *price as f32 / 500.0;
                out.push(row);
            }
        }
    }
    for (position, &id) in game.fake_merchant.iter().enumerate() {
        out.push(token(RELIC_COLLECTION, 5, id as usize + 1, 0, position + 1));
    }
    for (position, &gold) in game.reward_gold_parts.iter().enumerate() {
        let mut row = token(CONTINUATION_COLLECTION, 3, 0, 0, position + 1);
        row[8] = gold as f32 / 500.0;
        out.push(row);
    }
    if game.conveyor {
        for (position, &value) in game.event_data.iter().enumerate() {
            let mut row = token(CONTINUATION_COLLECTION, 6, 0, 0, position + 1);
            row[8] = value as f32 / 1_000.0;
            out.push(row);
        }
    }
    for (position, potion) in game.run.potions.iter().enumerate() {
        out.push(token(
            POTION_COLLECTION,
            0,
            potion.map_or(0, |id| id as usize + 1),
            0,
            position + 1,
        ));
    }
    if let Some(id) = game.pending_potion {
        out.push(token(POTION_COLLECTION, 1, id as usize + 1, 0, 0));
    }
    match &game.phase {
        Phase::Event(id, _) if trader || Some(*id) == layout.slippery_bridge => {}
        Phase::Event(_, _) => {
            for (position, &owned) in game.event_cards.iter().enumerate() {
                if let Some(&id) = game.run.relics.get(owned as usize) {
                    out.push(token(
                        RELIC_COLLECTION,
                        10,
                        id as usize + 1,
                        0,
                        position + 1,
                    ));
                }
            }
        }
        _ => {}
    }

    for node in &game.map.nodes {
        let mut row = token(
            MAP_COLLECTION,
            0,
            room_index(node.room) + 1,
            node.floor as usize,
            node.lane as usize + 1,
        );
        row[8] = game
            .map
            .current
            .and_then(|x| game.map.nodes.get(x))
            .is_some_and(|x| x.floor == node.floor && x.lane == node.lane) as u8
            as f32;
        row[9] = (game.fur_coat_act == Some(game.run.act)
            && game.fur_coat.contains(&(node.lane, node.floor))) as u8 as f32;
        row[10] = (game.spoils == Some((node.lane, node.floor))) as u8 as f32;
        out.push(row);
        for &next in &node.next {
            if let Some(next) = game.map.nodes.get(next) {
                out.push(token(
                    MAP_COLLECTION,
                    1,
                    next.lane as usize + 1,
                    node.floor as usize,
                    node.lane as usize + 1,
                ));
            }
        }
    }
    if let Some(crystal) = &game.crystal {
        for (cell, &clear) in crystal.clear.iter().enumerate() {
            let visible = crystal.cells[cell]
                .filter(|item| crystal.revealed.contains(item))
                .map_or(0, |item| crystal.items[item].4 as usize + 1);
            if clear || visible != 0 {
                let mut row = token(CRYSTAL_COLLECTION, 0, visible, cell / 11 + 1, cell % 11 + 1);
                row[8] = clear as u8 as f32;
                out.push(row);
            }
        }
    }
    let encounters = encounter_vocabs(content);
    if let Some(id) = game.bosses[0] {
        out.push(token(
            ENCOUNTER_COLLECTION,
            0,
            encounter_id(&encounters[2], id),
            0,
            0,
        ));
    }
    if !game.replaying {
        for &id in &game.encounters {
            out.push(token(
                ENCOUNTER_COLLECTION,
                1,
                encounter_id(&encounters[0], id),
                0,
                0,
            ));
        }
        for &id in &game.elites {
            out.push(token(
                ENCOUNTER_COLLECTION,
                2,
                encounter_id(&encounters[1], id),
                0,
                0,
            ));
        }
        if let Some(id) = game.last_encounter {
            out.push(token(
                ENCOUNTER_COLLECTION,
                3,
                encounter_id(&encounters[0], id),
                0,
                0,
            ));
        }
        if let Some(id) = game.last_elite {
            out.push(token(
                ENCOUNTER_COLLECTION,
                4,
                encounter_id(&encounters[1], id),
                0,
                0,
            ));
        }
    }
    continuation_tokens(game, content, &mut out);
    out.sort_by(|left, right| token_cmp(left, right));
    out
}

fn state_tokens(game: &Game, content: &Content, layout: Layout) -> Vec<Token> {
    state_tokens_with_bonuses(game, content, layout, (0, 0))
}

fn state_tokens_with_bonuses(
    game: &Game,
    content: &Content,
    layout: Layout,
    bonuses: (i16, i16),
) -> Vec<Token> {
    let mut out = state_entity_tokens(game, content, layout);
    state_card_tokens(game, content, &mut out);
    state_summary_tokens(game, content, bonuses, &mut out);
    out.sort_by(|left, right| token_cmp(left, right));
    out
}

fn strip_tokenized_state(game: &mut Game) {
    game.run.deck.clear();
    game.map.current = None;
    game.paels_cards.clear();
    game.parasol.clear();
    game.fake_merchant.clear();
    game.reward_gold_parts.clear();
    game.run_queue.clear();
    game.resume = None;
    match &mut game.phase {
        Phase::Combat(combat) => {
            combat.hand.clear();
            combat.draw.clear();
            combat.discard.clear();
            combat.exhaust.clear();
            combat.offer.clear();
            combat.known_draw_top = 0;
            combat.known_draw_bottom = 0;
            combat.nightmares.clear();
            combat.bombs.clear();
            combat.automation.clear();
            combat.panache.clear();
            combat.boulders.clear();
            combat.dampened.clear();
            combat.player.powers.clear();
            combat.osty.powers.clear();
            combat.enemies.clear();
            combat.hits.clear();
            combat.power_snapshot.clear();
            combat.enemy_power_snapshot.clear();
            combat.playing = None;
            combat.auto_plays.clear();
            combat.queue.clear();
            combat.unsettling_lamp = None;
            combat.history_course = None;
        }
        Phase::Rewards(rewards) => {
            rewards.cards.clear();
            rewards.relics.clear();
            rewards.potions.clear();
        }
        Phase::Shop(items) => items.retain(|item| matches!(item, ShopItem::Remove(_))),
        Phase::ChooseCards(cards, ..) => cards.clear(),
        Phase::ChooseBundles(bundles) => bundles.clear(),
        _ => {}
    }
}

fn global_features_with_bonuses(game: &Game, layout: Layout, bonuses: (i16, i16)) -> Vec<f32> {
    let resume = game
        .resume
        .as_ref()
        .map_or(0.0, |phase| (phase_index(phase) + 1) as f32 / PHASES as f32);
    let mut safe = game.clone();
    strip_tokenized_state(&mut safe);
    let dense = state_features(&safe, layout);
    let ranges = [
        0..layout.encounter,
        layout.enchantment..layout.known_draw,
        layout.orb..layout.unsettling_lamp,
        layout.run_state..layout.shop_card_price,
        layout.decision..layout.state_len,
    ];
    let mut out = ranges
        .into_iter()
        .flat_map(|range| dense[range].iter().copied())
        .collect::<Vec<_>>();
    *out.last_mut().unwrap() = resume;
    out.extend([bonuses.0 as f32 / 32.0, bonuses.1 as f32 / 32.0]);
    out
}

fn observation_metadata(game: &Game, layout: Layout) -> Vec<f32> {
    let mut out = vec![0.0; layout.characters];
    if let Some(value) = out.get_mut(game.run.character as usize) {
        *value = 1.0;
    }
    out
}

#[cfg(test)]
fn global_features(game: &Game, layout: Layout) -> Vec<f32> {
    global_features_with_bonuses(game, layout, (0, 0))
}

fn global_len(layout: Layout) -> usize {
    layout.encounter + layout.known_draw - layout.enchantment + layout.unsettling_lamp - layout.orb
        + layout.shop_card_price
        - layout.run_state
        + layout.state_len
        - layout.decision
        + 2
}

fn compact_action_values(
    game: &Game,
    layout: Layout,
    action: &Action,
) -> [f32; TOKEN_ACTION_VALUES] {
    let dense = action_features(game, layout, action);
    let mut out = [0.0; TOKEN_ACTION_VALUES];
    out[..ACTION_KINDS].copy_from_slice(&dense[..ACTION_KINDS]);
    out[ACTION_KINDS..ACTION_KINDS + ACTION_VALUES]
        .copy_from_slice(&dense[layout.action_value..layout.action_value + ACTION_VALUES]);
    out
}

fn action_entity_tokens(
    game: &Game,
    content: &Content,
    layout: Layout,
    action: &Action,
) -> Vec<Token> {
    let mut out = Vec::new();
    let target = match action {
        Action::Play { target, .. } | Action::Potion { target, .. } => *target,
        _ => None,
    };
    if let Some((position, enemy)) = target.and_then(|position| {
        game.combat()
            .and_then(|combat| combat.enemies.get(position))
            .map(|enemy| (position, enemy))
    }) {
        out.push(token(
            ENEMY_COLLECTION,
            2,
            enemy.creature.id as usize + 1,
            0,
            position + 1,
        ));
    }
    match action {
        Action::Potion { slot, .. } | Action::DiscardPotion(slot) => {
            if let Some(Some(id)) = game.run.potions.get(*slot) {
                out.push(token(POTION_COLLECTION, 5, *id as usize + 1, 0, slot + 1));
            }
        }
        Action::Path(index) => {
            if let Some(node) = game.map.nodes.get(*index) {
                out.push(token(
                    MAP_COLLECTION,
                    2,
                    room_index(node.room) + 1,
                    node.floor as usize,
                    node.lane as usize + 1,
                ));
            }
        }
        Action::RewardRelic(index) => {
            if let Phase::Rewards(rewards) = &game.phase
                && let Some(&id) = rewards.relics.get(*index)
            {
                out.push(token(
                    RELIC_COLLECTION,
                    if game.toy_box_offers.contains(&id) {
                        TOY_BOX_RELIC_KIND
                    } else {
                        11
                    },
                    id as usize + 1,
                    0,
                    index + 1,
                ));
            }
        }
        Action::RewardPotion(index) => {
            if let Phase::Rewards(rewards) = &game.phase
                && let Some(&id) = rewards.potions.get(*index)
            {
                out.push(token(POTION_COLLECTION, 6, id as usize + 1, 0, index + 1));
            }
        }
        Action::Buy(index) => {
            if let Phase::Shop(items) = &game.phase
                && let Some(item) = items.get(*index)
            {
                match item {
                    ShopItem::Relic(id, price) => {
                        let mut row = token(RELIC_COLLECTION, 12, *id as usize + 1, 0, index + 1);
                        row[8] = *price as f32 / 500.0;
                        out.push(row);
                    }
                    ShopItem::Potion(id, price) => {
                        let mut row = token(POTION_COLLECTION, 7, *id as usize + 1, 0, index + 1);
                        row[8] = *price as f32 / 500.0;
                        out.push(row);
                    }
                    _ => {}
                }
            }
        }
        Action::Event(index) => {
            if matches!(game.phase, Phase::Event(..)) {
                let features = action_features(game, layout, action);
                for kind in 0..EVENT_REQUIREMENTS {
                    if features[layout.action_requirement + kind] != 0.0 {
                        out.push(token(CONTINUATION_COLLECTION, 4, kind + 1, 0, 0));
                    }
                }
                for kind in 0..EVENT_EFFECTS {
                    let value = features[layout.action_effect + kind];
                    if value != 0.0 {
                        let mut row = token(CONTINUATION_COLLECTION, 5, kind + 1, 0, 0);
                        row[8] = value;
                        out.push(row);
                    }
                }
                if let Some(offer) = ancient_offer(game, content, *index) {
                    out.push(token(
                        RELIC_COLLECTION,
                        ANCIENT_RELIC_KIND,
                        offer as usize + 1,
                        index + 1,
                        0,
                    ));
                } else if let Some(&offer) = game.relic_queue.get(*index) {
                    out.push(token(
                        RELIC_COLLECTION,
                        13,
                        offer as usize + 1,
                        0,
                        index + 1,
                    ));
                }
                if let Some(&owned) = game.event_cards.get(*index)
                    && let Some(&id) = game.run.relics.get(owned as usize)
                {
                    out.push(token(RELIC_COLLECTION, 14, id as usize + 1, 0, index + 1));
                }
            }
        }
        Action::CrystalCell(x, y) => out.push(token(
            CRYSTAL_COLLECTION,
            1,
            0,
            *x as usize + 1,
            *y as usize + 1,
        )),
        Action::EventRelic(index, id) => {
            out.push(token(RELIC_COLLECTION, 15, *id as usize + 1, 0, *index + 1))
        }
        _ => {}
    }
    out
}

fn candidate_actions(game: &Game, content: &Content) -> (Vec<Action>, Vec<bool>) {
    let legal = game.actions(content);
    let mut represented = legal.clone();
    if let Phase::Shop(items) = &game.phase {
        for index in 0..items.len() {
            let action = Action::Buy(index);
            if !represented.contains(&action) {
                represented.push(action);
            }
        }
    }
    let mask = represented
        .iter()
        .map(|action| legal.contains(action))
        .collect();
    (represented, mask)
}

fn represented_actions(game: &Game, content: &Content) -> (Vec<Action>, Vec<Action>) {
    let legal = game.actions(content);
    let (represented, _) = candidate_actions(game, content);
    (legal, represented)
}

fn tokenized_candidate(
    game: &Game,
    content: &Content,
    layout: Layout,
    action: &Action,
    legal: bool,
) -> ([f32; TOKEN_ACTION_VALUES], Vec<Token>) {
    let mut values = compact_action_values(game, layout, action);
    values[TOKEN_ACTION_VALUES - 1] = legal as u8 as f32;
    let mut tokens = action_entity_tokens(game, content, layout, action);
    action_card_tokens(game, content, action, &mut values, &mut tokens);
    tokens.sort_by(|left, right| token_cmp(left, right));
    (values, tokens)
}

fn tokenized_action(
    game: &Game,
    content: &Content,
    layout: Layout,
    action: &Action,
) -> ([f32; TOKEN_ACTION_VALUES], Vec<Token>) {
    tokenized_candidate(game, content, layout, action, true)
}

fn event_effect_index(effect: &RunEffect) -> usize {
    match effect {
        RunEffect::Gold(_) => 0,
        RunEffect::RandomGold(..) => 1,
        RunEffect::LoseAllGold => 2,
        RunEffect::Heal(_) => 3,
        RunEffect::HealPercent(_) => 4,
        RunEffect::FullHeal => 5,
        RunEffect::LoseHp(_) => 6,
        RunEffect::MaxHp(_) => 7,
        RunEffect::MaxHpTo(_) => 8,
        RunEffect::AddCard(..) => 9,
        RunEffect::AddRelic(_) => 10,
        RunEffect::AddPotion(_) => 11,
        RunEffect::PotionRewards(..) => 12,
        RunEffect::RandomPotionReward(_) => 13,
        RunEffect::NextRelics(_) => 14,
        RunEffect::RelicOfRarity(_) => 15,
        RunEffect::EventRelic => 16,
        RunEffect::RandomCard(_) => 17,
        RunEffect::RandomRelic(_) => 18,
        RunEffect::DiscardPotion(_) => 19,
        RunEffect::DiscardRandomPotion => 20,
        RunEffect::RemoveCards(..) => 21,
        RunEffect::UpgradeCards(_) => 22,
        RunEffect::UpgradeRandom(_) => 23,
        RunEffect::UpgradeShuffled(_) => 24,
        RunEffect::UpgradeAll => 25,
        RunEffect::DowngradeRandom(_) => 26,
        RunEffect::CloneDeck => 27,
        RunEffect::TransformCards(..) => 28,
        RunEffect::EnchantCards(..) => 29,
        RunEffect::SkipEventRng(_) => 30,
        RunEffect::ChooseCommonCards(..) => 31,
        RunEffect::ChooseRewardCards(..) => 32,
        RunEffect::RemoveRandomCard => 33,
        RunEffect::Options(_) => 34,
        RunEffect::EventAction(_) => 35,
    }
}

fn add_event_option(out: &mut [f32], layout: Layout, option: &EventOption, action: bool) {
    let (requirement, effect, value) = if action {
        (
            layout.action_requirement,
            layout.action_effect,
            layout.action_value,
        )
    } else {
        (
            layout.event_requirement,
            layout.event_effect,
            layout.event_value,
        )
    };
    let (requirement_index, requirement_value) = match option.requirement {
        Requirement::Always => (0, 0.0),
        Requirement::Gold(amount) => (1, amount as f32 / 500.0),
        Requirement::Hp(amount) => (2, amount as f32 / 100.0),
        Requirement::Deck => (3, 1.0),
    };
    out[requirement + requirement_index] += 1.0;
    out[value] += requirement_value;
    for effect_item in option.effects {
        out[effect + event_effect_index(effect_item)] += 1.0;
        out[value + 7] += 0.1;
        match effect_item {
            RunEffect::Gold(amount) => out[value + 1] += *amount as f32 / 500.0,
            RunEffect::RandomGold(low, high) => out[value + 1] += (*low + *high) as f32 / 1_000.0,
            RunEffect::LoseAllGold => out[value + 1] -= 1.0,
            RunEffect::Heal(amount) => out[value + 2] += *amount as f32 / 100.0,
            RunEffect::HealPercent(amount) => out[value + 2] += *amount as f32 / 100.0,
            RunEffect::FullHeal => out[value + 2] += 1.0,
            RunEffect::LoseHp(amount) => out[value + 2] -= *amount as f32 / 100.0,
            RunEffect::MaxHp(amount) | RunEffect::MaxHpTo(amount) => {
                out[value + 3] += *amount as f32 / 100.0
            }
            RunEffect::AddCard(_, amount) => out[value + 4] += *amount as f32 / 10.0,
            RunEffect::AddRelic(_) | RunEffect::AddPotion(_) => out[value + 5] += 0.1,
            RunEffect::RandomCard(_) => out[value + 4] += 0.1,
            RunEffect::RandomRelic(_) => out[value + 5] += 0.1,
            RunEffect::RemoveCards(amount, _)
            | RunEffect::TransformCards(_, amount)
            | RunEffect::ChooseCommonCards(amount, _)
            | RunEffect::ChooseRewardCards(amount, _) => out[value + 6] += *amount as f32 / 10.0,
            RunEffect::UpgradeCards(amount)
            | RunEffect::UpgradeRandom(amount)
            | RunEffect::UpgradeShuffled(amount)
            | RunEffect::DowngradeRandom(amount)
            | RunEffect::NextRelics(amount)
            | RunEffect::RelicOfRarity(amount)
            | RunEffect::DiscardPotion(amount)
            | RunEffect::SkipEventRng(amount) => out[value + 6] += *amount as f32 / 10.0,
            RunEffect::PotionRewards(_, amount) => out[value + 5] += *amount as f32 / 10.0,
            RunEffect::EnchantCards(_, amount, count, _) => {
                out[value + 3] += *amount as f32 / 100.0;
                out[value + 6] += *count as f32 / 10.0;
            }
            RunEffect::Options(options) => {
                options
                    .iter()
                    .for_each(|nested| add_event_option(out, layout, nested, action));
            }
            RunEffect::EventAction(index) => out[value + 6] += *index as f32 / 10.0,
            _ => {}
        }
    }
}

fn state_features(game: &Game, layout: Layout) -> Vec<f32> {
    let mut out = vec![0.0; layout.state_len];
    let run = &game.run;
    let hp = run.hp.max(0) as f32;
    let max_hp = run.max_hp.max(1) as f32;
    let mut i = 0;
    scalar(&mut out, &mut i, hp / max_hp);
    scalar(&mut out, &mut i, max_hp / 100.0);
    scalar(&mut out, &mut i, run.gold as f64 / 500.0);
    scalar(&mut out, &mut i, run.act as f64 / 3.0);
    scalar(&mut out, &mut i, run.floor as f64 / 18.0);
    scalar(&mut out, &mut i, run.energy as f64 / 10.0);
    scalar(&mut out, &mut i, run.draw as f64 / 10.0);
    scalar(&mut out, &mut i, run.orb_slots as f64 / 10.0);
    scalar(&mut out, &mut i, run.card_shop_removals as f64 / 10.0);
    scalar(&mut out, &mut i, run.ascension as f64 / 10.0);
    scalar(&mut out, &mut i, game.happy_flower as f64 / 3.0);
    scalar(&mut out, &mut i, game.tea_set as f64);
    scalar(&mut out, &mut i, game.lasting_candy as f64 / 3.0);
    scalar(&mut out, &mut i, game.paels_wing as f64 / 3.0);
    scalar(&mut out, &mut i, game.silver_crucible as f64 / 10.0);
    scalar(&mut out, &mut i, game.silver_treasures as f64 / 10.0);
    scalar(&mut out, &mut i, game.winged_boots as f64 / 3.0);
    scalar(&mut out, &mut i, game.rest_used as f64 / 255.0);
    scalar(&mut out, &mut i, game.girya as f64 / 3.0);
    scalar(&mut out, &mut i, game.pumpkin_candle as f64 / 10.0);
    scalar(&mut out, &mut i, game.toy_box_combats as f64 / 10.0);
    scalar(&mut out, &mut i, game.nunchaku as f64 / 10.0);
    scalar(&mut out, &mut i, game.pendulum as f64 / 10.0);
    scalar(&mut out, &mut i, game.pen_nib as f64 / 10.0);
    scalar(&mut out, &mut i, game.iron_club as f64 / 10.0);
    scalar(&mut out, &mut i, game.joss_paper as f64 / 10.0);
    scalar(&mut out, &mut i, game.tuning_fork as f64 / 10.0);
    scalar(&mut out, &mut i, game.galactic_dust as f64 / 10.0);
    scalar(&mut out, &mut i, game.book_of_five_rings as f64 / 10.0);
    scalar(&mut out, &mut i, game.ember_tea as f64 / 10.0);
    scalar(&mut out, &mut i, game.sword_of_stone as f64 / 10.0);
    scalar(&mut out, &mut i, game.damage_taken as u8 as f64);
    scalar(&mut out, &mut i, game.maw_bank as u8 as f64);
    scalar(&mut out, &mut i, game.lizard_tail as u8 as f64);
    scalar(&mut out, &mut i, game.cooking as u8 as f64);
    scalar(&mut out, &mut i, game.rarity_offset as f64 / 10.0);
    scalar(&mut out, &mut i, game.potion_odds as f64 / 100.0);
    for odds in game.unknown_odds {
        scalar(&mut out, &mut i, odds as f64 / 100.0);
    }
    scalar(&mut out, &mut i, game.bosses_visited as f64 / 2.0);
    scalar(&mut out, &mut i, game.event_combat as f64 / 10.0);
    scalar(&mut out, &mut i, game.fake_happy_flower as f64 / 3.0);
    scalar(&mut out, &mut i, game.pollinous_core as f64 / 10.0);
    scalar(&mut out, &mut i, game.silken_tress as u8 as f64);
    scalar(&mut out, &mut i, game.bone_tea as u8 as f64);
    scalar(&mut out, &mut i, game.tea_of_discourtesy as u8 as f64);
    scalar(&mut out, &mut i, game.replacing_potion as u8 as f64);
    scalar(&mut out, &mut i, game.rerolled_cards as u8 as f64);
    scalar(&mut out, &mut i, game.removal_price as f64 / 500.0);
    scalar(&mut out, &mut i, game.weak_encounters_left as f64 / 15.0);
    scalar(&mut out, &mut i, game.regular_encounters_left as f64 / 15.0);
    scalar(&mut out, &mut i, game.elite_encounters_left as f64 / 15.0);
    scalar(&mut out, &mut i, game.fishing_rod as f64 / 3.0);
    if let Some(crystal) = &game.crystal {
        scalar(&mut out, &mut i, crystal.remaining as f64 / 10.0);
        scalar(&mut out, &mut i, crystal.big as u8 as f64);
        scalar(
            &mut out,
            &mut i,
            crystal.clear.iter().filter(|&&x| x).count() as f64 / 25.0,
        );
        scalar(&mut out, &mut i, crystal.revealed.len() as f64 / 25.0);
        for (cell, &clear) in crystal.clear.iter().enumerate() {
            out[layout.crystal + cell * 10] = clear as u8 as f32;
            if let Some(item) = crystal.cells[cell]
                && crystal.revealed.contains(&item)
            {
                out[layout.crystal + cell * 10 + 1 + crystal.items[item].4 as usize] = 1.0;
            }
        }
    } else {
        i += 4;
    }
    for &event in &game.visited_events {
        if (event as usize) < layout.events {
            out[layout.visited_event + event as usize] = 1.0;
        }
    }
    let run_state = &mut out[layout.run_state..layout.run_state + RUN_VALUES];
    run_state.copy_from_slice(&[
        game.pending_curse as u8 as f32,
        game.wongo_combats.is_some() as u8 as f32,
        game.wongo_combats.unwrap_or_default() as f32 / 5.0,
        (game.golden_compass == Some(run.act)) as u8 as f32,
        game.astrolabe as u8 as f32,
        game.transform_niche as u8 as f32,
        game.paels_tooth as u8 as f32,
        game.parasol_removal as u8 as f32,
        game.conveyor as u8 as f32,
        game.fake_shop as u8 as f32,
    ]);
    add_cards(&mut out, layout, 0, &run.deck);
    add_cards(&mut out, layout, 6, &game.paels_cards);
    for (index, &relic) in run.relics.iter().enumerate() {
        if (relic as usize) < layout.relics {
            let state = if game.melted_relics.contains(&index) {
                3
            } else {
                0
            };
            out[layout.relic + state * layout.relics + relic as usize] += 1.0;
            if let Some(priority) = game
                .wax_relics
                .iter()
                .filter(|wax| !game.melted_relics.contains(wax))
                .position(|&wax| wax == index)
            {
                out[layout.relic + 2 * layout.relics + relic as usize] +=
                    (priority + 1) as f32 / 4.0;
            }
        }
    }
    for (shared, deques) in [&game.relic_deques, &game.shared_relic_deques]
        .into_iter()
        .enumerate()
    {
        for (rarity, relics) in deques.iter().enumerate() {
            for &relic in relics {
                if (relic as usize) < layout.relics {
                    out[layout.relic_bag
                        + (shared * game.relic_deques.len() + rarity) * layout.relics
                        + relic as usize] += 1.0;
                }
            }
        }
    }
    let relic_trader = matches!(game.phase, Phase::Event(id, _) if Some(id) == layout.relic_trader);
    if !relic_trader || game.event_relic.is_some() {
        let mut hidden = if relic_trader {
            vec![]
        } else {
            game.relic_queue.clone()
        };
        hidden.extend(game.event_relic);
        hidden.sort_unstable();
        hidden.dedup();
        for relic in hidden {
            if game.run.relics.contains(&relic) || relic as usize >= layout.relics {
                continue;
            }
            let Some(rarity) = crate::game::relic_group(relic) else {
                continue;
            };
            if !game.relic_deques[rarity].contains(&relic) {
                out[layout.relic_bag + rarity * layout.relics + relic as usize] += 1.0;
            }
            if layout.shared_relics.contains(&relic)
                && !game.shared_relic_deques[rarity].contains(&relic)
            {
                out[layout.relic_bag
                    + (game.relic_deques.len() + rarity) * layout.relics
                    + relic as usize] += 1.0;
            }
        }
    }
    assert!(game.parasol.len() <= PARASOL_SLOTS);
    for (slot, item) in game.parasol.iter().enumerate() {
        let offset = layout.parasol + slot * PARASOL_VALUES;
        out[offset..offset + PARASOL_VALUES].copy_from_slice(&shop_item_token(item));
    }
    assert!(game.fake_merchant.len() <= FAKE_MERCHANT_SLOTS);
    for (slot, &relic) in game.fake_merchant.iter().enumerate() {
        out[layout.fake_merchant + slot] = (relic as u32 + 1) as f32 / (u16::MAX as f32 + 1.0);
    }
    assert!(game.reward_gold_parts.len() <= GOLD_PART_SLOTS);
    for (slot, &gold) in game.reward_gold_parts.iter().enumerate() {
        out[layout.reward_gold_parts + slot] = gold as f32 / 500.0;
    }
    for potion in run.potions.iter().flatten() {
        if (*potion as usize) < layout.potions {
            out[layout.potion + *potion as usize] += 1.0;
        }
    }
    if let Some(potion) = game.pending_potion
        && (potion as usize) < layout.potions
    {
        out[layout.potion + layout.potions + potion as usize] += 1.0;
    }
    if (run.character as usize) < layout.characters {
        out[layout.character + run.character as usize] = 1.0;
    }
    if (game.act as usize) < layout.acts {
        out[layout.act + game.act as usize] = 1.0;
    }
    out[layout.phase + phase_index(&game.phase)] = 1.0;
    out[layout.room + room_index(game.room)] = 1.0;
    let context = &mut out[layout.decision..layout.decision + 8];
    context[7] = game
        .resume
        .as_ref()
        .map_or(0.0, |phase| (phase_index(phase) + 1) as f32 / PHASES as f32);
    if let Phase::Combat(combat) = &game.phase {
        context[6] = (2.0 * combat.enemy_turn as u8 as f32
            + 4.0 * combat.ending as u8 as f32
            + 8.0 * combat.force_end as u8 as f32)
            / 15.0;
        if let Some(choice) = combat.choice {
            let (filter, filter_value) = filter_features(choice.filter);
            let (op, op_value) = op_features(choice.op);
            context[..7].copy_from_slice(&[
                (pile_index(choice.pile) + 1) as f32 / 5.0,
                filter as f32 / 15.0,
                filter_value as f32 / 1_000_000.0,
                op as f32 / 17.0,
                op_value as f32 / 1_000_000.0,
                choice.remaining as f32 / 10.0,
                (choice.optional as u8 as f32
                    + 2.0 * combat.enemy_turn as u8 as f32
                    + 4.0 * combat.ending as u8 as f32
                    + 8.0 * combat.force_end as u8 as f32)
                    / 15.0,
            ]);
        }
    }
    if let Some(id) = game.bosses[0]
        && (id as usize) < layout.encounters
    {
        out[layout.encounter + id as usize] = 1.0;
    }
    if !game.replaying {
        for (zone, encounters) in [game.encounters.as_slice(), game.elites.as_slice()]
            .into_iter()
            .enumerate()
        {
            for &id in encounters {
                if (id as usize) < layout.encounters {
                    out[layout.encounter + (zone + 1) * layout.encounters + id as usize] += 1.0;
                }
            }
        }
        for (zone, encounter) in [game.last_encounter, game.last_elite]
            .into_iter()
            .enumerate()
        {
            if let Some(id) = encounter
                && (id as usize) < layout.encounters
            {
                out[layout.encounter + (zone + 3) * layout.encounters + id as usize] = 1.0;
            }
        }
    }
    for node in &game.map.nodes {
        let floor = node.floor.saturating_sub(1) as usize;
        let lane = node.lane as usize;
        if floor >= MAP_FLOORS || lane >= MAP_LANES {
            continue;
        }
        let offset = layout.map + (floor * MAP_LANES + lane) * MAP_VALUES;
        out[offset] = 1.0;
        out[offset + 2 + room_index(node.room)] = 1.0;
        for &next in &node.next {
            if let Some(next) = game.map.nodes.get(next)
                && (next.lane as usize) < MAP_LANES
            {
                out[offset + 2 + ROOMS + next.lane as usize] = 1.0;
            }
        }
        out[offset + 2 + ROOMS + MAP_LANES] = (game.fur_coat_act == Some(run.act)
            && game.fur_coat.contains(&(node.lane, node.floor)))
            as u8 as f32;
        out[offset + 3 + ROOMS + MAP_LANES] =
            (game.spoils == Some((node.lane, node.floor))) as u8 as f32;
    }
    if let Some(current) = game.map.current.and_then(|x| game.map.nodes.get(x)) {
        let floor = current.floor.saturating_sub(1) as usize;
        let lane = current.lane as usize;
        if floor < MAP_FLOORS && lane < MAP_LANES {
            out[layout.map + (floor * MAP_LANES + lane) * MAP_VALUES + 1] = 1.0;
        }
    }
    match &game.phase {
        Phase::Combat(combat) => {
            let masters = master_cards(&run.deck);
            for (zone, cards) in [
                &combat.hand,
                &combat.draw,
                &combat.discard,
                &combat.exhaust,
                &combat.offer,
            ]
            .into_iter()
            .enumerate()
            {
                add_combat_cards(
                    &mut out,
                    layout,
                    zone + 1,
                    cards,
                    &run.deck,
                    &masters,
                    &combat.dampened,
                );
            }
            let (top, bottom) = (combat.known_draw_top, combat.known_draw_bottom);
            assert!(top <= KNOWN_DRAW_SLOTS && bottom <= KNOWN_DRAW_SLOTS);
            assert!(top + bottom <= combat.draw.len());
            for (slot, &card) in combat.draw.iter().rev().take(top).enumerate() {
                add_known_draw(
                    &mut out,
                    layout,
                    0,
                    slot,
                    card,
                    card_state(card, &run.deck, &masters, &combat.dampened),
                );
            }
            for (slot, &card) in combat.draw.iter().take(bottom).enumerate() {
                add_known_draw(
                    &mut out,
                    layout,
                    1,
                    slot,
                    card,
                    card_state(card, &run.deck, &masters, &combat.dampened),
                );
            }
            scalar(&mut out, &mut i, combat.energy as f64 / 10.0);
            scalar(&mut out, &mut i, combat.max_energy as f64 / 10.0);
            scalar(&mut out, &mut i, combat.draw_per_turn as f64 / 10.0);
            scalar(&mut out, &mut i, combat.stars as f64 / 10.0);
            scalar(&mut out, &mut i, combat.turn as f64 / 20.0);
            scalar(&mut out, &mut i, combat.orb_slots as f64 / 10.0);
            scalar(
                &mut out,
                &mut i,
                combat.player.hp.max(0) as f64 / combat.player.max_hp.max(1) as f64,
            );
            scalar(&mut out, &mut i, combat.player.block as f64 / 100.0);
            scalar(
                &mut out,
                &mut i,
                combat.osty.hp.max(0) as f64 / combat.osty.max_hp.max(1) as f64,
            );
            scalar(&mut out, &mut i, combat.osty.block as f64 / 100.0);
            let history = combat.history;
            for value in [
                history.cards,
                history.manual_cards,
                history.manual_plays,
                history.attacks,
                history.skills,
                history.powers,
                history.energy,
                history.exhausted,
                history.discarded,
                history.shivs,
                history.stars_gained,
                history.generated,
                history.ethereal,
                history.extra_drawn,
                history.doom_applied,
                history.osty_attacks,
                history.block_gains,
                history.hp_lost,
                history.hp_loss_events,
                history.feral_returns,
            ] {
                scalar(&mut out, &mut i, value as f64 / 20.0);
            }
            for value in [
                history.block_card,
                history.block_card_gains,
                combat.last_cards,
                combat.orbit_spent,
                combat.last_damage,
                combat.drawn,
                combat.lightning_channeled,
                combat.orbs_channeled as i16,
                combat.poisoned as i16,
                combat.card_energy,
                combat.card_stars,
                combat.card_plays as i16,
                combat.enemy_turn as i16,
                combat.ending as i16,
                combat.force_end as i16,
                combat.paels_tears as i16,
                combat.centennial_puzzle as i16,
                combat.demon_tongue as i16,
                combat.permafrost as i16,
                combat.pen_nib as i16,
                combat.ruined_helmet as i16,
                combat.music_box as i16,
                combat.mini_regent as i16,
                combat.rainbow_ring as i16,
                combat.kusarigama as i16,
                combat.unsettling_used as i16,
                combat.diamond_diadem as i16,
            ] {
                scalar(&mut out, &mut i, value as f64 / 20.0);
            }
            out[layout.combat] = combat.osty.max_hp.max(0) as f32 / 500.0;
            out[layout.combat + 1] = combat.burning_sticks as u8 as f32;
            out[layout.combat + 2] = combat.throwing_axe as u8 as f32;
            out[layout.combat + 3] = combat.paels_eye as u8 as f32;
            out[layout.combat + 4] = combat.paels_eye_extra as u8 as f32;
            out[layout.combat + 5] = combat.paels_legion as f32 / 2.0;
            out[layout.combat + 6] = top as f32 / KNOWN_DRAW_SLOTS as f32;
            out[layout.combat + 7] = bottom as f32 / KNOWN_DRAW_SLOTS as f32;
            out[layout.combat + 8] = combat.nightmares.len() as f32 / DELAYED_SLOTS as f32;
            out[layout.combat + 9] = combat.bombs.len() as f32 / DELAYED_SLOTS as f32;
            out[layout.combat + 10] = combat.automation.len() as f32 / DELAYED_SLOTS as f32;
            out[layout.combat + 11] = combat.panache.len() as f32 / DELAYED_SLOTS as f32;
            out[layout.combat + 12] = combat.boulders.len() as f32 / DELAYED_SLOTS as f32;
            if let Some(card) = combat.unsettling_lamp {
                assert!((card as usize) < layout.cards);
                out[layout.unsettling_lamp + card as usize] = 1.0;
            }
            if let Some(card) = combat.history_course {
                add_card(&mut out, layout, 5, card);
            }
            for (slot, &(card, count)) in combat.nightmares.iter().take(DELAYED_SLOTS).enumerate() {
                add_nightmare(&mut out, layout, slot, card, count);
            }
            let piles = [&combat.draw, &combat.hand, &combat.discard, &combat.exhaust];
            let mut dampened = Vec::new();
            for &(instance, upgrades) in &combat.dampened {
                for (zone, pile) in piles.iter().enumerate() {
                    if let Some(&card) = pile.iter().find(|card| card.instance == instance) {
                        dampened.push((zone, card, upgrades));
                        break;
                    }
                }
            }
            dampened.sort_by_key(|(zone, card, upgrades)| (*zone, card_key(card), *upgrades));
            out[layout.combat + 13] = dampened.len() as f32 / DELAYED_SLOTS as f32;
            for (slot, (zone, card, upgrades)) in
                dampened.into_iter().take(DELAYED_SLOTS).enumerate()
            {
                add_dampened(&mut out, layout, slot, zone, card, upgrades);
            }
            for (slot, &(turns, amount)) in combat.bombs.iter().take(DELAYED_SLOTS).enumerate() {
                let offset = layout.delayed + slot * 3;
                out[offset] = 1.0;
                out[offset + 1] = turns as f32 / 3.0;
                out[offset + 2] = amount as f32 / 100.0;
            }
            for &(turns, energy) in &combat.automation {
                if (1..=10).contains(&turns) {
                    out[layout.delayed + 3 * DELAYED_SLOTS + turns as usize - 1] += energy as f32;
                }
            }
            for (slot, &(cards, damage, start)) in
                combat.panache.iter().take(DELAYED_SLOTS).enumerate()
            {
                let offset = layout.delayed + 3 * DELAYED_SLOTS + 10 + slot * 4;
                out[offset] = 1.0;
                out[offset + 1] = cards as f32 / 5.0;
                out[offset + 2] = damage as f32 / 20.0;
                out[offset + 3] = start as f32 / 20.0;
            }
            for (slot, &amount) in combat.boulders.iter().take(DELAYED_SLOTS).enumerate() {
                let offset = layout.delayed + 7 * DELAYED_SLOTS + 10 + slot * 2;
                out[offset] = 1.0;
                out[offset + 1] = amount as f32 / 100.0;
            }
            for &power in &combat.player.powers {
                add_power(&mut out, layout.power, 2 + ENEMY_SLOTS, 0, power);
            }
            for &power in &combat.osty.powers {
                add_power(&mut out, layout.power, 2 + ENEMY_SLOTS, 1, power);
            }
            for &power in &combat.power_snapshot {
                add_power(&mut out, layout.power_snapshot, SNAPSHOT_ACTORS, 0, power);
            }
            for (slot, powers) in combat
                .enemy_power_snapshot
                .iter()
                .take(ENEMY_SLOTS)
                .enumerate()
            {
                for &power in powers {
                    add_power(
                        &mut out,
                        layout.power_snapshot,
                        SNAPSHOT_ACTORS,
                        1 + slot,
                        power,
                    );
                }
            }
            for (slot, enemy) in combat.enemies.iter().take(ENEMY_SLOTS).enumerate() {
                let id = enemy.creature.id as usize;
                if id < layout.enemies {
                    out[layout.enemy + slot * layout.enemies + id] = 1.0;
                }
                let offset = layout.enemy_value + slot * ENEMY_VALUES;
                out[offset] = enemy.creature.hp.max(0) as f32 / enemy.creature.max_hp.max(1) as f32;
                out[offset + 1] = enemy.creature.max_hp as f32 / 500.0;
                out[offset + 2] = enemy.creature.block as f32 / 100.0;
                out[offset + 3] = enemy.move_index.min(20) as f32 / 20.0;
                out[offset + 4] = if enemy.last_move == usize::MAX {
                    -0.05
                } else {
                    enemy.last_move.min(20) as f32 / 20.0
                };
                out[offset + 5] = enemy.repeats as f32 / 10.0;
                out[offset + 6] = enemy.stunned as u8 as f32;
                out[offset + 7] = enemy.value as f32 / 100.0;
                out[offset + 8] = combat.hits.get(slot).copied().unwrap_or_default() as f32 / 20.0;
                let history = layout.enemy_history + slot * ENEMY_HISTORY_VALUES;
                for &prior in &enemy.move_history {
                    if prior < 20 {
                        out[history + prior] += 0.1;
                    }
                }
                for (position, &prior) in enemy.move_history.iter().rev().take(3).enumerate() {
                    out[history + 20 + position] = (prior + 1) as f32 / 20.0;
                }
                for &power in &enemy.creature.powers {
                    add_power(&mut out, layout.power, 2 + ENEMY_SLOTS, 2 + slot, power);
                }
            }
            let powers = |values: &[Power]| {
                let mut values = values.iter().copied().map(power_token).collect::<Vec<_>>();
                values.sort_by(|left, right| token_cmp(left, right));
                values
            };
            let mut groups = Vec::new();
            for (index, enemy) in combat.enemies.iter().enumerate().skip(ENEMY_SLOTS) {
                let token = enemy_token(enemy, combat.hits.get(index).copied().unwrap_or_default());
                let current = powers(&enemy.creature.powers);
                let snapshot = combat
                    .enemy_power_snapshot
                    .get(index)
                    .map_or_else(Vec::new, |snapshot| powers(snapshot));
                if let Some((_, count, prior, prior_current, prior_snapshot)) = groups.last_mut()
                    && *prior == token
                    && *prior_current == current
                    && *prior_snapshot == snapshot
                {
                    *count += 1;
                } else {
                    groups.push((index, 1usize, token, current, snapshot));
                }
            }
            assert!(groups.len() <= OVERFLOW_ENEMY_GROUPS);
            let mut power_slot = 0;
            for (slot, (start, count, enemy, current, snapshot)) in groups.into_iter().enumerate() {
                let offset = layout.enemy_overflow + slot * OVERFLOW_ENEMY_VALUES;
                out[offset] = (start + 1) as f32 / 65536.0;
                out[offset + 1] = count as f32 / 256.0;
                out[offset + 2..offset + OVERFLOW_ENEMY_VALUES].copy_from_slice(&enemy);
                for (snapshot, powers) in [(0.0, current), (1.0, snapshot)] {
                    for power in powers {
                        assert!(power_slot < OVERFLOW_POWER_SLOTS);
                        let offset =
                            layout.enemy_overflow_power + power_slot * OVERFLOW_POWER_VALUES;
                        out[offset] = (slot + 1) as f32 / OVERFLOW_ENEMY_GROUPS as f32;
                        out[offset + 1] = snapshot;
                        out[offset + 2..offset + OVERFLOW_POWER_VALUES].copy_from_slice(&power);
                        power_slot += 1;
                    }
                }
            }
            for (slot, orb) in combat.orbs.iter().take(ORB_SLOTS).enumerate() {
                if (orb.id as usize) < layout.orbs {
                    let offset = layout.orb + slot * (layout.orbs + 1);
                    out[offset + orb.id as usize] = 1.0;
                    out[offset + layout.orbs] = orb.value as f32 / 100.0;
                }
            }
        }
        Phase::Rewards(rewards) => {
            assert!(rewards.card_rewards.len() <= 4);
            for (slot, reward) in rewards.card_rewards.iter().enumerate() {
                let (kind, detail) = match reward {
                    CardReward::Standard(room) => {
                        (1, (room_index(*room) + 1) as f32 / ROOMS as f32)
                    }
                    CardReward::Fixed(character, rarity) => (
                        2,
                        (*character as usize * 8 + *rarity as usize + 1) as f32
                            / (layout.characters * 8) as f32,
                    ),
                    CardReward::Kaleidoscope => (3, 0.0),
                    CardReward::Crystal(rarity) => (4, (*rarity as usize + 1) as f32 / 8.0),
                };
                out[SCALARS - 10 + slot * 2] = kind as f32 / 4.0;
                out[SCALARS - 9 + slot * 2] = detail;
            }
            scalar(&mut out, &mut i, rewards.gold as f64 / 500.0);
            scalar(&mut out, &mut i, rewards.removals as f64 / 10.0);
        }
        Phase::Shop(_) => {}
        Phase::Event(id, options) if (*id as usize) < layout.events => {
            out[layout.event + *id as usize] = 1.0;
            for option in options {
                add_event_option(&mut out, layout, option, false);
            }
            if Some(*id) == layout.slippery_bridge {
                out[layout.event_state] = game.event_data[0] as f32 / 1_000.0;
                for &instance in &game.event_cards {
                    if let Some(&card) = run.deck.iter().find(|card| card.instance == instance) {
                        add_card(&mut out, layout, 7, card);
                    }
                }
            } else if ![
                layout.tinker_time,
                layout.stone_of_all_time,
                layout.future_of_potions,
            ]
            .contains(&Some(*id))
                && !layout.ancient_events.contains(&Some(*id))
            {
                for (slot, value) in game.event_data.into_iter().enumerate() {
                    out[layout.event_state + slot] = value as f32 / 1_000.0;
                }
            }
            if Some(*id) != layout.slippery_bridge {
                for &index in &game.event_cards {
                    if let Some(&relic) = run.relics.get(index as usize)
                        && (relic as usize) < layout.relics
                    {
                        out[layout.relic + 5 * layout.relics + relic as usize] += 1.0;
                    }
                }
            }
            if Some(*id) == layout.tinker_time {
                out[SCALARS - 10] = game.event_data[3] as f32 / 3.0;
                for &offer in game.event_data.iter().take(options.len()) {
                    if (1..=9).contains(&offer) {
                        out[SCALARS - 10 + offer as usize] = 1.0;
                    }
                }
            }
        }
        Phase::RemoveCards(count, price, optional) => {
            scalar(&mut out, &mut i, *count as f64 / 10.0);
            scalar(&mut out, &mut i, *price as f64 / 500.0);
            scalar(&mut out, &mut i, *optional as u8 as f64);
        }
        Phase::UpgradeCards(count, optional) => {
            scalar(&mut out, &mut i, *count as f64 / 10.0);
            scalar(&mut out, &mut i, *optional as u8 as f64);
        }
        Phase::TransformCards(target, count, optional) => {
            scalar(
                &mut out,
                &mut i,
                target.map_or(0, |id| id + 1) as f64 / 1000.0,
            );
            scalar(&mut out, &mut i, *count as f64 / 10.0);
            scalar(&mut out, &mut i, *optional as u8 as f64);
        }
        Phase::EnchantCards(enchantment, amount, count, card_type, optional) => {
            out[layout.enchantment + *enchantment as usize] += 1.0;
            scalar(&mut out, &mut i, *amount as f64 / 100.0);
            scalar(&mut out, &mut i, *count as f64 / 10.0);
            scalar(
                &mut out,
                &mut i,
                card_type.map_or(0, |kind| kind as u8 + 1) as f64 / 10.0,
            );
            scalar(&mut out, &mut i, *optional as u8 as f64);
        }
        Phase::ChooseCards(_, count, reward) => {
            scalar(&mut out, &mut i, *count as f64 / 10.0);
            scalar(&mut out, &mut i, *reward as u8 as f64);
        }
        Phase::ChooseBundles(_) => {}
        _ => {}
    }
    add_continuation(&mut out, layout, game);
    assert!(i <= SCALARS - 10);
    bound(&mut out);
    out
}

fn action_kind(action: &Action) -> usize {
    match action {
        Action::Play { .. } => 0,
        Action::Potion { .. } => 1,
        Action::DiscardPotion(_) => 2,
        Action::Choose(_) => 3,
        Action::EndTurn => 4,
        Action::Path(_) => 5,
        Action::RewardGold => 6,
        Action::RewardCard(_) => 7,
        Action::RewardRelic(_) => 8,
        Action::RewardPotion(_) => 9,
        Action::RewardRemove => 10,
        Action::RerollCards => 11,
        Action::SacrificeCards => 12,
        Action::Buy(_) => 13,
        Action::Rest => 14,
        Action::Hatch => 15,
        Action::Lift => 16,
        Action::Cook => 17,
        Action::Kindle => 18,
        Action::Dig => 19,
        Action::Smith(_) => 20,
        Action::Event(_) => 21,
        Action::CrystalCell(..) => 22,
        Action::CrystalTool(_) => 23,
        Action::EventRelic(..) => 24,
        Action::EventCard(..) => 25,
        Action::Enchant(_) => 26,
        Action::RemoveCard(_) => 27,
        Action::Clone => 28,
        Action::Cancel => 29,
        Action::Done => 30,
        Action::Leave => 31,
    }
}

fn action_card(out: &mut [f32], layout: Layout, card: Card) {
    if (card.id as usize) < layout.cards {
        out[layout.action_card + card.id as usize] += 1.0;
    }
    if let Some(enchantment) = card.enchantment {
        out[layout.action_enchantment + enchantment as usize] += 1.0;
    }
    let values = &mut out[layout.action_value..layout.action_value + ACTION_VALUES];
    values[1] += card.upgrades as f32 / 3.0;
    values[2] += card.cost_delta as f32 / 10.0;
    values[3] += card.value as f32 / 100.0;
    values[4] += card.free as u8 as f32;
    values[5] += card.enchantment_amount as f32 / 100.0;
    values[10] += card.replays as f32 / 10.0;
    values[11] += card.cost_override.map_or(0, |cost| cost as i16 + 129) as f32 / 256.0;
    values[12] += card.flags as f32 / u16::MAX as f32;
    values[13] += card.turn_flags as f32 / u16::MAX as f32;
    values[14] += card.enchantment_value as f32 / 100.0;
    values[15] += card.variant as f32 / 10.0;
}

fn action_combat_card(out: &mut [f32], layout: Layout, game: &Game, card: Card) {
    action_card(out, layout, card);
    let combat = game.combat().unwrap();
    let masters = master_cards(&game.run.deck);
    let state = card_state(card, &game.run.deck, &masters, &combat.dampened);
    out[layout.action_card_state..layout.action_card_state + 2].copy_from_slice(&state[..2]);
}

fn action_target(out: &mut [f32], layout: Layout, combat: &Combat, target: Option<usize>) {
    let Some((target, enemy)) =
        target.and_then(|target| combat.enemies.get(target).map(|x| (target, x)))
    else {
        return;
    };
    if (enemy.creature.id as usize) < layout.enemies {
        out[layout.action_enemy + enemy.creature.id as usize] = 1.0;
    }
    let values = &mut out[layout.action_value..layout.action_value + ACTION_VALUES];
    values[6] = enemy.creature.hp.max(0) as f32 / enemy.creature.max_hp.max(1) as f32;
    values[7] = enemy.creature.block as f32 / 100.0;
    values[8] = enemy.move_index as f32 / 20.0;
    values[9] = (target + 1) as f32 / ENEMY_SLOTS as f32;
}

fn choice_card(combat: &Combat, index: usize) -> Option<Card> {
    let choice = combat.choice?;
    match choice.pile {
        Pile::Draw => combat.draw.get(index),
        Pile::Hand => combat.hand.get(index),
        Pile::Discard => combat.discard.get(index),
        Pile::Exhaust => combat.exhaust.get(index),
        Pile::Offer => combat.offer.get(index),
    }
    .copied()
}

fn action_features(game: &Game, layout: Layout, action: &Action) -> Vec<f32> {
    let mut out = vec![0.0; layout.action_len];
    out[action_kind(action)] = 1.0;
    let value = |out: &mut [f32], slot: usize, x: f32| {
        out[layout.action_value + slot] = x;
    };
    match action {
        Action::Play { hand, target } => {
            if let Some(combat) = game.combat()
                && let Some(&card) = combat.hand.get(*hand)
            {
                action_combat_card(&mut out, layout, game, card);
                action_target(&mut out, layout, combat, *target);
            }
        }
        Action::Potion { slot, target } => {
            if let Some(Some(id)) = game.run.potions.get(*slot)
                && (*id as usize) < layout.potions
            {
                out[layout.action_potion + *id as usize] = 1.0;
            }
            if let Some(combat) = game.combat() {
                action_target(&mut out, layout, combat, *target);
            }
            value(&mut out, 0, *slot as f32 / 3.0);
        }
        Action::DiscardPotion(slot) => {
            if let Some(Some(id)) = game.run.potions.get(*slot)
                && (*id as usize) < layout.potions
            {
                out[layout.action_potion + *id as usize] = 1.0;
            }
            value(&mut out, 0, *slot as f32 / 3.0);
        }
        Action::Choose(index) => {
            if let Some(combat) = game.combat() {
                if let Some(card) = choice_card(combat, *index) {
                    action_combat_card(&mut out, layout, game, card);
                }
                if combat
                    .choice
                    .is_some_and(|choice| choice.pile == Pile::Draw)
                {
                    let position = if *index < combat.known_draw_bottom {
                        index + 1
                    } else if *index >= combat.draw.len() - combat.known_draw_top {
                        KNOWN_DRAW_SLOTS + combat.draw.len() - index
                    } else {
                        0
                    };
                    value(
                        &mut out,
                        0,
                        position as f32 / (2 * KNOWN_DRAW_SLOTS + 1) as f32,
                    );
                }
            } else {
                match &game.phase {
                    Phase::ChooseCards(cards, ..) => {
                        if let Some(&card) = cards.get(*index) {
                            action_card(&mut out, layout, card);
                        }
                    }
                    Phase::ChooseBundles(bundles) => {
                        value(&mut out, 0, (*index + 1) as f32 / CARD_SLOTS as f32);
                        if let Some(cards) = bundles.get(*index) {
                            for &card in cards {
                                action_card(&mut out, layout, card);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Action::Path(index) => {
            if let Some(node) = game.map.nodes.get(*index) {
                out[layout.action_room + room_index(node.room)] = 1.0;
                value(&mut out, 0, node.floor as f32 / 18.0);
                value(&mut out, 1, node.lane as f32 / 8.0);
            }
        }
        Action::RewardCard(index) => {
            if let Phase::Rewards(rewards) = &game.phase
                && let Some(&card) = rewards.cards.get(*index)
            {
                action_card(&mut out, layout, card);
            }
        }
        Action::RewardRelic(index) => {
            if let Phase::Rewards(rewards) = &game.phase
                && let Some(&id) = rewards.relics.get(*index)
                && (id as usize) < layout.relics
            {
                out[layout.action_relic + id as usize] = 1.0;
            }
        }
        Action::RewardPotion(index) => {
            if let Phase::Rewards(rewards) = &game.phase
                && let Some(&id) = rewards.potions.get(*index)
                && (id as usize) < layout.potions
            {
                out[layout.action_potion + id as usize] = 1.0;
            }
        }
        Action::Buy(index) => {
            if let Phase::Shop(items) = &game.phase
                && let Some(item) = items.get(*index)
            {
                match item {
                    ShopItem::Card(card, price) => {
                        action_card(&mut out, layout, *card);
                        value(&mut out, 9, *price as f32 / 500.0);
                    }
                    ShopItem::Relic(id, price) => {
                        if (*id as usize) < layout.relics {
                            out[layout.action_relic + *id as usize] = 1.0;
                        }
                        value(&mut out, 9, *price as f32 / 500.0);
                    }
                    ShopItem::Potion(id, price) => {
                        if (*id as usize) < layout.potions {
                            out[layout.action_potion + *id as usize] = 1.0;
                        }
                        value(&mut out, 9, *price as f32 / 500.0);
                    }
                    ShopItem::Remove(price) => value(&mut out, 9, *price as f32 / 500.0),
                }
            }
        }
        Action::Smith(index) | Action::Enchant(index) | Action::RemoveCard(index) => {
            if let Some(&card) = game.run.deck.get(*index) {
                action_card(&mut out, layout, card);
            }
        }
        Action::Event(index) => {
            let semantic = match &game.phase {
                Phase::Event(id, options)
                    if Some(*id) == layout.tinker_time && *index < options.len() =>
                {
                    game.event_data[*index] as f32 / 10.0
                }
                _ => *index as f32 / 20.0,
            };
            value(&mut out, 0, semantic);
            if let Phase::Event(_, options) = &game.phase
                && let Some(option) = options.get(*index)
            {
                add_event_option(&mut out, layout, option, true);
            }
            if let Phase::Event(id, _) = &game.phase
                && Some(*id) == layout.relic_trader
            {
                if let Some(&relic) = game.relic_queue.get(*index)
                    && (relic as usize) < layout.relics
                {
                    out[layout.action_relic + relic as usize] += 1.0;
                }
                if let Some(&owned) = game.event_cards.get(*index)
                    && let Some(&relic) = game.run.relics.get(owned as usize)
                    && (relic as usize) < layout.relics
                {
                    out[layout.action_relic + relic as usize] -= 1.0;
                }
            }
            if let Phase::Event(id, _) = &game.phase
                && Some(*id) == layout.slippery_bridge
                && let Some(&card) = game
                    .run
                    .deck
                    .iter()
                    .find(|card| card.instance == game.event_data[1] as u32)
            {
                action_card(&mut out, layout, card);
            }
        }
        Action::CrystalCell(x, y) => {
            value(&mut out, 0, *x as f32 / 10.0);
            value(&mut out, 1, *y as f32 / 10.0);
        }
        Action::CrystalTool(tool) => value(&mut out, 0, *tool as u8 as f32),
        Action::EventRelic(index, id) => {
            if (*id as usize) < layout.relics {
                out[layout.action_relic + *id as usize] = 1.0;
            }
            value(&mut out, 0, *index as f32 / 20.0);
        }
        Action::EventCard(index, card) => {
            action_card(&mut out, layout, *card);
            value(&mut out, 0, *index as f32 / 20.0);
        }
        _ => {}
    }
    bound(&mut out);
    out
}

fn bound(values: &mut [f32]) {
    for value in values {
        *value = if value.is_finite() {
            value.clamp(-100.0, 100.0)
        } else {
            0.0
        };
    }
}

fn resample_crystal(crystal: &mut CrystalSphere, rng: &mut Rng) {
    let mut hidden = vec![
        (0, 4, 4),
        (1, 1, 3),
        (1, 1, 3),
        (2, 2, 2),
        (3, 2, 2),
        (4, 2, 2),
        (5, 2, 2),
        (6, 2, 2),
        (7, 1, 1),
        (7, 1, 1),
        (7, 1, 1),
        (7, 1, 1),
        (7, 1, 1),
        (8, 2, 1),
        (8, 2, 1),
    ];
    let mut visible = crystal
        .revealed
        .iter()
        .filter_map(|&item| crystal.items.get(item).copied())
        .collect::<Vec<_>>();
    visible.sort_unstable();
    for &(_, _, width, height, kind) in &visible {
        if let Some(index) = hidden
            .iter()
            .position(|item| *item == (kind, width, height))
        {
            hidden.remove(index);
        }
    }
    crystal.cells.fill(None);
    crystal.items.clear();
    crystal.revealed.clear();
    for item in visible {
        let id = crystal.items.len();
        crystal.items.push(item);
        crystal.revealed.push(id);
        let (x, y, width, height, _) = item;
        for x in x..x + width {
            for y in y..y + height {
                crystal.cells[x as usize * 11 + y as usize] = Some(id);
            }
        }
    }
    for (kind, width, height) in hidden {
        let mut positions = Vec::new();
        for x in 0..=11 - width as usize {
            for y in 0..=11 - height as usize {
                let cells = (x..x + width as usize)
                    .flat_map(|x| (y..y + height as usize).map(move |y| x * 11 + y));
                if cells.clone().all(|cell| {
                    let (x, y) = (cell / 11, cell % 11);
                    !(x + y <= 2 || 10 - x + y <= 2 || x + 10 - y <= 2 || 20 - x - y <= 2)
                        && crystal.cells[cell].is_none()
                }) && !cells.clone().all(|cell| crystal.clear[cell])
                {
                    positions.push((x, y));
                }
            }
        }
        if positions.is_empty() {
            break;
        }
        let (x, y) = positions[rng.below(positions.len() as u32) as usize];
        let id = crystal.items.len();
        crystal.items.push((x as u8, y as u8, width, height, kind));
        for x in x..x + width as usize {
            for y in y..y + height as usize {
                crystal.cells[x * 11 + y] = Some(id);
            }
        }
    }
}

fn resample_hidden(game: &mut Game, content: &Content, seed: u64) {
    game.seed = seed as u32;
    game.rngs = Rngs::from_seed(seed);
    if game.event_rng.is_some() {
        game.event_rng = Some(Rng::from_seed(seed ^ 0x4556_454e_5452_4e47));
    }
    let mut rng = Rng::from_seed(seed ^ 0x4849_4444_454e_524e);
    if let Phase::Combat(combat) = &mut game.phase {
        let (top, bottom) = (combat.known_draw_top, combat.known_draw_bottom);
        assert!(top <= KNOWN_DRAW_SLOTS && bottom <= KNOWN_DRAW_SLOTS);
        assert!(top + bottom <= combat.draw.len());
        let end = combat.draw.len() - top;
        let unknown = &mut combat.draw[bottom..end];
        unknown.sort_by_key(card_key);
        rng.shuffle(unknown);
    }
    let relic_trader = matches!(game.phase, Phase::Event(id, _) if content.events[id as usize].id == "EVENT.RELIC_TRADER");
    let mut hidden_relics = game.event_relic.take().into_iter().collect::<Vec<_>>();
    if !relic_trader {
        hidden_relics.append(&mut game.relic_queue);
    }
    hidden_relics.sort_unstable();
    hidden_relics.dedup();
    for relic in hidden_relics {
        if game.run.relics.contains(&relic) {
            continue;
        }
        let Some(rarity) = crate::game::relic_group(relic) else {
            continue;
        };
        if !game.relic_deques[rarity].contains(&relic) {
            game.relic_deques[rarity].push(relic);
        }
        if content.acts[0].relics.contains(&relic)
            && !game.shared_relic_deques[rarity].contains(&relic)
        {
            game.shared_relic_deques[rarity].push(relic);
        }
    }
    if !game.replaying {
        game.encounters.sort_unstable();
        game.elites.sort_unstable();
    }
    game.events.sort_unstable();
    rng.shuffle(&mut game.events);
    for relics in game
        .relic_deques
        .iter_mut()
        .chain(&mut game.shared_relic_deques)
    {
        relics.sort_unstable();
        rng.shuffle(relics);
    }
    if let Some(crystal) = &mut game.crystal {
        resample_crystal(crystal, &mut rng);
    }
    if game.run.ascension >= 10 && game.run.act == 3 && game.bosses_visited <= 1 {
        let bosses = content.acts[game.act as usize]
            .bosses
            .iter()
            .copied()
            .filter(|boss| Some(*boss) != game.bosses[0])
            .collect::<Vec<_>>();
        if !bosses.is_empty() {
            game.bosses[1] = Some(bosses[rng.next() as usize % bosses.len()]);
        }
    }
}

#[cfg(any(feature = "python", test))]
fn potential(game: &Game) -> f32 {
    if matches!(game.phase, Phase::Won | Phase::Dead) {
        return 0.0;
    }
    let progress = ((game.run.act.saturating_sub(1) as f32 * 17.0 + game.run.floor as f32) / 52.0)
        .clamp(0.0, 1.0);
    let hp = game
        .combat()
        .map_or(game.run.hp, |combat| combat.player.hp)
        .max(0) as f32
        / game.run.max_hp.max(1) as f32;
    let combat = game.combat().map_or(0.0, |combat| {
        let current: i32 = combat
            .enemies
            .iter()
            .map(|enemy| enemy.creature.hp.max(0) as i32)
            .sum();
        let maximum: i32 = combat
            .enemies
            .iter()
            .map(|enemy| enemy.creature.max_hp.max(1) as i32)
            .sum();
        1.0 - current as f32 / maximum.max(1) as f32
    });
    0.7 * progress + 0.2 * hp + 0.1 * combat
}

struct ContentHasher(u64);

impl Hasher for ContentHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&value.to_le_bytes())
    }
    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes())
    }
    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes())
    }
    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes())
    }
    fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes())
    }
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64)
    }
    fn write_i8(&mut self, value: i8) {
        self.write(&value.to_le_bytes())
    }
    fn write_i16(&mut self, value: i16) {
        self.write(&value.to_le_bytes())
    }
    fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes())
    }
    fn write_i64(&mut self, value: i64) {
        self.write(&value.to_le_bytes())
    }
    fn write_i128(&mut self, value: i128) {
        self.write(&value.to_le_bytes())
    }
    fn write_isize(&mut self, value: isize) {
        self.write_i64(value as i64)
    }
}

fn content_fingerprint(content: &Content) -> u64 {
    let mut hash = ContentHasher(0xcbf29ce484222325);
    content.hash(&mut hash);
    hash.finish()
}

struct TransformerLayer {
    qkv_w: Vec<f32>,
    qkv_b: Vec<f32>,
    out_w: Vec<f32>,
    out_b: Vec<f32>,
    norm1_w: Vec<f32>,
    norm1_b: Vec<f32>,
    norm2_w: Vec<f32>,
    norm2_b: Vec<f32>,
    linear1_w: Vec<f32>,
    linear1_b: Vec<f32>,
    linear2_w: Vec<f32>,
    linear2_b: Vec<f32>,
}

fn read_transformer_layers(
    input: &mut &[u8],
    count: usize,
    width: usize,
    feedforward: usize,
) -> io::Result<Vec<TransformerLayer>> {
    (0..count)
        .map(|_| {
            Ok(TransformerLayer {
                qkv_w: read_f32s(input, 3 * width * width)?,
                qkv_b: read_f32s(input, 3 * width)?,
                out_w: read_f32s(input, width * width)?,
                out_b: read_f32s(input, width)?,
                norm1_w: read_f32s(input, width)?,
                norm1_b: read_f32s(input, width)?,
                norm2_w: read_f32s(input, width)?,
                norm2_b: read_f32s(input, width)?,
                linear1_w: read_f32s(input, feedforward * width)?,
                linear1_b: read_f32s(input, feedforward)?,
                linear2_w: read_f32s(input, width * feedforward)?,
                linear2_b: read_f32s(input, width)?,
            })
        })
        .collect()
}

pub struct ValueModel<'a> {
    content: &'a Content,
    layout: Layout,
    width: usize,
    heads: usize,
    embedding_sizes: Vec<usize>,
    embedding_offsets: Vec<usize>,
    embedding: Vec<f32>,
    numeric: Vec<f32>,
    token_norm_w: Vec<f32>,
    token_norm_b: Vec<f32>,
    layers: Vec<TransformerLayer>,
    action_layers: Vec<TransformerLayer>,
    action_value_w: Vec<f32>,
    action_value_b: Vec<f32>,
    action_norm_w: Vec<f32>,
    action_norm_b: Vec<f32>,
    state_state: Vec<f32>,
    decision_state: Vec<f32>,
    value_w1: Vec<f32>,
    value_b1: Vec<f32>,
    value_w2: Vec<f32>,
    value_b2: f32,
    temperature: f32,
    bias: f32,
}

impl<'a> ValueModel<'a> {
    pub fn load(path: impl AsRef<Path>, content: &'a Content) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        let mut input = bytes.as_slice();
        if take_bytes(&mut input, 8)? != MAGIC {
            return Err(invalid("invalid value model"));
        }
        if read_u32(&mut input)? != VALUE_MODEL_VERSION || read_u32(&mut input)? != VERSION {
            return Err(invalid("unsupported value model version"));
        }
        if read_u64(&mut input)? != content_fingerprint(content) {
            return Err(invalid("value model content mismatch"));
        }
        let layout = Layout::new(content);
        let width = read_u32(&mut input)? as usize;
        let layer_count = read_u32(&mut input)? as usize;
        let action_layer_count = read_u32(&mut input)? as usize;
        let heads = read_u32(&mut input)? as usize;
        let feedforward = read_u32(&mut input)? as usize;
        let metadata = [
            read_u32(&mut input)? as usize,
            read_u32(&mut input)? as usize,
            read_u32(&mut input)? as usize,
            read_u32(&mut input)? as usize,
            read_u32(&mut input)? as usize,
        ];
        if width == 0
            || layer_count == 0
            || action_layer_count == 0
            || heads == 0
            || width % heads != 0
            || feedforward == 0
            || width > 4096
            || layer_count > 64
            || action_layer_count > 64
            || metadata
                != [
                    layout.characters,
                    ACTION_CARD_COLLECTION + 1,
                    TOKEN_CATEGORICAL,
                    TOKEN_NUMERIC,
                    TOKEN_ACTION_VALUES,
                ]
        {
            return Err(invalid("value model shape mismatch"));
        }
        let temperature = read_f32(&mut input)?;
        let bias = read_f32(&mut input)?;
        if !temperature.is_finite() || temperature <= 0.0 || !bias.is_finite() {
            return Err(invalid("invalid value calibration"));
        }
        let vocab = [
            ACTION_CARD_COLLECTION + 1,
            32,
            u16::MAX as usize + 2,
            u16::MAX as usize + 2,
            u16::MAX as usize + 2,
            7,
            ENCHANTMENTS + 1,
            4,
        ];
        let mut embedding_sizes = vocab
            .iter()
            .map(|size| (*size).min(257))
            .collect::<Vec<_>>();
        embedding_sizes.extend(
            vocab
                .iter()
                .map(|size| ((*size - 1).div_ceil(256) + 1).max(2)),
        );
        let mut embedding_offsets = Vec::with_capacity(embedding_sizes.len());
        let mut embedding_rows = 0;
        for &size in &embedding_sizes {
            embedding_offsets.push(embedding_rows);
            embedding_rows += size;
        }
        let embedding = read_f32s(&mut input, embedding_rows * width)?;
        let numeric = read_f32s(&mut input, width * TOKEN_NUMERIC)?;
        let token_norm_w = read_f32s(&mut input, width)?;
        let token_norm_b = read_f32s(&mut input, width)?;
        let layers = read_transformer_layers(&mut input, layer_count, width, feedforward)?;
        let action_layers =
            read_transformer_layers(&mut input, action_layer_count, width, feedforward)?;
        let action_value_w = read_f32s(&mut input, width * TOKEN_ACTION_VALUES)?;
        let action_value_b = read_f32s(&mut input, width)?;
        let action_norm_w = read_f32s(&mut input, width)?;
        let action_norm_b = read_f32s(&mut input, width)?;
        let state_state = read_f32s(&mut input, width)?;
        let decision_state = read_f32s(&mut input, width)?;
        let value_w1 = read_f32s(&mut input, width * width)?;
        let value_b1 = read_f32s(&mut input, width)?;
        let value_w2 = read_f32s(&mut input, width)?;
        let value_b2 = read_f32(&mut input)?;
        if !input.is_empty() {
            return Err(invalid("trailing value model data"));
        }
        Ok(Self {
            content,
            layout,
            width,
            heads,
            embedding_sizes,
            embedding_offsets,
            embedding,
            numeric,
            token_norm_w,
            token_norm_b,
            layers,
            action_layers,
            action_value_w,
            action_value_b,
            action_norm_w,
            action_norm_b,
            state_state,
            decision_state,
            value_w1,
            value_b1,
            value_w2,
            value_b2,
            temperature,
            bias,
        })
    }

    fn encode_token(&self, token: &Token) -> Vec<f32> {
        let mut out = vec![0.0; self.width];
        for row in 0..self.width {
            out[row] = self.numeric[row * TOKEN_NUMERIC..][..TOKEN_NUMERIC]
                .iter()
                .zip(&token[TOKEN_CATEGORICAL..])
                .map(|(weight, value)| weight * value)
                .sum();
        }
        for field in 0..TOKEN_CATEGORICAL {
            let categorical = token[field].round() as usize;
            if categorical == 0 {
                continue;
            }
            let value = categorical - 1;
            let indices = [
                (value % 256 + 1).min(self.embedding_sizes[field] - 1)
                    + self.embedding_offsets[field],
                (value / 256 + 1).min(self.embedding_sizes[field + TOKEN_CATEGORICAL] - 1)
                    + self.embedding_offsets[field + TOKEN_CATEGORICAL],
            ];
            for index in indices {
                for (column, output) in out.iter_mut().enumerate() {
                    *output += self.embedding[index * self.width + column];
                }
            }
        }
        layer_norm(&mut out, &self.token_norm_w, &self.token_norm_b);
        out.iter_mut().for_each(|value| *value = value.max(0.0));
        out
    }

    fn transform(&self, mut sequence: Vec<Vec<f32>>, layers: &[TransformerLayer]) -> Vec<Vec<f32>> {
        let head_width = self.width / self.heads;
        for layer in layers {
            let normalized_rows = sequence
                .iter()
                .map(|row| normalized(row, &layer.norm1_w, &layer.norm1_b))
                .collect::<Vec<_>>();
            let qkv = normalized_rows
                .iter()
                .map(|row| linear(row, &layer.qkv_w, &layer.qkv_b))
                .collect::<Vec<_>>();
            let mut attended = vec![vec![0.0; self.width]; sequence.len()];
            for query in 0..sequence.len() {
                for head in 0..self.heads {
                    let start = head * head_width;
                    let end = start + head_width;
                    let scores = qkv
                        .iter()
                        .map(|key| {
                            qkv[query][start..end]
                                .iter()
                                .zip(&key[self.width + start..self.width + end])
                                .map(|(left, right)| left * right)
                                .sum::<f32>()
                                / (head_width as f32).sqrt()
                        })
                        .collect::<Vec<_>>();
                    let peak = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    let weights = scores
                        .iter()
                        .map(|score| (score - peak).exp())
                        .collect::<Vec<_>>();
                    let total: f32 = weights.iter().sum();
                    for (key, weight) in qkv.iter().zip(weights) {
                        for column in start..end {
                            attended[query][column] +=
                                weight / total * key[2 * self.width + column];
                        }
                    }
                }
            }
            for (row, attention) in sequence.iter_mut().zip(attended) {
                let projected = linear(&attention, &layer.out_w, &layer.out_b);
                for (value, residual) in row.iter_mut().zip(projected) {
                    *value += residual;
                }
            }
            let normalized_rows = sequence
                .iter()
                .map(|row| normalized(row, &layer.norm2_w, &layer.norm2_b))
                .collect::<Vec<_>>();
            for (row, normalized) in sequence.iter_mut().zip(normalized_rows) {
                let hidden = linear(&normalized, &layer.linear1_w, &layer.linear1_b)
                    .into_iter()
                    .map(|value| value.max(0.0))
                    .collect::<Vec<_>>();
                let projected = linear(&hidden, &layer.linear2_w, &layer.linear2_b);
                for (value, residual) in row.iter_mut().zip(projected) {
                    *value += residual;
                }
            }
        }
        sequence
    }

    fn encode_decision(&self, game: &Game, state: Vec<f32>) -> Vec<f32> {
        let mut sequence = vec![self.decision_state.clone(), state];
        let (actions, legal) = candidate_actions(game, self.content);
        for (action, legal) in actions.iter().zip(legal) {
            let (values, tokens) =
                tokenized_candidate(game, self.content, self.layout, action, legal);
            let mut encoded = linear(&values, &self.action_value_w, &self.action_value_b);
            let divisor = (tokens.len().max(1) as f32).sqrt();
            for token in &tokens {
                for (sum, value) in encoded.iter_mut().zip(self.encode_token(token)) {
                    *sum += value / divisor;
                }
            }
            layer_norm(&mut encoded, &self.action_norm_w, &self.action_norm_b);
            encoded.iter_mut().for_each(|value| *value = value.max(0.0));
            sequence.push(encoded);
        }
        self.transform(sequence, &self.action_layers).remove(0)
    }

    fn state(&self, game: &Game) -> Vec<f32> {
        let tokens = state_tokens(game, self.content, self.layout);
        let mut sequence = vec![self.state_state.clone()];
        sequence.extend(tokens.iter().map(|token| self.encode_token(token)));
        let state = self.transform(sequence, &self.layers).remove(0);
        self.encode_decision(game, state)
    }

    pub fn win_probability(&self, game: &Game) -> f32 {
        match game.phase {
            Phase::Won => return 1.0,
            Phase::Dead => return 0.0,
            _ => {}
        }
        let hidden = dense_relu(&self.state(game), &self.value_w1, &self.value_b1);
        let logit = self
            .value_w2
            .iter()
            .zip(hidden)
            .fold(self.value_b2, |sum, (weight, value)| sum + weight * value);
        sigmoid(logit / self.temperature + self.bias)
    }
}

fn linear(input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
    bias.iter()
        .enumerate()
        .map(|(row, &bias)| {
            weights[row * input.len()..][..input.len()]
                .iter()
                .zip(input)
                .fold(bias, |sum, (weight, value)| sum + weight * value)
        })
        .collect()
}

fn dense_relu(input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
    linear(input, weights, bias)
        .into_iter()
        .map(|value| value.max(0.0))
        .collect()
}

fn normalized(input: &[f32], weight: &[f32], bias: &[f32]) -> Vec<f32> {
    let mut out = input.to_vec();
    layer_norm(&mut out, weight, bias);
    out
}

fn layer_norm(input: &mut [f32], weight: &[f32], bias: &[f32]) {
    let mean = input.iter().sum::<f32>() / input.len() as f32;
    let variance = input
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / input.len() as f32;
    let scale = (variance + 1e-5).sqrt().recip();
    for ((value, weight), bias) in input.iter_mut().zip(weight).zip(bias) {
        *value = (*value - mean) * scale * weight + bias;
    }
}

fn sigmoid(value: f32) -> f32 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn take_bytes<'a>(input: &mut &'a [u8], len: usize) -> io::Result<&'a [u8]> {
    if input.len() < len {
        return Err(invalid("truncated value model"));
    }
    let (value, rest) = input.split_at(len);
    *input = rest;
    Ok(value)
}

fn read_u32(input: &mut &[u8]) -> io::Result<u32> {
    Ok(u32::from_le_bytes(
        take_bytes(input, 4)?.try_into().unwrap(),
    ))
}

fn read_u64(input: &mut &[u8]) -> io::Result<u64> {
    Ok(u64::from_le_bytes(
        take_bytes(input, 8)?.try_into().unwrap(),
    ))
}

fn read_f32(input: &mut &[u8]) -> io::Result<f32> {
    Ok(f32::from_le_bytes(
        take_bytes(input, 4)?.try_into().unwrap(),
    ))
}

fn read_f32s(input: &mut &[u8], len: usize) -> io::Result<Vec<f32>> {
    (0..len).map(|_| read_f32(input)).collect()
}

#[cfg(feature = "python")]
mod python {
    use super::*;
    use numpy::{IntoPyArray, PyArray1, PyArray2, PyArray3, PyArray4, ndarray};
    use pyo3::{exceptions::PyValueError, prelude::*};
    use rayon::prelude::*;
    use std::collections::HashSet;

    #[pyclass]
    struct Batch {
        content: Content,
        layout: Layout,
        games: Vec<Game>,
        actions: Vec<Vec<Action>>,
        plans: Vec<Vec<Action>>,
        starts: Vec<Game>,
        root_ids: Vec<usize>,
        archive: Vec<Vec<Game>>,
        archive_seen: Vec<u64>,
        first_archive: Vec<Game>,
        random: u64,
        next_seed: u64,
        seed_stride: u64,
        character: Option<Id>,
        ascension: u8,
        teacher_width: usize,
        teacher_turns: usize,
        first_teacher: Option<(usize, usize)>,
        training_strength: i16,
        training_dexterity: i16,
        resample_archive: bool,
        archive_depth: usize,
    }

    fn action_descriptor(
        game: &Game,
        content: &Content,
        action: &Action,
    ) -> (String, Option<String>, Option<String>, Option<String>) {
        let card = |card: Card| Some(content.cards[card.id as usize].id.to_owned());
        let enemy = |index: usize| {
            game.combat()
                .and_then(|combat| combat.enemies.get(index))
                .map(|enemy| format!("creature:{}", enemy.instance.saturating_sub(1)))
        };
        match action {
            Action::Play { hand, target } => {
                let card = game.combat().and_then(|combat| combat.hand.get(*hand));
                (
                    "play_card".into(),
                    card.map(|card| format!("combat-card:{}", card.instance.saturating_sub(1))),
                    target.and_then(enemy),
                    card.map(|card| content.cards[card.id as usize].id.to_owned()),
                )
            }
            Action::Potion { slot, target } => (
                "use_potion".into(),
                Some(format!("potion-slot:{slot}")),
                target.and_then(enemy),
                game.run
                    .potions
                    .get(*slot)
                    .and_then(|id| *id)
                    .map(|id| content.potions[id as usize].id.to_owned()),
            ),
            Action::DiscardPotion(slot) => (
                "discard_potion".into(),
                Some(format!("potion-slot:{slot}")),
                None,
                game.run
                    .potions
                    .get(*slot)
                    .and_then(|id| *id)
                    .map(|id| content.potions[id as usize].id.to_owned()),
            ),
            Action::EndTurn => ("end_turn".into(), None, None, None),
            Action::Path(index) => {
                let node = game.map.nodes.get(*index);
                (
                    "map_node".into(),
                    None,
                    node.map(|node| format!("map:{}:{}", node.lane, node.floor)),
                    None,
                )
            }
            Action::Buy(index) => {
                let model = match game.phase {
                    Phase::Shop(ref items) => items.get(*index).and_then(|item| match item {
                        ShopItem::Card(value, _) => card(*value),
                        ShopItem::Relic(id, _) => Some(content.relics[*id as usize].id.to_owned()),
                        ShopItem::Potion(id, _) => {
                            Some(content.potions[*id as usize].id.to_owned())
                        }
                        ShopItem::Remove(_) => Some("card-removal".into()),
                    }),
                    _ => None,
                };
                ("shop_purchase".into(), model.clone(), None, model)
            }
            Action::Rest => ("rest_site".into(), Some("HEAL".into()), None, None),
            Action::Hatch => ("rest_site".into(), Some("HATCH".into()), None, None),
            Action::Lift => ("rest_site".into(), Some("LIFT".into()), None, None),
            Action::Cook => ("rest_site".into(), Some("COOK".into()), None, None),
            Action::Kindle => ("rest_site".into(), Some("KINDLE".into()), None, None),
            Action::Dig => ("rest_site".into(), Some("DIG".into()), None, None),
            Action::Clone => ("rest_site".into(), Some("CLONE".into()), None, None),
            Action::Smith(index) => (
                "rest_site".into(),
                Some("SMITH".into()),
                None,
                game.run.deck.get(*index).copied().and_then(card),
            ),
            Action::Event(index) | Action::EventRelic(index, _) | Action::EventCard(index, _) => {
                ("event_option".into(), Some(index.to_string()), None, None)
            }
            Action::Choose(index) => {
                let chosen = match &game.phase {
                    Phase::ChooseCards(cards, ..) => cards.get(*index).copied(),
                    _ => game.combat().and_then(|combat| {
                        let choice = combat.choice?;
                        let cards = match choice.pile {
                            Pile::Draw => &combat.draw,
                            Pile::Hand => &combat.hand,
                            Pile::Discard => &combat.discard,
                            Pile::Exhaust => &combat.exhaust,
                            Pile::Offer => &combat.offer,
                        };
                        cards.get(*index).copied()
                    }),
                };
                ("choose_cards".into(), None, None, chosen.and_then(card))
            }
            _ => (format!("{action:?}"), None, None, None),
        }
    }

    fn card_score(content: &Content, card: Card) -> f32 {
        let def = content.cards[card.id as usize];
        let rarity = match def.rarity {
            CardRarity::Rare | CardRarity::Ancient => 18.0,
            CardRarity::Uncommon => 8.0,
            CardRarity::Common => 0.0,
            CardRarity::Basic => -12.0,
            CardRarity::Quest | CardRarity::Event => -30.0,
        };
        let kind = match def.card_type {
            CardType::Curse => -60.0,
            CardType::Status => -30.0,
            _ => 0.0,
        };
        let special = match def.id {
            "CARD.FEED"
            | "CARD.GENETIC_ALGORITHM"
            | "CARD.BIG_BANG"
            | "CARD.UNDEATH"
            | "CARD.DEMON_FORM"
            | "CARD.ECHO_FORM"
            | "CARD.BIASED_COGNITION"
            | "CARD.WRAITH_FORM"
            | "CARD.REAPER_FORM" => 120.0,
            "CARD.ANGER"
            | "CARD.INFLAME"
            | "CARD.CORRUPTION"
            | "CARD.FEEL_NO_PAIN"
            | "CARD.BARRICADE"
            | "CARD.BODY_SLAM"
            | "CARD.OFFERING"
            | "CARD.RAMPAGE"
            | "CARD.RAGE"
            | "CARD.INFERNAL_BLADE"
            | "CARD.DEFRAGMENT"
            | "CARD.GLACIER"
            | "CARD.CREATIVE_AI"
            | "CARD.BEAM_CELL"
            | "CARD.REFRACT"
            | "CARD.FERAL"
            | "CARD.BULK_UP"
            | "CARD.MACHINE_LEARNING"
            | "CARD.SHATTER"
            | "CARD.NOXIOUS_FUMES"
            | "CARD.DEADLY_POISON"
            | "CARD.FOOTWORK"
            | "CARD.AFTERIMAGE"
            | "CARD.ADRENALINE"
            | "CARD.BLADE_DANCE"
            | "CARD.ACCURACY"
            | "CARD.BULLET_TIME"
            | "CARD.PHANTOM_BLADES"
            | "CARD.SHADOW_STEP"
            | "CARD.COMET"
            | "CARD.SHINING_STRIKE"
            | "CARD.BULWARK"
            | "CARD.ORBIT"
            | "CARD.SEEKING_EDGE"
            | "CARD.VOID_FORM"
            | "CARD.I_AM_INVINCIBLE"
            | "CARD.THE_SMITH"
            | "CARD.ROYAL_GAMBLE"
            | "CARD.MAKE_IT_SO"
            | "CARD.DECISIONS_DECISIONS"
            | "CARD.COUNTDOWN"
            | "CARD.MISERY"
            | "CARD.NEUROSURGE"
            | "CARD.DEATH_MARCH"
            | "CARD.REANIMATE"
            | "CARD.SACRIFICE"
            | "CARD.ERADICATE"
            | "CARD.PAGESTORM"
            | "CARD.THE_SCYTHE"
            | "CARD.SENTRY_MODE"
            | "CARD.TIMES_UP"
            | "CARD.END_OF_DAYS"
            | "CARD.DREDGE"
            | "CARD.HANG"
            | "CARD.HELIX_DRILL" => 60.0,
            _ => 0.0,
        };
        rarity + kind - def.cost[card.upgrades.min(1) as usize].max(0) as f32 * 2.0
            + card.upgrades as f32 * 6.0
            + card.value as f32 * 20.0
            + special
    }

    fn engine_score(game: &Game, content: &Content, card: Card) -> f32 {
        let id = content.cards[card.id as usize].id;
        let core = matches!(
            id,
            "CARD.FLAK_CANNON"
                | "CARD.FIEND_FIRE"
                | "CARD.FLAME_BARRIER"
                | "CARD.SECOND_WIND"
                | "CARD.FEEL_NO_PAIN"
                | "CARD.CORRUPTION"
                | "CARD.BODY_SLAM"
                | "CARD.INFERNAL_BLADE"
                | "CARD.HELIX_DRILL"
                | "CARD.BARRAGE"
                | "CARD.GLACIER"
                | "CARD.BEAM_CELL"
                | "CARD.GENETIC_ALGORITHM"
                | "CARD.BUFFER"
                | "CARD.ECHO_FORM"
                | "CARD.RICOCHET"
                | "CARD.BLADE_DANCE"
                | "CARD.FINISHER"
                | "CARD.FLECHETTES"
                | "CARD.AFTERIMAGE"
                | "CARD.WRAITH_FORM"
                | "CARD.REFLECT"
                | "CARD.HEAVENLY_DRILL"
                | "CARD.LUNAR_BLAST"
                | "CARD.RADIATE"
                | "CARD.SEVEN_STARS"
                | "CARD.STARDUST"
                | "CARD.OBLIVION"
                | "CARD.NO_ESCAPE"
                | "CARD.COUNTDOWN"
                | "CARD.NEGATIVE_PULSE"
                | "CARD.END_OF_DAYS"
                | "CARD.NEUROSURGE"
                | "CARD.REAPER_FORM"
                | "CARD.SEVERANCE"
                | "CARD.TIMES_UP"
        );
        let support = matches!(
            id,
            "CARD.WHIRLWIND"
                | "CARD.SWORD_BOOMERANG"
                | "CARD.ANGER"
                | "CARD.RAGE"
                | "CARD.INFLAME"
                | "CARD.OFFERING"
                | "CARD.FEED"
                | "CARD.JUGGERNAUT"
                | "CARD.DARK_EMBRACE"
                | "CARD.BARRICADE"
                | "CARD.BEAM_CELL"
                | "CARD.UPPERCUT"
                | "CARD.CHILL"
                | "CARD.REFRACT"
                | "CARD.COOLHEADED"
                | "CARD.REBOOT"
                | "CARD.DEFRAGMENT"
                | "CARD.FERAL"
                | "CARD.INFINITE_BLADES"
                | "CARD.FOOTWORK"
                | "CARD.PIERCING_WAIL"
                | "CARD.MALAISE"
                | "CARD.FAN_OF_KNIVES"
                | "CARD.BULWARK"
                | "CARD.I_AM_INVINCIBLE"
                | "CARD.ORBIT"
                | "CARD.SHINING_STRIKE"
                | "CARD.HIDDEN_CACHE"
                | "CARD.PAGESTORM"
                | "CARD.CRIMSON_MANTLE"
                | "CARD.SHROUD"
                | "CARD.DEATH_MARCH"
                | "CARD.REAP"
                | "CARD.REAVE"
                | "CARD.SQUEEZE"
        ) && (!matches!(id, "CARD.CRIMSON_MANTLE" | "CARD.SHROUD")
            || game.run.character == 4);
        let secondary = matches!(
            id,
            "CARD.ERADICATE"
                | "CARD.PULL_FROM_BELOW"
                | "CARD.RATTLE"
                | "CARD.RAMPAGE"
                | "CARD.INFERNAL_BLADE"
        );
        let copies = game
            .run
            .deck
            .iter()
            .filter(|owned| owned.id == card.id)
            .count() as f32;
        let repeatable = matches!(
            id,
            "CARD.WHIRLWIND"
                | "CARD.FLAK_CANNON"
                | "CARD.FLAME_BARRIER"
                | "CARD.SWORD_BOOMERANG"
                | "CARD.ANGER"
                | "CARD.RAGE"
                | "CARD.BODY_SLAM"
                | "CARD.FEED"
                | "CARD.BARRAGE"
                | "CARD.BEAM_CELL"
                | "CARD.GLACIER"
                | "CARD.BLADE_DANCE"
                | "CARD.RICOCHET"
                | "CARD.FLECHETTES"
                | "CARD.PIERCING_WAIL"
                | "CARD.HEAVENLY_DRILL"
                | "CARD.LUNAR_BLAST"
                | "CARD.RADIATE"
                | "CARD.SEVEN_STARS"
                | "CARD.STARDUST"
                | "CARD.NO_ESCAPE"
        );
        let bonus = if core {
            220.0
        } else if support {
            100.0
        } else if secondary {
            60.0
        } else {
            0.0
        };
        bonus * (0.55 + game.run.act as f32 * 0.15) - copies * if repeatable { 35.0 } else { 100.0 }
    }

    fn draft_score(game: &Game, content: &Content, card: Card) -> f32 {
        card_score(content, card) + engine_score(game, content, card)
    }

    fn upgrade_score(content: &Content, card: Card) -> f32 {
        let id = content.cards[card.id as usize].id;
        let starter = match id {
            "CARD.BASH" | "CARD.ZAP" | "CARD.NEUTRALIZE" | "CARD.VENERATE" | "CARD.UNLEASH" => {
                2_000.0
            }
            "CARD.DUALCAST" | "CARD.SURVIVOR" | "CARD.FALLING_STAR" | "CARD.BODYGUARD" => 1_000.0,
            _ => 0.0,
        };
        starter + card_score(content, card)
    }

    fn engine_card(game: &Game, content: &Content, card: Card) -> bool {
        let copies = game
            .run
            .deck
            .iter()
            .filter(|owned| owned.id == card.id)
            .count() as f32;
        engine_score(game, content, card) + copies * 100.0 > 0.0
    }

    fn intent_attack(intent: &str) -> (i16, i16) {
        let Some(attack) = intent.split("Attack ").nth(1) else {
            return (0, 0);
        };
        let token = attack.split(',').next().unwrap_or(attack);
        let mut parts = token.split('x');
        let damage = parts
            .next()
            .and_then(|x| x.parse::<i16>().ok())
            .unwrap_or(0);
        let hits = parts
            .next()
            .and_then(|x| x.parse::<i16>().ok())
            .unwrap_or(1);
        (damage, hits)
    }

    fn potion_score(content: &Content, potion: Id) -> f32 {
        match content.potions[potion as usize].id {
            "POTION.FAIRY_IN_A_BOTTLE" => 10_000.0,
            "POTION.GHOST_IN_A_JAR" | "POTION.ENTROPIC_BREW" => 5_000.0,
            "POTION.FRUIT_JUICE" => 4_000.0,
            _ => 2_000.0,
        }
    }

    fn rest_distance(game: &Game, node: usize) -> i32 {
        if game.map.nodes[node].room == Room::Rest {
            return 0;
        }
        1 + game.map.nodes[node]
            .next
            .iter()
            .map(|next| rest_distance(game, *next))
            .min()
            .unwrap_or(20)
    }

    fn elite_count(game: &Game, node: usize) -> i32 {
        (game.map.nodes[node].room == Room::Elite) as i32
            + game.map.nodes[node]
                .next
                .iter()
                .map(|next| elite_count(game, *next))
                .max()
                .unwrap_or(0)
    }

    fn apply_training_bonuses(game: &mut Game, content: &Content, strength: i16, dexterity: i16) {
        if strength == 0 && dexterity == 0 {
            return;
        }
        let Phase::Combat(combat) = &mut game.phase else {
            return;
        };
        if let Some(id) = content
            .powers
            .iter()
            .position(|power| power.id == "POWER.STRENGTH_POWER")
        {
            combat.player.add_power(id as Id, strength);
        }
        if let Some(id) = content
            .powers
            .iter()
            .position(|power| power.id == "POWER.DEXTERITY_POWER")
        {
            combat.player.add_power(id as Id, dexterity);
        }
    }

    fn teacher_state_score(game: &Game, content: &Content) -> f32 {
        match game.phase {
            Phase::Won => return 1e9,
            Phase::Dead => return -1e9,
            _ => {}
        }
        let progress = game.run.act.saturating_sub(1) as f32 * 20.0 + game.run.floor as f32;
        let hp = game.combat().map_or(game.run.hp, |combat| combat.player.hp);
        let deck_len = game.run.deck.len() as f32;
        let potion_factor = if game.combat().is_some() && game.room == Room::Boss {
            0.1
        } else {
            1.0
        };
        let mut score = progress * 100_000.0
            + hp as f32 * 500.0
            + game.run.max_hp as f32 * 80.0
            + game.run.gold as f32 * 0.1
            + game.run.relics.len() as f32 * 500.0
            + game
                .run
                .potions
                .iter()
                .flatten()
                .map(|potion| potion_score(content, *potion))
                .sum::<f32>()
                * potion_factor
            + game
                .run
                .deck
                .iter()
                .map(|card| card_score(content, *card) * 10.0)
                .sum::<f32>()
            - deck_len * deck_len * 5.0;
        if matches!(game.phase, Phase::Map) {
            score += game
                .run
                .deck
                .iter()
                .map(|card| engine_score(game, content, *card))
                .sum::<f32>()
                * 5.0;
        }
        if game.run.act == 3 {
            if game.room == Room::Elite {
                let reserve = game.run.max_hp * 2 / 3;
                score -= (reserve - hp).max(0) as f32 * 750.0;
            }
        }
        if game.run.character == 0 {
            for (index, card) in game.run.deck.iter().enumerate() {
                let id = content.cards[card.id as usize].id;
                let repeatable = matches!(
                    id,
                    "CARD.WHIRLWIND"
                        | "CARD.FLAK_CANNON"
                        | "CARD.FLAME_BARRIER"
                        | "CARD.SWORD_BOOMERANG"
                        | "CARD.ANGER"
                        | "CARD.RAGE"
                        | "CARD.BARRAGE"
                        | "CARD.BLADE_DANCE"
                        | "CARD.RICOCHET"
                        | "CARD.FLECHETTES"
                        | "CARD.HEAVENLY_DRILL"
                        | "CARD.LUNAR_BLAST"
                        | "CARD.RADIATE"
                        | "CARD.SEVEN_STARS"
                        | "CARD.STARDUST"
                        | "CARD.NO_ESCAPE"
                );
                if !repeatable
                    && game.run.deck[..index]
                        .iter()
                        .any(|prior| prior.id == card.id)
                {
                    score -= 700.0;
                }
            }
        }
        if game.run.character == 0 {
            let count = |id| {
                game.run
                    .deck
                    .iter()
                    .filter(|card| content.cards[card.id as usize].id == id)
                    .count() as f32
            };
            let rage = count("CARD.RAGE");
            let anger = count("CARD.ANGER");
            let available = |id| {
                game.combat().is_none_or(|combat| {
                    combat
                        .draw
                        .iter()
                        .chain(&combat.hand)
                        .chain(&combat.discard)
                        .any(|card| content.cards[card.id as usize].id == id)
                        || id == "CARD.FEEL_NO_PAIN"
                            && combat.player.powers.iter().any(|power| {
                                content.powers[power.id as usize].id == "POWER.FEEL_NO_PAIN_POWER"
                            })
                })
            };
            score += 500.0 * rage.min(3.0)
                + 500.0 * anger.min(2.0)
                + 700.0
                    * [
                        "CARD.INFLAME",
                        "CARD.OFFERING",
                        "CARD.FEED",
                        "CARD.JUGGERNAUT",
                    ]
                    .iter()
                    .map(|id| count(id).min(1.0))
                    .sum::<f32>()
                + 300.0
                    * ["CARD.RAMPAGE", "CARD.INFERNAL_BLADE"]
                        .iter()
                        .map(|id| count(id).min(1.0))
                        .sum::<f32>()
                + 1_000.0 * (rage > 0.0 && anger > 0.0) as u8 as f32
                + 1_000.0 * (rage > 0.0 && count("CARD.INFLAME") > 0.0) as u8 as f32
                + 1_000.0 * (rage > 0.0 && count("CARD.JUGGERNAUT") > 0.0) as u8 as f32
                + 1_000.0
                    * (count("CARD.FEEL_NO_PAIN") > 0.0
                        && count("CARD.SECOND_WIND") > 0.0
                        && count("CARD.BODY_SLAM") > 0.0
                        && available("CARD.FEEL_NO_PAIN")
                        && available("CARD.SECOND_WIND")
                        && available("CARD.BODY_SLAM")) as u8 as f32;
        }
        if game.run.character == 4 {
            let count = |id| {
                game.run
                    .deck
                    .iter()
                    .filter(|card| content.cards[card.id as usize].id == id)
                    .count() as f32
            };
            let no_escape = count("CARD.NO_ESCAPE");
            let countdown = count("CARD.COUNTDOWN");
            score += 1_000.0 * no_escape.min(3.0)
                + 700.0 * countdown.min(2.0)
                + 1_000.0
                    * ["CARD.END_OF_DAYS", "CARD.PAGESTORM", "CARD.MAD_SCIENCE"]
                        .iter()
                        .map(|id| count(id).min(1.0))
                        .sum::<f32>()
                + 3_000.0 * (no_escape >= 2.0 && countdown > 0.0) as u8 as f32
                + 2_000.0 * count("CARD.REAPER_FORM").min(1.0)
                + 800.0
                    * [
                        "CARD.DEATH_MARCH",
                        "CARD.TIMES_UP",
                        "CARD.REAP",
                        "CARD.REAVE",
                    ]
                    .iter()
                    .map(|id| count(id).min(1.0))
                    .sum::<f32>()
                + 2_500.0 * (count("CARD.REAPER_FORM") > 0.0 && countdown > 0.0) as u8 as f32;
        }
        if let Some(combat) = game.combat() {
            let test_subject = combat.enemies.iter().any(|enemy| {
                content.enemies[enemy.creature.id as usize].id == "MONSTER.TEST_SUBJECT"
            });
            let aeonglass = combat
                .enemies
                .iter()
                .any(|enemy| content.enemies[enemy.creature.id as usize].id == "MONSTER.AEONGLASS");
            let nemesis = combat.enemies.iter().any(|enemy| {
                enemy
                    .creature
                    .powers
                    .iter()
                    .any(|power| content.powers[power.id as usize].id == "POWER.NEMESIS_POWER")
            });
            let (incoming, attacks) = combat
                .enemies
                .iter()
                .filter(|enemy| enemy.creature.hp > 0)
                .fold((0i16, 0i16), |(incoming, attacks), enemy| {
                    let def = &content.enemies[enemy.creature.id as usize];
                    let (damage, mut hits) = intent_attack(def.moves[enemy.move_index].intent);
                    if def.id == "MONSTER.TEST_SUBJECT" && enemy.move_index == 3 {
                        hits = 2 + enemy
                            .move_history
                            .iter()
                            .filter(|&&prior| prior == 3)
                            .count() as i16;
                    }
                    (
                        incoming.saturating_add(damage.saturating_mul(hits)),
                        attacks.saturating_add(hits),
                    )
                });
            let reflect = combat.player.powers.iter().any(|power| {
                power.amount > 0 && content.powers[power.id as usize].id == "POWER.REFLECT_POWER"
            });
            let barricade = combat.player.powers.iter().any(|power| {
                power.amount > 0 && content.powers[power.id as usize].id == "POWER.BARRICADE_POWER"
            });
            let useful_block = if barricade {
                combat.player.block
            } else if test_subject {
                combat.player.block.min(incoming)
            } else {
                combat.player.block.min(incoming.max(20))
            };
            score += useful_block as f32
                * if nemesis && reflect && attacks > 0 {
                    185.0
                } else {
                    35.0
                };
            score += combat.energy as f32 * 2.0
                + combat.stars as f32 * 100.0
                + combat.osty.hp.max(0) as f32 * 50.0;
            score += combat
                .draw
                .iter()
                .chain(&combat.hand)
                .chain(&combat.discard)
                .map(|card| {
                    card.value as f32 * 20.0
                        - match card.id {
                            card_id::WOUND => 750.0,
                            card_id::BURN => 1_500.0,
                            card_id::WITHER => 750.0 + 770.0 * card.value.max(0) as f32,
                            _ => 0.0,
                        }
                })
                .sum::<f32>();
            score += combat
                .exhaust
                .iter()
                .map(|card| card.value as f32 * 20.0)
                .sum::<f32>();
            for orb in &combat.orbs {
                let def = content.orbs[orb.id as usize];
                score += match def.id {
                    "ORB.LIGHTNING_ORB" => 150.0,
                    "ORB.FROST_ORB" => 300.0,
                    "ORB.DARK_ORB" | "ORB.GLASS_ORB" => orb.value as f32 * 45.0,
                    "ORB.PLASMA_ORB" => 200.0,
                    _ => 0.0,
                };
            }
            for power in &combat.player.powers {
                let def = content.powers[power.id as usize];
                let weight = match def.id {
                    "POWER.STRENGTH_POWER" => 250.0,
                    "POWER.DEXTERITY_POWER" => 200.0,
                    "POWER.FOCUS_POWER" => 300.0,
                    "POWER.INTANGIBLE_POWER" => 1_000.0,
                    "POWER.COUNTDOWN_POWER" if game.room == Room::Boss => match power.amount {
                        ..=9 => 800.0,
                        10..=15 => 500.0,
                        _ => 350.0,
                    },
                    "POWER.REAPER_FORM_POWER"
                        if aeonglass || nemesis || game.room == Room::Boss && !test_subject =>
                    {
                        20_000.0
                    }
                    "POWER.ECHO_FORM_POWER" => 2_000.0,
                    "POWER.CREATIVE_AI_POWER" => 1_200.0,
                    "POWER.MACHINE_LEARNING_POWER" => 600.0,
                    "POWER.LOOP_POWER" => 500.0,
                    "POWER.FEEL_NO_PAIN_POWER" => 400.0,
                    "POWER.RAGE_POWER" => 300.0,
                    "POWER.FLAME_BARRIER_POWER" => attacks as f32 * 150.0,
                    _ if nemesis && matches!(def.kind, PowerKind::Thorns) => attacks as f32 * 150.0,
                    _ if def.debuff => -80.0,
                    _ => 80.0,
                };
                score += power.amount as f32 * weight;
            }
            for enemy in &combat.enemies {
                let enemy_id = content.enemies[enemy.creature.id as usize].id;
                let adaptable =
                    enemy.creature.powers.iter().any(|power| {
                        content.powers[power.id as usize].id == "POWER.ADAPTABLE_POWER"
                    });
                let future_hp = if adaptable && enemy_id == "MONSTER.TEST_SUBJECT" {
                    if enemy.creature.max_hp < 200 {
                        525
                    } else {
                        313
                    }
                } else {
                    0
                };
                score -= (enemy.creature.hp.max(0) + future_hp) as f32
                    * if matches!(
                        enemy_id,
                        "MONSTER.TORCH_HEAD_AMALGAM"
                            | "MONSTER.TEST_SUBJECT"
                            | "MONSTER.AEONGLASS"
                            | "MONSTER.QUEEN"
                    ) {
                        1_000.0
                    } else {
                        150.0
                    };
                score -= enemy.creature.block as f32 * 5.0;
                for power in &enemy.creature.powers {
                    let def = content.powers[power.id as usize];
                    score += power.amount as f32
                        * if def.id == "POWER.SANDPIT_POWER" {
                            2_000.0
                        } else if def.id == "POWER.POISON_POWER" {
                            150.0
                        } else if def.debuff {
                            80.0
                        } else {
                            -80.0
                        };
                }
            }
            score -= (incoming - combat.player.block).max(0) as f32 * 250.0;
        } else {
            score += 20_000.0;
        }
        score
    }

    fn combat_quality(game: &Game) -> f32 {
        if matches!(game.phase, Phase::Dead) {
            return 0.0;
        }
        let hp = game
            .combat()
            .map_or(game.run.hp, |combat| combat.player.hp)
            .max(0) as f32
            / game.run.max_hp.max(1) as f32;
        let Some(combat) = game.combat() else {
            return 1.0 + 0.25 * hp;
        };
        let future = |enemy: &Enemy| {
            if enemy
                .creature
                .powers
                .iter()
                .any(|power| power.id == power_id::ADAPTABLE)
            {
                if enemy.creature.max_hp < 200 {
                    525
                } else {
                    313
                }
            } else {
                0
            }
        };
        let current: i32 = combat
            .enemies
            .iter()
            .map(|enemy| enemy.creature.hp.max(0) as i32 + future(enemy))
            .sum();
        let maximum: i32 = combat
            .enemies
            .iter()
            .map(|enemy| enemy.creature.max_hp.max(1) as i32 + future(enemy))
            .sum();
        0.5 * hp + 0.5 * (1.0 - current as f32 / maximum.max(1) as f32)
    }

    fn teacher_choice(game: &Game, content: &Content) -> usize {
        let actions = game.actions(content);
        if let Phase::Event(id, _) = game.phase
            && content.events[id as usize].id == "EVENT.REFLECTIONS"
            && let Some(index) = actions
                .iter()
                .position(|action| matches!(action, Action::Event(0)))
        {
            return index;
        }
        if game
            .crystal
            .as_ref()
            .is_some_and(|crystal| crystal.remaining > 0)
        {
            return actions
                .iter()
                .position(|action| matches!(action, Action::CrystalCell(..)))
                .unwrap_or(0);
        }
        if actions
            .iter()
            .all(|action| matches!(action, Action::DiscardPotion(_)))
        {
            return actions
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let score = |action: &Action| match action {
                        Action::DiscardPotion(slot) => game.run.potions[*slot]
                            .map(|potion| potion_score(content, potion))
                            .unwrap_or_default(),
                        _ => 0.0,
                    };
                    score(a.1).total_cmp(&score(b.1))
                })
                .map_or(0, |x| x.0);
        }
        if matches!(game.phase, Phase::Map) {
            let hurt = game.run.hp * 2 < game.run.max_hp;
            return actions
                .iter()
                .enumerate()
                .filter(|(_, action)| matches!(action, Action::Path(_)))
                .max_by_key(|(_, action)| match action {
                    Action::Path(path) if hurt => {
                        let room = match game.map.nodes[*path].room {
                            Room::Rest => 200,
                            Room::Shop if game.run.gold >= 100 => 80,
                            Room::Treasure => 50,
                            Room::Unknown => 20,
                            Room::Combat => -50,
                            Room::Elite => -80,
                            Room::Boss => -100,
                            _ => 0,
                        };
                        room - rest_distance(game, *path) * 20
                    }
                    Action::Path(path) => {
                        let room = match game.map.nodes[*path].room {
                            Room::Elite if game.run.act == 1 => 120,
                            Room::Shop if game.run.gold >= 100 => 99,
                            Room::Treasure => 90,
                            Room::Unknown => 80,
                            Room::Rest => 60,
                            Room::Combat => 50,
                            Room::Shop => 40,
                            Room::Elite => 10,
                            Room::Boss => 0,
                            _ => 20,
                        };
                        room + if game.run.act == 1 {
                            elite_count(game, *path) * 50
                        } else {
                            0
                        }
                    }
                    _ => 0,
                })
                .map_or(0, |x| x.0);
        }
        if let Phase::Shop(items) = &game.phase
            && (game.run.deck.len() > 18
                || game.run.deck.iter().any(|card| {
                    let def = content.cards[card.id as usize];
                    matches!(def.card_type, CardType::Curse | CardType::Status)
                        && card.flags(def) & ETERNAL == 0
                }))
            && let Some((index, _)) = actions.iter().enumerate().find(|(_, action)| {
                matches!(action, Action::Buy(item) if matches!(items[*item], ShopItem::Remove(_)))
            })
        {
            return index;
        }
        if let Phase::Shop(items) = &game.phase
            && let Some((index, score)) = actions
                .iter()
                .enumerate()
                .filter_map(|(index, action)| match action {
                    Action::Buy(item) => Some((
                        index,
                        match items[*item] {
                            ShopItem::Card(card, _) => draft_score(game, content, card),
                            ShopItem::Relic(..) => 180.0,
                            _ => 0.0,
                        },
                    )),
                    _ => None,
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
            && score > 50.0
        {
            return index;
        }
        if matches!(game.phase, Phase::Shop(_))
            && let Some(index) = actions
                .iter()
                .position(|action| matches!(action, Action::Cancel | Action::Leave))
        {
            return index;
        }
        if let Phase::Rewards(rewards) = &game.phase {
            for kind in [8, 6, 9] {
                if let Some((index, _)) = actions
                    .iter()
                    .enumerate()
                    .find(|(_, action)| action_kind(action) == kind)
                {
                    return index;
                }
            }
            if !rewards.cards.is_empty()
                && let Some((index, _)) = actions
                    .iter()
                    .enumerate()
                    .filter_map(|(index, action)| {
                        Some((
                            index,
                            match action {
                                Action::RewardCard(card) => {
                                    let card = rewards.cards[*card];
                                    draft_score(game, content, card)
                                        - game
                                            .run
                                            .deck
                                            .iter()
                                            .filter(|owned| owned.id == card.id)
                                            .count()
                                            as f32
                                            * 30.0
                                        - game.run.deck.len().saturating_sub(12) as f32 * 2.0
                                }
                                Action::Cancel => 40.0,
                                _ => return None,
                            },
                        ))
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
            {
                return index;
            }
            for kind in [10, 29, 31] {
                if let Some((index, _)) = actions
                    .iter()
                    .enumerate()
                    .find(|(_, action)| action_kind(action) == kind)
                {
                    return index;
                }
            }
        }
        if matches!(
            game.phase,
            Phase::RemoveCards(..) | Phase::TransformCards(..)
        ) {
            return actions
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let score = |action: &Action| match action {
                        Action::RemoveCard(card) => {
                            draft_score(game, content, game.run.deck[*card])
                        }
                        Action::Cancel => 10_000.0,
                        _ => 9_999.0,
                    };
                    score(a.1).total_cmp(&score(b.1))
                })
                .map_or(0, |x| x.0);
        }
        if matches!(game.phase, Phase::UpgradeCards(..)) {
            return actions
                .iter()
                .enumerate()
                .max_by(|a, b| {
                    let score = |action: &Action| match action {
                        Action::Smith(card) => {
                            upgrade_score(content, game.run.deck[*card])
                                + engine_score(game, content, game.run.deck[*card])
                        }
                        _ => -10_000.0,
                    };
                    score(a.1).total_cmp(&score(b.1))
                })
                .map_or(0, |x| x.0);
        }
        if matches!(game.phase, Phase::Rest) {
            if game.run.hp * 4 < game.run.max_hp * 3
                && let Some(index) = actions
                    .iter()
                    .position(|action| matches!(action, Action::Rest))
            {
                return index;
            }
            for special in [
                Action::Cook,
                Action::Hatch,
                Action::Lift,
                Action::Dig,
                Action::Kindle,
            ] {
                if let Some(index) = actions.iter().position(|action| *action == special) {
                    return index;
                }
            }
            if let Some((index, _)) = actions
                .iter()
                .enumerate()
                .filter(|(_, action)| matches!(action, Action::Smith(_)))
                .max_by(|a, b| {
                    let score = |action: &Action| match action {
                        Action::Smith(card) => {
                            upgrade_score(content, game.run.deck[*card])
                                + engine_score(game, content, game.run.deck[*card])
                        }
                        _ => -10_000.0,
                    };
                    score(a.1).total_cmp(&score(b.1))
                })
            {
                return index;
            }
        }
        actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                if matches!(action, Action::DiscardPotion(_)) {
                    return Some((index, -2e9));
                }
                let mut next = game.clone();
                next.step(content, action.clone()).ok()?;
                Some((index, teacher_state_score(&next, content)))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0, |x| x.0)
    }

    fn rescue_choice(
        game: &Game,
        content: &Content,
        choice: usize,
        danger: f32,
        tactical: bool,
    ) -> usize {
        let actions = game.actions(content);
        let rest_threshold = match game.run.character {
            0 | 2 => 3,
            _ => 4,
        };
        if matches!(game.phase, Phase::Rest)
            && game.run.hp * 10 <= game.run.max_hp * rest_threshold
            && let Some(rest) = actions
                .iter()
                .position(|action| matches!(action, Action::Rest))
        {
            return rest;
        }
        if tactical
            && matches!(game.run.character, 1 | 3 | 4)
            && matches!(
                game.phase,
                Phase::Map
                    | Phase::Shop(_)
                    | Phase::Rewards(_)
                    | Phase::RemoveCards(..)
                    | Phase::TransformCards(..)
                    | Phase::UpgradeCards(..)
                    | Phase::Rest
            )
        {
            return teacher_choice(game, content);
        }
        if game.run.character == 2
            && game.run.hp * 5 < game.run.max_hp * 2
            && matches!(game.phase, Phase::Map)
            && actions.get(choice).is_some_and(|action| {
                matches!(
                    action,
                    Action::Path(path)
                        if matches!(game.map.nodes[*path].room, Room::Combat | Room::Elite)
                )
            })
        {
            let safer = teacher_choice(game, content);
            if actions.get(safer).is_some_and(|action| {
                matches!(
                    action,
                    Action::Path(path)
                        if !matches!(game.map.nodes[*path].room, Room::Combat | Room::Elite | Room::Boss)
                )
            }) {
                return safer;
            }
        }
        if matches!(actions.get(choice), Some(Action::Play { .. }))
            && game
                .combat()
                .is_some_and(|combat| combat.history.manual_plays >= 32)
            && let Some(end_turn) = actions
                .iter()
                .position(|action| matches!(action, Action::EndTurn))
        {
            return end_turn;
        }
        if !matches!(actions.get(choice), Some(Action::EndTurn)) {
            return choice;
        }
        let Some(combat) = game.combat() else {
            return choice;
        };
        let incoming = combat
            .enemies
            .iter()
            .filter(|enemy| enemy.creature.hp > 0)
            .map(|enemy| {
                let def = &content.enemies[enemy.creature.id as usize];
                let (mut damage, mut hits) = intent_attack(def.moves[enemy.move_index].intent);
                if def.id == "MONSTER.TEST_SUBJECT" && enemy.move_index == 3 {
                    damage = if game.run.ascension >= 9 { 11 } else { 10 };
                    hits = 2 + enemy
                        .move_history
                        .iter()
                        .filter(|&&prior| prior == 3)
                        .count() as i16;
                } else if def.id == "MONSTER.WATERFALL_GIANT" && enemy.move_index == 4 {
                    damage = (if game.run.ascension >= 9 { 23 } else { 20 }) + enemy.value;
                    hits = 1;
                } else if def.id == "MONSTER.WATERFALL_GIANT" && enemy.move_index == 6 {
                    damage = enemy.value;
                    hits = 1;
                }
                damage.saturating_mul(hits)
            })
            .fold(0i16, i16::saturating_add);
        let damage = incoming.saturating_sub(combat.player.block).max(0);
        if tactical
            && combat.history.manual_cards < 6
            && (damage as f32) >= combat.player.hp.max(1) as f32 * danger
        {
            return actions
                .iter()
                .enumerate()
                .filter(|(_, action)| matches!(action, Action::Play { .. } | Action::Potion { .. }))
                .filter_map(|(index, action)| {
                    let consumed = match action {
                        Action::Potion { slot, .. } => game.run.potions[*slot]
                            .map(|id| potion_score(content, id))
                            .unwrap_or_default(),
                        _ => 0.0,
                    };
                    let mut next = game.clone();
                    resample_hidden(&mut next, content, 0x5055_424c_4943_0001);
                    next.step(content, action.clone()).ok()?;
                    Some((index, teacher_state_score(&next, content) + consumed))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map_or(choice, |(index, _)| index);
        }
        if (damage as f32) < combat.player.hp.max(1) as f32 * danger {
            return choice;
        }
        actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                let Action::Potion { slot, target } = action else {
                    return None;
                };
                let id = content.potions[game.run.potions[*slot]? as usize].id;
                let priority = match id {
                    "POTION.GHOST_IN_A_JAR" => 100,
                    "POTION.BLOCK_POTION" | "POTION.SHIP_IN_A_BOTTLE" => 90,
                    "POTION.SHACKLING_POTION"
                    | "POTION.WEAK_POTION"
                    | "POTION.POTION_OF_BINDING" => 80,
                    "POTION.FIRE_POTION"
                    | "POTION.EXPLOSIVE_AMPOULE"
                    | "POTION.POTION_SHAPED_ROCK"
                    | "POTION.POTION_OF_DOOM"
                    | "POTION.POWDERED_DEMISE" => 70,
                    "POTION.FAIRY_IN_A_BOTTLE" => 0,
                    "POTION.FOUL_POTION" if combat.player.hp <= 12 => 0,
                    _ => 60,
                };
                let target_hp = target
                    .and_then(|target| combat.enemies.get(target))
                    .map_or(0, |enemy| enemy.creature.hp.max(0));
                Some((index, priority, -target_hp))
            })
            .filter(|(_, priority, _)| *priority > 0)
            .max_by_key(|(_, priority, target)| (*priority, *target))
            .map_or(choice, |(index, _, _)| index)
    }

    fn teacher_plan(
        game: &Game,
        content: &Content,
        width: usize,
        teacher_turns: usize,
    ) -> Vec<Action> {
        let actions = game.actions(content);
        let Some(turn) = game.combat().map(|combat| combat.turn) else {
            return actions
                .get(teacher_choice(game, content))
                .cloned()
                .into_iter()
                .collect();
        };
        if !actions
            .iter()
            .any(|action| matches!(action, Action::Play { .. } | Action::EndTurn))
        {
            return actions.into_iter().take(1).collect();
        }
        if width == 1 {
            return actions
                .get(teacher_choice(game, content))
                .cloned()
                .into_iter()
                .collect();
        }
        let turns = teacher_turns.max((game.room == Room::Boss) as usize + 1) as u16;
        let mut frontier = vec![(game.clone(), Vec::new())];
        let mut best: Option<(f32, Vec<Action>)> = None;
        let mut fallback: Option<(f32, Vec<Action>)> = None;
        let mut expansions = 0;
        let expansion_limit = 256;
        'search: for _ in 0..10 * turns as usize {
            let mut next_frontier = Vec::new();
            for (state, plan) in frontier {
                let actions = state.actions(content);
                let action_limit = if actions
                    .iter()
                    .any(|action| matches!(action, Action::Play { .. } | Action::EndTurn))
                {
                    64
                } else {
                    1
                };
                for action in actions
                    .into_iter()
                    .filter(|action| !matches!(action, Action::DiscardPotion(_)))
                    .take(action_limit)
                {
                    if expansions == expansion_limit {
                        break 'search;
                    }
                    expansions += 1;
                    let mut next = state.clone();
                    if next.step(content, action.clone()).is_err() {
                        continue;
                    }
                    let mut next_plan = plan.clone();
                    next_plan.push(action);
                    let candidate = (teacher_state_score(&next, content), next_plan.clone());
                    if fallback
                        .as_ref()
                        .is_none_or(|fallback| candidate.0 > fallback.0)
                    {
                        fallback = Some(candidate.clone());
                    }
                    if next
                        .combat()
                        .is_none_or(|combat| combat.turn >= turn.saturating_add(turns))
                    {
                        if best.as_ref().is_none_or(|best| candidate.0 > best.0) {
                            best = Some(candidate);
                        }
                    } else {
                        next_frontier.push((next, next_plan));
                    }
                }
            }
            next_frontier.sort_by(|a, b| {
                teacher_state_score(&b.0, content).total_cmp(&teacher_state_score(&a.0, content))
            });
            next_frontier.truncate(width);
            if next_frontier.is_empty() {
                break;
            }
            frontier = next_frontier;
        }
        best.or(fallback)
            .map_or_else(|| actions.into_iter().take(1).collect(), |(_, plan)| plan)
    }

    pub(super) fn combat_search(
        game: &Game,
        content: &Content,
        width: usize,
        depth: usize,
    ) -> (Game, Vec<Action>) {
        let mut links: Vec<(Option<usize>, Action)> = Vec::new();
        let mut frontier = vec![(game.clone(), None)];
        let mut best = frontier[0].clone();
        let mut best_clear = None;
        let mut first_clear = None;
        for layer in 0..depth {
            let mut next = Vec::new();
            let mut cleared = Vec::new();
            for (state, tail) in frontier {
                for action in search_actions(&state, content)
                    .into_iter()
                    .filter(|action| {
                        let Some(combat) = state.combat() else {
                            return true;
                        };
                        let preserves_barrier = matches!(action, Action::Play { hand, .. }
                        if content.cards[combat.hand[*hand].id as usize].id == "CARD.SECOND_WIND"
                            && combat.hand.iter().any(|card|
                                content.cards[card.id as usize].id == "CARD.FLAME_BARRIER"));
                        let nemesis = combat.enemies.iter().any(|enemy| {
                            enemy.creature.hp > 0
                                && enemy.creature.powers.iter().any(|power| {
                                    content.powers[power.id as usize].id == "POWER.NEMESIS_POWER"
                                })
                        });
                        !preserves_barrier || !nemesis
                    })
                {
                    let mut child = state.clone();
                    if child.step(content, action.clone()).is_err() {
                        continue;
                    }
                    links.push((tail, action));
                    let child_tail = Some(links.len() - 1);
                    if child.combat().is_none() && !matches!(child.phase, Phase::Dead) {
                        cleared.push((child, child_tail));
                    } else if !matches!(child.phase, Phase::Dead) {
                        next.push((child, child_tail));
                    }
                }
            }
            if let Some(clear) = cleared.into_iter().max_by(|a, b| {
                teacher_state_score(&a.0, content).total_cmp(&teacher_state_score(&b.0, content))
            }) && best_clear
                .as_ref()
                .is_none_or(|best: &(Game, Option<usize>)| {
                    teacher_state_score(&clear.0, content) > teacher_state_score(&best.0, content)
                })
            {
                best_clear = Some(clear);
                first_clear.get_or_insert(layer);
            }
            if first_clear.is_some_and(|first| layer >= first + 4) {
                let (child, tail) = best_clear.unwrap();
                return (child, collect_plan(&links, tail));
            }
            if next.is_empty() {
                if let Some((child, tail)) = best_clear {
                    return (child, collect_plan(&links, tail));
                }
                break;
            }
            next.sort_by(|a, b| {
                teacher_state_score(&b.0, content).total_cmp(&teacher_state_score(&a.0, content))
            });
            let mut seen = HashSet::new();
            next.retain(|(game, _)| seen.insert(combat_key(game, content)));
            next.truncate(width);
            if best.1.is_none()
                || teacher_state_score(&next[0].0, content) > teacher_state_score(&best.0, content)
            {
                best = next[0].clone();
            }
            frontier = next;
        }
        if let Some((child, tail)) = best_clear {
            return (child, collect_plan(&links, tail));
        }
        (best.0, collect_plan(&links, best.1))
    }

    fn collect_plan(links: &[(Option<usize>, Action)], mut tail: Option<usize>) -> Vec<Action> {
        let mut plan = Vec::new();
        while let Some(index) = tail {
            plan.push(links[index].1.clone());
            tail = links[index].0;
        }
        plan.reverse();
        plan
    }

    fn extend_plan(
        links: &mut Vec<(Option<usize>, Action)>,
        mut tail: Option<usize>,
        actions: impl IntoIterator<Item = Action>,
    ) -> Option<usize> {
        for action in actions {
            links.push((tail, action));
            tail = Some(links.len() - 1);
        }
        tail
    }

    fn combat_key(
        game: &Game,
        content: &Content,
    ) -> (u16, i16, i16, i16, i32, i16, i16, bool, u64) {
        let combat = game.combat().unwrap();
        let mut enemy_hp = 0;
        let mut doom = 0;
        let mut enemies = 0u64;
        let detailed = game.room == Room::Boss;
        for enemy in &combat.enemies {
            enemy_hp += enemy.creature.hp.max(0) as i32;
            let mut state = (enemy.creature.id as u64) << 40
                | (enemy.creature.hp.max(0) as u64) << 16
                | (enemy.creature.block.max(0) as u64) << 4
                | enemy.move_index as u64;
            if detailed {
                state ^= (enemy.creature.max_hp.max(0) as u64).rotate_left(32);
                for power in &enemy.creature.powers {
                    state ^= ((power.id as u64) << 32
                        | (power.amount as u16 as u64) << 16
                        | power.value as u16 as u64)
                        .rotate_left(power.id as u32 % 61);
                }
                for (index, &prior) in enemy.move_history.iter().enumerate() {
                    state ^= ((prior as u64) << 8 | index as u64)
                        .rotate_left((index * 7 + prior) as u32 % 61);
                }
            }
            enemies ^= state.rotate_left(enemy.creature.id as u32 % 61);
            doom += enemy
                .creature
                .powers
                .iter()
                .filter(|power| content.powers[power.id as usize].id == "POWER.DOOM_POWER")
                .map(|power| power.amount)
                .sum::<i16>();
        }
        let countdown = combat
            .player
            .powers
            .iter()
            .filter(|power| content.powers[power.id as usize].id == "POWER.COUNTDOWN_POWER")
            .map(|power| power.amount)
            .sum();
        let reaper = combat.player.powers.iter().any(|power| {
            content.powers[power.id as usize].id == "POWER.REAPER_FORM_POWER" && power.amount > 0
        });
        let cards = |pile: &[Card], salt: u64| {
            pile.iter().enumerate().fold(salt, |hash, (index, card)| {
                hash ^ ((card.id as u64) << 24
                    | (card.upgrades as u64) << 16
                    | (card.value as u64) << 8
                    | index as u64)
                    .rotate_left((card.id as usize + index * 7) as u32 % 61)
            })
        };
        let player_powers = combat.player.powers.iter().fold(0, |hash, power| {
            let state = (power.id as u64) << 16
                | power.amount as u16 as u64
                | if detailed {
                    (power.value as u16 as u64) << 32
                } else {
                    0
                };
            hash ^ state.rotate_left(power.id as u32 % 61)
        });
        let history = if detailed {
            [
                combat.stars,
                combat.osty.hp,
                combat.history.attacks,
                combat.history.skills,
                combat.history.shivs,
                combat.history.stars_gained,
                combat.history.block_gains,
                combat.history.cards,
                combat.history.manual_cards,
                combat.history.manual_plays,
                combat.history.powers,
                combat.history.energy,
                combat.history.exhausted,
                combat.history.discarded,
                combat.history.generated,
                combat.history.ethereal,
                combat.history.extra_drawn,
                combat.history.doom_applied,
                combat.history.osty_attacks,
                combat.history.hp_lost,
                combat.history.hp_loss_events,
                combat.history.feral_returns,
                combat.max_energy,
                combat.draw_per_turn as i16,
                combat.orb_slots as i16,
            ]
            .iter()
            .enumerate()
            .fold(0, |hash, (index, value)| {
                hash ^ (*value as u16 as u64).rotate_left((index * 8) as u32)
            })
        } else {
            0
        };
        let orbs = if detailed {
            combat
                .orbs
                .iter()
                .enumerate()
                .fold(0, |hash, (index, orb)| {
                    hash ^ ((orb.id as u64) << 16 | orb.value as u16 as u64)
                        .rotate_left((index * 11 + orb.id as usize) as u32 % 61)
                })
        } else {
            0
        };
        let resources = cards(&combat.hand, 0x4841_4e44)
            ^ cards(&combat.draw, 0x4452_4157)
            ^ cards(&combat.discard, 0x4449_5343)
            ^ cards(&combat.exhaust, 0x4558_4841)
            ^ player_powers
            ^ history
            ^ orbs;
        (
            combat.turn,
            combat.energy,
            combat.player.hp,
            combat.player.block,
            enemy_hp,
            doom,
            countdown,
            reaper,
            enemies ^ resources,
        )
    }

    fn room_key(game: &Game, content: &Content) -> (usize, u8, u8, u64) {
        let curses = game
            .run
            .deck
            .iter()
            .filter(|card| content.cards[card.id as usize].card_type == CardType::Curse)
            .count()
            .min(u8::MAX as usize) as u8;
        let engine = game.run.deck.iter().fold(0u64, |signature, card| {
            if !engine_card(game, content, *card) {
                return signature;
            }
            signature.wrapping_add(
                (card.id as u64 + 1)
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    .rotate_left(card.upgrades as u32 % 64),
            )
        });
        (
            game.map.current.unwrap_or(usize::MAX),
            (game.run.hp.max(0) as i32 * 8 / game.run.max_hp.max(1) as i32) as u8,
            curses,
            engine,
        )
    }

    fn room_values(game: &Game, content: &Content) -> [i32; 8] {
        let key = room_key(game, content);
        [
            game.run.hp as i32,
            game.run.max_hp as i32,
            -(key.2 as i32),
            game.run.relics.len() as i32,
            game.run.gold,
            game.run.potions.iter().flatten().count() as i32,
            -(game.run.deck.len() as i32),
            game.run
                .deck
                .iter()
                .map(|card| engine_score(game, content, *card) as i32)
                .sum(),
        ]
    }

    pub(super) fn retain_room_frontier<T>(
        states: &mut Vec<(Game, T)>,
        content: &Content,
        scalar_width: usize,
        width: usize,
    ) {
        let sort = |a: &(Game, T), b: &(Game, T)| {
            teacher_state_score(&b.0, content)
                .total_cmp(&teacher_state_score(&a.0, content))
                .then_with(|| room_key(&a.0, content).cmp(&room_key(&b.0, content)))
        };
        states.sort_by(sort);
        let mut pareto: Vec<(Game, T)> = Vec::new();
        for candidate in states.drain(..) {
            let key = room_key(&candidate.0, content);
            let values = room_values(&candidate.0, content);
            let dominates = |other: &(Game, T)| {
                let other_key = room_key(&other.0, content);
                other_key.0 == key.0
                    && other_key.3 == key.3
                    && room_values(&other.0, content)
                        .iter()
                        .zip(values)
                        .all(|(left, right)| *left >= right)
            };
            if pareto.iter().any(dominates) {
                continue;
            }
            pareto.retain(|other| {
                let other_key = room_key(&other.0, content);
                other_key.0 != key.0
                    || other_key.3 != key.3
                    || !values
                        .iter()
                        .zip(room_values(&other.0, content))
                        .all(|(left, right)| *left >= right)
            });
            pareto.push(candidate);
        }
        pareto.sort_by(sort);
        let scalar_width = scalar_width.min(width).min(pareto.len());
        let mut selected: Vec<(Game, T)> = pareto.drain(..scalar_width).collect();
        while selected.len() < width && !pareto.is_empty() {
            let index = (0..pareto.len())
                .max_by(|&a, &b| {
                    let novelty = |game: &Game| {
                        let key = room_key(game, content);
                        4 * selected
                            .iter()
                            .all(|(other, _)| key.0 != room_key(other, content).0)
                            as u8
                            + 2 * selected
                                .iter()
                                .all(|(other, _)| key.3 != room_key(other, content).3)
                                as u8
                            + selected
                                .iter()
                                .all(|(other, _)| key.1 != room_key(other, content).1)
                                as u8
                            + selected
                                .iter()
                                .all(|(other, _)| key.2 != room_key(other, content).2)
                                as u8
                    };
                    novelty(&pareto[a].0)
                        .cmp(&novelty(&pareto[b].0))
                        .then_with(|| {
                            teacher_state_score(&pareto[a].0, content)
                                .total_cmp(&teacher_state_score(&pareto[b].0, content))
                        })
                        .then_with(|| {
                            room_key(&pareto[b].0, content).cmp(&room_key(&pareto[a].0, content))
                        })
                })
                .unwrap();
            selected.push(pareto.swap_remove(index));
        }
        selected.sort_by(sort);
        *states = selected;
    }

    pub(super) fn search_actions(game: &Game, content: &Content) -> Vec<Action> {
        let mut actions = game.actions(content);
        if !game.replacing_potion
            && actions
                .iter()
                .any(|action| !matches!(action, Action::DiscardPotion(_)))
        {
            actions.retain(|action| !matches!(action, Action::DiscardPotion(_)));
        }
        actions
    }

    fn run_search(
        game: &Game,
        content: &Content,
        room_width: usize,
        combat_width: usize,
        combat_depth: usize,
        training_bonuses: (i16, i16),
    ) -> (Game, Vec<Action>) {
        let mut links: Vec<(Option<usize>, Action)> = Vec::new();
        let mut beam = vec![(game.clone(), None)];
        let mut best = beam[0].clone();
        let mut best_combat: Option<(Game, Option<usize>)> = None;
        for _ in 0..64 {
            let mut frontier = Vec::new();
            for (state, tail) in beam {
                for action in search_actions(&state, content) {
                    let mut child = state.clone();
                    let was_combat = child.combat().is_some();
                    if child.step(content, action.clone()).is_ok() {
                        if !was_combat && child.combat().is_some() {
                            apply_training_bonuses(
                                &mut child,
                                content,
                                training_bonuses.0,
                                training_bonuses.1,
                            );
                        }
                        links.push((tail, action));
                        frontier.push((child, Some(links.len() - 1)));
                    }
                }
            }
            let mut finished = Vec::new();
            for _ in 0..128 {
                let mut next = Vec::new();
                for (state, tail) in frontier {
                    if matches!(state.phase, Phase::Won) {
                        return (state, collect_plan(&links, tail));
                    }
                    if matches!(state.phase, Phase::Map) {
                        finished.push((state, tail));
                    } else if state.combat().is_some() {
                        let (child, path) =
                            combat_search(&state, content, combat_width, combat_depth);
                        if child.combat().is_none() {
                            let child_tail = extend_plan(&mut links, tail, path);
                            next.push((child, child_tail));
                        } else if !matches!(child.phase, Phase::Dead) {
                            let replace = best_combat.as_ref().is_none_or(|(best, _)| {
                                (child.run.act, child.run.floor) > (best.run.act, best.run.floor)
                                    || (child.run.act, child.run.floor)
                                        == (best.run.act, best.run.floor)
                                        && (combat_quality(&child) > combat_quality(best)
                                            || combat_quality(&child) == combat_quality(best)
                                                && teacher_state_score(&child, content)
                                                    > teacher_state_score(best, content))
                            });
                            if replace {
                                best_combat = Some((child, extend_plan(&mut links, tail, path)));
                            }
                        }
                    } else if !matches!(state.phase, Phase::Dead) {
                        for action in search_actions(&state, content) {
                            let mut child = state.clone();
                            let was_combat = child.combat().is_some();
                            if child.step(content, action.clone()).is_ok() {
                                if !was_combat && child.combat().is_some() {
                                    apply_training_bonuses(
                                        &mut child,
                                        content,
                                        training_bonuses.0,
                                        training_bonuses.1,
                                    );
                                }
                                links.push((tail, action));
                                next.push((child, Some(links.len() - 1)));
                            }
                        }
                    }
                }
                retain_room_frontier(&mut next, content, room_width * 8, room_width * 8 + 1);
                if next.is_empty() {
                    break;
                }
                frontier = next;
            }
            retain_room_frontier(&mut finished, content, room_width, room_width + 1);
            if finished.is_empty() {
                break;
            }
            if teacher_state_score(&finished[0].0, content) > teacher_state_score(&best.0, content)
            {
                best = finished[0].clone();
            }
            beam = finished;
        }
        let best = best_combat.unwrap_or(best);
        (best.0, collect_plan(&links, best.1))
    }

    #[pymethods]
    impl Batch {
        #[new]
        #[pyo3(signature = (size=256, seed=1, character=None, teacher_width=32, teacher_turns=1, ascension=10, seed_stride=1))]
        fn new(
            size: usize,
            seed: u64,
            character: Option<Id>,
            teacher_width: usize,
            teacher_turns: usize,
            ascension: u8,
            seed_stride: u64,
        ) -> PyResult<Self> {
            let content = foundation_content();
            if character.is_some_and(|x| x as usize >= content.characters.len()) {
                return Err(PyValueError::new_err("invalid character"));
            }
            let layout = Layout::new(&content);
            let mut batch = Self {
                archive: vec![vec![]; content.characters.len() * 64 * PHASES],
                archive_seen: vec![0; content.characters.len() * 64 * PHASES],
                first_archive: vec![],
                actions: vec![vec![]; size],
                plans: vec![vec![]; size],
                starts: vec![],
                root_ids: vec![usize::MAX; size],
                games: Vec::with_capacity(size),
                random: seed.max(1),
                next_seed: seed,
                seed_stride: seed_stride.max(1),
                character,
                ascension,
                teacher_width: teacher_width.max(1),
                teacher_turns: teacher_turns.max(1),
                first_teacher: None,
                training_strength: 0,
                training_dexterity: 0,
                resample_archive: true,
                archive_depth: 0,
                content,
                layout,
            };
            for _ in 0..size {
                let game = batch.fresh_game()?;
                batch.games.push(game);
            }
            Ok(batch)
        }

        fn sizes(&self) -> (usize, usize) {
            (self.layout.state_len, self.layout.action_len)
        }

        fn feature_layout(&self) -> Vec<usize> {
            vec![
                self.layout.card,
                CARD_ZONES,
                CARD_SLOTS,
                CARD_TOKEN_VALUES,
                self.layout.known_draw,
                2,
                KNOWN_DRAW_SLOTS,
                KNOWN_CARD_VALUES,
                self.layout.cards,
                self.layout.action_card,
                self.layout.character,
                self.layout.phase + 1,
                self.layout.room + room_index(Room::Boss),
            ]
        }

        fn token_layout(&self) -> std::collections::BTreeMap<&'static str, usize> {
            std::collections::BTreeMap::from([
                ("version", VERSION as usize),
                ("globals", self.layout.characters),
                ("token_values", TOKEN_VALUES),
                ("categorical", TOKEN_CATEGORICAL),
                ("numeric", TOKEN_NUMERIC),
                ("action_values", TOKEN_ACTION_VALUES),
                ("collections", ACTION_CARD_COLLECTION + 1),
                ("kinds", 32),
                ("ids", u16::MAX as usize + 2),
                ("owners", u16::MAX as usize + 2),
                ("positions", u16::MAX as usize + 2),
                ("card_types", 7),
                ("enchantments", ENCHANTMENTS + 1),
                ("afflictions", 4),
                ("character_start", 0),
                ("characters", self.layout.characters + 1),
                ("phases", PHASES + 1),
                ("rooms", ROOMS + 1),
                ("cards", self.layout.cards + 1),
                ("powers", self.layout.powers + 1),
                ("relics", self.layout.relics + 1),
                ("potions", self.layout.potions + 1),
                ("enemies", self.layout.enemies + 1),
                ("normal_encounters", 58),
                ("elite_encounters", 14),
                ("boss_encounters", 14),
                ("continuations", u16::MAX as usize + 2),
                ("card_zones", CARD_ZONES + 1),
                ("known_positions", 2 * KNOWN_DRAW_SLOTS + 1),
            ])
        }

        fn fingerprint(&self) -> u64 {
            content_fingerprint(&self.content)
        }

        fn load_snapshot(&mut self, snapshot: &str) -> PyResult<()> {
            let value: serde_json::Value = serde_json::from_str(snapshot)
                .map_err(|error| PyValueError::new_err(error.to_string()))?;
            let game = crate::replay::game_from_live_snapshot(
                &self.content,
                &value["before"],
                &value["oracle_before"],
            )
            .map_err(PyValueError::new_err)?;
            self.games = vec![game];
            self.actions = vec![vec![]];
            self.plans = vec![vec![]];
            self.starts.clear();
            self.root_ids = vec![usize::MAX];
            Ok(())
        }

        fn action_descriptors(
            &self,
        ) -> PyResult<Vec<(String, Option<String>, Option<String>, Option<String>)>> {
            let game = self
                .games
                .first()
                .ok_or_else(|| PyValueError::new_err("empty environment"))?;
            Ok(game
                .actions(&self.content)
                .iter()
                .map(|action| action_descriptor(game, &self.content, action))
                .collect())
        }

        fn seeds(&self) -> Vec<u32> {
            self.games.iter().map(|game| game.seed).collect()
        }

        fn root_ids(&self) -> Vec<usize> {
            self.root_ids.clone()
        }

        fn deck(&self, index: usize) -> PyResult<Vec<String>> {
            let game = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
            Ok(game
                .run
                .deck
                .iter()
                .map(|card| self.content.cards[card.id as usize].id.to_owned())
                .collect())
        }

        fn set_training_bonus(&mut self, bonus: i16) {
            let bonus = bonus.max(0);
            self.set_training_bonuses(bonus, bonus / 2);
        }

        fn set_training_bonuses(&mut self, strength: i16, dexterity: i16) {
            let bonuses = (strength.max(0), dexterity.max(0));
            let previous = (self.training_strength, self.training_dexterity);
            if bonuses == previous {
                return;
            }
            for game in self.games.iter_mut().chain(&mut self.starts) {
                apply_training_bonuses(game, &self.content, -previous.0, -previous.1);
                apply_training_bonuses(game, &self.content, bonuses.0, bonuses.1);
            }
            (self.training_strength, self.training_dexterity) = bonuses;
            self.plans.iter_mut().for_each(Vec::clear);
        }

        fn set_archive_resampling(&mut self, enabled: bool) {
            self.resample_archive = enabled;
        }

        fn set_archive_depth(&mut self, depth: usize) {
            self.archive_depth = depth;
        }

        #[pyo3(signature = (seed, choices, character=None))]
        fn archive_path(
            &mut self,
            seed: u64,
            choices: Vec<usize>,
            character: Option<Id>,
        ) -> PyResult<bool> {
            let character = character.or(self.character).unwrap_or(0);
            if character as usize >= self.content.characters.len() {
                return Err(PyValueError::new_err("invalid character"));
            }
            let mut game =
                Game::new_character_ascension(&self.content, seed, character, self.ascension)
                    .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
            game.begin_run(&self.content)
                .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
            for choice in choices {
                let was_combat = game.combat().is_some();
                let action = game
                    .actions(&self.content)
                    .get(choice)
                    .cloned()
                    .ok_or_else(|| PyValueError::new_err("invalid archived action"))?;
                game.step(&self.content, action.clone())
                    .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
                if matches!(action, Action::Path(_)) {
                    let floor = (game.run.act.saturating_sub(1) as usize * 18
                        + game.run.floor as usize)
                        .min(63);
                    let key = (game.run.character as usize * 64 + floor) * PHASES
                        + phase_index(&game.phase);
                    if self.archive[key].len() < 16 {
                        self.archive[key].push(game.clone());
                    }
                }
                if !was_combat && game.combat().is_some() {
                    apply_training_bonuses(
                        &mut game,
                        &self.content,
                        self.training_strength,
                        self.training_dexterity,
                    );
                }
                if matches!(game.phase, Phase::Won | Phase::Dead) {
                    break;
                }
            }
            Ok(matches!(game.phase, Phase::Won))
        }

        fn set_teacher(&mut self, width: usize, turns: usize) {
            self.teacher_width = width.max(1);
            self.teacher_turns = turns.max(1);
            self.plans.iter_mut().for_each(Vec::clear);
        }

        fn set_first_teacher(&mut self, width: usize, turns: usize) {
            self.first_teacher = Some((width.max(1), turns.max(1)));
            self.plans.first_mut().into_iter().for_each(Vec::clear);
        }

        fn mark_starts(&mut self) {
            self.starts.clone_from(&self.games);
        }

        fn repeat_exact(&mut self, index: usize) -> PyResult<()> {
            let game = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?
                .clone();
            self.games.fill(game);
            self.actions.iter_mut().for_each(Vec::clear);
            self.plans.iter_mut().for_each(Vec::clear);
            Ok(())
        }

        fn pair_exact(&mut self) {
            self.games.par_chunks_mut(2).for_each(|pair| {
                if pair.len() == 2 {
                    pair[1] = pair[0].clone();
                }
            });
            self.actions.iter_mut().for_each(Vec::clear);
            self.plans.iter_mut().for_each(Vec::clear);
        }

        fn repeat_groups(&mut self, size: usize) -> PyResult<()> {
            if size == 0 || self.games.len() % size != 0 {
                return Err(PyValueError::new_err("invalid group size"));
            }
            self.games.par_chunks_mut(size).for_each(|group| {
                let game = group[0].clone();
                group[1..].fill(game);
            });
            self.actions.iter_mut().for_each(Vec::clear);
            self.plans.iter_mut().for_each(Vec::clear);
            Ok(())
        }

        #[pyo3(signature = (index=0, repeats=1, seed=None))]
        fn repeat_resampled(
            &mut self,
            index: usize,
            repeats: usize,
            seed: Option<u64>,
        ) -> PyResult<()> {
            let base = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?
                .clone();
            let repeats = repeats.max(1);
            let mut random = seed.map(Rng::from_seed);
            let mut particle = 0;
            let seeds = (0..self.games.len())
                .map(|index| {
                    if index % repeats == 0 {
                        particle = random.as_mut().map_or_else(|| self.random_u64(), Rng::next);
                    }
                    particle
                })
                .collect::<Vec<_>>();
            let content = &self.content;
            self.games
                .par_iter_mut()
                .zip(seeds)
                .for_each(|(game, seed)| {
                    *game = base.clone();
                    resample_hidden(game, content, seed);
                });
            self.actions.iter_mut().for_each(Vec::clear);
            self.plans.iter_mut().for_each(Vec::clear);
            Ok(())
        }

        #[pyo3(signature = (depth=0))]
        fn repeat_best(&mut self, depth: usize) {
            let character = self.character.unwrap_or(self.games[0].run.character) as usize;
            let mut floors = self
                .archive
                .iter()
                .enumerate()
                .filter(|(key, games)| !games.is_empty() && *key / (64 * PHASES) == character)
                .map(|(key, _)| key / PHASES % 64)
                .collect::<Vec<_>>();
            floors.sort_unstable();
            floors.dedup();
            let Some(&floor) = floors.iter().rev().nth(depth) else {
                return;
            };
            let mut roots = self
                .archive
                .iter()
                .enumerate()
                .filter(|(key, _)| *key / (64 * PHASES) == character && *key / PHASES % 64 == floor)
                .flat_map(|(_, games)| games)
                .cloned()
                .collect::<Vec<_>>();
            roots.sort_by(|a, b| {
                teacher_state_score(b, &self.content)
                    .total_cmp(&teacher_state_score(a, &self.content))
            });
            if !roots.is_empty() {
                let seeds = (0..self.games.len())
                    .map(|_| self.random_u64())
                    .collect::<Vec<_>>();
                for (index, (game, seed)) in self.games.iter_mut().zip(seeds).enumerate() {
                    *game = roots[index % roots.len()].clone();
                    if self.resample_archive {
                        resample_hidden(game, &self.content, seed);
                    }
                    apply_training_bonuses(
                        game,
                        &self.content,
                        self.training_strength,
                        self.training_dexterity,
                    );
                }
                self.actions.iter_mut().for_each(Vec::clear);
                self.plans.iter_mut().for_each(Vec::clear);
            }
        }

        fn restore(&mut self) -> PyResult<()> {
            if self.starts.len() != self.games.len() {
                return Err(PyValueError::new_err("mark starts before restore"));
            }
            self.games.clone_from(&self.starts);
            self.actions.iter_mut().for_each(Vec::clear);
            self.plans.iter_mut().for_each(Vec::clear);
            Ok(())
        }

        #[pyo3(signature = (active=None))]
        fn teacher(&mut self, active: Option<Vec<bool>>) -> PyResult<Vec<usize>> {
            let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
            if active.len() != self.games.len() {
                return Err(PyValueError::new_err("invalid active mask"));
            }
            Ok(self
                .games
                .par_iter()
                .zip(self.plans.par_iter_mut())
                .zip(active)
                .enumerate()
                .map(|(index, ((game, plan), active))| {
                    if !active {
                        return 0;
                    }
                    let actions = game.actions(&self.content);
                    if plan.first().is_none_or(|action| !actions.contains(action)) {
                        let (width, turns) = if index == 0 {
                            self.first_teacher
                                .unwrap_or((self.teacher_width, self.teacher_turns))
                        } else {
                            (self.teacher_width, self.teacher_turns)
                        };
                        *plan = teacher_plan(game, &self.content, width, turns);
                    }
                    let Some(action) = plan.first().cloned() else {
                        return 0;
                    };
                    actions
                        .iter()
                        .position(|legal| *legal == action)
                        .unwrap_or(0)
                })
                .collect())
        }

        fn teacher_first(&self) -> usize {
            let game = &self.games[0];
            let actions = game.actions(&self.content);
            let (width, turns) = self
                .first_teacher
                .unwrap_or((self.teacher_width, self.teacher_turns));
            teacher_plan(game, &self.content, width, turns)
                .first()
                .and_then(|action| actions.iter().position(|legal| legal == action))
                .unwrap_or(0)
        }

        #[pyo3(signature = (choices, danger=1.0, tactical=false))]
        fn rescue(&self, choices: Vec<usize>, danger: f32, tactical: bool) -> PyResult<Vec<usize>> {
            if choices.len() != self.games.len() {
                return Err(PyValueError::new_err("invalid choices"));
            }
            Ok(self
                .games
                .par_iter()
                .zip(choices)
                .map(|(game, choice)| rescue_choice(game, &self.content, choice, danger, tactical))
                .collect())
        }

        #[pyo3(signature = (index, width=64, turns=3))]
        fn teacher_at(&self, index: usize, width: usize, turns: usize) -> PyResult<usize> {
            let game = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
            let actions = game.actions(&self.content);
            Ok(teacher_plan(game, &self.content, width, turns)
                .first()
                .and_then(|action| actions.iter().position(|legal| legal == action))
                .unwrap_or(0))
        }

        #[pyo3(signature = (index, width=1024, depth=128))]
        fn search_at(
            &self,
            index: usize,
            width: usize,
            depth: usize,
        ) -> PyResult<(bool, Vec<usize>, f32)> {
            let game = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
            let (result, plan) = combat_search(game, &self.content, width.max(1), depth.max(1));
            let mut replay = game.clone();
            let mut choices = Vec::with_capacity(plan.len());
            for action in plan {
                let actions = replay.actions(&self.content);
                choices.push(actions.iter().position(|legal| *legal == action).unwrap());
                replay.step(&self.content, action).unwrap();
            }
            Ok((
                result.combat().is_none() && !matches!(result.phase, Phase::Dead),
                choices,
                teacher_state_score(&result, &self.content),
            ))
        }

        #[pyo3(signature = (width=64, depth=160))]
        fn search_scores(&self, width: usize, depth: usize) -> Vec<(bool, f32)> {
            self.games
                .par_iter()
                .map(|game| {
                    let (result, _) = if game.combat().is_some() {
                        combat_search(game, &self.content, width.max(1), depth.max(1))
                    } else {
                        (game.clone(), Vec::new())
                    };
                    (
                        result.combat().is_none() && !matches!(result.phase, Phase::Dead),
                        combat_quality(&result),
                    )
                })
                .collect()
        }

        fn run_scores(
            &self,
            room_width: usize,
            combat_width: usize,
            combat_depth: usize,
        ) -> Vec<(bool, f32)> {
            self.games
                .par_iter()
                .map(|game| {
                    let (result, _) = run_search(
                        game,
                        &self.content,
                        room_width.max(1),
                        combat_width.max(1),
                        combat_depth.max(1),
                        (self.training_strength, self.training_dexterity),
                    );
                    (
                        matches!(result.phase, Phase::Won),
                        teacher_state_score(&result, &self.content),
                    )
                })
                .collect()
        }

        #[pyo3(signature = (width=64, depth=160))]
        fn search_all(&self, width: usize, depth: usize) -> Vec<(bool, Vec<usize>, f32)> {
            self.games
                .par_iter()
                .map(|game| {
                    let (result, plan) =
                        combat_search(game, &self.content, width.max(1), depth.max(1));
                    let mut replay = game.clone();
                    let choices = plan
                        .into_iter()
                        .map(|action| {
                            let actions = replay.actions(&self.content);
                            let choice = actions.iter().position(|legal| *legal == action).unwrap();
                            replay.step(&self.content, action).unwrap();
                            choice
                        })
                        .collect();
                    (
                        result.combat().is_none() && !matches!(result.phase, Phase::Dead),
                        choices,
                        combat_quality(&result),
                    )
                })
                .collect()
        }

        #[pyo3(signature = (index, room_width=4, combat_width=128, combat_depth=160))]
        fn run_search_at(
            &self,
            index: usize,
            room_width: usize,
            combat_width: usize,
            combat_depth: usize,
        ) -> PyResult<(bool, Vec<usize>, f32)> {
            let game = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
            let (result, plan) = run_search(
                game,
                &self.content,
                room_width.max(1),
                combat_width.max(1),
                combat_depth.max(1),
                (self.training_strength, self.training_dexterity),
            );
            let mut replay = game.clone();
            let mut choices = Vec::with_capacity(plan.len());
            for action in plan {
                let actions = replay.actions(&self.content);
                choices.push(actions.iter().position(|legal| *legal == action).unwrap());
                let was_combat = replay.combat().is_some();
                replay.step(&self.content, action).unwrap();
                if !was_combat && replay.combat().is_some() {
                    apply_training_bonuses(
                        &mut replay,
                        &self.content,
                        self.training_strength,
                        self.training_dexterity,
                    );
                }
            }
            Ok((
                matches!(result.phase, Phase::Won),
                choices,
                teacher_state_score(&result, &self.content),
            ))
        }

        #[pyo3(signature = (index, source_bonus, choices, room_width=2, combat_width=64, combat_depth=160, source_dexterity=None))]
        fn repair_path_at(
            &self,
            index: usize,
            source_bonus: i16,
            choices: Vec<usize>,
            room_width: usize,
            combat_width: usize,
            combat_depth: usize,
            source_dexterity: Option<i16>,
        ) -> PyResult<(bool, Vec<usize>, f32)> {
            let root = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
            let mut source = root.clone();
            let source_bonus = source_bonus.max(0);
            let source_dexterity = source_dexterity.unwrap_or(source_bonus / 2).max(0);
            if source.combat().is_some() {
                apply_training_bonuses(
                    &mut source,
                    &self.content,
                    -self.training_strength,
                    -self.training_dexterity,
                );
                apply_training_bonuses(&mut source, &self.content, source_bonus, source_dexterity);
            }
            let mut route = Vec::with_capacity(choices.len());
            for choice in choices {
                let action = source
                    .actions(&self.content)
                    .get(choice)
                    .cloned()
                    .ok_or_else(|| PyValueError::new_err("invalid source path"))?;
                let combat = source.combat().is_some();
                let features = (!combat)
                    .then(|| action_features(&source, self.layout, &action))
                    .unwrap_or_default();
                source
                    .step(&self.content, action.clone())
                    .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
                if !combat && source.combat().is_some() {
                    apply_training_bonuses(
                        &mut source,
                        &self.content,
                        source_bonus,
                        source_dexterity,
                    );
                }
                route.push((action, combat, features));
            }

            let mut target = root.clone();
            let mut repaired = Vec::new();
            let append = |game: &mut Game, path: &mut Vec<usize>, action: Action| {
                let choice = game
                    .actions(&self.content)
                    .iter()
                    .position(|candidate| *candidate == action)
                    .ok_or_else(|| PyValueError::new_err("repaired action is not legal"))?;
                let combat = game.combat().is_some();
                game.step(&self.content, action)
                    .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
                if !combat && game.combat().is_some() {
                    apply_training_bonuses(
                        game,
                        &self.content,
                        self.training_strength,
                        self.training_dexterity,
                    );
                }
                path.push(choice);
                Ok::<(), PyErr>(())
            };
            let mut cursor = 0;
            let mut fallback = false;
            loop {
                if target.combat().is_some() {
                    let (result, plan) = combat_search(
                        &target,
                        &self.content,
                        combat_width.max(1),
                        combat_depth.max(1),
                    );
                    for action in plan {
                        append(&mut target, &mut repaired, action)?;
                    }
                    while cursor < route.len() && route[cursor].1 {
                        cursor += 1;
                    }
                    if result.combat().is_some() || matches!(result.phase, Phase::Dead) {
                        break;
                    }
                    continue;
                }
                while cursor < route.len() && route[cursor].1 {
                    cursor += 1;
                }
                if cursor == route.len() || matches!(target.phase, Phase::Won | Phase::Dead) {
                    break;
                }
                let (action, _, features) = &route[cursor];
                let matches = target.actions(&self.content).iter().any(|candidate| {
                    candidate == action
                        && action_features(&target, self.layout, candidate) == *features
                });
                if !matches {
                    fallback = true;
                    break;
                }
                append(&mut target, &mut repaired, action.clone())?;
                cursor += 1;
            }
            if fallback {
                let (_, suffix) = run_search(
                    &target,
                    &self.content,
                    room_width.max(1),
                    combat_width.max(1),
                    combat_depth.max(1),
                    (self.training_strength, self.training_dexterity),
                );
                for action in suffix {
                    append(&mut target, &mut repaired, action)?;
                }
            }
            Ok((
                matches!(target.phase, Phase::Won),
                repaired,
                teacher_state_score(&target, &self.content),
            ))
        }

        #[pyo3(signature = (room_width=2, combat_width=64, combat_depth=160, count=None))]
        fn run_search_all(
            &self,
            room_width: usize,
            combat_width: usize,
            combat_depth: usize,
            count: Option<usize>,
        ) -> Vec<(u32, bool, Vec<usize>, f32)> {
            self.games[..count.unwrap_or(self.games.len()).min(self.games.len())]
                .par_iter()
                .map(|game| {
                    let (result, plan) = run_search(
                        game,
                        &self.content,
                        room_width.max(1),
                        combat_width.max(1),
                        combat_depth.max(1),
                        (self.training_strength, self.training_dexterity),
                    );
                    let mut replay = game.clone();
                    let choices = plan
                        .into_iter()
                        .map(|action| {
                            let actions = replay.actions(&self.content);
                            let choice = actions.iter().position(|legal| *legal == action).unwrap();
                            let was_combat = replay.combat().is_some();
                            replay.step(&self.content, action).unwrap();
                            if !was_combat && replay.combat().is_some() {
                                apply_training_bonuses(
                                    &mut replay,
                                    &self.content,
                                    self.training_strength,
                                    self.training_dexterity,
                                );
                            }
                            choice
                        })
                        .collect();
                    (
                        game.seed,
                        matches!(result.phase, Phase::Won),
                        choices,
                        teacher_state_score(&result, &self.content),
                    )
                })
                .collect()
        }

        fn stats(&self) -> Vec<(u8, u8, i16, i16, u8, u16, i16, i32, f32)> {
            let content = &self.content;
            self.games
                .iter()
                .map(|game| {
                    let (turn, player_hp, enemy_hp) = game.combat().map_or((0, 0, 0), |combat| {
                        (
                            combat.turn,
                            combat.player.hp,
                            combat
                                .enemies
                                .iter()
                                .map(|enemy| {
                                    let future = if content.enemies[enemy.creature.id as usize].id
                                        == "MONSTER.TEST_SUBJECT"
                                        && enemy.creature.powers.iter().any(|power| {
                                            content.powers[power.id as usize].id
                                                == "POWER.ADAPTABLE_POWER"
                                        }) {
                                        if enemy.creature.max_hp < 200 {
                                            525
                                        } else {
                                            313
                                        }
                                    } else {
                                        0
                                    };
                                    enemy.creature.hp.max(0) as i32 + future
                                })
                                .sum(),
                        )
                    });
                    (
                        game.run.act,
                        game.run.floor,
                        game.run.hp,
                        game.run.max_hp,
                        phase_index(&game.phase) as u8,
                        turn,
                        player_hp,
                        enemy_hp,
                        teacher_state_score(game, content),
                    )
                })
                .collect()
        }

        fn describe(&self, index: usize, choice: usize) -> PyResult<String> {
            let game = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
            let actions = game.actions(&self.content);
            let action = actions
                .get(choice)
                .ok_or_else(|| PyValueError::new_err("invalid action index"))?;
            let card = match action {
                Action::Play { hand, .. } => game
                    .combat()
                    .and_then(|combat| combat.hand.get(*hand))
                    .map(|card| self.content.cards[card.id as usize].id),
                Action::RewardCard(index) => match &game.phase {
                    Phase::Rewards(rewards) => rewards
                        .cards
                        .get(*index)
                        .map(|card| self.content.cards[card.id as usize].id),
                    _ => None,
                },
                Action::Buy(index) => match &game.phase {
                    Phase::Shop(items) => items.get(*index).and_then(|item| match item {
                        ShopItem::Card(card, _) => Some(self.content.cards[card.id as usize].id),
                        _ => None,
                    }),
                    _ => None,
                },
                _ => None,
            };
            let combat = game.combat().map_or_else(String::new, |combat| {
                format!(
                    " turn={} energy={} player={}/{} block={} enemies={:?}",
                    combat.turn,
                    combat.energy,
                    combat.player.hp,
                    combat.player.max_hp,
                    combat.player.block,
                    combat
                        .enemies
                        .iter()
                        .map(|enemy| (
                            self.content.enemies[enemy.creature.id as usize].id,
                            enemy.creature.hp,
                            enemy.creature.block,
                            enemy.move_index,
                        ))
                        .collect::<Vec<_>>()
                )
            });
            Ok(format!(
                "act={} floor={} hp={}/{} phase={}{} action={action:?} card={card:?}",
                game.run.act,
                game.run.floor,
                game.run.hp,
                game.run.max_hp,
                phase_index(&game.phase),
                combat,
            ))
        }

        fn run_summary(&self, index: usize) -> PyResult<String> {
            let game = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
            Ok(format!(
                "act={} floor={} hp={}/{} gold={} relics={:?} deck={:?}",
                game.run.act,
                game.run.floor,
                game.run.hp,
                game.run.max_hp,
                game.run.gold,
                game.run
                    .relics
                    .iter()
                    .map(|id| self.content.relics[*id as usize].id)
                    .collect::<Vec<_>>(),
                game.run
                    .deck
                    .iter()
                    .map(|card| (self.content.cards[card.id as usize].id, card.upgrades,))
                    .collect::<Vec<_>>(),
            ))
        }

        #[allow(clippy::type_complexity)]
        #[pyo3(signature = (active=None))]
        fn observe<'py>(
            &mut self,
            py: Python<'py>,
            active: Option<Vec<bool>>,
        ) -> PyResult<(
            Bound<'py, PyArray2<f32>>,
            Bound<'py, PyArray3<f32>>,
            Bound<'py, PyArray2<u8>>,
            Bound<'py, PyArray1<f32>>,
        )> {
            let layout = self.layout;
            let content = &self.content;
            let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
            if active.len() != self.games.len() {
                return Err(PyValueError::new_err("invalid active mask"));
            }
            let rows: Vec<_> = self
                .games
                .par_iter()
                .zip(&active)
                .map(|(game, &active)| {
                    if !active {
                        return (vec![0.0; layout.state_len], Vec::new(), Vec::new(), 0.0);
                    }
                    let (actions, represented) = represented_actions(game, content);
                    let action_rows = represented
                        .iter()
                        .map(|action| action_features(game, layout, action))
                        .collect::<Vec<_>>();
                    (
                        state_features(game, layout),
                        actions,
                        action_rows,
                        potential(game),
                    )
                })
                .collect();
            let max_actions = rows.iter().map(|x| x.2.len()).max().unwrap_or(0).max(1);
            let mut states = Vec::with_capacity(rows.len() * layout.state_len);
            let mut action_rows = vec![0.0; rows.len() * max_actions * layout.action_len];
            let mut mask = vec![0; rows.len() * max_actions];
            let mut potentials = Vec::with_capacity(rows.len());
            self.actions.clear();
            for (batch, (state, actions, features, value)) in rows.into_iter().enumerate() {
                let legal = actions.len();
                states.extend(state);
                for (index, features) in features.into_iter().enumerate() {
                    let offset = (batch * max_actions + index) * layout.action_len;
                    action_rows[offset..offset + layout.action_len].copy_from_slice(&features);
                    mask[batch * max_actions + index] = (index < legal) as u8;
                }
                self.actions.push(actions);
                potentials.push(value);
            }
            Ok((
                ndarray::Array2::from_shape_vec((self.games.len(), layout.state_len), states)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array3::from_shape_vec(
                    (self.games.len(), max_actions, layout.action_len),
                    action_rows,
                )
                .unwrap()
                .into_pyarray(py),
                ndarray::Array2::from_shape_vec((self.games.len(), max_actions), mask)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array1::from_vec(potentials).into_pyarray(py),
            ))
        }

        #[allow(clippy::type_complexity)]
        #[pyo3(signature = (active=None))]
        fn observe_tokens<'py>(
            &mut self,
            py: Python<'py>,
            active: Option<Vec<bool>>,
        ) -> PyResult<(
            Bound<'py, PyArray2<f32>>,
            Bound<'py, PyArray3<f32>>,
            Bound<'py, PyArray2<u8>>,
            Bound<'py, PyArray3<f32>>,
            Bound<'py, PyArray4<f32>>,
            Bound<'py, PyArray3<u8>>,
            Bound<'py, PyArray2<u8>>,
            Bound<'py, PyArray2<u8>>,
            Bound<'py, PyArray1<f32>>,
        )> {
            let layout = self.layout;
            let content = &self.content;
            let bonuses = (self.training_strength, self.training_dexterity);
            let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
            if active.len() != self.games.len() {
                return Err(PyValueError::new_err("invalid active mask"));
            }
            let rows = self
                .games
                .par_iter()
                .zip(&active)
                .map(|(game, &active)| {
                    if !active {
                        return (
                            vec![0.0; layout.characters],
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                            0.0,
                        );
                    }
                    let (actions, legal) = candidate_actions(game, content);
                    debug_assert!(game
                        .combat()
                        .is_none_or(|combat| combat.queue.is_empty() || combat.choice.is_some()));
                    let state = state_tokens_with_bonuses(game, content, layout, bonuses);
                    let action_rows = actions
                        .iter()
                        .zip(&legal)
                        .map(|(action, &legal)| {
                            tokenized_candidate(game, content, layout, action, legal)
                        })
                        .collect();
                    (
                        observation_metadata(game, layout),
                        state,
                        actions,
                        legal,
                        action_rows,
                        potential(game),
                    )
                })
                .collect::<Vec<_>>();
            let batches = rows.len();
            let max_state = rows.iter().map(|row| row.1.len()).max().unwrap_or(0).max(1);
            let max_actions = rows.iter().map(|row| row.4.len()).max().unwrap_or(0).max(1);
            let max_action_tokens = rows
                .iter()
                .flat_map(|row| row.4.iter().map(|action| action.1.len()))
                .max()
                .unwrap_or(0)
                .max(1);
            let mut metadata = Vec::with_capacity(batches * layout.characters);
            let mut state_tokens = vec![0.0; batches * max_state * TOKEN_VALUES];
            let mut state_mask = vec![0; batches * max_state];
            let mut action_values = vec![0.0; batches * max_actions * TOKEN_ACTION_VALUES];
            let mut action_tokens =
                vec![0.0; batches * max_actions * max_action_tokens * TOKEN_VALUES];
            let mut action_token_mask = vec![0; batches * max_actions * max_action_tokens];
            let mut candidate_mask = vec![0; batches * max_actions];
            let mut legal_mask = vec![0; batches * max_actions];
            let mut potentials = Vec::with_capacity(batches);
            self.actions.clear();
            for (batch, (meta, state, actions, legal, action_rows, potential)) in
                rows.into_iter().enumerate()
            {
                metadata.extend(meta);
                for (position, token) in state.into_iter().enumerate() {
                    let offset = (batch * max_state + position) * TOKEN_VALUES;
                    state_tokens[offset..offset + TOKEN_VALUES].copy_from_slice(&token);
                    state_mask[batch * max_state + position] = 1;
                }
                for (position, (values, tokens)) in action_rows.into_iter().enumerate() {
                    let offset = (batch * max_actions + position) * TOKEN_ACTION_VALUES;
                    action_values[offset..offset + TOKEN_ACTION_VALUES].copy_from_slice(&values);
                    candidate_mask[batch * max_actions + position] = 1;
                    legal_mask[batch * max_actions + position] = legal[position] as u8;
                    for (token_position, token) in tokens.into_iter().enumerate() {
                        let row =
                            (batch * max_actions + position) * max_action_tokens + token_position;
                        let offset = row * TOKEN_VALUES;
                        action_tokens[offset..offset + TOKEN_VALUES].copy_from_slice(&token);
                        action_token_mask[row] = 1;
                    }
                }
                self.actions.push(actions);
                potentials.push(potential);
            }
            Ok((
                ndarray::Array2::from_shape_vec((batches, layout.characters), metadata)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array3::from_shape_vec((batches, max_state, TOKEN_VALUES), state_tokens)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array2::from_shape_vec((batches, max_state), state_mask)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array3::from_shape_vec(
                    (batches, max_actions, TOKEN_ACTION_VALUES),
                    action_values,
                )
                .unwrap()
                .into_pyarray(py),
                ndarray::Array4::from_shape_vec(
                    (batches, max_actions, max_action_tokens, TOKEN_VALUES),
                    action_tokens,
                )
                .unwrap()
                .into_pyarray(py),
                ndarray::Array3::from_shape_vec(
                    (batches, max_actions, max_action_tokens),
                    action_token_mask,
                )
                .unwrap()
                .into_pyarray(py),
                ndarray::Array2::from_shape_vec((batches, max_actions), legal_mask)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array2::from_shape_vec((batches, max_actions), candidate_mask)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array1::from_vec(potentials).into_pyarray(py),
            ))
        }

        #[pyo3(signature = (choices, active=None))]
        fn step(
            &mut self,
            choices: Vec<usize>,
            active: Option<Vec<bool>>,
        ) -> PyResult<(Vec<f32>, Vec<bool>, Vec<f32>)> {
            if choices.len() != self.games.len() || self.actions.len() != self.games.len() {
                return Err(PyValueError::new_err("observe before step"));
            }
            let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
            if active.len() != self.games.len() {
                return Err(PyValueError::new_err("invalid active mask"));
            }
            let selected = self
                .actions
                .iter()
                .zip(&choices)
                .zip(&active)
                .enumerate()
                .map(|(environment, ((actions, &choice), &active))| {
                    if active {
                        actions.get(choice).cloned().map(Some).ok_or_else(|| {
                            format!(
                                "environment {environment} chose action {choice} from {}",
                                actions.len()
                            )
                        })
                    } else {
                        Ok(None)
                    }
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(PyValueError::new_err)?;
            let content = &self.content;
            let results = self
                .games
                .par_iter_mut()
                .zip(&selected)
                .map(|(game, action)| {
                    let Some(action) = action else {
                        return Ok((0.0, false, potential(game), false));
                    };
                    let was_combat = game.combat().is_some();
                    game.step(content, action.clone())
                        .map_err(|error| format!("{error:?}"))?;
                    let reward = matches!(game.phase, Phase::Won) as u8 as f32;
                    let done = matches!(game.phase, Phase::Won | Phase::Dead);
                    let next = potential(game);
                    Ok((reward, done, next, !was_combat && game.combat().is_some()))
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(PyValueError::new_err)?;
            for plan in &mut self.plans {
                plan.clear();
            }
            for (index, action) in selected.iter().enumerate() {
                if matches!(action, Some(Action::Path(_))) {
                    self.remember(index);
                }
            }
            for (game, result) in self.games.iter_mut().zip(&results) {
                if result.3 {
                    apply_training_bonuses(
                        game,
                        &self.content,
                        self.training_strength,
                        self.training_dexterity,
                    );
                }
            }
            let mut rewards = Vec::with_capacity(results.len());
            let mut done = Vec::with_capacity(results.len());
            let mut potentials = Vec::with_capacity(results.len());
            for (reward, terminal, value, _) in results {
                rewards.push(reward);
                done.push(terminal);
                potentials.push(value);
            }
            Ok((rewards, done, potentials))
        }

        #[pyo3(signature = (indices, archive_probability=0.0))]
        fn reset(&mut self, indices: Vec<usize>, archive_probability: f32) -> PyResult<()> {
            for index in indices {
                if index >= self.games.len() {
                    return Err(PyValueError::new_err("invalid environment index"));
                }
                let archived = self.random_f32() < archive_probability.clamp(0.0, 1.0);
                (self.games[index], self.root_ids[index]) = if archived {
                    if let Some(root) = self.archived_game() {
                        root
                    } else {
                        (self.fresh_game()?, usize::MAX)
                    }
                } else {
                    (self.fresh_game()?, usize::MAX)
                };
                apply_training_bonuses(
                    &mut self.games[index],
                    &self.content,
                    self.training_strength,
                    self.training_dexterity,
                );
                self.actions[index].clear();
                self.plans[index].clear();
            }
            Ok(())
        }

        fn rust_values(&self, path: &str) -> PyResult<Vec<f32>> {
            let model = ValueModel::load(path, &self.content)
                .map_err(|error| PyValueError::new_err(error.to_string()))?;
            Ok(self
                .games
                .par_iter()
                .map(|game| model.win_probability(game))
                .collect())
        }
    }

    impl Batch {
        fn random_u64(&mut self) -> u64 {
            self.random ^= self.random << 13;
            self.random ^= self.random >> 7;
            self.random ^= self.random << 17;
            self.random
        }

        fn random_f32(&mut self) -> f32 {
            (self.random_u64() >> 40) as f32 / (1u32 << 24) as f32
        }

        fn fresh_game(&mut self) -> PyResult<Game> {
            let character = self.character.unwrap_or_else(|| {
                (self.random_u64() as usize % self.content.characters.len()) as Id
            });
            let seed = self.next_seed;
            self.next_seed = self.next_seed.wrapping_add(self.seed_stride);
            let mut game =
                Game::new_character_ascension(&self.content, seed, character, self.ascension)
                    .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
            game.begin_run(&self.content)
                .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
            Ok(game)
        }

        fn remember(&mut self, index: usize) {
            let game = self.games[index].clone();
            if matches!(game.phase, Phase::Won | Phase::Dead) {
                return;
            }
            if index == 0 {
                self.first_archive.push(game.clone());
            }
            let floor =
                (game.run.act.saturating_sub(1) as usize * 18 + game.run.floor as usize).min(63);
            let key =
                (game.run.character as usize * 64 + floor) * PHASES + phase_index(&game.phase);
            self.archive_seen[key] += 1;
            let replacement = self.random_u64() % self.archive_seen[key];
            let bucket = &mut self.archive[key];
            if bucket.len() < 16 {
                bucket.push(game);
            } else if replacement < 16 {
                bucket[replacement as usize] = game;
            }
        }

        fn archived_game(&mut self) -> Option<(Game, usize)> {
            let character = if let Some(character) = self.character {
                character as usize
            } else {
                let characters = (0..self.content.characters.len())
                    .filter(|character| {
                        self.archive[character * 64 * PHASES..(character + 1) * 64 * PHASES]
                            .iter()
                            .any(|bucket| !bucket.is_empty())
                    })
                    .collect::<Vec<_>>();
                if characters.is_empty() {
                    return None;
                }
                *characters.get(self.random_u64() as usize % characters.len())?
            };
            let highest = self
                .archive
                .iter()
                .enumerate()
                .filter(|(key, bucket)| !bucket.is_empty() && *key / (64 * PHASES) == character)
                .map(|(key, _)| key / PHASES % 64)
                .max()?;
            let target = highest.saturating_sub(self.archive_depth);
            let distance = self
                .archive
                .iter()
                .enumerate()
                .filter(|(key, bucket)| !bucket.is_empty() && key / (64 * PHASES) == character)
                .map(|(key, _)| (key / PHASES % 64).abs_diff(target))
                .min()?;
            let candidates = self
                .archive
                .iter()
                .enumerate()
                .filter(|(key, bucket)| {
                    !bucket.is_empty()
                        && (key / PHASES % 64).abs_diff(target) == distance
                        && key / (64 * PHASES) == character
                })
                .map(|(key, _)| key)
                .collect::<Vec<_>>();
            let key = candidates[self.random_u64() as usize % candidates.len()];
            let index = self.random_u64() as usize % self.archive[key].len();
            let mut game = self.archive[key][index].clone();
            if self.resample_archive {
                let seed = self.random_u64();
                resample_hidden(&mut game, &self.content, seed);
            }
            Some((game, key * 16 + index))
        }
    }

    #[pymodule]
    fn sts2_sim(module: &Bound<'_, PyModule>) -> PyResult<()> {
        module.add_class::<Batch>()
    }

    #[test]
    fn rescue_ends_turn_after_32_manual_plays() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let actions = game.actions(&content);
        let play = actions
            .iter()
            .position(|action| matches!(action, Action::Play { .. }))
            .unwrap();
        let end_turn = actions
            .iter()
            .position(|action| matches!(action, Action::EndTurn))
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            panic!("combat did not start");
        };
        combat.history.manual_plays = 31;
        assert_eq!(rescue_choice(&game, &content, play, 0.5, true), play);
        let Phase::Combat(combat) = &mut game.phase else {
            panic!("combat ended");
        };
        combat.history.manual_plays = 32;
        assert_eq!(rescue_choice(&game, &content, play, 0.5, true), end_turn);
    }

    #[test]
    fn rescue_and_features_count_escalating_attacks() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 1, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let id = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.TEST_SUBJECT")
            .unwrap() as Id;
        let Phase::Combat(combat) = &mut game.phase else {
            panic!("combat did not start");
        };
        combat.player.hp = 50;
        combat.player.block = 0;
        combat.enemies.truncate(1);
        let enemy = &mut combat.enemies[0];
        enemy.creature.id = id;
        enemy.creature.hp = 100;
        enemy.creature.max_hp = 100;
        enemy.move_index = 3;
        enemy.move_history.clear();
        let actions = game.actions(&content);
        let end_turn = actions
            .iter()
            .position(|action| matches!(action, Action::EndTurn))
            .unwrap();
        assert_eq!(
            rescue_choice(&game, &content, end_turn, 0.75, true),
            end_turn
        );
        let layout = Layout::new(&content);
        let before = state_features(&game, layout);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[0].move_history = vec![3, 3];
        let after = state_features(&game, layout);
        assert_eq!(before[layout.enemy_history + 3], 0.0);
        assert_eq!(after[layout.enemy_history + 3], 0.2);
        assert_ne!(
            rescue_choice(&game, &content, end_turn, 0.75, true),
            end_turn
        );
    }

    #[test]
    fn run_search_returns_live_partial_combat() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            panic!("combat did not start")
        };
        combat.player.hp = 30_000;
        combat.player.max_hp = 30_000;
        combat.enemies.truncate(1);
        combat.enemies[0].creature.hp = 30_000;
        combat.enemies[0].creature.max_hp = 30_000;
        let (partial, plan) = run_search(&game, &content, 1, 1, 1, (0, 0));
        assert!(partial.combat().is_some());
        assert!(!plan.is_empty());
    }

    #[test]
    fn training_bonuses_replace_strength_and_dexterity_exactly() {
        let mut batch = Batch::new(1, 1, Some(0), 32, 1, 10, 1).unwrap();
        batch.games[0]
            .start_combat(&batch.content, batch.content.acts[0].encounters[0])
            .unwrap();
        let amounts = |batch: &Batch| {
            ["POWER.STRENGTH_POWER", "POWER.DEXTERITY_POWER"].map(|id| {
                batch.games[0]
                    .combat()
                    .unwrap()
                    .player
                    .powers
                    .iter()
                    .find(|power| batch.content.powers[power.id as usize].id == id)
                    .map_or(0, |power| power.amount)
            })
        };
        batch.set_training_bonuses(0, 0);
        assert_eq!(amounts(&batch), [0, 0]);
        batch.set_training_bonus(7);
        assert_eq!(amounts(&batch), [7, 3]);
        batch.set_training_bonuses(7, 4);
        assert_eq!(amounts(&batch), [7, 4]);
        batch.set_training_bonuses(0, 0);
        assert_eq!(amounts(&batch), [0, 0]);
    }

    #[test]
    fn training_bonus_changes_observation_and_transition() {
        let mut plain = Batch::new(1, 48, Some(0), 32, 1, 10, 1).unwrap();
        let mut boosted = Batch::new(1, 48, Some(0), 32, 1, 10, 1).unwrap();
        for batch in [&mut plain, &mut boosted] {
            batch.games[0]
                .start_combat(&batch.content, batch.content.acts[0].encounters[0])
                .unwrap();
            let strike = batch.content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
            let Phase::Combat(combat) = &mut batch.games[0].phase else {
                unreachable!()
            };
            combat.player.powers.clear();
            combat.hand = vec![Card {
                id: strike,
                ..Card::default()
            }];
            combat.enemies.truncate(1);
            combat.hits.truncate(1);
            combat.enemies[0].creature.hp = 100;
            combat.enemies[0].creature.max_hp = 100;
            combat.enemies[0].creature.block = 0;
            combat.enemies[0].creature.powers.clear();
        }
        boosted.set_training_bonus(24);
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        assert_eq!(
            plain.games[0].actions(&plain.content),
            boosted.games[0].actions(&boosted.content)
        );
        assert_ne!(
            global_features_with_bonuses(&plain.games[0], plain.layout, (0, 0)),
            global_features_with_bonuses(&boosted.games[0], boosted.layout, (24, 12))
        );
        assert_ne!(
            state_tokens(&plain.games[0], &plain.content, plain.layout),
            state_tokens(&boosted.games[0], &boosted.content, boosted.layout)
        );
        assert_ne!(
            compact_action_values(&plain.games[0], plain.layout, &action),
            compact_action_values(&boosted.games[0], boosted.layout, &action)
        );
        plain.games[0].step(&plain.content, action.clone()).unwrap();
        boosted.games[0].step(&boosted.content, action).unwrap();
        let hp = |batch: &Batch| batch.games[0].combat().unwrap().enemies[0].creature.hp;
        assert_eq!(hp(&plain) - hp(&boosted), 24);
    }

    #[test]
    fn reset_invalidates_cached_actions() {
        let mut batch = Batch::new(1, 1, Some(0), 32, 1, 10, 1).unwrap();
        batch.actions[0] = batch.games[0].actions(&batch.content);
        assert!(!batch.actions[0].is_empty());
        batch.reset(vec![0], 0.0).unwrap();
        assert!(batch.actions[0].is_empty());
    }

    #[test]
    fn repaired_path_replays_with_aligned_choices() {
        let mut batch = Batch::new(1, 1, Some(0), 32, 1, 10, 1).unwrap();
        batch.set_training_bonuses(7, 4);
        let mut source = batch.games[0].clone();
        let opening = source
            .actions(&batch.content)
            .iter()
            .position(|action| matches!(action, Action::Event(_)))
            .unwrap();
        source
            .step(
                &batch.content,
                source.actions(&batch.content)[opening].clone(),
            )
            .unwrap();
        let first = source
            .actions(&batch.content)
            .iter()
            .position(|action| matches!(action, Action::Path(_)))
            .unwrap();
        let first_action = source.actions(&batch.content)[first].clone();
        source.step(&batch.content, first_action.clone()).unwrap();
        apply_training_bonuses(&mut source, &batch.content, 20, 10);
        let (_, source_plan) = combat_search(&source, &batch.content, 16, 64);
        let mut choices = vec![opening, first];
        for action in source_plan {
            let choice = source
                .actions(&batch.content)
                .iter()
                .position(|legal| *legal == action)
                .unwrap();
            choices.push(choice);
            source.step(&batch.content, action).unwrap();
        }

        let (won, repaired, score) = batch
            .repair_path_at(0, 20, choices, 1, 16, 64, None)
            .unwrap();
        let mut replay = batch.games[0].clone();
        for (position, choice) in repaired.into_iter().enumerate() {
            let action = replay.actions(&batch.content).get(choice).cloned().unwrap();
            if position == 1 {
                assert_eq!(action, first_action);
            }
            let combat = replay.combat().is_some();
            replay.step(&batch.content, action).unwrap();
            if !combat && replay.combat().is_some() {
                apply_training_bonuses(
                    &mut replay,
                    &batch.content,
                    batch.training_strength,
                    batch.training_dexterity,
                );
            }
        }
        assert_eq!(won, matches!(replay.phase, Phase::Won));
        assert_eq!(score, teacher_state_score(&replay, &batch.content));
    }

    #[test]
    fn combat_key_keeps_boss_history() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.room = Room::Boss;
        let boss = combat_key(&game, &content);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.history.manual_plays = 1;
        assert_ne!(boss, combat_key(&game, &content));
        game.room = Room::Combat;
        let room = combat_key(&game, &content);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.history.manual_plays = 0;
        assert_eq!(room, combat_key(&game, &content));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_cover_every_character_and_legal_action() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        for character in 0..content.characters.len() as Id {
            let mut game = Game::new_character_ascension(&content, 7, character, 10).unwrap();
            game.begin_act(&content, 0).unwrap();
            let state = state_features(&game, layout);
            assert_eq!(state.len(), layout.state_len);
            assert!(state.iter().all(|value| value.is_finite()));
            for action in game.actions(&content) {
                let features = action_features(&game, layout, &action);
                assert_eq!(features.len(), layout.action_len);
                assert!(features.iter().all(|value| value.is_finite()));
                let mut next = game.clone();
                next.step(&content, action).unwrap();
            }
        }
    }

    #[test]
    fn v19_card_tokens_are_per_occurrence_raw_and_unordered() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 7, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let strike = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        let dazed = content.card_id("CARD.DAZED").unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![
            Card {
                id: strike,
                instance: 10,
                upgrades: 1,
                cost_delta: -2,
                flags: 5,
                turn_flags: 6,
                value: 7,
                replays: 2,
                free: true,
                cost_override: Some(-1),
                enchantment: Some(Enchantment::Sharp),
                enchantment_amount: 4,
                enchantment_value: 8,
                variant: 3,
            },
            Card {
                id: strike,
                instance: 11,
                value: 7,
                ..Card::default()
            },
            Card {
                id: dazed,
                instance: 12,
                ..Card::default()
            },
        ];
        combat.draw = vec![
            Card {
                id: strike,
                instance: 13,
                ..Card::default()
            },
            Card {
                id: dazed,
                instance: 14,
                ..Card::default()
            },
        ];
        combat.known_draw_bottom = 1;
        combat.known_draw_top = 1;
        let mut first = Vec::new();
        state_card_tokens(&game, &content, &mut first);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand.reverse();
        let mut second = Vec::new();
        state_card_tokens(&game, &content, &mut second);
        assert_eq!(first, second);
        let hand = first
            .iter()
            .filter(|row| row[0] == CARD_COLLECTION as f32 && row[1] == 2.0)
            .collect::<Vec<_>>();
        assert_eq!(hand.len(), 3);
        assert_eq!(
            hand.iter()
                .filter(|row| row[2] == strike as f32 + 1.0)
                .count(),
            2
        );
        let enchanted = hand
            .iter()
            .find(|row| row[6] == Enchantment::Sharp as usize as f32 + 1.0)
            .unwrap();
        let card = game.combat().unwrap().hand[2];
        let def = content.cards[strike as usize];
        assert_eq!(
            &enchanted[8..21],
            &[
                1.0,
                4.0,
                8.0,
                3.0,
                card.flags as f32,
                6.0,
                7.0,
                2.0,
                1.0,
                def.cost[1] as f32,
                def.star_cost[1] as f32,
                -2.0,
                128.0,
            ]
        );
        let status = hand
            .iter()
            .find(|row| row[2] == dazed as f32 + 1.0)
            .unwrap();
        assert_eq!(status[5], CardType::Status as usize as f32 + 1.0);
        assert_eq!(status[7], 1.0);
        let mut positions = first
            .iter()
            .filter(|row| row[0] == CARD_COLLECTION as f32 && row[1] == 3.0)
            .map(|row| row[4] as usize)
            .collect::<Vec<_>>();
        positions.sort_unstable();
        assert_eq!(positions, [1, KNOWN_DRAW_SLOTS + 1]);
    }

    #[test]
    fn v19_card_tokens_preview_cost_block_and_target_damage() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            instance: 20,
            ..Card::default()
        };
        let defend = Card {
            id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
            instance: 21,
            ..Card::default()
        };
        let whirlwind = Card {
            id: content.card_id("CARD.WHIRLWIND").unwrap(),
            instance: 22,
            ..Card::default()
        };
        let burning_pact = Card {
            id: content.card_id("CARD.BURNING_PACT").unwrap(),
            instance: 23,
            ..Card::default()
        };
        let survivor = Card {
            id: content.card_id("CARD.SURVIVOR").unwrap(),
            instance: 24,
            ..Card::default()
        };
        let mad_science = Card {
            id: content.card_id("CARD.MAD_SCIENCE").unwrap(),
            instance: 25,
            variant: 10,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![
            strike,
            defend,
            whirlwind,
            burning_pact,
            survivor,
            mad_science,
        ];
        combat.energy = 3;
        combat.player.powers.extend([
            Power {
                id: power_id::STRENGTH,
                amount: 2,
                skip_duration: false,
                value: 0,
            },
            Power {
                id: power_id::WEAK,
                amount: 1,
                skip_duration: false,
                value: 0,
            },
            Power {
                id: power_id::DEXTERITY,
                amount: 3,
                skip_duration: false,
                value: 0,
            },
            Power {
                id: power_id::FRAIL,
                amount: 1,
                skip_duration: false,
                value: 0,
            },
        ]);
        combat.enemies[0].creature.hp = 20;
        combat.enemies[0].creature.max_hp = 20;
        combat.enemies[0].creature.block = 3;
        combat.enemies[0].creature.powers.extend([
            Power {
                id: power_id::VULNERABLE,
                amount: 1,
                skip_duration: false,
                value: 0,
            },
            Power {
                id: power_id::HARD_TO_KILL,
                amount: 7,
                skip_duration: false,
                value: 0,
            },
        ]);

        let mut cards = Vec::new();
        state_card_tokens(&game, &content, &mut cards);
        let hand = |id| {
            cards
                .iter()
                .find(|row| {
                    row[0] == CARD_COLLECTION as f32 && row[1] == 2.0 && row[2] == id as f32 + 1.0
                })
                .unwrap()
        };
        assert_eq!(hand(strike.id)[24], 1.0);
        assert_eq!(hand(strike.id)[27], 6.0);
        assert_eq!(hand(defend.id)[26], 6.0);
        assert_eq!(hand(whirlwind.id)[24], 3.0);
        assert_eq!(hand(whirlwind.id)[28], 15.0);
        assert_eq!(hand(burning_pact.id)[29], 2.0);
        assert_eq!(hand(burning_pact.id)[31], 1.0);
        assert_eq!(hand(survivor.id)[30], 1.0);
        assert_eq!(
            hand(mad_science.id)[5],
            CardType::Attack as usize as f32 + 1.0
        );
        assert_eq!(hand(mad_science.id)[26], 0.0);
        assert_eq!(hand(mad_science.id)[27], 10.0);

        let mut values = compact_action_values(
            &game,
            Layout::new(&content),
            &Action::Play {
                hand: 0,
                target: Some(0),
            },
        );
        let mut tokens = Vec::new();
        action_card_tokens(
            &game,
            &content,
            &Action::Play {
                hand: 0,
                target: Some(0),
            },
            &mut values,
            &mut tokens,
        );
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0][0], ACTION_CARD_COLLECTION as f32);
        assert_eq!(&tokens[0][1..], &hand(strike.id)[1..]);
        assert_eq!(&values[48..], &[1.0, 3.0, 7.0, 7.0, 4.0, 0.0, 0.0, 20.0]);

        let fire = content.potion_id("POTION.FIRE_POTION").unwrap();
        game.run.potions[0] = Some(fire);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[0]
            .creature
            .powers
            .retain(|power| power.id != power_id::HARD_TO_KILL);
        let action = Action::Potion {
            slot: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        action_card_tokens(&game, &content, &action, &mut values, &mut Vec::new());
        assert_eq!(&values[48..], &[1.0, 3.0, 0.0, 20.0, 17.0, 0.0, 0.0, 20.0]);
    }

    #[test]
    fn v27_action_previews_use_pre_effect_play_state() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 46, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let cards = [
            Card {
                id: content.card_id("CARD.GENETIC_ALGORITHM").unwrap(),
                value: 9,
                ..Card::default()
            },
            Card {
                id: content.card_id("CARD.WHIRLWIND").unwrap(),
                ..Card::default()
            },
            Card {
                id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
                replays: 2,
                ..Card::default()
            },
            Card {
                id: content.card_id("CARD.FINISHER").unwrap(),
                ..Card::default()
            },
        ];
        game.run
            .relics
            .push(content.relic_id("RELIC.CHEMICAL_X").unwrap());
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = cards.to_vec();
        combat.energy = 3;
        combat.history.attacks = 2;
        combat.player.powers.clear();
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        let preview = |hand| {
            let action = Action::Play {
                hand,
                target: Some(0),
            };
            let mut values = compact_action_values(&game, Layout::new(&content), &action);
            let mut tokens = Vec::new();
            action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
            tokens[0]
        };
        assert_eq!(preview(0)[26], 10.0);
        assert_eq!(preview(1)[28], 25.0);
        assert_eq!(preview(2)[27], 18.0);
        assert_eq!(preview(3)[27], 12.0);
    }

    #[test]
    fn v27_card_tokens_keep_raw_flags() {
        let content = foundation_content();
        let game = Game::new_character_ascension(&content, 46, 0, 10).unwrap();
        let id = content.card_id("CARD.DAZED").unwrap();
        let def = content.cards[id as usize];
        let plain = Card {
            id,
            ..Card::default()
        };
        let explicit = Card {
            flags: def.flags[0],
            ..plain
        };
        assert_eq!(plain.flags(def), explicit.flags(def));
        let row = |card| {
            card_token_v19(
                &game,
                &content,
                CARD_COLLECTION,
                0,
                0,
                card,
                (0, 0, 0),
                None,
                None,
            )
        };
        assert_ne!(row(plain)[12], row(explicit)[12]);
    }

    #[test]
    fn v27_tokens_hide_instances_and_keep_full_pending_cards() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 47, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let base = state_tokens(&game, &content, layout);
        let mut changed = game.clone();
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.enemies[1].instance = combat.enemies[1].instance.wrapping_add(101);
        assert_eq!(base, state_tokens(&changed, &content, layout));

        let wriggler = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.WRIGGLER")
            .unwrap() as Id;
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[0].creature.id = wriggler;
        combat.enemies[0].instance = 2;
        let without_next = |tokens: Vec<Token>| {
            tokens
                .into_iter()
                .filter(|row| row[..2] != [ENEMY_COLLECTION as f32, 2.0])
                .collect::<Vec<_>>()
        };
        let even = without_next(state_tokens(&game, &content, layout));
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[0].instance = 4;
        assert_eq!(even, without_next(state_tokens(&game, &content, layout)));
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[0].instance = 3;
        assert_ne!(even, without_next(state_tokens(&game, &content, layout)));

        let mut card = Card {
            id: content.card_id("CARD.GENETIC_ALGORITHM").unwrap(),
            instance: 100,
            upgrades: 1,
            flags: RETAIN,
            turn_flags: FETCHED,
            value: 17,
            replays: 2,
            enchantment: Some(Enchantment::Sharp),
            enchantment_amount: 3,
            enchantment_value: 4,
            variant: 5,
            ..Card::default()
        };
        let mut dampened = game.clone();
        let Phase::Combat(combat) = &mut dampened.phase else {
            unreachable!()
        };
        card.upgrades = 0;
        combat.hand = vec![card];
        combat.dampened = vec![(card.instance, 2)];
        let first = state_tokens(&dampened, &content, layout);
        let mut renumbered = dampened;
        let Phase::Combat(combat) = &mut renumbered.phase else {
            unreachable!()
        };
        combat.hand[0].instance = 200;
        combat.dampened[0].0 = 200;
        assert_eq!(first, state_tokens(&renumbered, &content, layout));

        card.upgrades = 1;
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.nightmares.push((card, 3));
        combat.queue.push(Pending {
            effect: Effect::AutoPlay(card),
            context: Context::card(card),
        });
        game.resume = Some(Phase::Shop(vec![ShopItem::Card(card, 73)]));
        let tokens = state_tokens(&game, &content, layout);
        for (collection, kind) in [
            (DELAYED_COLLECTION, NIGHTMARE_CARD_KIND),
            (CONTINUATION_COLLECTION, QUEUED_CARD_KIND),
            (CONTINUATION_COLLECTION, RESUME_CARD_KIND),
        ] {
            let row = tokens
                .iter()
                .find(|row| row[0] == collection as f32 && row[1] == kind as f32)
                .unwrap();
            assert_eq!(row[2], card.id as f32 + 1.0);
            assert_eq!(row[4], 0.0);
            assert_eq!(
                &row[8..16],
                &[1.0, 3.0, 4.0, 5.0, RETAIN as f32, FETCHED as f32, 17.0, 2.0]
            );
        }
    }

    #[test]
    fn v28_training_bonus_is_public_and_used_by_previews() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 48, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let globals = global_features_with_bonuses(&game, layout, (24, 12));
        assert_eq!(&globals[globals.len() - 2..], &[0.75, 0.375]);
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers.clear();
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let defend = Card {
            id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        for (kind, amount) in [(PowerKind::Strength, 24), (PowerKind::Dexterity, 12)] {
            let id = content
                .powers
                .iter()
                .position(|power| power.kind == kind)
                .unwrap() as Id;
            combat.player.add_power(id, amount);
        }
        combat.hand = vec![strike, defend];
        let mut cards = Vec::new();
        state_card_tokens(&game, &content, &mut cards);
        let strike = cards
            .iter()
            .find(|row| row[0] == CARD_COLLECTION as f32 && row[1] == 2.0)
            .unwrap();
        assert_eq!(strike[27], 30.0);
        let defend = cards
            .iter()
            .find(|row| row[0] == CARD_COLLECTION as f32 && row[2] == defend.id as f32 + 1.0)
            .unwrap();
        assert_eq!(defend[26], 17.0);
    }

    #[test]
    fn v28_damage_preview_matches_public_modifier_chain() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 49, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            enchantment: Some(Enchantment::Sharp),
            enchantment_amount: 4,
            ..Card::default()
        };
        game.run.relics.extend(
            ["RELIC.STRIKE_DUMMY", "RELIC.PEN_NIB"].map(|id| content.relic_id(id).unwrap()),
        );
        game.pen_nib = 9;
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![card];
        combat.player.powers = [
            (power_id::VIGOR, 2),
            (power_id::STRENGTH, 1),
            (power_id::WEAK, 1),
        ]
        .into_iter()
        .map(|(id, amount)| Power {
            id,
            amount,
            skip_duration: false,
            value: 0,
        })
        .collect();
        combat.enemies[0].creature.hp = 1000;
        combat.enemies[0].creature.max_hp = 1000;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers = vec![Power {
            id: power_id::VULNERABLE,
            amount: 1,
            skip_duration: false,
            value: 0,
        }];
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        assert_eq!(tokens[0][27], 24.0);
        assert_eq!(&values[51..53], &[36.0, 36.0]);
        let before = game.combat().unwrap().enemies[0].creature.hp;
        game.step(&content, action).unwrap();
        assert_eq!(before - game.combat().unwrap().enemies[0].creature.hp, 36);
    }

    #[test]
    fn v28_preview_covers_deterministic_pile_effects() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 50, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let entrench = Card {
            id: content.card_id("CARD.ENTRENCH").unwrap(),
            ..Card::default()
        };
        let reboot = Card {
            id: content.card_id("CARD.REBOOT").unwrap(),
            ..Card::default()
        };
        let flak = Card {
            id: content.card_id("CARD.FLAK_CANNON").unwrap(),
            ..Card::default()
        };
        let status = Card {
            id: content.card_id("CARD.DAZED").unwrap(),
            ..Card::default()
        };
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let row = |game: &Game, target| {
            let action = Action::Play { hand: 0, target };
            let mut values = compact_action_values(game, Layout::new(&content), &action);
            let mut tokens = Vec::new();
            action_card_tokens(game, &content, &action, &mut values, &mut tokens);
            (tokens, values)
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers.clear();
        combat.player.block = 7;
        combat.hand = vec![entrench];
        assert_eq!(row(&game, None).0[0][26], 7.0);

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![reboot, strike, strike, strike];
        combat.draw = vec![strike];
        combat.discard = vec![strike, strike];
        assert_eq!(row(&game, None).0[0][29], 4.0);

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![flak, status];
        combat.draw = vec![status];
        combat.discard = vec![status];
        let (tokens, _) = row(&game, None);
        let card = tokens
            .iter()
            .find(|token| token[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        assert_eq!((card[27], card[31]), (24.0, 3.0));
        assert!(
            tokens
                .iter()
                .any(|token| token[0] == ENEMY_COLLECTION as f32 && token[1] == 3.0)
        );

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![strike; 10];
        combat.draw = vec![strike; 8];
        combat.discard.clear();
        {
            let preview = |effect| {
                let mut preview = CardPreview::new(&game, &content);
                preview_effects(
                    &game,
                    &content,
                    Context::player(),
                    &[effect],
                    1,
                    &mut preview,
                );
                preview.draw
            };
            assert_eq!(preview(Effect::Draw(2)), 0);
            assert_eq!(preview(Effect::ShuffleHandDraw(5)), 5);
            assert_eq!(preview(Effect::AutoPlayDraw(Amount::fixed(3, 3), false)), 3);
        }
        let no_draw = content
            .powers
            .iter()
            .position(|power| power.kind == PowerKind::NoDraw)
            .unwrap() as Id;
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand.clear();
        combat.player.powers.push(Power {
            id: no_draw,
            amount: 1,
            skip_duration: false,
            value: 0,
        });
        let mut preview = CardPreview::new(&game, &content);
        preview_effects(
            &game,
            &content,
            Context::player(),
            &[Effect::Draw(2)],
            1,
            &mut preview,
        );
        assert_eq!(preview.draw, 0);
    }

    #[test]
    fn v28_played_star_cost_includes_sealed_throne() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 51, 2, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: content.card_id("CARD.STARDUST").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![card];
        combat.stars = 3;
        combat.player.powers = vec![Power {
            id: power_id::THE_SEALED_THRONE,
            amount: 2,
            skip_duration: false,
            value: 0,
        }];
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        assert_eq!(
            tokens
                .iter()
                .find(|token| token[0] == ACTION_CARD_COLLECTION as f32)
                .unwrap()[25],
            5.0
        );
    }

    #[test]
    fn v29_fan_of_knives_shiv_preview_matches_play() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 52, 1, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let shiv = Card {
            id: content.card_id("CARD.SHIV").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        let mut second = combat.enemies[0].clone();
        second.instance = second.instance.saturating_add(100);
        combat.enemies.truncate(1);
        combat.enemies.push(second);
        combat.hits = vec![0; 2];
        for enemy in &mut combat.enemies {
            enemy.creature.hp = 40;
            enemy.creature.max_hp = 40;
            enemy.creature.block = 0;
            enemy.creature.powers.clear();
        }
        combat.hand = vec![shiv];
        combat.player.powers = vec![Power {
            id: power_id::FAN_OF_KNIVES,
            amount: 1,
            skip_duration: false,
            value: 0,
        }];
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        let card = tokens
            .iter()
            .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        assert_eq!((card[27], card[28]), (0.0, 4.0));

        let before = game
            .combat()
            .unwrap()
            .enemies
            .iter()
            .map(|enemy| enemy.creature.hp)
            .collect::<Vec<_>>();
        let mut played = game;
        played.step(&content, action).unwrap();
        let after = &played.combat().unwrap().enemies;
        let losses = before
            .iter()
            .zip(after)
            .map(|(before, enemy)| before - enemy.creature.hp)
            .collect::<Vec<_>>();
        assert_eq!(losses, [4, 4]);
        assert_eq!(values[53], losses.into_iter().sum::<i16>() as f32);
    }

    #[test]
    fn v30_seeking_edge_sovereign_blade_preview_matches_play() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 53, 2, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: card_id::SOVEREIGN_BLADE,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        let mut second = combat.enemies[0].clone();
        second.instance = second.instance.saturating_add(100);
        combat.enemies.truncate(1);
        combat.enemies.push(second);
        combat.hits = vec![0; 2];
        for enemy in &mut combat.enemies {
            enemy.creature.hp = 40;
            enemy.creature.max_hp = 40;
            enemy.creature.block = 0;
            enemy.creature.powers.clear();
        }
        combat.hand = vec![card];
        combat.energy = 3;
        combat.player.powers = vec![Power {
            id: power_id::SEEKING_EDGE,
            amount: 1,
            skip_duration: false,
            value: 0,
        }];
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        let card = tokens
            .iter()
            .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        assert_eq!(card[27], 0.0);

        let before = game
            .combat()
            .unwrap()
            .enemies
            .iter()
            .map(|enemy| enemy.creature.hp)
            .collect::<Vec<_>>();
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let losses = before
            .iter()
            .zip(&played.combat().unwrap().enemies)
            .map(|(before, enemy)| before - enemy.creature.hp)
            .collect::<Vec<_>>();
        assert_eq!(losses[0], losses[1]);
        assert_eq!(card[28], losses[0] as f32);
        assert_eq!(values[53], losses.into_iter().sum::<i16>() as f32);
    }

    #[test]
    fn v30_regent_star_power_previews_match_play() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 54, 2, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: content.card_id("CARD.ALIGNMENT").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        let mut second = combat.enemies[0].clone();
        second.instance = second.instance.saturating_add(100);
        combat.enemies.truncate(1);
        combat.enemies.push(second);
        combat.hits = vec![0; 2];
        for enemy in &mut combat.enemies {
            enemy.creature.hp = 40;
            enemy.creature.max_hp = 40;
            enemy.creature.block = 0;
            enemy.creature.powers.clear();
        }
        combat.hand = vec![card];
        combat.stars = 3;
        combat.player.block = 0;
        combat.player.powers = [
            (power_id::THE_SEALED_THRONE, 1),
            (power_id::CHILD_OF_THE_STARS, 2),
            (power_id::BLACK_HOLE, 5),
        ]
        .into_iter()
        .map(|(id, amount)| Power {
            id,
            amount,
            skip_duration: false,
            value: 0,
        })
        .collect();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        let card = tokens
            .iter()
            .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        assert_eq!((card[25], card[26], card[28]), (3.0, 6.0, 10.0));

        let before = game
            .combat()
            .unwrap()
            .enemies
            .iter()
            .map(|enemy| enemy.creature.hp)
            .collect::<Vec<_>>();
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let combat = played.combat().unwrap();
        let losses = before
            .iter()
            .zip(&combat.enemies)
            .map(|(before, enemy)| before - enemy.creature.hp)
            .collect::<Vec<_>>();
        assert_eq!(combat.player.block, 6);
        assert_eq!(losses, [10, 10]);
        assert_eq!(values[53], 20.0);
    }

    #[test]
    fn v31_end_of_days_preview_matches_doom_kills() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 55, 4, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: content.card_id("CARD.END_OF_DAYS").unwrap(),
            ..Card::default()
        };
        let slime = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.LEAF_SLIME_M")
            .unwrap() as Id;
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        let enemy = combat.enemies[0].clone();
        combat.enemies = vec![enemy; 3];
        combat.hits = vec![0; 3];
        for (index, enemy) in combat.enemies.iter_mut().enumerate() {
            enemy.instance = index as u32 + 1;
            enemy.creature.id = slime;
            enemy.creature.hp = [20, 35, 100][index];
            enemy.creature.max_hp = enemy.creature.hp;
            enemy.creature.block = 0;
            enemy.creature.powers.clear();
        }
        combat.enemies[1].creature.powers.push(Power {
            id: power_id::DOOM,
            amount: 10,
            skip_duration: false,
            value: 0,
        });
        combat.hand = vec![card];
        combat.energy = 3;
        combat.player.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        action_card_tokens(&game, &content, &action, &mut values, &mut Vec::new());

        let before = game
            .combat()
            .unwrap()
            .enemies
            .iter()
            .map(|enemy| enemy.creature.hp)
            .collect::<Vec<_>>();
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let losses = before
            .iter()
            .zip(&played.combat().unwrap().enemies)
            .map(|(before, enemy)| before - enemy.creature.hp)
            .collect::<Vec<_>>();
        assert_eq!(losses, [20, 35, 0]);
        assert_eq!(values[53], losses.into_iter().sum::<i16>() as f32);
    }

    #[test]
    fn v31_knockout_blow_preview_projects_lethal_stars() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 56, 3, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: content.card_id("CARD.KNOCKOUT_BLOW").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        let mut second = combat.enemies[0].clone();
        second.instance = second.instance.saturating_add(100);
        combat.enemies.truncate(1);
        combat.enemies.push(second);
        combat.hits = vec![0; 2];
        for (index, enemy) in combat.enemies.iter_mut().enumerate() {
            enemy.creature.hp = [20, 40][index];
            enemy.creature.max_hp = enemy.creature.hp;
            enemy.creature.block = 0;
            enemy.creature.powers.clear();
        }
        combat.hand = vec![card];
        combat.energy = 3;
        combat.stars = 0;
        combat.player.powers = vec![Power {
            id: power_id::BLACK_HOLE,
            amount: 3,
            skip_duration: false,
            value: 0,
        }];
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        action_card_tokens(&game, &content, &action, &mut values, &mut Vec::new());
        assert_eq!(&values[52..55], &[20.0, 3.0, 1.0]);

        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let combat = played.combat().unwrap();
        assert_eq!(combat.enemies[0].creature.hp, 0);
        assert_eq!(combat.enemies[1].creature.hp, 37);
        assert_eq!(combat.stars, 5);
    }

    #[test]
    fn v31_shrink_and_enchantment_previews_match_play() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 57, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            enchantment: Some(Enchantment::Adroit),
            enchantment_amount: 4,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![strike];
        combat.energy = 3;
        combat.player.block = 0;
        combat.player.powers = [
            (power_id::SHRINK, -1),
            (power_id::DEXTERITY, 3),
            (power_id::FRAIL, 1),
        ]
        .into_iter()
        .map(|(id, amount)| Power {
            id,
            amount,
            skip_duration: false,
            value: 0,
        })
        .collect();
        combat.enemies[0].creature.hp = 40;
        combat.enemies[0].creature.max_hp = 40;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        let card = tokens
            .iter()
            .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        assert_eq!((card[26], values[52]), (5.0, 4.0));
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let combat = played.combat().unwrap();
        assert_eq!(combat.player.block, 5);
        assert_eq!(combat.enemies[0].creature.hp, 36);

        let defend = Card {
            id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
            enchantment: Some(Enchantment::Swift),
            enchantment_amount: 2,
            enchantment_value: 2,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![defend];
        combat.draw = vec![strike; 3];
        combat.player.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        assert_eq!(
            tokens
                .iter()
                .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
                .unwrap()[29],
            2.0
        );
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        assert_eq!(played.combat().unwrap().hand.len(), 2);
    }

    #[test]
    fn v31_seeker_strike_preview_counts_the_selected_draw() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 58, 4, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let seeker = Card {
            id: content.card_id("CARD.SEEKER_STRIKE").unwrap(),
            ..Card::default()
        };
        let filler = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![seeker];
        combat.draw = vec![filler; 3];
        combat.energy = 3;
        combat.player.powers.clear();
        combat.enemies[0].creature.hp = 40;
        combat.enemies[0].creature.max_hp = 40;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        assert_eq!(
            tokens
                .iter()
                .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
                .unwrap()[29],
            1.0
        );

        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let choice = played.actions(&content).into_iter().next().unwrap();
        played.step(&content, choice).unwrap();
        assert_eq!(played.combat().unwrap().hand.len(), 1);

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![filler; 8];
        combat.hand.push(Card {
            enchantment: Some(Enchantment::Swift),
            enchantment_amount: 2,
            enchantment_value: 2,
            ..seeker
        });
        combat.draw = vec![filler; 3];
        combat.discard.clear();
        let action = Action::Play {
            hand: 8,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        let card = tokens
            .iter()
            .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        assert_eq!((card[29], card[30]), (2.0, 1.0));
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        assert_eq!(played.combat().unwrap().hand.len(), 10);
        assert_eq!(played.combat().unwrap().discard.len(), 2);
    }

    #[test]
    fn v33_on_play_relic_and_power_previews_match_play() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 59, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics = [
            "RELIC.INTIMIDATING_HELMET",
            "RELIC.LETTER_OPENER",
            "RELIC.IRON_CLUB",
            "RELIC.TUNING_FORK",
        ]
        .map(|id| content.relic_id(id).unwrap())
        .to_vec();
        game.iron_club = 1;
        game.tuning_fork = 7;
        let card = Card {
            id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
            flags: ETHEREAL,
            replays: 2,
            cost_override: Some(2),
            ..Card::default()
        };
        let filler = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.enemies[0].creature.max_hp = 100;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        combat.hand = vec![card];
        combat.draw = vec![filler];
        combat.discard.clear();
        combat.energy = 3;
        combat.player.block = 0;
        combat.player.powers = [(power_id::DANSE_MACABRE, 2), (power_id::SPIRIT_OF_ASH, 3)]
            .map(|(id, amount)| Power {
                id,
                amount,
                skip_duration: false,
                value: 0,
            })
            .to_vec();
        combat.history.attacks = 0;
        combat.history.skills = 0;
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        let card = tokens
            .iter()
            .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        assert_eq!((card[26], card[28], card[29]), (39.0, 5.0, 1.0));
        assert!(tokens.iter().any(|row| {
            row[..2] == [ENEMY_COLLECTION as f32, 4.0] && (row[11], row[12]) == (5.0, 5.0)
        }));

        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let combat = played.combat().unwrap();
        assert_eq!(combat.player.block, 39);
        assert_eq!(combat.enemies[0].creature.hp, 95);
        assert_eq!(combat.hand.len(), 1);
    }

    #[test]
    fn v33_attack_counter_previews_match_play() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 60, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics = ["RELIC.ORNAMENTAL_FAN", "RELIC.KUSARIGAMA"]
            .map(|id| content.relic_id(id).unwrap())
            .to_vec();
        let card = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            replays: 2,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.enemies[0].creature.max_hp = 100;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        combat.hand = vec![card];
        combat.energy = 3;
        combat.player.block = 0;
        combat.player.powers.clear();
        combat.history.attacks = 0;
        combat.kusarigama = 0;
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        let card = tokens
            .iter()
            .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        let random = tokens
            .iter()
            .find(|row| row[..2] == [ENEMY_COLLECTION as f32, 3.0])
            .unwrap();
        assert_eq!(card[26], 4.0);
        assert_eq!((random[11], random[12], random[16]), (6.0, 6.0, 1.0));

        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let combat = played.combat().unwrap();
        assert_eq!(combat.player.block, 4);
        assert_eq!(combat.enemies[0].creature.hp, 76);
    }

    #[test]
    fn v33_hand_moves_are_counted_as_entering_hand() {
        let mut content = foundation_content();
        let all_for_one = content.card_id("CARD.ALL_FOR_ONE").unwrap();
        let hologram = content.card_id("CARD.HOLOGRAM").unwrap();
        let custom = content.card_id("CARD.ZAP").unwrap();
        content.cards[custom as usize].effects = Box::leak(Box::new([Effect::SelectAmount(
            Pile::Discard,
            CardFilter::Any,
            Amount::fixed(2, 2),
            false,
            CardOp::Move(Pile::Hand),
        )]));
        let claw = Card {
            id: content.card_id("CARD.CLAW").unwrap(),
            ..Card::default()
        };
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        for (seed, card, discard, expected) in [
            (61, all_for_one, vec![claw, claw, strike], 2),
            (62, hologram, vec![strike], 1),
            (63, custom, vec![strike, strike], 2),
        ] {
            let mut game = Game::new_character_ascension(&content, seed, 3, 10).unwrap();
            game.begin_act(&content, 0).unwrap();
            game.start_combat(&content, content.acts[0].encounters[0])
                .unwrap();
            let Phase::Combat(combat) = &mut game.phase else {
                unreachable!()
            };
            combat.hand = vec![Card {
                id: card,
                ..Card::default()
            }];
            combat.draw.clear();
            combat.discard = discard;
            combat.energy = 3;
            combat.player.powers.clear();
            combat.enemies[0].creature.hp = 100;
            combat.enemies[0].creature.max_hp = 100;
            combat.enemies[0].creature.powers.clear();
            let played_card = combat.hand[0];
            let target = (card == all_for_one).then_some(0);
            let action = Action::Play { hand: 0, target };
            let preview = played_card_preview(&game, &content, played_card, target);
            assert_eq!(preview.draw, expected);
            let mut played = game.clone();
            played.step(&content, action).unwrap();
            assert_eq!(played.combat().unwrap().hand.len(), expected as usize);
        }
    }

    #[test]
    fn v33_targetless_aoe_rows_match_each_enemy() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 64, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: content.card_id("CARD.WHIRLWIND").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        let second = combat.enemies[0].clone();
        combat.enemies = vec![second.clone(), second];
        combat.hits = vec![0; 2];
        for (index, enemy) in combat.enemies.iter_mut().enumerate() {
            enemy.instance = index as u32 + 1;
            enemy.creature.hp = [20, 3][index];
            enemy.creature.max_hp = enemy.creature.hp;
            enemy.creature.block = [2, 0][index];
            enemy.creature.powers.clear();
        }
        combat.enemies[0].creature.powers.push(Power {
            id: power_id::VULNERABLE,
            amount: 1,
            skip_duration: false,
            value: 0,
        });
        combat.enemies[1].creature.powers.push(Power {
            id: power_id::HARD_TO_KILL,
            amount: 3,
            skip_duration: false,
            value: 0,
        });
        combat.hand = vec![card];
        combat.energy = 1;
        combat.player.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        let rows = tokens
            .iter()
            .filter(|row| row[..2] == [ENEMY_COLLECTION as f32, 4.0])
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        let first = rows.iter().find(|row| row[4] == 1.0).unwrap();
        let second = rows.iter().find(|row| row[4] == 2.0).unwrap();
        assert_eq!(
            &first[8..17],
            &[1.0, 2.0, 0.0, 7.0, 5.0, 0.0, 0.0, 20.0, 1.0]
        );
        assert_eq!(
            &second[8..17],
            &[0.0, 0.0, 3.0, 3.0, 3.0, 0.0, 1.0, 3.0, 1.0]
        );
        assert_eq!(values[53], 8.0);

        let before = [20, 3];
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        let losses = played
            .combat()
            .unwrap()
            .enemies
            .iter()
            .enumerate()
            .map(|(index, enemy)| before[index] - enemy.creature.hp)
            .collect::<Vec<_>>();
        assert_eq!(losses, [5, 3]);
    }

    #[test]
    fn v22_target_preview_matches_weak_vulnerable_order() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 81, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![strike];
        combat.player.powers = vec![
            Power {
                id: power_id::STRENGTH,
                amount: -1,
                skip_duration: false,
                value: 0,
            },
            Power {
                id: power_id::WEAK,
                amount: 1,
                skip_duration: false,
                value: 0,
            },
        ];
        combat.enemies[0].creature.hp = 5;
        combat.enemies[0].creature.max_hp = 5;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers = vec![Power {
            id: power_id::VULNERABLE,
            amount: 1,
            skip_duration: false,
            value: 0,
        }];
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        action_card_tokens(&game, &content, &action, &mut values, &mut Vec::new());
        assert_eq!(&values[51..55], &[5.0, 5.0, 0.0, 1.0]);
    }

    #[test]
    fn v22_actual_damage_resolves_block_buffer_and_shell_per_hit() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 82, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let twin = Card {
            id: content.card_id("CARD.TWIN_STRIKE").unwrap(),
            ..Card::default()
        };
        let buffer = content
            .powers
            .iter()
            .position(|power| power.id == "POWER.BUFFER_POWER")
            .unwrap() as Id;
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![twin];
        combat.player.powers.clear();
        combat.enemies[0].creature.hp = 20;
        combat.enemies[0].creature.max_hp = 20;
        combat.enemies[0].creature.block = 3;
        combat.enemies[0].creature.powers = vec![
            Power {
                id: buffer,
                amount: 1,
                skip_duration: false,
                value: 0,
            },
            Power {
                id: power_id::HARDENED_SHELL,
                amount: 5,
                skip_duration: false,
                value: 0,
            },
        ];
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        action_card_tokens(&game, &content, &action, &mut values, &mut Vec::new());
        assert_eq!(&values[51..55], &[10.0, 5.0, 0.0, 0.0]);

        let whirlwind = Card {
            id: content.card_id("CARD.WHIRLWIND").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        let mut second = combat.enemies[0].clone();
        second.creature.block = 0;
        second.creature.powers = vec![Power {
            id: power_id::HARDENED_SHELL,
            amount: 3,
            skip_duration: false,
            value: 0,
        }];
        combat.enemies.push(second);
        combat.hits.push(0);
        combat.hand = vec![whirlwind];
        combat.energy = 1;
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        action_card_tokens(&game, &content, &action, &mut values, &mut Vec::new());
        assert_eq!(values[53], 3.0);
    }

    #[test]
    fn v25_damage_preview_keeps_public_modifier_order_and_consumption() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 84, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let twin = Card {
            id: content.card_id("CARD.TWIN_STRIKE").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![strike, twin];
        combat.player.powers = vec![Power {
            id: power_id::DOUBLE_DAMAGE,
            amount: 1,
            skip_duration: false,
            value: 0,
        }];
        combat.enemies[0].creature.hp = 50;
        combat.enemies[0].creature.max_hp = 50;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers = vec![
            Power {
                id: power_id::SLOW,
                amount: 50,
                skip_duration: false,
                value: 0,
            },
            Power {
                id: power_id::FLUTTER,
                amount: 1,
                skip_duration: false,
                value: 0,
            },
        ];
        let mut values = compact_action_values(
            &game,
            Layout::new(&content),
            &Action::Play {
                hand: 0,
                target: Some(0),
            },
        );
        action_card_tokens(
            &game,
            &content,
            &Action::Play {
                hand: 0,
                target: Some(0),
            },
            &mut values,
            &mut Vec::new(),
        );
        assert_eq!(&values[51..53], &[9.0, 9.0]);

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers.clear();
        combat.enemies[0].creature.powers = vec![Power {
            id: power_id::SLIPPERY,
            amount: 1,
            skip_duration: false,
            value: 0,
        }];
        let mut values = compact_action_values(
            &game,
            Layout::new(&content),
            &Action::Play {
                hand: 1,
                target: Some(0),
            },
        );
        action_card_tokens(
            &game,
            &content,
            &Action::Play {
                hand: 1,
                target: Some(0),
            },
            &mut values,
            &mut Vec::new(),
        );
        assert_eq!(&values[51..53], &[10.0, 6.0]);

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[0].creature.block = 4;
        combat.enemies[0].creature.powers.clear();
        game.run
            .relics
            .push(content.relic_id("RELIC.THE_BOOT").unwrap());
        let mut values = compact_action_values(
            &game,
            Layout::new(&content),
            &Action::Play {
                hand: 0,
                target: Some(0),
            },
        );
        action_card_tokens(
            &game,
            &content,
            &Action::Play {
                hand: 0,
                target: Some(0),
            },
            &mut values,
            &mut Vec::new(),
        );
        assert_eq!(&values[51..53], &[6.0, 5.0]);
    }

    #[test]
    fn v22_preview_covers_osty_hp_loss_and_public_card_counts() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 83, 4, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let unleash = Card {
            id: content.card_id("CARD.UNLEASH").unwrap(),
            ..Card::default()
        };
        let capture = Card {
            id: content.card_id("CARD.CAPTURE_SPIRIT").unwrap(),
            ..Card::default()
        };
        let defend = Card {
            id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![unleash, capture];
        combat.osty.hp = 6;
        combat.osty.powers = vec![
            Power {
                id: power_id::STRENGTH,
                amount: 2,
                skip_duration: false,
                value: 0,
            },
            Power {
                id: power_id::WEAK,
                amount: 1,
                skip_duration: false,
                value: 0,
            },
        ];
        combat.enemies[0].creature.hp = 20;
        combat.enemies[0].creature.max_hp = 20;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers = vec![Power {
            id: power_id::VULNERABLE,
            amount: 1,
            skip_duration: false,
            value: 0,
        }];
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        action_card_tokens(&game, &content, &action, &mut values, &mut Vec::new());
        assert_eq!(&values[51..53], &[15.0, 15.0]);

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[0].creature.hp = 3;
        combat.enemies[0].creature.block = 99;
        let action = Action::Play {
            hand: 1,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        action_card_tokens(&game, &content, &action, &mut values, &mut Vec::new());
        assert_eq!(&values[51..55], &[3.0, 3.0, 0.0, 1.0]);

        let escape = Card {
            id: content.card_id("CARD.ESCAPE_PLAN").unwrap(),
            ..Card::default()
        };
        let pillage = Card {
            id: content.card_id("CARD.PILLAGE").unwrap(),
            ..Card::default()
        };
        let scrape = Card {
            id: content.card_id("CARD.SCRAPE").unwrap(),
            ..Card::default()
        };
        let dodge = Card {
            id: content.card_id("CARD.DODGE_AND_ROLL").unwrap(),
            ..Card::default()
        };
        let eidolon = Card {
            id: content.card_id("CARD.EIDOLON").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers.clear();
        combat.draw = vec![strike, defend];
        combat.known_draw_top = 2;
        combat.discard.clear();
        assert_eq!(card_preview(&game, &content, escape, None).draw, 1);
        assert_eq!(card_preview(&game, &content, escape, None).block, 3);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.draw = vec![defend, strike];
        assert_eq!(card_preview(&game, &content, pillage, Some(0)).draw, 2);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.draw = vec![defend, strike, defend, strike];
        combat.known_draw_top = 4;
        let scrape = card_preview(&game, &content, scrape, Some(0));
        assert_eq!((scrape.draw, scrape.discard), (4, 4));
        assert_eq!(card_preview(&game, &content, dodge, None).block, 4);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![eidolon, strike, defend];
        assert_eq!(card_preview(&game, &content, eidolon, None).exhaust, 2);
    }

    #[test]
    fn card_multisets_keep_same_id_field_correlations() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut paired = Game::new_character_ascension(&content, 7, 0, 10).unwrap();
        paired.begin_act(&content, 0).unwrap();
        paired
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let id = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        if let Phase::Combat(combat) = &mut paired.phase {
            combat.hand = vec![
                Card {
                    id,
                    instance: 1,
                    upgrades: 1,
                    free: true,
                    ..Card::default()
                },
                Card {
                    id,
                    instance: 2,
                    ..Card::default()
                },
            ];
        }
        let state = state_features(&paired, layout);
        if let Phase::Combat(combat) = &mut paired.phase {
            combat.hand.reverse();
        }
        assert_eq!(state_features(&paired, layout), state);

        let mut split = paired.clone();
        let Phase::Combat(combat) = &mut split.phase else {
            unreachable!()
        };
        combat.hand[0].free = true;
        combat.hand[1].free = false;
        assert_ne!(state_features(&split, layout), state);

        let mut override_cost = paired;
        let Phase::Combat(combat) = &mut override_cost.phase else {
            unreachable!()
        };
        combat.hand[0].cost_override = Some(-1);
        assert_ne!(state_features(&override_cost, layout), state);
    }

    #[test]
    fn card_multisets_compress_large_duplicate_piles_exactly() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 7, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let id = content.card_id("CARD.DAZED").unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.draw = (10_000..12_048)
            .map(|instance| Card {
                id,
                instance,
                ..Card::default()
            })
            .collect();
        combat.known_draw_top = 0;
        combat.known_draw_bottom = 0;
        let state = state_features(&game, layout);
        let zone = layout.card + 2 * CARD_SLOTS * CARD_TOKEN_VALUES;
        assert_ne!(state[zone], 0.0);
        assert_eq!(state[zone + CARD_TOKEN_VALUES], 0.0);
        let low = (state[zone + CARD_TOKEN_VALUES - 2] * u16::MAX as f32).round() as u32;
        let high = (state[zone + CARD_TOKEN_VALUES - 1] * u16::MAX as f32).round() as u32;
        assert_eq!(low | high << 16, 2048);
    }

    #[test]
    fn duplicate_card_actions_match_exactly_and_enchantments_do_not() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let id = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![
            Card {
                id,
                instance: 101,
                ..Card::default()
            },
            Card {
                id,
                instance: 102,
                ..Card::default()
            },
        ];
        combat.discard.clear();
        combat.choice = Some(Choice {
            pile: Pile::Hand,
            filter: CardFilter::Any,
            op: CardOp::Move(Pile::Discard),
            remaining: 1,
            optional: false,
        });
        assert_eq!(
            action_features(&game, layout, &Action::Choose(0)),
            action_features(&game, layout, &Action::Choose(1))
        );
        let mut left = game.clone();
        let mut right = game.clone();
        left.step(&content, Action::Choose(0)).unwrap();
        right.step(&content, Action::Choose(1)).unwrap();
        assert_eq!(
            state_features(&left, layout),
            state_features(&right, layout)
        );
        assert_eq!(left.actions(&content), right.actions(&content));

        if let Phase::Combat(combat) = &mut game.phase {
            combat.hand[0].enchantment = Some(Enchantment::Adroit);
            combat.hand[1].enchantment = Some(Enchantment::Momentum);
        }
        assert_ne!(
            action_features(&game, layout, &Action::Choose(0)),
            action_features(&game, layout, &Action::Choose(1))
        );

        if let Phase::Combat(combat) = &mut game.phase {
            combat.hand[0].enchantment = None;
            combat.hand[1].enchantment = None;
            combat.dampened = vec![(101, 1), (102, 2)];
        }
        assert_ne!(
            action_features(&game, layout, &Action::Choose(0)),
            action_features(&game, layout, &Action::Choose(1))
        );
        let mut left = game.clone();
        let mut right = game;
        left.step(&content, Action::Choose(0)).unwrap();
        right.step(&content, Action::Choose(1)).unwrap();
        assert_ne!(
            state_features(&left, layout),
            state_features(&right, layout)
        );
    }

    #[test]
    fn bundle_actions_keep_same_id_card_correlations() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let id = content.card_id("CARD.GENETIC_ALGORITHM").unwrap();
        game.phase = Phase::ChooseBundles(vec![
            vec![
                Card {
                    id,
                    value: 1,
                    ..Card::default()
                },
                Card {
                    id,
                    upgrades: 1,
                    value: 5,
                    ..Card::default()
                },
            ],
            vec![
                Card {
                    id,
                    value: 5,
                    ..Card::default()
                },
                Card {
                    id,
                    upgrades: 1,
                    value: 1,
                    ..Card::default()
                },
            ],
        ]);
        let mut first = action_features(&game, layout, &Action::Choose(0));
        let mut second = action_features(&game, layout, &Action::Choose(1));
        assert_ne!(first, second);
        first[layout.action_value] = 0.0;
        second[layout.action_value] = 0.0;
        assert_eq!(first, second);

        let mut left = game.clone();
        let mut right = game;
        left.step(&content, Action::Choose(0)).unwrap();
        right.step(&content, Action::Choose(1)).unwrap();
        assert_ne!(
            state_features(&left, layout),
            state_features(&right, layout)
        );
    }

    #[test]
    fn combat_cards_keep_their_public_master_card_relation() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let id = content.card_id("CARD.GENETIC_ALGORITHM").unwrap();
        let mut left = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        left.begin_act(&content, 0).unwrap();
        left.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        left.run.deck = vec![
            Card {
                id,
                instance: 101,
                value: 1,
                ..Card::default()
            },
            Card {
                id,
                instance: 102,
                value: 5,
                ..Card::default()
            },
        ];
        let selected = Card {
            id,
            instance: 101,
            value: 5,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut left.phase else {
            unreachable!()
        };
        combat.hand = vec![selected];
        combat.energy = 10;
        combat.choice = None;
        let mut right = left.clone();
        let Phase::Combat(combat) = &mut right.phase else {
            unreachable!()
        };
        combat.hand[0].instance = 102;
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        assert_ne!(
            state_features(&left, layout),
            state_features(&right, layout)
        );
        assert_ne!(
            action_features(&left, layout, &action),
            action_features(&right, layout, &action)
        );
        left.step(&content, action.clone()).unwrap();
        right.step(&content, action).unwrap();
        assert_ne!(
            state_features(&left, layout),
            state_features(&right, layout)
        );
    }

    #[test]
    fn run_choices_keep_their_pending_public_effects() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut left = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        left.begin_act(&content, 0).unwrap();
        let event = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.SPIRIT_GRAFTER")
            .unwrap() as Id;
        left.phase = Phase::Event(event, content.events[event as usize].options.to_vec());
        left.step(&content, Action::Event(1)).unwrap();
        assert!(matches!(left.phase, Phase::UpgradeCards(..)));
        assert_eq!(left.run_queue.len(), 1);
        let mut right = left.clone();
        right.run_queue[0] = RunEffect::LoseHp(9);
        assert_ne!(
            format!("{:?}", left.run_queue),
            format!("{:?}", right.run_queue)
        );
        assert_ne!(
            &state_features(&left, layout)[layout.continuation..layout.decision],
            &state_features(&right, layout)[layout.continuation..layout.decision]
        );
        let action = left.actions(&content)[0].clone();
        left.step(&content, action.clone()).unwrap();
        right.step(&content, action).unwrap();
        assert_eq!(right.run.hp, left.run.hp + 1);
    }

    #[test]
    fn combat_choices_keep_manual_and_automatic_continuations() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut manual = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        manual.begin_act(&content, 0).unwrap();
        manual
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let burning_pact = Card {
            id: content.card_id("CARD.BURNING_PACT").unwrap(),
            instance: 1001,
            ..Card::default()
        };
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            instance: 1002,
            ..Card::default()
        };
        let defend = Card {
            id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
            instance: 1003,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut manual.phase else {
            unreachable!()
        };
        combat.hand = vec![burning_pact, strike, defend];
        combat.energy = 10;
        combat.player.powers.push(Power {
            id: power_id::STRENGTH,
            amount: 2,
            skip_duration: false,
            value: 0,
        });
        manual
            .step(
                &content,
                Action::Play {
                    hand: 0,
                    target: None,
                },
            )
            .unwrap();
        let combat = manual.combat().unwrap();
        assert!(combat.choice.is_some());
        assert!(!combat.queue.is_empty());
        assert!(combat.playing.is_some());
        assert!(!combat.power_snapshot.is_empty());
        let tokens = state_tokens(&manual, &content, layout);
        assert!(tokens.iter().any(|row| {
            row[0] == CARD_COLLECTION as f32
                && row[1] == 9.0
                && row[2] == burning_pact.id as f32 + 1.0
                && row[4] == 0.0
        }));
        assert!(
            tokens
                .iter()
                .filter(|row| row[0] == CARD_COLLECTION as f32)
                .all(|row| row[4] == 0.0 || row[1] == 3.0)
        );
        let state = state_features(&manual, layout);
        assert!(
            state[layout.continuation..layout.decision]
                .iter()
                .any(|value| *value != 0.0)
        );
        let mut changed = manual.clone();
        if let Phase::Combat(combat) = &mut changed.phase {
            combat.queue.clear();
        }
        assert_ne!(state_features(&changed, layout), state);
        manual.step(&content, Action::Choose(0)).unwrap();
        assert!(manual.combat().unwrap().playing.is_none());
        assert!(
            state_features(&manual, layout)[layout.continuation..layout.decision]
                .iter()
                .all(|value| *value == 0.0)
        );

        let mut automatic = Game::new_character_ascension(&content, 9, 0, 10).unwrap();
        automatic.begin_act(&content, 0).unwrap();
        automatic
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let survivor = Card {
            id: content.card_id("CARD.SURVIVOR").unwrap(),
            instance: 1004,
            ..Card::default()
        };
        if let Phase::Combat(combat) = &mut automatic.phase {
            combat.hand = vec![
                survivor,
                Card {
                    flags: SLY,
                    ..burning_pact
                },
                strike,
                defend,
            ];
            combat.energy = 10;
        }
        automatic
            .step(
                &content,
                Action::Play {
                    hand: 0,
                    target: None,
                },
            )
            .unwrap();
        automatic.step(&content, Action::Choose(0)).unwrap();
        let combat = automatic.combat().unwrap();
        assert!(combat.choice.is_some());
        assert!(!combat.auto_plays.is_empty());
        let tokens = state_tokens(&automatic, &content, layout);
        assert_eq!(
            tokens
                .iter()
                .filter(|row| {
                    row[0] == CARD_COLLECTION as f32 && row[1] == 10.0 && row[4] == 0.0
                })
                .count(),
            combat.auto_plays.len()
        );
        assert!(
            tokens
                .iter()
                .filter(|row| row[0] == CARD_COLLECTION as f32)
                .all(|row| row[4] == 0.0 || row[1] == 3.0)
        );
        assert!(
            state_features(&automatic, layout)[layout.continuation..layout.decision]
                .iter()
                .any(|value| *value != 0.0)
        );
        automatic.step(&content, Action::Choose(0)).unwrap();
        assert!(automatic.combat().unwrap().auto_plays.is_empty());
        assert!(
            state_features(&automatic, layout)[layout.continuation..layout.decision]
                .iter()
                .all(|value| *value == 0.0)
        );
    }

    #[test]
    fn duplicate_powers_keep_value_and_duration_pairing() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut left = Game::new_character_ascension(&content, 10, 0, 10).unwrap();
        left.begin_act(&content, 0).unwrap();
        left.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let powers = vec![
            Power {
                id: power_id::STRENGTH,
                amount: 1,
                value: 10,
                skip_duration: false,
            },
            Power {
                id: power_id::STRENGTH,
                amount: 2,
                value: 20,
                skip_duration: true,
            },
        ];
        let Phase::Combat(combat) = &mut left.phase else {
            unreachable!()
        };
        combat.player.powers = powers.clone();
        let mut right = left.clone();
        let Phase::Combat(combat) = &mut right.phase else {
            unreachable!()
        };
        combat.player.powers = vec![
            Power {
                value: 20,
                skip_duration: true,
                ..powers[0]
            },
            Power {
                value: 10,
                skip_duration: false,
                ..powers[1]
            },
        ];
        assert_ne!(
            state_features(&left, layout),
            state_features(&right, layout)
        );
    }

    #[test]
    fn power_trigger_order_is_encoded() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 10, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let ids = [
            "POWER.MACHINE_LEARNING_POWER",
            "POWER.FOREGONE_CONCLUSION_POWER",
        ]
        .map(|name| {
            content
                .powers
                .iter()
                .position(|power| power.id == name)
                .unwrap() as Id
        });
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers = ids
            .map(|id| Power {
                id,
                amount: 1,
                value: 0,
                skip_duration: false,
            })
            .to_vec();
        let tokens = state_tokens(&game, &content, layout);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers.reverse();
        assert_ne!(state_tokens(&game, &content, layout), tokens);
    }

    #[test]
    fn power_snapshots_keep_full_tokens_and_token_order() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut left = Game::new_character_ascension(&content, 10, 0, 10).unwrap();
        left.begin_act(&content, 0).unwrap();
        left.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let powers = [
            Power {
                id: power_id::STRENGTH,
                amount: 1,
                value: 10,
                skip_duration: false,
            },
            Power {
                id: power_id::STRENGTH,
                amount: 2,
                value: 20,
                skip_duration: true,
            },
        ];
        let Phase::Combat(combat) = &mut left.phase else {
            unreachable!()
        };
        combat.power_snapshot = powers.to_vec();
        let mut right = left.clone();
        let Phase::Combat(combat) = &mut right.phase else {
            unreachable!()
        };
        combat.power_snapshot = vec![
            Power {
                value: 20,
                skip_duration: true,
                ..powers[0]
            },
            Power {
                value: 10,
                skip_duration: false,
                ..powers[1]
            },
        ];
        assert_ne!(
            state_features(&left, layout),
            state_features(&right, layout)
        );
        let Phase::Combat(combat) = &mut right.phase else {
            unreachable!()
        };
        combat.power_snapshot = vec![powers[1], powers[0]];
        assert_eq!(
            state_features(&left, layout),
            state_features(&right, layout)
        );
        assert_ne!(
            state_tokens(&left, &content, layout),
            state_tokens(&right, &content, layout)
        );
    }

    #[test]
    fn paused_debuffs_keep_unsettling_lamp_card_identity() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut doubled = Game::new_character_ascension(&content, 11, 0, 10).unwrap();
        doubled.begin_act(&content, 0).unwrap();
        doubled
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        doubled
            .run
            .relics
            .push(content.relic_id("RELIC.UNSETTLING_LAMP").unwrap());
        let played = Card {
            id: content.card_id("CARD.NEUTRALIZE").unwrap(),
            instance: 1001,
            ..Card::default()
        };
        let choice = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            instance: 1002,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut doubled.phase else {
            unreachable!()
        };
        combat.hand = vec![choice];
        combat.playing = Some(played);
        combat.choice = Some(Choice {
            pile: Pile::Hand,
            filter: CardFilter::Any,
            op: CardOp::Move(Pile::Discard),
            remaining: 1,
            optional: false,
        });
        combat.queue = vec![Pending {
            effect: Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(1, 1)),
            context: Context {
                target: Some(0),
                ..Context::card(played)
            },
        }];
        combat.unsettling_lamp = Some(played.id);
        let mut normal = doubled.clone();
        let Phase::Combat(combat) = &mut normal.phase else {
            unreachable!()
        };
        combat.unsettling_lamp = Some(choice.id);
        assert_ne!(
            state_features(&doubled, layout),
            state_features(&normal, layout)
        );
        doubled.step(&content, Action::Choose(0)).unwrap();
        normal.step(&content, Action::Choose(0)).unwrap();
        assert_eq!(
            doubled.combat().unwrap().enemies[0]
                .creature
                .power(power_id::WEAK),
            2
        );
        assert_eq!(
            normal.combat().unwrap().enemies[0]
                .creature
                .power(power_id::WEAK),
            1
        );
    }

    #[test]
    fn target_actions_identify_enemy_slots() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.push(combat.enemies[0].clone());
        let target = |slot| Action::Potion {
            slot: 0,
            target: Some(slot),
        };
        assert_ne!(
            action_features(&game, layout, &target(0)),
            action_features(&game, layout, &target(1))
        );
    }

    #[test]
    fn features_cover_live_combat_choice_context() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.choice = Some(Choice {
            pile: Pile::Hand,
            filter: CardFilter::Any,
            op: CardOp::Move(Pile::Discard),
            remaining: 1,
            optional: true,
        });
        let discard = state_features(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.choice.as_mut().unwrap().op = CardOp::Move(Pile::Exhaust);
        }
        assert_ne!(state_features(&game, layout), discard);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.choice.as_mut().unwrap().op = CardOp::Move(Pile::Discard);
            combat.choice.as_mut().unwrap().filter = CardFilter::Upgradable;
        }
        assert_ne!(state_features(&game, layout), discard);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.choice.as_mut().unwrap().filter = CardFilter::Any;
            combat.enemy_turn = true;
        }
        assert_ne!(state_features(&game, layout), discard);
    }

    #[test]
    fn features_cover_resume_phase() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.phase = Phase::TransformCards(None, 1, false);
        let no_resume = state_features(&game, layout);
        game.resume = Some(Phase::Map);
        assert_ne!(state_features(&game, layout), no_resume);

        game.resume = Some(Phase::Rewards(Rewards {
            gold: 10,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        }));
        let reward = state_features(&game, layout);
        let mut changed = game.clone();
        let Some(Phase::Rewards(rewards)) = &mut changed.resume else {
            unreachable!()
        };
        rewards.gold = 20;
        assert_ne!(state_features(&changed, layout), reward);

        let card = game.run.deck[0];
        game.resume = Some(Phase::Shop(vec![ShopItem::Card(card, 50)]));
        let shop = state_features(&game, layout);
        let mut changed = game.clone();
        let Some(Phase::Shop(items)) = &mut changed.resume else {
            unreachable!()
        };
        let ShopItem::Card(_, price) = &mut items[0] else {
            unreachable!()
        };
        *price = 51;
        assert_ne!(state_features(&changed, layout), shop);

        let mut renamed = game.clone();
        for card in &mut renamed.run.deck {
            card.instance += 100;
        }
        let Some(Phase::Shop(items)) = &mut renamed.resume else {
            unreachable!()
        };
        let ShopItem::Card(card, _) = &mut items[0] else {
            unreachable!()
        };
        card.instance += 100;
        assert_eq!(state_features(&renamed, layout), shop);

        game.resume = Some(Phase::Event(0, content.events[0].options.to_vec()));
        let event = state_features(&game, layout);
        game.resume = Some(Phase::Event(1, content.events[1].options.to_vec()));
        assert_ne!(state_features(&game, layout), event);
    }

    #[test]
    fn conveyor_state_survives_intermediate_choices() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let event = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.ENDLESS_CONVEYOR")
            .unwrap() as Id;
        game.phase = Phase::TransformCards(None, 1, false);
        game.resume = Some(Phase::Event(event, vec![]));
        game.conveyor = true;
        game.event_data = [3, 4, 5, 0];
        let tokens = state_tokens(&game, &content, layout);
        game.event_data[1] += 1;
        assert_ne!(state_tokens(&game, &content, layout), tokens);
    }

    #[test]
    fn resume_payload_determines_the_resumed_transition() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first.phase = Phase::RemoveCards(1, 0, false);
        first.resume = Some(Phase::Rewards(Rewards {
            gold: 10,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        }));
        let mut second = first.clone();
        let Some(Phase::Rewards(rewards)) = &mut second.resume else {
            unreachable!()
        };
        rewards.gold = 20;
        assert_ne!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        first.step(&content, Action::RemoveCard(0)).unwrap();
        second.step(&content, Action::RemoveCard(0)).unwrap();
        assert_ne!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
    }

    #[test]
    fn features_ignore_hidden_randomness_and_order() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let observe = |game: &Game| {
            let actions = game.actions(&content);
            let action_rows = actions
                .iter()
                .map(|action| {
                    let mut values = compact_action_values(game, layout, action);
                    let mut tokens = action_entity_tokens(game, &content, layout, action);
                    action_card_tokens(game, &content, action, &mut values, &mut tokens);
                    tokens.sort_by(|left, right| token_cmp(left, right));
                    (values, tokens)
                })
                .collect::<Vec<_>>();
            (
                global_features(game, layout),
                state_tokens(game, &content, layout),
                actions,
                action_rows,
                potential(game),
            )
        };
        let mut game = Game::new_character_ascension(&content, 9, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let expected = state_features(&game, layout);
        let expected_observation = observe(&game);
        assert!(expected.iter().all(|value| value.abs() <= 100.0));
        let mut reordered = game.clone();
        if let Phase::Combat(combat) = &mut reordered.phase {
            combat.draw.reverse();
        }
        reordered.rngs.shuffle.next();
        reordered.rngs.monster_ai.next();
        reordered.encounters.reverse();
        reordered.elites.reverse();
        reordered.events.reverse();
        for relics in &mut reordered.relic_deques {
            relics.reverse();
        }
        assert_eq!(observe(&reordered), expected_observation);
        resample_hidden(&mut game, &content, 123);
        resample_hidden(&mut reordered, &content, 123);
        assert_eq!(state_features(&reordered, layout), expected);
        assert_eq!(observe(&game), expected_observation);
        assert_eq!(observe(&reordered), expected_observation);
        let draws = |game: &Game| {
            game.combat()
                .unwrap()
                .draw
                .iter()
                .map(card_key)
                .collect::<Vec<_>>()
        };
        assert_eq!(draws(&game), draws(&reordered));
        assert_eq!(game.encounters, reordered.encounters);
        assert_eq!(game.elites, reordered.elites);
        assert_eq!(game.events, reordered.events);
        assert_eq!(game.relic_deques, reordered.relic_deques);
        assert_eq!(game.shared_relic_deques, reordered.shared_relic_deques);
        let actions = game.actions(&content);
        assert_eq!(actions, reordered.actions(&content));
        for action in actions {
            let mut left = game.clone();
            let mut right = reordered.clone();
            left.step(&content, action.clone()).unwrap();
            right.step(&content, action).unwrap();
            assert_eq!(
                state_features(&left, layout),
                state_features(&right, layout)
            );
            assert_eq!(left.actions(&content), right.actions(&content));
            assert_eq!(observe(&left), observe(&right));
        }
    }

    #[test]
    fn relic_bag_order_is_hidden_but_membership_is_public() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 17, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let shovel = content.relic_id("RELIC.SHOVEL").unwrap();
        game.run.relics.push(shovel);
        for bag in game
            .relic_deques
            .iter_mut()
            .chain(&mut game.shared_relic_deques)
        {
            bag.retain(|candidate| *candidate != shovel);
        }
        game.phase = Phase::Rest;
        let state = state_features(&game, layout);

        let mut reordered = game.clone();
        for bag in reordered
            .relic_deques
            .iter_mut()
            .chain(&mut reordered.shared_relic_deques)
        {
            bag.reverse();
        }
        assert_eq!(state_features(&reordered, layout), state);

        let mut missing = game.clone();
        let relic = missing
            .relic_deques
            .iter()
            .flatten()
            .next()
            .copied()
            .unwrap();
        for bag in &mut missing.relic_deques {
            bag.retain(|candidate| *candidate != relic);
        }
        assert_ne!(state_features(&missing, layout), state);
        let mut missing = game.clone();
        let relic = missing
            .shared_relic_deques
            .iter()
            .flatten()
            .next()
            .copied()
            .unwrap();
        for bag in &mut missing.shared_relic_deques {
            bag.retain(|candidate| *candidate != relic);
        }
        assert_ne!(state_features(&missing, layout), state);

        let mut first = game;
        resample_hidden(&mut first, &content, 123);
        resample_hidden(&mut reordered, &content, 123);
        assert_eq!(first.relic_deques, reordered.relic_deques);
        assert_eq!(first.shared_relic_deques, reordered.shared_relic_deques);
        let mut treasure_first = first.clone();
        let mut treasure_reordered = reordered.clone();
        assert!(first.actions(&content).contains(&Action::Dig));
        first.step(&content, Action::Dig).unwrap();
        reordered.step(&content, Action::Dig).unwrap();
        assert_eq!(
            state_features(&first, layout),
            state_features(&reordered, layout)
        );
        assert_eq!(first.actions(&content), reordered.actions(&content));

        treasure_first.phase = Phase::Map;
        treasure_reordered.phase = Phase::Map;
        treasure_first.map.current = None;
        treasure_reordered.map.current = None;
        let floor = treasure_first
            .map
            .nodes
            .iter()
            .map(|node| node.floor)
            .min()
            .unwrap();
        let node = treasure_first
            .map
            .nodes
            .iter()
            .position(|node| node.floor == floor)
            .unwrap();
        treasure_first.map.nodes[node].room = Room::Treasure;
        treasure_reordered.map.nodes[node].room = Room::Treasure;
        treasure_first.step(&content, Action::Path(node)).unwrap();
        treasure_reordered
            .step(&content, Action::Path(node))
            .unwrap();
        assert_eq!(
            state_features(&treasure_first, layout),
            state_features(&treasure_reordered, layout)
        );
        assert_eq!(
            treasure_first.actions(&content),
            treasure_reordered.actions(&content)
        );
    }

    #[test]
    fn hidden_event_relics_and_future_queue_resample_to_equal_transitions() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 18, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.run.gold = 300;
        let wongo = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.WELCOME_TO_WONGOS")
            .unwrap() as Id;
        game.phase = Phase::Event(wongo, content.events[wongo as usize].options.to_vec());
        let relics = game.relic_deques[2]
            .iter()
            .copied()
            .take(4)
            .collect::<Vec<_>>();
        assert_eq!(relics.len(), 4);

        let hidden = |mut game: Game, relics: &[Id], event_relic: Option<Id>| {
            for relic in relics.iter().copied().chain(event_relic) {
                for bag in game
                    .relic_deques
                    .iter_mut()
                    .chain(&mut game.shared_relic_deques)
                {
                    bag.retain(|candidate| *candidate != relic);
                }
            }
            game.event_relic = event_relic;
            game.relic_queue = relics.to_vec();
            game
        };
        let mut first = hidden(game.clone(), &relics[..2], Some(relics[2]));
        let mut second = hidden(game, &relics[2..], Some(relics[0]));
        assert_eq!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert!(first.event_relic.is_none() && first.relic_queue.is_empty());
        assert_eq!(first.relic_deques, second.relic_deques);
        assert_eq!(first.shared_relic_deques, second.shared_relic_deques);
        first.step(&content, Action::Event(1)).unwrap();
        second.step(&content, Action::Event(1)).unwrap();
        assert_eq!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        assert_eq!(first.actions(&content), second.actions(&content));
    }

    #[test]
    fn hidden_ancient_relic_outcomes_are_generated_after_the_choice() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        for (event_name, relic_name) in [
            ("EVENT.DARV", "RELIC.DUSTY_TOME"),
            ("EVENT.OROBAS", "RELIC.SEA_GLASS"),
        ] {
            let event = content
                .events
                .iter()
                .position(|event| event.id == event_name)
                .unwrap() as Id;
            let relic = content.relic_id(relic_name).unwrap();
            let mut first = Game::new_character_ascension(&content, 21, 0, 10).unwrap();
            first.begin_act(&content, 0).unwrap();
            first.phase = Phase::Event(event, content.events[event as usize].options.to_vec());
            first.event_data[0] = relic as i64 + 1;
            let shared = token(
                RELIC_COLLECTION,
                ANCIENT_RELIC_KIND,
                relic as usize + 1,
                1,
                0,
            );
            assert!(
                action_entity_tokens(&first, &content, layout, &Action::Event(0)).contains(&shared)
            );
            let mut second = first.clone();
            for _ in 0..7 {
                second.rngs.rewards.next();
            }
            assert_eq!(
                state_features(&first, layout),
                state_features(&second, layout)
            );
            resample_hidden(&mut first, &content, 42);
            resample_hidden(&mut second, &content, 42);
            first.step(&content, Action::Event(0)).unwrap();
            second.step(&content, Action::Event(0)).unwrap();
            assert_eq!(
                state_features(&first, layout),
                state_features(&second, layout)
            );
            assert_eq!(first.actions(&content), second.actions(&content));
            assert!(first.run.relics.contains(&relic));
            if relic_name == "RELIC.DUSTY_TOME" {
                assert!(first.run.deck.iter().any(|card| {
                    card.upgrades == 1
                        && content.cards[card.id as usize].rarity == CardRarity::Ancient
                }));
            } else {
                assert!(matches!(first.phase, Phase::ChooseCards(_, 15, true)));
            }
        }
    }

    #[test]
    fn vakuu_offers_are_shared_action_identities_not_raw_globals() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let event = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.VAKUU")
            .unwrap() as Id;
        let first_relic = content.relic_id("RELIC.BLOOD_SOAKED_ROSE").unwrap();
        let second_relic = content.relic_id("RELIC.WHISPERING_EARRING").unwrap();
        let mut first = Game::new_character_ascension(&content, 21, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first.phase = Phase::Event(event, content.events[event as usize].options.to_vec());
        first.event_data[0] = first_relic as i64 + 1;
        let mut second = first.clone();
        second.event_data[0] = second_relic as i64 + 1;

        assert_eq!(
            global_features(&first, layout),
            global_features(&second, layout)
        );
        let first = tokenized_action(&first, &content, layout, &Action::Event(0));
        let second = tokenized_action(&second, &content, layout, &Action::Event(0));
        assert_ne!(first, second);
        assert!(first.1.contains(&token(
            RELIC_COLLECTION,
            ANCIENT_RELIC_KIND,
            first_relic as usize + 1,
            1,
            0,
        )));
    }

    #[test]
    fn relic_trader_encodes_visible_pairs_and_drops_unchosen_queue() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 19, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let relics = game
            .relic_deques
            .iter()
            .flatten()
            .copied()
            .take(6)
            .collect::<Vec<_>>();
        assert_eq!(relics.len(), 6);
        let first_owned = game.run.relics.len();
        game.run.relics.extend_from_slice(&relics[..3]);
        for &relic in &relics {
            for bag in game
                .relic_deques
                .iter_mut()
                .chain(&mut game.shared_relic_deques)
            {
                bag.retain(|candidate| *candidate != relic);
            }
        }
        let trader = layout.relic_trader.unwrap();
        game.phase = Phase::Event(trader, content.events[trader as usize].options.to_vec());
        game.event_cards = (first_owned..first_owned + 3)
            .map(|index| index as u32)
            .collect();
        game.relic_queue = relics[3..].to_vec();

        let state = state_features(&game, layout);
        let tokens = state_tokens(&game, &content, layout);
        let action = tokenized_action(&game, &content, layout, &Action::Event(0));
        let mut swapped = game.clone();
        swapped.relic_queue.swap(0, 1);
        assert_eq!(state_features(&swapped, layout), state);
        assert_eq!(state_tokens(&swapped, &content, layout), tokens);
        assert_ne!(
            tokenized_action(&swapped, &content, layout, &Action::Event(0)),
            action
        );

        let mut first = game.clone();
        let mut second = game;
        second.rngs.rewards.next();
        assert_eq!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert_eq!(first.relic_queue, second.relic_queue);
        first.step(&content, Action::Event(0)).unwrap();
        second.step(&content, Action::Event(0)).unwrap();
        assert!(first.relic_queue.is_empty() && second.relic_queue.is_empty());
        assert_eq!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        assert_eq!(first.actions(&content), second.actions(&content));
    }

    #[test]
    fn crystal_resampling_preserves_public_board_and_rerolls_hidden_layout() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 9, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first.phase = Phase::Event(0, vec![]);
        first.resume = Some(Phase::Map);
        first.event_rng = Some(Rng::from_seed(1));
        let mut clear = vec![false; 121];
        for x in 0..11usize {
            for y in 0..11usize {
                clear[x * 11 + y] =
                    x + y <= 2 || 10 - x + y <= 2 || x + 10 - y <= 2 || 20 - x - y <= 2;
            }
        }
        clear[3 * 11 + 3] = true;
        let mut cells = vec![None; 121];
        cells[3 * 11 + 3] = Some(0);
        cells[5 * 11 + 5] = Some(1);
        first.crystal = Some(CrystalSphere {
            cells,
            clear,
            items: vec![(3, 3, 1, 1, 7), (5, 5, 4, 4, 0)],
            revealed: vec![0],
            remaining: 1,
            big: false,
        });
        let mut second = first.clone();
        let crystal = second.crystal.as_mut().unwrap();
        crystal.items.swap(0, 1);
        crystal.revealed = vec![1];
        crystal.cells.fill(None);
        crystal.cells[3 * 11 + 3] = Some(1);
        crystal.cells[7 * 11 + 7] = Some(0);
        let public = state_features(&first, layout);
        assert_eq!(state_features(&second, layout), public);

        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert_eq!(state_features(&first, layout), public);
        assert_eq!(state_features(&second, layout), public);
        let (a, b) = (
            first.crystal.as_ref().unwrap(),
            second.crystal.as_ref().unwrap(),
        );
        assert_eq!(a.cells, b.cells);
        assert_eq!(a.items, b.items);
        assert_eq!(a.revealed, b.revealed);

        let crystal = first.crystal.as_ref().unwrap();
        let item = crystal
            .items
            .iter()
            .enumerate()
            .find(|(id, item)| !crystal.revealed.contains(id) && item.2 == 1 && item.3 == 1)
            .unwrap()
            .1;
        let action = Action::CrystalCell(item.0, item.1);
        first.step(&content, action.clone()).unwrap();
        second.step(&content, action).unwrap();
        assert_eq!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        assert_eq!(first.actions(&content), second.actions(&content));
    }

    #[test]
    fn draw_choices_ignore_hidden_order() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 10, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let cards = |kind| {
            content
                .cards
                .iter()
                .enumerate()
                .filter(|(_, card)| card.card_type == kind)
                .map(|(id, _)| Card {
                    id: id as Id,
                    ..Card::default()
                })
                .take(3)
                .collect::<Vec<_>>()
        };
        let attacks = cards(CardType::Attack);
        let skills = cards(CardType::Skill);
        let Phase::Combat(combat) = &mut game.phase else {
            panic!("combat did not start");
        };
        combat.draw = vec![
            skills[0], attacks[0], skills[1], attacks[1], skills[2], attacks[2],
        ];
        combat.choice = Some(Choice {
            pile: Pile::Draw,
            filter: CardFilter::Type(CardType::Attack),
            op: CardOp::Move(Pile::Hand),
            remaining: 1,
            optional: true,
        });
        let state = state_features(&game, layout);
        let actions = game.actions(&content);
        let mut features = actions
            .iter()
            .map(|action| {
                action_features(&game, layout, action)
                    .into_iter()
                    .map(f32::to_bits)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        features.sort();
        let mut changed = false;
        for seed in 1..=8 {
            let mut sampled = game.clone();
            resample_hidden(&mut sampled, &content, seed);
            changed |= sampled.combat().unwrap().draw != game.combat().unwrap().draw;
            assert_eq!(state_features(&sampled, layout), state);
            let sampled_actions = sampled.actions(&content);
            assert_eq!(sampled_actions.len(), actions.len());
            let mut sampled_features = sampled_actions
                .iter()
                .map(|action| {
                    action_features(&sampled, layout, action)
                        .into_iter()
                        .map(f32::to_bits)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            sampled_features.sort();
            assert_eq!(sampled_features, features);
        }
        assert!(changed);
    }

    #[test]
    fn known_draw_ends_survive_hidden_resampling() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 10, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let cards = (0..4)
            .map(|id| Card {
                id,
                ..Card::default()
            })
            .collect::<Vec<_>>();
        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.draw = cards.clone();
        combat.known_draw_bottom = 1;
        combat.known_draw_top = 1;
        let state = state_features(&first, layout);

        let mut second = first.clone();
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.draw.swap(1, 2);
        assert_eq!(state_features(&second, layout), state);
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.draw.swap(2, 3);
        assert_ne!(state_features(&second, layout), state);
        second = first.clone();
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.draw.swap(1, 2);

        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert_eq!(first.combat().unwrap().draw, second.combat().unwrap().draw);
        assert_eq!(first.combat().unwrap().draw.first(), Some(&cards[0]));
        assert_eq!(first.combat().unwrap().draw.last(), Some(&cards[3]));
        first.step(&content, Action::EndTurn).unwrap();
        second.step(&content, Action::EndTurn).unwrap();
        assert!(first.combat().unwrap().hand.contains(&cards[3]));
        assert_eq!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
    }

    #[test]
    fn moving_a_visible_card_to_draw_marks_the_top_known() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 10, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: 1,
            upgrades: 1,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.discard.push(card);
        combat.choice = Some(Choice {
            pile: Pile::Discard,
            filter: CardFilter::Any,
            op: CardOp::Move(Pile::Draw),
            remaining: 1,
            optional: false,
        });
        game.step(&content, Action::Choose(0)).unwrap();
        assert_eq!(game.combat().unwrap().known_draw_top, 1);
        assert_eq!(game.combat().unwrap().draw.last(), Some(&card));
        let state = state_features(&game, layout);
        let mut hidden = game.clone();
        let last = hidden.combat().unwrap().draw.len() - 1;
        let Phase::Combat(combat) = &mut hidden.phase else {
            unreachable!()
        };
        combat.draw.swap(0, last);
        assert_ne!(state_features(&hidden, layout), state);
        resample_hidden(&mut game, &content, 99);
        assert_eq!(game.combat().unwrap().draw.last(), Some(&card));
    }

    #[test]
    fn recycling_visible_hand_marks_the_draw_bottom_known() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 10, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let bottom = [
            Card::default(),
            Card {
                id: 1,
                ..Card::default()
            },
        ];
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![
            Card {
                id: content.card_id("CARD.REBOOT").unwrap(),
                ..Card::default()
            },
            bottom[0],
            bottom[1],
        ];
        combat.draw = (2..10)
            .map(|id| Card {
                id,
                ..Card::default()
            })
            .collect();
        combat.known_draw_top = 0;
        combat.known_draw_bottom = 0;
        game.step(
            &content,
            Action::Play {
                hand: 0,
                target: None,
            },
        )
        .unwrap();
        let combat = game.combat().unwrap();
        assert_eq!(combat.known_draw_bottom, 2);
        assert_eq!(&combat.draw[..2], &bottom);
    }

    #[test]
    fn features_change_with_visible_state() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 11, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let before = state_features(&game, layout);
        game.bosses[1] = Some(0);
        assert_eq!(state_features(&game, layout), before);
        game.bosses[0] = Some((game.bosses[0].unwrap_or(0) + 1) % layout.encounters as u16);
        assert_ne!(state_features(&game, layout), before);
        let before = state_features(&game, layout);
        game.run.hp -= 1;
        assert_ne!(state_features(&game, layout), before);
        let before = state_features(&game, layout);
        game.map.nodes[0].next.clear();
        assert_ne!(state_features(&game, layout), before);
        let before = state_features(&game, layout);
        game.fishing_rod = 1;
        assert_ne!(state_features(&game, layout), before);
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let before = state_features(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.orbs.push(Orb { id: 0, value: 7 });
        }
        assert_ne!(state_features(&game, layout), before);
        let before = state_features(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.hand[0].turn_flags ^= 1;
        }
        assert_ne!(state_features(&game, layout), before);
        let before = state_features(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.paels_tears = !combat.paels_tears;
        }
        assert_ne!(state_features(&game, layout), before);
        let before = state_features(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.diamond_diadem = !combat.diamond_diadem;
        }
        assert_ne!(state_features(&game, layout), before);
    }

    #[test]
    fn features_cover_public_run_state_machines() {
        fn changed(game: &Game, layout: Layout, edit: impl FnOnce(&mut Game)) {
            let state = state_features(game, layout);
            let mut edited = game.clone();
            edit(&mut edited);
            assert_ne!(state_features(&edited, layout), state);
        }

        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 15, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        changed(&game, layout, |game| game.visited_events.push(0));
        changed(&game, layout, |game| game.pending_curse = true);
        changed(&game, layout, |game| game.wongo_combats = Some(0));
        let mut wongo = game.clone();
        wongo.wongo_combats = Some(1);
        changed(&wongo, layout, |game| game.wongo_combats = Some(2));
        changed(&game, layout, |game| {
            game.golden_compass = Some(game.run.act)
        });
        changed(&game, layout, |game| game.astrolabe = true);
        changed(&game, layout, |game| game.transform_niche = true);
        changed(&game, layout, |game| game.paels_tooth = true);
        changed(&game, layout, |game| game.paels_cards.push(Card::default()));
        changed(&game, layout, |game| game.parasol_removal = true);
        changed(&game, layout, |game| game.conveyor = true);
        changed(&game, layout, |game| game.fake_shop = true);

        let node = &game.map.nodes[0];
        let mark = (node.lane, node.floor);
        changed(&game, layout, |game| {
            game.fur_coat_act = Some(game.run.act);
            game.fur_coat.push(mark);
        });
        changed(&game, layout, |game| game.spoils = Some(mark));
        changed(&game, layout, |game| game.melted_relics.push(0));
        changed(&game, layout, |game| game.wax_relics.push(0));

        let relic = game.run.relics[0];
        let mut reward = game.clone();
        reward.phase = Phase::Rewards(Rewards {
            gold: 0,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![relic],
            potions: vec![],
            removals: 0,
        });
        let state = state_tokens(&reward, &content, layout);
        let action = tokenized_action(&reward, &content, layout, &Action::RewardRelic(0));
        reward.toy_box_offers.push(relic);
        assert_eq!(state_tokens(&reward, &content, layout), state);
        assert_ne!(
            tokenized_action(&reward, &content, layout, &Action::RewardRelic(0)),
            action
        );

        let mut event = game;
        event.phase = Phase::Event(0, vec![]);
        let state = state_features(&event, layout);
        event.event_relic = Some(relic);
        assert_eq!(state_features(&event, layout), state);
    }

    #[test]
    fn pending_public_noncombat_sequences_are_encoded() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 20, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();

        let card = game.run.deck[0];
        game.phase = Phase::Rewards(Rewards {
            gold: 0,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        });
        game.parasol = vec![ShopItem::Remove(0), ShopItem::Card(card, 50)];
        let mut reversed = game.clone();
        reversed.parasol.reverse();
        let tokens = state_tokens(&game, &content, layout);
        let parasol = tokens
            .iter()
            .find(|row| {
                row[0] == CONTINUATION_COLLECTION as f32 && row[1] == PARASOL_CARD_KIND as f32
            })
            .unwrap();
        assert_eq!(parasol[2], card.id as f32 + 1.0);
        assert_eq!(parasol[3], 2.0);
        assert_eq!(parasol[4], 0.0);
        assert_ne!(tokens, state_tokens(&reversed, &content, layout));
        assert!(
            tokens
                .iter()
                .filter(|row| row[0] == CARD_COLLECTION as f32)
                .all(|row| row[4] == 0.0 || row[1] == 3.0)
        );
        assert_ne!(
            state_features(&game, layout),
            state_features(&reversed, layout)
        );
        game.step(&content, Action::Leave).unwrap();
        reversed.step(&content, Action::Leave).unwrap();
        assert_eq!(game.parasol.len(), 1);
        assert!(reversed.parasol.is_empty());

        let mut fake = game.clone();
        fake.fake_merchant = ["RELIC.FAKE_ANCHOR", "RELIC.FAKE_MANGO"]
            .into_iter()
            .map(|id| content.relic_id(id).unwrap())
            .collect();
        let state = state_features(&fake, layout);
        fake.fake_merchant.pop();
        assert_ne!(state_features(&fake, layout), state);

        let mut first = game.clone();
        first.reward_gold_parts = vec![10, 30];
        first.phase = Phase::Rewards(Rewards {
            gold: 40,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        });
        let mut second = first.clone();
        second.reward_gold_parts.reverse();
        assert_ne!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        first.step(&content, Action::RewardGold).unwrap();
        second.step(&content, Action::RewardGold).unwrap();
        assert_ne!(first.run.gold, second.run.gold);
        assert_ne!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
    }

    #[test]
    fn enemy_overflow_is_encoded() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 21, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let enemy = game.combat().unwrap().enemies[0].clone();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        while combat.enemies.len() < 2048 {
            let mut enemy = enemy.clone();
            enemy.instance = combat.enemies.len() as u32 + 2;
            combat.enemies.push(enemy);
            combat.hits.push(0);
        }
        combat.enemy_power_snapshot = vec![vec![]; combat.enemies.len()];
        combat.enemy_power_snapshot[ENEMY_SLOTS].push(Power {
            id: power_id::STRENGTH,
            amount: 1,
            skip_duration: false,
            value: 0,
        });
        let state = state_features(&game, layout);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemy_power_snapshot[ENEMY_SLOTS][0].value = 1;
        assert_ne!(state_features(&game, layout), state);
        let state = state_features(&game, layout);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[ENEMY_SLOTS].creature.hp -= 1;
        assert_ne!(state_features(&game, layout), state);

        let combat = game.combat().unwrap();
        let mut eighth = vec![0.0; layout.action_len];
        let mut ninth = eighth.clone();
        action_target(&mut eighth, layout, combat, Some(ENEMY_SLOTS - 1));
        action_target(&mut ninth, layout, combat, Some(ENEMY_SLOTS));
        assert_ne!(eighth, ninth);
    }

    #[test]
    fn toy_box_wax_priority_is_encoded() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 22, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let toy_box = content.relic_id("RELIC.TOY_BOX").unwrap();
        let courier = content.relic_id("RELIC.THE_COURIER").unwrap();
        let membership = content.relic_id("RELIC.MEMBERSHIP_CARD").unwrap();
        first.run.relics = vec![toy_box, courier, membership];
        first.wax_relics = vec![1, 2];
        first.toy_box_combats = 2;
        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        for enemy in &mut combat.enemies {
            enemy.creature.hp = 0;
        }
        let mut second = first.clone();
        second.run.relics.swap(1, 2);
        assert_ne!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        assert_eq!(
            action_features(&first, layout, &Action::EndTurn),
            action_features(&second, layout, &Action::EndTurn)
        );
        first.step(&content, Action::EndTurn).unwrap();
        second.step(&content, Action::EndTurn).unwrap();
        assert_eq!(first.toy_box_combats, 3);
        assert_eq!(second.toy_box_combats, 3);
        assert_eq!(first.run.relics[first.melted_relics[0]], courier);
        assert_eq!(second.run.relics[second.melted_relics[0]], membership);
    }

    #[test]
    fn features_cover_public_combat_state_machines() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 13, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let state = state_features(&game, layout);

        let mut changed = game.clone();
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.osty.max_hp += 1;
        assert_ne!(state_features(&changed, layout), state);

        let mut changed = game.clone();
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.hits[0] += 1;
        assert_ne!(state_features(&changed, layout), state);

        for field in 0..4 {
            let mut changed = game.clone();
            let Phase::Combat(combat) = &mut changed.phase else {
                unreachable!()
            };
            match field {
                0 => combat.burning_sticks = true,
                1 => combat.throwing_axe = true,
                2 => combat.paels_eye = true,
                _ => combat.paels_eye_extra = true,
            }
            assert_ne!(state_features(&changed, layout), state);
        }

        let mut changed = game.clone();
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.paels_legion = 2;
        assert_ne!(state_features(&changed, layout), state);

        for delayed in 0..6 {
            let mut changed = game.clone();
            let Phase::Combat(combat) = &mut changed.phase else {
                unreachable!()
            };
            match delayed {
                0 => combat.nightmares.push((Card::default(), 3)),
                1 => combat.bombs.push((3, 40)),
                2 => combat.automation.push((10, 1)),
                3 => combat.panache.push((5, 10, 0)),
                4 => combat.boulders.push(5),
                _ => {
                    let instance = combat
                        .hand
                        .first()
                        .or(combat.draw.first())
                        .unwrap()
                        .instance;
                    combat.dampened.push((instance, 1));
                }
            }
            assert_ne!(state_features(&changed, layout), state);
        }

        let mut changed = game;
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.history_course = Some(Card::default());
        assert_ne!(state_features(&changed, layout), state);
    }

    #[test]
    fn delayed_combat_order_is_visible() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 14, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let second_card = Card {
            id: 1,
            ..Card::default()
        };

        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.nightmares = vec![(Card::default(), 3), (second_card, 3)];
        let mut second = first.clone();
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.nightmares.reverse();
        assert_ne!(
            state_features(&first, layout),
            state_features(&second, layout)
        );

        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.nightmares.clear();
        combat.bombs = vec![(2, 40), (2, 50)];
        let mut second = first.clone();
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.bombs.reverse();
        assert_ne!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
    }

    #[test]
    fn hidden_second_boss_resamples_between_bosses() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let act = content
            .acts
            .iter()
            .position(|act| act.id == "ACT.THE_GLORY")
            .unwrap() as Id;
        let mut first = Game::new_character_ascension(&content, 11, 0, 10).unwrap();
        first.begin_act(&content, act).unwrap();
        let current = first
            .map
            .nodes
            .iter()
            .position(|node| node.room == Room::Boss && !node.next.is_empty())
            .unwrap();
        first.map.current = Some(current);
        first.room = Room::Boss;
        first.phase = Phase::Map;
        first.bosses_visited = 1;
        let choices = content.acts[act as usize]
            .bosses
            .iter()
            .copied()
            .filter(|boss| Some(*boss) != first.bosses[0])
            .collect::<Vec<_>>();
        assert!(choices.len() >= 2);
        first.bosses[1] = Some(choices[0]);
        let mut second = first.clone();
        second.bosses[1] = Some(choices[1]);
        let public = state_features(&first, layout);
        assert_eq!(state_features(&second, layout), public);
        let mut different_first_boss = first.clone();
        different_first_boss.bosses[0] = Some(choices[0]);
        assert_ne!(state_features(&different_first_boss, layout), public);
        assert_ne!(
            state_tokens(&different_first_boss, &content, layout),
            state_tokens(&first, &content, layout)
        );

        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert_eq!(first.bosses[1], second.bosses[1]);
        assert_eq!(state_features(&first, layout), public);
        assert_eq!(state_features(&second, layout), public);

        let next = first.map.nodes[current].next[0];
        first.step(&content, Action::Path(next)).unwrap();
        second.step(&content, Action::Path(next)).unwrap();
        assert_eq!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
        assert_eq!(first.actions(&content), second.actions(&content));
    }

    #[test]
    fn lazy_encounters_match_the_constrained_bag_generator() {
        fn generate(content: &Content, groups: &[(&[Id], usize)], mut rng: Rng) -> Vec<Id> {
            let mut result = Vec::new();
            for &(candidates, count) in groups {
                let mut bag = candidates.to_vec();
                bag.sort_unstable();
                for _ in 0..count {
                    if bag.is_empty() {
                        bag.extend_from_slice(candidates);
                        bag.sort_unstable();
                    }
                    let previous = result.last().copied();
                    let allowed = |id: Id| {
                        previous != Some(id)
                            && previous.is_none_or(|previous| {
                                encounter_tags(content.encounters[id as usize].id)
                                    & encounter_tags(content.encounters[previous as usize].id)
                                    == 0
                            })
                    };
                    let constrained = bag.iter().copied().any(allowed);
                    let index = loop {
                        let index = (rng.double() * bag.len() as f64) as usize;
                        if !constrained || allowed(bag[index]) {
                            break index;
                        }
                    };
                    result.push(bag.remove(index));
                }
            }
            result
        }

        let content = foundation_content();
        let act = content
            .acts
            .iter()
            .position(|act| {
                act.encounters
                    .iter()
                    .any(|id| content.encounters[*id as usize].id.contains("_WEAK"))
                    && act
                        .encounters
                        .iter()
                        .any(|id| !content.encounters[*id as usize].id.contains("_WEAK"))
                    && !act.elites.is_empty()
            })
            .unwrap() as Id;
        let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
        game.begin_act(&content, act).unwrap();
        let weak = content.acts[act as usize]
            .encounters
            .iter()
            .copied()
            .filter(|id| content.encounters[*id as usize].id.contains("_WEAK"))
            .collect::<Vec<_>>();
        let regular = content.acts[act as usize]
            .encounters
            .iter()
            .copied()
            .filter(|id| !content.encounters[*id as usize].id.contains("_WEAK"))
            .collect::<Vec<_>>();
        let counts = (
            game.weak_encounters_left as usize,
            game.regular_encounters_left as usize,
            game.elite_encounters_left as usize,
        );
        let seed = 91;
        let expected = generate(
            &content,
            &[(&weak, counts.0), (&regular, counts.1)],
            Rng::from_seed(seed),
        );
        game.rngs.up_front = Rng::from_seed(seed);
        let actual = (0..expected.len())
            .map(|_| game.next_encounter(&content, false).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);

        let expected = generate(
            &content,
            &[(content.acts[act as usize].elites, counts.2)],
            Rng::from_seed(seed),
        );
        game.rngs.up_front = Rng::from_seed(seed);
        let actual = (0..expected.len())
            .map(|_| game.next_encounter(&content, true).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn encounter_bags_are_public_and_resample_bisimilarly() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let act = content
            .acts
            .iter()
            .position(|act| {
                act.encounters
                    .iter()
                    .filter(|id| content.encounters[**id as usize].id.contains("_WEAK"))
                    .count()
                    > 1
                    && act.elites.len() > 2
            })
            .unwrap() as Id;
        let mut game = Game::new_character_ascension(&content, 2, 0, 10).unwrap();
        game.begin_act(&content, act).unwrap();
        game.next_encounter(&content, false).unwrap();
        game.next_encounter(&content, true).unwrap();
        game.map = Map {
            nodes: vec![
                MapNode {
                    floor: 1,
                    lane: 0,
                    room: Room::Combat,
                    next: vec![],
                },
                MapNode {
                    floor: 1,
                    lane: 1,
                    room: Room::Elite,
                    next: vec![],
                },
            ],
            current: None,
        };
        game.phase = Phase::Map;
        let state = state_features(&game, layout);
        let mut changed = game.clone();
        if changed.weak_encounters_left > 0 {
            changed.weak_encounters_left -= 1;
        } else {
            changed.regular_encounters_left -= 1;
        }
        assert_ne!(state_features(&changed, layout), state);
        let mut changed = game.clone();
        changed.encounters.pop();
        assert_ne!(state_features(&changed, layout), state);
        let mut changed = game.clone();
        changed.last_elite = None;
        assert_ne!(state_features(&changed, layout), state);
        let mut changed = game.clone();
        changed.act = (changed.act + 1) % layout.acts as Id;
        assert_ne!(state_features(&changed, layout), state);

        let mut reordered = game.clone();
        reordered.encounters.reverse();
        reordered.elites.reverse();
        reordered.rngs.up_front.next();
        assert_eq!(state_features(&reordered, layout), state);
        for seed in 1..=32 {
            let mut first = game.clone();
            let mut second = reordered.clone();
            resample_hidden(&mut first, &content, seed);
            resample_hidden(&mut second, &content, seed);
            let actions = first.actions(&content);
            assert_eq!(second.actions(&content), actions);
            for action in actions {
                let mut left = first.clone();
                let mut right = second.clone();
                left.step(&content, action.clone()).unwrap();
                right.step(&content, action).unwrap();
                assert_eq!(
                    state_features(&left, layout),
                    state_features(&right, layout)
                );
                assert_eq!(left.actions(&content), right.actions(&content));
            }
        }
    }

    #[test]
    fn encounter_bag_enforces_public_repeat_constraints() {
        let content = foundation_content();
        let (act, previous, allowed) = content
            .acts
            .iter()
            .enumerate()
            .find_map(|(act, def)| {
                def.encounters.iter().find_map(|&previous| {
                    def.encounters
                        .iter()
                        .copied()
                        .find(|&next| {
                            previous != next
                                && encounter_tags(content.encounters[previous as usize].id)
                                    & encounter_tags(content.encounters[next as usize].id)
                                    == 0
                        })
                        .map(|next| (act as Id, previous, next))
                })
            })
            .unwrap();
        let mut game = Game::new_character_ascension(&content, 2, 0, 10).unwrap();
        game.begin_act(&content, act).unwrap();
        game.weak_encounters_left = 1;
        game.regular_encounters_left = 0;
        game.last_encounter = Some(previous);
        game.encounters = vec![previous, allowed];
        for seed in 1..=32 {
            let mut sampled = game.clone();
            sampled.rngs.up_front = Rng::from_seed(seed);
            assert_eq!(sampled.next_encounter(&content, false), Some(allowed));
        }
        game.encounters = vec![previous];
        assert_eq!(game.next_encounter(&content, false), Some(previous));
    }

    #[test]
    fn replay_keeps_its_forced_encounter() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 3, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.replaying = true;
        game.encounters = vec![content.acts[0].encounters[0]];
        game.map = Map {
            nodes: vec![MapNode {
                floor: 1,
                lane: 0,
                room: Room::Combat,
                next: vec![],
            }],
            current: None,
        };
        resample_hidden(&mut game, &content, 9);
        game.step(&content, Action::Path(0)).unwrap();
        assert!(game.encounters.is_empty());
    }

    #[test]
    fn direct_event_combat_does_not_consume_encounter_bags() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 3, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let before = (
            game.weak_encounters_left,
            game.regular_encounters_left,
            game.elite_encounters_left,
            game.encounters.clone(),
            game.elites.clone(),
        );
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        assert_eq!(
            before,
            (
                game.weak_encounters_left,
                game.regular_encounters_left,
                game.elite_encounters_left,
                game.encounters,
                game.elites,
            )
        );
    }

    #[test]
    fn features_cover_visible_event_options() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 12, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.phase = Phase::Event(
            0,
            vec![EventOption {
                requirement: Requirement::Gold(10),
                effects: &[RunEffect::Heal(5)],
            }],
        );
        let state = state_features(&game, layout);
        let action = action_features(&game, layout, &Action::Event(0));
        game.phase = Phase::Event(
            0,
            vec![EventOption {
                requirement: Requirement::Hp(10),
                effects: &[RunEffect::LoseHp(5)],
            }],
        );
        assert_ne!(state_features(&game, layout), state);
        assert_ne!(action_features(&game, layout, &Action::Event(0)), action);
    }

    #[test]
    fn features_cover_public_event_state_without_instance_ids() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 16, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let event = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.JUNGLE_MAZE_ADVENTURE")
            .unwrap() as Id;
        game.phase = Phase::Event(event, vec![]);
        let state = state_features(&game, layout);
        game.event_data[0] = 150;
        assert_ne!(state_features(&game, layout), state);

        let ranwid = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.RANWID_THE_ELDER")
            .unwrap() as Id;
        game.phase = Phase::Event(ranwid, vec![]);
        game.event_data = [0; 4];
        game.event_cards.clear();
        let state = state_features(&game, layout);
        game.event_cards.push(0);
        assert_ne!(state_features(&game, layout), state);

        let event = layout.slippery_bridge.unwrap();
        game.phase = Phase::Event(event, vec![]);
        let first = game.run.deck[0].instance;
        let other = game
            .run
            .deck
            .iter()
            .find(|card| card.id != game.run.deck[0].id)
            .unwrap()
            .instance;
        game.event_data = [2, first as i64, 0, 0];
        game.event_cards = vec![other];
        let state = state_features(&game, layout);
        let tokens = state_tokens(&game, &content, layout);
        let action = tokenized_action(&game, &content, layout, &Action::Event(0));
        for card in &mut game.run.deck {
            card.instance += 100;
        }
        game.event_data[1] += 100;
        game.event_cards[0] += 100;
        assert_eq!(state_features(&game, layout), state);
        assert_eq!(state_tokens(&game, &content, layout), tokens);
        game.event_data[1] = game
            .run
            .deck
            .iter()
            .find(|card| card.id != game.run.deck[0].id)
            .unwrap()
            .instance as i64;
        assert_eq!(state_features(&game, layout), state);
        assert_eq!(state_tokens(&game, &content, layout), tokens);
        assert_ne!(
            tokenized_action(&game, &content, layout, &Action::Event(0)),
            action
        );
    }

    #[test]
    fn hidden_event_rolls_are_not_features() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        for (name, action) in [
            ("EVENT.STONE_OF_ALL_TIME", 0),
            ("EVENT.THE_FUTURE_OF_POTIONS", 1),
        ] {
            let id = content
                .events
                .iter()
                .position(|event| event.id == name)
                .unwrap() as Id;
            let mut first = Game::new_character_ascension(&content, 16, 0, 10).unwrap();
            first.begin_act(&content, 0).unwrap();
            first.run.potions = vec![
                Some(crate::foundation::UNCOMMON_POTIONS[0]),
                Some(crate::foundation::UNCOMMON_POTIONS[1]),
                None,
            ];
            first.phase = Phase::Event(id, content.events[id as usize].options.to_vec());
            first.event_rng = Some(Rng::from_seed(42));
            first.event_data = [1; 4];
            let mut second = first.clone();
            second.event_data = [3; 4];
            assert_eq!(
                state_features(&first, layout),
                state_features(&second, layout)
            );
            assert_eq!(
                state_tokens(&first, &content, layout),
                state_tokens(&second, &content, layout)
            );
            first.step(&content, Action::Event(action)).unwrap();
            second.step(&content, Action::Event(action)).unwrap();
            assert_eq!(
                state_tokens(&first, &content, layout),
                state_tokens(&second, &content, layout)
            );
            assert_eq!(first.actions(&content), second.actions(&content));
        }
    }

    #[test]
    fn features_cover_reward_reroll_state() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 12, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.run
            .relics
            .push(content.relic_id("RELIC.DRIFTWOOD").unwrap());
        game.phase = Phase::Rewards(Rewards {
            gold: 0,
            cards: vec![Card::default()],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        });
        let state = state_features(&game, layout);
        assert!(game.actions(&content).contains(&Action::RerollCards));
        game.rerolled_cards = true;
        assert_ne!(state_features(&game, layout), state);
        assert!(!game.actions(&content).contains(&Action::RerollCards));
    }

    #[test]
    fn lazy_card_rewards_resample_to_equal_transitions() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let paels_wing = content.relic_id("RELIC.PAELS_WING").unwrap();
        let rewards = [
            CardReward::Standard(Room::Combat),
            CardReward::Fixed(0, CardRarity::Common),
            CardReward::Kaleidoscope,
            CardReward::Crystal(CardRarity::Common),
        ];
        for reward in rewards {
            let mut game = Game::new_character_ascension(&content, 19, 0, 10).unwrap();
            game.begin_act(&content, 0).unwrap();
            game.run.relics.push(paels_wing);
            game.event_rng = Some(Rng::from_seed(1));
            game.phase = Phase::Rewards(Rewards {
                gold: 0,
                cards: vec![Card::default()],
                card_rewards: vec![reward],
                relics: vec![],
                potions: vec![],
                removals: 0,
            });
            let public = state_features(&game, layout);
            let mut no_queue = game.clone();
            if let Phase::Rewards(rewards) = &mut no_queue.phase {
                rewards.card_rewards.clear();
            }
            assert_ne!(state_features(&no_queue, layout), public);

            for action in [
                Action::RewardCard(0),
                Action::SacrificeCards,
                Action::Cancel,
            ] {
                let mut first = game.clone();
                let mut second = game.clone();
                second.rngs.rewards.next();
                second.rngs.niche.next();
                second.event_rng.as_mut().unwrap().next();
                assert_eq!(
                    state_features(&first, layout),
                    state_features(&second, layout)
                );
                resample_hidden(&mut first, &content, 42);
                resample_hidden(&mut second, &content, 42);
                first.step(&content, action.clone()).unwrap();
                second.step(&content, action).unwrap();
                assert_eq!(
                    state_features(&first, layout),
                    state_features(&second, layout)
                );
                assert_eq!(first.actions(&content), second.actions(&content));
                let Phase::Rewards(rewards) = &first.phase else {
                    panic!("queued card reward did not remain in rewards")
                };
                assert!(rewards.card_rewards.is_empty());
                assert!(!rewards.cards.is_empty());
            }
        }
    }

    #[test]
    fn features_cover_tinker_time_visible_offers() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 12, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let event = layout.tinker_time.unwrap();
        game.phase = Phase::Event(
            event,
            vec![
                EventOption {
                    requirement: Requirement::Always,
                    effects: &[RunEffect::EventAction(0)],
                },
                EventOption {
                    requirement: Requirement::Always,
                    effects: &[RunEffect::EventAction(1)],
                },
            ],
        );
        game.event_data = [3, 1, 0, -1];
        let types = state_features(&game, layout);
        let type_actions = [
            action_features(&game, layout, &Action::Event(0)),
            action_features(&game, layout, &Action::Event(1)),
        ];
        game.event_data = [3, 1, 0, 1];
        assert_ne!(state_features(&game, layout), types);
        assert_eq!(
            [
                action_features(&game, layout, &Action::Event(0)),
                action_features(&game, layout, &Action::Event(1)),
            ],
            type_actions
        );
        game.event_data = [5, 4, 0, 2];
        let riders = state_features(&game, layout);
        let rider_actions = [
            action_features(&game, layout, &Action::Event(0)),
            action_features(&game, layout, &Action::Event(1)),
        ];
        assert_ne!(riders, types);
        assert_ne!(rider_actions, type_actions);
        game.event_data[2] = 9;
        game.event_rng = Some(Rng::from_seed(99));
        assert_eq!(state_features(&game, layout), riders);
        assert_eq!(
            action_features(&game, layout, &Action::Event(0)),
            rider_actions[0]
        );
        game.event_data.swap(0, 1);
        assert_eq!(state_features(&game, layout), riders);
        assert_eq!(
            action_features(&game, layout, &Action::Event(0))[layout.action_value],
            rider_actions[1][layout.action_value]
        );
        assert_eq!(
            action_features(&game, layout, &Action::Event(1))[layout.action_value],
            rider_actions[0][layout.action_value]
        );
    }

    #[test]
    fn potential_has_a_common_terminal_value() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 12, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.phase = Phase::Won;
        assert_eq!(potential(&game), 0.0);
        game.phase = Phase::Dead;
        assert_eq!(potential(&game), 0.0);
    }

    #[test]
    fn content_fingerprint_tracks_gameplay_fields() {
        let content = foundation_content();
        let mut changed = foundation_content();
        assert_eq!(content_fingerprint(&content), content_fingerprint(&changed));
        assert_eq!(content.cards[0].id, changed.cards[0].id);
        changed.cards[0].cost[0] += 1;
        assert_ne!(content_fingerprint(&content), content_fingerprint(&changed));
    }

    #[test]
    fn checkpoint_loads_and_terminals_are_exact() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let (width, layers, action_layers, heads, feedforward) =
            (4usize, 1usize, 1usize, 1usize, 8usize);
        let mut bytes = Vec::new();
        bytes.extend(MAGIC);
        bytes.extend(VALUE_MODEL_VERSION.to_le_bytes());
        bytes.extend(VERSION.to_le_bytes());
        bytes.extend(content_fingerprint(&content).to_le_bytes());
        for value in [
            width,
            layers,
            action_layers,
            heads,
            feedforward,
            layout.characters,
            ACTION_CARD_COLLECTION + 1,
            TOKEN_CATEGORICAL,
            TOKEN_NUMERIC,
            TOKEN_ACTION_VALUES,
        ] {
            bytes.extend((value as u32).to_le_bytes());
        }
        bytes.extend(1f32.to_le_bytes());
        bytes.extend(0f32.to_le_bytes());
        let vocab = [
            ACTION_CARD_COLLECTION + 1,
            32,
            65537,
            65537,
            65537,
            7,
            ENCHANTMENTS + 1,
            4,
        ];
        let embedding_rows: usize = vocab.iter().map(|size| (*size).min(257)).sum::<usize>()
            + vocab
                .iter()
                .map(|size| ((*size - 1).div_ceil(256) + 1).max(2))
                .sum::<usize>();
        let layer = 3 * width * width
            + 3 * width
            + width * width
            + width
            + 4 * width
            + 2 * feedforward * width
            + feedforward
            + width;
        let floats = embedding_rows * width
            + width * TOKEN_NUMERIC
            + 2 * width
            + layers * layer
            + action_layers * layer
            + width * TOKEN_ACTION_VALUES
            + 5 * width
            + width * width
            + 2 * width
            + 1;
        bytes.resize(bytes.len() + floats * 4, 0);
        let path = std::env::temp_dir().join(format!("sts2-value-{}.bin", std::process::id()));
        fs::write(&path, bytes).unwrap();
        let mut changed = foundation_content();
        changed.cards[0].cost[0] += 1;
        assert!(ValueModel::load(&path, &changed).is_err());
        let model = ValueModel::load(&path, &content).unwrap();
        fs::remove_file(path).unwrap();
        let mut game = Game::new_character_ascension(&content, 13, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        assert_eq!(model.win_probability(&game), 0.5);
        game.phase = Phase::Won;
        assert_eq!(model.win_probability(&game), 1.0);
        game.phase = Phase::Dead;
        assert_eq!(model.win_probability(&game), 0.0);
    }

    #[test]
    fn cancel_never_stalls_rewards_or_rest() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 17, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.phase = Phase::Rewards(Rewards {
            gold: 0,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        });
        assert!(!game.actions(&content).contains(&Action::Cancel));
        game.phase = Phase::Rest;
        game.step(&content, Action::Cancel).unwrap();
        assert!(matches!(game.phase, Phase::Map));
    }

    #[test]
    fn empty_crystal_clears_resume() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 19, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.crystal = Some(CrystalSphere {
            cells: vec![None; 121],
            clear: vec![false; 121],
            items: vec![],
            revealed: vec![],
            remaining: 1,
            big: false,
        });
        game.resume = Some(Phase::Map);
        game.phase = Phase::Event(0, vec![]);
        game.step(&content, Action::CrystalCell(0, 0)).unwrap();
        assert!(matches!(game.phase, Phase::Map));
        assert!(game.resume.is_none());
    }

    #[cfg(feature = "python")]
    #[test]
    fn room_frontier_preserves_pareto_routes_and_engines() {
        let content = foundation_content();
        let mut base = Game::new_character_ascension(&content, 21, 4, 10).unwrap();
        base.begin_act(&content, 0).unwrap();
        base.map.current = Some(0);
        base.run.hp = 50;
        let mut dominated = base.clone();
        dominated.run.hp = 49;
        let mut route = base.clone();
        route.map.current = Some(1);
        route.run.hp = 30;
        let mut engine = base.clone();
        engine.run.hp = 45;
        engine.run.deck.push(Card {
            id: content
                .cards
                .iter()
                .position(|card| card.id == "CARD.COUNTDOWN")
                .unwrap() as Id,
            ..Card::default()
        });
        let candidates = vec![(base, 0), (dominated, 1), (route, 2), (engine, 3)];
        let mut pareto = candidates.clone();
        python::retain_room_frontier(&mut pareto, &content, 4, 4);
        assert_eq!(pareto.len(), 3);
        assert!(!pareto.iter().any(|(_, id)| *id == 1));
        assert!(pareto.iter().any(|(_, id)| *id == 3));
        python::retain_room_frontier(&mut pareto, &content, 1, 2);
        assert!(pareto.iter().any(|(_, id)| *id == 2));
    }

    #[cfg(feature = "python")]
    #[test]
    fn search_keeps_only_mandatory_potion_discards() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 22, 3, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.run.potions[0] = Some(0);
        let actions = python::search_actions(&game, &content);
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::Path(_)))
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::DiscardPotion(_)))
        );
        game.replacing_potion = true;
        let actions = python::search_actions(&game, &content);
        assert!(!actions.is_empty());
        assert!(
            actions
                .iter()
                .all(|action| matches!(action, Action::DiscardPotion(_)))
        );
    }

    #[cfg(feature = "python")]
    #[test]
    fn combat_search_selects_best_clear_in_layer() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 23, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        let encounter = content
            .encounters
            .iter()
            .position(|encounter| encounter.id == "ENCOUNTER.SLIMES_WEAK")
            .unwrap() as Id;
        game.start_combat(&content, encounter).unwrap();
        let strike = content
            .cards
            .iter()
            .position(|card| card.id == "CARD.STRIKE_IRONCLAD")
            .unwrap() as Id;
        let feed = content
            .cards
            .iter()
            .position(|card| card.id == "CARD.FEED")
            .unwrap() as Id;
        let before = game.run.max_hp;
        let Phase::Combat(combat) = &mut game.phase else {
            panic!("combat did not start");
        };
        combat.hand = vec![
            Card {
                id: strike,
                ..Card::default()
            },
            Card {
                id: feed,
                ..Card::default()
            },
        ];
        combat.draw.clear();
        combat.discard.clear();
        combat.exhaust.clear();
        combat.energy = 3;
        for enemy in &mut combat.enemies {
            enemy.creature.hp = 0;
            enemy.creature.powers.clear();
        }
        combat.enemies[0].creature.hp = 1;
        let (result, plan) = python::combat_search(&game, &content, 8, 1);
        assert!(matches!(plan[0], Action::Play { hand: 1, .. }));
        assert!(result.run.max_hp > before);

        let blood = content
            .potions
            .iter()
            .position(|potion| potion.id == "POTION.BLOOD_POTION")
            .unwrap() as Id;
        game.run.potions[0] = Some(blood);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!();
        };
        combat.hand.truncate(1);
        combat.player.hp = 40;
        let (result, plan) = python::combat_search(&game, &content, 8, 6);
        assert!(matches!(plan[0], Action::Potion { slot: 0, .. }));
        assert!(result.run.hp > 40);
    }

    #[test]
    fn draw_choice_actions_keep_known_position() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 31, 3, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let source = content.card_id("CARD.WITHER").unwrap();
        let target = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.draw = (0..29)
            .map(|instance| Card {
                id: source,
                instance,
                value: 3,
                ..Card::default()
            })
            .collect();
        combat.known_draw_bottom = 0;
        combat.known_draw_top = 1;
        combat.choice = Some(Choice {
            pile: Pile::Draw,
            filter: CardFilter::Any,
            op: CardOp::Transform(target, 1),
            remaining: 2,
            optional: false,
        });

        let hidden = action_features(&game, layout, &Action::Choose(16));
        assert_eq!(hidden, action_features(&game, layout, &Action::Choose(17)));
        let mut first = game.clone();
        let mut second = game.clone();
        first.step(&content, Action::Choose(16)).unwrap();
        second.step(&content, Action::Choose(17)).unwrap();
        assert_eq!(
            state_features(&first, layout),
            state_features(&second, layout)
        );

        let mut known = action_features(&game, layout, &Action::Choose(28));
        assert_ne!(hidden, known);
        known[layout.action_value] = 0.0;
        assert_eq!(hidden, known);
        second = game;
        second.step(&content, Action::Choose(28)).unwrap();
        assert_ne!(
            state_features(&first, layout),
            state_features(&second, layout)
        );
    }

    #[test]
    fn tokens_use_distinct_encounter_vocabularies_and_keep_history() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let vocab = encounter_vocabs(&content);
        assert_eq!(vocab.iter().map(Vec::len).collect::<Vec<_>>(), [57, 13, 13]);
        let mut game = Game::new_character_ascension(&content, 41, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.last_encounter = Some(vocab[0][20]);
        game.last_elite = Some(vocab[1][7]);
        game.bosses[0] = Some(vocab[2][9]);
        let tokens = state_tokens(&game, &content, layout);
        assert!(tokens.contains(&token(ENCOUNTER_COLLECTION, 3, 21, 0, 0)));
        assert!(tokens.contains(&token(ENCOUNTER_COLLECTION, 4, 8, 0, 0)));
        assert!(tokens.contains(&token(ENCOUNTER_COLLECTION, 0, 10, 0, 0)));
    }

    #[test]
    fn tokens_canonicalize_unordered_public_bags() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 42, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        let mut second = first.clone();
        second.encounters.reverse();
        second.elites.reverse();
        for bags in [&mut second.relic_deques, &mut second.shared_relic_deques] {
            bags.iter_mut().for_each(|bag| bag.reverse());
        }
        second
            .map
            .nodes
            .iter_mut()
            .for_each(|node| node.next.reverse());
        let nodes = second.map.nodes.len();
        second.map.nodes.reverse();
        second.map.nodes.iter_mut().for_each(|node| {
            node.next
                .iter_mut()
                .for_each(|next| *next = nodes - 1 - *next)
        });
        second.map.current = second.map.current.map(|current| nodes - 1 - current);
        assert_eq!(
            state_tokens(&first, &content, layout),
            state_tokens(&second, &content, layout)
        );
        assert_eq!(
            global_features(&first, layout),
            global_features(&second, layout)
        );
        second.last_encounter = first.encounters.first().copied();
        assert_ne!(
            state_tokens(&first, &content, layout),
            state_tokens(&second, &content, layout)
        );
    }

    #[test]
    fn tokens_have_no_enemy_or_continuation_cap() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 43, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        let enemy = combat.enemies[0].clone();
        combat.enemies.resize(2048, enemy);
        combat.enemy_power_snapshot.resize(2048, vec![]);
        let tokens = state_tokens(&game, &content, layout);
        assert_eq!(
            tokens
                .iter()
                .filter(|row| row[0] == ENEMY_COLLECTION as f32 && row[1] == 0.0)
                .count(),
            2048
        );
        let mut changed = game.clone();
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.enemies[1500].creature.hp -= 1;
        assert_ne!(tokens, state_tokens(&changed, &content, layout));
        game.run_queue = vec![RunEffect::Gold(1); 3000];
        assert!(continuation(&game).len() > CONTINUATION_BYTES);
        assert!(state_tokens(&game, &content, layout).len() > 3000);
        assert_eq!(observation_metadata(&game, layout).len(), layout.characters);
    }

    #[test]
    fn tokens_track_relic_potion_map_crystal_and_delayed_state() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 44, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let base = state_tokens(&game, &content, layout);
        game.run.potions[0] = Some(0);
        assert_ne!(state_tokens(&game, &content, layout), base);
        let base = state_tokens(&game, &content, layout);
        game.map.nodes[0].room = Room::Elite;
        assert_ne!(state_tokens(&game, &content, layout), base);
        game.crystal = Some(CrystalSphere {
            cells: vec![None; 121],
            clear: vec![false; 121],
            items: vec![],
            revealed: vec![],
            remaining: 1,
            big: false,
        });
        let base = state_tokens(&game, &content, layout);
        game.crystal.as_mut().unwrap().clear[60] = true;
        assert_ne!(state_tokens(&game, &content, layout), base);
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let base = state_tokens(&game, &content, layout);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.bombs.push((2, 17));
        assert_ne!(state_tokens(&game, &content, layout), base);
    }

    #[test]
    fn next_wriggler_parity_is_public_without_enemy_instances() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 46, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let enemy = first.combat().unwrap().enemies[0].clone();
        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.enemies = vec![enemy.clone(), enemy];
        combat.enemies[0].instance = 1;
        combat.enemies[1].instance = 2;
        combat.hits = vec![0; 2];
        combat.enemy_power_snapshot = vec![vec![]; 2];
        let mut second = first.clone();
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.enemies[1].instance = 3;

        let without_next = |tokens: Vec<Token>| {
            tokens
                .into_iter()
                .filter(|row| row[..2] != [ENEMY_COLLECTION as f32, 2.0])
                .collect::<Vec<_>>()
        };
        assert_eq!(
            without_next(state_tokens(&first, &content, layout)),
            without_next(state_tokens(&second, &content, layout))
        );
        assert_eq!(
            global_features(&first, layout),
            global_features(&second, layout)
        );
        assert_ne!(
            state_tokens(&first, &content, layout),
            state_tokens(&second, &content, layout)
        );

        first.spawn_wrigglers(&content, 1);
        second.spawn_wrigglers(&content, 1);
        assert_ne!(
            state_tokens(&first, &content, layout),
            state_tokens(&second, &content, layout)
        );
        let first = first.combat().unwrap().enemies.last().unwrap().instance;
        let second = second.combat().unwrap().enemies.last().unwrap().instance;
        assert_ne!((first - 1) % 2, (second - 1) % 2);
    }

    #[test]
    fn action_tokens_align_with_legal_actions() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 45, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        for action in represented_actions(&game, &content).1 {
            let mut values = compact_action_values(&game, layout, &action);
            let mut tokens = action_entity_tokens(&game, &content, layout, &action);
            action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
            assert_eq!(values[action_kind(&action)], 1.0);
            assert!(values.iter().all(|value| value.is_finite()));
            assert!(tokens.iter().flatten().all(|value| value.is_finite()));
            if matches!(action, Action::Path(_)) {
                assert!(tokens.iter().any(|row| row[0] == MAP_COLLECTION as f32));
            }
        }
    }

    #[test]
    fn action_set_carries_unavailable_shop_offers() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 46, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let id = (0..content.cards.len())
            .find(|id| !game.run.deck.iter().any(|card| card.id as usize == *id))
            .unwrap() as Id;
        game.run.gold = 0;
        game.phase = Phase::Shop(vec![ShopItem::Card(
            Card {
                id,
                ..Card::default()
            },
            100,
        )]);
        let (legal, represented) = represented_actions(&game, &content);
        assert!(!legal.contains(&Action::Buy(0)));
        assert!(represented.contains(&Action::Buy(0)));
        assert!(
            !state_tokens(&game, &content, layout)
                .iter()
                .any(|row| { row[0] == CARD_COLLECTION as f32 && row[2] == id as f32 + 1.0 })
        );
        assert!(
            tokenized_action(&game, &content, layout, &Action::Buy(0))
                .1
                .iter()
                .any(|row| {
                    row[0] == ACTION_CARD_COLLECTION as f32 && row[2] == id as f32 + 1.0
                })
        );
    }

    #[test]
    fn v36_orb_previews_match_dualcast_overflow_and_zero_slots() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 71, 2, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics.clear();
        let dualcast = Card {
            id: content.card_id("CARD.DUALCAST").unwrap(),
            ..Card::default()
        };
        let zap = Card {
            id: content.card_id("CARD.ZAP").unwrap(),
            ..Card::default()
        };
        let lightning = content
            .orbs
            .iter()
            .position(|orb| orb.id == "ORB.LIGHTNING_ORB")
            .unwrap() as Id;
        let dark = content
            .orbs
            .iter()
            .position(|orb| orb.id == "ORB.DARK_ORB")
            .unwrap() as Id;
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.enemies[0].creature.max_hp = 100;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        combat.player.powers.clear();
        combat.energy = 3;
        combat.hand = vec![dualcast];
        combat.orb_slots = 1;
        combat.orbs = vec![Orb {
            id: lightning,
            value: 0,
        }];
        let preview = played_card_preview(&game, &content, dualcast, None);
        assert_eq!(
            resolved_hits(&game, &content, 0, None, false, true, &preview.hits).2,
            16
        );
        assert!(preview.orbs.is_empty());
        let mut played = game.clone();
        played
            .step(
                &content,
                Action::Play {
                    hand: 0,
                    target: None,
                },
            )
            .unwrap();
        assert_eq!(played.combat().unwrap().enemies[0].creature.hp, 84);

        for (slots, expected) in [(1, 11), (0, 0)] {
            let mut game = game.clone();
            let Phase::Combat(combat) = &mut game.phase else {
                unreachable!()
            };
            combat.hand = vec![zap];
            combat.orb_slots = slots;
            combat.orbs = (slots > 0)
                .then_some(Orb {
                    id: dark,
                    value: 11,
                })
                .into_iter()
                .collect();
            let preview = played_card_preview(&game, &content, zap, None);
            assert_eq!(
                resolved_hits(&game, &content, 0, None, false, false, &preview.hits).2,
                expected
            );
            assert_eq!((preview.orb_slots, preview.orbs.len()), (1, 1));
            let mut played = game.clone();
            played
                .step(
                    &content,
                    Action::Play {
                        hand: 0,
                        target: None,
                    },
                )
                .unwrap();
            let combat = played.combat().unwrap();
            assert_eq!(combat.enemies[0].creature.hp, 100 - expected);
            assert_eq!((combat.orb_slots, combat.orbs.len()), (1, 1));
        }
    }

    #[test]
    fn v36_fisticuffs_and_misery_previews_match_play() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 72, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics.clear();
        let fisticuffs = Card {
            id: content.card_id("CARD.FISTICUFFS").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.enemies[0].creature.max_hp = 100;
        combat.enemies[0].creature.block = 3;
        combat.enemies[0].creature.powers = vec![Power {
            id: power_id::VULNERABLE,
            amount: 1,
            skip_duration: false,
            value: 0,
        }];
        combat.hand = vec![fisticuffs];
        combat.energy = 3;
        combat.player.block = 0;
        combat.player.powers = [
            (power_id::STRENGTH, 2),
            (power_id::DEXTERITY, 3),
            (power_id::FRAIL, 1),
        ]
        .map(|(id, amount)| Power {
            id,
            amount,
            skip_duration: false,
            value: 0,
        })
        .to_vec();
        let preview = played_card_preview(&game, &content, fisticuffs, Some(0));
        assert_eq!(preview.block, 12);
        assert_eq!(
            resolved_hits(&game, &content, 0, Some(0), false, false, &preview.hits).2,
            10
        );
        let mut played = game.clone();
        played
            .step(
                &content,
                Action::Play {
                    hand: 0,
                    target: Some(0),
                },
            )
            .unwrap();
        assert_eq!(played.combat().unwrap().player.block, 12);
        assert_eq!(played.combat().unwrap().enemies[0].creature.hp, 90);

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers.push(Power {
            id: power_id::NO_BLOCK,
            amount: 1,
            skip_duration: false,
            value: 0,
        });
        assert_eq!(
            played_card_preview(&game, &content, fisticuffs, Some(0)).block,
            0
        );

        let misery = Card {
            id: content.card_id("CARD.MISERY").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![misery];
        combat.player.powers.clear();
        combat.enemies[0].creature.block = 0;
        let preview = played_card_preview(&game, &content, misery, Some(0));
        let damage = resolved_hits(&game, &content, 0, Some(0), false, false, &preview.hits).2;
        let mut played = game.clone();
        played
            .step(
                &content,
                Action::Play {
                    hand: 0,
                    target: Some(0),
                },
            )
            .unwrap();
        assert_eq!(
            100 - played.combat().unwrap().enemies[0].creature.hp,
            damage
        );
    }

    #[test]
    fn v36_duplicate_hand_exhaust_previews_match_play() {
        let mut content = foundation_content();
        let id = content.card_id("CARD.ZAP").unwrap();
        for (seed, effect) in [
            (
                73,
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Exhaust),
                ),
            ),
            (74, Effect::ExhaustForBlock(Amount::fixed(5, 5))),
        ] {
            content.cards[id as usize].effects = Box::leak(vec![effect].into_boxed_slice());
            let mut game = Game::new_character_ascension(&content, seed, 2, 10).unwrap();
            game.begin_act(&content, 0).unwrap();
            game.start_combat(&content, content.acts[0].encounters[0])
                .unwrap();
            game.run.relics.clear();
            let card = Card {
                id,
                ..Card::default()
            };
            let Phase::Combat(combat) = &mut game.phase else {
                unreachable!()
            };
            combat.hand = vec![card, card];
            combat.energy = 3;
            combat.player.block = 0;
            combat.player.powers.clear();
            let preview = played_card_preview(&game, &content, card, None);
            assert_eq!(preview.exhaust, 1);
            let mut played = game.clone();
            played
                .step(
                    &content,
                    Action::Play {
                        hand: 0,
                        target: None,
                    },
                )
                .unwrap();
            assert_eq!(played.combat().unwrap().exhaust.len(), 1);
            assert_eq!(played.combat().unwrap().player.block, preview.block);
        }
    }

    #[test]
    fn v36_play_draw_and_exhaust_hooks_match_play() {
        let mut content = foundation_content();
        let id = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        content.cards[id as usize].effects = Box::leak(Box::new([
            Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 6), 1),
            Effect::Draw(1),
        ]));
        content.cards[id as usize].flags = [EXHAUST; 2];
        let ids = [
            "POWER.FEEL_NO_PAIN_POWER",
            "POWER.DARK_EMBRACE_POWER",
            "POWER.JUGGERNAUT_POWER",
            "POWER.AFTERIMAGE_POWER",
            "POWER.SERPENT_FORM_POWER",
            "POWER.SPEEDSTER_POWER",
            "POWER.ITERATION_POWER",
        ]
        .map(|name| {
            content
                .powers
                .iter()
                .position(|power| power.id == name)
                .unwrap() as Id
        });
        let mut game = Game::new_character_ascension(&content, 75, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics = vec![content.relic_id("RELIC.CHARONS_ASHES").unwrap()];
        let card = Card {
            id,
            ..Card::default()
        };
        let filler = Card {
            id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let status = Card {
            id: content.card_id("CARD.DAZED").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.enemies[0].creature.max_hp = 100;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        combat.hand = vec![card];
        combat.draw = vec![filler, filler, status];
        combat.known_draw_top = 3;
        combat.discard.clear();
        combat.energy = 3;
        combat.player.block = 0;
        combat.player.powers = ids
            .into_iter()
            .zip([3, 1, 5, 2, 3, 2, 1])
            .map(|(id, amount)| Power {
                id,
                amount,
                skip_duration: false,
                value: 0,
            })
            .collect();
        let preview = played_card_preview(&game, &content, card, Some(0));
        assert_eq!((preview.block, preview.draw, preview.exhaust), (5, 3, 1));
        assert_eq!(
            resolved_hits(&game, &content, 0, Some(0), false, true, &preview.hits).2,
            28
        );
        let drawn = game.combat().unwrap().drawn;
        let mut played = game.clone();
        played
            .step(
                &content,
                Action::Play {
                    hand: 0,
                    target: Some(0),
                },
            )
            .unwrap();
        let combat = played.combat().unwrap();
        assert_eq!(combat.player.block, 5);
        assert_eq!(combat.enemies[0].creature.hp, 72);
        assert_eq!(combat.drawn - drawn, 3);
        assert_eq!(combat.exhaust.len(), 1);
    }
}
