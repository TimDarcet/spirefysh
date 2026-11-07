use super::*;

// Tables mirror effect_index, event_effect_index, action_kind, *_payload, room_index,
// target_index, pile_index and phase_index; the model tags token roles and collections.
pub(super) const EFFECTS: [&str; 125] = [
    "Attack",
    "AttackMany",
    "OstyAttack",
    "OstyAttackMany",
    "MoveDamage",
    "Damage",
    "Kill",
    "Stun",
    "DoomKill",
    "LoseHp",
    "Block",
    "BlockNextTurn",
    "DodgeRoll",
    "DrawBlockIf",
    "RawBlock",
    "Heal",
    "HealPercent",
    "MaxHp",
    "Gold",
    "RandomPotion",
    "Bomb",
    "Automation",
    "Panache",
    "RollingBoulder",
    "ToricToughness",
    "ApplyPower",
    "ApplyDebuff",
    "StackPower",
    "DoublePower",
    "Misery",
    "RemovePower",
    "TemporaryStrength",
    "Draw",
    "HandDraw",
    "DrawAmount",
    "DrawTo",
    "DrawUntilNot",
    "ChooseDraw",
    "FreeHand",
    "Energy",
    "DoubleEnergy",
    "AddCard",
    "AddUpgradedCard",
    "AddFlaggedCard",
    "RandomCard",
    "RandomColorless",
    "RandomColorlessOther",
    "OfferColorless",
    "OfferCharacter",
    "OfferCharacterRetain",
    "OfferCharacterType",
    "OfferOtherCharacter",
    "RandomCharacter",
    "DistinctCharacter",
    "RandomCharacterCost",
    "RandomCardOp",
    "AutoPlayRandom",
    "ChooseRandomDraw",
    "SelectAmount",
    "ShuffleHandDraw",
    "FillPotions",
    "RandomizeHandCosts",
    "ReplayTagged",
    "ChannelSlots",
    "DistinctColorless",
    "AddRandomCard",
    "AddRandom",
    "FranticEscape",
    "Aggression",
    "AutoPlayDraw",
    "Stoke",
    "Stampede",
    "Fire",
    "FlakCannon",
    "CopyCard",
    "Discard",
    "DiscardHandDraw",
    "DiscardHandAdd",
    "PlayExhaustedShivs",
    "AutoPlay",
    "PlacePlayed",
    "WhisperingEarring",
    "MoveAll",
    "DrawFiltered",
    "DrawFilteredStep",
    "FinishDrawFiltered",
    "Exhaust",
    "ExhaustForBlock",
    "ExhaustForAttack",
    "ExhaustAttackStep",
    "Upgrade",
    "TransformHand",
    "If",
    "Repeat",
    "Random",
    "Channel",
    "RandomOrb",
    "Evoke",
    "EvokeMany",
    "EvokeLast",
    "EvokeAll",
    "PassiveFirst",
    "PassiveLast",
    "PassiveAll",
    "OrbSlots",
    "Stars",
    "Forge",
    "Summon",
    "MaxEnergy",
    "GrowCard",
    "GrowDrawn",
    "GrowAll",
    "PersistCard",
    "SetCardCost",
    "ReduceCardCost",
    "CapHandCosts",
    "RecycleHand",
    "EndTurn",
    "Select",
    "PassiveLightning",
    "Listen",
    "Step",
    "BeforePlay",
    "OnPlay",
    "AfterPlay",
];

pub(super) const TRIGGERS: [&str; 35] = [
    "AfterRoomEntered",
    "BeforeCombatStart",
    "BeforeCombatStartLate",
    "BeforeSideTurnStart",
    "AfterBlockCleared",
    "AfterEnergyReset",
    "AfterEnergyResetLate",
    "BeforeHandDraw",
    "AfterPlayerTurnStart",
    "AfterPlayerTurnStartLate",
    "AfterSideTurnStart",
    "AfterSideTurnStartLate",
    "AutoPrePlay",
    "AutoPrePlayLate",
    "BeforeSideTurnEndVeryEarly",
    "BeforeSideTurnEndEarly",
    "BeforeSideTurnEnd",
    "AfterSideTurnEnd",
    "AfterSideTurnEndLate",
    "TurnEnd",
    "AfterCardPlayed",
    "AttackPlayed",
    "SkillPlayed",
    "PowerPlayed",
    "AfterBlockGained",
    "AfterDamageGiven",
    "AfterDamageReceived",
    "AfterDeath",
    "AfterCardDrawn",
    "AfterCardGeneratedForCombat",
    "AfterCardExhausted",
    "AfterShuffle",
    "AfterPotionUsed",
    "AfterCombatVictory",
    "BeforeCardPlayed",
];

pub(super) const RUN_EFFECTS: [&str; 37] = [
    "Gold",
    "RandomGold",
    "LoseAllGold",
    "Heal",
    "HealPercent",
    "FullHeal",
    "LoseHp",
    "MaxHp",
    "MaxHpTo",
    "AddCard",
    "AddRelic",
    "AddPotion",
    "PotionRewards",
    "RandomPotionReward",
    "NextRelics",
    "RelicOfRarity",
    "EventRelic",
    "RandomCard",
    "RandomRelic",
    "DiscardPotion",
    "DiscardRandomPotion",
    "RemoveCards",
    "UpgradeCards",
    "UpgradeRandom",
    "UpgradeShuffled",
    "UpgradeAll",
    "DowngradeRandom",
    "CloneDeck",
    "TransformCards",
    "EnchantCards",
    "SkipEventRng",
    "ChooseCommonCards",
    "ChooseRewardCards",
    "RemoveRandomCard",
    "Options",
    "EventAction",
    "FinishCooking",
];

pub(super) const ACTIONS: [&str; 32] = [
    "Play",
    "Potion",
    "DiscardPotion",
    "Choose",
    "EndTurn",
    "Path",
    "RewardGold",
    "RewardCard",
    "RewardRelic",
    "RewardPotion",
    "RewardRemove",
    "RerollCards",
    "SacrificeCards",
    "Buy",
    "Rest",
    "Hatch",
    "Lift",
    "Cook",
    "Kindle",
    "Dig",
    "Smith",
    "Event",
    "CrystalCell",
    "CrystalTool",
    "EventRelic",
    "EventCard",
    "Enchant",
    "RemoveCard",
    "Clone",
    "Cancel",
    "Done",
    "Leave",
];

pub(super) const CONDITIONS: [&str; 23] = [
    "Always",
    "Upgraded",
    "TargetAlive",
    "TargetHasPower",
    "SourceHasPower",
    "HandAtMost",
    "HandAtLeast",
    "HandWithout",
    "HandEmpty",
    "EventAtLeast",
    "ExhaustAtLeast",
    "ExhaustedThisTurn",
    "HpLostThisTurn",
    "TargetDead",
    "FirstTurn",
    "HasOrb",
    "CardType",
    "PriorCardsBelow",
    "TargetAttacking",
    "XAtLeast",
    "OstyAlive",
    "DoomApplied",
    "Card",
];

pub(super) const FILTERS: [&str; 16] = [
    "Any",
    "Type",
    "AttackOrPower",
    "TypeWithoutTurnFlag",
    "WithoutFlag",
    "NotType",
    "Id",
    "Cost",
    "PlayableCost",
    "Flag",
    "Upgradable",
    "Colorless",
    "Rare",
    "NoReplay",
    "PlayableOrAny",
    "CostsResource",
];

pub(super) const CARD_OPS: [&str; 18] = [
    "Move",
    "Upgrade",
    "Cost",
    "SetCost",
    "Flag",
    "TurnFlag",
    "CopyNextTurn",
    "Transform",
    "AutoPlay",
    "CopySelected",
    "TakeOffer",
    "Transfigure",
    "Replay",
    "TakeFetched",
    "TransformRandom",
    "DiscardDraw",
    "MoveFree",
    "FreeCombat",
];

pub(super) const SCALES: [&str; 47] = [
    "None",
    "X",
    "Block",
    "Energy",
    "MissingHp",
    "DrawSize",
    "DiscardSize",
    "ExhaustSize",
    "ExhaustId",
    "HandSize",
    "CardsPlayed",
    "AttacksPlayed",
    "PriorAttacks",
    "SkillsPlayed",
    "EnergySpent",
    "Exhausted",
    "HpLost",
    "HpLossEvents",
    "CardValue",
    "LivingEnemies",
    "Orbs",
    "OrbTypes",
    "OrbTypesPower",
    "Stars",
    "TargetPower",
    "TargetPowerDiv",
    "Tagged",
    "Power",
    "Event",
    "EventPower",
    "HandType",
    "EnemyPowerTotal",
    "TargetDebuffs",
    "Discarded",
    "DrawnCombat",
    "StarCards",
    "StarsGained",
    "Generated",
    "PriorTargetHits",
    "OstyHp",
    "OstyMaxHp",
    "OstyAttacks",
    "EtherealPlayed",
    "LightningChanneled",
    "LastDamage",
    "ExtraDrawn",
    "TurnDiv",
];

pub(super) const CARD_TYPES: [&str; 6] = ["Attack", "Skill", "Power", "Status", "Curse", "Quest"];

pub(super) const RARITIES: [&str; 7] = [
    "Basic", "Common", "Uncommon", "Rare", "Ancient", "Event", "Quest",
];

pub(super) const ENCHANTMENTS: [&str; 22] = [
    "Adroit",
    "Clone",
    "Corrupted",
    "Glam",
    "Goopy",
    "Imbued",
    "Inky",
    "Instinct",
    "Momentum",
    "Nimble",
    "PerfectFit",
    "RoyallyApproved",
    "Sharp",
    "Slither",
    "SlumberingEssence",
    "SoulsPower",
    "Sown",
    "Spiral",
    "Steady",
    "Swift",
    "TezcatarasEmber",
    "Vigorous",
];

pub(super) const ROOMS: [&str; 8] = [
    "Combat", "Elite", "Boss", "Unknown", "Event", "Shop", "Rest", "Treasure",
];

pub(super) const TARGETS: [&str; 9] = [
    "Source",
    "Player",
    "Osty",
    "ChosenEnemy",
    "ChosenEnemyOrDead",
    "AllEnemies",
    "OtherEnemies",
    "RandomEnemy",
    "LowestHpEnemy",
];

pub(super) const PILES: [&str; 5] = ["Draw", "Hand", "Discard", "Exhaust", "Offer"];

pub(super) const PHASES: [&str; 12] = [
    "Map",
    "Combat",
    "Rewards",
    "Shop",
    "Rest",
    "Event",
    "RemoveCards",
    "UpgradeCards",
    "TransformCards",
    "EnchantCards",
    "ChooseCards",
    "ChooseBundles",
];

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

pub(super) const ORB_TIMINGS: [&str; 2] = ["turn start", "turn end"];

pub(super) const REQUIREMENTS: [&str; 4] = ["always", "gold", "HP", "deck"];

pub(super) const ACTOR_KINDS: [&str; 3] = ["player", "Osty", "enemy"];

pub(super) const OFFER_KINDS: [&str; 8] = [
    "standard card reward",
    "fixed card reward",
    "Kaleidoscope card reward",
    "Crystal Sphere card reward",
    "shop card",
    "shop relic",
    "shop potion",
    "shop removal",
];

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
                n if n == Semantic::Enchantment as usize => named(&ENCHANTMENTS, value),
                n if n == Semantic::CardType as usize => named(&CARD_TYPES, value),
                n if n == Semantic::Rarity as usize => named(&RARITIES, value),
                n if n == Semantic::Room as usize => named(&ROOMS, value),
                n if n == Semantic::Target as usize => named(&TARGETS, value),
                n if n == Semantic::Pile as usize => named(&PILES, value),
                n if n == Semantic::Phase as usize || n == Semantic::SuspendedPhase as usize => {
                    named(&PHASES, value)
                }
                n if n == Semantic::Action as usize => named(&ACTIONS, value),
                n if n == Semantic::Effect as usize => named(&EFFECTS, value),
                n if n == Semantic::RunEffect as usize => named(&RUN_EFFECTS, value),
                n if n == Semantic::Condition as usize => named(&CONDITIONS, value),
                n if n == Semantic::Filter as usize => named(&FILTERS, value),
                n if n == Semantic::CardOp as usize => named(&CARD_OPS, value),
                n if n == Semantic::Scale as usize => named(&SCALES, value),
                n if n == Semantic::FlagBit as usize || n == Semantic::TurnFlagBit as usize => {
                    named(&CARD_FLAGS, value)
                }
                n if n == Semantic::TagBit as usize => named(&TAGS, value),
                n if n == Semantic::OrbTiming as usize => named(&ORB_TIMINGS, value),
                n if n == Semantic::Requirement as usize => named(&REQUIREMENTS, value),
                n if n == Semantic::ActorKind as usize => named(&ACTOR_KINDS, value),
                n if n == Semantic::OfferKind as usize => named(&OFFER_KINDS, value),
                n if n == Semantic::PlayKind as usize => named(&PLAY_KINDS, value),
                n if n == Semantic::Flag as usize => named(&FLAG_NAMES, value),
                n if n == Semantic::PowerValue as usize => format!("{} value", content.powers[value].id),
                n if n == Semantic::EventValue as usize => {
                    format!("{} slot {}", content.events[value / 4].id, value % 4)
                }
                n if n == Semantic::EffectFlag as usize => {
                    format!("{} flag {}", named(&EFFECTS, value / 2), value % 2)
                }
                n if n == Semantic::CrystalWidth as usize
                    || n == Semantic::CrystalHeight as usize =>
                {
                    format!("{}", value + 1)
                }
                n if n == Semantic::TokenRole as usize => named(&TOKEN_ROLES, value),
                n if n == Semantic::Collection as usize => named(&COLLECTIONS, value),
                n if n == Semantic::Trigger as usize => named(&TRIGGERS, value),
                _ => format!("{value}"),
            };
            labels[layout.semantic_offsets[namespace] as usize + value] =
                format!("{name}: {label}");
        }
    }
    labels
}
