#![cfg_attr(not(feature = "python"), allow(dead_code))]

use crate::*;
#[cfg(feature = "python")]
use rayon::prelude::*;
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    io,
    ops::Deref,
    path::Path,
    sync::{Arc, Mutex},
};

const MAGIC: &[u8; 8] = b"STSVALUE";
const VERSION: u32 = 56;
const VALUE_MODEL_VERSION: u32 = 72;
const VALUE_CATEGORIES: usize = 83;
const TOKEN_CATEGORICAL: usize = 10;
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
const EVENT_COLLECTION: usize = 13;
const CARD_ZONES: usize = 5;
const DECK_ZONE: usize = 0;
const HAND_ZONE: usize = 1;
const DRAW_ZONE: usize = 2;
const DISCARD_ZONE: usize = 3;
const EXHAUST_ZONE: usize = 4;
const ATTACHED_CARD_ZONE: usize = CARD_ZONES;
const COMBAT_OFFER_ZONE: usize = 5;
const OFFER_ZONE: usize = 6;
const HISTORY_COURSE_ZONE: usize = 7;
const PAEL_ZONE: usize = 8;
const SLIPPERY_PREVIOUS_ZONE: usize = 9;
const SLIPPERY_CANDIDATE_ZONE: usize = 10;
const PLAYING_ZONE: usize = 11;
const AUTOPLAY_ZONE: usize = 12;
const CARD_POOL_ZONE: usize = 13;
const HISTORY_COURSE_STATUS: u32 = 6;
const PARASOL_CARD_KIND: usize = 6;
const ANCIENT_RELIC_KIND: usize = 16;
const NIGHTMARE_CARD_KIND: usize = 17;
const TOY_BOX_RELIC_KIND: usize = 18;
const DAMPENED_CARD_KIND: usize = 18;
const QUEUED_CARD_KIND: usize = 19;
const RESUME_CARD_KIND: usize = 20;
const EFFECT_DETAIL_KIND: usize = 21;
const CURRENT_EVENT_OPTION_KIND: usize = 22;
const SEEN_RELIC_KIND: usize = 11;
const VIRTUAL_POWER_KIND: usize = 6;
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
const PHASES: usize = 14;
const ENCHANTMENTS: usize = 22;
const MAP_FLOORS: usize = 18;
const PUBLIC_GLOBALS: usize = 0;
const MODEL_WIDTH: usize = 128;
const MODEL_LAYERS: usize = 4;
const MODEL_HEADS: usize = 8;
const MODEL_FEEDFORWARD: usize = 384;
const POSITION_CAPS: [u32; 14] = [32, 64, 16, 64, 256, 256, 256, 256, 256, 256, 11, 11, 4, 4];
#[cfg(test)]
const ENEMY_SLOTS: usize = 8;
const KNOWN_DRAW_SLOTS: usize = u8::MAX as usize;
const ACTION_KINDS: usize = 32;
const ACTION_VALUES: usize = 16;
const CONTINUATION_BYTES: usize = 16_384;
const DOMAIN_NAMES: [&str; 16] = [
    "run",
    "phase",
    "card",
    "actor",
    "power",
    "history",
    "status",
    "relic",
    "potion",
    "orb",
    "event",
    "encounter",
    "crystal",
    "continuation",
    "map_node",
    "map_edge",
];
const DOMAIN_WIDTHS: [(usize, usize, usize, usize); 16] = [
    (24, 16, 6, 36),
    (16, 8, 16, 16),
    (20, 16, 73, 23),
    (12, 16, 5, 27),
    (8, 4, 5, 8),
    (6, 29, 4, 32),
    (20, 8, 72, 17),
    (12, 4, 8, 16),
    (6, 2, 5, 6),
    (6, 2, 5, 6),
    (4, 1, 4, 4),
    (4, 0, 3, 4),
    (8, 2, 6, 10),
    (24, 16, 80, 24),
    (12, 0, 7, 8),
    (8, 0, 2, 4),
];
const RUN_DOMAIN: usize = 0;
const PHASE_DOMAIN: usize = 1;
const CARD_DOMAIN: usize = 2;
const ACTOR_DOMAIN: usize = 3;
const POWER_DOMAIN: usize = 4;
const HISTORY_DOMAIN: usize = 5;
const STATUS_DOMAIN: usize = 6;
const RELIC_DOMAIN: usize = 7;
const POTION_DOMAIN: usize = 8;
const ORB_DOMAIN: usize = 9;
const EVENT_DOMAIN: usize = 10;
const ENCOUNTER_DOMAIN: usize = 11;
const CRYSTAL_DOMAIN: usize = 12;
const CONTINUATION_DOMAIN: usize = 13;
const MAP_NODE_DOMAIN: usize = 14;
const MAP_EDGE_DOMAIN: usize = 15;
const NO_NODE: u32 = u32::MAX;
const STATE_SCOPE: i32 = -1;
const PHASE_SCOPE: i32 = -2;
const ACTION_U: usize = 15;
const ACTION_S: usize = 12;
const ACTION_C: usize = 8;
const ACTION_F: usize = 20;

#[repr(usize)]
#[derive(Clone, Copy)]
enum Semantic {
    Character,
    Card,
    Power,
    Relic,
    Potion,
    Enemy,
    EnemyMove,
    Orb,
    OrbTiming,
    Event,
    EncounterNormal,
    EncounterElite,
    EncounterBoss,
    Enchantment,
    CardType,
    Rarity,
    Room,
    Target,
    Pile,
    Phase,
    Action,
    Effect,
    RunEffect,
    Condition,
    Filter,
    CardOp,
    Scale,
    EnemyPosition,
    PowerPosition,
    OrbPosition,
    MapFloorPosition,
    DeckOrigin,
    DrawTopPosition,
    DrawBottomPosition,
    RelicPosition,
    WaxPosition,
    ContinuationPosition,
    CrystalRow,
    CrystalColumn,
    CrystalWidth,
    CrystalHeight,
    FlagBit,
    TurnFlagBit,
    TagBit,
    RunKind,
    PhaseKind,
    CardZone,
    CardRole,
    ActorKind,
    PowerSource,
    HistoryKind,
    StatusKind,
    RelicKind,
    PotionKind,
    OrbKind,
    EventKind,
    EncounterKind,
    CrystalKind,
    ContinuationKind,
    ContinuationVariant,
    MapNodeKind,
    MapEdgeKind,
    RouteKind,
    Requirement,
    TinkerRider,
    ConveyorDish,
    Boolean,
    TokenRole,
    Collection,
    Count,
}

const SEMANTIC_NAMES: [&str; Semantic::Count as usize] = [
    "character",
    "card",
    "power",
    "relic",
    "potion",
    "enemy",
    "enemy_move",
    "orb",
    "orb_timing",
    "event",
    "encounter_normal",
    "encounter_elite",
    "encounter_boss",
    "enchantment",
    "card_type",
    "rarity",
    "room",
    "target",
    "pile",
    "phase",
    "action",
    "effect",
    "run_effect",
    "condition",
    "filter",
    "card_op",
    "scale",
    "enemy_position",
    "power_position",
    "orb_position",
    "map_floor_position",
    "deck_origin",
    "draw_top_position",
    "draw_bottom_position",
    "relic_position",
    "wax_position",
    "continuation_position",
    "crystal_row",
    "crystal_column",
    "crystal_width",
    "crystal_height",
    "flag_bit",
    "turn_flag_bit",
    "tag_bit",
    "run_kind",
    "phase_kind",
    "card_zone",
    "card_role",
    "actor_kind",
    "power_source",
    "history_kind",
    "status_kind",
    "relic_kind",
    "potion_kind",
    "orb_kind",
    "event_kind",
    "encounter_kind",
    "crystal_kind",
    "continuation_kind",
    "continuation_variant",
    "map_node_kind",
    "map_edge_kind",
    "route_kind",
    "requirement",
    "tinker_rider",
    "conveyor_dish",
    "boolean",
    "token_role",
    "collection",
];

#[derive(Clone, Debug, PartialEq)]
struct DomainRow {
    scope: i32,
    u: Vec<u32>,
    s: Vec<i32>,
    c: Vec<u32>,
    f: Vec<f32>,
}

impl DomainRow {
    fn new(domain: usize, scope: i32) -> Self {
        let (u, s, c, f) = DOMAIN_WIDTHS[domain];
        Self {
            scope,
            u: vec![0; u],
            s: vec![0; s],
            c: vec![0; c],
            f: vec![0.0; f],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum DomainRows {
    Owned(Vec<DomainRow>),
    Shared(Arc<[DomainRow]>),
}

impl Deref for DomainRows {
    type Target = [DomainRow];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(rows) => rows,
            Self::Shared(rows) => rows,
        }
    }
}

impl<'a> IntoIterator for &'a DomainRows {
    type Item = &'a DomainRow;
    type IntoIter = std::slice::Iter<'a, DomainRow>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CandidateRow {
    action: Action,
    u: [u32; ACTION_U],
    s: [i32; ACTION_S],
    c: [u32; ACTION_C],
    f: [f32; ACTION_F],
    legal: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ObservationV56 {
    character: u8,
    globals: Vec<f32>,
    domains: [DomainRows; 16],
    candidates: Vec<CandidateRow>,
    potential: f32,
}

const POOL_COLLECTIONS: [usize; 6] = [
    RELIC_COLLECTION,
    POTION_COLLECTION,
    EVENT_COLLECTION,
    ENCOUNTER_COLLECTION,
    DELAYED_COLLECTION,
    CRYSTAL_COLLECTION,
];

#[derive(Clone, Copy)]
struct Layout {
    characters: usize,
    semantic_offsets: [u32; Semantic::Count as usize],
    semantic_sizes: [u32; Semantic::Count as usize],
    tinker_time: Option<Id>,
    slippery_bridge: Option<Id>,
    relic_trader: Option<Id>,
    shared_relics: &'static [Id],
}

impl Layout {
    fn new(content: &Content) -> Self {
        let characters = content.characters.len();
        let mut semantic_sizes = [0; Semantic::Count as usize];
        for (namespace, size) in [
            (Semantic::Character, content.characters.len()),
            (Semantic::Card, content.cards.len() * 2 + 512),
            (Semantic::Power, content.powers.len()),
            (Semantic::Relic, content.relics.len()),
            (Semantic::Potion, content.potions.len()),
            (Semantic::Enemy, content.enemies.len()),
            (
                Semantic::EnemyMove,
                content
                    .enemies
                    .iter()
                    .map(|enemy| enemy.moves.len())
                    .sum::<usize>()
                    + 1,
            ),
            (Semantic::Orb, content.orbs.len()),
            (Semantic::OrbTiming, 8),
            (Semantic::Event, content.events.len()),
            (Semantic::EncounterNormal, 57),
            (Semantic::EncounterElite, 13),
            (Semantic::EncounterBoss, 13),
            (Semantic::Enchantment, ENCHANTMENTS),
            (Semantic::CardType, 6),
            (Semantic::Rarity, 7),
            (Semantic::Room, 8),
            (Semantic::Target, 9),
            (Semantic::Pile, 5),
            (Semantic::Phase, PHASES),
            (Semantic::Action, ACTION_KINDS),
            (Semantic::Effect, 119),
            (Semantic::RunEffect, 36),
            (Semantic::Condition, 23),
            (Semantic::Filter, 16),
            (Semantic::CardOp, 18),
            (Semantic::Scale, 47),
            (Semantic::EnemyPosition, 33),
            (Semantic::PowerPosition, 65),
            (Semantic::OrbPosition, 17),
            (Semantic::MapFloorPosition, 65),
            (Semantic::DeckOrigin, 257),
            (Semantic::DrawTopPosition, 257),
            (Semantic::DrawBottomPosition, 257),
            (Semantic::RelicPosition, 257),
            (Semantic::WaxPosition, 257),
            (Semantic::ContinuationPosition, 257),
            (Semantic::CrystalRow, 11),
            (Semantic::CrystalColumn, 11),
            (Semantic::CrystalWidth, 4),
            (Semantic::CrystalHeight, 4),
            (Semantic::FlagBit, 16),
            (Semantic::TurnFlagBit, 16),
            (Semantic::TagBit, 16),
            (Semantic::RunKind, 32),
            (Semantic::PhaseKind, 32),
            (Semantic::CardZone, CARD_POOL_ZONE + 1),
            (Semantic::CardRole, 16),
            (Semantic::ActorKind, 4),
            (Semantic::PowerSource, 2),
            (Semantic::HistoryKind, 4),
            (Semantic::StatusKind, 12),
            (Semantic::RelicKind, 16),
            (Semantic::PotionKind, 8),
            (Semantic::OrbKind, 8),
            (Semantic::EventKind, 4),
            (Semantic::EncounterKind, 5),
            (Semantic::CrystalKind, 9),
            (Semantic::ContinuationKind, 10),
            (Semantic::ContinuationVariant, 128),
            (Semantic::MapNodeKind, 3),
            (Semantic::MapEdgeKind, 2),
            (Semantic::RouteKind, 4),
            (Semantic::Requirement, 4),
            (Semantic::TinkerRider, 9),
            (Semantic::ConveyorDish, 8),
            (Semantic::Boolean, 256),
            (Semantic::TokenRole, 16),
            (Semantic::Collection, 16),
        ] {
            semantic_sizes[namespace as usize] = size as u32;
        }
        let mut semantic_offsets = [0; Semantic::Count as usize];
        let mut next = 1u32;
        for (offset, &size) in semantic_offsets.iter_mut().zip(&semantic_sizes) {
            *offset = next;
            next = next
                .checked_add(size)
                .expect("semantic vocabulary overflow");
        }
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
        let shared_relics = content.acts.first().map_or(&[][..], |act| act.relics);
        Self {
            characters,
            semantic_offsets,
            semantic_sizes,
            tinker_time,
            slippery_bridge,
            relic_trader,
            shared_relics,
        }
    }

    fn semantic(self, namespace: Semantic, value: u32) -> u32 {
        assert!(
            value < self.semantic_sizes[namespace as usize],
            "semantic namespace {} value {value} exceeds {}",
            namespace as usize,
            self.semantic_sizes[namespace as usize]
        );
        self.semantic_offsets[namespace as usize] + value
    }

    fn concept_vocab(self) -> usize {
        (self.semantic_offsets[Semantic::Count as usize - 1]
            + self.semantic_sizes[Semantic::Count as usize - 1]) as usize
    }
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

fn push_semantic(row: &mut DomainRow, layout: Layout, namespace: Semantic, value: u32) {
    let slot = row
        .c
        .iter()
        .position(|&value| value == 0)
        .expect("semantic row overflow");
    row.c[slot] = layout.semantic(namespace, value);
}

fn push_boolean(row: &mut DomainRow, layout: Layout, field: u32, value: bool) {
    push_semantic(row, layout, Semantic::Boolean, field * 2 + value as u32);
}

fn push_bits(row: &mut DomainRow, layout: Layout, namespace: Semantic, bits: u32) {
    for bit in 0..16 {
        if bits & 1 << bit != 0 {
            push_semantic(row, layout, namespace, bit);
        }
    }
}

fn push_filter_semantics(row: &mut DomainRow, layout: Layout, kind: usize, argument: usize) {
    let (kind, argument) = (row.u[kind], row.u[argument]);
    push_semantic(row, layout, Semantic::Filter, kind);
    match kind {
        1 | 3 | 5 => {
            push_semantic(row, layout, Semantic::CardType, argument);
            if kind == 3 {
                push_bits(row, layout, Semantic::TurnFlagBit, row.s[0] as u32);
            }
        }
        4 | 9 => push_bits(row, layout, Semantic::FlagBit, argument),
        6 => push_semantic(row, layout, Semantic::Card, base_card_id(argument)),
        _ => {}
    }
}

fn push_op_semantics(row: &mut DomainRow, layout: Layout, kind: usize, argument: usize) {
    let (kind, argument) = (row.u[kind], row.u[argument]);
    push_semantic(row, layout, Semantic::CardOp, kind);
    match kind {
        0 | 16 => push_semantic(row, layout, Semantic::Pile, argument),
        4 => push_bits(row, layout, Semantic::FlagBit, argument),
        5 => push_bits(row, layout, Semantic::TurnFlagBit, argument),
        7 => push_semantic(row, layout, Semantic::Card, base_card_id(argument)),
        _ => {}
    }
}

fn push_scale_semantics(row: &mut DomainRow, layout: Layout, kind: usize, argument: usize) {
    let (kind, argument) = (row.u[kind], row.u[argument]);
    push_semantic(row, layout, Semantic::Scale, kind);
    match kind {
        8 => push_semantic(row, layout, Semantic::Card, base_card_id(argument)),
        22 | 24 | 25 | 27 | 29 | 31 => push_semantic(row, layout, Semantic::Power, argument),
        26 => push_bits(row, layout, Semantic::TagBit, argument),
        30 => push_semantic(row, layout, Semantic::CardType, argument),
        _ => {}
    }
}

fn push_condition_semantics(row: &mut DomainRow, layout: Layout, kind: usize, argument: usize) {
    let (kind, argument) = (row.u[kind], row.u[argument]);
    push_semantic(row, layout, Semantic::Condition, kind);
    match kind {
        3 | 4 => push_semantic(row, layout, Semantic::Power, argument),
        7 | 16 => push_semantic(row, layout, Semantic::CardType, argument),
        15 => push_semantic(row, layout, Semantic::Orb, argument),
        22 => push_semantic(row, layout, Semantic::Card, base_card_id(argument)),
        _ => {}
    }
}

fn push_effect_semantics(row: &mut DomainRow, layout: Layout) {
    let variant = row.u[1];
    let target = |row: &mut DomainRow| push_semantic(row, layout, Semantic::Target, row.u[11]);
    let pile = |row: &mut DomainRow, slot| push_semantic(row, layout, Semantic::Pile, row.u[slot]);
    let card = |row: &mut DomainRow, slot| {
        push_semantic(row, layout, Semantic::Card, base_card_id(row.u[slot]))
    };
    match variant {
        0 | 2 => {
            target(row);
            push_scale_semantics(row, layout, 12, 13);
        }
        1 | 3 => {
            target(row);
            push_scale_semantics(row, layout, 12, 13);
            push_scale_semantics(row, layout, 15, 16);
        }
        4 | 5 | 9 | 10 | 14 | 15 | 16 | 88 => {
            target(row);
            push_scale_semantics(row, layout, 12, 13);
        }
        6 | 7 => target(row),
        11 | 12 | 17 | 18 | 20 | 22 | 23 | 24 | 29 | 34 | 37 | 68 | 73 | 87 | 98 | 99 | 105
        | 106 | 107 | 109 | 110 | 112 => push_scale_semantics(row, layout, 11, 12),
        13 => {
            push_semantic(row, layout, Semantic::CardType, row.u[11]);
            push_scale_semantics(row, layout, 12, 13);
        }
        25 | 26 | 27 | 31 => {
            target(row);
            push_semantic(row, layout, Semantic::Power, row.u[12]);
            push_scale_semantics(row, layout, 13, 14);
        }
        28 | 30 => {
            target(row);
            push_semantic(row, layout, Semantic::Power, row.u[12]);
        }
        36 => push_semantic(row, layout, Semantic::CardType, row.u[11]),
        41 | 42 | 66 => {
            pile(row, 11);
            card(row, 12);
        }
        43 => {
            pile(row, 11);
            card(row, 12);
            push_bits(row, layout, Semantic::FlagBit, row.u[14]);
        }
        44 => {
            pile(row, 11);
            push_semantic(row, layout, Semantic::CardType, row.u[12]);
            push_semantic(row, layout, Semantic::Boolean, row.u[14]);
        }
        45 | 54 => {
            pile(row, 11);
            push_semantic(row, layout, Semantic::Boolean, row.u[12]);
            push_scale_semantics(row, layout, 13, 14);
        }
        46 => {
            pile(row, 11);
            push_scale_semantics(row, layout, 12, 13);
        }
        47 => {
            push_semantic(row, layout, Semantic::Boolean, row.u[12]);
            push_semantic(row, layout, Semantic::Boolean, row.u[13]);
        }
        48 => push_semantic(row, layout, Semantic::Boolean, row.u[12]),
        50 | 51 => {
            push_semantic(row, layout, Semantic::CardType, row.u[11]);
            if variant == 51 {
                push_semantic(row, layout, Semantic::Boolean, row.u[13]);
            }
        }
        52 => {
            pile(row, 11);
            push_bits(row, layout, Semantic::FlagBit, row.u[12]);
            push_scale_semantics(row, layout, 13, 14);
        }
        53 | 64 => {
            pile(row, 11);
            push_semantic(row, layout, Semantic::Boolean, row.u[13]);
        }
        74 => pile(row, 11),
        75 | 86 => push_semantic(row, layout, Semantic::Boolean, row.u[12]),
        90 => {
            pile(row, 11);
            push_semantic(row, layout, Semantic::Boolean, row.u[13]);
        }
        55 => {
            pile(row, 11);
            push_filter_semantics(row, layout, 12, 13);
            push_op_semantics(row, layout, 14, 15);
            push_scale_semantics(row, layout, 17, 18);
        }
        56 | 58 => {
            pile(row, 11);
            push_filter_semantics(row, layout, 12, 13);
            if variant == 58 {
                push_semantic(row, layout, Semantic::Boolean, row.u[14]);
                push_op_semantics(row, layout, 15, 16);
            }
            push_scale_semantics(row, layout, 18, 19);
        }
        62 => push_bits(row, layout, Semantic::TagBit, row.u[11]),
        63 | 95 => push_semantic(row, layout, Semantic::Orb, row.u[11]),
        65 | 77 => {
            card(row, 11);
            if variant == 65 {
                push_semantic(row, layout, Semantic::Boolean, row.u[14]);
            }
        }
        69 => {
            push_semantic(row, layout, Semantic::Boolean, row.u[11]);
            push_scale_semantics(row, layout, 12, 13);
        }
        82 => {
            pile(row, 11);
            push_filter_semantics(row, layout, 12, 13);
            pile(row, 14);
        }
        83 | 84 => push_filter_semantics(row, layout, 12, 13),
        85 => push_filter_semantics(row, layout, 11, 12),
        89 => {
            target(row);
            push_scale_semantics(row, layout, 13, 14);
        }
        91 => card(row, 13),
        92 => push_condition_semantics(row, layout, 11, 12),
        93 => push_scale_semantics(row, layout, 11, 12),
        97 => push_semantic(row, layout, Semantic::Boolean, row.u[11]),
        111 => {
            card(row, 11);
            push_scale_semantics(row, layout, 12, 13);
        }
        118 => {
            pile(row, 11);
            push_filter_semantics(row, layout, 12, 13);
            push_semantic(row, layout, Semantic::Boolean, row.u[16]);
            push_semantic(row, layout, Semantic::Boolean, row.u[17]);
            push_op_semantics(row, layout, 18, 19);
        }
        _ => {}
    }
}

fn put_continuation_numbers(row: &mut DomainRow, slots: &[usize], scale: f32) {
    assert!(slots.len() <= 6);
    for (target, &source) in slots.iter().enumerate() {
        row.f[10 + target] = row.u[source] as f32 / scale;
    }
}

fn event_actions(options: &[EventOption]) -> impl Iterator<Item = u8> + '_ {
    options.iter().filter_map(|option| {
        option.effects.iter().find_map(|effect| match effect {
            RunEffect::EventAction(action) => Some(*action),
            _ => None,
        })
    })
}

fn populate_domain_features(
    game: &Game,
    content: &Content,
    layout: Layout,
    domain: usize,
    row: &mut DomainRow,
) {
    let sf = |value: i32, scale: f32| value as f32 / scale;
    match domain {
        RUN_DOMAIN => {
            push_semantic(row, layout, Semantic::Character, row.u[0]);
            for (field, value) in <[i32; 4]>::try_from(&row.s[6..10])
                .unwrap()
                .into_iter()
                .enumerate()
            {
                push_boolean(row, layout, 30 + field as u32, value > 0);
            }
            row.f[..22].copy_from_slice(&[
                row.u[2] as f32 / 20.0,
                row.u[3] as f32 / 3.0,
                row.u[4] as f32 / 60.0,
                row.u[6] as f32 / 3.0,
                row.u[7] as f32 / 20.0,
                row.u[8] as f32 / 20.0,
                row.u[9] as f32 / 20.0,
                sf(row.s[0], 30.0),
                sf(row.s[1], 30.0),
                row.s[0] as f32 / row.s[1].max(1) as f32,
                (row.s[1] - row.s[0]) as f32 / row.s[1].max(1) as f32,
                sf(row.s[2], 1_000.0),
                row.u[10] as f32 / 10.0,
                row.u[11] as f32 / 10.0,
                row.u[12] as f32 / 10.0,
                row.u[13] as f32 / 10.0,
                sf(row.s[4], 10_000.0),
                sf(row.s[5], 100.0),
                sf(row.s[6], 1_000.0),
                sf(row.s[7], 1_000.0),
                sf(row.s[8], 1_000.0),
                sf(row.s[9], 1_000.0),
            ]);
        }
        PHASE_DOMAIN => {
            for (namespace, value) in [(Semantic::Phase, row.u[0]), (Semantic::Room, row.u[2])] {
                push_semantic(row, layout, namespace, value);
            }
            push_semantic(row, layout, Semantic::PhaseKind, row.u[0]);
            if row.u[3] > 0 {
                push_semantic(row, layout, Semantic::Phase, row.u[3] - 1);
                push_semantic(
                    row,
                    layout,
                    Semantic::PhaseKind,
                    PHASES as u32 + row.u[3] - 1,
                );
            }
            if row.u[1] > 0 {
                push_semantic(row, layout, Semantic::Event, row.u[1] - 1);
            }
            if let Phase::Event(id, options) = &game.phase {
                match content.events[*id as usize].id {
                    "EVENT.COLORFUL_PHILOSOPHERS" => {
                        for value in [row.s[0], row.s[1], row.s[2]] {
                            if value > 0 {
                                push_semantic(row, layout, Semantic::Character, value as u32 - 1);
                            }
                        }
                    }
                    "EVENT.DOLL_ROOM"
                        if !options.is_empty()
                            && event_actions(options).all(|action| (10..=12).contains(&action)) =>
                    {
                        let values = [row.s[0], row.s[1], row.s[2]];
                        for &value in &values[..options.len()] {
                            push_semantic(row, layout, Semantic::Relic, value as u32);
                        }
                    }
                    "EVENT.TINKER_TIME" if row.s[3] < 0 => {
                        for value in [row.s[0], row.s[1]] {
                            if value > 0 {
                                push_semantic(row, layout, Semantic::CardType, value as u32 - 1);
                            }
                        }
                    }
                    "EVENT.TINKER_TIME" if row.s[3] > 0 => {
                        push_semantic(row, layout, Semantic::CardType, row.s[3] as u32 - 1);
                        for value in [row.s[0], row.s[1]] {
                            if value > 0 {
                                push_semantic(row, layout, Semantic::TinkerRider, value as u32 - 1);
                            }
                        }
                    }
                    "EVENT.ENDLESS_CONVEYOR" => {
                        for &value in [row.s[0], row.s[2]].iter().filter(|&&value| value > 0) {
                            push_semantic(row, layout, Semantic::ConveyorDish, value as u32 - 1);
                        }
                    }
                    "EVENT.RANWID_THE_ELDER" => {}
                    _ => {}
                }
            }
            match &game.phase {
                Phase::Combat(combat) => {
                    if combat.choice.is_some() {
                        push_semantic(row, layout, Semantic::Pile, row.u[5]);
                        push_filter_semantics(row, layout, 6, 7);
                        push_op_semantics(row, layout, 8, 9);
                    }
                }
                Phase::TransformCards(target, ..) => {
                    if target.is_some() {
                        push_semantic(row, layout, Semantic::Card, base_card_id(row.u[4] - 1));
                    }
                }
                Phase::EnchantCards(..) => {
                    push_semantic(row, layout, Semantic::Enchantment, row.u[4] - 1);
                    if row.u[5] > 0 {
                        push_semantic(row, layout, Semantic::CardType, row.u[5] - 1);
                    }
                }
                Phase::RemoveCards(..) => push_bits(row, layout, Semantic::TagBit, row.u[5]),
                _ => {}
            }
            match &game.phase {
                Phase::Combat(combat) => {
                    push_boolean(row, layout, 0, combat.choice.is_some());
                    push_boolean(row, layout, 1, combat.enemy_turn);
                    push_boolean(row, layout, 2, combat.ending);
                    push_boolean(row, layout, 3, combat.force_end);
                    if let Some(choice) = combat.choice {
                        push_boolean(row, layout, 4, choice.optional);
                        row.f[..3].copy_from_slice(&[
                            choice.remaining as f32 / 10.0,
                            row.s[0] as f32 / 10.0,
                            row.s[1] as f32 / 10.0,
                        ]);
                    }
                    if combat.playing.is_some() {
                        row.f[3..6].copy_from_slice(&[
                            combat.card_energy as f32 / 5.0,
                            combat.card_stars as f32 / 5.0,
                            combat.card_plays as f32 / 5.0,
                        ]);
                    }
                }
                Phase::Rewards(rewards) => row.f[..3].copy_from_slice(&[
                    rewards.gold as f32 / 1_000.0,
                    rewards.removals as f32 / 10.0,
                    rewards.card_rewards.len() as f32 / 10.0,
                ]),
                Phase::Event(id, options) => {
                    row.f[0] = options.len() as f32 / 10.0;
                    let values = event_public_data(game, content);
                    match content.events[*id as usize].id {
                        "EVENT.CRYSTAL_SPHERE"
                        | "EVENT.JUNGLE_MAZE_ADVENTURE"
                        | "EVENT.LUMINOUS_CHOIR"
                        | "EVENT.WHISPERING_HOLLOW" => {
                            row.f[1] = values[0] as f32 / 1_000.0;
                            row.f[2] = values[1] as f32 / 1_000.0;
                        }
                        "EVENT.ABYSSAL_BATHS" | "EVENT.SLIPPERY_BRIDGE" => {
                            row.f[1] = values[0] as f32 / 30.0;
                        }
                        _ => {}
                    }
                }
                Phase::RemoveCards(count, _, optional)
                | Phase::UpgradeCards(count, optional)
                | Phase::TransformCards(_, count, optional)
                | Phase::ChooseCards(_, count, optional) => {
                    row.f[0] = *count as f32 / 10.0;
                    push_boolean(row, layout, 4, *optional);
                }
                Phase::EnchantCards(_, amount, count, _, optional) => {
                    row.f[..2].copy_from_slice(&[*amount as f32 / 30.0, *count as f32 / 10.0]);
                    push_boolean(row, layout, 4, *optional);
                }
                Phase::ChooseBundles(bundles) => row.f[0] = bundles.len() as f32 / 10.0,
                _ => {}
            }
            if let Some(crystal) = &game.crystal {
                row.f[15] = crystal.remaining as f32 / 10.0;
                push_boolean(row, layout, 5, crystal.big);
            }
        }
        CARD_DOMAIN => {
            for (namespace, value) in [
                (Semantic::CardZone, row.u[0]),
                (Semantic::CardRole, row.u[1]),
                (
                    Semantic::Card,
                    card_version_id(content, row.u[4] as Id, row.u[5] > 0, row.u[12] as u8),
                ),
            ] {
                push_semantic(row, layout, namespace, value);
            }
            if row.u[0] == DRAW_ZONE as u32 && row.u[3] > 0 {
                let namespace = if row.u[2] == 1 {
                    Semantic::DrawTopPosition
                } else {
                    Semantic::DrawBottomPosition
                };
                push_semantic(row, layout, namespace, row.u[3].saturating_sub(1).min(256));
            }
            if row.u[19] > 0 {
                push_semantic(row, layout, Semantic::DeckOrigin, (row.u[19] - 1).min(256));
            }
            if row.u[13] > 0 {
                push_semantic(
                    row,
                    layout,
                    Semantic::Card,
                    card_version_id(content, row.u[4] as Id, row.u[13] > 1, row.u[12] as u8),
                );
            }
            if row.u[11] > 0 {
                push_semantic(row, layout, Semantic::Enchantment, row.u[11] - 1);
            }
            for (namespace, bits) in [
                (Semantic::FlagBit, row.u[6]),
                (Semantic::TurnFlagBit, row.u[7]),
            ] {
                push_bits(row, layout, namespace, bits);
            }
            push_boolean(row, layout, 10, row.u[9] != 0);
            push_boolean(row, layout, 11, row.u[10] != 0);
            row.f[..6].copy_from_slice(&[
                sf(row.s[0], 10.0),
                sf(row.s[1], 30.0),
                sf(row.s[2], 10.0),
                sf(row.s[3], 30.0),
                sf(row.s[4], 30.0),
                row.u[8] as f32 / 5.0,
            ]);
        }
        ACTOR_DOMAIN => {
            push_semantic(row, layout, Semantic::ActorKind, row.u[1]);
            if row.u[1] == 2 {
                push_semantic(row, layout, Semantic::Enemy, row.u[2]);
                push_semantic(
                    row,
                    layout,
                    Semantic::EnemyPosition,
                    row.u[0].saturating_sub(3).min(32),
                );
                if row.u[8] != 0 {
                    push_semantic(
                        row,
                        layout,
                        Semantic::EnemyMove,
                        content
                            .enemies
                            .iter()
                            .map(|enemy| enemy.moves.len() as u32)
                            .sum(),
                    );
                } else if row.u[3] != 0 {
                    push_semantic(
                        row,
                        layout,
                        Semantic::EnemyMove,
                        enemy_move_id(content, row.u[2] as Id, row.u[4] as usize),
                    );
                }
            }
            row.f[..3].copy_from_slice(&[
                sf(row.s[0], 30.0),
                sf(row.s[1], 30.0),
                sf(row.s[2], 30.0),
            ]);
            if row.u[1] == 0 {
                row.f[3..9].copy_from_slice(&[
                    sf(row.s[5], 5.0),
                    sf(row.s[6], 10.0),
                    sf(row.s[7], 10.0),
                    sf(row.s[8], 5.0),
                    sf(row.s[9], 5.0),
                    sf(row.s[10], 10.0),
                ]);
            }
            let max_hp = row.s[1].max(1) as f32;
            row.f[9] = row.s[0] as f32 / max_hp;
            row.f[10] = (row.s[1] - row.s[0]) as f32 / max_hp;
            row.f[11] = row.s[2] as f32 / max_hp;
        }
        POWER_DOMAIN => {
            if row.u[0] >= 3 {
                let position = row.u[0] - 3;
                push_semantic(row, layout, Semantic::EnemyPosition, position.min(32));
                if let Some(enemy) = game
                    .combat()
                    .and_then(|combat| combat.enemies.get(position as usize))
                {
                    push_semantic(row, layout, Semantic::Enemy, enemy.creature.id as u32);
                }
            } else {
                push_semantic(row, layout, Semantic::ActorKind, row.u[0].saturating_sub(1));
            }
            push_semantic(row, layout, Semantic::Power, row.u[1]);
            push_boolean(row, layout, 20, row.u[3] != 0);
            if row.u[6] > 0 {
                push_semantic(row, layout, Semantic::PowerPosition, (row.u[6] - 1).min(64));
            }
            row.f.copy_from_slice(&[
                sf(row.s[0], 30.0),
                sf(row.s[1], 30.0),
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
            ]);
        }
        HISTORY_DOMAIN => {
            if row.u[1] == 1 && row.u[4] > 0 {
                push_semantic(
                    row,
                    layout,
                    Semantic::EnemyMove,
                    enemy_move_id(content, (row.u[4] - 1) as Id, row.u[3] as usize),
                );
            } else {
                push_semantic(row, layout, Semantic::ActorKind, 0);
            }
            if row.u[1] == 0 {
                for index in 0..29 {
                    row.f[index] = sf(
                        row.s[index],
                        if matches!(index, 19 | 27) { 30.0 } else { 10.0 },
                    );
                }
            }
        }
        STATUS_DOMAIN => {
            if row.u[0] >= 3 {
                let position = row.u[0] - 3;
                push_semantic(row, layout, Semantic::EnemyPosition, position.min(32));
                if let Some(enemy) = game
                    .combat()
                    .and_then(|combat| combat.enemies.get(position as usize))
                {
                    push_semantic(row, layout, Semantic::Enemy, enemy.creature.id as u32);
                }
            } else {
                push_semantic(row, layout, Semantic::ActorKind, row.u[0].saturating_sub(1));
            }
            push_semantic(row, layout, Semantic::StatusKind, row.u[1]);
            if matches!(row.u[1], 1 | HISTORY_COURSE_STATUS) {
                push_semantic(
                    row,
                    layout,
                    Semantic::Card,
                    card_version_id(content, row.u[3] as Id, row.u[4] > 0, row.u[11] as u8),
                );
                if row.u[10] > 0 {
                    push_semantic(row, layout, Semantic::Enchantment, row.u[10] - 1);
                }
                push_bits(row, layout, Semantic::FlagBit, row.u[5]);
                push_bits(row, layout, Semantic::TurnFlagBit, row.u[6]);
            }
            match row.u[1] {
                1 => row.f[..6].copy_from_slice(&[
                    sf(row.s[0], 5.0),
                    sf(row.s[1], 10.0),
                    sf(row.s[2], 30.0),
                    sf(row.s[3], 10.0),
                    sf(row.s[4], 30.0),
                    sf(row.s[5], 30.0),
                ]),
                2 => row.f[..2].copy_from_slice(&[sf(row.s[0], 10.0), sf(row.s[1], 30.0)]),
                3 => row.f[..2].copy_from_slice(&[sf(row.s[0], 10.0), sf(row.s[1], 5.0)]),
                4 => row.f[..3].copy_from_slice(&[
                    sf(row.s[0], 10.0),
                    sf(row.s[1], 30.0),
                    sf(row.s[2], 10.0),
                ]),
                5 | 8 => row.f[0] = sf(row.s[0], 30.0),
                9 | 10 | 11 => row.f[0] = sf(row.s[0], 10.0),
                _ => {}
            }
            if matches!(row.u[1], 1 | HISTORY_COURSE_STATUS) {
                push_boolean(row, layout, 21, row.u[8] != 0);
                push_boolean(row, layout, 22, row.u[9] != 0);
                if row.u[17] > 0 {
                    push_semantic(row, layout, Semantic::DeckOrigin, (row.u[17] - 1).min(256));
                }
                if row.u[18] > 0 {
                    push_semantic(
                        row,
                        layout,
                        Semantic::Card,
                        card_version_id(content, row.u[3] as Id, row.u[18] > 1, row.u[11] as u8),
                    );
                }
            }
        }
        RELIC_DOMAIN => {
            push_semantic(row, layout, Semantic::RelicKind, row.u[0]);
            push_semantic(row, layout, Semantic::Relic, row.u[1]);
            if matches!(row.u[0], 1 | 2) {
                push_semantic(row, layout, Semantic::Rarity, row.u[3]);
            }
            if row.u[0] == 0 && row.u[2] > 0 {
                push_semantic(
                    row,
                    layout,
                    Semantic::RelicPosition,
                    (row.u[2] - 1).min(256),
                );
            }
            if row.u[6] > 0 {
                push_semantic(row, layout, Semantic::WaxPosition, (row.u[6] - 1).min(256));
            }
            if row.scope == STATE_SCOPE && row.u[8] > 0 {
                push_semantic(row, layout, Semantic::Card, base_card_id(row.u[8] - 1));
            }
            push_boolean(row, layout, 40, row.u[5] != 0);
            for bit in 0..2 {
                if row.u[4] & 1 << bit != 0 {
                    push_semantic(row, layout, Semantic::RunKind, 24 + bit);
                }
            }
            for index in 0..4 {
                row.f[index] = sf(row.s[index], 10.0);
            }
            let name = content.relics[row.u[1] as usize].id;
            let boolean_slots: &[usize] = match name {
                "RELIC.PEN_NIB" | "RELIC.WONGOS_MYSTERY_TICKET" => &[1],
                "RELIC.ASTROLABE" | "RELIC.PAELS_EYE" => &[0, 1],
                "RELIC.MAW_BANK"
                | "RELIC.LIZARD_TAIL"
                | "RELIC.SILKEN_TRESS"
                | "RELIC.BONE_TEA"
                | "RELIC.TEA_OF_DISCOURTESY"
                | "RELIC.GOLDEN_COMPASS"
                | "RELIC.MEAT_CLEAVER"
                | "RELIC.NEOWS_BONES"
                | "RELIC.PAELS_TOOTH"
                | "RELIC.PAELS_TEARS"
                | "RELIC.CENTENNIAL_PUZZLE"
                | "RELIC.DEMON_TONGUE"
                | "RELIC.PERMAFROST"
                | "RELIC.RUINED_HELMET"
                | "RELIC.MUSIC_BOX"
                | "RELIC.MINI_REGENT"
                | "RELIC.RAINBOW_RING"
                | "RELIC.UNSETTLING_LAMP"
                | "RELIC.DIAMOND_DIADEM"
                | "RELIC.BURNING_STICKS"
                | "RELIC.THROWING_AXE"
                | "RELIC.BELT_BUCKLE" => &[0],
                _ => &[],
            };
            for &slot in boolean_slots {
                push_boolean(row, layout, 64 + slot as u32, row.s[slot] != 0);
                row.f[slot] = 0.0;
            }
            let period = match name {
                "RELIC.LASTING_CANDY" | "RELIC.PAELS_WING" => 2,
                "RELIC.HAPPY_FLOWER" | "RELIC.PENDULUM" | "RELIC.FISHING_ROD" => 3,
                "RELIC.IRON_CLUB" | "RELIC.POLLINOUS_CORE" => 4,
                "RELIC.FAKE_HAPPY_FLOWER" | "RELIC.JOSS_PAPER" | "RELIC.BOOK_OF_FIVE_RINGS" => 5,
                "RELIC.NUNCHAKU"
                | "RELIC.PEN_NIB"
                | "RELIC.TUNING_FORK"
                | "RELIC.GALACTIC_DUST" => 10,
                _ => 0,
            };
            if period > 0 {
                let phase = row.s[0].rem_euclid(period);
                row.f[10] = phase as f32 / period as f32;
                row.f[11] = (period - phase) as f32 / period as f32;
            }
            let cap = match name {
                "RELIC.PAELS_LEGION" => 2,
                "RELIC.SILVER_CRUCIBLE" | "RELIC.WINGED_BOOTS" | "RELIC.GIRYA" => 3,
                "RELIC.EMBER_TEA" | "RELIC.SWORD_OF_STONE" => 5,
                "RELIC.TOY_BOX" => 12,
                _ => 0,
            };
            if cap > 0 {
                let value = row.s[0].clamp(0, cap);
                if matches!(name, "RELIC.PAELS_LEGION") {
                    row.f[12] = (cap - value) as f32 / cap as f32;
                    row.f[13] = value as f32 / cap as f32;
                    row.f[14] = (value > 0) as u8 as f32;
                } else {
                    row.f[12] = value as f32 / cap as f32;
                    row.f[13] = (cap - value) as f32 / cap as f32;
                    row.f[14] = (value < cap) as u8 as f32;
                    row.f[15] = (value == cap) as u8 as f32;
                }
            }
            if name == "RELIC.TOY_BOX" && row.s[0] < 12 {
                let phase = row.s[0].rem_euclid(3);
                row.f[10] = phase as f32 / 3.0;
                row.f[11] = (3 - phase) as f32 / 3.0;
            } else if name == "RELIC.PUMPKIN_CANDLE" {
                let remaining = row.s[0].clamp(0, 5);
                row.f[12] = (5 - remaining) as f32 / 5.0;
                row.f[13] = remaining as f32 / 5.0;
                row.f[14] = (remaining > 0) as u8 as f32;
            }
        }
        POTION_DOMAIN => {
            push_semantic(row, layout, Semantic::PotionKind, row.u[0]);
            if row.u[2] > 0 {
                push_semantic(row, layout, Semantic::Potion, row.u[2] - 1);
            }
            push_boolean(row, layout, 50, row.u[3] != 0);
            row.f[0] = sf(row.s[0], 1_000.0);
        }
        ORB_DOMAIN => {
            push_semantic(row, layout, Semantic::OrbKind, 0);
            if row.u[1] > 0 {
                push_semantic(row, layout, Semantic::Orb, row.u[1] - 1);
            }
            if row.u[3] > 0 {
                push_semantic(row, layout, Semantic::OrbTiming, row.u[3] - 1);
            }
            push_semantic(row, layout, Semantic::OrbPosition, row.u[0].min(16));
            push_boolean(row, layout, 51, row.u[2] != 0);
            row.f[0] = sf(row.s[0], 30.0);
        }
        EVENT_DOMAIN => {
            push_semantic(row, layout, Semantic::EventKind, row.u[0]);
            push_semantic(row, layout, Semantic::Event, row.u[1]);
            if row.u[0] == 2
                && row.u[3] > 0
                && let Phase::Event(id, options) = &game.phase
            {
                match content.events[*id as usize].id {
                    "EVENT.COLORFUL_PHILOSOPHERS" if row.s[0] > 0 => {
                        push_semantic(row, layout, Semantic::Character, row.s[0] as u32 - 1)
                    }
                    "EVENT.DOLL_ROOM"
                        if !options.is_empty()
                            && event_actions(options).all(|action| (10..=12).contains(&action)) =>
                    {
                        push_semantic(row, layout, Semantic::Relic, row.s[0] as u32);
                    }
                    "EVENT.TINKER_TIME" if game.event_data[3] < 0 && row.s[0] > 0 => {
                        push_semantic(row, layout, Semantic::CardType, row.s[0] as u32 - 1);
                    }
                    "EVENT.TINKER_TIME" if game.event_data[3] > 0 && row.s[0] > 0 => {
                        push_semantic(row, layout, Semantic::TinkerRider, row.s[0] as u32 - 1);
                    }
                    _ => {}
                }
            }
            row.f.fill(0.0);
        }
        ENCOUNTER_DOMAIN => {
            push_semantic(row, layout, Semantic::EncounterKind, row.u[0]);
            let namespace = match row.u[0] {
                0 => Semantic::EncounterBoss,
                2 | 4 => Semantic::EncounterElite,
                _ => Semantic::EncounterNormal,
            };
            push_semantic(row, layout, namespace, row.u[1]);
            row.f.fill(0.0);
        }
        CRYSTAL_DOMAIN => {
            push_semantic(row, layout, Semantic::CrystalKind, row.u[0]);
            if row.u[3] > 0 {
                push_semantic(row, layout, Semantic::CrystalKind, row.u[3] - 1);
            }
            push_semantic(row, layout, Semantic::CrystalRow, row.u[1].min(10));
            push_semantic(row, layout, Semantic::CrystalColumn, row.u[2].min(10));
            if row.u[0] == 0 {
                push_semantic(
                    row,
                    layout,
                    Semantic::CrystalWidth,
                    row.u[4].saturating_sub(1).min(3),
                );
                push_semantic(
                    row,
                    layout,
                    Semantic::CrystalHeight,
                    row.u[5].saturating_sub(1).min(3),
                );
            }
        }
        CONTINUATION_DOMAIN => {
            push_semantic(row, layout, Semantic::ContinuationKind, row.u[0]);
            push_semantic(row, layout, Semantic::Boolean, row.u[10]);
            if row.scope == PHASE_SCOPE && row.u[23] > 0 {
                push_semantic(row, layout, Semantic::PhaseKind, 27 + row.u[23]);
            }
            let namespace = match row.u[0] {
                0 => Semantic::RunEffect,
                1 => Semantic::Effect,
                4 => Semantic::Phase,
                _ => Semantic::ContinuationVariant,
            };
            push_semantic(row, layout, namespace, row.u[1]);
            if row.scope == STATE_SCOPE && row.u[8] > 0 {
                push_semantic(
                    row,
                    layout,
                    Semantic::ContinuationPosition,
                    (row.u[8] - 1).min(256),
                );
            }
            match row.u[0] {
                0 => match row.u[1] {
                    9 => push_semantic(row, layout, Semantic::Card, base_card_id(row.u[11])),
                    10 => push_semantic(row, layout, Semantic::Relic, row.u[11]),
                    11 | 12 => push_semantic(row, layout, Semantic::Potion, row.u[11]),
                    13 => push_semantic(row, layout, Semantic::Boolean, row.u[11]),
                    15 => push_semantic(row, layout, Semantic::Rarity, row.u[11]),
                    21 => push_bits(row, layout, Semantic::TagBit, row.u[12]),
                    28 if row.u[11] > 0 => {
                        push_semantic(row, layout, Semantic::Card, base_card_id(row.u[11] - 1))
                    }
                    29 => {
                        push_semantic(row, layout, Semantic::Enchantment, row.u[11]);
                        if row.u[13] > 0 {
                            push_semantic(row, layout, Semantic::CardType, row.u[13] - 1);
                        }
                    }
                    _ => {}
                },
                1 => push_effect_semantics(row, layout),
                2 => {
                    push_semantic(row, layout, Semantic::ActorKind, row.u[11].min(2));
                    if row.u[11] >= 3 {
                        let position = row.u[11] - 3;
                        push_semantic(row, layout, Semantic::EnemyPosition, position.min(32));
                        if let Some(combat) = game.combat()
                            && let Some(enemy) = combat.enemies.get(position as usize)
                        {
                            push_semantic(row, layout, Semantic::Enemy, enemy.creature.id as u32);
                        }
                    }
                    if row.u[12] > 0 {
                        let position = row.u[12] - 1;
                        push_semantic(row, layout, Semantic::EnemyPosition, position.min(32));
                        if let Some(combat) = game.combat()
                            && let Some(enemy) = combat.enemies.get(position as usize)
                        {
                            push_semantic(row, layout, Semantic::Enemy, enemy.creature.id as u32);
                        }
                    }
                    if row.u[13] > 0 {
                        push_semantic(
                            row,
                            layout,
                            Semantic::Card,
                            card_version_id(content, (row.u[13] - 1) as Id, row.u[14] != 0, 0),
                        );
                    }
                    if row.u[16] > 0 {
                        push_semantic(row, layout, Semantic::Orb, row.u[16] - 1);
                    }
                    push_boolean(row, layout, 62, row.u[14] != 0);
                    push_boolean(row, layout, 63, row.u[17] != 0);
                }
                4 => match row.u[1] {
                    5 => push_semantic(row, layout, Semantic::Event, row.u[11]),
                    6 => {
                        push_bits(row, layout, Semantic::TagBit, row.u[12]);
                        push_boolean(row, layout, 70, row.u[13] != 0);
                    }
                    7 | 10 => push_boolean(row, layout, 70, row.u[12] != 0),
                    8 if row.u[11] > 0 => {
                        push_semantic(row, layout, Semantic::Card, base_card_id(row.u[11] - 1));
                        push_boolean(row, layout, 70, row.u[13] != 0);
                    }
                    9 => {
                        push_semantic(row, layout, Semantic::Enchantment, row.u[11]);
                        if row.u[13] > 0 {
                            push_semantic(row, layout, Semantic::CardType, row.u[13] - 1);
                        }
                        push_boolean(row, layout, 70, row.u[14] != 0);
                    }
                    _ => {}
                },
                5 => {
                    push_semantic(
                        row,
                        layout,
                        Semantic::Card,
                        card_version_id(content, row.u[11] as Id, row.u[12] > 0, row.u[19] as u8),
                    );
                    if row.u[18] > 0 {
                        push_semantic(row, layout, Semantic::Enchantment, row.u[18] - 1);
                    }
                    push_bits(row, layout, Semantic::FlagBit, row.u[13]);
                    push_bits(row, layout, Semantic::TurnFlagBit, row.u[14]);
                    push_boolean(row, layout, 60, row.u[16] != 0);
                    push_boolean(row, layout, 61, row.u[17] != 0);
                    if row.u[20] > 0 {
                        push_semantic(row, layout, Semantic::DeckOrigin, (row.u[20] - 1).min(256));
                    }
                    if row.u[21] > 0 {
                        push_semantic(
                            row,
                            layout,
                            Semantic::Card,
                            card_version_id(
                                content,
                                row.u[11] as Id,
                                row.u[21] > 0,
                                row.u[19] as u8,
                            ),
                        );
                    }
                }
                6 => {
                    push_semantic(row, layout, Semantic::Power, row.u[11]);
                    push_semantic(row, layout, Semantic::Boolean, row.u[12]);
                }
                7 => push_semantic(row, layout, Semantic::Requirement, row.u[11]),
                8 => match row.u[1] {
                    0 => push_semantic(row, layout, Semantic::Card, base_card_id(row.u[11])),
                    1 | 3 | 5 => push_semantic(row, layout, Semantic::Relic, row.u[11]),
                    4 | 6 => push_semantic(row, layout, Semantic::Potion, row.u[11]),
                    2 => match row.u[11] {
                        0 => push_semantic(row, layout, Semantic::Room, row.u[12]),
                        1 => {
                            push_semantic(row, layout, Semantic::Card, base_card_id(row.u[12]));
                        }
                        3 => push_semantic(row, layout, Semantic::Rarity, row.u[12]),
                        _ => {}
                    },
                    _ => {}
                },
                9 if row.u[1] == 10 => push_semantic(row, layout, Semantic::Relic, row.u[11]),
                _ => {}
            }
            match row.u[0] {
                0 => match row.u[1] {
                    0 => row.f[0] = sf(row.s[0], 1_000.0),
                    1 => {
                        row.f[..2].copy_from_slice(&[sf(row.s[0], 1_000.0), sf(row.s[1], 1_000.0)])
                    }
                    3 | 6 | 7 | 8 => row.f[0] = sf(row.s[0], 30.0),
                    4 => put_continuation_numbers(row, &[11], 100.0),
                    9 | 12 | 28 => put_continuation_numbers(row, &[12], 10.0),
                    14 | 21 | 22 | 23 | 24 | 26 | 30 | 35 => {
                        put_continuation_numbers(row, &[11], 10.0)
                    }
                    29 => {
                        put_continuation_numbers(row, &[12], 10.0);
                        row.f[0] = sf(row.s[0], 30.0);
                    }
                    31 | 32 => put_continuation_numbers(row, &[11, 12], 10.0),
                    _ => {}
                },
                1 => match row.u[1] {
                    0 | 2 => put_continuation_numbers(row, &[15], 10.0),
                    21 | 39 | 104 | 108 | 113 | 114 => row.f[0] = sf(row.s[0], 10.0),
                    28 => row.f[0] = sf(row.s[0], 30.0),
                    32 | 33 | 35 | 47 | 48 | 49 | 57 | 59 | 74 | 75 | 81 | 83 | 84 | 86 | 94
                    | 100 | 101 | 102 => put_continuation_numbers(row, &[11], 10.0),
                    41 | 42 | 43 | 44 | 66 => put_continuation_numbers(row, &[13], 10.0),
                    50 | 51 | 53 | 62 | 64 | 89 | 90 | 95 => {
                        put_continuation_numbers(row, &[12], 10.0)
                    }
                    65 | 91 | 96 | 116 => put_continuation_numbers(row, &[11, 12], 10.0),
                    118 => put_continuation_numbers(row, &[14, 15], 10.0),
                    _ => {}
                },
                2 => {
                    row.f[0] = sf(row.s[0], 5.0);
                    row.f[1] = sf(row.s[1], 30.0);
                }
                3 => put_continuation_numbers(row, &[11], 5.0),
                4 => match row.u[1] {
                    2 => {
                        row.f[0] = sf(row.s[0], 1_000.0);
                        row.f[1] = sf(row.s[1], 10.0);
                    }
                    6 | 7 | 8 | 10 => put_continuation_numbers(row, &[11], 10.0),
                    9 => {
                        put_continuation_numbers(row, &[12], 10.0);
                        row.f[0] = sf(row.s[0], 30.0);
                    }
                    _ => {}
                },
                7 => {
                    row.f[0] = match row.u[11] {
                        1 => sf(row.s[0], 1_000.0),
                        2 => sf(row.s[0], 30.0),
                        _ => 0.0,
                    }
                }
                8 if matches!(row.u[1], 5..=7) => row.f[0] = sf(row.s[0], 1_000.0),
                _ => {}
            }
            if row.u[0] == 5 {
                row.f[..10].copy_from_slice(&[
                    0.0,
                    row.u[15] as f32 / 5.0,
                    0.0,
                    0.0,
                    sf(row.s[0], 10.0),
                    sf(row.s[1], 30.0),
                    sf(row.s[2], 10.0),
                    sf(row.s[3], 30.0),
                    sf(row.s[4], 30.0),
                    sf(row.s[5], 1_000.0),
                ]);
            }
        }
        MAP_NODE_DOMAIN => {
            push_semantic(row, layout, Semantic::MapNodeKind, row.u[1]);
            push_semantic(row, layout, Semantic::Room, row.u[4]);
            push_semantic(row, layout, Semantic::MapFloorPosition, row.u[2].min(64));
            push_boolean(row, layout, 52, row.u[5] != 0);
            push_boolean(row, layout, 53, row.u[6] != 0);
            push_boolean(row, layout, 54, row.u[7] != 0);
            row.f.copy_from_slice(&[
                0.0,
                0.0,
                row.u[9] as f32 / 8.0,
                (row.u[9] as f32).ln_1p() / 3.0,
                0.0,
                0.0,
                0.0,
                0.0,
            ]);
        }
        MAP_EDGE_DOMAIN => {
            push_semantic(row, layout, Semantic::MapEdgeKind, 0);
            row.f.copy_from_slice(&[
                row.u[2] as f32 / 8.0,
                (row.u[2] as f32).ln_1p() / 3.0,
                1.0,
                0.0,
            ]);
        }
        _ => unreachable!(),
    }
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

fn map_floor_index(game: &Game, floor: u8) -> usize {
    floor.saturating_sub(
        game.map
            .nodes
            .iter()
            .map(|node| node.floor)
            .min()
            .unwrap_or(0),
    ) as usize
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

fn canonical_progress(game: &Game) -> u8 {
    let floor = game.run.floor;
    let progress = match game.run.act {
        0 | 1 => floor,
        2 if game.golden_compass == Some(2) => 25 + floor,
        2 => 25 + floor + u8::from(floor >= 7) + u8::from(floor >= 10),
        3 => 54 + floor,
        act => panic!("invalid canonical-progress act {act}"),
    };
    assert!(progress <= 72, "canonical progress {progress} exceeds 72");
    progress
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

fn token_cmp(left: &[f32], right: &[f32]) -> std::cmp::Ordering {
    left.iter()
        .zip(right)
        .find_map(|(left, right)| (left != right).then(|| left.total_cmp(right)))
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn master_cards(deck: &[Card]) -> HashMap<u32, usize> {
    deck.iter()
        .enumerate()
        .filter(|(_, card)| card.instance != 0)
        .map(|(index, card)| (card.instance, index + 1))
        .collect()
}

fn card_relation(
    card: Card,
    _deck: &[Card],
    masters: &HashMap<u32, usize>,
    dampened: &[(u32, u8)],
) -> (usize, u16) {
    let master = masters.get(&card.instance).copied().unwrap_or(0);
    let upgrades = dampened
        .iter()
        .find(|(instance, _)| *instance == card.instance)
        .map_or(0, |(_, upgrades)| *upgrades as u16 + 1);
    (master, upgrades)
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
    sovereign_blade: bool,
    player_hp_delta: i16,
    player_max_hp_delta: i16,
    outcome: crate::game::ActionOutcome,
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
            sovereign_blade: combat
                .draw
                .iter()
                .chain(&combat.hand)
                .chain(&combat.discard)
                .chain(combat.playing.iter())
                .chain(combat.auto_plays.iter().map(|play| &play.card))
                .any(|card| card.id == card_id::SOVEREIGN_BLADE),
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

fn targets_player(target: Target, context: Context) -> bool {
    target == Target::Player || target == Target::Source && context.source == Actor::Player
}

fn preview_player_health(game: &Game) -> (i16, i16) {
    game.combat()
        .map_or((game.run.hp, game.run.max_hp), |combat| {
            (combat.player.hp, combat.player.max_hp)
        })
}

fn preview_player_hp(game: &Game, preview: &mut CardPreview, amount: i16) {
    let (hp, max_hp) = preview_player_health(game);
    let hp = hp.saturating_add(preview.player_hp_delta);
    let max_hp = max_hp.saturating_add(preview.player_max_hp_delta).max(1);
    let next = hp.saturating_add(amount).clamp(0, max_hp);
    preview.player_hp_delta = preview.player_hp_delta.saturating_add(next - hp);
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

fn preview_generated(
    game: &Game,
    content: &Content,
    card: Option<Id>,
    count: i16,
    preview: &mut CardPreview,
) {
    let powers = game
        .combat()
        .map_or_else(Vec::new, |combat| combat.player.powers.clone());
    let context = Context {
        card,
        event: 1,
        ..Context::player()
    };
    for _ in 0..count.max(0) {
        preview_hooks(
            game,
            content,
            Trigger::CardGenerated,
            Actor::Player,
            context,
            &powers,
            preview,
        );
    }
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
            Effect::Heal(target, amount) | Effect::HealPercent(target, amount) => {
                if targets_player(target, context) {
                    let mut amount = preview_amount(game, content, context, amount).max(0);
                    if matches!(effect, Effect::HealPercent(..)) {
                        amount = preview_player_health(game)
                            .1
                            .saturating_add(preview.player_max_hp_delta)
                            .saturating_mul(amount)
                            / 100;
                    }
                    for _ in 0..repeats.max(0) {
                        preview_player_hp(game, preview, amount);
                    }
                }
            }
            Effect::MaxHp(amount) => {
                let amount = preview_amount(game, content, context, amount);
                for _ in 0..repeats.max(0) {
                    preview.player_max_hp_delta =
                        preview.player_max_hp_delta.saturating_add(amount);
                    if amount >= 0 {
                        preview_player_hp(game, preview, amount);
                    } else {
                        preview_player_hp(game, preview, 0);
                    }
                }
            }
            Effect::LoseHp(target, amount) => {
                let amount = preview_amount(game, content, context, amount).max(0);
                if targets_player(target, context) {
                    for _ in 0..repeats.max(0) {
                        preview_player_hp(game, preview, -amount);
                    }
                } else if let Some(target) = preview_target(target, context) {
                    push_preview_hit(
                        game,
                        context,
                        target,
                        PreviewDamage::LoseHp,
                        amount,
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
            Effect::AddCard(_, id, count)
            | Effect::AddUpgradedCard(_, id, count)
            | Effect::AddFlaggedCard(_, id, count, _) => preview_generated(
                game,
                content,
                Some(id),
                (count as i16).saturating_mul(repeats),
                preview,
            ),
            Effect::RandomCard(_, kind, count, _) => {
                let available = content.characters[game.run.character as usize]
                    .cards
                    .iter()
                    .any(|&id| {
                        let card = content.cards[id as usize];
                        card.card_type == kind
                            && matches!(
                                card.rarity,
                                CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                            )
                            && card.flags[0] & NO_GENERATE == 0
                    });
                preview_generated(
                    game,
                    content,
                    None,
                    (count as i16)
                        .saturating_mul(repeats)
                        .saturating_mul(available as i16),
                    preview,
                );
            }
            Effect::RandomColorless(_, amount, _) | Effect::RandomColorlessOther(_, amount) => {
                let other = matches!(*effect, Effect::RandomColorlessOther(..));
                let available = content
                    .colorless
                    .iter()
                    .filter(|&&id| {
                        content.cards[id as usize].flags[0] & NO_GENERATE == 0
                            && (!other || Some(id) != context.card)
                    })
                    .count() as i16;
                preview_generated(
                    game,
                    content,
                    None,
                    preview_effect_amount(game, content, context, amount, preview)
                        .max(0)
                        .min(available)
                        .saturating_mul(repeats),
                    preview,
                );
            }
            Effect::RandomCharacter(_, amount, _) | Effect::RandomCharacterCost0(_, amount, _) => {
                let cost_zero = matches!(*effect, Effect::RandomCharacterCost0(..));
                let available = content.characters[game.run.character as usize]
                    .cards
                    .iter()
                    .any(|&id| {
                        let card = content.cards[id as usize];
                        card.flags[0] & NO_GENERATE == 0
                            && if cost_zero {
                                card.cost[0] == 0
                            } else {
                                !matches!(card.rarity, CardRarity::Basic | CardRarity::Ancient)
                            }
                    });
                preview_generated(
                    game,
                    content,
                    None,
                    preview_effect_amount(game, content, context, amount, preview)
                        .max(0)
                        .saturating_mul(repeats)
                        .saturating_mul(available as i16),
                    preview,
                );
            }
            Effect::DistinctCharacter(_, count, _) => {
                let available = content.characters[game.run.character as usize]
                    .cards
                    .iter()
                    .filter(|&&id| {
                        let card = content.cards[id as usize];
                        card.flags[0] & NO_GENERATE == 0
                            && matches!(
                                card.rarity,
                                CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                            )
                    })
                    .count() as i16;
                preview_generated(
                    game,
                    content,
                    None,
                    (count as i16).min(available).saturating_mul(repeats),
                    preview,
                );
            }
            Effect::DistinctColorless(_, count, _) => {
                let available = content
                    .colorless
                    .iter()
                    .filter(|&&id| {
                        !matches!(
                            content.cards[id as usize].id,
                            "CARD.ALCHEMIZE" | "CARD.HAND_OF_GREED" | "CARD.HIDDEN_GEM"
                        )
                    })
                    .count() as i16;
                preview_generated(
                    game,
                    content,
                    None,
                    (count as i16).min(available).saturating_mul(repeats),
                    preview,
                );
            }
            Effect::Forge(_) if repeats > 0 && !preview.sovereign_blade => {
                preview.sovereign_blade = true;
                preview_generated(game, content, Some(card_id::SOVEREIGN_BLADE), 1, preview);
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
    if stars > 0 && relic("RELIC.GALACTIC_DUST") {
        preview_gain_block(
            &projected,
            content,
            &mut preview,
            10,
            (game.galactic_dust as i16 + stars) / 10,
        );
    }
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
    out[8] = card.flags as f32 + 1.0;
    out[9] = card.turn_flags as f32 + 1.0;
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
        0.0,
        0.0,
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
        preview.outcome.block.unwrap_or(preview.block as f32),
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
        preview.outcome.draw.unwrap_or(preview.draw as f32),
        preview.outcome.discard.unwrap_or(preview.discard as f32),
        preview.outcome.exhaust.unwrap_or(preview.exhaust as f32),
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
            let preview =
                (zone == HAND_ZONE).then(|| played_card_preview(game, content, card, None));
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
    extend_card_tokens(game, content, out, DECK_ZONE, &game.run.deck, |_| 0, 0, &[]);
    extend_card_tokens(
        game,
        content,
        out,
        PAEL_ZONE,
        &game.paels_cards,
        |_| 0,
        0,
        &[],
    );
    match &game.phase {
        Phase::Combat(combat) => {
            extend_card_tokens(
                game,
                content,
                out,
                HAND_ZONE,
                &combat.hand,
                |_| 0,
                0,
                &combat.dampened,
            );
            extend_card_tokens(
                game,
                content,
                out,
                DRAW_ZONE,
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
            for (cards, zone) in [
                (&combat.discard, DISCARD_ZONE),
                (&combat.exhaust, EXHAUST_ZONE),
            ] {
                extend_card_tokens(game, content, out, zone, cards, |_| 0, 0, &combat.dampened);
            }
            extend_card_tokens(
                game,
                content,
                out,
                COMBAT_OFFER_ZONE,
                &combat.offer,
                |_| 0,
                0,
                &combat.dampened,
            );
            if let Some(card) = combat.history_course {
                extend_card_tokens(
                    game,
                    content,
                    out,
                    HISTORY_COURSE_ZONE,
                    &[card],
                    |_| 0,
                    0,
                    &combat.dampened,
                );
            }
            if let Some(card) = combat.playing {
                extend_card_tokens(
                    game,
                    content,
                    out,
                    PLAYING_ZONE,
                    &[card],
                    |_| 0,
                    0,
                    &combat.dampened,
                );
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
                AUTOPLAY_ZONE,
                &auto_plays,
                |_| 0,
                0,
                &combat.dampened,
            );
        }
        Phase::Event(id, _) if content.events[*id as usize].id == "EVENT.SLIPPERY_BRIDGE" => {
            for &instance in &game.event_cards {
                if let Some(&card) = game.run.deck.iter().find(|card| card.instance == instance) {
                    extend_card_tokens(
                        game,
                        content,
                        out,
                        SLIPPERY_PREVIOUS_ZONE,
                        &[card],
                        |_| 0,
                        0,
                        &[],
                    );
                }
            }
            if let Some(&card) = game
                .run
                .deck
                .iter()
                .find(|card| card.instance == game.event_data[1] as u32)
            {
                extend_card_tokens(
                    game,
                    content,
                    out,
                    SLIPPERY_CANDIDATE_ZONE,
                    &[card],
                    |_| 0,
                    0,
                    &[],
                );
            }
        }
        Phase::Rewards(rewards) => extend_card_tokens(
            game,
            content,
            out,
            OFFER_ZONE,
            &rewards.cards,
            |_| 0,
            0,
            &[],
        ),
        Phase::Shop(items) => {
            for item in items {
                if let ShopItem::Card(card, price) = item {
                    extend_card_tokens(
                        game,
                        content,
                        out,
                        OFFER_ZONE,
                        &[*card],
                        |_| 0,
                        *price,
                        &[],
                    );
                }
            }
        }
        Phase::ChooseCards(cards, ..) => {
            extend_card_tokens(game, content, out, OFFER_ZONE, cards, |_| 0, 0, &[]);
        }
        Phase::ChooseBundles(bundles) => {
            for (bundle, cards) in bundles.iter().enumerate() {
                extend_card_tokens(
                    game,
                    content,
                    out,
                    OFFER_ZONE,
                    cards,
                    |_| 0,
                    bundle as i32 + 1,
                    &[],
                );
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
) -> (i16, i16, i16, i16) {
    let enemy = &game.combat().unwrap().enemies[target].creature;
    let mut total = 0i16;
    let mut cap = 0;
    let mut actual = 0i16;
    let mut uncapped = 0i16;
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
                    uncapped = uncapped.saturating_add(hit.amount);
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
                    uncapped = uncapped.saturating_add(lost);
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
                let uncapped_lost = lost;
                let lost = uncapped_lost.min(enemy.hp.max(0) - hp_lost);
                hp_lost = hp_lost.saturating_add(lost);
                if counted {
                    actual = actual.saturating_add(lost);
                    uncapped = uncapped.saturating_add(uncapped_lost);
                }
            }
        }
    }
    (total, cap, actual, uncapped)
}

fn action_preview(game: &Game, content: &Content, action: &Action) -> Option<CardPreview> {
    action_preview_resolved(game, content, action, true)
}

fn action_preview_resolved(
    game: &Game,
    content: &Content,
    action: &Action,
    resolve: bool,
) -> Option<CardPreview> {
    let mut preview = match action {
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
    }?;
    if !resolve {
        return Some(preview);
    }
    if game.replaying {
        let mut live = game.clone();
        live.replaying = false;
        preview.outcome = live.expected_action_outcome(content, action);
    } else {
        preview.outcome = game.expected_action_outcome(content, action);
    }
    Some(preview)
}

fn action_preview_values(
    game: &Game,
    content: &Content,
    action: &Action,
    preview: Option<&CardPreview>,
) -> [f32; 9] {
    let mut values = [0.0; 9];
    let Some(preview) = preview else {
        return values;
    };
    let Some(combat) = game.combat() else {
        return values;
    };
    let resolved = preview.outcome.hp_loss.as_deref().filter(|_| {
        preview
            .hits
            .iter()
            .all(|hit| hit.target != PreviewTarget::Random)
    });
    let target = match action {
        Action::Play { target, .. } | Action::Potion { target, .. } => *target,
        _ => None,
    };
    let Some(target) = target else {
        values[5] = resolved.map_or_else(
            || {
                combat
                    .enemies
                    .iter()
                    .enumerate()
                    .filter(|(_, enemy)| enemy.creature.hp > 0)
                    .map(|(target, enemy)| {
                        resolved_hits(game, content, target, None, true, false, &preview.hits)
                            .2
                            .min(enemy.creature.hp)
                    })
                    .fold(0i16, i16::saturating_add) as f32
            },
            |losses| losses.iter().sum(),
        );
        return values;
    };
    let Some(enemy) = combat.enemies.get(target) else {
        return values;
    };
    let (damage, cap, actual, uncapped) = resolved_hits(
        game,
        content,
        target,
        Some(target),
        false,
        false,
        &preview.hits,
    );
    let actual = resolved.map_or(actual as f32, |losses| losses[target]);
    values = [
        enemy.creature.kind(content, PowerKind::Vulnerable) as f32,
        enemy.creature.block as f32,
        cap as f32,
        damage as f32,
        actual,
        combat
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, enemy)| enemy.creature.hp > 0)
            .map(|(other, enemy)| {
                resolved_hits(
                    game,
                    content,
                    other,
                    Some(target),
                    true,
                    false,
                    &preview.hits,
                )
                .2
                .min(enemy.creature.hp)
            })
            .fold(0i16, i16::saturating_add) as f32,
        (enemy.creature.hp > 0 && actual >= enemy.creature.hp as f32) as u8 as f32,
        enemy.creature.hp.max(0) as f32,
        uncapped as f32,
    ];
    values
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
                push_action_card_token(
                    game,
                    content,
                    out,
                    HAND_ZONE,
                    0,
                    0,
                    card,
                    target,
                    preview.as_ref(),
                );
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
                    Pile::Hand => HAND_ZONE,
                    Pile::Draw => DRAW_ZONE,
                    Pile::Discard => DISCARD_ZONE,
                    Pile::Exhaust => EXHAUST_ZONE,
                    Pile::Offer => COMBAT_OFFER_ZONE,
                };
                let position = if zone == DRAW_ZONE && *index < combat.known_draw_bottom {
                    KNOWN_DRAW_SLOTS + index + 1
                } else if zone == DRAW_ZONE && *index >= combat.draw.len() - combat.known_draw_top {
                    combat.draw.len() - index
                } else {
                    0
                };
                push_action_card_token(game, content, out, zone, position, 0, card, None, None);
            } else {
                match &game.phase {
                    Phase::ChooseCards(cards, ..) => {
                        if let Some(&card) = cards.get(*index) {
                            push_action_card_token(
                                game, content, out, OFFER_ZONE, 0, 0, card, None, None,
                            );
                        }
                    }
                    Phase::ChooseBundles(bundles) => {
                        if let Some(cards) = bundles.get(*index) {
                            for &card in cards {
                                push_action_card_token(
                                    game,
                                    content,
                                    out,
                                    OFFER_ZONE,
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
                push_action_card_token(game, content, out, OFFER_ZONE, 0, 0, card, None, None);
            }
        }
        Action::Buy(index) => {
            if let Phase::Shop(items) = &game.phase
                && let Some(ShopItem::Card(card, price)) = items.get(*index)
            {
                push_action_card_token(
                    game, content, out, OFFER_ZONE, 0, *price, *card, None, None,
                );
            }
        }
        Action::Smith(index) | Action::Enchant(index) | Action::RemoveCard(index) => {
            if let Some(&card) = game.run.deck.get(*index) {
                push_action_card_token(game, content, out, DECK_ZONE, 0, 0, card, None, None);
            }
        }
        Action::EventCard(index, card) => push_action_card_token(
            game,
            content,
            out,
            OFFER_ZONE,
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
                push_action_card_token(
                    game,
                    content,
                    out,
                    SLIPPERY_CANDIDATE_ZONE,
                    0,
                    0,
                    card,
                    None,
                    None,
                );
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
    let resolved = preview.outcome.hp_loss.as_deref().filter(|_| {
        preview
            .hits
            .iter()
            .all(|hit| hit.target != PreviewTarget::Random)
    });
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
            let (damage, cap, actual, _) =
                resolved_hits(game, content, target, None, false, true, &[hit]);
            let mut row = token(
                ENEMY_COLLECTION,
                3,
                enemy.creature.id as usize + 1,
                hit_index + 1,
                target + 1,
            );
            row[10..19].copy_from_slice(&[
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
            let (damage, cap, actual, _) =
                resolved_hits(game, content, target, None, false, false, &preview.hits);
            let actual = resolved.map_or(actual as f32, |losses| losses[target]);
            let mut row = token(
                ENEMY_COLLECTION,
                4,
                enemy.creature.id as usize + 1,
                0,
                target + 1,
            );
            row[10..19].copy_from_slice(&[
                enemy.creature.kind(content, PowerKind::Vulnerable) as f32,
                enemy.creature.block as f32,
                cap as f32,
                damage as f32,
                actual,
                0.0,
                (actual >= enemy.creature.hp as f32) as u8 as f32,
                enemy.creature.hp as f32,
                1.0,
            ]);
            out.push(row);
        }
    }
    values[48..57].copy_from_slice(&action_preview_values(
        game,
        content,
        action,
        Some(&preview),
    ));
    out.sort_by(|left, right| token_cmp(left, right));
}

fn public_card(
    mut card: Card,
    deck: &[Card],
    masters: &HashMap<u32, usize>,
    dampened: &[(u32, u8)],
) -> Card {
    let (master, upgrades) = card_relation(card, deck, masters, dampened);
    card.instance = master as u32 * 257 + upgrades as u32;
    card
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
    out.push(token(STATE_COLLECTION, 1, run.character as usize + 1, 0, 0));

    let mut row = token(STATE_COLLECTION, 2, game.act as usize + 1, 0, 0);
    row[10..16].copy_from_slice(&[
        run.act as f32 / 3.0,
        run.floor as f32 / 18.0,
        game.bosses_visited as f32 / 2.0,
        game.weak_encounters_left as f32 / 20.0,
        game.regular_encounters_left as f32 / 20.0,
        game.elite_encounters_left as f32 / 20.0,
    ]);
    out.push(row);

    let mut phase = token(
        STATE_COLLECTION,
        3,
        phase_index(&game.phase) + 1,
        0,
        room_index(game.room) + 1,
    );
    phase[29..34].copy_from_slice(&[
        game.replacing_potion as u8 as f32,
        game.rerolled_cards as u8 as f32,
        game.parasol_removal as u8 as f32,
        game.conveyor as u8 as f32,
        game.fake_shop as u8 as f32,
    ]);

    let mut row = token(STATE_COLLECTION, 4, 0, 0, 0);
    row[10..12].copy_from_slice(&[
        run.hp.max(0) as f32 / run.max_hp.max(1) as f32,
        run.max_hp as f32 / 100.0,
    ]);
    out.push(row);

    let mut row = token(STATE_COLLECTION, 5, 0, 0, 0);
    row[10] = run.gold as f32 / 500.0;
    out.push(row);

    let mut row = token(STATE_COLLECTION, 6, 0, 0, 0);
    row[10..13].copy_from_slice(&[
        run.energy as f32 / 10.0,
        run.draw as f32 / 10.0,
        run.orb_slots as f32 / 10.0,
    ]);
    out.push(row);

    let mut row = token(STATE_COLLECTION, 7, 0, 0, 0);
    row[10..12].copy_from_slice(&[
        run.card_shop_removals as f32 / 10.0,
        game.removal_price as f32 / 500.0,
    ]);
    out.push(row);

    let mut row = token(STATE_COLLECTION, 8, 0, 0, 0);
    row[10..13].copy_from_slice(&[
        run.ascension as f32 / 10.0,
        bonuses.0 as f32 / 32.0,
        bonuses.1 as f32 / 32.0,
    ]);
    out.push(row);

    let mut row = token(STATE_COLLECTION, 9, 0, 0, 0);
    row[10] = game.rarity_offset as f32 / 10.0;
    out.push(row);

    let mut row = token(STATE_COLLECTION, 10, 0, 0, 0);
    row[10] = game.potion_odds as f32 / 100.0;
    out.push(row);

    let mut row = token(STATE_COLLECTION, 11, 0, 0, 0);
    row[10] = game.event_combat as f32 / 10.0;
    out.push(row);

    if matches!(game.phase, Phase::Rest) {
        out.push(token(
            STATE_COLLECTION,
            12,
            game.rest_used as usize + 1,
            0,
            0,
        ));
    }

    let mut row = token(STATE_COLLECTION, 13, 0, 0, 0);
    row[10..14].copy_from_slice(&game.unknown_odds.map(|value| value as f32 / 100.0));
    out.push(row);

    if let Some(crystal) = &game.crystal {
        let mut row = token(STATE_COLLECTION, 14, 0, 0, 0);
        row[10..14].copy_from_slice(&[
            crystal.remaining as f32 / 10.0,
            crystal.big as u8 as f32,
            crystal.clear.iter().filter(|&&clear| clear).count() as f32 / 25.0,
            crystal.revealed.len() as f32 / 25.0,
        ]);
        out.push(row);
    }
    match &game.phase {
        Phase::Combat(combat) => {
            let mut row = token(STATE_COLLECTION, 15, 0, 0, 0);
            row[10] = combat.energy as f32 / 10.0;
            out.push(row);

            let mut row = token(STATE_COLLECTION, 16, 0, 0, 0);
            row[10] = combat.stars as f32 / 10.0;
            out.push(row);

            let mut row = token(STATE_COLLECTION, 17, 0, 0, 0);
            row[10] = combat.turn as f32 / 20.0;
            out.push(row);

            let mut row = token(STATE_COLLECTION, 18, 0, 1, 0);
            row[10..16].copy_from_slice(&[
                combat.player.hp.max(0) as f32 / combat.player.max_hp.max(1) as f32,
                combat.player.max_hp as f32 / 500.0,
                combat.player.block as f32 / 100.0,
                combat.max_energy as f32 / 10.0,
                combat.draw_per_turn as f32 / 10.0,
                combat.orb_slots as f32 / 10.0,
            ]);
            out.push(row);

            if combat.osty.max_hp > 0 {
                let mut row = token(STATE_COLLECTION, 19, 0, 2, 0);
                row[10..13].copy_from_slice(&[
                    combat.osty.hp.max(0) as f32 / combat.osty.max_hp as f32,
                    combat.osty.max_hp as f32 / 500.0,
                    combat.osty.block as f32 / 100.0,
                ]);
                out.push(row);
            }

            let history = combat.history;
            let mut row = token(STATE_COLLECTION, 20, 0, 1, 0);
            row[10] = history.cards as f32 / 20.0;
            out.push(row);

            let mut row = token(STATE_COLLECTION, 21, 0, 1, 0);
            row[10..32].copy_from_slice(
                &[
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
                    combat.last_cards,
                ]
                .map(|value| value as f32 / 20.0),
            );
            out.push(row);

            let mut row = token(STATE_COLLECTION, 22, 0, 1, 0);
            row[10..15].copy_from_slice(&[
                combat.last_damage as f32 / 20.0,
                combat.drawn as f32 / 20.0,
                combat.lightning_channeled as f32 / 20.0,
                combat.orbs_channeled as f32 / 20.0,
                combat.poisoned as u8 as f32,
            ]);
            out.push(row);
            for slot in 0..combat.orb_slots as usize {
                let orb = combat.orbs.get(slot);
                let mut row = token(
                    STATE_COLLECTION,
                    23,
                    orb.map_or(0, |orb| orb.id as usize + 1),
                    0,
                    slot + 1,
                );
                row[10] = orb.map_or(0.0, |orb| orb.value as f32 / 100.0);
                out.push(row);
            }
            if let Some(choice) = combat.choice {
                let (filter, filter_value) = filter_features(choice.filter);
                let (op, op_value) = op_features(choice.op);
                let mut row = token(
                    STATE_COLLECTION,
                    24,
                    pile_index(choice.pile) + 1,
                    0,
                    filter + 1,
                );
                row[6] = (op + 1) as f32;
                row[10..17].copy_from_slice(&[
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
            if combat.choice.is_some()
                || !combat.queue.is_empty()
                || !combat.auto_plays.is_empty()
                || combat.enemy_turn
                || combat.ending
                || combat.force_end
                || combat.card_energy != 0
                || combat.card_stars != 0
                || combat.card_plays != 0
            {
                let mut row = token(STATE_COLLECTION, 25, 0, 1, 0);
                row[10..16].copy_from_slice(&[
                    combat.card_energy as f32 / 20.0,
                    combat.card_stars as f32 / 20.0,
                    combat.card_plays as f32 / 20.0,
                    combat.enemy_turn as u8 as f32,
                    combat.ending as u8 as f32,
                    combat.force_end as u8 as f32,
                ]);
                out.push(row);
            }
        }
        Phase::Rewards(rewards) => {
            phase[10] = rewards.gold as f32 / 500.0;
            phase[11] = rewards.removals as f32 / 10.0;
            for (position, reward) in rewards.card_rewards.iter().enumerate() {
                let (kind, id) = match reward {
                    CardReward::Standard(room) => (1, room_index(*room) + 1),
                    CardReward::Fixed(character, rarity) => {
                        (2, *character as usize * 8 + *rarity as usize + 1)
                    }
                    CardReward::Kaleidoscope => (3, 0),
                    CardReward::Crystal(rarity) => (4, *rarity as usize + 1),
                };
                let mut row = token(STATE_COLLECTION, 26, id, 0, position + 1);
                row[5] = kind as f32;
                out.push(row);
            }
        }
        Phase::Event(id, options) => {
            phase[2] = *id as f32 + 1.0;
            let name = content.events[*id as usize].id;
            if name == "EVENT.SLIPPERY_BRIDGE" {
                phase[10] = game.event_data[0] as f32 / 1_000.0;
            } else if name == "EVENT.TINKER_TIME" {
                phase[10] = game.event_data[3] as f32 / 3.0;
                for (slot, &offer) in game.event_data.iter().take(options.len()).enumerate() {
                    phase[TOKEN_CATEGORICAL + 1 + slot] = offer as f32 / 10.0;
                }
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
                    phase[TOKEN_CATEGORICAL + slot] = value as f32 / 1_000.0;
                }
            }
        }
        Phase::RemoveCards(count, price, optional) => {
            phase[10..13].copy_from_slice(&[
                *count as f32 / 10.0,
                *price as f32 / 500.0,
                *optional as u8 as f32,
            ]);
        }
        Phase::UpgradeCards(count, optional) => {
            phase[10] = *count as f32 / 10.0;
            phase[11] = *optional as u8 as f32;
        }
        Phase::TransformCards(target, count, optional) => {
            phase[2] = target.map_or(0, |id| id as usize + 1) as f32;
            phase[10] = *count as f32 / 10.0;
            phase[11] = *optional as u8 as f32;
        }
        Phase::EnchantCards(enchantment, amount, count, card_type, optional) => {
            phase[2] = *enchantment as usize as f32 + 1.0;
            phase[5] = card_type.map_or(0, |kind| kind as usize + 1) as f32;
            phase[10..13].copy_from_slice(&[
                *amount as f32 / 100.0,
                *count as f32 / 10.0,
                *optional as u8 as f32,
            ]);
        }
        Phase::ChooseCards(_, count, reward) => {
            phase[10] = *count as f32 / 10.0;
            phase[11] = *reward as u8 as f32;
        }
        _ => {}
    }
    out.push(phase);
}

fn push_power_tokens(
    content: &Content,
    out: &mut Vec<Token>,
    snapshot: bool,
    owner: usize,
    powers: &[Power],
) {
    for (position, &power) in powers.iter().enumerate() {
        let name = content.powers[power.id as usize].id;
        let kind = match name {
            "POWER.SURROUNDED_POWER" => 2 + snapshot as usize * 2 + (power.value > 0) as usize,
            _ => snapshot as usize,
        };
        let mut row = token(
            POWER_COLLECTION,
            kind,
            power.id as usize + 1,
            owner,
            if name == "POWER.CONSTRICT_POWER" {
                power.value.max(0) as usize + 1
            } else {
                position + 1
            },
        );
        if matches!(
            name,
            "POWER.POSSESS_STRENGTH_POWER" | "POWER.POSSESS_SPEED_POWER"
        ) {
            row[10] = power.value as f32 / 32.0;
        } else {
            row[10] = power.amount as f32 / 32.0;
            if !matches!(name, "POWER.CONSTRICT_POWER" | "POWER.SURROUNDED_POWER") {
                row[11] = power.value as f32 / 32.0;
            }
        }
        row[12] = power.skip_next_decay as u8 as f32;
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

#[derive(Default)]
struct ExactBytes(Vec<u8>);

impl Hasher for ExactBytes {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
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
        self.write_u64(value as u64);
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
        self.write_i64(value as i64);
    }
}

fn push_exact_token<T: Hash>(
    out: &mut Vec<Token>,
    kind: usize,
    position: usize,
    id: usize,
    value: &T,
) {
    let mut bytes = ExactBytes::default();
    value.hash(&mut bytes);
    let bytes = bytes.0;
    assert!(bytes.len() <= CONTINUATION_BYTES);
    let mut row = token(CONTINUATION_COLLECTION, kind, id + 1, 0, position + 1);
    row[10] = bytes.len() as f32 / CONTINUATION_BYTES as f32;
    out.push(row);
    for (part, bytes) in bytes.chunks(TOKEN_NUMERIC).enumerate() {
        let mut row = token(
            CONTINUATION_COLLECTION,
            EFFECT_DETAIL_KIND,
            kind + 1,
            position + 1,
            part + 1,
        );
        for (slot, &byte) in bytes.iter().enumerate() {
            row[TOKEN_CATEGORICAL + slot] = byte as f32 / 255.0;
        }
        out.push(row);
    }
}

fn effect_index(effect: &Effect) -> usize {
    match effect {
        Effect::Attack(..) => 0,
        Effect::AttackMany(..) => 1,
        Effect::OstyAttack(..) => 2,
        Effect::OstyAttackMany(..) => 3,
        Effect::MoveDamage(..) => 4,
        Effect::Damage(..) => 5,
        Effect::Kill(..) => 6,
        Effect::Stun(..) => 7,
        Effect::DoomKill => 8,
        Effect::LoseHp(..) => 9,
        Effect::Block(..) => 10,
        Effect::BlockNextTurn(..) => 11,
        Effect::DodgeRoll(..) => 12,
        Effect::DrawBlockIf(..) => 13,
        Effect::RawBlock(..) => 14,
        Effect::Heal(..) => 15,
        Effect::HealPercent(..) => 16,
        Effect::MaxHp(..) => 17,
        Effect::Gold(..) => 18,
        Effect::RandomPotion => 19,
        Effect::Bomb(..) => 20,
        Effect::Automation(..) => 21,
        Effect::Panache(..) => 22,
        Effect::RollingBoulder(..) => 23,
        Effect::ToricToughness(..) => 24,
        Effect::ApplyPower(..) => 25,
        Effect::ApplyDebuff(..) => 26,
        Effect::StackPower(..) => 27,
        Effect::DoublePower(..) => 28,
        Effect::Misery(..) => 29,
        Effect::RemovePower(..) => 30,
        Effect::TemporaryStrength(..) => 31,
        Effect::Draw(..) => 32,
        Effect::HandDraw(..) => 33,
        Effect::DrawAmount(..) => 34,
        Effect::DrawTo(..) => 35,
        Effect::DrawUntilNot(..) => 36,
        Effect::ChooseDraw(..) => 37,
        Effect::FreeHand => 38,
        Effect::Energy(..) => 39,
        Effect::DoubleEnergy => 40,
        Effect::AddCard(..) => 41,
        Effect::AddUpgradedCard(..) => 42,
        Effect::AddFlaggedCard(..) => 43,
        Effect::RandomCard(..) => 44,
        Effect::RandomColorless(..) => 45,
        Effect::RandomColorlessOther(..) => 46,
        Effect::OfferColorless(..) => 47,
        Effect::OfferCharacter(..) => 48,
        Effect::OfferCharacterRetain(..) => 49,
        Effect::OfferCharacterType(..) => 50,
        Effect::OfferOtherCharacter(..) => 51,
        Effect::RandomCharacter(..) => 52,
        Effect::DistinctCharacter(..) => 53,
        Effect::RandomCharacterCost0(..) => 54,
        Effect::RandomCardOp(..) => 55,
        Effect::AutoPlayRandom(..) => 56,
        Effect::ChooseRandomDraw(..) => 57,
        Effect::SelectAmount(..) => 58,
        Effect::ShuffleHandDraw(..) => 59,
        Effect::FillPotions => 60,
        Effect::RandomizeHandCosts => 61,
        Effect::ReplayTagged(..) => 62,
        Effect::ChannelSlots(..) => 63,
        Effect::DistinctColorless(..) => 64,
        Effect::AddRandomCard(..) => 65,
        Effect::AddRandom(..) => 66,
        Effect::FranticEscape => 67,
        Effect::Aggression(..) => 68,
        Effect::AutoPlayDraw(..) => 69,
        Effect::Stoke => 70,
        Effect::Stampede => 71,
        Effect::ContinueEndTurn => 72,
        Effect::FlakCannon(..) => 73,
        Effect::CopyCard(..) => 74,
        Effect::Discard(..) => 75,
        Effect::DiscardHandDraw => 76,
        Effect::DiscardHandAdd(..) => 77,
        Effect::PlayExhaustedShivs => 78,
        Effect::AutoPlay(..) => 79,
        Effect::FinishAutoPlay => 80,
        Effect::WhisperingEarring(..) => 81,
        Effect::MoveAll(..) => 82,
        Effect::DrawFiltered(..) => 83,
        Effect::DrawFilteredStep(..) => 84,
        Effect::FinishDrawFiltered(..) => 85,
        Effect::Exhaust(..) => 86,
        Effect::ExhaustForBlock(..) => 87,
        Effect::ExhaustForAttack(..) => 88,
        Effect::ExhaustAttackStep(..) => 89,
        Effect::Upgrade(..) => 90,
        Effect::TransformHand(..) => 91,
        Effect::If(..) => 92,
        Effect::Repeat(..) => 93,
        Effect::Random(..) => 94,
        Effect::Channel(..) => 95,
        Effect::RandomOrb(..) => 96,
        Effect::Evoke(..) => 97,
        Effect::EvokeMany(..) => 98,
        Effect::EvokeLast(..) => 99,
        Effect::EvokeAll(..) => 100,
        Effect::PassiveFirst(..) => 101,
        Effect::PassiveLast(..) => 102,
        Effect::PassiveAll => 103,
        Effect::OrbSlots(..) => 104,
        Effect::Stars(..) => 105,
        Effect::Forge(..) => 106,
        Effect::Summon(..) => 107,
        Effect::MaxEnergy(..) => 108,
        Effect::GrowCard(..) => 109,
        Effect::GrowDrawn(..) => 110,
        Effect::GrowAll(..) => 111,
        Effect::PersistCard(..) => 112,
        Effect::SetCardCost(..) => 113,
        Effect::ReduceCardCost(..) => 114,
        Effect::CapHandCosts => 115,
        Effect::RecycleHand(..) => 116,
        Effect::EndTurn => 117,
        Effect::Select(..) => 118,
    }
}

fn actor_index(actor: Actor) -> usize {
    match actor {
        Actor::Player => 1,
        Actor::Osty => 2,
        Actor::Enemy(index) => index + 3,
    }
}

fn continuation_tokens(game: &Game, content: &Content, out: &mut Vec<Token>) {
    for (position, effect) in game.run_queue.iter().rev().enumerate() {
        push_exact_token(out, 7, position, event_effect_index(effect), effect);
    }
    if let Some(combat) = game.combat() {
        let masters = master_cards(&game.run.deck);
        let normalize = |card| public_card(card, &game.run.deck, &masters, &combat.dampened);
        for (position, pending) in combat.queue.iter().rev().enumerate() {
            let effect = match pending.effect {
                Effect::AutoPlay(card) => Effect::AutoPlay(normalize(card)),
                effect => effect,
            };
            push_exact_token(out, 8, position, effect_index(&effect), &effect);
            let mut context = token(
                CONTINUATION_COLLECTION,
                18,
                pending.context.card.map_or(0, |id| id as usize + 1),
                actor_index(pending.context.source),
                position + 1,
            );
            context[10..17].copy_from_slice(&[
                pending.context.target.map_or(0, |target| target + 1) as f32,
                pending.context.upgraded as u8 as f32,
                pending.context.x as f32,
                pending.context.event as f32,
                pending.context.orb as u8 as f32,
                pending.context.orb_id.map_or(0, |id| id as usize + 1) as f32,
                pending.context.pen_nib as u8 as f32,
            ]);
            out.push(context);
        }
        for (position, play) in combat.auto_plays.iter().rev().enumerate() {
            let mut public = play.clone();
            public.card = normalize(public.card);
            push_exact_token(out, 9, position, public.card.id as usize, &public);
        }
    }
    let Some(phase) = &game.resume else {
        return;
    };
    let mut row = token(CONTINUATION_COLLECTION, 10, phase_index(phase) + 1, 0, 0);
    match phase {
        Phase::Rewards(rewards) => {
            row[10] = rewards.gold as f32 / 500.0;
            row[11] = rewards.removals as f32 / 10.0;
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
                let kind = match reward {
                    CardReward::Standard(_) => 0,
                    CardReward::Fixed(..) => 1,
                    CardReward::Kaleidoscope => 2,
                    CardReward::Crystal(_) => 3,
                };
                push_exact_token(out, 13, position, kind, reward);
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
                        item[10] = *price as f32 / 500.0;
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
                        item[10] = *price as f32 / 500.0;
                        out.push(item);
                    }
                    ShopItem::Remove(price) => {
                        let mut item = token(CONTINUATION_COLLECTION, 16, 0, 0, position + 1);
                        item[10] = *price as f32 / 500.0;
                        out.push(item);
                    }
                    ShopItem::Card(..) => {}
                }
            }
        }
        Phase::Event(id, options) => {
            row[2] = *id as f32 + 1.0;
            for (position, option) in options.iter().enumerate() {
                let kind = match option.requirement {
                    Requirement::Always => 0,
                    Requirement::Gold(_) => 1,
                    Requirement::Hp(_) => 2,
                    Requirement::Deck => 3,
                };
                push_exact_token(out, 17, position, kind, option);
            }
        }
        Phase::RemoveCards(count, tag, optional) => {
            row[10..13].copy_from_slice(&[
                *count as f32 / 10.0,
                *tag as f32 / u16::MAX as f32,
                *optional as u8 as f32,
            ]);
        }
        Phase::UpgradeCards(count, optional) => {
            row[10] = *count as f32 / 10.0;
            row[11] = *optional as u8 as f32;
        }
        Phase::TransformCards(target, count, optional) => {
            row[2] = target.map_or(0, |id| id as usize + 1) as f32;
            row[10] = *count as f32 / 10.0;
            row[11] = *optional as u8 as f32;
        }
        Phase::EnchantCards(enchantment, amount, count, card_type, optional) => {
            row[2] = *enchantment as usize as f32 + 1.0;
            row[3] = card_type.map_or(0, |kind| kind as usize + 1) as f32;
            row[10..13].copy_from_slice(&[
                *amount as f32 / 100.0,
                *count as f32 / 10.0,
                *optional as u8 as f32,
            ]);
        }
        Phase::ChooseCards(_, count, reward) => {
            row[10] = *count as f32 / 10.0;
            row[11] = *reward as u8 as f32;
        }
        Phase::Combat(_) => unreachable!("combat cannot be a resume phase"),
        _ => {}
    }
    out.push(row);
    phase_card_tokens(game, content, phase, out);
}

fn public_relic_bags(game: &Game, content: &Content, layout: Layout) -> [Vec<Vec<Id>>; 2] {
    let trader = matches!(game.phase, Phase::Event(id, _) if Some(id) == layout.relic_trader);
    let mut bags = [
        game.relic_deques.to_vec(),
        game.shared_relic_deques.to_vec(),
    ];
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
    bags
}

fn public_encounters<'a>(game: &Game, content: &'a Content, elite: bool) -> Vec<Id> {
    let full = public_encounter_pool(game, content, elite);
    let active = if elite {
        game.elite_encounters_left > 0
    } else {
        game.weak_encounters_left > 0 || game.regular_encounters_left > 0
    };
    if !active {
        return full;
    }
    let remaining = if elite {
        &game.elites
    } else {
        &game.encounters
    };
    let mut public = remaining
        .iter()
        .copied()
        .filter(|id| full.contains(id))
        .collect::<Vec<_>>();
    public.sort_unstable();
    public.dedup();
    if public.is_empty() { full } else { public }
}

fn relic_state(game: &Game, content: &Content, id: Id) -> [f32; 4] {
    let combat = game.combat();
    let flag = |value: bool| value as u8 as f32;
    match content.relics[id as usize].id {
        "RELIC.HAPPY_FLOWER" => [game.happy_flower as f32, 0.0, 0.0, 0.0],
        "RELIC.FAKE_HAPPY_FLOWER" => [game.fake_happy_flower as f32, 0.0, 0.0, 0.0],
        "RELIC.VENERABLE_TEA_SET" => [(game.tea_set / 2 * 2) as f32, 0.0, 0.0, 0.0],
        "RELIC.FAKE_VENERABLE_TEA_SET" => [(game.tea_set % 2) as f32, 0.0, 0.0, 0.0],
        "RELIC.LASTING_CANDY" => [game.lasting_candy as f32, 0.0, 0.0, 0.0],
        "RELIC.PAELS_WING" => [game.paels_wing as f32, 0.0, 0.0, 0.0],
        "RELIC.SILVER_CRUCIBLE" => [
            game.silver_crucible as f32,
            game.silver_treasures as f32,
            0.0,
            0.0,
        ],
        "RELIC.WINGED_BOOTS" => [game.winged_boots as f32, 0.0, 0.0, 0.0],
        "RELIC.GIRYA" => [game.girya as f32, 0.0, 0.0, 0.0],
        "RELIC.PUMPKIN_CANDLE" => [game.pumpkin_candle as f32, 0.0, 0.0, 0.0],
        "RELIC.TOY_BOX" => [game.toy_box_combats as f32, 0.0, 0.0, 0.0],
        "RELIC.NUNCHAKU" => [game.nunchaku as f32, 0.0, 0.0, 0.0],
        "RELIC.PENDULUM" => [game.pendulum as f32, 0.0, 0.0, 0.0],
        "RELIC.PEN_NIB" => [
            game.pen_nib as f32,
            flag(combat.is_some_and(|combat| combat.pen_nib)),
            0.0,
            0.0,
        ],
        "RELIC.IRON_CLUB" => [game.iron_club as f32, 0.0, 0.0, 0.0],
        "RELIC.JOSS_PAPER" => [game.joss_paper as f32, 0.0, 0.0, 0.0],
        "RELIC.TUNING_FORK" => [game.tuning_fork as f32, 0.0, 0.0, 0.0],
        "RELIC.GALACTIC_DUST" => [game.galactic_dust as f32, 0.0, 0.0, 0.0],
        "RELIC.BOOK_OF_FIVE_RINGS" => [game.book_of_five_rings as f32, 0.0, 0.0, 0.0],
        "RELIC.EMBER_TEA" => [game.ember_tea as f32, 0.0, 0.0, 0.0],
        "RELIC.SWORD_OF_STONE" => [game.sword_of_stone as f32, 0.0, 0.0, 0.0],
        "RELIC.MAW_BANK" => [flag(game.maw_bank), 0.0, 0.0, 0.0],
        "RELIC.LIZARD_TAIL" => [flag(game.lizard_tail), 0.0, 0.0, 0.0],
        "RELIC.SILKEN_TRESS" => [flag(game.silken_tress), 0.0, 0.0, 0.0],
        "RELIC.BONE_TEA" => [flag(game.bone_tea), 0.0, 0.0, 0.0],
        "RELIC.TEA_OF_DISCOURTESY" => [flag(game.tea_of_discourtesy), 0.0, 0.0, 0.0],
        "RELIC.WONGOS_MYSTERY_TICKET" => [
            game.wongo_combats.unwrap_or_default() as f32,
            flag(game.wongo_combats.is_some()),
            0.0,
            0.0,
        ],
        "RELIC.GOLDEN_COMPASS" => [
            flag(game.golden_compass == Some(game.run.act)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.MEAT_CLEAVER" => [flag(game.cooking), 0.0, 0.0, 0.0],
        "RELIC.NEOWS_BONES" => [flag(game.pending_curse), 0.0, 0.0, 0.0],
        "RELIC.ASTROLABE" => [flag(game.astrolabe), flag(game.transform_niche), 0.0, 0.0],
        "RELIC.FISHING_ROD" => [game.fishing_rod as f32, 0.0, 0.0, 0.0],
        "RELIC.POLLINOUS_CORE" => [game.pollinous_core as f32, 0.0, 0.0, 0.0],
        "RELIC.PAELS_TOOTH" => [flag(game.paels_tooth), 0.0, 0.0, 0.0],
        "RELIC.PAELS_TEARS" => [
            flag(combat.is_some_and(|combat| combat.paels_tears)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.CENTENNIAL_PUZZLE" => [
            flag(combat.is_some_and(|combat| combat.centennial_puzzle)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.DEMON_TONGUE" => [
            flag(combat.is_some_and(|combat| combat.demon_tongue)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.PERMAFROST" => [
            flag(combat.is_some_and(|combat| combat.permafrost)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.RUINED_HELMET" => [
            flag(combat.is_some_and(|combat| combat.ruined_helmet)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.MUSIC_BOX" => [
            flag(combat.is_some_and(|combat| combat.music_box)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.MINI_REGENT" => [
            flag(combat.is_some_and(|combat| combat.mini_regent)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.RAINBOW_RING" => [
            flag(combat.is_some_and(|combat| combat.rainbow_ring)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.KUSARIGAMA" => [
            combat.map_or(0, |combat| combat.kusarigama) as f32,
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.UNSETTLING_LAMP" => [
            flag(combat.is_some_and(|combat| combat.unsettling_used)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.DIAMOND_DIADEM" => [
            flag(combat.is_some_and(|combat| combat.diamond_diadem)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.BURNING_STICKS" => [
            flag(combat.is_some_and(|combat| combat.burning_sticks)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.THROWING_AXE" => [
            flag(combat.is_some_and(|combat| combat.throwing_axe)),
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.PAELS_EYE" => [
            flag(combat.is_some_and(|combat| combat.paels_eye)),
            flag(combat.is_some_and(|combat| combat.paels_eye_extra)),
            0.0,
            0.0,
        ],
        "RELIC.PAELS_LEGION" => [
            combat.map_or(0, |combat| combat.paels_legion) as f32,
            0.0,
            0.0,
            0.0,
        ],
        "RELIC.BELT_BUCKLE" => [
            flag(combat.is_some_and(|combat| combat.belt_buckle)),
            0.0,
            0.0,
            0.0,
        ],
        _ => [0.0; 4],
    }
}

fn state_entity_tokens(game: &Game, content: &Content, layout: Layout) -> Vec<Token> {
    let mut out = Vec::new();
    if let Some(combat) = game.combat() {
        for (position, enemy) in combat.enemies.iter().enumerate() {
            let mut row = token(
                ENEMY_COLLECTION,
                0,
                enemy.creature.id as usize + 1,
                position + 3,
                position + 1,
            );
            row[10] = if content.enemies[enemy.creature.id as usize].id == "MONSTER.WRIGGLER" {
                (position % 2 + 1) as f32
            } else {
                0.0
            };
            row[12] = enemy.creature.hp as f32 / 500.0;
            row[13] = enemy.creature.max_hp as f32 / 500.0;
            row[14] = enemy.creature.block as f32 / 100.0;
            row[15] = enemy.move_index as f32 / 20.0;
            row[16] = if enemy.last_move == usize::MAX {
                -1.0
            } else {
                enemy.last_move as f32 / 20.0
            };
            row[17] = enemy.repeats as f32 / 10.0;
            row[18] = enemy.stunned as u8 as f32;
            row[19] = enemy.value as f32 / 100.0;
            row[20] = combat.hits.get(position).copied().unwrap_or_default() as f32 / 20.0;
            out.push(row);
            for (history, &prior) in enemy.move_history.iter().enumerate() {
                out.push(token(
                    ENEMY_COLLECTION,
                    1,
                    prior + 1,
                    position + 3,
                    history + 1,
                ));
            }
            push_power_tokens(
                content,
                &mut out,
                false,
                position + 3,
                &enemy.creature.powers,
            );
            if let Some(powers) = combat.enemy_power_snapshot.get(position) {
                push_power_tokens(content, &mut out, true, position + 3, powers);
            }
        }
        push_power_tokens(content, &mut out, false, 1, &combat.player.powers);
        push_power_tokens(content, &mut out, false, 2, &combat.osty.powers);
        push_power_tokens(content, &mut out, true, 1, &combat.power_snapshot);
        if game.damage_taken {
            let mut row = token(POWER_COLLECTION, VIRTUAL_POWER_KIND, 1, 1, 0);
            row[10] = 1.0;
            out.push(row);
        }
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
                row[24] = upgrades as f32 + 1.0;
                out.push(row);
            }
        }
        for (position, pending) in combat.queue.iter().rev().enumerate() {
            if let Effect::AutoPlay(card) = pending.effect {
                let mut row = semantic_card_token(
                    game,
                    content,
                    CONTINUATION_COLLECTION,
                    QUEUED_CARD_KIND,
                    position + 1,
                    card,
                    0,
                );
                row[4] = (position + 1) as f32;
                out.push(row);
            }
        }
        for (position, &(turns, amount)) in combat.bombs.iter().enumerate() {
            let mut row = token(DELAYED_COLLECTION, 2, 0, 0, position + 1);
            row[10] = turns as f32 / 3.0;
            row[11] = amount as f32 / 100.0;
            out.push(row);
        }
        for (position, &(turns, energy)) in combat.automation.iter().enumerate() {
            let mut row = token(DELAYED_COLLECTION, 3, 0, 0, position + 1);
            row[10] = turns as f32 / 10.0;
            row[11] = energy as f32 / 10.0;
            out.push(row);
        }
        for (position, &(cards, damage, start)) in combat.panache.iter().enumerate() {
            let mut row = token(DELAYED_COLLECTION, 4, 0, 0, position + 1);
            row[10] = cards as f32 / 5.0;
            row[11] = damage as f32 / 20.0;
            row[12] = start as f32 / 20.0;
            out.push(row);
        }
        for (position, &amount) in combat.boulders.iter().enumerate() {
            let mut row = token(DELAYED_COLLECTION, 5, 0, 0, position + 1);
            row[10] = amount as f32 / 100.0;
            out.push(row);
        }
        if let Some(id) = combat.unsettling_lamp {
            out.push(token(DELAYED_COLLECTION, 6, id as usize + 1, 0, 0));
        }
    }
    for (position, &id) in game.run.relics.iter().enumerate() {
        let mut row = token(RELIC_COLLECTION, 0, id as usize + 1, 0, position + 1);
        row[10] = game.melted_relics.contains(&position) as u8 as f32;
        row[11] = game
            .wax_relics
            .iter()
            .filter(|wax| !game.melted_relics.contains(wax))
            .position(|&wax| wax == position)
            .map_or(0, |x| x + 1) as f32;
        row[12..16].copy_from_slice(&relic_state(game, content, id));
        out.push(row);
    }
    match &game.phase {
        Phase::Rewards(rewards) => {
            for (position, &id) in rewards.relics.iter().enumerate() {
                out.push(token(
                    RELIC_COLLECTION,
                    if game.toy_box_offers.contains(&id) {
                        TOY_BOX_RELIC_KIND
                    } else {
                        3
                    },
                    id as usize + 1,
                    0,
                    position + 1,
                ));
            }
            for (position, &id) in rewards.potions.iter().enumerate() {
                out.push(token(
                    POTION_COLLECTION,
                    2,
                    id as usize + 1,
                    0,
                    position + 1,
                ));
            }
        }
        Phase::Shop(items) => {
            for (position, item) in items.iter().enumerate() {
                let mut row = match item {
                    ShopItem::Relic(id, _) => {
                        token(RELIC_COLLECTION, 7, *id as usize + 1, 0, position + 1)
                    }
                    ShopItem::Potion(id, _) => {
                        token(POTION_COLLECTION, 3, *id as usize + 1, 0, position + 1)
                    }
                    ShopItem::Remove(_) => token(CONTINUATION_COLLECTION, 2, 0, 0, position + 1),
                    ShopItem::Card(..) => continue,
                };
                row[10] = match item {
                    ShopItem::Relic(_, price)
                    | ShopItem::Potion(_, price)
                    | ShopItem::Remove(price) => *price as f32 / 500.0,
                    ShopItem::Card(..) => unreachable!(),
                };
                out.push(row);
            }
        }
        _ => {}
    }
    let trader = matches!(game.phase, Phase::Event(id, _) if Some(id) == layout.relic_trader);
    if let Phase::Event(_, options) = &game.phase {
        for (position, option) in options.iter().enumerate() {
            let requirement = match option.requirement {
                Requirement::Always => 0,
                Requirement::Gold(_) => 1,
                Requirement::Hp(_) => 2,
                Requirement::Deck => 3,
            };
            push_exact_token(
                &mut out,
                CURRENT_EVENT_OPTION_KIND,
                position,
                requirement,
                option,
            );
            if let Some(id) = ancient_offer(game, content, position) {
                out.push(token(
                    RELIC_COLLECTION,
                    ANCIENT_RELIC_KIND,
                    id as usize + 1,
                    position + 1,
                    0,
                ));
            }
        }
        if trader {
            for (position, &id) in game.relic_queue.iter().enumerate() {
                out.push(token(
                    RELIC_COLLECTION,
                    13,
                    id as usize + 1,
                    0,
                    position + 1,
                ));
            }
            for (position, &owned) in game.event_cards.iter().enumerate() {
                if let Some(&id) = game.run.relics.get(owned as usize) {
                    out.push(token(
                        RELIC_COLLECTION,
                        14,
                        id as usize + 1,
                        0,
                        position + 1,
                    ));
                }
            }
        }
    }
    let bags = public_relic_bags(game, content, layout);
    for (pool, bags) in bags.iter().enumerate() {
        for &id in bags.iter().flatten() {
            out.push(token(RELIC_COLLECTION, pool + 1, id as usize + 1, 0, 0));
        }
    }
    let character_pool = content.characters[game.run.character as usize].relic_pool;
    let mut pool = character_pool
        .iter()
        .chain(layout.shared_relics)
        .copied()
        .collect::<Vec<_>>();
    pool.sort_unstable();
    pool.dedup();
    for id in pool {
        if game.run.relics.contains(&id) {
            continue;
        }
        let in_general = bags[0].iter().flatten().any(|&candidate| candidate == id);
        let shared = layout.shared_relics.contains(&id);
        let in_shared = bags[1].iter().flatten().any(|&candidate| candidate == id);
        let excluded = (!in_general) as usize | ((shared && !in_shared) as usize) << 1;
        if excluded != 0 {
            out.push(token(
                RELIC_COLLECTION,
                SEEN_RELIC_KIND,
                id as usize + 1,
                excluded,
                0,
            ));
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
                row[10] = *price as f32 / 500.0;
                out.push(row);
            }
            ShopItem::Potion(id, price) => {
                let mut row = token(POTION_COLLECTION, 4, *id as usize + 1, 0, position + 1);
                row[10] = *price as f32 / 500.0;
                out.push(row);
            }
            ShopItem::Remove(price) => {
                let mut row = token(CONTINUATION_COLLECTION, 2, 0, 0, position + 1);
                row[10] = *price as f32 / 500.0;
                out.push(row);
            }
        }
    }
    for (position, &id) in game.fake_merchant.iter().enumerate() {
        out.push(token(RELIC_COLLECTION, 5, id as usize + 1, 0, position + 1));
    }
    for (position, &gold) in game.reward_gold_parts.iter().enumerate() {
        let mut row = token(CONTINUATION_COLLECTION, 3, 0, 0, position + 1);
        row[10] = gold as f32 / 500.0;
        out.push(row);
    }
    if game.conveyor {
        for (position, &value) in game.event_data.iter().enumerate() {
            let mut row = token(CONTINUATION_COLLECTION, 6, 0, 0, position + 1);
            row[10] = value as f32 / 1_000.0;
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

    for &id in content.acts[game.act as usize].events {
        out.push(token(
            EVENT_COLLECTION,
            game.visited_events.contains(&id) as usize + 1,
            id as usize + 1,
            0,
            0,
        ));
    }

    let encounters = encounter_vocabs(content);
    if let Some(id) = game.bosses[0] {
        out.push(token(
            ENCOUNTER_COLLECTION,
            1,
            encounter_id(&encounters[2], id),
            0,
            0,
        ));
    }
    for id in public_encounters(game, content, false) {
        out.push(token(
            ENCOUNTER_COLLECTION,
            2,
            encounter_id(&encounters[0], id),
            0,
            0,
        ));
    }
    for id in public_encounters(game, content, true) {
        out.push(token(
            ENCOUNTER_COLLECTION,
            3,
            encounter_id(&encounters[1], id),
            0,
            0,
        ));
    }
    if let Some(id) = game.last_encounter {
        out.push(token(
            ENCOUNTER_COLLECTION,
            4,
            encounter_id(&encounters[0], id),
            0,
            0,
        ));
    }
    if let Some(id) = game.last_elite {
        out.push(token(
            ENCOUNTER_COLLECTION,
            5,
            encounter_id(&encounters[1], id),
            0,
            0,
        ));
    }

    for node in game.map.nodes.iter() {
        let mut row = token(
            MAP_COLLECTION,
            0,
            room_index(node.room) + 1,
            map_floor_index(game, node.floor),
            node.lane as usize + 1,
        );
        row[10] = game
            .map
            .current
            .and_then(|x| game.map.nodes.get(x))
            .is_some_and(|x| x.floor == node.floor && x.lane == node.lane) as u8
            as f32;
        row[11] = (game.fur_coat_act == Some(game.run.act)
            && game.fur_coat.contains(&(node.lane, node.floor))) as u8 as f32;
        row[12] = (game.spoils == Some((node.lane, node.floor))) as u8 as f32;
        out.push(row);
        for &next in &node.next {
            if let Some(next) = game.map.nodes.get(next) {
                out.push(token(
                    MAP_COLLECTION,
                    1,
                    next.lane as usize + 1,
                    map_floor_index(game, node.floor),
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
                row[10] = clear as u8 as f32;
                out.push(row);
            }
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
    let mut summaries = Vec::new();
    state_summary_tokens(game, content, bonuses, &mut summaries);
    out.extend(summaries);
    out.sort_by(|left, right| token_cmp(left, right));
    out
}

fn globals_len(_: Layout) -> usize {
    PUBLIC_GLOBALS
}

fn observation_globals_with_bonuses(
    _: &Game,
    _: &Content,
    _: Layout,
    _: (i16, i16),
    _: bool,
) -> Vec<f32> {
    Vec::new()
}

#[cfg(test)]
fn observation_globals(game: &Game, content: &Content, layout: Layout, active: bool) -> Vec<f32> {
    observation_globals_with_bonuses(game, content, layout, (0, 0), active)
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

fn compact_action_values(
    game: &Game,
    layout: Layout,
    action: &Action,
) -> [f32; TOKEN_ACTION_VALUES] {
    let mut out = [0.0; TOKEN_ACTION_VALUES];
    out[action_kind(action)] = 1.0;
    if let Action::Event(index) = action {
        out[ACTION_KINDS] = (*index + 1) as f32 / 20.0;
        if matches!(&game.phase, Phase::Event(id, _) if Some(*id) == layout.tinker_time) {
            out[ACTION_KINDS + 1] =
                game.event_data.get(*index).copied().unwrap_or_default() as f32 / 10.0;
        }
    }
    out
}

fn action_entity_tokens(
    game: &Game,
    content: &Content,
    _layout: Layout,
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
                    map_floor_index(game, node.floor),
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
                        row[10] = *price as f32 / 500.0;
                        out.push(row);
                    }
                    ShopItem::Potion(id, price) => {
                        let mut row = token(POTION_COLLECTION, 7, *id as usize + 1, 0, index + 1);
                        row[10] = *price as f32 / 500.0;
                        out.push(row);
                    }
                    ShopItem::Remove(price) => {
                        let mut row = token(CONTINUATION_COLLECTION, 2, 0, 0, index + 1);
                        row[10] = *price as f32 / 500.0;
                        out.push(row);
                    }
                    ShopItem::Card(..) => {}
                }
            }
        }
        Action::Event(index) => {
            if let Phase::Event(_, options) = &game.phase {
                if let Some(option) = options.get(*index) {
                    push_event_option_tokens(option, &mut out);
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
        Action::CrystalTool(big) => out.push(token(CRYSTAL_COLLECTION, 2, *big as usize + 1, 0, 0)),
        Action::EventRelic(index, id) => {
            out.push(token(RELIC_COLLECTION, 15, *id as usize + 1, 0, *index + 1))
        }
        _ => {}
    }
    out
}

fn candidate_actions(game: &Game, content: &Content) -> (Vec<Action>, Vec<bool>) {
    let legal = game.actions(content);
    let mask = vec![true; legal.len()];
    (legal, mask)
}

#[cfg(test)]
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
    let _ = legal;
    let mut tokens = action_entity_tokens(game, content, layout, action);
    action_card_tokens(game, content, action, &mut values, &mut tokens);
    tokens.sort_by(|left, right| token_cmp(left, right));
    (values, tokens)
}

#[cfg(test)]
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

fn push_event_option_tokens(option: &EventOption, out: &mut Vec<Token>) {
    let (kind, value) = match option.requirement {
        Requirement::Always => (1, 0.0),
        Requirement::Gold(amount) => (2, amount as f32 / 500.0),
        Requirement::Hp(amount) => (3, amount as f32 / 100.0),
        Requirement::Deck => (4, 1.0),
    };
    let mut requirement = token(CONTINUATION_COLLECTION, 4, kind, 0, 0);
    requirement[10] = value;
    out.push(requirement);
    for (position, effect) in option.effects.iter().enumerate() {
        push_exact_token(out, 5, position, event_effect_index(effect), effect);
    }
}

fn target_index(target: Target) -> u32 {
    match target {
        Target::Source => 0,
        Target::Player => 1,
        Target::Osty => 2,
        Target::ChosenEnemy => 3,
        Target::ChosenEnemyOrDead => 4,
        Target::AllEnemies => 5,
        Target::OtherEnemies => 6,
        Target::RandomEnemy => 7,
        Target::LowestHpEnemy => 8,
    }
}

struct CardEncoding {
    masters: HashMap<u32, usize>,
}

impl CardEncoding {
    fn new(game: &Game, content: &Content) -> Self {
        let _ = content;
        Self {
            masters: master_cards(&game.run.deck),
        }
    }
}

fn card_domain_row(
    game: &Game,
    content: &Content,
    encoding: &mut CardEncoding,
    scope: i32,
    zone: u32,
    role: u32,
    order_kind: u32,
    order: u32,
    card: Card,
    context: i32,
) -> DomainRow {
    let def = content.cards[card.id as usize];
    let combat_dampened = game
        .combat()
        .map_or(&[][..], |combat| combat.dampened.as_slice());
    let (master, dampened) =
        card_relation(card, &game.run.deck, &encoding.masters, combat_dampened);
    let mut row = DomainRow::new(CARD_DOMAIN, scope);
    row.u.copy_from_slice(&[
        zone,
        role,
        order_kind,
        order,
        card.id as u32,
        card.upgrades as u32,
        card.flags as u32,
        card.turn_flags as u32,
        card.replays as u32,
        card.free as u32,
        card.cost_override.is_some() as u32,
        card.enchantment.map_or(0, |value| value as u32 + 1),
        card.variant as u32,
        dampened as u32,
        card.card_type(def) as u32,
        def.rarity as u32,
        target_index(card.target(def)),
        card.flags(def) as u32,
        def.tags as u32,
        master as u32,
    ]);
    row.s[..6].copy_from_slice(&[
        card.cost_delta as i32,
        card.value as i32,
        card.cost_override.unwrap_or_default() as i32,
        card.enchantment_amount as i32,
        card.enchantment_value as i32,
        context,
    ]);
    row
}

fn push_cards(
    domains: &mut [Vec<DomainRow>; 16],
    game: &Game,
    content: &Content,
    encoding: &mut CardEncoding,
    scope: i32,
    zone: u32,
    role: u32,
    cards: impl IntoIterator<Item = (u32, u32, Card, i32)>,
) {
    domains[CARD_DOMAIN].extend(cards.into_iter().map(|(order_kind, order, card, context)| {
        card_domain_row(
            game, content, encoding, scope, zone, role, order_kind, order, card, context,
        )
    }));
}

fn state_card_domains(
    game: &Game,
    content: &Content,
    encoding: &mut CardEncoding,
    domains: &mut [Vec<DomainRow>; 16],
) {
    push_cards(
        domains,
        game,
        content,
        encoding,
        STATE_SCOPE,
        DECK_ZONE as u32,
        0,
        game.run.deck.iter().copied().map(|card| (0, 0, card, 0)),
    );
    let active_relic = |wanted: &str| {
        game.run.relics.iter().enumerate().any(|(index, &id)| {
            !game.melted_relics.contains(&index) && content.relics[id as usize].id == wanted
        })
    };
    let mut pool = if active_relic("RELIC.PRISMATIC_GEM") {
        content
            .characters
            .iter()
            .flat_map(|character| character.cards)
            .copied()
            .collect()
    } else {
        content.characters[game.run.character as usize]
            .cards
            .to_vec()
    };
    if !active_relic("RELIC.PRISMATIC_GEM") && active_relic("RELIC.DINGY_RUG") {
        pool.extend_from_slice(content.colorless());
    }
    pool.sort_unstable();
    pool.dedup();
    push_cards(
        domains,
        game,
        content,
        encoding,
        STATE_SCOPE,
        CARD_POOL_ZONE as u32,
        0,
        pool.into_iter().map(|id| {
            (
                0,
                0,
                Card {
                    id,
                    ..Card::default()
                },
                0,
            )
        }),
    );
    push_cards(
        domains,
        game,
        content,
        encoding,
        STATE_SCOPE,
        PAEL_ZONE as u32,
        0,
        game.paels_cards
            .iter()
            .copied()
            .enumerate()
            .map(|(order, card)| (0, order as u32 + 1, card, 0)),
    );
    match &game.phase {
        Phase::Combat(combat) => {
            for (zone, cards) in [
                (HAND_ZONE, combat.hand.as_slice()),
                (DISCARD_ZONE, combat.discard.as_slice()),
                (EXHAUST_ZONE, combat.exhaust.as_slice()),
            ] {
                push_cards(
                    domains,
                    game,
                    content,
                    encoding,
                    STATE_SCOPE,
                    zone as u32,
                    0,
                    cards
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(order, card)| (3, order as u32 + 1, card, 0)),
                );
            }
            let draw_len = combat.draw.len();
            push_cards(
                domains,
                game,
                content,
                encoding,
                STATE_SCOPE,
                DRAW_ZONE as u32,
                0,
                combat
                    .draw
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(index, card)| {
                        if index < combat.known_draw_bottom {
                            (2, (index + 1) as u32, card, 0)
                        } else if index >= draw_len - combat.known_draw_top {
                            (1, (draw_len - index) as u32, card, 0)
                        } else {
                            (0, 0, card, 0)
                        }
                    }),
            );
        }
        _ => {}
    }
}

fn event_public_data(game: &Game, content: &Content) -> [i32; 4] {
    let Phase::Event(id, _) = game.phase else {
        return [0; 4];
    };
    let slots: &[usize] = match content.events[id as usize].id {
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
    };
    let mut out = [0; 4];
    for &slot in slots {
        out[slot] = i32::try_from(game.event_data[slot]).expect("public event value exceeds i32");
    }
    out
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

fn phase_domain_row(game: &Game, content: &Content) -> DomainRow {
    let mut row = DomainRow::new(PHASE_DOMAIN, STATE_SCOPE);
    row.u[0] = phase_index(&game.phase) as u32;
    row.u[2] = room_index(game.room) as u32;
    row.u[3] = game
        .resume
        .as_ref()
        .map_or(0, |phase| phase_index(phase) as u32 + 1);
    match &game.phase {
        Phase::Combat(combat) => {
            row.u[4] = combat.choice.is_some() as u32;
            row.u[13] = combat.enemy_turn as u32;
            row.u[14] = combat.ending as u32;
            row.u[15] = combat.force_end as u32;
            if let Some(choice) = combat.choice {
                let (filter, filter_value, filter_signed) = filter_payload(choice.filter);
                let (op, op_value, op_value2, op_signed) = op_payload(choice.op);
                row.u[5..13].copy_from_slice(&[
                    pile_index(choice.pile) as u32,
                    filter,
                    filter_value,
                    op,
                    op_value,
                    op_value2,
                    choice.remaining as u32,
                    choice.optional as u32,
                ]);
                row.s[0] = filter_signed;
                row.s[1] = op_signed;
            }
        }
        Phase::Rewards(rewards) => {
            row.s[0] = rewards.gold;
            row.u[4] = rewards.removals as u32;
            row.u[5] = rewards.card_rewards.len() as u32;
        }
        Phase::Event(id, options) => {
            row.u[1] = *id as u32 + 1;
            row.u[4] = options.len() as u32;
            row.s[..4].copy_from_slice(&event_public_data(game, content));
        }
        Phase::RemoveCards(count, tag, optional) => {
            row.u[4..7].copy_from_slice(&[*count as u32, *tag as u32, *optional as u32]);
        }
        Phase::UpgradeCards(count, optional) => {
            row.u[4..6].copy_from_slice(&[*count as u32, *optional as u32]);
        }
        Phase::TransformCards(target, count, optional) => {
            row.u[4..7].copy_from_slice(&[
                target.map_or(0, |id| id as u32 + 1),
                *count as u32,
                *optional as u32,
            ]);
        }
        Phase::EnchantCards(enchantment, amount, count, card_type, optional) => {
            row.u[4..8].copy_from_slice(&[
                *enchantment as u32 + 1,
                card_type.map_or(0, |kind| kind as u32 + 1),
                *count as u32,
                *optional as u32,
            ]);
            row.s[0] = *amount as i32;
        }
        Phase::ChooseCards(_, count, reward) => {
            row.u[4..6].copy_from_slice(&[*count as u32, *reward as u32]);
        }
        Phase::ChooseBundles(bundles) => row.u[4] = bundles.len() as u32,
        Phase::Map | Phase::Shop(_) | Phase::Rest | Phase::Won | Phase::Dead => {}
    }
    if let Some(crystal) = &game.crystal {
        row.u[13] = crystal.big as u32;
        row.u[14] = crystal.remaining as u32;
    }
    row
}

type CanonicalMap = (Arc<[DomainRow]>, Arc<[DomainRow]>, Arc<[u32]>, u32);

fn canonical_map(game: &Game, content: &Content, layout: Layout) -> CanonicalMap {
    let mut order = (0..game.map.nodes.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| {
        let node = &game.map.nodes[index];
        (node.floor, node.lane, room_index(node.room))
    });
    for pair in order.windows(2) {
        let left = &game.map.nodes[pair[0]];
        let right = &game.map.nodes[pair[1]];
        assert_ne!((left.floor, left.lane), (right.floor, right.lane));
    }
    let mut ids = vec![0; game.map.nodes.len()];
    for (position, &index) in order.iter().enumerate() {
        ids[index] = position as u32 + 1;
    }
    let terminal = order.len() as u32 + 1;
    let min_floor = order
        .iter()
        .map(|&index| game.map.nodes[index].floor)
        .min()
        .unwrap_or(0);
    let max_floor = order
        .iter()
        .map(|&index| game.map.nodes[index].floor)
        .max()
        .unwrap_or(0);
    let entry_degree = order
        .iter()
        .filter(|&&index| game.map.nodes[index].floor == min_floor)
        .count() as u32;
    let mut nodes = Vec::with_capacity(order.len() + 2);
    let mut entry = DomainRow::new(MAP_NODE_DOMAIN, STATE_SCOPE);
    entry.u[1] = 1;
    entry.u[8] = 0;
    entry.u[9] = entry_degree;
    nodes.push(entry);
    for &index in &order {
        let node = &game.map.nodes[index];
        let mut row = DomainRow::new(MAP_NODE_DOMAIN, STATE_SCOPE);
        row.u[..10].copy_from_slice(&[
            ids[index],
            0,
            node.floor as u32,
            node.lane as u32,
            room_index(node.room) as u32,
            0,
            (game.fur_coat_act == Some(game.run.act)
                && game.fur_coat.contains(&(node.lane, node.floor))) as u32,
            (game.spoils == Some((node.lane, node.floor))) as u32,
            node.floor.saturating_sub(min_floor) as u32 + 1,
            node.next.len() as u32 + node.next.is_empty() as u32,
        ]);
        nodes.push(row);
    }
    let mut end = DomainRow::new(MAP_NODE_DOMAIN, STATE_SCOPE);
    end.u[..10].copy_from_slice(&[
        terminal,
        2,
        max_floor as u32 + 1,
        0,
        0,
        1,
        0,
        0,
        max_floor.saturating_sub(min_floor) as u32 + 2,
        0,
    ]);
    nodes.push(end);
    let mut counts = std::collections::BTreeMap::new();
    for &index in &order {
        let node = &game.map.nodes[index];
        if node.next.is_empty() {
            *counts.entry((ids[index], terminal)).or_insert(0u32) += 1;
        }
        for &next in &node.next {
            let child = game.map.nodes.get(next).expect("invalid map edge");
            assert!(child.floor > node.floor);
            *counts.entry((ids[index], ids[next])).or_insert(0) += 1;
        }
    }
    for &index in &order {
        if game.map.nodes[index].floor == min_floor {
            *counts.entry((0, ids[index])).or_insert(0) += 1;
        }
    }
    let mut edges = counts
        .into_iter()
        .map(|((src, dst), multiplicity)| {
            let mut row = DomainRow::new(MAP_EDGE_DOMAIN, STATE_SCOPE);
            row.u[..3].copy_from_slice(&[src, dst, multiplicity]);
            row
        })
        .collect::<Vec<_>>();
    for row in &mut nodes {
        populate_domain_features(game, content, layout, MAP_NODE_DOMAIN, row);
    }
    for row in &mut edges {
        populate_domain_features(game, content, layout, MAP_EDGE_DOMAIN, row);
    }
    let current = game.map.current.map_or(0, |index| ids[index]);
    (nodes.into(), edges.into(), ids.into(), current)
}

fn run_domain_row(game: &Game, bonuses: (i16, i16), current: u32) -> DomainRow {
    let run = &game.run;
    let mut row = DomainRow::new(RUN_DOMAIN, STATE_SCOPE);
    row.u.copy_from_slice(&[
        run.character as u32,
        game.act as u32,
        run.ascension as u32,
        run.act as u32,
        run.floor as u32,
        room_index(game.room) as u32,
        game.bosses_visited as u32,
        game.weak_encounters_left as u32,
        game.regular_encounters_left as u32,
        game.elite_encounters_left as u32,
        run.energy as u32,
        run.draw as u32,
        run.orb_slots as u32,
        run.card_shop_removals as u32,
        game.event_combat as u32,
        game.rest_used as u32,
        game.replacing_potion as u32,
        game.pending_potion.map_or(0, |id| id as u32 + 1),
        game.pending_curse as u32,
        game.rerolled_cards as u32,
        game.parasol_removal as u32,
        game.conveyor as u32,
        game.fake_shop as u32,
        current,
    ]);
    row.s[..12].copy_from_slice(&[
        run.hp as i32,
        run.max_hp as i32,
        run.gold,
        game.removal_price,
        game.rarity_offset as i32,
        game.potion_odds as i32,
        game.unknown_odds[0] as i32,
        game.unknown_odds[1] as i32,
        game.unknown_odds[2] as i32,
        game.unknown_odds[3] as i32,
        bonuses.0 as i32,
        bonuses.1 as i32,
    ]);
    row
}

fn push_actor_domains(
    game: &Game,
    content: &Content,
    encoding: &CardEncoding,
    domains: &mut [Vec<DomainRow>; 16],
) {
    let Some(combat) = game.combat() else { return };
    let mut actor = |owner: u32, kind: u32, creature: &Creature, enemy: Option<&Enemy>| {
        let mut row = DomainRow::new(ACTOR_DOMAIN, STATE_SCOPE);
        let (move_index, last_move, repeats, stunned, value, hits, wriggler) =
            enemy.map_or((None, None, 0, false, 0, 0, 0), |enemy| {
                (
                    Some(enemy.move_index),
                    (enemy.last_move != usize::MAX).then_some(enemy.last_move),
                    enemy.repeats,
                    enemy.stunned,
                    enemy.value,
                    combat
                        .hits
                        .get(owner.saturating_sub(3) as usize)
                        .copied()
                        .unwrap_or_default(),
                    (content.enemies[enemy.creature.id as usize].id == "MONSTER.WRIGGLER")
                        .then(|| (owner - 3) % 2 + 1)
                        .unwrap_or(0),
                )
            });
        row.u[..10].copy_from_slice(&[
            owner,
            kind,
            creature.id as u32,
            move_index.is_some() as u32,
            move_index.unwrap_or_default() as u32,
            last_move.is_some() as u32,
            last_move.unwrap_or_default() as u32,
            repeats as u32,
            stunned as u32,
            wriggler,
        ]);
        row.s[..8].copy_from_slice(&[
            creature.hp as i32,
            creature.max_hp as i32,
            creature.block as i32,
            value as i32,
            hits as i32,
            if owner == 1 {
                combat.max_energy as i32
            } else {
                0
            },
            if owner == 1 {
                combat.draw_per_turn as i32
            } else {
                0
            },
            if owner == 1 {
                combat.orb_slots as i32
            } else {
                0
            },
        ]);
        if owner == 1 {
            row.s[8..16].copy_from_slice(&[
                combat.energy as i32,
                combat.stars as i32,
                combat.turn as i32,
                combat.card_energy as i32,
                combat.card_stars as i32,
                combat.card_plays as i32,
                combat.last_damage as i32,
                combat.drawn as i32,
            ]);
        }
        domains[ACTOR_DOMAIN].push(row);
    };
    actor(1, 0, &combat.player, None);
    if combat.osty.max_hp > 0 {
        actor(2, 1, &combat.osty, None);
    }
    for (index, enemy) in combat.enemies.iter().enumerate() {
        actor(index as u32 + 3, 2, &enemy.creature, Some(enemy));
    }
    push_power_domains(
        content,
        &mut domains[POWER_DOMAIN],
        1,
        false,
        &combat.player.powers,
    );
    if combat.osty.max_hp > 0 {
        push_power_domains(
            content,
            &mut domains[POWER_DOMAIN],
            2,
            false,
            &combat.osty.powers,
        );
    }
    for (index, enemy) in combat.enemies.iter().enumerate() {
        let owner = index as u32 + 3;
        push_power_domains(
            content,
            &mut domains[POWER_DOMAIN],
            owner,
            false,
            &enemy.creature.powers,
        );
        if enemy.stunned {
            push_status_domain(&mut domains[STATUS_DOMAIN], owner, 7, 0, &[]);
        }
        if enemy.value != 0 {
            push_status_domain(
                &mut domains[STATUS_DOMAIN],
                owner,
                8,
                0,
                &[enemy.value as i32],
            );
        }
        if let Some(&hits) = combat.hits.get(index)
            && hits != 0
        {
            push_status_domain(&mut domains[STATUS_DOMAIN], owner, 9, 0, &[hits as i32]);
        }
        if enemy.repeats != 0 {
            push_status_domain(
                &mut domains[STATUS_DOMAIN],
                owner,
                10,
                0,
                &[enemy.repeats as i32],
            );
        }
        if content.enemies[enemy.creature.id as usize].id == "MONSTER.WRIGGLER" {
            push_status_domain(
                &mut domains[STATUS_DOMAIN],
                owner,
                11,
                0,
                &[((index % 2) + 1) as i32],
            );
        }
        for (order, &movement) in enemy.move_history.iter().enumerate() {
            let mut row = DomainRow::new(HISTORY_DOMAIN, STATE_SCOPE);
            row.u[..5].copy_from_slice(&[
                index as u32 + 3,
                1,
                order as u32 + 1,
                movement as u32,
                enemy.creature.id as u32 + 1,
            ]);
            domains[HISTORY_DOMAIN].push(row);
        }
    }
    let history = combat.history;
    let mut row = DomainRow::new(HISTORY_DOMAIN, STATE_SCOPE);
    row.u[0] = 1;
    row.s.copy_from_slice(&[
        history.cards as i32,
        history.manual_cards as i32,
        history.manual_plays as i32,
        history.attacks as i32,
        history.skills as i32,
        history.powers as i32,
        history.energy as i32,
        history.exhausted as i32,
        history.discarded as i32,
        history.shivs as i32,
        history.stars_gained as i32,
        history.generated as i32,
        history.ethereal as i32,
        history.extra_drawn as i32,
        history.doom_applied as i32,
        history.osty_attacks as i32,
        history.block_gains as i32,
        history.block_card as i32,
        history.block_card_gains as i32,
        history.hp_lost as i32,
        history.hp_loss_events as i32,
        history.feral_returns as i32,
        combat.last_cards as i32,
        combat.orbit_spent as i32,
        combat.lightning_channeled as i32,
        combat.orbs_channeled as i32,
        combat.poisoned as i32,
        combat.last_damage as i32,
        combat.drawn as i32,
    ]);
    domains[HISTORY_DOMAIN].push(row);
    if game.damage_taken {
        push_status_domain(&mut domains[STATUS_DOMAIN], 1, 0, 0, &[]);
    }
    for (order, &(card, count)) in combat.nightmares.iter().enumerate() {
        domains[STATUS_DOMAIN].push(card_status_domain_row(
            game,
            content,
            encoding,
            1,
            order,
            card,
            count as i32,
        ));
    }
    if let Some(card) = combat.history_course {
        domains[STATUS_DOMAIN].push(card_status_domain_row(
            game,
            content,
            encoding,
            HISTORY_COURSE_STATUS,
            0,
            card,
            1,
        ));
    }
    for (order, &(turns, amount)) in combat.bombs.iter().enumerate() {
        push_status_domain(
            &mut domains[STATUS_DOMAIN],
            1,
            2,
            order,
            &[turns as i32, amount as i32],
        );
    }
    for (order, &(turns, energy)) in combat.automation.iter().enumerate() {
        push_status_domain(
            &mut domains[STATUS_DOMAIN],
            1,
            3,
            order,
            &[turns as i32, energy as i32],
        );
    }
    for (order, &(cards, damage, start)) in combat.panache.iter().enumerate() {
        push_status_domain(
            &mut domains[STATUS_DOMAIN],
            1,
            4,
            order,
            &[cards as i32, damage as i32, start as i32],
        );
    }
    for (order, &amount) in combat.boulders.iter().enumerate() {
        push_status_domain(&mut domains[STATUS_DOMAIN], 1, 5, order, &[amount as i32]);
    }
}

fn card_status_domain_row(
    game: &Game,
    content: &Content,
    encoding: &CardEncoding,
    kind: u32,
    order: usize,
    card: Card,
    count: i32,
) -> DomainRow {
    let def = content.cards[card.id as usize];
    let combat = game.combat().unwrap();
    let (master, dampened) =
        card_relation(card, &game.run.deck, &encoding.masters, &combat.dampened);
    let mut row = DomainRow::new(STATUS_DOMAIN, STATE_SCOPE);
    row.u.copy_from_slice(&[
        1,
        kind,
        order as u32 + 1,
        card.id as u32,
        card.upgrades as u32,
        card.flags as u32,
        card.turn_flags as u32,
        card.replays as u32,
        card.free as u32,
        card.cost_override.is_some() as u32,
        card.enchantment.map_or(0, |value| value as u32 + 1),
        card.variant as u32,
        card.card_type(def) as u32,
        def.rarity as u32,
        target_index(card.target(def)),
        card.flags(def) as u32,
        def.tags as u32,
        master as u32,
        dampened as u32,
        0,
    ]);
    row.s[..6].copy_from_slice(&[
        count,
        card.cost_delta as i32,
        card.value as i32,
        card.cost_override.unwrap_or_default() as i32,
        card.enchantment_amount as i32,
        card.enchantment_value as i32,
    ]);
    row
}

fn push_power_domains(
    content: &Content,
    out: &mut Vec<DomainRow>,
    owner: u32,
    snapshot: bool,
    powers: &[Power],
) {
    for (order, power) in powers.iter().enumerate() {
        let name = content.powers[power.id as usize].id;
        let mut row = DomainRow::new(POWER_DOMAIN, STATE_SCOPE);
        row.u[..7].copy_from_slice(&[
            owner,
            power.id as u32,
            snapshot as u32,
            power.skip_next_decay as u32,
            (name == "POWER.SURROUNDED_POWER") as u32 * (power.value > 0) as u32,
            if name == "POWER.CONSTRICT_POWER" {
                power.value.max(0) as u32 + 1
            } else {
                0
            },
            order as u32 + 1,
        ]);
        row.s[0] = power.amount as i32;
        row.s[1] = power.value as i32;
        out.push(row);
    }
}

fn push_status_domain(
    out: &mut Vec<DomainRow>,
    owner: u32,
    kind: u32,
    order: usize,
    values: &[i32],
) {
    let mut row = DomainRow::new(STATUS_DOMAIN, STATE_SCOPE);
    row.u[..3].copy_from_slice(&[owner, kind, order as u32 + 1]);
    row.s[..values.len()].copy_from_slice(values);
    out.push(row);
}

fn public_encounter_pool<'a>(game: &Game, content: &'a Content, elite: bool) -> Vec<Id> {
    let act = &content.acts[game.act as usize];
    if elite {
        return act.elites.to_vec();
    }
    act.encounters
        .iter()
        .copied()
        .filter(|&id| {
            let weak = content.encounters[id as usize].id.contains("_WEAK");
            if game.weak_encounters_left > 0 {
                weak
            } else if game.regular_encounters_left > 0 {
                !weak
            } else {
                true
            }
        })
        .collect()
}

fn push_collection_domains(
    game: &Game,
    content: &Content,
    layout: Layout,
    domains: &mut [Vec<DomainRow>; 16],
) {
    for (order, &id) in game.run.relics.iter().enumerate() {
        let mut row = DomainRow::new(RELIC_DOMAIN, STATE_SCOPE);
        row.u[..8].copy_from_slice(&[
            0,
            id as u32,
            order as u32 + 1,
            crate::game::relic_group(id).map_or(4, |rarity| rarity as u32),
            0,
            game.melted_relics.contains(&order) as u32,
            game.wax_relics
                .iter()
                .filter(|wax| !game.melted_relics.contains(wax))
                .position(|&wax| wax == order)
                .map_or(0, |position| position as u32 + 1),
            order as u32 + 1,
        ]);
        if content.relics[id as usize].id == "RELIC.UNSETTLING_LAMP" {
            row.u[8] = game
                .combat()
                .and_then(|combat| combat.unsettling_lamp)
                .map_or(0, |id| id as u32 + 1);
        }
        row.u[9] = (content.relics[id as usize].id == "RELIC.PAELS_TOOTH") as u32;
        row.s
            .copy_from_slice(&relic_state(game, content, id).map(|value| value.round() as i32));
        domains[RELIC_DOMAIN].push(row);
    }
    let bags = public_relic_bags(game, content, layout);
    for (pool, bag) in bags.iter().enumerate() {
        for &id in bag.iter().flatten() {
            let mut row = DomainRow::new(RELIC_DOMAIN, STATE_SCOPE);
            row.u[..4].copy_from_slice(&[
                pool as u32 + 1,
                id as u32,
                0,
                crate::game::relic_group(id).unwrap_or(4) as u32,
            ]);
            domains[RELIC_DOMAIN].push(row);
        }
    }
    let mut pool = content.characters[game.run.character as usize]
        .relic_pool
        .iter()
        .chain(layout.shared_relics)
        .copied()
        .collect::<Vec<_>>();
    pool.sort_unstable();
    pool.dedup();
    for id in pool {
        if game.run.relics.contains(&id) {
            continue;
        }
        let general = bags[0].iter().flatten().any(|&candidate| candidate == id);
        let shared = layout.shared_relics.contains(&id);
        let shared_left = bags[1].iter().flatten().any(|&candidate| candidate == id);
        let excluded = (!general) as u32 | ((shared && !shared_left) as u32) << 1;
        if excluded != 0 {
            let mut row = DomainRow::new(RELIC_DOMAIN, STATE_SCOPE);
            row.u[..5].copy_from_slice(&[3, id as u32, 0, 4, excluded]);
            domains[RELIC_DOMAIN].push(row);
        }
    }
    for (slot, potion) in game.run.potions.iter().enumerate() {
        let mut row = DomainRow::new(POTION_DOMAIN, STATE_SCOPE);
        row.u[..5].copy_from_slice(&[
            0,
            slot as u32,
            potion.map_or(0, |id| id as u32 + 1),
            potion.is_none() as u32,
            slot as u32 + 1,
        ]);
        domains[POTION_DOMAIN].push(row);
    }
    if let Some(id) = game.pending_potion {
        let mut row = DomainRow::new(POTION_DOMAIN, PHASE_SCOPE);
        row.u[..3].copy_from_slice(&[1, u32::MAX, id as u32 + 1]);
        domains[POTION_DOMAIN].push(row);
    }
    if let Some(combat) = game.combat() {
        for slot in 0..combat.orb_slots as usize {
            let orb = combat.orbs.get(slot);
            let mut row = DomainRow::new(ORB_DOMAIN, STATE_SCOPE);
            row.u[..4].copy_from_slice(&[
                slot as u32,
                orb.map_or(0, |orb| orb.id as u32 + 1),
                orb.is_none() as u32,
                orb.map_or(0, |orb| content.orbs[orb.id as usize].timing as u32 + 1),
            ]);
            row.s[0] = orb.map_or(0, |orb| orb.value as i32);
            domains[ORB_DOMAIN].push(row);
        }
    }
    for &id in content.acts[game.act as usize]
        .events
        .iter()
        .filter(|&&id| !game.visited_events.contains(&id) && game.event_allowed(content, id))
    {
        let mut row = DomainRow::new(EVENT_DOMAIN, STATE_SCOPE);
        row.u[..2].copy_from_slice(&[0, id as u32]);
        domains[EVENT_DOMAIN].push(row);
    }
    let encounter_vocabs = encounter_vocabs(content);
    if game.bosses_visited < 2
        && let Some(id) = game.bosses[game.bosses_visited as usize]
    {
        let mut row = DomainRow::new(ENCOUNTER_DOMAIN, STATE_SCOPE);
        row.u[..2].copy_from_slice(&[0, encounter_id(&encounter_vocabs[2], id) as u32 - 1]);
        domains[ENCOUNTER_DOMAIN].push(row);
    }
    for (kind, (items, vocab)) in [
        (
            1,
            (
                public_encounters(game, content, false),
                &encounter_vocabs[0],
            ),
        ),
        (
            2,
            (public_encounters(game, content, true), &encounter_vocabs[1]),
        ),
    ] {
        for id in items {
            let mut row = DomainRow::new(ENCOUNTER_DOMAIN, STATE_SCOPE);
            row.u[..2].copy_from_slice(&[kind, encounter_id(vocab, id) as u32 - 1]);
            domains[ENCOUNTER_DOMAIN].push(row);
        }
    }
    for (kind, id, vocab) in [
        (3, game.last_encounter, &encounter_vocabs[0]),
        (4, game.last_elite, &encounter_vocabs[1]),
    ] {
        if let Some(id) = id {
            let mut row = DomainRow::new(ENCOUNTER_DOMAIN, STATE_SCOPE);
            row.u[..2].copy_from_slice(&[kind, encounter_id(vocab, id) as u32 - 1]);
            domains[ENCOUNTER_DOMAIN].push(row);
        }
    }
    if let Some(crystal) = &game.crystal {
        for &item in &crystal.revealed {
            if let Some(&(x, y, width, height, kind)) = crystal.items.get(item) {
                let mut row = DomainRow::new(CRYSTAL_DOMAIN, STATE_SCOPE);
                row.u[..6].copy_from_slice(&[
                    0,
                    x as u32,
                    y as u32,
                    kind as u32 + 1,
                    width as u32,
                    height as u32,
                ]);
                domains[CRYSTAL_DOMAIN].push(row);
            }
        }
        for (cell, &clear) in crystal.clear.iter().enumerate() {
            if !clear {
                continue;
            }
            let mut row = DomainRow::new(CRYSTAL_DOMAIN, STATE_SCOPE);
            row.u[..3].copy_from_slice(&[1, (cell / 11) as u32, (cell % 11) as u32]);
            domains[CRYSTAL_DOMAIN].push(row);
        }
    }
}

fn scale_payload(scale: Scale) -> (u32, u32, i32) {
    match scale {
        Scale::None => (0, 0, 0),
        Scale::X => (1, 0, 0),
        Scale::Block => (2, 0, 0),
        Scale::Energy => (3, 0, 0),
        Scale::MissingHp => (4, 0, 0),
        Scale::DrawSize => (5, 0, 0),
        Scale::DiscardSize => (6, 0, 0),
        Scale::ExhaustSize => (7, 0, 0),
        Scale::ExhaustId(id) => (8, id as u32, 0),
        Scale::HandSize => (9, 0, 0),
        Scale::CardsPlayed => (10, 0, 0),
        Scale::AttacksPlayed => (11, 0, 0),
        Scale::PriorAttacks => (12, 0, 0),
        Scale::SkillsPlayed => (13, 0, 0),
        Scale::EnergySpent => (14, 0, 0),
        Scale::Exhausted => (15, 0, 0),
        Scale::HpLost => (16, 0, 0),
        Scale::HpLossEvents => (17, 0, 0),
        Scale::CardValue => (18, 0, 0),
        Scale::LivingEnemies => (19, 0, 0),
        Scale::Orbs => (20, 0, 0),
        Scale::OrbTypes => (21, 0, 0),
        Scale::OrbTypesPower(id) => (22, id as u32, 0),
        Scale::Stars => (23, 0, 0),
        Scale::TargetPower(id) => (24, id as u32, 0),
        Scale::TargetPowerDiv(id, divisor) => (25, id as u32, divisor as i32),
        Scale::Tagged(tag) => (26, tag as u32, 0),
        Scale::Power(id) => (27, id as u32, 0),
        Scale::Event => (28, 0, 0),
        Scale::EventPower(id) => (29, id as u32, 0),
        Scale::HandType(kind) => (30, kind as u32, 0),
        Scale::EnemyPowerTotal(id) => (31, id as u32, 0),
        Scale::TargetDebuffs => (32, 0, 0),
        Scale::Discarded => (33, 0, 0),
        Scale::DrawnCombat => (34, 0, 0),
        Scale::StarCards => (35, 0, 0),
        Scale::StarsGained => (36, 0, 0),
        Scale::Generated => (37, 0, 0),
        Scale::PriorTargetHits => (38, 0, 0),
        Scale::OstyHp => (39, 0, 0),
        Scale::OstyMaxHp => (40, 0, 0),
        Scale::OstyAttacks => (41, 0, 0),
        Scale::EtherealPlayed => (42, 0, 0),
        Scale::LightningChanneled => (43, 0, 0),
        Scale::LastDamage => (44, 0, 0),
        Scale::ExtraDrawn => (45, 0, 0),
        Scale::TurnDiv(divisor) => (46, 0, divisor as i32),
    }
}

fn put_amount(row: &mut DomainRow, slot: usize, amount: Amount) {
    let (kind, id, extra) = scale_payload(amount.scale);
    let u = 11 + slot * 3;
    let s = slot * 5;
    put_amount_at(row, u, s, kind, id, extra, amount);
}

fn put_amount_at(
    row: &mut DomainRow,
    u: usize,
    s: usize,
    kind: u32,
    id: u32,
    extra: i32,
    amount: Amount,
) {
    row.u[u..u + 3].copy_from_slice(&[kind, id, amount.ascension as u32]);
    row.s[s..s + 5].copy_from_slice(&[
        amount.base as i32,
        amount.upgraded as i32,
        amount.multiplier as i32,
        amount.divisor as i32,
        extra,
    ]);
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
        | Summon(amount) => vec![(amount, 10.0)],
        _ => vec![],
    }
}

fn put_evaluated_amounts(
    row: &mut DomainRow,
    game: &Game,
    content: &Content,
    context: Context,
    effect: Effect,
) {
    for (slot, (amount, scale)) in effect_amounts(effect).into_iter().enumerate() {
        let base = if context.upgraded
            || matches!(context.source, Actor::Enemy(_))
                && amount.ascension > 0
                && game.run.ascension >= amount.ascension
        {
            amount.upgraded
        } else {
            amount.base
        };
        let offset = 16 + slot * 4;
        row.f[offset..offset + 4].copy_from_slice(&[
            base as f32 / scale,
            amount.multiplier as f32 / 10.0,
            amount.divisor as f32 / 10.0,
            game.amount(content, context, amount) as f32 / scale,
        ]);
    }
}

fn condition_payload(condition: Condition) -> (u32, u32, i32, i32) {
    match condition {
        Condition::Always => (0, 0, 0, 0),
        Condition::Upgraded => (1, 0, 0, 0),
        Condition::TargetAlive => (2, 0, 0, 0),
        Condition::TargetHasPower(id) => (3, id as u32, 0, 0),
        Condition::SourceHasPower(id) => (4, id as u32, 0, 0),
        Condition::HandAtMost(value) => (5, value as u32, 0, 0),
        Condition::HandAtLeast(value) => (6, value as u32, 0, 0),
        Condition::HandWithout(kind) => (7, kind as u32, 0, 0),
        Condition::HandEmpty => (8, 0, 0, 0),
        Condition::EventAtLeast(value) => (9, 0, value as i32, 0),
        Condition::ExhaustAtLeast(value) => (10, value as u32, 0, 0),
        Condition::ExhaustedThisTurn => (11, 0, 0, 0),
        Condition::HpLostThisTurn => (12, 0, 0, 0),
        Condition::TargetDead => (13, 0, 0, 0),
        Condition::FirstTurn => (14, 0, 0, 0),
        Condition::HasOrb(id) => (15, id as u32, 0, 0),
        Condition::CardType(kind) => (16, kind as u32, 0, 0),
        Condition::PriorCardsBelow(values) => (17, values[0] as u32, values[1] as i32, 0),
        Condition::TargetAttacking => (18, 0, 0, 0),
        Condition::XAtLeast(value) => (19, 0, value as i32, 0),
        Condition::OstyAlive => (20, 0, 0, 0),
        Condition::DoomApplied => (21, 0, 0, 0),
        Condition::Card(id) => (22, id as u32, 0, 0),
    }
}

fn filter_payload(filter: CardFilter) -> (u32, u32, i32) {
    match filter {
        CardFilter::Any => (0, 0, 0),
        CardFilter::Type(kind) => (1, kind as u32, 0),
        CardFilter::AttackOrPower => (2, 0, 0),
        CardFilter::TypeWithoutTurnFlag(kind, flag) => (3, kind as u32, flag as i32),
        CardFilter::WithoutFlag(flag) => (4, flag as u32, 0),
        CardFilter::NotType(kind) => (5, kind as u32, 0),
        CardFilter::Id(id) => (6, id as u32, 0),
        CardFilter::Cost(cost) => (7, 0, cost as i32),
        CardFilter::PlayableCost(cost) => (8, 0, cost as i32),
        CardFilter::Flag(flag) => (9, flag as u32, 0),
        CardFilter::Upgradable => (10, 0, 0),
        CardFilter::Colorless => (11, 0, 0),
        CardFilter::Rare => (12, 0, 0),
        CardFilter::NoReplay => (13, 0, 0),
        CardFilter::PlayableOrAny => (14, 0, 0),
        CardFilter::CostsResource => (15, 0, 0),
    }
}

fn op_payload(op: CardOp) -> (u32, u32, u32, i32) {
    match op {
        CardOp::Move(pile) => (0, pile as u32, 0, 0),
        CardOp::Upgrade => (1, 0, 0, 0),
        CardOp::Cost(cost) => (2, 0, 0, cost as i32),
        CardOp::SetCost(cost) => (3, 0, 0, cost as i32),
        CardOp::Flag(flag) => (4, flag as u32, 0, 0),
        CardOp::TurnFlag(flag) => (5, flag as u32, 0, 0),
        CardOp::CopyNextTurn(count) => (6, count as u32, 0, 0),
        CardOp::Transform(id, upgrades) => (7, id as u32, upgrades as u32, 0),
        CardOp::AutoPlay(count) => (8, count as u32, 0, 0),
        CardOp::CopySelected(count) => (9, count as u32, 0, 0),
        CardOp::TakeOffer => (10, 0, 0, 0),
        CardOp::Transfigure => (11, 0, 0, 0),
        CardOp::Replay(count) => (12, count as u32, 0, 0),
        CardOp::TakeFetched => (13, 0, 0, 0),
        CardOp::TransformRandom => (14, 0, 0, 0),
        CardOp::DiscardDraw => (15, 0, 0, 0),
        CardOp::MoveFree(pile) => (16, pile as u32, 0, 0),
        CardOp::FreeCombat => (17, 0, 0, 0),
    }
}

struct ContinuationBuilder {
    rows: Vec<DomainRow>,
    next: u32,
    scope: i32,
    card_relations: HashMap<u32, (u32, u32)>,
}

impl ContinuationBuilder {
    fn new(scope: i32) -> Self {
        Self {
            rows: Vec::new(),
            next: 0,
            scope,
            card_relations: HashMap::new(),
        }
    }

    fn for_game(game: &Game, scope: i32) -> Self {
        let masters = master_cards(&game.run.deck);
        let dampened = game
            .combat()
            .map_or(&[][..], |combat| combat.dampened.as_slice());
        let card_relations = game
            .run
            .deck
            .iter()
            .filter(|card| card.instance != 0)
            .map(|&card| {
                let (master, upgrades) = card_relation(card, &game.run.deck, &masters, dampened);
                (card.instance, (master as u32, upgrades as u32))
            })
            .collect();
        Self {
            card_relations,
            ..Self::new(scope)
        }
    }

    fn row(
        &mut self,
        kind: u32,
        variant: u32,
        parent: u32,
        branch: u32,
        path: u32,
        list: u32,
        order: u32,
        arity: u32,
    ) -> usize {
        let frame = self.next;
        self.next += 1;
        let mut row = DomainRow::new(CONTINUATION_DOMAIN, self.scope);
        row.u[..11].copy_from_slice(&[
            kind,
            variant,
            frame,
            parent,
            branch,
            path,
            0,
            list,
            order,
            arity,
            (arity == 0) as u32,
        ]);
        self.rows.push(row);
        self.rows.len() - 1
    }

    fn effect(
        &mut self,
        effect: Effect,
        parent: u32,
        branch: u32,
        path: u32,
        list: u32,
        order: u32,
        evaluation: Option<(&Game, &Content, Context)>,
    ) {
        if let Some((game, content, context)) = evaluation {
            match effect {
                Effect::If(condition, yes, no) => {
                    let selected = if game.condition(content, context, condition) {
                        yes
                    } else {
                        no
                    };
                    for (order, &child) in selected.iter().enumerate() {
                        self.effect(child, parent, branch, path, list, order as u32, evaluation);
                    }
                    return;
                }
                Effect::Repeat(amount, effects) => {
                    let repeats = game.amount(content, context, amount).max(0) as usize;
                    for (order, child) in effects
                        .iter()
                        .cycle()
                        .take(repeats * effects.len())
                        .enumerate()
                    {
                        self.effect(*child, parent, branch, path, list, order as u32, evaluation);
                    }
                    return;
                }
                _ => {}
            }
        }
        let children = match effect {
            Effect::If(_, yes, no) => yes.len() + no.len(),
            Effect::Repeat(_, effects) | Effect::Random(_, effects) => effects.len(),
            Effect::AutoPlay(_) => 1,
            _ => 0,
        };
        let index = self.row(
            1,
            effect_index(&effect) as u32,
            parent,
            branch,
            path,
            list,
            order,
            children as u32,
        );
        let frame = self.rows[index].u[2];
        let row = &mut self.rows[index];
        macro_rules! amount {
            ($slot:expr, $value:expr) => {
                put_amount(row, $slot, $value)
            };
        }
        macro_rules! target_amount {
            ($target:expr, $value:expr) => {{
                row.u[11] = target_index($target);
                let (kind, id, extra) = scale_payload($value.scale);
                put_amount_at(row, 12, 0, kind, id, extra, $value);
            }};
        }
        match effect {
            Effect::Attack(target, value, count) | Effect::OstyAttack(target, value, count) => {
                target_amount!(target, value);
                row.u[15] = count as u32;
            }
            Effect::AttackMany(target, first, second)
            | Effect::OstyAttackMany(target, first, second) => {
                row.u[11] = target_index(target);
                let (kind, id, extra) = scale_payload(first.scale);
                put_amount_at(row, 12, 0, kind, id, extra, first);
                let (kind, id, extra) = scale_payload(second.scale);
                put_amount_at(row, 15, 5, kind, id, extra, second);
            }
            Effect::MoveDamage(target, value)
            | Effect::Damage(target, value)
            | Effect::LoseHp(target, value)
            | Effect::Block(target, value)
            | Effect::RawBlock(target, value)
            | Effect::Heal(target, value)
            | Effect::HealPercent(target, value)
            | Effect::ExhaustForAttack(target, value) => target_amount!(target, value),
            Effect::Kill(target) | Effect::Stun(target) => row.u[11] = target_index(target),
            Effect::DoomKill
            | Effect::RandomPotion
            | Effect::FreeHand
            | Effect::DoubleEnergy
            | Effect::FillPotions
            | Effect::RandomizeHandCosts
            | Effect::FranticEscape
            | Effect::Stoke
            | Effect::Stampede
            | Effect::ContinueEndTurn
            | Effect::DiscardHandDraw
            | Effect::PlayExhaustedShivs
            | Effect::FinishAutoPlay
            | Effect::PassiveAll
            | Effect::CapHandCosts
            | Effect::EndTurn => {}
            Effect::BlockNextTurn(value)
            | Effect::DodgeRoll(value)
            | Effect::MaxHp(value)
            | Effect::Gold(value)
            | Effect::Bomb(value)
            | Effect::Panache(value)
            | Effect::RollingBoulder(value)
            | Effect::ToricToughness(value)
            | Effect::Misery(value)
            | Effect::DrawAmount(value)
            | Effect::ChooseDraw(value)
            | Effect::Aggression(value)
            | Effect::FlakCannon(value)
            | Effect::EvokeMany(value)
            | Effect::EvokeLast(value)
            | Effect::Stars(value)
            | Effect::Forge(value)
            | Effect::Summon(value)
            | Effect::GrowCard(value)
            | Effect::GrowDrawn(value)
            | Effect::PersistCard(value)
            | Effect::ExhaustForBlock(value) => amount!(0, value),
            Effect::DrawBlockIf(kind, value) => {
                row.u[11] = kind as u32;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 12, 0, kind, id, extra, value);
            }
            Effect::Automation(value) => row.s[0] = value as i32,
            Effect::ApplyPower(target, id, value)
            | Effect::ApplyDebuff(target, id, value)
            | Effect::StackPower(target, id, value)
            | Effect::TemporaryStrength(target, id, value) => {
                row.u[11] = target_index(target);
                row.u[12] = id as u32;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 13, 0, kind, id, extra, value);
            }
            Effect::DoublePower(target, id, value) => {
                row.u[11] = target_index(target);
                row.u[12] = id as u32;
                row.s[0] = value as i32;
            }
            Effect::RemovePower(target, id) => {
                row.u[11] = target_index(target);
                row.u[12] = id as u32;
            }
            Effect::Draw(count)
            | Effect::HandDraw(count)
            | Effect::ChooseRandomDraw(count)
            | Effect::ShuffleHandDraw(count)
            | Effect::EvokeAll(count)
            | Effect::PassiveFirst(count)
            | Effect::PassiveLast(count)
            | Effect::WhisperingEarring(count) => row.u[11] = count as u32,
            Effect::DrawTo(values)
            | Effect::RandomOrb(values)
            | Effect::TransformHand(_, values)
            | Effect::RecycleHand(values) => {
                row.u[11] = values[0] as u32;
                row.u[12] = values[1] as u32;
                if let Effect::TransformHand(id, _) = effect {
                    row.u[13] = id as u32;
                }
            }
            Effect::DrawUntilNot(kind) => row.u[11] = kind as u32,
            Effect::Energy(value)
            | Effect::OrbSlots(value)
            | Effect::MaxEnergy(value)
            | Effect::SetCardCost(value)
            | Effect::ReduceCardCost(value) => row.s[0] = value as i32,
            Effect::AddCard(pile, id, count)
            | Effect::AddUpgradedCard(pile, id, count)
            | Effect::AddRandom(pile, id, count) => {
                row.u[11..14].copy_from_slice(&[pile as u32, id as u32, count as u32]);
            }
            Effect::AddFlaggedCard(pile, id, count, flags) => {
                row.u[11..15].copy_from_slice(&[
                    pile as u32,
                    id as u32,
                    count as u32,
                    flags as u32,
                ]);
            }
            Effect::RandomCard(pile, kind, count, flag) => {
                row.u[11..15].copy_from_slice(&[
                    pile as u32,
                    kind as u32,
                    count as u32,
                    flag as u32,
                ]);
            }
            Effect::RandomColorless(pile, value, flag)
            | Effect::RandomCharacterCost0(pile, value, flag) => {
                row.u[11] = pile as u32;
                row.u[12] = flag as u32;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 13, 0, kind, id, extra, value);
            }
            Effect::RandomColorlessOther(pile, value) => {
                row.u[11] = pile as u32;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 12, 0, kind, id, extra, value);
            }
            Effect::OfferColorless(count, first, second) => {
                row.u[11..14].copy_from_slice(&[count as u32, first as u32, second as u32]);
            }
            Effect::OfferOtherCharacter(kind, count, flag) => {
                row.u[11..14].copy_from_slice(&[kind as u32, count as u32, flag as u32]);
            }
            Effect::OfferCharacter(count, flag) => {
                row.u[11..13].copy_from_slice(&[count as u32, flag as u32]);
            }
            Effect::OfferCharacterRetain(count) => row.u[11] = count as u32,
            Effect::OfferCharacterType(kind, count) => {
                row.u[11..13].copy_from_slice(&[kind as u32, count as u32]);
            }
            Effect::RandomCharacter(pile, value, flags) => {
                row.u[11] = pile as u32;
                row.u[12] = flags as u32;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 13, 0, kind, id, extra, value);
            }
            Effect::DistinctCharacter(pile, count, flag)
            | Effect::DistinctColorless(pile, count, flag) => {
                row.u[11..14].copy_from_slice(&[pile as u32, count as u32, flag as u32]);
            }
            Effect::RandomCardOp(pile, filter, op, value) => {
                let (filter, filter_arg, filter_signed) = filter_payload(filter);
                let (op, op_arg, op_arg2, op_signed) = op_payload(op);
                row.u[11..16].copy_from_slice(&[pile as u32, filter, filter_arg, op, op_arg]);
                row.u[16] = op_arg2;
                row.s[0] = filter_signed;
                row.s[1] = op_signed;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 17, 5, kind, id, extra, value);
            }
            Effect::AutoPlayRandom(pile, filter, value)
            | Effect::SelectAmount(pile, filter, value, _, _) => {
                let (filter, filter_arg, filter_signed) = filter_payload(filter);
                row.u[11..14].copy_from_slice(&[pile as u32, filter, filter_arg]);
                row.s[0] = filter_signed;
                if let Effect::SelectAmount(_, _, _, optional, op) = effect {
                    let (op, arg, arg2, signed) = op_payload(op);
                    row.u[14..18].copy_from_slice(&[optional as u32, op, arg, arg2]);
                    row.s[1] = signed;
                }
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 18, 5, kind, id, extra, value);
            }
            Effect::ReplayTagged(tag, count) => {
                row.u[11..13].copy_from_slice(&[tag as u32, count as u32]);
            }
            Effect::ChannelSlots(id) => row.u[11] = id as u32,
            Effect::AddRandomCard(id, values, flag) => {
                row.u[11..15].copy_from_slice(&[
                    id as u32,
                    values[0] as u32,
                    values[1] as u32,
                    flag as u32,
                ]);
            }
            Effect::AutoPlayDraw(value, flag) => {
                row.u[11] = flag as u32;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 12, 0, kind, id, extra, value);
            }
            Effect::CopyCard(pile, count) => {
                row.u[11..13].copy_from_slice(&[pile as u32, count as u32]);
            }
            Effect::Discard(count, flag) | Effect::Exhaust(count, flag) => {
                row.u[11..13].copy_from_slice(&[count as u32, flag as u32]);
            }
            Effect::Upgrade(pile, count, flag) => {
                row.u[11..14].copy_from_slice(&[pile as u32, count as u32, flag as u32]);
            }
            Effect::DiscardHandAdd(id) => row.u[11] = id as u32,
            Effect::MoveAll(from, filter, to) => {
                let (filter, arg, signed) = filter_payload(filter);
                row.u[11..15].copy_from_slice(&[from as u32, filter, arg, to as u32]);
                row.s[0] = signed;
            }
            Effect::DrawFiltered(count, filter) | Effect::DrawFilteredStep(count, filter) => {
                let (filter, arg, signed) = filter_payload(filter);
                row.u[11..14].copy_from_slice(&[count as u32, filter, arg]);
                row.s[0] = signed;
            }
            Effect::FinishDrawFiltered(filter) => {
                let (filter, arg, signed) = filter_payload(filter);
                row.u[11..13].copy_from_slice(&[filter, arg]);
                row.s[0] = signed;
            }
            Effect::ExhaustAttackStep(target, value, count) => {
                row.u[11] = target_index(target);
                row.u[12] = count as u32;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 13, 0, kind, id, extra, value);
            }
            Effect::Channel(id, count) => {
                row.u[11..13].copy_from_slice(&[id as u32, count as u32]);
            }
            Effect::Evoke(flag) => row.u[11] = flag as u32,
            Effect::GrowAll(id, value) => {
                row.u[11] = id as u32;
                let (kind, id, extra) = scale_payload(value.scale);
                put_amount_at(row, 12, 0, kind, id, extra, value);
            }
            Effect::If(condition, yes, no) => {
                let (kind, arg, first, second) = condition_payload(condition);
                row.u[11..13].copy_from_slice(&[kind, arg]);
                row.s[..2].copy_from_slice(&[first, second]);
                for (order, &child) in yes.iter().enumerate() {
                    self.effect(child, frame, 1, path + 1, 0, order as u32, evaluation);
                }
                for (order, &child) in no.iter().enumerate() {
                    self.effect(child, frame, 2, path + 1, 0, order as u32, evaluation);
                }
            }
            Effect::Repeat(value, effects) => {
                put_amount(&mut self.rows[index], 0, value);
                for (order, &child) in effects.iter().enumerate() {
                    self.effect(child, frame, 1, path + 1, 0, order as u32, evaluation);
                }
            }
            Effect::Random(count, effects) => {
                self.rows[index].u[11] = count as u32;
                for (order, &child) in effects.iter().enumerate() {
                    self.effect(child, frame, 1, path + 1, 0, order as u32, evaluation);
                }
            }
            Effect::AutoPlay(card) => self.card(card, frame, path + 1, 0),
            Effect::Select(pile, filter, values, first, second, op) => {
                let (filter, arg, signed) = filter_payload(filter);
                let (op, op_arg, op_arg2, op_signed) = op_payload(op);
                row.u[11..20].copy_from_slice(&[
                    pile as u32,
                    filter,
                    arg,
                    values[0] as u32,
                    values[1] as u32,
                    first as u32,
                    second as u32,
                    op,
                    op_arg,
                ]);
                row.u[20] = op_arg2;
                row.s[..2].copy_from_slice(&[signed, op_signed]);
            }
        }
        if let Some((game, content, context)) = evaluation {
            put_evaluated_amounts(&mut self.rows[index], game, content, context, effect);
        }
    }

    fn card(&mut self, card: Card, parent: u32, path: u32, order: u32) {
        let relation = self
            .card_relations
            .get(&card.instance)
            .copied()
            .unwrap_or_default();
        let index = self.row(5, 0, parent, 0, path, 0, order, 0);
        let row = &mut self.rows[index];
        row.u[11..23].copy_from_slice(&[
            card.id as u32,
            card.upgrades as u32,
            card.flags as u32,
            card.turn_flags as u32,
            card.replays as u32,
            card.free as u32,
            card.cost_override.is_some() as u32,
            card.enchantment.map_or(0, |value| value as u32 + 1),
            card.variant as u32,
            relation.0,
            relation.1,
            0,
        ]);
        row.s[..5].copy_from_slice(&[
            card.cost_delta as i32,
            card.value as i32,
            card.cost_override.unwrap_or_default() as i32,
            card.enchantment_amount as i32,
            card.enchantment_value as i32,
        ]);
    }

    fn card_reward(&mut self, reward: CardReward, parent: u32, path: u32, order: u32) {
        let child = self.row(8, 2, parent, 1, path, 0, order, 0);
        match reward {
            CardReward::Standard(room) => {
                self.rows[child].u[11..13].copy_from_slice(&[0, room_index(room) as u32]);
            }
            CardReward::Fixed(id, rarity) => {
                self.rows[child].u[11..14].copy_from_slice(&[1, id as u32, rarity as u32]);
            }
            CardReward::Kaleidoscope => self.rows[child].u[11] = 2,
            CardReward::Crystal(rarity) => {
                self.rows[child].u[11..13].copy_from_slice(&[3, rarity as u32]);
            }
        }
    }

    fn run_effect(
        &mut self,
        effect: RunEffect,
        content: &Content,
        parent: u32,
        branch: u32,
        path: u32,
        list: u32,
        order: u32,
    ) {
        let children = match effect {
            RunEffect::RandomCard(values) | RunEffect::RandomRelic(values) => values.len(),
            RunEffect::Options(options) => options.len(),
            _ => 0,
        };
        let index = self.row(
            0,
            event_effect_index(&effect) as u32,
            parent,
            branch,
            path,
            list,
            order,
            children as u32,
        );
        let frame = self.rows[index].u[2];
        let row = &mut self.rows[index];
        let card_id = |name| content.card_id(name).expect("unknown continuation card") as u32;
        let relic_id = |name| content.relic_id(name).expect("unknown continuation relic") as u32;
        let potion_id = |name| {
            content
                .potion_id(name)
                .expect("unknown continuation potion") as u32
        };
        match effect {
            RunEffect::Gold(value) => row.s[0] = value,
            RunEffect::Heal(value)
            | RunEffect::LoseHp(value)
            | RunEffect::MaxHp(value)
            | RunEffect::MaxHpTo(value) => row.s[0] = value as i32,
            RunEffect::RandomGold(min, max) => row.s[..2].copy_from_slice(&[min, max]),
            RunEffect::LoseAllGold
            | RunEffect::FullHeal
            | RunEffect::EventRelic
            | RunEffect::DiscardRandomPotion
            | RunEffect::UpgradeAll
            | RunEffect::CloneDeck
            | RunEffect::RemoveRandomCard => {}
            RunEffect::HealPercent(value)
            | RunEffect::NextRelics(value)
            | RunEffect::RelicOfRarity(value)
            | RunEffect::DiscardPotion(value)
            | RunEffect::UpgradeCards(value)
            | RunEffect::UpgradeRandom(value)
            | RunEffect::UpgradeShuffled(value)
            | RunEffect::DowngradeRandom(value)
            | RunEffect::SkipEventRng(value)
            | RunEffect::EventAction(value) => row.u[11] = value as u32,
            RunEffect::AddCard(name, count) => {
                row.u[11..13].copy_from_slice(&[card_id(name), count as u32]);
            }
            RunEffect::AddRelic(name) => row.u[11] = relic_id(name),
            RunEffect::AddPotion(name) => row.u[11] = potion_id(name),
            RunEffect::PotionRewards(name, count) => {
                row.u[11..13].copy_from_slice(&[potion_id(name), count as u32]);
            }
            RunEffect::RandomPotionReward(flag) => row.u[11] = flag as u32,
            RunEffect::RandomCard(values) => {
                for (order, name) in values.iter().enumerate() {
                    let child = self.row(8, 0, frame, 1, path + 1, 0, order as u32, 0);
                    self.rows[child].u[11] = card_id(name);
                }
            }
            RunEffect::RandomRelic(values) => {
                for (order, name) in values.iter().enumerate() {
                    let child = self.row(8, 1, frame, 1, path + 1, 0, order as u32, 0);
                    self.rows[child].u[11] = relic_id(name);
                }
            }
            RunEffect::RemoveCards(count, tag) => {
                row.u[11..13].copy_from_slice(&[count as u32, tag as u32]);
            }
            RunEffect::TransformCards(target, count) => {
                row.u[11..13]
                    .copy_from_slice(&[target.map_or(0, |name| card_id(name) + 1), count as u32]);
            }
            RunEffect::EnchantCards(enchantment, amount, count, card_type) => {
                row.u[11..14].copy_from_slice(&[
                    enchantment as u32,
                    count as u32,
                    card_type.map_or(0, |kind| kind as u32 + 1),
                ]);
                row.s[0] = amount as i32;
            }
            RunEffect::ChooseCommonCards(first, second)
            | RunEffect::ChooseRewardCards(first, second) => {
                row.u[11..13].copy_from_slice(&[first as u32, second as u32]);
            }
            RunEffect::Options(options) => {
                for (order, option) in options.iter().enumerate() {
                    self.option(*option, content, frame, path + 1, order as u32);
                }
            }
        }
    }

    fn option(
        &mut self,
        option: EventOption,
        content: &Content,
        parent: u32,
        path: u32,
        order: u32,
    ) {
        let index = self.row(7, 0, parent, 1, path, 0, order, option.effects.len() as u32);
        match option.requirement {
            Requirement::Always => self.rows[index].u[11] = 0,
            Requirement::Gold(value) => {
                self.rows[index].u[11] = 1;
                self.rows[index].s[0] = value;
            }
            Requirement::Hp(value) => {
                self.rows[index].u[11] = 2;
                self.rows[index].s[0] = value as i32;
            }
            Requirement::Deck => self.rows[index].u[11] = 3,
        }
        let frame = self.rows[index].u[2];
        for (order, &effect) in option.effects.iter().enumerate() {
            self.run_effect(effect, content, frame, 1, path + 1, 0, order as u32);
        }
    }

    fn context(&mut self, context: Context, parent: u32, path: u32) {
        let index = self.row(2, 0, parent, 0, path, 0, 0, 0);
        let row = &mut self.rows[index];
        row.u[11..18].copy_from_slice(&[
            actor_index(context.source) as u32,
            context.target.map_or(0, |target| target as u32 + 1),
            context.card.map_or(0, |id| id as u32 + 1),
            context.upgraded as u32,
            context.orb as u32,
            context.orb_id.map_or(0, |id| id as u32 + 1),
            context.pen_nib as u32,
        ]);
        row.s[..2].copy_from_slice(&[context.x as i32, context.event as i32]);
    }

    fn resume_phase(&mut self, phase: &Phase, content: &Content, parent: u32) {
        let child_count = match phase {
            Phase::Rewards(rewards) => {
                rewards.cards.len()
                    + rewards.card_rewards.len()
                    + rewards.relics.len()
                    + rewards.potions.len()
            }
            Phase::Shop(items) => items.len(),
            Phase::Event(_, options) => options.len(),
            Phase::ChooseCards(cards, ..) => cards.len(),
            Phase::ChooseBundles(bundles) => bundles.iter().map(Vec::len).sum(),
            _ => 0,
        };
        let index = self.row(
            4,
            phase_index(phase) as u32,
            parent,
            0,
            1,
            3,
            0,
            child_count as u32,
        );
        let frame = self.rows[index].u[2];
        match phase {
            Phase::Rewards(rewards) => {
                self.rows[index].s[..2].copy_from_slice(&[rewards.gold, rewards.removals as i32]);
                let mut order = 0;
                for &card in &rewards.cards {
                    self.card(card, frame, 2, order);
                    order += 1;
                }
                for reward in &rewards.card_rewards {
                    self.card_reward(*reward, frame, 2, order);
                    order += 1;
                }
                for &id in &rewards.relics {
                    let child = self.row(8, 3, frame, 1, 2, 0, order, 0);
                    self.rows[child].u[11] = id as u32;
                    order += 1;
                }
                for &id in &rewards.potions {
                    let child = self.row(8, 4, frame, 1, 2, 0, order, 0);
                    self.rows[child].u[11] = id as u32;
                    order += 1;
                }
            }
            Phase::Shop(items) => {
                for (order, item) in items.iter().enumerate() {
                    match item {
                        ShopItem::Card(card, price) => {
                            self.card(*card, frame, 2, order as u32);
                            self.rows.last_mut().unwrap().s[5] = *price;
                        }
                        ShopItem::Relic(id, price) | ShopItem::Potion(id, price) => {
                            let variant = matches!(item, ShopItem::Potion(..)) as u32 + 5;
                            let child = self.row(8, variant, frame, 1, 2, 0, order as u32, 0);
                            self.rows[child].u[11] = *id as u32;
                            self.rows[child].s[0] = *price;
                        }
                        ShopItem::Remove(price) => {
                            let child = self.row(8, 7, frame, 1, 2, 0, order as u32, 0);
                            self.rows[child].s[0] = *price;
                        }
                    }
                }
            }
            Phase::Event(id, options) => {
                self.rows[index].u[11] = *id as u32;
                for (order, option) in options.iter().enumerate() {
                    self.option(*option, content, frame, 2, order as u32);
                }
            }
            Phase::RemoveCards(count, tag, optional) => {
                self.rows[index].u[11..14].copy_from_slice(&[
                    *count as u32,
                    *tag as u32,
                    *optional as u32,
                ]);
            }
            Phase::UpgradeCards(count, optional) => {
                self.rows[index].u[11..13].copy_from_slice(&[*count as u32, *optional as u32]);
            }
            Phase::TransformCards(target, count, optional) => {
                self.rows[index].u[11..14].copy_from_slice(&[
                    target.map_or(0, |id| id as u32 + 1),
                    *count as u32,
                    *optional as u32,
                ]);
            }
            Phase::EnchantCards(enchantment, amount, count, kind, optional) => {
                self.rows[index].u[11..15].copy_from_slice(&[
                    *enchantment as u32,
                    *count as u32,
                    kind.map_or(0, |kind| kind as u32 + 1),
                    *optional as u32,
                ]);
                self.rows[index].s[0] = *amount as i32;
            }
            Phase::ChooseCards(cards, count, optional) => {
                self.rows[index].u[11..13].copy_from_slice(&[*count as u32, *optional as u32]);
                for (order, &card) in cards.iter().enumerate() {
                    self.card(card, frame, 2, order as u32);
                }
            }
            Phase::ChooseBundles(bundles) => {
                for (bundle, cards) in bundles.iter().enumerate() {
                    for (order, &card) in cards.iter().enumerate() {
                        let before = self.rows.len();
                        self.card(card, frame, 2, order as u32);
                        self.rows[before].u[7] = bundle as u32;
                    }
                }
            }
            Phase::Map | Phase::Rest | Phase::Won | Phase::Dead => {}
            Phase::Combat(_) => unreachable!("combat cannot be a resume phase"),
        }
    }
}

fn group_continuation_item(rows: &mut [DomainRow], item: u32, order: u32) {
    for row in rows {
        row.u[2] = item;
        row.u[3] = NO_NODE;
        row.u[4..8].fill(0);
        row.u[8] = order + 1;
        row.u[9] = 0;
    }
}

fn normalized_effect(
    out: &mut ContinuationBuilder,
    game: &Game,
    content: &Content,
    effect: Effect,
    context: Context,
    order: &mut u32,
) {
    match effect {
        Effect::If(condition, yes, no) => {
            let selected = if game.condition(content, context, condition) {
                yes
            } else {
                no
            };
            for effect in selected {
                normalized_effect(out, game, content, *effect, context, order);
            }
        }
        Effect::Repeat(amount, effects) => {
            for _ in 0..game.amount(content, context, amount).max(0) {
                for effect in effects {
                    normalized_effect(out, game, content, *effect, context, order);
                }
            }
        }
        _ => {
            let before = out.rows.len();
            out.effect(
                effect,
                NO_NODE,
                0,
                0,
                0,
                *order,
                Some((game, content, context)),
            );
            let parent = out.rows[before].u[2];
            out.context(context, parent, 0);
            let item = out.rows[before].u[2];
            group_continuation_item(&mut out.rows[before..], item, *order);
            *order += 1;
        }
    }
}

fn phase_continuation_domains(game: &Game, content: &Content) -> Vec<DomainRow> {
    let mut out = ContinuationBuilder::for_game(game, PHASE_SCOPE);
    if !matches!(
        game.phase,
        Phase::Combat(_) | Phase::Map | Phase::Rest | Phase::Won | Phase::Dead
    ) {
        let before = out.rows.len();
        out.resume_phase(&game.phase, content, NO_NODE);
        for row in &mut out.rows[before..] {
            row.u[23] = 1;
        }
    }
    if let Some(phase) = &game.resume {
        let before = out.rows.len();
        out.resume_phase(phase, content, NO_NODE);
        for row in &mut out.rows[before..] {
            row.u[23] = 2;
        }
    }
    if let Some(combat) = game.combat()
        && let Some(card) = combat.playing
    {
        let before = out.rows.len();
        out.card(card, NO_NODE, 1, 0);
        for row in &mut out.rows[before..] {
            row.u[23] = 1;
        }
    }
    if matches!(&game.phase, Phase::Event(id, _) if content.events[*id as usize].id == "EVENT.SLIPPERY_BRIDGE")
    {
        let before = out.rows.len();
        for (order, card) in game
            .event_cards
            .iter()
            .filter_map(|instance| game.run.deck.iter().find(|card| card.instance == *instance))
            .copied()
            .enumerate()
        {
            out.card(card, NO_NODE, 1, order as u32);
        }
        for row in &mut out.rows[before..] {
            row.u[23] = 1;
        }
    }
    out.rows
}

fn continuation_domains(game: &Game, content: &Content) -> Vec<DomainRow> {
    let mut out = ContinuationBuilder::for_game(game, STATE_SCOPE);
    let mut order = 0;
    for &effect in game.run_queue.iter().rev() {
        let before = out.rows.len();
        out.run_effect(effect, content, NO_NODE, 0, 0, 0, order);
        let item = out.rows[before].u[2];
        group_continuation_item(&mut out.rows[before..], item, order);
        order += 1;
    }
    if let Some(combat) = game.combat() {
        for pending in combat.queue.iter().rev() {
            normalized_effect(
                &mut out,
                game,
                content,
                pending.effect,
                pending.context,
                &mut order,
            );
        }
        for play in combat.auto_plays.iter().rev() {
            let before = out.rows.len();
            let item = out.row(3, 0, NO_NODE, 0, 0, 0, order, 1);
            out.rows[item].u[11] = play.plays as u32;
            out.card(play.card, out.rows[item].u[2], 0, 0);
            let frame = out.rows[before].u[2];
            group_continuation_item(&mut out.rows[before..], frame, order);
            order += 1;
        }
    }
    for item in &game.parasol {
        let before = out.rows.len();
        match item {
            ShopItem::Card(card, price) => {
                out.card(*card, NO_NODE, 0, order);
                out.rows[before].s[5] = *price;
            }
            ShopItem::Relic(id, price) | ShopItem::Potion(id, price) => {
                let variant = 5 + matches!(item, ShopItem::Potion(..)) as u32;
                let row = out.row(8, variant, NO_NODE, 0, 0, 0, order, 0);
                out.rows[row].u[11] = *id as u32;
                out.rows[row].s[0] = *price;
            }
            ShopItem::Remove(price) => {
                let row = out.row(8, 7, NO_NODE, 0, 0, 0, order, 0);
                out.rows[row].s[0] = *price;
            }
        }
        let frame = out.rows[before].u[2];
        group_continuation_item(&mut out.rows[before..], frame, order);
        order += 1;
    }
    out.rows
}

fn candidate_card(
    game: &Game,
    content: &Content,
    encoding: &mut CardEncoding,
    layout: Layout,
    scope: i32,
    action: &Action,
) -> Option<DomainRow> {
    let Some((zone, role, order_kind, order, card, context)) = (match action {
        Action::Play { hand, .. } => game.combat().and_then(|combat| {
            combat
                .hand
                .get(*hand)
                .copied()
                .map(|card| (1, 1, 3, *hand as u32 + 1, card, 0))
        }),
        Action::Choose(index) => {
            if let Some(combat) = game.combat()
                && let Some(choice) = combat.choice
            {
                let cards = match choice.pile {
                    Pile::Draw => &combat.draw,
                    Pile::Hand => &combat.hand,
                    Pile::Discard => &combat.discard,
                    Pile::Exhaust => &combat.exhaust,
                    Pile::Offer => &combat.offer,
                };
                cards.get(*index).copied().map(|card| {
                    let zone = match choice.pile {
                        Pile::Draw => 2,
                        Pile::Hand => 1,
                        Pile::Discard => 3,
                        Pile::Exhaust => 4,
                        Pile::Offer => ATTACHED_CARD_ZONE as u32,
                    };
                    let (kind, order) = match choice.pile {
                        Pile::Draw if *index < combat.known_draw_bottom => (2, *index as u32 + 1),
                        Pile::Draw if *index >= combat.draw.len() - combat.known_draw_top => {
                            (1, (combat.draw.len() - index) as u32)
                        }
                        Pile::Draw => (0, 0),
                        Pile::Hand | Pile::Discard | Pile::Exhaust => (3, *index as u32 + 1),
                        Pile::Offer => (1, *index as u32 + 1),
                    };
                    (zone, 2, kind, order, card, 0)
                })
            } else {
                match &game.phase {
                    Phase::ChooseCards(cards, ..) => cards
                        .get(*index)
                        .copied()
                        .map(|card| (ATTACHED_CARD_ZONE as u32, 3, 0, *index as u32 + 1, card, 0)),
                    _ => None,
                }
            }
        }
        Action::RewardCard(index) => match &game.phase {
            Phase::Rewards(rewards) => rewards
                .cards
                .get(*index)
                .copied()
                .map(|card| (ATTACHED_CARD_ZONE as u32, 4, 0, *index as u32 + 1, card, 0)),
            _ => None,
        },
        Action::Buy(index) => match &game.phase {
            Phase::Shop(items) => items.get(*index).and_then(|item| match item {
                ShopItem::Card(card, price) => Some((
                    ATTACHED_CARD_ZONE as u32,
                    5,
                    0,
                    *index as u32 + 1,
                    *card,
                    *price,
                )),
                _ => None,
            }),
            _ => None,
        },
        Action::Smith(index) | Action::Enchant(index) | Action::RemoveCard(index) => game
            .run
            .deck
            .get(*index)
            .copied()
            .map(|card| (0, 6, 0, 0, card, 0)),
        Action::EventCard(index, card) => {
            Some((ATTACHED_CARD_ZONE as u32, 7, 0, *index as u32 + 1, *card, 0))
        }
        Action::Event(_) if matches!(&game.phase, Phase::Event(id, _) if Some(*id) == layout.slippery_bridge) => {
            game.run
                .deck
                .iter()
                .find(|card| card.instance == game.event_data[1] as u32)
                .copied()
                .map(|card| (ATTACHED_CARD_ZONE as u32, 8, 0, 0, card, 0))
        }
        _ => None,
    }) else {
        return None;
    };
    Some(card_domain_row(
        game, content, encoding, scope, zone, role, order_kind, order, card, context,
    ))
}

fn candidate_domain_rows(
    domains: &mut [Vec<DomainRow>; 16],
    game: &Game,
    content: &Content,
    encoding: &mut CardEncoding,
    layout: Layout,
    scope: i32,
    action: &Action,
) {
    if let Some(row) = candidate_card(game, content, encoding, layout, scope, action) {
        domains[CARD_DOMAIN].push(row);
    }
    if let Action::Choose(index) = action
        && let Phase::ChooseBundles(bundles) = &game.phase
        && let Some(cards) = bundles.get(*index)
    {
        domains[CARD_DOMAIN].extend(cards.iter().copied().enumerate().map(|(order, card)| {
            card_domain_row(
                game,
                content,
                encoding,
                scope,
                ATTACHED_CARD_ZONE as u32,
                9,
                1,
                order as u32 + 1,
                card,
                *index as i32 + 1,
            )
        }));
    }
    match action {
        Action::Potion { slot, .. } | Action::DiscardPotion(slot) => {
            if let Some(Some(id)) = game.run.potions.get(*slot) {
                let mut row = DomainRow::new(POTION_DOMAIN, scope);
                row.u[..5].copy_from_slice(&[2, *slot as u32, *id as u32 + 1, 0, *slot as u32 + 1]);
                domains[POTION_DOMAIN].push(row);
            }
        }
        Action::RewardRelic(index) => {
            if let Phase::Rewards(rewards) = &game.phase
                && let Some(&id) = rewards.relics.get(*index)
            {
                let mut row = DomainRow::new(RELIC_DOMAIN, scope);
                row.u[..4].copy_from_slice(&[
                    if game.toy_box_offers.contains(&id) {
                        5
                    } else {
                        4
                    },
                    id as u32,
                    *index as u32 + 1,
                    crate::game::relic_group(id).unwrap_or(4) as u32,
                ]);
                domains[RELIC_DOMAIN].push(row);
            }
        }
        Action::RewardPotion(index) => {
            if let Phase::Rewards(rewards) = &game.phase
                && let Some(&id) = rewards.potions.get(*index)
            {
                let mut row = DomainRow::new(POTION_DOMAIN, scope);
                row.u[..5].copy_from_slice(&[3, u32::MAX, id as u32 + 1, 0, *index as u32 + 1]);
                domains[POTION_DOMAIN].push(row);
            }
        }
        Action::Buy(index) => {
            if let Phase::Shop(items) = &game.phase
                && let Some(item) = items.get(*index)
            {
                match item {
                    ShopItem::Relic(id, price) => {
                        let mut row = DomainRow::new(RELIC_DOMAIN, scope);
                        row.u[..4].copy_from_slice(&[
                            6,
                            *id as u32,
                            *index as u32 + 1,
                            crate::game::relic_group(*id).unwrap_or(4) as u32,
                        ]);
                        row.s[0] = *price;
                        domains[RELIC_DOMAIN].push(row);
                    }
                    ShopItem::Potion(id, price) => {
                        let mut row = DomainRow::new(POTION_DOMAIN, scope);
                        row.u[..5].copy_from_slice(&[
                            4,
                            u32::MAX,
                            *id as u32 + 1,
                            0,
                            *index as u32 + 1,
                        ]);
                        row.s[0] = *price;
                        domains[POTION_DOMAIN].push(row);
                    }
                    ShopItem::Remove(price) => {
                        let mut continuation = ContinuationBuilder::for_game(game, scope);
                        let item = continuation.row(8, 7, NO_NODE, 0, 0, 0, 0, 0);
                        continuation.rows[item].s[0] = *price;
                        domains[CONTINUATION_DOMAIN].extend(continuation.rows);
                    }
                    ShopItem::Card(..) => {}
                }
            }
        }
        Action::Event(index) => {
            if let Phase::Event(id, options) = &game.phase {
                let mut event = DomainRow::new(EVENT_DOMAIN, scope);
                let payload = event_option_payload(game, content, *index);
                event.u[..4].copy_from_slice(&[
                    2,
                    *id as u32,
                    *index as u32 + 1,
                    payload.is_some() as u32,
                ]);
                event.s[0] = payload.unwrap_or_default();
                domains[EVENT_DOMAIN].push(event);
                if let Some(option) = options.get(*index) {
                    let mut continuation = ContinuationBuilder::for_game(game, scope);
                    continuation.option(*option, content, NO_NODE, 0, *index as u32);
                    domains[CONTINUATION_DOMAIN].extend(continuation.rows);
                }
                if let Some(id) = ancient_offer(game, content, *index) {
                    let mut row = DomainRow::new(RELIC_DOMAIN, scope);
                    row.u[..4].copy_from_slice(&[
                        7,
                        id as u32,
                        *index as u32 + 1,
                        crate::game::relic_group(id).unwrap_or(4) as u32,
                    ]);
                    domains[RELIC_DOMAIN].push(row);
                }
                if content.events[*id as usize].id == "EVENT.RANWID_THE_ELDER"
                    && *index == 2
                    && let Some(&owned) = game.event_cards.first()
                    && let Some(&id) = game.run.relics.get(owned as usize)
                {
                    let mut row = DomainRow::new(RELIC_DOMAIN, scope);
                    row.u[..4].copy_from_slice(&[
                        11,
                        id as u32,
                        owned + 1,
                        crate::game::relic_group(id).unwrap_or(4) as u32,
                    ]);
                    domains[RELIC_DOMAIN].push(row);
                }
                if Some(*id) == layout.relic_trader {
                    for (kind, id) in [
                        game.relic_queue.get(*index).copied(),
                        game.event_cards
                            .get(*index)
                            .and_then(|owned| game.run.relics.get(*owned as usize))
                            .copied(),
                    ]
                    .into_iter()
                    .enumerate()
                    .filter_map(|(kind, id)| id.map(|id| (kind, id)))
                    {
                        let mut row = DomainRow::new(RELIC_DOMAIN, scope);
                        row.u[..4].copy_from_slice(&[
                            8 + kind as u32,
                            id as u32,
                            *index as u32 + 1,
                            crate::game::relic_group(id).unwrap_or(4) as u32,
                        ]);
                        domains[RELIC_DOMAIN].push(row);
                    }
                }
            }
        }
        Action::CrystalCell(x, y) => {
            let mut row = DomainRow::new(CRYSTAL_DOMAIN, scope);
            row.u[..3].copy_from_slice(&[2, *x as u32, *y as u32]);
            domains[CRYSTAL_DOMAIN].push(row);
        }
        Action::CrystalTool(big) => {
            let mut row = DomainRow::new(CRYSTAL_DOMAIN, scope);
            row.u[..2].copy_from_slice(&[3, *big as u32]);
            domains[CRYSTAL_DOMAIN].push(row);
        }
        Action::EventRelic(index, id) => {
            let mut row = DomainRow::new(RELIC_DOMAIN, scope);
            row.u[..4].copy_from_slice(&[
                10,
                *id as u32,
                *index as u32 + 1,
                crate::game::relic_group(*id).unwrap_or(4) as u32,
            ]);
            domains[RELIC_DOMAIN].push(row);
        }
        Action::RerollCards => {
            if let Phase::Rewards(rewards) = &game.phase {
                let mut continuation = ContinuationBuilder::for_game(game, scope);
                for (order, reward) in rewards.card_rewards.iter().enumerate() {
                    let frame = continuation.row(8, 2, NO_NODE, 0, 0, 0, order as u32, 0);
                    match reward {
                        CardReward::Standard(room) => {
                            continuation.rows[frame].u[11..13]
                                .copy_from_slice(&[0, room_index(*room) as u32]);
                        }
                        CardReward::Fixed(id, rarity) => {
                            continuation.rows[frame].u[11..14].copy_from_slice(&[
                                1,
                                *id as u32,
                                *rarity as u32,
                            ]);
                        }
                        CardReward::Kaleidoscope => continuation.rows[frame].u[11] = 2,
                        CardReward::Crystal(rarity) => {
                            continuation.rows[frame].u[11..13]
                                .copy_from_slice(&[3, *rarity as u32]);
                        }
                    }
                }
                domains[CONTINUATION_DOMAIN].extend(continuation.rows);
            }
        }
        _ => {}
    }
}

fn action_features(u: &[u32; ACTION_U], s: &[i32; ACTION_S]) -> [f32; ACTION_F] {
    let expected = |slot: usize| f32::from_bits(u[8 + slot]);
    let mut out = [0.0; ACTION_F];
    out[..9].copy_from_slice(&[
        expected(2) / 30.0,
        expected(0) / 30.0,
        expected(1) / 30.0,
        s[7] as f32 / 30.0,
        expected(3) / 10.0,
        expected(4) / 10.0,
        expected(5) / 10.0,
        s[8] as f32 / 10.0,
        s[9] as f32 / 10.0,
    ]);
    out[9] = match u[0] as usize {
        6 => s[0] as f32 / 1_000.0,
        10 => s[0] as f32 / 10.0,
        13 => s[0] as f32 / 1_000.0,
        _ => 0.0,
    };
    out
}

fn packed_observation(row: &ObservationV56) -> (Vec<f32>, Vec<u32>, Vec<u32>, Vec<u32>, u64) {
    packed_observation_known(row, None)
}

fn packed_observation_known(
    row: &ObservationV56,
    known_digest: Option<u64>,
) -> (Vec<f32>, Vec<u32>, Vec<u32>, Vec<u32>, u64) {
    let mut counts = Vec::with_capacity(DOMAIN_NAMES.len());
    let mut exact = Vec::new();
    let mut actions = Vec::new();
    for domain in 0..DOMAIN_NAMES.len() {
        counts.push(row.domains[domain].len() as u32);
        for record in &row.domains[domain] {
            exact.extend(&record.u);
            exact.extend(record.s.iter().map(|&value| value as u32));
            exact.extend(&record.c);
            exact.extend(record.f.iter().map(|value| value.to_bits()));
            exact.push(record.scope as u32);
        }
    }
    for candidate in &row.candidates {
        actions.extend(candidate.u);
        actions.extend(candidate.s.map(|value| value as u32));
        actions.extend(candidate.c);
        actions.extend(candidate.f.map(f32::to_bits));
        actions.push(candidate.legal as u32);
    }
    let digest = known_digest.unwrap_or_else(|| {
        packed_observation_digest(row.character, &row.globals, &counts, &exact, &actions)
    });
    (row.globals.clone(), counts, exact, actions, digest)
}

fn packed_observation_digest(
    character: u8,
    globals: &[f32],
    counts: &[u32],
    exact: &[u32],
    actions: &[u32],
) -> u64 {
    let mut digest = 0xcbf2_9ce4_8422_2325u64;
    let mut update = |value: u32| {
        digest = (digest ^ value as u64).wrapping_mul(0x100_0000_01b3);
    };
    update(VERSION);
    update(VALUE_MODEL_VERSION);
    update(character as u32);
    update(globals.len() as u32);
    for value in globals {
        update(value.to_bits());
    }
    for values in [counts, exact, actions] {
        update(values.len() as u32);
        values.iter().copied().for_each(&mut update);
    }
    digest
}

fn observation_digest(row: &ObservationV56) -> u64 {
    let mut digest = 0xcbf2_9ce4_8422_2325u64;
    let mut update = |value: u32| {
        digest = (digest ^ value as u64).wrapping_mul(0x100_0000_01b3);
    };
    update(VERSION);
    update(VALUE_MODEL_VERSION);
    update(row.character as u32);
    update(row.globals.len() as u32);
    row.globals.iter().for_each(|value| update(value.to_bits()));
    update(row.domains.len() as u32);
    row.domains
        .iter()
        .for_each(|domain| update(domain.len() as u32));
    update(
        row.domains
            .iter()
            .flatten()
            .map(|record| record.u.len() + record.s.len() + record.c.len() + record.f.len() + 1)
            .sum::<usize>() as u32,
    );
    for record in row.domains.iter().flatten() {
        record.u.iter().copied().for_each(&mut update);
        record.s.iter().for_each(|value| update(*value as u32));
        record.c.iter().copied().for_each(&mut update);
        record.f.iter().for_each(|value| update(value.to_bits()));
        update(record.scope as u32);
    }
    update((row.candidates.len() * (ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1)) as u32);
    for candidate in &row.candidates {
        candidate.u.iter().copied().for_each(&mut update);
        candidate.s.iter().for_each(|value| update(*value as u32));
        candidate.c.iter().copied().for_each(&mut update);
        candidate.f.iter().for_each(|value| update(value.to_bits()));
        update(candidate.legal as u32);
    }
    digest
}

fn action_row(
    game: &Game,
    content: &Content,
    layout: Layout,
    map_ids: &[u32],
    action: &Action,
    legal: bool,
) -> CandidateRow {
    let mut row = CandidateRow {
        action: action.clone(),
        u: [0; ACTION_U],
        s: [0; ACTION_S],
        c: [0; ACTION_C],
        f: [0.0; ACTION_F],
        legal,
    };
    row.u[4] = NO_NODE;
    row.u[0] = action_kind(action) as u32;
    row.u[14] = match action {
        Action::Play { hand, .. } => *hand as u32 + 1,
        Action::Potion { slot, .. } | Action::DiscardPotion(slot) => *slot as u32 + 1,
        Action::Choose(index)
        | Action::RewardCard(index)
        | Action::RewardRelic(index)
        | Action::RewardPotion(index)
        | Action::Buy(index)
        | Action::Smith(index)
        | Action::Event(index)
        | Action::EventRelic(index, _)
        | Action::EventCard(index, _)
        | Action::Enchant(index)
        | Action::RemoveCard(index) => *index as u32 + 1,
        Action::Path(index) => map_ids.get(*index).copied().unwrap_or(NO_NODE),
        Action::CrystalCell(x, y) => (*x * 11 + *y) as u32 + 1,
        Action::CrystalTool(big) => *big as u32 + 1,
        _ => 1,
    };
    row.u[1] = match action {
        Action::Play { target, .. } | Action::Potion { target, .. } => {
            target.map_or(0, |target| target as u32 + 3)
        }
        _ => 0,
    };
    match action {
        Action::Potion { slot, .. } | Action::DiscardPotion(slot) => row.u[3] = *slot as u32,
        Action::Choose(index) if game.combat().is_none() => row.u[2] = *index as u32 + 1,
        Action::RewardCard(index)
        | Action::RewardRelic(index)
        | Action::RewardPotion(index)
        | Action::Buy(index)
        | Action::Event(index) => row.u[2] = *index as u32 + 1,
        Action::Path(index) => {
            row.u[4] = map_ids.get(*index).copied().unwrap_or(NO_NODE);
            let normal = game
                .map
                .current
                .map_or(true, |current| game.map.nodes[current].next.contains(index));
            row.u[5] = if normal { 1 } else { 2 };
            if !normal {
                row.u[6] = 1;
                row.u[7] = 3u32.saturating_sub(game.winged_boots as u32);
            }
        }
        Action::CrystalCell(x, y) => {
            row.u[2] = *x as u32;
            row.u[3] = *y as u32;
        }
        Action::CrystalTool(big) => row.u[7] = *big as u32,
        Action::EventRelic(index, _) | Action::EventCard(index, _) => {
            row.u[2] = *index as u32 + 1;
        }
        _ => {}
    }
    if let Action::Buy(index) = action
        && let Phase::Shop(items) = &game.phase
        && let Some(item) = items.get(*index)
    {
        row.s[0] = match item {
            ShopItem::Card(_, price)
            | ShopItem::Relic(_, price)
            | ShopItem::Potion(_, price)
            | ShopItem::Remove(price) => *price,
        };
    }
    match action {
        Action::RewardGold => {
            if let Phase::Rewards(rewards) = &game.phase {
                row.s[0] = rewards.gold;
            }
        }
        Action::RewardRemove => {
            if let Phase::Rewards(rewards) = &game.phase {
                row.s[0] = rewards.removals as i32;
            }
        }
        _ => {}
    }
    let mut semantic = Vec::new();
    semantic.push(layout.semantic(Semantic::Action, row.u[0]));
    semantic.push(layout.semantic(Semantic::Phase, phase_index(&game.phase) as u32));
    if row.u[1] > 0 {
        semantic.push(layout.semantic(Semantic::Target, 3));
        let position = row.u[1].saturating_sub(3);
        if let Some(enemy) = game
            .combat()
            .and_then(|combat| combat.enemies.get(position as usize))
        {
            semantic.push(layout.semantic(Semantic::Enemy, enemy.creature.id as u32));
            semantic.push(layout.semantic(Semantic::EnemyPosition, position.min(32)));
        }
    }
    if row.u[5] > 0 {
        semantic.push(layout.semantic(Semantic::RouteKind, row.u[5]));
    }
    if row.u[4] != NO_NODE {
        semantic.push(layout.semantic(Semantic::RunKind, 28));
    }
    row.c[..semantic.len()].copy_from_slice(&semantic);
    let preview = action_preview_resolved(game, content, action, legal);
    let values = action_preview_values(game, content, action, preview.as_ref());
    let (energy, stars) = game
        .combat()
        .map_or((0, 0), |combat| (combat.energy, combat.stars));
    row.s[1..7].copy_from_slice(&[
        values[0].round() as i32,
        values[1].round() as i32,
        values[2].round() as i32,
        values[3].round() as i32,
        values[7].round() as i32,
        values[8].round() as i32,
    ]);
    row.s[10] = energy as i32;
    row.s[11] = stars as i32;
    row.u[8] = values[4].to_bits();
    row.u[9] = values[5].to_bits();
    if preview.is_some() {
        let slot = row.c.iter().position(|&value| value == 0).unwrap();
        row.c[slot] = layout.semantic(Semantic::Boolean, 120 + (values[6] != 0.0) as u32);
    }
    if let Some(preview) = preview {
        row.s[7] = preview.player_hp_delta as i32;
        let (energy_cost, star_cost) = preview.costs.unwrap_or((0, 0));
        row.s[8] = energy_cost as i32;
        row.s[9] = star_cost as i32;
        for (slot, value) in [
            preview.outcome.block.unwrap_or(preview.block as f32),
            preview.outcome.draw.unwrap_or(preview.draw as f32),
            preview.outcome.discard.unwrap_or(preview.discard as f32),
            preview.outcome.exhaust.unwrap_or(preview.exhaust as f32),
        ]
        .into_iter()
        .enumerate()
        {
            row.u[10 + slot] = value.to_bits();
        }
    }
    row.f = action_features(&row.u, &row.s);
    row
}

fn observation_v56(
    game: &Game,
    content: &Content,
    layout: Layout,
    bonuses: (i16, i16),
) -> ObservationV56 {
    observation_v56_with_map(game, content, layout, bonuses, None)
}

fn candidate_signature(
    game: &Game,
    content: &Content,
    layout: Layout,
    action: &Action,
) -> Option<(CandidateRow, [Vec<DomainRow>; 16])> {
    let observation = observation_v56(game, content, layout, (0, 0));
    let index = observation
        .candidates
        .iter()
        .position(|candidate| &candidate.action == action)?;
    let domains = std::array::from_fn(|domain| {
        observation.domains[domain]
            .iter()
            .filter(|row| row.scope == index as i32)
            .cloned()
            .map(|mut row| {
                row.scope = 0;
                row
            })
            .collect()
    });
    Some((observation.candidates[index].clone(), domains))
}

fn observation_v56_with_map(
    game: &Game,
    content: &Content,
    layout: Layout,
    bonuses: (i16, i16),
    map: Option<&CanonicalMap>,
) -> ObservationV56 {
    let (nodes, edges, map_ids, current) = map
        .cloned()
        .unwrap_or_else(|| canonical_map(game, content, layout));
    let mut domains: [Vec<DomainRow>; 16] = std::array::from_fn(|_| Vec::new());
    domains[RUN_DOMAIN].push(run_domain_row(game, bonuses, current));
    domains[PHASE_DOMAIN].push(phase_domain_row(game, content));
    let mut card_encoding = CardEncoding::new(game, content);
    state_card_domains(game, content, &mut card_encoding, &mut domains);
    push_actor_domains(game, content, &card_encoding, &mut domains);
    push_collection_domains(game, content, layout, &mut domains);
    domains[CONTINUATION_DOMAIN] = continuation_domains(game, content);
    domains[CONTINUATION_DOMAIN].extend(phase_continuation_domains(game, content));
    let (actions, legal) = candidate_actions(game, content);
    assert!(!actions.is_empty() || matches!(game.phase, Phase::Won | Phase::Dead));
    let candidates = actions
        .iter()
        .zip(&legal)
        .map(|(action, &legal)| action_row(game, content, layout, &map_ids, action, legal))
        .collect::<Vec<_>>();
    for (scope, action) in actions.iter().enumerate() {
        candidate_domain_rows(
            &mut domains,
            game,
            content,
            &mut card_encoding,
            layout,
            scope as i32,
            action,
        );
    }
    if matches!(&game.phase, Phase::Combat(combat) if combat.choice.is_some()) {
        let mut frame = 0u32;
        for action in &actions {
            let before = domains[CONTINUATION_DOMAIN].len();
            candidate_domain_rows(
                &mut domains,
                game,
                content,
                &mut card_encoding,
                layout,
                PHASE_SCOPE,
                action,
            );
            for row in &mut domains[CONTINUATION_DOMAIN][before..] {
                row.u[2] += frame;
                if row.u[3] != NO_NODE {
                    row.u[3] += frame;
                }
            }
            frame += (domains[CONTINUATION_DOMAIN].len() - before) as u32;
        }
    }
    for (domain, rows) in domains.iter_mut().enumerate() {
        if !matches!(
            domain,
            CONTINUATION_DOMAIN | MAP_NODE_DOMAIN | MAP_EDGE_DOMAIN
        ) {
            rows.sort_by(|left, right| {
                left.scope
                    .cmp(&right.scope)
                    .then_with(|| left.u.cmp(&right.u))
                    .then_with(|| left.s.cmp(&right.s))
            });
        }
        for row in rows {
            populate_domain_features(game, content, layout, domain, row);
        }
    }
    let mut domains = domains.map(DomainRows::Owned);
    domains[MAP_NODE_DOMAIN] = DomainRows::Shared(nodes);
    domains[MAP_EDGE_DOMAIN] = DomainRows::Shared(edges);
    ObservationV56 {
        character: game.run.character as u8,
        globals: observation_globals_with_bonuses(game, content, layout, bonuses, true),
        domains,
        candidates,
        potential: potential(game),
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
    if replaying || game.encounters.is_empty() {
        game.encounters = encounters;
    }
    if replaying || game.elites.is_empty() {
        game.elites = elites;
    }
    game.events = content.acts[game.act as usize].events.to_vec();
    game.encounters.sort_unstable();
    game.elites.sort_unstable();
    game.events.sort_unstable();
    rng.shuffle(&mut game.encounters);
    rng.shuffle(&mut game.elites);
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

struct LinearWeights {
    w: Vec<f32>,
    b: Vec<f32>,
}

impl LinearWeights {
    fn read(input: &mut &[u8], input_width: usize, output_width: usize) -> io::Result<Self> {
        Ok(Self {
            w: read_f32s(input, input_width * output_width)?,
            b: read_f32s(input, output_width)?,
        })
    }

    fn read_without_bias(
        input: &mut &[u8],
        input_width: usize,
        output_width: usize,
    ) -> io::Result<Self> {
        Ok(Self {
            w: read_f32s(input, input_width * output_width)?,
            b: vec![0.0; output_width],
        })
    }

    fn apply(&self, input: &[f32]) -> Vec<f32> {
        linear(input, &self.w, &self.b)
    }
}

struct TokenEncoderWeights {
    numeric: LinearWeights,
    norm_w: Vec<f32>,
    norm_b: Vec<f32>,
}

impl TokenEncoderWeights {
    fn read(input: &mut &[u8], _semantic: usize, numeric: usize, width: usize) -> io::Result<Self> {
        Ok(Self {
            numeric: LinearWeights::read_without_bias(input, numeric, width)?,
            norm_w: read_f32s(input, width)?,
            norm_b: read_f32s(input, width)?,
        })
    }

    fn encode(&self, semantic: &[u32], numeric: &[f32], table: &[f32], width: usize) -> Vec<f32> {
        self.finish(semantic, table, width, self.numeric.apply(numeric))
    }

    fn finish(&self, semantic: &[u32], table: &[f32], width: usize, mut out: Vec<f32>) -> Vec<f32> {
        for &code in semantic.iter().filter(|&&code| code != 0) {
            let embedding = &table[code as usize * width..][..width];
            out.iter_mut()
                .zip(embedding)
                .for_each(|(value, embedding)| *value += embedding);
        }
        layer_norm(&mut out, &self.norm_w, &self.norm_b);
        out
    }
}

type MapEncoding = std::collections::BTreeMap<u32, Vec<f32>>;

#[derive(Default)]
struct EncodingCache {
    rows: HashMap<(u64, u64), Vec<f32>>,
    groups: HashMap<(usize, u64, u64), Vec<f32>>,
}

struct GruWeights {
    input: LinearWeights,
    hidden: LinearWeights,
}

impl GruWeights {
    fn read(input: &mut &[u8], width: usize) -> io::Result<Self> {
        Ok(Self {
            input: LinearWeights::read(input, width, 3 * width)?,
            hidden: LinearWeights::read(input, width, 3 * width)?,
        })
    }

    fn apply(&self, sequence: impl IntoIterator<Item = Vec<f32>>, width: usize) -> Vec<f32> {
        let mut state = vec![0.0; width];
        for value in sequence {
            let input = self.input.apply(&value);
            let hidden = self.hidden.apply(&state);
            for column in 0..width {
                let reset = sigmoid(input[column] + hidden[column]);
                let update = sigmoid(input[width + column] + hidden[width + column]);
                let candidate =
                    (input[2 * width + column] + reset * hidden[2 * width + column]).tanh();
                state[column] = (1.0 - update) * candidate + update * state[column];
            }
        }
        state
    }
}

pub struct ValueModel {
    layout: Layout,
    width: usize,
    heads: usize,
    pooling: [u8; 13],
    semantic_embedding: Vec<f32>,
    encoders: Vec<TokenEncoderWeights>,
    action_encoder: TokenEncoderWeights,
    summary_seed: Vec<Vec<f32>>,
    pool_layers: Vec<Option<TransformerLayer>>,
    move_gru: GruWeights,
    continuation_gru: Option<GruWeights>,
    actor_norm_w: Vec<f32>,
    actor_norm_b: Vec<f32>,
    action_norm_w: Vec<f32>,
    action_norm_b: Vec<f32>,
    global_layers: Vec<TransformerLayer>,
    global_norm_w: Vec<f32>,
    global_norm_b: Vec<f32>,
    graph_norm_w: Vec<f32>,
    graph_norm_b: Vec<f32>,
    graph_query: LinearWeights,
    graph_key_value: LinearWeights,
    graph_edge: LinearWeights,
    graph_out: LinearWeights,
    graph_degree: LinearWeights,
    graph_ff_norm_w: Vec<f32>,
    graph_ff_norm_b: Vec<f32>,
    graph_ff1: LinearWeights,
    graph_ff2: LinearWeights,
    policy: Option<LinearWeights>,
    critic: LinearWeights,
    temperature: f32,
    bias: f32,
    encode_caches: Vec<Mutex<EncodingCache>>,
}

impl ValueModel {
    pub fn load(path: impl AsRef<Path>, content: &Content) -> io::Result<Self> {
        Self::from_bytes(&fs::read(path)?, content)
    }

    fn from_bytes(bytes: &[u8], content: &Content) -> io::Result<Self> {
        let mut input = bytes;
        if take_bytes(&mut input, 8)? != MAGIC
            || read_u32(&mut input)? != VALUE_MODEL_VERSION
            || read_u32(&mut input)? != VERSION
            || read_u64(&mut input)? != content_fingerprint(content)
        {
            return Err(invalid("incompatible value model"));
        }
        let dimensions = (0..10)
            .map(|_| read_u32(&mut input))
            .collect::<io::Result<Vec<_>>>()?;
        let [
            width,
            layers,
            heads,
            feedforward,
            domains,
            concepts,
            concept_vocab,
            action_c,
            action_f,
            categories,
        ] = <[u32; 10]>::try_from(dimensions).unwrap();
        let widths = (0..domains)
            .map(|_| {
                Ok((
                    read_u32(&mut input)? as usize,
                    read_u32(&mut input)? as usize,
                    read_u32(&mut input)? as usize,
                    read_u32(&mut input)? as usize,
                ))
            })
            .collect::<io::Result<Vec<_>>>()?;
        let concept_sizes = (0..concepts)
            .map(|_| read_u32(&mut input))
            .collect::<io::Result<Vec<_>>>()?;
        let position_caps = (0..POSITION_CAPS.len())
            .map(|_| read_u32(&mut input))
            .collect::<io::Result<Vec<_>>>()?;
        let pooling = <[u8; 13]>::try_from(take_bytes(&mut input, 13)?).unwrap();
        let actor = take_bytes(&mut input, 1)?[0] != 0;
        let layout = Layout::new(content);
        if (width, layers, heads, feedforward) != (128, 4, 8, 384)
            || domains as usize != DOMAIN_NAMES.len()
            || concepts as usize != Semantic::Count as usize
            || concept_vocab as usize != layout.concept_vocab()
            || action_c as usize != ACTION_C
            || action_f as usize != ACTION_F
            || categories as usize != VALUE_CATEGORIES
            || widths != DOMAIN_WIDTHS
            || concept_sizes != layout.semantic_sizes
            || position_caps != POSITION_CAPS
            || !valid_pooling(pooling)
        {
            return Err(invalid("value model shape mismatch"));
        }
        let temperature = read_f32(&mut input)?;
        let bias = read_f32(&mut input)?;
        if !temperature.is_finite() || temperature <= 0.0 || !bias.is_finite() {
            return Err(invalid("invalid value calibration"));
        }
        let width = width as usize;
        let semantic_embedding = read_f32s(&mut input, concept_vocab as usize * width)?;
        let encoders = DOMAIN_WIDTHS
            .iter()
            .map(|&(_, _, semantic, numeric)| {
                TokenEncoderWeights::read(&mut input, semantic, numeric, width)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let action_encoder = TokenEncoderWeights::read(&mut input, ACTION_C, ACTION_F, width)?;
        let summary_seed = (0..17)
            .map(|_| read_f32s(&mut input, width))
            .collect::<io::Result<Vec<_>>>()?;
        let mut pool_layers = Vec::with_capacity(13);
        for (index, &mode) in pooling.iter().enumerate() {
            pool_layers.push(if transformer_pool(index, mode) {
                Some(read_transformer_layers(&mut input, 1, width, feedforward as usize)?.remove(0))
            } else {
                None
            });
        }
        let move_gru = GruWeights::read(&mut input, width)?;
        let continuation_gru = (pooling[10] == 2)
            .then(|| GruWeights::read(&mut input, width))
            .transpose()?;
        let actor_norm_w = read_f32s(&mut input, width)?;
        let actor_norm_b = read_f32s(&mut input, width)?;
        let action_norm_w = read_f32s(&mut input, width)?;
        let action_norm_b = read_f32s(&mut input, width)?;
        let global_layers =
            read_transformer_layers(&mut input, layers as usize, width, feedforward as usize)?;
        let global_norm_w = read_f32s(&mut input, width)?;
        let global_norm_b = read_f32s(&mut input, width)?;
        let graph_norm_w = read_f32s(&mut input, width)?;
        let graph_norm_b = read_f32s(&mut input, width)?;
        let graph_query = LinearWeights::read(&mut input, width, width)?;
        let graph_key_value = LinearWeights::read(&mut input, width, 2 * width)?;
        let graph_edge = LinearWeights::read_without_bias(&mut input, width, 2 * width)?;
        let graph_out = LinearWeights::read(&mut input, width, width)?;
        let graph_degree = LinearWeights::read_without_bias(&mut input, 2, width)?;
        let graph_ff_norm_w = read_f32s(&mut input, width)?;
        let graph_ff_norm_b = read_f32s(&mut input, width)?;
        let graph_ff1 = LinearWeights::read(&mut input, width, feedforward as usize)?;
        let graph_ff2 = LinearWeights::read(&mut input, feedforward as usize, width)?;
        let policy = actor
            .then(|| LinearWeights::read(&mut input, width, 1))
            .transpose()?;
        let critic = LinearWeights::read(&mut input, width, VALUE_CATEGORIES)?;
        if !input.is_empty() {
            return Err(invalid("trailing value model data"));
        }
        Ok(Self {
            layout,
            width,
            heads: heads as usize,
            pooling,
            semantic_embedding,
            encoders,
            action_encoder,
            summary_seed,
            pool_layers,
            move_gru,
            continuation_gru,
            actor_norm_w,
            actor_norm_b,
            action_norm_w,
            action_norm_b,
            global_layers,
            global_norm_w,
            global_norm_b,
            graph_norm_w,
            graph_norm_b,
            graph_query,
            graph_key_value,
            graph_edge,
            graph_out,
            graph_degree,
            graph_ff_norm_w,
            graph_ff_norm_b,
            graph_ff1,
            graph_ff2,
            policy,
            critic,
            temperature,
            bias,
            encode_caches: (0..16).map(|_| Mutex::default()).collect(),
        })
    }

    fn embedding(&self, semantic: Semantic, value: u32) -> Vec<f32> {
        let code = self.layout.semantic(semantic, value) as usize;
        self.semantic_embedding[code * self.width..][..self.width].to_vec()
    }

    #[cfg(test)]
    fn candidate_object_key(observation: &ObservationV56, index: usize) -> (u32, u32) {
        let candidate = &observation.candidates[index];
        (candidate.u[0], candidate.u[14])
    }

    fn encode(&self, domain: usize, row: &DomainRow) -> Vec<f32> {
        self.encoders[domain].encode(&row.c, &row.f, &self.semantic_embedding, self.width)
    }

    fn tag(&self, mut value: Vec<f32>, role: u32, collection: Option<u32>) -> Vec<f32> {
        for (sum, embedding) in value
            .iter_mut()
            .zip(self.embedding(Semantic::TokenRole, role))
        {
            *sum += embedding;
        }
        if let Some(collection) = collection {
            for (sum, embedding) in value
                .iter_mut()
                .zip(self.embedding(Semantic::Collection, collection))
            {
                *sum += embedding;
            }
        }
        value
    }

    fn attention(&self, query: &[f32], keys_values: &[Vec<f32>]) -> Vec<f32> {
        let dimension = self.width / self.heads;
        let mut output = vec![0.0; self.width];
        for head in 0..self.heads {
            let columns = head * dimension..(head + 1) * dimension;
            let scores = keys_values
                .iter()
                .map(|row| {
                    query[columns.clone()]
                        .iter()
                        .zip(&row[columns.clone()])
                        .map(|(left, right)| left * right)
                        .sum::<f32>()
                        / (dimension as f32).sqrt()
                })
                .collect::<Vec<_>>();
            let peak = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let weights = scores
                .iter()
                .map(|score| (score - peak).exp())
                .collect::<Vec<_>>();
            let total = weights.iter().sum::<f32>();
            for (row, weight) in keys_values.iter().zip(weights) {
                for column in columns.clone() {
                    output[column] += weight / total * row[self.width + column];
                }
            }
        }
        output
    }

    fn transform(&self, sequence: &mut [Vec<f32>], layer: &TransformerLayer) {
        let qkv = sequence
            .iter()
            .map(|row| {
                linear(
                    &normalized(row, &layer.norm1_w, &layer.norm1_b),
                    &layer.qkv_w,
                    &layer.qkv_b,
                )
            })
            .collect::<Vec<_>>();
        let keys_values = qkv
            .iter()
            .map(|row| row[self.width..].to_vec())
            .collect::<Vec<_>>();
        let attended = qkv
            .iter()
            .map(|row| self.attention(&row[..self.width], &keys_values))
            .collect::<Vec<_>>();
        for (row, value) in sequence.iter_mut().zip(attended) {
            for (left, right) in row
                .iter_mut()
                .zip(linear(&value, &layer.out_w, &layer.out_b))
            {
                *left += right;
            }
        }
        for row in sequence {
            let hidden = dense_gelu(
                &normalized(row, &layer.norm2_w, &layer.norm2_b),
                &layer.linear1_w,
                &layer.linear1_b,
            );
            for (left, right) in
                row.iter_mut()
                    .zip(linear(&hidden, &layer.linear2_w, &layer.linear2_b))
            {
                *left += right;
            }
        }
    }

    fn summarize(&self, name: usize, mode: u8, values: Vec<Vec<f32>>) -> Vec<f32> {
        match mode {
            0 => values
                .into_iter()
                .fold(vec![0.0; self.width], |mut sum, value| {
                    sum.iter_mut()
                        .zip(value)
                        .for_each(|(sum, value)| *sum += value);
                    sum
                }),
            1 => {
                let mut sequence = vec![self.summary_seed[name].clone()];
                sequence.extend(values);
                self.transform(
                    &mut sequence,
                    self.pool_layers[name.min(12)].as_ref().unwrap(),
                );
                sequence.remove(0)
            }
            2 if name == 10 => self
                .continuation_gru
                .as_ref()
                .unwrap()
                .apply(values, self.width),
            _ => unreachable!(),
        }
    }

    fn map(&self, observation: &ObservationV56, current: u32) -> (Vec<f32>, MapEncoding) {
        let nodes = observation.domains[MAP_NODE_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .collect::<Vec<_>>();
        let edges = observation.domains[MAP_EDGE_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .map(|row| (row, self.encode(MAP_EDGE_DOMAIN, row)))
            .collect::<Vec<_>>();
        let mut encoded = nodes
            .iter()
            .map(|row| (row.u[0], self.encode(MAP_NODE_DOMAIN, row)))
            .collect::<MapEncoding>();
        let mut levels = nodes.iter().map(|row| row.u[8]).collect::<Vec<_>>();
        levels.sort_unstable();
        levels.dedup();
        for level in levels.into_iter().rev() {
            let normalized_nodes = encoded
                .iter()
                .map(|(&id, value)| {
                    (
                        id,
                        normalized(value, &self.graph_norm_w, &self.graph_norm_b),
                    )
                })
                .collect::<MapEncoding>();
            let updates = nodes
                .iter()
                .filter(|row| row.u[8] == level && row.u[9] > 0)
                .map(|row| {
                    let id = row.u[0];
                    let query = self.graph_query.apply(&normalized_nodes[&id]);
                    let children = edges
                        .iter()
                        .filter(|(edge, _)| edge.u[0] == id)
                        .map(|(edge, value)| {
                            self.graph_key_value
                                .apply(&normalized_nodes[&edge.u[1]])
                                .into_iter()
                                .zip(self.graph_edge.apply(value))
                                .map(|(left, right)| left + right)
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    let mut value = encoded[&id].clone();
                    let attention = self.graph_out.apply(&self.attention(&query, &children));
                    let degree = self
                        .graph_degree
                        .apply(&[row.u[9] as f32 / 8.0, (row.u[9] as f32).ln_1p() / 3.0]);
                    for column in 0..self.width {
                        value[column] += attention[column] + degree[column];
                    }
                    let hidden = dense_gelu(
                        &normalized(&value, &self.graph_ff_norm_w, &self.graph_ff_norm_b),
                        &self.graph_ff1.w,
                        &self.graph_ff1.b,
                    );
                    for (left, right) in value.iter_mut().zip(self.graph_ff2.apply(&hidden)) {
                        *left += right;
                    }
                    (id, value)
                })
                .collect::<Vec<_>>();
            for (id, value) in updates {
                encoded.insert(id, value);
            }
        }
        let normalized_nodes = encoded
            .values()
            .map(|value| {
                self.graph_key_value.apply(&normalized(
                    value,
                    &self.graph_norm_w,
                    &self.graph_norm_b,
                ))
            })
            .collect::<Vec<_>>();
        let mut selected = encoded[&current].clone();
        let query = self.graph_query.apply(&normalized(
            &selected,
            &self.graph_norm_w,
            &self.graph_norm_b,
        ));
        for (left, right) in selected.iter_mut().zip(
            self.graph_out
                .apply(&self.attention(&query, &normalized_nodes)),
        ) {
            *left += right;
        }
        let hidden = dense_gelu(
            &normalized(&selected, &self.graph_ff_norm_w, &self.graph_ff_norm_b),
            &self.graph_ff1.w,
            &self.graph_ff1.b,
        );
        for (left, right) in selected.iter_mut().zip(self.graph_ff2.apply(&hidden)) {
            *left += right;
        }
        (selected, encoded)
    }

    fn collection(&self, mut values: Vec<Vec<f32>>, name: usize, collection: u32) -> Vec<Vec<f32>> {
        let mode = self.pooling[name];
        if mode == 2 {
            return values
                .into_iter()
                .map(|value| self.tag(value, 9, Some(collection)))
                .collect();
        }
        vec![self.tag(
            self.summarize(name, mode, std::mem::take(&mut values)),
            14,
            Some(collection),
        )]
    }

    fn actors(&self, observation: &ObservationV56) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let mut actors = observation.domains[ACTOR_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .collect::<Vec<_>>();
        actors.sort_by_key(|row| row.u[0]);
        let mut values = Vec::new();
        let mut effects_out = Vec::new();
        for actor in actors {
            let owner = actor.u[0];
            let mut value = self.encode(ACTOR_DOMAIN, actor);
            for history in observation.domains[HISTORY_DOMAIN]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE && row.u[0] == owner && row.u[1] == 0)
            {
                let encoded = self.encode(HISTORY_DOMAIN, history);
                value
                    .iter_mut()
                    .zip(encoded)
                    .for_each(|(left, right)| *left += right);
            }
            let mut moves = observation.domains[HISTORY_DOMAIN]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE && row.u[0] == owner && row.u[1] == 1)
                .collect::<Vec<_>>();
            moves.sort_by_key(|row| row.u[2]);
            let history = self.move_gru.apply(
                moves.into_iter().map(|row| {
                    let code = row.c[0] as usize;
                    self.semantic_embedding[code * self.width..][..self.width].to_vec()
                }),
                self.width,
            );
            value
                .iter_mut()
                .zip(history)
                .for_each(|(left, right)| *left += right);
            let effects = [POWER_DOMAIN, STATUS_DOMAIN]
                .into_iter()
                .flat_map(|domain| {
                    observation.domains[domain]
                        .iter()
                        .filter(move |row| row.scope == STATE_SCOPE && row.u[0] == owner)
                        .map(move |row| self.encode(domain, row))
                })
                .collect::<Vec<_>>();
            let index = if actor.u[1] == 2 { 8 } else { 9 };
            let mode = self.pooling[index];
            if mode == 4 {
                effects_out.extend(
                    effects
                        .into_iter()
                        .map(|effect| self.tag(effect, 10, Some(if index == 8 { 9 } else { 8 }))),
                );
            } else {
                let pooled = self.summarize(index, if mode % 2 == 0 { 0 } else { 1 }, effects);
                if mode < 2 {
                    value
                        .iter_mut()
                        .zip(pooled)
                        .for_each(|(left, right)| *left += right);
                } else {
                    effects_out.push(self.tag(pooled, 10, Some(if index == 8 { 9 } else { 8 })));
                }
            }
            value = self.tag(value, 8, None);
            layer_norm(&mut value, &self.actor_norm_w, &self.actor_norm_b);
            values.push(value);
        }
        (values, effects_out)
    }

    fn continuation_items(&self, observation: &ObservationV56) -> Vec<Vec<f32>> {
        let mut rows = observation.domains[CONTINUATION_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| (row.u[8], row.u[2]));
        let mut items = Vec::<(u32, Vec<f32>)>::new();
        for row in rows {
            if items.last().is_none_or(|(id, _)| *id != row.u[2]) {
                items.push((row.u[2], vec![0.0; self.width]));
            }
            let encoded = self.encode(CONTINUATION_DOMAIN, row);
            items
                .last_mut()
                .unwrap()
                .1
                .iter_mut()
                .zip(encoded)
                .for_each(|(left, right)| *left += right);
        }
        items.into_iter().map(|(_, value)| value).collect()
    }

    fn state_actions_uncached(&self, observation: &ObservationV56) -> (Vec<f32>, Vec<Vec<f32>>) {
        let state_rows = |domain: usize| {
            observation.domains[domain]
                .iter()
                .filter(|row| row.scope == STATE_SCOPE)
                .collect::<Vec<_>>()
        };
        let run = state_rows(RUN_DOMAIN);
        let current_id = run[0].u[23];
        let (map, nodes) = self.map(observation, current_id);
        let mut phase = state_rows(PHASE_DOMAIN)
            .into_iter()
            .map(|row| self.encode(PHASE_DOMAIN, row))
            .collect::<Vec<_>>();
        for domain in 0..DOMAIN_NAMES.len() {
            phase.extend(
                observation.domains[domain]
                    .iter()
                    .filter(|row| row.scope == PHASE_SCOPE)
                    .map(|row| self.encode(domain, row)),
            );
        }
        let generation = [
            (CARD_DOMAIN, 13, 4),
            (RELIC_DOMAIN, 14, 5),
            (ENCOUNTER_DOMAIN, 15, 6),
            (EVENT_DOMAIN, 16, 7),
        ]
        .map(|(domain, seed, role)| {
            let values = state_rows(domain)
                .into_iter()
                .filter(|row| match domain {
                    CARD_DOMAIN => row.u[0] == CARD_POOL_ZONE as u32,
                    RELIC_DOMAIN => matches!(row.u[0], 1 | 2),
                    ENCOUNTER_DOMAIN => row.u[0] <= 2,
                    EVENT_DOMAIN => row.u[0] == 0,
                    _ => false,
                })
                .map(|row| self.encode(domain, row))
                .collect();
            self.tag(self.summarize(seed, self.pooling[12], values), role, None)
        });
        let mut tokens = vec![
            self.tag(self.encode(RUN_DOMAIN, run[0]), 1, None),
            self.tag(self.summarize(11, self.pooling[11], phase), 2, None),
            self.tag(map, 3, None),
        ];
        tokens.extend(generation);
        let cards = state_rows(CARD_DOMAIN);
        let deck = cards
            .iter()
            .filter(|row| row.u[0] == 0)
            .map(|row| self.encode(CARD_DOMAIN, row))
            .collect();
        tokens.append(&mut self.collection(deck, 1, 0));
        let mut relics = state_rows(RELIC_DOMAIN)
            .into_iter()
            .filter(|row| row.u[0] == 0)
            .map(|row| (row, self.encode(RELIC_DOMAIN, row)))
            .collect::<Vec<_>>();
        let stored = cards
            .iter()
            .filter(|row| row.u[0] == PAEL_ZONE as u32)
            .map(|row| self.encode(CARD_DOMAIN, row))
            .fold(vec![0.0; self.width], |mut sum, value| {
                sum.iter_mut()
                    .zip(value)
                    .for_each(|(left, right)| *left += right);
                sum
            });
        for (row, value) in &mut relics {
            if row.u[9] != 0 {
                value
                    .iter_mut()
                    .zip(&stored)
                    .for_each(|(left, right)| *left += right);
            }
        }
        tokens.append(&mut self.collection(
            relics.into_iter().map(|(_, value)| value).collect(),
            0,
            5,
        ));
        let potions = state_rows(POTION_DOMAIN)
            .into_iter()
            .filter(|row| row.u[0] == 0)
            .map(|row| self.encode(POTION_DOMAIN, row))
            .collect();
        tokens.append(&mut self.collection(potions, 7, 6));
        let continuations = self.continuation_items(observation);
        if self.pooling[10] == 3 {
            tokens.extend(
                continuations
                    .into_iter()
                    .map(|value| self.tag(value, 11, Some(10))),
            );
        } else {
            tokens.push(self.tag(
                self.summarize(10, self.pooling[10], continuations),
                14,
                Some(10),
            ));
        }
        tokens.extend(
            state_rows(CRYSTAL_DOMAIN)
                .into_iter()
                .map(|row| self.tag(self.encode(CRYSTAL_DOMAIN, row), 12, None)),
        );
        if !state_rows(ACTOR_DOMAIN).is_empty() {
            let (actors, effects) = self.actors(observation);
            tokens.extend(actors);
            for (zone, name, collection) in [(1, 5, 1), (2, 2, 2), (3, 4, 3), (4, 3, 4)] {
                let values = cards
                    .iter()
                    .filter(|row| row.u[0] == zone)
                    .map(|row| self.encode(CARD_DOMAIN, row))
                    .collect();
                tokens.append(&mut self.collection(values, name, collection));
            }
            let orbs = state_rows(ORB_DOMAIN)
                .into_iter()
                .map(|row| self.encode(ORB_DOMAIN, row))
                .collect();
            tokens.append(&mut self.collection(orbs, 6, 7));
            tokens.extend(effects);
        }
        let mut actions = Vec::with_capacity(observation.candidates.len());
        for (index, candidate) in observation.candidates.iter().enumerate() {
            let mut action = self.action_encoder.encode(
                &candidate.c,
                &candidate.f,
                &self.semantic_embedding,
                self.width,
            );
            for domain in 0..DOMAIN_NAMES.len() {
                for row in observation.domains[domain]
                    .iter()
                    .filter(|row| row.scope == index as i32)
                {
                    let value = self.encode(domain, row);
                    action
                        .iter_mut()
                        .zip(value)
                        .for_each(|(left, right)| *left += right);
                }
            }
            if candidate.u[4] != NO_NODE {
                action
                    .iter_mut()
                    .zip(&nodes[&candidate.u[4]])
                    .for_each(|(left, right)| *left += right);
            }
            action = self.tag(action, 13, None);
            layer_norm(&mut action, &self.action_norm_w, &self.action_norm_b);
            actions.push(action.clone());
            tokens.push(action);
        }
        let action_start = tokens.len() - actions.len() + 1;
        let mut sequence = vec![self.embedding(Semantic::TokenRole, 0)];
        sequence.extend(tokens);
        for layer in &self.global_layers {
            self.transform(&mut sequence, layer);
        }
        for value in &mut sequence {
            layer_norm(value, &self.global_norm_w, &self.global_norm_b);
        }
        (sequence.remove(0), sequence[action_start - 1..].to_vec())
    }

    #[cfg(feature = "python")]
    fn state_actions_batch(
        &self,
        observations: &[&ObservationV56],
    ) -> Vec<(Vec<f32>, Vec<Vec<f32>>)> {
        observations
            .par_iter()
            .map(|observation| self.state_actions_uncached(observation))
            .collect()
    }

    fn state_actions(
        &self,
        observation: &ObservationV56,
        _cache: &mut EncodingCache,
    ) -> (Vec<f32>, Vec<Vec<f32>>) {
        self.state_actions_uncached(observation)
    }

    fn state(&self, observation: &ObservationV56) -> Vec<f32> {
        self.state_actions_uncached(observation).0
    }

    fn evaluate(
        &self,
        observation: &ObservationV56,
        temperature: f32,
        _cache: &mut EncodingCache,
    ) -> io::Result<(Vec<f32>, f32, f32, Vec<f32>)> {
        let policy = self
            .policy
            .as_ref()
            .ok_or_else(|| invalid("value model has no actor head"))?;
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(invalid("invalid policy temperature"));
        }
        let (state, actions) = self.state_actions_uncached(observation);
        let raw = actions
            .iter()
            .map(|action| policy.apply(action)[0] / temperature)
            .collect::<Vec<_>>();
        let normalizer = log_sum_exp(&raw);
        let scores = raw.into_iter().map(|score| score - normalizer).collect();
        let probabilities = softmax(&self.critic.apply(&state));
        let expected = probabilities
            .iter()
            .enumerate()
            .map(|(i, p)| i as f32 * p)
            .sum::<f32>()
            / (VALUE_CATEGORIES - 1) as f32;
        Ok((
            scores,
            probabilities[VALUE_CATEGORIES - 1],
            expected,
            probabilities,
        ))
    }

    fn evaluate_batch(
        &self,
        observations: &[&ObservationV56],
        features: &[(Vec<f32>, Vec<Vec<f32>>)],
        temperature: f32,
        rollout_temperature: Option<f32>,
        values: bool,
    ) -> io::Result<Vec<(Vec<f32>, f32, f32, Vec<f32>, Option<Vec<f32>>)>> {
        let policy = self
            .policy
            .as_ref()
            .ok_or_else(|| invalid("value model has no actor head"))?;
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(invalid("invalid policy temperature"));
        }
        Ok(observations
            .iter()
            .zip(features)
            .map(|(_observation, (state, actions))| {
                let scores = |temperature: f32| {
                    let raw = actions
                        .iter()
                        .map(|action| policy.apply(action)[0] / temperature)
                        .collect::<Vec<_>>();
                    let normalizer = log_sum_exp(&raw);
                    raw.into_iter()
                        .map(|score| score - normalizer)
                        .collect::<Vec<_>>()
                };
                let probabilities = values
                    .then(|| softmax(&self.critic.apply(state)))
                    .unwrap_or_default();
                let expected = probabilities
                    .iter()
                    .enumerate()
                    .map(|(i, p)| i as f32 * p)
                    .sum::<f32>()
                    / (VALUE_CATEGORIES - 1) as f32;
                (
                    scores(temperature),
                    probabilities.last().copied().unwrap_or_default(),
                    expected,
                    probabilities,
                    rollout_temperature.map(scores),
                )
            })
            .collect())
    }

    pub fn win_probability(&self, game: &Game, content: &Content) -> f32 {
        match game.phase {
            Phase::Won => return 1.0,
            Phase::Dead => return 0.0,
            _ => {}
        }
        let observation = observation_v56(game, content, self.layout, (0, 0));
        let logits = self.critic.apply(&self.state(&observation));
        let logit = logits[VALUE_CATEGORIES - 1] - log_sum_exp(&logits[..VALUE_CATEGORIES - 1]);
        sigmoid(logit / self.temperature + self.bias)
    }
}

fn transformer_pool(index: usize, mode: u8) -> bool {
    match index {
        0..=7 | 11 | 12 => mode == 1,
        8 | 9 => matches!(mode, 1 | 3),
        10 => mode == 1,
        _ => false,
    }
}

fn valid_pooling(pooling: [u8; 13]) -> bool {
    pooling[..8].iter().all(|&mode| mode <= 2)
        && pooling[8..10].iter().all(|&mode| mode <= 4)
        && pooling[10] <= 3
        && pooling[11..].iter().all(|&mode| mode <= 1)
}

fn linear(input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
    if input.is_empty() {
        return bias.to_vec();
    }
    #[cfg(target_os = "macos")]
    {
        #[link(name = "Accelerate", kind = "framework")]
        unsafe extern "C" {
            fn cblas_sgemv(
                order: i32,
                transpose: i32,
                rows: i32,
                columns: i32,
                alpha: f32,
                matrix: *const f32,
                stride: i32,
                input: *const f32,
                input_stride: i32,
                beta: f32,
                output: *mut f32,
                output_stride: i32,
            );
        }
        let mut output = bias.to_vec();
        unsafe {
            cblas_sgemv(
                101,
                111,
                bias.len() as i32,
                input.len() as i32,
                1.0,
                weights.as_ptr(),
                input.len() as i32,
                input.as_ptr(),
                1,
                1.0,
                output.as_mut_ptr(),
                1,
            );
        }
        return output;
    }
    #[cfg(not(target_os = "macos"))]
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

fn dense_gelu(input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
    linear(input, weights, bias)
        .into_iter()
        .map(|value| {
            let x = value / 2.0f32.sqrt();
            let sign = x.signum();
            let x = x.abs();
            let t = 1.0 / (1.0 + 0.3275911 * x);
            let erf = sign
                * (1.0
                    - (((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736)
                        * t
                        + 0.254829592)
                        * t
                        * (-x * x).exp()));
            value * 0.5 * (1.0 + erf)
        })
        .collect()
}

fn normalized(input: &[f32], weight: &[f32], bias: &[f32]) -> Vec<f32> {
    let mut out = input.to_vec();
    layer_norm(&mut out, weight, bias);
    out
}

fn layer_norm(input: &mut [f32], weight: &[f32], bias: &[f32]) {
    normalize_block(input);
    for ((value, weight), bias) in input.iter_mut().zip(weight).zip(bias) {
        *value = *value * weight + bias;
    }
}

fn normalize_block(input: &mut [f32]) {
    let mean = input.iter().sum::<f32>() / input.len() as f32;
    let variance = input
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / input.len() as f32;
    let scale = (variance + 1e-5).sqrt().recip();
    input
        .iter_mut()
        .for_each(|value| *value = (*value - mean) * scale);
}

fn sigmoid(value: f32) -> f32 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn log_sum_exp(values: &[f32]) -> f32 {
    let maximum = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    maximum
        + values
            .iter()
            .map(|value| (value - maximum).exp())
            .sum::<f32>()
            .ln()
}

fn softmax(values: &[f32]) -> Vec<f32> {
    let normalizer = log_sum_exp(values);
    values
        .iter()
        .map(|value| (value - normalizer).exp())
        .collect()
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
    use numpy::{IntoPyArray, PyArray1, PyArray2, PyReadonlyArray1, PyReadonlyArray2, ndarray};
    use pyo3::{
        exceptions::PyValueError,
        prelude::*,
        pybacked::PyBackedBytes,
        types::{PyBytes, PyList, PyTuple},
    };
    use std::collections::{HashMap, HashSet};
    use std::fmt;
    use std::hash::{BuildHasherDefault, Hasher};
    use std::path::Path;
    use tracing::{Event, Level, Subscriber};
    use tracing_subscriber::{
        filter::filter_fn,
        fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
        prelude::*,
        registry::LookupSpan,
    };

    #[derive(Clone)]
    struct GlogFormat {
        role: String,
    }

    fn native_thread_id() -> u64 {
        #[cfg(target_os = "macos")]
        {
            let mut id = 0;
            unsafe { libc::pthread_threadid_np(0, &mut id) };
            id
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&std::thread::current().id(), &mut hash);
            hash.finish()
        }
    }

    impl<S, N> FormatEvent<S, N> for GlogFormat
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        N: for<'a> FormatFields<'a> + 'static,
    {
        fn format_event(
            &self,
            context: &FmtContext<'_, S, N>,
            mut writer: Writer<'_>,
            event: &Event<'_>,
        ) -> fmt::Result {
            let metadata = event.metadata();
            let severity = match *metadata.level() {
                Level::TRACE => 'D',
                Level::DEBUG => 'D',
                Level::INFO => 'I',
                Level::WARN => 'W',
                Level::ERROR => 'E',
            };
            let mut now = unsafe { std::mem::zeroed::<libc::timeval>() };
            let mut local = unsafe { std::mem::zeroed::<libc::tm>() };
            unsafe {
                libc::gettimeofday(&mut now, std::ptr::null_mut());
                libc::localtime_r(&now.tv_sec, &mut local);
            }
            let thread = std::thread::current();
            let rayon_name = rayon::current_thread_index().map(|index| format!("rayon-{index}"));
            let thread_name = rayon_name.as_deref().or(thread.name()).unwrap_or("unnamed");
            let file = metadata
                .file()
                .and_then(|file| Path::new(file).file_name())
                .and_then(|file| file.to_str())
                .unwrap_or("unknown");
            write!(
                writer,
                "{severity}{:02}{:02} {:02}:{:02}:{:02}.{:06} {:06} {:08} {:<12} {:<16} {:>20}:{:05}] ",
                local.tm_mon + 1,
                local.tm_mday,
                local.tm_hour,
                local.tm_min,
                local.tm_sec,
                now.tv_usec,
                std::process::id(),
                native_thread_id(),
                self.role,
                thread_name,
                file,
                metadata.line().unwrap_or(0),
            )?;
            context
                .field_format()
                .format_fields(writer.by_ref(), event)?;
            writeln!(writer)
        }
    }

    fn level_enabled(level: &Level, configured: Level) -> bool {
        let rank = |level: &Level| match *level {
            Level::ERROR => 0,
            Level::WARN => 1,
            Level::INFO => 2,
            Level::DEBUG => 3,
            Level::TRACE => 4,
        };
        rank(level) <= rank(&configured)
    }

    #[pyfunction]
    fn configure_logging(role: String, level: &str) -> PyResult<()> {
        let configured = match level.to_ascii_uppercase().as_str() {
            "ERROR" => Level::ERROR,
            "WARNING" | "WARN" => Level::WARN,
            "INFO" => Level::INFO,
            "DEBUG" => Level::DEBUG,
            other => return Err(PyValueError::new_err(format!("invalid log level {other}"))),
        };
        let stdout = tracing_subscriber::fmt::layer()
            .event_format(GlogFormat { role: role.clone() })
            .with_ansi(false)
            .with_writer(std::io::stdout)
            .with_filter(filter_fn(move |metadata| {
                level_enabled(metadata.level(), configured)
                    && matches!(*metadata.level(), Level::TRACE | Level::DEBUG | Level::INFO)
            }));
        let stderr = tracing_subscriber::fmt::layer()
            .event_format(GlogFormat { role })
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .with_filter(filter_fn(move |metadata| {
                level_enabled(metadata.level(), configured)
                    && matches!(*metadata.level(), Level::WARN | Level::ERROR)
            }));
        tracing_subscriber::registry()
            .with(stdout)
            .with(stderr)
            .try_init()
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    struct FastHasher(u64);

    impl Default for FastHasher {
        fn default() -> Self {
            Self(0x517cc1b727220a95)
        }
    }

    impl Hasher for FastHasher {
        fn finish(&self) -> u64 {
            self.0
        }

        fn write(&mut self, bytes: &[u8]) {
            let mut chunks = bytes.chunks_exact(8);
            for chunk in &mut chunks {
                self.write_u64(u64::from_ne_bytes(chunk.try_into().unwrap()));
            }
            for &byte in chunks.remainder() {
                self.write_u8(byte);
            }
        }

        fn write_u8(&mut self, value: u8) {
            self.write_u64(value as u64);
        }

        fn write_u32(&mut self, value: u32) {
            self.write_u64(value as u64);
        }

        fn write_u64(&mut self, value: u64) {
            self.0 ^= value.wrapping_add(0x9e3779b97f4a7c15);
            self.0 = self.0.rotate_left(27).wrapping_mul(0x3c79ac492ba7b653);
        }

        fn write_usize(&mut self, value: usize) {
            self.write_u64(value as u64);
        }
    }

    type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

    fn compress_words(values: &[u32]) -> Vec<u8> {
        let bitmap = values.len().div_ceil(8);
        let mut output = vec![0; 4 + bitmap];
        output[..4].copy_from_slice(&(values.len() as u32).to_le_bytes());
        for (index, &value) in values.iter().enumerate() {
            if value != 0 {
                output[4 + index / 8] |= 1 << (index % 8);
                output.extend_from_slice(&value.to_le_bytes());
            }
        }
        output
    }

    #[cfg(target_os = "macos")]
    fn compress_bytes(input: &[u8]) -> Vec<u8> {
        #[link(name = "compression")]
        unsafe extern "C" {
            fn compression_encode_buffer(
                output: *mut u8,
                output_size: usize,
                input: *const u8,
                input_size: usize,
                scratch: *mut std::ffi::c_void,
                algorithm: u32,
            ) -> usize;
        }
        let mut output = vec![0; input.len() + input.len() / 255 + 16];
        let size = unsafe {
            compression_encode_buffer(
                output.as_mut_ptr(),
                output.len(),
                input.as_ptr(),
                input.len(),
                std::ptr::null_mut(),
                0x100,
            )
        };
        assert!(size > 0);
        output.truncate(size);
        output
    }

    #[cfg(target_os = "macos")]
    fn decompress_bytes(input: &[u8], size: usize) -> PyResult<Vec<u8>> {
        #[link(name = "compression")]
        unsafe extern "C" {
            fn compression_decode_buffer(
                output: *mut u8,
                output_size: usize,
                input: *const u8,
                input_size: usize,
                scratch: *mut std::ffi::c_void,
                algorithm: u32,
            ) -> usize;
        }
        let mut output = vec![0; size];
        let written = unsafe {
            compression_decode_buffer(
                output.as_mut_ptr(),
                output.len(),
                input.as_ptr(),
                input.len(),
                std::ptr::null_mut(),
                0x100,
            )
        };
        if written != size {
            return Err(PyValueError::new_err("invalid compressed observation"));
        }
        Ok(output)
    }

    fn compact_packed_observation(row: &ObservationV56) -> Vec<u8> {
        compact_packed_observation_known(row, None)
    }

    fn compact_packed_observation_known(row: &ObservationV56, digest: Option<u64>) -> Vec<u8> {
        compact_packed_observation_and_digest(row, digest).0
    }

    fn compact_packed_observation_and_digest(
        row: &ObservationV56,
        digest: Option<u64>,
    ) -> (Vec<u8>, u64) {
        let (globals, counts, exact, actions, digest) = packed_observation_known(row, digest);
        let exact = compress_words(&exact);
        let exact_len = exact.len();
        let width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
        let action_count = actions.len() / width;
        let legal_count = actions
            .chunks_exact(width)
            .filter(|row| row[width - 1] != 0)
            .count();
        let mut payload = exact;
        actions
            .iter()
            .for_each(|value| payload.extend(value.to_le_bytes()));
        #[cfg(target_os = "macos")]
        let payload = compress_bytes(&payload);
        let mut output =
            Vec::with_capacity(32 + 4 * (globals.len() + counts.len()) + payload.len());
        output.extend(if cfg!(target_os = "macos") {
            b"SP68"
        } else {
            b"SP67"
        });
        output.push(row.character);
        output.extend([0; 3]);
        output.extend(digest.to_le_bytes());
        output.extend((action_count as u32).to_le_bytes());
        output.extend((legal_count as u32).to_le_bytes());
        output.extend((exact_len as u32).to_le_bytes());
        output.extend(0u32.to_le_bytes());
        globals
            .iter()
            .for_each(|value| output.extend(value.to_bits().to_le_bytes()));
        counts
            .iter()
            .for_each(|value| output.extend(value.to_le_bytes()));
        output.extend(payload);
        (output, digest)
    }

    fn decompress_words(input: &[u8], expected: usize) -> PyResult<Vec<u32>> {
        if input.len() < 4 {
            return Err(PyValueError::new_err("invalid packed word stream"));
        }
        let words = u32::from_le_bytes(input[..4].try_into().unwrap()) as usize;
        let bitmap = words.div_ceil(8);
        if words != expected || input.len() < 4 + bitmap {
            return Err(PyValueError::new_err("invalid packed word stream"));
        }
        let mut output = vec![0; words];
        let mut offset = 4 + bitmap;
        for (index, value) in output.iter_mut().enumerate() {
            if input[4 + index / 8] & (1 << (index % 8)) != 0 {
                let word = input
                    .get(offset..offset + 4)
                    .ok_or_else(|| PyValueError::new_err("invalid packed word stream"))?;
                *value = u32::from_le_bytes(word.try_into().unwrap());
                offset += 4;
            }
        }
        if offset != input.len() {
            return Err(PyValueError::new_err("invalid packed word stream"));
        }
        Ok(output)
    }

    struct PackedData {
        character: u8,
        globals: Vec<f32>,
        counts: Vec<u32>,
        exact: Vec<u32>,
        actions: Vec<u32>,
        digest: u64,
    }

    fn compact_data(input: &[u8]) -> PyResult<PackedData> {
        let width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
        let fixed = 32 + 4 * (PUBLIC_GLOBALS + DOMAIN_WIDTHS.len());
        if input.len() < fixed || !matches!(&input[..4], b"SP67" | b"SP68") {
            return Err(PyValueError::new_err("invalid compact observation"));
        }
        let digest = u64::from_le_bytes(input[8..16].try_into().unwrap());
        let actions = u32::from_le_bytes(input[16..20].try_into().unwrap()) as usize;
        let exact_bytes = u32::from_le_bytes(input[24..28].try_into().unwrap()) as usize;
        let mut offset = 32;
        let globals = input[offset..offset + 4 * PUBLIC_GLOBALS]
            .chunks_exact(4)
            .map(|value| f32::from_bits(u32::from_le_bytes(value.try_into().unwrap())))
            .collect::<Vec<_>>();
        offset += 4 * PUBLIC_GLOBALS;
        let counts = input[offset..offset + 4 * DOMAIN_WIDTHS.len()]
            .chunks_exact(4)
            .map(|value| u32::from_le_bytes(value.try_into().unwrap()))
            .collect::<Vec<_>>();
        offset += 4 * DOMAIN_WIDTHS.len();
        let action_bytes = actions * width * 4;
        #[cfg(target_os = "macos")]
        let payload = if &input[..4] == b"SP68" {
            decompress_bytes(&input[offset..], exact_bytes + action_bytes)?
        } else {
            input[offset..].to_vec()
        };
        #[cfg(not(target_os = "macos"))]
        let payload = input[offset..].to_vec();
        if payload.len() != exact_bytes + action_bytes {
            return Err(PyValueError::new_err("invalid compact observation length"));
        }
        let expected = counts
            .iter()
            .zip(DOMAIN_WIDTHS)
            .map(|(&count, (u, s, c, f))| count as usize * (u + s + c + f + 1))
            .sum();
        let exact = decompress_words(&payload[..exact_bytes], expected)?;
        let actions = payload[exact_bytes..]
            .chunks_exact(4)
            .map(|value| u32::from_le_bytes(value.try_into().unwrap()))
            .collect();
        Ok(PackedData {
            character: input[4],
            globals,
            counts,
            exact,
            actions,
            digest,
        })
    }

    fn packed_data(row: &Bound<'_, PyAny>) -> PyResult<PackedData> {
        if let Ok(row) = row.downcast::<PyBytes>() {
            return compact_data(row.as_bytes());
        }
        let row = row.downcast::<PyTuple>()?;
        let character = row.get_item(0)?.extract()?;
        let globals = row
            .get_item(1)?
            .extract::<PyReadonlyArray1<'_, f32>>()?
            .as_slice()?
            .to_vec();
        let counts = row
            .get_item(2)?
            .extract::<PyReadonlyArray1<'_, u32>>()?
            .as_slice()?
            .to_vec();
        let expected = counts
            .iter()
            .zip(DOMAIN_WIDTHS)
            .map(|(&count, (u, s, c, f))| count as usize * (u + s + c + f + 1))
            .sum();
        let exact_item = row.get_item(3)?;
        let exact = if let Ok(exact) = exact_item.downcast::<PyBytes>() {
            decompress_words(exact.as_bytes(), expected)?
        } else {
            exact_item
                .extract::<PyReadonlyArray1<'_, u32>>()?
                .as_slice()?
                .to_vec()
        };
        let actions = row
            .get_item(4)?
            .extract::<PyReadonlyArray2<'_, u32>>()?
            .as_slice()?
            .to_vec();
        Ok(PackedData {
            character,
            globals,
            counts,
            exact,
            actions,
            digest: row.get_item(5)?.extract()?,
        })
    }

    #[pyfunction]
    fn unique_rows<'py>(
        py: Python<'py>,
        values: PyReadonlyArray2<'py, u32>,
    ) -> (
        Bound<'py, numpy::PyArray1<i64>>,
        Bound<'py, numpy::PyArray1<i64>>,
    ) {
        let values = values.as_array();
        if values.nrows() == 0 {
            return (Vec::new().into_pyarray(py), Vec::new().into_pyarray(py));
        }
        let columns = values.ncols();
        let flat = values.as_slice().unwrap();
        let mut unique =
            FastMap::with_capacity_and_hasher(values.nrows(), BuildHasherDefault::default());
        let mut first = Vec::new();
        let mut inverse = Vec::with_capacity(values.nrows());
        for (index, row) in flat.chunks(columns).enumerate() {
            let next = unique.len();
            let value = *unique.entry(row).or_insert_with(|| {
                first.push(index as i64);
                next
            });
            inverse.push(value as i64);
        }
        (first.into_pyarray(py), inverse.into_pyarray(py))
    }

    #[pyfunction]
    fn unique_feature_rows<'py>(
        py: Python<'py>,
        semantic: PyReadonlyArray2<'py, u32>,
        numeric: PyReadonlyArray2<'py, u32>,
    ) -> PyResult<(
        Bound<'py, numpy::PyArray1<i64>>,
        Bound<'py, numpy::PyArray1<i64>>,
    )> {
        let semantic = semantic.as_array();
        let numeric = numeric.as_array();
        if semantic.nrows() != numeric.nrows() {
            return Err(PyValueError::new_err("feature row count mismatch"));
        }
        if semantic.nrows() == 0 {
            return Ok((Vec::new().into_pyarray(py), Vec::new().into_pyarray(py)));
        }
        let rows = semantic.nrows();
        let semantic = semantic
            .as_slice()
            .ok_or_else(|| PyValueError::new_err("semantic rows must be contiguous"))?;
        let numeric = numeric
            .as_slice()
            .ok_or_else(|| PyValueError::new_err("numeric rows must be contiguous"))?;
        let semantic_width = semantic.len() / rows;
        let numeric_width = numeric.len() / rows;
        let mut unique = FastMap::with_capacity_and_hasher(rows, BuildHasherDefault::default());
        let mut first = Vec::new();
        let mut inverse = Vec::with_capacity(rows);
        for (index, (semantic, numeric)) in semantic
            .chunks(semantic_width)
            .zip(numeric.chunks(numeric_width))
            .enumerate()
        {
            let next = unique.len();
            let value = *unique.entry((semantic, numeric)).or_insert_with(|| {
                first.push(index as i64);
                next
            });
            inverse.push(value as i64);
        }
        Ok((first.into_pyarray(py), inverse.into_pyarray(py)))
    }

    #[pyfunction]
    #[allow(clippy::too_many_arguments)]
    fn unique_graphs<'py>(
        py: Python<'py>,
        node_u: PyReadonlyArray2<'py, u32>,
        node_c: PyReadonlyArray2<'py, u32>,
        node_f: PyReadonlyArray2<'py, f32>,
        node_source: PyReadonlyArray1<'py, i32>,
        node_offsets: PyReadonlyArray1<'py, i64>,
        edge_u: PyReadonlyArray2<'py, u32>,
        edge_c: PyReadonlyArray2<'py, u32>,
        edge_f: PyReadonlyArray2<'py, f32>,
        edge_source: PyReadonlyArray1<'py, i32>,
        edge_offsets: PyReadonlyArray1<'py, i64>,
    ) -> PyResult<(
        Bound<'py, numpy::PyArray1<i64>>,
        Bound<'py, numpy::PyArray1<i64>>,
    )> {
        let (node_u, node_c, node_f) = (node_u.as_array(), node_c.as_array(), node_f.as_array());
        let (edge_u, edge_c, edge_f) = (edge_u.as_array(), edge_c.as_array(), edge_f.as_array());
        let node_source = node_source.as_slice()?;
        let edge_source = edge_source.as_slice()?;
        let node_offsets = node_offsets.as_slice()?;
        let edge_offsets = edge_offsets.as_slice()?;
        if node_offsets.len() != edge_offsets.len() {
            return Err(PyValueError::new_err("graph row count mismatch"));
        }
        let rows = node_offsets.len().saturating_sub(1);
        let mut unique = FastMap::with_capacity_and_hasher(rows, BuildHasherDefault::default());
        let mut first = Vec::new();
        let mut inverse = Vec::with_capacity(rows);
        for row in 0..rows {
            let node_range = node_offsets[row] as usize..node_offsets[row + 1] as usize;
            let edge_range = edge_offsets[row] as usize..edge_offsets[row + 1] as usize;
            let mut key = Vec::with_capacity(
                2 + node_range.len() * (node_u.ncols() + node_c.ncols() + node_f.ncols())
                    + edge_range.len() * (edge_u.ncols() + edge_c.ncols() + edge_f.ncols()),
            );
            key.push(node_range.len() as u32);
            for &source in &node_source[node_range] {
                let source = source as usize;
                key.extend(node_u.row(source));
                key.extend(node_c.row(source));
                key.extend(node_f.row(source).iter().map(|value| value.to_bits()));
            }
            key.push(edge_range.len() as u32);
            for &source in &edge_source[edge_range] {
                let source = source as usize;
                key.extend(edge_u.row(source));
                key.extend(edge_c.row(source));
                key.extend(edge_f.row(source).iter().map(|value| value.to_bits()));
            }
            let next = unique.len();
            let value = *unique.entry(key).or_insert_with(|| {
                first.push(row as i64);
                next
            });
            inverse.push(value as i64);
        }
        Ok((first.into_pyarray(py), inverse.into_pyarray(py)))
    }

    fn check_action_features(unsigned: &[u32], signed: &[i32], numeric: &[f32]) -> PyResult<()> {
        let unsigned: &[u32; ACTION_U] = unsigned
            .try_into()
            .map_err(|_| PyValueError::new_err("invalid action unsigned width"))?;
        let signed: &[i32; ACTION_S] = signed
            .try_into()
            .map_err(|_| PyValueError::new_err("invalid action signed width"))?;
        let expected = action_features(unsigned, signed);
        if numeric.len() != ACTION_F
            || numeric
                .iter()
                .zip(expected)
                .any(|(actual, expected)| actual.to_bits() != expected.to_bits())
        {
            return Err(PyValueError::new_err("action auxiliary mismatch"));
        }
        Ok(())
    }

    #[pyfunction]
    fn validate_action_features(
        unsigned: PyReadonlyArray2<'_, u32>,
        signed: PyReadonlyArray2<'_, i32>,
        numeric: PyReadonlyArray2<'_, f32>,
    ) -> PyResult<()> {
        let (unsigned, signed, numeric) =
            (unsigned.as_array(), signed.as_array(), numeric.as_array());
        if unsigned.nrows() != signed.nrows() || unsigned.nrows() != numeric.nrows() {
            return Err(PyValueError::new_err("action row count mismatch"));
        }
        for row in 0..unsigned.nrows() {
            check_action_features(
                unsigned.row(row).as_slice().unwrap(),
                signed.row(row).as_slice().unwrap(),
                numeric.row(row).as_slice().unwrap(),
            )?;
        }
        Ok(())
    }

    #[pyfunction]
    fn compress_packed_observations<'py>(
        py: Python<'py>,
        rows: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let mut output = Vec::with_capacity(rows.len());
        for item in rows.iter() {
            let row = item.downcast::<PyTuple>()?;
            let exact = row.get_item(3)?.extract::<PyReadonlyArray1<'_, u32>>()?;
            let compressed = compress_words(exact.as_slice()?);
            output.push(PyTuple::new(
                py,
                [
                    row.get_item(0)?,
                    row.get_item(1)?,
                    row.get_item(2)?,
                    PyBytes::new(py, &compressed).into_any(),
                    row.get_item(4)?,
                    row.get_item(5)?,
                ],
            )?);
        }
        PyTuple::new(py, output)
    }

    #[pyfunction]
    fn validate_packed_observation(
        character: u8,
        globals: PyReadonlyArray1<'_, f32>,
        counts: PyReadonlyArray1<'_, u32>,
        exact: &Bound<'_, PyAny>,
        actions: PyReadonlyArray2<'_, u32>,
        digest: u64,
    ) -> PyResult<()> {
        let globals = globals
            .as_slice()
            .map_err(|_| PyValueError::new_err("noncontiguous public globals"))?;
        let counts = counts
            .as_slice()
            .map_err(|_| PyValueError::new_err("noncontiguous domain counts"))?;
        let expected = counts
            .iter()
            .zip(DOMAIN_WIDTHS)
            .map(|(&count, (u, s, c, f))| count as usize * (u + s + c + f + 1))
            .sum::<usize>();
        let exact_array: PyReadonlyArray1<'_, u32>;
        let exact_owned: Vec<u32>;
        let exact = if let Ok(bytes) = exact.downcast::<PyBytes>() {
            exact_owned = decompress_words(bytes.as_bytes(), expected)?;
            exact_owned.as_slice()
        } else {
            exact_array = exact.extract()?;
            exact_array
                .as_slice()
                .map_err(|_| PyValueError::new_err("noncontiguous domain rows"))?
        };
        let actions = actions.as_array();
        let action_width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
        if globals.len() != PUBLIC_GLOBALS
            || !globals.iter().all(|value| value.is_finite())
            || counts.len() != DOMAIN_WIDTHS.len()
            || exact.len() != expected
            || actions.ncols() != action_width
        {
            return Err(PyValueError::new_err("invalid packed observation schema"));
        }
        let action_words = actions
            .as_slice()
            .ok_or_else(|| PyValueError::new_err("noncontiguous action rows"))?;
        if packed_observation_digest(character, globals, counts, exact, action_words) != digest {
            return Err(PyValueError::new_err(
                "observation auxiliary digest mismatch",
            ));
        }
        for row in actions.rows() {
            let row = row.as_slice().unwrap();
            let signed = row[ACTION_U..ACTION_U + ACTION_S]
                .iter()
                .map(|&value| value as i32)
                .collect::<Vec<_>>();
            let numeric = row
                [ACTION_U + ACTION_S + ACTION_C..ACTION_U + ACTION_S + ACTION_C + ACTION_F]
                .iter()
                .map(|&value| f32::from_bits(value))
                .collect::<Vec<_>>();
            check_action_features(&row[..ACTION_U], &signed, &numeric)?;
            if row[action_width - 1] > 1 {
                return Err(PyValueError::new_err("invalid action legality"));
            }
        }
        Ok(())
    }

    #[pyfunction]
    fn validate_compact_observation(row: &Bound<'_, PyBytes>) -> PyResult<()> {
        let row = compact_data(row.as_bytes())?;
        let width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
        if packed_observation_digest(
            row.character,
            &row.globals,
            &row.counts,
            &row.exact,
            &row.actions,
        ) != row.digest
        {
            return Err(PyValueError::new_err(
                "observation auxiliary digest mismatch",
            ));
        }
        for action in row.actions.chunks_exact(width) {
            let signed = action[ACTION_U..ACTION_U + ACTION_S]
                .iter()
                .map(|&value| value as i32)
                .collect::<Vec<_>>();
            let numeric = action
                [ACTION_U + ACTION_S + ACTION_C..ACTION_U + ACTION_S + ACTION_C + ACTION_F]
                .iter()
                .map(|&value| f32::from_bits(value))
                .collect::<Vec<_>>();
            check_action_features(&action[..ACTION_U], &signed, &numeric)?;
            if action[width - 1] > 1 {
                return Err(PyValueError::new_err("invalid action legality"));
            }
        }
        Ok(())
    }

    type DomainArrays<'py> = (
        Bound<'py, PyArray2<u32>>,
        Bound<'py, PyArray2<i32>>,
        Bound<'py, PyArray2<u32>>,
        Bound<'py, PyArray2<f32>>,
        Bound<'py, PyArray1<i32>>,
        Bound<'py, PyArray1<i32>>,
    );

    #[pyfunction]
    fn unpack_packed_observations<'py>(
        py: Python<'py>,
        rows: &Bound<'py, PyList>,
    ) -> PyResult<(
        Bound<'py, PyArray1<u8>>,
        Bound<'py, PyArray2<f32>>,
        Vec<DomainArrays<'py>>,
        (
            Bound<'py, PyArray2<u32>>,
            Bound<'py, PyArray2<i32>>,
            Bound<'py, PyArray2<u32>>,
            Bound<'py, PyArray2<f32>>,
        ),
        Bound<'py, PyArray1<i32>>,
        Bound<'py, PyArray1<i32>>,
        Bound<'py, PyArray2<bool>>,
    )> {
        let batch = rows.len();
        let compact = rows
            .iter()
            .map(|row| row.extract::<PyBackedBytes>())
            .collect::<PyResult<Vec<_>>>();
        let packed = match compact {
            Ok(rows) => py.allow_threads(|| {
                rows.par_iter()
                    .map(|row| compact_data(row))
                    .collect::<PyResult<Vec<_>>>()
            })?,
            Err(_) => rows
                .iter()
                .map(|row| packed_data(&row))
                .collect::<PyResult<Vec<_>>>()?,
        };
        let mut characters = Vec::with_capacity(batch);
        let mut globals = Vec::with_capacity(batch * PUBLIC_GLOBALS);
        let mut lengths = Vec::with_capacity(batch);
        let mut domain_rows = [0; DOMAIN_WIDTHS.len()];
        for row in &packed {
            characters.push(row.character);
            if row.globals.len() != PUBLIC_GLOBALS {
                return Err(PyValueError::new_err("invalid packed globals"));
            }
            globals.extend_from_slice(&row.globals);
            let width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
            if row.actions.len() % width != 0 {
                return Err(PyValueError::new_err("invalid packed action width"));
            }
            lengths.push(row.actions.len() / width);
            if row.counts.len() != DOMAIN_WIDTHS.len() {
                return Err(PyValueError::new_err("invalid packed domain count"));
            }
            for (total, &count) in domain_rows.iter_mut().zip(&row.counts) {
                *total += count as usize;
            }
        }
        let max_actions = lengths.iter().copied().max().unwrap_or(1).max(1);
        let total_actions: usize = lengths.iter().sum();
        let mut unsigned = DOMAIN_WIDTHS
            .iter()
            .enumerate()
            .map(|(domain, &(width, ..))| Vec::with_capacity(domain_rows[domain] * width))
            .collect::<Vec<Vec<u32>>>();
        let mut signed = DOMAIN_WIDTHS
            .iter()
            .enumerate()
            .map(|(domain, &(_, width, ..))| Vec::with_capacity(domain_rows[domain] * width))
            .collect::<Vec<Vec<i32>>>();
        let mut semantic = DOMAIN_WIDTHS
            .iter()
            .enumerate()
            .map(|(domain, &(_, _, width, _))| Vec::with_capacity(domain_rows[domain] * width))
            .collect::<Vec<Vec<u32>>>();
        let mut numeric = DOMAIN_WIDTHS
            .iter()
            .enumerate()
            .map(|(domain, &(_, _, _, width))| Vec::with_capacity(domain_rows[domain] * width))
            .collect::<Vec<Vec<f32>>>();
        let mut row_index = domain_rows.map(Vec::with_capacity).to_vec();
        let mut scope = domain_rows.map(Vec::with_capacity).to_vec();
        let mut action_u = Vec::with_capacity(total_actions * ACTION_U);
        let mut action_s = Vec::with_capacity(total_actions * ACTION_S);
        let mut action_c = Vec::with_capacity(total_actions * ACTION_C);
        let mut action_f = Vec::with_capacity(total_actions * ACTION_F);
        let mut action_row = Vec::with_capacity(total_actions);
        let mut action_position = Vec::with_capacity(total_actions);
        let mut legal = vec![false; batch * max_actions];
        let mut action_offset = 0;
        for (batch_index, row) in packed.iter().enumerate() {
            let mut offset = 0;
            for (domain, (&count, &(u, s, c, f))) in
                row.counts.iter().zip(DOMAIN_WIDTHS.iter()).enumerate()
            {
                let width = u + s + c + f + 1;
                for record in row.exact[offset..offset + count as usize * width].chunks_exact(width)
                {
                    unsigned[domain].extend_from_slice(&record[..u]);
                    signed[domain].extend(record[u..u + s].iter().map(|&value| value as i32));
                    semantic[domain].extend_from_slice(&record[u + s..u + s + c]);
                    numeric[domain].extend(
                        record[u + s + c..u + s + c + f]
                            .iter()
                            .map(|&value| f32::from_bits(value)),
                    );
                    row_index[domain].push(batch_index as i32);
                    let value = record[width - 1] as i32;
                    scope[domain].push(if value < 0 {
                        value
                    } else {
                        value + action_offset as i32
                    });
                }
                offset += count as usize * width;
            }
            if offset != row.exact.len() {
                return Err(PyValueError::new_err("invalid packed domain rows"));
            }
            let action_width = ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1;
            for (position, action) in row.actions.chunks_exact(action_width).enumerate() {
                action_u.extend_from_slice(&action[..ACTION_U]);
                action_s.extend(
                    action[ACTION_U..ACTION_U + ACTION_S]
                        .iter()
                        .map(|&value| value as i32),
                );
                action_c.extend_from_slice(
                    &action[ACTION_U + ACTION_S..ACTION_U + ACTION_S + ACTION_C],
                );
                action_f.extend(
                    action
                        [ACTION_U + ACTION_S + ACTION_C..ACTION_U + ACTION_S + ACTION_C + ACTION_F]
                        .iter()
                        .map(|&value| f32::from_bits(value)),
                );
                action_row.push(batch_index as i32);
                action_position.push(position as i32);
                legal[batch_index * max_actions + position] =
                    action[ACTION_U + ACTION_S + ACTION_C + ACTION_F] != 0;
            }
            action_offset += row.actions.len() / action_width;
        }
        let domains = DOMAIN_WIDTHS
            .iter()
            .enumerate()
            .map(|(domain, &(u, s, c, f))| {
                let rows = row_index[domain].len();
                (
                    ndarray::Array2::from_shape_vec(
                        (rows, u),
                        std::mem::take(&mut unsigned[domain]),
                    )
                    .unwrap()
                    .into_pyarray(py),
                    ndarray::Array2::from_shape_vec((rows, s), std::mem::take(&mut signed[domain]))
                        .unwrap()
                        .into_pyarray(py),
                    ndarray::Array2::from_shape_vec(
                        (rows, c),
                        std::mem::take(&mut semantic[domain]),
                    )
                    .unwrap()
                    .into_pyarray(py),
                    ndarray::Array2::from_shape_vec(
                        (rows, f),
                        std::mem::take(&mut numeric[domain]),
                    )
                    .unwrap()
                    .into_pyarray(py),
                    std::mem::take(&mut row_index[domain]).into_pyarray(py),
                    std::mem::take(&mut scope[domain]).into_pyarray(py),
                )
            })
            .collect();
        Ok((
            characters.into_pyarray(py),
            ndarray::Array2::from_shape_vec((batch, PUBLIC_GLOBALS), globals)
                .unwrap()
                .into_pyarray(py),
            domains,
            (
                ndarray::Array2::from_shape_vec((total_actions, ACTION_U), action_u)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array2::from_shape_vec((total_actions, ACTION_S), action_s)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array2::from_shape_vec((total_actions, ACTION_C), action_c)
                    .unwrap()
                    .into_pyarray(py),
                ndarray::Array2::from_shape_vec((total_actions, ACTION_F), action_f)
                    .unwrap()
                    .into_pyarray(py),
            ),
            action_row.into_pyarray(py),
            action_position.into_pyarray(py),
            ndarray::Array2::from_shape_vec((batch, max_actions), legal)
                .unwrap()
                .into_pyarray(py),
        ))
    }

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
        policy: Option<ValueModel>,
        searched_turns: Vec<Option<u16>>,
    }

    struct SearchEdge {
        row: CandidateRow,
        candidate: usize,
        occurrence: usize,
        kind_occurrence: usize,
        prior: f32,
        behavior: f32,
        rank: usize,
        visits: u32,
        value_sum: f32,
        children: Vec<(usize, u32)>,
        terminal_visits: u32,
        terminal_value_sum: f32,
        invalid: bool,
    }

    struct SearchNode {
        value: f32,
        value_samples: u32,
        packed: Option<Vec<u8>>,
        action_count: usize,
        depth: usize,
        visits: u32,
        ranked: Vec<usize>,
        edges: Vec<SearchEdge>,
    }

    impl SearchNode {
        fn new(
            candidates: Vec<CandidateRow>,
            log_policy: &[f32],
            value: f32,
            depth: usize,
            behavior_exponent: f32,
            packed: Option<Vec<u8>>,
        ) -> Self {
            let action_count = candidates.len();
            let behavior_normalizer = log_policy
                .iter()
                .filter(|probability| probability.is_finite())
                .map(|probability| (probability * behavior_exponent).exp())
                .sum::<f32>();
            let mut occurrences = FastMap::<u64, usize>::with_capacity_and_hasher(
                candidates.len(),
                BuildHasherDefault::default(),
            );
            let mut kind_counts = [0; ACTION_KINDS];
            let mut edges = Vec::with_capacity(action_count);
            for (candidate, (row, probability)) in candidates
                .into_iter()
                .zip(log_policy.iter().copied())
                .enumerate()
            {
                let occurrence = occurrences
                    .entry(public_candidate_digest(&row))
                    .or_default();
                let current_occurrence = *occurrence;
                *occurrence += 1;
                let kind = action_kind(&row.action);
                let kind_occurrence = kind_counts[kind];
                kind_counts[kind] += usize::from(row.legal);
                if row.legal && probability.is_finite() {
                    edges.push(SearchEdge {
                        row,
                        candidate,
                        occurrence: current_occurrence,
                        kind_occurrence,
                        prior: probability.exp(),
                        behavior: (probability * behavior_exponent).exp() / behavior_normalizer,
                        rank: 0,
                        visits: 0,
                        value_sum: 0.0,
                        children: Vec::new(),
                        terminal_visits: 0,
                        terminal_value_sum: 0.0,
                        invalid: false,
                    });
                }
            }
            let mut order = (0..edges.len()).collect::<Vec<_>>();
            order.sort_by(|&left, &right| edges[right].prior.total_cmp(&edges[left].prior));
            for (rank, &edge) in order.iter().enumerate() {
                edges[edge].rank = rank;
            }
            Self {
                value,
                value_samples: 1,
                packed,
                action_count,
                depth,
                visits: 0,
                ranked: order,
                edges,
            }
        }

        fn select(&self, exploration: f32, lane: usize) -> Option<usize> {
            let lane = lane % 16;
            let mut unvisited = [0; 16];
            let mut unvisited_count = 0;
            for &index in &self.ranked {
                if !self.edges[index].invalid && self.edges[index].visits == 0 {
                    if unvisited_count < unvisited.len() {
                        unvisited[unvisited_count] = index;
                    }
                    unvisited_count += 1;
                }
            }
            if unvisited_count > 0 {
                return Some(unvisited[lane % unvisited_count]);
            }
            let visits = self.visits as f32;
            let root = (visits + lane as f32).sqrt();
            let mut best = None;
            for (index, edge) in self.edges.iter().enumerate() {
                if edge.invalid {
                    continue;
                }
                let virtual_visits =
                    lane / self.edges.len() + usize::from(edge.rank < lane % self.edges.len());
                let score = edge.value_sum / edge.visits as f32
                    + exploration * edge.prior * root
                        / (1 + edge.visits as usize + virtual_visits) as f32;
                if best.is_none_or(|(_, best_score): (usize, f32)| {
                    best_score.total_cmp(&score) != std::cmp::Ordering::Greater
                }) {
                    best = Some((index, score));
                }
            }
            best.map(|(index, _)| index)
        }
    }

    fn search_candidate<'a>(
        observation: &'a ObservationV56,
        edge: &SearchEdge,
    ) -> Option<&'a CandidateRow> {
        if let Some(candidate) = observation.candidates.get(edge.candidate)
            && candidate.legal
            && candidate.action == edge.row.action
            && same_public_candidate(candidate, &edge.row)
        {
            return Some(candidate);
        }
        observation
            .candidates
            .iter()
            .filter(|candidate| candidate.legal && same_public_candidate(candidate, &edge.row))
            .nth(edge.occurrence)
            .or_else(|| {
                observation.candidates.iter().find(|candidate| {
                    candidate.legal && same_public_candidate(candidate, &edge.row)
                })
            })
            .or_else(|| {
                if !matches!(edge.row.action, Action::Choose(_)) {
                    return None;
                }
                let count = observation
                    .candidates
                    .iter()
                    .filter(|candidate| {
                        candidate.legal && matches!(candidate.action, Action::Choose(_))
                    })
                    .count();
                (count > 0).then(|| {
                    observation
                        .candidates
                        .iter()
                        .filter(|candidate| {
                            candidate.legal && matches!(candidate.action, Action::Choose(_))
                        })
                        .nth(edge.kind_occurrence % count)
                        .unwrap()
                })
            })
            .or_else(|| {
                observation
                    .candidates
                    .get(edge.candidate)
                    .filter(|candidate| candidate.legal)
            })
    }

    fn search_terminal_value(game: &Game, progress: bool) -> Option<f32> {
        match game.phase {
            Phase::Won => Some(1.0),
            Phase::Dead if progress => {
                Some(canonical_progress(game) as f32 / (VALUE_CATEGORIES - 1) as f32)
            }
            Phase::Dead => Some(0.0),
            _ => None,
        }
    }

    fn heuristic_combat_value(game: &Game) -> f32 {
        let player = game.combat().map_or_else(
            || game.run.hp.max(0) as f32 / game.run.max_hp.max(1) as f32,
            |combat| combat.player.hp.max(0) as f32 / combat.player.max_hp.max(1) as f32,
        );
        let enemy = game.combat().map_or(0.0, |combat| {
            let hp = combat
                .enemies
                .iter()
                .map(|enemy| enemy.creature.hp.max(0) as i32)
                .sum::<i32>();
            let maximum = combat
                .enemies
                .iter()
                .map(|enemy| enemy.creature.max_hp.max(0) as i32)
                .sum::<i32>();
            hp as f32 / maximum.max(1) as f32
        });
        player - 0.1 * enemy
    }

    struct SearchTree {
        root: Game,
        root_observation: ObservationV56,
        root_digest: u64,
        map: CanonicalMap,
        root_turn: u16,
        simulations: usize,
        budget: usize,
        turns: usize,
        max_depth: usize,
        nodes: Vec<SearchNode>,
        lookup: FastMap<(u64, usize), usize>,
        games: Option<Vec<Game>>,
        progress: bool,
        behavior_exponent: f32,
        pack_visits: u32,
        capture_children: bool,
        heuristic: bool,
    }

    struct SearchLeaf {
        path: SearchPath,
        observation: ObservationV56,
        digest: u64,
        depth: usize,
        game: Option<Game>,
    }

    struct SearchPath {
        steps: Vec<SearchStep>,
        packed: Vec<(Option<usize>, Vec<u8>)>,
    }

    struct SearchStep {
        node: usize,
        edge: usize,
        child: Option<usize>,
    }

    enum SearchResult {
        Value(SearchPath, f32),
        Leaf(SearchLeaf),
        Invalid(SearchPath),
    }

    impl SearchTree {
        fn new(
            root: &Game,
            observation: &ObservationV56,
            content: &Content,
            layout: Layout,
            log_policy: &[f32],
            value: f32,
            budget: usize,
            turns: usize,
            max_depth: usize,
            capture_games: bool,
            progress: bool,
            behavior_exponent: f32,
            pack_visits: u32,
            capture_children: bool,
            heuristic: bool,
        ) -> Self {
            let digest = observation_digest(observation);
            let capacity = budget
                .saturating_mul(if turns == 0 { max_depth.min(16) } else { 1 })
                .saturating_add(1);
            let mut lookup =
                FastMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default());
            lookup.insert((digest, 0), 0);
            let games = capture_games.then(|| vec![root.clone()]);
            let mut root = root.clone();
            canonicalize_combat_hidden(&mut root);
            let root_turn = root.combat().map_or(0, |combat| combat.turn);
            let map = canonical_map(&root, content, layout);
            let mut nodes = Vec::with_capacity(capacity);
            nodes.push(SearchNode::new(
                observation.candidates.clone(),
                log_policy,
                value,
                0,
                behavior_exponent,
                Some(compact_packed_observation_known(observation, Some(digest))),
            ));
            Self {
                root,
                root_observation: observation.clone(),
                root_digest: digest,
                map,
                root_turn,
                simulations: 0,
                budget,
                turns,
                max_depth,
                nodes,
                lookup,
                games,
                progress,
                behavior_exponent,
                pack_visits,
                capture_children,
                heuristic,
            }
        }

        fn horizon(&self, game: &Game) -> bool {
            game.combat().is_none()
                || self.turns > 0
                    && game.combat().is_some_and(|combat| {
                        combat.turn >= self.root_turn.saturating_add(self.turns as u16)
                    })
        }

        fn simulate(
            &self,
            content: &Content,
            layout: Layout,
            bonuses: (i16, i16),
            seed: u64,
            exploration: f32,
            lane: usize,
        ) -> Result<SearchResult, String> {
            let mut game = self.root.clone();
            resample_canonical_combat_hidden(&mut game, seed);
            let mut observation = None;
            let mut node = 0usize;
            let mut path = SearchPath {
                steps: Vec::with_capacity(8),
                packed: Vec::new(),
            };
            for depth in 1..=self.max_depth {
                let (current, current_digest) = observation
                    .as_ref()
                    .map(|(observation, digest)| (observation, *digest))
                    .unwrap_or((&self.root_observation, self.root_digest));
                let node_packed = self.nodes[node].packed.is_some()
                    || path
                        .packed
                        .iter()
                        .any(|(packed_node, _)| *packed_node == Some(node));
                let should_pack = !node_packed
                    && self.nodes[node].visits + (lane % 16) as u32 + 1 >= self.pack_visits;
                let pack_children = self.capture_children && (node_packed || should_pack);
                let Some(edge) =
                    self.nodes[node].select(exploration, lane.wrapping_add(depth * 17))
                else {
                    return Ok(SearchResult::Invalid(path));
                };
                if should_pack {
                    path.packed.push((
                        Some(node),
                        compact_packed_observation_known(current, Some(current_digest)),
                    ));
                }
                let search_edge = &self.nodes[node].edges[edge];
                let candidate = search_candidate(current, search_edge).ok_or_else(|| {
                    format!(
                        "MCTS public action mismatch: wanted {:?}, have {:?}",
                        search_edge.row.action,
                        current
                            .candidates
                            .iter()
                            .filter(|candidate| candidate.legal)
                            .map(|candidate| &candidate.action)
                            .collect::<Vec<_>>()
                    )
                })?;
                let action = candidate.action.clone();
                let was_combat = game.combat().is_some();
                game.step(content, action)
                    .map_err(|error| format!("MCTS step failed: {error:?}"))?;
                if !was_combat && game.combat().is_some() {
                    apply_training_bonuses(&mut game, content, bonuses.0, bonuses.1);
                }
                if matches!(game.phase, Phase::Won | Phase::Dead) {
                    let value = if self.heuristic {
                        heuristic_combat_value(&game)
                    } else {
                        search_terminal_value(&game, self.progress).unwrap()
                    };
                    path.steps.push(SearchStep {
                        node,
                        edge,
                        child: None,
                    });
                    return Ok(SearchResult::Value(path, value));
                }
                let next_observation =
                    observation_v56_with_map(&game, content, layout, bonuses, Some(&self.map));
                let digest = observation_digest(&next_observation);
                let next = self.lookup.get(&(digest, depth)).copied();
                let child_packed = (pack_children
                    && next.is_none_or(|index| self.nodes[index].packed.is_none()))
                .then(|| compact_packed_observation_known(&next_observation, Some(digest)));
                if let Some(packed) = child_packed {
                    path.packed.push((next, packed));
                }
                if self.horizon(&game) {
                    path.steps.push(SearchStep {
                        node,
                        edge,
                        child: next,
                    });
                    return Ok(if let Some(index) = next {
                        SearchResult::Value(path, self.nodes[index].value)
                    } else {
                        SearchResult::Leaf(SearchLeaf {
                            path,
                            observation: next_observation,
                            digest,
                            depth,
                            game: (self.games.is_some() || self.heuristic || self.turns == 0)
                                .then_some(game),
                        })
                    });
                }
                if depth >= self.max_depth {
                    path.steps.push(SearchStep {
                        node,
                        edge,
                        child: next,
                    });
                    return Ok(SearchResult::Invalid(path));
                }
                if let Some(next) = next {
                    path.steps.push(SearchStep {
                        node,
                        edge,
                        child: Some(next),
                    });
                    node = next;
                    observation = Some((next_observation, digest));
                } else {
                    path.steps.push(SearchStep {
                        node,
                        edge,
                        child: None,
                    });
                    return Ok(SearchResult::Leaf(SearchLeaf {
                        path,
                        observation: next_observation,
                        digest,
                        depth,
                        game: (self.games.is_some() || self.heuristic || self.turns == 0)
                            .then_some(game),
                    }));
                }
            }
            unreachable!()
        }

        fn backup(&mut self, path: &mut SearchPath, value: f32) {
            for (node, packed) in path.packed.drain(..) {
                let node = node.expect("unexpanded MCTS packed observation");
                if self.nodes[node].packed.is_none() {
                    self.nodes[node].packed = Some(packed);
                }
            }
            for step in &path.steps {
                self.nodes[step.node].visits += 1;
                let edge = &mut self.nodes[step.node].edges[step.edge];
                edge.visits += 1;
                edge.value_sum += value;
                if let Some(child) = step.child {
                    if let Some((_, count)) = edge
                        .children
                        .iter_mut()
                        .find(|(candidate, _)| *candidate == child)
                    {
                        *count += 1;
                    } else {
                        edge.children.push((child, 1));
                    }
                } else {
                    edge.terminal_visits += 1;
                    edge.terminal_value_sum += value;
                }
            }
            self.simulations += 1;
        }

        fn invalidate(&mut self, path: &mut SearchPath) {
            if let Some(step) = path.steps.last() {
                if self.nodes[step.node].packed.is_none() {
                    if let Some(index) = path
                        .packed
                        .iter()
                        .position(|(node, _)| *node == Some(step.node))
                    {
                        self.nodes[step.node].packed = Some(path.packed.swap_remove(index).1);
                    }
                }
                self.nodes[step.node].edges[step.edge].invalid = true;
            }
            self.simulations += 1;
        }

        fn expand(
            &mut self,
            observation: ObservationV56,
            digest: u64,
            depth: usize,
            mut path: SearchPath,
            mut duplicates: Vec<SearchPath>,
            log_policy: &[f32],
            value: f32,
            game: Option<Game>,
        ) {
            let child = self.insert(observation, digest, depth, log_policy, value, game, None);
            path.steps
                .last_mut()
                .expect("MCTS leaf has an empty path")
                .child = Some(child);
            path.packed
                .iter_mut()
                .filter(|(node, _)| node.is_none())
                .for_each(|(node, _)| *node = Some(child));
            self.backup(&mut path, value);
            for path in &mut duplicates {
                path.steps
                    .last_mut()
                    .expect("MCTS leaf has an empty path")
                    .child = Some(child);
                path.packed
                    .iter_mut()
                    .filter(|(node, _)| node.is_none())
                    .for_each(|(node, _)| *node = Some(child));
                self.backup(path, value);
            }
        }

        fn insert(
            &mut self,
            observation: ObservationV56,
            digest: u64,
            depth: usize,
            log_policy: &[f32],
            value: f32,
            game: Option<Game>,
            packed: Option<Vec<u8>>,
        ) -> usize {
            let key = (digest, depth);
            if let Some(&index) = self.lookup.get(&key) {
                let child = &mut self.nodes[index];
                child.value = (child.value * child.value_samples as f32 + value)
                    / (child.value_samples + 1) as f32;
                child.value_samples += 1;
                if child.packed.is_none() {
                    child.packed = packed;
                }
                return index;
            }
            let index = self.nodes.len();
            self.lookup.insert(key, index);
            self.nodes.push(SearchNode::new(
                observation.candidates,
                log_policy,
                value,
                depth,
                self.behavior_exponent,
                packed,
            ));
            if let Some(games) = &mut self.games {
                games.push(game.expect("captured MCTS node has no game"));
            }
            index
        }

        fn expand_rollout(&mut self, mut leaf: PendingLeaf, rollout: Vec<RolloutStep>, value: f32) {
            for step in rollout {
                let RolloutStep {
                    game,
                    observation,
                    policy,
                    digest,
                    choice,
                    depth,
                } = step;
                let packed = (self.capture_children
                    || self.lookup.get(&(digest, depth)).is_some_and(|&index| {
                        let node = &self.nodes[index];
                        node.packed.is_none()
                            && node.visits + 1 >= self.pack_visits
                            && node
                                .edges
                                .iter()
                                .filter(|edge| edge.visits > 0 || edge.candidate == choice)
                                .take(2)
                                .count()
                                >= 2
                    }))
                .then(|| compact_packed_observation_known(&observation, Some(digest)));
                let child = self.insert(observation, digest, depth, &policy, value, game, packed);
                let previous = leaf.path.steps.last_mut().expect("empty rollout path");
                previous.child = Some(child);
                leaf.path
                    .packed
                    .iter_mut()
                    .filter(|(node, _)| node.is_none())
                    .for_each(|(node, _)| *node = Some(child));
                let edge = self.nodes[child]
                    .edges
                    .iter()
                    .position(|edge| edge.candidate == choice)
                    .expect("rollout action is absent from its node");
                leaf.path.steps.push(SearchStep {
                    node: child,
                    edge,
                    child: None,
                });
            }
            self.backup(&mut leaf.path, value);
        }

        fn expectimax(&self) -> (Vec<f32>, Vec<Vec<Option<f32>>>) {
            let mut values = self.nodes.iter().map(|node| node.value).collect::<Vec<_>>();
            let mut actions = self
                .nodes
                .iter()
                .map(|node| vec![None; node.edges.len()])
                .collect::<Vec<_>>();
            let maximum_depth = self.nodes.iter().map(|node| node.depth).max().unwrap_or(0);
            for depth in (0..=maximum_depth).rev() {
                for (index, node) in self
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, node)| node.depth == depth)
                {
                    for (edge_index, edge) in node.edges.iter().enumerate() {
                        let visits = edge.visits;
                        if edge.invalid || visits == 0 {
                            continue;
                        }
                        let value = edge.terminal_value_sum
                            + edge
                                .children
                                .iter()
                                .map(|(child, count)| *count as f32 * values[*child])
                                .sum::<f32>();
                        actions[index][edge_index] = Some(value / visits as f32);
                    }
                    if let Some(value) = actions[index]
                        .iter()
                        .flatten()
                        .copied()
                        .max_by(f32::total_cmp)
                    {
                        values[index] = value;
                    }
                }
            }
            (values, actions)
        }

        fn target(
            &self,
            node: usize,
            values: &[Option<f32>],
            temperature: f32,
        ) -> Option<Vec<f32>> {
            let mut target = vec![0.0; self.nodes[node].action_count];
            let maximum = values
                .iter()
                .flatten()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            if !maximum.is_finite() || values.iter().flatten().count() < 2 {
                return None;
            }
            let mut sum = 0.0;
            for (edge, value) in self.nodes[node].edges.iter().zip(values) {
                if let Some(value) = value {
                    target[edge.candidate] = ((*value - maximum) / temperature).exp();
                    sum += target[edge.candidate];
                }
            }
            target.iter_mut().for_each(|value| *value /= sum);
            Some(target)
        }

        fn consistency(&self, node: usize) -> SearchConsistency {
            let mut children = HashMap::<usize, f32>::new();
            let mut covered = 0.0;
            let mut terminal_value = 0.0;
            for edge in &self.nodes[node].edges {
                if edge.invalid || edge.visits == 0 {
                    continue;
                }
                let scale = edge.behavior / edge.visits as f32;
                covered += scale * edge.terminal_visits as f32;
                terminal_value += scale * edge.terminal_value_sum;
                for (child, count) in &edge.children {
                    if self.nodes[*child].packed.is_some() {
                        covered += scale * *count as f32;
                        *children.entry(*child).or_default() += scale * *count as f32;
                    }
                }
            }
            let mut children = children.into_iter().collect::<Vec<_>>();
            children.sort_by_key(|&(child, _)| child);
            let (packed, weights) = children
                .into_iter()
                .map(|(child, weight)| {
                    (
                        self.nodes[child]
                            .packed
                            .clone()
                            .expect("consistency child was not packed"),
                        weight,
                    )
                })
                .unzip();
            SearchConsistency {
                packed,
                weights,
                self_weight: (1.0 - covered).max(0.0),
                terminal_value,
            }
        }
    }

    fn same_public_candidate(left: &CandidateRow, right: &CandidateRow) -> bool {
        left.u == right.u
            && left.s == right.s
            && left.c == right.c
            && left.f == right.f
            && left.legal == right.legal
    }

    fn public_candidate_digest(row: &CandidateRow) -> u64 {
        let mut digest = 0xcbf2_9ce4_8422_2325u64;
        for value in row
            .u
            .iter()
            .copied()
            .chain(row.s.iter().map(|&value| value as u32))
            .chain(row.c.iter().copied())
            .chain(row.f.iter().map(|value| value.to_bits()))
            .chain(std::iter::once(row.legal as u32))
        {
            digest = (digest ^ value as u64).wrapping_mul(0x100_0000_01b3);
        }
        digest
    }

    #[derive(Default)]
    struct SearchStats {
        turn_starts: usize,
        roots: usize,
        simulations: usize,
        leaves: usize,
        nodes: usize,
        batches: usize,
        targets: usize,
        micros: u64,
        simulate_micros: u64,
        encode_micros: u64,
        inference_micros: u64,
        backup_micros: u64,
        rollout_steps: usize,
        rollout_completed: usize,
        rollout_invalid: usize,
        rollout_micros: u64,
        timed_out: bool,
    }

    struct ExpertTarget {
        packed: Vec<u8>,
        target: Vec<f32>,
        visits: u32,
        depth: usize,
        consistency: SearchConsistency,
    }

    #[derive(Default)]
    struct SearchConsistency {
        packed: Vec<Vec<u8>>,
        weights: Vec<f32>,
        self_weight: f32,
        terminal_value: f32,
    }

    struct PendingLeaf {
        tree: usize,
        observation: ObservationV56,
        digest: u64,
        depth: usize,
        path: SearchPath,
        duplicates: Vec<SearchPath>,
        game: Option<Game>,
    }

    struct RolloutStep {
        game: Option<Game>,
        observation: ObservationV56,
        policy: Vec<f32>,
        digest: u64,
        choice: usize,
        depth: usize,
    }

    struct CombatRollout {
        value: Option<f32>,
        steps: Vec<RolloutStep>,
    }

    fn sample_policy(log_policy: &[f32], random: &mut u64) -> Option<usize> {
        let draw = random_f32(random);
        let mut cumulative = 0.0;
        log_policy
            .iter()
            .enumerate()
            .filter(|(_, probability)| probability.is_finite())
            .find_map(|(index, probability)| {
                cumulative += probability.exp();
                (cumulative >= draw).then_some(index)
            })
            .or_else(|| log_policy.iter().rposition(|value| value.is_finite()))
    }

    fn rollout_combat(
        model: &ValueModel,
        trees: &[SearchTree],
        leaves: &mut [PendingLeaf],
        initial: &[(Vec<f32>, f32, f32, Vec<f32>, Option<Vec<f32>>)],
        content: &Content,
        layout: Layout,
        bonuses: (i16, i16),
        random: &mut u64,
        temperature: f32,
        prior_temperature: f32,
        deadline: Option<std::time::Instant>,
        stats: &mut SearchStats,
    ) -> Result<Option<Vec<CombatRollout>>, String> {
        let started = std::time::Instant::now();
        let mut games = leaves
            .iter_mut()
            .map(|leaf| {
                if trees[leaf.tree].games.is_some() {
                    leaf.game.clone()
                } else {
                    leaf.game.take()
                }
                .expect("combat rollout leaf has no game")
            })
            .collect::<Vec<_>>();
        let mut depths = leaves.iter().map(|leaf| leaf.depth).collect::<Vec<_>>();
        let mut values = vec![None; leaves.len()];
        let mut paths = leaves
            .iter()
            .map(|leaf| Vec::with_capacity((trees[leaf.tree].max_depth - leaf.depth).min(16)))
            .collect::<Vec<_>>();
        let mut pending = (0..leaves.len()).collect::<Vec<_>>();
        let mut indices = Vec::with_capacity(leaves.len());
        let mut terminal = Vec::with_capacity(leaves.len());
        let mut actions = vec![None; leaves.len()];
        let mut first = true;
        loop {
            if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                stats.rollout_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
                return Ok(None);
            }
            indices.clear();
            terminal.clear();
            for index in pending.drain(..) {
                let game = &games[index];
                let tree = &trees[leaves[index].tree];
                if matches!(game.phase, Phase::Won | Phase::Dead) {
                    values[index] = Some(if tree.heuristic {
                        if matches!(game.phase, Phase::Dead) {
                            0.0
                        } else {
                            1.0 + 0.1 * heuristic_combat_value(game)
                        }
                    } else {
                        search_terminal_value(game, tree.progress).unwrap()
                    });
                    stats.rollout_completed += 1;
                } else if game.combat().is_none() {
                    if tree.heuristic {
                        values[index] = Some(1.0 + 0.1 * heuristic_combat_value(game));
                        stats.rollout_completed += 1;
                    } else {
                        indices.push(index);
                        terminal.push(true);
                    }
                } else if depths[index] >= tree.max_depth {
                    stats.rollout_invalid += 1;
                } else {
                    indices.push(index);
                    terminal.push(false);
                }
            }
            if indices.is_empty() {
                break;
            }
            if first {
                for (&index, &terminal) in indices.iter().zip(&terminal) {
                    let output = &initial[index];
                    if terminal {
                        values[index] = Some(if trees[leaves[index].tree].progress {
                            output.2
                        } else {
                            output.1
                        });
                        stats.rollout_completed += 1;
                    } else {
                        let choice = sample_policy(
                            output.4.as_ref().expect("combat rollout policy missing"),
                            random,
                        )
                        .ok_or_else(|| "combat rollout has no legal action".to_owned())?;
                        actions[index] =
                            Some(leaves[index].observation.candidates[choice].action.clone());
                        paths[index].push(RolloutStep {
                            game: trees[leaves[index].tree]
                                .games
                                .is_some()
                                .then(|| games[index].clone()),
                            observation: leaves[index].observation.clone(),
                            policy: output.0.clone(),
                            digest: leaves[index].digest,
                            choice,
                            depth: depths[index],
                        });
                        depths[index] += 1;
                        stats.rollout_steps += 1;
                    }
                }
                first = false;
            } else {
                let observations = indices
                    .par_iter()
                    .map(|&index| {
                        observation_v56_with_map(
                            &games[index],
                            content,
                            layout,
                            bonuses,
                            Some(&trees[leaves[index].tree].map),
                        )
                    })
                    .collect::<Vec<_>>();
                let rows = observations.iter().collect::<Vec<_>>();
                let encoded = model.state_actions_batch(&rows);
                let outputs = model
                    .evaluate_batch(
                        &rows,
                        &encoded,
                        temperature,
                        None,
                        terminal.iter().any(|&terminal| terminal),
                    )
                    .map_err(|error| error.to_string())?;
                let digests = observations
                    .par_iter()
                    .zip(&terminal)
                    .map(|(observation, &terminal)| {
                        (!terminal).then(|| observation_digest(observation))
                    })
                    .collect::<Vec<_>>();
                for ((((index, terminal), output), observation), digest) in indices
                    .iter()
                    .copied()
                    .zip(terminal.iter().copied())
                    .zip(outputs)
                    .zip(observations)
                    .zip(digests)
                {
                    if terminal {
                        values[index] = Some(if trees[leaves[index].tree].progress {
                            output.2
                        } else {
                            output.1
                        });
                        stats.rollout_completed += 1;
                    } else {
                        let choice = sample_policy(&output.0, random)
                            .ok_or_else(|| "combat rollout has no legal action".to_owned())?;
                        let mut policy = output
                            .0
                            .iter()
                            .map(|value| value * temperature / prior_temperature)
                            .collect::<Vec<_>>();
                        let maximum = policy.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                        let normalizer = policy
                            .iter()
                            .filter(|value| value.is_finite())
                            .map(|value| (value - maximum).exp())
                            .sum::<f32>()
                            .ln()
                            + maximum;
                        policy
                            .iter_mut()
                            .filter(|value| value.is_finite())
                            .for_each(|value| *value -= normalizer);
                        let digest = digest.expect("combat rollout digest missing");
                        actions[index] = Some(observation.candidates[choice].action.clone());
                        paths[index].push(RolloutStep {
                            game: trees[leaves[index].tree]
                                .games
                                .is_some()
                                .then(|| games[index].clone()),
                            digest,
                            observation,
                            policy,
                            choice,
                            depth: depths[index],
                        });
                        depths[index] += 1;
                        stats.rollout_steps += 1;
                    }
                }
            }
            pending.extend(
                indices
                    .iter()
                    .copied()
                    .zip(&terminal)
                    .filter_map(|(index, terminal)| (!terminal).then_some(index)),
            );
            games
                .par_iter_mut()
                .zip(actions.par_iter_mut())
                .try_for_each(|(game, action)| {
                    if let Some(action) = action.take() {
                        game.step(content, action)
                            .map_err(|error| format!("combat rollout step failed: {error:?}"))?;
                    }
                    Ok::<_, String>(())
                })?;
        }
        stats.rollout_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        Ok(Some(
            values
                .into_iter()
                .zip(paths)
                .map(|(value, steps)| CombatRollout { value, steps })
                .collect(),
        ))
    }

    fn run_mcts(
        model: &ValueModel,
        trees: &mut [SearchTree],
        content: &Content,
        layout: Layout,
        bonuses: (i16, i16),
        random: &mut u64,
        batch_size: usize,
        prior_temperature: f32,
        policy_temperature: f32,
        exploration: f32,
        deadline: Option<std::time::Instant>,
        stats: &mut SearchStats,
    ) -> Result<(), String> {
        let mut cursor = 0;
        let mut tasks = Vec::with_capacity(batch_size);
        let mut leaves = Vec::<PendingLeaf>::with_capacity(batch_size);
        let mut leaf_lookup = FastMap::<(usize, u64, usize), usize>::default();
        let mut updates: Vec<Vec<(SearchPath, Option<f32>)>> =
            (0..trees.len()).map(|_| Vec::new()).collect();
        let mut expansions: Vec<Vec<(PendingLeaf, Vec<f32>, f32)>> =
            (0..trees.len()).map(|_| Vec::new()).collect();
        let mut rollout_expansions: Vec<Vec<(PendingLeaf, Vec<RolloutStep>, f32)>> =
            (0..trees.len()).map(|_| Vec::new()).collect();
        while trees.iter().any(|tree| tree.simulations < tree.budget) {
            if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                stats.timed_out = true;
                break;
            }
            tasks.clear();
            for lane in 0..16 {
                for offset in 0..trees.len() {
                    let tree = (cursor + offset) % trees.len();
                    let search = &trees[tree];
                    if search.simulations + lane < search.budget {
                        tasks.push((tree, random_u64(random), lane));
                        if tasks.len() == batch_size {
                            break;
                        }
                    }
                }
                if tasks.len() == batch_size {
                    break;
                }
            }
            cursor = (cursor + tasks.len()) % trees.len();
            let started = std::time::Instant::now();
            let results = tasks
                .par_iter()
                .map(|&(tree, seed, lane)| {
                    trees[tree]
                        .simulate(content, layout, bonuses, seed, exploration, lane)
                        .map(|result| (tree, result))
                })
                .collect::<Result<Vec<_>, _>>()?;
            stats.simulate_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            let started = std::time::Instant::now();
            leaves.clear();
            leaf_lookup.clear();
            updates.iter_mut().for_each(Vec::clear);
            for (tree_index, result) in results {
                match result {
                    SearchResult::Value(path, value) => {
                        updates[tree_index].push((path, Some(value)))
                    }
                    SearchResult::Invalid(path) => updates[tree_index].push((path, None)),
                    SearchResult::Leaf(leaf) => {
                        let key = (tree_index, leaf.digest, leaf.depth);
                        if trees[tree_index].turns != 0 {
                            if let Some(&index) = leaf_lookup.get(&key) {
                                leaves[index].duplicates.push(leaf.path);
                                continue;
                            }
                            leaf_lookup.insert(key, leaves.len());
                        }
                        leaves.push(PendingLeaf {
                            tree: tree_index,
                            observation: leaf.observation,
                            digest: leaf.digest,
                            depth: leaf.depth,
                            path: leaf.path,
                            duplicates: Vec::new(),
                            game: leaf.game,
                        });
                    }
                }
            }
            trees
                .par_iter_mut()
                .zip(&mut updates)
                .for_each(|(tree, updates)| {
                    for (mut path, value) in updates.drain(..) {
                        if let Some(value) = value {
                            tree.backup(&mut path, value);
                        } else {
                            tree.invalidate(&mut path);
                        }
                    }
                });
            if leaves.is_empty() {
                stats.backup_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
                continue;
            }
            stats.backup_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            let rows = leaves
                .iter()
                .map(|leaf| &leaf.observation)
                .collect::<Vec<_>>();
            let started = std::time::Instant::now();
            let features = model.state_actions_batch(&rows);
            stats.encode_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            let started = std::time::Instant::now();
            let rollout = trees.iter().any(|tree| tree.turns == 0);
            let evaluated = model
                .evaluate_batch(
                    &rows,
                    &features,
                    prior_temperature,
                    rollout.then_some(policy_temperature),
                    !rollout || trees.iter().any(|tree| !tree.heuristic),
                )
                .map_err(|error| error.to_string())?;
            stats.inference_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
            let mut rollout_values = if rollout {
                let Some(values) = rollout_combat(
                    model,
                    trees,
                    &mut leaves,
                    &evaluated,
                    content,
                    layout,
                    bonuses,
                    random,
                    policy_temperature,
                    prior_temperature,
                    deadline,
                    stats,
                )?
                else {
                    stats.timed_out = true;
                    break;
                };
                Some(values)
            } else {
                None
            };
            tracing::debug!(
                tasks = tasks.len(),
                leaves = leaves.len(),
                trees = trees.len(),
                "mcts_wave"
            );
            stats.leaves += leaves.len();
            stats.batches += 1;
            let started = std::time::Instant::now();
            expansions.iter_mut().for_each(Vec::clear);
            rollout_expansions.iter_mut().for_each(Vec::clear);
            for (index, (mut leaf, (policy, win, progress, _, _))) in
                leaves.drain(..).zip(evaluated).enumerate()
            {
                let value = if let Some(rollouts) = &rollout_values {
                    rollouts[index].value
                } else if trees[leaf.tree].heuristic {
                    Some(heuristic_combat_value(
                        leaf.game.as_ref().expect("heuristic leaf has no game"),
                    ))
                } else if trees[leaf.tree].progress {
                    Some(progress)
                } else {
                    Some(win)
                };
                if let Some(value) = value {
                    if let Some(rollouts) = &mut rollout_values {
                        rollout_expansions[leaf.tree].push((
                            leaf,
                            std::mem::take(&mut rollouts[index].steps),
                            value,
                        ));
                    } else {
                        expansions[leaf.tree].push((leaf, policy, value));
                    }
                } else {
                    trees[leaf.tree].invalidate(&mut leaf.path);
                }
            }
            trees
                .par_iter_mut()
                .zip(&mut expansions)
                .zip(&mut rollout_expansions)
                .for_each(|((tree, expansions), rollouts)| {
                    for (leaf, rollout, value) in rollouts.drain(..) {
                        tree.expand_rollout(leaf, rollout, value);
                    }
                    for (leaf, policy, value) in expansions.drain(..) {
                        tree.expand(
                            leaf.observation,
                            leaf.digest,
                            leaf.depth,
                            leaf.path,
                            leaf.duplicates,
                            &policy,
                            value,
                            leaf.game,
                        );
                    }
                });
            stats.backup_micros += started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }
        Ok(())
    }

    fn mcts_targets(
        model: &ValueModel,
        games: &[Game],
        observations: &[ObservationV56],
        outputs: &[(Vec<f32>, f32, f32, Vec<f32>, Option<Vec<f32>>)],
        turn_starts: &[bool],
        content: &Content,
        layout: Layout,
        bonuses: (i16, i16),
        random: &mut u64,
        fraction: f32,
        simulations: usize,
        boss_simulations: usize,
        turns: usize,
        max_depth: usize,
        batch_size: usize,
        min_visits: u32,
        max_targets: usize,
        prior_temperature: f32,
        q_temperature: f32,
        exploration: f32,
        policy_temperature: f32,
        value_consistency: bool,
        heuristic: bool,
        timeout: f64,
    ) -> Result<(Vec<ExpertTarget>, SearchStats), String> {
        let turn_start_count = observations
            .iter()
            .zip(turn_starts)
            .filter(|(observation, turn_start)| {
                **turn_start
                    && observation
                        .candidates
                        .iter()
                        .filter(|row| row.legal)
                        .count()
                        >= 2
            })
            .count();
        let mut trees = games
            .iter()
            .zip(observations)
            .zip(outputs)
            .zip(turn_starts)
            .filter_map(
                |(((game, observation), (log_policy, _, progress, _, _)), turn_start)| {
                    if !turn_start
                        || observation
                            .candidates
                            .iter()
                            .filter(|row| row.legal)
                            .count()
                            < 2
                    {
                        return None;
                    }
                    let forced = matches!(game.room, Room::Elite | Room::Boss);
                    let budget = if forced && boss_simulations > 0 {
                        boss_simulations
                    } else if simulations > 0 && random_f32(random) < fraction {
                        simulations
                    } else {
                        return None;
                    };
                    let mut prior = log_policy
                        .iter()
                        .map(|value| value * policy_temperature / prior_temperature)
                        .collect::<Vec<_>>();
                    let maximum = prior.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    let normalizer = prior
                        .iter()
                        .filter(|value| value.is_finite())
                        .map(|value| (value - maximum).exp())
                        .sum::<f32>()
                        .ln()
                        + maximum;
                    prior
                        .iter_mut()
                        .filter(|value| value.is_finite())
                        .for_each(|value| *value -= normalizer);
                    Some(SearchTree::new(
                        game,
                        observation,
                        content,
                        layout,
                        &prior,
                        *progress,
                        budget,
                        turns,
                        max_depth,
                        false,
                        true,
                        prior_temperature / policy_temperature,
                        min_visits,
                        value_consistency,
                        heuristic,
                    ))
                },
            )
            .collect::<Vec<_>>();
        let mut stats = SearchStats {
            turn_starts: turn_start_count,
            roots: trees.len(),
            ..SearchStats::default()
        };
        let deadline = if timeout > 0.0 {
            Some(
                std::time::Instant::now()
                    .checked_add(
                        std::time::Duration::try_from_secs_f64(timeout)
                            .map_err(|_| "invalid MCTS timeout")?,
                    )
                    .ok_or("invalid MCTS timeout")?,
            )
        } else {
            None
        };
        run_mcts(
            model,
            &mut trees,
            content,
            layout,
            bonuses,
            random,
            batch_size,
            prior_temperature,
            policy_temperature,
            exploration,
            deadline,
            &mut stats,
        )?;
        let mut targets = Vec::new();
        for tree in trees {
            stats.simulations += tree.simulations;
            stats.nodes += tree.nodes.len();
            if tree.simulations < tree.budget {
                continue;
            }
            let (_, action_values) = tree.expectimax();
            let mut nodes = tree
                .nodes
                .iter()
                .enumerate()
                .map(|(index, node)| (index, node.visits))
                .filter(|(index, visits)| {
                    *visits >= min_visits && action_values[*index].iter().flatten().count() >= 2
                })
                .collect::<Vec<_>>();
            nodes.sort_by_key(|&(index, visits)| (std::cmp::Reverse(visits), index));
            for (index, visits) in nodes.into_iter().take(max_targets) {
                if let Some(target) = tree.target(index, &action_values[index], q_temperature) {
                    targets.push(ExpertTarget {
                        packed: tree.nodes[index]
                            .packed
                            .clone()
                            .expect("eligible search target was not packed"),
                        target,
                        visits,
                        depth: tree.nodes[index].depth,
                        consistency: if value_consistency {
                            tree.consistency(index)
                        } else {
                            SearchConsistency {
                                self_weight: 1.0,
                                ..SearchConsistency::default()
                            }
                        },
                    });
                }
            }
        }
        stats.targets = targets.len();
        Ok((targets, stats))
    }

    struct ExactLeaf {
        value: f32,
        player_hp: i16,
        enemy_hp: i32,
        depth: usize,
        weight: usize,
        rng: bool,
        state: serde_json::Value,
    }

    fn exact_leaf_state(game: &Game, content: &Content) -> serde_json::Value {
        let card = |card: &Card| {
            format!(
                "{}+{}:{}:{}:{}:{:?}",
                content.cards[card.id as usize].id,
                card.upgrades,
                card.cost_delta,
                card.value,
                card.free as u8,
                card.cost_override,
            )
        };
        let powers = |creature: &Creature| {
            creature
                .powers
                .iter()
                .map(|power| {
                    format!(
                        "{}:{}:{}",
                        content.powers[power.id as usize].id, power.amount, power.value
                    )
                })
                .collect::<Vec<_>>()
        };
        let common = serde_json::json!({
            "phase": phase_index(&game.phase),
            "actions": game.actions(content).len(),
            "gold": game.run.gold,
            "potions": game.run.potions.iter().map(|id| id.map(|id| content.potions[id as usize].id)).collect::<Vec<_>>(),
        });
        match &game.phase {
            Phase::Combat(combat) => serde_json::json!({
                "common": common,
                "turn": combat.turn,
                "energy": combat.energy,
                "stars": combat.stars,
                "player_block": combat.player.block,
                "player_powers": powers(&combat.player),
                "hand": combat.hand.iter().map(&card).collect::<Vec<_>>(),
                "draw": combat.draw.iter().map(&card).collect::<Vec<_>>(),
                "discard": combat.discard.iter().map(&card).collect::<Vec<_>>(),
                "exhaust": combat.exhaust.iter().map(&card).collect::<Vec<_>>(),
                "enemies": combat.enemies.iter().map(|enemy| serde_json::json!({
                    "id": content.enemies[enemy.creature.id as usize].id,
                    "hp": enemy.creature.hp,
                    "block": enemy.creature.block,
                    "move": enemy.move_index,
                    "powers": powers(&enemy.creature),
                })).collect::<Vec<_>>(),
            }),
            Phase::Rewards(rewards) => serde_json::json!({
                "common": common,
                "reward_gold": rewards.gold,
                "cards": rewards.cards.iter().map(&card).collect::<Vec<_>>(),
                "card_rewards": rewards.card_rewards.iter().map(|reward| format!("{reward:?}")).collect::<Vec<_>>(),
                "relics": rewards.relics.iter().map(|id| content.relics[*id as usize].id).collect::<Vec<_>>(),
                "reward_potions": rewards.potions.iter().map(|id| content.potions[*id as usize].id).collect::<Vec<_>>(),
                "removals": rewards.removals,
            }),
            _ => common,
        }
    }

    struct ExactResult {
        value: f32,
        rng: bool,
    }

    struct ExactSearch<'a> {
        model: &'a ValueModel,
        content: &'a Content,
        layout: Layout,
        bonuses: (i16, i16),
        map: CanonicalMap,
        root_turn: u16,
        turns: usize,
        max_depth: usize,
        max_states: usize,
        states: usize,
        transitions: usize,
        rng_transitions: usize,
        stochastic_splits: usize,
        leaves: Vec<ExactLeaf>,
        action_values: Vec<f32>,
        choices: HashMap<(u64, usize), (Vec<u8>, usize)>,
        record_leaves: bool,
        heuristic: bool,
        cache: EncodingCache,
        progress: bool,
    }

    impl ExactSearch<'_> {
        fn enter(&mut self) -> Result<(), String> {
            if self.states >= self.max_states {
                return Err(format!(
                    "exhaustive search exceeded {} public states",
                    self.max_states
                ));
            }
            self.states += 1;
            Ok(())
        }

        fn horizon(&self, game: &Game) -> bool {
            game.combat().is_none()
                || self.turns > 0
                    && game.combat().is_some_and(|combat| {
                        combat.turn >= self.root_turn.saturating_add(self.turns as u16)
                    })
        }

        fn leaf(
            &mut self,
            particles: &[Game],
            observation: Option<&ObservationV56>,
            depth: usize,
            rng: bool,
        ) -> Result<ExactResult, String> {
            let game = &particles[0];
            let value = if self.heuristic {
                heuristic_combat_value(game)
            } else if let Some(value) = search_terminal_value(game, self.progress) {
                value
            } else {
                let row =
                    observation.ok_or_else(|| "missing exhaustive leaf observation".to_owned())?;
                let output = self
                    .model
                    .evaluate(row, 1.0, &mut self.cache)
                    .map_err(|error| error.to_string())?;
                if self.progress { output.2 } else { output.1 }
            };
            self.record_leaf(game, particles.len(), depth, rng, value);
            Ok(ExactResult { value, rng })
        }

        fn record_leaf(&mut self, game: &Game, weight: usize, depth: usize, rng: bool, value: f32) {
            if self.record_leaves {
                let (player_hp, enemy_hp) = game.combat().map_or((game.run.hp, 0), |combat| {
                    (
                        combat.player.hp,
                        combat
                            .enemies
                            .iter()
                            .map(|enemy| enemy.creature.hp.max(0) as i32)
                            .sum(),
                    )
                });
                self.leaves.push(ExactLeaf {
                    value,
                    player_hp,
                    enemy_hp,
                    depth,
                    weight,
                    rng,
                    state: exact_leaf_state(game, self.content),
                });
            }
        }

        fn flush_leaves(
            &mut self,
            pending: &mut Vec<(f32, usize, Option<Game>, ObservationV56)>,
            depth: usize,
            rng: bool,
        ) -> Result<f32, String> {
            if pending.is_empty() {
                return Ok(0.0);
            }
            if self.heuristic {
                return Ok(pending
                    .drain(..)
                    .map(|(weight, count, game, _)| {
                        let game = game.expect("heuristic leaf has no game");
                        let value = heuristic_combat_value(&game);
                        self.record_leaf(&game, count, depth, rng, value);
                        weight * value
                    })
                    .sum());
            }
            if pending.len() == 1 {
                let (weight, count, game, observation) = pending.pop().unwrap();
                let output = self
                    .model
                    .evaluate(&observation, 1.0, &mut self.cache)
                    .map_err(|error| error.to_string())?;
                let value = if self.progress { output.2 } else { output.1 };
                if let Some(game) = game.as_ref() {
                    self.record_leaf(game, count, depth, rng, value);
                }
                return Ok(weight * value);
            }
            let evaluated = pending
                .par_iter()
                .map(|(_, _, _, observation)| {
                    let index =
                        rayon::current_thread_index().unwrap_or(0) % self.model.encode_caches.len();
                    self.model
                        .evaluate(
                            observation,
                            1.0,
                            &mut self.model.encode_caches[index].lock().unwrap(),
                        )
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(pending
                .drain(..)
                .zip(evaluated)
                .map(|((weight, count, game, _), (_, win, progress, _))| {
                    let value = if self.progress { progress } else { win };
                    if let Some(game) = game.as_ref() {
                        self.record_leaf(game, count, depth, rng, value);
                    }
                    weight * value
                })
                .sum())
        }

        fn solve(
            &mut self,
            particles: Vec<Game>,
            row: Option<ObservationV56>,
            depth: usize,
            rng: bool,
        ) -> Result<ExactResult, String> {
            self.enter()?;
            if matches!(particles[0].phase, Phase::Won | Phase::Dead) || self.horizon(&particles[0])
            {
                return self.leaf(&particles, row.as_ref(), depth, rng);
            }
            if depth >= self.max_depth {
                return Ok(ExactResult {
                    value: f32::NEG_INFINITY,
                    rng,
                });
            }
            let row = row.ok_or_else(|| "missing exhaustive observation".to_owned())?;
            let policy = row
                .candidates
                .iter()
                .map(|candidate| {
                    if candidate.legal {
                        0.0
                    } else {
                        f32::NEG_INFINITY
                    }
                })
                .collect::<Vec<_>>();
            let node = SearchNode::new(row.candidates.clone(), &policy, 0.0, depth, 1.0, None);
            if node.edges.is_empty() {
                return self.leaf(&particles, Some(&row), depth, rng);
            }
            let mut actions = (0..node.edges.len())
                .map(|_| Vec::with_capacity(particles.len()))
                .collect::<Vec<_>>();
            for (particle, game) in particles.iter().enumerate() {
                let observation = (particle > 0).then(|| {
                    observation_v56_with_map(
                        game,
                        self.content,
                        self.layout,
                        self.bonuses,
                        Some(&self.map),
                    )
                });
                let observation = observation.as_ref().unwrap_or(&row);
                for (actions, edge) in actions.iter_mut().zip(&node.edges) {
                    actions.push(
                        search_candidate(observation, edge)
                            .ok_or_else(|| "exhaustive public action mismatch".to_owned())?
                            .action
                            .clone(),
                    );
                }
            }
            let mut best = f32::NEG_INFINITY;
            let mut best_choice = 0;
            let mut any_rng = rng;
            let mut groups = HashMap::<u64, (Option<ObservationV56>, Vec<Game>)>::new();
            for (edge, actions) in node.edges.iter().zip(actions) {
                groups.clear();
                let mut action_rng = false;
                let content = self.content;
                let layout = self.layout;
                let bonuses = self.bonuses;
                let map = &self.map;
                let successors = particles
                    .par_iter()
                    .zip(actions.into_par_iter())
                    .map(|(game, action)| {
                        let mut next = game.clone();
                        let action_rng = next
                            .step_with_rng(content, action)
                            .map_err(|error| format!("exhaustive step failed: {error:?}"))?;
                        let (digest, observation) = match next.phase {
                            Phase::Won => (u64::MAX, None),
                            Phase::Dead => (u64::MAX - 1, None),
                            _ => {
                                let observation = observation_v56_with_map(
                                    &next,
                                    content,
                                    layout,
                                    bonuses,
                                    Some(map),
                                );
                                (observation_digest(&observation), Some(observation))
                            }
                        };
                        Ok((next, action_rng, digest, observation))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                for (next, next_rng, digest, observation) in successors {
                    action_rng |= next_rng;
                    groups
                        .entry(digest)
                        .or_insert_with(|| (observation, Vec::new()))
                        .1
                        .push(next);
                }
                self.transitions += particles.len();
                self.rng_transitions += usize::from(action_rng);
                self.stochastic_splits += usize::from(groups.len() > 1);
                let branch_rng = rng || action_rng || groups.len() > 1;
                let mut value = 0.0;
                let mut pending = Vec::new();
                for (_, (observation, group)) in groups.drain() {
                    let weight = group.len() as f32 / particles.len() as f32;
                    if self.horizon(&group[0])
                        && !matches!(group[0].phase, Phase::Won | Phase::Dead)
                    {
                        self.enter()?;
                        let count = group.len();
                        let game = (self.record_leaves || self.heuristic)
                            .then(|| group.into_iter().next().unwrap());
                        pending.push((weight, count, game, observation.unwrap()));
                    } else {
                        any_rng |= !pending.is_empty() && branch_rng;
                        value += self.flush_leaves(&mut pending, depth + 1, branch_rng)?;
                        let result = self.solve(group, observation, depth + 1, branch_rng)?;
                        value += weight * result.value;
                        any_rng |= result.rng;
                    }
                }
                if !pending.is_empty() {
                    any_rng |= branch_rng;
                }
                value += self.flush_leaves(&mut pending, depth + 1, branch_rng)?;
                if depth == 0 {
                    self.action_values.push(value);
                }
                if value > best {
                    best = value;
                    best_choice = edge.candidate;
                }
            }
            if best.is_finite() {
                self.choices.insert(
                    (observation_digest(&row), depth),
                    (compact_packed_observation(&row), best_choice),
                );
            }
            Ok(ExactResult {
                value: best,
                rng: any_rng,
            })
        }
    }

    fn exact_search<'a>(
        model: &'a ValueModel,
        game: &Game,
        content: &'a Content,
        layout: Layout,
        bonuses: (i16, i16),
        root_turn: u16,
        turns: usize,
        max_depth: usize,
        samples: usize,
        max_states: usize,
        seed: u64,
        record_leaves: bool,
        progress: bool,
        heuristic: bool,
    ) -> Result<(ExactResult, ExactSearch<'a>), String> {
        let mut random = seed.max(1);
        let particles = (0..samples)
            .map(|_| {
                let mut particle = game.clone();
                resample_combat_hidden(&mut particle, random_u64(&mut random));
                particle
            })
            .collect::<Vec<_>>();
        let map = canonical_map(&particles[0], content, layout);
        let row = observation_v56_with_map(&particles[0], content, layout, bonuses, Some(&map));
        let mut search = ExactSearch {
            model,
            content,
            layout,
            bonuses,
            map,
            root_turn,
            turns,
            max_depth,
            max_states,
            states: 0,
            transitions: 0,
            rng_transitions: 0,
            stochastic_splits: 0,
            leaves: Vec::new(),
            action_values: Vec::new(),
            choices: HashMap::new(),
            record_leaves,
            heuristic,
            cache: EncodingCache::default(),
            progress,
        };
        let result = search.solve(particles, Some(row), 0, false)?;
        Ok((result, search))
    }

    fn search_diagnostic(
        model: &ValueModel,
        game: &Game,
        content: &Content,
        layout: Layout,
        bonuses: (i16, i16),
        simulations: usize,
        turns: usize,
        max_depth: usize,
        batch_size: usize,
        prior_temperature: f32,
        exploration: f32,
        policy_temperature: f32,
        exact_samples: usize,
        max_exact_states: usize,
        seed: u64,
        progress: bool,
        q_temperature: f32,
    ) -> Result<serde_json::Value, String> {
        let observation = observation_v56(game, content, layout, bonuses);
        let (log_policy, win, progress_value, _) = model
            .evaluate(
                &observation,
                policy_temperature,
                &mut EncodingCache::default(),
            )
            .map_err(|error| error.to_string())?;
        let value = if progress { progress_value } else { win };
        let mut prior = log_policy
            .iter()
            .map(|value| value * policy_temperature / prior_temperature)
            .collect::<Vec<_>>();
        let maximum = prior.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let normalizer = prior
            .iter()
            .filter(|value| value.is_finite())
            .map(|value| (value - maximum).exp())
            .sum::<f32>()
            .ln()
            + maximum;
        prior
            .iter_mut()
            .filter(|value| value.is_finite())
            .for_each(|value| *value -= normalizer);
        let mut tree = SearchTree::new(
            game,
            &observation,
            content,
            layout,
            &prior,
            value,
            simulations,
            turns,
            max_depth,
            true,
            progress,
            1.0,
            u32::MAX,
            false,
            false,
        );
        let mut random = seed.max(1);
        let mut stats = SearchStats {
            roots: 1,
            ..SearchStats::default()
        };
        run_mcts(
            model,
            std::slice::from_mut(&mut tree),
            content,
            layout,
            bonuses,
            &mut random,
            batch_size,
            prior_temperature,
            policy_temperature,
            exploration,
            None,
            &mut stats,
        )?;
        let root_turn = tree.root_turn;
        let (root_exact, exact) = exact_search(
            model,
            game,
            content,
            layout,
            bonuses,
            root_turn,
            turns,
            max_depth,
            exact_samples,
            max_exact_states,
            seed ^ 0x4558_4143_5452_4e47,
            true,
            progress,
            false,
        )?;
        let (search_values, search_actions) = tree.expectimax();
        let games = tree
            .games
            .as_ref()
            .expect("diagnostic MCTS did not capture games");
        let mut nodes = Vec::with_capacity(tree.nodes.len());
        for (index, (node, game)) in tree.nodes.iter().zip(games).enumerate() {
            let visits = node.edges.iter().map(|edge| edge.visits).sum::<u32>();
            let (result, exact_actions, search) = if index == 0 {
                (
                    ExactResult {
                        value: root_exact.value,
                        rng: root_exact.rng,
                    },
                    exact.action_values.clone(),
                    None,
                )
            } else {
                let (result, search) = exact_search(
                    model,
                    game,
                    content,
                    layout,
                    bonuses,
                    root_turn,
                    turns,
                    max_depth.saturating_sub(node.depth).max(1),
                    exact_samples,
                    max_exact_states,
                    seed ^ index as u64,
                    false,
                    progress,
                    false,
                )?;
                let action_values = search.action_values.clone();
                (result, action_values, Some(search))
            };
            let target = tree.target(index, &search_actions[index], q_temperature);
            let actions = node
                .edges
                .iter()
                .zip(&search_actions[index])
                .zip(exact_actions)
                .map(|((edge, approximate), exact)| {
                    serde_json::json!({
                        "action": format!("{:?}", edge.row.action),
                        "visits": edge.visits,
                        "prior": edge.prior,
                        "rollout_mean": (edge.visits > 0).then(|| edge.value_sum / edge.visits as f32),
                        "approximate": approximate,
                        "exact": exact,
                        "target": target.as_ref().map(|target| target[edge.candidate]),
                    })
                })
                .collect::<Vec<_>>();
            nodes.push(serde_json::json!({
                "node": index,
                "depth": node.depth,
                "visits": visits,
                "model_value": node.value,
                "approximate": search_values[index],
                "exact": result.value,
                "absolute_error": (search_values[index] - result.value).abs(),
                "rng": result.rng,
                "exact_states": search.as_ref().map_or(exact.states, |search| search.states),
                "exact_transitions": search.as_ref().map_or(exact.transitions, |search| search.transitions),
                "rng_transitions": search.as_ref().map_or(exact.rng_transitions, |search| search.rng_transitions),
                "stochastic_splits": search.as_ref().map_or(exact.stochastic_splits, |search| search.stochastic_splits),
                "actions": actions,
            }));
        }
        let root_node = &tree.nodes[0];
        let policy_choice = root_node
            .edges
            .iter()
            .enumerate()
            .max_by(|left, right| {
                log_policy[left.1.candidate].total_cmp(&log_policy[right.1.candidate])
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        let search_choice = root_node
            .edges
            .iter()
            .enumerate()
            .max_by_key(|(_, edge)| edge.visits)
            .map(|(index, _)| index)
            .unwrap_or(0);
        let search_q_choice = root_node
            .edges
            .iter()
            .enumerate()
            .filter_map(|(index, _)| search_actions[0][index].map(|value| (index, value)))
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| index)
            .unwrap_or(search_choice);
        let q_target = tree
            .target(0, &search_actions[0], q_temperature)
            .ok_or_else(|| "root has insufficient Q estimates".to_owned())?;
        let policy_value = root_node
            .edges
            .iter()
            .zip(&exact.action_values)
            .map(|(edge, value)| log_policy[edge.candidate].exp() * value)
            .sum::<f32>();
        let q_target_value = root_node
            .edges
            .iter()
            .zip(&exact.action_values)
            .map(|(edge, value)| q_target[edge.candidate] * value)
            .sum::<f32>();
        let q_target_entropy = -q_target
            .iter()
            .filter(|value| **value > 0.0)
            .map(|value| value * value.ln())
            .sum::<f32>();
        let root_actions = root_node
            .edges
            .iter()
            .enumerate()
            .zip(&exact.action_values)
            .map(|((index, edge), exact)| {
                serde_json::json!({
                    "action": format!("{:?}", edge.row.action),
                    "policy": log_policy[edge.candidate].exp(),
                    "search_prior": edge.prior,
                    "visits": edge.visits,
                    "rollout_mean": (edge.visits > 0).then(|| edge.value_sum / edge.visits as f32),
                    "approximate": search_actions[0][index],
                    "exact": exact,
                    "q_target": q_target[edge.candidate],
                })
            })
            .collect::<Vec<_>>();
        let leaves = exact
            .leaves
            .iter()
            .map(|leaf| {
                serde_json::json!({
                    "value": leaf.value,
                    "player_hp": leaf.player_hp,
                    "enemy_hp": leaf.enemy_hp,
                    "depth": leaf.depth,
                    "weight": leaf.weight,
                    "rng": leaf.rng,
                    "state": leaf.state,
                })
            })
            .collect::<Vec<_>>();
        let combat = game
            .combat()
            .ok_or_else(|| "diagnostic root is not in combat".to_owned())?;
        Ok(serde_json::json!({
            "root": {
                "seed": game.seed,
                "character": game.run.character,
                "act": game.run.act,
                "floor": game.run.floor,
                "turn": combat.turn,
                "player_hp": combat.player.hp,
                "enemy_hp": combat.enemies.iter().map(|enemy| enemy.creature.hp.max(0) as i32).sum::<i32>(),
            },
            "settings": {
                "simulations": simulations,
                "turns": turns,
                "max_depth": max_depth,
                "exact_samples": exact_samples,
                "max_exact_states": max_exact_states,
                "objective": if progress { "progress" } else { "win" },
                "q_temperature": q_temperature,
            },
            "mcts": {
                "simulations": tree.simulations,
                "nodes": tree.nodes.len(),
                "leaves": stats.leaves,
                "batches": stats.batches,
            },
            "exact": {
                "value": root_exact.value,
                "rng": root_exact.rng,
                "states": exact.states,
                "transitions": exact.transitions,
                "rng_transitions": exact.rng_transitions,
                "stochastic_splits": exact.stochastic_splits,
                "leaves": leaves.len(),
            },
            "root_policy": {
                "policy_expected_value": policy_value,
                "policy_greedy_value": exact.action_values.get(policy_choice),
                "search_selected_value": exact.action_values.get(search_choice),
                "search_q_selected_value": exact.action_values.get(search_q_choice),
                "optimal_value": root_exact.value,
                "policy_expected_gap": root_exact.value - policy_value,
                "q_target_expected_value": q_target_value,
                "q_target_gap": root_exact.value - q_target_value,
                "q_target_entropy": q_target_entropy,
                "policy_greedy_gap": exact.action_values.get(policy_choice).map(|value| root_exact.value - value),
                "search_gap": exact.action_values.get(search_choice).map(|value| root_exact.value - value),
                "search_q_gap": exact.action_values.get(search_q_choice).map(|value| root_exact.value - value),
                "actions": root_actions,
            },
            "nodes": nodes,
            "leaves": leaves,
        }))
    }

    fn random_u64(random: &mut u64) -> u64 {
        *random ^= *random << 13;
        *random ^= *random >> 7;
        *random ^= *random << 17;
        *random
    }

    fn random_f32(random: &mut u64) -> f32 {
        (random_u64(random) >> 40) as f32 / (1u32 << 24) as f32
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
                policy: None,
                searched_turns: vec![None; size],
                content,
                layout,
            };
            for _ in 0..size {
                let game = batch.fresh_game()?;
                batch.games.push(game);
            }
            Ok(batch)
        }

        fn token_layout(&self) -> std::collections::BTreeMap<String, usize> {
            let mut out = std::collections::BTreeMap::from([
                ("version".into(), VERSION as usize),
                ("characters".into(), self.layout.characters),
                ("globals".into(), globals_len(self.layout)),
                ("model_width".into(), MODEL_WIDTH),
                ("model_layers".into(), MODEL_LAYERS),
                ("model_heads".into(), MODEL_HEADS),
                ("model_feedforward".into(), MODEL_FEEDFORWARD),
                ("entity_summaries".into(), 0),
                ("base_state_width".into(), MODEL_WIDTH),
                ("state_width".into(), MODEL_WIDTH),
                ("action_width".into(), MODEL_WIDTH),
                ("head_width".into(), MODEL_WIDTH),
                ("domain_count".into(), DOMAIN_NAMES.len()),
                ("action_u".into(), ACTION_U),
                ("action_s".into(), ACTION_S),
                ("action_f".into(), ACTION_F),
                ("state_scope".into(), u32::MAX as usize),
                ("absent_node".into(), u32::MAX as usize),
                ("run_current_node".into(), 23),
                ("action_path_node".into(), 4),
                ("action_object".into(), 14),
                ("actor_owner".into(), 0),
                ("child_owner".into(), 0),
                ("child_kind".into(), 1),
                ("card_zone".into(), 0),
                ("card_order_kind".into(), 2),
                ("card_order".into(), 3),
                ("continuation_frame".into(), 2),
                ("continuation_parent".into(), 3),
                ("continuation_branch".into(), 4),
                ("continuation_path".into(), 5),
                ("continuation_list".into(), 7),
                ("continuation_order".into(), 8),
                ("map_node_id".into(), 0),
                ("map_node_floor".into(), 2),
                ("map_node_lane".into(), 3),
                ("map_node_topo".into(), 8),
                ("map_node_outdegree".into(), 9),
                ("map_edge_src".into(), 0),
                ("map_edge_dst".into(), 1),
                ("map_entry".into(), 0),
                ("player_owner".into(), 1),
                ("osty_owner".into(), 2),
                ("enemy_owner_start".into(), 3),
                ("card_zones".into(), CARD_ZONES),
                ("card_candidate_zone".into(), ATTACHED_CARD_ZONE),
                (
                    "history_course_status".into(),
                    HISTORY_COURSE_STATUS as usize,
                ),
                ("normal_encounters".into(), 58),
                ("elite_encounters".into(), 14),
                ("boss_encounters".into(), 14),
            ]);
            for (name, (u, s, c, f)) in DOMAIN_NAMES.into_iter().zip(DOMAIN_WIDTHS) {
                out.insert(format!("{name}_u"), u);
                out.insert(format!("{name}_s"), s);
                out.insert(format!("{name}_c"), c);
                out.insert(format!("{name}_f"), f);
            }
            out.insert("action_c".into(), ACTION_C);
            out.insert("concept_vocab".into(), self.layout.concept_vocab());
            for (index, name) in SEMANTIC_NAMES.into_iter().enumerate() {
                out.insert(
                    format!("{name}_semantic_start"),
                    self.layout.semantic_offsets[index] as usize,
                );
                out.insert(
                    format!("{name}_semantic_count"),
                    self.layout.semantic_sizes[index] as usize,
                );
            }
            out
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
            self.searched_turns = vec![None];
            Ok(())
        }

        fn load_snapshot_actions(&mut self, snapshot: &str) -> PyResult<()> {
            let value: serde_json::Value = serde_json::from_str(snapshot)
                .map_err(|error| PyValueError::new_err(error.to_string()))?;
            let game = crate::replay::game_from_live_snapshot(
                &self.content,
                &value["before"],
                &value["oracle_before"],
            )
            .map_err(PyValueError::new_err)?;
            let actions = game.actions(&self.content);
            self.games = actions
                .into_iter()
                .map(|action| {
                    let mut next = game.clone();
                    next.step(&self.content, action)
                        .map_err(|error| PyValueError::new_err(format!("{error:?}")))?;
                    Ok(next)
                })
                .collect::<PyResult<_>>()?;
            self.actions = vec![vec![]; self.games.len()];
            self.plans = vec![vec![]; self.games.len()];
            self.starts.clear();
            self.root_ids = vec![usize::MAX; self.games.len()];
            self.searched_turns = vec![None; self.games.len()];
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

        #[pyo3(signature = (source, indices, repeats=1, seed=None))]
        fn copy_resampled(
            &mut self,
            source: PyRef<'_, Batch>,
            indices: Vec<usize>,
            repeats: usize,
            seed: Option<u64>,
        ) -> PyResult<()> {
            if repeats == 0 || indices.len() * repeats != self.games.len() {
                return Err(PyValueError::new_err("invalid repeat count"));
            }
            let roots = indices
                .iter()
                .map(|&index| {
                    source
                        .games
                        .get(index)
                        .cloned()
                        .ok_or_else(|| PyValueError::new_err("invalid environment index"))
                })
                .collect::<PyResult<Vec<_>>>()?;
            let mut random = seed.map(Rng::from_seed);
            let seeds = (0..self.games.len())
                .map(|_| random.as_mut().map_or_else(|| self.random_u64(), Rng::next))
                .collect::<Vec<_>>();
            let content = &self.content;
            self.games
                .par_iter_mut()
                .zip(seeds)
                .enumerate()
                .for_each(|(index, (game, seed))| {
                    *game = roots[index / repeats].clone();
                    resample_hidden(game, content, seed);
                });
            self.training_strength = source.training_strength;
            self.training_dexterity = source.training_dexterity;
            self.actions.iter_mut().for_each(Vec::clear);
            self.plans.iter_mut().for_each(Vec::clear);
            self.starts.clear();
            self.root_ids.fill(usize::MAX);
            self.searched_turns.fill(None);
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
                .zip(&self.actions)
                .zip(active)
                .enumerate()
                .map(|(index, (((game, plan), represented), active))| {
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
                    represented
                        .iter()
                        .position(|candidate| *candidate == action)
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
                let features = (!combat).then(|| {
                    candidate_signature(&source, &self.content, self.layout, &action).unwrap()
                });
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
                        && features.as_ref().is_some_and(|features| {
                            candidate_signature(&target, &self.content, self.layout, candidate)
                                .is_some_and(|candidate| candidate == *features)
                        })
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

        fn stats(&self) -> Vec<(u8, u8, i16, i16, u8, u16, i16, i32, f32, u8)> {
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
                        canonical_progress(game),
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

        #[pyo3(signature = (active=None, flat=false))]
        fn observe_tokens<'py>(
            &mut self,
            py: Python<'py>,
            active: Option<Vec<bool>>,
            flat: bool,
        ) -> PyResult<Bound<'py, PyTuple>> {
            let layout = self.layout;
            let content = &self.content;
            let bonuses = (self.training_strength, self.training_dexterity);
            let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
            if active.len() != self.games.len() {
                return Err(PyValueError::new_err("invalid active mask"));
            }
            let rows = py.allow_threads(|| {
                self.games
                    .par_iter()
                    .zip(&active)
                    .map(|(game, &active)| {
                        if !active {
                            return None;
                        }
                        debug_assert!(game.combat().is_none_or(|combat| {
                            combat.queue.is_empty() || combat.choice.is_some()
                        }));
                        Some(observation_v56(game, content, layout, bonuses))
                    })
                    .collect::<Vec<_>>()
            });
            let batches = rows.len();
            let mut characters = vec![u8::MAX; batches];
            let mut globals = vec![0.0; batches * globals_len(layout)];
            let mut potentials = vec![0.0; batches];
            let mut digests = vec![0u64; batches];
            let max_actions = rows
                .iter()
                .filter_map(Option::as_ref)
                .map(|row| row.candidates.len())
                .max()
                .unwrap_or(0)
                .max(1);
            let padded_actions = if flat { 0 } else { batches * max_actions };
            let mut action_u = vec![0; padded_actions * ACTION_U];
            let mut action_s = vec![0; padded_actions * ACTION_S];
            let mut action_c = vec![0; padded_actions * ACTION_C];
            let mut action_f = vec![0.0; padded_actions * ACTION_F];
            let mut represented = vec![0; padded_actions];
            let mut legal = vec![0; batches * max_actions];
            for (batch, row) in rows.iter().enumerate() {
                let Some(row) = row else {
                    continue;
                };
                characters[batch] = row.character;
                globals[batch * globals_len(layout)..(batch + 1) * globals_len(layout)]
                    .copy_from_slice(&row.globals);
                potentials[batch] = row.potential;
                for (position, candidate) in row.candidates.iter().enumerate() {
                    let base = batch * max_actions + position;
                    if !flat {
                        action_u[base * ACTION_U..(base + 1) * ACTION_U]
                            .copy_from_slice(&candidate.u);
                        action_s[base * ACTION_S..(base + 1) * ACTION_S]
                            .copy_from_slice(&candidate.s);
                        action_c[base * ACTION_C..(base + 1) * ACTION_C]
                            .copy_from_slice(&candidate.c);
                        action_f[base * ACTION_F..(base + 1) * ACTION_F]
                            .copy_from_slice(&candidate.f);
                        represented[base] = 1;
                    }
                    legal[base] = candidate.legal as u8;
                }
            }
            if flat {
                let packed_data = py.allow_threads(|| {
                    rows.par_iter()
                        .map(|row| {
                            row.as_ref().map(|row| {
                                let (globals, counts, exact, actions, digest) =
                                    packed_observation(row);
                                (
                                    row.character,
                                    globals,
                                    counts,
                                    compress_words(&exact),
                                    actions,
                                    digest,
                                )
                            })
                        })
                        .collect::<Vec<_>>()
                });
                let mut packed_domains = Vec::with_capacity(DOMAIN_NAMES.len());
                let action_offsets = rows
                    .iter()
                    .scan(0usize, |offset, row| {
                        let start = *offset;
                        *offset += row.as_ref().map_or(0, |row| row.candidates.len());
                        Some(start)
                    })
                    .collect::<Vec<_>>();
                for domain in 0..DOMAIN_NAMES.len() {
                    let (u_width, s_width, c_width, f_width) = DOMAIN_WIDTHS[domain];
                    let total = rows
                        .iter()
                        .filter_map(Option::as_ref)
                        .map(|row| row.domains[domain].len())
                        .sum();
                    let mut exact_u: Vec<u32> = Vec::with_capacity(total * u_width);
                    let mut exact_s: Vec<i32> = Vec::with_capacity(total * s_width);
                    let mut semantic: Vec<u32> = Vec::with_capacity(total * c_width);
                    let mut numeric: Vec<f32> = Vec::with_capacity(total * f_width);
                    let mut scope: Vec<i32> = Vec::with_capacity(total);
                    let mut row_index: Vec<i32> = Vec::with_capacity(total);
                    for (batch, row) in rows.iter().enumerate() {
                        let Some(row) = row else { continue };
                        for record in &row.domains[domain] {
                            exact_u.extend(&record.u);
                            exact_s.extend(&record.s);
                            semantic.extend(&record.c);
                            numeric.extend(&record.f);
                            scope.push(if record.scope < 0 {
                                record.scope
                            } else {
                                record.scope + action_offsets[batch] as i32
                            });
                            row_index.push(batch as i32);
                        }
                    }
                    packed_domains.push(
                        PyTuple::new(
                            py,
                            [
                                ndarray::Array2::from_shape_vec((total, u_width), exact_u)
                                    .unwrap()
                                    .into_pyarray(py)
                                    .into_any(),
                                ndarray::Array2::from_shape_vec((total, s_width), exact_s)
                                    .unwrap()
                                    .into_pyarray(py)
                                    .into_any(),
                                ndarray::Array2::from_shape_vec((total, c_width), semantic)
                                    .unwrap()
                                    .into_pyarray(py)
                                    .into_any(),
                                ndarray::Array2::from_shape_vec((total, f_width), numeric)
                                    .unwrap()
                                    .into_pyarray(py)
                                    .into_any(),
                                ndarray::Array1::from_vec(row_index)
                                    .into_pyarray(py)
                                    .into_any(),
                                ndarray::Array1::from_vec(scope).into_pyarray(py).into_any(),
                            ],
                        )?
                        .into_any(),
                    );
                }
                let total_actions = action_offsets.last().copied().unwrap_or(0)
                    + rows
                        .last()
                        .and_then(Option::as_ref)
                        .map_or(0, |row| row.candidates.len());
                let mut flat_u: Vec<u32> = Vec::with_capacity(total_actions * ACTION_U);
                let mut flat_s: Vec<i32> = Vec::with_capacity(total_actions * ACTION_S);
                let mut flat_c: Vec<u32> = Vec::with_capacity(total_actions * ACTION_C);
                let mut flat_f: Vec<f32> = Vec::with_capacity(total_actions * ACTION_F);
                let mut action_row: Vec<i32> = Vec::with_capacity(total_actions);
                let mut action_position: Vec<i32> = Vec::with_capacity(total_actions);
                for (batch, row) in rows.iter().enumerate() {
                    let Some(row) = row else { continue };
                    for (position, candidate) in row.candidates.iter().enumerate() {
                        flat_u.extend(candidate.u);
                        flat_s.extend(candidate.s);
                        flat_c.extend(candidate.c);
                        flat_f.extend(candidate.f);
                        action_row.push(batch as i32);
                        action_position.push(position as i32);
                    }
                }
                let actions = PyTuple::new(
                    py,
                    [
                        ndarray::Array2::from_shape_vec((total_actions, ACTION_U), flat_u)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array2::from_shape_vec((total_actions, ACTION_S), flat_s)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array2::from_shape_vec((total_actions, ACTION_C), flat_c)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array2::from_shape_vec((total_actions, ACTION_F), flat_f)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array1::from_vec(action_row)
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array1::from_vec(action_position)
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array2::from_shape_vec((batches, max_actions), legal)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                    ],
                )?;
                let mut packed_rows = Vec::with_capacity(batches);
                for (batch, row) in packed_data.into_iter().enumerate() {
                    let (character, globals, counts, exact, actions, digest) = row.map_or_else(
                        || {
                            (
                                u8::MAX,
                                vec![0.0; globals_len(layout)],
                                vec![0; DOMAIN_NAMES.len()],
                                Vec::new(),
                                Vec::new(),
                                0,
                            )
                        },
                        |row| row,
                    );
                    digests[batch] = digest;
                    packed_rows.push(PyTuple::new(
                        py,
                        [
                            character.into_pyobject(py)?.into_any(),
                            ndarray::Array1::from_vec(globals)
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array1::from_vec(counts)
                                .into_pyarray(py)
                                .into_any(),
                            PyBytes::new(py, &exact).into_any(),
                            ndarray::Array2::from_shape_vec(
                                (
                                    actions.len() / (ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1),
                                    ACTION_U + ACTION_S + ACTION_C + ACTION_F + 1,
                                ),
                                actions,
                            )
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                            digest.into_pyobject(py)?.into_any(),
                        ],
                    )?);
                }
                self.actions = rows
                    .iter()
                    .map(|row| {
                        row.as_ref().map_or_else(Vec::new, |row| {
                            row.candidates
                                .iter()
                                .map(|candidate| candidate.action.clone())
                                .collect()
                        })
                    })
                    .collect();
                return PyTuple::new(
                    py,
                    [
                        ndarray::Array1::from_vec(characters)
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array2::from_shape_vec((batches, globals_len(layout)), globals)
                            .unwrap()
                            .into_pyarray(py)
                            .into_any(),
                        PyTuple::new(py, packed_domains)?.into_any(),
                        actions.into_any(),
                        ndarray::Array1::from_vec(potentials)
                            .into_pyarray(py)
                            .into_any(),
                        ndarray::Array1::from_vec(digests)
                            .into_pyarray(py)
                            .into_any(),
                        PyTuple::new(py, packed_rows)?.into_any(),
                    ],
                );
            }
            let mut packed_domains = Vec::with_capacity(DOMAIN_NAMES.len());
            for domain in 0..DOMAIN_NAMES.len() {
                let (u_width, s_width, c_width, f_width) = DOMAIN_WIDTHS[domain];
                let max_rows = rows
                    .iter()
                    .filter_map(Option::as_ref)
                    .map(|row| row.domains[domain].len())
                    .max()
                    .unwrap_or(0)
                    .max(1);
                let mut exact_u = vec![0; batches * max_rows * u_width];
                let mut exact_s = vec![0; batches * max_rows * s_width];
                let mut semantic = vec![0; batches * max_rows * c_width];
                let mut numeric = vec![0.0; batches * max_rows * f_width];
                let mut scope = vec![0; batches * max_rows];
                let mut mask = vec![0; batches * max_rows];
                for (batch, row) in rows.iter().enumerate() {
                    let Some(row) = row else { continue };
                    for (position, record) in row.domains[domain].iter().enumerate() {
                        let base = batch * max_rows + position;
                        exact_u[base * u_width..(base + 1) * u_width].copy_from_slice(&record.u);
                        exact_s[base * s_width..(base + 1) * s_width].copy_from_slice(&record.s);
                        semantic[base * c_width..(base + 1) * c_width].copy_from_slice(&record.c);
                        numeric[base * f_width..(base + 1) * f_width].copy_from_slice(&record.f);
                        scope[base] = record.scope;
                        mask[base] = 1;
                    }
                }
                packed_domains.push(
                    PyTuple::new(
                        py,
                        [
                            ndarray::Array3::from_shape_vec((batches, max_rows, u_width), exact_u)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array3::from_shape_vec((batches, max_rows, s_width), exact_s)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array3::from_shape_vec((batches, max_rows, c_width), semantic)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array3::from_shape_vec((batches, max_rows, f_width), numeric)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array2::from_shape_vec((batches, max_rows), scope)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                            ndarray::Array2::from_shape_vec((batches, max_rows), mask)
                                .unwrap()
                                .into_pyarray(py)
                                .into_any(),
                        ],
                    )?
                    .into_any(),
                );
            }
            let domains = PyTuple::new(py, packed_domains)?;
            let actions = PyTuple::new(
                py,
                [
                    ndarray::Array3::from_shape_vec((batches, max_actions, ACTION_U), action_u)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array3::from_shape_vec((batches, max_actions, ACTION_S), action_s)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array3::from_shape_vec((batches, max_actions, ACTION_C), action_c)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array3::from_shape_vec((batches, max_actions, ACTION_F), action_f)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array2::from_shape_vec((batches, max_actions), represented)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array2::from_shape_vec((batches, max_actions), legal)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                ],
            )?;
            for (batch, row) in rows.iter().enumerate() {
                if let Some(row) = row {
                    digests[batch] = packed_observation(row).4;
                }
            }
            self.actions = rows
                .iter()
                .map(|row| {
                    row.as_ref().map_or_else(Vec::new, |row| {
                        row.candidates
                            .iter()
                            .map(|candidate| candidate.action.clone())
                            .collect()
                    })
                })
                .collect();
            PyTuple::new(
                py,
                [
                    ndarray::Array1::from_vec(characters)
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array2::from_shape_vec((batches, globals_len(layout)), globals)
                        .unwrap()
                        .into_pyarray(py)
                        .into_any(),
                    domains.into_any(),
                    actions.into_any(),
                    ndarray::Array1::from_vec(potentials)
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array1::from_vec(digests)
                        .into_pyarray(py)
                        .into_any(),
                ],
            )
        }

        fn load_policy(&mut self, data: &[u8]) -> PyResult<()> {
            self.policy = Some(
                ValueModel::from_bytes(data, &self.content)
                    .map_err(|error| PyValueError::new_err(error.to_string()))?,
            );
            Ok(())
        }

        #[pyo3(signature = (
            index=0,
            simulations=128,
            turns=1,
            max_depth=64,
            batch_size=256,
            prior_temperature=1.0,
            exploration=1.5,
            policy_temperature=0.8,
            exact_samples=16,
            max_exact_states=100_000,
            seed=1,
            progress=false,
            q_temperature=0.002,
        ))]
        fn search_diagnostics(
            &self,
            py: Python<'_>,
            index: usize,
            simulations: usize,
            turns: usize,
            max_depth: usize,
            batch_size: usize,
            prior_temperature: f32,
            exploration: f32,
            policy_temperature: f32,
            exact_samples: usize,
            max_exact_states: usize,
            seed: u64,
            progress: bool,
            q_temperature: f32,
        ) -> PyResult<String> {
            if simulations == 0
                || max_depth == 0
                || batch_size == 0
                || exact_samples == 0
                || max_exact_states == 0
                || !prior_temperature.is_finite()
                || prior_temperature <= 0.0
                || !exploration.is_finite()
                || exploration < 0.0
                || !policy_temperature.is_finite()
                || policy_temperature <= 0.0
                || !q_temperature.is_finite()
                || q_temperature <= 0.0
            {
                return Err(PyValueError::new_err("invalid search diagnostic settings"));
            }
            let game = self
                .games
                .get(index)
                .ok_or_else(|| PyValueError::new_err("invalid environment index"))?;
            if game.combat().is_none() {
                return Err(PyValueError::new_err("diagnostic root is not in combat"));
            }
            let model = self
                .policy
                .as_ref()
                .ok_or_else(|| PyValueError::new_err("policy is not loaded"))?;
            let result = py.allow_threads(|| {
                search_diagnostic(
                    model,
                    game,
                    &self.content,
                    self.layout,
                    (self.training_strength, self.training_dexterity),
                    simulations,
                    turns,
                    max_depth,
                    batch_size,
                    prior_temperature,
                    exploration,
                    policy_temperature,
                    exact_samples,
                    max_exact_states,
                    seed,
                    progress,
                    q_temperature,
                )
            });
            serde_json::to_string_pretty(&result.map_err(PyValueError::new_err)?)
                .map_err(|error| PyValueError::new_err(error.to_string()))
        }

        #[pyo3(signature = (
            indices,
            turns=1,
            max_depth=64,
            samples=16,
            max_states=100_000,
            seed=1,
            progress=true,
            heuristic=false,
        ))]
        fn exact_choices(
            &self,
            py: Python<'_>,
            indices: Vec<usize>,
            turns: usize,
            max_depth: usize,
            samples: usize,
            max_states: usize,
            seed: u64,
            progress: bool,
            heuristic: bool,
        ) -> PyResult<
            Vec<(
                i64,
                f32,
                u64,
                u64,
                u64,
                bool,
                Vec<(Vec<u8>, i64, u64)>,
                String,
            )>,
        > {
            if max_depth == 0 || samples == 0 || max_states == 0 {
                return Err(PyValueError::new_err("invalid exact-search settings"));
            }
            let model = self
                .policy
                .as_ref()
                .ok_or_else(|| PyValueError::new_err("policy is not loaded"))?;
            let content = &self.content;
            let layout = self.layout;
            let bonuses = (self.training_strength, self.training_dexterity);
            Ok(py.allow_threads(|| {
                indices
                    .par_iter()
                    .map(|&index| {
                        let result = (|| {
                            let game = self
                                .games
                                .get(index)
                                .ok_or_else(|| "invalid environment index".to_owned())?;
                            let root_turn = game
                                .combat()
                                .ok_or_else(|| "exact-search root is not in combat".to_owned())?
                                .turn;
                            let observation = observation_v56(game, content, layout, bonuses);
                            let search_seed = seed
                                ^ game.seed as u64
                                ^ observation_digest(&observation).rotate_left(17);
                            let (result, search) = exact_search(
                                model,
                                game,
                                content,
                                layout,
                                bonuses,
                                root_turn,
                                turns,
                                max_depth,
                                samples,
                                max_states,
                                search_seed,
                                false,
                                progress,
                                heuristic,
                            )?;
                            let policy = observation
                                .candidates
                                .iter()
                                .map(|candidate| {
                                    if candidate.legal {
                                        0.0
                                    } else {
                                        f32::NEG_INFINITY
                                    }
                                })
                                .collect::<Vec<_>>();
                            let node = SearchNode::new(
                                observation.candidates.clone(),
                                &policy,
                                0.0,
                                0,
                                1.0,
                                None,
                            );
                            if search.action_values.len() != node.edges.len() {
                                return Err(
                                    "exact search returned incomplete root actions".to_owned()
                                );
                            }
                            let edge = search
                                .action_values
                                .iter()
                                .enumerate()
                                .filter(|(_, value)| value.is_finite())
                                .max_by(|left, right| left.1.total_cmp(right.1))
                                .map(|(edge, _)| edge)
                                .ok_or_else(|| "exact-search root has no action".to_owned())?;
                            let mut choices = search
                                .choices
                                .into_iter()
                                .map(|((_digest, depth), (row, choice))| {
                                    (row, choice as i64, depth as u64)
                                })
                                .collect::<Vec<_>>();
                            choices.sort_by_key(|(_, _, depth)| *depth);
                            Ok::<_, String>((
                                node.edges[edge].candidate as i64,
                                result.value,
                                search.states as u64,
                                search.transitions as u64,
                                search.rng_transitions as u64,
                                result.rng,
                                choices,
                            ))
                        })();
                        match result {
                            Ok((
                                choice,
                                value,
                                states,
                                transitions,
                                rng_transitions,
                                rng,
                                plan,
                            )) => (
                                choice,
                                value,
                                states,
                                transitions,
                                rng_transitions,
                                rng,
                                plan,
                                String::new(),
                            ),
                            Err(error) => (-1, 0.0, 0, 0, 0, false, Vec::new(), error),
                        }
                    })
                    .collect()
            }))
        }

        #[pyo3(signature = (
            temperature=1.0,
            sample=true,
            advance=false,
            mcts_fraction=0.0,
            mcts_simulations=0,
            mcts_boss_simulations=0,
            mcts_turns=1,
            mcts_max_depth=64,
            mcts_batch_size=256,
            mcts_min_visits=16,
            mcts_max_targets=64,
            mcts_prior_temperature=1.0,
            mcts_q_temperature=0.002,
            mcts_exploration=1.5,
            mcts_value_consistency=false,
            mcts_heuristic=false,
            mcts_timeout=0.0,
        ))]
        fn policy<'py>(
            &mut self,
            py: Python<'py>,
            temperature: f32,
            sample: bool,
            advance: bool,
            mcts_fraction: f32,
            mcts_simulations: usize,
            mcts_boss_simulations: usize,
            mcts_turns: usize,
            mcts_max_depth: usize,
            mcts_batch_size: usize,
            mcts_min_visits: u32,
            mcts_max_targets: usize,
            mcts_prior_temperature: f32,
            mcts_q_temperature: f32,
            mcts_exploration: f32,
            mcts_value_consistency: bool,
            mcts_heuristic: bool,
            mcts_timeout: f64,
        ) -> PyResult<Bound<'py, PyTuple>> {
            if !(0.0..=1.0).contains(&mcts_fraction)
                || mcts_max_depth == 0
                || mcts_batch_size == 0
                || mcts_min_visits == 0
                || mcts_max_targets == 0
                || !mcts_prior_temperature.is_finite()
                || mcts_prior_temperature <= 0.0
                || !mcts_q_temperature.is_finite()
                || mcts_q_temperature <= 0.0
                || !mcts_exploration.is_finite()
                || mcts_exploration < 0.0
                || !mcts_timeout.is_finite()
                || mcts_timeout < 0.0
            {
                return Err(PyValueError::new_err("invalid MCTS settings"));
            }
            let search_enabled = mcts_simulations > 0 || mcts_boss_simulations > 0;
            let turn_starts = self
                .games
                .iter()
                .zip(&mut self.searched_turns)
                .map(|(game, searched)| {
                    let Some(turn) = game.combat().map(|combat| combat.turn) else {
                        *searched = None;
                        return false;
                    };
                    let fresh = search_enabled && *searched != Some(turn);
                    if search_enabled {
                        *searched = Some(turn);
                    }
                    fresh
                })
                .collect::<Vec<_>>();
            let model = self
                .policy
                .as_ref()
                .ok_or_else(|| PyValueError::new_err("policy is not loaded"))?;
            let content = &self.content;
            let layout = self.layout;
            let bonuses = (self.training_strength, self.training_dexterity);
            let rows = py.allow_threads(|| {
                self.games
                    .par_iter()
                    .map(|game| observation_v56(game, content, layout, bonuses))
                    .collect::<Vec<_>>()
            });
            let features = py.allow_threads(|| {
                rows.par_iter()
                    .map(|row| {
                        let index =
                            rayon::current_thread_index().unwrap_or(0) % model.encode_caches.len();
                        let mut cache = model.encode_caches[index].lock().unwrap();
                        if cache.rows.len() > 65_536 {
                            cache.rows.clear();
                        }
                        if cache.groups.len() > 65_536 {
                            cache.groups.clear();
                        }
                        model.state_actions(row, &mut cache)
                    })
                    .collect::<Vec<_>>()
            });
            let packed = py.allow_threads(|| {
                rows.par_iter()
                    .map(compact_packed_observation)
                    .collect::<Vec<_>>()
            });
            let outputs = model
                .evaluate_batch(
                    &rows.iter().collect::<Vec<_>>(),
                    &features,
                    temperature,
                    None,
                    true,
                )
                .map_err(|error| PyValueError::new_err(error.to_string()))?;
            let mut random = self.random;
            let mut search_random = random ^ 0x4d43_5453_524e_4701;
            let (expert_targets, search_stats) = if search_enabled {
                let started = std::time::Instant::now();
                let (targets, mut stats) = py
                    .allow_threads(|| {
                        mcts_targets(
                            model,
                            &self.games,
                            &rows,
                            &outputs,
                            &turn_starts,
                            content,
                            layout,
                            bonuses,
                            &mut search_random,
                            mcts_fraction,
                            mcts_simulations,
                            mcts_boss_simulations,
                            mcts_turns,
                            mcts_max_depth,
                            mcts_batch_size,
                            mcts_min_visits,
                            mcts_max_targets,
                            mcts_prior_temperature,
                            mcts_q_temperature,
                            mcts_exploration,
                            temperature,
                            mcts_value_consistency,
                            mcts_heuristic,
                            mcts_timeout,
                        )
                    })
                    .map_err(PyValueError::new_err)?;
                stats.micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
                if stats.roots > 0 {
                    tracing::debug!(
                        turn_starts = stats.turn_starts,
                        roots = stats.roots,
                        simulations = stats.simulations,
                        leaves = stats.leaves,
                        nodes = stats.nodes,
                        batches = stats.batches,
                        targets = stats.targets,
                        elapsed_us = stats.micros,
                        simulate_us = stats.simulate_micros,
                        encode_us = stats.encode_micros,
                        inference_us = stats.inference_micros,
                        backup_us = stats.backup_micros,
                        rollout_steps = stats.rollout_steps,
                        rollout_completed = stats.rollout_completed,
                        rollout_invalid = stats.rollout_invalid,
                        rollout_us = stats.rollout_micros,
                        timed_out = stats.timed_out,
                        "mcts"
                    );
                    if stats.timed_out {
                        tracing::warn!(
                            roots = stats.roots,
                            simulations = stats.simulations,
                            elapsed_us = stats.micros,
                            "mcts_timeout"
                        );
                    } else if stats.micros > 5_000_000 {
                        tracing::warn!(
                            roots = stats.roots,
                            simulations = stats.simulations,
                            elapsed_us = stats.micros,
                            "slow_mcts"
                        );
                    }
                }
                (targets, stats)
            } else {
                (Vec::new(), SearchStats::default())
            };
            let mut characters = Vec::with_capacity(rows.len());
            let mut choices = Vec::with_capacity(rows.len());
            let mut log_probabilities = Vec::with_capacity(rows.len());
            let mut critic_probabilities = Vec::with_capacity(rows.len() * VALUE_CATEGORIES);
            let mut packed_rows = Vec::with_capacity(rows.len());
            let mut selected_actions = Vec::with_capacity(rows.len());
            self.actions.clear();
            if advance {
                self.actions.resize_with(rows.len(), Vec::new);
            }
            for (((row, packed), _features), (log_policy, _win, _expected, probabilities, _)) in
                rows.into_iter().zip(packed).zip(features).zip(outputs)
            {
                let choice = if sample {
                    sample_policy(&log_policy, &mut random)
                } else {
                    log_policy
                        .iter()
                        .enumerate()
                        .filter(|(_, value)| value.is_finite())
                        .max_by(|left, right| left.1.total_cmp(right.1))
                        .map(|(index, _)| index)
                }
                .ok_or_else(|| PyValueError::new_err("observation has no legal action"))?;
                characters.push(row.character);
                choices.push(choice as i64);
                log_probabilities.push(log_policy[choice]);
                critic_probabilities.extend(probabilities);
                if advance {
                    selected_actions.push(row.candidates[choice].action.clone());
                } else {
                    self.actions.push(
                        row.candidates
                            .iter()
                            .map(|candidate| candidate.action.clone())
                            .collect(),
                    );
                }
                packed_rows.push(PyBytes::new(py, &packed));
            }
            self.random = random;
            let expert_targets = expert_targets
                .into_iter()
                .map(|target| {
                    if target.consistency.self_weight == 1.0 {
                        return PyTuple::new(
                            py,
                            [
                                PyBytes::new(py, &target.packed).into_any(),
                                ndarray::Array1::from_vec(target.target)
                                    .into_pyarray(py)
                                    .into_any(),
                                target.visits.into_pyobject(py)?.into_any(),
                                target.depth.into_pyobject(py)?.into_any(),
                            ],
                        )
                        .map(Bound::into_any);
                    }
                    let children = PyTuple::new(
                        py,
                        target
                            .consistency
                            .packed
                            .iter()
                            .map(|packed| PyBytes::new(py, packed)),
                    )?;
                    PyTuple::new(
                        py,
                        [
                            PyBytes::new(py, &target.packed).into_any(),
                            ndarray::Array1::from_vec(target.target)
                                .into_pyarray(py)
                                .into_any(),
                            target.visits.into_pyobject(py)?.into_any(),
                            target.depth.into_pyobject(py)?.into_any(),
                            children.into_any(),
                            ndarray::Array1::from_vec(target.consistency.weights)
                                .into_pyarray(py)
                                .into_any(),
                            target.consistency.self_weight.into_pyobject(py)?.into_any(),
                            target
                                .consistency
                                .terminal_value
                                .into_pyobject(py)?
                                .into_any(),
                        ],
                    )
                    .map(Bound::into_any)
                })
                .collect::<PyResult<Vec<_>>>()?;
            let batch = characters.len();
            let mut output = vec![
                ndarray::Array1::from_vec(characters)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array1::from_vec(choices)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array1::from_vec(log_probabilities)
                    .into_pyarray(py)
                    .into_any(),
                ndarray::Array2::from_shape_vec((batch, VALUE_CATEGORIES), critic_probabilities)
                    .unwrap()
                    .into_pyarray(py)
                    .into_any(),
                PyTuple::new(py, packed_rows)?.into_any(),
                PyTuple::new(py, expert_targets)?.into_any(),
                vec![
                    search_stats.roots as u64,
                    search_stats.simulations as u64,
                    search_stats.leaves as u64,
                    search_stats.nodes as u64,
                    search_stats.batches as u64,
                    search_stats.targets as u64,
                    search_stats.turn_starts as u64,
                    search_stats.micros,
                    search_stats.simulate_micros,
                    search_stats.encode_micros,
                    search_stats.inference_micros,
                    search_stats.backup_micros,
                    search_stats.rollout_steps as u64,
                    search_stats.rollout_completed as u64,
                    search_stats.rollout_invalid as u64,
                    search_stats.rollout_micros,
                    search_stats.timed_out as u64,
                ]
                .into_pyobject(py)?
                .into_any(),
            ];
            if advance {
                let paths = selected_actions
                    .iter()
                    .map(|action| matches!(action, Action::Path(_)))
                    .collect::<Vec<_>>();
                let content = &self.content;
                let results = py
                    .allow_threads(|| {
                        self.games
                            .par_iter_mut()
                            .zip(selected_actions)
                            .map(|(game, action)| {
                                let was_combat = game.combat().is_some();
                                game.step(content, action)
                                    .map_err(|error| format!("{error:?}"))?;
                                Ok((
                                    matches!(game.phase, Phase::Won) as u8 as f32,
                                    matches!(game.phase, Phase::Won | Phase::Dead),
                                    was_combat,
                                    !was_combat && game.combat().is_some(),
                                ))
                            })
                            .collect::<Result<Vec<_>, String>>()
                    })
                    .map_err(PyValueError::new_err)?;
                for plan in &mut self.plans {
                    plan.clear();
                }
                for (index, path) in paths.into_iter().enumerate() {
                    if path {
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
                let rewards = results.iter().map(|result| result.0).collect::<Vec<_>>();
                let done = results.iter().map(|result| result.1).collect::<Vec<_>>();
                let in_combat = results.iter().map(|result| result.2).collect::<Vec<_>>();
                let stats = self.stats();
                let legal = self
                    .games
                    .par_iter()
                    .zip(&done)
                    .map(|(game, done)| {
                        !done
                            && candidate_actions(game, &self.content)
                                .1
                                .into_iter()
                                .any(|legal| legal)
                    })
                    .collect::<Vec<_>>();
                output.extend([
                    ndarray::Array1::from_vec(rewards)
                        .into_pyarray(py)
                        .into_any(),
                    ndarray::Array1::from_vec(done).into_pyarray(py).into_any(),
                    stats.into_pyobject(py)?.into_any(),
                    ndarray::Array1::from_vec(legal).into_pyarray(py).into_any(),
                    ndarray::Array1::from_vec(in_combat)
                        .into_pyarray(py)
                        .into_any(),
                ]);
            }
            PyTuple::new(py, output)
        }

        #[pyo3(signature = (active=None))]
        fn has_legal_actions(&self, active: Option<Vec<bool>>) -> PyResult<Vec<bool>> {
            let active = active.unwrap_or_else(|| vec![true; self.games.len()]);
            if active.len() != self.games.len() {
                return Err(PyValueError::new_err("invalid active mask"));
            }
            Ok(self
                .games
                .par_iter()
                .zip(active)
                .map(|(game, active)| {
                    active
                        && candidate_actions(game, &self.content)
                            .1
                            .into_iter()
                            .any(|legal| legal)
                })
                .collect())
        }

        #[pyo3(signature = (choices, active=None))]
        fn step(
            &mut self,
            py: Python<'_>,
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
            self.actions
                .iter()
                .zip(&choices)
                .zip(&active)
                .enumerate()
                .try_for_each(|(environment, ((actions, &choice), &active))| {
                    if active && choice >= actions.len() {
                        return Err(format!(
                            "environment {environment} chose action {choice} from {}",
                            actions.len()
                        ));
                    }
                    Ok(())
                })
                .map_err(PyValueError::new_err)?;
            let selected = self
                .actions
                .iter_mut()
                .zip(choices)
                .zip(&active)
                .map(|((actions, choice), &active)| active.then(|| actions.swap_remove(choice)))
                .collect::<Vec<_>>();
            let paths = selected
                .iter()
                .map(|action| matches!(action, Some(Action::Path(_))))
                .collect::<Vec<_>>();
            let content = &self.content;
            let results = py
                .allow_threads(|| {
                    self.games
                        .par_iter_mut()
                        .zip(selected.into_par_iter())
                        .map(|(game, action)| {
                            let Some(action) = action else {
                                return Ok((0.0, false, 0.0, false));
                            };
                            let was_combat = game.combat().is_some();
                            game.step(content, action)
                                .map_err(|error| format!("{error:?}"))?;
                            let reward = matches!(game.phase, Phase::Won) as u8 as f32;
                            let done = matches!(game.phase, Phase::Won | Phase::Dead);
                            Ok((reward, done, 0.0, !was_combat && game.combat().is_some()))
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .map_err(PyValueError::new_err)?;
            for plan in &mut self.plans {
                plan.clear();
            }
            for (index, path) in paths.into_iter().enumerate() {
                if path {
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
                self.searched_turns[index] = None;
            }
            Ok(())
        }

        fn rust_values(&self, path: &str) -> PyResult<Vec<f32>> {
            let model = ValueModel::load(path, &self.content)
                .map_err(|error| PyValueError::new_err(error.to_string()))?;
            Ok(self
                .games
                .par_iter()
                .map(|game| model.win_probability(game, &self.content))
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
        module.add_class::<Batch>()?;
        module.add_function(wrap_pyfunction!(configure_logging, module)?)?;
        module.add_function(wrap_pyfunction!(unique_rows, module)?)?;
        module.add_function(wrap_pyfunction!(unique_feature_rows, module)?)?;
        module.add_function(wrap_pyfunction!(unique_graphs, module)?)?;
        module.add_function(wrap_pyfunction!(validate_action_features, module)?)?;
        module.add_function(wrap_pyfunction!(compress_packed_observations, module)?)?;
        module.add_function(wrap_pyfunction!(validate_packed_observation, module)?)?;
        module.add_function(wrap_pyfunction!(validate_compact_observation, module)?)?;
        module.add_function(wrap_pyfunction!(unpack_packed_observations, module)?)
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
        let before = state_tokens(&game, &content, layout);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[0].move_history = vec![3, 3];
        assert_ne!(state_tokens(&game, &content, layout), before);
        assert_eq!(
            state_tokens(&game, &content, layout)
                .iter()
                .filter(|row| {
                    row[0] == ENEMY_COLLECTION as f32
                        && row[1] == 1.0
                        && row[2] == 4.0
                        && row[3] == 1.0
                })
                .count(),
            2
        );
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
            state_tokens(&plain.games[0], &plain.content, plain.layout),
            state_tokens(&boosted.games[0], &boosted.content, boosted.layout)
        );
        assert_ne!(
            state_tokens_with_bonuses(&plain.games[0], &plain.content, plain.layout, (0, 0)),
            state_tokens_with_bonuses(&plain.games[0], &plain.content, plain.layout, (24, 12))
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
    fn canonical_progress_matches_route_geometry() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 0, 0, 1).unwrap();
        for (act, floor, compass, expected) in [
            (1, 0, None, 0),
            (1, 15, None, 15),
            (2, 0, None, 25),
            (2, 6, None, 31),
            (2, 7, None, 33),
            (2, 10, None, 37),
            (2, 10, Some(2), 35),
            (2, 17, None, 44),
            (3, 0, None, 54),
            (3, 18, None, 72),
        ] {
            game.run.act = act;
            game.run.floor = floor;
            game.golden_compass = compass;
            assert_eq!(canonical_progress(&game), expected);
        }
    }

    fn state_signature(game: &Game, layout: Layout) -> Vec<f32> {
        let content = foundation_content();
        let mut out = observation_globals(game, &content, layout, true);
        out.extend(state_tokens(game, &content, layout).into_iter().flatten());
        out
    }

    fn summary_signature_with_bonuses(
        game: &Game,
        _layout: Layout,
        bonuses: (i16, i16),
    ) -> Vec<f32> {
        let content = foundation_content();
        let mut tokens = vec![];
        state_summary_tokens(game, &content, bonuses, &mut tokens);
        tokens.into_iter().flatten().collect()
    }

    fn summary_signature(game: &Game, layout: Layout) -> Vec<f32> {
        summary_signature_with_bonuses(game, layout, (0, 0))
    }

    fn action_signature(game: &Game, layout: Layout, action: &Action) -> Vec<f32> {
        let content = foundation_content();
        let (values, tokens) = tokenized_candidate(game, &content, layout, action, true);
        values
            .into_iter()
            .chain(tokens.into_iter().flatten())
            .collect()
    }

    #[test]
    fn features_cover_every_character_and_legal_action() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        for character in 0..content.characters.len() as Id {
            let mut game = Game::new_character_ascension(&content, 7, character, 10).unwrap();
            game.begin_act(&content, 0).unwrap();
            let state = state_signature(&game, layout);
            assert!(!state.is_empty());
            assert!(state.iter().all(|value| value.is_finite()));
            for action in game.actions(&content) {
                let features = action_signature(&game, layout, &action);
                assert!(features.len() >= TOKEN_ACTION_VALUES);
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
            enchanted[8..10],
            [card.flags as f32 + 1.0, card.turn_flags as f32 + 1.0]
        );
        assert_eq!(
            &enchanted[10..23],
            &[
                1.0,
                4.0,
                8.0,
                3.0,
                0.0,
                0.0,
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
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::WEAK,
                amount: 1,
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::DEXTERITY,
                amount: 3,
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::FRAIL,
                amount: 1,
                skip_next_decay: false,
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
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::HARD_TO_KILL,
                amount: 7,
                skip_next_decay: false,
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
        assert_eq!(hand(strike.id)[26], 1.0);
        assert_eq!(hand(strike.id)[29], 6.0);
        assert_eq!(hand(defend.id)[28], 6.0);
        assert_eq!(hand(whirlwind.id)[26], 3.0);
        assert_eq!(hand(whirlwind.id)[30], 15.0);
        assert_eq!(hand(burning_pact.id)[31], 2.0);
        assert_eq!(hand(burning_pact.id)[33], 1.0);
        assert_eq!(hand(survivor.id)[32], 1.0);
        assert_eq!(
            hand(mad_science.id)[5],
            CardType::Attack as usize as f32 + 1.0
        );
        assert_eq!(hand(mad_science.id)[28], 0.0);
        assert_eq!(hand(mad_science.id)[29], 10.0);

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
        assert_eq!(&values[48..56], &[1.0, 3.0, 7.0, 7.0, 4.0, 0.0, 0.0, 20.0]);

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
        assert_eq!(
            &values[48..56],
            &[1.0, 3.0, 0.0, 20.0, 17.0, 0.0, 0.0, 20.0]
        );
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
        assert_eq!(preview(0)[28], 10.0);
        assert_eq!(preview(1)[30], 25.0);
        assert_eq!(preview(2)[29], 18.0);
        assert_eq!(preview(3)[29], 12.0);
    }

    #[test]
    fn v27_card_tokens_keep_raw_flags() {
        let content = foundation_content();
        let game = Game::new_character_ascension(&content, 46, 0, 10).unwrap();
        let id = content.card_id("CARD.DAZED").unwrap();
        let plain = Card {
            id,
            ..Card::default()
        };
        let explicit = Card {
            flags: u16::MAX,
            turn_flags: u16::MAX - 1,
            ..plain
        };
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
        assert_ne!(row(plain)[8], row(explicit)[8]);
        assert_eq!(
            row(explicit)[8..10],
            [u16::MAX as f32 + 1.0, u16::MAX as f32]
        );
        assert_eq!(row(explicit)[14..16], [0.0, 0.0]);
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
        combat.enemies[0].instance = 103;
        assert_eq!(even, without_next(state_tokens(&game, &content, layout)));

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
            assert_eq!(row[4], (kind == QUEUED_CARD_KIND) as u8 as f32);
            assert_eq!(row[8..10], [RETAIN as f32 + 1.0, FETCHED as f32 + 1.0]);
            assert_eq!(&row[10..18], &[1.0, 3.0, 4.0, 5.0, 0.0, 0.0, 17.0, 2.0]);
        }
    }

    #[test]
    fn v38_continuations_hide_autoplay_card_instances() {
        let mut content = foundation_content();
        let source = content.card_id("CARD.ZAP").unwrap();
        content.cards[source as usize].effects = Box::leak(Box::new([Effect::AutoPlayRandom(
            Pile::Discard,
            CardFilter::Type(CardType::Attack),
            Amount::fixed(2, 2),
        )]));
        let headbutt = content.card_id("CARD.HEADBUTT").unwrap();
        let defend = content.card_id("CARD.DEFEND_IRONCLAD").unwrap();
        let mut game = Game::new_character_ascension(&content, 76, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.hand = vec![Card {
            id: source,
            ..Card::default()
        }];
        combat.discard = (1001..=1002)
            .map(|instance| Card {
                id: headbutt,
                instance,
                ..Card::default()
            })
            .chain((1003..=1004).map(|instance| Card {
                id: defend,
                instance,
                ..Card::default()
            }))
            .collect();
        combat.energy = 10;
        game.step(
            &content,
            Action::Play {
                hand: 0,
                target: None,
            },
        )
        .unwrap();
        let combat = game.combat().unwrap();
        assert!(combat.choice.is_some());
        assert_eq!(combat.auto_plays.len(), 1);
        assert!(
            combat
                .queue
                .iter()
                .any(|pending| matches!(pending.effect, Effect::AutoPlay(_)))
        );

        let mut renumbered = game.clone();
        let Phase::Combat(combat) = &mut renumbered.phase else {
            unreachable!()
        };
        combat.auto_plays[0].card.instance += 100;
        for pending in &mut combat.queue {
            if let Effect::AutoPlay(mut card) = pending.effect {
                card.instance += 100;
                pending.effect = Effect::AutoPlay(card);
            }
        }
        let layout = Layout::new(&content);
        let observe = |game: &Game| {
            let (actions, legal) = candidate_actions(game, &content);
            (
                observation_globals(game, &content, layout, true),
                state_tokens(game, &content, layout),
                actions
                    .iter()
                    .zip(&legal)
                    .map(|(action, &legal)| {
                        tokenized_candidate(game, &content, layout, action, legal)
                    })
                    .collect::<Vec<_>>(),
                legal,
            )
        };
        assert_eq!(observe(&game), observe(&renumbered));
    }

    #[test]
    fn v28_training_bonus_is_public_and_used_by_previews() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 48, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let tokens = state_tokens_with_bonuses(&game, &content, layout, (24, 12));
        let training = tokens
            .iter()
            .find(|row| row[..2] == [STATE_COLLECTION as f32, 8.0])
            .unwrap();
        assert_eq!(&training[11..13], &[0.75, 0.375]);
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
        assert_eq!(strike[29], 30.0);
        let defend = cards
            .iter()
            .find(|row| row[0] == CARD_COLLECTION as f32 && row[2] == defend.id as f32 + 1.0)
            .unwrap();
        assert_eq!(defend[28], 17.0);
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
            skip_next_decay: false,
            value: 0,
        })
        .collect();
        combat.enemies[0].creature.hp = 1000;
        combat.enemies[0].creature.max_hp = 1000;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers = vec![Power {
            id: power_id::VULNERABLE,
            amount: 1,
            skip_next_decay: false,
            value: 0,
        }];
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let mut values = compact_action_values(&game, Layout::new(&content), &action);
        let mut tokens = Vec::new();
        action_card_tokens(&game, &content, &action, &mut values, &mut tokens);
        assert_eq!(tokens[0][29], 24.0);
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
        assert_eq!(row(&game, None).0[0][28], 7.0);

        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![reboot, strike, strike, strike];
        combat.draw = vec![strike];
        combat.discard = vec![strike, strike];
        assert_eq!(row(&game, None).0[0][31], 4.0);

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
        assert_eq!((card[29], card[33]), (24.0, 3.0));
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
            skip_next_decay: false,
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
            skip_next_decay: false,
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
                .unwrap()[27],
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
            skip_next_decay: false,
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
        assert_eq!((card[29], card[30]), (0.0, 4.0));

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
            skip_next_decay: false,
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
        assert_eq!(card[29], 0.0);

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
        assert_eq!(card[30], losses[0] as f32);
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
            skip_next_decay: false,
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
        assert_eq!((card[27], card[28], card[30]), (3.0, 6.0, 10.0));

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
    fn v44_regent_generated_block_previews_match_play() {
        let content = foundation_content();
        let mut base = Game::new_character_ascension(&content, 64, 3, 10).unwrap();
        base.begin_act(&content, 0).unwrap();
        base.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        base.run
            .relics
            .push(content.relic_id("RELIC.REGALITE").unwrap());
        let Phase::Combat(combat) = &mut base.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 999;
        combat.enemies[0].creature.max_hp = 999;
        combat.player.block = 0;
        combat.energy = 10;

        let check = |game: Game, action: Action, expected| {
            let preview = action_preview(&game, &content, &action).unwrap().block;
            let before = game.combat().unwrap().player.block;
            let mut played = game.clone();
            played.step(&content, action).unwrap();
            assert_eq!(preview, expected);
            assert_eq!(played.combat().unwrap().player.block - before, expected);
        };
        let with_card = |id, upgrades| {
            let mut game = base.clone();
            let Phase::Combat(combat) = &mut game.phase else {
                unreachable!()
            };
            combat.hand = vec![Card {
                id: content.card_id(id).unwrap(),
                upgrades,
                ..Card::default()
            }];
            game
        };
        check(
            with_card("CARD.COLLISION_COURSE", 0),
            Action::Play {
                hand: 0,
                target: Some(0),
            },
            2,
        );
        check(
            with_card("CARD.BLADE_OF_INK", 0),
            Action::Play {
                hand: 0,
                target: None,
            },
            4,
        );
        check(
            with_card("CARD.BIG_BANG", 0),
            Action::Play {
                hand: 0,
                target: None,
            },
            2,
        );
        let mut game = base;
        game.run.potions = vec![Some(content.potion_id("POTION.CUNNING_POTION").unwrap())];
        check(
            game,
            Action::Potion {
                slot: 0,
                target: None,
            },
            6,
        );
    }

    #[test]
    fn v45_random_generated_block_previews_match_play() {
        let content = foundation_content();
        let mut base = Game::new_character_ascension(&content, 66, 3, 10).unwrap();
        base.begin_act(&content, 0).unwrap();
        base.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        base.run
            .relics
            .push(content.relic_id("RELIC.REGALITE").unwrap());
        let Phase::Combat(combat) = &mut base.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 999;
        combat.enemies[0].creature.max_hp = 999;
        combat.player.block = 0;
        combat.energy = 10;

        let check = |game: Game, action: Action, expected| {
            let preview = action_preview(&game, &content, &action).unwrap().block;
            let before = game.combat().unwrap().player.block;
            let mut played = game.clone();
            played.step(&content, action).unwrap();
            assert_eq!(preview, expected);
            assert_eq!(played.combat().unwrap().player.block - before, expected);
        };
        let with_card = |id, target, expected| {
            let mut game = base.clone();
            let Phase::Combat(combat) = &mut game.phase else {
                unreachable!()
            };
            combat.hand = vec![Card {
                id: content.card_id(id).unwrap(),
                ..Card::default()
            }];
            check(game, Action::Play { hand: 0, target }, expected);
        };
        with_card("CARD.BUNDLE_OF_JOY", None, 6);
        with_card("CARD.WHITE_NOISE", None, 2);
        with_card("CARD.JACK_OF_ALL_TRADES", None, 2);
        with_card("CARD.JACKPOT", Some(0), 6);
        let mut game = base;
        game.run.potions = vec![Some(content.potion_id("POTION.COSMIC_CONCOCTION").unwrap())];
        check(
            game,
            Action::Potion {
                slot: 0,
                target: None,
            },
            6,
        );
    }

    #[test]
    fn v45_character_generated_block_previews_match_play() {
        const EFFECTS: [&[Effect]; 2] = [
            &[Effect::RandomCharacter(Pile::Hand, Amount::fixed(2, 2), 0)],
            &[Effect::DistinctCharacter(Pile::Hand, 2, false)],
        ];
        for effects in EFFECTS {
            let mut content = foundation_content();
            let card = content.card_id("CARD.STRIKE_REGENT").unwrap();
            content.cards[card as usize].effects = effects;
            let mut game = Game::new_character_ascension(&content, 67, 3, 10).unwrap();
            game.begin_act(&content, 0).unwrap();
            game.start_combat(&content, content.acts[0].encounters[0])
                .unwrap();
            game.run
                .relics
                .push(content.relic_id("RELIC.REGALITE").unwrap());
            let Phase::Combat(combat) = &mut game.phase else {
                unreachable!()
            };
            combat.enemies.truncate(1);
            combat.hits.truncate(1);
            combat.enemies[0].creature.hp = 999;
            combat.enemies[0].creature.max_hp = 999;
            combat.hand = vec![Card {
                id: card,
                ..Card::default()
            }];
            combat.energy = 10;
            combat.player.block = 0;
            let action = Action::Play {
                hand: 0,
                target: Some(0),
            };
            let preview = action_preview(&game, &content, &action).unwrap().block;
            let mut played = game.clone();
            played.step(&content, action).unwrap();
            assert_eq!(preview, 4);
            assert_eq!(played.combat().unwrap().player.block, preview);
        }
    }

    #[test]
    fn v44_galactic_dust_block_preview_matches_play() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 65, 3, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run
            .relics
            .push(content.relic_id("RELIC.GALACTIC_DUST").unwrap());
        game.galactic_dust = 9;
        let card = Card {
            id: content.card_id("CARD.FALLING_STAR").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 999;
        combat.enemies[0].creature.max_hp = 999;
        combat.hand = vec![card];
        combat.stars = 2;
        combat.player.block = 0;
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        let preview = played_card_preview(&game, &content, card, Some(0));
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        assert_eq!(preview.block, 10);
        assert_eq!(played.combat().unwrap().player.block, preview.block);
        assert_eq!(played.galactic_dust, 1);
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
            skip_next_decay: false,
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
            skip_next_decay: false,
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
            skip_next_decay: false,
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
        assert_eq!((card[28], values[52]), (5.0, 4.0));
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
                .unwrap()[31],
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
                .unwrap()[31],
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
        assert_eq!((card[31], card[32]), (2.0, 1.0));
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
                skip_next_decay: false,
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
        assert_eq!((card[28], card[30], card[31]), (39.0, 5.0, 1.0));
        assert!(tokens.iter().any(|row| {
            row[..2] == [ENEMY_COLLECTION as f32, 4.0] && (row[13], row[14]) == (5.0, 5.0)
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
        assert_eq!(card[28], 4.0);
        assert_eq!((random[13], random[14], random[18]), (6.0, 6.0, 1.0));

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
            skip_next_decay: false,
            value: 0,
        });
        combat.enemies[1].creature.powers.push(Power {
            id: power_id::HARD_TO_KILL,
            amount: 3,
            skip_next_decay: false,
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
            &first[10..19],
            &[1.0, 2.0, 0.0, 7.0, 5.0, 0.0, 0.0, 20.0, 1.0]
        );
        assert_eq!(
            &second[10..19],
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
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::WEAK,
                amount: 1,
                skip_next_decay: false,
                value: 0,
            },
        ];
        combat.enemies[0].creature.hp = 5;
        combat.enemies[0].creature.max_hp = 5;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers = vec![Power {
            id: power_id::VULNERABLE,
            amount: 1,
            skip_next_decay: false,
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
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::HARDENED_SHELL,
                amount: 5,
                skip_next_decay: false,
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
            skip_next_decay: false,
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
            skip_next_decay: false,
            value: 0,
        }];
        combat.enemies[0].creature.hp = 50;
        combat.enemies[0].creature.max_hp = 50;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers = vec![
            Power {
                id: power_id::SLOW,
                amount: 50,
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::FLUTTER,
                amount: 1,
                skip_next_decay: false,
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
            skip_next_decay: false,
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
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::WEAK,
                amount: 1,
                skip_next_decay: false,
                value: 0,
            },
        ];
        combat.enemies[0].creature.hp = 20;
        combat.enemies[0].creature.max_hp = 20;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers = vec![Power {
            id: power_id::VULNERABLE,
            amount: 1,
            skip_next_decay: false,
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
        let state = state_signature(&paired, layout);
        if let Phase::Combat(combat) = &mut paired.phase {
            combat.hand.reverse();
        }
        assert_eq!(state_signature(&paired, layout), state);

        let mut split = paired.clone();
        let Phase::Combat(combat) = &mut split.phase else {
            unreachable!()
        };
        combat.hand[0].free = true;
        combat.hand[1].free = false;
        assert_ne!(state_signature(&split, layout), state);

        let mut override_cost = paired;
        let Phase::Combat(combat) = &mut override_cost.phase else {
            unreachable!()
        };
        combat.hand[0].cost_override = Some(-1);
        assert_ne!(state_signature(&override_cost, layout), state);
    }

    #[test]
    fn card_multisets_keep_every_occurrence() {
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
        let cards = state_tokens(&game, &content, layout)
            .into_iter()
            .filter(|row| {
                row[0] == CARD_COLLECTION as f32 && row[1] == 3.0 && row[2] == id as f32 + 1.0
            })
            .collect::<Vec<_>>();
        assert_eq!(cards.len(), 2048);
        assert!(cards.iter().all(|row| row[4] == 0.0));
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
            action_signature(&game, layout, &Action::Choose(0)),
            action_signature(&game, layout, &Action::Choose(1))
        );
        let mut left = game.clone();
        let mut right = game.clone();
        left.step(&content, Action::Choose(0)).unwrap();
        right.step(&content, Action::Choose(1)).unwrap();
        assert_eq!(
            state_signature(&left, layout),
            state_signature(&right, layout)
        );
        assert_eq!(left.actions(&content), right.actions(&content));

        if let Phase::Combat(combat) = &mut game.phase {
            combat.hand[0].enchantment = Some(Enchantment::Adroit);
            combat.hand[1].enchantment = Some(Enchantment::Momentum);
        }
        assert_ne!(
            action_signature(&game, layout, &Action::Choose(0)),
            action_signature(&game, layout, &Action::Choose(1))
        );

        if let Phase::Combat(combat) = &mut game.phase {
            combat.hand[0].enchantment = None;
            combat.hand[1].enchantment = None;
            combat.dampened = vec![(101, 1), (102, 2)];
        }
        assert_ne!(
            action_signature(&game, layout, &Action::Choose(0)),
            action_signature(&game, layout, &Action::Choose(1))
        );
        let mut left = game.clone();
        let mut right = game;
        left.step(&content, Action::Choose(0)).unwrap();
        right.step(&content, Action::Choose(1)).unwrap();
        assert_ne!(
            state_signature(&left, layout),
            state_signature(&right, layout)
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
        let first = tokenized_action(&game, &content, layout, &Action::Choose(0));
        let second = tokenized_action(&game, &content, layout, &Action::Choose(1));
        assert_ne!(first, second);

        let mut left = game.clone();
        let mut right = game;
        left.step(&content, Action::Choose(0)).unwrap();
        right.step(&content, Action::Choose(1)).unwrap();
        assert_ne!(
            state_signature(&left, layout),
            state_signature(&right, layout)
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
            state_signature(&left, layout),
            state_signature(&right, layout)
        );
        assert_ne!(
            action_signature(&left, layout, &action),
            action_signature(&right, layout, &action)
        );
        left.step(&content, action.clone()).unwrap();
        right.step(&content, action).unwrap();
        assert_ne!(
            state_signature(&left, layout),
            state_signature(&right, layout)
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
        let mut left_continuation = vec![];
        let mut right_continuation = vec![];
        continuation_tokens(&left, &content, &mut left_continuation);
        continuation_tokens(&right, &content, &mut right_continuation);
        assert_ne!(left_continuation, right_continuation);
        assert_ne!(
            state_tokens(&left, &content, layout),
            state_tokens(&right, &content, layout)
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
            skip_next_decay: false,
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
                && row[1] == (PLAYING_ZONE + 1) as f32
                && row[2] == burning_pact.id as f32 + 1.0
                && row[4] == 0.0
        }));
        assert!(
            tokens
                .iter()
                .filter(|row| row[0] == CARD_COLLECTION as f32)
                .all(|row| row[4] == 0.0 || row[1] == (DRAW_ZONE + 1) as f32)
        );
        let state = state_signature(&manual, layout);
        let mut continuation = vec![];
        continuation_tokens(&manual, &content, &mut continuation);
        assert!(!continuation.is_empty());
        let mut changed = manual.clone();
        if let Phase::Combat(combat) = &mut changed.phase {
            combat.queue.clear();
        }
        assert_ne!(state_signature(&changed, layout), state);
        manual.step(&content, Action::Choose(0)).unwrap();
        assert!(manual.combat().unwrap().playing.is_none());
        continuation.clear();
        continuation_tokens(&manual, &content, &mut continuation);
        assert!(continuation.is_empty());

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
                    row[0] == CARD_COLLECTION as f32
                        && row[1] == (AUTOPLAY_ZONE + 1) as f32
                        && row[4] == 0.0
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
        continuation.clear();
        continuation_tokens(&automatic, &content, &mut continuation);
        assert!(!continuation.is_empty());
        automatic.step(&content, Action::Choose(0)).unwrap();
        assert!(automatic.combat().unwrap().auto_plays.is_empty());
        continuation.clear();
        continuation_tokens(&automatic, &content, &mut continuation);
        assert!(continuation.is_empty());
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
                skip_next_decay: false,
            },
            Power {
                id: power_id::STRENGTH,
                amount: 2,
                value: 20,
                skip_next_decay: true,
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
                skip_next_decay: true,
                ..powers[0]
            },
            Power {
                value: 10,
                skip_next_decay: false,
                ..powers[1]
            },
        ];
        assert_ne!(
            state_signature(&left, layout),
            state_signature(&right, layout)
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
                skip_next_decay: false,
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
                skip_next_decay: false,
            },
            Power {
                id: power_id::STRENGTH,
                amount: 2,
                value: 20,
                skip_next_decay: true,
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
                skip_next_decay: true,
                ..powers[0]
            },
            Power {
                value: 10,
                skip_next_decay: false,
                ..powers[1]
            },
        ];
        assert_ne!(
            state_signature(&left, layout),
            state_signature(&right, layout)
        );
        let Phase::Combat(combat) = &mut right.phase else {
            unreachable!()
        };
        combat.power_snapshot = vec![powers[1], powers[0]];
        assert_ne!(
            state_signature(&left, layout),
            state_signature(&right, layout)
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
            state_signature(&doubled, layout),
            state_signature(&normal, layout)
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
            action_signature(&game, layout, &target(0)),
            action_signature(&game, layout, &target(1))
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
        let discard = state_signature(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.choice.as_mut().unwrap().op = CardOp::Move(Pile::Exhaust);
        }
        assert_ne!(state_signature(&game, layout), discard);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.choice.as_mut().unwrap().op = CardOp::Move(Pile::Discard);
            combat.choice.as_mut().unwrap().filter = CardFilter::Upgradable;
        }
        assert_ne!(state_signature(&game, layout), discard);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.choice.as_mut().unwrap().filter = CardFilter::Any;
            combat.enemy_turn = true;
        }
        assert_ne!(state_signature(&game, layout), discard);
    }

    #[test]
    fn features_cover_resume_phase() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 8, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.phase = Phase::TransformCards(None, 1, false);
        let no_resume = state_signature(&game, layout);
        let no_resume_tokens = state_tokens(&game, &content, layout);
        game.resume = Some(Phase::Map);
        assert_ne!(state_signature(&game, layout), no_resume);
        assert_ne!(state_tokens(&game, &content, layout), no_resume_tokens);

        game.resume = Some(Phase::Rewards(Rewards {
            gold: 10,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        }));
        let reward = state_signature(&game, layout);
        let mut changed = game.clone();
        let Some(Phase::Rewards(rewards)) = &mut changed.resume else {
            unreachable!()
        };
        rewards.gold = 20;
        assert_ne!(state_signature(&changed, layout), reward);
        assert_ne!(
            state_tokens(&changed, &content, layout),
            state_tokens(&game, &content, layout)
        );

        let card = game.run.deck[0];
        game.resume = Some(Phase::Shop(vec![ShopItem::Card(card, 50)]));
        let shop = state_signature(&game, layout);
        let mut changed = game.clone();
        let Some(Phase::Shop(items)) = &mut changed.resume else {
            unreachable!()
        };
        let ShopItem::Card(_, price) = &mut items[0] else {
            unreachable!()
        };
        *price = 51;
        assert_ne!(state_signature(&changed, layout), shop);

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
        assert_eq!(state_signature(&renamed, layout), shop);

        game.resume = Some(Phase::Event(0, content.events[0].options.to_vec()));
        let event = state_signature(&game, layout);
        game.resume = Some(Phase::Event(1, content.events[1].options.to_vec()));
        assert_ne!(state_signature(&game, layout), event);
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
            state_signature(&first, layout),
            state_signature(&second, layout)
        );
        first.step(&content, Action::RemoveCard(0)).unwrap();
        second.step(&content, Action::RemoveCard(0)).unwrap();
        assert_ne!(
            state_signature(&first, layout),
            state_signature(&second, layout)
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
                summary_signature(game, layout),
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
        let expected = state_signature(&game, layout);
        let expected_observation = observe(&game);
        assert!(expected.iter().all(|value| value.is_finite()));
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
        assert_eq!(state_signature(&reordered, layout), expected);
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
                state_signature(&left, layout),
                state_signature(&right, layout)
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
        let state = state_signature(&game, layout);

        let mut reordered = game.clone();
        for bag in reordered
            .relic_deques
            .iter_mut()
            .chain(&mut reordered.shared_relic_deques)
        {
            bag.reverse();
        }
        assert_eq!(state_signature(&reordered, layout), state);

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
        assert_ne!(state_signature(&missing, layout), state);
        assert!(state_tokens(&missing, &content, layout).contains(&token(
            RELIC_COLLECTION,
            SEEN_RELIC_KIND,
            relic as usize + 1,
            1,
            0,
        )));
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
        assert_ne!(state_signature(&missing, layout), state);
        assert!(state_tokens(&missing, &content, layout).contains(&token(
            RELIC_COLLECTION,
            SEEN_RELIC_KIND,
            relic as usize + 1,
            2,
            0,
        )));

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
            state_signature(&first, layout),
            state_signature(&reordered, layout)
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
        std::sync::Arc::make_mut(&mut treasure_first.map.nodes)[node].room = Room::Treasure;
        std::sync::Arc::make_mut(&mut treasure_reordered.map.nodes)[node].room = Room::Treasure;
        treasure_first.step(&content, Action::Path(node)).unwrap();
        treasure_reordered
            .step(&content, Action::Path(node))
            .unwrap();
        assert_eq!(
            state_signature(&treasure_first, layout),
            state_signature(&treasure_reordered, layout)
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
            state_signature(&first, layout),
            state_signature(&second, layout)
        );
        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert!(first.event_relic.is_none() && first.relic_queue.is_empty());
        assert_eq!(first.relic_deques, second.relic_deques);
        assert_eq!(first.shared_relic_deques, second.shared_relic_deques);
        first.step(&content, Action::Event(1)).unwrap();
        second.step(&content, Action::Event(1)).unwrap();
        assert_eq!(
            state_signature(&first, layout),
            state_signature(&second, layout)
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
                state_signature(&first, layout),
                state_signature(&second, layout)
            );
            resample_hidden(&mut first, &content, 42);
            resample_hidden(&mut second, &content, 42);
            first.step(&content, Action::Event(0)).unwrap();
            second.step(&content, Action::Event(0)).unwrap();
            assert_eq!(
                state_signature(&first, layout),
                state_signature(&second, layout)
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

        assert_ne!(
            state_signature(&first, layout),
            state_signature(&second, layout)
        );
        assert_eq!(
            summary_signature(&first, layout),
            summary_signature(&second, layout)
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

        let state = state_signature(&game, layout);
        let tokens = state_tokens(&game, &content, layout);
        let action = tokenized_action(&game, &content, layout, &Action::Event(0));
        let mut swapped = game.clone();
        swapped.relic_queue.swap(0, 1);
        assert_ne!(state_signature(&swapped, layout), state);
        assert_ne!(state_tokens(&swapped, &content, layout), tokens);
        assert_ne!(
            tokenized_action(&swapped, &content, layout, &Action::Event(0)),
            action
        );

        let mut first = game.clone();
        let mut second = game;
        second.rngs.rewards.next();
        assert_eq!(
            state_signature(&first, layout),
            state_signature(&second, layout)
        );
        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert_eq!(first.relic_queue, second.relic_queue);
        first.step(&content, Action::Event(0)).unwrap();
        second.step(&content, Action::Event(0)).unwrap();
        assert!(first.relic_queue.is_empty() && second.relic_queue.is_empty());
        assert_eq!(
            state_signature(&first, layout),
            state_signature(&second, layout)
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
        let public = state_signature(&first, layout);
        assert_eq!(state_signature(&second, layout), public);

        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert_eq!(state_signature(&first, layout), public);
        assert_eq!(state_signature(&second, layout), public);
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
            state_signature(&first, layout),
            state_signature(&second, layout)
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
        let state = state_signature(&game, layout);
        let (actions, legal) = candidate_actions(&game, &content);
        let mut features = actions
            .iter()
            .zip(legal)
            .map(|(action, legal)| {
                let mut row = action_signature(&game, layout, action)
                    .into_iter()
                    .map(f32::to_bits)
                    .collect::<Vec<_>>();
                row.push(legal as u32);
                row
            })
            .collect::<Vec<_>>();
        features.sort();
        let mut changed = false;
        for seed in 1..=8 {
            let mut sampled = game.clone();
            resample_hidden(&mut sampled, &content, seed);
            changed |= sampled.combat().unwrap().draw != game.combat().unwrap().draw;
            assert_eq!(state_signature(&sampled, layout), state);
            let (sampled_actions, sampled_legal) = candidate_actions(&sampled, &content);
            assert_eq!(sampled_actions.len(), actions.len());
            let mut sampled_features = sampled_actions
                .iter()
                .zip(sampled_legal)
                .map(|(action, legal)| {
                    let mut row = action_signature(&sampled, layout, action)
                        .into_iter()
                        .map(f32::to_bits)
                        .collect::<Vec<_>>();
                    row.push(legal as u32);
                    row
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
        let state = state_signature(&first, layout);

        let mut second = first.clone();
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.draw.swap(1, 2);
        assert_eq!(state_signature(&second, layout), state);
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.draw.swap(2, 3);
        assert_ne!(state_signature(&second, layout), state);
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
            state_signature(&first, layout),
            state_signature(&second, layout)
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
        let state = state_signature(&game, layout);
        let mut hidden = game.clone();
        let last = hidden.combat().unwrap().draw.len() - 1;
        let Phase::Combat(combat) = &mut hidden.phase else {
            unreachable!()
        };
        combat.draw.swap(0, last);
        assert_ne!(state_signature(&hidden, layout), state);
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
        let before = state_signature(&game, layout);
        game.bosses[1] = Some(0);
        assert_eq!(state_signature(&game, layout), before);
        game.bosses[0] = content.acts[game.run.act as usize]
            .bosses
            .iter()
            .copied()
            .find(|boss| Some(*boss) != game.bosses[0]);
        assert_ne!(state_signature(&game, layout), before);
        let before = state_signature(&game, layout);
        game.run.hp -= 1;
        assert_ne!(state_signature(&game, layout), before);
        let before = state_signature(&game, layout);
        std::sync::Arc::make_mut(&mut game.map.nodes)[0]
            .next
            .clear();
        assert_ne!(state_signature(&game, layout), before);
        game.run
            .relics
            .push(content.relic_id("RELIC.FISHING_ROD").unwrap());
        let before = state_signature(&game, layout);
        game.fishing_rod = 1;
        assert_ne!(state_signature(&game, layout), before);
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        if let Phase::Combat(combat) = &mut game.phase {
            combat.orb_slots = 1;
        }
        game.run
            .relics
            .push(content.relic_id("RELIC.PAELS_TEARS").unwrap());
        let before = state_signature(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.orbs.push(Orb { id: 0, value: 7 });
        }
        assert_ne!(state_signature(&game, layout), before);
        game.run
            .relics
            .push(content.relic_id("RELIC.DIAMOND_DIADEM").unwrap());
        let before = state_signature(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.hand[0].turn_flags ^= 1;
        }
        assert_ne!(state_signature(&game, layout), before);
        let before = state_signature(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.paels_tears = !combat.paels_tears;
        }
        assert_ne!(state_signature(&game, layout), before);
        let before = state_signature(&game, layout);
        if let Phase::Combat(combat) = &mut game.phase {
            combat.diamond_diadem = !combat.diamond_diadem;
        }
        assert_ne!(state_signature(&game, layout), before);
    }

    #[test]
    fn features_cover_public_run_state_machines() {
        fn changed(game: &Game, layout: Layout, edit: impl FnOnce(&mut Game)) {
            let state = state_signature(game, layout);
            let mut edited = game.clone();
            edit(&mut edited);
            assert_ne!(state_signature(&edited, layout), state);
        }

        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 15, 0, 10).unwrap();
        let act = content
            .acts
            .iter()
            .position(|act| !act.events.is_empty())
            .unwrap() as Id;
        game.begin_act(&content, act).unwrap();
        let event = content.acts[game.act as usize].events[0];
        changed(&game, layout, |game| game.visited_events.push(event));
        let relic_changed = |name, edit: fn(&mut Game)| {
            let mut game = game.clone();
            game.run.relics.push(content.relic_id(name).unwrap());
            changed(&game, layout, edit);
        };
        relic_changed("RELIC.NEOWS_BONES", |game| game.pending_curse = true);
        relic_changed("RELIC.WONGOS_MYSTERY_TICKET", |game| {
            game.wongo_combats = Some(0)
        });
        let mut wongo = game.clone();
        wongo
            .run
            .relics
            .push(content.relic_id("RELIC.WONGOS_MYSTERY_TICKET").unwrap());
        wongo.wongo_combats = Some(1);
        changed(&wongo, layout, |game| game.wongo_combats = Some(2));
        relic_changed("RELIC.GOLDEN_COMPASS", |game| {
            game.golden_compass = Some(game.run.act)
        });
        relic_changed("RELIC.ASTROLABE", |game| game.astrolabe = true);
        relic_changed("RELIC.ASTROLABE", |game| game.transform_niche = true);
        relic_changed("RELIC.PAELS_TOOTH", |game| game.paels_tooth = true);
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
        assert_ne!(state_tokens(&reward, &content, layout), state);
        assert_ne!(
            tokenized_action(&reward, &content, layout, &Action::RewardRelic(0)),
            action
        );

        let mut event = game;
        event.phase = Phase::Event(0, vec![]);
        let state = state_signature(&event, layout);
        event.event_relic = Some(relic);
        assert_eq!(state_signature(&event, layout), state);
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
            state_signature(&game, layout),
            state_signature(&reversed, layout)
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
        let state = state_signature(&fake, layout);
        fake.fake_merchant.pop();
        assert_ne!(state_signature(&fake, layout), state);

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
            state_signature(&first, layout),
            state_signature(&second, layout)
        );
        first.step(&content, Action::RewardGold).unwrap();
        second.step(&content, Action::RewardGold).unwrap();
        assert_ne!(first.run.gold, second.run.gold);
        assert_ne!(
            state_signature(&first, layout),
            state_signature(&second, layout)
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
            skip_next_decay: false,
            value: 0,
        });
        let state = state_signature(&game, layout);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemy_power_snapshot[ENEMY_SLOTS][0].value = 1;
        assert_ne!(state_signature(&game, layout), state);
        let state = state_signature(&game, layout);
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies[ENEMY_SLOTS].creature.hp -= 1;
        assert_ne!(state_signature(&game, layout), state);

        let target = |target| {
            action_entity_tokens(
                &game,
                &content,
                layout,
                &Action::Play {
                    hand: 0,
                    target: Some(target),
                },
            )
        };
        let eighth = target(ENEMY_SLOTS - 1);
        let ninth = target(ENEMY_SLOTS);
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
            state_signature(&first, layout),
            state_signature(&second, layout)
        );
        assert_eq!(
            action_signature(&first, layout, &Action::EndTurn),
            action_signature(&second, layout, &Action::EndTurn)
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
        let state = state_signature(&game, layout);

        let mut changed = game.clone();
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.osty.max_hp += 1;
        assert_ne!(state_signature(&changed, layout), state);

        let mut changed = game.clone();
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.hits[0] += 1;
        assert_ne!(state_signature(&changed, layout), state);

        for (relic, field) in [
            ("RELIC.BURNING_STICKS", 0),
            ("RELIC.THROWING_AXE", 1),
            ("RELIC.PAELS_EYE", 2),
            ("RELIC.PAELS_EYE", 3),
        ] {
            let mut changed = game.clone();
            changed.run.relics.push(content.relic_id(relic).unwrap());
            let state = state_signature(&changed, layout);
            let Phase::Combat(combat) = &mut changed.phase else {
                unreachable!()
            };
            match field {
                0 => combat.burning_sticks = true,
                1 => combat.throwing_axe = true,
                2 => combat.paels_eye = true,
                _ => combat.paels_eye_extra = true,
            }
            assert_ne!(state_signature(&changed, layout), state);
        }

        let mut changed = game.clone();
        changed
            .run
            .relics
            .push(content.relic_id("RELIC.PAELS_LEGION").unwrap());
        let state = state_signature(&changed, layout);
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.paels_legion = 2;
        assert_ne!(state_signature(&changed, layout), state);

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
            assert_ne!(state_signature(&changed, layout), state);
        }

        let mut changed = game;
        let Phase::Combat(combat) = &mut changed.phase else {
            unreachable!()
        };
        combat.history_course = Some(Card::default());
        assert_ne!(state_signature(&changed, layout), state);
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
            state_signature(&first, layout),
            state_signature(&second, layout)
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
            state_signature(&first, layout),
            state_signature(&second, layout)
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
        let public = state_signature(&first, layout);
        assert_eq!(state_signature(&second, layout), public);
        let mut different_first_boss = first.clone();
        different_first_boss.bosses[0] = Some(choices[0]);
        assert_ne!(state_signature(&different_first_boss, layout), public);
        assert_eq!(
            observation_globals(&different_first_boss, &content, layout, true),
            observation_globals(&first, &content, layout, true)
        );

        resample_hidden(&mut first, &content, 42);
        resample_hidden(&mut second, &content, 42);
        assert_eq!(first.bosses[1], second.bosses[1]);
        assert_eq!(state_signature(&first, layout), public);
        assert_eq!(state_signature(&second, layout), public);

        let next = first.map.nodes[current].next[0];
        first.step(&content, Action::Path(next)).unwrap();
        second.step(&content, Action::Path(next)).unwrap();
        assert_eq!(
            state_signature(&first, layout),
            state_signature(&second, layout)
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
    fn encounter_bag_membership_is_public_but_order_is_hidden() {
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
            ]
            .into(),
            current: None,
        };
        game.phase = Phase::Map;
        let state = state_signature(&game, layout);
        let mut changed = game.clone();
        if changed.weak_encounters_left > 0 {
            changed.weak_encounters_left -= 1;
        } else {
            changed.regular_encounters_left -= 1;
        }
        assert_ne!(state_signature(&changed, layout), state);
        let mut changed = game.clone();
        changed.encounters.pop();
        assert_ne!(state_signature(&changed, layout), state);
        let mut changed = game.clone();
        changed.last_elite = None;
        assert_ne!(state_signature(&changed, layout), state);
        let mut changed = game.clone();
        changed.act = (changed.act + 1) % content.acts.len() as Id;
        assert_ne!(state_signature(&changed, layout), state);

        let mut reordered = game.clone();
        reordered.encounters.reverse();
        reordered.elites.reverse();
        reordered.rngs.up_front.next();
        assert_eq!(state_signature(&reordered, layout), state);
        let pool = public_encounter_pool(&game, &content, false);
        let mut first_replay = game.clone();
        first_replay.replaying = true;
        first_replay.encounters = pool.clone();
        let mut second_replay = first_replay.clone();
        let forced = second_replay
            .encounters
            .iter()
            .position(|&id| id == pool[1])
            .unwrap();
        second_replay.encounters.swap(0, forced);
        assert_eq!(
            v56_bytes(&observation_v56(&first_replay, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second_replay, &content, layout, (0, 0)))
        );
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
                    state_signature(&left, layout),
                    state_signature(&right, layout)
                );
                assert_eq!(left.actions(&content), right.actions(&content));
            }
        }
    }

    #[test]
    fn v56_distinguishes_remaining_encounter_bags_with_the_same_last_encounter() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let (act, first_id, shared, second_id) = content
            .acts
            .iter()
            .enumerate()
            .find_map(|(act, def)| {
                let pool = def
                    .encounters
                    .iter()
                    .copied()
                    .filter(|id| !content.encounters[*id as usize].id.contains("_WEAK"))
                    .collect::<Vec<_>>();
                pool.iter().copied().find_map(|shared| {
                    let compatible = pool
                        .iter()
                        .copied()
                        .filter(|&id| {
                            id != shared
                                && encounter_tags(content.encounters[id as usize].id)
                                    & encounter_tags(content.encounters[shared as usize].id)
                                    == 0
                        })
                        .collect::<Vec<_>>();
                    (compatible.len() >= 2).then_some((
                        act as Id,
                        compatible[0],
                        shared,
                        compatible[1],
                    ))
                })
            })
            .unwrap();
        let mut game = Game::new_character_ascension(&content, 568, 0, 10).unwrap();
        game.begin_act(&content, act).unwrap();
        game.weak_encounters_left = 0;
        game.regular_encounters_left = 3;
        game.encounters = vec![first_id, shared, second_id];
        let reach = |wanted| {
            (0..1024)
                .find_map(|seed| {
                    let mut reached = game.clone();
                    reached.rngs.up_front = Rng::from_seed(seed);
                    (reached.next_encounter(&content, false) == Some(wanted)
                        && reached.next_encounter(&content, false) == Some(shared))
                    .then_some(reached)
                })
                .unwrap()
        };
        let mut first = reach(first_id);
        let mut second = reach(second_id);
        assert_eq!(first.last_encounter, second.last_encounter);
        assert_ne!(first.encounters, second.encounters);
        assert_ne!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );
        assert_eq!(first.next_encounter(&content, false), Some(second_id));
        assert_eq!(second.next_encounter(&content, false), Some(first_id));
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
            }]
            .into(),
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
    fn action_tokens_cover_visible_event_options() {
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
        let action = action_signature(&game, layout, &Action::Event(0));
        game.phase = Phase::Event(
            0,
            vec![EventOption {
                requirement: Requirement::Hp(10),
                effects: &[RunEffect::LoseHp(5)],
            }],
        );
        assert_ne!(action_signature(&game, layout, &Action::Event(0)), action);
    }

    #[test]
    fn v43_current_event_pages_are_state() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let event = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.ROUND_TEA_PARTY")
            .unwrap() as Id;
        let mut game = Game::new_character_ascension(&content, 80, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.phase = Phase::Event(event, content.events[event as usize].options.to_vec());
        let before = state_signature(&game, layout);
        game.step(&content, Action::Event(1)).unwrap();
        assert!(
            matches!(&game.phase, Phase::Event(id, options) if *id == event && options.len() == 1)
        );
        assert_ne!(state_signature(&game, layout), before);
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
        let state = state_signature(&game, layout);
        game.event_data[0] = 150;
        assert_ne!(state_signature(&game, layout), state);

        let ranwid = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.RANWID_THE_ELDER")
            .unwrap() as Id;
        game.phase = Phase::Event(ranwid, vec![]);
        game.event_data = [0; 4];
        game.event_cards.clear();
        let state = state_signature(&game, layout);
        game.event_cards.push(0);
        assert_ne!(state_signature(&game, layout), state);

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
        let state = state_signature(&game, layout);
        let tokens = state_tokens(&game, &content, layout);
        let action = tokenized_action(&game, &content, layout, &Action::Event(0));
        for card in &mut game.run.deck {
            card.instance += 100;
        }
        game.event_data[1] += 100;
        game.event_cards[0] += 100;
        assert_eq!(state_signature(&game, layout), state);
        assert_eq!(state_tokens(&game, &content, layout), tokens);
        game.event_data[1] = game
            .run
            .deck
            .iter()
            .find(|card| card.id != game.run.deck[0].id)
            .unwrap()
            .instance as i64;
        assert_ne!(state_signature(&game, layout), state);
        assert_ne!(state_tokens(&game, &content, layout), tokens);
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
                state_signature(&first, layout),
                state_signature(&second, layout)
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
        let state = state_signature(&game, layout);
        assert!(game.actions(&content).contains(&Action::RerollCards));
        game.rerolled_cards = true;
        assert_ne!(state_signature(&game, layout), state);
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
            let public = state_signature(&game, layout);
            let mut no_queue = game.clone();
            if let Phase::Rewards(rewards) = &mut no_queue.phase {
                rewards.card_rewards.clear();
            }
            assert_ne!(state_signature(&no_queue, layout), public);

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
                    state_signature(&first, layout),
                    state_signature(&second, layout)
                );
                resample_hidden(&mut first, &content, 42);
                resample_hidden(&mut second, &content, 42);
                first.step(&content, action.clone()).unwrap();
                second.step(&content, action).unwrap();
                assert_eq!(
                    state_signature(&first, layout),
                    state_signature(&second, layout)
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
        let types = state_signature(&game, layout);
        let type_actions = [
            action_signature(&game, layout, &Action::Event(0)),
            action_signature(&game, layout, &Action::Event(1)),
        ];
        game.event_data = [3, 1, 0, 1];
        assert_ne!(state_signature(&game, layout), types);
        assert_eq!(
            [
                action_signature(&game, layout, &Action::Event(0)),
                action_signature(&game, layout, &Action::Event(1)),
            ],
            type_actions
        );
        game.event_data = [5, 4, 0, 2];
        let riders = state_signature(&game, layout);
        let rider_actions = [
            action_signature(&game, layout, &Action::Event(0)),
            action_signature(&game, layout, &Action::Event(1)),
        ];
        assert_ne!(riders, types);
        assert_ne!(rider_actions, type_actions);
        game.event_data[2] = 9;
        game.event_rng = Some(Rng::from_seed(99));
        assert_eq!(state_signature(&game, layout), riders);
        assert_eq!(
            action_signature(&game, layout, &Action::Event(0)),
            rider_actions[0]
        );
        game.event_data.swap(0, 1);
        assert_ne!(state_signature(&game, layout), riders);
        assert_eq!(
            action_signature(&game, layout, &Action::Event(0))[ACTION_KINDS + 1],
            rider_actions[1][ACTION_KINDS + 1]
        );
        assert_eq!(
            action_signature(&game, layout, &Action::Event(1))[ACTION_KINDS + 1],
            rider_actions[0][ACTION_KINDS + 1]
        );
    }

    #[test]
    fn token_actions_keep_event_identity_and_tinker_offers() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 76, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();

        let dummy = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.BATTLEWORN_DUMMY")
            .unwrap() as Id;
        game.phase = Phase::Event(dummy, content.events[dummy as usize].options.to_vec());
        assert_ne!(
            tokenized_action(&game, &content, layout, &Action::Event(1)),
            tokenized_action(&game, &content, layout, &Action::Event(2))
        );
        let mut first = game.clone();
        let mut second = game;
        first.step(&content, Action::Event(1)).unwrap();
        second.step(&content, Action::Event(2)).unwrap();
        assert_ne!(
            state_signature(&first, layout),
            state_signature(&second, layout)
        );

        let tinker = layout.tinker_time.unwrap();
        first = Game::new_character_ascension(&content, 77, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first.phase = crate::game::event_page(tinker, &[0, 1]);
        first.event_data = [5, 4, 0, 2];
        second = first.clone();
        second.event_data.swap(0, 1);
        assert_ne!(
            state_signature(&first, layout),
            state_signature(&second, layout)
        );
        assert_ne!(
            tokenized_action(&first, &content, layout, &Action::Event(0)),
            tokenized_action(&second, &content, layout, &Action::Event(0))
        );
        first.step(&content, Action::Event(0)).unwrap();
        second.step(&content, Action::Event(0)).unwrap();
        assert_ne!(
            state_tokens(&first, &content, layout),
            state_tokens(&second, &content, layout)
        );
    }

    #[test]
    fn queued_autoplay_cards_match_next_effect_positions() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 78, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let first = Card {
            id: content.card_id("CARD.ZAP").unwrap(),
            ..Card::default()
        };
        let second = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.queue = [first, second]
            .map(|card| Pending {
                effect: Effect::AutoPlay(card),
                context: Context::card(card),
            })
            .to_vec();
        let tokens = state_tokens(&game, &content, layout);
        let queued = |position| {
            tokens
                .iter()
                .find(|row| {
                    row[..2] == [CONTINUATION_COLLECTION as f32, QUEUED_CARD_KIND as f32]
                        && row[4] == position as f32
                })
                .unwrap()
        };
        assert_eq!(queued(1)[2], second.id as f32 + 1.0);
        assert_eq!(queued(2)[2], first.id as f32 + 1.0);
        for position in 1..=2 {
            assert!(tokens.iter().any(|row| {
                row[..2] == [CONTINUATION_COLLECTION as f32, 8.0] && row[4] == position as f32
            }));
        }
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

        let hidden = action_signature(&game, layout, &Action::Choose(16));
        assert_eq!(hidden, action_signature(&game, layout, &Action::Choose(17)));
        let mut first = game.clone();
        let mut second = game.clone();
        first.step(&content, Action::Choose(16)).unwrap();
        second.step(&content, Action::Choose(17)).unwrap();
        assert_eq!(
            state_signature(&first, layout),
            state_signature(&second, layout)
        );

        let known = action_signature(&game, layout, &Action::Choose(28));
        assert_ne!(hidden, known);
        second = game;
        second.step(&content, Action::Choose(28)).unwrap();
        assert_ne!(
            state_signature(&first, layout),
            state_signature(&second, layout)
        );
    }

    #[test]
    fn mixed_active_metadata_has_fixed_width() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let game = Game::new_character_ascension(&content, 40, 0, 10).unwrap();
        let rows = [true, false].map(|active| observation_globals(&game, &content, layout, active));
        assert_eq!(PUBLIC_GLOBALS, 0);
        assert_eq!(globals_len(layout), PUBLIC_GLOBALS);
        assert!(rows.into_iter().all(|row| row.is_empty()));
    }

    #[test]
    fn v62_packed_observation_has_no_direct_globals_and_is_digest_protected() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 600, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let observation = observation_v56(&game, &content, layout, (3, 4));
        let (globals, counts, mut exact, actions, digest) = packed_observation(&observation);
        assert_eq!(observation_digest(&observation), digest);
        assert_eq!(globals, observation.globals);
        assert_eq!(globals.len(), PUBLIC_GLOBALS);
        assert!(globals.iter().all(|value| value.is_finite()));
        assert_eq!(
            packed_observation_digest(observation.character, &globals, &counts, &exact, &actions),
            digest
        );
        exact[0] ^= 1;
        assert_ne!(
            packed_observation_digest(observation.character, &globals, &counts, &exact, &actions),
            digest
        );
    }

    #[test]
    fn v62_map_uses_the_contextual_current_node_without_direct_globals() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 82, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.map = Map {
            nodes: vec![
                MapNode {
                    floor: 1,
                    lane: 2,
                    room: Room::Shop,
                    next: vec![1],
                },
                MapNode {
                    floor: 18,
                    lane: 4,
                    room: Room::Elite,
                    next: vec![],
                },
            ]
            .into(),
            current: Some(0),
        };
        game.fur_coat_act = Some(game.run.act);
        game.fur_coat = vec![(2, 1)];
        game.spoils = Some((2, 1));
        let globals = observation_globals(&game, &content, layout, true);
        assert!(globals.is_empty());
        let observation = observation_v56(&game, &content, layout, (0, 0));
        let run = &observation.domains[RUN_DOMAIN][0];
        let current = observation.domains[MAP_NODE_DOMAIN]
            .iter()
            .find(|row| row.u[0] == run.u[23])
            .unwrap();
        assert_eq!(
            &current.u[1..8],
            &[0, 1, 2, room_index(Room::Shop) as u32, 0, 1, 1]
        );
        assert!(
            observation.domains[MAP_EDGE_DOMAIN]
                .iter()
                .any(|row| row.u[0] == current.u[0])
        );
        let tokens = state_tokens(&game, &content, layout);
        let node = tokens
            .iter()
            .find(|row| {
                row[..5]
                    == [
                        MAP_COLLECTION as f32,
                        0.0,
                        room_index(Room::Shop) as f32 + 1.0,
                        0.0,
                        3.0,
                    ]
            })
            .unwrap();
        assert_eq!(&node[10..13], &[1.0, 1.0, 1.0]);
        assert!(
            tokens
                .iter()
                .any(|row| { row[..5] == [MAP_COLLECTION as f32, 0.0, 2.0, 17.0, 5.0] })
        );
        assert!(
            tokens
                .iter()
                .any(|row| { row[..5] == [MAP_COLLECTION as f32, 1.0, 5.0, 0.0, 3.0] })
        );
        assert!(
            action_entity_tokens(&game, &content, layout, &Action::Path(1))
                .iter()
                .any(|row| row[..5] == [MAP_COLLECTION as f32, 2.0, 2.0, 17.0, 5.0])
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
        for (kind, id) in [(1, 10), (4, 21), (5, 8)] {
            assert!(
                tokens.iter().any(|row| {
                    row[..3] == [ENCOUNTER_COLLECTION as f32, kind as f32, id as f32]
                })
            );
        }
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
        std::sync::Arc::make_mut(&mut second.map.nodes)
            .iter_mut()
            .for_each(|node| node.next.reverse());
        let nodes = second.map.nodes.len();
        std::sync::Arc::make_mut(&mut second.map.nodes).reverse();
        std::sync::Arc::make_mut(&mut second.map.nodes)
            .iter_mut()
            .for_each(|node| {
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
            summary_signature(&first, layout),
            summary_signature(&second, layout)
        );
        assert_eq!(
            observation_globals(&first, &content, layout, true),
            observation_globals(&second, &content, layout, true)
        );
        second.last_encounter = first.encounters.first().copied();
        assert_ne!(
            state_tokens(&first, &content, layout),
            state_tokens(&second, &content, layout)
        );
        assert_eq!(
            observation_globals(&first, &content, layout, true),
            observation_globals(&second, &content, layout, true)
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
        let tokens = state_tokens(&game, &content, layout);
        assert_eq!(
            tokens
                .iter()
                .filter(|row| row[0] == CONTINUATION_COLLECTION as f32 && row[1] == 7.0)
                .count(),
            3000
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|row| {
                    row[0] == CONTINUATION_COLLECTION as f32 && row[1] == EFFECT_DETAIL_KIND as f32
                })
                .count(),
            3000
        );
        assert!(tokens.len() < CONTINUATION_BYTES);
        assert_eq!(
            observation_globals(&game, &content, layout, true).len(),
            globals_len(layout)
        );
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
        std::sync::Arc::make_mut(&mut game.map.nodes)[0].room = Room::Elite;
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
    fn wriggler_behavior_uses_public_slots_not_private_instances() {
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
            summary_signature(&first, layout),
            summary_signature(&second, layout)
        );
        assert_eq!(
            state_tokens(&first, &content, layout),
            state_tokens(&second, &content, layout)
        );

        first.spawn_wrigglers(&content, 1);
        second.spawn_wrigglers(&content, 1);
        assert_eq!(
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
    fn v46_tokens_use_exact_categories_and_public_actor_slots() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 46, 4, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let tokens = state_tokens(&game, &content, layout);
        for token in &tokens {
            assert!(
                token[..TOKEN_CATEGORICAL]
                    .iter()
                    .all(|value| *value >= 0.0 && value.fract() == 0.0)
            );
        }
        let combat = game.combat().unwrap();
        assert!(
            tokens
                .iter()
                .any(|row| { row[..4] == [STATE_COLLECTION as f32, 18.0, 0.0, 1.0] })
        );
        assert!(
            combat.osty.max_hp == 0
                || tokens
                    .iter()
                    .any(|row| { row[..4] == [STATE_COLLECTION as f32, 19.0, 0.0, 2.0] })
        );
        for (slot, enemy) in combat.enemies.iter().enumerate() {
            assert!(tokens.iter().any(|row| {
                row[..4]
                    == [
                        ENEMY_COLLECTION as f32,
                        0.0,
                        enemy.creature.id as f32 + 1.0,
                        slot as f32 + 3.0,
                    ]
            }));
        }
    }

    #[test]
    fn v46_legality_is_separate_from_candidate_content() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 47, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.phase = Phase::Event(
            0,
            vec![EventOption {
                requirement: Requirement::Always,
                effects: &[],
            }],
        );
        assert_eq!(
            tokenized_candidate(&game, &content, layout, &Action::Event(0), true),
            tokenized_candidate(&game, &content, layout, &Action::Event(0), false)
        );
    }

    #[test]
    fn v47_non_actor_state_payloads_keep_owner_zero() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 48, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.room = Room::Shop;
        game.phase = Phase::EnchantCards(Enchantment::Adroit, 2, 1, Some(CardType::Attack), false);
        let mut rows = Vec::new();
        state_summary_tokens(&game, &content, (0, 0), &mut rows);
        let phase = rows.iter().find(|row| row[1] == 3.0).unwrap();
        assert_eq!(phase[3], 0.0);
        assert_eq!(phase[4], room_index(Room::Shop) as f32 + 1.0);
        assert_eq!(phase[5], CardType::Attack as usize as f32 + 1.0);

        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let choice = Choice {
            pile: Pile::Hand,
            filter: CardFilter::Cost(1),
            op: CardOp::TransformRandom,
            remaining: 1,
            optional: false,
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.choice = Some(choice);
        rows.clear();
        state_summary_tokens(&game, &content, (0, 0), &mut rows);
        let row = rows.iter().find(|row| row[1] == 24.0).unwrap();
        assert_eq!(row[3], 0.0);
        assert_eq!(row[4], filter_features(choice.filter).0 as f32 + 1.0);
        assert_eq!(row[6], op_features(choice.op).0 as f32 + 1.0);

        game.phase = Phase::Rewards(Rewards {
            gold: 0,
            cards: vec![],
            card_rewards: vec![CardReward::Kaleidoscope],
            relics: vec![],
            potions: vec![],
            removals: 0,
        });
        rows.clear();
        state_summary_tokens(&game, &content, (0, 0), &mut rows);
        let row = rows.iter().find(|row| row[1] == 26.0).unwrap();
        assert_eq!(&row[3..6], &[0.0, 1.0, 3.0]);
        assert!(
            rows.iter()
                .all(|row| { row[3] == 0.0 || matches!(row[1] as usize, 18..=22 | 25) })
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
            skip_next_decay: false,
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
            skip_next_decay: false,
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
            skip_next_decay: false,
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
                skip_next_decay: false,
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

    #[test]
    fn v53_deterministic_action_outcomes_match_steps_for_every_character() {
        let content = foundation_content();
        let cards = [
            ("CARD.STRIKE_IRONCLAD", "CARD.DEFEND_IRONCLAD"),
            ("CARD.STRIKE_DEFECT", "CARD.DEFEND_DEFECT"),
            ("CARD.STRIKE_SILENT", "CARD.DEFEND_SILENT"),
            ("CARD.STRIKE_REGENT", "CARD.DEFEND_REGENT"),
            ("CARD.STRIKE_NECROBINDER", "CARD.DEFEND_NECROBINDER"),
        ];
        for (character, (strike, defend)) in cards.into_iter().enumerate() {
            let mut game = Game::new_character_ascension(
                &content,
                100 + character as u64,
                character as Id,
                10,
            )
            .unwrap();
            game.begin_act(&content, 0).unwrap();
            game.start_combat(&content, content.acts[0].encounters[0])
                .unwrap();
            game.run.relics.clear();
            let Phase::Combat(combat) = &mut game.phase else {
                unreachable!()
            };
            combat.enemies.truncate(1);
            combat.hits.truncate(1);
            combat.enemies[0].creature.hp = 100;
            combat.enemies[0].creature.max_hp = 100;
            combat.enemies[0].creature.block = 2;
            combat.enemies[0].creature.powers.clear();
            combat.player.block = 3;
            combat.player.powers.clear();
            combat.energy = 10;
            combat.hand = [strike, defend]
                .map(|id| Card {
                    id: content.card_id(id).unwrap(),
                    ..Card::default()
                })
                .to_vec();
            game.run.potions[0] = Some(content.potion_id("POTION.FIRE_POTION").unwrap());
            for action in game
                .actions(&content)
                .into_iter()
                .filter(|action| matches!(action, Action::Play { .. } | Action::Potion { .. }))
            {
                let (values, tokens) =
                    tokenized_action(&game, &content, Layout::new(&content), &action);
                let outcome = game.expected_action_outcome(&content, &action);
                let mut next = game.clone();
                let before = next.combat().unwrap();
                let (turn, block, drawn, exhausted, enemy_hp) = (
                    before.turn,
                    before.player.block,
                    before.drawn,
                    before.history.exhausted,
                    before.enemies[0].creature.hp,
                );
                next.step(&content, action.clone()).unwrap();
                let after = next.combat().unwrap();
                assert_eq!(after.turn, turn);
                assert_eq!(
                    outcome.block,
                    Some(after.player.block.saturating_sub(block).max(0) as f32)
                );
                assert_eq!(
                    outcome.draw,
                    Some(after.drawn.saturating_sub(drawn).max(0) as f32)
                );
                assert_eq!(
                    outcome.exhaust,
                    Some(after.history.exhausted.saturating_sub(exhausted).max(0) as f32)
                );
                assert_eq!(
                    outcome.hp_loss,
                    Some(vec![(enemy_hp - after.enemies[0].creature.hp) as f32])
                );
                if matches!(action, Action::Play { .. }) {
                    let card = tokens
                        .iter()
                        .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
                        .unwrap();
                    assert_eq!(
                        card[28],
                        after.player.block.saturating_sub(block).max(0) as f32
                    );
                    assert_eq!(card[31], after.drawn.saturating_sub(drawn).max(0) as f32);
                    assert_eq!(
                        card[33],
                        after.history.exhausted.saturating_sub(exhausted).max(0) as f32
                    );
                }
                if matches!(
                    action,
                    Action::Play {
                        target: Some(0),
                        ..
                    } | Action::Potion {
                        target: Some(0),
                        ..
                    }
                ) {
                    assert_eq!(values[52], (enemy_hp - after.enemies[0].creature.hp) as f32);
                }
            }
        }
    }

    #[test]
    fn v53_escape_plan_uses_public_draw_expectation() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 101, 2, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics.clear();
        let escape = Card {
            id: content.card_id("CARD.ESCAPE_PLAN").unwrap(),
            ..Card::default()
        };
        let strike = Card {
            id: content.card_id("CARD.STRIKE_SILENT").unwrap(),
            ..Card::default()
        };
        let defend = Card {
            id: content.card_id("CARD.DEFEND_SILENT").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![escape];
        combat.draw = vec![strike, defend];
        combat.discard.clear();
        combat.known_draw_bottom = 0;
        combat.known_draw_top = 0;
        combat.energy = 10;
        combat.player.block = 0;
        combat.player.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let outcome = game.expected_action_outcome(&content, &action);
        assert_eq!((outcome.block, outcome.draw), (Some(1.5), Some(1.0)));
        let (_, tokens) = tokenized_action(&game, &content, Layout::new(&content), &action);
        let card = tokens
            .iter()
            .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
            .unwrap();
        assert_eq!((card[28], card[31]), (1.5, 1.0));

        let mut hidden = game.clone();
        hidden.rngs = Rngs::from_seed(999);
        let Phase::Combat(combat) = &mut hidden.phase else {
            unreachable!()
        };
        combat.draw.reverse();
        assert_eq!(outcome, hidden.expected_action_outcome(&content, &action));
        assert_eq!(
            tokenized_action(&game, &content, Layout::new(&content), &action),
            tokenized_action(&hidden, &content, Layout::new(&content), &action)
        );
    }

    #[test]
    fn v53_random_target_outcome_is_probability_weighted() {
        let mut content = foundation_content();
        let id = content.card_id("CARD.ESCAPE_PLAN").unwrap();
        content.cards[id as usize].effects = Box::leak(Box::new([Effect::Damage(
            Target::RandomEnemy,
            Amount::fixed(6, 6),
        )]));
        let mut game = Game::new_character_ascension(&content, 102, 2, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics.clear();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        let enemy = combat.enemies[0].clone();
        combat.enemies = vec![enemy.clone(), enemy];
        combat.hits = vec![0; 2];
        combat.enemy_power_snapshot = vec![vec![]; 2];
        for (index, enemy) in combat.enemies.iter_mut().enumerate() {
            enemy.instance = index as u32 + 1;
            enemy.creature.hp = 100;
            enemy.creature.max_hp = 100;
            enemy.creature.block = 0;
            enemy.creature.powers.clear();
        }
        combat.hand = vec![Card {
            id,
            ..Card::default()
        }];
        combat.energy = 10;
        combat.player.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        assert_eq!(
            game.expected_action_outcome(&content, &action).hp_loss,
            Some(vec![3.0, 3.0])
        );
        let (_, tokens) = tokenized_action(&game, &content, Layout::new(&content), &action);
        let probabilities: Vec<_> = tokens
            .iter()
            .filter(|row| row[0] == ENEMY_COLLECTION as f32 && row[1] == 3.0)
            .map(|row| row[18])
            .collect();
        assert_eq!(probabilities, vec![0.5, 0.5]);
    }

    #[test]
    fn v53_random_autoplay_is_exact_until_the_branch_limit() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 103, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics.clear();
        let havoc = Card {
            id: content.card_id("CARD.HAVOC").unwrap(),
            ..Card::default()
        };
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
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.enemies[0].creature.max_hp = 100;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        combat.hand = vec![havoc];
        combat.draw = vec![strike, defend];
        combat.discard.clear();
        combat.known_draw_bottom = 0;
        combat.known_draw_top = 0;
        combat.energy = 10;
        combat.player.block = 0;
        combat.player.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let outcome = game.expected_action_outcome(&content, &action);
        assert_eq!(outcome.block, Some(2.5));
        assert_eq!(outcome.draw, Some(1.0));
        assert_eq!(outcome.exhaust, Some(1.0));
        assert_eq!(outcome.hp_loss, Some(vec![3.0]));

        let beat_down = content.card_id("CARD.BEAT_DOWN").unwrap();
        let silent_strike = Card {
            id: content.card_id("CARD.STRIKE_SILENT").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![Card {
            id: beat_down,
            ..Card::default()
        }];
        combat.draw.clear();
        combat.discard = vec![silent_strike; 2];
        combat.energy = 10;
        assert_eq!(
            game.expected_action_outcome(&content, &action).hp_loss,
            Some(vec![12.0])
        );

        let catastrophe = content.card_id("CARD.CATASTROPHE").unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![Card {
            id: catastrophe,
            ..Card::default()
        }];
        combat.draw = vec![silent_strike; 9];
        combat.discard.clear();
        combat.known_draw_bottom = 0;
        combat.known_draw_top = 0;
        assert_eq!(
            game.expected_action_outcome(&content, &action),
            crate::game::ActionOutcome::default()
        );
        assert!(
            action_preview(&game, &content, &action)
                .unwrap()
                .outcome
                .hp_loss
                .is_none()
        );
    }

    #[test]
    fn v53_unknown_random_generation_keeps_symbolic_fields() {
        let mut content = foundation_content();
        let id = content.card_id("CARD.ALCHEMIZE").unwrap();
        content.cards[id as usize].effects = Box::leak(Box::new([
            Effect::Block(Target::Player, Amount::fixed(7, 7)),
            Effect::RandomPotion,
        ]));
        let mut game = Game::new_character_ascension(&content, 104, 2, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics.clear();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![Card {
            id,
            ..Card::default()
        }];
        combat.energy = 10;
        combat.player.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        assert_eq!(
            game.expected_action_outcome(&content, &action),
            crate::game::ActionOutcome::default()
        );
        let preview = action_preview(&game, &content, &action).unwrap();
        assert_eq!(preview.block, 7);
        assert_eq!(preview.outcome, crate::game::ActionOutcome::default());
        let (_, tokens) = tokenized_action(&game, &content, Layout::new(&content), &action);
        assert_eq!(
            tokens
                .iter()
                .find(|row| row[0] == ACTION_CARD_COLLECTION as f32)
                .unwrap()[28],
            7.0
        );
    }

    fn model_row_bytes(domain: usize, row: &DomainRow) -> Vec<u8> {
        let structural: &[usize] = match domain {
            RUN_DOMAIN => &[23],
            CARD_DOMAIN => &[0],
            ACTOR_DOMAIN | POWER_DOMAIN | HISTORY_DOMAIN => &[0],
            STATUS_DOMAIN => &[0, 1],
            RELIC_DOMAIN | EVENT_DOMAIN | ENCOUNTER_DOMAIN => &[0],
            POTION_DOMAIN => &[1],
            ORB_DOMAIN => &[0],
            CRYSTAL_DOMAIN => &[7],
            CONTINUATION_DOMAIN => &[2, 3, 5],
            MAP_NODE_DOMAIN => &[0, 8, 9],
            MAP_EDGE_DOMAIN => &[0, 1],
            _ => &[],
        };
        let mut out = Vec::new();
        for &index in structural {
            out.extend(row.u[index].to_le_bytes());
        }
        row.c
            .iter()
            .for_each(|value| out.extend(value.to_le_bytes()));
        row.f
            .iter()
            .for_each(|value| out.extend(value.to_bits().to_le_bytes()));
        out
    }

    fn v56_candidate_bytes(observation: &ObservationV56, index: usize) -> Vec<u8> {
        let row = &observation.candidates[index];
        let mut out = Vec::new();
        out.extend(row.u[1].to_le_bytes());
        out.extend(row.u[4].to_le_bytes());
        row.c
            .iter()
            .for_each(|value| out.extend(value.to_le_bytes()));
        row.f
            .iter()
            .for_each(|value| out.extend(value.to_bits().to_le_bytes()));
        out.push(row.legal as u8);
        for (domain, rows) in observation.domains.iter().enumerate() {
            for row in rows.iter().filter(|row| row.scope == index as i32) {
                out.push(domain as u8);
                out.extend(model_row_bytes(domain, row));
            }
        }
        out
    }

    fn v56_bytes(observation: &ObservationV56) -> Vec<u8> {
        let mut out = vec![observation.character];
        observation
            .globals
            .iter()
            .for_each(|value| out.extend(value.to_bits().to_le_bytes()));
        for (domain, rows) in observation.domains.iter().enumerate() {
            let rows = rows
                .iter()
                .filter(|row| row.scope == STATE_SCOPE)
                .collect::<Vec<_>>();
            out.extend((rows.len() as u32).to_le_bytes());
            for row in rows {
                out.extend(model_row_bytes(domain, row));
            }
        }
        let mut candidates = (0..observation.candidates.len())
            .map(|index| v56_candidate_bytes(observation, index))
            .collect::<Vec<_>>();
        candidates.sort();
        out.extend((candidates.len() as u32).to_le_bytes());
        for candidate in candidates {
            out.extend((candidate.len() as u32).to_le_bytes());
            out.extend(candidate);
        }
        out
    }

    #[test]
    fn v53_exact_fields_keep_integer_extremes() {
        let mut row = DomainRow::new(CONTINUATION_DOMAIN, STATE_SCOPE);
        row.u[..6].copy_from_slice(&[0, 255, 256, 65_535, 65_536, u32::MAX]);
        row.s[..4].copy_from_slice(&[0, -1, i32::MIN, i32::MAX]);
        let mut changed = row.clone();
        changed.u[2] = 255;
        assert_ne!(row, changed);
        assert_eq!(row.u[5], u32::MAX);
        assert_eq!(row.s[2], i32::MIN);
    }

    #[test]
    fn v53_observation_is_canonical_under_hidden_resampling() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 401, 2, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        let mut second = first.clone();
        resample_hidden(&mut first, &content, 17);
        resample_hidden(&mut second, &content, 99);
        assert_eq!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );
    }

    #[test]
    fn v53_combat_choice_keeps_typed_filter_and_operation_fields() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 407, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let target = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.choice = Some(Choice {
            pile: Pile::Draw,
            filter: CardFilter::TypeWithoutTurnFlag(CardType::Attack, 0x8123),
            op: CardOp::Transform(target, 7),
            remaining: 2,
            optional: true,
        });
        combat.enemy_turn = true;
        combat.ending = true;
        combat.force_end = true;
        let row = phase_domain_row(&game, &content);
        assert_eq!(
            &row.u[5..16],
            &[
                Pile::Draw as u32,
                3,
                CardType::Attack as u32,
                7,
                target as u32,
                7,
                2,
                1,
                1,
                1,
                1,
            ]
        );
        assert_eq!(&row.s[..2], &[0x8123, 0]);
    }

    #[test]
    fn v53_map_is_relabel_invariant_and_edge_sensitive() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 404, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        let mut second = first.clone();
        let len = second.map.nodes.len();
        let old = second.map.nodes.clone();
        let permutation = (0..len).rev().collect::<Vec<_>>();
        let mut inverse = vec![0; len];
        for (new, &old_index) in permutation.iter().enumerate() {
            inverse[old_index] = new;
        }
        second.map.nodes = permutation
            .iter()
            .map(|&old_index| {
                let mut node = old[old_index].clone();
                node.next = node.next.iter().map(|&next| inverse[next]).collect();
                node
            })
            .collect::<Vec<_>>()
            .into();
        second.map.current = first.map.current.map(|current| inverse[current]);
        assert_eq!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );
        let mut changed = first.clone();
        let node = std::sync::Arc::make_mut(&mut changed.map.nodes)
            .iter_mut()
            .find(|node| node.next.len() > 1)
            .unwrap();
        node.next.pop();
        assert_ne!(
            observation_v56(&first, &content, layout, (0, 0)).domains[MAP_EDGE_DOMAIN],
            observation_v56(&changed, &content, layout, (0, 0)).domains[MAP_EDGE_DOMAIN]
        );
    }

    #[test]
    fn v53_sparse_map_matches_an_independent_scalar_dag() {
        fn node_value(row: &DomainRow) -> i64 {
            row.u[1..8]
                .iter()
                .enumerate()
                .map(|(index, &value)| (index as i64 + 2) * value as i64)
                .sum()
        }
        fn edge_value(row: &DomainRow) -> i64 {
            row.u[2] as i64 * 37 + row.u[3..].iter().map(|&value| value as i64).sum::<i64>()
        }
        fn recursive(
            id: u32,
            nodes: &std::collections::BTreeMap<u32, &DomainRow>,
            edges: &[&DomainRow],
            memo: &mut std::collections::BTreeMap<u32, i64>,
        ) -> i64 {
            if let Some(&value) = memo.get(&id) {
                return value;
            }
            let value = node_value(nodes[&id])
                + edges
                    .iter()
                    .filter(|edge| edge.u[0] == id)
                    .map(|edge| edge_value(edge) + recursive(edge.u[1], nodes, edges, memo))
                    .sum::<i64>();
            memo.insert(id, value);
            value
        }

        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 408, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let observation = observation_v56(&game, &content, layout, (0, 0));
        let nodes = observation.domains[MAP_NODE_DOMAIN]
            .iter()
            .map(|row| (row.u[0], row))
            .collect::<std::collections::BTreeMap<_, _>>();
        let edges = observation.domains[MAP_EDGE_DOMAIN]
            .iter()
            .collect::<Vec<_>>();
        let mut batched = nodes
            .iter()
            .map(|(&id, row)| (id, node_value(row)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut levels = nodes.values().map(|row| row.u[8]).collect::<Vec<_>>();
        levels.sort_unstable();
        levels.dedup();
        for level in levels.into_iter().rev() {
            for row in nodes.values().filter(|row| row.u[8] == level) {
                batched.insert(
                    row.u[0],
                    node_value(row)
                        + edges
                            .iter()
                            .filter(|edge| edge.u[0] == row.u[0])
                            .map(|edge| edge_value(edge) + batched[&edge.u[1]])
                            .sum::<i64>(),
                );
            }
        }
        let mut reference = std::collections::BTreeMap::new();
        for &id in nodes.keys() {
            recursive(id, &nodes, &edges, &mut reference);
        }
        assert_eq!(batched, reference);
        for candidate in observation
            .candidates
            .iter()
            .filter(|row| matches!(row.action, Action::Path(_)))
        {
            assert_eq!(batched[&candidate.u[4]], reference[&candidate.u[4]]);
        }
        let mut changed = observation.domains[MAP_EDGE_DOMAIN].to_vec();
        changed[0].u[2] += 1;
        let changed = changed.iter().collect::<Vec<_>>();
        assert_ne!(
            recursive(0, &nodes, &edges, &mut std::collections::BTreeMap::new()),
            recursive(0, &nodes, &changed, &mut std::collections::BTreeMap::new())
        );
    }

    #[test]
    fn v53_path_candidates_reuse_canonical_destination_ids() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 405, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let observation = observation_v56(&game, &content, layout, (0, 0));
        assert_eq!(observation.domains[RUN_DOMAIN][0].u[23], 0);
        let nodes = observation.domains[MAP_NODE_DOMAIN]
            .iter()
            .map(|row| row.u[0])
            .collect::<std::collections::BTreeSet<_>>();
        for candidate in observation
            .candidates
            .iter()
            .filter(|row| matches!(row.action, Action::Path(_)))
        {
            assert!(nodes.contains(&candidate.u[4]));
        }
        assert!(
            observation
                .candidates
                .iter()
                .filter(|row| !matches!(row.action, Action::Path(_)))
                .all(|row| row.u[4] == NO_NODE)
        );
        let path = observation
            .candidates
            .iter()
            .find_map(|row| match row.action {
                Action::Path(index) => Some((index, row.u[4])),
                _ => None,
            })
            .unwrap();
        game.map.current = Some(path.0);
        let entered = observation_v56(&game, &content, layout, (0, 0));
        assert_eq!(entered.domains[RUN_DOMAIN][0].u[23], path.1);
        assert_ne!(path.1, 0);

        let current = game
            .map
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| {
                !node.next.is_empty()
                    && game.map.nodes.iter().enumerate().any(|(candidate, next)| {
                        next.floor == node.floor + 1 && !node.next.contains(&candidate)
                    })
            })
            .map(|(index, _)| index)
            .unwrap();
        game.map.current = Some(current);
        game.run
            .relics
            .push(content.relic_id("RELIC.WINGED_BOOTS").unwrap());
        game.winged_boots = 0;
        let observation = observation_v56(&game, &content, layout, (0, 0));
        let paths = observation
            .candidates
            .iter()
            .filter(|row| matches!(row.action, Action::Path(_)))
            .collect::<Vec<_>>();
        assert!(paths.iter().any(|row| row.u[5] == 1));
        assert!(paths.iter().any(|row| row.u[5] == 2));
        assert!(paths.iter().all(|row| row.u[14] == row.u[4]));
        assert_eq!(
            paths
                .iter()
                .map(|row| row.u[14])
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            paths.len()
        );
    }

    #[test]
    fn v54_continuation_parent_and_path_associations_do_not_alias() {
        fn tree(swapped: bool) -> Vec<DomainRow> {
            let mut tree = ContinuationBuilder::new(STATE_SCOPE);
            tree.row(9, 0, NO_NODE, 0, 0, 0, 0, 2);
            tree.row(1, 10, 0, 1, 1, 0, 0, 1);
            tree.row(1, 11, 0, 2, 1, 0, 1, 1);
            tree.row(1, 20, if swapped { 2 } else { 1 }, 1, 2, 0, 0, 0);
            tree.row(1, 21, if swapped { 1 } else { 2 }, 2, 2, 0, 0, 0);
            tree.rows
        }
        fn without_structure(mut rows: Vec<DomainRow>) -> Vec<DomainRow> {
            for row in &mut rows {
                for field in [2, 3, 5] {
                    row.u[field] = 0;
                }
            }
            rows
        }

        let first = tree(false);
        let second = tree(true);
        assert_eq!(
            without_structure(first.clone()),
            without_structure(second.clone())
        );
        assert_ne!(first, second);
        let mut different_paths = first.clone();
        different_paths[3].u[5] = 3;
        different_paths[4].u[5] = 1;
        assert_eq!(
            without_structure(first.clone()),
            without_structure(different_paths.clone())
        );
        assert_ne!(first, different_paths);
        assert!(
            first
                .iter()
                .all(|row| row.f[16..22].iter().all(|value| value.is_finite()))
        );
    }

    #[test]
    fn v56_categorical_embeddings_sum_without_field_weights() {
        let encoder = TokenEncoderWeights {
            numeric: LinearWeights {
                w: vec![],
                b: vec![0.0; 2],
            },
            norm_w: vec![1.0; 2],
            norm_b: vec![0.0; 2],
        };
        let table = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        assert_eq!(
            encoder.encode(&[1, 2], &[], &table, 2),
            encoder.encode(&[2, 1], &[], &table, 2)
        );
    }

    #[test]
    fn v55_production_populates_flag_heavy_cards_once() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 549, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        let card = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            flags: u16::MAX,
            turn_flags: u16::MAX,
            ..Card::default()
        };
        game.run.deck.push(card);
        let observation = observation_v56(&game, &content, layout, (0, 0));
        let actual = observation.domains[CARD_DOMAIN]
            .iter()
            .find(|row| row.u[4] == card.id as u32 && row.u[6] == u16::MAX as u32)
            .unwrap();
        let mut encoding = CardEncoding::new(&game, &content);
        let mut expected = card_domain_row(
            &game,
            &content,
            &mut encoding,
            STATE_SCOPE,
            0,
            0,
            0,
            0,
            card,
            0,
        );
        populate_domain_features(&game, &content, layout, CARD_DOMAIN, &mut expected);
        assert_eq!(
            (actual.c.clone(), actual.f.clone()),
            (expected.c, expected.f)
        );
    }

    #[test]
    fn v56_continuation_booleans_and_discard_counts_do_not_alias() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let game = Game::new_character_ascension(&content, 565, 0, 10).unwrap();
        let effect = |effect| {
            let mut tree = ContinuationBuilder::for_game(&game, STATE_SCOPE);
            tree.effect(effect, NO_NODE, 0, 0, 0, 0, None);
            let mut row = tree.rows.remove(0);
            populate_domain_features(&game, &content, layout, CONTINUATION_DOMAIN, &mut row);
            (row.c, row.f)
        };
        for (left, right) in [
            (
                Effect::RandomCard(Pile::Hand, CardType::Attack, 1, false),
                Effect::RandomCard(Pile::Hand, CardType::Attack, 1, true),
            ),
            (
                Effect::OfferColorless(3, false, true),
                Effect::OfferColorless(3, true, true),
            ),
            (
                Effect::OfferCharacter(3, false),
                Effect::OfferCharacter(3, true),
            ),
            (
                Effect::DistinctCharacter(Pile::Hand, 2, false),
                Effect::DistinctCharacter(Pile::Hand, 2, true),
            ),
            (
                Effect::AutoPlayDraw(Amount::fixed(2, 2), false),
                Effect::AutoPlayDraw(Amount::fixed(2, 2), true),
            ),
            (Effect::Discard(1, false), Effect::Discard(1, true)),
            (Effect::Exhaust(1, false), Effect::Exhaust(1, true)),
            (
                Effect::Upgrade(Pile::Hand, 1, false),
                Effect::Upgrade(Pile::Hand, 1, true),
            ),
            (Effect::Evoke(false), Effect::Evoke(true)),
        ] {
            assert_ne!(effect(left), effect(right));
        }
        assert_ne!(
            effect(Effect::Discard(1, false)),
            effect(Effect::Discard(2, false))
        );
        let select = |first, second| {
            effect(Effect::Select(
                Pile::Hand,
                CardFilter::Any,
                [2, 3],
                first,
                second,
                CardOp::Move(Pile::Exhaust),
            ))
        };
        assert_ne!(select(false, false), select(true, false));
        assert_ne!(select(false, false), select(false, true));

        let run_effect = |effect| {
            let mut tree = ContinuationBuilder::for_game(&game, STATE_SCOPE);
            tree.run_effect(effect, &content, NO_NODE, 0, 0, 0, 0);
            let mut row = tree.rows.remove(0);
            populate_domain_features(&game, &content, layout, CONTINUATION_DOMAIN, &mut row);
            row.c
        };
        assert_ne!(
            run_effect(RunEffect::RandomPotionReward(false)),
            run_effect(RunEffect::RandomPotionReward(true))
        );
    }

    #[test]
    fn v55_potion_replacement_menu_is_unique_and_exactly_legal() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 554, 0, 24).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.phase = Phase::Map;
        game.run.potions[0] = Some(content.potion_id("POTION.BLOOD_POTION").unwrap());
        game.run.potions[1] = Some(content.potion_id("POTION.FRUIT_JUICE").unwrap());
        game.pending_potion = Some(content.potion_id("POTION.ENTROPIC_BREW").unwrap());
        game.replacing_potion = true;
        let legal = game.actions(&content);
        let (represented, mask) = candidate_actions(&game, &content);
        assert_eq!(represented, legal);
        assert!(mask.into_iter().all(|legal| legal));
        let observation = observation_v56(&game, &content, layout, (0, 0));
        assert_eq!(
            observation
                .candidates
                .iter()
                .map(|candidate| candidate.action.clone())
                .collect::<Vec<_>>(),
            legal
        );
    }

    #[test]
    fn v58_unequal_target_previews_remain_distinct_candidates() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let encounter = content
            .encounters
            .iter()
            .position(|encounter| encounter.enemies.len() >= 2)
            .unwrap() as Id;
        let mut game = Game::new_character_ascension(&content, 567, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, encounter).unwrap();
        let strike = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(2);
        combat.hits.resize(2, 0);
        combat.enemy_power_snapshot.resize(2, Vec::new());
        combat.enemies[0].creature.hp = 5;
        combat.enemies[0].creature.block = 0;
        combat.enemies[1].creature.hp = 50;
        combat.enemies[1].creature.block = 20;
        combat.hand = vec![Card {
            id: strike,
            ..Card::default()
        }];
        combat.energy = 99;
        combat.queue.clear();
        combat.choice = None;
        combat.player.hp = combat.player.hp.max(1);
        let observation = observation_v56(&game, &content, layout, (0, 0));
        let targets = [0, 1].map(|target| {
            observation
                .candidates
                .iter()
                .position(|row| matches!(row.action, Action::Play { hand: 0, target: Some(value) } if value == target))
                .unwrap()
        });
        assert_ne!(
            observation.candidates[targets[0]].f,
            observation.candidates[targets[1]].f
        );
        assert_ne!(
            observation.candidates[targets[0]].u,
            observation.candidates[targets[1]].u
        );
        assert_eq!(
            observation.candidates[targets[0]].u[14],
            observation.candidates[targets[1]].u[14]
        );
        assert_eq!(
            ValueModel::candidate_object_key(&observation, targets[0]),
            ValueModel::candidate_object_key(&observation, targets[1])
        );
    }

    #[test]
    fn v56_action_auxiliaries_rederive_from_exact_sources() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 566, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let observation = observation_v56(&game, &content, layout, (0, 0));
        for candidate in &observation.candidates {
            assert_eq!(candidate.f, action_features(&candidate.u, &candidate.s));
        }
        let (globals, counts, mut exact, mut actions, digest) = packed_observation(&observation);
        assert_eq!(
            packed_observation_digest(observation.character, &globals, &counts, &exact, &actions,),
            digest
        );
        exact[0] ^= 1;
        assert_ne!(
            packed_observation_digest(observation.character, &globals, &counts, &exact, &actions,),
            digest
        );
        exact[0] ^= 1;
        let numeric = ACTION_U + ACTION_S + ACTION_C;
        actions[numeric] ^= 1;
        assert_ne!(
            packed_observation_digest(observation.character, &globals, &counts, &exact, &actions,),
            digest
        );
        let candidate = &observation.candidates[0];
        let mut unsigned = candidate.u;
        unsigned[8] ^= 1;
        assert_ne!(candidate.f, action_features(&unsigned, &candidate.s));
    }

    #[test]
    fn v55_power_trigger_order_is_semantic_and_actor_ratios_are_finite() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 554, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers = vec![
            Power {
                id: 0,
                amount: 1,
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: 1,
                amount: 1,
                skip_next_decay: false,
                value: 0,
            },
        ];
        let mut ordered = game.clone();
        let first = observation_v56(&game, &content, layout, (0, 0));
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.player.powers.reverse();
        let second = observation_v56(&game, &content, layout, (0, 0));
        let visible = |observation: &ObservationV56| {
            observation.domains[POWER_DOMAIN]
                .iter()
                .map(|row| (row.c.clone(), row.f.clone()))
                .collect::<Vec<_>>()
        };
        assert_ne!(visible(&first), visible(&second));
        ordered.step(&content, Action::EndTurn).unwrap();
        game.step(&content, Action::EndTurn).unwrap();
        assert_ne!(
            v56_bytes(&observation_v56(&ordered, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&game, &content, layout, (0, 0)))
        );
        let mut zero = DomainRow::new(ACTOR_DOMAIN, STATE_SCOPE);
        zero.u[..3].copy_from_slice(&[1, 0, 0]);
        populate_domain_features(&game, &content, layout, ACTOR_DOMAIN, &mut zero);
        assert!(zero.f.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn v56_ordered_piles_change_model_inputs_and_successors() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 558, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let bash = Card {
            id: content.card_id("CARD.BASH").unwrap(),
            ..Card::default()
        };
        let right = content.card_id("CARD.RIGHT_HAND_HAND").unwrap();
        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.hand = vec![bash];
        combat.discard = vec![
            Card {
                id: right,
                value: 1,
                ..Card::default()
            },
            Card {
                id: right,
                value: 2,
                ..Card::default()
            },
        ];
        combat.energy = 10;
        combat.enemies[0].creature.hp = 999;
        combat.enemies[0].creature.max_hp = 999;
        let mut second = first.clone();
        if let Phase::Combat(combat) = &mut second.phase {
            combat.discard.reverse();
        }
        assert_ne!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        first.step(&content, action.clone()).unwrap();
        second.step(&content, action).unwrap();
        assert_ne!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );

        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.hand = vec![
            Card { value: 1, ..bash },
            Card { value: 2, ..bash },
            Card { value: 3, ..bash },
        ];
        combat.choice = Some(Choice {
            pile: Pile::Hand,
            filter: CardFilter::Any,
            op: CardOp::TurnFlag(RETAIN),
            remaining: 1,
            optional: false,
        });
        combat.queue = vec![Pending {
            effect: Effect::ExhaustAttackStep(Target::ChosenEnemy, Amount::fixed(1, 1), 3),
            context: Context {
                target: Some(0),
                ..Context::player()
            },
        }];
        let mut second = first.clone();
        if let Phase::Combat(combat) = &mut second.phase {
            combat.hand.swap(0, 1);
        }
        first.step(&content, Action::Choose(2)).unwrap();
        second.step(&content, Action::Choose(2)).unwrap();
        assert_ne!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );
    }

    #[test]
    fn v56_unknown_draw_middle_remains_permutation_invariant() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 559, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let cards = first.run.deck.iter().copied().take(5).collect();
        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.draw = cards;
        combat.known_draw_bottom = 1;
        combat.known_draw_top = 1;
        let mut second = first.clone();
        if let Phase::Combat(combat) = &mut second.phase {
            combat.draw[1..4].reverse();
        }
        assert_eq!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );
    }

    #[test]
    fn v56_public_pools_and_previews_ignore_replay_oracles() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut first = Game::new_character_ascension(&content, 560, 0, 10).unwrap();
        let act = content
            .acts
            .iter()
            .position(|act| {
                !act.events.is_empty() && !act.encounters.is_empty() && !act.elites.is_empty()
            })
            .unwrap() as Id;
        first.begin_act(&content, act).unwrap();
        let mut second = first.clone();
        second.replaying = true;
        second.events = vec![content.acts[act as usize].events[0]];
        second.encounters.rotate_left(1);
        second.elites.rotate_left(1);
        assert_eq!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );

        first
            .start_combat(&content, content.acts[act as usize].encounters[0])
            .unwrap();
        let bombardment = Card {
            id: content.card_id("CARD.BOMBARDMENT").unwrap(),
            ..Card::default()
        };
        let strike = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.hand = vec![strike];
        combat.exhaust = vec![bombardment];
        combat.energy = 10;
        combat.enemies[0].creature.hp = 999;
        combat.enemies[0].creature.max_hp = 999;
        second = first.clone();
        second.replaying = true;
        let action = Action::Play {
            hand: 0,
            target: Some(0),
        };
        assert_eq!(
            action_preview(&first, &content, &action).unwrap().outcome,
            action_preview(&second, &content, &action).unwrap().outcome
        );
    }

    #[test]
    fn v56_enemy_instance_renumbering_preserves_wriggler_transitions() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let wriggler = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.WRIGGLER")
            .unwrap() as Id;
        let mut first = Game::new_character_ascension(&content, 561, 0, 10).unwrap();
        first.begin_act(&content, 0).unwrap();
        first
            .start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut first.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemy_power_snapshot.truncate(1);
        combat.player.hp = 999;
        combat.player.max_hp = 999;
        combat.enemies[0].creature.id = wriggler;
        combat.enemies[0].instance = 1;
        combat.enemies[0].move_index = 0;
        combat.enemies[0].last_move = 0;
        combat.enemies[0].move_history = vec![0];
        let mut second = first.clone();
        let Phase::Combat(combat) = &mut second.phase else {
            unreachable!()
        };
        combat.enemies[0].instance = 2;
        assert_eq!(first.actions(&content), second.actions(&content));
        assert_eq!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );
        first.step(&content, Action::EndTurn).unwrap();
        second.step(&content, Action::EndTurn).unwrap();
        assert_eq!(first.actions(&content), second.actions(&content));
        assert_eq!(
            v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
            v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
        );
    }

    #[test]
    fn v56_dynamic_event_ids_use_typed_semantics() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let event = |name| {
            content
                .events
                .iter()
                .position(|event| event.id == name)
                .unwrap() as Id
        };
        let mut game = Game::new_character_ascension(&content, 564, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();

        let colorful = event("EVENT.COLORFUL_PHILOSOPHERS");
        game.event_data = [1, 2, 3, 0];
        game.phase = Phase::Event(
            colorful,
            content.events[colorful as usize].options[..3].to_vec(),
        );
        let observation = observation_v56(&game, &content, layout, (0, 0));
        for character in 0..3 {
            assert!(
                observation.domains[PHASE_DOMAIN][0]
                    .c
                    .contains(&layout.semantic(Semantic::Character, character))
            );
            let scope = observation
                .candidates
                .iter()
                .position(|row| row.action == Action::Event(character as usize))
                .unwrap() as i32;
            assert!(observation.domains[EVENT_DOMAIN].iter().any(|row| {
                row.scope == scope
                    && row
                        .c
                        .contains(&layout.semantic(Semantic::Character, character))
            }));
        }

        let doll = event("EVENT.DOLL_ROOM");
        game.event_data = [0, 1, 2, 0];
        game.phase = Phase::Event(
            doll,
            vec![
                EventOption {
                    requirement: Requirement::Always,
                    effects: &[RunEffect::EventAction(10)],
                },
                EventOption {
                    requirement: Requirement::Always,
                    effects: &[RunEffect::EventAction(11)],
                },
                EventOption {
                    requirement: Requirement::Always,
                    effects: &[RunEffect::EventAction(12)],
                },
            ],
        );
        let observation = observation_v56(&game, &content, layout, (0, 0));
        for relic in 0..3 {
            assert!(
                observation.domains[PHASE_DOMAIN][0]
                    .c
                    .contains(&layout.semantic(Semantic::Relic, relic))
            );
            let scope = observation
                .candidates
                .iter()
                .position(|row| row.action == Action::Event(relic as usize))
                .unwrap() as i32;
            assert!(observation.domains[EVENT_DOMAIN].iter().any(|row| {
                row.scope == scope && row.c.contains(&layout.semantic(Semantic::Relic, relic))
            }));
        }

        let tinker = event("EVENT.TINKER_TIME");
        let options = vec![
            EventOption {
                requirement: Requirement::Always,
                effects: &[],
            };
            2
        ];
        game.event_data = [1, 3, 0, -1];
        game.phase = Phase::Event(tinker, options.clone());
        let observation = observation_v56(&game, &content, layout, (0, 0));
        for (option, card_type) in [0, 2].into_iter().enumerate() {
            assert!(
                observation.domains[PHASE_DOMAIN][0]
                    .c
                    .contains(&layout.semantic(Semantic::CardType, card_type))
            );
            let scope = observation
                .candidates
                .iter()
                .position(|row| row.action == Action::Event(option))
                .unwrap() as i32;
            assert!(observation.domains[EVENT_DOMAIN].iter().any(|row| {
                row.scope == scope
                    && row
                        .c
                        .contains(&layout.semantic(Semantic::CardType, card_type))
            }));
        }
        game.event_data = [4, 6, 0, 2];
        game.phase = Phase::Event(tinker, options);
        let observation = observation_v56(&game, &content, layout, (0, 0));
        assert!(
            observation.domains[PHASE_DOMAIN][0]
                .c
                .contains(&layout.semantic(Semantic::CardType, 1))
        );
        for (option, rider) in [3, 5].into_iter().enumerate() {
            assert!(
                observation.domains[PHASE_DOMAIN][0]
                    .c
                    .contains(&layout.semantic(Semantic::TinkerRider, rider))
            );
            let scope = observation
                .candidates
                .iter()
                .position(|row| row.action == Action::Event(option))
                .unwrap() as i32;
            assert!(observation.domains[EVENT_DOMAIN].iter().any(|row| {
                row.scope == scope
                    && row
                        .c
                        .contains(&layout.semantic(Semantic::TinkerRider, rider))
            }));
        }

        let conveyor = event("EVENT.ENDLESS_CONVEYOR");
        game.event_data = [7, 2, 3, 0];
        game.phase = Phase::Event(
            conveyor,
            content.events[conveyor as usize].options[..2].to_vec(),
        );
        let phase = &observation_v56(&game, &content, layout, (0, 0)).domains[PHASE_DOMAIN][0];
        assert!(
            phase
                .c
                .contains(&layout.semantic(Semantic::ConveyorDish, 6))
        );
        assert!(
            phase
                .c
                .contains(&layout.semantic(Semantic::ConveyorDish, 2))
        );
    }

    #[test]
    fn v55_periodic_relics_encode_phase_and_remaining() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let game = Game::new_character_ascension(&content, 555, 0, 10).unwrap();
        let id = content.relic_id("RELIC.HAPPY_FLOWER").unwrap();
        let row = |counter| {
            let mut row = DomainRow::new(RELIC_DOMAIN, STATE_SCOPE);
            row.u[..4].copy_from_slice(&[0, id as u32, 1, 0]);
            row.s[0] = counter;
            populate_domain_features(&game, &content, layout, RELIC_DOMAIN, &mut row);
            row
        };
        assert_eq!((row(0).f[10], row(0).f[11]), (0.0, 1.0));
        assert!((row(2).f[10] - 2.0 / 3.0).abs() < 1e-6);
        assert!((row(2).f[11] - 1.0 / 3.0).abs() < 1e-6);
        let id = content.relic_id("RELIC.SILVER_CRUCIBLE").unwrap();
        let capped = |counter| {
            let mut row = DomainRow::new(RELIC_DOMAIN, STATE_SCOPE);
            row.u[..4].copy_from_slice(&[0, id as u32, 1, 0]);
            row.s[0] = counter;
            populate_domain_features(&game, &content, layout, RELIC_DOMAIN, &mut row);
            row
        };
        assert_eq!((capped(2).f[12], capped(2).f[13]), (2.0 / 3.0, 1.0 / 3.0));
        assert_eq!((capped(3).f[14], capped(3).f[15]), (0.0, 1.0));
    }

    #[test]
    fn v55_action_and_phase_discriminants_are_exhaustive() {
        let actions = [
            Action::Play {
                hand: 0,
                target: None,
            },
            Action::Potion {
                slot: 0,
                target: None,
            },
            Action::DiscardPotion(0),
            Action::Choose(0),
            Action::EndTurn,
            Action::Path(0),
            Action::RewardGold,
            Action::RewardCard(0),
            Action::RewardRelic(0),
            Action::RewardPotion(0),
            Action::RewardRemove,
            Action::RerollCards,
            Action::SacrificeCards,
            Action::Buy(0),
            Action::Rest,
            Action::Hatch,
            Action::Lift,
            Action::Cook,
            Action::Kindle,
            Action::Dig,
            Action::Smith(0),
            Action::Event(0),
            Action::CrystalCell(0, 0),
            Action::CrystalTool(false),
            Action::EventRelic(0, 0),
            Action::EventCard(0, Card::default()),
            Action::Enchant(0),
            Action::RemoveCard(0),
            Action::Clone,
            Action::Cancel,
            Action::Done,
            Action::Leave,
        ];
        assert_eq!(
            actions.iter().map(action_kind).collect::<Vec<_>>(),
            (0..ACTION_KINDS).collect::<Vec<_>>()
        );

        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 556, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let combat = game.phase.clone();
        let phases = vec![
            Phase::Map,
            combat,
            Phase::Rewards(Rewards {
                gold: 0,
                cards: vec![],
                card_rewards: vec![],
                relics: vec![],
                potions: vec![],
                removals: 0,
            }),
            Phase::Shop(vec![]),
            Phase::Rest,
            Phase::Event(0, vec![]),
            Phase::RemoveCards(1, 0, false),
            Phase::UpgradeCards(1, false),
            Phase::TransformCards(None, 1, false),
            Phase::EnchantCards(Enchantment::Adroit, 1, 1, None, false),
            Phase::ChooseCards(vec![], 1, false),
            Phase::ChooseBundles(vec![]),
            Phase::Won,
            Phase::Dead,
        ];
        assert_eq!(
            phases.iter().map(phase_index).collect::<Vec<_>>(),
            (0..PHASES).collect::<Vec<_>>()
        );
    }

    #[test]
    fn v56_layout_uses_explicit_token_dimensions_and_positions() {
        let layout = Layout::new(&foundation_content());
        assert_eq!((VERSION, VALUE_MODEL_VERSION), (56, 72));
        assert_eq!(
            (MODEL_WIDTH, MODEL_LAYERS, MODEL_HEADS, MODEL_FEEDFORWARD),
            (128, 4, 8, 384)
        );
        for (semantic, size) in [
            (Semantic::EnemyPosition, 33),
            (Semantic::PowerPosition, 65),
            (Semantic::OrbPosition, 17),
            (Semantic::MapFloorPosition, 65),
            (Semantic::DeckOrigin, 257),
            (Semantic::DrawTopPosition, 257),
            (Semantic::DrawBottomPosition, 257),
            (Semantic::RelicPosition, 257),
            (Semantic::WaxPosition, 257),
            (Semantic::ContinuationPosition, 257),
            (Semantic::CrystalRow, 11),
            (Semantic::CrystalColumn, 11),
            (Semantic::CrystalWidth, 4),
            (Semantic::CrystalHeight, 4),
        ] {
            assert_eq!(layout.semantic_sizes[semantic as usize], size);
        }
    }

    #[test]
    fn v56_event_pool_contains_only_eligible_unvisited_events() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 45, 0, 10).unwrap();
        game.begin_act(&content, 1).unwrap();
        let eligible = content.acts[game.act as usize]
            .events
            .iter()
            .copied()
            .filter(|&id| game.event_allowed(&content, id))
            .collect::<Vec<_>>();
        let visited = eligible[0];
        game.visited_events.push(visited);
        let observation = observation_v56(&game, &content, layout, (0, 0));
        let actual = observation.domains[EVENT_DOMAIN]
            .iter()
            .filter(|row| row.scope == STATE_SCOPE && row.u[0] == 0)
            .map(|row| row.u[1] as Id)
            .collect::<Vec<_>>();
        assert!(!actual.contains(&visited));
        assert!(
            actual
                .iter()
                .all(|&id| game.event_allowed(&content, id) && !game.visited_events.contains(&id))
        );
    }

    #[test]
    fn v56_actions_are_legal_only_but_all_offers_are_phase_local() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 46, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.run.gold = 0;
        game.run.potions.fill(None);
        let card = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        };
        game.phase = Phase::Shop(vec![ShopItem::Card(card, 100)]);
        let observation = observation_v56(&game, &content, layout, (0, 0));
        assert_eq!(
            observation
                .candidates
                .iter()
                .map(|row| &row.action)
                .collect::<Vec<_>>(),
            [&Action::Leave]
        );
        assert!(observation.domains[CONTINUATION_DOMAIN].iter().any(|row| {
            row.scope == PHASE_SCOPE
                && row.u[0] == 5
                && row.u[11] == card.id as u32
                && row.s[5] == 100
        }));
        let position = layout.semantic_offsets[Semantic::ContinuationPosition as usize];
        let position_end =
            position + layout.semantic_sizes[Semantic::ContinuationPosition as usize];
        assert!(
            observation.domains[CONTINUATION_DOMAIN]
                .iter()
                .filter(|row| row.scope == PHASE_SCOPE)
                .all(|row| row
                    .c
                    .iter()
                    .all(|code| !(*code >= position && *code < position_end)))
        );

        let effects = Box::leak(Box::new([RunEffect::Gold(-999)]));
        game.phase = Phase::Event(
            0,
            vec![
                EventOption {
                    requirement: Requirement::Always,
                    effects: &[],
                },
                EventOption {
                    requirement: Requirement::Gold(999),
                    effects,
                },
            ],
        );
        let observation = observation_v56(&game, &content, layout, (0, 0));
        assert_eq!(
            observation
                .candidates
                .iter()
                .map(|row| &row.action)
                .collect::<Vec<_>>(),
            [&Action::Event(0)]
        );
        assert_eq!(
            observation.domains[CONTINUATION_DOMAIN]
                .iter()
                .filter(|row| row.scope == PHASE_SCOPE && row.u[0] == 7)
                .count(),
            2
        );
    }

    #[test]
    fn v56_card_versions_origins_and_enemy_targets_are_explicit() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let strike = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        assert_ne!(
            card_version_id(&content, strike, false, 0),
            card_version_id(&content, strike, true, 0)
        );
        let mad = content.card_id("CARD.MAD_SCIENCE").unwrap();
        assert_ne!(
            card_version_id(&content, mad, false, 1),
            card_version_id(&content, mad, false, 2)
        );

        let mut game = Game::new_character_ascension(&content, 84, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![Card {
            id: strike,
            instance: game.run.deck[0].instance,
            ..Card::default()
        }];
        combat.energy = 99;
        let observation = observation_v56(&game, &content, layout, (0, 0));
        let origin = layout.semantic(Semantic::DeckOrigin, 0);
        assert!(
            observation.domains[CARD_DOMAIN]
                .iter()
                .filter(|row| row.c.contains(&origin))
                .count()
                >= 2
        );
        let action = observation
            .candidates
            .iter()
            .find(|row| {
                matches!(
                    row.action,
                    Action::Play {
                        hand: 0,
                        target: Some(0)
                    }
                )
            })
            .unwrap();
        let actor = observation.domains[ACTOR_DOMAIN]
            .iter()
            .find(|row| row.u[1] == 2)
            .unwrap();
        let enemy = layout.semantic(Semantic::Enemy, actor.u[2]);
        let position = layout.semantic(Semantic::EnemyPosition, 0);
        assert!(action.c.contains(&enemy) && action.c.contains(&position));
        assert!(actor.c.contains(&enemy) && actor.c.contains(&position));
    }

    #[test]
    fn v56_continuations_are_flat_execution_items() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 91, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let repeated: &'static [Effect] = Box::leak(Box::new([Effect::Draw(1)]));
        let yes: &'static [Effect] = Box::leak(Box::new([Effect::Block(
            Target::Player,
            Amount::fixed(3, 3),
        )]));
        let no: &'static [Effect] = Box::leak(Box::new([Effect::Damage(
            Target::Player,
            Amount::fixed(99, 99),
        )]));
        let random: &'static [Effect] = Box::leak(Box::new([
            Effect::If(Condition::Always, yes, no),
            Effect::Repeat(Amount::fixed(2, 2), repeated),
        ]));
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.queue = vec![
            Pending {
                effect: Effect::Repeat(Amount::fixed(2, 2), repeated),
                context: Context::player(),
            },
            Pending {
                effect: Effect::If(Condition::Always, yes, no),
                context: Context::player(),
            },
            Pending {
                effect: Effect::Random(1, random),
                context: Context::player(),
            },
        ];
        let rows = continuation_domains(&game, &content);
        let items = rows
            .iter()
            .map(|row| row.u[2])
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(items.len(), 4);
        assert!(
            rows.iter()
                .all(|row| row.u[3] == NO_NODE && row.u[4..8] == [0; 4] && row.u[8] > 0)
        );
        assert!(rows.iter().all(|row| !matches!(row.u[1], 92 | 93)));
    }

    #[test]
    fn v56_continuation_amounts_use_declared_scales_and_current_values() {
        let content = foundation_content();
        let layout = Layout::new(&content);
        let mut game = Game::new_character_ascension(&content, 92, 0, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let amount = Amount {
            base: 3,
            upgraded: 5,
            ascension: 0,
            scale: Scale::X,
            multiplier: 2,
            divisor: 1,
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.queue.push(Pending {
            effect: Effect::Damage(Target::Player, amount),
            context: Context {
                upgraded: true,
                x: 4,
                ..Context::player()
            },
        });
        let mut rows = continuation_domains(&game, &content);
        for row in &mut rows {
            populate_domain_features(&game, &content, layout, CONTINUATION_DOMAIN, row);
        }
        let effect = rows
            .iter()
            .find(|row| row.scope == STATE_SCOPE && row.u[0] == 1)
            .unwrap();
        assert_eq!(effect.f[16..20], [5.0 / 30.0, 0.2, 0.1, 13.0 / 30.0]);
        let context = rows
            .iter()
            .find(|row| row.scope == STATE_SCOPE && row.u[0] == 2)
            .unwrap();
        assert_eq!(context.f[..2], [0.8, 0.0]);
    }
}
