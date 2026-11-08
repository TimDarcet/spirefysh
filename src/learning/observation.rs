use crate::*;
#[cfg(feature = "python")]
use rayon::prelude::*;
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    io,
    path::Path,
    sync::{Arc, Mutex},
};

const MAGIC: &[u8; 8] = b"STSVALUE";
const VERSION: u32 = 65;
const VALUE_MODEL_VERSION: u32 = 81;
const TERMINAL_CATEGORIES: usize = 83;
const PHASES: usize = 12;
const MAP_NODE_KINDS: usize = 26;
const PUBLIC_GLOBALS: usize = 26;
const DEFAULT_MODEL_WIDTH: usize = 128;
const DEFAULT_MODEL_LAYERS: usize = 4;
const DEFAULT_MODEL_HEADS: usize = 8;
const DEFAULT_MODEL_FEEDFORWARD: usize = 384;
const POSITION_CAPS: [u32; 12] = [32, 64, 16, 256, 256, 256, 256, 256, 11, 11, 4, 4];
const POSITION_NAMES: [&str; 12] = [
    "enemy",
    "power",
    "orb",
    "deck_origin",
    "draw_top",
    "draw_bottom",
    "relic",
    "continuation",
    "crystal_row",
    "crystal_column",
    "crystal_width",
    "crystal_height",
];
const KNOWN_DRAW_SLOTS: usize = u8::MAX as usize;
const NONE: u32 = u32::MAX;

const CARD_FIELDS: [&str; 42] = [
    "energy cost ÷ 5",
    "star cost ÷ 5",
    "enchantment value ÷ 30",
    "replays ÷ 5",
    "exhaust",
    "ethereal",
    "retain",
    "innate",
    "unplayable",
    "eternal",
    "limit three",
    "sly",
    "inky",
    "replay",
    "dupe / return to hand",
    "return to draw",
    "no generate",
    "fetched",
    "free combat",
    "filtered draw",
    "exhaust this turn",
    "ethereal this turn",
    "retain this turn",
    "innate this turn",
    "unplayable this turn",
    "eternal this turn",
    "limit three this turn",
    "sly this turn",
    "inky this turn",
    "replay this turn",
    "dupe / return to hand this turn",
    "return to draw this turn",
    "no generate this turn",
    "fetched this turn",
    "free combat this turn",
    "filtered draw this turn",
    "absolute cost modifier",
    "this-turn cost modifier",
    "until-played cost modifier",
    "reduce-only cost modifier",
    "star cost modifier",
    "cost modifiers ÷ 6",
];

const HISTORY_FIELDS: [&str; 29] = [
    "cards played ÷ 10",
    "manual cards ÷ 10",
    "manual plays ÷ 10",
    "attacks ÷ 10",
    "skills ÷ 10",
    "powers ÷ 10",
    "energy spent ÷ 5",
    "exhausted ÷ 10",
    "discarded ÷ 10",
    "shivs ÷ 10",
    "stars gained ÷ 5",
    "ln(1 + generated this combat)",
    "ln(1 + ethereal this combat)",
    "extra drawn ÷ 10",
    "doom applied ÷ 10",
    "Osty attacks ÷ 10",
    "block gains ÷ 10",
    "block card is the current play",
    "block card gains ÷ 10",
    "HP lost ÷ 30",
    "ln(1 + HP loss events this combat)",
    "feral returns ÷ 10",
    "last turn cards ÷ 10",
    "ln(1 + orbit spent)",
    "ln(1 + lightning channeled)",
    "ln(1 + orbs channeled)",
    "ln(1 + poisoned)",
    "last damage ÷ 30",
    "ln(1 + drawn this combat)",
];

/// Each token type: its name, concept-code slots (each with an amount) and named numeric fields.
const DOMAINS: [(&str, usize, &[&str]); 22] = [
    (
        "run",
        2,
        &[
            "ascension ÷ 20",
            "act ÷ 3",
            "floor ÷ 60",
            "bosses visited ÷ 3",
            "weak encounters left ÷ 20",
            "regular encounters left ÷ 20",
            "elite encounters left ÷ 20",
            "HP ÷ 30",
            "max HP ÷ 30",
            "HP fraction",
            "missing HP fraction",
            "ln(1 + HP)",
            "gold ÷ 1000",
            "energy ÷ 5",
            "draw ÷ 10",
            "orb slots ÷ 10",
            "shop removals ÷ 10",
            "card rarity offset ÷ 10000",
            "potion odds ÷ 100",
            "unknown combat weight ÷ 1000",
            "unknown elite weight ÷ 10000",
            "unknown treasure weight ÷ 1000",
            "unknown shop weight ÷ 1000",
        ],
    ),
    (
        "phase",
        42,
        &[
            "cards to pick ÷ 10",
            "filter cost ÷ 10",
            "operation cost ÷ 10",
            "Crystal Sphere tools ÷ 10",
            "reward gold ÷ 1000",
            "reward removals ÷ 10",
            "card rewards ÷ 10",
        ],
    ),
    ("card", 6, &CARD_FIELDS),
    (
        "actor",
        5,
        &[
            "HP ÷ 30, at most 5",
            "max HP ÷ 30, at most 5",
            "block ÷ 30, at most 5",
            "HP fraction",
            "missing HP fraction",
            "block fraction, at most 5",
            "ln(1 + HP)",
            "max energy ÷ 5",
            "draw per turn ÷ 10",
            "orb slots ÷ 10",
            "energy ÷ 5",
            "stars ÷ 5",
            "turn ÷ 10",
            "stored value ÷ 30",
            "hits this turn ÷ 10",
            "move repeats ÷ 10",
            "intent damage ÷ 30",
            "intent hits ÷ 10",
        ],
    ),
    ("power", 6, &[]),
    ("history", 1, &HISTORY_FIELDS),
    ("move", 1, &[]),
    (
        "relic",
        4,
        &["wax", "melted", "wax relics melting first ÷ 4"],
    ),
    ("potion", 1, &[]),
    ("orb", 4, &[]),
    ("event", 2, &[]),
    ("encounter", 1, &[]),
    ("crystal", 7, &[]),
    (
        "run_effect",
        20,
        &["count ÷ 10", "second count ÷ 10", "maximum gold ÷ 1000"],
    ),
    (
        "effect",
        48,
        &[
            "signed ln(1 + base ÷ scale)",
            "signed ln(1 + upgraded ÷ scale)",
            "signed ln(1 + multiplier ÷ 10)",
            "signed ln(1 + divisor ÷ 10)",
            "signed ln(1 + second amount ÷ scale)",
            "signed ln(1 + second base ÷ scale)",
            "signed ln(1 + second upgraded ÷ scale)",
            "signed ln(1 + second multiplier ÷ 10)",
            "signed ln(1 + second divisor ÷ 10)",
            "count ÷ 10",
            "second count ÷ 10",
            "signed value ÷ 5",
            "evaluated",
        ],
    ),
    ("context", 9, &["X ÷ 5", "event value ÷ 30"]),
    ("option", 1, &["gold requirement ÷ 1000", "HP requirement ÷ 30"]),
    ("offer", 4, &["price ÷ 1000"]),
    ("play", 3, &["energy ÷ 5", "stars ÷ 5", "plays ÷ 5"]),
    ("map_node", 3, &[]),
    ("map_edge", 0, &[]),
    (
        "action",
        8,
        &[
            "expected block ÷ 30, at most 5",
            "expected target damage ÷ 30, at most 5",
            "expected total damage ÷ 30, at most 5",
            "target kill chance",
            "expected player HP change ÷ 30, within ±5",
            "expected draw ÷ 10",
            "expected discard ÷ 10",
            "expected exhaust ÷ 10",
            "expected energy spent ÷ 5",
            "expected stars spent ÷ 5",
            "reward gold ÷ 1000",
            "removal reward ÷ 10",
            "price ÷ 1000",
        ],
    ),
];
const RUN_DOMAIN: usize = 0;
const PHASE_DOMAIN: usize = 1;
const CARD_DOMAIN: usize = 2;
const ACTOR_DOMAIN: usize = 3;
const POWER_DOMAIN: usize = 4;
const HISTORY_DOMAIN: usize = 5;
const MOVE_DOMAIN: usize = 6;
const RELIC_DOMAIN: usize = 7;
const POTION_DOMAIN: usize = 8;
const ORB_DOMAIN: usize = 9;
const EVENT_DOMAIN: usize = 10;
const ENCOUNTER_DOMAIN: usize = 11;
const CRYSTAL_DOMAIN: usize = 12;
const RUN_EFFECT_DOMAIN: usize = 13;
const EFFECT_DOMAIN: usize = 14;
const CONTEXT_DOMAIN: usize = 15;
const OPTION_DOMAIN: usize = 16;
const OFFER_DOMAIN: usize = 17;
const PLAY_DOMAIN: usize = 18;
const MAP_NODE_DOMAIN: usize = 19;
const MAP_EDGE_DOMAIN: usize = 20;
const ACTION_DOMAIN: usize = 21;

/// Where a token goes in the model. Child tokens are summed into their parent.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Run,
    Phase,
    MapNode,
    MapEdge,
    Deck,
    Hand,
    Draw,
    Discard,
    Exhaust,
    Relic,
    Potion,
    Orb,
    Player,
    Ally,
    Enemy,
    Effect,
    Move,
    Child,
    RemovedRelic,
    EncounterPool,
    EventPool,
    Continuation,
    Crystal,
    Candidate,
}

const PLACE_NAMES: [&str; 24] = [
    "run",
    "phase",
    "map_node",
    "map_edge",
    "deck",
    "hand",
    "draw",
    "discard",
    "exhaust",
    "relic",
    "potion",
    "orb",
    "player",
    "ally",
    "enemy",
    "effect",
    "move",
    "child",
    "removed_relic",
    "encounter_pool",
    "event_pool",
    "continuation",
    "crystal",
    "candidate",
];

#[repr(u32)]
#[derive(Clone, Copy)]
enum Flag {
    ChoicePending,
    EnemyTurn,
    TurnEnding,
    ForcedEnd,
    Optional,
    BigCrystal,
    Astrolabe,
    WrigglerOdd,
    SkipNextDecay,
    Empty,
    Upgraded,
    PenNib,
    UnsettlingLamp,
    Preview,
    DamageTaken,
    Nightmare,
    HistoryCourse,
    ElseBranch,
    UncommonReward,
    AnyNumber,
}

const FLAG_NAMES: [&str; 20] = [
    "combat choice pending",
    "enemy turn",
    "turn ending",
    "forced end",
    "optional",
    "big Crystal Sphere tool",
    "Astrolabe transform",
    "odd Wriggler",
    "skips next decay",
    "empty slot",
    "upgraded",
    "Pen Nib",
    "Unsettling Lamp",
    "simulated preview",
    "took damage this run",
    "nightmare",
    "history course",
    "else branch",
    "uncommon potion reward",
    "any number",
];

#[repr(usize)]
#[derive(Clone, Copy)]
enum Semantic {
    Character,
    Card,
    Power,
    PowerValue,
    Relic,
    RelicValue,
    RelicCombat,
    Potion,
    Enemy,
    EnemyMove,
    Orb,
    OrbTiming,
    Event,
    EventValue,
    EncounterNormal,
    EncounterElite,
    Enchantment,
    CardType,
    Rarity,
    Room,
    Target,
    Pile,
    Phase,
    SuspendedPhase,
    Action,
    Effect,
    EffectFlag,
    RunEffect,
    Condition,
    Filter,
    CardOp,
    Scale,
    EnemyPosition,
    PowerPosition,
    OrbPosition,
    DeckOrigin,
    DrawTopPosition,
    DrawBottomPosition,
    RelicPosition,
    ContinuationPosition,
    CrystalRow,
    CrystalColumn,
    CrystalWidth,
    CrystalHeight,
    CrystalKind,
    FlagBit,
    TurnFlagBit,
    TagBit,
    ActorKind,
    MapNodeKind,
    MapModifier,
    RouteKind,
    Requirement,
    TinkerRider,
    ConveyorDish,
    EventAction,
    CardReward,
    ShopItem,
    PlayKind,
    Flag,
    TokenRole,
    Collection,
    Trigger,
    Count,
}

const SEMANTIC_NAMES: [&str; Semantic::Count as usize] = [
    "character",
    "card",
    "power",
    "power_value",
    "relic",
    "relic_value",
    "relic_combat",
    "potion",
    "enemy",
    "enemy_move",
    "orb",
    "orb_timing",
    "event",
    "event_value",
    "encounter_normal",
    "encounter_elite",
    "enchantment",
    "card_type",
    "rarity",
    "room",
    "target",
    "pile",
    "phase",
    "suspended_phase",
    "action",
    "effect",
    "effect_flag",
    "run_effect",
    "condition",
    "filter",
    "card_op",
    "scale",
    "enemy_position",
    "power_position",
    "orb_position",
    "deck_origin",
    "draw_top_position",
    "draw_bottom_position",
    "relic_position",
    "continuation_position",
    "crystal_row",
    "crystal_column",
    "crystal_width",
    "crystal_height",
    "crystal_kind",
    "flag_bit",
    "turn_flag_bit",
    "tag_bit",
    "actor_kind",
    "map_node_kind",
    "map_modifier",
    "route_kind",
    "requirement",
    "tinker_rider",
    "conveyor_dish",
    "event_action",
    "card_reward",
    "shop_item",
    "play_kind",
    "flag",
    "token_role",
    "collection",
    "trigger",
];

/// One model input: its type (domain), where it goes (place), the token it is summed into or
/// belongs to (parent), the token it points at (link: a map edge's destination, a path's or the
/// run's map node), its concept codes, one amount per code, and its named fields.
#[derive(Clone, Debug, PartialEq)]
struct Token {
    domain: u8,
    place: Place,
    parent: u32,
    link: u32,
    c: Vec<u32>,
    a: Vec<f32>,
    f: Vec<f32>,
}

impl Token {
    fn new(domain: usize, place: Place, parent: u32) -> Self {
        let (_, codes, fields) = DOMAINS[domain];
        Self {
            domain: domain as u8,
            place,
            parent,
            link: NONE,
            c: vec![0; codes],
            a: vec![0.0; codes],
            f: vec![0.0; fields.len()],
        }
    }

    fn code(&mut self, layout: Layout, namespace: Semantic, value: u32) {
        self.amount(layout, namespace, value, 0.0);
    }

    fn amount(&mut self, layout: Layout, namespace: Semantic, value: u32, amount: f32) {
        let slot = (self.c.iter().position(|&code| code == 0)).unwrap_or_else(|| {
            panic!(
                "{} token overflow at {} {value}: {:?}",
                DOMAINS[self.domain as usize].0, SEMANTIC_NAMES[namespace as usize], self.c
            )
        });
        self.c[slot] = layout.semantic(namespace, value);
        self.a[slot] = amount;
    }

    fn bits(&mut self, layout: Layout, namespace: Semantic, bits: u32) {
        for bit in (0..16).filter(|bit| bits & 1 << bit != 0) {
            self.code(layout, namespace, bit);
        }
    }

    fn flag(&mut self, layout: Layout, flag: Flag, value: bool) {
        if value {
            self.code(layout, Semantic::Flag, flag as u32);
        }
    }

    fn enemy(&mut self, game: &Game, layout: Layout, index: usize) {
        self.code(layout, Semantic::EnemyPosition, index.min(32) as u32);
        if let Some(enemy) = game.combat().and_then(|combat| combat.enemies.get(index)) {
            self.code(layout, Semantic::Enemy, enemy.creature.id as u32);
        }
    }

    fn actor(&mut self, game: &Game, layout: Layout, actor: Actor) {
        self.code(layout, Semantic::ActorKind, actor.variant());
        if let Actor::Enemy(index) = actor {
            self.enemy(game, layout, index);
        }
    }

    fn scale(&mut self, layout: Layout, scale: Scale) {
        self.code(layout, Semantic::Scale, scale.variant());
        match scale {
            Scale::ExhaustId(id) => self.code(layout, Semantic::Card, base_card_id(id as u32)),
            Scale::OrbTypesPower(id)
            | Scale::TargetPower(id)
            | Scale::TargetPowerDiv(id, _)
            | Scale::Power(id)
            | Scale::EventPower(id)
            | Scale::EnemyPowerTotal(id) => self.code(layout, Semantic::Power, id as u32),
            Scale::Tagged(tag) => self.bits(layout, Semantic::TagBit, tag as u32),
            Scale::HandType(kind) => self.code(layout, Semantic::CardType, kind as u32),
            _ => {}
        }
    }

    /// Codes the filter; returns its cost argument.
    fn filter(&mut self, layout: Layout, filter: CardFilter) -> i32 {
        self.code(layout, Semantic::Filter, filter.variant());
        match filter {
            CardFilter::Type(kind) | CardFilter::NotType(kind) => {
                self.code(layout, Semantic::CardType, kind as u32)
            }
            CardFilter::TypeWithoutTurnFlag(kind, flag) => {
                self.code(layout, Semantic::CardType, kind as u32);
                self.bits(layout, Semantic::TurnFlagBit, flag as u32);
            }
            CardFilter::WithoutFlag(flag) | CardFilter::Flag(flag) => {
                self.bits(layout, Semantic::FlagBit, flag as u32)
            }
            CardFilter::Id(id) => self.code(layout, Semantic::Card, base_card_id(id as u32)),
            CardFilter::Cost(cost) | CardFilter::PlayableCost(cost) => return cost as i32,
            _ => {}
        }
        0
    }

    /// Codes the operation; returns its cost argument.
    fn op(&mut self, layout: Layout, op: CardOp) -> i32 {
        self.code(layout, Semantic::CardOp, op.variant());
        match op {
            CardOp::Move(pile) | CardOp::MoveFree(pile) => {
                self.code(layout, Semantic::Pile, pile as u32)
            }
            CardOp::Flag(flag) => self.bits(layout, Semantic::FlagBit, flag as u32),
            CardOp::TurnFlag(flag) => self.bits(layout, Semantic::TurnFlagBit, flag as u32),
            CardOp::Transform(id, _) => self.code(layout, Semantic::Card, base_card_id(id as u32)),
            CardOp::Cost(cost) | CardOp::SetCost(cost) => return cost as i32,
            _ => {}
        }
        0
    }

    fn condition(&mut self, layout: Layout, condition: Condition) {
        self.code(layout, Semantic::Condition, condition.variant());
        match condition {
            Condition::TargetHasPower(id) | Condition::SourceHasPower(id) => {
                self.code(layout, Semantic::Power, id as u32)
            }
            Condition::HandWithout(kind) | Condition::CardType(kind) => {
                self.code(layout, Semantic::CardType, kind as u32)
            }
            Condition::HasOrb(id) => self.code(layout, Semantic::Orb, id as u32),
            Condition::Card(id) => self.code(layout, Semantic::Card, base_card_id(id as u32)),
            _ => {}
        }
    }
}

/// The candidate action's token and the tokens summed into it.
#[derive(Clone, Debug, PartialEq)]
struct CandidateRow {
    action: Action,
    legal: bool,
    token: Token,
    attached: Vec<Token>,
}

#[derive(Clone, Debug, PartialEq)]
struct Observation {
    character: u8,
    globals: Vec<f32>,
    tokens: Vec<Token>,
    candidates: Vec<CandidateRow>,
}

#[derive(Clone, Copy, PartialEq)]
struct Layout {
    characters: usize,
    semantic_offsets: [u32; Semantic::Count as usize],
    semantic_sizes: [u32; Semantic::Count as usize],
    slippery_bridge: Option<Id>,
    relic_trader: Option<Id>,
}

impl Layout {
    fn new(content: &Content) -> Self {
        let mut semantic_sizes = [0; Semantic::Count as usize];
        for (namespace, size) in [
            (Semantic::Character, content.characters.len()),
            (Semantic::Card, content.cards.len() * 2 + 512),
            (Semantic::Power, content.powers.len()),
            (Semantic::PowerValue, content.powers.len()),
            (Semantic::Relic, content.relics.len()),
            (Semantic::RelicValue, content.relics.len()),
            (Semantic::RelicCombat, content.relics.len()),
            (Semantic::Potion, content.potions.len() + 1),
            (Semantic::Enemy, content.enemies.len()),
            (
                Semantic::EnemyMove,
                content.enemies.iter().map(|enemy| enemy.moves.len()).sum::<usize>() + 1,
            ),
            (Semantic::Orb, content.orbs.len()),
            (Semantic::OrbTiming, labels::variants("OrbTiming").len()),
            (Semantic::Event, content.events.len()),
            (Semantic::EventValue, content.events.len() * 4),
            (Semantic::EncounterNormal, 57),
            (Semantic::EncounterElite, 13),
            (Semantic::Enchantment, labels::variants("Enchantment").len()),
            (Semantic::CardType, labels::variants("CardType").len()),
            (Semantic::Rarity, labels::variants("CardRarity").len()),
            (Semantic::Room, labels::variants("Room").len()),
            (Semantic::Target, labels::variants("Target").len()),
            (Semantic::Pile, labels::variants("Pile").len()),
            (Semantic::Phase, PHASES),
            (Semantic::SuspendedPhase, PHASES),
            (Semantic::Action, labels::variants("Action").len()),
            (Semantic::Effect, labels::variants("Effect").len()),
            (Semantic::EffectFlag, labels::variants("Effect").len() * 2),
            (Semantic::RunEffect, labels::variants("RunEffect").len()),
            (Semantic::Condition, labels::variants("Condition").len()),
            (Semantic::Filter, labels::variants("CardFilter").len()),
            (Semantic::CardOp, labels::variants("CardOp").len()),
            (Semantic::Scale, labels::variants("Scale").len()),
            (Semantic::EnemyPosition, 33),
            (Semantic::PowerPosition, 65),
            (Semantic::OrbPosition, 17),
            (Semantic::DeckOrigin, 257),
            (Semantic::DrawTopPosition, 257),
            (Semantic::DrawBottomPosition, 257),
            (Semantic::RelicPosition, 257),
            (Semantic::ContinuationPosition, 257),
            (Semantic::CrystalRow, 11),
            (Semantic::CrystalColumn, 11),
            (Semantic::CrystalWidth, 4),
            (Semantic::CrystalHeight, 4),
            (Semantic::CrystalKind, 9),
            (Semantic::FlagBit, 16),
            (Semantic::TurnFlagBit, 16),
            (Semantic::TagBit, 16),
            (Semantic::ActorKind, labels::variants("Actor").len()),
            (Semantic::MapNodeKind, MAP_NODE_KINDS),
            (Semantic::MapModifier, 2),
            (Semantic::RouteKind, 4),
            (Semantic::Requirement, labels::variants("Requirement").len()),
            (Semantic::TinkerRider, 9),
            (Semantic::ConveyorDish, 8),
            (Semantic::EventAction, 13),
            (Semantic::CardReward, labels::variants("CardReward").len()),
            (Semantic::ShopItem, labels::variants("ShopItem").len()),
            (Semantic::PlayKind, labels::PLAY_KINDS.len()),
            (Semantic::Flag, FLAG_NAMES.len()),
            (Semantic::TokenRole, labels::TOKEN_ROLES.len()),
            (Semantic::Collection, labels::COLLECTIONS.len()),
            (Semantic::Trigger, labels::variants("Trigger").len()),
        ] {
            semantic_sizes[namespace as usize] = size as u32;
        }
        let mut semantic_offsets = [0; Semantic::Count as usize];
        let mut next = 1u32;
        for (offset, &size) in semantic_offsets.iter_mut().zip(&semantic_sizes) {
            *offset = next;
            next = next.checked_add(size).expect("semantic vocabulary overflow");
        }
        let event = |name| (content.events.iter().position(|event| event.id == name)).map(|id| id as Id);
        Self {
            characters: content.characters.len(),
            semantic_offsets,
            semantic_sizes,
            slippery_bridge: event("EVENT.SLIPPERY_BRIDGE"),
            relic_trader: event("EVENT.RELIC_TRADER"),
        }
    }

    fn semantic(self, namespace: Semantic, value: u32) -> u32 {
        assert!(
            value < self.semantic_sizes[namespace as usize],
            "semantic namespace {} value {value} exceeds {}",
            SEMANTIC_NAMES[namespace as usize],
            self.semantic_sizes[namespace as usize]
        );
        self.semantic_offsets[namespace as usize] + value
    }

    fn concept_vocab(self) -> usize {
        (self.semantic_offsets[Semantic::Count as usize - 1]
            + self.semantic_sizes[Semantic::Count as usize - 1]) as usize
    }
}

/// sign(x) ln(1 + |x|): event values span counters to gold amounts.
fn signed_log(value: f32) -> f32 {
    value.signum() * value.abs().ln_1p()
}


/// What every token constructor reads.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    game: &'a Game,
    content: &'a Content,
    layout: Layout,
    masters: &'a HashMap<u32, usize>,
}

impl Ctx<'_> {
    fn card(self, card: Card, place: Place, parent: u32) -> Token {
        let (content, layout) = (self.content, self.layout);
        let def = content.cards[card.id as usize];
        let mut token = Token::new(CARD_DOMAIN, place, parent);
        // In whole points (Thrash keeps hundredths).
        let value = card.value as i32 / def.damage().map_or(1, |amount| amount.divisor.max(1)) as i32;
        let version = |upgraded| card_version_id(content, card.id, upgraded, card.variant);
        token.amount(layout, Semantic::Card, version(card.upgrades > 0), value as f32 / 30.0);
        let dampened = self.game.combat().and_then(|combat| {
            combat.dampened.iter().find(|(instance, _)| *instance == card.instance)
        });
        if let Some(&(_, upgrades)) = dampened {
            token.code(layout, Semantic::Card, version(upgrades > 0));
        }
        if let Some(enchantment) = card.enchantment {
            let amount = card.enchantment_amount as f32 / 30.0;
            token.amount(layout, Semantic::Enchantment, enchantment as u32, amount);
        }
        if let Some(&master) = self.masters.get(&card.instance) {
            token.code(layout, Semantic::DeckOrigin, (master as u32 - 1).min(256));
        }
        let star_cost = def.star_cost[card.upgrades.min(1) as usize] as i16;
        token.f[..4].copy_from_slice(&[
            crate::game::card_cost(card, def, -1) as f32 / 5.0,
            card.star_override().unwrap_or(star_cost) as f32 / 5.0,
            card.enchantment_value as f32 / 30.0,
            card.replays as f32 / 5.0,
        ]);
        let kinds = card.cost_mods().fold(0, |kinds, cost| kinds | cost.kind);
        for bit in 0..16 {
            token.f[4 + bit] = (card.flags as u32 >> bit & 1) as f32;
            token.f[20 + bit] = (card.turn_flags as u32 >> bit & 1) as f32;
        }
        for bit in 0..5 {
            token.f[36 + bit] = (kinds >> bit & 1) as f32;
        }
        token.f[41] = card.cost_mods().count() as f32 / 6.0;
        token
    }

    /// A draw-pile card at `index`, with its position when known.
    fn draw_card(self, combat: &Combat, index: usize, place: Place, parent: u32) -> Token {
        let mut token = self.card(combat.draw[index], place, parent);
        let len = combat.draw.len();
        if index < combat.known_draw_bottom {
            token.code(self.layout, Semantic::DrawBottomPosition, index.min(256) as u32);
        } else if index >= len - combat.known_draw_top {
            token.code(self.layout, Semantic::DrawTopPosition, (len - index - 1).min(256) as u32);
        }
        token
    }

    fn relic(self, id: Id, place: Place, parent: u32) -> Token {
        let mut token = Token::new(RELIC_DOMAIN, place, parent);
        token.code(self.layout, Semantic::Relic, id as u32);
        token
    }

    fn potion(self, id: Option<Id>, place: Place, parent: u32) -> Token {
        let mut token = Token::new(POTION_DOMAIN, place, parent);
        let empty = self.content.potions.len() as u32;
        token.code(self.layout, Semantic::Potion, id.map_or(empty, |id| id as u32));
        token
    }

    fn phase(self, phase: &Phase, suspended: bool) -> Token {
        let (game, layout) = (self.game, self.layout);
        let mut token = Token::new(PHASE_DOMAIN, Place::Phase, NONE);
        let index = phase_index(phase).expect("terminal phase has no observation") as u32;
        let namespace = if suspended { Semantic::SuspendedPhase } else { Semantic::Phase };
        token.code(layout, namespace, index);
        token.code(layout, Semantic::Room, game.room as u32);
        let mut f = [0.0; 7];
        match phase {
            Phase::Combat(combat) => {
                token.flag(layout, Flag::ChoicePending, combat.choice.is_some());
                token.flag(layout, Flag::EnemyTurn, combat.enemy_turn);
                token.flag(layout, Flag::TurnEnding, ending(combat));
                token.flag(layout, Flag::ForcedEnd, combat.force_end);
                if let Some(choice) = combat.choice {
                    token.flag(layout, Flag::Optional, choice.optional);
                    token.code(layout, Semantic::Pile, choice.pile as u32);
                    f[0] = choice.remaining as f32 / 10.0;
                    f[1] = token.filter(layout, choice.filter) as f32 / 10.0;
                    f[2] = token.op(layout, choice.op) as f32 / 10.0;
                }
            }
            Phase::Rewards(rewards) => {
                f[4] = rewards.gold as f32 / 1_000.0;
                f[5] = rewards.removals as f32 / 10.0;
                f[6] = rewards.card_rewards.len() as f32 / 10.0;
            }
            Phase::Event(id, options) => self.event_values(&mut token, *id, options),
            Phase::RemoveCards(count, tag, optional) => {
                token.bits(layout, Semantic::TagBit, *tag as u32);
                token.flag(layout, Flag::Optional, *optional);
                f[0] = *count as f32 / 10.0;
            }
            Phase::UpgradeCards(count, optional) | Phase::ChooseCards(_, count, optional) => {
                token.flag(layout, Flag::Optional, *optional);
                f[0] = *count as f32 / 10.0;
            }
            Phase::TransformCards(target, count, optional) => {
                if let Some(id) = target {
                    token.code(layout, Semantic::Card, base_card_id(*id as u32));
                }
                token.flag(layout, Flag::Astrolabe, game.astrolabe);
                token.flag(layout, Flag::Optional, *optional);
                f[0] = *count as f32 / 10.0;
            }
            Phase::EnchantCards(enchantment, amount, count, card_type, optional) => {
                let amount = *amount as f32 / 30.0;
                token.amount(layout, Semantic::Enchantment, *enchantment as u32, amount);
                if let Some(kind) = card_type {
                    token.code(layout, Semantic::CardType, *kind as u32);
                }
                token.flag(layout, Flag::Optional, *optional);
                f[0] = *count as f32 / 10.0;
            }
            Phase::Map | Phase::Shop(_) | Phase::Rest | Phase::ChooseBundles(_) => {}
            Phase::Won | Phase::Dead => unreachable!(),
        }
        if !suspended && let Some(crystal) = &game.crystal {
            token.flag(layout, Flag::BigCrystal, crystal.big);
            f[3] = crystal.remaining as f32 / 10.0;
        }
        token.f.copy_from_slice(&f);
        token
    }

    /// The event and its public values: identifiers some events keep there as concepts, every
    /// other value as the amount of its own (event, slot) concept.
    fn event_values(self, token: &mut Token, id: Id, options: &[EventOption]) {
        let (game, content, layout) = (self.game, self.content, self.layout);
        token.code(layout, Semantic::Event, id as u32);
        let mut values = [0; 4];
        for &slot in public_event_slots(content, id) {
            values[slot] = i32::try_from(game.event_data[slot]).expect("public event value exceeds i32");
        }
        let mut decoded = [false; 4];
        let mut decode = |token: &mut Token, slot: usize, namespace, value: i32| {
            decoded[slot] = true;
            if value > 0 {
                token.code(layout, namespace, value as u32 - 1);
            }
        };
        match content.events[id as usize].id {
            "EVENT.COLORFUL_PHILOSOPHERS" => {
                for slot in 0..3 {
                    decode(token, slot, Semantic::Character, values[slot]);
                }
            }
            "EVENT.DOLL_ROOM"
                if !options.is_empty()
                    && event_actions(options).all(|action| (10..=12).contains(&action)) =>
            {
                for slot in 0..options.len().min(3) {
                    decode(token, slot, Semantic::Relic, values[slot] + 1);
                }
            }
            "EVENT.TINKER_TIME" if values[3] < 0 => {
                decode(token, 0, Semantic::CardType, values[0]);
                decode(token, 1, Semantic::CardType, values[1]);
                decoded[3] = true;
            }
            "EVENT.TINKER_TIME" if values[3] > 0 => {
                decode(token, 3, Semantic::CardType, values[3]);
                decode(token, 0, Semantic::TinkerRider, values[0]);
                decode(token, 1, Semantic::TinkerRider, values[1]);
            }
            "EVENT.ENDLESS_CONVEYOR" => {
                decode(token, 0, Semantic::ConveyorDish, values[0]);
                decode(token, 2, Semantic::ConveyorDish, values[2]);
            }
            _ => {}
        }
        for &slot in public_event_slots(content, id) {
            if !decoded[slot] && values[slot] != 0 {
                let code = id as u32 * 4 + slot as u32;
                token.amount(layout, Semantic::EventValue, code, signed_log(values[slot] as f32));
            }
        }
    }

    fn run(self, current: u32) -> Token {
        let (game, layout) = (self.game, self.layout);
        let run = &game.run;
        let mut token = Token::new(RUN_DOMAIN, Place::Run, NONE);
        token.link = current;
        token.code(layout, Semantic::Character, run.character as u32);
        token.flag(layout, Flag::DamageTaken, game.damage_taken);
        let (hp, max_hp) = (run.hp.max(0) as f32, run.max_hp.max(1) as f32);
        let weak = weak_encounters(game, self.content);
        let visited = game.encounters_visited as usize;
        let left = game.encounters.len().saturating_sub(visited);
        token.f.copy_from_slice(&[
            run.ascension as f32 / 20.0,
            run.act as f32 / 3.0,
            run.floor as f32 / 60.0,
            game.bosses_visited as f32 / 3.0,
            weak.saturating_sub(visited) as f32 / 20.0,
            left.min(game.encounters.len() - weak) as f32 / 20.0,
            game.elites
                .len()
                .saturating_sub(game.elites_visited as usize) as f32
                / 20.0,
            hp / 30.0,
            max_hp / 30.0,
            hp / max_hp,
            (max_hp - hp) / max_hp,
            hp.ln_1p(),
            run.gold as f32 / 1_000.0,
            run.energy as f32 / 5.0,
            run.draw as f32 / 10.0,
            run.orb_slots as f32 / 10.0,
            run.card_shop_removals as f32 / 10.0,
            game.rarity_offset as f32 / 10_000.0,
            game.potion_odds as f32 / 100.0,
            game.unknown_odds[0] as f32 / 1_000.0,
            game.unknown_odds[1] as f32 / 10_000.0,
            game.unknown_odds[2] as f32 / 1_000.0,
            game.unknown_odds[3] as f32 / 1_000.0,
        ]);
        token
    }

    fn candidate(self, action: Action, legal: bool, map_ids: &[u32], enemies: &[u32]) -> CandidateRow {
        let (game, content, layout) = (self.game, self.content, self.layout);
        let mut token = Token::new(ACTION_DOMAIN, Place::Candidate, NONE);
        token.code(layout, Semantic::Action, action.variant());
        let phase = phase_index(&game.phase).expect("terminal phase has no action");
        token.code(layout, Semantic::Phase, phase as u32);
        let target = match action {
            Action::Play { target, .. } | Action::Potion { target, .. } => target,
            _ => None,
        };
        if let Some(target) = target {
            token.code(layout, Semantic::Target, 3);
            token.enemy(game, layout, target);
            token.link = enemies.get(target).copied().unwrap_or(NONE);
        }
        if let Action::Path(index) = action {
            token.link = map_ids.get(index).copied().unwrap_or(NONE);
            let normal =
                (game.map.current).is_none_or(|current| game.map.nodes[current].next.contains(&index));
            token.code(layout, Semantic::RouteKind, if normal { 1 } else { 2 });
        }
        if let Action::DiscardPotion(_) = action
            && let Some(id) = game.pending_potion
        {
            token.code(layout, Semantic::Potion, id as u32);
        }
        let outcome = match action {
            _ if game.combat().is_none() => None,
            Action::EndTurn => end_turn_outcome(game, content),
            _ if game.replaying => {
                let mut live = game.clone();
                live.replaying = false;
                live.expected_action_outcome(content, &action)
            }
            _ => game.expected_action_outcome(content, &action),
        };
        if let Some(outcome) = outcome {
            token.flag(layout, Flag::Preview, true);
            let on_target = |values: &[f32]| target.map_or(0.0, |target| values[target]);
            let hp = |value: f32| (value / 30.0).clamp(-5.0, 5.0);
            token.f[..10].copy_from_slice(&[
                hp(outcome.block),
                hp(on_target(&outcome.hp_loss)),
                hp(outcome.hp_loss.iter().sum::<f32>()),
                on_target(&outcome.deaths),
                hp(outcome.player_hp),
                outcome.draw / 10.0,
                outcome.discard / 10.0,
                outcome.exhaust / 10.0,
                outcome.energy / 5.0,
                outcome.stars / 5.0,
            ]);
        }
        match (&action, &game.phase) {
            (Action::RewardGold, Phase::Rewards(rewards)) => {
                token.f[10] = rewards.gold as f32 / 1_000.0
            }
            (Action::RewardRemove, Phase::Rewards(rewards)) => {
                token.f[11] = rewards.removals as f32 / 10.0
            }
            (Action::Buy(index), Phase::Shop(items)) => {
                if let Some(
                    ShopItem::Card(_, price)
                    | ShopItem::Relic(_, price)
                    | ShopItem::Potion(_, price)
                    | ShopItem::Remove(price),
                ) = items.get(*index)
                {
                    token.f[12] = *price as f32 / 1_000.0;
                }
            }
            _ => {}
        }
        let mut items = Items::new(self, 0, NONE);
        items.attach(&action);
        CandidateRow { action, legal, token, attached: items.out }
    }
}

/// Builds tokens summed into `parent`, or queue items: a head token (`item()`) with its
/// descendants summed into it.
struct Items<'a> {
    ctx: Ctx<'a>,
    out: Vec<Token>,
    base: u32,
    parent: u32,
    head: bool,
    order: u32,
}

impl<'a> Items<'a> {
    fn new(ctx: Ctx<'a>, base: u32, parent: u32) -> Self {
        Self { ctx, out: Vec::new(), base, parent, head: false, order: 0 }
    }

    /// The next token heads a new queue item.
    fn item(&mut self) {
        self.head = true;
    }

    fn push(&mut self, mut token: Token) {
        if self.head {
            token.place = Place::Continuation;
            token.parent = NONE;
            token.code(self.ctx.layout, Semantic::ContinuationPosition, self.order.min(256));
            self.parent = self.base + self.out.len() as u32;
            self.head = false;
            self.order += 1;
        } else {
            token.place = Place::Child;
            token.parent = self.parent;
        }
        self.out.push(token);
    }

    fn card(&mut self, card: Card) {
        let token = self.ctx.card(card, Place::Child, NONE);
        self.push(token);
    }

    fn relic(&mut self, id: Id) {
        let token = self.ctx.relic(id, Place::Child, NONE);
        self.push(token);
    }

    fn potion(&mut self, id: Id) {
        let token = self.ctx.potion(Some(id), Place::Child, NONE);
        self.push(token);
    }

    fn effect(&mut self, effect: Effect, evaluation: Option<Context>, else_branch: bool) {
        let Ctx { game, content, layout, .. } = self.ctx;
        if let Some(context) = evaluation {
            match effect {
                Effect::If(condition, yes, no) => {
                    let selected = if game.condition(content, context, condition) { yes } else { no };
                    for &child in selected {
                        self.effect(child, evaluation, else_branch);
                    }
                    return;
                }
                Effect::Repeat(amount, effects) => {
                    for _ in 0..game.amount(content, context, amount).max(0) {
                        for &child in effects {
                            self.effect(child, evaluation, else_branch);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }
        let index = effect.variant();
        let mut token = Token::new(EFFECT_DOMAIN, Place::Child, NONE);
        let t = &mut token;
        let target = |t: &mut Token, target| t.code(layout, Semantic::Target, target as u32);
        let pile = |t: &mut Token, pile| t.code(layout, Semantic::Pile, pile as u32);
        let card = |t: &mut Token, id: Id| t.code(layout, Semantic::Card, base_card_id(id as u32));
        let flag = |t: &mut Token, slot: u32, value: bool| {
            if value {
                t.code(layout, Semantic::EffectFlag, index * 2 + slot);
            }
        };
        let (mut count, mut second, mut signed) = (None, None, None);
        use Effect::*;
        match effect {
            Attack(to, _, hits) | OstyAttack(to, _, hits) => {
                target(t, to);
                count = Some(hits as f32);
            }
            AttackMany(to, ..)
            | OstyAttackMany(to, ..)
            | MoveDamage(to, _)
            | Damage(to, _)
            | LoseHp(to, _)
            | Block(to, _)
            | RawBlock(to, _)
            | Heal(to, _)
            | HealPercent(to, _)
            | ExhaustForAttack(to, _)
            | Kill(to)
            | Stun(to) => target(t, to),
            Fire(trigger) => t.code(layout, Semantic::Trigger, trigger as u32),
            Listen(trigger, listener) => {
                t.code(layout, Semantic::Trigger, trigger as u32);
                match listener {
                    Listener::Power(_, power) => t.code(layout, Semantic::Power, power.id as u32),
                    Listener::Relic(slot) => {
                        if let Some(relic) = game.run.relics.get(slot as usize) {
                            t.code(layout, Semantic::Relic, relic.id as u32);
                        }
                    }
                    Listener::Card(listener) => card(t, listener.id),
                }
            }
            DrawBlockIf(kind, _) | DrawUntilNot(kind) => {
                t.code(layout, Semantic::CardType, kind as u32)
            }
            Automation(value) => signed = Some(value as f32),
            DoublePower(to, id, value) => {
                target(t, to);
                t.code(layout, Semantic::Power, id as u32);
                signed = Some(value as f32);
            }
            Energy(value) | OrbSlots(value) | MaxEnergy(value) | SetCardCost(value)
            | ReduceCardCost(value) => signed = Some(value as f32),
            ApplyPower(to, id, _)
            | ApplyDebuff(to, id, _)
            | StackPower(to, id, _)
            | TemporaryStrength(to, id, _)
            | RemovePower(to, id) => {
                target(t, to);
                t.code(layout, Semantic::Power, id as u32);
            }
            Draw(n) | HandDraw(n) | ChooseRandomDraw(n) | ShuffleHandDraw(n) | EvokeAll(n)
            | PassiveFirst(n) | PassiveLast(n) | PassiveLightning(n) | WhisperingEarring(n)
            | OfferCharacterRetain(n) | Random(n, _) => count = Some(n as f32),
            DrawTo(values) | RandomOrb(values) | TransformHand(_, values) | RecycleHand(values) => {
                (count, second) = (Some(values[0] as f32), Some(values[1] as f32));
            }
            AddCard(to, id, n) | AddUpgradedCard(to, id, n) | AddRandom(to, id, n) => {
                pile(t, to);
                card(t, id);
                count = Some(n as f32);
            }
            AddFlaggedCard(to, id, n, flags) => {
                pile(t, to);
                card(t, id);
                t.bits(layout, Semantic::FlagBit, flags as u32);
                count = Some(n as f32);
            }
            RandomCard(to, kind, n, value) => {
                pile(t, to);
                t.code(layout, Semantic::CardType, kind as u32);
                flag(t, 0, value);
                count = Some(n as f32);
            }
            RandomColorless(to, _, value) | RandomCharacterCost0(to, _, value) => {
                pile(t, to);
                flag(t, 0, value);
            }
            RandomColorlessOther(to, _) | CopyCard(to, _) => {
                pile(t, to);
                if let CopyCard(_, n) = effect {
                    count = Some(n as f32);
                }
            }
            OfferColorless(n, first, second) => {
                flag(t, 0, first);
                flag(t, 1, second);
                count = Some(n as f32);
            }
            OfferCharacter(n, value) | Discard(n, value) | Exhaust(n, value) => {
                flag(t, 0, value);
                count = Some(n as f32);
            }
            OfferCharacterType(kind, n) | OfferOtherCharacter(kind, n, _) => {
                t.code(layout, Semantic::CardType, kind as u32);
                if let OfferOtherCharacter(_, _, value) = effect {
                    flag(t, 0, value);
                }
                count = Some(n as f32);
            }
            RandomCharacter(to, _, flags) => {
                pile(t, to);
                t.bits(layout, Semantic::FlagBit, flags as u32);
            }
            DistinctCharacter(to, n, value)
            | DistinctColorless(to, n, value)
            | Upgrade(to, n, value) => {
                pile(t, to);
                flag(t, 0, value);
                count = Some(n as f32);
            }
            RandomCardOp(to, filter, op, _) => {
                pile(t, to);
                t.filter(layout, filter);
                t.op(layout, op);
            }
            AutoPlayRandom(to, filter, _) => {
                pile(t, to);
                t.filter(layout, filter);
            }
            SelectAmount(to, filter, _, optional, op) => {
                pile(t, to);
                t.filter(layout, filter);
                flag(t, 0, optional);
                t.op(layout, op);
            }
            ReplayTagged(tag, n) => {
                t.bits(layout, Semantic::TagBit, tag as u32);
                count = Some(n as f32);
            }
            ChannelSlots(id) => t.code(layout, Semantic::Orb, id as u32),
            Channel(id, n) => {
                t.code(layout, Semantic::Orb, id as u32);
                count = Some(n as f32);
            }
            AddRandomCard(id, values, value) => {
                card(t, id);
                flag(t, 0, value);
                (count, second) = (Some(values[0] as f32), Some(values[1] as f32));
            }
            AutoPlayDraw(_, value) | Evoke(value) => flag(t, 0, value),
            DiscardHandAdd(id) | GrowAll(id, _) => card(t, id),
            MoveAll(from, filter, to) => {
                pile(t, from);
                t.filter(layout, filter);
                pile(t, to);
            }
            DrawFiltered(n, filter) | DrawFilteredStep(n, filter) => {
                t.filter(layout, filter);
                count = Some(n as f32);
            }
            FinishDrawFiltered(filter) => {
                t.filter(layout, filter);
            }
            ExhaustAttackStep(to, _, n) => {
                target(t, to);
                count = Some(n as f32);
            }
            If(condition, ..) => t.condition(layout, condition),
            Select(from, filter, values, first, second_flag, op) => {
                pile(t, from);
                t.filter(layout, filter);
                flag(t, 0, first);
                flag(t, 1, second_flag);
                t.op(layout, op);
                (count, second) = (Some(values[0] as f32), Some(values[1] as f32));
            }
            _ => {}
        }
        let mut primary = None;
        for (slot, (amount, scale)) in effect_amounts(effect).into_iter().take(2).enumerate() {
            t.scale(layout, amount.scale);
            let value = signed_log(
                evaluation.map_or(amount.base, |context| game.amount(content, context, amount)) as f32
                    / scale,
            );
            let start = if slot == 0 { 0 } else { 5 };
            t.f[start..start + 4].copy_from_slice(&[
                signed_log(amount.base as f32 / scale),
                signed_log(amount.upgraded as f32 / scale),
                signed_log(amount.multiplier as f32 / 10.0),
                signed_log(amount.divisor as f32 / 10.0),
            ]);
            if slot == 0 {
                primary = Some(value);
            } else {
                t.f[4] = value;
            }
        }
        let any = |value: Option<f32>| value == Some(u8::MAX as f32);
        t.flag(layout, Flag::AnyNumber, any(count) || any(second));
        let count = count.filter(|_| !any(count)).map(|value| value / 10.0);
        let second = second.filter(|_| !any(second));
        let signed = signed.map(|value| value / 5.0);
        t.f[9..].copy_from_slice(&[
            count.unwrap_or(0.0),
            second.map_or(0.0, |value| value / 10.0),
            signed.unwrap_or(0.0),
            evaluation.is_some() as u8 as f32,
        ]);
        let primary = primary.or(count).or(signed).unwrap_or(0.0);
        t.amount(layout, Semantic::Effect, index, primary);
        t.flag(layout, Flag::ElseBranch, else_branch);
        self.push(token);
        match effect {
            If(_, yes, no) => {
                for &child in yes {
                    self.effect(child, evaluation, false);
                }
                for &child in no {
                    self.effect(child, evaluation, true);
                }
            }
            Repeat(_, effects) | Random(_, effects) => {
                for &child in effects {
                    self.effect(child, evaluation, else_branch);
                }
            }
            AutoPlay(played) => self.card(played),
            _ => {}
        }
    }

    /// A queued combat effect: conditions and repeats resolve now, one item per effect.
    fn queued(&mut self, effect: Effect, context: Context) {
        let Ctx { game, content, .. } = self.ctx;
        match effect {
            Effect::If(condition, yes, no) => {
                let selected = if game.condition(content, context, condition) { yes } else { no };
                for &child in selected {
                    self.queued(child, context);
                }
            }
            Effect::Repeat(amount, effects) => {
                for _ in 0..game.amount(content, context, amount).max(0) {
                    for &child in effects {
                        self.queued(child, context);
                    }
                }
            }
            _ => {
                self.item();
                self.effect(effect, Some(context), false);
                self.context(context);
            }
        }
    }

    fn context(&mut self, context: Context) {
        let Ctx { game, content, layout, .. } = self.ctx;
        let mut token = Token::new(CONTEXT_DOMAIN, Place::Child, NONE);
        token.actor(game, layout, context.source);
        if let Some(target) = context.target {
            token.enemy(game, layout, target);
        }
        if let Some(id) = context.card {
            let version = card_version_id(content, id, context.upgraded, 0);
            token.code(layout, Semantic::Card, version);
        }
        if let Some(id) = context.orb_id {
            token.code(layout, Semantic::Orb, id as u32);
        }
        token.flag(layout, Flag::Upgraded, context.upgraded);
        token.flag(layout, Flag::PenNib, context.pen_nib);
        token.f.copy_from_slice(&[context.x as f32 / 5.0, context.event as f32 / 30.0]);
        self.push(token);
    }

    fn run_effect(&mut self, effect: RunEffect) {
        let Ctx { content, layout, .. } = self.ctx;
        let mut token = Token::new(RUN_EFFECT_DOMAIN, Place::Child, NONE);
        let t = &mut token;
        let card_id = |name| content.card_id(name).expect("unknown continuation card");
        let relic_id = |name| content.relic_id(name).expect("unknown continuation relic") as u32;
        let potion_id = |name| content.potion_id(name).expect("unknown continuation potion") as u32;
        let (mut quantity, mut count, mut second, mut most) = (None, None, 0.0, 0.0);
        use RunEffect::*;
        match effect {
            Gold(value) => quantity = Some(value as f32 / 1_000.0),
            RandomGold(least, highest) => {
                quantity = Some(least as f32 / 1_000.0);
                most = highest as f32 / 1_000.0;
            }
            Heal(value) | LoseHp(value) | MaxHp(value) | MaxHpTo(value) => {
                quantity = Some(value as f32 / 30.0)
            }
            HealPercent(value) => quantity = Some(value as f32 / 100.0),
            NextRelics(n) | UpgradeCards(n) | UpgradeRandom(n) | UpgradeShuffled(n)
            | DowngradeRandom(n) | SkipEventRng(n) => count = Some(n as f32),
            RelicOfRarity(rarity) => t.code(layout, Semantic::Rarity, rarity as u32),
            EventAction(action) => t.code(layout, Semantic::EventAction, action as u32),
            AddCard(name, n) => {
                t.code(layout, Semantic::Card, base_card_id(card_id(name) as u32));
                count = Some(n as f32);
            }
            AddRelic(name) => t.code(layout, Semantic::Relic, relic_id(name)),
            AddPotion(name) => t.code(layout, Semantic::Potion, potion_id(name)),
            PotionRewards(name, n) => {
                t.code(layout, Semantic::Potion, potion_id(name));
                count = Some(n as f32);
            }
            RandomPotionReward(uncommon) => t.flag(layout, Flag::UncommonReward, uncommon),
            RemoveCards(n, tag) => {
                t.bits(layout, Semantic::TagBit, tag as u32);
                count = Some(n as f32);
            }
            TransformCards(target, n) => {
                if let Some(name) = target {
                    t.code(layout, Semantic::Card, base_card_id(card_id(name) as u32));
                }
                count = Some(n as f32);
            }
            EnchantCards(enchantment, amount, n, card_type) => {
                let amount = amount as f32 / 30.0;
                t.amount(layout, Semantic::Enchantment, enchantment as u32, amount);
                if let Some(kind) = card_type {
                    t.code(layout, Semantic::CardType, kind as u32);
                }
                count = Some(n as f32);
            }
            ChooseCommonCards(first, other) | ChooseRewardCards(first, other) => {
                count = Some(first as f32);
                second = other as f32 / 10.0;
            }
            _ => {}
        }
        let count = count.map(|value| value / 10.0);
        let primary = quantity.or(count).unwrap_or(0.0);
        t.amount(layout, Semantic::RunEffect, effect.variant(), primary);
        t.f.copy_from_slice(&[count.unwrap_or(0.0), second, most]);
        self.push(token);
        match effect {
            RandomCard(names) => {
                for name in names {
                    self.card(Card { id: card_id(name), ..Card::default() });
                }
            }
            RandomRelic(names) => {
                for name in names {
                    self.relic(relic_id(name) as Id);
                }
            }
            Options(options) => {
                for option in options {
                    self.option(*option);
                }
            }
            _ => {}
        }
    }

    fn option(&mut self, option: EventOption) {
        let mut token = Token::new(OPTION_DOMAIN, Place::Child, NONE);
        let (gold, hp) = match option.requirement {
            Requirement::Gold(value) => (value as f32 / 1_000.0, 0.0),
            Requirement::Hp(value) => (0.0, value as f32 / 30.0),
            Requirement::Always | Requirement::Deck => (0.0, 0.0),
        };
        token.code(
            self.ctx.layout,
            Semantic::Requirement,
            option.requirement.variant(),
        );
        token.f.copy_from_slice(&[gold, hp]);
        self.push(token);
        for &effect in option.effects {
            self.run_effect(effect);
        }
    }

    fn card_reward(&mut self, reward: CardReward) {
        let layout = self.ctx.layout;
        let mut token = Token::new(OFFER_DOMAIN, Place::Child, NONE);
        token.code(layout, Semantic::CardReward, reward.variant());
        match reward {
            CardReward::Standard(room) => token.code(layout, Semantic::Room, room as u32),
            CardReward::Fixed(character, rarity) => {
                token.code(layout, Semantic::Character, character as u32);
                token.code(layout, Semantic::Rarity, rarity as u32);
            }
            CardReward::Crystal(rarity) => token.code(layout, Semantic::Rarity, rarity as u32),
            CardReward::Kaleidoscope => {}
        }
        self.push(token);
    }

    fn shop(&mut self, item: &ShopItem) {
        let price = match item {
            ShopItem::Card(_, price) | ShopItem::Relic(_, price) | ShopItem::Potion(_, price) => {
                price
            }
            ShopItem::Remove(price) => price,
        };
        let mut token = Token::new(OFFER_DOMAIN, Place::Child, NONE);
        token.code(self.ctx.layout, Semantic::ShopItem, item.variant());
        token.f[0] = *price as f32 / 1_000.0;
        self.push(token);
        match item {
            ShopItem::Card(card, _) => self.card(*card),
            ShopItem::Relic(id, _) => self.relic(*id),
            ShopItem::Potion(id, _) => self.potion(*id),
            ShopItem::Remove(_) => {}
        }
    }

    fn play(&mut self, kind: u32, numbers: [f32; 3], lamp: bool) {
        let mut token = Token::new(PLAY_DOMAIN, Place::Child, NONE);
        token.code(self.ctx.layout, Semantic::PlayKind, kind);
        token.flag(self.ctx.layout, Flag::UnsettlingLamp, lamp);
        token.f.copy_from_slice(&numbers.map(|value| value / 5.0));
        self.push(token);
    }

    /// The suspended phase's offers, summed into its token.
    fn resumed(&mut self, phase: &Phase) {
        match phase {
            Phase::Rewards(rewards) => {
                for &card in &rewards.cards {
                    self.card(card);
                }
                for &reward in &rewards.card_rewards {
                    self.card_reward(reward);
                }
                for &id in &rewards.relics {
                    self.relic(id);
                }
                for &id in &rewards.potions {
                    self.potion(id);
                }
            }
            Phase::Shop(items) => items.iter().for_each(|item| self.shop(item)),
            Phase::Event(_, options) => options.iter().for_each(|option| self.option(*option)),
            Phase::ChooseCards(cards, ..) => cards.iter().for_each(|card| self.card(*card)),
            Phase::ChooseBundles(bundles) => {
                bundles.iter().flatten().for_each(|card| self.card(*card))
            }
            _ => {}
        }
    }

    /// The payload a candidate action picks, plays or reveals.
    fn attach(&mut self, action: &Action) {
        let Ctx { game, content, layout, .. } = self.ctx;
        match (action, &game.phase) {
            (Action::Play { hand, target }, Phase::Combat(combat)) => {
                if let Some(&card) = combat.hand.get(*hand) {
                    self.card(card);
                    let context = Context {
                        target: *target,
                        card: Some(card.id),
                        upgraded: card.upgrades > 0,
                        x: combat.energy,
                        ..Context::player()
                    };
                    for &effect in content.cards[card.id as usize].effects {
                        self.effect(effect, Some(context), false);
                    }
                }
            }
            (Action::Potion { slot, target }, _) => {
                if let Some(&Some(id)) = game.run.potions.get(*slot) {
                    self.potion(id);
                    let context = Context { target: *target, ..Context::player() };
                    for &effect in content.potions[id as usize].effects {
                        self.effect(effect, game.combat().map(|_| context), false);
                    }
                }
            }
            (Action::DiscardPotion(slot), _) => {
                if let Some(&Some(id)) = game.run.potions.get(*slot) {
                    self.potion(id);
                }
            }
            (Action::Choose(index), Phase::Combat(combat)) if combat.choice.is_some() => {
                let choice = combat.choice.unwrap();
                if choice.pile == Pile::Draw {
                    if *index < combat.draw.len() {
                        let token = self.ctx.draw_card(combat, *index, Place::Child, NONE);
                        self.push(token);
                    }
                } else {
                    let cards = match choice.pile {
                        Pile::Hand => &combat.hand,
                        Pile::Discard => &combat.discard,
                        Pile::Exhaust => &combat.exhaust,
                        Pile::Offer | Pile::Draw => &combat.offer,
                    };
                    if let Some(&card) = cards.get(*index) {
                        self.card(card);
                    }
                }
            }
            (Action::Choose(index), Phase::ChooseCards(cards, ..)) => {
                if let Some(&card) = cards.get(*index) {
                    self.card(card);
                }
            }
            (Action::Choose(index), Phase::ChooseBundles(bundles)) => {
                for &card in bundles.get(*index).into_iter().flatten() {
                    self.card(card);
                }
            }
            (Action::RewardCard(index), Phase::Rewards(rewards)) => {
                if let Some(&card) = rewards.cards.get(*index) {
                    self.card(card);
                }
            }
            (Action::RewardRelic(index), Phase::Rewards(rewards)) => {
                if let Some(&id) = rewards.relics.get(*index) {
                    self.relic(id);
                }
            }
            (Action::RewardPotion(index), Phase::Rewards(rewards)) => {
                if let Some(&id) = rewards.potions.get(*index) {
                    self.potion(id);
                }
            }
            (Action::RerollCards, Phase::Rewards(rewards)) => {
                for &reward in &rewards.card_rewards {
                    self.card_reward(reward);
                }
            }
            (Action::Buy(index), Phase::Shop(items)) => match items.get(*index) {
                Some(ShopItem::Card(card, _)) => self.card(*card),
                Some(ShopItem::Relic(id, _)) => self.relic(*id),
                Some(ShopItem::Potion(id, _)) => self.potion(*id),
                Some(item @ ShopItem::Remove(_)) => self.shop(item),
                None => {}
            },
            (Action::Smith(index) | Action::Enchant(index) | Action::RemoveCard(index), _) => {
                if let Some(&card) = game.run.deck.get(*index) {
                    self.card(card);
                }
            }
            (Action::EventCard(_, card), _) => self.card(*card),
            (Action::EventRelic(_, id), _) => self.relic(*id),
            (Action::Event(index), Phase::Event(id, options)) => self.event_option(*index, *id, options),
            (Action::CrystalCell(x, y), _) => {
                let mut token = Token::new(CRYSTAL_DOMAIN, Place::Child, NONE);
                token.code(layout, Semantic::CrystalKind, 2);
                token.code(layout, Semantic::CrystalRow, *x as u32);
                token.code(layout, Semantic::CrystalColumn, *y as u32);
                self.push(token);
            }
            (Action::CrystalTool(big), _) => {
                let mut token = Token::new(CRYSTAL_DOMAIN, Place::Child, NONE);
                token.code(layout, Semantic::CrystalKind, 3);
                token.flag(layout, Flag::BigCrystal, *big);
                self.push(token);
            }
            _ => {}
        }
    }

    fn event_option(&mut self, index: usize, id: Id, options: &[EventOption]) {
        let Ctx { game, content, layout, .. } = self.ctx;
        let mut token = Token::new(EVENT_DOMAIN, Place::Child, NONE);
        token.code(layout, Semantic::Event, id as u32);
        if let Some(payload) = event_option_payload(game, content, index) {
            match content.events[id as usize].id {
                "EVENT.COLORFUL_PHILOSOPHERS" if payload > 0 => {
                    token.code(layout, Semantic::Character, payload as u32 - 1)
                }
                "EVENT.DOLL_ROOM"
                    if !options.is_empty()
                        && event_actions(options).all(|action| (10..=12).contains(&action)) =>
                {
                    token.code(layout, Semantic::Relic, payload as u32)
                }
                "EVENT.TINKER_TIME" if game.event_data[3] < 0 && payload > 0 => {
                    token.code(layout, Semantic::CardType, payload as u32 - 1)
                }
                "EVENT.TINKER_TIME" if game.event_data[3] > 0 && payload > 0 => {
                    token.code(layout, Semantic::TinkerRider, payload as u32 - 1)
                }
                _ => {}
            }
        }
        self.push(token);
        if let Some(option) = options.get(index) {
            self.option(*option);
        }
        if let Some(relic) = ancient_offer(game, content, index) {
            self.relic(relic);
        }
        if content.events[id as usize].id == "EVENT.RANWID_THE_ELDER"
            && index == 2
            && let Some(&owned) = game.event_cards.first()
            && let Some(relic) = game.run.relics.get(owned as usize)
        {
            self.relic(relic.id);
        }
        if Some(id) == layout.relic_trader {
            let offered = game.relic_queue.get(index).copied();
            let traded = (game.event_cards.get(index))
                .and_then(|&owned| game.run.relics.get(owned as usize).map(|relic| relic.id));
            for relic in [offered, traded].into_iter().flatten() {
                self.relic(relic);
            }
        }
        if Some(id) == layout.slippery_bridge
            && let Some(&card) = (game.run.deck.iter())
                .find(|card| card.instance == game.event_data[1] as u32)
        {
            self.card(card);
        }
    }
}

/// The state's tokens: map, run, phases, cards, actors, collections and the queue.
struct Encoder<'a> {
    ctx: Ctx<'a>,
    tokens: Vec<Token>,
}

impl Encoder<'_> {
    fn push(&mut self, token: Token) -> u32 {
        self.tokens.push(token);
        self.tokens.len() as u32 - 1
    }

    fn extend(&mut self, build: impl FnOnce(&mut Items)) {
        let mut items = Items::new(self.ctx, self.tokens.len() as u32, NONE);
        build(&mut items);
        self.tokens.extend(items.out);
    }

    fn state(&mut self, current: u32) {
        let Ctx { game, .. } = self.ctx;
        let run = self.ctx.run(current);
        self.push(run);
        let phase = self.ctx.phase(&game.phase, false);
        self.push(phase);
        if let Some(suspended) = &game.resume {
            let phase = self.ctx.phase(suspended, true);
            let parent = self.push(phase);
            let mut items = Items::new(self.ctx, self.tokens.len() as u32, parent);
            items.resumed(suspended);
            self.tokens.extend(items.out);
        }
        self.cards();
        self.actors();
        self.collections();
        self.queue();
    }

    fn cards(&mut self) {
        let ctx = self.ctx;
        for &card in &ctx.game.run.deck {
            self.push(ctx.card(card, Place::Deck, NONE));
        }
        let Some(combat) = ctx.game.combat() else { return };
        for (place, cards) in [
            (Place::Hand, &combat.hand),
            (Place::Discard, &combat.discard),
            (Place::Exhaust, &combat.exhaust),
        ] {
            for &card in cards {
                self.push(ctx.card(card, place, NONE));
            }
        }
        let start = self.tokens.len();
        for index in 0..combat.draw.len() {
            self.push(ctx.draw_card(combat, index, Place::Draw, NONE));
        }
        // The draw pile's unknown middle is a set.
        let end = start + combat.draw.len() - combat.known_draw_top;
        self.tokens[start + combat.known_draw_bottom..end].sort_by(token_order);
    }

    fn actors(&mut self) {
        let Ctx { game, content, layout, .. } = self.ctx;
        let Some(combat) = game.combat() else { return };
        let player = self.actor(Place::Player, &combat.player, None);
        let history = combat.history;
        let mut token = Token::new(HISTORY_DOMAIN, Place::Child, player);
        token.code(layout, Semantic::ActorKind, Actor::Player.variant());
        token.f.copy_from_slice(&[
            history.cards as f32 / 10.0,
            history.manual_cards as f32 / 10.0,
            history.manual_plays as f32 / 10.0,
            history.attacks as f32 / 10.0,
            history.skills as f32 / 10.0,
            history.powers as f32 / 10.0,
            history.energy as f32 / 5.0,
            history.exhausted as f32 / 10.0,
            history.discarded as f32 / 10.0,
            history.shivs as f32 / 10.0,
            history.stars_gained as f32 / 5.0,
            (history.generated as f32).ln_1p(),
            (history.ethereal as f32).ln_1p(),
            history.extra_drawn as f32 / 10.0,
            history.doom_applied as f32 / 10.0,
            history.osty_attacks as f32 / 10.0,
            history.block_gains as f32 / 10.0,
            (history.block_card == game.play_key()) as u8 as f32,
            history.block_card_gains as f32 / 10.0,
            history.hp_lost as f32 / 30.0,
            (history.hp_loss_events as f32).ln_1p(),
            history.feral_returns as f32 / 10.0,
            combat.last_turn.cards as f32 / 10.0,
            (combat.orbit_spent as f32).ln_1p(),
            (combat.lightning_channeled as f32).ln_1p(),
            (combat.orbs_channeled as f32).ln_1p(),
            (combat.poisoned as f32).ln_1p(),
            combat.last_damage as f32 / 30.0,
            (combat.drawn as f32).ln_1p(),
        ]);
        self.push(token);
        self.powers(player, Actor::Player, &combat.player.powers);
        for &(card, count) in &combat.nightmares {
            for _ in 0..count {
                let mut token = self.ctx.card(card, Place::Effect, player);
                token.flag(layout, Flag::Nightmare, true);
                self.push(token);
            }
        }
        if let Some(card) = combat.history_course {
            let mut token = self.ctx.card(card, Place::Effect, player);
            token.flag(layout, Flag::HistoryCourse, true);
            self.push(token);
        }
        if combat.osty.max_hp > 0 {
            let osty = self.actor(Place::Ally, &combat.osty, None);
            self.powers(osty, Actor::Osty, &combat.osty.powers);
        }
        for (index, enemy) in combat.enemies.iter().enumerate() {
            let actor = self.actor(Place::Enemy, &enemy.creature, Some((index, enemy)));
            self.powers(actor, Actor::Enemy(index), &enemy.creature.powers);
            for &movement in &enemy.move_history {
                let mut token = Token::new(MOVE_DOMAIN, Place::Move, actor);
                let id = enemy_move_id(content, enemy.creature.id, movement);
                token.code(layout, Semantic::EnemyMove, id);
                self.push(token);
            }
        }
    }

    fn actor(
        &mut self,
        place: Place,
        creature: &Creature,
        enemy: Option<(usize, &Enemy)>,
    ) -> u32 {
        let Ctx { game, content, layout, .. } = self.ctx;
        let combat = game.combat().unwrap();
        let mut token = Token::new(ACTOR_DOMAIN, place, NONE);
        // Capped: unkillable enemies sit at i16::MAX HP.
        let (hp, max_hp) = (creature.hp.min(999) as f32, creature.max_hp.min(999) as f32);
        let block = creature.block as f32;
        let ratio = max_hp.max(1.0);
        let mut f = [0.0; 18];
        f[..7].copy_from_slice(&[
            (hp / 30.0).min(5.0),
            (max_hp / 30.0).min(5.0),
            (block / 30.0).min(5.0),
            hp / ratio,
            (max_hp - hp) / ratio,
            (block / ratio).min(5.0),
            hp.max(0.0).ln_1p(),
        ]);
        match (place, enemy) {
            (Place::Player, _) => {
                token.code(layout, Semantic::ActorKind, Actor::Player.variant());
                f[7..13].copy_from_slice(&[
                    game.max_energy(content) as f32 / 5.0,
                    combat.draw_per_turn as f32 / 10.0,
                    combat.orb_slots as f32 / 10.0,
                    combat.energy as f32 / 5.0,
                    combat.stars as f32 / 5.0,
                    combat.turn as f32 / 10.0,
                ]);
            }
            (_, Some((index, enemy))) => {
                token.code(layout, Semantic::ActorKind, Actor::Enemy(index).variant());
                token.code(layout, Semantic::Enemy, enemy.creature.id as u32);
                token.code(layout, Semantic::EnemyPosition, index.min(32) as u32);
                let movement = if enemy.stunned > 0 {
                    content.enemies.iter().map(|enemy| enemy.moves.len() as u32).sum()
                } else {
                    enemy_move_id(content, enemy.creature.id, enemy.move_index)
                };
                token.code(layout, Semantic::EnemyMove, movement);
                let wriggler = content.enemies[enemy.creature.id as usize].id == "MONSTER.WRIGGLER";
                token.flag(layout, Flag::WrigglerOdd, wriggler && index % 2 == 1);
                let hits = combat.hits.get(index).copied().unwrap_or_default();
                let (damage, intent_hits) = intent(game, content, index);
                f[13..18].copy_from_slice(&[
                    enemy.value as f32 / 30.0,
                    hits as f32 / 10.0,
                    enemy.repeats as f32 / 10.0,
                    damage / 30.0,
                    intent_hits / 10.0,
                ]);
            }
            _ => token.code(layout, Semantic::ActorKind, Actor::Osty.variant()),
        }
        token.f.copy_from_slice(&f);
        self.push(token)
    }

    fn powers(&mut self, owner: u32, actor: Actor, powers: &[Power]) {
        let Ctx { game, layout, .. } = self.ctx;
        for (order, power) in powers.iter().enumerate() {
            let mut token = Token::new(POWER_DOMAIN, Place::Effect, owner);
            match actor {
                Actor::Enemy(index) => token.enemy(game, layout, index),
                _ => token.actor(game, layout, actor),
            }
            let id = power.id as u32;
            token.amount(layout, Semantic::Power, id, power.amount as f32 / 30.0);
            if power.value != 0 {
                token.amount(layout, Semantic::PowerValue, id, power.value as f32 / 30.0);
            }
            let decay = power.skip_next_decay && power.id != power_id::PANACHE;
            token.flag(layout, Flag::SkipNextDecay, decay);
            token.code(layout, Semantic::PowerPosition, order.min(64) as u32);
            self.push(token);
        }
    }

    fn collections(&mut self) {
        let Ctx { game, content, layout, .. } = self.ctx;
        let mut paels = NONE;
        for (order, relic) in game.run.relics.iter().enumerate() {
            let mut token = Token::new(RELIC_DOMAIN, Place::Relic, NONE);
            let id = relic.id as u32;
            token.amount(layout, Semantic::Relic, id, relic.counters[0] as f32 / 10.0);
            if relic.counters[1] != 0 {
                token.amount(
                    layout,
                    Semantic::RelicValue,
                    id,
                    relic.counters[1] as f32 / 10.0,
                );
            }
            if game.combat().is_some() && relic.combat != 0 {
                token.amount(
                    layout,
                    Semantic::RelicCombat,
                    id,
                    relic.combat as f32 / 10.0,
                );
            }
            token.code(layout, Semantic::RelicPosition, order.min(256) as u32);
            if let Some(position) = game.wax_relics.iter().position(|&wax| wax == order) {
                let melted = |wax: &usize| game.melted_relics.contains(wax);
                let ahead = game.wax_relics[..position].iter().filter(|wax| !melted(wax)).count();
                token.f.copy_from_slice(&[1.0, melted(&order) as u8 as f32, ahead as f32 / 4.0]);
            }
            let index = self.push(token);
            if id == paels_tooth() {
                paels = index;
            }
        }
        if paels != NONE {
            for &card in &game.paels_cards {
                self.push(self.ctx.card(card, Place::Child, paels));
            }
        }
        // The bag starts as the shared and character pools and only shrinks: encode what left it.
        let bag = public_relic_bag(game, layout).concat();
        for id in starting_relic_pool(game, content).into_iter().filter(|id| !bag.contains(id)) {
            self.push(self.ctx.relic(id, Place::RemovedRelic, NONE));
        }
        for &potion in &game.run.potions {
            self.push(self.ctx.potion(potion, Place::Potion, NONE));
        }
        if let Some(combat) = game.combat() {
            for slot in 0..combat.orb_slots as usize {
                let mut token = Token::new(ORB_DOMAIN, Place::Orb, NONE);
                match combat.orbs.get(slot) {
                    Some(orb) => {
                        let value = orb.value as f32 / 30.0;
                        token.amount(layout, Semantic::Orb, orb.id as u32, value);
                        let timing = content.orbs[orb.id as usize].timing as u32;
                        token.code(layout, Semantic::OrbTiming, timing);
                    }
                    None => token.flag(layout, Flag::Empty, true),
                }
                token.code(layout, Semantic::OrbPosition, slot.min(16) as u32);
                self.push(token);
            }
        }
        for &id in (content.acts[game.act as usize].events.iter())
            .filter(|&&id| !game.visited_events.contains(&id) && game.event_allowed(content, id))
        {
            let mut token = Token::new(EVENT_DOMAIN, Place::EventPool, NONE);
            token.code(layout, Semantic::Event, id as u32);
            self.push(token);
        }
        let vocabs = encounter_vocabs(content);
        for (elite, namespace) in [(false, Semantic::EncounterNormal), (true, Semantic::EncounterElite)] {
            for id in public_encounters(game, content, elite) {
                let mut token = Token::new(ENCOUNTER_DOMAIN, Place::EncounterPool, NONE);
                let value = encounter_id(&vocabs[elite as usize], id) as u32 - 1;
                token.code(layout, namespace, value);
                self.push(token);
            }
        }
        if let Some(crystal) = &game.crystal {
            for &item in &crystal.revealed {
                if let Some(&(x, y, width, height, kind)) = crystal.items.get(item) {
                    let mut token = Token::new(CRYSTAL_DOMAIN, Place::Crystal, NONE);
                    token.code(layout, Semantic::CrystalKind, 0);
                    token.code(layout, Semantic::CrystalKind, kind as u32);
                    token.code(layout, Semantic::CrystalRow, (x as u32).min(10));
                    token.code(layout, Semantic::CrystalColumn, (y as u32).min(10));
                    token.code(layout, Semantic::CrystalWidth, (width as u32).saturating_sub(1).min(3));
                    token.code(layout, Semantic::CrystalHeight, (height as u32).saturating_sub(1).min(3));
                    self.push(token);
                }
            }
            for cell in (0..crystal.clear.len()).filter(|&cell| crystal.clear[cell]) {
                let mut token = Token::new(CRYSTAL_DOMAIN, Place::Crystal, NONE);
                token.code(layout, Semantic::CrystalKind, 1);
                token.code(layout, Semantic::CrystalRow, (cell / 11) as u32);
                token.code(layout, Semantic::CrystalColumn, (cell % 11) as u32);
                self.push(token);
            }
        }
    }

    /// Pending run effects, queued combat effects, plays and parasol offers, next first.
    fn queue(&mut self) {
        let game = self.ctx.game;
        self.extend(|items| {
            for &effect in game.run_queue.iter().rev() {
                items.item();
                items.run_effect(effect);
            }
            if let Some(combat) = game.combat() {
                for pending in combat.queue.iter().rev() {
                    items.queued(pending.effect, pending.context);
                }
                for play in combat.plays.iter().rev() {
                    items.item();
                    let lamp = combat.unsettling_lamp == Some(play.card.id);
                    let plays = play.plays as f32;
                    if play.manual {
                        let numbers = [play.energy.max(0) as f32, play.stars.max(0) as f32, plays];
                        items.play(1, numbers, lamp);
                    } else {
                        items.play(0, [0.0, 0.0, plays], lamp);
                    }
                    items.card(play.card);
                }
                if combat.extra_turn && !combat.setup {
                    items.item();
                    items.play(2, [0.0; 3], false);
                }
            }
            if matches!(game.reward_continuation, Some(RewardContinuation::AddRandomCurse)) {
                items.item();
                items.play(3, [0.0; 3], false);
            }
            for item in &game.parasol {
                items.item();
                items.shop(item);
            }
        });
    }
}

fn token_order(left: &Token, right: &Token) -> std::cmp::Ordering {
    let bits = |values: &[f32]| values.iter().map(|value| value.to_bits()).collect::<Vec<_>>();
    (left.c.cmp(&right.c))
        .then_with(|| bits(&left.a).cmp(&bits(&right.a)))
        .then_with(|| bits(&left.f).cmp(&bits(&right.f)))
}

/// The enemy's intended damage to the player this turn after every modifier, and its hits.
fn intent(game: &Game, content: &Content, index: usize) -> (f32, f32) {
    let enemy = &game.combat().unwrap().enemies[index];
    if enemy.stunned > 0 || enemy.creature.hp <= 0 {
        return (0.0, 0.0);
    }
    let context = Context { source: Actor::Enemy(index), ..Context::player() };
    let mut out = (0.0, 0.0);
    fn walk(game: &Game, content: &Content, context: Context, effects: &[Effect], out: &mut (f32, f32)) {
        let hit = |amount, kind| {
            let base = game.amount(content, context, amount);
            game.modify_damage(content, Some(Actor::Player), base, kind, context, None) as f32
        };
        for &effect in effects {
            let (damage, hits) = match effect {
                Effect::Attack(Target::Player, amount, hits) => {
                    (hit(amount, DamageKind::Attack), hits as f32)
                }
                Effect::AttackMany(Target::Player, amount, hits) => {
                    (hit(amount, DamageKind::Attack), game.amount(content, context, hits).max(0) as f32)
                }
                Effect::MoveDamage(Target::Player, amount) => (hit(amount, DamageKind::Move), 1.0),
                Effect::If(condition, yes, no) => {
                    let selected = if game.condition(content, context, condition) { yes } else { no };
                    walk(game, content, context, selected, out);
                    continue;
                }
                Effect::Repeat(amount, effects) => {
                    for _ in 0..game.amount(content, context, amount).max(0) {
                        walk(game, content, context, effects, out);
                    }
                    continue;
                }
                _ => continue,
            };
            out.0 += damage * hits;
            out.1 += hits;
        }
    }
    let effects = content.enemies[enemy.creature.id as usize].moves[enemy.move_index].effects;
    walk(game, content, context, effects, &mut out);
    out
}

/// The HP the player loses by ending the turn now: the enemy turn simulated on the public state
/// (the draw pile's unknown middle and every RNG canonicalized), so block and every hook count.
/// Ending the turn draws from the hidden draw pile next turn, too many outcomes to enumerate: its
/// preview follows one canonical sample of the hidden state up to the next decision.
fn end_turn_outcome(game: &Game, content: &Content) -> Option<crate::game::ActionOutcome> {
    let before = game.combat()?.drawn;
    let mut next = game.clone();
    next.replaying = false;
    canonicalize_combat_hidden(&mut next);
    resample_canonical_combat_hidden(&mut next, 0);
    next.step(content, Action::EndTurn).ok()?;
    let drawn = next.combat().map_or(0, |combat| combat.drawn.saturating_sub(before));
    Some(game.outcome_to(&next, drawn as f32, 0.0))
}

type CanonicalMap = (Arc<[Token]>, Arc<[u32]>);

struct CanonicalMapCache {
    source: Arc<[MapNode]>,
    fur_coat: Option<Vec<(u8, u8)>>,
    spoils: Option<(u8, u8)>,
    ancient: Option<Id>,
    bosses: [Option<Id>; 2],
    layout: Layout,
    map: CanonicalMap,
}

/// Map nodes in canonical order (token index = canonical id), then edges from parent to link.
fn canonical_map(game: &Game, content: &Content, layout: Layout) -> CanonicalMap {
    let mut order = (0..game.map.nodes.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| {
        let node = &game.map.nodes[index];
        (node.floor, node.lane, node.room as usize)
    });
    for pair in order.windows(2) {
        let (left, right) = (&game.map.nodes[pair[0]], &game.map.nodes[pair[1]]);
        assert_ne!((left.floor, left.lane), (right.floor, right.lane));
    }
    let mut ids = vec![0; game.map.nodes.len()];
    for (position, &index) in order.iter().enumerate() {
        ids[index] = position as u32;
    }
    let bosses = map_bosses(content);
    let mut boss = 0;
    let mut tokens = Vec::with_capacity(order.len());
    for &index in &order {
        let node = &game.map.nodes[index];
        let kind = map_node_kind(game, content, &bosses, node, boss);
        boss += (node.room == Room::Boss) as usize;
        let mut token = Token::new(MAP_NODE_DOMAIN, Place::MapNode, NONE);
        token.code(layout, Semantic::MapNodeKind, kind as u32);
        if game.fur_coat_act == Some(game.run.act) && game.fur_coat.contains(&(node.lane, node.floor)) {
            token.code(layout, Semantic::MapModifier, 0);
        }
        if game.spoils == Some((node.lane, node.floor)) {
            token.code(layout, Semantic::MapModifier, 1);
        }
        tokens.push(token);
    }
    for &index in &order {
        let node = &game.map.nodes[index];
        for &next in &node.next {
            let child = game.map.nodes.get(next).expect("invalid map edge");
            assert!(child.floor > node.floor);
            let mut token = Token::new(MAP_EDGE_DOMAIN, Place::MapEdge, ids[index]);
            token.link = ids[next];
            tokens.push(token);
        }
    }
    (tokens.into(), ids.into())
}

fn cached_canonical_map(
    game: &Game,
    content: &Content,
    layout: Layout,
    cache: &mut Option<CanonicalMapCache>,
) -> CanonicalMap {
    let fur_coat = (game.fur_coat_act == Some(game.run.act)).then_some(game.fur_coat.as_slice());
    if cache.as_ref().is_none_or(|cache| {
        !Arc::ptr_eq(&cache.source, &game.map.nodes)
            || cache.fur_coat.as_deref() != fur_coat
            || cache.spoils != game.spoils
            || cache.ancient != game.ancient
            || cache.bosses != game.bosses
            || cache.layout != layout
    }) {
        *cache = Some(CanonicalMapCache {
            source: game.map.nodes.clone(),
            fur_coat: fur_coat.map(<[_]>::to_vec),
            spoils: game.spoils,
            ancient: game.ancient,
            bosses: game.bosses,
            layout,
            map: canonical_map(game, content, layout),
        });
    }
    cache.as_ref().unwrap().map.clone()
}

fn build_observation(game: &Game, content: &Content, layout: Layout) -> Observation {
    build_observation_with_map(game, content, layout, None)
}

fn build_observation_cached(
    game: &Game,
    content: &Content,
    layout: Layout,
    cache: &mut Option<CanonicalMapCache>,
) -> Observation {
    let map = cached_canonical_map(game, content, layout, cache);
    build_observation_with_map(game, content, layout, Some(&map))
}

fn build_observation_with_map(
    game: &Game,
    content: &Content,
    layout: Layout,
    map: Option<&CanonicalMap>,
) -> Observation {
    let (actions, legal) = candidate_actions(game, content);
    build_observation_with_candidates(game, content, layout, map, actions, legal)
}

fn candidate_signature(
    game: &Game,
    content: &Content,
    layout: Layout,
    action: &Action,
) -> Option<CandidateRow> {
    let observation = build_observation(game, content, layout);
    (observation.candidates.into_iter()).find(|candidate| &candidate.action == action)
}

fn build_observation_with_candidates(
    game: &Game,
    content: &Content,
    layout: Layout,
    map: Option<&CanonicalMap>,
    actions: Vec<Action>,
    legal: Vec<bool>,
) -> Observation {
    assert!(!actions.is_empty() || matches!(game.phase, Phase::Won | Phase::Dead));
    let (map, ids) = map.cloned().unwrap_or_else(|| canonical_map(game, content, layout));
    let masters = master_cards(&game.run.deck);
    let ctx = Ctx { game, content, layout, masters: &masters };
    let mut encoder = Encoder { ctx, tokens: map.to_vec() };
    let current = game.map.current.map_or(0, |index| ids[index]);
    encoder.state(current);
    let enemies = (0..encoder.tokens.len() as u32)
        .filter(|&index| encoder.tokens[index as usize].place == Place::Enemy)
        .collect::<Vec<_>>();
    let candidates = (actions.into_iter().zip(legal))
        .map(|(action, legal)| ctx.candidate(action, legal, &ids, &enemies))
        .collect();
    Observation {
        character: game.run.character as u8,
        globals: run_features(game, content),
        tokens: encoder.tokens,
        candidates,
    }
}

/// A forced decision's observation: only its single candidate, without tokens.
fn forced_observation(game: &Game, content: &Content, action: Action) -> Observation {
    Observation {
        character: game.run.character as u8,
        globals: run_features(game, content),
        tokens: Vec::new(),
        candidates: vec![CandidateRow {
            action,
            legal: true,
            token: Token::new(ACTION_DOMAIN, Place::Candidate, NONE),
            attached: Vec::new(),
        }],
    }
}

fn token_words(token: &Token, parent: u32, words: &mut Vec<u32>) {
    words.extend([token.domain as u32 | (token.place as u32) << 8, parent, token.link]);
    words.extend(&token.c);
    words.extend(token.a.iter().map(|value| value.to_bits()));
    words.extend(token.f.iter().map(|value| value.to_bits()));
}

/// The state tokens, then each candidate's legality, attachment count, token and attachments.
fn observation_words(row: &Observation) -> Vec<u32> {
    let mut words = Vec::new();
    for token in &row.tokens {
        token_words(token, token.parent, &mut words);
    }
    for candidate in &row.candidates {
        words.extend([candidate.legal as u32, candidate.attached.len() as u32]);
        token_words(&candidate.token, NONE, &mut words);
        for token in &candidate.attached {
            token_words(token, NONE, &mut words);
        }
    }
    words
}

fn packed_observation_digest(character: u8, globals: &[f32], counts: [u32; 2], words: &[u32]) -> u64 {
    let update = |digest: u64, value: u64| (digest ^ value).wrapping_mul(0x100_0000_01b3);
    let header = [VERSION, VALUE_MODEL_VERSION, character as u32, globals.len() as u32];
    let mut digest = (header.into_iter())
        .chain(globals.iter().map(|value| value.to_bits()))
        .chain(counts)
        .fold(0xcbf2_9ce4_8422_2325, |digest, value| update(digest, value as u64));
    let mut lanes = [digest ^ words.len() as u64, 1, 2, 3];
    let mut chunks = words.chunks_exact(4);
    for chunk in &mut chunks {
        for (lane, &value) in lanes.iter_mut().zip(chunk) {
            *lane = update(*lane, value as u64);
        }
    }
    for (lane, &value) in lanes.iter_mut().zip(chunks.remainder()) {
        *lane = update(*lane, value as u64);
    }
    digest = lanes.into_iter().fold(0, update);
    digest
}

fn observation_counts(row: &Observation) -> [u32; 2] {
    [row.tokens.len() as u32, row.candidates.len() as u32]
}

fn observation_digest(row: &Observation) -> u64 {
    packed_observation_digest(row.character, &row.globals, observation_counts(row), &observation_words(row))
}

fn enemy_move_id(content: &Content, enemy: Id, movement: usize) -> u32 {
    assert!(movement < content.enemies[enemy as usize].moves.len());
    content.enemies[..enemy as usize]
        .iter()
        .map(|enemy| enemy.moves.len() as u32)
        .sum::<u32>()
        + movement as u32
}

fn card_version_id(content: &Content, id: Id, upgraded: bool, variant: u8) -> u32 {
    if content.cards[id as usize].id == "CARD.MAD_SCIENCE" && variant > 0 {
        (content.cards.len() * 2 + variant as usize * 2 + upgraded as usize) as u32
    } else {
        id as u32 * 2 + upgraded as u32
    }
}

fn base_card_id(id: u32) -> u32 {
    id * 2
}

/// The turn is ending: the next turn step is the hand flush (Well-Laid Plans' pick).
fn ending(combat: &Combat) -> bool {
    (combat.queue.iter().rev()).find_map(|pending| match pending.effect {
        Effect::Step(Step::PlayedCleanup(_)) => None,
        Effect::Step(step) => Some(step),
        _ => None,
    }) == Some(Step::Flush)
}

fn event_actions(options: &[EventOption]) -> impl Iterator<Item = u8> + '_ {
    options.iter().filter_map(|option| {
        option.effects.iter().find_map(|effect| match effect {
            RunEffect::EventAction(action) => Some(*action),
            _ => None,
        })
    })
}

fn map_bosses(content: &Content) -> Vec<Id> {
    let mut bosses = Vec::new();
    for &id in content
        .acts
        .iter()
        .filter(|act| act.id != "ACT.FOUNDATION")
        .flat_map(|act| act.bosses)
    {
        if !bosses.contains(&id) {
            bosses.push(id);
        }
    }
    assert_eq!(bosses.len(), 12);
    bosses
}

fn map_node_kind(
    game: &Game,
    content: &Content,
    bosses: &[Id],
    node: &MapNode,
    boss: usize,
) -> usize {
    if node.floor == 0 {
        const ANCIENTS: [&str; 8] = [
            "EVENT.NEOW",
            "EVENT.OROBAS",
            "EVENT.PAEL",
            "EVENT.TEZCATARA",
            "EVENT.DARV",
            "EVENT.NONUPEIPE",
            "EVENT.TANX",
            "EVENT.VAKUU",
        ];
        return game
            .ancient
            .and_then(|id| {
                ANCIENTS
                    .iter()
                    .position(|&name| content.events[id as usize].id == name)
            })
            .map_or(0, |index| 6 + index);
    }
    if node.room == Room::Boss {
        return game
            .bosses
            .get(boss)
            .copied()
            .flatten()
            .and_then(|id| bosses.iter().position(|&candidate| candidate == id))
            .map_or(0, |index| 14 + index);
    }
    match node.room {
        Room::Unknown => 0,
        Room::Combat => 1,
        Room::Elite => 2,
        Room::Rest => 3,
        Room::Treasure => 4,
        Room::Shop => 5,
        Room::Event | Room::Boss => unreachable!(),
    }
}

fn phase_index(phase: &Phase) -> Option<usize> {
    (!matches!(phase, Phase::Won | Phase::Dead)).then(|| phase.variant() as usize)
}

fn canonical_progress(game: &Game) -> u8 {
    let floor = game.run.floor;
    let progress = match game.run.act {
        0 | 1 => floor,
        2 if (game.run.relics.iter())
            .any(|relic| relic.id as u32 == golden_compass() && relic.counters[0] == 3) =>
        {
            25 + floor
        }
        2 => 25 + floor + u8::from(floor >= 7) + u8::from(floor >= 10),
        3 => 54 + floor,
        act => panic!("invalid canonical-progress act {act}"),
    };
    assert!(progress <= 72, "canonical progress {progress} exceeds 72");
    progress
}

fn master_cards(deck: &[Card]) -> HashMap<u32, usize> {
    deck.iter()
        .enumerate()
        .filter(|(_, card)| card.instance != 0)
        .map(|(index, card)| (card.instance, index + 1))
        .collect()
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

/// The relics a run's bag starts with, sorted.
fn starting_relic_pool(game: &Game, content: &Content) -> Vec<Id> {
    let character = content.characters[game.run.character as usize].relic_pool;
    let mut pool = (content.acts[0].relics.iter().chain(character))
        .copied()
        .filter(|&id| crate::game::relic_group(id).is_some())
        .collect::<Vec<_>>();
    pool.sort_unstable();
    pool.dedup();
    pool
}

fn public_relic_bag(game: &Game, layout: Layout) -> Vec<Vec<Id>> {
    let trader = matches!(game.phase, Phase::Event(id, _) if Some(id) == layout.relic_trader);
    let mut bag = game.relic_deques.to_vec();
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
            if game.run.relics.iter().any(|relic| relic.id == id) {
                continue;
            }
            if let Some(rarity) = crate::game::relic_group(id)
                && !bag[rarity].contains(&id)
            {
                bag[rarity].push(id);
            }
        }
    }
    bag
}

fn weak_encounters(game: &Game, content: &Content) -> usize {
    (game.encounters.iter())
        .take_while(|&&id| content.encounters[id as usize].id.contains("_WEAK"))
        .count()
}

/// The encounters left in the bag of the act's current stage (weak encounters, then regular
/// ones, or the elites): its pool less those fought since the bag was last refilled.
fn public_encounters(game: &Game, content: &Content, elite: bool) -> Vec<Id> {
    let act = &content.acts[game.act as usize];
    let (list, visited, weak) = if elite {
        (&game.elites, game.elites_visited as usize, 0)
    } else {
        (
            &game.encounters,
            game.encounters_visited as usize,
            weak_encounters(game, content),
        )
    };
    let start = if visited < weak { 0 } else { weak };
    let mut pool = if elite {
        act.elites.to_vec()
    } else {
        (act.encounters.iter().copied())
            .filter(|&id| content.encounters[id as usize].id.contains("_WEAK") == (visited < weak))
            .collect()
    };
    pool.sort_unstable();
    if visited >= list.len() || pool.is_empty() {
        return pool;
    }
    let refill = start + (visited - start) / pool.len() * pool.len();
    pool.retain(|id| !list[refill..visited].contains(id));
    pool
}

fn candidate_actions(game: &Game, content: &Content) -> (Vec<Action>, Vec<bool>) {
    let legal = game.actions(content);
    let mask = vec![true; legal.len()];
    (legal, mask)
}

fn public_event_slots(content: &Content, id: Id) -> &'static [usize] {
    match content.events[id as usize].id {
        "EVENT.ABYSSAL_BATHS" => &[0],
        "EVENT.COLORFUL_PHILOSOPHERS" => &[0, 1, 2],
        "EVENT.CRYSTAL_SPHERE" => &[0],
        "EVENT.DENSE_VEGETATION" => &[0, 3],
        "EVENT.DOLL_ROOM" => &[0, 1, 2],
        "EVENT.ENDLESS_CONVEYOR" => &[0, 1, 2],
        "EVENT.JUNGLE_MAZE_ADVENTURE" => &[0, 1],
        "EVENT.LUMINOUS_CHOIR" | "EVENT.RANWID_THE_ELDER" | "EVENT.WHISPERING_HOLLOW" => &[0],
        "EVENT.PUNCH_OFF" | "EVENT.THE_LANTERN_KEY" | "EVENT.WAR_HISTORIAN_REPY" => &[3],
        "EVENT.SLIPPERY_BRIDGE" => &[0],
        "EVENT.TINKER_TIME" => &[0, 1, 3],
        _ => &[],
    }
}

fn event_option_payload(game: &Game, content: &Content, index: usize) -> Option<i32> {
    let Phase::Event(id, _) = game.phase else {
        return None;
    };
    match content.events[id as usize].id {
        "EVENT.COLORFUL_PHILOSOPHERS"
        | "EVENT.DOLL_ROOM"
        | "EVENT.JUNGLE_MAZE_ADVENTURE"
        | "EVENT.TINKER_TIME" => game
            .event_data
            .get(index)
            .copied()
            .map(|value| i32::try_from(value).expect("public event option exceeds i32")),
        _ => None,
    }
}

fn effect_amounts(effect: Effect) -> Vec<(Amount, f32)> {
    use Effect::*;
    match effect {
        Attack(_, amount, _)
        | OstyAttack(_, amount, _)
        | MoveDamage(_, amount)
        | Damage(_, amount)
        | LoseHp(_, amount)
        | Block(_, amount)
        | BlockNextTurn(amount)
        | DodgeRoll(amount)
        | DrawBlockIf(_, amount)
        | RawBlock(_, amount)
        | Heal(_, amount)
        | MaxHp(amount)
        | Bomb(amount)
        | Panache(amount)
        | RollingBoulder(amount)
        | ToricToughness(amount)
        | ApplyPower(_, _, amount)
        | ApplyDebuff(_, _, amount)
        | StackPower(_, _, amount)
        | Misery(amount)
        | TemporaryStrength(_, _, amount)
        | Aggression(amount)
        | FlakCannon(amount)
        | ExhaustForBlock(amount)
        | ExhaustForAttack(_, amount)
        | ExhaustAttackStep(_, amount, _)
        | Forge(amount)
        | GrowCard(amount)
        | GrowDrawn(amount)
        | GrowAll(_, amount)
        | PersistCard(amount) => {
            vec![(amount, 30.0)]
        }
        AttackMany(_, first, second) | OstyAttackMany(_, first, second) => {
            vec![(first, 30.0), (second, 30.0)]
        }
        HealPercent(_, amount) => vec![(amount, 100.0)],
        Gold(amount) => vec![(amount, 1_000.0)],
        DrawAmount(amount)
        | ChooseDraw(amount)
        | RandomColorless(_, amount, _)
        | RandomColorlessOther(_, amount)
        | RandomCharacter(_, amount, _)
        | RandomCharacterCost0(_, amount, _)
        | RandomCardOp(_, _, _, amount)
        | AutoPlayRandom(_, _, amount)
        | SelectAmount(_, _, amount, _, _)
        | AutoPlayDraw(amount, _)
        | EvokeMany(amount)
        | EvokeLast(amount)
        | Stars(amount)
        | Summon(amount)
        | Repeat(amount, _) => vec![(amount, 10.0)],
        _ => vec![],
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
    let replaying = game.replaying;
    let encounters = public_encounters(game, content, false);
    let elites = public_encounters(game, content, true);
    game.replaying = false;
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
        unknown.sort_by_key(|card| (card_key(card), card.instance));
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
        if game.run.relics.iter().any(|owned| owned.id == relic) {
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
    // The rooms still ahead are rolled again, from what is left of their bags; a replay keeps
    // the trace's next encounters.
    let act = &content.acts[game.act as usize];
    if !replaying {
        let (weak, regular): (Vec<_>, Vec<_>) = (act.encounters.iter())
            .partition(|&&id| content.encounters[id as usize].id.contains("_WEAK"));
        let weak_count = weak_encounters(game, content);
        let (length, visited) = (game.encounters.len(), game.encounters_visited as usize);
        let mut list = std::mem::take(&mut game.encounters);
        list.truncate(visited);
        if visited < weak_count {
            game.grab_encounters(content, &weak, encounters, weak_count - visited, &mut list);
            game.grab_encounters(content, &regular, vec![], length - weak_count, &mut list);
        } else if visited < length {
            game.grab_encounters(content, &regular, encounters, length - visited, &mut list);
        }
        game.encounters = list;
        let (length, visited) = (game.elites.len(), game.elites_visited as usize);
        let mut list = std::mem::take(&mut game.elites);
        list.truncate(visited);
        let left = length.saturating_sub(visited);
        game.grab_encounters(content, act.elites, elites, left, &mut list);
        game.elites = list;
    }
    let mut darv: Vec<_> = (content.event_id("EVENT.DARV").into_iter())
        .filter(|&id| game.ancient != Some(id))
        .collect();
    for index in 0..game.later_rooms.len() {
        let count = game.rngs.up_front.below(darv.len() as u32 + 1) as usize;
        let shared: Vec<_> = darv.drain(..count).collect();
        game.later_rooms[index] = game.roll_rooms(content, game.later_rooms[index].act, &shared);
    }
    game.events = act.events.to_vec();
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
}

fn resample_combat_hidden(game: &mut Game, seed: u64) {
    canonicalize_combat_hidden(game);
    resample_canonical_combat_hidden(game, seed);
}

fn canonicalize_combat_hidden(game: &mut Game) {
    if let Phase::Combat(combat) = &mut game.phase
        && combat.choice.is_none()
    {
        let (top, bottom) = (combat.known_draw_top, combat.known_draw_bottom);
        let end = combat.draw.len() - top;
        combat.draw[bottom..end].sort_by_key(|card| (card_key(card), card.instance));
    }
}

fn resample_canonical_combat_hidden(game: &mut Game, seed: u64) {
    game.seed = seed as u32;
    game.rngs = Rngs::from_seed(seed);
    if game.event_rng.is_some() {
        game.event_rng = Some(Rng::from_seed(seed ^ 0x4556_454e_5452_4e47));
    }
    if let Phase::Combat(combat) = &mut game.phase
        && combat.choice.is_none()
    {
        let (top, bottom) = (combat.known_draw_top, combat.known_draw_bottom);
        let end = combat.draw.len() - top;
        let unknown = &mut combat.draw[bottom..end];
        Rng::from_seed(seed ^ 0x4849_4444_454e_524e).shuffle(unknown);
    }
}

fn run_features(game: &Game, content: &Content) -> Vec<f32> {
    if matches!(game.phase, Phase::Won | Phase::Dead) {
        return vec![0.0; PUBLIC_GLOBALS];
    }
    let hp = game
        .combat()
        .map_or(game.run.hp, |combat| combat.player.hp)
        .max(0) as f32;
    let relics = &game.run.relics;
    let deck = &game.run.deck;
    let resources = [
        game.run.max_hp as f32,
        deck.iter()
            .map(|card| card.upgrades as usize)
            .sum::<usize>() as f32,
        relics.len() as f32,
        game.run.potions.iter().flatten().count() as f32,
        game.run.gold as f32,
        deck.len() as f32,
        relics
            .iter()
            .filter(|relic| crate::game::relic_group(relic.id) == Some(1))
            .count() as f32,
        relics
            .iter()
            .filter(|relic| crate::game::relic_group(relic.id) == Some(2))
            .count() as f32,
        game.run
            .potions
            .iter()
            .flatten()
            .filter(|id| crate::foundation::UNCOMMON_POTIONS.contains(id))
            .count() as f32,
        game.run
            .potions
            .iter()
            .flatten()
            .filter(|id| crate::foundation::RARE_POTIONS.contains(id))
            .count() as f32,
        deck.iter()
            .filter(|card| content.cards[card.id as usize].rarity == CardRarity::Uncommon)
            .count() as f32,
        deck.iter()
            .filter(|card| content.cards[card.id as usize].rarity == CardRarity::Rare)
            .count() as f32,
    ];
    [
        canonical_progress(game) as f32 / (TERMINAL_CATEGORIES - 1) as f32,
        hp,
    ]
    .into_iter()
    .chain(resources)
    .chain(resources.map(|resource| hp * resource))
    .collect()
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

#[path = "labels.rs"]
mod labels;

#[path = "value_model.rs"]
mod value_model;

pub use value_model::ValueModel;
#[cfg(test)]
use value_model::{content_fingerprint, valid_model_shape};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Pael's Tooth's relic id: the relic row its stored cards attach to.
pub(crate) fn paels_tooth() -> u32 {
    static ID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| {
        crate::foundation_content()
            .relic_id("RELIC.PAELS_TOOTH")
            .unwrap() as u32
    });
    *ID
}

/// Golden Compass's relic id: it counts the act whose map it replaced, plus one.
fn golden_compass() -> u32 {
    static ID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| {
        crate::foundation_content()
            .relic_id("RELIC.GOLDEN_COMPASS")
            .unwrap() as u32
    });
    *ID
}
