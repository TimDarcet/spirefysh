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
const VALUE_MODEL_VERSION: u32 = 74;
const MIN_VALUE_MODEL_VERSION: u32 = 74;
const TERMINAL_CATEGORIES: usize = 83;
const POTENTIAL_WEIGHT_COUNT: usize = 13;
const CARD_ZONES: usize = 5;
const DECK_ZONE: usize = 0;
const HAND_ZONE: usize = 1;
const DRAW_ZONE: usize = 2;
const DISCARD_ZONE: usize = 3;
const EXHAUST_ZONE: usize = 4;
const ATTACHED_CARD_ZONE: usize = CARD_ZONES;
const PAEL_ZONE: usize = 8;
const CARD_POOL_ZONE: usize = 13;
const HISTORY_COURSE_STATUS: u32 = 6;

const PHASES: usize = 14;
const ENCHANTMENTS: usize = 22;
const PUBLIC_GLOBALS: usize = 0;
const DEFAULT_MODEL_WIDTH: usize = 128;
const DEFAULT_MODEL_LAYERS: usize = 4;
const DEFAULT_MODEL_HEADS: usize = 8;
const DEFAULT_MODEL_FEEDFORWARD: usize = 384;
const POSITION_CAPS: [u32; 14] = [32, 64, 16, 64, 256, 256, 256, 256, 256, 256, 11, 11, 4, 4];
const POSITION_NAMES: [&str; 14] = [
    "enemy",
    "power",
    "orb",
    "map_floor",
    "deck_origin",
    "draw_top",
    "draw_bottom",
    "relic",
    "wax",
    "continuation",
    "crystal_row",
    "crystal_column",
    "crystal_width",
    "crystal_height",
];
const KNOWN_DRAW_SLOTS: usize = u8::MAX as usize;
const ACTION_KINDS: usize = 32;
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

#[derive(Clone, Debug, Default, PartialEq)]
struct Potential {
    floor: f32,
    resources: [f32; POTENTIAL_WEIGHT_COUNT],
}

#[derive(Clone, Debug, PartialEq)]
struct ObservationV56 {
    character: u8,
    globals: Vec<f32>,
    domains: [DomainRows; 16],
    candidates: Vec<CandidateRow>,
    potential: Potential,
}

#[derive(Clone, Copy)]
struct Layout {
    characters: usize,
    semantic_offsets: [u32; Semantic::Count as usize],
    semantic_sizes: [u32; Semantic::Count as usize],
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
                Phase::Combat(combat) if combat.choice.is_some() => {
                    push_semantic(row, layout, Semantic::Pile, row.u[5]);
                    push_filter_semantics(row, layout, 6, 7);
                    push_op_semantics(row, layout, 8, 9);
                }
                Phase::TransformCards(target, ..) if target.is_some() => {
                    push_semantic(row, layout, Semantic::Card, base_card_id(row.u[4] - 1));
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
                9..=11 => row.f[0] = sf(row.s[0], 10.0),
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

fn pile_index(pile: Pile) -> usize {
    match pile {
        Pile::Draw => 0,
        Pile::Hand => 1,
        Pile::Discard => 2,
        Pile::Exhaust => 3,
        Pile::Offer => 4,
    }
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
                if let Some(count) = exact_draw_until_not(game, content, kind)
                    && let Some(drawn) = preview_draw_count(
                        game,
                        content,
                        count.saturating_mul(repeats),
                        false,
                        preview,
                    )
                {
                    preview.draw = preview.draw.saturating_add(drawn);
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
            Effect::RecycleHand(draw) if !draw_forbidden(game, content, false) => {
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
            Effect::ShuffleHandDraw(draw) if !draw_forbidden(game, content, false) => {
                preview.draw_available = preview.draw_available.saturating_add(preview.hand);
                preview.hand = 0;
                if let Some(drawn) = preview_draw_count(game, content, draw as i16, false, preview)
                {
                    preview.draw = preview.draw.saturating_add(drawn.saturating_mul(repeats));
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
            Effect::Heal(target, amount) | Effect::HealPercent(target, amount)
                if targets_player(target, context) =>
            {
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
    let danse = if cost >= 2 {
        combat.player.power(power_id::DANSE_MACABRE)
    } else {
        0
    };
    let ash = if card.flags(def) & ETHEREAL != 0 {
        combat.player.power(power_id::SPIRIT_OF_ASH)
    } else {
        0
    };
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
    let attack_triggers = if card_type == CardType::Attack {
        (old_attacks + plays as i16) / 3 - old_attacks / 3
    } else {
        0
    };
    let skill_triggers = if card_type == CardType::Skill {
        (old_skills + plays as i16) / 3 - old_skills / 3
    } else {
        0
    };
    if relic("RELIC.ORNAMENTAL_FAN") {
        preview_gain_block(&projected, content, &mut preview, 4, attack_triggers);
    }
    let kusarigama = if card_type == CardType::Attack && relic("RELIC.KUSARIGAMA") {
        (original.kusarigama as i16 + plays as i16) / 3
    } else {
        0
    };
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

fn public_encounters(game: &Game, content: &Content, elite: bool) -> Vec<Id> {
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

fn candidate_actions(game: &Game, content: &Content) -> (Vec<Action>, Vec<bool>) {
    let legal = game.actions(content);
    let mask = vec![true; legal.len()];
    (legal, mask)
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
    if let Phase::Combat(combat) = &game.phase {
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
                    if content.enemies[enemy.creature.id as usize].id == "MONSTER.WRIGGLER" {
                        (owner - 3) % 2 + 1
                    } else {
                        0
                    },
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

fn public_encounter_pool(game: &Game, content: &Content, elite: bool) -> Vec<Id> {
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
    let (zone, role, order_kind, order, card, context) = (match action {
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
    })?;
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
                .is_none_or(|current| game.map.nodes[current].next.contains(index));
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
    let (actions, legal) = candidate_actions(game, content);
    observation_v56_with_candidates(game, content, layout, bonuses, map, actions, legal)
}

fn observation_v56_with_candidates(
    game: &Game,
    content: &Content,
    layout: Layout,
    bonuses: (i16, i16),
    map: Option<&CanonicalMap>,
    actions: Vec<Action>,
    legal: Vec<bool>,
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
        potential: potential(game, content),
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

fn potential(game: &Game, content: &Content) -> Potential {
    if matches!(game.phase, Phase::Won | Phase::Dead) {
        return Potential::default();
    }
    let hp = game
        .combat()
        .map_or(game.run.hp, |combat| combat.player.hp)
        .max(0) as f32;
    let relics = &game.run.relics;
    let deck = &game.run.deck;
    let resources = [
        1.0,
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
            .filter(|&&id| crate::game::relic_group(id) == Some(1))
            .count() as f32,
        relics
            .iter()
            .filter(|&&id| crate::game::relic_group(id) == Some(2))
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
    ]
    .map(|term| hp * term);
    Potential {
        floor: canonical_progress(game) as f32 / (TERMINAL_CATEGORIES - 1) as f32,
        resources,
    }
}

fn potential_value(potential: &Potential, weights: &[f32; POTENTIAL_WEIGHT_COUNT]) -> f32 {
    let resources = potential
        .resources
        .iter()
        .zip(weights)
        .map(|(term, weight)| term * weight)
        .sum::<f32>();
    potential.floor + resources
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

#[path = "value_model.rs"]
mod value_model;

pub use value_model::ValueModel;
#[cfg(test)]
use value_model::{LinearWeights, TokenEncoderWeights, content_fingerprint, valid_model_shape};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
