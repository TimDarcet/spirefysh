#![allow(clippy::too_many_arguments)]

use crate::game::{
    ENERGY_RELICS, FABRICATOR_BOTS, TEMPORARY_STRENGTH_DOWN, base_cost, eligible, event_page,
    price, price_mut, relic_deques,
};
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
    total_cards: i16,
    manual_cards: i16,
    manual_plays: i16,
    attacks: i16,
    skills: i16,
    powers: i16,
    shivs: i16,
    last_card: Option<Card>,
    // The previous turn's last manual play, which History Course copies at turn start.
    previous_card: Option<Card>,
    // Older captures list each card played this turn once, losing a repeat's order.
    played: Vec<u32>,
    last_repeat: bool,
    block_gains: i16,
    last_cards: i16,
    values: HashMap<u32, i16>,
    upgrades: HashMap<u32, u8>,
    toric: Vec<(u8, i16)>,
    automation_drawn: Option<i16>,
    withering: i16,
    wither_upgrades: i16,
    bolas: Vec<u32>,
    panache_start: Option<i16>,
    enemy_moves: HashMap<u32, Vec<String>>,
}

struct ReplayState {
    episode: String,
    act: u8,
    encounter_act: Option<Id>,
    encounters: Vec<Id>,
    elites: Vec<Id>,
    weak_encounters_left: u8,
    regular_encounters_left: u8,
    elite_encounters_left: u8,
    last_encounter: Option<Id>,
    last_elite: Option<Id>,
    seen_encounter: Option<(u8, bool, Id)>,
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
    maw_bank: u8,
    silver_treasures: u8,
    golden_compass: Option<u8>,
    // The sim's count of not-fully-blocked hits on the player (turn, hits); not captured.
    unblocked_hits: (u16, i16),
    winged_boots: u8,
    fishing_rod: u8,
    rest_used: u8,
    girya: u8,
    pumpkin_candle: i32,
    /// The newest aggro bot seen this combat (instance, 1 + FABRICATOR_BOTS index): the
    /// Fabricator's last spawn, which may already be dead and removed.
    aggro_bot: Option<(u32, i16)>,
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
    courier_restock: Option<(usize, ShopItem)>,
    fake_happy_flower: u8,
    /// The discard piles seen by the choices offered during the decision being replayed.
    offered_discards: Vec<Value>,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            episode: String::new(),
            act: 0,
            encounter_act: None,
            encounters: vec![],
            elites: vec![],
            weak_encounters_left: 0,
            regular_encounters_left: 0,
            elite_encounters_left: 0,
            last_encounter: None,
            last_elite: None,
            seen_encounter: None,
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
            maw_bank: 0,
            silver_treasures: 0,
            golden_compass: None,
            unblocked_hits: (0, 0),
            winged_boots: 0,
            fishing_rod: 0,
            rest_used: 0,
            girya: 0,
            pumpkin_candle: 0,
            aggro_bot: None,
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
            courier_restock: None,
            fake_happy_flower: 0,
            offered_discards: vec![],
        }
    }
}

impl ReplayState {
    fn encounter_bag(
        &self,
        content: &Content,
        act: Id,
        elite: bool,
        forced: Option<Id>,
    ) -> Vec<Id> {
        let def = &content.acts[act as usize];
        let full = if elite {
            def.elites.to_vec()
        } else {
            def.encounters
                .iter()
                .copied()
                .filter(|&id| {
                    let weak = content.encounters[id as usize].id.contains("_WEAK");
                    if self.weak_encounters_left > 0 {
                        weak
                    } else if self.regular_encounters_left > 0 {
                        !weak
                    } else {
                        true
                    }
                })
                .collect()
        };
        let mut bag = if elite {
            self.elites.clone()
        } else {
            self.encounters.clone()
        };
        if bag.is_empty() || forced.is_some_and(|id| !bag.contains(&id) && full.contains(&id)) {
            bag = full;
        }
        if let Some(id) = forced {
            if let Some(index) = bag.iter().position(|&candidate| candidate == id) {
                bag.swap(0, index);
            } else {
                bag.insert(0, id);
            }
        }
        bag
    }

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
        self.aggro_bot =
            newest_aggro_bot(state).max(self.aggro_bot.filter(|_| state["phase"] == "combat"));
        // Newer captures record the grab bags themselves.
        let bags = &record["oracle_before"]["relic_grab_bags"];
        if bags.is_object() {
            for (name, deques) in [
                ("player", &mut self.relic_deques),
                ("shared", &mut self.shared_relic_deques),
            ] {
                for (rarity, label) in ["Common", "Uncommon", "Rare", "Shop"].iter().enumerate() {
                    deques[rarity] = array(&bags[format!("{name}.{label}")])
                        .iter()
                        .filter_map(|id| content.relic_id(id.as_str()?))
                        .collect();
                }
            }
        }
        // Shops pull their relics out of the grab bags whether or not they're bought.
        for offer in [&record["before"], &record["after"]]
            .into_iter()
            .flat_map(|state| array(&state["visible_offers"]))
        {
            if let Some(id) = offer["id"]
                .as_str()
                .filter(|id| id.starts_with("shop:"))
                .and_then(|id| id.split(':').nth(2))
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
        let Some(encounter_act) = trace_act(content, state) else {
            return;
        };
        if self.encounter_act != Some(encounter_act) {
            let def = &content.acts[encounter_act as usize];
            let weak = def
                .encounters
                .iter()
                .copied()
                .filter(|&id| content.encounters[id as usize].id.contains("_WEAK"))
                .collect::<Vec<_>>();
            let regular = def
                .encounters
                .iter()
                .copied()
                .filter(|&id| !content.encounters[id as usize].id.contains("_WEAK"))
                .collect::<Vec<_>>();
            let weak_count = if act == 3 { 2 } else { 3 };
            let room_count = if act == 2 {
                14
            } else if act == 3 {
                13
            } else {
                15
            };
            self.encounter_act = Some(encounter_act);
            self.weak_encounters_left = if weak.is_empty() { 0 } else { weak_count };
            self.regular_encounters_left = if regular.is_empty() {
                0
            } else {
                room_count - weak_count
            };
            self.elite_encounters_left = if def.elites.is_empty() { 0 } else { 15 };
            self.encounters = if self.weak_encounters_left > 0 {
                weak
            } else {
                regular
            };
            self.elites = def.elites.to_vec();
            self.last_encounter = None;
            self.last_elite = None;
            self.seen_encounter = None;
        }
        let def = &content.acts[encounter_act as usize];
        let room = trace_room(state["room_type"].as_str().unwrap_or("Map"));
        let Some(id) = trace_encounter(content, state) else {
            return;
        };
        let elite = match room {
            Room::Combat if def.encounters.contains(&id) => false,
            Room::Elite if def.elites.contains(&id) => true,
            _ => return,
        };
        let seen = (
            state["act_floor"].as_u64().unwrap_or_default() as u8,
            elite,
            id,
        );
        if self.seen_encounter == Some(seen) {
            return;
        }
        self.seen_encounter = Some(seen);
        if elite {
            if self.elites.is_empty() {
                self.elites = def.elites.to_vec();
            }
            self.elites.retain(|&candidate| candidate != id);
            self.elite_encounters_left = self.elite_encounters_left.saturating_sub(1);
            self.last_elite = Some(id);
            if self.elite_encounters_left == 0 {
                self.elites.clear();
                self.last_elite = None;
            }
            return;
        }
        let weak = content.encounters[id as usize].id.contains("_WEAK");
        let pool = def
            .encounters
            .iter()
            .copied()
            .filter(|&candidate| {
                content.encounters[candidate as usize].id.contains("_WEAK") == weak
            })
            .collect::<Vec<_>>();
        if self.encounters.is_empty()
            || self.encounters.iter().any(|&candidate| {
                content.encounters[candidate as usize].id.contains("_WEAK") != weak
            })
        {
            self.encounters = pool;
        }
        self.encounters.retain(|&candidate| candidate != id);
        if weak {
            self.weak_encounters_left = self.weak_encounters_left.saturating_sub(1);
            if self.weak_encounters_left == 0 {
                self.encounters = def
                    .encounters
                    .iter()
                    .copied()
                    .filter(|&id| !content.encounters[id as usize].id.contains("_WEAK"))
                    .collect();
            }
        } else {
            self.weak_encounters_left = 0;
            self.regular_encounters_left = self.regular_encounters_left.saturating_sub(1);
            if self.regular_encounters_left == 0 {
                self.encounters.clear();
            }
        }
        self.last_encounter = Some(id);
        if self.weak_encounters_left == 0 && self.regular_encounters_left == 0 {
            self.last_encounter = None;
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
    let mut game = game_from_snapshot(
        content,
        state,
        oracle,
        &action,
        state,
        oracle,
        &history,
        &replay_state,
        0,
    )?;
    game.replaying = false;
    game.encounters.clear();
    game.elites.clear();
    game.events.clear();
    Ok(game)
}

#[cfg(feature = "python")]
pub(crate) fn compare_live_transition(content: &Content, record: &Value) -> Vec<String> {
    let before = &record["before"];
    let mut replay_state = ReplayState::default();
    replay_state.prepare(content, record);
    let mut history = TurnHistory::default();
    if before["phase"] == "combat" {
        update_history(&mut history, before, &record["oracle_before"]);
    }
    let mut happy_flower = 0;
    let mut encounter = String::new();
    let mut round = 0;
    observe_happy_flower(before, &mut happy_flower, &mut encounter, &mut round);
    replay_decision(
        content,
        before,
        &record["oracle_before"],
        &record["action"],
        array(&record["choices"]),
        &record["after"],
        &record["oracle_after"],
        &history,
        &mut replay_state,
        happy_flower,
        false,
        record["metadata"]["settled_at"] == "combat_ended",
    )
    .err()
    .unwrap_or_default()
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
    // Out of combat, newer captures show TurnsSeen itself (3 while its energy flash plays).
    if state["phase"] != "combat"
        && player.get("cards_played_this_turn").is_some()
        && let Some(turns_seen) = relic["amount"].as_u64()
    {
        *charge = (turns_seen % 3) as u8;
        encounter.clear();
        *observed_round = 0;
        return;
    }
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
    // Newer captures record the relic's TurnsSeen as its amount (older ones its stack count).
    // In combat the relic is Active exactly at 2; a melted wax copy freezes and breaks that.
    if player.get("cards_played_this_turn").is_some()
        && let Some(turns_seen) = relic["amount"].as_u64()
        && (turns_seen == 2) == (relic["state"] == "Active")
    {
        *charge = (turns_seen % 3) as u8;
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
    let mut offered_discards = vec![];
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
            if record["kind"] == "choice_offered" {
                offered_discards.push(record["before"]["players"][0]["discard_pile"].clone());
            }
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
        replay_state.offered_discards = std::mem::take(&mut offered_discards);
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
    // An event option that started a fight is only recorded once that fight is over.
    if traced_action["kind"] == "event_option" && before["phase"] != "event" {
        return Ok(());
    }
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
    if traced_action["kind"] == "play_card"
        && let Some(source) = traced_action["source_id"].as_str()
        && let Some(target) = traced_action["target_id"].as_str()
        && let Some(traced_card) = array(&before["players"][0]["hand"])
            .iter()
            .find(|card| card["instance_id"] == source)
        && traced_card["model_id"] == "CARD.SOVEREIGN_BLADE"
        && !traced_card["sovereign_blade_bonus_damage"].is_number()
    {
        let before_hp = array(&before["combat"]["enemies"])
            .iter()
            .find(|enemy| enemy["instance_id"] == target)
            .and_then(|enemy| enemy["hit_points"].as_i64())
            .unwrap_or_default();
        let after_hp = array(&after["combat"]["enemies"])
            .iter()
            .find(|enemy| enemy["instance_id"] == target)
            .and_then(|enemy| enemy["hit_points"].as_i64())
            .unwrap_or_default();
        if let Phase::Combat(combat) = &mut game.phase
            && let Some(card) = combat
                .hand
                .iter_mut()
                .find(|card| card.instance == instance_id(source))
        {
            card.value = (before_hp - after_hp - 10).max(0) as i16;
        }
    }
    if traced_action["kind"] == "end_turn"
        && let Phase::Combat(combat) = &mut game.phase
    {
        for traced in array(&after["players"][0]["exhaust_pile"]) {
            if traced["ethereal"].is_boolean() {
                continue;
            }
            let instance = instance_id(traced["instance_id"].as_str().unwrap_or_default());
            if let Some(card) = combat
                .hand
                .iter_mut()
                .find(|card| card.instance == instance)
            {
                card.turn_flags |= ETHEREAL;
            }
        }
    }
    let card_reward = traced_action["kind"] == "take_reward"
        && traced_action["runtime_type"]
            .as_str()
            .is_some_and(|runtime| runtime.ends_with("CardReward"));
    let reward_choice = card_reward
        .then(|| {
            traced_choices
                .iter()
                .find(|choice| choice["action"]["kind"] == "choose_cards")
        })
        .flatten();
    if let Some(choice) = reward_choice {
        let cards: Vec<_> = array(&choice["action"]["candidate_ids"])
            .iter()
            .filter_map(|id| model_card(content, id.as_str()?))
            .collect();
        if !cards.is_empty()
            && let Phase::Rewards(rewards) = &mut game.phase
        {
            rewards.cards = cards;
        }
    }
    let action = if let Some(choice) = reward_choice {
        let selected = array(&choice["action"]["selected_ids"]);
        if selected.is_empty() {
            Action::Cancel
        } else {
            let selected = added_value(
                &before["players"][0]["deck"],
                &after["players"][0]["deck"],
                "instance_id",
            )
            .map(|value| card(content, value))
            .transpose()
            .map_err(|error| vec![error])?
            .or_else(|| model_card(content, selected[0].as_str().unwrap_or_default()))
            .ok_or_else(|| vec!["invalid card reward choice".into()])?;
            let Phase::Rewards(rewards) = &mut game.phase else {
                return Err(vec!["card reward did not reconstruct rewards".into()]);
            };
            let index = rewards
                .cards
                .iter()
                .position(|card| card.id == selected.id && card.upgrades == selected.upgrades)
                .ok_or_else(|| vec!["selected card reward is absent".into()])?;
            rewards.cards[index] = selected;
            Action::RewardCard(index)
        }
    } else {
        match action_from_snapshot(content, &game, traced_action, after) {
            Ok(action) => action,
            Err(error) => return Err(vec![error]),
        }
    };
    let player_state = |state: &Value| {
        let mut player = state["players"][0].clone();
        player
            .as_object_mut()
            .map(|player| player.remove("legal_actions"));
        player
    };
    let unchanged_shop =
        traced_action["kind"] == "shop_purchase" && player_state(before) == player_state(after);
    // A claimed reward can grant nothing (gold under Ectoplasm); stepping would leave the screen.
    let unchanged_reward = traced_action["kind"] == "take_reward"
        && before["phase"] == after["phase"]
        && player_state(before) == player_state(after);
    // Lord's Parasol buys the shop asynchronously, after the traced map_node state.
    let parasol = (traced_action["kind"] == "map_node" && after["phase"] == "shop")
        .then(|| {
            game.run
                .relics
                .iter()
                .position(|&id| content.relics[id as usize].id == "RELIC.LORDS_PARASOL")
        })
        .flatten()
        .map(|index| (index, game.run.relics.remove(index)));
    // Its purchases are then traced one by one, free of charge; The Courier's restocks still cost.
    let paid = before["players"][0]["gold"].as_i64() > after["players"][0]["gold"].as_i64();
    if traced_action["kind"] == "shop_purchase"
        && !paid
        && game
            .run
            .relics
            .iter()
            .any(|&id| content.relics[id as usize].id == "RELIC.LORDS_PARASOL")
        && let Phase::Shop(items) = &mut game.phase
    {
        for item in items {
            *price_mut(item) = 0;
        }
    }
    let stepped = if unchanged_shop || unchanged_reward {
        Ok(())
    } else {
        game.step(content, action)
    };
    if let Some((index, id)) = parasol {
        game.run.relics.insert(index, id);
    }
    if let Err(error) = stepped {
        return Err(vec![format!("action rejected: {error:?}")]);
    }
    // A shop pickup's last reward (or skipping the rest) closes its screen and completes the
    // purchase, so The Courier refills the slot; the rebuilt reward screen doesn't know when that
    // is, the trace does.
    if matches!(
        traced_action["kind"].as_str(),
        Some("take_reward" | "skip_rewards")
    ) && game.courier_restock.is_some()
        && array(&after["visible_offers"]).iter().any(|offer| {
            offer["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("shop:"))
        })
    {
        game.roll_courier_restock(content);
    }
    if matches!(
        traced_action["kind"].as_str(),
        Some("take_reward" | "skip_rewards")
    ) {
        let mut relics = game.run.relics.clone();
        for traced in array(&after["players"][0]["relics"]) {
            let Some(id) = traced["model_id"]
                .as_str()
                .and_then(|id| content.relic_id(id))
            else {
                continue;
            };
            if let Some(index) = relics.iter().position(|&current| current == id) {
                relics.remove(index);
            } else {
                game.obtain_relic(content, id);
            }
        }
        let mut deck: Vec<_> = game
            .run
            .deck
            .iter()
            .map(|card| (card.id, card.upgrades, card.enchantment))
            .collect();
        for traced in array(&after["players"][0]["deck"]) {
            let value = card(content, traced).map_err(|error| vec![error])?;
            let key = (value.id, value.upgrades, value.enchantment);
            if let Some(index) = deck.iter().position(|current| current == &key) {
                deck.remove(index);
            } else {
                game.add_card(content, value);
            }
        }
        let mut niche = Rng::from_seed(
            oracle_after["rng_seeds"]["run.Niche"]
                .as_u64()
                .unwrap_or_default(),
        );
        niche.forward(
            oracle_after["rng_counters"]["run.Niche"]
                .as_u64()
                .unwrap_or_default(),
        );
        game.rngs.niche = niche;
        game.run.hp = number(&after["players"][0], "hit_points").unwrap_or_default() as i16;
        game.run.max_hp = number(&after["players"][0], "max_hit_points").unwrap_or_default() as i16;
        game.run.gold = number(&after["players"][0], "gold").unwrap_or_default() as i32;
        sync_deck(content, &mut game, &after["players"][0]["deck"])?;
        game.run.relics = ids(&content.relics, &after["players"][0]["relics"], |relic| {
            relic.id
        })
        .map_err(|error| vec![error])?;
    }
    // An event fight runs on inside this decision; its own gold (Seal of Gold) may come after
    // the sync below, which already shows the fight's outcome.
    let event_fight = traced_action["kind"] == "event_option" && game.combat().is_some();
    const RIDERS: [&str; 9] = [
        "SAPPING",
        "VIOLENCE",
        "CHOKING",
        "ENERGIZED",
        "WISDOM",
        "CHAOS",
        "EXPERTISE",
        "CURIOUS",
        "IMPROVEMENT",
    ];
    if traced_action["kind"] == "event_option" {
        game.run.hp =
            number(&after["players"][0], "hit_points").map_err(|error| vec![error])? as i16;
        game.run.max_hp =
            number(&after["players"][0], "max_hit_points").map_err(|error| vec![error])? as i16;
        game.run.gold = number(&after["players"][0], "gold").map_err(|error| vec![error])? as i32;
        sync_deck(content, &mut game, &after["players"][0]["deck"])?;
        // Mad Science's rider comes from the chosen option.
        if let Some(rider) = traced_action["id"]
            .as_str()
            .and_then(|id| id.split("CHOOSE_RIDER.options.").nth(1))
            .and_then(|rider| RIDERS.iter().position(|name| *name == rider))
            && let Some(card) = game
                .run
                .deck
                .iter_mut()
                .rfind(|card| content.cards[card.id as usize].id == "CARD.MAD_SCIENCE")
        {
            card.variant = (rider / 3 + 1) as u8 * 10 + rider as u8 + 1;
        }
        game.run.relics = ids(&content.relics, &after["players"][0]["relics"], |relic| {
            relic.id
        })
        .map_err(|error| vec![error])?;
    }
    // An untraced pick from a potion's offer still generates a card (Regalite, ...); the hand
    // is resynced below, so any offered card will do.
    if traced_action["kind"] == "use_potion"
        && game.combat().is_some_and(|combat| {
            combat
                .choice
                .is_some_and(|choice| choice.op == CardOp::TakeOffer)
                && !combat.offer.is_empty()
        })
    {
        game.step(content, Action::Choose(0))
            .map_err(|error| vec![format!("potion offer pick rejected: {error:?}")])?;
    }
    // Discard, exhaust, transform, put-on-top and auto-play picks (Gambler's Brew, an auto-played
    // Prepared, Ashwater, Charge, Headbutt, Decisions, Decisions) are inferred below so their
    // triggers (Tough Bandages, Feel No Pain, Regalite) fire.
    let inferred = |op: CardOp| {
        matches!(
            op,
            CardOp::DiscardDraw
                | CardOp::Move(Pile::Discard | Pile::Exhaust | Pile::Draw)
                | CardOp::Transform(..)
                | CardOp::AutoPlay(_)
        )
    };
    if traced_action["kind"] == "use_potion"
        && game
            .combat()
            .is_some_and(|combat| !combat.choice.is_some_and(|choice| inferred(choice.op)))
    {
        // Effects queued behind skipped picks (Reptile Trinket) still resolve; piles are resynced next.
        while let Phase::Combat(combat) = &mut game.phase {
            // AutoSlay's selector takes as many cards as allowed (Dredge under Distilled Chaos),
            // which later picks from that pile can see (Drain Power).
            if combat.choice.is_some_and(|choice| {
                choice.op == CardOp::Move(Pile::Hand) && choice.pile == Pile::Discard
            }) {
                while game.combat().is_some_and(|combat| {
                    combat.choice.is_some_and(|choice| {
                        choice.op == CardOp::Move(Pile::Hand) && choice.pile == Pile::Discard
                    })
                }) && game.step(content, Action::Choose(0)).is_ok()
                {}
                let Phase::Combat(combat) = &mut game.phase else {
                    break;
                };
                if combat
                    .choice
                    .is_some_and(|choice| choice.op == CardOp::Move(Pile::Hand))
                {
                    combat.choice = None;
                }
                game.resolve(content);
            } else if combat
                .choice
                .is_some_and(|choice| matches!(choice.op, CardOp::CopyNextTurn(_)))
                && !combat.hand.is_empty()
            {
                // Nightmare still becomes a power; its card is recovered from next turn's copies.
                game.step(content, Action::Choose(0))
                    .map_err(|error| vec![format!("potion Nightmare pick rejected: {error:?}")])?;
            } else {
                combat.choice = None;
                game.resolve(content);
            }
            if !game
                .combat()
                .is_some_and(|combat| combat.choice.is_some_and(|choice| !inferred(choice.op)))
            {
                break;
            }
        }
        game.finish_combat(content);
    }
    if traced_action["kind"] == "use_potion"
        && let Phase::Combat(combat) = &mut game.phase
        && !combat.choice.is_some_and(|choice| inferred(choice.op))
    {
        combat.hand = hand_cards(content, &after["players"][0]).map_err(|error| vec![error])?;
        combat.discard =
            cards(content, &after["players"][0]["discard_pile"]).map_err(|error| vec![error])?;
        combat.exhaust =
            cards(content, &after["players"][0]["exhaust_pile"]).map_err(|error| vec![error])?;
        combat.draw = cards(content, &oracle_after["hidden_draw_order"])
            .map_err(|error| vec![error])?
            .into_iter()
            .rev()
            .collect();
        combat.choice = None;
        let target = oracle_after["rng_counters"]["run.Shuffle"]
            .as_u64()
            .unwrap_or(game.rngs.shuffle.1);
        if target > game.rngs.shuffle.1 {
            game.rngs.shuffle.forward(target - game.rngs.shuffle.1);
        }
    }
    if traced_action["kind"] == "play_card"
        && array(&before["players"][0]["hand"]).iter().any(|card| {
            card["instance_id"] == traced_action["source_id"]
                && matches!(
                    card["model_id"].as_str(),
                    Some("CARD.MURDER" | "CARD.SOVEREIGN_BLADE")
                )
        })
        && let Some(target) = traced_action["target_id"].as_str()
        && let Some(expected) = array(&after["combat"]["enemies"])
            .iter()
            .find(|enemy| enemy["instance_id"] == target)
        && let Phase::Combat(combat) = &mut game.phase
        && let Some(enemy) = combat
            .enemies
            .iter_mut()
            .find(|enemy| enemy.instance == instance_id(target))
    {
        enemy.creature.hp = number(expected, "hit_points")
            .unwrap_or(enemy.creature.hp as i64)
            .min(i16::MAX as i64) as i16;
    }
    // A fight already over at the next decision has its piles and enemies cleared, so the
    // simulated combat is kept instead.
    if traced_action["kind"] == "event_option" && after["phase"] == "combat" && !combat_ended {
        let expected = game_from_snapshot(
            content,
            after,
            oracle_after,
            traced_action,
            after,
            oracle_after,
            history,
            replay_state,
            happy_flower,
        )
        .map_err(|error| vec![error])?;
        game.phase = expected.phase;
        // The combat comes from the capture, so its turn-1 rolls (untraced Toolbox pick,
        // auto-plays) do too.
        game.rngs.combat_card_generation = expected.rngs.combat_card_generation;
        game.rngs.combat_card_selection = expected.rngs.combat_card_selection;
        game.rngs.combat_energy_costs = expected.rngs.combat_energy_costs;
        game.rngs.combat_orb_generation = expected.rngs.combat_orb_generation;
        game.rngs.combat_potion_generation = expected.rngs.combat_potion_generation;
        game.rngs.combat_targets = expected.rngs.combat_targets;
        game.rngs.monster_ai = expected.rngs.monster_ai;
        game.rngs.shuffle = expected.rngs.shuffle;
    }
    if traced_action["kind"] == "play_card"
        && array(&before["players"][0]["hand"]).iter().any(|card| {
            card["instance_id"] == traced_action["source_id"]
                && card["enchantment_model_id"] == "ENCHANTMENT.SOWN"
        })
        && let Phase::Combat(combat) = &mut game.phase
    {
        combat.energy =
            number(&after["players"][0], "energy").unwrap_or(combat.energy as i64) as i16;
    }
    if before["phase"] != "combat"
        && !matches!(
            traced_action["kind"].as_str(),
            Some("event_option" | "take_reward" | "skip_rewards")
        )
        && let Phase::ChooseBundles(bundles) = &game.phase
    {
        let mut before_cards: Vec<_> = array(&before["players"][0]["deck"])
            .iter()
            .filter_map(|card| {
                Some((
                    card["model_id"].as_str()?,
                    card["upgrade"].as_u64().unwrap_or_default() as u8,
                ))
            })
            .collect();
        let mut added = Vec::new();
        let mut selected = Vec::new();
        for traced in array(&after["players"][0]["deck"]) {
            let value = (
                traced["model_id"].as_str().unwrap_or_default(),
                traced["upgrade"].as_u64().unwrap_or_default() as u8,
            );
            if let Some(index) = before_cards
                .iter()
                .position(|candidate| candidate == &value)
            {
                before_cards.remove(index);
            } else {
                added.push(value);
                selected.push(card(content, traced).map_err(|error| vec![error])?);
            }
        }
        let index = bundles.iter().position(|bundle| {
            let mut expected = added.clone();
            bundle.iter().all(|card| {
                let value = (content.cards[card.id as usize].id, card.upgrades);
                expected
                    .iter()
                    .position(|candidate| candidate == &value)
                    .is_some_and(|index| {
                        expected.remove(index);
                        true
                    })
            }) && expected.is_empty()
        });
        let index = if let Some(index) = index {
            index
        } else {
            game.phase = Phase::ChooseBundles(vec![selected]);
            0
        };
        if !added.is_empty() {
            game.step(content, Action::Choose(index))
                .map_err(|error| vec![format!("inferred bundle rejected: {error:?}")])?;
        }
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
    let mut inferred_choice = false;
    // A fight won without any decision (event fights) settles past the combat entirely.
    let fight_over = combat_ended || after["phase"] != "combat";
    while game.combat().is_some() {
        let Some(choice) = game.combat().and_then(|combat| combat.choice) else {
            break;
        };
        inferred_choice = true;
        // Nightmare's pick only shows next turn, so any eligible card reproduces this step.
        if let CardOp::CopyNextTurn(_) = choice.op {
            let index = game.combat().and_then(|combat| {
                replay_pile(combat, choice.pile)
                    .iter()
                    .position(|card| eligible(content, &card, choice.filter, choice.op))
            });
            let action = index.map_or(Action::Done, Action::Choose);
            game.step(content, action)
                .map_err(|error| vec![format!("inferred choice rejected: {error:?}")])?;
            continue;
        }
        let destination = match choice.op {
            CardOp::Move(pile) | CardOp::MoveFree(pile) => pile,
            CardOp::TakeOffer | CardOp::TakeFetched | CardOp::CopySelected(_) => Pile::Hand,
            CardOp::Upgrade | CardOp::Flag(_) | CardOp::TurnFlag(_) | CardOp::Transfigure => {
                choice.pile
            }
            CardOp::Transform(..) | CardOp::TransformRandom | CardOp::AutoPlay(_) => choice.pile,
            CardOp::DiscardDraw => Pile::Hand,
            _ => break,
        };
        let flag = match choice.op {
            CardOp::Flag(flag) | CardOp::TurnFlag(flag) => Some(flag),
            _ => None,
        };
        let expected = match destination {
            Pile::Draw => array(&oracle_after["hidden_draw_order"]),
            Pile::Hand => array(&after["players"][0]["hand"]),
            Pile::Discard => array(&after["players"][0]["discard_pile"]),
            Pile::Exhaust => array(&after["players"][0]["exhaust_pile"]),
            Pile::Offer => &[],
        };
        let source_expected = match choice.pile {
            Pile::Draw => array(&oracle_after["hidden_draw_order"]),
            Pile::Hand => array(&after["players"][0]["hand"]),
            Pile::Discard => array(&after["players"][0]["discard_pile"]),
            Pile::Exhaust => array(&after["players"][0]["exhaust_pile"]),
            Pile::Offer => &[],
        };
        let mut index = game.combat().and_then(|combat| {
            let source = replay_pile(combat, choice.pile);
            // The Knowledge Demon's curse becomes a power whose amount grows.
            if choice.pile == Pile::Offer && combat.enemy_turn {
                let gained = |card: &Card| {
                    let name = content.cards[card.id as usize]
                        .id
                        .replacen("CARD.", "POWER.", 1)
                        + "_POWER";
                    let after = array(&after["players"][0]["powers"])
                        .iter()
                        .find(|power| power["model_id"] == name.as_str())
                        .and_then(|power| power["amount"].as_i64())
                        .unwrap_or_default();
                    let before = content
                        .powers
                        .iter()
                        .position(|power| power.id == name)
                        .map_or(0, |id| combat.player.power(id as Id));
                    after > before as i64
                };
                if let Some(index) = source.iter().position(gained) {
                    return Some(index);
                }
            }
            if choice.op == CardOp::DiscardDraw {
                // AutoSlay's selector takes min(max, count) cards, so Gambler's Brew discards the
                // whole hand (some may be redrawn after a reshuffle), in its shuffled order. The
                // traced discard pile keeps that order, except for cards a later effect took from
                // its front (Graveblast under Whispering Earring) and put back on top. Turn-1
                // instance ids can differ from the game's, so cards match by model.
                let player = &after["players"][0];
                let discard = array(&player["discard_pile"]);
                let same = |card: &Card, value: &Value| {
                    value["model_id"] == content.cards[card.id as usize].id
                        && value["upgrade"].as_u64().unwrap_or_default() == card.upgrades as u64
                };
                // A card played during this step came back from elsewhere (a second Cosmic
                // Indifference), even when it matches a discarded card's model.
                let played_before = array(&before["players"][0]["cards_played_this_turn_ids"]);
                let played_ids = array(&player["cards_played_this_turn_ids"]);
                let played_now = |value: &Value| {
                    (played_ids.iter())
                        .any(|id| *id == value["instance_id"] && !played_before.contains(id))
                };
                // A later choice saw the discard pile with the next discarded card on top.
                if let Some(next) = replay_state
                    .offered_discards
                    .iter()
                    .map(array)
                    .filter(|seen| {
                        seen.len() > combat.discard.len()
                            && combat
                                .discard
                                .iter()
                                .zip(*seen)
                                .all(|(card, value)| same(card, value))
                    })
                    .find_map(|seen| {
                        let value = &seen[combat.discard.len()];
                        let instance =
                            instance_id(value["instance_id"].as_str().unwrap_or_default());
                        // By instance when the ids agree, else by model, preferring the same
                        // cost (Jeweled Mask's free Royalties).
                        let shown = card(content, value).ok();
                        source
                            .iter()
                            .position(|card| card.instance == instance && same(card, value))
                            .or_else(|| {
                                source.iter().position(|card| {
                                    same(card, value)
                                        && shown.is_some_and(|shown| {
                                            (shown.free, shown.cost_override)
                                                == (card.free, card.cost_override)
                                        })
                                })
                            })
                            .or_else(|| source.iter().position(|card| same(card, value)))
                    })
                {
                    return Some(next);
                }
                // The pile keeps the discard order up to its first card from elsewhere.
                let mut pool: Vec<&Card> = source.iter().chain(&combat.discard).collect();
                let frontier = discard
                    .iter()
                    .position(|value| {
                        match pool
                            .iter()
                            .position(|card| same(card, value))
                            .filter(|_| !played_now(value))
                        {
                            Some(at) => {
                                pool.swap_remove(at);
                                false
                            }
                            None => true,
                        }
                    })
                    .unwrap_or(discard.len());
                let piles: Vec<&Value> = ["hand", "discard_pile", "exhaust_pile"]
                    .iter()
                    .flat_map(|pile| array(&player[*pile]))
                    .collect();
                let played: Vec<&Value> = played_ids
                    .iter()
                    .filter_map(|id| {
                        piles
                            .iter()
                            .copied()
                            .find(|value| value["instance_id"] == *id)
                    })
                    .collect();
                // Older captures lack the play list; there a card past the frontier was taken.
                let has_plays = player["cards_played_this_turn_ids"].is_array();
                let order = |card: &Card| {
                    // By instance when the ids agree (powers leave no pile to read the model from).
                    let played_at = played_ids
                        .iter()
                        .position(|id| {
                            instance_id(id.as_str().unwrap_or_default()) == card.instance
                        })
                        .or_else(|| played.iter().position(|value| same(card, value)));
                    // Sly cards play themselves in discard order and return on top.
                    if card.flags(content.cards[card.id as usize]) & SLY != 0 {
                        return played_at.map_or(usize::MAX, |at| 2 * discard.len() + 1 + at);
                    }
                    let taken_rank = played_at.unwrap_or(played.len());
                    // Positions past the frontier only count for copies the prefix misses.
                    let copies = |cards: &mut dyn Iterator<Item = &Card>| {
                        cards
                            .filter(|other| other.id == card.id && other.upgrades == card.upgrades)
                            .count()
                    };
                    let held = copies(&mut source.iter().chain(&combat.discard));
                    let prefix = (0..frontier).filter(|&at| same(card, &discard[at])).count();
                    let mut ranks: Vec<usize> = (0..discard.len())
                        .filter(|&at| same(card, &discard[at]) && (at < frontier || prefix < held))
                        .map(|at| {
                            let taken = at >= frontier && (played_at.is_some() || !has_plays);
                            if taken {
                                taken_rank
                            } else {
                                played.len() + 1 + at
                            }
                        })
                        .collect();
                    ranks.sort_unstable();
                    // Copies already discarded took the earliest of these positions.
                    ranks
                        .get(copies(&mut combat.discard.iter()))
                        .copied()
                        .unwrap_or(taken_rank)
                };
                return (0..source.len()).min_by_key(|&i| order(&source[i]));
            }
            if choice.op == CardOp::Upgrade {
                return source.iter().position(|card| {
                    expected.iter().any(|value| {
                        card.instance
                            == instance_id(value["instance_id"].as_str().unwrap_or_default())
                            && card.upgrades != value["upgrade"].as_u64().unwrap_or_default() as u8
                    })
                });
            }
            if choice.op == CardOp::Transfigure {
                return source.iter().position(|card| {
                    expected.iter().any(|value| {
                        card.instance
                            == instance_id(value["instance_id"].as_str().unwrap_or_default())
                            && card.replays
                                != value["base_replay_count"].as_u64().unwrap_or_default() as u8
                    })
                });
            }
            if let Some(flag) = flag {
                let find = |value: &Value| {
                    let instance = instance_id(value["instance_id"].as_str().unwrap_or_default());
                    source.iter().position(|card| {
                        card.instance == instance
                            && card.flags(content.cards[card.id as usize]) & flag == 0
                            && card.turn_flags & flag == 0
                            && eligible(content, &card, choice.filter, choice.op)
                    })
                };
                // AutoSlay always retains; a retained card that a turn-start discard (Tools of
                // the Trade) removed is the last one in the discard pile.
                return expected
                    .iter()
                    .find_map(find)
                    .or_else(|| {
                        // The marked card may already have been played on (Whispering Earring).
                        let marker = match flag {
                            ETHEREAL => "ethereal",
                            RETAIN => "retain",
                            _ => return None,
                        };
                        let marked: Vec<_> = [
                            &after["players"][0]["discard_pile"],
                            &after["players"][0]["exhaust_pile"],
                            &oracle_after["hidden_draw_order"],
                        ]
                        .into_iter()
                        .flat_map(array)
                        .filter(|value| value[marker] == true)
                        .collect();
                        marked.iter().copied().find_map(find).or_else(|| {
                            // Turn-1 instance ids can differ; fall back to the model.
                            marked.iter().find_map(|value| {
                                source.iter().position(|card| {
                                    value["model_id"] == content.cards[card.id as usize].id
                                        && value["upgrade"].as_u64() == Some(card.upgrades as u64)
                                        && card.flags(content.cards[card.id as usize]) & flag == 0
                                        && card.turn_flags & flag == 0
                                        && eligible(content, &card, choice.filter, choice.op)
                                })
                            })
                        })
                    })
                    .or_else(|| {
                        (flag == RETAIN && combat.player.power(power_id::TOOLS_OF_THE_TRADE) > 0)
                            .then(|| {
                                array(&after["players"][0]["discard_pile"])
                                    .iter()
                                    .rev()
                                    .find_map(find)
                            })
                            .flatten()
                    })
                    .or_else(|| {
                        // A retained card that Entropy then transformed is in no pile afterwards.
                        (flag == RETAIN && combat.player.power(power_id::ENTROPY) > 0).then(
                            || {
                                source.iter().position(|card| {
                                    eligible(content, &card, choice.filter, choice.op)
                                        && ![
                                            &after["players"][0]["hand"],
                                            &after["players"][0]["discard_pile"],
                                            &after["players"][0]["exhaust_pile"],
                                            &oracle_after["hidden_draw_order"],
                                        ]
                                        .into_iter()
                                        .flat_map(array)
                                        .any(|value| {
                                            card.instance
                                                == instance_id(
                                                    value["instance_id"]
                                                        .as_str()
                                                        .unwrap_or_default(),
                                                )
                                        })
                                })
                            },
                        )?
                    });
            }
            // AutoSlay picks draw-pile cards at random: prefer one fetched to hand, then exhausted
            // the same step (Foregone Conclusion, then Tyranny).
            if choice.pile == Pile::Draw
                && destination == Pile::Hand
                && combat.player.power(power_id::TYRANNY) > 0
            {
                let instances = |state: &Value, key: &str| -> Vec<u32> {
                    array(&state["players"][0][key])
                        .iter()
                        .map(|value| instance_id(value["instance_id"].as_str().unwrap_or_default()))
                        .collect()
                };
                let spent_after = instances(after, "exhaust_pile");
                let spent_before = instances(before, "exhaust_pile");
                // The hand lists the fetched cards before the drawn ones; a card the hand draw
                // reaches past the cards fetched first was drawn.
                let taken: Vec<u32> = instances(after, "hand")
                    .into_iter()
                    .filter(|&instance| source.iter().any(|card| card.instance == instance))
                    .collect();
                let fetched = &taken[..taken.len().min(choice.remaining as usize)];
                let drawn: Vec<u32> = source
                    .iter()
                    .rev()
                    .map(|card| card.instance)
                    .filter(|instance| !fetched.contains(instance))
                    .take(taken.len() - fetched.len() + 1)
                    .collect();
                if let Some(index) = source.iter().position(|card| {
                    !drawn.contains(&card.instance)
                        && eligible(content, &card, choice.filter, choice.op)
                        && spent_after.contains(&card.instance)
                        && !spent_before.contains(&card.instance)
                        && !instances(before, "hand").contains(&card.instance)
                }) {
                    return Some(index);
                }
            }
            expected
                .iter()
                .find_map(|value| {
                    let instance = instance_id(value["instance_id"].as_str().unwrap_or_default());
                    (!replay_pile(combat, destination)
                        .iter()
                        .any(|card| card.instance == instance))
                    .then(|| {
                        let model = value["model_id"]
                            .as_str()
                            .and_then(|id| content.card_id(id));
                        let upgrades = value["upgrade"].as_u64().unwrap_or_default() as u8;
                        // An offered card must be one the game ends up with an extra copy of.
                        let extra = |id: Id| {
                            let player = &after["players"][0];
                            let traced = [
                                &player["hand"],
                                &player["discard_pile"],
                                &player["exhaust_pile"],
                                &player["play_pile"],
                                &oracle_after["hidden_draw_order"],
                            ]
                            .into_iter()
                            .flat_map(array)
                            .filter(|value| value["model_id"] == content.cards[id as usize].id)
                            .count();
                            let simulated = [&combat.hand, &combat.discard, &combat.exhaust]
                                .into_iter()
                                .chain([&combat.draw])
                                .flat_map(|pile| pile.iter())
                                .filter(|card| card.id == id)
                                .count();
                            traced > simulated
                        };
                        (choice.pile != Pile::Offer)
                            .then(|| source.iter().position(|card| card.instance == instance))
                            .flatten()
                            .or_else(|| {
                                source.iter().position(|card| {
                                    model == Some(card.id)
                                        && card.upgrades == upgrades
                                        && (choice.pile != Pile::Offer || extra(card.id))
                                })
                            })
                    })
                    .flatten()
                })
                .or_else(|| {
                    (choice.pile != Pile::Offer).then(|| {
                        let gone = |card: &Card| {
                            !source_expected.iter().any(|value| {
                                card.instance
                                    == instance_id(
                                        value["instance_id"].as_str().unwrap_or_default(),
                                    )
                            })
                        };
                        // Prefer a card the game already had that vanished from every pile (it was
                        // transformed); a card generated this step may carry another instance id.
                        let found = |piles: [&Value; 4], card: &Card| {
                            piles.into_iter().flat_map(array).any(|value| {
                                card.instance
                                    == instance_id(
                                        value["instance_id"].as_str().unwrap_or_default(),
                                    )
                            })
                        };
                        fn piles<'a>(state: &'a Value, oracle: &'a Value) -> [&'a Value; 4] {
                            [
                                &state["players"][0]["hand"],
                                &state["players"][0]["discard_pile"],
                                &state["players"][0]["exhaust_pile"],
                                &oracle["hidden_draw_order"],
                            ]
                        }
                        source
                            .iter()
                            .position(|card| {
                                gone(card)
                                    && found(piles(before, oracle_before), card)
                                    && !found(piles(after, oracle_after), card)
                            })
                            .or_else(|| source.iter().position(gone))
                    })?
                })
        });
        // A card put on top of the draw pile and drawn again reappears at the end of the hand.
        if index.is_none()
            && choice.pile == Pile::Hand
            && choice.op == CardOp::Move(Pile::Draw)
            && let Some(last) = array(&after["players"][0]["hand"]).last()
        {
            let instance = instance_id(last["instance_id"].as_str().unwrap_or_default());
            index = game.combat().and_then(|combat| {
                combat
                    .hand
                    .iter()
                    .position(|card| card.instance == instance)
            });
        }
        // A taken offer may already have been played on (Whispering Earring): pick the offered
        // card the game has more copies of than the simulator, the earliest generated one if
        // several (Infernal Blade's card comes after the turn-start offer).
        if index.is_none() && choice.pile == Pile::Offer && choice.op == CardOp::TakeOffer {
            let piles = ["hand", "discard_pile", "exhaust_pile"]
                .map(|pile| array(&after["players"][0][pile]))
                .into_iter()
                .chain([array(&oracle_after["hidden_draw_order"])])
                .collect::<Vec<_>>();
            index = game.combat().and_then(|combat| {
                let simulated: Vec<&Card> = [Pile::Hand, Pile::Discard, Pile::Exhaust, Pile::Draw]
                    .into_iter()
                    .flat_map(|pile| replay_pile(combat, pile))
                    .collect();
                (0..combat.offer.len())
                    .filter_map(|i| {
                        let offered = combat.offer[i].id;
                        let mut traced: Vec<u32> = piles
                            .iter()
                            .flat_map(|pile| pile.iter())
                            .filter(|value| value["model_id"] == content.cards[offered as usize].id)
                            .map(|value| {
                                instance_id(value["instance_id"].as_str().unwrap_or_default())
                            })
                            .collect();
                        // The extra copies are the newest instances.
                        let held = simulated.iter().filter(|card| card.id == offered).count();
                        traced.sort_unstable();
                        (traced.len() > held).then(|| (traced[held], i))
                    })
                    .min()
                    .map(|(_, i)| i)
            });
        }
        // A taken power card that was already played leaves no copy, only its power.
        if index.is_none() && choice.pile == Pile::Offer && choice.op == CardOp::TakeOffer {
            let powers = array(&after["players"][0]["powers"]);
            index = game.combat().and_then(|combat| {
                combat.offer.iter().position(|offered| {
                    let def = content.cards[offered.id as usize];
                    let power = format!("POWER.{}_POWER", def.id.trim_start_matches("CARD."));
                    def.card_type == CardType::Power
                        && powers
                            .iter()
                            .any(|value| value["model_id"] == power.as_str())
                        && !combat
                            .player
                            .powers
                            .iter()
                            .any(|x| content.powers[x.id as usize].id == power)
                })
            });
        }
        // Otherwise try each offered card and keep the one whose outcome matches the traced
        // player powers and energy (a played power card like Prowess leaves no copy).
        if index.is_none() && choice.pile == Pile::Offer && choice.op == CardOp::TakeOffer {
            let player = &after["players"][0];
            let count = game.combat().map_or(0, |combat| combat.offer.len());
            index = (0..count).find(|&i| {
                let mut trial = game.clone();
                if trial.step(content, Action::Choose(i)).is_err() {
                    return false;
                }
                trial.combat().is_some_and(|combat| {
                    let mut diff = Vec::new();
                    compare_powers(
                        &mut diff,
                        content,
                        "",
                        &combat.player.powers,
                        &player["powers"],
                    );
                    diff.is_empty() && player["energy"].as_i64() == Some(combat.energy as i64)
                })
            });
        }
        // The profile's unlocked pool can differ from the simulator's (Toolbox): take the card
        // the game has one more copy of than the simulator, even if it wasn't offered here.
        if index.is_none()
            && choice.pile == Pile::Offer
            && choice.op == CardOp::TakeOffer
            && let Some(combat) = game.combat()
        {
            let player = &after["players"][0];
            let traced: Vec<&Value> = [
                &player["hand"],
                &player["discard_pile"],
                &player["exhaust_pile"],
                &player["play_pile"],
                &oracle_after["hidden_draw_order"],
            ]
            .into_iter()
            .flat_map(|pile| array(pile))
            .collect();
            let simulated = |id: &Value| {
                [&combat.hand, &combat.discard, &combat.exhaust, &combat.draw]
                    .iter()
                    .flat_map(|pile| pile.iter())
                    .filter(|card| content.cards[card.id as usize].id == id)
                    .count()
            };
            if let Some(value) = traced.iter().find(|value| {
                traced
                    .iter()
                    .filter(|other| other["model_id"] == value["model_id"])
                    .count()
                    == simulated(&value["model_id"]) + 1
            }) {
                let selected = card(content, value).map_err(|error| vec![error])?;
                if let Phase::Combat(combat) = &mut game.phase {
                    combat.offer = vec![selected];
                    index = Some(0);
                }
            }
        }
        if index.is_none()
            && choice.pile == Pile::Offer
            && choice.op == CardOp::TakeOffer
            && let Some(value) = expected.iter().find(|value| {
                let instance = instance_id(value["instance_id"].as_str().unwrap_or_default());
                game.combat().is_some_and(|combat| {
                    !replay_pile(combat, destination)
                        .iter()
                        .any(|card| card.instance == instance)
                })
            })
        {
            let selected = card(content, value).map_err(|error| vec![error])?;
            if let Phase::Combat(combat) = &mut game.phase {
                combat.offer = vec![selected];
                index = Some(0);
            }
        }
        // The offered card may already have left the hand (played by Whispering Earring), so
        // look for an offered model that gained a copy anywhere in the piles.
        if index.is_none()
            && choice.op == CardOp::TakeOffer
            && let Some(combat) = game.combat()
        {
            let count = |id: Id, piles: &[&Value]| {
                piles
                    .iter()
                    .flat_map(|pile| array(pile))
                    .filter(|value| value["model_id"] == content.cards[id as usize].id)
                    .count()
            };
            let player = &after["players"][0];
            index = combat.offer.iter().position(|offered| {
                let sim = [&combat.hand, &combat.discard, &combat.exhaust, &combat.draw]
                    .iter()
                    .flat_map(|pile| pile.iter())
                    .filter(|card| card.id == offered.id)
                    .count();
                count(
                    offered.id,
                    &[
                        &player["hand"],
                        &player["discard_pile"],
                        &player["exhaust_pile"],
                        &player["play_pile"],
                        &oracle_after["hidden_draw_order"],
                    ],
                ) > sim
            });
        }
        if choice.op == CardOp::TakeFetched
            && let Some(index) = index
            && let Phase::Combat(combat) = &mut game.phase
        {
            combat.draw[index].turn_flags |= FETCHED;
        }
        // Once combat ended the traced piles are cleared; prefer a choice that ends the fight,
        // then one whose rng use matches the game (a discarded Sly Ricochet rolls targets).
        let ends = |i: usize| {
            let mut trial = game.clone();
            (trial.step(content, Action::Choose(i)).is_ok() && trial.combat().is_none()).then(
                || {
                    compare_snapshot(content, &trial, after, oracle_after, false)
                        .iter()
                        .filter(|line| line.starts_with("run.") || line.starts_with("player.1."))
                        .count()
                },
            )
        };
        if fight_over
            && index.and_then(ends) != Some(0)
            && let Some(count) = game
                .combat()
                .map(|combat| replay_pile(combat, choice.pile).len())
            && let Some((winner, misses)) = (0..count)
                .filter_map(|i| Some((i, ends(i)?)))
                .min_by_key(|&(_, misses)| misses)
            && index.and_then(ends).is_none_or(|current| misses < current)
        {
            index = Some(winner);
        }
        // With the piles cleared there is no evidence left; any pick keeps the replay going.
        if fight_over
            && index.is_none()
            && game
                .combat()
                .is_some_and(|combat| !replay_pile(combat, choice.pile).is_empty())
        {
            index = Some(0);
        }
        // No pile evidence (the pick was played right away, e.g. Graveblast under Beat Down): take
        // the pick that best matches the game.
        if index.is_none()
            && !fight_over
            && let Some(count) = game
                .combat()
                .map(|combat| replay_pile(combat, choice.pile).len())
        {
            index = (0..count)
                .filter_map(|i| {
                    let mut trial = game.clone();
                    trial.step(content, Action::Choose(i)).ok()?;
                    Some((
                        i,
                        compare_snapshot(content, &trial, after, oracle_after, false).len(),
                    ))
                })
                .min_by_key(|&(_, misses)| misses)
                .map(|(i, _)| i);
        }
        // Several cards can fit the pile evidence (Decisions, Decisions' skill left the hand along
        // with the card its True Grit exhausted): prefer the pick whose rng use matches the game.
        if let Some(current) = index
            && choice.pile != Pile::Offer
        {
            let score = |trial: &Game| {
                let diff = compare_snapshot(content, trial, after, oracle_after, false);
                // Then prefer a pick that ends the choices with the exact traced state.
                let pending = trial.combat().is_some_and(|combat| combat.choice.is_some());
                // Mid-sequence counters only lag the traced ones; only an overshoot rules a pick out.
                let overshoot = |line: &&String| {
                    let number = |key: &str| {
                        line.split(key)
                            .nth(1)?
                            .split_whitespace()
                            .next()?
                            .parse::<i64>()
                            .ok()
                    };
                    number("simulator ") > number("game ")
                };
                let rng = diff
                    .iter()
                    .filter(|line| line.starts_with("run.") || line.starts_with("player.1."))
                    .filter(|line| !pending || overshoot(line))
                    .count();
                (rng, (pending || !diff.is_empty()) as usize)
            };
            let misses = |i: usize| {
                let mut trial = game.clone();
                if trial.step(content, Action::Choose(i)).is_err() {
                    return (usize::MAX, usize::MAX);
                }
                // A pick that opens another choice (a discarded Sly Survivor discards in turn)
                // scores by its best follow-up pick.
                let follow_ups = trial
                    .combat()
                    .filter(|combat| combat.choice.is_some())
                    .map_or(0, |combat| {
                        replay_pile(combat, combat.choice.unwrap().pile).len()
                    });
                (0..follow_ups)
                    .filter_map(|j| {
                        let mut next = trial.clone();
                        next.step(content, Action::Choose(j)).ok()?;
                        Some(score(&next))
                    })
                    .min()
                    .unwrap_or_else(|| score(&trial))
            };
            let current_misses = misses(current);
            let count = game
                .combat()
                .map_or(0, |combat| replay_pile(combat, choice.pile).len());
            if current_misses != (0, 0)
                && let Some((better, _)) = (0..count)
                    .map(|i| (i, misses(i)))
                    .filter(|&(_, m)| m < current_misses)
                    .min_by_key(|&(_, m)| m)
            {
                index = Some(better);
            }
        }
        // Only eligible cards can be picked (a Shiv retained by Phantom Blades isn't offered to
        // Well-Laid Plans); with no evidence left, take the first eligible one.
        let pickable = |i: usize| {
            game.combat().is_some_and(|combat| {
                replay_pile(combat, choice.pile)
                    .get(i)
                    .is_some_and(|card| eligible(content, &card, choice.filter, choice.op))
            })
        };
        if index.is_some_and(|i| !pickable(i)) {
            let count = game
                .combat()
                .map_or(0, |combat| replay_pile(combat, choice.pile).len());
            index = (0..count).find(|&i| pickable(i));
        }
        if let Some(index) = index {
            game.step(content, Action::Choose(index))
                .map_err(|error| vec![format!("inferred choice rejected: {error:?}")])?;
        } else if choice.optional {
            game.step(content, Action::Done)
                .map_err(|error| vec![format!("inferred choice rejection: {error:?}")])?;
        } else {
            break;
        }
    }
    if inferred_choice && let Phase::Combat(combat) = &mut game.phase {
        combat.hand = hand_cards(content, &after["players"][0]).map_err(|error| vec![error])?;
        combat.discard =
            cards(content, &after["players"][0]["discard_pile"]).map_err(|error| vec![error])?;
        combat.draw = cards(content, &oracle_after["hidden_draw_order"])
            .map_err(|error| vec![error])?
            .into_iter()
            .rev()
            .collect();
        // The piles now come from the capture, so the shuffle rng follows them.
        let target = oracle_after["rng_counters"]["run.Shuffle"]
            .as_u64()
            .unwrap_or(game.rngs.shuffle.1);
        if target > game.rngs.shuffle.1 {
            game.rngs.shuffle.forward(target - game.rngs.shuffle.1);
        }
    }
    if array(&after["combat"]["enemies"]).iter().any(|enemy| {
        !array(&before["combat"]["enemies"])
            .iter()
            .any(|before| before["instance_id"] == enemy["instance_id"])
    }) {
        let expected = game_from_snapshot(
            content,
            after,
            oracle_after,
            traced_action,
            after,
            oracle_after,
            history,
            replay_state,
            happy_flower,
        )
        .map_err(|error| vec![error])?;
        if let (Phase::Combat(actual), Phase::Combat(expected)) = (&mut game.phase, expected.phase)
        {
            for enemy in expected.enemies.into_iter().filter(|enemy| {
                !array(&before["combat"]["enemies"]).iter().any(|before| {
                    enemy.instance
                        == instance_id(before["instance_id"].as_str().unwrap_or_default())
                })
            }) {
                if let Some(index) = actual
                    .enemies
                    .iter()
                    .position(|current| current.instance == enemy.instance)
                {
                    actual.enemies[index] = enemy;
                } else if let Some(index) = actual.enemies.iter().position(|current| {
                    !array(&before["combat"]["enemies"]).iter().any(|before| {
                        current.instance
                            == instance_id(before["instance_id"].as_str().unwrap_or_default())
                    })
                }) {
                    actual.enemies[index] = enemy;
                } else {
                    actual.enemies.push(enemy);
                }
            }
        }
    }
    if traced_action["kind"] == "end_turn"
        && let Phase::Combat(combat) = &mut game.phase
    {
        for enemy in &mut combat.enemies {
            let instance = format!("creature:{}", enemy.instance.saturating_sub(1));
            let move_id = oracle_after["enemy_move_ids"][&instance]
                .as_str()
                .unwrap_or_default();
            enemy.stunned = move_id == "STUNNED";
            if let Some(expected) = array(&after["combat"]["enemies"]).iter().find(|expected| {
                expected["instance_id"] == instance.as_str()
                    && expected["hit_points"].as_i64().unwrap_or_default() > 0
            }) {
                enemy.move_index = infer_move(
                    &content.enemies[enemy.creature.id as usize],
                    expected,
                    move_id,
                );
            }
        }
        if array(&before["players"][0]["powers"])
            .iter()
            .any(|power| power["model_id"] == "POWER.ENTROPY_POWER")
        {
            combat.hand = hand_cards(content, &after["players"][0]).map_err(|error| vec![error])?;
        }
    }
    'choices: for choice in traced_choices
        .iter()
        .filter(|_| before["phase"] != "combat")
        .filter(|choice| !card_reward || choice["action"]["kind"] != "choose_cards")
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
                // An event's or reward's after-state already holds the bundle and its pickup
                // triggers (Lucky Fysh), synced above; keep it rather than adding them twice.
                if matches!(
                    traced_action["kind"].as_str(),
                    Some("event_option" | "take_reward" | "skip_rewards")
                ) {
                    game.run.hp = number(&after["players"][0], "hit_points")
                        .map_err(|error| vec![error])? as i16;
                    game.run.max_hp = number(&after["players"][0], "max_hit_points")
                        .map_err(|error| vec![error])? as i16;
                    game.run.gold =
                        number(&after["players"][0], "gold").map_err(|error| vec![error])? as i32;
                    sync_deck(content, &mut game, &after["players"][0]["deck"])?;
                }
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
    while before["phase"] != "combat"
        && matches!(
            game.phase,
            Phase::RemoveCards(..)
                | Phase::TransformCards(..)
                | Phase::UpgradeCards(..)
                | Phase::EnchantCards(..)
        )
    {
        let mut expected: Vec<_> = array(&after["players"][0]["deck"])
            .iter()
            .filter_map(|card| {
                Some((
                    content.card_id(card["model_id"].as_str()?)?,
                    card["upgrade"].as_u64().unwrap_or_default() as u8,
                ))
            })
            .collect();
        let index =
            if matches!(game.phase, Phase::EnchantCards(..)) {
                game.run.deck.iter().enumerate().position(|(index, card)| {
                    card.enchantment.is_none()
                        && after["players"][0]["deck"][index]["enchantment_model_id"].is_string()
                })
            } else if matches!(game.phase, Phase::UpgradeCards(..)) {
                game.run.deck.iter().enumerate().position(|(index, card)| {
                    expected.get(index) != Some(&(card.id, card.upgrades))
                })
            } else if matches!(game.phase, Phase::RemoveCards(..)) {
                // Removal keeps the deck order: the first card out of step was removed.
                let mut remaining = array(&after["players"][0]["deck"]).iter().peekable();
                game.run.deck.iter().position(|card| {
                    remaining
                        .next_if(|kept| {
                            kept["model_id"] == content.cards[card.id as usize].id
                                && kept["upgrade"] == card.upgrades
                                && kept["enchantment_model_id"].as_str()
                                    == card.enchantment.map(enchantment_id)
                        })
                        .is_none()
                })
            } else {
                game.run.deck.iter().position(|card| {
                    expected
                        .iter()
                        .position(|candidate| candidate == &(card.id, card.upgrades))
                        .is_none_or(|index| {
                            expected.remove(index);
                            false
                        })
                })
            };
        let Some(index) = index else {
            break;
        };
        let action = match game.phase {
            Phase::UpgradeCards(..) => Action::Smith(index),
            Phase::EnchantCards(..) => Action::Enchant(index),
            _ => Action::RemoveCard(index),
        };
        game.step(content, action)
            .map_err(|error| vec![format!("inferred choice rejected: {error:?}")])?;
    }
    if before["phase"] != "combat"
        && !matches!(
            traced_action["kind"].as_str(),
            Some("event_option" | "take_reward" | "skip_rewards")
        )
    {
        let key = |card: &Value| {
            Some((
                card["model_id"].as_str()?.to_owned(),
                card["upgrade"].as_u64().unwrap_or_default() as u8,
                card["enchantment_model_id"].as_str().map(str::to_owned),
            ))
        };
        let mut existing: Vec<_> = array(&before["players"][0]["deck"])
            .iter()
            .filter_map(key)
            .collect();
        for traced in array(&after["players"][0]["deck"]) {
            let id = traced["model_id"].as_str().unwrap_or_default();
            if let Some(index) = existing
                .iter()
                .position(|current| Some(current) == key(traced).as_ref())
            {
                existing.remove(index);
                continue;
            }
            let Phase::ChooseCards(cards, _, _) = &game.phase else {
                continue;
            };
            // Prefer the exact copy (Dolly's Mirror duplicates an enchanted card).
            let wanted = card(content, traced).ok();
            let index = cards
                .iter()
                .position(|card| {
                    wanted.is_some_and(|wanted| {
                        card.id == wanted.id
                            && card.upgrades == wanted.upgrades
                            && card.enchantment == wanted.enchantment
                    })
                })
                .or_else(|| {
                    cards
                        .iter()
                        .position(|card| content.cards[card.id as usize].id == id)
                });
            let index = if let Some(index) = index {
                index
            } else {
                game.phase = Phase::ChooseCards(
                    vec![card(content, traced).map_err(|error| vec![error])?],
                    1,
                    false,
                );
                0
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
    // A deck resynced from the capture only knows each Mad Science's type, not its rider.
    let previous = std::mem::take(&mut replay_state.card_variants);
    replay_state.card_variants = game
        .run
        .deck
        .iter()
        .filter(|card| content.cards[card.id as usize].id == "CARD.MAD_SCIENCE")
        .enumerate()
        .map(|(index, card)| match previous.get(index) {
            Some(&variant) if card.variant % 10 == 0 && variant / 10 == card.variant / 10 => {
                variant
            }
            _ => card.variant,
        })
        .collect();
    // The traced Tinker Time option names the new Mad Science's rider (and so its type).
    if let Some(rider) = traced_action["id"]
        .as_str()
        .and_then(|id| id.split("TINKER_TIME.pages.CHOOSE_RIDER.options.").nth(1))
        .and_then(|name| RIDERS.iter().position(|&rider| rider == name))
        && let Some(variant) = replay_state.card_variants.last_mut()
    {
        *variant = (rider / 3 + 1) as u8 * 10 + rider as u8 + 1;
    }
    replay_state.lasting_candy = game.lasting_candy;
    replay_state.paels_wing = game.paels_wing;
    replay_state.silver_crucible = game.silver_crucible;
    replay_state.silken_tress = game.silken_tress;
    replay_state.maw_bank = game.maw_bank;
    replay_state.silver_treasures = game.silver_treasures;
    replay_state.golden_compass = game.golden_compass;
    replay_state.winged_boots = game.winged_boots;
    replay_state.fishing_rod = game.fishing_rod;
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
    replay_state.courier_restock = game.courier_restock.clone();
    replay_state.fake_happy_flower = game.fake_happy_flower;
    if quarantined && before["phase"] == "combat" {
        return Ok(());
    }
    if game.combat().is_some_and(|combat| combat.choice.is_some()) {
        return Err(vec![format!(
            "simulator requested an untraced card choice: {:?}",
            game.combat().unwrap().choice
        )]);
    }
    // A potion used while a card replay still waits on its animation captures the game mid-play
    // (the card in the play pile, its hooks pending); the next decision compares instead.
    if !array(&after["players"][0]["play_pile"]).is_empty() {
        return Ok(());
    }
    // Event outcomes the simulator doesn't roll; synced last, as an event fight finished by
    // inferred choices above still rolls its own rewards (Battleworn Dummy).
    if traced_action["kind"] == "event_option" {
        game.rngs.rewards = Rng::from_seed(
            oracle_after["rng_seeds"]["player.1.Rewards"]
                .as_u64()
                .unwrap_or_default(),
        );
        game.rngs.rewards.forward(
            oracle_after["rng_counters"]["player.1.Rewards"]
                .as_u64()
                .unwrap_or_default(),
        );
        game.rngs.niche = Rng::from_seed(
            oracle_after["rng_seeds"]["run.Niche"]
                .as_u64()
                .unwrap_or_default(),
        );
        game.rngs.niche.forward(
            oracle_after["rng_counters"]["run.Niche"]
                .as_u64()
                .unwrap_or_default(),
        );
        // The fight's own outcome (Battleworn Dummy's upgrades) is already in the synced deck.
        if event_fight && game.combat().is_none() {
            game.run.gold =
                number(&after["players"][0], "gold").map_err(|error| vec![error])? as i32;
            sync_deck(content, &mut game, &after["players"][0]["deck"])?;
        }
    }
    if let Some(combat) = game.combat() {
        replay_state.unblocked_hits = (combat.turn, combat.history.unblocked_hits);
    }
    let differences = compare_snapshot(content, &game, after, oracle_after, combat_ended);
    if differences.is_empty() {
        Ok(())
    } else {
        Err(differences)
    }
}

/// Rebuilds the deck from a capture, which only shows Mad Science's type, keeping its variants.
fn sync_deck(content: &Content, game: &mut Game, deck: &Value) -> Result<(), Vec<String>> {
    let mad_science = |card: &&mut Card| content.cards[card.id as usize].id == "CARD.MAD_SCIENCE";
    let variants: Vec<u8> = game
        .run
        .deck
        .iter_mut()
        .filter(mad_science)
        .map(|card| card.variant)
        .collect();
    game.run.deck = cards(content, deck).map_err(|error| vec![error])?;
    for (card, variant) in game.run.deck.iter_mut().filter(mad_science).zip(variants) {
        card.variant = variant;
    }
    Ok(())
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
    // In combat, deck cards take their combat card's instance; the rest (played powers, stolen
    // or removed cards) get one no combat card can share.
    if let Some(indices) = oracle["card_deck_version_indices"]
        .as_object()
        .filter(|_| state["phase"] == "combat")
    {
        for (index, card) in deck.iter_mut().enumerate() {
            card.instance = u32::MAX - index as u32;
        }
        for (instance, index) in indices {
            if let Some(card) = deck.get_mut(index.as_u64().unwrap_or(usize::MAX as u64) as usize) {
                card.instance = instance_id(instance);
            }
        }
    }
    let relics = ids(&content.relics, &player["relics"], |x| x.id)?;
    let energy_relics = relics
        .iter()
        .filter(|&&id| ENERGY_RELICS.contains(&content.relics[id as usize].id))
        .count() as u8;
    let ascension = state["ascension"].as_u64().unwrap_or_default() as u8;
    let slots = player["potion_slots"].as_u64().map_or_else(
        || {
            (if ascension >= 4 { 2 } else { 3 })
                + relics
                    .iter()
                    .map(|&id| match content.relics[id as usize].id {
                        "RELIC.POTION_BELT" => 2,
                        "RELIC.PHIAL_HOLSTER" => 1,
                        "RELIC.ALCHEMICAL_COFFER" => 4,
                        _ => 0,
                    })
                    .sum::<usize>()
        },
        |slots| slots as usize,
    );
    let mut potions = vec![None; slots];
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
        // Outside combat the captured max energy is stale, so rebuild the run's relic bonuses.
        energy: (number(player, "max_energy")? as u8)
            .max(content.characters[character as usize].energy + energy_relics),
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
    // Slots order targeting; other names use the encounter's slot index, else the game's list order.
    let slot_number = |enemy: &Value| {
        let name = enemy["slot"].as_str()?;
        ["first", "second", "third", "fourth", "fifth"]
            .iter()
            .position(|slot| name == *slot)
            .or_else(|| {
                name.strip_prefix("egg")?
                    .parse::<usize>()
                    .ok()?
                    .checked_sub(1)
            })
            .map(|slot| slot as u8 + 1)
    };
    let named_slots = array(&combat_state["enemies"])
        .iter()
        .all(|enemy| slot_number(enemy).is_some());
    for (order, enemy) in array(&combat_state["enemies"]).iter().enumerate() {
        let id = find_id(&content.enemies, text(enemy, "model_id")?, |x| x.id)?;
        let instance_text = text(enemy, "instance_id")?;
        let move_id = oracle["enemy_move_ids"][instance_text]
            .as_str()
            .unwrap_or_default();
        let inferred_move = if move_id == "STUNNED" {
            oracle_after["enemy_move_ids"][instance_text]
                .as_str()
                .unwrap_or(move_id)
        } else {
            move_id
        };
        let move_index = infer_move(&content.enemies[id as usize], enemy, inferred_move);
        let instance = instance_id(instance_text);
        let move_history: Vec<usize> = history
            .enemy_moves
            .get(&instance)
            .into_iter()
            .flatten()
            .map(|move_id| infer_move(&content.enemies[id as usize], enemy, move_id))
            .collect();
        let repeats = move_history
            .iter()
            .rev()
            .take_while(|&&prior| prior == move_index)
            .count()
            .max(1) as u8;
        let claws = move_history.iter().filter(|&&prior| prior == 3).count() as i16;
        enemies.push(Enemy {
            instance,
            creature: Creature {
                id,
                // An about-to-blow Waterfall Giant has 999999999 HP.
                hp: number(enemy, "hit_points")?.min(i16::MAX as i64) as i16,
                max_hp: number(enemy, "max_hit_points")?.min(i16::MAX as i64) as i16,
                block: number(enemy, "block")? as i16,
                powers: powers(content, &enemy["powers"])?,
            },
            move_index,
            last_move: move_index,
            repeats,
            move_history,
            stunned: move_id == "STUNNED"
                && !content.enemies[id as usize].moves[move_index]
                    .intent
                    .contains("Stun"),
            // The Waterfall Giant's pressure gun grows by 5 each use and its explosion deals the
            // stored steam; the intents show the modified totals.
            value: match content.enemies[id as usize].id {
                "MONSTER.WATERFALL_GIANT" if matches!(move_index, 4 | 6) => {
                    let (base, step) = match move_index {
                        4 => (if run.ascension >= 9 { 23 } else { 20 }, 5),
                        _ => (0, 1),
                    };
                    let power = |id: &str| {
                        array(&enemy["powers"])
                            .iter()
                            .find(|power| power["model_id"] == id)
                            .and_then(|power| power["amount"].as_i64())
                            .unwrap_or_default()
                    };
                    let vulnerable = array(&player["powers"])
                        .iter()
                        .any(|power| power["model_id"] == "POWER.VULNERABLE_POWER");
                    let shown = enemy["visible_intents"][0]["damage"]
                        .as_i64()
                        .unwrap_or(base);
                    (0..1000)
                        .map(|index| index * step)
                        .find(|extra| {
                            let mut damage = (base + extra + power("POWER.STRENGTH_POWER")) as f64;
                            if power("POWER.WEAK_POWER") > 0 {
                                damage *= 0.75;
                            }
                            if vulnerable {
                                damage *= 1.5;
                            }
                            damage.floor() as i64 == shown
                        })
                        .unwrap_or(0) as i16
                }
                // Multi Claw's intent shows its hit count, three plus one per earlier claw;
                // otherwise count the claws seen.
                "MONSTER.TEST_SUBJECT" => enemy["visible_intents"][0]["hits"]
                    .as_i64()
                    .filter(|_| move_index == 3)
                    .map_or(claws - (move_index == 3) as i16, |hits| hits as i16 - 3)
                    .max(0),
                "MONSTER.WRIGGLER" => enemy["slot"]
                    .as_str()
                    .and_then(|slot| slot.strip_prefix("wriggler")?.parse().ok())
                    .unwrap_or(0),
                "MONSTER.FABRICATOR" => newest_aggro_bot(state)
                    .max(replay_state.aggro_bot)
                    .map_or(0, |(_, bot)| bot),
                _ => 0,
            },
            slot: if named_slots {
                slot_number(enemy).unwrap_or_default()
            } else {
                enemy["slot_index"]
                    .as_u64()
                    .map_or(order as u8, |index| index as u8)
                    + 1
            },
        });
        let creature = &mut enemies.last_mut().unwrap().creature;
        // Stolen Strength is what's left once temporary Strength losses are undone.
        let strength = (creature.power(power_id::STRENGTH)
            + creature
                .powers
                .iter()
                .filter(|power| TEMPORARY_STRENGTH_DOWN.contains(&power.id))
                .map(|power| power.amount)
                .sum::<i16>())
        .max(0);
        let dexterity = creature.power(power_id::DEXTERITY).max(0);
        let intangible = creature.power(power_id::INTANGIBLE) > 0;
        for power in &mut creature.powers {
            let name = content.powers[power.id as usize].id;
            let stored = |field| oracle_power_value(oracle, instance_text, name, field);
            if power.id == power_id::NEMESIS {
                power.value = intangible as i16;
            } else if power.id == power_id::POSSESS_STRENGTH {
                // Newer captures record what was actually stolen.
                power.value = stored("stolen").unwrap_or(strength);
            } else if power.id == power_id::POSSESS_SPEED {
                power.value = stored("stolen").unwrap_or(dexterity);
            } else if power.id == power_id::WITHERING_PRESENCE {
                // The captured display amount is the cards left before the next Wither.
                power.value = stored("display_amount")
                    .filter(|&left| left > 0)
                    .unwrap_or(history.withering);
            } else if power.id == power_id::HARDENED_SHELL {
                power.value = stored("hardened_shell_damage").unwrap_or_else(|| {
                    let remaining = array(&after["combat"]["enemies"])
                        .iter()
                        .find(|after| after["instance_id"] == instance_text)
                        .map(|after| {
                            number(enemy, "hit_points").unwrap_or_default()
                                - number(after, "hit_points").unwrap_or_default()
                        })
                        .unwrap_or_default();
                    (power.amount as i64 - remaining).clamp(0, power.amount as i64) as i16
                });
            } else if power.id == power_id::SKITTISH {
                power.value = array(&oracle["power_states"][instance_text])
                    .iter()
                    .find(|state| state["model_id"] == "POWER.SKITTISH_POWER")
                    .and_then(|state| state["skittish_block"].as_bool())
                    .unwrap_or_else(|| number(enemy, "block").unwrap_or_default() > 0)
                    as i16;
            }
        }
    }
    let variants: HashMap<_, _> = run
        .deck
        .iter()
        .filter(|card| card.variant != 0)
        .map(|card| (card.instance, card.variant))
        .collect();
    let borrowed_time = player_creature.power(power_id::BORROWED_TIME);
    let tangled = player_creature.power(power_id::TANGLED);
    let curious = player_creature.power(power_id::CURIOUS);
    let spiked_gauntlets = run
        .relics
        .iter()
        .any(|&id| content.relics[id as usize].id == "RELIC.SPIKED_GAUNTLETS");
    let set_values = |mut cards: Vec<Card>| {
        for card in &mut cards {
            let card_type = card.card_type(content.cards[card.id as usize]);
            let global_cost = borrowed_time + (card_type == CardType::Attack) as i16 * tangled
                - (card_type == CardType::Power) as i16 * curious
                + (card_type == CardType::Power && spiked_gauntlets) as i16;
            // The captured cost already includes Pinpoint's per-skill discount (auto-plays too).
            let dynamic_discount = match card.id {
                card_id::PINPOINT => player["skills_played_this_turn"]
                    .as_i64()
                    .map_or(history.skills, |skills| skills as i16),
                _ => 0,
            };
            let local = |cost: i8| {
                (cost as i16 - global_cost + dynamic_discount).clamp(i8::MIN as i16, i8::MAX as i16)
                    as i8
            };
            card.cost_delta = local(card.cost_delta);
            if card.until_played || card.when_played {
                card.cost_override = card.cost_override.map(local);
            }
            if let Some(&variant) = variants.get(&card.instance) {
                card.variant = variant;
            } else if let Some(source) = run.deck.iter().find(|source| {
                // A copy (Music Box's) has its deck Mad Science's rider.
                card.variant % 10 == 0 && source.variant / 10 == card.variant / 10
            }) {
                card.variant = source.variant;
            }
            // A captured value (Thrash's extra damage, ...) beats the tracked one.
            if card.value == 0 {
                card.value = history.values.get(&card.instance).copied().unwrap_or(
                    if card.id == card_id::WITHER {
                        history.wither_upgrades
                    } else {
                        card.value
                    },
                );
            }
            if matches!(
                content.cards[card.id as usize].id,
                "CARD.BOLAS" | "CARD.THRUMMING_HATCHET"
            ) && (history.bolas.contains(&card.instance)
                || array(&player["cards_played_this_turn_ids"])
                    .iter()
                    .any(|id| instance_id(id.as_str().unwrap_or_default()) == card.instance))
            {
                card.value = 1;
            }
            // Fetch draws only on its first finished play this turn.
            if card.id == card_id::FETCH
                && array(&player["cards_played_this_turn_ids"])
                    .iter()
                    .any(|id| instance_id(id.as_str().unwrap_or_default()) == card.instance)
            {
                card.turn_flags |= FETCHED;
            }
        }
        cards
    };
    let draw = set_values(
        cards(content, &oracle["hidden_draw_order"])?
            .into_iter()
            .rev()
            .collect(),
    );
    let hand = set_values(hand_cards(content, player)?);
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
    // Newer captures record each Toric Toughness instance's stored block.
    let captured: Vec<i16> = array(&oracle["power_states"]["creature:0"])
        .iter()
        .filter(|state| state["model_id"] == "POWER.TORIC_TOUGHNESS_POWER")
        // Older captures hard-coded 0; newer ones (with next_creature_id) store No Block's 0 too.
        .filter_map(|state| {
            state["toric_toughness_block"]
                .as_i64()
                .filter(|&x| x > 0 || oracle["next_creature_id"].is_u64())
        })
        .map(|x| x as i16)
        .collect();
    let mut toric_blocks = if captured.len() == toric_count {
        captured
    } else if history.toric.len() == toric_count {
        history.toric.iter().map(|x| x.1).collect::<Vec<_>>()
    } else {
        fallback
    }
    .into_iter();
    let mut orbit_states = array(&oracle["power_states"]["creature:0"])
        .iter()
        .filter(|state| state["model_id"] == "POWER.ORBIT_POWER")
        .map(|state| 4 - state["display_amount"].as_i64().unwrap_or(4) as i16);
    for power in &mut player_creature.powers {
        let name = content.powers[power.id as usize].id;
        let stored = |field| oracle_power_value(oracle, "creature:0", name, field);
        if power.id == power_id::TORIC_TOUGHNESS {
            power.value = toric_blocks.next().unwrap_or(5);
        } else if power.id == power_id::ORBIT {
            power.value = orbit_states.next().unwrap_or_default();
        } else if power.id == power_id::CONSTRICT {
            power.value = enemies
                .iter()
                .position(|enemy| {
                    content.enemies[enemy.creature.id as usize].id == "MONSTER.SLITHERING_STRANGLER"
                })
                .map_or(0, |index| index as i16 + 1);
        } else if power.id == power_id::SURROUNDED {
            power.value = stored("facing").unwrap_or_default();
        } else if power.id == power_id::CRIMSON_MANTLE {
            power.value = stored("self_damage").unwrap_or_default();
        } else if power.id == power_id::HARDENED_SHELL {
            power.value = stored("hardened_shell_damage").unwrap_or_default();
        }
    }
    let orbit_spent = player_creature
        .powers
        .iter()
        .filter(|power| power.id == power_id::ORBIT)
        .map(|power| power.value)
        .max()
        .unwrap_or(0);
    if let Some(amount) =
        oracle_power_value(oracle, "creature:0", "POWER.INFERNO_POWER", "self_damage")
            .filter(|&amount| amount > 0)
    {
        player_creature.powers.push(Power {
            id: power_id::INFERNO_DAMAGE,
            amount,
            skip_next_decay: false,
            value: 0,
        });
    }
    let monologue = array(&oracle["power_states"]["creature:0"])
        .iter()
        .filter(|state| state["model_id"] == "POWER.MONOLOGUE_POWER")
        .filter_map(|state| state["display_amount"].as_i64())
        .sum::<i64>() as i16;
    if monologue > 0 {
        player_creature.powers.push(Power {
            id: power_id::MONOLOGUE_RESTORE,
            amount: monologue,
            skip_next_decay: false,
            value: 0,
        });
    }
    let orbs = if player["orbs"].is_array() {
        array(&player["orbs"])
            .iter()
            .map(|orb| {
                Ok(Orb {
                    id: find_id(&content.orbs, text(orb, "model_id")?, |orb| orb.id)?,
                    value: number(orb, "value")? as i16,
                })
            })
            .collect::<Result<_, String>>()?
    } else {
        let before = oracle["rng_counters"]["run.CombatTargets"]
            .as_u64()
            .unwrap_or_default();
        let after = oracle_after["rng_counters"]["run.CombatTargets"]
            .as_u64()
            .unwrap_or(before);
        let rolls = after.saturating_sub(before) as usize;
        let dualcast = action["kind"] == "play_card"
            && hand.iter().any(|card| {
                card.instance == instance_id(action["source_id"].as_str().unwrap_or_default())
                    && content.cards[card.id as usize].id == "CARD.DUALCAST"
            });
        let count = if action["kind"] == "end_turn" {
            rolls
        } else if dualcast && rolls >= 2 {
            1
        } else {
            run.relics
                .iter()
                .any(|&id| content.relics[id as usize].id == "RELIC.CRACKED_CORE")
                as usize
        };
        let lightning = find_id(&content.orbs, "ORB.LIGHTNING_ORB", |orb| orb.id)?;
        vec![
            Orb {
                id: lightning,
                value: 0
            };
            count
        ]
    };
    let play_pile = array(&player["play_pile"]);
    let stars_gained = player["stars_gained_this_turn"]
        .as_i64()
        .unwrap_or_else(|| {
            let source = action["source_id"].as_str().unwrap_or_default();
            array(&player["hand"])
                .iter()
                .any(|card| card["instance_id"] == source && card["model_id"] == "CARD.RADIATE")
                .then(|| number(player, "stars").unwrap_or_default())
                .unwrap_or_default()
        }) as i16;
    let osty = array(&player["summons"])
        .iter()
        .find(|summon| summon["model_id"] == "MONSTER.OSTY");
    let drawn = number(player, "cards_drawn_this_combat").unwrap_or_default() as i16;
    let generated = oracle["player_generated_cards"]
        .as_i64()
        .unwrap_or_else(|| {
            oracle["card_deck_version_indices"]
                .as_object()
                .map_or(0, |indices| {
                    [
                        &player["hand"],
                        &player["play_pile"],
                        &player["discard_pile"],
                        &player["exhaust_pile"],
                        &oracle["hidden_draw_order"],
                    ]
                    .into_iter()
                    .flat_map(array)
                    .filter(|card| {
                        !matches!(card["type"].as_str(), Some("Status" | "Curse"))
                            && card["instance_id"]
                                .as_str()
                                .and_then(|instance| indices.get(instance))
                                .is_some_and(Value::is_null)
                    })
                    .count() as i64
                })
        }) as i16;
    // One counter per Automation played, as the game keeps them instanced.
    let fallback = history
        .automation_drawn
        .map_or(10, |start| 10 - (drawn - start).rem_euclid(10));
    let automation: Vec<(u8, i16)> = array(&oracle["power_states"]["creature:0"])
        .iter()
        .filter(|state| state["model_id"] == "POWER.AUTOMATION_POWER")
        .map(|state| {
            let left = state["automation_cards_left"]
                .as_i64()
                .filter(|&left| left > 0);
            (
                left.map_or(fallback, |left| left as i16) as u8,
                state["amount"].as_i64().unwrap_or(1) as i16,
            )
        })
        .collect();
    let automation = if automation.is_empty() {
        let amount: i16 = player_creature
            .powers
            .iter()
            .filter(|power| power.id == power_id::AUTOMATION)
            .map(|power| power.amount)
            .sum();
        (amount > 0)
            .then_some((fallback as u8, amount))
            .into_iter()
            .collect()
    } else {
        automation
    };
    let boulders = player_creature
        .powers
        .iter()
        .filter(|power| power.id == power_id::ROLLING_BOULDER && power.amount > 0)
        .map(|power| power.amount)
        .collect();
    let bomb_damage =
        40 + 10
            * run.deck.iter().any(|card| {
                content.cards[card.id as usize].id == "CARD.THE_BOMB" && card.upgrades > 0
            }) as i16;
    // Newer traces record each Bomb's damage; it isn't always the deck's copy (generated Bombs).
    let mut bomb_damages = array(&oracle["power_states"]["creature:0"])
        .iter()
        .filter(|state| state["model_id"] == "POWER.THE_BOMB_POWER")
        .map(|state| state["bomb_damage"].as_i64().filter(|&damage| damage > 0))
        .collect::<Vec<_>>()
        .into_iter();
    let bombs = player_creature
        .powers
        .iter()
        .filter(|power| power.id == power_id::THE_BOMB && power.amount > 0)
        .map(|power| {
            let damage = bomb_damages
                .next()
                .flatten()
                .map_or(bomb_damage, |damage| damage as i16);
            (power.amount as u8, damage)
        })
        .collect();
    // Newer traces count every card play this turn, including automatic ones.
    let played = |kind: &str, fallback: i16| {
        player[format!("{kind}_played_this_turn")]
            .as_i64()
            .map_or(fallback, |count| count as i16)
    };
    let cards = played("cards", history.cards).max(play_pile.len() as i16);
    let panache_played = (history.cards - history.panache_start.unwrap_or(0)).max(0);
    // Newer traces show Panache's cards left.
    let panache_left = oracle_power_value(
        oracle,
        "creature:0",
        "POWER.PANACHE_POWER",
        "display_amount",
    )
    .unwrap_or(5 - panache_played.rem_euclid(5));
    // Each Panache instance counts its own cards left.
    let panache_states: Vec<_> = array(&oracle["power_states"]["creature:0"])
        .iter()
        .filter(|state| state["model_id"] == "POWER.PANACHE_POWER")
        .collect();
    let panache = player_creature
        .powers
        .iter()
        .filter(|power| power.id == power_id::PANACHE)
        .enumerate()
        .map(|(index, power)| {
            let left = panache_states
                .get(index)
                .and_then(|state| state["display_amount"].as_i64())
                .map_or(panache_left, |left| left as i16);
            (left as u8, power.amount, cards)
        })
        .collect();
    let combat = Combat {
        player: player_creature,
        osty: Creature {
            id: 0,
            hp: osty.map_or(0, |osty| {
                number(osty, "hit_points").unwrap_or_default() as i16
            }),
            max_hp: osty.map_or(0, |osty| {
                number(osty, "max_hit_points").unwrap_or_default() as i16
            }),
            block: osty.map_or(0, |osty| number(osty, "block").unwrap_or_default() as i16),
            powers: osty
                .map(|osty| powers(content, &osty["powers"]))
                .transpose()?
                .unwrap_or_default(),
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
        // Bread's extra energy only starts on turn 2; the simulator counts it from turn 1.
        max_energy: number(player, "max_energy")? as i16
            + (combat_state["round"] == 1
                && array(&player["relics"])
                    .iter()
                    .any(|relic| relic["model_id"] == "RELIC.BREAD")) as i16,
        draw_per_turn: run.draw,
        stars: number(player, "stars")? as i16,
        turn: number(player, "turn_number").or_else(|_| number(combat_state, "round"))? as u16,
        orbs,
        orb_slots: player["orb_slots"].as_u64().unwrap_or(run.orb_slots as u64) as u8,
        history: History {
            cards,
            first_plays: player["first_card_plays_this_turn"]
                .as_i64()
                .map_or(played("cards", history.cards), |plays| plays as i16),
            manual_cards: history.manual_cards.max(play_pile.len() as i16),
            // Brilliant Scarf shows this turn's non-auto plays until it reaches 5 (then 1).
            manual_plays: array(&player["relics"])
                .iter()
                .find(|relic| relic["model_id"] == "RELIC.BRILLIANT_SCARF")
                .and_then(|relic| relic["amount"].as_i64())
                .filter(|&amount| amount != 1 || history.manual_plays < 5)
                .map_or(history.manual_plays.max(play_pile.len() as i16), |amount| {
                    amount as i16
                }),
            attacks: played("attacks", history.attacks),
            skills: played("skills", history.skills),
            powers: played("powers", history.powers),
            ethereal: number(player, "ethereal_cards_played_this_combat").unwrap_or_default()
                as i16,
            doom_applied: number(player, "doom_applied_this_turn").unwrap_or_default() as i16,
            discarded: number(player, "cards_discarded_this_turn").unwrap_or_default() as i16,
            statuses_drawn: player["statuses_drawn_this_turn"]
                .as_i64()
                .unwrap_or_default() as i16,
            extra_drawn: player["extra_cards_drawn_this_turn"]
                .as_i64()
                .unwrap_or_default() as i16,
            shivs: played_shivs(content, player, oracle, history).unwrap_or(history.shivs),
            generated,
            block_gains: number(player, "card_block_gains_this_turn")
                .map_or(history.block_gains, |gains| gains as i16),
            energy: number(player, "energy_spent_this_turn")? as i16,
            stars_gained,
            exhausted: number(player, "cards_exhausted_this_turn")? as i16,
            hp_lost: player["hp_lost_this_turn"].as_i64().unwrap_or_else(|| {
                player["player_lost_hp_this_turn"]
                    .as_bool()
                    .unwrap_or(false) as i64
            }) as i16,
            remnant_lost: player["hp_lost_this_turn"].as_i64().unwrap_or_else(|| {
                player["player_lost_hp_this_turn"]
                    .as_bool()
                    .unwrap_or(false) as i64
            }) as i16,
            hp_loss_events: number(player, "player_hp_loss_events_this_combat")? as i16,
            osty_attacks: number(player, "osty_attacks_this_turn")? as i16,
            feral_returns: oracle_power_value(
                oracle,
                "creature:0",
                "POWER.FERAL_POWER",
                "feral_returns_this_turn",
            )
            .unwrap_or_default(),
            unblocked_hits: {
                let turn = number(player, "turn_number")
                    .or_else(|_| number(combat_state, "round"))
                    .unwrap_or_default() as u16;
                if replay_state.unblocked_hits.0 == turn {
                    replay_state.unblocked_hits.1
                } else {
                    0
                }
            },
            ..History::default()
        },
        total_cards: player["cards_played_this_combat"]
            .as_i64()
            .map_or(history.total_cards, |cards| cards as i16),
        last_cards: history.last_cards,
        orbit_spent,
        hits: vec![0; array(&combat_state["enemies"]).len()],
        last_damage: 0,
        last_killed: false,
        drawn,
        innate_ordered: true,
        lightning_channeled: player["lightning_channeled_this_combat"]
            .as_i64()
            .unwrap_or_default() as i16,
        orbs_channeled: player["orbs_channeled_this_combat"]
            .as_u64()
            .unwrap_or_default() as u8,
        // Outbreak shows how many poisonings it has counted toward its next burst.
        poisoned: oracle_power_value(
            oracle,
            "creature:0",
            "POWER.OUTBREAK_POWER",
            "display_amount",
        )
        .unwrap_or_default()
        .max(0) as u8,
        nightmares: nightmare_copies(content, player, after),
        bombs,
        automation,
        panache,
        boulders,
        dampened,
        paels_tears: false,
        ending: false,
        playing: None,
        auto_plays: vec![],
        queue: vec![],
        control: vec![],
        choice: None,
        power_snapshot: vec![],
        enemy_power_snapshot: vec![],
        rebounds: vec![],
        ferals: vec![],
        card_energy: 0,
        card_stars: 0,
        card_plays: 1,
        pending_plays: vec![],
        dying: None,
        envenomed: Vec::new(),
        force_end: false,
        enemy_turn: false,
        centennial_puzzle: number(player, "player_hp_loss_events_this_combat")? > 0,
        // Demon Tongue heals the first HP loss of each player turn.
        demon_tongue: player["player_lost_hp_this_turn"]
            .as_bool()
            .unwrap_or(false),
        // Permafrost fires on the combat's first power play.
        permafrost: number(player, "powers_played_this_combat").unwrap_or_default() > 0
            || played("powers", history.powers) > 0,
        // Older captures lack used_relics; any Strength gained this combat used the helmet.
        ruined_helmet: used_relic(player, "RELIC.RUINED_HELMET").unwrap_or_else(|| {
            array(&player["powers"]).iter().any(|power| {
                power["model_id"] == "POWER.STRENGTH_POWER" && power["amount"].as_i64() > Some(0)
            })
        }),
        // Music Box copies only the turn's first finished attack.
        music_box: played("attacks", history.attacks) > 0,
        music_box_card: None,
        swift_after_return: 0,
        juggled: vec![],
        music_box_copy: None,
        ethereal_ending: None,
        mini_regent: used_relic(player, "RELIC.MINI_REGENT").unwrap_or(false),
        rainbow_ring: relic_active(player, "RELIC.RAINBOW_RING"),
        attacks_mod3: (played("attacks", history.attacks) % 3).max(0) as u8,
        discard_draw: (0, vec![]),
        // The relic shows its own counter, which misses skills auto-played before its
        // AfterSideTurnStart reset (Tools of the Trade's Sly discards). Older captures, which
        // predate used_relics, also show a melted wax copy's frozen counter.
        letter_opener: relic_amount_opt(player, "RELIC.LETTER_OPENER")
            .filter(|_| player.get("used_relics").is_some())
            .unwrap_or((played("skills", history.skills) % 3).max(0) as u8),
        bound_played: array(&oracle["power_states"]["creature:0"])
            .iter()
            .any(|state| {
                state["model_id"] == "POWER.CHAINS_OF_BINDING_POWER"
                    && state["bound_card_played"] == true
            }),
        // Played cards keep Bound, but a played Bound power leaves every pile: count it when Bound
        // was played this turn and no played-looking Bound card remains.
        bound_this_turn: {
            let bound = |pile: &Value| {
                array(pile)
                    .iter()
                    .filter(|card| card["affliction_model_id"] == "AFFLICTION.BOUND")
                    .count()
            };
            let played = bound(&player["discard_pile"]) + bound(&player["exhaust_pile"]);
            let bound_played = array(&oracle["power_states"]["creature:0"])
                .iter()
                .any(|state| {
                    state["model_id"] == "POWER.CHAINS_OF_BINDING_POWER"
                        && state["bound_card_played"] == true
                });
            (bound(&player["hand"])
                + played
                + bound(&oracle["hidden_draw_order"])
                + (bound_played && played == 0) as usize) as u8
        },
        auto_pre_play: false,
        setup: false,
        potions: 0,
        unsettling_lamp: None,
        unsettling_used: !relic_active(player, "RELIC.UNSETTLING_LAMP"),
        applying_hook: false,
        belt_buckle: relic_active(player, "RELIC.BELT_BUCKLE"),
        exhaust_queue: None,
        burning_sticks: array(&player["relics"])
            .iter()
            .any(|relic| relic["model_id"] == "RELIC.BURNING_STICKS" && relic["state"] != "Active"),
        red_skull: relic_active(player, "RELIC.RED_SKULL"),
        throwing_axe: !relic_active(player, "RELIC.THROWING_AXE"),
        vambrace: array(&player["relics"])
            .iter()
            .any(|relic| relic["model_id"] == "RELIC.VAMBRACE" && relic["state"] != "Active"),
        block_doubler: None,
        curled: Vec::new(),
        // Pael's Eye goes Normal once used, or after a manual play this turn.
        paels_eye: history.manual_cards == 0
            && array(&player["relics"])
                .iter()
                .any(|relic| relic["model_id"] == "RELIC.PAELS_EYE" && relic["state"] == "Normal"),
        extra_turn: false,
        // A ready Pael's Legion hides its counter and reports StackCount 1.
        paels_legion: if relic_active(player, "RELIC.PAELS_LEGION") {
            0
        } else {
            relic_amount(player, "RELIC.PAELS_LEGION")
        },
        history_course: last_played(content, player, oracle, history).unwrap_or(history.last_card),
        beat_down: Vec::new(),
        next_creature: oracle["next_creature_id"]
            .as_u64()
            .filter(|&id| id > 0)
            .map_or_else(
                || {
                    array(&combat_state["enemies"])
                        .iter()
                        .chain(array(&player["summons"]))
                        .map(|creature| {
                            instance_id(creature["instance_id"].as_str().unwrap_or_default())
                        })
                        .max()
                        .unwrap_or(1)
                },
                |id| id as u32,
            ),
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
    let act = replay_state
        .encounter_act
        .unwrap_or(number(state, "act")? as Id);
    let mut game = Game {
        run,
        map: trace_map(state, action, after),
        phase: Phase::Combat(Box::new(combat)),
        act,
        room: match state["room_type"].as_str().unwrap_or("Monster") {
            "Elite" => Room::Elite,
            "Boss" => Room::Boss,
            _ => Room::Combat,
        },
        // Newer captures show the relic's combat count modulo 3.
        fishing_rod: relic_amount_opt(player, "RELIC.FISHING_ROD")
            .filter(|_| player.get("cards_played_this_turn").is_some())
            .map_or(replay_state.fishing_rod, |amount| amount % 3),
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
        encounters: replay_state.encounter_bag(content, act, false, None),
        elites: replay_state.encounter_bag(content, act, true, None),
        weak_encounters_left: replay_state.weak_encounters_left,
        regular_encounters_left: replay_state.regular_encounters_left,
        elite_encounters_left: replay_state.elite_encounters_left,
        last_encounter: replay_state.last_encounter,
        last_elite: replay_state.last_elite,
        // The act's opening ancient is only known once its node is entered.
        ancient: after["room_model_id"]
            .as_str()
            .and_then(|id| trace_event(content, id).ok()),
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
        reward_continuation: None,
        prefetched_card_rewards: vec![],
        wongo_combats: wongo_combats(player),
        spoils: None,
        event_combat,
        damage_taken: false,
        lasting_candy: replay_state.lasting_candy,
        paels_wing: replay_state.paels_wing,
        silver_crucible: replay_state.silver_crucible,
        silken_tress: replay_state.silken_tress,
        rerolled_cards: false,
        maw_bank: spent_maw_banks(player).unwrap_or(replay_state.maw_bank),
        silver_treasures: replay_state.silver_treasures,
        golden_compass: replay_state.golden_compass,
        winged_boots: replay_state.winged_boots,
        rest_used: replay_state.rest_used,
        girya: replay_state.girya,
        pumpkin_candle: replay_state.pumpkin_candle,
        astrolabe: false,
        transform_niche: false,
        paels_tooth: false,
        paels_cards: paels_cards(content, player, &replay_state.paels_cards),
        pollinous_core: replay_state.pollinous_core,
        fur_coat_act: replay_state.fur_coat_act,
        fur_coat: replay_state.fur_coat.clone(),
        toy_box_offers: replay_state.toy_box_offers.clone(),
        wax_relics: replay_state.wax_relics.clone(),
        melted_relics: melted_relics(player, &replay_state.melted_relics),
        toy_box_combats: replay_state.toy_box_combats,
        parasol: replay_state.parasol.clone(),
        parasol_removal: replay_state.parasol_removal,
        conveyor: replay_state.conveyor,
        // A sphere left unfinished (AutoSlay walked away) doesn't follow into the next room.
        crystal: replay_state
            .crystal
            .clone()
            .filter(|_| state["room_model_id"] == "EVENT.CRYSTAL_SPHERE"),
        reward_gold_parts: replay_state.reward_gold_parts.clone(),
        fake_merchant: replay_state.fake_merchant.clone(),
        fake_shop: replay_state.fake_shop,
        courier_restock: replay_state.courier_restock.clone(),
        corpse_hp: None,
        fake_happy_flower: replay_state.fake_happy_flower,
        lizard_tail: relic_disabled(player, "RELIC.LIZARD_TAIL"),
        nunchaku: relic_amount(player, "RELIC.NUNCHAKU") % 10,
        pendulum: player["pendulum_turns_seen"].as_u64().map_or_else(
            || {
                if relic_active(player, "RELIC.PENDULUM") {
                    2
                } else {
                    relic_amount(player, "RELIC.PENDULUM") % 3
                }
            },
            |turns| turns as u8 % 3,
        ),
        pen_nib: if relic_active(player, "RELIC.PEN_NIB") {
            9
        } else {
            relic_amount(player, "RELIC.PEN_NIB") % 10
        },
        iron_club: relic_amount(player, "RELIC.IRON_CLUB") % 4,
        joss_paper: relic_amount(player, "RELIC.JOSS_PAPER") % 5,
        joss_ethereal: None,
        turn_end_exhaust: false,
        tuning_fork: relic_amount(player, "RELIC.TUNING_FORK") % 10,
        bone_tea: relic_disabled(player, "RELIC.BONE_TEA"),
        tea_of_discourtesy: relic_disabled(player, "RELIC.TEA_OF_DISCOURTESY"),
        galactic_dust: relic_amount(player, "RELIC.GALACTIC_DUST") % 10,
        book_of_five_rings: relic_amount(player, "RELIC.BOOK_OF_FIVE_RINGS") % 5,
        ember_tea: relic_amount_opt(player, "RELIC.EMBER_TEA")
            .map_or(0, |amount| 5u8.saturating_sub(amount.min(5))),
        sword_of_stone: relic_amount(player, "RELIC.SWORD_OF_STONE"),
        replaying: true,
        expectation: None,
    };
    // The captured grab bags still hold relics the game only drops when it next pulls.
    game.purge_disallowed_relics(content);
    Ok(game)
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
            if state["room_model_id"] == "EVENT.ABYSSAL_BATHS"
                && action["id"]
                    .as_str()
                    .is_some_and(|id| id.contains(".pages.ALL."))
            {
                event_page(id, &[0, 9])
            } else {
                Phase::Event(id, content.events[id as usize].options.to_vec())
            }
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
    // Starts follow encounter order (creature ids from 1); an enemy already dead at the first
    // decision (killed by turn-1 auto-plays) leaves a gap, so the sim's own starts are used.
    let contiguous = array(&after["combat"]["enemies"])
        .iter()
        .enumerate()
        .all(|(index, enemy)| {
            instance_id(enemy["instance_id"].as_str().unwrap_or_default()) == index as u32 + 1
        });
    if enemy_starts.len() != array(&after["combat"]["enemies"]).len() || !contiguous {
        enemy_starts.clear();
    }
    let large_capsule = action["id"]
        .as_str()
        .is_some_and(|id| id.ends_with("LARGE_CAPSULE"))
        || (action["runtime_type"]
            .as_str()
            .is_some_and(|runtime| runtime.ends_with("RelicReward"))
            && added_value(
                &state["players"][0]["relics"],
                &after["players"][0]["relics"],
                "model_id",
            )
            .is_some_and(|relic| relic["model_id"] == "RELIC.LARGE_CAPSULE"));
    let relic_queue = if large_capsule {
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
    let mut event_cards = vec![];
    if state["room_model_id"] == "EVENT.RANWID_THE_ELDER" {
        // Ranwid wants one potion slot and one relic; the traded ones are those that disappear.
        let slots = |player: &Value| -> Vec<i64> {
            array(&player["potions"])
                .iter()
                .filter_map(|potion| potion["amount"].as_i64())
                .collect()
        };
        let (before, kept) = (slots(player), slots(&after["players"][0]));
        if let Some(slot) = before
            .iter()
            .find(|slot| !kept.contains(slot))
            .or(before.first())
        {
            event_data[0] = slot + 1;
        }
        let ids = |player: &Value| -> Vec<Value> {
            array(&player["relics"])
                .iter()
                .map(|relic| relic["model_id"].clone())
                .collect()
        };
        let (before, kept) = (ids(player), ids(&after["players"][0]));
        let index = (0..before.len())
            .find(|&index| kept.get(index) != Some(&before[index]))
            .filter(|_| kept.len() < before.len());
        event_cards.extend(index.map(|index| index as u32));
    }
    if state["room_model_id"] == "EVENT.COLORFUL_PHILOSOPHERS" {
        // Each offered option names the character whose card pool it rewards.
        let options = array(&state["players"][0]["legal_actions"])
            .iter()
            .filter_map(|action| action["id"].as_str()?.split("options.").nth(1))
            .filter_map(|name| {
                let id = format!("CHARACTER.{name}");
                content
                    .characters
                    .iter()
                    .position(|character| character.id == id)
            });
        for (slot, character) in options.take(4).enumerate() {
            event_data[slot] = character as i64 + 1;
        }
    }
    if state["room_model_id"] == "EVENT.DARV"
        && let Some(card) = added_value(
            &state["players"][0]["deck"],
            &after["players"][0]["deck"],
            "instance_id",
        )
        .and_then(|card| content.card_id(card["model_id"].as_str()?))
        .filter(|&card| content.cards[card as usize].rarity == CardRarity::Ancient)
    {
        // Dusty Tome's ancient card was rolled when the event was offered.
        event_data[3] = card as i64 + 1;
    }
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
        "EVENT.ABYSSAL_BATHS"
            if action["id"]
                .as_str()
                .is_some_and(|id| id.ends_with(".options.LINGER")) =>
        {
            event_data[0] = (number(player, "hit_points")? + 2
                - number(&after["players"][0], "hit_points")?
                - 3)
            .max(0);
        }
        "EVENT.WHISPERING_HOLLOW" if action["kind"] == "event_option" => {
            event_data[0] =
                (number(player, "gold")? - number(&after["players"][0], "gold")?).max(0);
        }
        "EVENT.JUNGLE_MAZE_ADVENTURE" if action["kind"] == "event_option" => {
            let index = action["id"]
                .as_str()
                .and_then(|id| id.split(':').nth(2))
                .and_then(|index| index.parse::<usize>().ok())
                .unwrap_or_default();
            event_data[index] =
                (number(&after["players"][0], "gold")? - number(player, "gold")?).max(0);
        }
        "EVENT.SLIPPERY_BRIDGE" if !run.deck.is_empty() => {
            event_rng.as_mut().unwrap().below(run.deck.len() as u32);
        }
        "EVENT.PUNCH_OFF" => {
            event_data[0] = event_rng.as_mut().unwrap().below(8) as i64 + 91;
            // Constructs are creatures 2 and 3; one may already have died on turn 1.
            for enemy in array(&after["combat"]["enemies"]) {
                let index = instance_id(enemy["instance_id"].as_str().unwrap_or_default());
                if (2..4).contains(&index) {
                    event_data[index as usize - 1] =
                        number(enemy, "max_hit_points").unwrap_or_default();
                }
            }
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
    let after_player = &after["players"][0];
    let pendulum = player["pendulum_turns_seen"].as_u64().map_or_else(
        || {
            if number(after_player, "cards_drawn_this_combat").unwrap_or_default() > run.draw as i64
            {
                2
            } else {
                relic_amount(player, "RELIC.PENDULUM") % 3
            }
        },
        |turns| turns as u8 % 3,
    );
    let mut game = Game {
        run,
        map,
        phase,
        act,
        room,
        // Newer captures show the relic's combat count modulo 3.
        fishing_rod: relic_amount_opt(player, "RELIC.FISHING_ROD")
            .filter(|_| player.get("cards_played_this_turn").is_some())
            .map_or(replay_state.fishing_rod, |amount| amount % 3),
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
        encounters: replay_state.encounter_bag(
            content,
            act,
            false,
            (next_room == Room::Combat)
                .then_some(next_encounter)
                .flatten(),
        ),
        elites: replay_state.encounter_bag(
            content,
            act,
            true,
            (next_room == Room::Elite)
                .then_some(next_encounter)
                .flatten(),
        ),
        weak_encounters_left: replay_state.weak_encounters_left,
        regular_encounters_left: replay_state.regular_encounters_left,
        elite_encounters_left: replay_state.elite_encounters_left,
        last_encounter: replay_state.last_encounter,
        last_elite: replay_state.last_elite,
        // The act's opening ancient is only known once its node is entered.
        ancient: after["room_model_id"]
            .as_str()
            .and_then(|id| trace_event(content, id).ok()),
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
        event_cards,
        relic_queue,
        relic_deques: replay_state.relic_deques.clone(),
        shared_relic_deques: replay_state.shared_relic_deques.clone(),
        reward_continuation: pending_curse.then_some(RewardContinuation::AddRandomCurse),
        prefetched_card_rewards: vec![],
        wongo_combats: wongo_combats(player),
        spoils,
        event_combat: 0,
        damage_taken: false,
        lasting_candy: replay_state.lasting_candy,
        paels_wing: replay_state.paels_wing,
        silver_crucible: replay_state.silver_crucible,
        silken_tress: replay_state.silken_tress,
        rerolled_cards: false,
        maw_bank: spent_maw_banks(player).unwrap_or(replay_state.maw_bank),
        silver_treasures: replay_state.silver_treasures,
        golden_compass: replay_state.golden_compass,
        winged_boots: replay_state.winged_boots,
        rest_used: replay_state.rest_used,
        girya: replay_state.girya,
        pumpkin_candle: replay_state.pumpkin_candle,
        astrolabe: false,
        transform_niche: false,
        paels_tooth: false,
        paels_cards: paels_cards(content, player, &replay_state.paels_cards),
        pollinous_core: replay_state.pollinous_core,
        fur_coat_act: replay_state.fur_coat_act,
        fur_coat: replay_state.fur_coat.clone(),
        toy_box_offers: replay_state.toy_box_offers.clone(),
        wax_relics: replay_state.wax_relics.clone(),
        melted_relics: melted_relics(player, &replay_state.melted_relics),
        toy_box_combats: replay_state.toy_box_combats,
        parasol: replay_state.parasol.clone(),
        parasol_removal: replay_state.parasol_removal,
        conveyor: replay_state.conveyor,
        // A sphere left unfinished (AutoSlay walked away) doesn't follow into the next room.
        crystal: replay_state
            .crystal
            .clone()
            .filter(|_| state["room_model_id"] == "EVENT.CRYSTAL_SPHERE"),
        reward_gold_parts: replay_state.reward_gold_parts.clone(),
        fake_merchant,
        fake_shop: state["room_model_id"] == "EVENT.FAKE_MERCHANT",
        courier_restock: replay_state.courier_restock.clone(),
        corpse_hp: None,
        fake_happy_flower: replay_state.fake_happy_flower,
        lizard_tail: relic_disabled(player, "RELIC.LIZARD_TAIL"),
        nunchaku: relic_amount(player, "RELIC.NUNCHAKU") % 10,
        pendulum,
        pen_nib: relic_amount(player, "RELIC.PEN_NIB") % 10,
        iron_club: relic_amount(player, "RELIC.IRON_CLUB") % 4,
        joss_paper: relic_amount(player, "RELIC.JOSS_PAPER") % 5,
        joss_ethereal: None,
        turn_end_exhaust: false,
        tuning_fork: relic_amount(player, "RELIC.TUNING_FORK") % 10,
        bone_tea: relic_disabled(player, "RELIC.BONE_TEA"),
        tea_of_discourtesy: relic_disabled(player, "RELIC.TEA_OF_DISCOURTESY"),
        galactic_dust: relic_amount(player, "RELIC.GALACTIC_DUST") % 10,
        book_of_five_rings: relic_amount(player, "RELIC.BOOK_OF_FIVE_RINGS") % 5,
        ember_tea: relic_amount_opt(player, "RELIC.EMBER_TEA")
            .map_or(0, |amount| 5u8.saturating_sub(amount.min(5))),
        sword_of_stone: relic_amount(player, "RELIC.SWORD_OF_STONE"),
        replaying: true,
        expectation: None,
    };
    game.purge_disallowed_relics(content);
    Ok(game)
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
    Map {
        nodes: nodes.into(),
        current,
    }
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
        removals: runtime.ends_with("CardRemovalReward") as u8,
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
    if let Some(encounter) = trace_encounter(content, state)
        && let Some(index) = content.acts.iter().position(|act| {
            act.encounters.contains(&encounter)
                || act.elites.contains(&encounter)
                || act.bosses.contains(&encounter)
        })
    {
        return Some(index as Id);
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
                "MONSTER.BATTLE_FRIEND_V1" => 1,
                "MONSTER.BATTLE_FRIEND_V2" => 2,
                "MONSTER.BATTLE_FRIEND_V3" => 3,
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
            // A boss reward clicked as the act transition starts grants nothing.
            runtime if runtime.ends_with("GoldReward") => Ok(
                if number(&after["players"][0], "gold")? > game.run.gold as i64 {
                    Action::RewardGold
                } else {
                    Action::Leave
                },
            ),
            runtime if runtime.ends_with("CardReward") => Ok(
                if array(&after["players"][0]["deck"]).len() > game.run.deck.len() {
                    Action::RewardCard(0)
                } else {
                    Action::Leave
                },
            ),
            runtime if runtime.ends_with("RelicReward") => Ok(Action::RewardRelic(0)),
            runtime if runtime.ends_with("PotionReward") => Ok(Action::RewardPotion(0)),
            runtime if runtime.ends_with("CardRemovalReward") => Ok(Action::RewardRemove),
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
            let matches = |item: &ShopItem| match item {
                ShopItem::Card(card, _) => source == content.cards[card.id as usize].id,
                ShopItem::Relic(id, _) => source == content.relics[*id as usize].id,
                ShopItem::Potion(id, _) => source == content.potions[*id as usize].id,
                ShopItem::Remove(_) => source == "card-removal",
            };
            // Duplicate offers (two Weak Potions) are told apart by the traced price.
            let traced = action["id"]
                .as_str()
                .and_then(|id| id.rsplit(':').next()?.parse::<i32>().ok());
            items
                .iter()
                .position(|item| matches(item) && Some(price(item)) == traced)
                .or_else(|| items.iter().position(matches))
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
        // An Adaptable enemy (Test Subject) at 0 HP respawns, so the fight goes on; minions left
        // alone are about to die with the fight (the capture can land just before that).
        enemies.iter().any(|enemy| {
            let has = |id| {
                array(&enemy["powers"])
                    .iter()
                    .any(|power| power["model_id"] == id)
            };
            enemy["hit_points"].as_i64().unwrap_or_default() > 0 && !has("POWER.MINION_POWER")
                || has("POWER.ADAPTABLE_POWER")
        })
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
    // The simulator's turn counts player turns (TurnNumber), which a Pael's Eye extra turn
    // advances without a new round.
    compare_number(
        &mut diff,
        "turn",
        combat.turn as i64,
        if expected_player["turn_number"].is_number() {
            &expected_player["turn_number"]
        } else {
            &state["combat"]["round"]
        },
    );
    let expected_enemies = array(&state["combat"]["enemies"]);
    let expected_ids: Vec<_> = expected_enemies
        .iter()
        .map(|enemy| instance_id(enemy["instance_id"].as_str().unwrap_or_default()))
        .collect();
    let extras: Vec<_> = (combat.enemies.iter())
        .filter(|enemy| enemy.creature.hp > 0 && !expected_ids.contains(&enemy.instance))
        .map(|enemy| {
            let id = content.enemies[enemy.creature.id as usize].id;
            (id, enemy.instance, enemy.creature.hp)
        })
        .collect();
    if !extras.is_empty() {
        diff.push(format!(
            "enemies: simulator has {} unexpected living enemies {extras:?}",
            extras.len()
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
        state["room_model_id"].as_str().unwrap_or_default()
    );
    // Per-turn history follows player turns, which Pael's Eye can add within a round.
    let round = state["players"][0]["turn_number"]
        .as_u64()
        .or_else(|| state["combat"]["round"].as_u64())
        .unwrap_or_default() as u16;
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
        history.shivs = 0;
        history.previous_card = history.last_card.take();
        history.played.clear();
        history.last_repeat = false;
        history.block_gains = 0;
        history.bolas.clear();
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
    let player = &state["players"][0];
    let has_panache = array(&player["powers"])
        .iter()
        .any(|power| power["model_id"] == "POWER.PANACHE_POWER");
    if !has_panache {
        history.panache_start = None;
    } else if observe_moves {
        history.panache_start = Some(0);
    }
    if array(&player["powers"])
        .iter()
        .any(|power| power["model_id"] == "POWER.AUTOMATION_POWER")
    {
        history
            .automation_drawn
            .get_or_insert(number(player, "cards_drawn_this_combat").unwrap_or_default() as i16);
    } else {
        history.automation_drawn = None;
    }
    let dampened = array(&player["powers"])
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
    // Aeonglass's Increasing Intensity (its status intent) upgrades every Wither when the enemy turn
    // runs, whether the turn ended by end_turn or by Void Form.
    if after["combat"]["round"].as_u64() > before["combat"]["round"].as_u64()
        && array(&before["combat"]["enemies"]).iter().any(|enemy| {
            enemy["model_id"] == "MONSTER.AEONGLASS"
                && array(&enemy["visible_intents"])
                    .iter()
                    .any(|intent| intent["kind"] == "StatusCard")
        })
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
    if matches!(
        content.cards[id as usize].id,
        "CARD.BOLAS" | "CARD.THRUMMING_HATCHET"
    ) {
        history.bolas.push(instance_id(source));
    }
    let card_type = match card["type"].as_str() {
        Some("Attack") => CardType::Attack,
        Some("Power") => CardType::Power,
        Some("Status") => CardType::Status,
        Some("Curse") => CardType::Curse,
        Some("Quest") => CardType::Quest,
        _ => CardType::Skill,
    };
    let plays = 1
        + card["base_replay_count"].as_i64().unwrap_or_default() as i16
        + (card["enchantment_model_id"] == "ENCHANTMENT.SPIRAL") as i16
        + (card["enchantment_model_id"] == "ENCHANTMENT.GLAM"
            && card["enchantment_combat_amount"]
                .as_i64()
                .unwrap_or_default()
                > 0) as i16
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
    history.total_cards += plays + automatic;
    history.manual_cards += 1;
    history.manual_plays += plays;
    match card_type {
        CardType::Attack => history.attacks += plays,
        CardType::Skill => history.skills += plays,
        CardType::Power => history.powers += plays,
        _ => {}
    }
    if content.cards[id as usize].tags & SHIV_TAG != 0 {
        history.shivs += plays;
    }
    if content.cards[id as usize].id == "CARD.PANACHE" {
        history.panache_start = Some(history.cards);
    }
    if matches!(card_type, CardType::Attack | CardType::Skill) {
        history.last_card = self::card(content, card).ok();
        let instance = history.last_card.map_or(0, |card| card.instance);
        history.last_repeat = history.played.contains(&instance);
        history.played.push(instance);
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

fn model_card(content: &Content, value: &str) -> Option<Card> {
    let mut parts = value.split(':').skip(1);
    Some(Card {
        id: content.card_id(parts.next()?)?,
        upgrades: parts.next()?.parse().ok()?,
        ..Card::default()
    })
}

/// Hand cards whose shown cost is 0 only through a global hook the simulator applies itself
/// (Free Skill, Free Attack, Free Power, Corruption) keep their own cost.
fn hand_cards(content: &Content, player: &Value) -> Result<Vec<Card>, String> {
    let values = array(&player["hand"]);
    let mut hand = cards(content, &player["hand"])?;
    let has = |power: &str| {
        array(&player["powers"])
            .iter()
            .any(|value| value["model_id"] == power && value["amount"].as_i64() > Some(0))
    };
    for (card, value) in hand.iter_mut().zip(values) {
        let def = content.cards[card.id as usize];
        let free = match card.card_type(def) {
            CardType::Skill => has("POWER.FREE_SKILL_POWER") || has("POWER.CORRUPTION_POWER"),
            CardType::Attack => has("POWER.FREE_ATTACK_POWER"),
            CardType::Power => has("POWER.FREE_POWER_POWER"),
            _ => false,
        };
        if free
            && value["energy_cost"] == 0
            && array(&value["cost_modifiers"]).is_empty()
            && card.cost_override.is_none()
            && !card.free
        {
            card.cost_delta = 0;
        }
    }
    Ok(hand)
}

fn card(content: &Content, value: &Value) -> Result<Card, String> {
    let id = content
        .card_id(text(value, "model_id")?)
        .ok_or_else(|| format!("simulator has no card {}", value["model_id"]))?;
    let upgrades = value["upgrade"].as_u64().unwrap_or_default() as u8;
    let base = if value["enchantment_model_id"] == "ENCHANTMENT.TEZCATARAS_EMBER" {
        0
    } else {
        content.cards[id as usize].cost[upgrades.min(1) as usize]
    };
    let shown = value["energy_cost"].as_i64().unwrap_or(base as i64) as i8;
    let modifiers = array(&value["cost_modifiers"]);
    // Stomp's this-turn discounts are its value, reset at turn start, not a lasting cost delta.
    let stomp = if content.cards[id as usize].id == "CARD.STOMP" {
        -modifiers
            .iter()
            .filter(|modifier| modifier["expiration"] == 2 && modifier["type"] == "Relative")
            .filter_map(|modifier| modifier["amount"].as_i64())
            .sum::<i64>() as i8
    } else {
        0
    };
    // A cost of 0 for this turn only (Flatten's free turn, Mummified Hand, ...) is `free`, which
    // resets at turn start; the lasting cost is then the base plus combat-long modifiers.
    let turn_free = shown == 0
        && match modifiers
            .iter()
            .rev()
            .find(|modifier| modifier["type"] == "Absolute")
        {
            Some(modifier) => modifier["expiration"] == 2 && modifier["amount"] == 0,
            None => modifiers.is_empty() && value["cost_for_turn"].as_i64() == Some(0),
        };
    let lasting = modifiers
        .iter()
        .filter(|modifier| modifier["expiration"] == 0 && modifier["type"] == "Relative")
        .filter_map(|modifier| modifier["amount"].as_i64())
        .sum::<i64>() as i8;
    let mut card = Card {
        id,
        instance: instance_id(value["instance_id"].as_str().unwrap_or_default()),
        upgrades,
        cost_delta: if turn_free {
            lasting
        } else {
            shown.saturating_sub(base).saturating_add(stomp)
        },
        // Only keywords the card doesn't have by itself, so an upgrade can still remove its own
        // (Apparition's Ethereal).
        flags: (ETHEREAL * value["ethereal"].as_bool().unwrap_or_default() as u16
            | RETAIN * value["retain"].as_bool().unwrap_or_default() as u16
            | SLY * value["sly"].as_bool().unwrap_or_default() as u16)
            & !content.cards[id as usize].flags[upgrades.min(1) as usize]
            | DUPE * value["is_dupe"].as_bool().unwrap_or_default() as u16,
        turn_flags: SLY * value["sly_this_turn"].as_bool().unwrap_or_default() as u16,
        affliction: Affliction::from_model(value["affliction_model_id"].as_str().unwrap_or("")),
        value: [
            "rampage_extra_damage",
            "claw_extra_damage",
            "genetic_algorithm_bonus_block",
            "sovereign_blade_bonus_damage",
            "extra_damage",
            "combats_seen",
        ]
        .iter()
        .filter_map(|key| value[*key].as_i64())
        .max()
        .unwrap_or_default() as i16,
        // The last absolute cost modifier wins; one that lasts only until played is already in
        // the shown cost.
        cost_override: match value["cost_modifiers"].as_array() {
            Some(modifiers) => modifiers
                .iter()
                .rev()
                .find(|modifier| modifier["type"] == "Absolute")
                .filter(|modifier| {
                    matches!(modifier["expiration"].as_i64(), Some(0 | 2))
                        && modifier["is_reduce_only"] == false
                })
                .and_then(|modifier| modifier["amount"].as_i64())
                // Confused's combat cost isn't a listed modifier.
                .or_else(|| {
                    modifiers
                        .is_empty()
                        .then(|| value["combat_cost_override"].as_i64())
                        .flatten()
                }),
            None => value["cost_for_turn"]
                .as_i64()
                .or_else(|| value["combat_cost_override"].as_i64()),
        }
        .filter(|_| !turn_free)
        .map(|x| x as i8),
        replays: value["base_replay_count"].as_u64().unwrap_or_default() as u8,
        free: turn_free
            || shown == 0
                && value["star_cost"] == 0
                && content.cards[id as usize].star_cost[upgrades.min(1) as usize] > 0,
        // SetToFreeThisTurn zeroes the star cost even when a global hook raises the energy cost.
        free_stars: value["star_cost"] == 0
            && content.cards[id as usize].star_cost[upgrades.min(1) as usize] > 0
            && modifiers.iter().any(|modifier| {
                modifier["type"] == "Absolute"
                    && modifier["amount"] == 0
                    && modifier["expiration"].as_i64().is_some_and(|e| e & 2 != 0)
            }),
        // A star cost freed for the combat whose energy cost something else set (Confused).
        free_stars_combat: value["star_cost"] == 0
            && shown > 0
            && content.cards[id as usize].star_cost[upgrades.min(1) as usize] > 0
            && !modifiers.iter().any(|modifier| {
                modifier["type"] == "Absolute"
                    && modifier["amount"] == 0
                    && modifier["expiration"].as_i64().is_some_and(|e| e & 2 != 0)
            }),
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
        enchantment_value: value["enchantment_combat_amount"].as_i64().unwrap_or_else(
            || match value["enchantment_model_id"].as_str() {
                Some(
                    "ENCHANTMENT.GLAM"
                    | "ENCHANTMENT.SOWN"
                    | "ENCHANTMENT.SWIFT"
                    | "ENCHANTMENT.VIGOROUS",
                ) => value["enchantment_amount"].as_i64().unwrap_or_default(),
                _ => 0,
            },
        ) as i16,
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
    };
    if stomp > 0 {
        card.value = stomp as i16;
    }
    // An absolute cost set this turn or until played (Snecko Oil) is the override, worn off once
    // played; the cost underneath then comes from the modifiers that stay. The shown cost holds
    // the global modifiers (Borrowed Time), which set_values takes back out of `cost_delta`.
    let relative = |modifier: &&Value| modifier["type"] == "Relative";
    let amount = |modifier: &Value| modifier["amount"].as_i64().unwrap_or_default();
    if let Some(index) = modifiers
        .iter()
        .rposition(|modifier| modifier["type"] == "Absolute")
        .filter(|&index| {
            modifiers[index]["expiration"].as_i64().unwrap_or_default() & 4 != 0
                && modifiers[index]["is_reduce_only"] == false
                && !turn_free
        })
    {
        let set = amount(&modifiers[index])
            + modifiers[index + 1..]
                .iter()
                .filter(relative)
                .map(amount)
                .sum::<i64>();
        let kept = modifiers
            .iter()
            .filter(relative)
            .filter(|modifier| modifier["expiration"].as_i64().unwrap_or_default() & 4 == 0)
            .map(amount)
            .sum::<i64>();
        // Both hold the shown cost's global part until set_values takes the simulated one out;
        // Stomp's discounts stay in its value.
        card.cost_override = Some(shown.saturating_add(stomp));
        // EndOfTurn | WhenPlayed (Snecko Oil) also wears off at turn start; WhenPlayed alone
        // (Rocket Punch) lasts until played.
        card.until_played = modifiers[index]["expiration"].as_i64().unwrap_or_default() & 2 != 0;
        card.when_played = !card.until_played;
        card.cost_delta = (shown as i64 - set + kept + stomp as i64) as i8;
    }
    Ok(card)
}

fn powers(content: &Content, values: &Value) -> Result<Vec<Power>, String> {
    array(values)
        .iter()
        .map(|value| {
            Ok(Power {
                id: find_id(&content.powers, text(value, "model_id")?, |x| x.id)?,
                amount: number(value, "amount")? as i16,
                skip_next_decay: false,
                value: 0,
            })
        })
        .collect()
}

// Nightmare copies a hand card next turn; recover it from the copies that appear (the first new
// cards, made before the hand draw; Entropy may transform one of them afterwards). The game keeps
// one power per Nightmare played, each with its own card.
fn nightmare_copies(content: &Content, player: &Value, after: &Value) -> Vec<(Card, u8)> {
    let counts: Vec<u64> = array(&player["powers"])
        .iter()
        .filter(|power| power["model_id"] == "POWER.NIGHTMARE_POWER")
        .filter_map(|power| power["amount"].as_u64())
        .collect();
    let known: Vec<_> = ["hand", "discard_pile", "exhaust_pile"]
        .iter()
        .flat_map(|pile| array(&player[*pile]))
        .map(|card| &card["instance_id"])
        .collect();
    let mut new: Vec<_> = array(&after["players"][0]["hand"])
        .iter()
        .filter(|card| !known.contains(&&card["instance_id"]))
        .collect();
    let same =
        |a: &Value, b: &Value| a["model_id"] == b["model_id"] && a["upgrade"] == b["upgrade"];
    let mut copies = vec![];
    // Copies are made at turn start, after enemy-turn cards (Mecha Knight's Burns), in power order:
    // the last power's copies are the newest.
    for count in counts.into_iter().rev() {
        let Some(found) = new
            .iter()
            .filter(|card| new.iter().filter(|other| same(other, card)).count() as u64 >= count)
            .max_by_key(|card| instance_id(card["instance_id"].as_str().unwrap_or_default()))
            .or_else(|| {
                new.iter().min_by_key(|card| {
                    instance_id(card["instance_id"].as_str().unwrap_or_default())
                })
            })
            .copied()
        else {
            break;
        };
        let mut taken = 0;
        new.retain(|card| {
            let take = taken < count && same(card, found);
            taken += take as u64;
            !take
        });
        if let Ok(mut card) = card(content, found) {
            card.instance = 0;
            copies.push((card, count as u8));
        }
    }
    copies.reverse();
    copies
}

fn oracle_power_value(oracle: &Value, creature: &str, power: &str, field: &str) -> Option<i16> {
    array(&oracle["power_states"][creature])
        .iter()
        .find(|state| state["model_id"] == power)?[field]
        .as_i64()
        .map(|value| value as i16)
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
        ("MONSTER.AXE_RUBY_RAIDER", "SWING_1") => Some(0),
        ("MONSTER.AXE_RUBY_RAIDER", "SWING_2") => Some(1),
        ("MONSTER.AXE_RUBY_RAIDER", "BIG_SWING") => Some(2),
        ("MONSTER.FABRICATOR", "FABRICATE_MOVE") => Some(0),
        ("MONSTER.FABRICATOR", "FABRICATING_STRIKE_MOVE") => Some(1),
        ("MONSTER.FABRICATOR", "DISINTEGRATE_MOVE") => Some(2),
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
        ("MONSTER.SHRINKER_BEETLE", "SHRINKER_MOVE") => Some(0),
        ("MONSTER.SHRINKER_BEETLE", "CHOMP_MOVE") => Some(1),
        ("MONSTER.SHRINKER_BEETLE", "STOMP_MOVE") => Some(2),
        ("MONSTER.SLUDGE_SPINNER", "OIL_SPRAY_MOVE") => Some(0),
        ("MONSTER.SLUDGE_SPINNER", "SLAM_MOVE") => Some(1),
        ("MONSTER.SLUDGE_SPINNER", "RAGE_MOVE") => Some(2),
        ("MONSTER.GREMLIN_MERC", "GIMME_MOVE") => Some(0),
        ("MONSTER.GREMLIN_MERC", "DOUBLE_SMASH_MOVE") => Some(1),
        ("MONSTER.GREMLIN_MERC", "HEHE_MOVE") => Some(2),
        ("MONSTER.TWO_TAILED_RAT", "SCRATCH_MOVE") => Some(0),
        ("MONSTER.TWO_TAILED_RAT", "DISEASE_BITE_MOVE") => Some(1),
        ("MONSTER.TWO_TAILED_RAT", "SCREECH_MOVE") => Some(2),
        ("MONSTER.TWO_TAILED_RAT", "CALL_FOR_BACKUP_MOVE") => Some(3),
        ("MONSTER.BOWLBUG_EGG", "BITE_MOVE") => Some(0),
        ("MONSTER.BOWLBUG_NECTAR", "THRASH_MOVE") => Some(0),
        ("MONSTER.BOWLBUG_NECTAR", "BUFF_MOVE") => Some(1),
        ("MONSTER.BOWLBUG_NECTAR", "THRASH2_MOVE") => Some(2),
        ("MONSTER.BOWLBUG_ROCK", "HEADBUTT_MOVE") => Some(0),
        ("MONSTER.BOWLBUG_ROCK", "DIZZY_MOVE" | "STUNNED") => Some(1),
        ("MONSTER.BOWLBUG_SILK", "THRASH_MOVE") => Some(0),
        ("MONSTER.BOWLBUG_SILK", "TOXIC_SPIT_MOVE") => Some(1),
        ("MONSTER.NIBBIT", "BUTT_MOVE") => Some(0),
        ("MONSTER.NIBBIT", "SLICE_MOVE") => Some(1),
        ("MONSTER.NIBBIT", "HISS_MOVE") => Some(2),
        ("MONSTER.CORPSE_SLUG", "WHIP_SLAP_MOVE" | "STUNNED") => Some(0),
        ("MONSTER.CORPSE_SLUG", "GLOMP_MOVE") => Some(1),
        ("MONSTER.CORPSE_SLUG", "GOOP_MOVE") => Some(2),
        ("MONSTER.CUBEX_CONSTRUCT", "CHARGE_UP_MOVE") => Some(0),
        ("MONSTER.CUBEX_CONSTRUCT", "REPEATER_BLAST_MOVE") => Some(1),
        ("MONSTER.CUBEX_CONSTRUCT", "REPEATER_BLAST_MOVE_2") => Some(2),
        ("MONSTER.CUBEX_CONSTRUCT", "EXPEL_MOVE") => Some(3),
        ("MONSTER.SLITHERING_STRANGLER", "CONSTRICT") => Some(0),
        ("MONSTER.SLITHERING_STRANGLER", "THWACK") => Some(1),
        ("MONSTER.SLITHERING_STRANGLER", "LASH") => Some(2),
        ("MONSTER.SKULKING_COLONY", "ZOOM_MOVE") => Some(0),
        ("MONSTER.SKULKING_COLONY", "ZOOM_MOVE_2") => Some(1),
        ("MONSTER.SKULKING_COLONY", "INERTIA_MOVE") => Some(2),
        ("MONSTER.SKULKING_COLONY", "PIERCING_STABS_MOVE") => Some(3),
        ("MONSTER.TWIG_SLIME_M", "POKEY_POUNCE_MOVE") => Some(0),
        ("MONSTER.TWIG_SLIME_M", "STICKY_SHOT_MOVE") => Some(1),
        ("MONSTER.TOADPOLE", "SPIKE_SPIT_MOVE") => Some(0),
        ("MONSTER.TOADPOLE", "WHIRL_MOVE") => Some(1),
        ("MONSTER.TOADPOLE", "SPIKEN_MOVE") => Some(2),
        ("MONSTER.TERROR_EEL", "CRASH_MOVE") => Some(0),
        ("MONSTER.TERROR_EEL", "THRASH_MOVE") => Some(1),
        ("MONSTER.TERROR_EEL", "DIZZY_MOVE" | "STUNNED") => Some(2),
        ("MONSTER.TERROR_EEL", "TERROR_MOVE") => Some(3),
        ("MONSTER.THE_OBSCURA", "ILLUSION_MOVE") => Some(0),
        ("MONSTER.THE_OBSCURA", "PIERCING_GAZE_MOVE") => Some(1),
        ("MONSTER.THE_OBSCURA", "SAIL_MOVE") => Some(2),
        ("MONSTER.THE_OBSCURA", "HARDENING_STRIKE_MOVE") => Some(3),
        ("MONSTER.PHANTASMAL_GARDENER", "BITE_MOVE") => Some(0),
        ("MONSTER.PHANTASMAL_GARDENER", "LASH_MOVE") => Some(1),
        ("MONSTER.PHANTASMAL_GARDENER", "FLAIL_MOVE") => Some(2),
        ("MONSTER.PHANTASMAL_GARDENER", "ENLARGE_MOVE") => Some(3),
        ("MONSTER.INKLET", "JAB_MOVE") => Some(0),
        ("MONSTER.INKLET", "WHIRLWIND_MOVE") => Some(1),
        ("MONSTER.INKLET", "PIERCING_GAZE_MOVE") => Some(2),
        ("MONSTER.LAGAVULIN_MATRIARCH", "SLEEP_MOVE") => Some(0),
        ("MONSTER.LAGAVULIN_MATRIARCH", "SLASH_MOVE") => Some(1),
        ("MONSTER.LAGAVULIN_MATRIARCH", "SLASH2_MOVE") => Some(2),
        ("MONSTER.LAGAVULIN_MATRIARCH", "DISEMBOWEL_MOVE") => Some(3),
        ("MONSTER.LAGAVULIN_MATRIARCH", "SOUL_SIPHON_MOVE") => Some(4),
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
        ("MONSTER.OVICOPTER", "LAY_EGGS_MOVE") => Some(0),
        ("MONSTER.OVICOPTER", "NUTRITIONAL_PASTE_MOVE") => Some(1),
        ("MONSTER.OVICOPTER", "SMASH_MOVE") => Some(2),
        ("MONSTER.OVICOPTER", "TENDERIZER_MOVE") => Some(3),
        ("MONSTER.FUZZY_WURM_CRAWLER", "INHALE") => Some(1),
        ("MONSTER.FUZZY_WURM_CRAWLER", "ACID_GOOP") => Some(2),
        ("MONSTER.SNAPPING_JAXFRUIT", "ENERGY_ORB_MOVE") => Some(0),
        ("MONSTER.FLYCONID", "VULNERABLE_SPORES_MOVE") => Some(0),
        ("MONSTER.FLYCONID", "FRAIL_SPORES_MOVE") => Some(1),
        ("MONSTER.FLYCONID", "SMASH_MOVE") => Some(2),
        ("MONSTER.FOSSIL_STALKER", "TACKLE_MOVE") => Some(0),
        ("MONSTER.FOSSIL_STALKER", "LATCH_MOVE") => Some(1),
        ("MONSTER.FOSSIL_STALKER", "LASH_MOVE") => Some(2),
        ("MONSTER.LIVING_FOG", "ADVANCED_GAS_MOVE" | "SUPER_GAS_BLAST_MOVE") => Some(0),
        ("MONSTER.LIVING_FOG", "BLOAT_MOVE") => Some(1),
        ("MONSTER.FOGMOG", "ILLUSION_MOVE") => Some(0),
        ("MONSTER.FOGMOG", "SWIPE_MOVE") => Some(1),
        ("MONSTER.FOGMOG", "SWIPE_RANDOM_MOVE") => Some(2),
        ("MONSTER.FOGMOG", "HEADBUTT_MOVE") => Some(3),
        ("MONSTER.EYE_WITH_TEETH", "DISTRACT_MOVE") => Some(0),
        ("MONSTER.EYE_WITH_TEETH", "REVIVE_MOVE") => Some(1),
        ("MONSTER.CEREMONIAL_BEAST", "STAMP_MOVE") => Some(0),
        ("MONSTER.CEREMONIAL_BEAST", "PLOW_MOVE") => Some(1),
        ("MONSTER.CEREMONIAL_BEAST", "STUNNED") => Some(2),
        ("MONSTER.LOUSE_PROGENITOR", "WEB_CANNON_MOVE") => Some(0),
        ("MONSTER.LOUSE_PROGENITOR", "POUNCE_MOVE") => Some(1),
        ("MONSTER.LOUSE_PROGENITOR", "CURL_AND_GROW_MOVE") => Some(2),
        ("MONSTER.SOUL_FYSH", "BECKON_MOVE") => Some(0),
        ("MONSTER.SOUL_FYSH", "DE_GAS_MOVE") => Some(1),
        ("MONSTER.SOUL_FYSH", "GAZE_MOVE") => Some(2),
        ("MONSTER.SOUL_FYSH", "FADE_MOVE") => Some(3),
        ("MONSTER.SOUL_FYSH", "SCREAM_MOVE") => Some(4),
        ("MONSTER.FROG_KNIGHT", "FOR_THE_QUEEN") => Some(0),
        ("MONSTER.FROG_KNIGHT", "STRIKE_DOWN_EVIL") => Some(1),
        ("MONSTER.FROG_KNIGHT", "TONGUE_LASH") => Some(2),
        ("MONSTER.FROG_KNIGHT", "BEETLE_CHARGE") => Some(3),
        ("MONSTER.KNOWLEDGE_DEMON", "CURSE_OF_KNOWLEDGE_MOVE") => Some(0),
        ("MONSTER.KNOWLEDGE_DEMON", "SLAP_MOVE") => Some(1),
        ("MONSTER.KNOWLEDGE_DEMON", "KNOWLEDGE_OVERWHELMING_MOVE") => Some(2),
        ("MONSTER.KNOWLEDGE_DEMON", "PONDER_MOVE") => Some(3),
        ("MONSTER.EXOSKELETON", "SKITTER_MOVE") => Some(0),
        ("MONSTER.EXOSKELETON", "MANDIBLES_MOVE") => Some(1),
        ("MONSTER.EXOSKELETON", "ENRAGE_MOVE") => Some(2),
        ("MONSTER.MYTE", "TOXIC_MOVE") => Some(0),
        ("MONSTER.MYTE", "BITE_MOVE") => Some(1),
        ("MONSTER.MYTE", "SUCK_MOVE") => Some(2),
        ("MONSTER.TEST_SUBJECT", "RESPAWN_MOVE") => Some(0),
        ("MONSTER.TEST_SUBJECT", "BITE_MOVE") => Some(1),
        ("MONSTER.TEST_SUBJECT", "SKULL_BASH_MOVE") => Some(2),
        ("MONSTER.TEST_SUBJECT", "MULTI_CLAW_MOVE") => Some(3),
        ("MONSTER.TEST_SUBJECT", "PHASE3_LACERATE_MOVE") => Some(4),
        ("MONSTER.TEST_SUBJECT", "BIG_POUNCE") => Some(5),
        ("MONSTER.TEST_SUBJECT", "BURNING_GROWL_MOVE") => Some(6),
        ("MONSTER.CRUSHER", "THRASH_MOVE") => Some(0),
        ("MONSTER.CRUSHER", "ENLARGING_STRIKE_MOVE") => Some(1),
        ("MONSTER.CRUSHER", "BUG_STING_MOVE") => Some(2),
        ("MONSTER.CRUSHER", "ADAPT_MOVE") => Some(3),
        ("MONSTER.CRUSHER", "GUARDED_STRIKE_MOVE") => Some(4),
        ("MONSTER.ROCKET", "TARGETING_RETICLE_MOVE") => Some(0),
        ("MONSTER.ROCKET", "PRECISION_BEAM_MOVE") => Some(1),
        ("MONSTER.ROCKET", "CHARGE_UP_MOVE") => Some(2),
        ("MONSTER.ROCKET", "LASER_MOVE") => Some(3),
        ("MONSTER.ROCKET", "RECHARGE_MOVE") => Some(4),
        ("MONSTER.CEREMONIAL_BEAST", "BEAST_CRY_MOVE") => Some(3),
        ("MONSTER.CEREMONIAL_BEAST", "STOMP_MOVE") => Some(4),
        ("MONSTER.CEREMONIAL_BEAST", "CRUSH_MOVE") => Some(5),
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
        ("MONSTER.WATERFALL_GIANT", "PRESSURIZE_MOVE") => Some(0),
        ("MONSTER.WATERFALL_GIANT", "STOMP_MOVE") => Some(1),
        ("MONSTER.WATERFALL_GIANT", "RAM_MOVE") => Some(2),
        ("MONSTER.WATERFALL_GIANT", "SIPHON_MOVE") => Some(3),
        ("MONSTER.WATERFALL_GIANT", "PRESSURE_GUN_MOVE") => Some(4),
        ("MONSTER.WATERFALL_GIANT", "PRESSURE_UP_MOVE") => Some(5),
        ("MONSTER.WATERFALL_GIANT", "EXPLODE_MOVE") => Some(6),
        ("MONSTER.WATERFALL_GIANT", "ABOUT_TO_BLOW_MOVE") => Some(7),
        _ => None,
    } {
        return index;
    }
    let intents = array(&state["visible_intents"]);
    // Shown damage includes the enemy's Strength; move intents list base damage.
    let strength = array(&state["powers"])
        .iter()
        .find(|power| power["model_id"] == "POWER.STRENGTH_POWER")
        .and_then(|power| power["amount"].as_i64())
        .unwrap_or_default();
    let damage = intents
        .first()
        .and_then(|intent| intent["damage"].as_i64())
        .map_or(0, |damage| (damage - strength).max(0));
    let hits = intents
        .first()
        .and_then(|intent| intent["hits"].as_i64())
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
                * 10_000
                + (intent_matches(candidate.intent, "StatusCard")
                    && !intents.iter().any(|intent| intent["kind"] == "StatusCard"))
                    as i64
                    * 10_000
                + (intent_matches(candidate.intent, "Debuff")
                    && !intents.iter().any(|intent| {
                        matches!(
                            intent["kind"].as_str(),
                            Some("Debuff" | "DebuffStrong" | "CardDebuff")
                        )
                    })) as i64
                    * 10_000;
            let candidate_damage = candidate
                .intent
                .strip_prefix("Attack")
                .and_then(|x| {
                    x.split(|c: char| !c.is_ascii_digit())
                        .find_map(|x| x.parse::<i64>().ok())
                })
                .unwrap_or_default();
            let semantic_penalty = candidate
                .intent
                .split(|c: char| !c.is_ascii_alphabetic())
                .filter(|word| word.len() >= 4)
                .all(|word| !move_id.contains(&word[..4].to_ascii_uppercase()))
                as i64;
            let candidate_hits = if !candidate.intent.starts_with("Attack") {
                0
            } else {
                candidate
                    .intent
                    .split_once('x')
                    .and_then(|(_, x)| x.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok())
                    .unwrap_or(1)
            };
            let hits_penalty = (hits > 0 && candidate_hits != hits) as i64;
            kind_penalty
                + hits_penalty * 1_000
                + semantic_penalty * 100
                + (candidate_damage - damage).abs()
        })
        .map(|(index, _)| index)
        .unwrap_or_default()
}

fn intent_matches(intent: &str, kind: &str) -> bool {
    match kind {
        "Attack" => intent.starts_with("Attack"),
        "Defend" => intent.contains("Block") || intent.contains("Defend"),
        "Buff" => !["Attack", "Debuff", "DeathBlow", "Stun", "Sleep"]
            .iter()
            .any(|word| intent.contains(word)),
        "Escape" => intent.contains("Escape"),
        "Heal" => intent.contains("Heal"),
        "DeathBlow" => intent.starts_with("Attack") || intent.contains("DeathBlow"),
        "Sleep" => intent.contains("Sleep"),
        "Stun" => intent.contains("Stun"),
        "Summon" => intent.contains("Summon"),
        "Debuff" => {
            [
                "Debuff",
                "Vulnerable",
                "Weak",
                "Frail",
                "Constrict",
                "Shrink",
                "Tangled",
                "Ringing",
            ]
            .iter()
            .any(|word| intent.contains(word))
                || intent.contains(" -")
        }
        "StatusCard" => {
            intent.contains("Status")
                || intent.contains("Dazed")
                || intent.contains("Infection")
                || intent.contains("Slimed")
                || intent.contains("Beckon")
                || intent.contains("Toxic")
        }
        _ => !intent.starts_with("Attack"),
    }
}

fn compare_number(diff: &mut Vec<String>, path: &str, actual: i64, expected: &Value) {
    // Values beyond i16 (an about-to-blow Waterfall Giant's 999999999 HP) match any huge value.
    let huge = |x: i64| x > i16::MAX as i64 - 10_000;
    if let Some(expected) = expected
        .as_i64()
        .filter(|&expected| expected != actual && !(expected > i16::MAX as i64 && huge(actual)))
    {
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
    // The game keeps one instance per Automation, Monologue, Nightmare, Panache or Rolling Boulder;
    // the simulator stacks them.
    fn stack(powers: impl Iterator<Item = (String, i64)>) -> Vec<String> {
        let mut stacked: Vec<(String, i64)> = vec![];
        for (id, amount) in powers {
            match stacked.iter_mut().find(|(other, _)| {
                *other == id
                    && matches!(
                        id.as_str(),
                        "POWER.AUTOMATION_POWER"
                            | "POWER.MONOLOGUE_POWER"
                            | "POWER.NIGHTMARE_POWER"
                            | "POWER.PANACHE_POWER"
                            | "POWER.ROLLING_BOULDER_POWER"
                    )
            }) {
                Some((_, total)) => *total += amount,
                None => stacked.push((id, amount)),
            }
        }
        let mut stacked: Vec<_> = stacked
            .into_iter()
            .map(|(id, amount)| format!("{id}={amount}"))
            .collect();
        stacked.sort();
        stacked
    }
    let actual = stack(
        actual
            .iter()
            .filter(|power| {
                !matches!(
                    power.id,
                    power_id::INFERNO_DAMAGE | power_id::MONOLOGUE_RESTORE
                )
            })
            .map(|power| {
                (
                    content.powers[power.id as usize].id.to_owned(),
                    power.amount as i64,
                )
            }),
    );
    let expected = stack(array(expected).iter().map(|power| {
        (
            power["model_id"].as_str().unwrap_or("?").to_owned(),
            power["amount"].as_i64().unwrap_or_default(),
        )
    }));
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

// Pael's Tooth keeps its cards out of the deck; newer captures record them as
// "ID+upgrade[@ENCHANTMENT:amount]". Older ones drop the enchantment, kept from `known`.
fn paels_cards(content: &Content, player: &Value, known: &[Card]) -> Vec<Card> {
    let Some(cards) = player["paels_tooth_cards"].as_array() else {
        return known.to_vec();
    };
    let mut known = known.to_vec();
    cards
        .iter()
        .filter_map(|card| {
            let text = card.as_str()?;
            let old = !text.contains('@');
            let (card, enchantment) = text.split_once('@').unwrap_or((text, ""));
            let (id, upgrade) = card.rsplit_once('+')?;
            let (enchantment, amount) = enchantment.split_once(':').unwrap_or((enchantment, "0"));
            self::card(
                content,
                &serde_json::json!({
                    "model_id": id,
                    "upgrade": upgrade.parse::<u64>().ok()?,
                    "enchantment_model_id": (!enchantment.is_empty()).then_some(enchantment),
                    "enchantment_amount": amount.parse::<i64>().ok()?,
                }),
            )
            .ok()
            .map(|mut card| {
                if let Some(index) = known.iter().position(|other| other.id == card.id)
                    && old
                {
                    let other = known.remove(index);
                    card.enchantment = other.enchantment;
                    card.enchantment_amount = other.enchantment_amount;
                }
                card
            })
        })
        .collect()
}

// The Fabricator never repeats its last spawn: the newest aggro bot (every move spawns an aggro
// bot last, so a defensive bot is never the last spawn).
fn newest_aggro_bot(state: &Value) -> Option<(u32, i16)> {
    array(&state["combat"]["enemies"])
        .iter()
        .filter_map(|enemy| {
            let bot = FABRICATOR_BOTS
                .iter()
                .position(|&bot| enemy["model_id"] == bot)
                .filter(|&bot| bot >= 2)?;
            Some((instance_id(enemy["instance_id"].as_str()?), bot as i16 + 1))
        })
        .max()
}

// History Course's card: the last attack or skill whose play finished this turn, auto-plays
// included (dupes leave combat, so they're never found).
fn last_played(
    content: &Content,
    player: &Value,
    oracle: &Value,
    history: &TurnHistory,
) -> Option<Option<Card>> {
    let played = player["cards_played_this_turn_ids"].as_array()?;
    let cards: Vec<&Value> = [
        &player["hand"],
        &player["play_pile"],
        &player["discard_pile"],
        &player["exhaust_pile"],
        &oracle["hidden_draw_order"],
    ]
    .into_iter()
    .flat_map(array)
    .collect();
    // Older captures list each played card once: a repeated last play comes from the traced
    // actions, unless it vanished like History Course's own copies.
    let distinct = played
        .iter()
        .enumerate()
        .all(|(index, id)| !played[..index].contains(id));
    if history.last_repeat
        && distinct
        && let Some(card) = history.last_card
        && cards.iter().any(|value| {
            instance_id(value["instance_id"].as_str().unwrap_or_default()) == card.instance
        })
    {
        return Some(Some(card));
    }
    Some(played.iter().rev().find_map(|id| {
        cards
            .iter()
            .find(|card| {
                card["instance_id"] == *id
                    && matches!(card["type"].as_str(), Some("Attack" | "Skill"))
            })
            .and_then(|value| next_turn_card(content, player, value))
    }))
}

/// History Course clones the card at the next turn start, once its turn and when-played cost
/// modifiers are gone.
fn next_turn_card(content: &Content, player: &Value, value: &Value) -> Option<Card> {
    let mut card = card(content, value).ok()?;
    // Only when the captured modifiers explain the shown cost (hook costs aren't listed),
    // besides this turn's global modifiers, which are gone by then (Borrowed Time, Tangled).
    let base = base_cost(card, content.cards[card.id as usize]) as i64;
    let power = |id: &str| {
        array(&player["powers"])
            .iter()
            .filter(|power| power["model_id"] == id)
            .filter_map(|power| power["amount"].as_i64())
            .sum::<i64>()
    };
    let global = power("POWER.BORROWED_TIME_POWER")
        + (value["type"] == "Attack") as i64 * power("POWER.TANGLED_POWER");
    let cost = |modifiers: &[Value], keep: &dyn Fn(&Value) -> bool| {
        let kept = || modifiers.iter().filter(|modifier| keep(modifier));
        let start = kept()
            .filter(|modifier| modifier["type"] == "Absolute")
            .filter_map(|modifier| modifier["amount"].as_i64())
            .last()
            .unwrap_or(base);
        start
            + kept()
                .filter(|modifier| modifier["type"] == "Relative")
                .filter_map(|modifier| modifier["amount"].as_i64())
                .sum::<i64>()
    };
    let shown = value["energy_cost"].as_i64().unwrap_or(base);
    if let Some(modifiers) = value["cost_modifiers"].as_array()
        && base >= 0
        && (cost(modifiers, &|_| true).max(0) + global == shown
            || modifiers.is_empty() && value["cost_for_turn"].as_i64() == Some(shown - global))
    {
        let combat = |modifier: &&Value| modifier["expiration"] == 0;
        card.cost_delta = modifiers
            .iter()
            .filter(combat)
            .filter(|modifier| modifier["type"] == "Relative")
            .filter_map(|modifier| modifier["amount"].as_i64())
            .sum::<i64>() as i8;
        card.cost_override = modifiers
            .iter()
            .filter(combat)
            .rev()
            .find(|modifier| modifier["type"] == "Absolute")
            .filter(|modifier| modifier["is_reduce_only"] == false)
            .and_then(|modifier| modifier["amount"].as_i64())
            .or_else(|| {
                modifiers
                    .is_empty()
                    .then(|| value["combat_cost_override"].as_i64())
                    .flatten()
            })
            .map(|x| x as i8);
    }
    card.free = false;
    Some(card)
}

// Shivs whose play finished this turn, auto-plays included; newer captures list played cards.
fn played_shivs(
    content: &Content,
    player: &Value,
    oracle: &Value,
    history: &TurnHistory,
) -> Option<i16> {
    let played = player["cards_played_this_turn_ids"].as_array()?;
    let cards: Vec<&Value> = [
        &player["hand"],
        &player["play_pile"],
        &player["discard_pile"],
        &player["exhaust_pile"],
        &oracle["hidden_draw_order"],
    ]
    .into_iter()
    .flat_map(array)
    .collect();
    // A played card found in no pile vanished: History Course's copy of last turn's last play.
    let copied_shiv = history.previous_card.is_some_and(|card| {
        content.cards[card.id as usize].tags & SHIV_TAG != 0
            && array(&player["relics"])
                .iter()
                .any(|relic| relic["model_id"] == "RELIC.HISTORY_COURSE")
    });
    Some(
        played
            .iter()
            .filter(|id| {
                let found = cards.iter().find(|card| card["instance_id"] == **id);
                found.map_or(copied_shiv, |card| {
                    card["model_id"]
                        .as_str()
                        .and_then(|model| content.card_id(model))
                        .is_some_and(|model| content.cards[model as usize].tags & SHIV_TAG != 0)
                })
            })
            .count() as i16,
    )
}

/// Each Maw Bank is spent on its own; captured relic states say which ones are.
fn spent_maw_banks(player: &Value) -> Option<u8> {
    let banks = array(&player["relics"])
        .iter()
        .filter(|relic| relic["model_id"] == "RELIC.MAW_BANK");
    (banks.clone().all(|relic| relic["state"].is_string()))
        .then(|| banks.filter(|relic| relic["state"] == "Disabled").count() as u8)
}

// A melted wax relic shows as Disabled; tracked indices go stale when the relic list shifts.
// Relics that disable themselves once used up are not melted.
fn melted_relics(player: &Value, melted: &[usize]) -> Vec<usize> {
    const USED_UP: [&str; 13] = [
        "RELIC.PAELS_TOOTH",
        "RELIC.LIZARD_TAIL",
        "RELIC.SILVER_CRUCIBLE",
        "RELIC.SILKEN_TRESS",
        "RELIC.PUMPKIN_CANDLE",
        "RELIC.EMBER_TEA",
        "RELIC.TEA_OF_DISCOURTESY",
        "RELIC.LAVA_ROCK",
        "RELIC.TOY_BOX",
        "RELIC.MAW_BANK",
        "RELIC.WINGED_BOOTS",
        "RELIC.WONGOS_MYSTERY_TICKET",
        "RELIC.BONE_TEA",
    ];
    array(&player["relics"])
        .iter()
        .enumerate()
        .filter(|(index, relic)| {
            // Newer captures mark melted wax relics directly.
            relic["state"] == "Melted"
                || relic["state"] == "Disabled"
                    && (melted.contains(index)
                        || !USED_UP.iter().any(|id| relic["model_id"] == *id))
        })
        .map(|(index, _)| index)
        .collect()
}

fn used_relic(player: &Value, id: &str) -> Option<bool> {
    player["used_relics"]
        .as_array()
        .map(|relics| relics.iter().any(|relic| relic == id))
}

fn relic_active(player: &Value, id: &str) -> bool {
    array(&player["relics"])
        .iter()
        .any(|relic| relic["model_id"] == id && relic["state"] == "Active")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_reconstructs_distinct_a_b_and_c_b_encounter_bags() {
        let content = foundation_content();
        let (act, run_act, first, shared, second) = content
            .acts
            .iter()
            .enumerate()
            .find_map(|(act, def)| {
                let pool = def
                    .encounters
                    .iter()
                    .copied()
                    .filter(|&id| !content.encounters[id as usize].id.contains("_WEAK"))
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
                        match def.id {
                            "ACT.THE_HIVE" => 2,
                            "ACT.THE_GLORY" => 3,
                            _ => 1,
                        },
                        compatible[0],
                        shared,
                        compatible[1],
                    ))
                })
            })
            .unwrap();
        let record = |episode: &str, floor: u8, id: Id| {
            serde_json::json!({
                "episode_id": episode,
                "before": {
                    "act": run_act,
                    "act_floor": floor,
                    "room_type": "Monster",
                    "room_model_id": content.encounters[id as usize].id,
                    "players": [{"relics": []}],
                },
                "oracle_before": {"rng_seeds": {"run.UpFront": 0}},
            })
        };
        let history = |episode, first| {
            let mut state = ReplayState::default();
            state.prepare(&content, &record(episode, 1, first));
            state.prepare(&content, &record(episode, 2, shared));
            state
        };
        let left = history("left", first);
        let right = history("right", second);
        assert_eq!(left.last_encounter, right.last_encounter);
        assert_eq!(left.regular_encounters_left, right.regular_encounters_left);
        assert_ne!(left.encounters, right.encounters);
        assert!(left.encounters.contains(&second) && !left.encounters.contains(&first));
        assert!(right.encounters.contains(&first) && !right.encounters.contains(&second));
        assert_eq!(left.encounter_act, Some(act));
    }
}
