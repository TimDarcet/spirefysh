use crate::game::{event_page, relic_deques};
use crate::*;
use serde_json::Value;
#[cfg(feature = "python")]
use serde_json::json;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
};

#[derive(Debug)]
pub struct Divergence {
    pub sequence: u64,
    pub game_action_id: Option<u64>,
    pub action: String,
    pub differences: Vec<String>,
}

#[derive(Debug, Default)]
pub struct ReplayReport {
    pub decisions: usize,
    pub replayed: usize,
    pub unsupported: usize,
    pub divergences: Vec<Divergence>,
}

#[derive(Default)]
struct TurnHistory {
    encounter: String,
    round: u16,
    cards: i16,
    manual_cards: i16,
    manual_plays: i16,
    attacks: i16,
    skills: i16,
    powers: i16,
    last_card: Option<Card>,
    block_gains: i16,
    last_cards: i16,
    values: HashMap<u32, i16>,
    upgrades: HashMap<u32, u8>,
    toric: Vec<(u8, i16)>,
    withering: i16,
    wither_upgrades: i16,
    enemy_moves: HashMap<u32, Vec<String>>,
}

struct ReplayState {
    episode: String,
    act: u8,
    unknown_odds: [i16; 4],
    rarity_offset: i16,
    potion_odds: i8,
    card_shop_removals: u16,
    relic_deques: [Vec<Id>; 4],
    shared_relic_deques: [Vec<Id>; 4],
    card_variants: Vec<u8>,
    lasting_candy: u8,
    paels_wing: u8,
    silver_crucible: u8,
    silken_tress: bool,
    maw_bank: bool,
    silver_treasures: u8,
    golden_compass: Option<u8>,
    winged_boots: u8,
    rest_used: u8,
    girya: u8,
    pumpkin_candle: u8,
    paels_cards: Vec<Card>,
    pollinous_core: u8,
    fur_coat_act: Option<u8>,
    fur_coat: Vec<(u8, u8)>,
    toy_box_offers: Vec<Id>,
    wax_relics: Vec<usize>,
    melted_relics: Vec<usize>,
    toy_box_combats: u8,
    parasol: Vec<ShopItem>,
    parasol_removal: bool,
    conveyor: bool,
    visited_events: Vec<Id>,
    crystal: Option<CrystalSphere>,
    reward_gold_parts: Vec<i32>,
    fake_merchant: Vec<Id>,
    fake_shop: bool,
    fake_happy_flower: u8,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            episode: String::new(),
            act: 0,
            unknown_odds: [1000, -10_000, 200, 300],
            rarity_offset: -500,
            potion_odds: 40,
            card_shop_removals: 0,
            relic_deques: std::array::from_fn(|_| vec![]),
            shared_relic_deques: std::array::from_fn(|_| vec![]),
            card_variants: vec![],
            lasting_candy: 0,
            paels_wing: 0,
            silver_crucible: 0,
            silken_tress: false,
            maw_bank: false,
            silver_treasures: 0,
            golden_compass: None,
            winged_boots: 0,
            rest_used: 0,
            girya: 0,
            pumpkin_candle: 0,
            paels_cards: vec![],
            pollinous_core: 0,
            fur_coat_act: None,
            fur_coat: vec![],
            toy_box_offers: vec![],
            wax_relics: vec![],
            melted_relics: vec![],
            toy_box_combats: 0,
            parasol: vec![],
            parasol_removal: false,
            conveyor: false,
            visited_events: vec![],
            crystal: None,
            reward_gold_parts: vec![],
            fake_merchant: vec![],
            fake_shop: false,
            fake_happy_flower: 0,
        }
    }
}

impl ReplayState {
    fn prepare(&mut self, content: &Content, record: &Value) {
        let state = &record["before"];
        let episode = record["episode_id"].as_str().unwrap_or_default();
        let act = state["act"].as_u64().unwrap_or_default() as u8;
        if self.episode != episode {
            *self = Self::default();
            self.episode = episode.to_owned();
            self.act = act;
            let seed = record["oracle_before"]["rng_seeds"]["run.UpFront"]
                .as_u64()
                .unwrap_or_default();
            let mut rng = Rng::from_seed(seed);
            let character = state["players"][0]["character_id"].as_str().and_then(|id| {
                content
                    .characters
                    .iter()
                    .find(|character| character.id == id)
            });
            let shared = content.acts[0].relics;
            self.shared_relic_deques = relic_deques(shared.iter().copied(), &mut rng);
            self.relic_deques = relic_deques(
                shared.iter().copied().chain(
                    character
                        .into_iter()
                        .flat_map(|x| x.relic_pool.iter().copied()),
                ),
                &mut rng,
            );
            for relic in array(&state["players"][0]["relics"]) {
                if let Some(id) = relic["model_id"]
                    .as_str()
                    .and_then(|id| content.relic_id(id))
                {
                    for deque in self
                        .relic_deques
                        .iter_mut()
                        .chain(self.shared_relic_deques.iter_mut())
                    {
                        deque.retain(|relic| *relic != id);
                    }
                }
            }
        } else if self.act != act {
            self.act = act;
            self.unknown_odds = [1000, -10_000, 200, 300];
        }
    }
}

#[cfg(feature = "python")]
pub(crate) fn game_from_live_snapshot(
    content: &Content,
    state: &Value,
    oracle: &Value,
) -> Result<Game, String> {
    let action = array(&state["players"][0]["legal_actions"])
        .first()
        .or_else(|| array(&state["visible_offers"]).first())
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "kind": match state["phase"].as_str().unwrap_or_default() {
                    "event" => "event_option",
                    "rest_site" => "rest_site",
                    "shop" => "shop_purchase",
                    "reward" => "skip_rewards",
                    _ => "map_node",
                }
            })
        });
    let record = json!({
        "episode_id": "live",
        "before": state,
        "oracle_before": oracle,
    });
    let mut replay_state = ReplayState::default();
    replay_state.prepare(content, &record);
    let mut history = TurnHistory::default();
    if state["phase"] == "combat" {
        update_history(&mut history, state, oracle);
    }
    game_from_snapshot(
        content,
        state,
        oracle,
        &action,
        state,
        oracle,
        &history,
        &replay_state,
        0,
    )
}

fn observe_happy_flower(
    state: &Value,
    charge: &mut u8,
    encounter: &mut String,
    observed_round: &mut u16,
) {
    let Some(player) = state["players"]
        .as_array()
        .and_then(|players| players.first())
    else {
        return;
    };
    let relic = array(&player["relics"])
        .iter()
        .find(|relic| relic["model_id"] == "RELIC.HAPPY_FLOWER");
    let Some(relic) = relic else {
        *charge = 0;
        encounter.clear();
        *observed_round = 0;
        return;
    };
    let round = state["combat"]["round"].as_u64().unwrap_or_default() as u16;
    let Some(room) = state["room_model_id"]
        .as_str()
        .filter(|_| state["phase"] == "combat" && round > 0)
    else {
        return;
    };
    let current = format!("{}:{room}", state["total_floor"]);
    let turns = if *encounter == current {
        round.saturating_sub(*observed_round)
    } else {
        round
    };
    *charge = (*charge + turns as u8) % 3;
    if relic["state"] == "Active" {
        *charge = 2;
    }
    *encounter = current;
    *observed_round = round;
}

pub fn replay_trace(path: impl AsRef<Path>) -> io::Result<ReplayReport> {
    let content = foundation_content();
    let mut report = ReplayReport::default();
    let mut history = TurnHistory::default();
    let mut replay_state = ReplayState::default();
    let mut pending: Option<(Value, Vec<Value>, u8)> = None;
    let mut quarantined: Option<(Value, Vec<Value>, u8)> = None;
    let mut gap_plays = 0;
    let mut gap_before = None;
    let mut gap_selected = Vec::new();
    let mut happy_flower = 0;
    let mut flower_encounter = String::new();
    let mut flower_round = 0;
    for line in BufReader::new(File::open(path)?).lines() {
        let record: Value = serde_json::from_str(&line?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let kind = record["kind"].as_str().unwrap_or_default();
        if kind == "decision" {
            observe_happy_flower(
                &record["before"],
                &mut happy_flower,
                &mut flower_encounter,
                &mut flower_round,
            );
        }
        let record_flower = happy_flower;
        if kind == "decision"
            || kind == "action_completed"
                && !array(&record["before"]["players"][0]["relics"])
                    .iter()
                    .any(|relic| relic["model_id"] == "RELIC.HAPPY_FLOWER")
                && array(&record["after"]["players"][0]["relics"])
                    .iter()
                    .any(|relic| relic["model_id"] == "RELIC.HAPPY_FLOWER")
        {
            observe_happy_flower(
                &record["after"],
                &mut happy_flower,
                &mut flower_encounter,
                &mut flower_round,
            );
        }
        if record["kind"] != "decision" {
            if record["kind"] == "transition_quarantined" {
                if record["action"]["kind"] == "choose_cards" {
                    if let Some((_, choices, _)) = &mut quarantined {
                        choices.push(record.clone());
                    }
                } else {
                    if let Some((decision, choices, flower)) = pending.take() {
                        replay_record(
                            &content,
                            &mut report,
                            &mut history,
                            &mut replay_state,
                            &decision,
                            &choices,
                            flower,
                        );
                    }
                    quarantined = Some((record.clone(), vec![], record_flower));
                }
                gap_before.get_or_insert_with(|| record["before"].clone());
                gap_selected.extend(
                    array(&record["action"]["selected_ids"])
                        .iter()
                        .filter_map(|id| id.as_str()?.split(':').nth(1).map(str::to_owned)),
                );
            } else if record["kind"] == "capture_resynchronized" {
                let mut replayed_source = None;
                if let Some((mut transition, choices, flower)) = quarantined.take() {
                    replayed_source = transition["action"]["source_id"]
                        .as_str()
                        .map(str::to_owned);
                    transition["after"] = record["before"].clone();
                    transition["oracle_after"] = record["oracle_before"].clone();
                    report.decisions += 1 + choices.len();
                    replay_record(
                        &content,
                        &mut report,
                        &mut history,
                        &mut replay_state,
                        &transition,
                        &choices,
                        flower,
                    );
                }
                let Some(before) = gap_before.take() else {
                    continue;
                };
                let state = &record["before"]["players"][0];
                let hand = array(&state["hand"]);
                let played = array(&state["discard_pile"])
                    .iter()
                    .chain(array(&state["exhaust_pile"]));
                gap_plays += array(&before["players"][0]["hand"])
                    .iter()
                    .filter(|card| {
                        card["instance_id"].as_str() != replayed_source.as_deref()
                            && !hand
                                .iter()
                                .any(|other| other["instance_id"] == card["instance_id"])
                            && played
                                .clone()
                                .any(|other| other["instance_id"] == card["instance_id"])
                    })
                    .count() as i16;
                gap_plays += gap_selected
                    .drain(..)
                    .filter(|id| !hand.iter().any(|card| card["model_id"] == *id))
                    .count() as i16;
            }
            continue;
        }
        report.decisions += 1;
        if record["action"]["kind"] == "choose_cards" {
            if let Some((_, choices, _)) = &mut pending {
                choices.push(record);
            } else {
                report.unsupported += 1;
            }
            continue;
        }
        if let Some((record, choices, flower)) = pending.take() {
            replay_record(
                &content,
                &mut report,
                &mut history,
                &mut replay_state,
                &record,
                &choices,
                flower,
            );
        }
        if gap_plays > 0 {
            history.cards += gap_plays;
            history.manual_cards += gap_plays;
            history.manual_plays += gap_plays;
            if history.withering > 0 {
                history.withering = (history.withering - gap_plays - 1).rem_euclid(6) + 1;
            }
            gap_plays = 0;
        }
        pending = Some((record, Vec::new(), record_flower));
    }
    if let Some((record, choices, flower)) = pending {
        replay_record(
            &content,
            &mut report,
            &mut history,
            &mut replay_state,
            &record,
            &choices,
            flower,
        );
    }
    Ok(report)
}

fn replay_record(
    content: &Content,
    report: &mut ReplayReport,
    history: &mut TurnHistory,
    replay_state: &mut ReplayState,
    record: &Value,
    choices: &[Value],
    happy_flower: u8,
) {
    replay_state.prepare(content, record);
    let before = &record["before"];
    let action = &record["action"];
    if before["phase"] == "combat" {
        update_history(history, before, &record["oracle_before"]);
    } else {
        *history = TurnHistory::default();
    }
    let result = replay_decision(
        content,
        before,
        &record["oracle_before"],
        action,
        choices,
        &record["after"],
        &record["oracle_after"],
        history,
        replay_state,
        happy_flower,
        record["kind"] == "transition_quarantined",
        record["metadata"]["settled_at"] == "combat_ended",
    );
    report.replayed += 1 + choices.len();
    if let Err(differences) = result {
        report.divergences.push(Divergence {
            sequence: record["sequence"].as_u64().unwrap_or_default(),
            game_action_id: record["game_action_id"].as_u64(),
            action: action["kind"].as_str().unwrap_or("unknown").to_owned(),
            differences,
        });
    }
    advance_history(content, history, before, &record["after"], action);
}

fn replay_decision(
    content: &Content,
    before: &Value,
    oracle_before: &Value,
    traced_action: &Value,
    traced_choices: &[Value],
    after: &Value,
    oracle_after: &Value,
    history: &TurnHistory,
    replay_state: &mut ReplayState,
    happy_flower: u8,
    quarantined: bool,
    combat_ended: bool,
) -> Result<(), Vec<String>> {
    let mut game = match game_from_snapshot(
        content,
        before,
        oracle_before,
        traced_action,
        after,
        oracle_after,
        history,
        replay_state,
        happy_flower,
    ) {
        Ok(game) => game,
        Err(error) => return Err(vec![error]),
    };
    let action = match action_from_snapshot(content, &game, traced_action, after) {
        Ok(action) => action,
        Err(error) => return Err(vec![error]),
    };
    if let Err(error) = game.step(content, action) {
        return Err(vec![format!("action rejected: {error:?}")]);
    }
    for choice in traced_choices
        .iter()
        .filter(|_| before["phase"] == "combat")
    {
        if quarantined {
            let mut differences = compare_snapshot(
                content,
                &game,
                &choice["before"],
                &choice["oracle_before"],
                false,
            );
            let expected: Vec<_> = array(&choice["action"]["candidate_ids"])
                .iter()
                .filter_map(|id| {
                    let mut parts = id.as_str()?.split(':').skip(1);
                    Some((parts.next()?, parts.next()?.parse::<u8>().ok()?))
                })
                .collect();
            if !expected.is_empty() {
                let actual: Vec<_> = game
                    .combat()
                    .into_iter()
                    .flat_map(|combat| &combat.offer)
                    .map(|card| (content.cards[card.id as usize].id, card.upgrades))
                    .collect();
                if actual != expected {
                    differences.push(format!(
                        "choice candidates: simulator {actual:?} != game {expected:?}"
                    ));
                }
            }
            if !differences.is_empty() {
                return Err(differences);
            }
        }
        let selected = array(&choice["action"]["selected_ids"]);
        if selected.is_empty() && choice["action"]["can_skip"] == true {
            game.step(content, Action::Done)
                .map_err(|error| vec![format!("choice rejected: {error:?}")])?;
        }
        for selected in selected {
            let selected = selected.as_str().unwrap_or_default();
            let combat = game
                .combat()
                .ok_or_else(|| vec!["choice ended combat".into()])?;
            let choice = combat
                .choice
                .ok_or_else(|| vec!["simulator did not request card choice".into()])?;
            let index = replay_pile(combat, choice.pile)
                .iter()
                .position(|card| {
                    if selected.starts_with("model-card:") {
                        let mut parts = selected.split(':').skip(1);
                        let model = parts.next().unwrap_or_default();
                        let upgrades = parts
                            .next()
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(0);
                        content.cards[card.id as usize].id == model && card.upgrades == upgrades
                    } else {
                        card.instance == instance_id(selected)
                    }
                })
                .ok_or_else(|| {
                    vec![format!(
                        "choice has no {selected}; offered {:?}",
                        replay_pile(combat, choice.pile)
                            .iter()
                            .map(|card| content.cards[card.id as usize].id)
                            .collect::<Vec<_>>()
                    )]
                })?;
            game.step(content, Action::Choose(index))
                .map_err(|error| vec![format!("choice rejected: {error:?}")])?;
        }
    }
    'choices: for choice in traced_choices
        .iter()
        .filter(|_| before["phase"] != "combat")
    {
        if let Phase::ChooseBundles(bundles) = &game.phase {
            let selected: Vec<_> = array(&choice["action"]["selected_ids"])
                .iter()
                .filter_map(|selected| {
                    let mut parts = selected.as_str()?.split(':').skip(1);
                    Some((parts.next()?, parts.next()?.parse::<u8>().ok()?))
                })
                .collect();
            if let Some(index) = bundles.iter().position(|bundle| {
                let mut actual: Vec<_> = bundle
                    .iter()
                    .map(|card| (content.cards[card.id as usize].id, card.upgrades))
                    .collect();
                selected.iter().all(|card| {
                    actual
                        .iter()
                        .position(|candidate| candidate == card)
                        .is_some_and(|index| {
                            actual.remove(index);
                            true
                        })
                })
            }) {
                game.step(content, Action::Choose(index))
                    .map_err(|error| vec![format!("choice rejected: {error:?}")])?;
                continue 'choices;
            }
        }
        for selected in array(&choice["action"]["selected_ids"]) {
            let selected = selected.as_str().unwrap_or_default();
            if let Phase::ChooseCards(cards, _, _) = &game.phase
                && selected.starts_with("model-card:")
            {
                let mut parts = selected.split(':').skip(1);
                let model = parts.next().unwrap_or_default();
                let upgrades = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                let index = cards
                    .iter()
                    .position(|card| {
                        content.cards[card.id as usize].id == model && card.upgrades == upgrades
                    })
                    .ok_or_else(|| {
                        vec![format!(
                            "card choice has no {selected}; offered {:?}",
                            cards
                                .iter()
                                .map(|card| content.cards[card.id as usize].id)
                                .collect::<Vec<_>>()
                        )]
                    })?;
                game.step(content, Action::Choose(index))
                    .map_err(|error| vec![format!("choice rejected: {error:?}")])?;
                continue;
            }
            if !matches!(
                game.phase,
                Phase::RemoveCards(..)
                    | Phase::UpgradeCards(..)
                    | Phase::TransformCards(..)
                    | Phase::EnchantCards(..)
            ) {
                let cards: Vec<_> = array(&choice["action"]["candidate_ids"])
                    .iter()
                    .filter_map(|id| {
                        let mut parts = id.as_str()?.split(':').skip(1);
                        Some(Card {
                            id: content.card_id(parts.next()?)?,
                            upgrades: parts.next()?.parse().ok()?,
                            ..Card::default()
                        })
                    })
                    .collect();
                if !cards.is_empty() {
                    game.phase = Phase::ChooseCards(
                        cards,
                        choice["action"]["maximum"].as_u64().unwrap_or(1) as u8,
                        choice["action"]["can_skip"].as_bool().unwrap_or(false),
                    );
                    let mut parts = selected.split(':').skip(1);
                    let model = parts.next().unwrap_or_default();
                    let upgrades = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                    let Phase::ChooseCards(cards, _, _) = &game.phase else {
                        unreachable!()
                    };
                    let index = cards
                        .iter()
                        .position(|card| {
                            content.cards[card.id as usize].id == model && card.upgrades == upgrades
                        })
                        .ok_or_else(|| vec![format!("card choice has no {selected}")])?;
                    game.step(content, Action::Choose(index))
                        .map_err(|error| vec![format!("choice rejected: {error:?}")])?;
                    continue;
                }
            }
            let index = game
                .run
                .deck
                .iter()
                .position(|card| card.instance == instance_id(selected))
                .ok_or_else(|| vec![format!("deck has no {selected}")])?;
            let action = match game.phase {
                Phase::RemoveCards(..) => Action::RemoveCard(index),
                Phase::UpgradeCards(..) => Action::Smith(index),
                Phase::TransformCards(..) => Action::RemoveCard(index),
                Phase::EnchantCards(..) => Action::Enchant(index),
                _ => continue,
            };
            game.step(content, action)
                .map_err(|error| vec![format!("choice rejected: {error:?}")])?;
        }
    }
    if quarantined && before["phase"] != "combat" {
        let mut existing: Vec<_> = array(&before["players"][0]["deck"])
            .iter()
            .filter_map(|card| {
                Some((
                    card["model_id"].as_str()?.to_owned(),
                    card["upgrade"].as_u64().unwrap_or_default() as u8,
                ))
            })
            .collect();
        for card in array(&after["players"][0]["deck"]) {
            let id = card["model_id"].as_str().unwrap_or_default();
            let upgrades = card["upgrade"].as_u64().unwrap_or_default() as u8;
            if let Some(index) = existing
                .iter()
                .position(|current| current == &(id.to_owned(), upgrades))
            {
                existing.remove(index);
                continue;
            }
            let Phase::ChooseCards(cards, _, _) = &game.phase else {
                continue;
            };
            let Some(index) = cards
                .iter()
                .position(|card| content.cards[card.id as usize].id == id)
            else {
                continue;
            };
            game.step(content, Action::Choose(index))
                .map_err(|error| vec![format!("inferred choice rejected: {error:?}")])?;
        }
    }
    replay_state.unknown_odds = game.unknown_odds;
    replay_state.rarity_offset = game.rarity_offset;
    replay_state.potion_odds = game.potion_odds;
    replay_state.card_shop_removals = game.run.card_shop_removals;
    replay_state.relic_deques = game.relic_deques.clone();
    replay_state.shared_relic_deques = game.shared_relic_deques.clone();
    replay_state.card_variants = game
        .run
        .deck
        .iter()
        .filter(|card| content.cards[card.id as usize].id == "CARD.MAD_SCIENCE")
        .map(|card| card.variant)
        .collect();
    replay_state.lasting_candy = game.lasting_candy;
    replay_state.paels_wing = game.paels_wing;
    replay_state.silver_crucible = game.silver_crucible;
    replay_state.silken_tress = game.silken_tress;
    replay_state.maw_bank = game.maw_bank;
    replay_state.silver_treasures = game.silver_treasures;
    replay_state.golden_compass = game.golden_compass;
    replay_state.winged_boots = game.winged_boots;
    replay_state.rest_used = game.rest_used;
    replay_state.girya = game.girya;
    replay_state.pumpkin_candle = game.pumpkin_candle;
    replay_state.paels_cards = game.paels_cards.clone();
    replay_state.pollinous_core = game.pollinous_core;
    replay_state.fur_coat_act = game.fur_coat_act;
    replay_state.fur_coat = game.fur_coat.clone();
    replay_state.toy_box_offers = game.toy_box_offers.clone();
    replay_state.wax_relics = game.wax_relics.clone();
    replay_state.melted_relics = game.melted_relics.clone();
    replay_state.toy_box_combats = game.toy_box_combats;
    replay_state.parasol = game.parasol.clone();
    replay_state.parasol_removal = game.parasol_removal;
    replay_state.conveyor = game.conveyor;
    replay_state.visited_events = game.visited_events.clone();
    replay_state.crystal = game.crystal.clone();
    replay_state.reward_gold_parts = game.reward_gold_parts.clone();
    replay_state.fake_merchant = game.fake_merchant.clone();
    replay_state.fake_shop = game.fake_shop;
    replay_state.fake_happy_flower = game.fake_happy_flower;
    if quarantined && before["phase"] == "combat" {
        return Ok(());
    }
    if game.combat().is_some_and(|combat| combat.choice.is_some()) {
        return Err(vec!["simulator requested an untraced card choice".into()]);
    }
    let differences = compare_snapshot(content, &game, after, oracle_after, combat_ended);
    if differences.is_empty() {
        Ok(())
    } else {
        Err(differences)
    }
}

fn replay_pile(combat: &Combat, pile: Pile) -> &[Card] {
    match pile {
        Pile::Draw => &combat.draw,
        Pile::Hand => &combat.hand,
        Pile::Discard => &combat.discard,
        Pile::Exhaust => &combat.exhaust,
        Pile::Offer => &combat.offer,
    }
}

fn game_from_snapshot(
    content: &Content,
    state: &Value,
    oracle: &Value,
    action: &Value,
    after: &Value,
    oracle_after: &Value,
    history: &TurnHistory,
    replay_state: &ReplayState,
    happy_flower: u8,
) -> Result<Game, String> {
    let player = &state["players"][0];
    let character = find_id(&content.characters, text(player, "character_id")?, |x| x.id)?;
    let mut deck = cards(content, &player["deck"])?;
    for (card, &variant) in deck
        .iter_mut()
        .filter(|card| content.cards[card.id as usize].id == "CARD.MAD_SCIENCE")
        .zip(&replay_state.card_variants)
    {
        card.variant = variant;
    }
    for (instance, index) in oracle["card_deck_version_indices"]
        .as_object()
        .into_iter()
        .flatten()
    {
        if let Some(card) = deck.get_mut(index.as_u64().unwrap_or(usize::MAX as u64) as usize) {
            card.instance = instance_id(instance);
        }
    }
    let relics = ids(&content.relics, &player["relics"], |x| x.id)?;
    let ascension = state["ascension"].as_u64().unwrap_or_default() as u8;
    let mut potions = vec![
        None;
        (if ascension >= 4 { 2 } else { 3 })
            + 2 * relics
                .iter()
                .filter(|&&id| content.relics[id as usize].id == "RELIC.POTION_BELT")
                .count()
    ];
    for (fallback, potion) in array(&player["potions"]).iter().enumerate() {
        let slot = potion["amount"].as_u64().unwrap_or(fallback as u64) as usize;
        if slot >= potions.len() {
            potions.resize(slot + 1, None);
        }
        potions[slot] = Some(find_id(&content.potions, text(potion, "model_id")?, |x| {
            x.id
        })?);
    }
    let run = Run {
        ascension,
        character,
        hp: number(player, "hit_points")? as i16,
        max_hp: number(player, "max_hit_points")? as i16,
        gold: number(player, "gold")? as i32,
        deck,
        relics,
        potions,
        act: number(state, "act")? as u8,
        floor: number(state, "act_floor")? as u8,
        energy: (number(player, "max_energy")? as u8)
            .max(content.characters[character as usize].energy),
        draw: content.characters[character as usize].draw,
        orb_slots: content.characters[character as usize].orb_slots,
        card_shop_removals: replay_state.card_shop_removals,
    };
    if state["phase"] != "combat" {
        return run_game_from_snapshot(
            content,
            state,
            oracle,
            action,
            after,
            oracle_after,
            run,
            replay_state,
            happy_flower,
        );
    }
    let combat_state = &state["combat"];
    let mut player_creature = Creature {
        id: 0,
        hp: run.hp,
        max_hp: run.max_hp,
        block: number(player, "block")? as i16,
        powers: powers(content, &player["powers"])?,
    };
    let mut enemies = Vec::new();
    for enemy in array(&combat_state["enemies"]) {
        let id = find_id(&content.enemies, text(enemy, "model_id")?, |x| x.id)?;
        let move_id = oracle["enemy_move_ids"][text(enemy, "instance_id")?]
            .as_str()
            .unwrap_or_default();
        let move_index = infer_move(&content.enemies[id as usize], enemy, move_id);
        let instance = instance_id(text(enemy, "instance_id")?);
        let move_history = history
            .enemy_moves
            .get(&instance)
            .into_iter()
            .flatten()
            .map(|move_id| infer_move(&content.enemies[id as usize], enemy, move_id))
            .collect();
        enemies.push(Enemy {
            instance,
            creature: Creature {
                id,
                hp: number(enemy, "hit_points")? as i16,
                max_hp: number(enemy, "max_hit_points")? as i16,
                block: number(enemy, "block")? as i16,
                powers: powers(content, &enemy["powers"])?,
            },
            move_index,
            last_move: move_index,
            repeats: 1,
            move_history,
            stunned: move_id == "STUNNED"
                && !content.enemies[id as usize].moves[move_index]
                    .intent
                    .contains("Stun"),
            value: 0,
        });
        let creature = &mut enemies.last_mut().unwrap().creature;
        let strength = creature.power(power_id::STRENGTH).max(0);
        let dexterity = creature.power(power_id::DEXTERITY).max(0);
        for power in &mut creature.powers {
            if power.id == power_id::POSSESS_STRENGTH {
                power.value = strength;
            } else if power.id == power_id::POSSESS_SPEED {
                power.value = dexterity;
            } else if power.id == power_id::WITHERING_PRESENCE {
                power.value = history.withering;
            }
        }
    }
    let variants: HashMap<_, _> = run
        .deck
        .iter()
        .filter(|card| card.variant != 0)
        .map(|card| (card.instance, card.variant))
        .collect();
    let set_values = |mut cards: Vec<Card>| {
        for card in &mut cards {
            if let Some(&variant) = variants.get(&card.instance) {
                card.variant = variant;
            }
            card.value = history.values.get(&card.instance).copied().unwrap_or(
                if card.id == card_id::WITHER {
                    history.wither_upgrades
                } else {
                    card.value
                },
            );
        }
        cards
    };
    let draw = set_values(
        cards(content, &oracle["hidden_draw_order"])?
            .into_iter()
            .rev()
            .collect(),
    );
    let hand = set_values(cards(content, &player["hand"])?);
    let discard = set_values(cards(content, &player["discard_pile"])?);
    let exhaust = set_values(cards(content, &player["exhaust_pile"])?);
    let dampened = draw
        .iter()
        .chain(&hand)
        .chain(&discard)
        .chain(&exhaust)
        .filter_map(|card| {
            history
                .upgrades
                .get(&card.instance)
                .filter(|&&upgrades| upgrades > card.upgrades)
                .map(|&upgrades| (card.instance, upgrades))
        })
        .collect();
    let toric_count = player_creature
        .powers
        .iter()
        .filter(|power| power.id == power_id::TORIC_TOUGHNESS)
        .count();
    let fallback: Vec<_> = discard
        .iter()
        .chain(&exhaust)
        .chain(&draw)
        .chain(&hand)
        .filter(|card| card.id == card_id::TORIC_TOUGHNESS)
        .enumerate()
        .map(|(index, card)| {
            let mut block = (if card.upgrades > 0 { 7 } else { 5 })
                + player_creature.kind(content, PowerKind::Dexterity);
            for _ in 0..player_creature.power(power_id::SHADOWMELD).max(0) {
                block *= 2;
            }
            if index < player_creature.power(power_id::UNMOVABLE).max(0) as usize {
                block *= 2;
            }
            if player_creature.kind(content, PowerKind::Frail) > 0 {
                block = block * 3 / 4;
            }
            block.max(0)
        })
        .collect();
    let mut toric_blocks = if history.toric.len() == toric_count {
        history.toric.iter().map(|x| x.1).collect::<Vec<_>>()
    } else {
        fallback
    }
    .into_iter();
    for power in &mut player_creature.powers {
        if power.id == power_id::TORIC_TOUGHNESS {
            power.value = toric_blocks.next().unwrap_or(5);
        } else if power.id == power_id::CONSTRICT {
            power.value = enemies
                .iter()
                .position(|enemy| {
                    content.enemies[enemy.creature.id as usize].id == "MONSTER.SLITHERING_STRANGLER"
                })
                .map_or(0, |index| index as i16 + 1);
        } else if power.id == power_id::CRIMSON_MANTLE {
            power.value = array(&oracle["power_states"]["creature:0"])
                .iter()
                .find(|state| state["model_id"] == "POWER.CRIMSON_MANTLE_POWER")
                .and_then(|state| state["self_damage"].as_i64())
                .unwrap_or_default() as i16;
        }
    }
    let play_pile = array(&player["play_pile"]);
    let combat = Combat {
        player: player_creature,
        osty: Creature {
            id: 0,
            hp: 0,
            max_hp: 0,
            block: 0,
            powers: vec![],
        },
        enemies,
        draw,
        known_draw_top: 0,
        known_draw_bottom: 0,
        hand,
        discard,
        exhaust,
        offer: vec![],
        energy: number(player, "energy")? as i16,
        max_energy: number(player, "max_energy")? as i16,
        draw_per_turn: run.draw,
        stars: number(player, "stars")? as i16,
        turn: number(combat_state, "round")? as u16,
        orbs: vec![],
        orb_slots: run.orb_slots,
        history: History {
            cards: history.cards.max(play_pile.len() as i16),
            manual_cards: history.manual_cards.max(play_pile.len() as i16),
            manual_plays: history.manual_plays.max(play_pile.len() as i16),
            attacks: history.attacks,
            skills: history.skills,
            powers: history.powers,
            block_gains: history.block_gains,
            energy: number(player, "energy_spent_this_turn")? as i16,
            exhausted: number(player, "cards_exhausted_this_turn")? as i16,
            hp_lost: player["player_lost_hp_this_turn"]
                .as_bool()
                .unwrap_or(false) as i16,
            hp_loss_events: number(player, "player_hp_loss_events_this_combat")? as i16,
            osty_attacks: number(player, "osty_attacks_this_turn")? as i16,
            ..History::default()
        },
        last_cards: history.last_cards,
        orbit_spent: 0,
        hits: vec![0; array(&combat_state["enemies"]).len()],
        last_damage: 0,
        drawn: 0,
        lightning_channeled: 0,
        orbs_channeled: 0,
        poisoned: 0,
        nightmares: vec![],
        bombs: vec![],
        automation: vec![],
        panache: vec![],
        boulders: vec![],
        dampened,
        paels_tears: false,
        ending: false,
        playing: None,
        auto_plays: vec![],
        queue: vec![],
        choice: None,
        power_snapshot: vec![],
        enemy_power_snapshot: vec![],
        card_energy: 0,
        card_stars: 0,
        card_plays: 1,
        force_end: false,
        enemy_turn: false,
        centennial_puzzle: false,
        demon_tongue: false,
        permafrost: false,
        pen_nib: false,
        ruined_helmet: false,
        music_box: false,
        mini_regent: false,
        rainbow_ring: false,
        kusarigama: (history.attacks % 3).max(0) as u8,
        unsettling_lamp: None,
        unsettling_used: false,
        diamond_diadem: false,
        belt_buckle: false,
        burning_sticks: false,
        red_skull: false,
        throwing_axe: false,
        paels_eye: false,
        paels_eye_extra: false,
        paels_legion: relic_amount(player, "RELIC.PAELS_LEGION"),
        history_course: history.last_card,
    };
    let rng = |key: String| {
        let mut rng = Rng::from_seed(oracle["rng_seeds"][&key].as_u64().unwrap_or_default());
        rng.forward(oracle["rng_counters"][&key].as_u64().unwrap_or_default());
        rng
    };
    let seed = (oracle["rng_seeds"]["run.UpFront"]
        .as_u64()
        .unwrap_or_default() as u32)
        .wrapping_sub(hash("up_front"));
    let event_combat = trace_event_combat(state);
    let event_rng = match event_combat {
        1..=3 => Some(Rng::from_seed(
            seed.wrapping_add(hash("BATTLEWORN_DUMMY")) as u64
        )),
        4 => {
            let mut rng = Rng::from_seed(seed.wrapping_add(hash("PUNCH_OFF")) as u64);
            rng.below(9);
            Some(rng)
        }
        6 => {
            let mut rng = Rng::from_seed(seed.wrapping_add(hash("DENSE_VEGETATION")) as u64);
            rng.below(40);
            Some(rng)
        }
        _ => None,
    };
    Ok(Game {
        run,
        map: Map::default(),
        phase: Phase::Combat(Box::new(combat)),
        act: number(state, "act")? as Id,
        room: match state["room_type"].as_str().unwrap_or("Monster") {
            "Elite" => Room::Elite,
            "Boss" => Room::Boss,
            _ => Room::Combat,
        },
        fishing_rod: 0,
        run_queue: vec![],
        resume: None,
        rngs: Rngs {
            combat_card_generation: rng("run.CombatCardGeneration".into()),
            combat_card_selection: rng("run.CombatCardSelection".into()),
            combat_energy_costs: rng("run.CombatEnergyCosts".into()),
            combat_orb_generation: rng("run.CombatOrbGeneration".into()),
            combat_potion_generation: rng("run.CombatPotionGeneration".into()),
            combat_targets: rng("run.CombatTargets".into()),
            monster_ai: rng("run.MonsterAi".into()),
            niche: rng("run.Niche".into()),
            shuffle: rng("run.Shuffle".into()),
            treasure_room_relics: rng("run.TreasureRoomRelics".into()),
            unknown_map_point: rng("run.UnknownMapPoint".into()),
            up_front: rng("run.UpFront".into()),
            rewards: rng(format!("player.{}.Rewards", player["net_id"])),
            shops: rng(format!("player.{}.Shops", player["net_id"])),
            transformations: rng(format!("player.{}.Transformations", player["net_id"])),
        },
        next_card: oracle["next_combat_card_id"].as_u64().unwrap_or_default() as u32 + 1,
        rarity_offset: replay_state.rarity_offset,
        potion_odds: replay_state.potion_odds,
        bosses: [None; 2],
        bosses_visited: 0,
        encounters: vec![],
        elites: vec![],
        weak_encounters_left: 0,
        regular_encounters_left: 0,
        elite_encounters_left: 0,
        last_encounter: None,
        last_elite: None,
        events: vec![],
        visited_events: replay_state.visited_events.clone(),
        enemy_starts: vec![],
        seed,
        unknown_odds: replay_state.unknown_odds,
        happy_flower,
        tea_set: array(&player["relics"])
            .iter()
            .find(|relic| {
                relic["state"] == "Active"
                    && matches!(
                        relic["model_id"].as_str(),
                        Some("RELIC.VENERABLE_TEA_SET" | "RELIC.FAKE_VENERABLE_TEA_SET")
                    )
            })
            .map_or(0, |relic| {
                if relic["model_id"] == "RELIC.VENERABLE_TEA_SET" {
                    2
                } else {
                    1
                }
            }),
        replacing_potion: false,
        pending_potion: None,
        removal_price: 0,
        event_rng,
        event_relic: None,
        event_data: [0; 4],
        event_cards: vec![],
        relic_queue: vec![],
        relic_deques: replay_state.relic_deques.clone(),
        shared_relic_deques: replay_state.shared_relic_deques.clone(),
        pending_curse: false,
        wongo_combats: wongo_combats(player),
        spoils: None,
        event_combat,
        damage_taken: false,
        lasting_candy: replay_state.lasting_candy,
        paels_wing: replay_state.paels_wing,
        silver_crucible: replay_state.silver_crucible,
        silken_tress: replay_state.silken_tress,
        rerolled_cards: false,
        maw_bank: replay_state.maw_bank,
        silver_treasures: replay_state.silver_treasures,
        golden_compass: replay_state.golden_compass,
        winged_boots: replay_state.winged_boots,
        rest_used: replay_state.rest_used,
        girya: replay_state.girya,
        pumpkin_candle: replay_state.pumpkin_candle,
        cooking: false,
        astrolabe: false,
        transform_niche: false,
        paels_tooth: false,
        paels_cards: replay_state.paels_cards.clone(),
        pollinous_core: replay_state.pollinous_core,
        fur_coat_act: replay_state.fur_coat_act,
        fur_coat: replay_state.fur_coat.clone(),
        toy_box_offers: replay_state.toy_box_offers.clone(),
        wax_relics: replay_state.wax_relics.clone(),
        melted_relics: replay_state.melted_relics.clone(),
        toy_box_combats: replay_state.toy_box_combats,
        parasol: replay_state.parasol.clone(),
        parasol_removal: replay_state.parasol_removal,
        conveyor: replay_state.conveyor,
        crystal: replay_state.crystal.clone(),
        reward_gold_parts: replay_state.reward_gold_parts.clone(),
        fake_merchant: replay_state.fake_merchant.clone(),
        fake_shop: replay_state.fake_shop,
        fake_happy_flower: replay_state.fake_happy_flower,
        lizard_tail: relic_disabled(player, "RELIC.LIZARD_TAIL"),
        nunchaku: relic_amount(player, "RELIC.NUNCHAKU") % 10,
        pendulum: relic_amount(player, "RELIC.PENDULUM") % 3,
        pen_nib: relic_amount(player, "RELIC.PEN_NIB") % 10,
        iron_club: relic_amount(player, "RELIC.IRON_CLUB") % 4,
        joss_paper: relic_amount(player, "RELIC.JOSS_PAPER") % 5,
        tuning_fork: relic_amount(player, "RELIC.TUNING_FORK") % 10,
        bone_tea: relic_disabled(player, "RELIC.BONE_TEA"),
        tea_of_discourtesy: relic_disabled(player, "RELIC.TEA_OF_DISCOURTESY"),
        galactic_dust: relic_amount(player, "RELIC.GALACTIC_DUST") % 10,
        book_of_five_rings: relic_amount(player, "RELIC.BOOK_OF_FIVE_RINGS") % 5,
        ember_tea: relic_amount_opt(player, "RELIC.EMBER_TEA")
            .map_or(0, |amount| 5u8.saturating_sub(amount.min(5))),
        sword_of_stone: relic_amount(player, "RELIC.SWORD_OF_STONE"),
        replaying: true,
    })
}

fn run_game_from_snapshot(
    content: &Content,
    state: &Value,
    oracle: &Value,
    action: &Value,
    after: &Value,
    oracle_after: &Value,
    run: Run,
    replay_state: &ReplayState,
    happy_flower: u8,
) -> Result<Game, String> {
    let player = &state["players"][0];
    let map = trace_map(state, action, after);
    let mut phase = match action["kind"].as_str().unwrap_or_default() {
        "take_reward" | "skip_rewards" | "pick_relic" => {
            Phase::Rewards(trace_rewards(content, state, action, after)?)
        }
        "shop_purchase" => Phase::Shop(trace_shop(content, state)?),
        "use_potion" if state["room_model_id"] == "EVENT.FAKE_MERCHANT" => {
            Phase::Shop(trace_shop(content, state)?)
        }
        "event_option" => {
            let id = trace_event(content, text(state, "room_model_id")?)?;
            Phase::Event(id, content.events[id as usize].options.to_vec())
        }
        "rest_site" => Phase::Rest,
        _ => Phase::Map,
    };
    let rng = |key: String| {
        let mut rng = Rng::from_seed(oracle["rng_seeds"][&key].as_u64().unwrap_or_default());
        rng.forward(oracle["rng_counters"][&key].as_u64().unwrap_or_default());
        rng
    };
    let seed = (oracle["rng_seeds"]["run.UpFront"]
        .as_u64()
        .unwrap_or_default() as u32)
        .wrapping_sub(hash("up_front"));
    let act = trace_act(content, after)
        .or_else(|| trace_act(content, state))
        .unwrap_or_else(|| {
            state["act"]
                .as_u64()
                .unwrap_or(1)
                .min(content.acts.len().saturating_sub(1) as u64) as Id
        });
    let room = trace_room(state["room_type"].as_str().unwrap_or("Map"));
    let next_card = run
        .deck
        .iter()
        .map(|card| card.instance)
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    let next_model = after["room_model_id"].as_str().unwrap_or_default();
    let next_room = trace_room(after["room_type"].as_str().unwrap_or("Map"));
    let next_encounter = trace_encounter(content, after);
    let next_event = trace_event(content, next_model).ok();
    let mut enemy_starts: Vec<_> = array(&after["combat"]["enemies"])
        .iter()
        .filter_map(|enemy| {
            let id = find_id(&content.enemies, enemy["model_id"].as_str()?, |enemy| {
                enemy.id
            })
            .ok()?;
            Some(infer_move(
                &content.enemies[id as usize],
                enemy,
                oracle_after["enemy_move_ids"][enemy["instance_id"].as_str()?].as_str()?,
            ))
        })
        .collect();
    if enemy_starts.len() != array(&after["combat"]["enemies"]).len() {
        enemy_starts.clear();
    }
    let relic_queue = if action["id"]
        .as_str()
        .is_some_and(|id| id.ends_with("LARGE_CAPSULE"))
    {
        ids(&content.relics, &after["players"][0]["relics"], |relic| {
            relic.id
        })?
        .into_iter()
        .skip(run.relics.len() + 1)
        .collect()
    } else {
        vec![]
    };
    let pending_curse = state["room_model_id"] == "EVENT.NEOW"
        && array(&state["visible_offers"])
            .iter()
            .rev()
            .find(|offer| {
                offer["runtime_type"]
                    .as_str()
                    .is_some_and(|runtime| runtime.ends_with("RelicReward"))
            })
            .is_some_and(|offer| offer["id"] == action["id"]);
    let spoils = (run.act == 2
        && run
            .deck
            .iter()
            .any(|card| content.cards[card.id as usize].id == "CARD.SPOILS_MAP"))
    .then(|| {
        map.nodes
            .iter()
            .find(|node| node.room == Room::Treasure)
            .map(|node| (node.lane, node.floor))
    })
    .flatten();
    let mut event_data = [0; 4];
    if state["room_model_id"] == "EVENT.SLIPPERY_BRIDGE" {
        let mut removed = run.deck.clone();
        for value in array(&after["players"][0]["deck"]) {
            if let Some(index) = removed.iter().position(|card| {
                content.cards[card.id as usize].id == value["model_id"]
                    && card.upgrades as u64 == value["upgrade"].as_u64().unwrap_or_default()
            }) {
                removed.remove(index);
            }
        }
        if let Some(card) = removed.first() {
            event_data[1] = card.instance as i64;
        }
    }
    let mut event_rng = state["room_model_id"]
        .as_str()
        .and_then(|id| id.strip_prefix("EVENT."))
        .map(|id| {
            let mut rng = Rng::from_seed(seed.wrapping_add(hash(id)) as u64);
            if id == "WHISPERING_HOLLOW" {
                rng.below(19);
            }
            rng
        });
    match state["room_model_id"].as_str().unwrap_or_default() {
        "EVENT.SLIPPERY_BRIDGE" => {
            if !run.deck.is_empty() {
                event_rng.as_mut().unwrap().below(run.deck.len() as u32);
            }
        }
        "EVENT.PUNCH_OFF" => {
            event_data[0] = event_rng.as_mut().unwrap().below(8) as i64 + 91;
            if action["id"]
                .as_str()
                .is_some_and(|id| id.contains("I_CAN_TAKE_THEM.options.FIGHT"))
            {
                event_data[3] = 1;
                phase = event_page(trace_event(content, "EVENT.PUNCH_OFF")?, &[0]);
            }
        }
        "EVENT.DENSE_VEGETATION" => {
            event_data[0] = event_rng.as_mut().unwrap().below(39) as i64 + 61;
            if action["id"]
                .as_str()
                .is_some_and(|id| id.contains("REST.options.FIGHT"))
            {
                event_data[3] = 1;
                phase = event_page(trace_event(content, "EVENT.DENSE_VEGETATION")?, &[0]);
            }
        }
        "EVENT.THE_LANTERN_KEY"
            if action["id"]
                .as_str()
                .is_some_and(|id| id.contains("KEEP_THE_KEY.options.FIGHT")) =>
        {
            event_data[3] = 1;
            phase = event_page(trace_event(content, "EVENT.THE_LANTERN_KEY")?, &[0]);
        }
        "EVENT.TINKER_TIME" => {
            let option = action["id"].as_str().unwrap_or_default();
            if action["kind"] == "event_option" && !option.contains("INITIAL") {
                let mut types = [1, 2, 3];
                event_rng.as_mut().unwrap().shuffle(&mut types);
                event_data[..2].copy_from_slice(&types[..2]);
                event_data[3] = -1;
                phase = event_page(trace_event(content, "EVENT.TINKER_TIME")?, &[0, 1]);
            }
            if option.contains("CHOOSE_RIDER") {
                let card_type = if option.contains("SAPPING")
                    || option.contains("VIOLENCE")
                    || option.contains("CHOKING")
                {
                    1
                } else if option.contains("ENERGIZED")
                    || option.contains("WISDOM")
                    || option.contains("CHAOS")
                {
                    2
                } else {
                    3
                };
                let mut riders = match card_type {
                    1 => [1, 2, 3],
                    2 => [4, 5, 6],
                    _ => [7, 8, 9],
                };
                event_rng.as_mut().unwrap().shuffle(&mut riders);
                event_data[..2].copy_from_slice(&riders[..2]);
                event_data[3] = card_type;
            }
        }
        _ => {}
    }
    let fake_merchant = if state["room_model_id"] == "EVENT.FAKE_MERCHANT" {
        match &phase {
            Phase::Shop(items) => items
                .iter()
                .filter_map(|item| match item {
                    ShopItem::Relic(id, _) => Some(*id),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    } else {
        replay_state.fake_merchant.clone()
    };
    Ok(Game {
        run,
        map,
        phase,
        act,
        room,
        fishing_rod: 0,
        run_queue: vec![],
        resume: None,
        rngs: Rngs {
            combat_card_generation: rng("run.CombatCardGeneration".into()),
            combat_card_selection: rng("run.CombatCardSelection".into()),
            combat_energy_costs: rng("run.CombatEnergyCosts".into()),
            combat_orb_generation: rng("run.CombatOrbGeneration".into()),
            combat_potion_generation: rng("run.CombatPotionGeneration".into()),
            combat_targets: rng("run.CombatTargets".into()),
            monster_ai: rng("run.MonsterAi".into()),
            niche: rng("run.Niche".into()),
            shuffle: rng("run.Shuffle".into()),
            treasure_room_relics: rng("run.TreasureRoomRelics".into()),
            unknown_map_point: rng("run.UnknownMapPoint".into()),
            up_front: rng("run.UpFront".into()),
            rewards: rng(format!("player.{}.Rewards", player["net_id"])),
            shops: rng(format!("player.{}.Shops", player["net_id"])),
            transformations: rng(format!("player.{}.Transformations", player["net_id"])),
        },
        next_card,
        rarity_offset: replay_state.rarity_offset,
        potion_odds: replay_state.potion_odds,
        bosses: if next_room == Room::Boss {
            [next_encounter, None]
        } else {
            [None; 2]
        },
        bosses_visited: 0,
        encounters: (next_room == Room::Combat)
            .then_some(next_encounter)
            .flatten()
            .into_iter()
            .collect(),
        elites: (next_room == Room::Elite)
            .then_some(next_encounter)
            .flatten()
            .into_iter()
            .collect(),
        weak_encounters_left: 0,
        regular_encounters_left: 0,
        elite_encounters_left: 0,
        last_encounter: None,
        last_elite: None,
        events: (next_room == Room::Event)
            .then_some(next_event)
            .flatten()
            .into_iter()
            .collect(),
        visited_events: replay_state.visited_events.clone(),
        enemy_starts,
        seed,
        unknown_odds: replay_state.unknown_odds,
        happy_flower,
        tea_set: array(&player["relics"])
            .iter()
            .find(|relic| {
                relic["state"] == "Active"
                    && matches!(
                        relic["model_id"].as_str(),
                        Some("RELIC.VENERABLE_TEA_SET" | "RELIC.FAKE_VENERABLE_TEA_SET")
                    )
            })
            .map_or(0, |relic| {
                if relic["model_id"] == "RELIC.VENERABLE_TEA_SET" {
                    2
                } else {
                    1
                }
            }),
        replacing_potion: action["kind"] == "discard_potion",
        pending_potion: None,
        removal_price: 0,
        event_rng,
        event_relic: None,
        event_data,
        event_cards: vec![],
        relic_queue,
        relic_deques: replay_state.relic_deques.clone(),
        shared_relic_deques: replay_state.shared_relic_deques.clone(),
        pending_curse,
        wongo_combats: wongo_combats(player),
        spoils,
        event_combat: 0,
        damage_taken: false,
        lasting_candy: replay_state.lasting_candy,
        paels_wing: replay_state.paels_wing,
        silver_crucible: replay_state.silver_crucible,
        silken_tress: replay_state.silken_tress,
        rerolled_cards: false,
        maw_bank: replay_state.maw_bank,
        silver_treasures: replay_state.silver_treasures,
        golden_compass: replay_state.golden_compass,
        winged_boots: replay_state.winged_boots,
        rest_used: replay_state.rest_used,
        girya: replay_state.girya,
        pumpkin_candle: replay_state.pumpkin_candle,
        cooking: false,
        astrolabe: false,
        transform_niche: false,
        paels_tooth: false,
        paels_cards: replay_state.paels_cards.clone(),
        pollinous_core: replay_state.pollinous_core,
        fur_coat_act: replay_state.fur_coat_act,
        fur_coat: replay_state.fur_coat.clone(),
        toy_box_offers: replay_state.toy_box_offers.clone(),
        wax_relics: replay_state.wax_relics.clone(),
        melted_relics: replay_state.melted_relics.clone(),
        toy_box_combats: replay_state.toy_box_combats,
        parasol: replay_state.parasol.clone(),
        parasol_removal: replay_state.parasol_removal,
        conveyor: replay_state.conveyor,
        crystal: replay_state.crystal.clone(),
        reward_gold_parts: replay_state.reward_gold_parts.clone(),
        fake_merchant,
        fake_shop: state["room_model_id"] == "EVENT.FAKE_MERCHANT",
        fake_happy_flower: replay_state.fake_happy_flower,
        lizard_tail: relic_disabled(player, "RELIC.LIZARD_TAIL"),
        nunchaku: relic_amount(player, "RELIC.NUNCHAKU") % 10,
        pendulum: relic_amount(player, "RELIC.PENDULUM") % 3,
        pen_nib: relic_amount(player, "RELIC.PEN_NIB") % 10,
        iron_club: relic_amount(player, "RELIC.IRON_CLUB") % 4,
        joss_paper: relic_amount(player, "RELIC.JOSS_PAPER") % 5,
        tuning_fork: relic_amount(player, "RELIC.TUNING_FORK") % 10,
        bone_tea: relic_disabled(player, "RELIC.BONE_TEA"),
        tea_of_discourtesy: relic_disabled(player, "RELIC.TEA_OF_DISCOURTESY"),
        galactic_dust: relic_amount(player, "RELIC.GALACTIC_DUST") % 10,
        book_of_five_rings: relic_amount(player, "RELIC.BOOK_OF_FIVE_RINGS") % 5,
        ember_tea: relic_amount_opt(player, "RELIC.EMBER_TEA")
            .map_or(0, |amount| 5u8.saturating_sub(amount.min(5))),
        sword_of_stone: relic_amount(player, "RELIC.SWORD_OF_STONE"),
        replaying: true,
    })
}

fn trace_map(state: &Value, action: &Value, after: &Value) -> Map {
    let mut coords = HashMap::new();
    let mut nodes = Vec::new();
    for point in array(&state["map"]) {
        let coord = (
            point["column"].as_u64().unwrap_or_default() as u8,
            point["row"].as_u64().unwrap_or_default() as u8,
        );
        coords.insert(coord, nodes.len());
        nodes.push(MapNode {
            floor: coord.1,
            lane: coord.0,
            room: trace_room(point["point_type"].as_str().unwrap_or("Monster")),
            next: vec![],
        });
    }
    let target = trace_coord(action["target_id"].as_str());
    if let Some(coord) = target.filter(|coord| !coords.contains_key(coord)) {
        coords.insert(coord, nodes.len());
        nodes.push(MapNode {
            floor: coord.1,
            lane: coord.0,
            room: trace_room(after["room_type"].as_str().unwrap_or("Boss")),
            next: vec![],
        });
    }
    let current_coord = state["current_map_coord"].as_object().and_then(|coord| {
        Some((
            coord["column"].as_u64()? as u8,
            coord["row"].as_u64()? as u8,
        ))
    });
    if let Some(coord) = current_coord.filter(|coord| !coords.contains_key(coord)) {
        coords.insert(coord, nodes.len());
        nodes.push(MapNode {
            floor: coord.1,
            lane: coord.0,
            room: trace_room(state["room_type"].as_str().unwrap_or("Boss")),
            next: target
                .and_then(|coord| coords.get(&coord).copied())
                .into_iter()
                .collect(),
        });
    }
    for (index, point) in array(&state["map"]).iter().enumerate() {
        nodes[index].next = array(&point["children"])
            .iter()
            .filter_map(|child| {
                coords
                    .get(&(
                        child["column"].as_u64()? as u8,
                        child["row"].as_u64()? as u8,
                    ))
                    .copied()
            })
            .collect();
    }
    let current = current_coord.and_then(|coord| coords.get(&coord).copied());
    Map { nodes, current }
}

fn trace_rewards(
    content: &Content,
    state: &Value,
    action: &Value,
    after: &Value,
) -> Result<Rewards, String> {
    let before = &state["players"][0];
    let after_player = &after["players"][0];
    let runtime = action["runtime_type"].as_str().unwrap_or_default();
    let cards = if runtime.ends_with("CardReward") {
        added_value(&before["deck"], &after_player["deck"], "instance_id")
            .map(|value| card(content, value))
            .transpose()?
            .into_iter()
            .collect()
    } else {
        vec![]
    };
    let relics = if runtime.ends_with("RelicReward") || action["kind"] == "pick_relic" {
        added_value(&before["relics"], &after_player["relics"], "model_id")
            .map(|value| {
                find_id(
                    &content.relics,
                    value["model_id"].as_str().unwrap_or_default(),
                    |x| x.id,
                )
            })
            .transpose()?
            .into_iter()
            .collect()
    } else {
        vec![]
    };
    let potions = if runtime.ends_with("PotionReward") {
        Some(
            added_value(&before["potions"], &after_player["potions"], "model_id")
                .and_then(|value| value["model_id"].as_str())
                .map(|id| find_id(&content.potions, id, |x| x.id))
                .transpose()?
                .unwrap_or_default(),
        )
        .into_iter()
        .collect()
    } else {
        vec![]
    };
    Ok(Rewards {
        gold: if runtime.ends_with("GoldReward") {
            number(after_player, "gold")? as i32 - number(before, "gold")? as i32
        } else {
            0
        },
        cards,
        card_rewards: vec![],
        relics,
        potions,
        removals: 0,
    })
}

fn added_value<'a>(before: &'a Value, after: &'a Value, key: &str) -> Option<&'a Value> {
    let mut remaining = array(before).to_vec();
    array(after).iter().find(|value| {
        if let Some(index) = remaining.iter().position(|old| old[key] == value[key]) {
            remaining.remove(index);
            false
        } else {
            true
        }
    })
}

fn trace_shop(content: &Content, state: &Value) -> Result<Vec<ShopItem>, String> {
    array(&state["visible_offers"])
        .iter()
        .filter(|offer| offer["kind"] == "shop_purchase")
        .map(|offer| {
            let price = offer["id"]
                .as_str()
                .and_then(|id| id.rsplit(':').next())
                .and_then(|price| price.parse().ok())
                .ok_or("invalid shop price")?;
            let source = text(offer, "source_id")?;
            let runtime = text(offer, "runtime_type")?;
            if runtime.ends_with("MerchantCardEntry") {
                Ok(ShopItem::Card(
                    Card {
                        id: find_id(&content.cards, source, |x| x.id)?,
                        ..Card::default()
                    },
                    price,
                ))
            } else if runtime.ends_with("MerchantRelicEntry") {
                Ok(ShopItem::Relic(
                    find_id(&content.relics, source, |x| x.id)?,
                    price,
                ))
            } else if runtime.ends_with("MerchantPotionEntry") {
                Ok(ShopItem::Potion(
                    find_id(&content.potions, source, |x| x.id)?,
                    price,
                ))
            } else {
                Ok(ShopItem::Remove(price))
            }
        })
        .collect()
}

fn trace_event(content: &Content, wanted: &str) -> Result<Id, String> {
    let suffix = wanted.strip_prefix("EVENT.").unwrap_or(wanted);
    content
        .events
        .iter()
        .position(|event| event.id == wanted || event.id.ends_with(suffix))
        .map(|index| index as Id)
        .ok_or_else(|| format!("simulator has no {wanted}"))
}

fn trace_act(content: &Content, state: &Value) -> Option<Id> {
    let model = state["room_model_id"].as_str()?;
    if let Some(encounter) = trace_encounter(content, state) {
        if let Some(index) = content.acts.iter().position(|act| {
            act.encounters.contains(&encounter)
                || act.elites.contains(&encounter)
                || act.bosses.contains(&encounter)
        }) {
            return Some(index as Id);
        }
    }
    if let Ok(event) = trace_event(content, model)
        && let Some(index) = content
            .acts
            .iter()
            .position(|act| act.events.contains(&event))
    {
        return Some(index as Id);
    }
    let id = match state["act"].as_u64()? {
        1 => "ACT.THE_OVERGROWTH",
        2 => "ACT.THE_HIVE",
        3 => "ACT.THE_GLORY",
        _ => "ACT.FOUNDATION",
    };
    content
        .acts
        .iter()
        .position(|act| act.id == id)
        .map(|index| index as Id)
}

fn trace_encounter(content: &Content, state: &Value) -> Option<Id> {
    if let Some(model) = state["room_model_id"].as_str()
        && let entry = encounter_entry(model)
        && let Some(index) = content
            .encounters
            .iter()
            .position(|encounter| encounter.id == model || encounter_entry(encounter.id) == entry)
    {
        return Some(index as Id);
    }
    let enemies: Vec<_> = array(&state["combat"]["enemies"])
        .iter()
        .filter_map(|enemy| {
            let model = enemy["model_id"].as_str()?;
            content
                .enemies
                .iter()
                .position(|candidate| candidate.id == model)
                .map(|index| index as Id)
        })
        .collect();
    (!enemies.is_empty())
        .then(|| {
            content
                .encounters
                .iter()
                .position(|encounter| encounter.enemies == enemies)
                .map(|index| index as Id)
        })
        .flatten()
}

fn trace_event_combat(state: &Value) -> u8 {
    match state["room_model_id"].as_str().unwrap_or_default() {
        "ENCOUNTER.EVENT_ENCOUNTERS_NORMAL_PUNCH_CONSTRUCTS"
        | "ENCOUNTER.PUNCH_OFF_EVENT_ENCOUNTER" => 4,
        "ENCOUNTER.EVENT_ENCOUNTERS_NORMAL_MYSTERIOUS_KNIGHT"
        | "ENCOUNTER.MYSTERIOUS_KNIGHT_EVENT_ENCOUNTER" => 5,
        "ENCOUNTER.EVENT_ENCOUNTERS_NORMAL_WRIGGLERS"
        | "ENCOUNTER.DENSE_VEGETATION_EVENT_ENCOUNTER" => 6,
        "ENCOUNTER.FAKE_MERCHANT_EVENT_ENCOUNTER" => 7,
        "ENCOUNTER.EVENT_ENCOUNTERS_NORMAL_BATTLEWORN_DUMMY"
        | "ENCOUNTER.BATTLEWORN_DUMMY_EVENT_ENCOUNTER" => array(&state["combat"]["enemies"])
            .first()
            .and_then(|enemy| enemy["model_id"].as_str())
            .map_or(0, |id| match id {
                "MONSTER.BATTLE_FRIEND_V1_0" => 1,
                "MONSTER.BATTLE_FRIEND_V2_0" => 2,
                "MONSTER.BATTLE_FRIEND_V3_0" => 3,
                _ => 0,
            }),
        _ => 0,
    }
}

fn trace_room(room: &str) -> Room {
    match room {
        "Elite" => Room::Elite,
        "Boss" => Room::Boss,
        "Unknown" => Room::Unknown,
        "Event" => Room::Event,
        "Shop" | "Merchant" => Room::Shop,
        "RestSite" => Room::Rest,
        "Treasure" => Room::Treasure,
        _ => Room::Combat,
    }
}

fn trace_coord(id: Option<&str>) -> Option<(u8, u8)> {
    let mut parts = id?.split(':').skip(1);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

fn action_from_snapshot(
    content: &Content,
    game: &Game,
    action: &Value,
    after: &Value,
) -> Result<Action, String> {
    match action["kind"].as_str().unwrap_or_default() {
        "play_card" => {
            let combat = game.combat().ok_or("trace is not in combat")?;
            let source = text(action, "source_id")?;
            let hand = combat
                .hand
                .iter()
                .position(|card| card.instance == instance_id(source))
                .ok_or_else(|| format!("hand has no {source}"))?;
            Ok(Action::Play {
                hand,
                target: target(combat, action["target_id"].as_str())?,
            })
        }
        "use_potion" => Ok(Action::Potion {
            slot: action["source_id"]
                .as_str()
                .and_then(|x| x.rsplit(':').next())
                .and_then(|x| x.parse().ok())
                .ok_or("invalid potion slot")?,
            target: game
                .combat()
                .map(|combat| target(combat, action["target_id"].as_str()))
                .transpose()?
                .flatten(),
        }),
        "discard_potion" => Ok(Action::DiscardPotion(
            action["source_id"]
                .as_str()
                .and_then(|id| id.rsplit(':').next())
                .and_then(|slot| slot.parse().ok())
                .ok_or("invalid potion slot")?,
        )),
        "end_turn" => Ok(Action::EndTurn),
        "map_node" => {
            let coord = trace_coord(action["target_id"].as_str()).ok_or("invalid map node")?;
            game.map
                .nodes
                .iter()
                .position(|node| (node.lane, node.floor) == coord)
                .map(Action::Path)
                .ok_or_else(|| format!("map has no node {coord:?}"))
        }
        "take_reward" => match action["runtime_type"].as_str().unwrap_or_default() {
            runtime if runtime.ends_with("GoldReward") => Ok(Action::RewardGold),
            runtime if runtime.ends_with("CardReward") => Ok(
                if array(&after["players"][0]["deck"]).len() > game.run.deck.len() {
                    Action::RewardCard(0)
                } else {
                    Action::Cancel
                },
            ),
            runtime if runtime.ends_with("RelicReward") => Ok(Action::RewardRelic(0)),
            runtime if runtime.ends_with("PotionReward") => Ok(Action::RewardPotion(0)),
            runtime => Err(format!("unsupported reward {runtime}")),
        },
        "pick_relic" => Ok(Action::RewardRelic(0)),
        "skip_rewards" => Ok(Action::Leave),
        "shop_purchase" => {
            let source = text(action, "source_id")?;
            let items = match &game.phase {
                Phase::Shop(items) => items,
                _ => return Err("trace is not in a shop".into()),
            };
            items
                .iter()
                .position(|item| match item {
                    ShopItem::Card(card, _) => source == content.cards[card.id as usize].id,
                    ShopItem::Relic(id, _) => source == content.relics[*id as usize].id,
                    ShopItem::Potion(id, _) => source == content.potions[*id as usize].id,
                    ShopItem::Remove(_) => source == "card-removal",
                })
                .map(Action::Buy)
                .ok_or_else(|| format!("shop has no {source}"))
        }
        "event_option" => {
            let id = text(action, "id")?;
            let index = id
                .split(':')
                .nth(2)
                .and_then(|index| index.parse().ok())
                .ok_or("invalid event option")?;
            let name = id
                .rsplit(['.', ':'])
                .next()
                .unwrap_or_default()
                .replace(' ', "_")
                .to_uppercase();
            if let Some(relic) = content
                .relics
                .iter()
                .position(|relic| relic.id == format!("RELIC.{name}"))
            {
                Ok(Action::EventRelic(index, relic as Id))
            } else if id.contains("TINKER_TIME.pages.CHOOSE_RIDER") {
                Ok(Action::Event(index))
            } else {
                Ok(Action::Event(index))
            }
        }
        "rest_site" => match action["source_id"].as_str().unwrap_or_default() {
            "HEAL" => Ok(Action::Rest),
            "HATCH" => Ok(Action::Hatch),
            "CLONE" => Ok(Action::Clone),
            "LIFT" => Ok(Action::Lift),
            "COOK" => Ok(Action::Cook),
            "KINDLE" => Ok(Action::Kindle),
            "DIG" => Ok(Action::Dig),
            "SMITH" => Ok(array(&after["players"][0]["deck"])
                .iter()
                .enumerate()
                .find(|(index, card)| {
                    card["upgrade"].as_u64().unwrap_or_default()
                        > game.run.deck.get(*index).map_or(0, |card| card.upgrades) as u64
                })
                .map(|(index, _)| Action::Smith(index))
                .unwrap_or(Action::Cancel)),
            option => Err(format!("unsupported rest option {option}")),
        },
        kind => Err(format!("unsupported action {kind}")),
    }
}

fn compare_snapshot(
    content: &Content,
    game: &Game,
    state: &Value,
    oracle: &Value,
    combat_ended: bool,
) -> Vec<String> {
    let mut diff = Vec::new();
    for (key, rng) in [
        (
            "run.CombatCardGeneration",
            &game.rngs.combat_card_generation,
        ),
        ("run.CombatCardSelection", &game.rngs.combat_card_selection),
        ("run.CombatEnergyCosts", &game.rngs.combat_energy_costs),
        ("run.CombatOrbGeneration", &game.rngs.combat_orb_generation),
        (
            "run.CombatPotionGeneration",
            &game.rngs.combat_potion_generation,
        ),
        ("run.CombatTargets", &game.rngs.combat_targets),
        ("run.MonsterAi", &game.rngs.monster_ai),
        ("run.Niche", &game.rngs.niche),
        ("run.Shuffle", &game.rngs.shuffle),
        ("run.TreasureRoomRelics", &game.rngs.treasure_room_relics),
        ("run.UnknownMapPoint", &game.rngs.unknown_map_point),
        ("run.UpFront", &game.rngs.up_front),
        ("player.1.Rewards", &game.rngs.rewards),
        ("player.1.Shops", &game.rngs.shops),
        ("player.1.Transformations", &game.rngs.transformations),
    ] {
        if combat_ended && matches!(key, "run.Shuffle" | "player.1.Rewards") {
            continue;
        }
        if let Some(expected) = oracle["rng_counters"][key].as_u64()
            && rng.1 != expected
        {
            diff.push(format!("{key}: simulator {} != game {expected}", rng.1));
        }
    }
    let expected_player = &state["players"][0];
    compare_number(
        &mut diff,
        "player.max_hp",
        game.run.max_hp as i64,
        &expected_player["max_hit_points"],
    );
    compare_number(
        &mut diff,
        "player.gold",
        game.run.gold as i64,
        &expected_player["gold"],
    );
    compare_pile(
        &mut diff,
        content,
        "deck",
        &game.run.deck,
        &expected_player["deck"],
    );
    let relics: Vec<_> = game
        .run
        .relics
        .iter()
        .map(|&id| content.relics[id as usize].id)
        .collect();
    let expected_relics: Vec<_> = array(&expected_player["relics"])
        .iter()
        .filter_map(|relic| relic["model_id"].as_str())
        .collect();
    if relics != expected_relics {
        diff.push(format!(
            "relics: simulator {relics:?} != game {expected_relics:?}"
        ));
    }
    let potions: Vec<_> = game
        .run
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| potion.map(|id| (slot, content.potions[id as usize].id)))
        .collect();
    let expected_potions: Vec<_> = array(&expected_player["potions"])
        .iter()
        .enumerate()
        .filter_map(|(fallback, potion)| {
            Some((
                potion["amount"].as_u64().unwrap_or(fallback as u64) as usize,
                potion["model_id"].as_str()?,
            ))
        })
        .collect();
    if potions != expected_potions {
        diff.push(format!(
            "potions: simulator {potions:?} != game {expected_potions:?}"
        ));
    }
    let expected_combat = state["combat"]["enemies"].as_array().filter(|enemies| {
        enemies
            .iter()
            .any(|enemy| enemy["hit_points"].as_i64().unwrap_or_default() > 0)
    });
    let Some(combat) = game.combat() else {
        compare_number(
            &mut diff,
            "player.hp",
            game.run.hp as i64,
            &expected_player["hit_points"],
        );
        if expected_combat.is_some()
            && expected_player["hit_points"].as_i64().unwrap_or_default() > 0
        {
            diff.push("combat: simulator ended combat, game did not".into());
        }
        return diff;
    };
    if expected_combat.is_none() {
        diff.push("combat: game ended combat, simulator did not".into());
        return diff;
    }
    compare_number(
        &mut diff,
        "player.hp",
        combat.player.hp as i64,
        &expected_player["hit_points"],
    );
    compare_number(
        &mut diff,
        "player.block",
        combat.player.block as i64,
        &expected_player["block"],
    );
    compare_number(
        &mut diff,
        "player.energy",
        combat.energy as i64,
        &expected_player["energy"],
    );
    compare_number(
        &mut diff,
        "player.stars",
        combat.stars as i64,
        &expected_player["stars"],
    );
    compare_pile(
        &mut diff,
        content,
        "hand",
        &combat.hand,
        &expected_player["hand"],
    );
    compare_pile(
        &mut diff,
        content,
        "discard",
        &combat.discard,
        &expected_player["discard_pile"],
    );
    compare_pile(
        &mut diff,
        content,
        "exhaust",
        &combat.exhaust,
        &expected_player["exhaust_pile"],
    );
    let draw: Vec<_> = combat.draw.iter().rev().copied().collect();
    compare_pile(
        &mut diff,
        content,
        "draw",
        &draw,
        &oracle["hidden_draw_order"],
    );
    compare_powers(
        &mut diff,
        content,
        "player.powers",
        &combat.player.powers,
        &expected_player["powers"],
    );
    compare_number(
        &mut diff,
        "round",
        combat.turn as i64,
        &state["combat"]["round"],
    );
    let expected_enemies = array(&state["combat"]["enemies"]);
    let expected_ids: Vec<_> = expected_enemies
        .iter()
        .map(|enemy| instance_id(enemy["instance_id"].as_str().unwrap_or_default()))
        .collect();
    let extras = combat
        .enemies
        .iter()
        .filter(|enemy| enemy.creature.hp > 0 && !expected_ids.contains(&enemy.instance))
        .count();
    if extras > 0 {
        diff.push(format!(
            "enemies: simulator has {extras} unexpected living enemies"
        ));
    }
    for (index, expected) in expected_enemies.iter().enumerate() {
        let path = format!("enemy[{index}]");
        let Some(actual) = combat
            .enemies
            .iter()
            .find(|enemy| enemy.instance == expected_ids[index])
        else {
            diff.push(format!("{path}: missing enemy"));
            continue;
        };
        compare_number(
            &mut diff,
            &format!("{path}.hp"),
            actual.creature.hp as i64,
            &expected["hit_points"],
        );
        compare_number(
            &mut diff,
            &format!("{path}.block"),
            actual.creature.block as i64,
            &expected["block"],
        );
        compare_powers(
            &mut diff,
            content,
            &format!("{path}.powers"),
            &actual.creature.powers,
            &expected["powers"],
        );
        let expected_kind = expected["visible_intents"][0]["kind"]
            .as_str()
            .unwrap_or("");
        let Some(move_def) = content.enemies[actual.creature.id as usize]
            .moves
            .get(actual.move_index)
        else {
            diff.push(format!("{path}.move: invalid index {}", actual.move_index));
            continue;
        };
        let actual_intent = if actual.stunned {
            "Stun"
        } else {
            move_def.intent
        };
        if expected["hit_points"].as_i64().unwrap_or_default() > 0
            && !intent_matches(actual_intent, expected_kind)
        {
            diff.push(format!(
                "{path}.intent: simulator {actual_intent:?} != game {expected_kind:?}"
            ));
        }
    }
    diff
}

fn update_history(history: &mut TurnHistory, state: &Value, oracle: &Value) {
    let enemies = array(&state["combat"]["enemies"]);
    let encounter = format!(
        "{}:{}",
        state["total_floor"],
        enemies
            .first()
            .map(|x| &x["instance_id"])
            .unwrap_or(&Value::Null)
    );
    let round = state["combat"]["round"].as_u64().unwrap_or_default() as u16;
    let observe_moves = history.encounter != encounter || history.round != round;
    if history.encounter != encounter {
        *history = TurnHistory {
            encounter,
            round,
            withering: if enemies
                .iter()
                .any(|enemy| enemy["model_id"] == "MONSTER.AEONGLASS")
            {
                6
            } else {
                0
            },
            ..TurnHistory::default()
        };
    } else if history.round != round {
        history.last_cards = history.cards;
        history.cards = 0;
        history.manual_cards = 0;
        history.manual_plays = 0;
        history.attacks = 0;
        history.skills = 0;
        history.powers = 0;
        history.last_card = None;
        history.block_gains = 0;
        for (turns, _) in &mut history.toric {
            *turns = turns.saturating_sub(1);
        }
        history.toric.retain(|x| x.0 > 0);
        history.round = round;
    }
    if observe_moves {
        for enemy in enemies {
            let instance_id_text = enemy["instance_id"].as_str().unwrap_or_default();
            let instance = instance_id(instance_id_text);
            if let Some(move_id) = oracle["enemy_move_ids"][instance_id_text]
                .as_str()
                .map(str::to_owned)
            {
                history
                    .enemy_moves
                    .entry(instance)
                    .or_default()
                    .push(move_id);
            }
        }
    }
    let dampened = array(&state["players"][0]["powers"])
        .iter()
        .any(|power| power["model_id"] == "POWER.DAMPEN_POWER");
    for card in [
        &state["players"][0]["hand"],
        &state["players"][0]["discard_pile"],
        &state["players"][0]["exhaust_pile"],
        &oracle["hidden_draw_order"],
    ]
    .into_iter()
    .flat_map(array)
    {
        let instance = instance_id(card["instance_id"].as_str().unwrap_or_default());
        let upgrades = card["upgrade"].as_u64().unwrap_or_default() as u8;
        if !dampened || !history.upgrades.contains_key(&instance) {
            history.upgrades.insert(instance, upgrades);
        }
    }
    if array(&state["players"][0]["relics"])
        .iter()
        .any(|relic| relic["model_id"] == "RELIC.ORNAMENTAL_FAN" && relic["state"] == "Active")
    {
        history.attacks += (2 - history.attacks.rem_euclid(3)).rem_euclid(3);
    }
}

fn advance_history(
    content: &Content,
    history: &mut TurnHistory,
    before: &Value,
    after: &Value,
    action: &Value,
) {
    if action["kind"] == "end_turn"
        && before["combat"]["round"].as_u64().unwrap_or_default() % 3 == 0
        && array(&before["combat"]["enemies"])
            .iter()
            .any(|enemy| enemy["model_id"] == "MONSTER.AEONGLASS")
    {
        history.wither_upgrades += 1;
    }
    if action["kind"] != "play_card" {
        return;
    }
    let source = action["source_id"].as_str().unwrap_or_default();
    let Some(card) = array(&before["players"][0]["hand"])
        .iter()
        .find(|card| card["instance_id"] == source)
    else {
        return;
    };
    let Some(id) = card["model_id"].as_str().and_then(|id| content.card_id(id)) else {
        return;
    };
    let card_type = match card["type"].as_str() {
        Some("Attack") => CardType::Attack,
        Some("Power") => CardType::Power,
        Some("Status") => CardType::Status,
        Some("Curse") => CardType::Curse,
        Some("Quest") => CardType::Quest,
        _ => CardType::Skill,
    };
    let plays = 1
        + (card_type == CardType::Attack
            && array(&before["players"][0]["powers"])
                .iter()
                .any(|power| power["model_id"] == "POWER.ONE_TWO_PUNCH_POWER")) as i16;
    let mut automatic: i16 = content.cards[id as usize]
        .effects
        .iter()
        .map(|effect| match effect {
            Effect::AutoPlayDraw(amount, _) => {
                if card["upgrade"].as_u64().unwrap_or_default() > 0 {
                    amount.upgraded
                } else {
                    amount.base
                }
            }
            _ => 0,
        })
        .sum();
    if automatic > 0 && before["players"][0]["draw_pile_count"] == 0 {
        automatic = automatic.max(
            array(&before["players"][0]["discard_pile"]).len() as i16
                - after["players"][0]["draw_pile_count"]
                    .as_i64()
                    .unwrap_or_default() as i16,
        );
    }
    if history.withering > 0 {
        history.withering -= plays + automatic;
        while history.withering <= 0 {
            history.withering += 6;
        }
    }
    history.cards += plays + automatic;
    history.manual_cards += 1;
    history.manual_plays += plays;
    match card_type {
        CardType::Attack => history.attacks += plays,
        CardType::Skill => history.skills += plays,
        CardType::Power => history.powers += plays,
        _ => {}
    }
    if matches!(card_type, CardType::Attack | CardType::Skill) {
        history.last_card = self::card(content, card).ok();
    }
    let gained = number(&after["players"][0], "block").unwrap_or_default()
        - number(&before["players"][0], "block").unwrap_or_default();
    if gained > 0 {
        history.block_gains += plays
            * if content.cards[id as usize].id == "CARD.MAD_SCIENCE" && card_type == CardType::Skill
            {
                1
            } else {
                block_gains(
                    content.cards[id as usize].effects,
                    number(&before["players"][0], "cards_exhausted_this_turn").unwrap_or_default()
                        > 0,
                    card["upgrade"].as_u64().unwrap_or_default() > 0,
                )
            };
    }
    if content.cards[id as usize].id == "CARD.TORIC_TOUGHNESS" {
        history.toric.push((2, gained as i16));
    }
    if content.cards[id as usize].id != "CARD.THRASH" {
        return;
    }
    let exhausted_before = array(&before["players"][0]["exhaust_pile"]);
    let Some(exhausted) = array(&after["players"][0]["exhaust_pile"])
        .iter()
        .find(|card| {
            !exhausted_before
                .iter()
                .any(|old| old["instance_id"] == card["instance_id"])
                && card["type"] == "Attack"
        })
    else {
        return;
    };
    let Some(exhausted_id) = exhausted["model_id"]
        .as_str()
        .and_then(|id| content.card_id(id))
    else {
        return;
    };
    let upgraded = exhausted["upgrade"].as_u64().unwrap_or_default() > 0;
    let damage = content.cards[exhausted_id as usize]
        .effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Attack(_, amount, _) | Effect::AttackMany(_, amount, _) => Some(if upgraded {
                amount.upgraded
            } else {
                amount.base
            }),
            _ => None,
        })
        .unwrap_or_default();
    let instance = instance_id(source);
    *history.values.entry(instance).or_default() += damage;
}

fn block_gains(effects: &[Effect], exhausted: bool, upgraded: bool) -> i16 {
    effects
        .iter()
        .map(|effect| match effect {
            Effect::Block(..)
            | Effect::DodgeRoll(..)
            | Effect::DrawBlockIf(..)
            | Effect::ExhaustForBlock(..) => 1,
            Effect::If(condition, yes, no) => block_gains(
                match condition {
                    Condition::ExhaustedThisTurn if exhausted => yes,
                    Condition::ExhaustedThisTurn => no,
                    Condition::Upgraded if upgraded => yes,
                    Condition::Upgraded => no,
                    _ if block_gains(yes, exhausted, upgraded) > 0 => yes,
                    _ => no,
                },
                exhausted,
                upgraded,
            ),
            Effect::Repeat(_, effects) | Effect::Random(_, effects) => {
                block_gains(effects, exhausted, upgraded)
            }
            _ => 0,
        })
        .sum()
}

fn cards(content: &Content, values: &Value) -> Result<Vec<Card>, String> {
    array(values)
        .iter()
        .map(|value| card(content, value))
        .collect()
}

fn card(content: &Content, value: &Value) -> Result<Card, String> {
    let id = content
        .card_id(text(value, "model_id")?)
        .ok_or_else(|| format!("simulator has no card {}", value["model_id"]))?;
    let upgrades = value["upgrade"].as_u64().unwrap_or_default() as u8;
    let base = content.cards[id as usize].cost[upgrades.min(1) as usize];
    let shown = value["energy_cost"].as_i64().unwrap_or(base as i64) as i8;
    Ok(Card {
        id,
        instance: instance_id(value["instance_id"].as_str().unwrap_or_default()),
        upgrades,
        cost_delta: shown.saturating_sub(base),
        value: [
            "rampage_extra_damage",
            "claw_extra_damage",
            "genetic_algorithm_bonus_block",
            "combats_seen",
        ]
        .iter()
        .filter_map(|key| value[*key].as_i64())
        .max()
        .unwrap_or_default() as i16,
        cost_override: value["cost_for_turn"]
            .as_i64()
            .or_else(|| value["combat_cost_override"].as_i64())
            .map(|x| x as i8),
        enchantment: match value["enchantment_model_id"].as_str() {
            Some("ENCHANTMENT.ADROIT") => Some(Enchantment::Adroit),
            Some("ENCHANTMENT.CLONE") => Some(Enchantment::Clone),
            Some("ENCHANTMENT.CORRUPTED") => Some(Enchantment::Corrupted),
            Some("ENCHANTMENT.GLAM") => Some(Enchantment::Glam),
            Some("ENCHANTMENT.GOOPY") => Some(Enchantment::Goopy),
            Some("ENCHANTMENT.IMBUED") => Some(Enchantment::Imbued),
            Some("ENCHANTMENT.INKY") => Some(Enchantment::Inky),
            Some("ENCHANTMENT.INSTINCT") => Some(Enchantment::Instinct),
            Some("ENCHANTMENT.MOMENTUM") => Some(Enchantment::Momentum),
            Some("ENCHANTMENT.NIMBLE") => Some(Enchantment::Nimble),
            Some("ENCHANTMENT.PERFECT_FIT") => Some(Enchantment::PerfectFit),
            Some("ENCHANTMENT.ROYALLY_APPROVED") => Some(Enchantment::RoyallyApproved),
            Some("ENCHANTMENT.SHARP") => Some(Enchantment::Sharp),
            Some("ENCHANTMENT.SLITHER") => Some(Enchantment::Slither),
            Some("ENCHANTMENT.SLUMBERING_ESSENCE") => Some(Enchantment::SlumberingEssence),
            Some("ENCHANTMENT.SOULS_POWER") => Some(Enchantment::SoulsPower),
            Some("ENCHANTMENT.SOWN") => Some(Enchantment::Sown),
            Some("ENCHANTMENT.SPIRAL") => Some(Enchantment::Spiral),
            Some("ENCHANTMENT.STEADY") => Some(Enchantment::Steady),
            Some("ENCHANTMENT.SWIFT") => Some(Enchantment::Swift),
            Some("ENCHANTMENT.TEZCATARAS_EMBER") => Some(Enchantment::TezcatarasEmber),
            Some("ENCHANTMENT.VIGOROUS") => Some(Enchantment::Vigorous),
            _ => None,
        },
        enchantment_amount: value["enchantment_amount"].as_i64().unwrap_or_default() as i16,
        enchantment_value: value["enchantment_combat_amount"]
            .as_i64()
            .unwrap_or_default() as i16,
        variant: if content.cards[id as usize].id == "CARD.MAD_SCIENCE" {
            match value["type"].as_str() {
                Some("Attack") => 10,
                Some("Power") => 30,
                _ => 20,
            }
        } else {
            0
        },
        ..Card::default()
    })
}

fn powers(content: &Content, values: &Value) -> Result<Vec<Power>, String> {
    array(values)
        .iter()
        .map(|value| {
            Ok(Power {
                id: find_id(&content.powers, text(value, "model_id")?, |x| x.id)?,
                amount: number(value, "amount")? as i16,
                skip_duration: false,
                value: 0,
            })
        })
        .collect()
}

fn ids<T>(
    values: &[T],
    traced: &Value,
    name: impl Fn(&T) -> &'static str + Copy,
) -> Result<Vec<Id>, String> {
    array(traced)
        .iter()
        .map(|value| find_id(values, text(value, "model_id")?, name))
        .collect()
}

fn find_id<T>(values: &[T], wanted: &str, name: impl Fn(&T) -> &'static str) -> Result<Id, String> {
    values
        .iter()
        .position(|value| name(value) == wanted)
        .map(|x| x as Id)
        .ok_or_else(|| format!("simulator has no {wanted}"))
}

fn target(combat: &Combat, target: Option<&str>) -> Result<Option<usize>, String> {
    let Some(target) = target else {
        return Ok(None);
    };
    if target == "creature:0" {
        return Ok(None);
    }
    let instance = instance_id(target);
    combat
        .enemies
        .iter()
        .position(|enemy| enemy.instance == instance && enemy.creature.hp > 0)
        .map(Some)
        .ok_or_else(|| format!("unknown target {target}"))
}

fn infer_move(enemy: &EnemyDef, state: &Value, move_id: &str) -> usize {
    if let Some(index) = match (enemy.id, move_id) {
        ("MONSTER.KIN_FOLLOWER", "QUICK_SLASH_MOVE") => Some(0),
        ("MONSTER.KIN_FOLLOWER", "BOOMERANG_MOVE") => Some(1),
        ("MONSTER.KIN_FOLLOWER", "POWER_DANCE_MOVE") => Some(2),
        ("MONSTER.KIN_PRIEST", "ORB_OF_FRAILTY_MOVE") => Some(0),
        ("MONSTER.KIN_PRIEST", "ORB_OF_WEAKNESS_MOVE") => Some(1),
        ("MONSTER.KIN_PRIEST", "BEAM_MOVE") => Some(2),
        ("MONSTER.KIN_PRIEST", "RITUAL_MOVE") => Some(3),
        ("MONSTER.TUNNELER", "BITE_MOVE") => Some(0),
        ("MONSTER.TUNNELER", "BURROW_MOVE") => Some(1),
        ("MONSTER.TUNNELER", "BELOW_MOVE") => Some(2),
        ("MONSTER.TUNNELER", "DIZZY_MOVE" | "STUNNED") => Some(3),
        ("MONSTER.THIEVING_HOPPER", "THIEVERY_MOVE") => Some(0),
        ("MONSTER.THIEVING_HOPPER", "NAB_MOVE") => Some(1),
        ("MONSTER.THIEVING_HOPPER", "HAT_TRICK_MOVE") => Some(2),
        ("MONSTER.THIEVING_HOPPER", "FLUTTER_MOVE") => Some(3),
        ("MONSTER.THIEVING_HOPPER", "ESCAPE_MOVE") => Some(4),
        ("MONSTER.THIEVING_HOPPER", "STUNNED") => Some(5),
        ("MONSTER.HUNTER_KILLER", "TENDERIZING_GOOP_MOVE") => Some(0),
        ("MONSTER.HUNTER_KILLER", "BITE_MOVE") => Some(1),
        ("MONSTER.HUNTER_KILLER", "PUNCTURE_MOVE") => Some(2),
        ("MONSTER.CHOMPER", "CLAMP_MOVE") => Some(0),
        ("MONSTER.CHOMPER", "SCREECH_MOVE") => Some(1),
        ("MONSTER.BOWLBUG_EGG", "BITE_MOVE") => Some(0),
        ("MONSTER.BOWLBUG_NECTAR", "THRASH_MOVE") => Some(0),
        ("MONSTER.BOWLBUG_NECTAR", "BUFF_MOVE") => Some(1),
        ("MONSTER.BOWLBUG_NECTAR", "THRASH2_MOVE") => Some(2),
        ("MONSTER.BOWLBUG_ROCK", "HEADBUTT_MOVE") => Some(0),
        ("MONSTER.BOWLBUG_ROCK", "DIZZY_MOVE" | "STUNNED") => Some(1),
        ("MONSTER.BOWLBUG_SILK", "THRASH_MOVE") => Some(0),
        ("MONSTER.BOWLBUG_SILK", "TOXIC_SPIT_MOVE") => Some(1),
        ("MONSTER.THE_OBSCURA", "ILLUSION_MOVE") => Some(0),
        ("MONSTER.THE_OBSCURA", "PIERCING_GAZE_MOVE") => Some(1),
        ("MONSTER.THE_OBSCURA", "SAIL_MOVE") => Some(2),
        ("MONSTER.THE_OBSCURA", "HARDENING_STRIKE_MOVE") => Some(3),
        ("MONSTER.PARAFRIGHT", "SLAM_MOVE") => Some(0),
        ("MONSTER.PARAFRIGHT", "REVIVE_MOVE") => Some(1),
        (
            "MONSTER.DECIMILLIPEDE_SEGMENT_FRONT"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_MIDDLE"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_BACK",
            "WRITHE_MOVE",
        ) => Some(0),
        (
            "MONSTER.DECIMILLIPEDE_SEGMENT_FRONT"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_MIDDLE"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_BACK",
            "BULK_MOVE",
        ) => Some(1),
        (
            "MONSTER.DECIMILLIPEDE_SEGMENT_FRONT"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_MIDDLE"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_BACK",
            "CONSTRICT_MOVE",
        ) => Some(2),
        (
            "MONSTER.DECIMILLIPEDE_SEGMENT_FRONT"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_MIDDLE"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_BACK",
            "DEAD_MOVE",
        ) => Some(3),
        (
            "MONSTER.DECIMILLIPEDE_SEGMENT_FRONT"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_MIDDLE"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_BACK",
            "REATTACH_MOVE",
        ) => Some(4),
        ("MONSTER.FUZZY_WURM_CRAWLER", "FIRST_ACID_GOOP") => Some(0),
        ("MONSTER.FUZZY_WURM_CRAWLER", "INHALE") => Some(1),
        ("MONSTER.FUZZY_WURM_CRAWLER", "ACID_GOOP") => Some(2),
        ("MONSTER.SNAPPING_JAXFRUIT", "ENERGY_ORB_MOVE") => Some(0),
        ("MONSTER.FLYCONID", "VULNERABLE_SPORES_MOVE") => Some(0),
        ("MONSTER.FLYCONID", "FRAIL_SPORES_MOVE") => Some(1),
        ("MONSTER.FLYCONID", "SMASH_MOVE") => Some(2),
        ("MONSTER.BYGONE_EFFIGY", "SLEEP_MOVE") => Some(0),
        ("MONSTER.BYGONE_EFFIGY", "WAKE_MOVE") => Some(1),
        ("MONSTER.BYGONE_EFFIGY", "SLEEP_MOVE_2") => Some(2),
        ("MONSTER.BYGONE_EFFIGY", "SLASHES_MOVE") => Some(3),
        ("MONSTER.VINE_SHAMBLER", "GRASPING_VINES_MOVE") => Some(0),
        ("MONSTER.VINE_SHAMBLER", "SWIPE_MOVE") => Some(1),
        ("MONSTER.VINE_SHAMBLER", "CHOMP_MOVE") => Some(2),
        ("MONSTER.VANTOM", "INK_BLOT_MOVE") => Some(0),
        ("MONSTER.VANTOM", "INKY_LANCE_MOVE") => Some(1),
        ("MONSTER.VANTOM", "DISMEMBER_MOVE") => Some(2),
        ("MONSTER.VANTOM", "PREPARE_MOVE") => Some(3),
        ("MONSTER.ENTOMANCER", "BEES_MOVE") => Some(0),
        ("MONSTER.ENTOMANCER", "SPEAR_MOVE") => Some(1),
        ("MONSTER.ENTOMANCER", "PHEROMONE_SPIT_MOVE") => Some(2),
        ("MONSTER.INFESTED_PRISM", "JAB_MOVE") => Some(0),
        ("MONSTER.INFESTED_PRISM", "RADIATE_MOVE") => Some(1),
        ("MONSTER.INFESTED_PRISM", "WHIRLWIND_MOVE") => Some(2),
        ("MONSTER.INFESTED_PRISM", "PULSATE_MOVE") => Some(3),
        ("MONSTER.SPINY_TOAD", "PROTRUDING_SPIKES_MOVE") => Some(0),
        ("MONSTER.SPINY_TOAD", "SPIKE_EXPLOSION_MOVE") => Some(1),
        ("MONSTER.SPINY_TOAD", "TONGUE_LASH_MOVE") => Some(2),
        ("MONSTER.THE_INSATIABLE", "LIQUIFY_GROUND_MOVE") => Some(0),
        ("MONSTER.THE_INSATIABLE", "LUNGING_BITE_MOVE") => Some(1),
        ("MONSTER.THE_INSATIABLE", "THRASH_MOVE") => Some(2),
        ("MONSTER.THE_INSATIABLE", "THRASH_MOVE_2") => Some(3),
        ("MONSTER.THE_INSATIABLE", "SALIVATE_MOVE") => Some(4),
        ("MONSTER.LIVING_SHIELD", "SHIELD_SLAM_MOVE") => Some(0),
        ("MONSTER.LIVING_SHIELD", "SMASH_MOVE") => Some(1),
        ("MONSTER.TURRET_OPERATOR", "UNLOAD_MOVE") => Some(0),
        ("MONSTER.TURRET_OPERATOR", "UNLOAD_MOVE_2") => Some(1),
        ("MONSTER.TURRET_OPERATOR", "RELOAD_MOVE") => Some(2),
        ("MONSTER.SCROLL_OF_BITING", "CHOMP") => Some(0),
        ("MONSTER.SCROLL_OF_BITING", "CHEW") => Some(1),
        ("MONSTER.SCROLL_OF_BITING", "MORE_TEETH") => Some(2),
        ("MONSTER.THE_LOST", "DEBILITATING_SMOG") => Some(0),
        ("MONSTER.THE_LOST", "EYE_LASERS") => Some(1),
        ("MONSTER.THE_FORGOTTEN", "MIASMA") => Some(0),
        ("MONSTER.THE_FORGOTTEN", "DREAD") => Some(1),
        ("MONSTER.SOUL_NEXUS", "SOUL_BURN_MOVE") => Some(0),
        ("MONSTER.SOUL_NEXUS", "MAELSTROM_MOVE") => Some(1),
        ("MONSTER.SOUL_NEXUS", "DRAIN_LIFE_MOVE") => Some(2),
        ("MONSTER.MECHA_KNIGHT", "CHARGE_MOVE") => Some(0),
        ("MONSTER.MECHA_KNIGHT", "FLAMETHROWER_MOVE") => Some(1),
        ("MONSTER.MECHA_KNIGHT", "WINDUP_MOVE") => Some(2),
        ("MONSTER.MECHA_KNIGHT", "HEAVY_CLEAVE_MOVE") => Some(3),
        ("MONSTER.FLAIL_KNIGHT", "WAR_CHANT") => Some(0),
        ("MONSTER.FLAIL_KNIGHT", "FLAIL_MOVE") => Some(1),
        ("MONSTER.FLAIL_KNIGHT", "RAM_MOVE") => Some(2),
        ("MONSTER.SPECTRAL_KNIGHT", "HEX") => Some(0),
        ("MONSTER.SPECTRAL_KNIGHT", "SOUL_SLASH") => Some(1),
        ("MONSTER.SPECTRAL_KNIGHT", "SOUL_FLAME") => Some(2),
        ("MONSTER.MAGI_KNIGHT", "POWER_SHIELD_MOVE") => Some(0),
        ("MONSTER.MAGI_KNIGHT", "DAMPEN_MOVE") => Some(1),
        ("MONSTER.MAGI_KNIGHT", "RAM_MOVE") => Some(2),
        ("MONSTER.MAGI_KNIGHT", "PREP_MOVE") => Some(3),
        ("MONSTER.MAGI_KNIGHT", "MAGIC_BOMB") => Some(4),
        ("MONSTER.TORCH_HEAD_AMALGAM", "TACKLE_MOVE") => Some(0),
        ("MONSTER.TORCH_HEAD_AMALGAM", "TACKLE_2_MOVE") => Some(1),
        ("MONSTER.TORCH_HEAD_AMALGAM", "BEAM_MOVE") => Some(2),
        ("MONSTER.TORCH_HEAD_AMALGAM", "TACKLE_3_MOVE") => Some(3),
        ("MONSTER.TORCH_HEAD_AMALGAM", "TACKLE_4_MOVE") => Some(4),
        ("MONSTER.QUEEN", "PUPPET_STRINGS_MOVE") => Some(0),
        ("MONSTER.QUEEN", "YOU_ARE_MINE_MOVE") => Some(1),
        ("MONSTER.QUEEN", "BURN_BRIGHT_FOR_ME_MOVE") => Some(2),
        ("MONSTER.QUEEN", "OFF_WITH_YOUR_HEAD_MOVE") => Some(3),
        ("MONSTER.QUEEN", "EXECUTION_MOVE") => Some(4),
        ("MONSTER.QUEEN", "ENRAGE_MOVE") => Some(5),
        _ => None,
    } {
        return index;
    }
    let intents = array(&state["visible_intents"]);
    let damage = intents
        .first()
        .and_then(|intent| intent["damage"].as_i64())
        .unwrap_or_default();
    enemy
        .moves
        .iter()
        .enumerate()
        .min_by_key(|(_, candidate)| {
            let kind_penalty = intents
                .iter()
                .filter(|intent| {
                    !intent_matches(
                        candidate.intent,
                        intent["kind"].as_str().unwrap_or_default(),
                    )
                })
                .count() as i64
                * 10_000;
            let candidate_damage = candidate
                .intent
                .split(|c: char| !c.is_ascii_digit())
                .find_map(|x| x.parse::<i64>().ok())
                .unwrap_or_default();
            let semantic_penalty = candidate
                .intent
                .split(|c: char| !c.is_ascii_alphabetic())
                .filter(|word| word.len() >= 4)
                .all(|word| !move_id.contains(&word[..4].to_ascii_uppercase()))
                as i64;
            kind_penalty + semantic_penalty * 100 + (candidate_damage - damage).abs()
        })
        .map(|(index, _)| index)
        .unwrap_or_default()
}

fn intent_matches(intent: &str, kind: &str) -> bool {
    match kind {
        "Attack" => intent.starts_with("Attack"),
        "Defend" => intent.contains("Block") || intent.contains("Defend"),
        "Buff" => !intent.starts_with("Attack") && !intent.contains("Debuff"),
        "Escape" => intent.contains("Escape"),
        "Heal" => intent.contains("Heal"),
        "Sleep" => intent.contains("Sleep"),
        "Stun" => intent.contains("Stun"),
        "Summon" => intent.contains("Summon"),
        "StatusCard" => {
            intent.contains("Status") || intent.contains("Infection") || intent.contains("Slimed")
        }
        _ => !intent.starts_with("Attack"),
    }
}

fn compare_number(diff: &mut Vec<String>, path: &str, actual: i64, expected: &Value) {
    if let Some(expected) = expected.as_i64().filter(|&expected| expected != actual) {
        diff.push(format!("{path}: simulator {actual} != game {expected}"));
    }
}

fn compare_pile(
    diff: &mut Vec<String>,
    content: &Content,
    path: &str,
    actual: &[Card],
    expected: &Value,
) {
    let actual: Vec<_> = actual
        .iter()
        .map(|card| {
            let mut value = format!("{}+{}", content.cards[card.id as usize].id, card.upgrades);
            if let Some(enchantment) = card.enchantment {
                value.push_str(&format!(
                    "@{}:{}",
                    enchantment_id(enchantment),
                    card.enchantment_amount
                ));
            }
            value
        })
        .collect();
    let expected: Vec<_> = array(expected)
        .iter()
        .map(|card| {
            let mut value = format!(
                "{}+{}",
                card["model_id"].as_str().unwrap_or("?"),
                card["upgrade"].as_u64().unwrap_or_default()
            );
            if let Some(enchantment) = card["enchantment_model_id"].as_str() {
                value.push_str(&format!(
                    "@{}:{}",
                    enchantment,
                    card["enchantment_amount"].as_i64().unwrap_or_default()
                ));
            }
            value
        })
        .collect();
    if actual != expected {
        diff.push(format!("{path}: simulator {actual:?} != game {expected:?}"));
    }
}

fn enchantment_id(enchantment: Enchantment) -> &'static str {
    match enchantment {
        Enchantment::Adroit => "ENCHANTMENT.ADROIT",
        Enchantment::Clone => "ENCHANTMENT.CLONE",
        Enchantment::Corrupted => "ENCHANTMENT.CORRUPTED",
        Enchantment::Glam => "ENCHANTMENT.GLAM",
        Enchantment::Goopy => "ENCHANTMENT.GOOPY",
        Enchantment::Imbued => "ENCHANTMENT.IMBUED",
        Enchantment::Inky => "ENCHANTMENT.INKY",
        Enchantment::Instinct => "ENCHANTMENT.INSTINCT",
        Enchantment::Momentum => "ENCHANTMENT.MOMENTUM",
        Enchantment::Nimble => "ENCHANTMENT.NIMBLE",
        Enchantment::PerfectFit => "ENCHANTMENT.PERFECT_FIT",
        Enchantment::RoyallyApproved => "ENCHANTMENT.ROYALLY_APPROVED",
        Enchantment::Sharp => "ENCHANTMENT.SHARP",
        Enchantment::Slither => "ENCHANTMENT.SLITHER",
        Enchantment::SlumberingEssence => "ENCHANTMENT.SLUMBERING_ESSENCE",
        Enchantment::SoulsPower => "ENCHANTMENT.SOULS_POWER",
        Enchantment::Sown => "ENCHANTMENT.SOWN",
        Enchantment::Spiral => "ENCHANTMENT.SPIRAL",
        Enchantment::Steady => "ENCHANTMENT.STEADY",
        Enchantment::Swift => "ENCHANTMENT.SWIFT",
        Enchantment::TezcatarasEmber => "ENCHANTMENT.TEZCATARAS_EMBER",
        Enchantment::Vigorous => "ENCHANTMENT.VIGOROUS",
    }
}

fn compare_powers(
    diff: &mut Vec<String>,
    content: &Content,
    path: &str,
    actual: &[Power],
    expected: &Value,
) {
    let mut actual: Vec<_> = actual
        .iter()
        .map(|power| format!("{}={}", content.powers[power.id as usize].id, power.amount))
        .collect();
    let mut expected: Vec<_> = array(expected)
        .iter()
        .map(|power| {
            format!(
                "{}={}",
                power["model_id"].as_str().unwrap_or("?"),
                power["amount"].as_i64().unwrap_or_default()
            )
        })
        .collect();
    actual.sort();
    expected.sort();
    if actual != expected {
        diff.push(format!("{path}: simulator {actual:?} != game {expected:?}"));
    }
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key].as_str().ok_or_else(|| format!("missing {key}"))
}

fn wongo_combats(player: &Value) -> Option<u8> {
    array(&player["relics"])
        .iter()
        .find(|relic| {
            relic["model_id"] == "RELIC.WONGOS_MYSTERY_TICKET" && relic["state"] != "Disabled"
        })
        .map(|relic| 5u8.saturating_sub(relic["amount"].as_u64().unwrap_or(5) as u8))
}

fn relic_amount(player: &Value, id: &str) -> u8 {
    relic_amount_opt(player, id).unwrap_or_default()
}

fn relic_amount_opt(player: &Value, id: &str) -> Option<u8> {
    array(&player["relics"])
        .iter()
        .find(|relic| relic["model_id"] == id)
        .and_then(|relic| relic["amount"].as_u64())
        .map(|amount| amount as u8)
}

fn relic_disabled(player: &Value, id: &str) -> bool {
    array(&player["relics"])
        .iter()
        .any(|relic| relic["model_id"] == id && relic["state"] == "Disabled")
}

fn number(value: &Value, key: &str) -> Result<i64, String> {
    value[key].as_i64().ok_or_else(|| format!("missing {key}"))
}

fn instance_id(id: &str) -> u32 {
    id.rsplit(':')
        .next()
        .and_then(|x| x.parse::<u32>().ok())
        .unwrap_or_default()
        + 1
}
