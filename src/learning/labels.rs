use super::*;
use std::sync::LazyLock;

/// Each enum of lib.rs, its variant names in declaration order (their `Variant` index), read from
/// the declarations so the names follow them.
pub(super) fn variants(name: &str) -> &'static [&'static str] {
    static ENUMS: LazyLock<HashMap<&str, Vec<&str>>> = LazyLock::new(|| {
        (include_str!("../lib.rs").split("\npub enum ").skip(1))
            .map(|declaration| {
                let (name, body) = declaration.split_once(" {\n").unwrap();
                let variants = (body.split("\n}").next().unwrap().lines())
                    .filter_map(|line| line.strip_prefix("    "))
                    .filter(|line| line.starts_with(char::is_uppercase))
                    .map(|line| line.split(|c: char| !c.is_alphanumeric()).next().unwrap())
                    .collect();
                (name, variants)
            })
            .collect()
    });
    &ENUMS[name]
}

pub(super) const CARD_FLAGS: [&str; 16] = [
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
];

pub(super) const TAGS: [&str; 5] = ["strike", "defend", "shiv", "Osty attack", "minion"];

pub(super) const PLAY_KINDS: [&str; 4] = ["queued auto-play", "manual play", "extra turn", "random curse"];

pub(super) const TOKEN_ROLES: [&str; 1] = ["current map node"];

pub(super) const COLLECTIONS: [&str; 11] = [
    "deck",
    "hand",
    "draw",
    "discard",
    "exhaust",
    "relic",
    "potion",
    "orb",
    "friendly effect",
    "enemy effect",
    "continuation",
];

fn named(names: &[&str], value: usize) -> String {
    names
        .get(value)
        .map_or_else(|| format!("{value}"), |name| name.to_string())
}

pub(super) fn concept_labels(content: &Content) -> Vec<String> {
    let layout = Layout::new(content);
    let encounters = encounter_vocabs(content);
    let moves = content
        .enemies
        .iter()
        .flat_map(|enemy| {
            enemy
                .moves
                .iter()
                .map(move |movement| format!("{} {}", enemy.id, movement.intent))
        })
        .collect::<Vec<_>>();
    let mut labels = vec![String::from("padding"); layout.concept_vocab()];
    for (namespace, name) in SEMANTIC_NAMES.into_iter().enumerate() {
        for value in 0..layout.semantic_sizes[namespace] as usize {
            let label = match namespace {
                n if n == Semantic::Character as usize => content.characters[value].id.to_string(),
                n if n == Semantic::Card as usize => match content.cards.get(value / 2) {
                    Some(card) => format!("{}{}", card.id, if value % 2 == 1 { "+" } else { "" }),
                    None => format!(
                        "CARD.MAD_SCIENCE variant {}{}",
                        (value - content.cards.len() * 2) / 2,
                        if value % 2 == 1 { "+" } else { "" }
                    ),
                },
                n if n == Semantic::Power as usize => content.powers[value].id.to_string(),
                n if n == Semantic::Relic as usize => content.relics[value].id.to_string(),
                n if n == Semantic::Potion as usize => content
                    .potions
                    .get(value)
                    .map_or_else(|| "none".to_string(), |potion| potion.id.to_string()),
                n if n == Semantic::Enemy as usize => content.enemies[value].id.to_string(),
                n if n == Semantic::EnemyMove as usize => moves
                    .get(value)
                    .cloned()
                    .unwrap_or_else(|| "none".to_string()),
                n if n == Semantic::Orb as usize => content.orbs[value].id.to_string(),
                n if n == Semantic::Event as usize => content.events[value].id.to_string(),
                n if n == Semantic::EncounterNormal as usize => content.encounters
                    [encounters[0][value] as usize]
                    .id
                    .to_string(),
                n if n == Semantic::EncounterElite as usize => content.encounters
                    [encounters[1][value] as usize]
                    .id
                    .to_string(),
                n if n == Semantic::Enchantment as usize => named(variants("Enchantment"), value),
                n if n == Semantic::CardType as usize => named(variants("CardType"), value),
                n if n == Semantic::Rarity as usize => named(variants("CardRarity"), value),
                n if n == Semantic::Room as usize => named(variants("Room"), value),
                n if n == Semantic::Target as usize => named(variants("Target"), value),
                n if n == Semantic::Pile as usize => named(variants("Pile"), value),
                n if n == Semantic::Phase as usize || n == Semantic::SuspendedPhase as usize => {
                    named(variants("Phase"), value)
                }
                n if n == Semantic::Action as usize => named(variants("Action"), value),
                n if n == Semantic::Effect as usize => named(variants("Effect"), value),
                n if n == Semantic::RunEffect as usize => named(variants("RunEffect"), value),
                n if n == Semantic::Condition as usize => named(variants("Condition"), value),
                n if n == Semantic::Filter as usize => named(variants("CardFilter"), value),
                n if n == Semantic::CardOp as usize => named(variants("CardOp"), value),
                n if n == Semantic::Scale as usize => named(variants("Scale"), value),
                n if n == Semantic::FlagBit as usize || n == Semantic::TurnFlagBit as usize => {
                    named(&CARD_FLAGS, value)
                }
                n if n == Semantic::TagBit as usize => named(&TAGS, value),
                n if n == Semantic::OrbTiming as usize => named(variants("OrbTiming"), value),
                n if n == Semantic::Requirement as usize => named(variants("Requirement"), value),
                n if n == Semantic::ActorKind as usize => named(variants("Actor"), value),
                n if n == Semantic::CardReward as usize => named(variants("CardReward"), value),
                n if n == Semantic::ShopItem as usize => named(variants("ShopItem"), value),
                n if n == Semantic::PlayKind as usize => named(&PLAY_KINDS, value),
                n if n == Semantic::Flag as usize => named(&FLAG_NAMES, value),
                n if n == Semantic::PowerValue as usize => format!("{} value", content.powers[value].id),
                n if n == Semantic::EventValue as usize => {
                    format!("{} slot {}", content.events[value / 4].id, value % 4)
                }
                n if n == Semantic::EffectFlag as usize => {
                    format!(
                        "{} flag {}",
                        named(variants("Effect"), value / 2),
                        value % 2
                    )
                }
                n if n == Semantic::CrystalWidth as usize
                    || n == Semantic::CrystalHeight as usize =>
                {
                    format!("{}", value + 1)
                }
                n if n == Semantic::TokenRole as usize => named(&TOKEN_ROLES, value),
                n if n == Semantic::Collection as usize => named(&COLLECTIONS, value),
                n if n == Semantic::Trigger as usize => named(variants("Trigger"), value),
                _ => format!("{value}"),
            };
            labels[layout.semantic_offsets[namespace] as usize + value] =
                format!("{name}: {label}");
        }
    }
    labels
}
