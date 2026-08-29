use crate::foundation::{
    ANYTIME_POTIONS, AUTOMATIC_POTIONS, COMMON_RELICS, NO_COMBAT_POTIONS, RARE_POTIONS,
    RARE_RELICS, SHOP_RELICS, UNCOMMON_POTIONS, UNCOMMON_RELICS,
};
use crate::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ActionOutcome {
    pub block: Option<f32>,
    pub draw: Option<f32>,
    pub discard: Option<f32>,
    pub exhaust: Option<f32>,
    pub hp_loss: Option<Vec<f32>>,
}

const MAX_EXPECTATION_BRANCHES: usize = 64;

impl Combat {
    fn remove_draw(&mut self, index: usize) -> Card {
        let len = self.draw.len();
        assert!(index < len);
        assert!(self.known_draw_bottom + self.known_draw_top <= len);
        if index < self.known_draw_bottom {
            self.known_draw_bottom -= 1;
        } else if index >= len - self.known_draw_top {
            self.known_draw_top -= 1;
        }
        self.draw.remove(index)
    }

    fn pop_draw(&mut self) -> Option<Card> {
        let index = self.draw.len().checked_sub(1)?;
        Some(self.remove_draw(index))
    }

    fn push_draw(&mut self, card: Card) {
        assert!(self.known_draw_top < u8::MAX as usize);
        self.draw.push(card);
        self.known_draw_top += 1;
    }

    fn prepend_draw(&mut self, cards: Vec<Card>) {
        assert!(self.known_draw_bottom + cards.len() <= u8::MAX as usize);
        self.known_draw_bottom += cards.len();
        self.draw.splice(0..0, cards);
    }

    fn insert_unknown_draw(&mut self, index: usize, card: Card) {
        let len = self.draw.len();
        assert!(index <= len);
        assert!(self.known_draw_bottom + self.known_draw_top <= len);
        if index < self.known_draw_bottom {
            self.known_draw_bottom = index;
        }
        if index > len - self.known_draw_top {
            self.known_draw_top = len - index;
        }
        self.draw.insert(index, card);
    }

    fn forget_draw_order(&mut self) {
        self.known_draw_top = 0;
        self.known_draw_bottom = 0;
    }
}

impl Game {
    fn expectation_choice(&mut self, count: usize) -> Option<usize> {
        if count == 1 {
            return Some(0);
        }
        let expectation = self.expectation.as_mut()?;
        if expectation.next.is_some() {
            return None;
        }
        if expectation.used < expectation.choices.len() {
            let choice = expectation.choices[expectation.used];
            expectation.used += 1;
            return (choice < count).then_some(choice);
        }
        expectation.next = Some(count);
        None
    }

    fn expectation_unknown(&mut self) {
        if let Some(expectation) = &mut self.expectation {
            expectation.unknown = true;
        }
    }

    fn expectation_discard(&mut self, count: usize) {
        if let Some(expectation) = &mut self.expectation {
            expectation.discarded = expectation.discarded.saturating_add(count as i16);
        }
    }

    fn expectation_draw(&mut self) {
        if let Some(expectation) = &mut self.expectation {
            expectation.drawn = expectation.drawn.saturating_add(1);
        }
    }

    pub(crate) fn expected_action_outcome(
        &self,
        content: &Content,
        action: &Action,
    ) -> ActionOutcome {
        let Some(before) = self.combat() else {
            return ActionOutcome::default();
        };
        let turn = before.turn;
        let enemy_turn = before.enemy_turn;
        let block = before.player.block;
        let exhausted = before.history.exhausted;
        let enemies: Vec<_> = before
            .enemies
            .iter()
            .map(|enemy| (enemy.instance, enemy.creature.hp.max(0)))
            .collect();
        let mut pending = vec![(Vec::new(), 1.0f32)];
        let mut total = 0.0;
        let mut outcome = ActionOutcome {
            block: Some(0.0),
            draw: Some(0.0),
            discard: Some(0.0),
            exhaust: Some(0.0),
            hp_loss: Some(vec![0.0; enemies.len()]),
        };
        let mut leaves = 0;
        while let Some((choices, probability)) = pending.pop() {
            let mut next = self.clone();
            next.rngs = Rngs::from_seed(0);
            next.expectation = Some(Expectation {
                choices: choices.clone(),
                ..Expectation::default()
            });
            if next.step(content, action.clone()).is_err() {
                return ActionOutcome::default();
            }
            let expectation = next.expectation.take().unwrap();
            if let Some(count) = expectation.next {
                if count == 0 || pending.len() + leaves + count > MAX_EXPECTATION_BRANCHES {
                    return ActionOutcome::default();
                }
                for choice in 0..count {
                    let mut choices = choices.clone();
                    choices.push(choice);
                    pending.push((choices, probability / count as f32));
                }
                continue;
            }
            let Some(after) = next.combat().filter(|combat| {
                combat.turn == turn && combat.enemy_turn == enemy_turn && combat.choice.is_none()
            }) else {
                return ActionOutcome::default();
            };
            if expectation.unknown || expectation.used != expectation.choices.len() {
                return ActionOutcome::default();
            }
            *outcome.block.as_mut().unwrap() +=
                after.player.block.saturating_sub(block).max(0) as f32 * probability;
            *outcome.draw.as_mut().unwrap() += expectation.drawn as f32 * probability;
            *outcome.discard.as_mut().unwrap() += expectation.discarded as f32 * probability;
            *outcome.exhaust.as_mut().unwrap() +=
                after.history.exhausted.saturating_sub(exhausted).max(0) as f32 * probability;
            for (index, (loss, (instance, hp))) in outcome
                .hp_loss
                .as_mut()
                .unwrap()
                .iter_mut()
                .zip(&enemies)
                .enumerate()
            {
                let after = after
                    .enemies
                    .get(index)
                    .filter(|enemy| enemy.instance == *instance)
                    .map_or(0, |enemy| enemy.creature.hp.max(0));
                *loss += hp.saturating_sub(after).max(0) as f32 * probability;
            }
            total += probability;
            leaves += 1;
        }
        if leaves == 0 || (total - 1.0).abs() > 1e-4 {
            ActionOutcome::default()
        } else {
            outcome
        }
    }

    pub fn run(&self) -> &Run {
        &self.run
    }

    pub fn combat(&self) -> Option<&Combat> {
        match &self.phase {
            Phase::Combat(combat) => Some(combat),
            _ => None,
        }
    }

    fn combat_mut(&mut self) -> Option<&mut Combat> {
        match &mut self.phase {
            Phase::Combat(combat) => Some(combat),
            _ => None,
        }
    }

    pub fn map(&self) -> &Map {
        &self.map
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    fn new(seed: u64, hp: i16, gold: i32, mut deck: Vec<Card>) -> Self {
        let mut next_card = 1;
        for card in &mut deck {
            if card.instance == 0 {
                card.instance = next_card;
            }
            next_card = next_card.max(card.instance.saturating_add(1));
        }
        Self {
            run: Run {
                ascension: 0,
                character: 0,
                hp,
                max_hp: hp,
                gold,
                deck,
                relics: vec![],
                potions: vec![None; 3],
                act: 0,
                floor: 0,
                energy: 3,
                draw: 5,
                orb_slots: 0,
                card_shop_removals: 0,
            },
            map: Map::default(),
            phase: Phase::Map,
            act: 0,
            room: Room::Combat,
            run_queue: vec![],
            resume: None,
            rngs: Rngs::from_seed(seed),
            next_card,
            rarity_offset: -500,
            potion_odds: 40,
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
            visited_events: vec![],
            enemy_starts: vec![],
            seed: seed as u32,
            unknown_odds: [1000, -10_000, 200, 300],
            happy_flower: 0,
            tea_set: 0,
            replacing_potion: false,
            pending_potion: None,
            removal_price: 0,
            event_rng: None,
            event_relic: None,
            event_data: [0; 4],
            event_cards: vec![],
            relic_queue: vec![],
            relic_deques: std::array::from_fn(|_| vec![]),
            shared_relic_deques: std::array::from_fn(|_| vec![]),
            pending_curse: false,
            wongo_combats: None,
            spoils: None,
            event_combat: 0,
            damage_taken: false,
            lasting_candy: 0,
            paels_wing: 0,
            silver_crucible: 0,
            silken_tress: false,
            rerolled_cards: false,
            maw_bank: false,
            silver_treasures: 0,
            golden_compass: None,
            winged_boots: 0,
            rest_used: 0,
            girya: 0,
            pumpkin_candle: 0,
            fishing_rod: 0,
            cooking: false,
            astrolabe: false,
            transform_niche: false,
            paels_tooth: false,
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
            crystal: None,
            reward_gold_parts: vec![],
            fake_merchant: vec![],
            fake_shop: false,
            fake_happy_flower: 0,
            lizard_tail: false,
            nunchaku: 0,
            pendulum: 0,
            pen_nib: 0,
            iron_club: 0,
            joss_paper: 0,
            tuning_fork: 0,
            bone_tea: false,
            tea_of_discourtesy: false,
            galactic_dust: 0,
            book_of_five_rings: 0,
            ember_tea: 0,
            sword_of_stone: 0,
            replaying: false,
            expectation: None,
        }
    }

    pub fn new_character(content: &Content, seed: u64, character: Id) -> Result<Self, Error> {
        Self::new_character_ascension(content, seed, character, 0)
    }

    pub fn new_character_ascension(
        content: &Content,
        seed: u64,
        character: Id,
        ascension: u8,
    ) -> Result<Self, Error> {
        content.validate().map_err(|_| Error::InvalidContent)?;
        let def = *content
            .characters
            .get(character as usize)
            .ok_or(Error::InvalidContent)?;
        let deck = def
            .deck
            .iter()
            .map(|&(id, upgrades)| Card {
                id,
                upgrades,
                ..Card::default()
            })
            .collect();
        let mut game = Self::new(seed, def.hp, def.gold, deck);
        game.run.ascension = ascension.min(10);
        game.run.character = character;
        game.run.energy = def.energy;
        game.run.draw = def.draw;
        game.run.orb_slots = def.orb_slots;
        game.run.relics.extend_from_slice(def.relics);
        game.populate_relics(content, def.relic_pool);
        if ascension >= 4 {
            game.run.potions.pop();
        }
        if ascension >= 5 {
            game.run.deck.push(Card {
                id: content
                    .card_id("CARD.ASCENDERS_BANE")
                    .ok_or(Error::InvalidContent)?,
                instance: game.next_card,
                ..Card::default()
            });
            game.next_card += 1;
        }
        Ok(game)
    }

    fn populate_relics(&mut self, content: &Content, character: &[Id]) {
        let shared = content.acts[0].relics;
        self.shared_relic_deques = relic_deques(shared.iter().copied(), &mut self.rngs.up_front);
        self.relic_deques = relic_deques(
            shared.iter().copied().chain(character.iter().copied()),
            &mut self.rngs.up_front,
        );
    }

    pub fn begin_act(&mut self, content: &Content, act: Id) -> Result<(), Error> {
        if self.combat().is_some() {
            return Err(Error::CombatActive);
        }
        if content.acts.get(act as usize).is_none() {
            return Err(Error::InvalidContent);
        }
        let new_run = self.run.act == 0;
        let standard = matches!(
            content.acts[act as usize].id,
            "ACT.THE_OVERGROWTH" | "ACT.THE_UNDERDOCKS" | "ACT.THE_HIVE" | "ACT.THE_GLORY"
        );
        if new_run {
            self.run.hp = 0;
        }
        if !standard {
            let missing = self.run.max_hp - self.run.hp;
            self.run.hp += if self.run.ascension >= 2 {
                missing * 4 / 5
            } else {
                missing
            };
        }
        self.act = act;
        self.run.act = match content.acts[act as usize].id {
            "ACT.THE_HIVE" => 2,
            "ACT.THE_GLORY" => 3,
            _ => 1,
        };
        self.run.floor = u8::from(new_run && !standard);
        if new_run {
            self.rngs.up_front.forward(2);
        }
        let def = &content.acts[act as usize];
        self.events = def.events.to_vec();
        self.rngs.up_front.shuffle(&mut self.events);
        let weak: Vec<_> = def
            .encounters
            .iter()
            .copied()
            .filter(|&id| content.encounters[id as usize].id.contains("_WEAK"))
            .collect();
        let regular: Vec<_> = def
            .encounters
            .iter()
            .copied()
            .filter(|&id| !content.encounters[id as usize].id.contains("_WEAK"))
            .collect();
        let weak_count = if self.run.act == 3 { 2 } else { 3 };
        let room_count = match self.run.act {
            2 => 14,
            3 => 13,
            _ => 15,
        };
        self.weak_encounters_left = if weak.is_empty() { 0 } else { weak_count as u8 };
        self.regular_encounters_left = if regular.is_empty() {
            0
        } else {
            (room_count - weak_count) as u8
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
        self.bosses[0] = self.pick(def.bosses);
        self.rngs.up_front.forward(1);
        self.bosses[1] = if self.run.ascension >= 10 && self.run.act == 3 {
            let remaining: Vec<_> = def
                .bosses
                .iter()
                .copied()
                .filter(|boss| Some(*boss) != self.bosses[0])
                .collect();
            self.pick(&remaining)
        } else {
            None
        };
        self.bosses_visited = 0;
        self.unknown_odds = [1000, -10_000, 200, 300];
        self.map = self.make_map(content);
        self.spoils = (self.run.act == 2
            && self
                .run
                .deck
                .iter()
                .any(|card| content.cards[card.id as usize].id == "CARD.SPOILS_MAP"))
        .then(|| {
            self.map
                .nodes
                .iter()
                .find(|node| node.room == Room::Treasure)
                .map(|node| (node.lane, node.floor))
        })
        .flatten();
        self.phase = Phase::Map;
        Ok(())
    }

    pub(crate) fn begin_run(&mut self, content: &Content) -> Result<(), Error> {
        let find = |name| {
            content
                .acts
                .iter()
                .position(|act| act.id == name)
                .map(|act| act as Id)
                .ok_or(Error::InvalidContent)
        };
        let acts = [find("ACT.THE_OVERGROWTH")?, find("ACT.THE_UNDERDOCKS")?];
        let act = acts[self.rngs.up_front.below(acts.len() as u32) as usize];
        self.begin_act(content, act)?;
        let ancient = self
            .map
            .nodes
            .iter()
            .position(|node| node.floor == 0)
            .ok_or(Error::InvalidContent)?;
        self.enter_room(content, ancient);
        Ok(())
    }

    fn next_ancient(&mut self, content: &Content) -> Option<Id> {
        let darv = content
            .events
            .iter()
            .position(|event| event.id == "EVENT.DARV")? as Id;
        let choices: &[(&str, u32)] = match self.run.act {
            1 => &[("EVENT.NEOW", 1)],
            2 => &[
                ("EVENT.OROBAS", 7),
                ("EVENT.PAEL", 7),
                ("EVENT.TEZCATARA", 7),
                ("EVENT.DARV", 3),
            ],
            3 if self.visited_events.contains(&darv) => &[
                ("EVENT.NONUPEIPE", 1),
                ("EVENT.TANX", 1),
                ("EVENT.VAKUU", 1),
            ],
            3 => &[
                ("EVENT.NONUPEIPE", 13),
                ("EVENT.TANX", 13),
                ("EVENT.VAKUU", 13),
                ("EVENT.DARV", 3),
            ],
            _ => return None,
        };
        let mut roll = self
            .rngs
            .up_front
            .below(choices.iter().map(|choice| choice.1).sum());
        choices.iter().find_map(|&(name, weight)| {
            if roll < weight {
                content
                    .events
                    .iter()
                    .position(|event| event.id == name)
                    .map(|id| id as Id)
            } else {
                roll -= weight;
                None
            }
        })
    }

    pub fn start_combat(&mut self, content: &Content, encounter: Id) -> Result<(), Error> {
        if self.combat().is_some() {
            return Err(Error::CombatActive);
        }
        let encounter = content
            .encounters
            .get(encounter as usize)
            .ok_or(Error::InvalidContent)?;
        self.damage_taken = false;
        let entry = encounter_entry(encounter.id);
        let seed = self
            .seed
            .wrapping_add(self.run.act.saturating_sub(1) as u32 * 17 + self.run.floor as u32)
            .wrapping_add(hash(entry));
        let mut rng = Rng::from_seed(seed as u64);
        let mut enemy_ids = encounter.enemies.to_vec();
        let mut generated_starts = vec![];
        let mut hp_reductions = vec![];
        match entry {
            "BOWLBUGS_NORMAL" => {
                let mut workers = vec![8, 11, 9];
                enemy_ids = vec![10];
                for _ in 0..2 {
                    let index = rng.below(workers.len() as u32) as usize;
                    enemy_ids.push(workers.remove(index));
                }
            }
            "BOWLBUGS_WEAK" => enemy_ids = vec![10, [8, 9][rng.below(2) as usize]],
            "CORPSE_SLUGS_NORMAL" | "CORPSE_SLUGS_WEAK" => {
                let start = rng.below(3) as usize;
                generated_starts = (0..enemy_ids.len()).map(|i| (start + i) % 3).collect();
            }
            "DECIMILLIPEDE_ELITE" | "TWO_TAILED_RATS_NORMAL" => {
                let start = rng.below(3) as usize;
                generated_starts = (0..3).map(|i| (start + i) % 3).collect();
            }
            "EXOSKELETONS_NORMAL" => {
                generated_starts = vec![0, 1, 2, rng.below(2) as usize];
            }
            "EXOSKELETONS_WEAK" => generated_starts = vec![0, 1, 2],
            "FLYCONID_NORMAL" => enemy_ids = vec![[1, 106][rng.below(2) as usize], 32],
            "INKLETS_NORMAL" => generated_starts = vec![0, 1, 0],
            "MYTES_NORMAL" => generated_starts = vec![0, 2],
            "PHANTASMAL_GARDENERS_ELITE" => generated_starts = vec![2, 0, 1, 3],
            "PUNCH_OFF_EVENT_ENCOUNTER" => {
                enemy_ids = vec![63, 63];
                generated_starts = vec![1, 0];
                hp_reductions = vec![rng.range(2, 9), rng.range(2, 9)];
            }
            "RUBY_RAIDERS_NORMAL" => {
                let mut raiders = vec![67, 66, 68, 69, 70];
                enemy_ids.clear();
                for _ in 0..3 {
                    let index = rng.below(raiders.len() as u32) as usize;
                    enemy_ids.push(raiders.remove(index));
                }
            }
            "SCROLLS_OF_BITING_NORMAL" | "SCROLLS_OF_BITING_WEAK" => {
                let start = rng.below(3) as usize;
                generated_starts = (0..enemy_ids.len()).map(|i| (start + i) % 3).collect();
                if generated_starts.len() == 4 {
                    generated_starts[3] = 2;
                }
            }
            "SLIMES_NORMAL" => {
                let leaf_first = rng.below(2) == 0;
                enemy_ids = vec![
                    106,
                    1,
                    if leaf_first { 0 } else { 107 },
                    if leaf_first { 107 } else { 0 },
                ];
            }
            "SLIMES_WEAK" => {
                let mut small = vec![0, 107];
                let first = small.remove(rng.below(2) as usize);
                let second = small.remove(rng.below(1) as usize);
                enemy_ids = vec![first, [1, 106][rng.below(2) as usize], second];
            }
            "SLITHERING_STRANGLER_NORMAL" => {
                enemy_ids = match rng.below(3) {
                    0 => vec![80],
                    1 => vec![[1, 106][rng.below(2) as usize]],
                    _ => vec![
                        [0, 107][rng.below(2) as usize],
                        [0, 107][rng.below(2) as usize],
                    ],
                };
                enemy_ids.push(77);
            }
            _ => {}
        }
        let supplied_starts = std::mem::take(&mut self.enemy_starts);
        let enemy_starts = if supplied_starts.is_empty() {
            generated_starts
        } else {
            supplied_starts
        };
        let mut enemies = Vec::with_capacity(enemy_ids.len());
        for (index, &id) in enemy_ids.iter().enumerate() {
            let def = content
                .enemies
                .get(id as usize)
                .ok_or(Error::InvalidContent)?;
            let range = def.hp(self.run.ascension);
            let candidates: Vec<_> = range
                .clone()
                .filter(|hp| {
                    enemies
                        .iter()
                        .all(|enemy: &Enemy| enemy.creature.max_hp != *hp)
                })
                .collect();
            let hp = if candidates.is_empty() {
                self.rngs.niche.range(*range.start(), *range.end())
            } else {
                candidates[self.rngs.niche.below(candidates.len() as u32) as usize]
            };
            let powers = def
                .powers(self.run.ascension)
                .into_iter()
                .map(|(id, amount)| Power {
                    id,
                    amount,
                    skip_next_decay: false,
                    value: 0,
                })
                .collect();
            enemies.push(Enemy {
                instance: index as u32 + 2,
                creature: Creature {
                    id,
                    hp: hp - hp_reductions.get(index).copied().unwrap_or_default(),
                    max_hp: hp,
                    block: 0,
                    powers,
                },
                move_index: {
                    let generated = match def.id {
                        "MONSTER.LEAF_SLIME_S" => self.rngs.monster_ai.below(2) as usize,
                        "MONSTER.TWIG_SLIME_M" => 1,
                        "MONSTER.FLAIL_KNIGHT" | "MONSTER.MYSTERIOUS_KNIGHT" => 2,
                        "MONSTER.FABRICATOR" => self.rngs.monster_ai.below(2) as usize,
                        "MONSTER.FLYCONID" => {
                            if self.rngs.monster_ai.below(3) < 2 {
                                1
                            } else {
                                2
                            }
                        }
                        "MONSTER.KIN_FOLLOWER" if index == 0 && enemy_ids.len() == 3 => 2,
                        "MONSTER.CHOMPER" if index == 1 && enemy_ids.len() == 2 => 1,
                        "MONSTER.NIBBIT" if enemy_ids.len() == 2 => {
                            if index == 0 {
                                1
                            } else {
                                2
                            }
                        }
                        "MONSTER.TOADPOLE" if enemy_ids.len() == 2 => {
                            if index == 0 {
                                2
                            } else {
                                1
                            }
                        }
                        _ => def.start,
                    };
                    enemy_starts.get(index).copied().unwrap_or(generated)
                },
                last_move: usize::MAX,
                repeats: 0,
                move_history: vec![],
                stunned: false,
                value: 0,
            });
        }
        for index in 0..enemies.len() {
            let def = &content.enemies[enemies[index].creature.id as usize];
            if !def.id.starts_with("MONSTER.DECIMILLIPEDE_SEGMENT_") {
                continue;
            }
            let range = def.hp(self.run.ascension);
            let mut hp = enemies[index].creature.max_hp;
            hp += hp % 2;
            while enemies
                .iter()
                .enumerate()
                .any(|(other, enemy)| other != index && enemy.creature.max_hp == hp)
            {
                hp += 2;
                if hp > *range.end() {
                    hp = *range.start();
                }
            }
            enemies[index].creature.hp = hp;
            enemies[index].creature.max_hp = hp;
        }
        for enemy in &mut enemies {
            enemy.creature.block = enemy.creature.power(power_id::PLATING).max(0);
            if content.enemies[enemy.creature.id as usize].id == "MONSTER.CUBEX_CONSTRUCT" {
                enemy.creature.block += 13;
            }
        }
        if self.fur_coat_active() {
            for enemy in &mut enemies {
                enemy.creature.hp = 1;
            }
        }
        for card in &mut self.run.deck {
            if card.instance == 0 {
                card.instance = self.next_card;
                self.next_card = self.next_card.saturating_add(1);
            }
        }
        let mut draw = self.run.deck.clone();
        for card in &mut draw {
            card.enchantment_value = match card.enchantment {
                Some(Enchantment::Glam) => 1,
                Some(Enchantment::Goopy) => card.enchantment_amount,
                Some(Enchantment::Sown | Enchantment::Swift | Enchantment::Vigorous) => {
                    card.enchantment_amount
                }
                _ => 0,
            };
        }
        self.rngs.shuffle.shuffle(&mut draw);
        draw.reverse();
        draw.sort_by_key(|card| {
            let def = content.cards[card.id as usize];
            (card.flags(def) & INNATE != 0) as u8
        });
        if self.has_relic(content, "RELIC.GHOST_SEED") {
            for card in &mut draw {
                let def = content.cards[card.id as usize];
                if def.rarity == CardRarity::Basic && def.tags & (STRIKE_TAG | DEFEND_TAG) != 0 {
                    card.flags |= ETHEREAL;
                }
            }
        }
        let enemy_count = enemies.len();
        self.phase = Phase::Combat(Box::new(Combat {
            player: Creature {
                id: 0,
                hp: self.run.hp,
                max_hp: self.run.max_hp,
                block: 0,
                powers: vec![],
            },
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
            hand: vec![],
            discard: vec![],
            exhaust: vec![],
            offer: vec![],
            energy: self.run.energy as i16
                + (self.pumpkin_candle > 0) as i16
                + self.has_relic(content, "RELIC.BREAD") as i16,
            max_energy: self.run.energy as i16
                + (self.pumpkin_candle > 0) as i16
                + self.has_relic(content, "RELIC.BREAD") as i16,
            draw_per_turn: self.run.draw,
            stars: 0,
            turn: 0,
            orbs: vec![],
            orb_slots: self.run.orb_slots,
            history: History::default(),
            last_cards: 0,
            orbit_spent: 0,
            hits: vec![0; enemy_count],
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
            dampened: vec![],
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
            kusarigama: 0,
            unsettling_lamp: None,
            unsettling_used: false,
            diamond_diadem: false,
            belt_buckle: false,
            burning_sticks: false,
            red_skull: false,
            throwing_axe: false,
            paels_eye: false,
            paels_eye_extra: false,
            paels_legion: 0,
            history_course: None,
        }));
        let crusher =
            self.combat().unwrap().enemies.iter().position(|enemy| {
                content.enemies[enemy.creature.id as usize].id == "MONSTER.CRUSHER"
            });
        let rocket =
            self.combat().unwrap().enemies.iter().position(|enemy| {
                content.enemies[enemy.creature.id as usize].id == "MONSTER.ROCKET"
            });
        if let (Some(crusher), Some(rocket)) = (crusher, rocket) {
            self.creature_mut(Actor::Player)
                .add_power(power_id::SURROUNDED, 1);
            for (enemy, back) in [
                (crusher, power_id::BACK_ATTACK_LEFT),
                (rocket, power_id::BACK_ATTACK_RIGHT),
            ] {
                self.creature_mut(Actor::Enemy(enemy)).add_power(back, 1);
                self.creature_mut(Actor::Enemy(enemy))
                    .add_power(power_id::CRAB_RAGE, 1);
            }
        }
        for (index, relic) in self.run.relics.clone().into_iter().enumerate() {
            if self.melted_relics.contains(&index) {
                continue;
            }
            match content.relics[relic as usize].id {
                "RELIC.DELICATE_FROND" => {
                    while self.run.potions.iter().any(Option::is_none) {
                        let pool = self.potion_pool(content);
                        let potion = self.random_potions(&pool, 1, false, false).pop();
                        if self.has_relic(content, "RELIC.SOZU") {
                            break;
                        }
                        let Some(potion) = potion else { break };
                        *self
                            .run
                            .potions
                            .iter_mut()
                            .find(|slot| slot.is_none())
                            .unwrap() = Some(potion);
                    }
                }
                "RELIC.PETRIFIED_TOAD" if !self.has_relic(content, "RELIC.SOZU") => {
                    if let Some(id) = content.potion_id("POTION.POTION_SHAPED_ROCK")
                        && let Some(slot) = self.run.potions.iter_mut().find(|slot| slot.is_none())
                    {
                        *slot = Some(id);
                    }
                }
                "RELIC.PHILOSOPHERS_STONE" => {
                    for enemy in &mut self.combat_mut().unwrap().enemies {
                        enemy.creature.add_power(power_id::STRENGTH, 1);
                    }
                }
                "RELIC.GORGET" => self.apply_power(content, Actor::Player, power_id::PLATING, 4),
                "RELIC.BELT_BUCKLE" if self.run.potions.iter().all(Option::is_none) => {
                    self.combat_mut().unwrap().belt_buckle = true;
                    self.apply_power(content, Actor::Player, power_id::DEXTERITY, 2);
                }
                "RELIC.EMBER_TEA" if self.ember_tea < 5 => {
                    self.ember_tea += 1;
                    self.apply_power(content, Actor::Player, power_id::STRENGTH, 2);
                }
                "RELIC.SLING_OF_COURAGE" if self.room == Room::Elite => {
                    self.apply_power(content, Actor::Player, power_id::STRENGTH, 2)
                }
                "RELIC.SWORD_OF_JADE" => {
                    self.apply_power(content, Actor::Player, power_id::STRENGTH, 3)
                }
                "RELIC.TEA_OF_DISCOURTESY" if !self.tea_of_discourtesy => {
                    self.tea_of_discourtesy = true;
                    for _ in 0..2 {
                        self.add_random(
                            content,
                            Pile::Draw,
                            Card {
                                id: card_id::DAZED,
                                ..Card::default()
                            },
                        );
                    }
                }
                "RELIC.STONE_CRACKER" => {
                    let mut cards: Vec<_> = self
                        .combat()
                        .unwrap()
                        .draw
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            card.upgrades == 0
                                && !matches!(
                                    card.card_type(content.cards[card.id as usize]),
                                    CardType::Status | CardType::Curse | CardType::Quest
                                )
                        })
                        .map(|(index, _)| index)
                        .collect();
                    self.rngs.combat_card_selection.shuffle(&mut cards);
                    for index in cards.into_iter().take(2) {
                        self.combat_mut().unwrap().draw[index].upgrades = 1;
                    }
                }
                _ => {}
            }
        }
        if self.has_relic(content, "RELIC.RED_SKULL")
            && self.creature(Actor::Player).hp * 2 <= self.creature(Actor::Player).max_hp
        {
            self.combat_mut().unwrap().red_skull = true;
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 3);
        }
        if self.girya > 0 {
            let girya = self.girya as i16;
            self.combat_mut()
                .unwrap()
                .player
                .add_power(power_id::STRENGTH, girya);
        }
        self.start_turn(content);
        self.trigger(content, Trigger::CombatStart, Actor::Player, 0);
        self.resolve(content);
        Ok(())
    }

    pub fn actions(&self, content: &Content) -> Vec<Action> {
        let Some(combat) = self.combat() else {
            return self.run_actions(content);
        };
        if combat.player.hp <= 0 {
            return vec![];
        }
        if let Some(choice) = combat.choice {
            let pile = cards(combat, choice.pile);
            let mut actions: Vec<_> = pile
                .iter()
                .enumerate()
                .filter(|(_, card)| eligible(content, card, choice.filter, choice.op))
                .map(|(i, _)| Action::Choose(i))
                .collect();
            if choice.optional {
                actions.push(Action::Done);
            }
            return actions;
        }
        if !combat.queue.is_empty() {
            return vec![];
        }
        let mut actions = Vec::new();
        let spiked_gauntlets = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
        let brilliant_scarf =
            self.has_relic(content, "RELIC.BRILLIANT_SCARF") && combat.history.manual_plays == 4;
        let velvet_choker =
            self.has_relic(content, "RELIC.VELVET_CHOKER") && combat.history.cards >= 6;
        let sloth = combat.player.power(power_id::SLOTH);
        let cards_locked = combat.player.power(power_id::RINGING) > 0 && combat.history.cards > 0
            || sloth > 0 && combat.history.cards >= sloth
            || combat.history.cards >= 3
                && combat.hand.iter().any(|card| {
                    let def = content.cards[card.id as usize];
                    card.flags(def) & LIMIT_THREE != 0
                });
        for (hand, card) in combat.hand.iter().enumerate() {
            let Some(def) = content.cards.get(card.id as usize) else {
                continue;
            };
            let cost = if brilliant_scarf {
                0
            } else {
                energy_cost(combat, *card, *def, spiked_gauntlets)
            };
            let playable = match def.playable {
                PlayCondition::Always => true,
                PlayCondition::AttacksOnly => combat.hand.iter().enumerate().all(|(i, card)| {
                    i == hand || card.card_type(content.cards[card.id as usize]) == CardType::Attack
                }),
                PlayCondition::EmptyDrawPile => combat.draw.is_empty(),
                PlayCondition::OstyAlive => combat.osty.hp > 0,
            };
            if cards_locked
                || velvet_choker
                || !playable
                || card.flags(*def) & UNPLAYABLE != 0
                || def.card_type == CardType::Skill
                    && combat.player.power(power_id::SMOGGY) > 0
                    && combat.history.skills > 0
                || cost > combat.energy
                || if brilliant_scarf {
                    0
                } else {
                    star_cost(combat, *card, *def)
                } > combat.stars
            {
                continue;
            }
            let target_type = if card.id == card_id::SOVEREIGN_BLADE
                && combat.player.power(power_id::SEEKING_EDGE) > 0
            {
                Target::AllEnemies
            } else if def.id == "CARD.SHIV" && combat.player.power(power_id::FAN_OF_KNIVES) > 0 {
                Target::AllEnemies
            } else {
                card.target(*def)
            };
            if target_type == Target::ChosenEnemy {
                actions.extend(
                    combat
                        .enemies
                        .iter()
                        .enumerate()
                        .filter(|(_, x)| x.creature.hp > 0)
                        .map(|(target, _)| Action::Play {
                            hand,
                            target: Some(target),
                        }),
                );
            } else {
                actions.push(Action::Play { hand, target: None });
            }
        }
        for (slot, potion) in self.run.potions.iter().enumerate() {
            let Some(id) = potion else { continue };
            if AUTOMATIC_POTIONS.contains(id) {
                continue;
            }
            let Some(def) = content.potions.get(*id as usize) else {
                continue;
            };
            if def.target == Target::ChosenEnemy {
                actions.extend(
                    combat
                        .enemies
                        .iter()
                        .enumerate()
                        .filter(|(_, x)| x.creature.hp > 0)
                        .map(|(target, _)| Action::Potion {
                            slot,
                            target: Some(target),
                        }),
                );
            } else {
                actions.push(Action::Potion { slot, target: None });
            }
            actions.push(Action::DiscardPotion(slot));
        }
        actions.push(Action::EndTurn);
        actions
    }

    pub fn step(&mut self, content: &Content, action: Action) -> Result<(), Error> {
        if !self.actions(content).contains(&action)
            && !(matches!(self.phase, Phase::Event(..))
                && matches!(action, Action::EventRelic(..) | Action::EventCard(..)))
        {
            return Err(Error::InvalidAction);
        }
        match action {
            Action::Play { hand, target } => self.play(content, hand, target),
            Action::Potion { slot, target } => self.use_potion(content, slot, target),
            Action::DiscardPotion(slot) => {
                self.run.potions[slot] = self.pending_potion.take();
                self.replacing_potion = false;
                self.sync_belt_buckle(content);
            }
            Action::Choose(index) if self.combat().is_some() => self.choose(content, index),
            Action::Choose(index) => match std::mem::replace(&mut self.phase, Phase::Map) {
                Phase::ChooseCards(mut cards, remaining, can_skip) => {
                    let card = cards.remove(index);
                    self.add_card(content, card);
                    if remaining > 1 {
                        self.phase = Phase::ChooseCards(cards, remaining - 1, can_skip);
                    } else {
                        self.finish_run_choice(content);
                    }
                }
                Phase::ChooseBundles(mut bundles) => {
                    for card in bundles.remove(index) {
                        self.add_card(content, card);
                    }
                    self.finish_run_choice(content);
                }
                _ => unreachable!(),
            },
            Action::Done => {
                let combat = self.combat_mut().unwrap();
                if combat
                    .choice
                    .is_some_and(|choice| choice.pile == Pile::Offer)
                {
                    combat.offer.clear();
                }
                combat.choice = None;
            }
            Action::EndTurn => self.end_turn(content),
            action => self.run_step(content, action),
        }
        self.finish_empty_run_choices(content);
        if self
            .combat()
            .is_some_and(|combat| combat.ending && combat.choice.is_none())
        {
            self.finish_end_turn(content);
        }
        if self.combat().is_some() {
            self.resolve(content);
            self.finish_play(content);
            self.resolve(content);
            if self.combat().is_some_and(|combat| {
                combat.force_end && combat.choice.is_none() && combat.queue.is_empty()
            }) {
                self.combat_mut().unwrap().force_end = false;
                self.end_turn(content);
            }
            self.finish_combat(content);
        }
        if self.combat().is_none() && self.run.hp <= 0 && !matches!(self.phase, Phase::Won) {
            self.phase = Phase::Dead;
            self.resume = None;
        }
        Ok(())
    }

    fn make_map(&mut self, content: &Content) -> Map {
        #[derive(Clone)]
        struct Node {
            col: usize,
            row: usize,
            room: Option<Room>,
            parents: Vec<usize>,
            children: Vec<usize>,
            fixed: bool,
        }

        fn add(nodes: &mut [Node], parent: usize, child: usize) {
            if !nodes[parent].children.contains(&child) {
                nodes[parent].children.push(child);
            }
            if !nodes[child].parents.contains(&parent) {
                nodes[child].parents.push(parent);
            }
        }

        fn valid(nodes: &[Node], length: usize, id: usize, room: Room) -> bool {
            let node = &nodes[id];
            if node.row < 6 && matches!(room, Room::Rest | Room::Elite)
                || node.row >= length - 3 && room == Room::Rest
            {
                return false;
            }
            let consecutive =
                matches!(room, Room::Elite | Room::Rest | Room::Treasure | Room::Shop);
            if consecutive
                && node
                    .parents
                    .iter()
                    .chain(&node.children)
                    .any(|&other| nodes[other].room == Some(room))
            {
                return false;
            }
            if matches!(
                room,
                Room::Rest | Room::Combat | Room::Unknown | Room::Elite | Room::Shop
            ) && node.parents.iter().any(|&parent| {
                nodes[parent]
                    .children
                    .iter()
                    .any(|&sibling| sibling != id && nodes[sibling].room == Some(room))
            }) {
                return false;
            }
            true
        }

        fn paths(nodes: &[Node], id: usize, boss: usize) -> Vec<Vec<usize>> {
            if id == boss {
                return vec![vec![id]];
            }
            let mut result = Vec::new();
            for &child in &nodes[id].children {
                for mut path in paths(nodes, child, boss) {
                    path.insert(0, id);
                    result.push(path);
                }
            }
            result
        }

        fn duplicate_segments(nodes: &[Node], ancient: usize, boss: usize) -> Vec<Vec<Vec<usize>>> {
            use std::collections::BTreeMap;
            let mut groups: BTreeMap<String, Vec<Vec<usize>>> = BTreeMap::new();
            for path in paths(nodes, ancient, boss) {
                for start in 0..path.len() - 1 {
                    let first = &nodes[path[start]];
                    if first.children.len() <= 1 && first.row != 0 {
                        continue;
                    }
                    for offset in 2..path.len() - start {
                        let segment = path[start..=start + offset].to_vec();
                        let last = &nodes[*segment.last().unwrap()];
                        if last.parents.len() < 2 {
                            continue;
                        }
                        let prefix = if first.row == 0 {
                            format!("0-{},{}-", last.col, last.row)
                        } else {
                            format!("{},{}-{},{}-", first.col, first.row, last.col, last.row)
                        };
                        let key = prefix
                            + &segment
                                .iter()
                                .map(|&id| match nodes[id].room {
                                    Some(Room::Unknown | Room::Event) => 1,
                                    Some(Room::Shop) => 2,
                                    Some(Room::Treasure) => 3,
                                    Some(Room::Rest) => 4,
                                    Some(Room::Combat) => 5,
                                    Some(Room::Elite) => 6,
                                    Some(Room::Boss) => 7,
                                    None => 8,
                                })
                                .map(|room| room.to_string())
                                .collect::<Vec<_>>()
                                .join(",");
                        let group = groups.entry(key).or_default();
                        if !group.iter().any(|other| {
                            (1..other.len() - 1).any(|index| other[index] == segment[index])
                        }) {
                            group.push(segment);
                        }
                    }
                }
            }
            groups
                .into_values()
                .filter(|group| group.len() > 1)
                .collect()
        }

        fn remove_edge(nodes: &mut [Node], parent: usize, child: usize) {
            nodes[parent].children.retain(|&id| id != child);
            nodes[child].parents.retain(|&id| id != parent);
        }

        fn remove_node(
            nodes: &mut [Node],
            grid: &mut [[Option<usize>; 7]],
            starts: &mut Vec<usize>,
            id: usize,
        ) {
            grid[nodes[id].row][nodes[id].col] = None;
            starts.retain(|&start| start != id);
            let children = nodes[id].children.clone();
            let parents = nodes[id].parents.clone();
            for child in children {
                remove_edge(nodes, id, child);
            }
            for parent in parents {
                remove_edge(nodes, parent, id);
            }
        }

        fn prune(
            nodes: &mut [Node],
            grid: &mut [[Option<usize>; 7]],
            starts: &mut Vec<usize>,
            ancient: usize,
            boss: usize,
            rng: &mut Rng,
        ) {
            for _ in 0..=50 {
                let groups = duplicate_segments(nodes, ancient, boss);
                let mut changed = false;
                'groups: for mut group in groups {
                    rng.shuffle(&mut group);
                    let mut removed = 0;
                    for segment in &group {
                        if removed == group.len() - 1 {
                            changed = removed != 0;
                            break;
                        }
                        let mut segment_changed = false;
                        for index in 0..segment.len() - 1 {
                            let id = segment[index];
                            if id != ancient
                                && grid[nodes[id].row][nodes[id].col].is_none()
                                && id != boss
                            {
                                segment_changed = true;
                                break;
                            }
                            if nodes[id].children.len() > 1
                                || nodes[id].parents.len() > 1
                                || nodes[id].parents.iter().any(|&parent| {
                                    nodes[parent].children.len() == 1
                                        && parent != ancient
                                        && grid[nodes[parent].row][nodes[parent].col].is_some()
                                })
                            {
                                continue;
                            }
                            if segment[index..].iter().any(|&node| {
                                nodes[node].children.len() > 1 && nodes[node].parents.len() == 1
                            }) {
                                continue;
                            }
                            if nodes[*segment.last().unwrap()].parents.len() == 1 {
                                segment_changed = false;
                                break;
                            }
                            if !nodes[id].children.iter().any(|child| {
                                !segment.contains(child) && nodes[*child].parents.len() == 1
                            }) {
                                remove_node(nodes, grid, starts, id);
                                segment_changed = true;
                            }
                        }
                        if segment_changed {
                            removed += 1;
                        }
                    }
                    if removed != 0 {
                        changed = true;
                    } else {
                        for segment in &group {
                            for pair in segment.windows(2) {
                                if nodes[pair[0]].children.len() >= 2
                                    && nodes[pair[1]].parents.len() != 1
                                {
                                    remove_edge(nodes, pair[0], pair[1]);
                                    changed = true;
                                    break 'groups;
                                }
                            }
                        }
                    }
                    if changed {
                        break;
                    }
                }
                if !changed {
                    break;
                }
            }
        }

        let length = match self.run.act {
            1 => 14,
            2 => 15,
            _ => 16,
        };
        let spoils = self.run.act == 2
            && self
                .run
                .deck
                .iter()
                .any(|card| content.cards[card.id as usize].id == "CARD.SPOILS_MAP");
        let rng_name = if spoils {
            "spoils_map".to_owned()
        } else {
            format!("act_{}_map", self.run.act)
        };
        let mut rng = Rng::from_seed(self.seed.wrapping_add(hash(&rng_name)) as u64);
        let (rests, unknowns) = match self.run.act {
            1 => (
                5 + rng.below(2) as usize,
                rng.gaussian(12.0, 1.0, 10, 14) as usize - 1,
            ),
            2 => (
                rng.gaussian(6.0, 1.0, 6, 7) as usize,
                rng.gaussian(12.0, 1.0, 10, 14) as usize - 1,
            ),
            _ => (
                rng.gaussian(7.0, 1.0, 6, 7) as usize,
                rng.gaussian(12.0, 1.0, 10, 14) as usize,
            ),
        };
        let mut nodes = Vec::new();
        let mut grid: Vec<[Option<usize>; 7]> = vec![[None; 7]; length];
        let point = |nodes: &mut Vec<Node>,
                     grid: &mut Vec<[Option<usize>; 7]>,
                     col: usize,
                     row: usize|
         -> usize {
            if let Some(id) = grid[row][col] {
                return id;
            }
            let id = nodes.len();
            nodes.push(Node {
                col,
                row,
                room: None,
                parents: vec![],
                children: vec![],
                fixed: false,
            });
            grid[row][col] = Some(id);
            id
        };
        let ancient = nodes.len();
        nodes.push(Node {
            col: 3,
            row: 0,
            room: Some(Room::Event),
            parents: vec![],
            children: vec![],
            fixed: true,
        });
        let mut starts = Vec::new();
        for iteration in 0..7 {
            let mut current = point(&mut nodes, &mut grid, rng.below(7) as usize, 1);
            if iteration == 1 {
                while starts.contains(&current) {
                    current = point(&mut nodes, &mut grid, rng.below(7) as usize, 1);
                }
            }
            if !starts.contains(&current) {
                starts.push(current);
            }
            while nodes[current].row < length - 1 {
                let col = nodes[current].col;
                let row = nodes[current].row + 1;
                let treasure_row = length - 7;
                let mut directions = vec![-1i8, 0, 1];
                if spoils
                    && treasure_row > nodes[current].row
                    && treasure_row - nodes[current].row <= 3
                {
                    let toward = (3i8 - col as i8).signum();
                    directions = if toward == 0 {
                        vec![0, -1, 1]
                    } else {
                        vec![toward, 0, -toward]
                    };
                } else {
                    rng.shuffle(&mut directions);
                }
                let (min, max) = if spoils {
                    let spread = 3
                        .min(row.abs_diff(treasure_row))
                        .min(3.min(length - 1 - row + 1));
                    (3 - spread, 3 + spread)
                } else {
                    (0, 6)
                };
                let next = directions
                    .into_iter()
                    .find_map(|direction| {
                        let target = (col as i8 + direction).clamp(0, 6) as usize;
                        let delta = target as i8 - col as i8;
                        let crossover = delta != 0
                            && grid[nodes[current].row][target].is_some_and(|other| {
                                nodes[other]
                                    .children
                                    .iter()
                                    .any(|&child| nodes[child].col as i8 - target as i8 == -delta)
                            });
                        let existing = grid[row][target];
                        (target >= min
                            && target <= max
                            && !crossover
                            && existing.is_none_or(|id| {
                                (nodes[id].parents.contains(&current)
                                    || nodes[id].parents.len() < 3)
                                    && (nodes[current].children.len() < 3
                                        || nodes[current].children.contains(&id))
                            }))
                        .then_some(target)
                    })
                    .unwrap_or_else(|| col.clamp(min, max));
                let child = point(&mut nodes, &mut grid, next, row);
                add(&mut nodes, current, child);
                current = child;
            }
        }
        if spoils {
            let row = length - 7;
            let treasure = point(&mut nodes, &mut grid, 3, row);
            nodes[treasure].room = Some(Room::Treasure);
            nodes[treasure].fixed = true;
            for col in 0..7 {
                let Some(stray) = grid[row][col].filter(|&id| id != treasure) else {
                    continue;
                };
                for parent in nodes[stray].parents.clone() {
                    remove_edge(&mut nodes, parent, stray);
                    add(&mut nodes, parent, treasure);
                }
                for child in nodes[stray].children.clone() {
                    remove_edge(&mut nodes, stray, child);
                    add(&mut nodes, treasure, child);
                }
                grid[row][col] = None;
            }
        }
        let boss = nodes.len();
        nodes.push(Node {
            col: 3,
            row: length,
            room: Some(Room::Boss),
            parents: vec![],
            children: vec![],
            fixed: true,
        });
        for id in grid[length - 1].into_iter().flatten() {
            add(&mut nodes, id, boss);
        }
        for &id in &starts {
            add(&mut nodes, ancient, id);
        }
        for id in grid[length - 1].into_iter().flatten() {
            nodes[id].room = Some(Room::Rest);
            nodes[id].fixed = true;
        }
        if !spoils {
            for id in grid[length - 7].into_iter().flatten() {
                nodes[id].room = Some(Room::Treasure);
                nodes[id].fixed = true;
            }
        }
        for id in grid[1].into_iter().flatten() {
            nodes[id].room = Some(Room::Combat);
            nodes[id].fixed = true;
        }
        let mut rooms = Vec::new();
        rooms.extend(std::iter::repeat_n(Room::Rest, rests));
        rooms.extend(std::iter::repeat_n(Room::Shop, 3));
        rooms.extend(std::iter::repeat_n(
            Room::Elite,
            if self.run.ascension >= 1 { 8 } else { 5 },
        ));
        rooms.extend(std::iter::repeat_n(Room::Unknown, unknowns));
        for _ in 0..3 {
            if rooms.is_empty() {
                break;
            }
            let mut unassigned = Vec::new();
            for col in 0..7 {
                for row in 0..length {
                    if let Some(id) = grid[row][col].filter(|&id| nodes[id].room.is_none()) {
                        unassigned.push(id);
                    }
                }
            }
            unassigned.sort_by_key(|&id| (nodes[id].col, nodes[id].row));
            rng.shuffle(&mut unassigned);
            for id in unassigned {
                let mut assigned = None;
                for _ in 0..rooms.len() {
                    let room = rooms.remove(0);
                    if valid(&nodes, length, id, room) {
                        assigned = Some(room);
                        break;
                    }
                    rooms.push(room);
                }
                nodes[id].room = assigned;
                if rooms.is_empty() {
                    break;
                }
            }
        }
        for row in &grid {
            for id in row.into_iter().flatten() {
                nodes[*id].room.get_or_insert(Room::Combat);
            }
        }
        for _ in 0..3 {
            prune(&mut nodes, &mut grid, &mut starts, ancient, boss, &mut rng);
            let mut repaired = false;
            for (room, target) in [
                (Room::Shop, 3),
                (Room::Elite, if self.run.ascension >= 1 { 8 } else { 5 }),
                (Room::Rest, rests),
                (Room::Unknown, unknowns),
            ] {
                let current = grid
                    .iter()
                    .flatten()
                    .flatten()
                    .filter(|&&id| nodes[id].room == Some(room))
                    .count();
                let mut missing = target.saturating_sub(current);
                if missing == 0 {
                    continue;
                }
                let mut candidates: Vec<_> = grid
                    .iter()
                    .flatten()
                    .flatten()
                    .copied()
                    .filter(|&id| nodes[id].room == Some(Room::Combat) && !nodes[id].fixed)
                    .collect();
                candidates.sort_by_key(|&id| (nodes[id].col, nodes[id].row));
                rng.shuffle(&mut candidates);
                for id in candidates {
                    if valid(&nodes, length, id, room) {
                        nodes[id].room = Some(room);
                        missing -= 1;
                        repaired = true;
                        if missing == 0 {
                            break;
                        }
                    }
                }
            }
            if !repaired {
                break;
            }
        }
        let second = if self.run.ascension >= 10 && self.run.act == 3 {
            let id = nodes.len();
            nodes.push(Node {
                col: 3,
                row: length + 1,
                room: Some(Room::Boss),
                parents: vec![],
                children: vec![],
                fixed: true,
            });
            add(&mut nodes, boss, id);
            Some(id)
        } else {
            None
        };

        if !spoils {
            let left_empty =
                (0..length).all(|row| grid[row][0].is_none() && grid[row][1].is_none());
            let right_empty =
                (0..length).all(|row| grid[row][5].is_none() && grid[row][6].is_none());
            let shift = match (left_empty, right_empty) {
                (true, false) => -1,
                (false, true) => 1,
                _ => 0,
            };
            if shift != 0 {
                for row in 0..length {
                    let old = grid[row];
                    grid[row] = [None; 7];
                    for id in old.into_iter().flatten() {
                        let col = (nodes[id].col as i8 + shift) as usize;
                        nodes[id].col = col;
                        grid[row][col] = Some(id);
                    }
                }
            }
            for row in 0..length {
                let row_nodes: Vec<_> = grid[row].into_iter().flatten().collect();
                loop {
                    let mut moved = false;
                    for &id in &row_nodes {
                        let old = nodes[id].col;
                        let gap = |col: usize| {
                            row_nodes
                                .iter()
                                .filter(|&&other| other != id)
                                .map(|&other| col.abs_diff(nodes[other].col))
                                .min()
                                .unwrap_or(usize::MAX)
                        };
                        let allowed = (0..7).filter(|&col| {
                            nodes[id]
                                .parents
                                .iter()
                                .chain(&nodes[id].children)
                                .all(|&other| nodes[other].col.abs_diff(col) <= 1)
                        });
                        let next = allowed
                            .filter(|&col| col == old || grid[row][col].is_none())
                            .max_by_key(|&col| (gap(col), std::cmp::Reverse(col)))
                            .unwrap_or(old);
                        if gap(next) > gap(old) {
                            grid[row][old] = None;
                            grid[row][next] = Some(id);
                            nodes[id].col = next;
                            moved = true;
                        }
                    }
                    if !moved {
                        break;
                    }
                }
            }
            for row in 0..length {
                for col in 0..7 {
                    let Some(id) = grid[row][col] else { continue };
                    if nodes[id].parents.len() != 1 || nodes[id].children.len() != 1 {
                        continue;
                    }
                    let parent = nodes[id].parents[0];
                    let child = nodes[id].children[0];
                    let next = if nodes[id].col < nodes[parent].col
                        && nodes[id].col < nodes[child].col
                    {
                        (col < 6 && grid[row][col + 1].is_none()).then_some(col + 1)
                    } else if nodes[id].col > nodes[parent].col && nodes[id].col > nodes[child].col
                    {
                        (col > 0 && grid[row][col - 1].is_none()).then_some(col - 1)
                    } else {
                        None
                    };
                    if let Some(next) = next {
                        grid[row][col] = None;
                        grid[row][next] = Some(id);
                        nodes[id].col = next;
                    }
                }
            }
        }

        let mut ids = Vec::new();
        if matches!(
            content.acts[self.act as usize].id,
            "ACT.THE_OVERGROWTH" | "ACT.THE_UNDERDOCKS" | "ACT.THE_HIVE" | "ACT.THE_GLORY"
        ) {
            ids.push(ancient);
        }
        for row in 1..length {
            ids.extend(grid[row].into_iter().flatten());
        }
        ids.push(boss);
        ids.extend(second);
        let mut indices = vec![usize::MAX; nodes.len()];
        for (index, &id) in ids.iter().enumerate() {
            indices[id] = index;
        }
        Map {
            nodes: ids
                .into_iter()
                .map(|id| MapNode {
                    floor: nodes[id].row as u8,
                    lane: nodes[id].col as u8,
                    room: nodes[id].room.unwrap(),
                    next: nodes[id]
                        .children
                        .iter()
                        .filter_map(|&child| {
                            (indices[child] != usize::MAX).then_some(indices[child])
                        })
                        .collect(),
                })
                .collect(),
            current: None,
        }
    }

    fn golden_map(&self) -> Map {
        let rooms = [
            Room::Combat,
            Room::Unknown,
            Room::Combat,
            Room::Rest,
            Room::Combat,
            Room::Rest,
            Room::Unknown,
            Room::Treasure,
            Room::Unknown,
            Room::Treasure,
            Room::Unknown,
            Room::Shop,
            Room::Elite,
            Room::Rest,
            Room::Elite,
            Room::Rest,
        ];
        let mut nodes: Vec<_> = rooms
            .into_iter()
            .enumerate()
            .map(|(index, room)| MapNode {
                floor: index as u8 + 1,
                lane: 3,
                room,
                next: vec![index + 1],
            })
            .collect();
        nodes.push(MapNode {
            floor: 17,
            lane: 3,
            room: Room::Boss,
            next: vec![],
        });
        if self.run.ascension >= 10 && self.run.act == 3 {
            nodes.last_mut().unwrap().next.push(17);
            nodes.push(MapNode {
                floor: 18,
                lane: 3,
                room: Room::Boss,
                next: vec![],
            });
        }
        Map {
            nodes,
            current: None,
        }
    }

    fn run_actions(&self, content: &Content) -> Vec<Action> {
        if self.replacing_potion {
            return self
                .run
                .potions
                .iter()
                .enumerate()
                .filter_map(|(slot, potion)| potion.map(|_| Action::DiscardPotion(slot)))
                .collect();
        }
        let can_remove = self.run.deck.iter().any(|card| {
            let def = content.cards[card.id as usize];
            card.flags(def) & ETERNAL == 0
        });
        let mut actions = match &self.phase {
            Phase::Map => {
                if let Some(current) = self.map.current {
                    let mut paths: Vec<_> = self.map.nodes[current]
                        .next
                        .iter()
                        .copied()
                        .map(Action::Path)
                        .collect();
                    if self.winged_boots < 3 && self.has_relic(content, "RELIC.WINGED_BOOTS") {
                        let floor = self.map.nodes[current].floor + 1;
                        paths.extend(
                            self.map
                                .nodes
                                .iter()
                                .enumerate()
                                .filter(|(index, node)| {
                                    node.floor == floor
                                        && !self.map.nodes[current].next.contains(index)
                                })
                                .map(|(index, _)| Action::Path(index)),
                        );
                    }
                    paths
                } else {
                    let floor = self
                        .map
                        .nodes
                        .iter()
                        .map(|node| node.floor)
                        .min()
                        .unwrap_or(1);
                    self.map
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(|(_, x)| x.floor == floor)
                        .map(|(i, _)| Action::Path(i))
                        .collect()
                }
            }
            Phase::Rewards(rewards) => {
                let mut actions = Vec::new();
                if rewards.gold > 0 {
                    actions.push(Action::RewardGold);
                }
                actions.extend((0..rewards.cards.len()).map(Action::RewardCard));
                if !rewards.cards.is_empty()
                    && self.has_relic(content, "RELIC.DRIFTWOOD")
                    && !self.rerolled_cards
                {
                    actions.push(Action::RerollCards);
                }
                if !rewards.cards.is_empty() && self.has_relic(content, "RELIC.PAELS_WING") {
                    actions.push(Action::SacrificeCards);
                }
                actions.extend((0..rewards.relics.len()).map(Action::RewardRelic));
                actions.extend((0..rewards.potions.len()).map(Action::RewardPotion));
                if rewards.removals > 0 && can_remove {
                    actions.push(Action::RewardRemove);
                }
                if !rewards.cards.is_empty() {
                    actions.push(Action::Cancel);
                }
                actions.push(Action::Leave);
                actions
            }
            Phase::Shop(items) => {
                let mut actions: Vec<_> = items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        self.run.gold >= price(item)
                            && !matches!(item, ShopItem::Potion(_, _) if self.has_relic(content, "RELIC.SOZU"))
                            && !matches!(item, ShopItem::Potion(_, _) if self.run.potions.iter().all(Option::is_some))
                            && !matches!(item, ShopItem::Remove(_) if self.run.deck.len() <= 1 || !can_remove)
                    })
                    .map(|(i, _)| Action::Buy(i))
                    .collect();
                actions.extend(
                    self.run
                        .potions
                        .iter()
                        .enumerate()
                        .filter_map(|(slot, potion)| {
                            potion.filter(|potion| {
                                content.potions[*potion as usize].id == "POTION.FOUL_POTION"
                            })?;
                            Some(Action::Potion { slot, target: None })
                        }),
                );
                actions.push(Action::Leave);
                actions
            }
            Phase::Rest => {
                let mut actions = vec![Action::Cancel];
                if self.rest_used & 1 == 0 {
                    actions.push(Action::Rest);
                }
                if self.rest_used & 4 == 0
                    && self
                        .run
                        .deck
                        .iter()
                        .any(|card| content.cards[card.id as usize].id == "CARD.BYRDONIS_EGG")
                {
                    actions.push(Action::Hatch);
                }
                if self.rest_used & 2 == 0 {
                    actions.extend(
                        self.run
                            .deck
                            .iter()
                            .enumerate()
                            .filter(|(_, card)| {
                                card.upgrades == 0
                                    && !matches!(
                                        content.cards[card.id as usize].card_type,
                                        CardType::Status | CardType::Curse | CardType::Quest
                                    )
                            })
                            .map(|(i, _)| Action::Smith(i)),
                    );
                }
                if self.rest_used & 128 == 0
                    && self
                        .run
                        .deck
                        .iter()
                        .any(|card| card.enchantment == Some(Enchantment::Clone))
                {
                    actions.push(Action::Clone);
                }
                if self.rest_used & 8 == 0
                    && self.girya < 3
                    && self.has_relic(content, "RELIC.GIRYA")
                {
                    actions.push(Action::Lift);
                }
                if self.rest_used & 16 == 0
                    && self.has_relic(content, "RELIC.MEAT_CLEAVER")
                    && self
                        .run
                        .deck
                        .iter()
                        .filter(|card| card.flags(content.cards[card.id as usize]) & ETERNAL == 0)
                        .count()
                        >= 2
                {
                    actions.push(Action::Cook);
                }
                if self.rest_used & 32 == 0 && self.has_relic(content, "RELIC.PUMPKIN_CANDLE") {
                    actions.push(Action::Kindle);
                }
                if self.rest_used & 64 == 0 && self.has_relic(content, "RELIC.SHOVEL") {
                    actions.push(Action::Dig);
                }
                actions
            }
            Phase::Event(_, options) => {
                if let Some(crystal) = &self.crystal
                    && crystal.remaining > 0
                {
                    let mut actions: Vec<_> = crystal
                        .clear
                        .iter()
                        .enumerate()
                        .filter(|(_, clear)| !**clear)
                        .map(|(cell, _)| Action::CrystalCell((cell / 11) as u8, (cell % 11) as u8))
                        .collect();
                    actions.push(Action::CrystalTool(!crystal.big));
                    actions
                } else {
                    let actions: Vec<_> = options
                        .iter()
                        .enumerate()
                        .filter(|(_, option)| {
                            self.requirement(option.requirement)
                                && option.effects.iter().all(|effect| match effect {
                                    RunEffect::EnchantCards(enchantment, _, _, card_type) => {
                                        self.run.deck.iter().any(|card| {
                                            can_enchant(content, card, *enchantment, *card_type)
                                        })
                                    }
                                    _ => true,
                                })
                        })
                        .map(|(i, _)| Action::Event(i))
                        .collect();
                    if actions.is_empty() {
                        vec![Action::Leave]
                    } else {
                        actions
                    }
                }
            }
            Phase::RemoveCards(_, tag, can_skip) => self
                .run
                .deck
                .iter()
                .enumerate()
                .filter(|(_, card)| {
                    let def = content.cards[card.id as usize];
                    card.flags(def) & ETERNAL == 0
                        && (!self.paels_tooth
                            || card.upgrades == 0
                                && !matches!(
                                    def.card_type,
                                    CardType::Status | CardType::Curse | CardType::Quest
                                ))
                        && (*tag == 0 || def.rarity == CardRarity::Basic && def.tags & tag != 0)
                })
                .map(|(i, _)| Action::RemoveCard(i))
                .chain(can_skip.then_some(Action::Cancel))
                .collect(),
            Phase::UpgradeCards(_, can_skip) => self
                .run
                .deck
                .iter()
                .enumerate()
                .filter(|(_, card)| {
                    card.upgrades == 0
                        && !matches!(
                            content.cards[card.id as usize].card_type,
                            CardType::Status | CardType::Curse | CardType::Quest
                        )
                })
                .map(|(i, _)| Action::Smith(i))
                .chain(can_skip.then_some(Action::Cancel))
                .collect(),
            Phase::TransformCards(_, _, can_skip) => self
                .run
                .deck
                .iter()
                .enumerate()
                .filter(|(_, card)| {
                    let def = content.cards[card.id as usize];
                    def.card_type != CardType::Quest && card.flags(def) & ETERNAL == 0
                })
                .map(|(i, _)| Action::RemoveCard(i))
                .chain(can_skip.then_some(Action::Cancel))
                .collect(),
            Phase::EnchantCards(enchantment, _, _, card_type, can_skip) => self
                .run
                .deck
                .iter()
                .enumerate()
                .filter(|(_, card)| can_enchant(content, card, *enchantment, *card_type))
                .map(|(i, _)| Action::Enchant(i))
                .chain(can_skip.then_some(Action::Cancel))
                .collect(),
            Phase::ChooseCards(cards, _, can_skip) => (0..cards.len())
                .map(Action::Choose)
                .chain(can_skip.then_some(Action::Cancel))
                .collect(),
            Phase::ChooseBundles(bundles) => (0..bundles.len()).map(Action::Choose).collect(),
            _ => vec![],
        };
        if !matches!(self.phase, Phase::Won | Phase::Dead) {
            actions.extend(
                self.run
                    .potions
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, potion)| {
                        potion
                            .filter(|id| ANYTIME_POTIONS.contains(id))
                            .map(|_| Action::Potion { slot, target: None })
                    }),
            );
            actions.extend(
                self.run
                    .potions
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, potion)| potion.map(|_| Action::DiscardPotion(slot))),
            );
        }
        actions
    }

    fn requirement(&self, requirement: Requirement) -> bool {
        match requirement {
            Requirement::Always => true,
            Requirement::Gold(gold) => self.run.gold >= gold,
            Requirement::Hp(hp) => self.run.hp > hp,
            Requirement::Deck => !self.run.deck.is_empty(),
        }
    }

    fn next_event(&mut self, content: &Content, pool: &[Id]) -> Option<Id> {
        if self.events.is_empty() {
            self.events = pool.to_vec();
            self.rngs.up_front.shuffle(&mut self.events);
        }
        for unique in [true, false] {
            for _ in 0..self.events.len() {
                let id = self.events.remove(0);
                self.events.push(id);
                if self.event_allowed(content, id)
                    && (!unique || !self.visited_events.contains(&id))
                {
                    self.visited_events.push(id);
                    return Some(id);
                }
            }
        }
        None
    }

    fn event_allowed(&self, content: &Content, id: Id) -> bool {
        let removable = self
            .run
            .deck
            .iter()
            .any(|card| card.flags(content.cards[card.id as usize]) & ETERNAL == 0);
        let transformable = self
            .run
            .deck
            .iter()
            .filter(|card| card.flags(content.cards[card.id as usize]) & ETERNAL == 0)
            .count();
        let potions = self.run.potions.iter().flatten().count();
        let total_floor = self.run.act.saturating_sub(1) as u16 * 17 + self.run.floor as u16;
        match content.events[id as usize].id {
            "EVENT.AMALGAMATOR" => [STRIKE_TAG, DEFEND_TAG].into_iter().all(|tag| {
                self.run
                    .deck
                    .iter()
                    .filter(|card| {
                        let def = content.cards[card.id as usize];
                        def.rarity == CardRarity::Basic
                            && def.tags & tag != 0
                            && card.flags(def) & ETERNAL == 0
                    })
                    .count()
                    >= 2
            }),
            "EVENT.BRAIN_LEECH" | "EVENT.ROOM_FULL_OF_CHEESE" => self.run.act < 3,
            "EVENT.BYRDONIS_NEST" => {
                !self.has_relic(content, "RELIC.BYRDPIP")
                    && !self.has_relic(content, "RELIC.PAELS_LEGION")
                    && !self
                        .run
                        .deck
                        .iter()
                        .any(|card| content.cards[card.id as usize].id == "CARD.BYRDONIS_EGG")
            }
            "EVENT.COLOSSAL_FLOWER" => self.run.hp >= 19,
            "EVENT.CRYSTAL_SPHERE" => self.run.act > 1 && self.run.gold >= 100,
            "EVENT.DOLL_ROOM" => self.run.act == 2,
            "EVENT.ENDLESS_CONVEYOR" => self.run.gold >= 120,
            "EVENT.FAKE_MERCHANT" => {
                self.run.act > 1
                    && (self.run.gold >= 100
                        || self.run.potions.iter().flatten().any(|potion| {
                            content.potions[*potion as usize].id == "POTION.FOUL_POTION"
                        }))
            }
            "EVENT.FIELD_OF_MAN_SIZED_HOLES" => self
                .run
                .deck
                .iter()
                .any(|card| can_enchant(content, card, Enchantment::PerfectFit, None)),
            "EVENT.GRAVE_OF_THE_FORGOTTEN" => self
                .run
                .deck
                .iter()
                .any(|card| can_enchant(content, card, Enchantment::SoulsPower, None)),
            "EVENT.LUMINOUS_CHOIR" => {
                self.run.gold >= 149 && self.relic_deques.iter().any(|relics| !relics.is_empty())
            }
            "EVENT.MORPHIC_GROVE" => self.run.gold >= 100 && transformable >= 2,
            "EVENT.POTION_COURIER" | "EVENT.SYMBIOTE" => self.run.act > 1,
            "EVENT.PUNCH_OFF" => total_floor >= 6,
            "EVENT.RANWID_THE_ELDER" => {
                self.run.act > 1
                    && self.run.gold >= 100
                    && potions > 0
                    && !self.tradable_relics(content).is_empty()
            }
            "EVENT.RELIC_TRADER" => self.run.act > 1 && self.tradable_relics(content).len() >= 5,
            "EVENT.ROUND_TEA_PARTY" => self.run.hp >= 12,
            "EVENT.SLIPPERY_BRIDGE" => total_floor > 6 && removable,
            "EVENT.SPIRALING_WHIRLPOOL" => self
                .run
                .deck
                .iter()
                .any(|card| can_enchant(content, card, Enchantment::Spiral, None)),
            "EVENT.STONE_OF_ALL_TIME" => self.run.act == 2 && potions > 0,
            "EVENT.TEA_MASTER" => self.run.act < 3 && self.run.gold >= 150,
            "EVENT.THE_FUTURE_OF_POTIONS" => potions >= 2,
            "EVENT.THE_LEGENDS_WERE_TRUE" => {
                self.run.act == 1 && !self.run.deck.is_empty() && self.run.hp >= 10
            }
            "EVENT.TRASH_HEAP" => self.run.hp > 5,
            "EVENT.UNREST_SITE" => self.run.hp as i32 * 10 <= self.run.max_hp as i32 * 7,
            "EVENT.WAR_HISTORIAN_REPY" => false,
            "EVENT.WATERLOGGED_SCRIPTORIUM" => self.run.gold >= 55,
            "EVENT.WELCOME_TO_WONGOS" => self.run.act == 2 && self.run.gold >= 100,
            "EVENT.WHISPERING_HOLLOW" => self.run.gold >= 44,
            "EVENT.WOOD_CARVINGS" => self.run.deck.iter().any(|card| {
                let def = content.cards[card.id as usize];
                def.rarity == CardRarity::Basic && card.flags(def) & ETERNAL == 0
            }),
            "EVENT.ZEN_WEAVER" => self.run.gold >= 50,
            _ => true,
        }
    }

    fn run_step(&mut self, content: &Content, action: Action) {
        if matches!(&action, Action::Leave) {
            self.reward_gold_parts.clear();
            if self.fake_shop {
                self.fake_shop = false;
                self.fake_merchant.clear();
            }
        }
        match action {
            Action::Path(node) => self.enter_room(content, node),
            Action::RewardGold => {
                if let Phase::Rewards(rewards) = &mut self.phase {
                    let gold = if self.reward_gold_parts.is_empty() {
                        std::mem::take(&mut rewards.gold)
                    } else {
                        let gold = self.reward_gold_parts.remove(0);
                        rewards.gold -= gold;
                        gold
                    };
                    self.gain_gold(content, gold);
                }
            }
            Action::RewardCard(index) => {
                let card = if let Phase::Rewards(rewards) = &mut self.phase {
                    rewards.cards[index]
                } else {
                    unreachable!()
                };
                self.next_card_reward(content);
                self.rerolled_cards = false;
                self.add_card(content, card);
            }
            Action::RewardRelic(index) => {
                let (relic, exhausted, remaining) = if let Phase::Rewards(rewards) = &mut self.phase
                {
                    let id = rewards.relics.remove(index);
                    (Some(id), rewards.relics.is_empty(), rewards.clone())
                } else {
                    unreachable!()
                };
                if let Some(id) = relic {
                    let wax = self
                        .toy_box_offers
                        .iter()
                        .position(|offer| *offer == id)
                        .map(|index| self.toy_box_offers.remove(index))
                        .is_some();
                    self.phase = Phase::Map;
                    self.obtain_relic(content, id);
                    if wax {
                        self.wax_relics.push(self.run.relics.len() - 1);
                    }
                    if matches!(self.phase, Phase::Map) {
                        self.phase = Phase::Rewards(remaining);
                    } else {
                        self.resume = Some(Phase::Rewards(remaining));
                    }
                }
                if exhausted && self.pending_curse {
                    self.pending_curse = false;
                    let mut curses: Vec<_> = content
                        .cards
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            card.card_type == CardType::Curse
                                && card.id != "CARD.ASCENDERS_BANE"
                                && card.flags[0] & ETERNAL == 0
                                && card.flags[0] & NO_GENERATE == 0
                        })
                        .map(|(id, _)| id as Id)
                        .collect();
                    curses.sort_unstable_by_key(|&id| content.cards[id as usize].id);
                    if !curses.is_empty() {
                        let id = curses[self.rngs.niche.below(curses.len() as u32) as usize];
                        self.add_card(
                            content,
                            Card {
                                id,
                                ..Card::default()
                            },
                        );
                    }
                }
            }
            Action::RewardPotion(index) => {
                if self.has_relic(content, "RELIC.SOZU") {
                    if let Phase::Rewards(rewards) = &mut self.phase {
                        rewards.potions.remove(index);
                    }
                    return;
                }
                if let Phase::Rewards(rewards) = &mut self.phase {
                    if self.run.potions.iter().all(Option::is_some) {
                        self.pending_potion = Some(rewards.potions.remove(index));
                        self.replacing_potion = true;
                        return;
                    }
                    let potion = rewards.potions.remove(index);
                    *self.run.potions.iter_mut().find(|x| x.is_none()).unwrap() = Some(potion);
                }
            }
            Action::RewardRemove => {
                let Phase::Rewards(mut rewards) = std::mem::replace(&mut self.phase, Phase::Map)
                else {
                    unreachable!()
                };
                rewards.removals -= 1;
                self.resume = Some(Phase::Rewards(rewards));
                self.phase = Phase::RemoveCards(1, 0, true);
            }
            Action::RerollCards => {
                let room = match &self.phase {
                    Phase::Rewards(rewards)
                        if !rewards.cards.is_empty()
                            && rewards.cards.iter().all(|card| {
                                content.cards[card.id as usize].rarity == CardRarity::Rare
                            }) =>
                    {
                        Room::Boss
                    }
                    _ => self.room,
                };
                let pool = content.characters[self.run.character as usize].cards;
                let pool = if pool.is_empty() {
                    content.acts[self.act as usize].cards
                } else {
                    pool
                };
                let cards = self.reward_cards(content, pool, 3, room);
                if let Phase::Rewards(rewards) = &mut self.phase {
                    rewards.cards = cards;
                }
                self.rerolled_cards = true;
            }
            Action::SacrificeCards => {
                self.next_card_reward(content);
                self.paels_wing = self.paels_wing.saturating_add(1);
                if self.paels_wing % 2 == 0 {
                    let rarity = self.roll_relic_rarity();
                    if let Some(relic) = self.pull_relic(false, rarity, false, false) {
                        self.obtain_relic(content, relic);
                    }
                }
                self.rerolled_cards = false;
            }
            Action::Buy(index) => self.buy(content, index),
            Action::Rest => {
                let rewards = self.rest_heal(content);
                let destination = self.rest_destination(content, 1);
                if rewards.cards.is_empty() && rewards.potions.is_empty() {
                    self.phase = destination;
                } else {
                    self.resume = Some(destination);
                    self.phase = Phase::Rewards(rewards);
                }
            }
            Action::Hatch => {
                if let Some(id) = content.relic_id("RELIC.BYRDPIP") {
                    self.obtain_relic(content, id);
                }
                self.phase = self.rest_destination(content, 4);
            }
            Action::Smith(index) if matches!(self.phase, Phase::Rest) => {
                self.run.deck[index].upgrades = 1;
                self.phase = self.rest_destination(content, 2);
            }
            Action::Lift => {
                self.girya += 1;
                self.phase = self.rest_destination(content, 8);
            }
            Action::Cook => {
                self.cooking = true;
                self.phase = Phase::RemoveCards(2, 0, true);
            }
            Action::Kindle => {
                self.pumpkin_candle = self.pumpkin_candle.saturating_add(5);
                self.phase = self.rest_destination(content, 32);
            }
            Action::Dig => {
                let destination = self.rest_destination(content, 64);
                self.resume = Some(destination);
                let rarity = self.roll_relic_rarity();
                if let Some(relic) = self.pull_relic(false, rarity, false, false) {
                    self.obtain_relic(content, relic);
                }
                if matches!(self.phase, Phase::Rest) {
                    self.phase = self.resume.take().unwrap_or(Phase::Map);
                }
            }
            Action::Smith(index) => {
                self.run.deck[index].upgrades = 1;
                if let Phase::UpgradeCards(remaining, _) = &mut self.phase
                    && *remaining > 1
                {
                    *remaining -= 1;
                } else {
                    self.finish_run_choice(content);
                }
            }
            Action::Event(index) => {
                if let Phase::Event(id, _) = &self.phase
                    && matches!(
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
                    && self.event_data[index] > 0
                {
                    self.obtain_relic(content, self.event_data[index] as Id - 1);
                    if matches!(self.phase, Phase::Event(..)) {
                        self.phase = Phase::Map;
                    }
                    return;
                }
                if let Phase::Event(id, _) = &self.phase {
                    let id = *id;
                    match content.events[id as usize].id {
                        "EVENT.WAR_HISTORIAN_REPY" => {
                            let second = self.event_data[3] != 0;
                            if second {
                                self.run.deck.retain(|card| {
                                    content.cards[card.id as usize].id != "CARD.LANTERN_KEY"
                                });
                            } else if let Some(key) = self.run.deck.iter().position(|card| {
                                content.cards[card.id as usize].id == "CARD.LANTERN_KEY"
                            }) {
                                self.run.deck.remove(key);
                            }
                            let cage = if second {
                                self.event_data[3] == 2
                            } else {
                                index == 0
                            };
                            let another = self.run.deck.iter().any(|card| {
                                content.cards[card.id as usize].id == "CARD.LANTERN_KEY"
                            });
                            if cage {
                                if let Some(relic) = content.relic_id("RELIC.HISTORY_COURSE") {
                                    self.obtain_relic(content, relic);
                                }
                                self.phase = if !second && another {
                                    self.event_data[3] = 1;
                                    event_page(id, &[1])
                                } else {
                                    Phase::Map
                                };
                            } else {
                                let pool = self.potion_pool(content);
                                let mut potions = self.reward_potions(&pool, 1);
                                potions.extend(self.reward_potions(&pool, 1));
                                let mut relics = Vec::new();
                                for _ in 0..2 {
                                    let rarity = self.roll_relic_rarity();
                                    relics.extend(self.pull_relic(false, rarity, false, false));
                                }
                                self.resume = (!second && another).then(|| {
                                    self.event_data[3] = 2;
                                    event_page(id, &[0])
                                });
                                self.phase = Phase::Rewards(Rewards {
                                    gold: 0,
                                    cards: vec![],
                                    card_rewards: vec![],
                                    relics,
                                    potions,
                                    removals: 0,
                                });
                            }
                            return;
                        }
                        "EVENT.BATTLEWORN_DUMMY" => {
                            self.start_event_combat(
                                content,
                                [
                                    "ENCOUNTER.BATTLEWORN_DUMMY_1",
                                    "ENCOUNTER.BATTLEWORN_DUMMY_2",
                                    "ENCOUNTER.BATTLEWORN_DUMMY_3",
                                ][index],
                                index as u8 + 1,
                            );
                            return;
                        }
                        "EVENT.PUNCH_OFF" if self.event_data[3] == 0 && index == 0 => {
                            if let Some(card) = content.card_id("CARD.INJURY") {
                                self.add_card(
                                    content,
                                    Card {
                                        id: card,
                                        ..Card::default()
                                    },
                                );
                            }
                            let rarity = self.roll_relic_rarity();
                            let relics = self
                                .pull_relic(false, rarity, false, false)
                                .into_iter()
                                .collect();
                            self.phase = Phase::Rewards(Rewards {
                                gold: 0,
                                cards: vec![],
                                card_rewards: vec![],
                                relics,
                                potions: vec![],
                                removals: 0,
                            });
                            return;
                        }
                        "EVENT.PUNCH_OFF" if self.event_data[3] == 0 => {
                            self.event_data[3] = 1;
                            self.phase = event_page(id, &[0]);
                            return;
                        }
                        "EVENT.PUNCH_OFF" => {
                            self.start_event_combat(
                                content,
                                "ENCOUNTER.PUNCH_OFF_EVENT_ENCOUNTER",
                                4,
                            );
                            return;
                        }
                        "EVENT.THE_LANTERN_KEY" if self.event_data[3] == 0 && index == 1 => {
                            self.event_data[3] = 1;
                            self.phase = event_page(id, &[0]);
                            return;
                        }
                        "EVENT.THE_LANTERN_KEY" if self.event_data[3] != 0 => {
                            self.start_event_combat(
                                content,
                                "ENCOUNTER.MYSTERIOUS_KNIGHT_EVENT_ENCOUNTER",
                                5,
                            );
                            return;
                        }
                        "EVENT.DENSE_VEGETATION" if self.event_data[3] == 0 && index == 0 => {
                            self.run.hp = (self.run.hp - 8).max(0);
                            self.gain_gold(content, self.event_data[0] as i32);
                            self.phase = Phase::Map;
                            return;
                        }
                        "EVENT.DENSE_VEGETATION" if self.event_data[3] == 0 => {
                            let rewards = self.rest_heal(content);
                            self.event_data[3] = 1;
                            let next = event_page(id, &[0]);
                            if rewards.cards.is_empty() && rewards.potions.is_empty() {
                                self.phase = next;
                            } else {
                                self.resume = Some(next);
                                self.phase = Phase::Rewards(rewards);
                            }
                            return;
                        }
                        "EVENT.DENSE_VEGETATION" => {
                            self.start_event_combat(
                                content,
                                "ENCOUNTER.DENSE_VEGETATION_EVENT_ENCOUNTER",
                                6,
                            );
                            return;
                        }
                        _ => {}
                    }
                }
                if let Phase::Event(id, _) = &self.phase
                    && content.events[*id as usize].id == "EVENT.TINKER_TIME"
                {
                    let id = *id;
                    if self.event_data[3] == 0 {
                        let mut types = [1, 2, 3];
                        self.event_rng.as_mut().unwrap().shuffle(&mut types);
                        self.event_data[..2].copy_from_slice(&types[..2]);
                        self.event_data[3] = -1;
                        self.phase = event_page(id, &[0, 1]);
                    } else if self.event_data[3] < 0 {
                        let card_type = self.event_data[index];
                        let mut riders = match card_type {
                            1 => [1, 2, 3],
                            2 => [4, 5, 6],
                            _ => [7, 8, 9],
                        };
                        self.event_rng.as_mut().unwrap().shuffle(&mut riders);
                        self.event_data[..2].copy_from_slice(&riders[..2]);
                        self.event_data[3] = card_type;
                        self.phase = event_page(id, &[0, 1]);
                    } else {
                        self.add_card(
                            content,
                            Card {
                                id: content.card_id("CARD.MAD_SCIENCE").unwrap(),
                                variant: self.event_data[3] as u8 * 10
                                    + self.event_data[index] as u8,
                                ..Card::default()
                            },
                        );
                        self.phase = Phase::Map;
                    }
                    return;
                }
                let options = match &self.phase {
                    Phase::Event(_, options) => options,
                    _ => unreachable!(),
                };
                self.run_queue
                    .extend(options[index].effects.iter().rev().copied());
                self.resume = Some(Phase::Map);
                self.resolve_run(content);
            }
            Action::CrystalCell(x, y) => self.clear_crystal(content, x, y),
            Action::CrystalTool(big) => self.crystal.as_mut().unwrap().big = big,
            Action::EventRelic(_, id) => {
                self.obtain_relic(content, id);
                if matches!(self.phase, Phase::Event(..)) {
                    self.phase = Phase::Map;
                }
            }
            Action::EventCard(_, card) => {
                self.add_card(content, card);
                self.phase = Phase::Map;
            }
            Action::RemoveCard(index) => {
                if let Phase::TransformCards(target, remaining, can_skip) = self.phase {
                    self.transform_selected(content, index, target);
                    if remaining > 1 {
                        self.phase = Phase::TransformCards(target, remaining - 1, can_skip);
                    } else {
                        self.finish_run_choice(content);
                    }
                    return;
                }
                if matches!(self.resume, Some(Phase::Shop(_))) {
                    self.run.gold -= self.removal_price;
                    self.removal_price = 0;
                    self.run.card_shop_removals += 1;
                    if self.has_relic(content, "RELIC.MAW_BANK") {
                        self.maw_bank = true;
                    }
                }
                let card = self.run.deck.remove(index);
                if self.paels_tooth {
                    self.paels_cards.push(card);
                }
                if self.parasol_removal {
                    self.parasol_removal = false;
                    self.run.card_shop_removals += 1;
                }
                if let Phase::RemoveCards(remaining, _, _) = &mut self.phase
                    && *remaining > 1
                {
                    *remaining -= 1;
                } else {
                    if self.paels_tooth {
                        self.paels_cards
                            .sort_by_key(|card| content.cards[card.id as usize].id);
                        self.paels_tooth = false;
                    }
                    self.finish_run_choice(content);
                }
            }
            Action::Clone => {
                let cards: Vec<_> = self
                    .run
                    .deck
                    .iter()
                    .filter(|card| card.enchantment == Some(Enchantment::Clone))
                    .copied()
                    .collect();
                for mut card in cards {
                    card.instance = self.next_card;
                    self.next_card += 1;
                    self.run.deck.push(card);
                }
                self.phase = self.rest_destination(content, 128);
            }
            Action::Enchant(index) => {
                let Phase::EnchantCards(enchantment, amount, remaining, card_type, can_skip) =
                    self.phase
                else {
                    unreachable!()
                };
                let card = &mut self.run.deck[index];
                card.enchantment = Some(enchantment);
                card.enchantment_amount = amount;
                if remaining > 1 {
                    self.phase = Phase::EnchantCards(
                        enchantment,
                        amount,
                        remaining - 1,
                        card_type,
                        can_skip,
                    );
                } else {
                    self.finish_run_choice(content);
                }
            }
            Action::Cancel if self.cooking && matches!(self.phase, Phase::RemoveCards(..)) => {
                self.cooking = false;
                self.phase = Phase::Rest;
            }
            Action::Cancel
                if matches!(
                    self.phase,
                    Phase::RemoveCards(..)
                        | Phase::UpgradeCards(..)
                        | Phase::TransformCards(..)
                        | Phase::EnchantCards(..)
                        | Phase::ChooseCards(..)
                ) =>
            {
                self.finish_run_choice(content)
            }
            Action::Cancel if matches!(self.phase, Phase::Rest) => self.phase = Phase::Map,
            Action::Cancel => {
                if let Phase::Rewards(rewards) = &mut self.phase
                    && !rewards.cards.is_empty()
                {
                    self.next_card_reward(content);
                    self.rerolled_cards = false;
                }
            }
            Action::Leave if matches!(self.phase, Phase::Rewards(_)) && self.conveyor => {
                self.continue_conveyor();
            }
            Action::Leave if matches!(self.phase, Phase::Rewards(_)) && self.resume.is_some() => {
                self.phase = self.resume.take().unwrap();
            }
            Action::Leave
                if matches!(self.phase, Phase::Rewards(_)) && !self.parasol.is_empty() =>
            {
                self.continue_parasol(content);
            }
            Action::Leave if (1..=3).contains(&self.event_combat) => {
                let setting = std::mem::take(&mut self.event_combat);
                self.resume_battle_dummy(content, setting);
            }
            Action::Leave => self.leave_room(content),
            _ => unreachable!(),
        }
    }

    fn enter_room(&mut self, content: &Content, node: usize) {
        self.rerolled_cards = false;
        let previous = self.room;
        let used_boots = self.map.current.is_some_and(|current| {
            !self.map.nodes[current].next.contains(&node)
                && self.map.nodes[node].floor == self.map.nodes[current].floor + 1
        });
        if self.map.nodes[node].floor == 0 {
            let missing = self.run.max_hp - self.run.hp;
            self.run.hp += if self.run.ascension >= 2 {
                missing * 4 / 5
            } else {
                missing
            };
        }
        self.map.current = Some(node);
        if used_boots {
            self.winged_boots += 1;
        }
        self.run.floor = self.map.nodes[node].floor + 1;
        self.room = self.map.nodes[node].room;
        let unknown = self.room == Room::Unknown;
        if !self.maw_bank && self.has_relic(content, "RELIC.MAW_BANK") {
            self.gain_gold(content, 12);
        }
        if unknown && self.has_relic(content, "RELIC.PLANISPHERE") {
            self.run.hp = (self.run.hp + 5).min(self.run.max_hp);
        }
        if self.room == Room::Treasure
            && self.spoils == Some((self.map.nodes[node].lane, self.map.nodes[node].floor))
        {
            let before = self.run.deck.len();
            self.run
                .deck
                .retain(|card| content.cards[card.id as usize].id != "CARD.SPOILS_MAP");
            self.gain_gold(content, 600 * (before - self.run.deck.len()) as i32);
            self.spoils = None;
        }
        if self.room == Room::Rest {
            self.rest_used = 0;
            if self.has_relic(content, "RELIC.ETERNAL_FEATHER") {
                self.run.hp =
                    (self.run.hp + (self.run.deck.len() / 5 * 3) as i16).min(self.run.max_hp);
            }
            self.tea_set = 2 * self.has_relic(content, "RELIC.VENERABLE_TEA_SET") as u8
                + self.has_relic(content, "RELIC.FAKE_VENERABLE_TEA_SET") as u8;
        }
        if unknown {
            let shop_allowed = previous != Room::Shop
                && !(!self.map.nodes[node].next.is_empty()
                    && self.map.nodes[node]
                        .next
                        .iter()
                        .all(|&next| self.map.nodes[next].room == Room::Shop));
            let roll = self.rngs.unknown_map_point.single();
            let rooms = [Room::Combat, Room::Elite, Room::Treasure, Room::Shop];
            let event_only = self.golden_compass == Some(self.run.act)
                || self.run.act == 3
                    && self
                        .run
                        .deck
                        .iter()
                        .any(|card| content.cards[card.id as usize].id == "CARD.LANTERN_KEY");
            let combat_allowed = !self.has_relic(content, "RELIC.JUZU_BRACELET");
            let mut total = 0;
            self.room = Room::Event;
            for (index, room) in rooms.into_iter().enumerate() {
                if !event_only
                    && (room != Room::Combat || combat_allowed)
                    && (room != Room::Shop || shop_allowed)
                    && self.unknown_odds[index] >= 0
                {
                    total += self.unknown_odds[index];
                    if roll <= total as f32 / 10_000.0 {
                        self.room = room;
                        break;
                    }
                }
            }
            for (index, room) in rooms.into_iter().enumerate() {
                if self.room == room {
                    self.unknown_odds[index] = [1000, -10_000, 200, 300][index];
                } else if !event_only
                    && (room != Room::Combat || combat_allowed)
                    && self.unknown_odds[index] >= 0
                    && (room != Room::Shop || shop_allowed)
                {
                    self.unknown_odds[index] += [1000, -10_000, 200, 300][index];
                }
            }
        }
        if self.room == Room::Shop && self.has_relic(content, "RELIC.MEAL_TICKET") {
            self.run.hp = (self.run.hp + 15).min(self.run.max_hp);
        }
        if self.room == Room::Boss && self.has_relic(content, "RELIC.PANTOGRAPH") {
            self.run.hp = (self.run.hp + 25).min(self.run.max_hp);
        }
        let act = &content.acts[self.act as usize];
        match self.room {
            Room::Combat => {
                let id = if !self.replaying {
                    self.next_encounter(content, false)
                } else if self.encounters.is_empty() {
                    self.pick(act.encounters)
                } else {
                    Some(self.encounters.remove(0))
                };
                if let Some(id) = id {
                    self.start_combat(content, id).unwrap();
                } else {
                    self.phase = Phase::Map;
                }
            }
            Room::Elite => {
                let id = if !self.replaying {
                    self.next_encounter(content, true)
                } else if self.elites.is_empty() {
                    self.pick(act.elites)
                } else {
                    Some(self.elites.remove(0))
                };
                if let Some(id) = id {
                    self.start_combat(content, id).unwrap();
                } else {
                    self.phase = Phase::Map;
                }
            }
            Room::Boss => {
                let id = self.bosses[self.bosses_visited.min(1) as usize];
                if let Some(id) = id {
                    self.bosses_visited += 1;
                    self.start_combat(content, id).unwrap();
                } else {
                    self.phase = Phase::Won;
                }
            }
            Room::Event => {
                self.event_data = [0; 4];
                self.event_cards.clear();
                let lantern = self.run.act == 3
                    && self
                        .run
                        .deck
                        .iter()
                        .any(|card| content.cards[card.id as usize].id == "CARD.LANTERN_KEY");
                let event = if self.map.nodes[node].floor == 0 {
                    self.next_ancient(content)
                } else if lantern {
                    content
                        .events
                        .iter()
                        .position(|event| event.id == "EVENT.WAR_HISTORIAN_REPY")
                        .map(|id| id as Id)
                } else {
                    self.next_event(content, act.events)
                };
                self.event_rng = event.map(|id| {
                    Rng::from_seed(
                        self.seed.wrapping_add(hash(
                            content.events[id as usize]
                                .id
                                .strip_prefix("EVENT.")
                                .unwrap_or(content.events[id as usize].id),
                        )) as u64,
                    )
                });
                if event.is_some_and(|id| {
                    matches!(
                        content.events[id as usize].id,
                        "EVENT.PUNCH_OFF" | "EVENT.DENSE_VEGETATION"
                    )
                }) {
                    let (min, max) = if event
                        .is_some_and(|id| content.events[id as usize].id == "EVENT.PUNCH_OFF")
                    {
                        (91, 99)
                    } else {
                        (61, 100)
                    };
                    self.event_data[0] =
                        self.event_rng.as_mut().unwrap().below((max - min) as u32) as i64 + min;
                }
                if let Some(id) = event {
                    if self.map.nodes[node].floor == 0 {
                        self.visited_events.push(id);
                    }
                    match content.events[id as usize].id {
                        "EVENT.COLORFUL_PHILOSOPHERS" => {
                            let order = ["NECROBINDER", "IRONCLAD", "REGENT", "SILENT", "DEFECT"];
                            let mut characters: Vec<_> = order
                                .into_iter()
                                .filter_map(|name| {
                                    content.characters.iter().position(|character| {
                                        character.id == format!("CHARACTER.{name}")
                                    })
                                })
                                .filter(|character| *character != self.run.character as usize)
                                .collect();
                            while characters.len() > 3 {
                                let index = self
                                    .event_rng
                                    .as_mut()
                                    .unwrap()
                                    .below(characters.len() as u32);
                                characters.remove(index as usize);
                            }
                            for (slot, character) in characters.into_iter().enumerate() {
                                self.event_data[slot] = character as i64 + 1;
                            }
                        }
                        "EVENT.CRYSTAL_SPHERE" => {
                            self.event_data[0] =
                                51 + self.event_rng.as_mut().unwrap().below(49) as i64;
                        }
                        "EVENT.FAKE_MERCHANT" => {
                            self.fake_shop = true;
                            self.fake_merchant = [
                                "RELIC.FAKE_ANCHOR",
                                "RELIC.FAKE_BLOOD_VIAL",
                                "RELIC.FAKE_HAPPY_FLOWER",
                                "RELIC.FAKE_LEES_WAFFLE",
                                "RELIC.FAKE_MANGO",
                                "RELIC.FAKE_ORICHALCUM",
                                "RELIC.FAKE_SNECKO_EYE",
                                "RELIC.FAKE_STRIKE_DUMMY",
                                "RELIC.FAKE_VENERABLE_TEA_SET",
                            ]
                            .into_iter()
                            .filter_map(|relic| content.relic_id(relic))
                            .collect();
                            self.event_rng
                                .as_mut()
                                .unwrap()
                                .shuffle(&mut self.fake_merchant);
                            self.fake_merchant.truncate(6);
                        }
                        "EVENT.RANWID_THE_ELDER" => {
                            let potions: Vec<_> = self
                                .run
                                .potions
                                .iter()
                                .enumerate()
                                .filter_map(|(slot, potion)| potion.map(|_| slot))
                                .collect();
                            if !potions.is_empty() {
                                let choice =
                                    self.event_rng.as_mut().unwrap().below(potions.len() as u32);
                                self.event_data[0] = potions[choice as usize] as i64 + 1;
                            }
                            let relics = self.tradable_relics(content);
                            if !relics.is_empty() {
                                let choice =
                                    self.event_rng.as_mut().unwrap().below(relics.len() as u32);
                                self.event_cards.push(relics[choice as usize] as u32);
                            }
                        }
                        "EVENT.RELIC_TRADER" => {
                            let mut relics = self.tradable_relics(content);
                            relics.sort_by_key(|index| {
                                content.relics[self.run.relics[*index] as usize].id
                            });
                            self.event_rng.as_mut().unwrap().shuffle(&mut relics);
                            relics.truncate(3);
                            self.event_cards =
                                relics.into_iter().map(|index| index as u32).collect();
                            for _ in 0..3 {
                                let rarity = self.roll_relic_rarity();
                                if let Some(relic) = self.pull_relic(false, rarity, false, false) {
                                    self.relic_queue.push(relic);
                                }
                            }
                        }
                        "EVENT.JUNGLE_MAZE_ADVENTURE" => {
                            self.event_data[0] = (150.0
                                + self.event_rng.as_mut().unwrap().float(-15.0, 15.0))
                                as i64;
                            self.event_data[1] =
                                (50.0 + self.event_rng.as_mut().unwrap().float(-15.0, 15.0)) as i64;
                        }
                        "EVENT.LUMINOUS_CHOIR" => {
                            self.event_data[0] =
                                149 - self.event_rng.as_mut().unwrap().below(50) as i64;
                        }
                        "EVENT.WHISPERING_HOLLOW" => {
                            self.event_data[0] =
                                35 + self.event_rng.as_mut().unwrap().below(19) as i64 - 9;
                        }
                        "EVENT.ENDLESS_CONVEYOR" => self.roll_conveyor(),
                        _ => {}
                    }
                }
                self.event_relic = None;
                if event.is_some_and(|id| content.events[id as usize].id == "EVENT.SLIPPERY_BRIDGE")
                {
                    self.pick_slippery_card(content, None);
                }
                if let Some(id) = event {
                    self.setup_ancient(content, id);
                }
                self.phase = event.map_or(Phase::Map, |id| match content.events[id as usize].id {
                    "EVENT.THE_FUTURE_OF_POTIONS" => {
                        let count = self.run.potions.iter().flatten().count().min(3);
                        event_page(id, &[0, 1, 2][..count])
                    }
                    "EVENT.COLORFUL_PHILOSOPHERS" => {
                        let count = self
                            .event_data
                            .iter()
                            .take_while(|value| **value > 0)
                            .count();
                        event_page(id, &[0, 1, 2][..count])
                    }
                    "EVENT.RANWID_THE_ELDER" => event_page(id, &[0, 1, 2]),
                    "EVENT.RELIC_TRADER" => event_page(id, &[0, 1, 2][..self.event_cards.len()]),
                    "EVENT.ENDLESS_CONVEYOR" => self.conveyor_phase(id),
                    "EVENT.SELF_HELP_BOOK"
                        if ![
                            (Enchantment::Sharp, CardType::Attack),
                            (Enchantment::Nimble, CardType::Skill),
                            (Enchantment::Swift, CardType::Power),
                        ]
                        .into_iter()
                        .any(|(enchantment, card_type)| {
                            self.run.deck.iter().any(|card| {
                                can_enchant(content, card, enchantment, Some(card_type))
                            })
                        }) =>
                    {
                        event_page(id, &[9])
                    }
                    "EVENT.FAKE_MERCHANT" => Phase::Shop(
                        self.fake_merchant
                            .iter()
                            .map(|relic| {
                                ShopItem::Relic(
                                    *relic,
                                    (50.0 * self.rngs.shops.float(0.85, 1.15)).round() as i32,
                                )
                            })
                            .collect(),
                    ),
                    _ => Phase::Event(id, content.events[id as usize].options.to_vec()),
                });
            }
            Room::Unknown => unreachable!(),
            Room::Shop => {
                let items = self.shop(content);
                if self.has_relic(content, "RELIC.LORDS_PARASOL") {
                    self.parasol = items;
                    self.continue_parasol(content);
                } else {
                    self.phase = Phase::Shop(items);
                }
            }
            Room::Rest => self.phase = Phase::Rest,
            Room::Treasure => {
                if self.has_relic(content, "RELIC.SILVER_CRUCIBLE") {
                    self.silver_treasures = self.silver_treasures.saturating_add(1);
                    if self.silver_treasures == 1 {
                        self.phase = Phase::Map;
                        return;
                    }
                }
                let gold = self.rngs.rewards.range(42, 52) as i32;
                self.gain_gold(
                    content,
                    if self.run.ascension >= 3 {
                        gold * 3 / 4
                    } else {
                        gold
                    },
                );
                let roll = self.rngs.treasure_room_relics.single();
                let relics = self
                    .pull_relic(true, relic_rarity(roll), false, false)
                    .into_iter()
                    .collect();
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics,
                    potions: vec![],
                    removals: 0,
                })
            }
        }
    }

    fn start_event_combat(&mut self, content: &Content, encounter: &str, event: u8) {
        let Some(encounter) = content.encounters.iter().position(|x| x.id == encounter) else {
            self.phase = Phase::Map;
            return;
        };
        self.event_combat = event;
        self.room = Room::Combat;
        self.start_combat(content, encounter as Id).unwrap();
    }

    fn ancient_relic(&mut self, pool: &[Id]) -> Id {
        pool[self.event_rng.as_mut().unwrap().below(pool.len() as u32) as usize]
    }

    fn setup_ancient(&mut self, content: &Content, event: Id) {
        let relics = |ids: &[&str]| {
            ids.iter()
                .filter_map(|id| content.relic_id(&format!("RELIC.{id}")))
                .collect::<Vec<_>>()
        };
        let name = content.events[event as usize].id;
        let choices = match name {
            "EVENT.NEOW" => {
                let curses = relics(&[
                    "CURSED_PEARL",
                    "HEFTY_TABLET",
                    "LARGE_CAPSULE",
                    "LEAFY_POULTICE",
                    "NEOWS_BONES",
                    "PRECARIOUS_SHEARS",
                    "SILKEN_TRESS",
                    "SILVER_CRUCIBLE",
                ]);
                let curse = self.ancient_relic(&curses);
                let mut positive = relics(&[
                    "ARCANE_SCROLL",
                    "BOOMING_CONCH",
                    "FISHING_ROD",
                    "GOLDEN_PEARL",
                    "KALEIDOSCOPE",
                    "LEAD_PAPERWEIGHT",
                    "LOST_COFFER",
                    "NEOWS_TORMENT",
                    "NEW_LEAF",
                    "PHIAL_HOLSTER",
                    "PRECISE_SCISSORS",
                    "SCROLL_BOXES",
                    "WINGED_BOOTS",
                ]);
                for (bad, good) in [
                    ("CURSED_PEARL", "GOLDEN_PEARL"),
                    ("HEFTY_TABLET", "ARCANE_SCROLL"),
                    ("LEAFY_POULTICE", "NEW_LEAF"),
                    ("PRECARIOUS_SHEARS", "PRECISE_SCISSORS"),
                ] {
                    if content.relics[curse as usize].id == format!("RELIC.{bad}") {
                        positive.retain(|&id| {
                            content.relics[id as usize].id != format!("RELIC.{good}")
                        });
                    }
                }
                if content.relics[curse as usize].id != "RELIC.LARGE_CAPSULE" {
                    positive.extend(relics(&[
                        if self.event_rng.as_mut().unwrap().below(2) == 0 {
                            "LAVA_ROCK"
                        } else {
                            "SMALL_CAPSULE"
                        },
                    ]));
                }
                positive.extend(relics(&[
                    if self.event_rng.as_mut().unwrap().below(2) == 0 {
                        "NUTRITIOUS_OYSTER"
                    } else {
                        "STONE_HUMIDIFIER"
                    },
                ]));
                positive.extend(relics(&[
                    if self.event_rng.as_mut().unwrap().below(2) == 0 {
                        "NEOWS_TALISMAN"
                    } else {
                        "POMANDER"
                    },
                ]));
                self.event_rng.as_mut().unwrap().shuffle(&mut positive);
                vec![positive[0], positive[1], curse]
            }
            "EVENT.DARV" => {
                let mut source = Vec::new();
                for group in [
                    relics(&["ASTROLABE"]),
                    relics(&["BLACK_STAR"]),
                    relics(&["CALLING_BELL"]),
                    relics(&["EMPTY_CAGE"]),
                    relics(&["PANDORAS_BOX"]),
                    relics(&["RUNIC_PYRAMID"]),
                    relics(&["SNECKO_EYE"]),
                    if self.run.act == 2 {
                        relics(&["ECTOPLASM", "SOZU"])
                    } else if self.run.act == 3 {
                        relics(&["PHILOSOPHERS_STONE", "VELVET_CHOKER"])
                    } else {
                        vec![]
                    },
                ] {
                    if !group.is_empty() {
                        source.push(self.ancient_relic(&group));
                    }
                }
                self.event_rng.as_mut().unwrap().shuffle(&mut source);
                if self.event_rng.as_mut().unwrap().below(2) == 0 {
                    source.truncate(2);
                    if let Some(dusty) = content.relic_id("RELIC.DUSTY_TOME") {
                        source.push(dusty);
                    }
                } else {
                    source.truncate(3);
                }
                source
            }
            "EVENT.NONUPEIPE" => {
                let mut pool = relics(&[
                    "BLESSED_ANTLER",
                    "BRILLIANT_SCARF",
                    "DELICATE_FROND",
                    "DIAMOND_DIADEM",
                    "FUR_COAT",
                    "GLITTER",
                    "JEWELRY_BOX",
                    "LOOMING_FRUIT",
                    "SIGNET_RING",
                ]);
                if self
                    .run
                    .deck
                    .iter()
                    .filter(|card| can_enchant(content, card, Enchantment::Swift, None))
                    .count()
                    >= 4
                {
                    pool.extend(relics(&["BEAUTIFUL_BRACELET"]));
                }
                self.event_rng.as_mut().unwrap().shuffle(&mut pool);
                pool.truncate(3);
                pool
            }
            "EVENT.OROBAS" => {
                let special = if self.event_rng.as_mut().unwrap().single() < 1.0 / 3.0 {
                    "PRISMATIC_GEM"
                } else {
                    "SEA_GLASS"
                };
                let mut first = relics(&["ELECTRIC_SHRYMP", "GLASS_EYE", "SAND_CASTLE"]);
                first.extend(relics(&[special]));
                let second = relics(&["ALCHEMICAL_COFFER", "DRIFTWOOD", "RADIANT_PEARL"]);
                let mut third = Vec::new();
                if self.run.relics.iter().any(|&id| {
                    matches!(
                        content.relics[id as usize].id,
                        "RELIC.BURNING_BLOOD"
                            | "RELIC.RING_OF_THE_SNAKE"
                            | "RELIC.DIVINE_RIGHT"
                            | "RELIC.BOUND_PHYLACTERY"
                            | "RELIC.CRACKED_CORE"
                    )
                }) {
                    third.extend(relics(&["TOUCH_OF_OROBAS"]));
                }
                if self.run.deck.iter().any(|card| {
                    matches!(
                        content.cards[card.id as usize].id,
                        "CARD.BASH"
                            | "CARD.NEUTRALIZE"
                            | "CARD.UNLEASH"
                            | "CARD.FALLING_STAR"
                            | "CARD.DUALCAST"
                    )
                }) {
                    third.extend(relics(&["ARCHAIC_TOOTH"]));
                }
                if third.is_empty() {
                    third.extend(relics(&["CIRCLET"]));
                }
                vec![
                    self.ancient_relic(&first),
                    self.ancient_relic(&second),
                    self.ancient_relic(&third),
                ]
            }
            "EVENT.TANX" => {
                let mut pool = relics(&[
                    "CLAWS",
                    "CROSSBOW",
                    "IRON_CLUB",
                    "MEAT_CLEAVER",
                    "SAI",
                    "SPIKED_GAUNTLETS",
                    "TANXS_WHISTLE",
                    "THROWING_AXE",
                    "WAR_HAMMER",
                ]);
                if self
                    .run
                    .deck
                    .iter()
                    .filter(|card| can_enchant(content, card, Enchantment::Instinct, None))
                    .count()
                    >= 3
                {
                    pool.extend(relics(&["TRI_BOOMERANG"]));
                }
                self.event_rng.as_mut().unwrap().shuffle(&mut pool);
                pool.truncate(3);
                pool
            }
            "EVENT.TEZCATARA" => {
                let mut first = relics(&["VERY_HOT_COCOA", "YUMMY_COOKIE"]);
                if self.run.deck.iter().any(|card| {
                    let def = content.cards[card.id as usize];
                    def.rarity == CardRarity::Basic && def.tags & STRIKE_TAG != 0
                }) {
                    first.extend(relics(&["NUTRITIOUS_SOUP"]));
                }
                let second = relics(&["BIIIG_HUG", "STORYBOOK", "TOASTY_MITTENS"]);
                let third = relics(&[
                    "GOLDEN_COMPASS",
                    "PUMPKIN_CANDLE",
                    "TOY_BOX",
                    "SEAL_OF_GOLD",
                ]);
                vec![
                    self.ancient_relic(&first),
                    self.ancient_relic(&second),
                    self.ancient_relic(&third),
                ]
            }
            "EVENT.PAEL" => {
                let first = relics(&["PAELS_FLESH", "PAELS_HORN", "PAELS_TEARS"]);
                let mut second = relics(&["PAELS_WING"]);
                if self
                    .run
                    .deck
                    .iter()
                    .filter(|card| can_enchant(content, card, Enchantment::Goopy, None))
                    .count()
                    >= 3
                {
                    second.extend(relics(&["PAELS_CLAW"]));
                }
                if self
                    .run
                    .deck
                    .iter()
                    .filter(|card| card.flags(content.cards[card.id as usize]) & ETERNAL == 0)
                    .count()
                    >= 5
                {
                    second.extend(relics(&["PAELS_TOOTH"]));
                }
                let copy = second.clone();
                second.extend(copy);
                second.extend(relics(&["PAELS_GROWTH"]));
                let mut third = relics(&["PAELS_EYE", "PAELS_BLOOD"]);
                let has_pet = self.run.relics.iter().enumerate().any(|(index, &id)| {
                    !self.melted_relics.contains(&index)
                        && matches!(
                            content.relics[id as usize].id,
                            "RELIC.BYRDPIP" | "RELIC.PAELS_LEGION" | "RELIC.TANXS_WHISTLE"
                        )
                });
                if !has_pet {
                    third.extend(relics(&["PAELS_LEGION"]));
                }
                vec![
                    self.ancient_relic(&first),
                    self.ancient_relic(&second),
                    self.ancient_relic(&third),
                ]
            }
            "EVENT.VAKUU" => {
                let mut first = relics(&["BLOOD_SOAKED_ROSE", "WHISPERING_EARRING", "FIDDLE"]);
                let mut second = relics(&["PRESERVED_FOG", "SERE_TALON", "DISTINGUISHED_CAPE"]);
                let mut third = relics(&[
                    "CHOICES_PARADOX",
                    "MUSIC_BOX",
                    "LORDS_PARASOL",
                    "JEWELED_MASK",
                ]);
                self.event_rng.as_mut().unwrap().shuffle(&mut first);
                self.event_rng.as_mut().unwrap().shuffle(&mut second);
                self.event_rng.as_mut().unwrap().shuffle(&mut third);
                vec![first[0], second[0], third[0]]
            }
            _ => return,
        };
        for (slot, relic) in self.event_data.iter_mut().zip(choices) {
            *slot = relic as i64 + 1;
        }
    }

    fn rewards(&mut self, content: &Content) -> Rewards {
        if self.room == Room::Boss && self.run.act == 3 {
            let mut rewards = Rewards {
                gold: 0,
                cards: vec![],
                card_rewards: vec![],
                relics: vec![],
                potions: vec![],
                removals: 0,
            };
            self.modify_combat_rewards(content, &mut rewards);
            return rewards;
        }
        let act = &content.acts[self.act as usize];
        let cards = content.characters[self.run.character as usize].cards;
        let potion_pool = self.potion_pool(content);
        let potion_roll = self.rngs.rewards.single();
        let potion = self.has_relic(content, "RELIC.WHITE_BEAST_STATUE")
            || potion_roll
                < self.potion_odds as f32 / 100.0
                    + if self.room == Room::Elite { 0.125 } else { 0.0 };
        self.potion_odds += if potion { -10 } else { 10 };
        let (mut min, mut max) = match self.room {
            Room::Elite => (35, 45),
            Room::Boss => (100, 100),
            _ => (10, 20),
        };
        if self.run.ascension >= 3 {
            min = min * 3 / 4;
            max = max * 3 / 4;
        }
        let gold = min + self.rngs.rewards.below((max - min + 1) as u32) as i32;
        let potions = if potion {
            self.reward_potions(&potion_pool, 1)
        } else {
            vec![]
        };
        let cards = self.reward_cards(
            content,
            if cards.is_empty() { act.cards } else { cards },
            3,
            self.room,
        );
        let relics = if self.room == Room::Elite {
            let rarity = self.roll_relic_rarity();
            self.pull_relic(false, rarity, false, false)
                .into_iter()
                .collect()
        } else {
            vec![]
        };
        let mut rewards = Rewards {
            gold: gold + self.reward_gold_parts.iter().sum::<i32>(),
            cards,
            card_rewards: vec![],
            relics,
            potions,
            removals: 0,
        };
        self.modify_combat_rewards(content, &mut rewards);
        rewards
    }

    fn resume_battle_dummy(&mut self, content: &Content, setting: u8) {
        match setting {
            1 => {
                let pool = self.potion_pool(content);
                let potions = (!pool.is_empty())
                    .then(|| pool[self.rngs.rewards.below(pool.len() as u32) as usize])
                    .into_iter()
                    .collect();
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics: vec![],
                    potions,
                    removals: 0,
                });
            }
            2 => {
                let mut cards: Vec<_> = self
                    .run
                    .deck
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        card.upgrades == 0
                            && !matches!(
                                card.card_type(content.cards[card.id as usize]),
                                CardType::Status | CardType::Curse | CardType::Quest
                            )
                    })
                    .map(|(index, _)| index)
                    .collect();
                cards.sort_by_key(|&index| {
                    (
                        content.cards[self.run.deck[index].id as usize].id,
                        self.run.deck[index].instance,
                    )
                });
                self.event_rng.as_mut().unwrap().shuffle(&mut cards);
                for index in cards.into_iter().take(2) {
                    self.run.deck[index].upgrades = 1;
                }
                self.phase = Phase::Map;
            }
            3 => {
                let rarity = self.roll_relic_rarity();
                if let Some(relic) = self.pull_relic(false, rarity, false, false) {
                    self.obtain_relic(content, relic);
                }
                self.phase = Phase::Map;
            }
            _ => self.phase = Phase::Map,
        }
    }

    fn finish_event_combat(&mut self, content: &Content, royalties: i32, removals: u8) -> Phase {
        let event = self.event_combat;
        if event & 128 != 0 {
            self.event_combat = 0;
            return Phase::Map;
        }
        let mut rewards = Rewards {
            gold: royalties,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals,
        };
        match event {
            4 => {
                let rarity = self.roll_relic_rarity();
                rewards
                    .relics
                    .extend(self.pull_relic(false, rarity, false, false));
                let pool = self.potion_pool(content);
                rewards.potions = self.reward_potions(&pool, 1);
            }
            5 => {
                if let Some(id) = content.card_id("CARD.LANTERN_KEY") {
                    rewards.cards.push(Card {
                        id,
                        ..Card::default()
                    });
                }
            }
            7 => {
                rewards.relics.extend(
                    content
                        .relic_id("RELIC.FAKE_MERCHANTS_RUG")
                        .into_iter()
                        .chain(std::mem::take(&mut self.fake_merchant)),
                );
                self.fake_shop = false;
            }
            _ => {}
        }
        self.modify_combat_rewards(content, &mut rewards);
        if (1..=3).contains(&event)
            && rewards.gold == 0
            && rewards.cards.is_empty()
            && rewards.card_rewards.is_empty()
            && rewards.relics.is_empty()
            && rewards.potions.is_empty()
            && rewards.removals == 0
        {
            self.event_combat = 0;
            self.resume_battle_dummy(content, event);
            return std::mem::replace(&mut self.phase, Phase::Map);
        }
        if event > 3 {
            self.event_combat = 0;
        }
        if rewards.gold != 0
            || !rewards.cards.is_empty()
            || !rewards.card_rewards.is_empty()
            || !rewards.relics.is_empty()
            || !rewards.potions.is_empty()
            || rewards.removals != 0
        {
            Phase::Rewards(rewards)
        } else {
            Phase::Map
        }
    }

    fn card_rewards(
        &mut self,
        content: &Content,
        pool: &[Id],
        count: usize,
        room: Room,
    ) -> Vec<Card> {
        let (rare, uncommon, offset) = match room {
            Room::Elite => (if self.run.ascension >= 7 { 500 } else { 1000 }, 4000, true),
            Room::Boss => (10_000, 0, false),
            _ => (if self.run.ascension >= 7 { 149 } else { 300 }, 3700, true),
        };
        let mut chosen = Vec::new();
        for _ in 0..count {
            let roll = self.rngs.rewards.single();
            let rare_chance = rare + if offset { self.rarity_offset } else { 0 };
            let rarity = if roll < rare_chance as f32 / 10_000.0 {
                self.rarity_offset = -500;
                CardRarity::Rare
            } else {
                self.rarity_offset =
                    (self.rarity_offset + if self.run.ascension >= 7 { 50 } else { 100 }).min(4000);
                if roll < rare_chance as f32 / 10_000.0 + uncommon as f32 / 10_000.0 {
                    CardRarity::Uncommon
                } else {
                    CardRarity::Common
                }
            };
            let order = match rarity {
                CardRarity::Common => [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare],
                CardRarity::Uncommon => {
                    [CardRarity::Uncommon, CardRarity::Rare, CardRarity::Common]
                }
                _ => [CardRarity::Rare, CardRarity::Common, CardRarity::Uncommon],
            };
            let options: Vec<_> = order
                .into_iter()
                .find_map(|rarity| {
                    let cards: Vec<_> = pool
                        .iter()
                        .copied()
                        .filter(|id| {
                            content.cards[*id as usize].rarity == rarity
                                && !chosen.iter().any(|card: &Card| card.id == *id)
                        })
                        .collect();
                    (!cards.is_empty()).then_some(cards)
                })
                .unwrap_or_default();
            if options.is_empty() {
                break;
            }
            let id = options[self.rngs.rewards.below(options.len() as u32) as usize];
            let upgrade = self.rngs.rewards.single()
                <= (self.run.act - 1) as f32 * if self.run.ascension >= 7 { 0.125 } else { 0.25 };
            chosen.push(Card {
                id,
                upgrades: (upgrade && rarity != CardRarity::Rare) as u8,
                ..Card::default()
            });
        }
        chosen
    }

    fn next_card_reward(&mut self, content: &Content) {
        let reward = match &mut self.phase {
            Phase::Rewards(rewards) if !rewards.card_rewards.is_empty() => {
                Some(rewards.card_rewards.remove(0))
            }
            Phase::Rewards(_) => None,
            _ => return,
        };
        let cards = reward.map_or_else(Vec::new, |reward| self.make_card_reward(content, reward));
        if let Phase::Rewards(rewards) = &mut self.phase {
            rewards.cards = cards;
        }
    }

    fn make_card_reward(&mut self, content: &Content, reward: CardReward) -> Vec<Card> {
        match reward {
            CardReward::Standard(room) => {
                let pool = content.characters[self.run.character as usize].cards;
                let pool = if pool.is_empty() {
                    content.acts[self.act as usize].cards
                } else {
                    pool
                };
                self.reward_cards(content, pool, 3, room)
            }
            CardReward::Fixed(character, rarity) => {
                let mut cards = self.fixed_cards(
                    content,
                    content.characters[character as usize].cards,
                    rarity,
                    3,
                );
                self.modify_reward_cards(content, &mut cards);
                cards
            }
            CardReward::Kaleidoscope => {
                let mut characters: Vec<_> = content
                    .characters
                    .iter()
                    .enumerate()
                    .filter(|(id, character)| {
                        *id != self.run.character as usize && !character.cards.is_empty()
                    })
                    .map(|(id, _)| id as Id)
                    .collect();
                characters.sort_by_key(|id| content.characters[*id as usize].id);
                self.rngs.niche.shuffle(&mut characters);
                let mut cards = Vec::new();
                for character in characters.into_iter().take(3) {
                    let mut card = self.card_rewards(
                        content,
                        content.characters[character as usize].cards,
                        1,
                        Room::Combat,
                    );
                    self.modify_reward_cards(content, &mut card);
                    cards.extend(card);
                }
                cards
            }
            CardReward::Crystal(rarity) => {
                let mut cards = self.crystal_cards(content, rarity);
                self.modify_reward_cards(content, &mut cards);
                cards
            }
        }
    }

    fn reward_cards(
        &mut self,
        content: &Content,
        base: &[Id],
        count: usize,
        room: Room,
    ) -> Vec<Card> {
        let mut pool = base.to_vec();
        if self.has_relic(content, "RELIC.PRISMATIC_GEM") {
            pool = content
                .characters
                .iter()
                .flat_map(|character| character.cards)
                .copied()
                .collect();
        } else if self.has_relic(content, "RELIC.DINGY_RUG") {
            pool.extend_from_slice(content.colorless());
        }
        let mut unique = Vec::new();
        pool.retain(|id| {
            if unique.contains(id) {
                false
            } else {
                unique.push(*id);
                true
            }
        });
        let mut cards = self.card_rewards(content, &pool, count, room);
        if self.lasting_candy > 0 && self.lasting_candy % 2 == 0 {
            let mut powers: Vec<_> = pool
                .iter()
                .copied()
                .filter(|id| {
                    content.cards[*id as usize].card_type == CardType::Power
                        && !cards.iter().any(|card| card.id == *id)
                })
                .collect();
            if powers.is_empty() {
                powers = pool
                    .iter()
                    .copied()
                    .filter(|id| content.cards[*id as usize].card_type == CardType::Power)
                    .collect();
            }
            if !powers.is_empty() {
                cards.extend(self.card_rewards(content, &powers, 1, room));
            }
        }
        self.modify_reward_cards(content, &mut cards);
        cards
    }

    fn modify_reward_cards(&mut self, content: &Content, cards: &mut Vec<Card>) {
        for (index, relic) in self.run.relics.clone().into_iter().enumerate() {
            if self.melted_relics.contains(&index) {
                continue;
            }
            match content.relics[relic as usize].id {
                "RELIC.MOLTEN_EGG" => upgrade_type(content, cards, CardType::Attack),
                "RELIC.TOXIC_EGG" => upgrade_type(content, cards, CardType::Skill),
                "RELIC.FROZEN_EGG" => upgrade_type(content, cards, CardType::Power),
                "RELIC.SILVER_CRUCIBLE" if self.silver_crucible < 3 => {
                    upgrade_all(content, cards);
                    self.silver_crucible += 1;
                }
                "RELIC.LAVA_LAMP" if !self.damage_taken && self.room == Room::Combat => {
                    upgrade_all(content, cards)
                }
                "RELIC.FRESNEL_LENS" => enchant_all(content, cards, Enchantment::Nimble, 2),
                "RELIC.GLITTER" => enchant_all(content, cards, Enchantment::Glam, 1),
                "RELIC.SILKEN_TRESS" if !self.silken_tress => {
                    enchant_all(content, cards, Enchantment::Glam, 1);
                    self.silken_tress = true;
                }
                "RELIC.WING_CHARM" => {
                    let valid: Vec<_> = cards
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| can_enchant(content, card, Enchantment::Swift, None))
                        .map(|(index, _)| index)
                        .collect();
                    if !valid.is_empty() {
                        let index = valid[self.rngs.niche.below(valid.len() as u32) as usize];
                        cards[index].enchantment = Some(Enchantment::Swift);
                        cards[index].enchantment_amount = 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn fixed_cards(
        &mut self,
        content: &Content,
        pool: &[Id],
        rarity: CardRarity,
        count: usize,
    ) -> Vec<Card> {
        let mut options: Vec<_> = pool
            .iter()
            .copied()
            .filter(|id| content.cards[*id as usize].rarity == rarity)
            .collect();
        let mut cards = Vec::new();
        for _ in 0..count.min(options.len()) {
            let id = options.remove(self.rngs.rewards.below(options.len() as u32) as usize);
            let upgrade = self.rngs.rewards.single()
                <= (self.run.act - 1) as f32 * if self.run.ascension >= 7 { 0.125 } else { 0.25 };
            cards.push(Card {
                id,
                upgrades: (upgrade && rarity != CardRarity::Rare) as u8,
                ..Card::default()
            });
        }
        cards
    }

    fn modify_combat_rewards(&mut self, content: &Content, rewards: &mut Rewards) {
        for (index, relic) in self.run.relics.clone().into_iter().enumerate() {
            if self.melted_relics.contains(&index) {
                continue;
            }
            match content.relics[relic as usize].id {
                "RELIC.AMETHYST_AUBERGINE" if !(self.room == Room::Boss && self.run.act == 3) => {
                    rewards.gold += 15
                }
                "RELIC.PRAYER_WHEEL" if self.room == Room::Combat => {
                    rewards
                        .card_rewards
                        .push(CardReward::Standard(Room::Combat));
                }
                "RELIC.WHITE_STAR" if self.room == Room::Elite => {
                    rewards.card_rewards.push(CardReward::Standard(Room::Boss));
                }
                "RELIC.BLACK_STAR" if self.room == Room::Elite => {
                    let rarity = self.roll_relic_rarity();
                    rewards
                        .relics
                        .extend(self.pull_relic(false, rarity, false, false));
                }
                "RELIC.LAVA_ROCK" if self.room == Room::Boss && self.run.act == 1 => {
                    for _ in 0..2 {
                        let rarity = self.roll_relic_rarity();
                        rewards
                            .relics
                            .extend(self.pull_relic(false, rarity, false, false));
                    }
                }
                "RELIC.WONGOS_MYSTERY_TICKET"
                    if self.wongo_combats.is_some_and(|combats| combats >= 5) =>
                {
                    self.wongo_combats = None;
                    for _ in 0..3 {
                        let rarity = self.roll_relic_rarity();
                        rewards
                            .relics
                            .extend(self.pull_relic(false, rarity, false, false));
                    }
                }
                _ => {}
            }
        }
    }

    fn shop(&mut self, content: &Content) -> Vec<ShopItem> {
        let act = &content.acts[self.act as usize];
        let cards = content.characters[self.run.character as usize].cards;
        let potion_pool = self.potion_pool(content);
        let pool = if cards.is_empty() { act.cards } else { cards };
        let sale = self.rngs.shops.below(5) as usize;
        let mut chosen = Vec::new();
        let mut items = Vec::new();
        for (index, card_type) in [
            CardType::Attack,
            CardType::Attack,
            CardType::Skill,
            CardType::Skill,
            CardType::Power,
        ]
        .into_iter()
        .enumerate()
        {
            let rarity = self.shop_rarity();
            let order = match rarity {
                CardRarity::Common => [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare],
                CardRarity::Uncommon => {
                    [CardRarity::Uncommon, CardRarity::Rare, CardRarity::Common]
                }
                _ => [CardRarity::Rare, CardRarity::Common, CardRarity::Uncommon],
            };
            let id = order.into_iter().find_map(|rarity| {
                let options: Vec<_> = pool
                    .iter()
                    .copied()
                    .filter(|id| {
                        let card = content.cards[*id as usize];
                        card.rarity == rarity && card.card_type == card_type && !chosen.contains(id)
                    })
                    .collect();
                (!options.is_empty())
                    .then(|| options[self.rngs.shops.below(options.len() as u32) as usize])
            });
            let Some(id) = id else { continue };
            chosen.push(id);
            let upgrade = self.rngs.rewards.single()
                <= (self.run.act - 1) as f32 * if self.run.ascension >= 7 { 0.125 } else { 0.25 };
            let base = match content.cards[id as usize].rarity {
                CardRarity::Rare => 150,
                CardRarity::Uncommon => 75,
                _ => 50,
            };
            let mut cost = (base as f32 * self.rngs.shops.float(0.95, 1.05)).round() as i32;
            if index == sale {
                cost = (base as f32 * self.rngs.shops.float(0.95, 1.05)).round() as i32 / 2;
            }
            items.push(ShopItem::Card(
                self.merchant_card(
                    content,
                    Card {
                        id,
                        upgrades: (upgrade && content.cards[id as usize].rarity != CardRarity::Rare)
                            as u8,
                        ..Card::default()
                    },
                ),
                cost,
            ));
        }
        for rarity in [CardRarity::Uncommon, CardRarity::Rare] {
            let options: Vec<_> = content
                .colorless
                .iter()
                .copied()
                .filter(|id| content.cards[*id as usize].rarity == rarity && !chosen.contains(id))
                .collect();
            if options.is_empty() {
                continue;
            }
            let id = options[self.rngs.shops.below(options.len() as u32) as usize];
            chosen.push(id);
            let upgrade = self.rngs.rewards.single()
                <= (self.run.act - 1) as f32 * if self.run.ascension >= 7 { 0.125 } else { 0.25 };
            let base = if rarity == CardRarity::Rare { 150 } else { 75 };
            let base = (base as f32 * 1.15).round();
            let cost = (base * self.rngs.shops.float(0.95, 1.05)).round() as i32;
            items.push(ShopItem::Card(
                self.merchant_card(
                    content,
                    Card {
                        id,
                        upgrades: (upgrade && rarity != CardRarity::Rare) as u8,
                        ..Card::default()
                    },
                ),
                cost,
            ));
        }
        let rarities = [self.roll_relic_rarity(), self.roll_relic_rarity(), 3];
        for rarity in rarities {
            if let Some(id) = self.pull_relic(false, rarity, true, true) {
                let base = [175, 225, 275, 200][relic_group(id).unwrap()];
                items.push(ShopItem::Relic(
                    id,
                    (base as f32 * self.rngs.shops.float(0.85, 1.15)).round() as i32,
                ));
            }
        }
        let potions = self.shop_potions(&potion_pool, 3);
        items.extend(potions.into_iter().map(|id| {
            let base = if RARE_POTIONS.contains(&id) {
                100
            } else if UNCOMMON_POTIONS.contains(&id) {
                75
            } else {
                50
            };
            ShopItem::Potion(
                id,
                (base as f32 * self.rngs.shops.float(0.95, 1.05)).round() as i32,
            )
        }));
        let (base, increase) = if self.run.ascension >= 6 {
            (100, 50)
        } else {
            (75, 25)
        };
        items.push(ShopItem::Remove(
            base + increase * self.run.card_shop_removals as i32,
        ));
        let numerator = if self.has_relic(content, "RELIC.THE_COURIER") {
            4
        } else {
            5
        } * if self.has_relic(content, "RELIC.MEMBERSHIP_CARD") {
            1
        } else {
            2
        };
        if numerator != 10 {
            for item in &mut items {
                *price_mut(item) = price(item) * numerator / 10;
            }
        }
        items
    }

    fn merchant_card(&self, content: &Content, mut card: Card) -> Card {
        let card_type = card.card_type(content.cards[card.id as usize]);
        if card.upgrades == 0
            && self.run.relics.iter().enumerate().any(|(index, &id)| {
                !self.melted_relics.contains(&index)
                    && content.relics[id as usize].id
                        == match card_type {
                            CardType::Attack => "RELIC.MOLTEN_EGG",
                            CardType::Skill => "RELIC.TOXIC_EGG",
                            CardType::Power => "RELIC.FROZEN_EGG",
                            _ => "",
                        }
            })
        {
            card.upgrades = 1;
        }
        if self.has_relic(content, "RELIC.FRESNEL_LENS")
            && can_enchant(content, &card, Enchantment::Nimble, None)
        {
            card.enchantment = Some(Enchantment::Nimble);
            card.enchantment_amount = 2;
        }
        card
    }

    fn has_relic(&self, content: &Content, wanted: &str) -> bool {
        self.run.relics.iter().enumerate().any(|(index, &id)| {
            !self.melted_relics.contains(&index) && content.relics[id as usize].id == wanted
        })
    }

    fn tradable_relics(&self, content: &Content) -> Vec<usize> {
        self.run
            .relics
            .iter()
            .enumerate()
            .filter(|(index, id)| {
                !self.melted_relics.contains(index)
                    && relic_group(**id).is_some()
                    && !matches!(
                        content.relics[**id as usize].id,
                        "RELIC.ALCHEMICAL_COFFER"
                            | "RELIC.ASTROLABE"
                            | "RELIC.BIG_MUSHROOM"
                            | "RELIC.CALLING_BELL"
                            | "RELIC.CAULDRON"
                            | "RELIC.DOLLYS_MIRROR"
                            | "RELIC.ELECTRIC_SHRYMP"
                            | "RELIC.EMPTY_CAGE"
                            | "RELIC.GNARLED_HAMMER"
                            | "RELIC.JEWELRY_BOX"
                            | "RELIC.LEES_WAFFLE"
                            | "RELIC.MANGO"
                            | "RELIC.OLD_COIN"
                            | "RELIC.ORRERY"
                            | "RELIC.PANDORAS_BOX"
                            | "RELIC.PEAR"
                            | "RELIC.POTION_BELT"
                            | "RELIC.ROYAL_STAMP"
                            | "RELIC.SAND_CASTLE"
                            | "RELIC.STRAWBERRY"
                            | "RELIC.WAR_PAINT"
                            | "RELIC.WHETSTONE"
                    )
                    && !(content.relics[**id as usize].id == "RELIC.MAW_BANK" && self.maw_bank)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn remove_relic_at(&mut self, content: &Content, index: usize) {
        let active = !self.melted_relics.contains(&index);
        let id = self.run.relics.remove(index);
        if active
            && matches!(
                content.relics[id as usize].id,
                "RELIC.BLESSED_ANTLER"
                    | "RELIC.ECTOPLASM"
                    | "RELIC.PHILOSOPHERS_STONE"
                    | "RELIC.PRISMATIC_GEM"
                    | "RELIC.SOZU"
                    | "RELIC.SPIKED_GAUNTLETS"
                    | "RELIC.VELVET_CHOKER"
                    | "RELIC.WHISPERING_EARRING"
            )
        {
            self.run.energy = self.run.energy.saturating_sub(1);
        }
        for indices in [&mut self.wax_relics, &mut self.melted_relics] {
            indices.retain(|tracked| *tracked != index);
            for tracked in indices {
                *tracked -= (*tracked > index) as usize;
            }
        }
    }

    fn restock(
        &mut self,
        content: &Content,
        items: &[ShopItem],
        item: &ShopItem,
    ) -> Option<ShopItem> {
        let cards = content.characters[self.run.character as usize].cards;
        let pool = if cards.is_empty() {
            content.acts[self.act as usize].cards
        } else {
            cards
        };
        let stocked: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                ShopItem::Card(card, _) => Some(card.id),
                _ => None,
            })
            .collect();
        let replacement = match item {
            ShopItem::Card(card, _) => {
                let colorless = content.colorless.contains(&card.id);
                let rarity = if colorless {
                    content.cards[card.id as usize].rarity
                } else {
                    self.shop_rarity()
                };
                let card_type = content.cards[card.id as usize].card_type;
                let order = match rarity {
                    CardRarity::Common => {
                        [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare]
                    }
                    CardRarity::Uncommon => {
                        [CardRarity::Uncommon, CardRarity::Rare, CardRarity::Common]
                    }
                    _ => [CardRarity::Rare, CardRarity::Common, CardRarity::Uncommon],
                };
                let pool = if colorless { content.colorless() } else { pool };
                let options = order.into_iter().find_map(|rarity| {
                    let options: Vec<_> = pool
                        .iter()
                        .copied()
                        .filter(|id| {
                            let candidate = content.cards[*id as usize];
                            candidate.rarity == rarity
                                && (colorless || candidate.card_type == card_type)
                                && !stocked.contains(id)
                        })
                        .collect();
                    (!options.is_empty()).then_some(options)
                })?;
                let id = options[self.rngs.shops.below(options.len() as u32) as usize];
                let upgrade = self.rngs.rewards.single()
                    <= (self.run.act - 1) as f32
                        * if self.run.ascension >= 7 { 0.125 } else { 0.25 };
                let rarity = content.cards[id as usize].rarity;
                let base = match rarity {
                    CardRarity::Rare => 150,
                    CardRarity::Uncommon => 75,
                    _ => 50,
                };
                let base = if colorless {
                    (base as f32 * 1.15).round() as i32
                } else {
                    base
                };
                ShopItem::Card(
                    self.merchant_card(
                        content,
                        Card {
                            id,
                            upgrades: (upgrade && rarity != CardRarity::Rare) as u8,
                            ..Card::default()
                        },
                    ),
                    (base as f32 * self.rngs.shops.float(0.95, 1.05)).round() as i32,
                )
            }
            ShopItem::Relic(_, _) => {
                let rarity = self.roll_relic_rarity();
                let id = self.pull_relic(false, rarity, true, true)?;
                let base = [175, 225, 275, 200][relic_group(id).unwrap()];
                ShopItem::Relic(
                    id,
                    (base as f32 * self.rngs.shops.float(0.85, 1.15)).round() as i32,
                )
            }
            ShopItem::Potion(_, _) => {
                let id = self.shop_potions(&self.potion_pool(content), 1).pop()?;
                let base = if RARE_POTIONS.contains(&id) {
                    100
                } else if UNCOMMON_POTIONS.contains(&id) {
                    75
                } else {
                    50
                };
                ShopItem::Potion(
                    id,
                    (base as f32 * self.rngs.shops.float(0.95, 1.05)).round() as i32,
                )
            }
            ShopItem::Remove(_) => return None,
        };
        let numerator = if self.has_relic(content, "RELIC.THE_COURIER") {
            4
        } else {
            5
        } * if self.has_relic(content, "RELIC.MEMBERSHIP_CARD") {
            1
        } else {
            2
        };
        Some(with_price(
            replacement.clone(),
            price(&replacement) * numerator / 10,
        ))
    }

    fn shop_rarity(&mut self) -> CardRarity {
        let rare = if self.run.ascension >= 7 { 450 } else { 900 };
        let roll = self.rngs.rewards.single();
        if roll < (rare + self.rarity_offset) as f32 / 10_000.0 {
            CardRarity::Rare
        } else if roll < (rare + self.rarity_offset + 3700) as f32 / 10_000.0 {
            CardRarity::Uncommon
        } else {
            CardRarity::Common
        }
    }

    fn reward_potions(&mut self, pool: &[Id], count: usize) -> Vec<Id> {
        let mut chosen = Vec::new();
        for _ in 0..count {
            let roll = self.rngs.rewards.single();
            let options: Vec<_> = pool
                .iter()
                .copied()
                .filter(|id| {
                    !chosen.contains(id)
                        && if roll <= 0.1 {
                            RARE_POTIONS.contains(id)
                        } else if roll <= 0.35 {
                            UNCOMMON_POTIONS.contains(id)
                        } else {
                            !RARE_POTIONS.contains(id) && !UNCOMMON_POTIONS.contains(id)
                        }
                })
                .collect();
            if options.is_empty() {
                break;
            }
            chosen.push(options[self.rngs.rewards.below(options.len() as u32) as usize]);
        }
        chosen
    }

    fn shop_potions(&mut self, pool: &[Id], count: usize) -> Vec<Id> {
        let mut chosen = Vec::new();
        for _ in 0..count {
            let roll = self.rngs.shops.single();
            let options: Vec<_> = pool
                .iter()
                .copied()
                .filter(|id| {
                    !chosen.contains(id)
                        && if roll <= 0.1 {
                            RARE_POTIONS.contains(id)
                        } else if roll <= 0.35 {
                            UNCOMMON_POTIONS.contains(id)
                        } else {
                            !RARE_POTIONS.contains(id) && !UNCOMMON_POTIONS.contains(id)
                        }
                })
                .collect();
            if options.is_empty() {
                break;
            }
            chosen.push(options[self.rngs.shops.below(options.len() as u32) as usize]);
        }
        chosen
    }

    fn buy(&mut self, content: &Content, index: usize) {
        let Phase::Shop(mut items) = std::mem::replace(&mut self.phase, Phase::Map) else {
            unreachable!()
        };
        let item = items.remove(index);
        let purchased = item.clone();
        let item_price = price(&item);
        let courier = self.has_relic(content, "RELIC.THE_COURIER");
        let membership = self.has_relic(content, "RELIC.MEMBERSHIP_CARD");
        match item {
            ShopItem::Card(card, _) => {
                self.run.gold -= item_price;
                self.add_card(content, card)
            }
            ShopItem::Relic(id, _) => {
                self.run.gold -= item_price;
                self.obtain_relic(content, id)
            }
            ShopItem::Potion(id, _) => {
                self.run.gold -= item_price;
                if let Some(slot) = self.run.potions.iter_mut().find(|x| x.is_none()) {
                    *slot = Some(id)
                }
            }
            ShopItem::Remove(_) => {
                self.removal_price = item_price;
                self.resume = Some(Phase::Shop(items));
                self.phase = Phase::RemoveCards(1, 0, true);
                return;
            }
        }
        if self.fake_shop {
            if let ShopItem::Relic(id, _) = purchased {
                self.fake_merchant.retain(|relic| *relic != id);
            }
            self.phase = Phase::Shop(items);
            return;
        }
        let pickup = !matches!(self.phase, Phase::Map);
        if self.has_relic(content, "RELIC.MAW_BANK") && item_price > 0 {
            self.maw_bank = true;
        }
        let numerator = if !courier && self.has_relic(content, "RELIC.THE_COURIER") {
            4
        } else {
            5
        } * if !membership && self.has_relic(content, "RELIC.MEMBERSHIP_CARD") {
            1
        } else {
            2
        };
        if numerator != 10 {
            for item in &mut items {
                *price_mut(item) = price(item) * numerator / 10;
            }
        }
        if self.has_relic(content, "RELIC.THE_COURIER")
            && let Some(replacement) = self.restock(content, &items, &purchased)
        {
            items.insert(index, replacement);
        }
        if pickup {
            self.resume = Some(Phase::Shop(items));
        } else {
            self.phase = Phase::Shop(items);
        }
    }

    fn obtain_relic(&mut self, content: &Content, id: Id) {
        self.remove_relic_from_bags(id);
        self.run.relics.push(id);
        match content.relics[id as usize].id {
            "RELIC.STRAWBERRY" => {
                self.run.max_hp += 7;
                self.run.hp += 7;
            }
            "RELIC.PEAR" => {
                self.run.max_hp += 10;
                self.run.hp += 10;
            }
            "RELIC.MANGO" => {
                self.run.max_hp += 14;
                self.run.hp += 14;
            }
            "RELIC.LEES_WAFFLE" => {
                self.run.max_hp += 7;
                self.run.hp = self.run.max_hp;
            }
            "RELIC.FAKE_LEES_WAFFLE" => {
                self.run.hp = (self.run.hp + self.run.max_hp / 10).min(self.run.max_hp)
            }
            "RELIC.FAKE_MANGO" => {
                self.run.max_hp += 3;
                self.run.hp += 3;
            }
            "RELIC.BLESSED_ANTLER"
            | "RELIC.ECTOPLASM"
            | "RELIC.PHILOSOPHERS_STONE"
            | "RELIC.PRISMATIC_GEM"
            | "RELIC.SOZU"
            | "RELIC.SPIKED_GAUNTLETS"
            | "RELIC.VELVET_CHOKER"
            | "RELIC.WHISPERING_EARRING" => self.run.energy += 1,
            "RELIC.BIG_MUSHROOM" => {
                self.run.max_hp += 20;
                self.run.hp += 20;
            }
            "RELIC.OLD_COIN" => self.gain_gold(content, 300),
            "RELIC.CALLING_BELL" => {
                if let Some(curse) = content.card_id("CARD.CURSE_OF_THE_BELL") {
                    self.add_card(
                        content,
                        Card {
                            id: curse,
                            ..Card::default()
                        },
                    );
                }
                let relics = (0..3)
                    .filter_map(|rarity| self.pull_relic(false, rarity, false, false))
                    .collect();
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics,
                    potions: vec![],
                    removals: 0,
                });
            }
            "RELIC.CAULDRON" => {
                let pool = self.potion_pool(content);
                let mut potions = Vec::new();
                for _ in 0..5 {
                    potions.extend(self.reward_potions(&pool, 1));
                }
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics: vec![],
                    potions,
                    removals: 0,
                });
            }
            "RELIC.DOLLYS_MIRROR" => {
                let cards: Vec<_> = self
                    .run
                    .deck
                    .iter()
                    .copied()
                    .filter(|card| content.cards[card.id as usize].card_type != CardType::Quest)
                    .collect();
                self.phase = Phase::ChooseCards(cards, 1, false);
            }
            "RELIC.GNARLED_HAMMER" => {
                self.phase = Phase::EnchantCards(Enchantment::Sharp, 3, 3, None, false)
            }
            "RELIC.HEFTY_TABLET" => {
                let mut pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|id| content.cards[*id as usize].rarity == CardRarity::Rare)
                    .collect();
                let mut cards = Vec::new();
                for _ in 0..3.min(pool.len()) {
                    let index = self.rngs.rewards.below(pool.len() as u32) as usize;
                    cards.push(Card {
                        id: pool.remove(index),
                        ..Card::default()
                    });
                }
                if let Some(injury) = content.card_id("CARD.INJURY") {
                    self.add_card(
                        content,
                        Card {
                            id: injury,
                            ..Card::default()
                        },
                    );
                }
                self.phase = Phase::ChooseCards(cards, 1, true);
            }
            "RELIC.KIFUDA" => {
                self.phase = Phase::EnchantCards(Enchantment::Adroit, 3, 3, None, false)
            }
            "RELIC.PUNCH_DAGGER" => {
                self.phase = Phase::EnchantCards(Enchantment::Momentum, 5, 1, None, false)
            }
            "RELIC.ROYAL_STAMP" => {
                let mut cards: Vec<_> = self
                    .run
                    .deck
                    .iter()
                    .filter(|card| can_enchant(content, card, Enchantment::RoyallyApproved, None))
                    .map(|card| card.instance)
                    .collect();
                self.rngs.niche.shuffle(&mut cards);
                self.phase = Phase::EnchantCards(Enchantment::RoyallyApproved, 1, 1, None, false);
            }
            "RELIC.FRAGRANT_MUSHROOM" => {
                self.run.hp = (self.run.hp - 15).max(0);
                self.upgrade_random(content, 3, None);
            }
            "RELIC.SAND_CASTLE" => self.upgrade_random(content, 6, None),
            "RELIC.WAR_PAINT" => self.upgrade_random(content, 2, Some(CardType::Skill)),
            "RELIC.WHETSTONE" => self.upgrade_random(content, 2, Some(CardType::Attack)),
            "RELIC.LEAFY_POULTICE" => {
                self.run.max_hp = (self.run.max_hp - 12).max(1);
                self.run.hp = self.run.hp.min(self.run.max_hp);
                for tag in [STRIKE_TAG, DEFEND_TAG] {
                    if let Some(index) = self.run.deck.iter().position(|card| {
                        let def = content.cards[card.id as usize];
                        def.rarity == CardRarity::Basic && def.tags & tag != 0
                    }) {
                        self.rngs.transformations =
                            self.transform_at(content, index, self.rngs.transformations);
                    }
                }
            }
            "RELIC.ORRERY" => {
                let cards = self.make_card_reward(content, CardReward::Standard(Room::Combat));
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards,
                    card_rewards: vec![CardReward::Standard(Room::Combat); 4],
                    relics: vec![],
                    potions: vec![],
                    removals: 0,
                });
            }
            "RELIC.KALEIDOSCOPE" => {
                let cards = self.make_card_reward(content, CardReward::Kaleidoscope);
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards,
                    card_rewards: vec![CardReward::Kaleidoscope],
                    relics: vec![],
                    potions: vec![],
                    removals: 0,
                });
            }
            "RELIC.SCROLL_BOXES" => {
                let mut pool = content.characters[self.run.character as usize]
                    .cards
                    .to_vec();
                if self.has_relic(content, "RELIC.PRISMATIC_GEM") {
                    pool = content
                        .characters
                        .iter()
                        .flat_map(|character| character.cards)
                        .copied()
                        .collect();
                } else if self.has_relic(content, "RELIC.DINGY_RUG") {
                    pool.extend_from_slice(content.colorless());
                }
                let mut used = Vec::new();
                let mut bundles = Vec::new();
                for _ in 0..2 {
                    if content.characters[self.run.character as usize].id == "CHARACTER.DEFECT"
                        && self.rngs.rewards.below(100) == 0
                    {
                        if let Some(id) = content.card_id("CARD.CLAW") {
                            bundles.push(vec![
                                Card {
                                    id,
                                    ..Card::default()
                                };
                                3
                            ]);
                            continue;
                        }
                    }
                    let mut bundle = Vec::new();
                    for rarity in [CardRarity::Common, CardRarity::Common, CardRarity::Uncommon] {
                        let options: Vec<_> = pool
                            .iter()
                            .copied()
                            .filter(|id| {
                                content.cards[*id as usize].rarity == rarity && !used.contains(id)
                            })
                            .collect();
                        if !options.is_empty() {
                            let id =
                                options[self.rngs.rewards.below(options.len() as u32) as usize];
                            used.push(id);
                            bundle.push(Card {
                                id,
                                ..Card::default()
                            });
                        }
                    }
                    bundles.push(bundle);
                }
                self.phase = Phase::ChooseBundles(bundles);
            }
            "RELIC.GLASS_EYE" => {
                let character = self.run.character;
                let cards = self
                    .make_card_reward(content, CardReward::Fixed(character, CardRarity::Common));
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards,
                    card_rewards: [
                        CardRarity::Common,
                        CardRarity::Uncommon,
                        CardRarity::Uncommon,
                        CardRarity::Rare,
                    ]
                    .into_iter()
                    .map(|rarity| CardReward::Fixed(character, rarity))
                    .collect(),
                    relics: vec![],
                    potions: vec![],
                    removals: 0,
                });
            }
            "RELIC.SEA_GLASS" => {
                let characters: Vec<_> = content
                    .characters
                    .iter()
                    .enumerate()
                    .filter(|(id, character)| {
                        *id != self.run.character as usize && !character.cards.is_empty()
                    })
                    .map(|(id, _)| id as Id)
                    .collect();
                let character = if characters.is_empty() {
                    self.run.character
                } else {
                    let choice = if let Some(rng) = &mut self.event_rng {
                        rng.below(characters.len() as u32)
                    } else {
                        self.rngs.rewards.below(characters.len() as u32)
                    };
                    characters[choice as usize]
                };
                let pool = content.characters[character as usize].cards;
                let mut cards = Vec::new();
                for rarity in [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare] {
                    cards.extend(self.fixed_cards(content, pool, rarity, 5));
                }
                self.phase = Phase::ChooseCards(cards, 15, true);
            }
            "RELIC.PAELS_TOOTH" => {
                let cards: Vec<_> = self
                    .run
                    .deck
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        card.upgrades == 0
                            && card.flags(content.cards[card.id as usize]) & ETERNAL == 0
                            && !matches!(
                                content.cards[card.id as usize].card_type,
                                CardType::Status | CardType::Curse | CardType::Quest
                            )
                    })
                    .map(|(index, _)| index)
                    .collect();
                self.paels_tooth = true;
                if cards.len() <= 5 {
                    for index in cards.into_iter().rev() {
                        self.paels_cards.push(self.run.deck.remove(index));
                    }
                    self.paels_cards
                        .sort_by_key(|card| content.cards[card.id as usize].id);
                    self.paels_tooth = false;
                } else {
                    self.phase = Phase::RemoveCards(5, 0, false);
                }
            }
            "RELIC.SERE_TALON" => {
                let mut curses: Vec<_> = content
                    .cards
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        card.card_type == CardType::Curse
                            && card.id != "CARD.ASCENDERS_BANE"
                            && card.flags[0] & ETERNAL == 0
                            && card.flags[0] & NO_GENERATE == 0
                    })
                    .map(|(id, _)| id as Id)
                    .collect();
                curses.sort_by_key(|id| content.cards[*id as usize].id);
                for _ in 0..2.min(curses.len()) {
                    let id = curses.remove(self.rngs.niche.below(curses.len() as u32) as usize);
                    self.add_card(
                        content,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
                if let Some(id) = content.card_id("CARD.WISH") {
                    for _ in 0..3 {
                        self.add_card(
                            content,
                            Card {
                                id,
                                ..Card::default()
                            },
                        );
                    }
                }
            }
            "RELIC.TOY_BOX" => {
                let mut relics = Vec::new();
                for _ in 0..4 {
                    let rarity = self.roll_relic_rarity();
                    relics.extend(self.pull_relic(false, rarity, false, false));
                }
                self.toy_box_offers = relics.clone();
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics,
                    potions: vec![],
                    removals: 0,
                });
            }
            "RELIC.DUSTY_TOME" => {
                let cards: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|id| {
                        content.cards[*id as usize].rarity == CardRarity::Ancient
                            && !matches!(
                                content.cards[*id as usize].id,
                                "CARD.BREAK"
                                    | "CARD.SUPPRESS"
                                    | "CARD.PROTECTOR"
                                    | "CARD.METEOR_SHOWER"
                                    | "CARD.QUADCAST"
                            )
                    })
                    .collect();
                if !cards.is_empty() {
                    let id = cards[self.rngs.rewards.below(cards.len() as u32) as usize];
                    self.add_card(
                        content,
                        Card {
                            id,
                            upgrades: 1,
                            ..Card::default()
                        },
                    );
                }
            }
            "RELIC.ARCHAIC_TOOTH" => {
                for (from, to) in [
                    ("CARD.BASH", "CARD.BREAK"),
                    ("CARD.NEUTRALIZE", "CARD.SUPPRESS"),
                    ("CARD.UNLEASH", "CARD.PROTECTOR"),
                    ("CARD.FALLING_STAR", "CARD.METEOR_SHOWER"),
                    ("CARD.DUALCAST", "CARD.QUADCAST"),
                ] {
                    if let Some(card) = self
                        .run
                        .deck
                        .iter_mut()
                        .find(|card| content.cards[card.id as usize].id == from)
                        && let Some(id) = content.card_id(to)
                    {
                        card.id = id;
                        card.upgrades = 0;
                        card.enchantment = None;
                        break;
                    }
                }
            }
            "RELIC.TOUCH_OF_OROBAS" => {
                for (from, to) in [
                    ("RELIC.BURNING_BLOOD", "RELIC.BLACK_BLOOD"),
                    ("RELIC.RING_OF_THE_SNAKE", "RELIC.RING_OF_THE_DRAKE"),
                    ("RELIC.DIVINE_RIGHT", "RELIC.DIVINE_DESTINY"),
                    ("RELIC.BOUND_PHYLACTERY", "RELIC.PHYLACTERY_UNBOUND"),
                    ("RELIC.CRACKED_CORE", "RELIC.INFUSED_CORE"),
                ] {
                    if let Some(relic) = self
                        .run
                        .relics
                        .iter_mut()
                        .find(|id| content.relics[**id as usize].id == from)
                        && let Some(id) = content.relic_id(to)
                    {
                        *relic = id;
                        break;
                    }
                }
            }
            "RELIC.PANDORAS_BOX" => {
                let indices: Vec<_> = self
                    .run
                    .deck
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        let def = content.cards[card.id as usize];
                        def.rarity == CardRarity::Basic
                            && def.tags & (STRIKE_TAG | DEFEND_TAG) != 0
                            && card.flags(def) & ETERNAL == 0
                    })
                    .map(|(index, _)| index)
                    .collect();
                for (removed, index) in indices.into_iter().enumerate() {
                    self.rngs.niche = self.transform_at(content, index - removed, self.rngs.niche);
                }
            }
            "RELIC.PHIAL_HOLSTER" => {
                let pool = self.potion_pool(content);
                let potions = self.random_potions(&pool, 2, false, true);
                self.run.potions.push(None);
                if !self.has_relic(content, "RELIC.SOZU") {
                    for potion in potions {
                        if let Some(slot) = self.run.potions.iter_mut().find(|slot| slot.is_none())
                        {
                            *slot = Some(potion);
                        }
                    }
                }
            }
            "RELIC.PRESERVED_FOG" => {
                self.run_queue.push(RunEffect::AddCard("CARD.FOLLY", 1));
                self.phase = Phase::RemoveCards(3, 0, false);
            }
            "RELIC.NUTRITIOUS_OYSTER" => {
                self.run.max_hp += 11;
                self.run.hp += 11;
            }
            "RELIC.POTION_BELT" => self.run.potions.extend([None; 2]),
            "RELIC.BYRDPIP" => {
                if let Some(id) = content.card_id("CARD.BYRD_SWOOP") {
                    for card in &mut self.run.deck {
                        if content.cards[card.id as usize].id == "CARD.BYRDONIS_EGG" {
                            card.id = id;
                        }
                    }
                }
            }
            "RELIC.WONGOS_MYSTERY_TICKET" => self.wongo_combats = Some(0),
            "RELIC.GOLDEN_PEARL" => self.gain_gold(content, 150),
            "RELIC.DISTINGUISHED_CAPE" => {
                self.run.max_hp = (self.run.max_hp - 9).max(1);
                self.run.hp = self.run.hp.min(self.run.max_hp);
                if let Some(card) = content.card_id("CARD.APPARITION") {
                    for _ in 0..3 {
                        self.add_card(
                            content,
                            Card {
                                id: card,
                                ..Card::default()
                            },
                        );
                    }
                }
            }
            "RELIC.LARGE_CAPSULE" => {
                for _ in 0..2 {
                    let rarity = self.roll_relic_rarity();
                    let relic = if self.relic_queue.is_empty() {
                        self.pull_relic(false, rarity, false, false)
                    } else {
                        Some(self.relic_queue.remove(0))
                    };
                    if let Some(relic) = relic {
                        self.obtain_relic(content, relic);
                    }
                }
                let cards: Vec<_> = [STRIKE_TAG, DEFEND_TAG]
                    .into_iter()
                    .filter_map(|tag| {
                        self.run.deck.iter().find(|card| {
                            let id = card.id;
                            let card = content.cards[id as usize];
                            card.rarity == CardRarity::Basic && card.tags & tag != 0
                        })
                    })
                    .map(|card| card.id)
                    .collect();
                for id in cards {
                    self.add_card(
                        content,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
            }
            "RELIC.PAELS_GROWTH" => {
                self.resume = Some(Phase::Map);
                self.phase = Phase::EnchantCards(Enchantment::Clone, 4, 1, None, false);
            }
            "RELIC.EMPTY_CAGE" => self.phase = Phase::RemoveCards(2, 0, false),
            "RELIC.PRECISE_SCISSORS" => self.phase = Phase::RemoveCards(1, 0, false),
            "RELIC.BIIIG_HUG" => self.phase = Phase::RemoveCards(4, 0, false),
            "RELIC.PRECARIOUS_SHEARS" => {
                self.run.hp = (self.run.hp - 16).max(0);
                self.phase = Phase::RemoveCards(2, 0, false);
            }
            "RELIC.ASTROLABE" => {
                self.astrolabe = true;
                self.transform_niche = true;
                self.phase = Phase::TransformCards(None, 3, false);
            }
            "RELIC.NEW_LEAF" => {
                self.transform_niche = true;
                self.phase = Phase::TransformCards(None, 1, false);
            }
            "RELIC.CLAWS" => {
                if let Some(id) = content.card_id("CARD.MAUL") {
                    self.phase = Phase::TransformCards(Some(id), 6, false);
                }
            }
            "RELIC.POMANDER" => self.phase = Phase::UpgradeCards(1, false),
            "RELIC.YUMMY_COOKIE" => self.phase = Phase::UpgradeCards(4, false),
            "RELIC.ELECTRIC_SHRYMP" => {
                self.phase = Phase::EnchantCards(Enchantment::Imbued, 1, 1, None, false)
            }
            "RELIC.BEAUTIFUL_BRACELET" => {
                self.phase = Phase::EnchantCards(Enchantment::Swift, 3, 3, None, false)
            }
            "RELIC.TRI_BOOMERANG" => {
                self.phase = Phase::EnchantCards(Enchantment::Instinct, 1, 3, None, false)
            }
            "RELIC.PAELS_CLAW" => {
                for card in &mut self.run.deck {
                    if can_enchant(content, card, Enchantment::Goopy, None) {
                        card.enchantment = Some(Enchantment::Goopy);
                        card.enchantment_amount = 1;
                    }
                }
            }
            "RELIC.NUTRITIOUS_SOUP" => {
                for card in &mut self.run.deck {
                    let def = content.cards[card.id as usize];
                    if def.rarity == CardRarity::Basic
                        && def.tags & STRIKE_TAG != 0
                        && can_enchant(content, card, Enchantment::TezcatarasEmber, None)
                    {
                        card.enchantment = Some(Enchantment::TezcatarasEmber);
                        card.enchantment_amount = 1;
                    }
                }
            }
            "RELIC.NEOWS_TALISMAN" => {
                for tag in [STRIKE_TAG, DEFEND_TAG] {
                    if let Some(card) = self.run.deck.iter_mut().rev().find(|card| {
                        let def = content.cards[card.id as usize];
                        def.rarity == CardRarity::Basic && def.tags & tag != 0
                    }) {
                        card.upgrades = 1;
                    }
                }
            }
            "RELIC.CURSED_PEARL" => {
                self.gain_gold(content, 333);
                if let Some(id) = content.card_id("CARD.GREED") {
                    self.add_card(
                        content,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
            }
            "RELIC.SIGNET_RING" => self.gain_gold(content, 999),
            "RELIC.LOOMING_FRUIT" => {
                self.run.max_hp += 31;
                self.run.hp += 31;
            }
            "RELIC.BLOOD_SOAKED_ROSE" => {
                self.run.energy += 1;
                if let Some(id) = content.card_id("CARD.ENTHRALLED") {
                    self.add_card(
                        content,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
            }
            "RELIC.SILKEN_TRESS" => self.run.gold = 0,
            "RELIC.GOLDEN_COMPASS" => {
                self.golden_compass = Some(self.run.act);
                self.map = self.golden_map();
            }
            "RELIC.FUR_COAT" => {
                let mut rng = Rng::from_seed(self.seed.wrapping_add(hash("FUR_COAT")) as u64);
                self.fur_coat = self
                    .map
                    .nodes
                    .iter()
                    .filter(|node| matches!(node.room, Room::Combat | Room::Elite))
                    .map(|node| (node.lane, node.floor))
                    .collect();
                rng.shuffle(&mut self.fur_coat);
                self.fur_coat.truncate(7);
                self.fur_coat_act = Some(self.run.act);
            }
            "RELIC.PUMPKIN_CANDLE" => self.pumpkin_candle = 5,
            "RELIC.ALCHEMICAL_COFFER" => {
                let pool = self.potion_pool(content);
                let potions = self.random_potions(&pool, 4, false, false);
                self.run
                    .potions
                    .extend(if self.has_relic(content, "RELIC.SOZU") {
                        vec![None; 4]
                    } else {
                        potions.into_iter().map(Some).collect()
                    });
            }
            "RELIC.SMALL_CAPSULE" => {
                let rarity = self.roll_relic_rarity();
                let relics = self
                    .pull_relic(false, rarity, false, false)
                    .into_iter()
                    .collect();
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics,
                    potions: vec![],
                    removals: 0,
                });
            }
            "RELIC.LOST_COFFER" => {
                let pool = content.characters[self.run.character as usize].cards;
                let cards = self.reward_cards(content, pool, 3, Room::Combat);
                let potion_pool = self.potion_pool(content);
                let potions = self.reward_potions(&potion_pool, 1);
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards,
                    card_rewards: vec![],
                    relics: vec![],
                    potions,
                    removals: 0,
                });
            }
            "RELIC.ARCANE_SCROLL" => {
                let pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| content.cards[id as usize].rarity == CardRarity::Rare)
                    .collect();
                if !pool.is_empty() {
                    let id = pool[self.rngs.rewards.below(pool.len() as u32) as usize];
                    self.add_card(
                        content,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
            }
            id @ ("RELIC.NEOWS_TORMENT"
            | "RELIC.JEWELRY_BOX"
            | "RELIC.PAELS_HORN"
            | "RELIC.STORYBOOK"
            | "RELIC.TANXS_WHISTLE") => {
                let (card, count) = match id {
                    "RELIC.NEOWS_TORMENT" => ("CARD.NEOWS_FURY", 1),
                    "RELIC.JEWELRY_BOX" => ("CARD.APOTHEOSIS", 1),
                    "RELIC.PAELS_HORN" => ("CARD.RELAX", 2),
                    "RELIC.STORYBOOK" => ("CARD.BRIGHTEST_FLAME", 1),
                    _ => ("CARD.WHISTLE", 1),
                };
                if let Some(id) = content.card_id(card) {
                    for _ in 0..count {
                        self.add_card(
                            content,
                            Card {
                                id,
                                ..Card::default()
                            },
                        );
                    }
                }
            }
            "RELIC.LEAD_PAPERWEIGHT" => {
                let cards = self.card_rewards(content, content.colorless(), 2, Room::Combat);
                self.resume = Some(Phase::Map);
                self.phase = Phase::ChooseCards(cards, 1, true);
            }
            "RELIC.NEOWS_BONES" => {
                let mut relics: Vec<_> = [
                    "RELIC.CURSED_PEARL",
                    "RELIC.HEFTY_TABLET",
                    "RELIC.LARGE_CAPSULE",
                    "RELIC.LEAFY_POULTICE",
                    "RELIC.PRECARIOUS_SHEARS",
                    "RELIC.SILKEN_TRESS",
                    "RELIC.SILVER_CRUCIBLE",
                    "RELIC.ARCANE_SCROLL",
                    "RELIC.BOOMING_CONCH",
                    "RELIC.FISHING_ROD",
                    "RELIC.GOLDEN_PEARL",
                    "RELIC.KALEIDOSCOPE",
                    "RELIC.LEAD_PAPERWEIGHT",
                    "RELIC.LOST_COFFER",
                    "RELIC.NEOWS_TORMENT",
                    "RELIC.NEW_LEAF",
                    "RELIC.PHIAL_HOLSTER",
                    "RELIC.PRECISE_SCISSORS",
                    "RELIC.SCROLL_BOXES",
                    "RELIC.WINGED_BOOTS",
                    "RELIC.LAVA_ROCK",
                    "RELIC.NEOWS_TALISMAN",
                    "RELIC.NUTRITIOUS_OYSTER",
                    "RELIC.POMANDER",
                    "RELIC.SMALL_CAPSULE",
                    "RELIC.STONE_HUMIDIFIER",
                ]
                .into_iter()
                .filter_map(|id| content.relic_id(id))
                .collect();
                self.rngs.rewards.shuffle(&mut relics);
                relics.truncate(2);
                self.pending_curse = true;
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics,
                    potions: vec![],
                    removals: 0,
                });
            }
            _ => {}
        }
    }

    fn gain_gold(&mut self, content: &Content, amount: i32) {
        if amount <= 0 || self.has_relic(content, "RELIC.ECTOPLASM") {
            return;
        }
        let amount = if self.has_relic(content, "RELIC.BOWLER_HAT") {
            amount.saturating_mul(5) / 4
        } else {
            amount
        };
        self.run.gold = self.run.gold.saturating_add(amount);
        if self.has_relic(content, "RELIC.DRAGON_FRUIT") {
            self.run.max_hp += 1;
            self.run.hp += 1;
        }
    }

    fn add_card(&mut self, content: &Content, mut card: Card) {
        let relic = |wanted| self.has_relic(content, wanted);
        let duplicate = relic("RELIC.BING_BONG");
        let darkstone_periapt = relic("RELIC.DARKSTONE_PERIAPT");
        let book_of_five_rings = relic("RELIC.BOOK_OF_FIVE_RINGS");
        let gold = if relic("RELIC.LUCKY_FYSH") { 15 } else { 0 };
        let card_type = content.cards[card.id as usize].card_type;
        if card.upgrades == 0
            && match card_type {
                CardType::Attack => relic("RELIC.MOLTEN_EGG"),
                CardType::Skill => relic("RELIC.TOXIC_EGG"),
                CardType::Power => relic("RELIC.FROZEN_EGG"),
                _ => false,
            }
        {
            card.upgrades = 1;
        }
        if relic("RELIC.FRESNEL_LENS") && can_enchant(content, &card, Enchantment::Nimble, None) {
            card.enchantment = Some(Enchantment::Nimble);
            card.enchantment_amount = 2;
        }
        if card.instance == 0 {
            card.instance = self.next_card;
            self.next_card += 1;
        }
        self.run.deck.push(card);
        self.gain_gold(content, gold);
        let mut added = 1;
        if duplicate {
            card.instance = self.next_card;
            self.next_card += 1;
            self.run.deck.push(card);
            self.gain_gold(content, gold);
            added += 1;
        }
        if card_type == CardType::Curse && darkstone_periapt {
            self.run.max_hp += 6 * added;
            self.run.hp += 6 * added;
        }
        if book_of_five_rings {
            let cards = self.book_of_five_rings as i16 + added;
            self.book_of_five_rings = (cards % 5) as u8;
            self.run.hp = (self.run.hp + 20 * (cards / 5)).min(self.run.max_hp);
        }
    }

    fn upgrade_random(&mut self, content: &Content, count: usize, card_type: Option<CardType>) {
        let mut cards: Vec<_> = self
            .run
            .deck
            .iter()
            .enumerate()
            .filter(|(_, card)| {
                card.upgrades == 0
                    && !matches!(
                        content.cards[card.id as usize].card_type,
                        CardType::Status | CardType::Curse | CardType::Quest
                    )
                    && card_type.is_none_or(|card_type| {
                        content.cards[card.id as usize].card_type == card_type
                    })
            })
            .map(|(index, _)| index)
            .collect();
        cards.sort_by_key(|index| content.cards[self.run.deck[*index].id as usize].id);
        self.rngs.niche.shuffle(&mut cards);
        for index in cards.into_iter().take(count) {
            self.run.deck[index].upgrades = 1;
        }
    }

    fn transform_at(&mut self, content: &Content, index: usize, mut rng: Rng) -> Rng {
        let old = self.run.deck[index];
        let mut pool: Vec<_> = content.characters[self.run.character as usize]
            .cards
            .iter()
            .copied()
            .filter(|id| {
                *id != old.id
                    && matches!(
                        content.cards[*id as usize].rarity,
                        CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                    )
            })
            .collect();
        pool.sort_by_key(|id| content.cards[*id as usize].id);
        let id = pool[rng.below(pool.len() as u32) as usize];
        self.run.deck.remove(index);
        self.add_card(
            content,
            Card {
                id,
                ..Card::default()
            },
        );
        rng
    }

    fn transform_selected(&mut self, content: &Content, index: usize, target: Option<Id>) {
        let old = self.run.deck[index];
        let id = target.unwrap_or_else(|| {
            let source = if content.colorless().contains(&old.id) {
                content.colorless()
            } else {
                content.characters[self.run.character as usize].cards
            };
            let mut pool: Vec<_> = source
                .iter()
                .copied()
                .filter(|&id| {
                    id != old.id
                        && matches!(
                            content.cards[id as usize].rarity,
                            CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                        )
                })
                .collect();
            pool.sort_unstable_by_key(|&id| content.cards[id as usize].id);
            let rng = if self.transform_niche {
                &mut self.rngs.niche
            } else {
                self.event_rng
                    .as_mut()
                    .unwrap_or(&mut self.rngs.transformations)
            };
            pool[rng.below(pool.len() as u32) as usize]
        });
        self.run.deck.remove(index);
        let preserve = target.is_some();
        self.add_card(
            content,
            Card {
                id,
                upgrades: if self.astrolabe {
                    1
                } else if preserve {
                    old.upgrades
                } else {
                    0
                },
                enchantment: preserve
                    .then_some(old.enchantment)
                    .flatten()
                    .filter(|enchantment| {
                        can_enchant(
                            content,
                            &Card {
                                id,
                                ..Card::default()
                            },
                            *enchantment,
                            None,
                        )
                    }),
                enchantment_amount: old.enchantment_amount,
                enchantment_value: old.enchantment_value,
                ..Card::default()
            },
        );
    }

    fn resolve_run(&mut self, content: &Content) {
        while let Some(effect) = self.run_queue.pop() {
            match effect {
                RunEffect::Gold(amount) if amount > 0 => self.gain_gold(content, amount),
                RunEffect::Gold(amount) => self.run.gold = (self.run.gold + amount).max(0),
                RunEffect::RandomGold(min, max) => {
                    let amount = min
                        + self
                            .event_rng
                            .as_mut()
                            .map_or(0, |rng| rng.below((max - min + 1) as u32) as i32);
                    self.gain_gold(content, amount);
                }
                RunEffect::LoseAllGold => self.run.gold = 0,
                RunEffect::Heal(amount) => {
                    self.run.hp = (self.run.hp + amount).min(self.run.max_hp)
                }
                RunEffect::HealPercent(percent) => {
                    let amount = self.run.max_hp.saturating_mul(percent as i16) / 100;
                    self.run.hp = (self.run.hp + amount).min(self.run.max_hp);
                }
                RunEffect::FullHeal => self.run.hp = self.run.max_hp,
                RunEffect::LoseHp(amount) => self.run.hp = (self.run.hp - amount).max(0),
                RunEffect::MaxHp(amount) => {
                    self.run.max_hp = (self.run.max_hp + amount).max(1);
                    self.run.hp = (self.run.hp + amount.max(0)).min(self.run.max_hp);
                }
                RunEffect::MaxHpTo(amount) => {
                    self.run.max_hp = amount.max(1);
                    self.run.hp = self.run.hp.min(self.run.max_hp);
                }
                RunEffect::AddCard(id, count) => {
                    let id = content.card_id(id).unwrap();
                    for _ in 0..count {
                        self.add_card(
                            content,
                            Card {
                                id,
                                ..Card::default()
                            },
                        );
                    }
                }
                RunEffect::AddRelic(id) => {
                    self.obtain_relic(content, content.relic_id(id).unwrap())
                }
                RunEffect::AddPotion(id) => {
                    let id = content.potion_id(id).unwrap();
                    if !self.has_relic(content, "RELIC.SOZU")
                        && let Some(slot) = self.run.potions.iter_mut().find(|x| x.is_none())
                    {
                        *slot = Some(id)
                    }
                }
                RunEffect::PotionRewards(id, count) => {
                    self.phase = Phase::Rewards(Rewards {
                        gold: 0,
                        cards: vec![],
                        card_rewards: vec![],
                        relics: vec![],
                        potions: vec![content.potion_id(id).unwrap(); count as usize],
                        removals: 0,
                    });
                    return;
                }
                RunEffect::RandomPotionReward(uncommon) => {
                    let mut pool = self.potion_pool(content);
                    if uncommon {
                        pool.retain(|id| UNCOMMON_POTIONS.contains(id));
                    }
                    let id = pool[self.rngs.rewards.below(pool.len() as u32) as usize];
                    self.phase = Phase::Rewards(Rewards {
                        gold: 0,
                        cards: vec![],
                        card_rewards: vec![],
                        relics: vec![],
                        potions: vec![id],
                        removals: 0,
                    });
                    return;
                }
                RunEffect::NextRelics(count) => {
                    for _ in 0..count {
                        let id = if self.relic_queue.is_empty() {
                            let rarity = self.roll_relic_rarity();
                            self.pull_relic(false, rarity, false, false)
                        } else {
                            Some(self.relic_queue.remove(0))
                        };
                        if let Some(id) = id {
                            self.obtain_relic(content, id);
                        }
                    }
                }
                RunEffect::RelicOfRarity(rarity) => {
                    if let Some(id) = self.pull_relic(false, rarity as usize, false, true) {
                        self.obtain_relic(content, id);
                    }
                }
                RunEffect::EventRelic => {
                    if let Some(id) = self
                        .event_relic
                        .take()
                        .or_else(|| self.pull_relic(false, 2, false, true))
                    {
                        self.obtain_relic(content, id);
                    }
                }
                RunEffect::RandomCard(ids) => {
                    let index = self.event_rng.as_mut().unwrap().below(ids.len() as u32) as usize;
                    let id = content.card_id(ids[index]).unwrap();
                    self.add_card(
                        content,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
                RunEffect::RandomRelic(ids) => {
                    let index = self.event_rng.as_mut().unwrap().below(ids.len() as u32) as usize;
                    self.obtain_relic(content, content.relic_id(ids[index]).unwrap());
                }
                RunEffect::DiscardPotion(index) => {
                    if let Some(slot) = self
                        .run
                        .potions
                        .iter_mut()
                        .filter(|potion| potion.is_some())
                        .nth(index as usize)
                    {
                        *slot = None;
                    }
                }
                RunEffect::DiscardRandomPotion => {
                    let slots: Vec<_> = self
                        .run
                        .potions
                        .iter()
                        .enumerate()
                        .filter_map(|(slot, potion)| potion.map(|_| slot))
                        .collect();
                    if !slots.is_empty() {
                        let choice = self.event_rng.as_mut().unwrap().below(slots.len() as u32);
                        self.run.potions[slots[choice as usize]] = None;
                    }
                }
                RunEffect::RemoveCards(count, tag) => {
                    let mut cards: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            let def = content.cards[card.id as usize];
                            card.flags(def) & ETERNAL == 0
                                && (tag == 0
                                    || def.rarity == CardRarity::Basic && def.tags & tag != 0)
                        })
                        .map(|(index, _)| index)
                        .collect();
                    if cards.len() > count as usize {
                        self.phase = Phase::RemoveCards(count, tag, false);
                        return;
                    }
                    for index in cards.drain(..).rev() {
                        self.run.deck.remove(index);
                    }
                }
                RunEffect::UpgradeCards(count) => {
                    let cards: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            card.upgrades == 0
                                && !matches!(
                                    content.cards[card.id as usize].card_type,
                                    CardType::Status | CardType::Curse | CardType::Quest
                                )
                        })
                        .map(|(index, _)| index)
                        .collect();
                    if cards.len() > count as usize {
                        self.phase = Phase::UpgradeCards(count, false);
                        return;
                    }
                    for index in cards {
                        self.run.deck[index].upgrades = 1;
                    }
                }
                RunEffect::UpgradeRandom(count) => {
                    let mut indices: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            card.upgrades == 0
                                && !matches!(
                                    content.cards[card.id as usize].card_type,
                                    CardType::Status | CardType::Curse | CardType::Quest
                                )
                        })
                        .map(|(index, _)| index)
                        .collect();
                    for _ in 0..count {
                        if indices.is_empty() {
                            break;
                        }
                        let choice =
                            self.event_rng
                                .as_mut()
                                .unwrap_or(&mut self.rngs.transformations)
                                .below(indices.len() as u32) as usize;
                        self.run.deck[indices.remove(choice)].upgrades = 1;
                    }
                }
                RunEffect::UpgradeShuffled(count) => {
                    let mut cards: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            card.upgrades == 0
                                && !matches!(
                                    content.cards[card.id as usize].card_type,
                                    CardType::Status | CardType::Curse | CardType::Quest
                                )
                        })
                        .map(|(index, _)| index)
                        .collect();
                    cards.sort_by_key(|index| {
                        (
                            content.cards[self.run.deck[*index].id as usize].id,
                            self.run.deck[*index].instance,
                        )
                    });
                    self.event_rng
                        .as_mut()
                        .unwrap_or(&mut self.rngs.transformations)
                        .shuffle(&mut cards);
                    for index in cards.into_iter().take(count as usize) {
                        self.run.deck[index].upgrades = 1;
                    }
                }
                RunEffect::UpgradeAll => {
                    for card in &mut self.run.deck {
                        if !matches!(
                            content.cards[card.id as usize].card_type,
                            CardType::Status | CardType::Curse | CardType::Quest
                        ) {
                            card.upgrades = 1;
                        }
                    }
                }
                RunEffect::DowngradeRandom(count) => {
                    let mut indices: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| card.upgrades > 0)
                        .map(|(index, _)| index)
                        .collect();
                    for _ in 0..count {
                        if indices.is_empty() {
                            break;
                        }
                        let choice =
                            self.event_rng
                                .as_mut()
                                .unwrap_or(&mut self.rngs.transformations)
                                .below(indices.len() as u32) as usize;
                        self.run.deck[indices.remove(choice)].upgrades -= 1;
                    }
                }
                RunEffect::CloneDeck => {
                    let cards = self.run.deck.clone();
                    for mut card in cards {
                        card.instance = 0;
                        self.add_card(content, card);
                    }
                }
                RunEffect::TransformCards(target, count) => {
                    let target = target.and_then(|id| content.card_id(id));
                    let cards: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .filter(|card| {
                            let def = content.cards[card.id as usize];
                            def.card_type != CardType::Quest
                                && card.flags(def) & ETERNAL == 0
                                && target.is_none_or(|_| def.rarity == CardRarity::Basic)
                        })
                        .map(|card| card.instance)
                        .collect();
                    if cards.len() > count as usize {
                        self.phase = Phase::TransformCards(target, count, false);
                        return;
                    }
                    for instance in cards {
                        let index = self
                            .run
                            .deck
                            .iter()
                            .position(|card| card.instance == instance)
                            .unwrap();
                        self.transform_selected(content, index, target);
                    }
                }
                RunEffect::EnchantCards(enchantment, amount, count, card_type) => {
                    let cards: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| can_enchant(content, card, enchantment, card_type))
                        .map(|(index, _)| index)
                        .collect();
                    if cards.len() > count as usize {
                        self.phase =
                            Phase::EnchantCards(enchantment, amount, count, card_type, false);
                        return;
                    }
                    for index in cards {
                        let card = &mut self.run.deck[index];
                        card.enchantment = Some(enchantment);
                        card.enchantment_amount = amount;
                    }
                }
                RunEffect::SkipEventRng(count) => self
                    .event_rng
                    .as_mut()
                    .unwrap_or(&mut self.rngs.niche)
                    .forward(count as u64),
                RunEffect::ChooseCommonCards(offered, picks) => {
                    let mut pool: Vec<_> = content.characters[self.run.character as usize]
                        .cards
                        .iter()
                        .copied()
                        .filter(|&id| content.cards[id as usize].rarity == CardRarity::Common)
                        .collect();
                    pool.sort_unstable_by_key(|&id| content.cards[id as usize].id);
                    let mut cards = Vec::new();
                    for _ in 0..offered {
                        let index = self.rngs.rewards.below(pool.len() as u32) as usize;
                        cards.push(Card {
                            id: pool.remove(index),
                            ..Card::default()
                        });
                    }
                    self.phase = Phase::ChooseCards(cards, picks, false);
                    return;
                }
                RunEffect::ChooseRewardCards(offered, picks) => {
                    let pool = content.characters[self.run.character as usize].cards;
                    let pool = if pool.is_empty() {
                        content.acts[self.act as usize].cards
                    } else {
                        pool
                    };
                    let cards = self.card_rewards(content, pool, offered as usize, Room::Combat);
                    self.phase = Phase::ChooseCards(cards, picks, true);
                    return;
                }
                RunEffect::RemoveRandomCard => {
                    let removable =
                        |card: &Card| card.flags(content.cards[card.id as usize]) & ETERNAL == 0;
                    let mut cards: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            removable(card)
                                && content.cards[card.id as usize].rarity != CardRarity::Basic
                        })
                        .map(|(index, _)| index)
                        .collect();
                    if cards.is_empty() {
                        cards.extend(
                            self.run
                                .deck
                                .iter()
                                .enumerate()
                                .filter(|(_, card)| removable(card))
                                .map(|(index, _)| index),
                        );
                    }
                    if !cards.is_empty() {
                        let roll = self
                            .event_rng
                            .as_mut()
                            .map_or(0, |rng| rng.below(cards.len() as u32) as usize);
                        self.run.deck.remove(cards[roll]);
                    }
                }
                RunEffect::Options(options) => {
                    let Phase::Event(id, _) = self.phase else {
                        unreachable!()
                    };
                    self.phase = Phase::Event(id, options.to_vec());
                    self.resume = None;
                    return;
                }
                RunEffect::EventAction(action) => {
                    if self.event_action(content, action) {
                        return;
                    }
                }
            }
            if self.run.hp == 0 {
                self.phase = Phase::Dead;
                self.run_queue.clear();
                return;
            }
        }
        self.phase = self.resume.take().unwrap_or(Phase::Map);
    }

    fn finish_run_choice(&mut self, content: &Content) {
        if self.cooking {
            self.cooking = false;
            self.run.max_hp += 9;
            self.run.hp += 9;
            self.phase = self.rest_destination(content, 16);
            return;
        }
        self.astrolabe = false;
        self.transform_niche = false;
        if self.conveyor {
            self.continue_conveyor();
        } else if self.run_queue.is_empty() {
            if let Some(phase) = self.resume.take() {
                self.phase = phase;
            } else if !self.parasol.is_empty() {
                self.continue_parasol(content);
            } else {
                self.phase = Phase::Map;
            }
        } else {
            self.resolve_run(content);
        }
    }

    fn finish_empty_run_choices(&mut self, content: &Content) {
        loop {
            let empty = match &self.phase {
                Phase::RemoveCards(_, tag, false) => !self.run.deck.iter().any(|card| {
                    let def = content.cards[card.id as usize];
                    card.flags(def) & ETERNAL == 0
                        && (!self.paels_tooth
                            || card.upgrades == 0
                                && !matches!(
                                    def.card_type,
                                    CardType::Status | CardType::Curse | CardType::Quest
                                ))
                        && (*tag == 0 || def.rarity == CardRarity::Basic && def.tags & tag != 0)
                }),
                Phase::UpgradeCards(_, false) => !self.run.deck.iter().any(|card| {
                    card.upgrades == 0
                        && !matches!(
                            content.cards[card.id as usize].card_type,
                            CardType::Status | CardType::Curse | CardType::Quest
                        )
                }),
                Phase::TransformCards(_, _, false) => !self.run.deck.iter().any(|card| {
                    let def = content.cards[card.id as usize];
                    def.card_type != CardType::Quest && card.flags(def) & ETERNAL == 0
                }),
                Phase::EnchantCards(enchantment, _, _, card_type, false) => !self
                    .run
                    .deck
                    .iter()
                    .any(|card| can_enchant(content, card, *enchantment, *card_type)),
                Phase::ChooseCards(cards, _, false) => cards.is_empty(),
                Phase::ChooseBundles(bundles) => bundles.is_empty(),
                _ => false,
            };
            if !empty {
                return;
            }
            if self.paels_tooth && matches!(self.phase, Phase::RemoveCards(..)) {
                self.paels_cards
                    .sort_by_key(|card| content.cards[card.id as usize].id);
                self.paels_tooth = false;
            }
            self.finish_run_choice(content);
        }
    }

    fn continue_parasol(&mut self, content: &Content) {
        while !self.parasol.is_empty() {
            match self.parasol.remove(0) {
                ShopItem::Card(card, _) => self.add_card(content, card),
                ShopItem::Potion(id, _) => {
                    if !self.has_relic(content, "RELIC.SOZU")
                        && let Some(slot) = self.run.potions.iter_mut().find(|slot| slot.is_none())
                    {
                        *slot = Some(id);
                    }
                }
                ShopItem::Relic(id, _) => {
                    self.phase = Phase::Map;
                    self.obtain_relic(content, id);
                    if !matches!(self.phase, Phase::Map) {
                        return;
                    }
                }
                ShopItem::Remove(_) => {
                    if self
                        .run
                        .deck
                        .iter()
                        .any(|card| card.flags(content.cards[card.id as usize]) & ETERNAL == 0)
                    {
                        self.parasol_removal = true;
                        self.phase = Phase::RemoveCards(1, 0, false);
                    } else {
                        self.phase = Phase::Map;
                    }
                    return;
                }
            }
        }
        self.phase = Phase::Map;
    }

    fn roll_conveyor(&mut self) {
        self.event_data[1] += 1;
        if self.event_data[1] % 5 == 0 {
            self.event_data[0] = 8;
            self.event_data[2] = 8;
            return;
        }
        let mut dishes = vec![(1, 6.0), (2, 3.0), (3, 3.0), (4, 3.0)];
        if self.run.potions.iter().any(Option::is_none) {
            dishes.push((5, 3.0));
        }
        if self.run.hp != self.run.max_hp {
            dishes.push((6, 6.0));
        }
        if self.event_data[1] > 1 {
            dishes.push((7, 1.0));
        }
        dishes.retain(|(dish, _)| *dish != self.event_data[2]);
        let roll = self.event_rng.as_mut().unwrap().single()
            * dishes.iter().map(|(_, weight)| weight).sum::<f32>();
        let mut total = 0.0;
        for (dish, weight) in dishes {
            total += weight;
            if roll < total {
                self.event_data[0] = dish;
                self.event_data[2] = dish;
                break;
            }
        }
    }

    fn conveyor_phase(&self, id: Id) -> Phase {
        Phase::Event(
            id,
            vec![
                EventOption {
                    requirement: if self.event_data[0] == 7 {
                        Requirement::Always
                    } else {
                        Requirement::Gold(40)
                    },
                    effects: &[RunEffect::EventAction(0)],
                },
                EventOption {
                    requirement: Requirement::Always,
                    effects: &[RunEffect::EventAction(1)],
                },
            ],
        )
    }

    fn continue_conveyor(&mut self) {
        self.conveyor = false;
        self.roll_conveyor();
        let Some(Phase::Event(id, _)) = self.resume.take() else {
            self.phase = Phase::Map;
            return;
        };
        self.phase = self.conveyor_phase(id);
    }

    fn start_crystal(&mut self, remaining: u8) {
        let mut crystal = CrystalSphere {
            cells: vec![None; 121],
            clear: vec![false; 121],
            items: vec![],
            revealed: vec![],
            remaining,
            big: true,
        };
        for x in 0..11usize {
            for y in 0..11usize {
                crystal.clear[x * 11 + y] =
                    x + y <= 2 || 10 - x + y <= 2 || x + 10 - y <= 2 || 20 - x - y <= 2;
            }
        }
        for (kind, width, height) in [
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
        ] {
            let mut positions = Vec::new();
            for x in 0..=11 - width {
                for y in 0..=11 - height {
                    if (x..x + width).all(|x| {
                        (y..y + height).all(|y| {
                            !crystal.clear[x * 11 + y] && crystal.cells[x * 11 + y].is_none()
                        })
                    }) {
                        positions.push((x, y));
                    }
                }
            }
            if positions.is_empty() {
                break;
            }
            let position = positions[self
                .event_rng
                .as_mut()
                .unwrap()
                .below(positions.len() as u32) as usize];
            let item = crystal.items.len();
            crystal.items.push((
                position.0 as u8,
                position.1 as u8,
                width as u8,
                height as u8,
                kind,
            ));
            for x in position.0..position.0 + width {
                for y in position.1..position.1 + height {
                    crystal.cells[x * 11 + y] = Some(item);
                }
            }
        }
        self.crystal = Some(crystal);
    }

    fn clear_crystal(&mut self, content: &Content, x: u8, y: u8) {
        let mut curses = 0;
        let finished;
        {
            let crystal = self.crystal.as_mut().unwrap();
            crystal.remaining -= 1;
            let offsets: &[(i8, i8)] = if crystal.big {
                &[
                    (-1, 0),
                    (1, 0),
                    (0, -1),
                    (0, 1),
                    (-1, -1),
                    (-1, 1),
                    (1, -1),
                    (1, 1),
                    (0, 0),
                ]
            } else {
                &[(0, 0)]
            };
            for &(dx, dy) in offsets {
                let (x, y) = (x as i8 + dx, y as i8 + dy);
                if !(0..11).contains(&x) || !(0..11).contains(&y) {
                    continue;
                }
                let cell = x as usize * 11 + y as usize;
                if crystal.clear[cell] {
                    continue;
                }
                crystal.clear[cell] = true;
                let Some(item) = crystal.cells[cell] else {
                    continue;
                };
                let (ix, iy, width, height, kind) = crystal.items[item];
                if !crystal.revealed.contains(&item)
                    && (ix..ix + width).all(|x| {
                        (iy..iy + height).all(|y| crystal.clear[x as usize * 11 + y as usize])
                    })
                {
                    crystal.revealed.push(item);
                    curses += (kind == 6) as usize;
                }
            }
            finished = crystal.remaining == 0;
        }
        for _ in 0..curses {
            if let Some(id) = content.card_id("CARD.DOUBT") {
                self.add_card(
                    content,
                    Card {
                        id,
                        ..Card::default()
                    },
                );
            }
        }
        if finished {
            self.finish_crystal(content);
        }
    }

    fn finish_crystal(&mut self, content: &Content) {
        let crystal = self.crystal.take().unwrap();
        let mut rewards = Rewards {
            gold: 0,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        };
        self.reward_gold_parts.clear();
        for item in crystal.revealed {
            match crystal.items[item].4 {
                0 => {
                    let rarity = relic_rarity(self.event_rng.as_mut().unwrap().single());
                    rewards
                        .relics
                        .extend(self.pull_relic(false, rarity, false, false));
                }
                1 | 2 => {
                    let rare = crystal.items[item].4 == 2;
                    let pool: Vec<_> = self
                        .potion_pool(content)
                        .into_iter()
                        .filter(|potion| {
                            if rare {
                                RARE_POTIONS.contains(potion)
                            } else {
                                !RARE_POTIONS.contains(potion) && !UNCOMMON_POTIONS.contains(potion)
                            }
                        })
                        .collect();
                    if !pool.is_empty() {
                        let index = self.event_rng.as_mut().unwrap().below(pool.len() as u32);
                        rewards.potions.push(pool[index as usize]);
                    }
                }
                3..=5 => {
                    let rarity = [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare]
                        [(crystal.items[item].4 - 3) as usize];
                    if rewards.cards.is_empty() {
                        rewards.cards = self.make_card_reward(content, CardReward::Crystal(rarity));
                    } else {
                        rewards.card_rewards.push(CardReward::Crystal(rarity));
                    }
                }
                7 | 8 => {
                    let gold = if crystal.items[item].4 == 7 { 10 } else { 30 };
                    rewards.gold += gold;
                    self.reward_gold_parts.push(gold);
                }
                _ => {}
            }
        }
        if rewards.gold == 0
            && rewards.cards.is_empty()
            && rewards.relics.is_empty()
            && rewards.potions.is_empty()
        {
            self.phase = self.resume.take().unwrap_or(Phase::Map);
        } else {
            self.resume = Some(Phase::Map);
            self.phase = Phase::Rewards(rewards);
        }
    }

    fn crystal_cards(&mut self, content: &Content, rarity: CardRarity) -> Vec<Card> {
        let mut pool = content.characters[self.run.character as usize]
            .cards
            .to_vec();
        if self.has_relic(content, "RELIC.PRISMATIC_GEM") {
            pool = content
                .characters
                .iter()
                .flat_map(|character| character.cards)
                .copied()
                .collect();
        } else if self.has_relic(content, "RELIC.DINGY_RUG") {
            pool.extend_from_slice(content.colorless());
        }
        pool.retain(|id| content.cards[*id as usize].rarity == rarity);
        let mut cards = Vec::new();
        for _ in 0..3.min(pool.len()) {
            let index = self.event_rng.as_mut().unwrap().below(pool.len() as u32) as usize;
            let id = pool.remove(index);
            let upgrade = self.event_rng.as_mut().unwrap().single()
                <= (self.run.act - 1) as f32 * if self.run.ascension >= 7 { 0.125 } else { 0.25 };
            cards.push(Card {
                id,
                upgrades: (upgrade && rarity != CardRarity::Rare) as u8,
                ..Card::default()
            });
        }
        cards
    }

    fn event_action(&mut self, content: &Content, action: u8) -> bool {
        let id = match &self.phase {
            Phase::Event(id, _) => *id,
            _ => return false,
        };
        match content.events[id as usize].id {
            "EVENT.ABYSSAL_BATHS" => {
                let damage = 3 + self.event_data[0] as i16;
                self.run.max_hp += 2;
                self.run.hp = (self.run.hp + 2 - damage).max(0);
                self.event_data[0] += 1;
                self.phase = event_page(id, &[0, 9]);
                self.resume = None;
                true
            }
            "EVENT.BRAIN_LEECH" => {
                let pool = if action == 0 {
                    content.characters[self.run.character as usize].cards
                } else {
                    content.colorless()
                };
                let cards =
                    self.reward_cards(content, pool, if action == 0 { 5 } else { 3 }, Room::Combat);
                if action == 0 {
                    self.phase = Phase::ChooseCards(cards, 1, false);
                } else {
                    self.phase = Phase::Rewards(Rewards {
                        gold: 0,
                        cards,
                        card_rewards: vec![],
                        relics: vec![],
                        potions: vec![],
                        removals: 0,
                    });
                }
                true
            }
            "EVENT.COLORFUL_PHILOSOPHERS" => {
                let character = self.event_data[action as usize] as Id - 1;
                let cards = self
                    .make_card_reward(content, CardReward::Fixed(character, CardRarity::Common));
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards,
                    card_rewards: [CardRarity::Uncommon, CardRarity::Rare]
                        .into_iter()
                        .map(|rarity| CardReward::Fixed(character, rarity))
                        .collect(),
                    relics: vec![],
                    potions: vec![],
                    removals: 0,
                });
                true
            }
            "EVENT.CRYSTAL_SPHERE" => {
                if action == 0 {
                    self.run.gold -= self.event_data[0] as i32;
                } else if let Some(debt) = content.card_id("CARD.DEBT") {
                    self.add_card(
                        content,
                        Card {
                            id: debt,
                            ..Card::default()
                        },
                    );
                }
                self.start_crystal(if action == 0 { 3 } else { 6 });
                self.phase = Phase::Event(id, vec![]);
                true
            }
            "EVENT.ENDLESS_CONVEYOR" => {
                if action == 1 {
                    if self.event_data[1] == 1 {
                        let cards: Vec<_> = self
                            .run
                            .deck
                            .iter()
                            .enumerate()
                            .filter(|(_, card)| {
                                card.upgrades == 0
                                    && !matches!(
                                        content.cards[card.id as usize].card_type,
                                        CardType::Status | CardType::Curse | CardType::Quest
                                    )
                            })
                            .map(|(index, _)| index)
                            .collect();
                        if !cards.is_empty() {
                            let choice = self.event_rng.as_mut().unwrap().below(cards.len() as u32);
                            self.run.deck[cards[choice as usize]].upgrades = 1;
                        }
                    }
                    return false;
                }
                let dish = self.event_data[0];
                if dish != 7 {
                    self.run.gold -= 40;
                }
                match dish {
                    1 => {
                        self.run.max_hp += 4;
                        self.run.hp += 4;
                    }
                    2 => {
                        let cards: Vec<_> = self
                            .run
                            .deck
                            .iter()
                            .enumerate()
                            .filter(|(_, card)| {
                                card.upgrades == 0
                                    && !matches!(
                                        content.cards[card.id as usize].card_type,
                                        CardType::Status | CardType::Curse | CardType::Quest
                                    )
                            })
                            .map(|(index, _)| index)
                            .collect();
                        if !cards.is_empty() {
                            let choice = self.event_rng.as_mut().unwrap().below(cards.len() as u32);
                            self.run.deck[cards[choice as usize]].upgrades = 1;
                        }
                    }
                    3 => {
                        self.conveyor = true;
                        self.resume = Some(self.conveyor_phase(id));
                        self.phase = Phase::TransformCards(None, 1, false);
                        return true;
                    }
                    4 => {
                        let cards =
                            self.reward_cards(content, content.colorless(), 1, Room::Combat);
                        if let Some(card) = cards.into_iter().next() {
                            self.add_card(content, card);
                        }
                    }
                    5 => {
                        let pool = self.potion_pool(content);
                        let potion = (!pool.is_empty())
                            .then(|| pool[self.rngs.rewards.below(pool.len() as u32) as usize]);
                        self.conveyor = true;
                        self.resume = Some(self.conveyor_phase(id));
                        self.phase = Phase::Rewards(Rewards {
                            gold: 0,
                            cards: vec![],
                            card_rewards: vec![],
                            relics: vec![],
                            potions: potion.into_iter().collect(),
                            removals: 0,
                        });
                        return true;
                    }
                    6 => self.run.hp = (self.run.hp + 10).min(self.run.max_hp),
                    7 => self.gain_gold(content, 75),
                    8 => {
                        if let Some(id) = content.card_id("CARD.FEEDING_FRENZY") {
                            self.add_card(
                                content,
                                Card {
                                    id,
                                    ..Card::default()
                                },
                            );
                        }
                    }
                    _ => {}
                }
                self.roll_conveyor();
                self.phase = self.conveyor_phase(id);
                self.resume = None;
                true
            }
            "EVENT.INFESTED_AUTOMATON" => {
                let base = content.characters[self.run.character as usize].cards;
                let mut pool = if action == 0 && self.has_relic(content, "RELIC.PRISMATIC_GEM") {
                    content
                        .characters
                        .iter()
                        .flat_map(|character| character.cards)
                        .copied()
                        .collect()
                } else {
                    base.to_vec()
                };
                if action == 0 && self.has_relic(content, "RELIC.DINGY_RUG") {
                    pool.extend_from_slice(content.colorless());
                }
                pool.retain(|id| {
                    let card = content.cards[*id as usize];
                    if action == 0 {
                        card.card_type == CardType::Power
                    } else {
                        card.cost[0] == 0
                    }
                });
                let mut cards = self.card_rewards(content, &pool, 1, Room::Combat);
                self.modify_reward_cards(content, &mut cards);
                if let Some(card) = cards.pop() {
                    self.add_card(content, card);
                }
                false
            }
            "EVENT.JUNGLE_MAZE_ADVENTURE" => {
                if action == 0 {
                    self.run.hp = (self.run.hp - 18).max(0);
                }
                self.gain_gold(content, self.event_data[action as usize] as i32);
                false
            }
            "EVENT.LUMINOUS_CHOIR" if action == 1 => {
                self.run.gold -= self.event_data[0] as i32;
                let rarity = self.roll_relic_rarity();
                if let Some(relic) = self.pull_relic(false, rarity, false, false) {
                    self.obtain_relic(content, relic);
                }
                !matches!(self.phase, Phase::Event(..))
            }
            "EVENT.RANWID_THE_ELDER" => {
                match action {
                    0 => self.run.potions[self.event_data[0] as usize - 1] = None,
                    1 => self.run.gold -= 100,
                    2 => {
                        if let Some(index) = self.event_cards.first().copied() {
                            self.remove_relic_at(content, index as usize);
                        }
                    }
                    _ => return false,
                }
                for _ in 0..if action == 2 { 2 } else { 1 } {
                    let rarity = self.roll_relic_rarity();
                    if let Some(relic) = self.pull_relic(false, rarity, false, false) {
                        self.obtain_relic(content, relic);
                    }
                }
                !matches!(self.phase, Phase::Event(..))
            }
            "EVENT.RELIC_TRADER" => {
                let trade = (
                    self.event_cards.get(action as usize).copied(),
                    self.relic_queue.get(action as usize).copied(),
                );
                self.relic_queue.clear();
                if let (Some(index), Some(relic)) = trade {
                    self.remove_relic_at(content, index as usize);
                    self.obtain_relic(content, relic);
                }
                !matches!(self.phase, Phase::Event(..))
            }
            "EVENT.WHISPERING_HOLLOW" if action == 0 => {
                self.run.gold -= self.event_data[0] as i32;
                let pool = self.potion_pool(content);
                let mut potions = self.reward_potions(&pool, 1);
                potions.extend(self.reward_potions(&pool, 1));
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics: vec![],
                    potions,
                    removals: 0,
                });
                true
            }
            "EVENT.THE_LEGENDS_WERE_TRUE" if action == 1 => {
                self.run.hp = (self.run.hp - 8).max(0);
                let pool = self.potion_pool(content);
                let potion = (!pool.is_empty())
                    .then(|| pool[self.rngs.rewards.below(pool.len() as u32) as usize]);
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics: vec![],
                    potions: potion.into_iter().collect(),
                    removals: 0,
                });
                true
            }
            "EVENT.WELLSPRING" if action == 0 => {
                let pool = self.potion_pool(content);
                let potion = (!pool.is_empty())
                    .then(|| pool[self.rngs.rewards.below(pool.len() as u32) as usize]);
                self.phase = Phase::Rewards(Rewards {
                    gold: 0,
                    cards: vec![],
                    card_rewards: vec![],
                    relics: vec![],
                    potions: potion.into_iter().collect(),
                    removals: 0,
                });
                true
            }
            "EVENT.DOLL_ROOM" if action <= 2 => {
                let relics: Vec<_> = [
                    "RELIC.DAUGHTER_OF_THE_WIND",
                    "RELIC.MR_STRUGGLES",
                    "RELIC.BING_BONG",
                ]
                .into_iter()
                .filter_map(|relic| content.relic_id(relic))
                .collect();
                if action == 0 {
                    if !relics.is_empty() {
                        let index = self.event_rng.as_mut().unwrap().below(relics.len() as u32);
                        self.obtain_relic(content, relics[index as usize]);
                    }
                    return false;
                }
                self.run.hp = (self.run.hp - if action == 1 { 5 } else { 15 }).max(0);
                let mut relics = relics;
                self.event_rng.as_mut().unwrap().shuffle(&mut relics);
                relics.truncate(action as usize + 1);
                for (slot, relic) in relics.into_iter().enumerate() {
                    self.event_data[slot] = relic as i64;
                }
                self.phase = event_page(
                    id,
                    if action == 1 {
                        &[10, 11]
                    } else {
                        &[10, 11, 12]
                    },
                );
                self.resume = None;
                true
            }
            "EVENT.DOLL_ROOM" => {
                self.obtain_relic(content, self.event_data[(action - 10) as usize] as Id);
                false
            }
            "EVENT.SLIPPERY_BRIDGE" if action == 0 => {
                let instance = self.event_data[1] as u32;
                if let Some(index) = self
                    .run
                    .deck
                    .iter()
                    .position(|card| card.instance == instance)
                {
                    self.run.deck.remove(index);
                }
                false
            }
            "EVENT.SLIPPERY_BRIDGE" => {
                let damage = 3 + self.event_data[0] as i16;
                self.run.hp = (self.run.hp - damage).max(0);
                self.event_data[0] += 1;
                let previous = self
                    .run
                    .deck
                    .iter()
                    .find(|card| card.instance == self.event_data[1] as u32)
                    .map(|card| card.id);
                self.event_cards.push(self.event_data[1] as u32);
                self.pick_slippery_card(content, previous);
                self.phase = event_page(id, &[0, 1]);
                self.resume = None;
                true
            }
            "EVENT.STONE_OF_ALL_TIME" => {
                let slots: Vec<_> = self
                    .run
                    .potions
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, potion)| potion.map(|_| slot))
                    .collect();
                if !slots.is_empty() {
                    let choice = self.event_rng.as_mut().unwrap().below(slots.len() as u32);
                    self.run.potions[slots[choice as usize]] = None;
                }
                self.run.max_hp += 10;
                self.run.hp += 10;
                self.event_rng.as_mut().unwrap().below(100);
                false
            }
            "EVENT.THE_FUTURE_OF_POTIONS" => {
                let slots: Vec<_> = self
                    .run
                    .potions
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, potion)| potion.map(|potion| (slot, potion)))
                    .collect();
                let Some(&(slot, potion)) = slots.get(action as usize) else {
                    return false;
                };
                self.run.potions[slot] = None;
                let rarity = if RARE_POTIONS.contains(&potion) {
                    CardRarity::Rare
                } else if UNCOMMON_POTIONS.contains(&potion) {
                    CardRarity::Uncommon
                } else {
                    CardRarity::Common
                };
                self.event_rng.as_mut().unwrap().forward(action as u64);
                let types = if matches!(rarity, CardRarity::Rare | CardRarity::Uncommon) {
                    3
                } else {
                    2
                };
                let card_type = match self.event_rng.as_mut().unwrap().below(types) {
                    0 => CardType::Attack,
                    1 => CardType::Skill,
                    _ => CardType::Power,
                };
                let mut pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|id| {
                        let card = content.cards[*id as usize];
                        card.rarity == rarity && card.card_type == card_type
                    })
                    .collect();
                let mut cards = Vec::new();
                for _ in 0..3.min(pool.len()) {
                    let index = self.rngs.rewards.below(pool.len() as u32) as usize;
                    cards.push(Card {
                        id: pool.remove(index),
                        upgrades: 1,
                        ..Card::default()
                    });
                }
                self.phase = Phase::ChooseCards(cards, 1, true);
                true
            }
            "EVENT.TRIAL" => match action {
                0 => {
                    let roll = self.event_rng.as_mut().unwrap().below(3);
                    self.phase = event_page(id, &[2 + roll as u8 * 2, 3 + roll as u8 * 2]);
                    self.resume = None;
                    true
                }
                1 => {
                    self.phase = event_page(id, &[0, 8]);
                    self.resume = None;
                    true
                }
                2 => {
                    self.add_named_card(content, "CARD.REGRET");
                    for _ in 0..2 {
                        let rarity = self.roll_relic_rarity();
                        if let Some(relic) = self.pull_relic(false, rarity, false, false) {
                            self.obtain_relic(content, relic);
                        }
                    }
                    false
                }
                3 => {
                    self.add_named_card(content, "CARD.SHAME");
                    self.phase = Phase::UpgradeCards(2, false);
                    true
                }
                4 => {
                    self.run.hp = (self.run.hp + 10).min(self.run.max_hp);
                    false
                }
                5 => {
                    self.add_named_card(content, "CARD.REGRET");
                    self.gain_gold(content, 300);
                    false
                }
                6 => {
                    self.add_named_card(content, "CARD.DOUBT");
                    self.run_queue.extend([
                        RunEffect::ChooseRewardCards(3, 1),
                        RunEffect::ChooseRewardCards(3, 1),
                    ]);
                    false
                }
                7 => {
                    self.add_named_card(content, "CARD.DOUBT");
                    self.phase = Phase::TransformCards(None, 2, false);
                    true
                }
                _ => {
                    self.phase = Phase::Dead;
                    self.resume = None;
                    true
                }
            },
            _ => false,
        }
    }

    fn add_named_card(&mut self, content: &Content, id: &str) {
        if let Some(id) = content.card_id(id) {
            self.add_card(
                content,
                Card {
                    id,
                    ..Card::default()
                },
            );
        }
    }

    fn pick_slippery_card(&mut self, content: &Content, previous: Option<Id>) {
        let removable = |card: &&Card| {
            card.flags(content.cards[card.id as usize]) & ETERNAL == 0
                && !self.event_cards.contains(&card.instance)
        };
        let mut cards: Vec<_> = self
            .run
            .deck
            .iter()
            .filter(removable)
            .filter(|card| {
                previous.map_or(
                    content.cards[card.id as usize].rarity != CardRarity::Basic,
                    |id| card.id != id,
                )
            })
            .map(|card| card.instance)
            .collect();
        if cards.is_empty() {
            cards = self
                .run
                .deck
                .iter()
                .filter(|card| card.flags(content.cards[card.id as usize]) & ETERNAL == 0)
                .map(|card| card.instance)
                .collect();
        }
        if !cards.is_empty() {
            let index = self.event_rng.as_mut().unwrap().below(cards.len() as u32);
            self.event_data[1] = cards[index as usize] as i64;
        }
    }

    fn leave_room(&mut self, content: &Content) {
        if self.room == Room::Boss {
            let next = match self.run.act {
                1 => content.acts.iter().position(|act| act.id == "ACT.THE_HIVE"),
                2 => content
                    .acts
                    .iter()
                    .position(|act| act.id == "ACT.THE_GLORY"),
                _ => None,
            };
            if let Some(next) = next {
                self.begin_act(content, next as Id).unwrap();
            } else {
                self.phase = Phase::Won;
            }
        } else {
            self.phase = Phase::Map;
        }
    }

    fn rest_destination(&mut self, content: &Content, option: u8) -> Phase {
        self.rest_used |= option;
        if self.has_relic(content, "RELIC.MINIATURE_TENT") {
            Phase::Rest
        } else {
            Phase::Map
        }
    }

    fn rest_heal(&mut self, content: &Content) -> Rewards {
        let heal = self.run.max_hp * 3 / 10
            + if self.has_relic(content, "RELIC.REGAL_PILLOW") {
                15
            } else {
                0
            };
        self.run.hp = (self.run.hp + heal).min(self.run.max_hp);
        if self.has_relic(content, "RELIC.STONE_HUMIDIFIER") {
            self.run.max_hp += 5;
            self.run.hp += 5;
        }
        let mut rewards = Rewards {
            gold: 0,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        };
        for (index, relic) in self.run.relics.clone().into_iter().enumerate() {
            if self.melted_relics.contains(&index) {
                continue;
            }
            match content.relics[relic as usize].id {
                "RELIC.DREAM_CATCHER" => {
                    let pool = content.characters[self.run.character as usize].cards;
                    rewards.cards = self.reward_cards(content, pool, 3, Room::Combat);
                }
                "RELIC.TINY_MAILBOX" => {
                    let pool = self.potion_pool(content);
                    rewards.potions.extend(self.reward_potions(&pool, 1));
                    rewards.potions.extend(self.reward_potions(&pool, 1));
                }
                _ => {}
            }
        }
        rewards
    }

    pub(crate) fn next_encounter(&mut self, content: &Content, elite: bool) -> Option<Id> {
        if elite {
            let candidates = content.acts[self.act as usize].elites;
            if self.elite_encounters_left == 0 {
                return self.pick(candidates);
            }
            let id = Self::pull_encounter(
                &mut self.rngs.up_front,
                content,
                candidates,
                &mut self.elites,
                self.last_elite,
            )?;
            self.elite_encounters_left -= 1;
            self.last_elite = Some(id);
            if self.elite_encounters_left == 0 {
                self.elites.clear();
                self.last_elite = None;
            }
            return Some(id);
        }
        let candidates = content.acts[self.act as usize].encounters;
        let weak = candidates
            .iter()
            .copied()
            .filter(|id| content.encounters[*id as usize].id.contains("_WEAK"))
            .collect::<Vec<_>>();
        let regular = candidates
            .iter()
            .copied()
            .filter(|id| !content.encounters[*id as usize].id.contains("_WEAK"))
            .collect::<Vec<_>>();
        let (candidates, left, weak) = if self.weak_encounters_left > 0 {
            (&weak, &mut self.weak_encounters_left, true)
        } else if self.regular_encounters_left > 0 {
            (&regular, &mut self.regular_encounters_left, false)
        } else {
            return self.pick(candidates);
        };
        let id = Self::pull_encounter(
            &mut self.rngs.up_front,
            content,
            candidates,
            &mut self.encounters,
            self.last_encounter,
        )?;
        *left -= 1;
        self.last_encounter = Some(id);
        if *left == 0 {
            self.encounters.clear();
            if !weak || self.regular_encounters_left == 0 {
                self.last_encounter = None;
            }
        }
        Some(id)
    }

    fn pull_encounter(
        rng: &mut Rng,
        content: &Content,
        candidates: &[Id],
        bag: &mut Vec<Id>,
        previous: Option<Id>,
    ) -> Option<Id> {
        if bag.is_empty() {
            bag.extend_from_slice(candidates);
        }
        bag.sort_unstable();
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
        Some(bag.remove(index))
    }

    fn pick(&mut self, values: &[Id]) -> Option<Id> {
        (!values.is_empty()).then(|| values[self.rngs.up_front.below(values.len() as u32) as usize])
    }

    fn roll_relic_rarity(&mut self) -> usize {
        relic_rarity(self.rngs.rewards.single())
    }

    fn pull_relic(&mut self, shared: bool, rarity: usize, back: bool, shop: bool) -> Option<Id> {
        let deques = if shared {
            &mut self.shared_relic_deques
        } else {
            &mut self.relic_deques
        };
        let mut rarity = rarity;
        let id = loop {
            let index = if back {
                deques[rarity]
                    .iter()
                    .rposition(|id| !shop || relic_allowed_in_shop(*id))
            } else {
                deques[rarity]
                    .iter()
                    .position(|id| !shop || relic_allowed_in_shop(*id))
            };
            if let Some(index) = index {
                break deques[rarity].remove(index);
            }
            rarity = match rarity {
                3 => 0,
                0 => 1,
                1 => 2,
                _ => return None,
            };
        };
        self.remove_relic_from_bags(id);
        Some(id)
    }

    fn remove_relic_from_bags(&mut self, id: Id) {
        for deque in self
            .relic_deques
            .iter_mut()
            .chain(self.shared_relic_deques.iter_mut())
        {
            deque.retain(|relic| *relic != id);
        }
    }

    fn potion_pool(&self, content: &Content) -> Vec<Id> {
        let mut potions = content.characters[self.run.character as usize]
            .potion_pool
            .to_vec();
        potions.sort_unstable_by_key(|&id| content.potions[id as usize].id);
        let mut shared = content.acts[self.act as usize].potions.to_vec();
        shared.sort_unstable_by_key(|&id| content.potions[id as usize].id);
        potions.extend(shared);
        potions
    }

    fn random_potions(&mut self, pool: &[Id], count: usize, combat: bool, unique: bool) -> Vec<Id> {
        let mut chosen = Vec::new();
        for _ in 0..count {
            let roll = self.rngs.combat_potion_generation.single();
            let rarity = if roll < 0.1 {
                2
            } else if roll < 0.35 {
                1
            } else {
                0
            };
            let mut options: Vec<_> = pool
                .iter()
                .copied()
                .filter(|id| {
                    (!unique || !chosen.contains(id))
                        && (!combat || !NO_COMBAT_POTIONS.contains(id))
                        && match rarity {
                            2 => RARE_POTIONS.contains(id),
                            1 => UNCOMMON_POTIONS.contains(id),
                            _ => !RARE_POTIONS.contains(id) && !UNCOMMON_POTIONS.contains(id),
                        }
                })
                .collect();
            if options.is_empty() {
                options = pool
                    .iter()
                    .copied()
                    .filter(|id| {
                        (!unique || !chosen.contains(id))
                            && (!combat || !NO_COMBAT_POTIONS.contains(id))
                    })
                    .collect();
            }
            if options.is_empty() {
                break;
            }
            chosen.push(
                options[self
                    .rngs
                    .combat_potion_generation
                    .below(options.len() as u32) as usize],
            );
        }
        chosen
    }

    fn face_target(&mut self, target: Option<usize>) {
        let Some(target) = target else { return };
        let enemy = &self.combat().unwrap().enemies[target].creature;
        let left = enemy.power(power_id::BACK_ATTACK_LEFT) > 0;
        let right = enemy.power(power_id::BACK_ATTACK_RIGHT) > 0;
        if let Some(surrounded) = self
            .creature_mut(Actor::Player)
            .powers
            .iter_mut()
            .find(|power| power.id == power_id::SURROUNDED)
        {
            if surrounded.value == 0 && left {
                surrounded.value = 1;
            } else if surrounded.value == 1 && right {
                surrounded.value = 0;
            }
        }
    }

    fn play(&mut self, content: &Content, hand: usize, target: Option<usize>) {
        let spiked_gauntlets = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
        let chemical_x = self.has_relic(content, "RELIC.CHEMICAL_X");
        let intimidating_helmet = self.has_relic(content, "RELIC.INTIMIDATING_HELMET");
        let brilliant_scarf = self.has_relic(content, "RELIC.BRILLIANT_SCARF")
            && self.combat().unwrap().history.manual_plays == 4;
        let throwing_axe =
            self.has_relic(content, "RELIC.THROWING_AXE") && !self.combat().unwrap().throwing_axe;
        let pen_nib = self.has_relic(content, "RELIC.PEN_NIB")
            && self.combat().unwrap().hand[hand]
                .card_type(content.cards[self.combat().unwrap().hand[hand].id as usize])
                == CardType::Attack;
        let mut pen_nib_counter = self.pen_nib;
        self.face_target(target);
        let combat = self.combat_mut().unwrap();
        combat.throwing_axe |= throwing_axe;
        combat.power_snapshot = combat.player.powers.clone();
        combat.enemy_power_snapshot = combat
            .enemies
            .iter()
            .map(|enemy| enemy.creature.powers.clone())
            .collect();
        let mut card = combat.hand.remove(hand);
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        if matches!(card_type, CardType::Attack | CardType::Skill)
            && combat.history.attacks + combat.history.skills
                < combat.player.power(power_id::NOSTALGIA)
        {
            card.flags |= RETURN_TO_DRAW;
        }
        let fetch =
            card.id == card_id::FETCH && card.turn_flags & FETCHED == 0 && combat.osty.hp > 0;
        if card.id == card_id::FETCH {
            card.turn_flags |= FETCHED;
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
        let veil =
            card.flags(def) & ETHEREAL != 0 && combat.player.power(power_id::VEILPIERCER) > 0;
        let free_attack =
            card_type == CardType::Attack && combat.player.power(power_id::FREE_ATTACK) > 0;
        let cost = if brilliant_scarf {
            0
        } else {
            energy_cost(combat, card, def, spiked_gauntlets)
        };
        if let Some(power) = free {
            combat.player.consume_power(power);
        }
        if veil {
            combat.player.consume_power(power_id::VEILPIERCER);
        }
        if free_attack {
            combat.player.consume_power(power_id::FREE_ATTACK);
        }
        let danse = if cost >= 2 {
            combat.player.power(power_id::DANSE_MACABRE)
        } else {
            0
        };
        let ethereal = card.flags(def);
        let ash = if ethereal & ETHEREAL != 0 {
            combat.player.power(power_id::SPIRIT_OF_ASH)
        } else {
            0
        };
        let x_cost = def.cost[card.upgrades.min(1) as usize] < 0;
        let x = if !void_free && x_cost {
            combat.energy
        } else {
            0
        } + 2 * (x_cost && chemical_x) as i16;
        combat.energy -= cost;
        trigger_orbit(combat, cost);
        let throne = combat.player.power(power_id::THE_SEALED_THRONE);
        if throne > 0 {
            combat.stars = combat.stars.saturating_add(throne);
            combat.history.stars_gained = combat.history.stars_gained.saturating_add(throne);
        }
        let stars = if brilliant_scarf {
            0
        } else {
            star_cost(combat, card, def)
        };
        combat.stars -= stars;
        combat.card_energy = cost;
        combat.card_stars = stars;
        let burst = card_type == CardType::Skill && combat.player.power(power_id::BURST) > 0;
        let punch =
            card_type == CardType::Attack && combat.player.power(power_id::ONE_TWO_PUNCH) > 0;
        if burst {
            combat.player.consume_power(power_id::BURST);
        }
        let duplicate = (combat.player.power(power_id::DUPLICATION) > 0) as u8;
        if duplicate > 0 {
            combat.player.consume_power(power_id::DUPLICATION);
        }
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
        if combat.player.power(power_id::SIGNAL_BOOST) > 0 {
            combat.player.consume_power(power_id::SIGNAL_BOOST);
        }
        if punch {
            combat.player.consume_power(power_id::ONE_TWO_PUNCH);
        }
        combat.history.energy += cost;
        combat.card_plays = plays;
        combat.history.cards += plays as i16;
        combat.history.manual_cards += 1;
        combat.history.manual_plays += plays as i16;
        match card_type {
            CardType::Attack => combat.history.attacks += plays as i16,
            CardType::Skill => combat.history.skills += plays as i16,
            CardType::Power => combat.history.powers += plays as i16,
            _ => {}
        }
        let context = Context {
            source: Actor::Player,
            target,
            card: Some(card.id),
            upgraded: card.upgrades > 0,
            x,
            event: stars,
            orb: false,
            orb_id: None,
            pen_nib: false,
        };
        let fan_shiv = def.id == "CARD.SHIV" && combat.player.power(power_id::FAN_OF_KNIVES) > 0;
        if card.enchantment == Some(Enchantment::Sown) && card.enchantment_value > 0 {
            combat.energy = combat.energy.saturating_add(card.enchantment_amount);
            combat.playing.as_mut().unwrap().enchantment_value = 0;
        }
        if fetch {
            combat.queue.push(Pending {
                effect: Effect::Draw(1),
                context,
            });
        }
        let mut pen_nibs = vec![false; plays as usize];
        for doubled in &mut pen_nibs {
            if pen_nib {
                pen_nib_counter = (pen_nib_counter + 1) % 10;
                *doubled = pen_nib_counter == 0;
            }
        }
        for play in (0..plays).rev() {
            let context = Context {
                pen_nib: pen_nibs[play as usize],
                ..context
            };
            if card.flags(def) & INKY != 0 {
                combat.queue.push(Pending {
                    effect: Effect::ApplyPower(
                        Target::ChosenEnemy,
                        power_id::WEAK,
                        Amount::fixed(1, 1),
                    ),
                    context,
                });
            }
            if fan_shiv {
                combat.queue.push(Pending {
                    effect: Effect::Attack(Target::AllEnemies, Amount::fixed(4, 6), 1),
                    context,
                });
            } else {
                push_card_effects(&mut combat.queue, card, def, context);
            }
        }
        if cost >= 2 && intimidating_helmet {
            for _ in 0..plays {
                combat.queue.push(Pending {
                    effect: Effect::RawBlock(Target::Player, Amount::fixed(4, 4)),
                    context,
                });
            }
        }
        match card.enchantment {
            Some(Enchantment::Adroit) => combat.queue.push(Pending {
                effect: Effect::Block(
                    Target::Player,
                    Amount::fixed(card.enchantment_amount, card.enchantment_amount),
                ),
                context,
            }),
            Some(Enchantment::Corrupted) => combat.queue.push(Pending {
                effect: Effect::LoseHp(Target::Player, Amount::fixed(2, 2)),
                context,
            }),
            Some(Enchantment::Swift) if card.enchantment_value > 0 => {
                combat.playing.as_mut().unwrap().enchantment_value = 0;
                combat.queue.push(Pending {
                    effect: Effect::Draw(card.enchantment_amount.max(0) as u8),
                    context,
                });
            }
            _ => {}
        }
        let block = combat
            .player
            .power(power_id::CHILD_OF_THE_STARS)
            .saturating_mul(stars);
        if block > 0 {
            combat.queue.push(Pending {
                effect: Effect::RawBlock(Target::Player, Amount::fixed(block, block)),
                context,
            });
        }
        let black_hole = combat.player.power(power_id::BLACK_HOLE);
        if throne > 0 && black_hole > 0 {
            combat.queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(black_hole, black_hole)),
                context,
            });
        }
        if !self.replaying {
            self.queue_bombardments();
        }
        if danse > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Block(Target::Player, Amount::fixed(danse, danse)),
                context,
            });
        }
        if ash > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::RawBlock(Target::Player, Amount::fixed(ash, ash)),
                context,
            });
        }
        self.pen_nib = pen_nib_counter;
    }

    fn finish_play(&mut self, content: &Content) {
        let (mut card, card_energy, card_stars) = {
            let combat = self.combat_mut().unwrap();
            if combat.choice.is_some() || !combat.queue.is_empty() || combat.playing.is_none() {
                return;
            }
            (
                combat.playing.take().unwrap(),
                combat.card_energy,
                combat.card_stars,
            )
        };
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        match card.enchantment {
            Some(Enchantment::Glam | Enchantment::Vigorous) => card.enchantment_value = 0,
            Some(Enchantment::Goopy) => {
                card.enchantment_amount = card.enchantment_amount.saturating_add(1);
                card.enchantment_value = card.enchantment_amount;
                if let Some(deck) = self
                    .run
                    .deck
                    .iter_mut()
                    .find(|deck| deck.instance == card.instance)
                {
                    deck.enchantment_amount = card.enchantment_amount;
                }
            }
            _ => {}
        }
        let feral = card_type == CardType::Attack && card_energy == 0;
        if card.id == card_id::MAUL {
            let increase = 1 + card.upgrades.min(1) as i16;
            card.value += increase;
            let combat = self.combat_mut().unwrap();
            for other in combat
                .draw
                .iter_mut()
                .chain(&mut combat.hand)
                .chain(&mut combat.discard)
                .chain(&mut combat.exhaust)
            {
                if other.id == card_id::MAUL {
                    other.value += increase;
                }
            }
            for other in &mut self.run.deck {
                if other.id == card_id::MAUL {
                    other.value += increase;
                }
            }
        }
        if card.upgrades == 0
            && matches!(card_type, CardType::Attack | CardType::Skill)
            && self.has_relic(content, "RELIC.RAZOR_TOOTH")
        {
            card.upgrades = 1;
        }
        let (card, exhausted) = self.finish_card_destination(content, card, false, feral);
        if matches!(card_type, CardType::Attack | CardType::Skill) && card.flags & DUPE == 0 {
            self.combat_mut().unwrap().history_course = Some(card);
        }
        self.combat_mut().unwrap().pen_nib = false;
        if self.combat().unwrap().unsettling_lamp == Some(card.id) {
            self.combat_mut().unwrap().unsettling_lamp = None;
            self.combat_mut().unwrap().unsettling_used = true;
        }
        self.unceasing_top(content);
        if card_type == CardType::Attack
            && self.has_relic(content, "RELIC.MUSIC_BOX")
            && !self.combat().unwrap().music_box
        {
            let mut copy = card;
            copy.instance = 0;
            copy.flags |= ETHEREAL;
            self.combat_mut().unwrap().music_box = true;
            self.add_generated(content, Pile::Hand, copy);
        }
        if card_type == CardType::Power
            && self.has_relic(content, "RELIC.PERMAFROST")
            && !self.combat().unwrap().permafrost
        {
            self.combat_mut().unwrap().permafrost = true;
            self.gain_block(content, Actor::Player, 7, None, false);
        }
        if card_type == CardType::Power && self.has_relic(content, "RELIC.MUMMIFIED_HAND") {
            for _ in 0..self.combat().unwrap().card_plays {
                self.mummified_hand(content);
            }
        }
        if self.combat().unwrap().paels_legion == 0
            && self.has_relic(content, "RELIC.PAELS_LEGION")
            && self.combat().unwrap().history.block_card == self.combat().unwrap().history.cards
        {
            self.combat_mut().unwrap().paels_legion = 2;
        }
        if exhausted {
            self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
            self.trigger_card(content, card, Trigger::CardExhausted);
        }
        let combat = self.combat().unwrap();
        let old_attacks = combat.history.attacks - combat.card_plays as i16;
        let old_skills = combat.history.skills - combat.card_plays as i16;
        if card_type == CardType::Attack
            && old_attacks < 3
            && self.combat().unwrap().history.attacks >= 3
        {
            let copies = self
                .creature(Actor::Player)
                .power(power_id::JUGGLING)
                .max(0) as u8;
            for _ in 0..copies {
                let mut copy = card;
                copy.instance = 0;
                self.add_generated(content, Pile::Hand, copy);
            }
        }
        let fan = card_type == CardType::Attack && self.has_relic(content, "RELIC.ORNAMENTAL_FAN");
        for _ in 0..if fan {
            self.combat().unwrap().history.attacks / 3 - old_attacks / 3
        } else {
            0
        } {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::RawBlock(Target::Player, Amount::fixed(4, 4)),
                context: Context::player(),
            });
        }
        if card_type == CardType::Attack {
            let triggers = self.combat().unwrap().history.attacks / 3 - old_attacks / 3;
            for _ in 0..triggers {
                if self.has_relic(content, "RELIC.KUNAI") {
                    self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
                }
                if self.has_relic(content, "RELIC.SHURIKEN") {
                    self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
                }
            }
            if self.has_relic(content, "RELIC.NUNCHAKU") {
                let attacks = self.nunchaku as i16 + self.combat().unwrap().card_plays as i16;
                self.combat_mut().unwrap().energy += attacks / 10;
                self.nunchaku = (attacks % 10) as u8;
            }
            if self.has_relic(content, "RELIC.KUSARIGAMA") {
                let attacks = self.combat().unwrap().kusarigama as i16
                    + self.combat().unwrap().card_plays as i16;
                self.combat_mut().unwrap().kusarigama = (attacks % 3) as u8;
                for _ in 0..attacks / 3 {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::Damage(Target::RandomEnemy, Amount::fixed(6, 6)),
                        context: Context::player(),
                    });
                }
            }
        }
        if card_type == CardType::Skill && self.has_relic(content, "RELIC.LETTER_OPENER") {
            for _ in 0..self.combat().unwrap().history.skills / 3 - old_skills / 3 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Damage(Target::AllEnemies, Amount::fixed(5, 5)),
                    context: Context::player(),
                });
            }
        }
        if self.has_relic(content, "RELIC.IRON_CLUB") {
            let cards = self.iron_club as i16 + self.combat().unwrap().card_plays as i16;
            self.iron_club = (cards % 4) as u8;
            for _ in 0..cards / 4 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Draw(1),
                    context: Context::player(),
                });
            }
        }
        if card_type == CardType::Skill && self.has_relic(content, "RELIC.TUNING_FORK") {
            let skills = self.tuning_fork as i16 + self.combat().unwrap().card_plays as i16;
            self.tuning_fork = (skills % 10) as u8;
            for _ in 0..skills / 10 {
                self.gain_block(content, Actor::Player, 7, None, false);
            }
        }
        if !self.combat().unwrap().rainbow_ring
            && self.has_relic(content, "RELIC.RAINBOW_RING")
            && self.combat().unwrap().history.attacks > 0
            && self.combat().unwrap().history.skills > 0
            && self.combat().unwrap().history.powers > 0
        {
            self.combat_mut().unwrap().rainbow_ring = true;
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
            self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
        }
        if card_stars > 0
            && self.has_relic(content, "RELIC.MINI_REGENT")
            && !self.combat().unwrap().mini_regent
        {
            self.combat_mut().unwrap().mini_regent = true;
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
        }
        if card_stars > 0 && self.has_relic(content, "RELIC.GALACTIC_DUST") {
            let stars = self.galactic_dust as i16 + card_stars;
            self.galactic_dust = (stars % 10) as u8;
            for _ in 0..stars / 10 {
                self.gain_block(content, Actor::Player, 10, None, false);
            }
        }
        if card_energy >= 3 && self.has_relic(content, "RELIC.IVORY_TILE") {
            self.combat_mut().unwrap().energy += self.combat().unwrap().card_plays as i16;
        }
        let powers = std::mem::take(&mut self.combat_mut().unwrap().power_snapshot);
        let enemies = std::mem::take(&mut self.combat_mut().unwrap().enemy_power_snapshot);
        let mut context = Context::card(card);
        for _ in 0..self.combat().unwrap().card_plays {
            context = self.queue_card_triggers(content, card, powers.clone(), enemies.clone());
        }
        let damage = self.combat().unwrap().player.power(power_id::BLACK_HOLE);
        if self.combat().unwrap().card_stars > 0 && damage > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(damage, damage)),
                context,
            });
        }
        if card_type == CardType::Skill {
            self.return_make_it_so(old_skills);
        }
        if self.combat().unwrap().card_energy >= 2 {
            while self.combat().unwrap().hand.len() < 10 {
                let Some(index) = self
                    .combat()
                    .unwrap()
                    .discard
                    .iter()
                    .position(|card| card.id == card_id::RIGHT_HAND_HAND)
                else {
                    break;
                };
                let card = self.combat_mut().unwrap().discard.remove(index);
                self.combat_mut().unwrap().hand.push(card);
            }
        }
        self.tick_panache();
        self.auto_play_top();
    }

    fn use_potion(&mut self, content: &Content, slot: usize, target: Option<usize>) {
        let id = self.run.potions[slot].take().unwrap();
        if self.combat().is_none() && content.potions[id as usize].id == "POTION.FOUL_POTION" {
            if self.fake_shop {
                self.fake_shop = false;
                self.start_event_combat(content, "ENCOUNTER.FAKE_MERCHANT_EVENT_ENCOUNTER", 7);
            } else {
                self.gain_gold(content, 100);
            }
            return;
        }
        if self.combat().is_none() {
            match id {
                potion_id::ENTROPIC_BREW => {
                    let pool = self.potion_pool(content);
                    let count = self
                        .run
                        .potions
                        .iter()
                        .filter(|slot| slot.is_none())
                        .count();
                    let sozu = self.has_relic(content, "RELIC.SOZU");
                    let potions = self.random_potions(
                        &pool,
                        if sozu { count.min(1) } else { count },
                        false,
                        false,
                    );
                    if sozu {
                        return;
                    }
                    for (slot, potion) in self
                        .run
                        .potions
                        .iter_mut()
                        .filter(|slot| slot.is_none())
                        .zip(potions)
                    {
                        *slot = Some(potion);
                    }
                }
                potion_id::FRUIT_JUICE => {
                    self.run.max_hp += 5;
                    self.run.hp += 5;
                }
                potion_id::BLOOD => {
                    self.run.hp = (self.run.hp + self.run.max_hp * 20 / 100).min(self.run.max_hp)
                }
                _ => {}
            }
            return;
        }
        self.face_target(target);
        let context = Context {
            source: Actor::Player,
            target,
            card: None,
            upgraded: false,
            x: 0,
            event: 0,
            orb: false,
            orb_id: None,
            pen_nib: false,
        };
        push_effects(
            &mut self.combat_mut().unwrap().queue,
            content.potions[id as usize].effects,
            context,
        );
        if self.has_relic(content, "RELIC.REPTILE_TRINKET") {
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 3);
            self.apply_power(content, Actor::Player, power_id::REPTILE_TRINKET, 3);
        }
        self.sync_belt_buckle(content);
    }

    fn choose(&mut self, content: &Content, index: usize) {
        let choice = self.combat().unwrap().choice.unwrap();
        let knowledge = (choice.pile == Pile::Offer && self.combat().unwrap().enemy_turn)
            .then(|| {
                let card = self.combat().unwrap().offer[index];
                self.combat()
                    .unwrap()
                    .enemies
                    .iter()
                    .enumerate()
                    .find(|(_, enemy)| {
                        enemy.move_index == 0
                            && content.enemies[enemy.creature.id as usize].id
                                == "MONSTER.KNOWLEDGE_DEMON"
                    })
                    .map(|(enemy, _)| (enemy, card))
            })
            .flatten();
        if let Some((enemy, card)) = knowledge {
            let (power, amount) = match card.id {
                card_id::DISINTEGRATION => (power_id::DISINTEGRATION, card.value),
                card_id::MIND_ROT => (power_id::MIND_ROT, 1),
                card_id::SLOTH => (power_id::SLOTH, 3),
                card_id::WASTE_AWAY => (power_id::WASTE_AWAY, 1),
                _ => unreachable!(),
            };
            let before = self.creature(Actor::Player).power(power);
            self.apply_power(content, Actor::Player, power, amount);
            if power == power_id::WASTE_AWAY && self.creature(Actor::Player).power(power) > before {
                let combat = self.combat_mut().unwrap();
                combat.max_energy = combat.max_energy.saturating_sub(1);
                combat.energy = combat.energy.min(combat.max_energy);
            }
            let combat = self.combat_mut().unwrap();
            combat.offer.clear();
            combat.choice = None;
            self.trigger(content, Trigger::TurnEnd, Actor::Enemy(enemy), 0);
            self.resolve(content);
            let count = self.combat().unwrap().enemies.len();
            self.finish_enemy_turn(content, count);
            return;
        }
        self.apply_card_op(content, choice.pile, index, choice.op);
        let combat = self.combat_mut().unwrap();
        let remaining = choice.remaining - 1;
        let candidates = cards(combat, choice.pile)
            .iter()
            .filter(|card| eligible(content, card, choice.filter, choice.op))
            .count();
        combat.choice = (remaining > 0 && candidates > 0).then_some(Choice {
            remaining,
            ..choice
        });
    }

    fn start_turn(&mut self, content: &Content) {
        for enemy in &mut self.combat_mut().unwrap().enemies {
            if let Some(power) = enemy
                .creature
                .powers
                .iter_mut()
                .find(|power| power.id == power_id::HARDENED_SHELL)
            {
                power.value = 0;
            }
        }
        let draw_next = self
            .creature(Actor::Player)
            .power(power_id::DRAW_CARDS_NEXT_TURN)
            .clamp(0, u8::MAX as i16) as u8;
        let happy_flower = self.has_relic(content, "RELIC.HAPPY_FLOWER");
        if happy_flower {
            self.happy_flower = (self.happy_flower + 1) % 3;
        }
        let happy_flower = (happy_flower && self.happy_flower == 0) as i16;
        let fake_happy_flower = self.has_relic(content, "RELIC.FAKE_HAPPY_FLOWER");
        if fake_happy_flower {
            self.fake_happy_flower = (self.fake_happy_flower + 1) % 5;
        }
        let fake_happy_flower = (fake_happy_flower && self.fake_happy_flower == 0) as i16;
        let candelabra = self.has_relic(content, "RELIC.CANDELABRA");
        let sparkling_rouge = self.has_relic(content, "RELIC.SPARKLING_ROUGE");
        let bread = self.has_relic(content, "RELIC.BREAD");
        let art_of_war = self.has_relic(content, "RELIC.ART_OF_WAR");
        let captains_wheel = self.has_relic(content, "RELIC.CAPTAINS_WHEEL");
        let horn_cleat = self.has_relic(content, "RELIC.HORN_CLEAT");
        let ninja_scroll = self.has_relic(content, "RELIC.NINJA_SCROLL");
        let ice_cream = self.has_relic(content, "RELIC.ICE_CREAM");
        let sturdy_clamp = self.has_relic(content, "RELIC.STURDY_CLAMP");
        let paels_flesh = self.has_relic(content, "RELIC.PAELS_FLESH");
        let booming_conch = self.has_relic(content, "RELIC.BOOMING_CONCH");
        let elite = self.room == Room::Elite;
        let tea_set = self.tea_set as i16;
        self.tea_set = 0;
        let seal_of_gold = self.has_relic(content, "RELIC.SEAL_OF_GOLD") && self.run.gold >= 5;
        if seal_of_gold {
            self.run.gold -= 5;
        }
        let combat = self.combat_mut().unwrap();
        let mut generated = Vec::new();
        combat.turn += 1;
        combat.paels_legion = combat.paels_legion.saturating_sub(1);
        combat.demon_tongue = false;
        combat.mini_regent = false;
        combat.music_box = false;
        combat.rainbow_ring = false;
        combat.kusarigama = 0;
        combat.diamond_diadem = false;
        let prior_attacks = combat.history.attacks;
        let prior_hp_lost = combat.history.hp_lost;
        if paels_flesh && combat.turn == 3 {
            combat.max_energy += 1;
        }
        let clarity = (combat.player.power(power_id::CLARITY) > 0) as u8;
        if clarity > 0 {
            combat.player.consume_power(power_id::CLARITY);
        }
        for pile in [
            &mut combat.draw,
            &mut combat.hand,
            &mut combat.discard,
            &mut combat.exhaust,
        ] {
            for card in pile {
                card.free = false;
                card.turn_flags = 0;
            }
        }
        let nightmares = std::mem::take(&mut combat.nightmares);
        for (mut card, count) in nightmares {
            card.instance = 0;
            card.free = false;
            card.turn_flags = 0;
            for _ in 0..count {
                if combat.hand.len() < 10 {
                    combat.hand.push(card);
                } else {
                    combat.discard.push(card);
                }
                generated.push(card.id);
            }
        }
        combat.energy = if ice_cream && combat.turn > 1 {
            combat.energy
        } else {
            0
        } + combat.max_energy
            + 2 * combat.paels_tears as i16
            + happy_flower
            + fake_happy_flower
            + tea_set
            + 2 * (candelabra && combat.turn == 2) as i16
            + (booming_conch && elite && combat.turn == 1) as i16
            + (art_of_war && combat.turn > 1 && prior_attacks == 0) as i16
            + seal_of_gold as i16
            - 3 * (bread && combat.turn == 1) as i16;
        combat.last_cards = combat.history.cards;
        combat.hits.fill(0);
        let hp_loss_events = combat.history.hp_loss_events;
        combat.history = History::default();
        combat.history.hp_loss_events = hp_loss_events;
        if let Some(slow) = combat
            .player
            .powers
            .iter_mut()
            .find(|power| power.id == power_id::SLOW)
        {
            slow.amount = 0;
        }
        let would_clear_block = combat.player.kind(content, PowerKind::BlockRetain) == 0
            && combat.player.kind(content, PowerKind::BlockRetainOnce) == 0;
        let clears_block = would_clear_block && !sturdy_clamp;
        if clears_block {
            combat.player.block = 0;
        } else if would_clear_block {
            combat.player.block = combat.player.block.min(10);
        }
        if combat.turn == 3 && sparkling_rouge {
            combat.player.add_power(power_id::STRENGTH, 1);
            combat.player.add_power(power_id::DEXTERITY, 1);
        }
        let mut toric_blocks = Vec::new();
        if clears_block {
            for power in &mut combat.player.powers {
                if power.id == power_id::TORIC_TOUGHNESS {
                    toric_blocks.push(power.value);
                    power.amount -= 1;
                }
            }
            combat
                .player
                .powers
                .retain(|power| power.id != power_id::TORIC_TOUGHNESS || power.amount != 0);
            if captains_wheel && combat.turn == 3 {
                toric_blocks.push(18);
            }
            if horn_cleat && combat.turn == 2 {
                toric_blocks.push(14);
            }
        }
        if let Some(power) = combat
            .player
            .powers
            .iter()
            .find(|power| content.powers[power.id as usize].kind == PowerKind::BlockRetainOnce)
            .copied()
        {
            combat.player.consume_power(power.id);
        }
        for id in generated {
            self.trigger_generated(content, id);
        }
        for block in toric_blocks {
            self.gain_block(content, Actor::Player, block, None, false);
        }
        self.poison(content, Actor::Player);
        self.trigger(content, Trigger::TurnStart, Actor::Player, 0);
        let rampart: i16 = self
            .combat()
            .unwrap()
            .enemies
            .iter()
            .filter(|enemy| enemy.creature.hp > 0)
            .map(|enemy| enemy.creature.power(power_id::RAMPART).max(0))
            .sum();
        if rampart > 0 {
            let turrets: Vec<_> = self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, enemy)| {
                    content.enemies[enemy.creature.id as usize].id == "MONSTER.TURRET_OPERATOR"
                        && enemy.creature.hp > 0
                })
                .map(|(index, _)| index)
                .collect();
            for index in turrets {
                self.combat_mut().unwrap().enemies[index].creature.block += rampart;
            }
        }
        self.tick_boulders();
        self.passive_orbs(content, OrbTiming::TurnStart);
        self.resolve(content);
        let tools = self
            .creature(Actor::Player)
            .power(power_id::TOOLS_OF_THE_TRADE)
            .clamp(0, u8::MAX as i16) as u8;
        let pale_blue_dot = if self.combat().unwrap().last_cards >= 5 {
            self.creature(Actor::Player)
                .power(power_id::PALE_BLUE_DOT)
                .clamp(0, u8::MAX as i16) as u8
        } else {
            0
        };
        let tyranny = self
            .creature(Actor::Player)
            .power(power_id::TYRANNY)
            .clamp(0, u8::MAX as i16) as u8;
        let turn = self.combat().unwrap().turn;
        let history_course = (turn > 1 && self.has_relic(content, "RELIC.HISTORY_COURSE"))
            .then(|| self.combat_mut().unwrap().history_course.take())
            .flatten();
        if self.has_relic(content, "RELIC.POLLINOUS_CORE") {
            self.pollinous_core += 1;
        }
        let mut relic_draw = 0;
        if turn == 1 {
            relic_draw -= 2 * self.has_relic(content, "RELIC.BIG_MUSHROOM") as i16;
            relic_draw += 2
                * self.has_relic(content, "RELIC.BOOMING_CONCH") as i16
                * (self.room == Room::Elite) as i16;
        }
        if turn <= 3 {
            relic_draw += 2 * self.has_relic(content, "RELIC.RING_OF_THE_DRAKE") as i16;
        }
        relic_draw += 2 * self.has_relic(content, "RELIC.FIDDLE") as i16;
        relic_draw += 2 * self.has_relic(content, "RELIC.SNECKO_EYE") as i16;
        if turn > 1 && self.combat().unwrap().last_cards <= 3 {
            relic_draw += 3 * self.has_relic(content, "RELIC.POCKETWATCH") as i16;
        }
        if self.pollinous_core == 4 {
            relic_draw += 2;
            self.pollinous_core = 0;
        }
        let count = (self
            .combat()
            .unwrap()
            .draw_per_turn
            .saturating_add(clarity)
            .saturating_add(tools)
            .saturating_add(pale_blue_dot)
            .saturating_add(tyranny)
            .saturating_add(draw_next)
            .saturating_add(
                self.creature(Actor::Player)
                    .power(power_id::DEMESNE)
                    .clamp(0, u8::MAX as i16) as u8,
            ) as i16
            + relic_draw
            - self.creature(Actor::Player).power(power_id::MIND_ROT))
        .clamp(0, u8::MAX as i16) as u8;
        if tools > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Discard(tools, false),
                context: Context::player(),
            });
        }
        if tyranny > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Exhaust(tyranny, false),
                context: Context::player(),
            });
        }
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::DRAW_CARDS_NEXT_TURN);
        if turn == 1 && self.has_relic(content, "RELIC.BLESSED_ANTLER") {
            if let Some(id) = content.card_id("CARD.DAZED") {
                for _ in 0..3 {
                    let index = self
                        .rngs
                        .shuffle
                        .below((self.combat().unwrap().draw.len() + 1) as u32)
                        as usize;
                    let card = Card {
                        id,
                        instance: self.next_card,
                        ..Card::default()
                    };
                    self.next_card += 1;
                    self.combat_mut().unwrap().insert_unknown_draw(index, card);
                }
            }
        }
        if turn == 1 && ninja_scroll {
            if let Some(id) = content.card_id("CARD.SHIV") {
                for _ in 0..3 {
                    self.add_generated(
                        content,
                        Pile::Hand,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
            }
        }
        if self.has_relic(content, "RELIC.TOASTY_MITTENS") {
            if self.combat().unwrap().draw.is_empty() && !self.combat().unwrap().discard.is_empty()
            {
                self.shuffle_discard_into_draw(content);
                self.trigger(content, Trigger::Shuffle, Actor::Player, 0);
                self.resolve(content);
            }
            let index = if turn == 1 {
                self.combat()
                    .unwrap()
                    .draw
                    .iter()
                    .rposition(|card| card.flags(content.cards[card.id as usize]) & INNATE == 0)
            } else {
                self.combat().unwrap().draw.len().checked_sub(1)
            };
            if let Some(index) = index {
                let card = self.combat_mut().unwrap().remove_draw(index);
                self.combat_mut().unwrap().exhaust.push(card);
                self.combat_mut().unwrap().history.exhausted += 1;
                self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
                self.trigger_card(content, card, Trigger::CardExhausted);
                self.resolve(content);
            }
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
        }
        let mut turn_effects = Vec::new();
        for id in self.run.relics.clone() {
            match content.relics[id as usize].id {
                "RELIC.BIG_HAT" if turn == 1 => {
                    let mut pool: Vec<_> = content.characters[self.run.character as usize]
                        .cards
                        .iter()
                        .copied()
                        .filter(|&id| {
                            let card = content.cards[id as usize];
                            card.flags[0] & ETHEREAL != 0
                                && card.flags[0] & NO_GENERATE == 0
                                && matches!(
                                    card.rarity,
                                    CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                                )
                        })
                        .collect();
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                    for card in pool.into_iter().take(2) {
                        self.add_generated(
                            content,
                            Pile::Hand,
                            Card {
                                id: card,
                                ..Card::default()
                            },
                        );
                    }
                }
                "RELIC.BRIMSTONE" => {
                    turn_effects.push(Effect::ApplyPower(
                        Target::Player,
                        power_id::STRENGTH,
                        Amount::fixed(2, 2),
                    ));
                    turn_effects.push(Effect::ApplyPower(
                        Target::AllEnemies,
                        power_id::STRENGTH,
                        Amount::fixed(1, 1),
                    ));
                }
                "RELIC.CHANDELIER" if turn == 3 => turn_effects.push(Effect::Energy(3)),
                "RELIC.CROSSBOW" => {
                    let mut pool: Vec<_> = content.characters[self.run.character as usize]
                        .cards
                        .iter()
                        .copied()
                        .filter(|&id| {
                            let card = content.cards[id as usize];
                            card.card_type == CardType::Attack
                                && card.flags[0] & NO_GENERATE == 0
                                && matches!(
                                    card.rarity,
                                    CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                                )
                        })
                        .collect();
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                    if let Some(id) = pool.into_iter().next() {
                        self.add_generated(
                            content,
                            Pile::Hand,
                            Card {
                                id,
                                free: true,
                                ..Card::default()
                            },
                        );
                    }
                }
                "RELIC.ORANGE_DOUGH" => {
                    turn_effects.push(Effect::DistinctColorless(Pile::Hand, 2, false))
                }
                "RELIC.POWER_CELL" if turn == 1 => turn_effects.push(Effect::RandomCardOp(
                    Pile::Draw,
                    CardFilter::PlayableCost(0),
                    CardOp::Move(Pile::Hand),
                    Amount::fixed(2, 2),
                )),
                "RELIC.VERY_HOT_COCOA" if turn == 1 => turn_effects.push(Effect::Energy(4)),
                _ => {}
            }
        }
        for id in self.run.relics.clone() {
            match content.relics[id as usize].id {
                "RELIC.FUNERARY_MASK" if turn == 1 => {
                    if let Some(card) = content.card_id("CARD.SOUL") {
                        turn_effects.push(Effect::AddRandom(Pile::Draw, card, 3));
                    }
                }
                "RELIC.JEWELED_MASK" if turn == 1 => {
                    turn_effects.push(Effect::RandomCardOp(
                        Pile::Draw,
                        CardFilter::Type(CardType::Power),
                        CardOp::MoveFree(Pile::Hand),
                        Amount::fixed(1, 1),
                    ));
                }
                "RELIC.RADIANT_PEARL" if turn == 1 => {
                    if let Some(card) = content.card_id("CARD.LUMINESCE") {
                        turn_effects.push(Effect::AddCard(Pile::Hand, card, 1));
                    }
                }
                "RELIC.TOOLBOX" if turn == 1 => {
                    turn_effects.push(Effect::OfferColorless(3, false, false))
                }
                _ => {}
            }
        }
        turn_effects.push(Effect::HandDraw(count));
        for id in self.run.relics.clone() {
            match content.relics[id as usize].id {
                "RELIC.BELLOWS" if turn == 1 => {
                    turn_effects.push(Effect::Upgrade(Pile::Hand, u8::MAX, false))
                }
                "RELIC.BONE_TEA" if turn == 1 && !self.bone_tea => {
                    self.bone_tea = true;
                    turn_effects.push(Effect::Upgrade(Pile::Hand, u8::MAX, false));
                }
                "RELIC.GAMBLING_CHIP" => turn_effects.push(Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [u8::MAX; 2],
                    false,
                    true,
                    CardOp::DiscardDraw,
                )),
                "RELIC.EMOTION_CHIP" if prior_hp_lost > 0 => turn_effects.push(Effect::PassiveAll),
                "RELIC.PENDULUM" => {
                    self.pendulum = (self.pendulum + 1) % 3;
                    if self.pendulum == 0 {
                        turn_effects.push(Effect::Draw(1));
                    }
                }
                "RELIC.ROYAL_POISON" => {
                    turn_effects.push(Effect::LoseHp(Target::Player, Amount::fixed(4, 4)))
                }
                "RELIC.VEXING_PUZZLEBOX" if turn == 1 => {
                    turn_effects.push(Effect::DistinctCharacter(Pile::Hand, 1, true))
                }
                "RELIC.CHOICES_PARADOX" if turn == 1 => {
                    turn_effects.push(Effect::OfferCharacterRetain(5))
                }
                _ => {}
            }
        }
        if let Some(mut card) = history_course {
            card.instance = 0;
            card.flags |= REPLAY | DUPE;
            turn_effects.push(Effect::AutoPlay(card));
        }
        if turn == 1 && self.has_relic(content, "RELIC.WHISPERING_EARRING") {
            turn_effects.push(Effect::WhisperingEarring(0));
        }
        for effect in turn_effects.into_iter().rev() {
            self.combat_mut().unwrap().queue.push(Pending {
                effect,
                context: Context::player(),
            });
        }
        self.roll_moves(content);
        self.resolve(content);
        self.queue_bombardments();
        self.resolve(content);
    }

    fn tick_boulders(&mut self) {
        let mut damage = Vec::new();
        for boulder in &mut self.combat_mut().unwrap().boulders {
            damage.push(*boulder);
            *boulder = boulder.saturating_add(5);
        }
        for amount in damage {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(amount, amount)),
                context: Context::player(),
            });
        }
    }

    fn end_turn(&mut self, content: &Content) {
        if self.has_relic(content, "RELIC.PAELS_EYE")
            && !self.combat().unwrap().paels_eye
            && self.combat().unwrap().history.manual_cards == 0
            && !(self.combat().unwrap().turn == 1
                && self.has_relic(content, "RELIC.WHISPERING_EARRING"))
        {
            let cards = std::mem::take(&mut self.combat_mut().unwrap().hand);
            self.combat_mut().unwrap().paels_eye = true;
            self.combat_mut().unwrap().paels_eye_extra = true;
            self.combat_mut().unwrap().history.exhausted += cards.len() as i16;
            for card in cards {
                self.combat_mut().unwrap().exhaust.push(card);
                self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
                self.trigger_card(content, card, Trigger::CardExhausted);
            }
            self.resolve(content);
        }
        if self.creature(Actor::Player).block == 0 {
            let block = 6 * self.has_relic(content, "RELIC.ORICHALCUM") as i16
                + 3 * self.has_relic(content, "RELIC.FAKE_ORICHALCUM") as i16;
            if block > 0 {
                self.gain_block(content, Actor::Player, block, None, false);
            }
        }
        if self.has_relic(content, "RELIC.SCREAMING_FLAGON")
            && self.combat().unwrap().hand.is_empty()
        {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(20, 20)),
                context: Context::player(),
            });
        }
        if self.has_relic(content, "RELIC.PARRYING_SHIELD")
            && self.creature(Actor::Player).block >= 10
        {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::RandomEnemy, Amount::fixed(6, 6)),
                context: Context::player(),
            });
        }
        self.combat_mut().unwrap().diamond_diadem = self.has_relic(content, "RELIC.DIAMOND_DIADEM")
            && self.combat().unwrap().history.cards <= 2;
        if self.has_relic(content, "RELIC.LUNAR_PASTRY") {
            self.combat_mut().unwrap().stars += 1;
        }
        self.resolve(content);
        if self.creature(Actor::Player).power(power_id::STAMPEDE) > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::ContinueEndTurn,
                context: Context::player(),
            });
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Stampede,
                context: Context::player(),
            });
            self.resolve(content);
            return;
        }
        self.continue_end_turn(content);
    }

    fn continue_end_turn(&mut self, content: &Content) {
        self.doom_kill(content, vec![Actor::Player, Actor::Osty]);
        self.resolve(content);
        if self.creature(Actor::Player).hp <= 0 {
            return;
        }
        self.tick_bombs(content);
        let howl = content.card_id("CARD.HOWL_FROM_BEYOND").unwrap();
        let mut howls = Vec::new();
        self.combat_mut().unwrap().exhaust.retain(|card| {
            if card.id == howl {
                howls.push(*card);
                false
            } else {
                true
            }
        });
        for mut card in howls.into_iter().rev() {
            card.turn_flags |= RETURN_TO_HAND;
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::AutoPlay(card),
                context: Context::card(card),
            });
        }
        self.resolve(content);
        if self.combat().unwrap().history.attacks == 0
            && self.has_relic(content, "RELIC.RIPPLE_BASIN")
        {
            self.gain_block(content, Actor::Player, 4, None, false);
        }
        self.trigger(content, Trigger::TurnEnd, Actor::Player, 0);
        self.resolve(content);
        if self.combat().unwrap().turn == 7 && self.has_relic(content, "RELIC.STONE_CALENDAR") {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(52, 52)),
                context: Context::player(),
            });
            self.resolve(content);
        }
        for panache in &mut self.combat_mut().unwrap().panache {
            panache.0 = 5;
        }
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::DUPLICATION);
        self.passive_orbs(content, OrbTiming::TurnEnd);
        self.resolve(content);
        for card in self.combat().unwrap().hand.clone().into_iter().rev() {
            self.trigger_card(content, card, Trigger::TurnEnd);
        }
        self.resolve(content);
        if self.creature(Actor::Player).power(power_id::RETAIN_HAND) > 0 {
            for card in &mut self.combat_mut().unwrap().hand {
                card.turn_flags |= RETAIN;
            }
            self.creature_mut(Actor::Player)
                .consume_power(power_id::RETAIN_HAND);
        }
        if self.has_relic(content, "RELIC.RUNIC_PYRAMID") {
            for card in &mut self.combat_mut().unwrap().hand {
                card.turn_flags |= RETAIN;
            }
        }
        if self.combat().unwrap().turn == 1 && self.has_relic(content, "RELIC.RINGING_TRIANGLE") {
            for card in &mut self.combat_mut().unwrap().hand {
                card.turn_flags |= RETAIN;
            }
        }
        let retain = self
            .creature(Actor::Player)
            .power(power_id::WELL_LAID_PLANS)
            .clamp(0, u8::MAX as i16) as u8;
        if retain > 0 {
            self.combat_mut().unwrap().ending = true;
            self.select(
                content,
                Pile::Hand,
                CardFilter::WithoutFlag(RETAIN),
                CardOp::TurnFlag(RETAIN),
                retain,
                false,
                true,
            );
            if self.combat().unwrap().choice.is_some() {
                return;
            }
        }
        self.finish_end_turn(content);
    }

    fn tick_bombs(&mut self, content: &Content) {
        let mut explosions = Vec::new();
        for bomb in &mut self.combat_mut().unwrap().bombs {
            if bomb.0 > 1 {
                bomb.0 -= 1;
            } else {
                explosions.push(bomb.1);
                bomb.0 = 0;
            }
        }
        self.combat_mut().unwrap().bombs.retain(|bomb| bomb.0 > 0);
        for amount in explosions {
            for target in self.targets(Target::AllEnemies, Context::player()) {
                self.damage(
                    content,
                    Actor::Player,
                    target,
                    amount,
                    DamageKind::Unpowered,
                    None,
                );
            }
        }
    }

    fn tick_panache(&mut self) {
        let cards = self.combat().unwrap().history.cards;
        let mut damage = Vec::new();
        for panache in &mut self.combat_mut().unwrap().panache {
            if panache.2 < cards {
                panache.0 -= 1;
                if panache.0 == 0 {
                    panache.0 = 5;
                    damage.push(panache.1);
                }
            }
        }
        for amount in damage {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(amount, amount)),
                context: Context::player(),
            });
        }
    }

    fn finish_end_turn(&mut self, content: &Content) {
        self.combat_mut().unwrap().ending = false;
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::RINGING);
        self.combat_mut().unwrap().paels_tears =
            self.combat().unwrap().energy > 0 && self.has_relic(content, "RELIC.PAELS_TEARS");
        self.reset_tender();
        let (count, exhausted) = {
            let combat = self.combat_mut().unwrap();
            let mut hand = Vec::new();
            let mut exhausted = Vec::new();
            let mut discarded = Vec::new();
            let hexed = combat.player.power(power_id::HEX) > 0;
            for card in std::mem::take(&mut combat.hand) {
                let def = content.cards[card.id as usize];
                let flags = card.flags(def);
                if flags & RETAIN != 0 {
                    hand.push(card);
                } else if flags & ETHEREAL != 0 || hexed {
                    combat.exhaust.push(card);
                    exhausted.push(card);
                } else {
                    discarded.push(card);
                }
            }
            discarded.sort_by_key(|card| {
                !content.cards[card.id as usize]
                    .hooks
                    .iter()
                    .any(|hook| hook.trigger == Trigger::TurnEnd)
            });
            combat.discard.extend(discarded);
            combat.hand = hand;
            combat.history.exhausted += exhausted.len() as i16;
            (combat.enemies.len(), exhausted)
        };
        for card in exhausted {
            self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
            self.trigger_card(content, card, Trigger::CardExhausted);
        }
        self.resolve(content);
        if self.has_relic(content, "RELIC.BOOKMARK") {
            let spiked = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
            let choices: Vec<_> = self
                .combat()
                .unwrap()
                .hand
                .iter()
                .enumerate()
                .filter(|(_, card)| {
                    let def = content.cards[card.id as usize];
                    def.cost[card.upgrades.min(1) as usize] >= 0
                        && energy_cost(self.combat().unwrap(), **card, def, spiked) > 0
                })
                .map(|(index, _)| index)
                .collect();
            if !choices.is_empty() {
                let choice =
                    choices[self.rngs.combat_card_selection.below(choices.len() as u32) as usize];
                let card = self.combat().unwrap().hand[choice];
                let def = content.cards[card.id as usize];
                let cost = energy_cost(self.combat().unwrap(), card, def, spiked) - 1;
                self.combat_mut().unwrap().hand[choice].cost_override = Some(cost as i8);
            }
        }
        self.creature_mut(Actor::Player).powers.retain(|power| {
            power.amount <= 0 || content.powers[power.id as usize].kind != PowerKind::NoDraw
        });
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::NO_ENERGY_GAIN);
        let disintegration = self.creature(Actor::Player).power(power_id::DISINTEGRATION);
        if disintegration > 0 {
            self.damage(
                content,
                Actor::Player,
                Actor::Player,
                disintegration,
                DamageKind::Unpowered,
                None,
            );
            if self.creature(Actor::Player).hp <= 0 {
                return;
            }
        }
        for enemy in &mut self.combat_mut().unwrap().enemies {
            enemy
                .creature
                .powers
                .retain(|power| power.id != power_id::OBLIVION);
            for power in &mut enemy.creature.powers {
                if matches!(power.id, power_id::HARDENED_SHELL | power_id::SKITTISH) {
                    power.value = 0;
                }
            }
            if enemy.creature.power(power_id::NEMESIS) > 0 {
                enemy
                    .creature
                    .powers
                    .retain(|power| power.id != power_id::INTANGIBLE);
            }
        }
        if self.combat().unwrap().paels_eye_extra {
            self.combat_mut().unwrap().paels_eye_extra = false;
            self.start_turn(content);
            return;
        }
        self.combat_mut().unwrap().enemy_turn = true;
        for enemy in &mut self.combat_mut().unwrap().enemies {
            if let Some(slow) = enemy
                .creature
                .powers
                .iter_mut()
                .find(|power| power.id == power_id::SLOW)
            {
                slow.amount = 0;
            }
        }
        let obscura =
            self.combat().unwrap().enemies.iter().any(|enemy| {
                content.enemies[enemy.creature.id as usize].id == "MONSTER.THE_OBSCURA"
            });
        let primary_alive = self
            .combat()
            .unwrap()
            .enemies
            .iter()
            .any(|enemy| enemy.creature.hp > 0 && enemy.creature.power(power_id::MINION) == 0);
        let segment_alive = self
            .combat()
            .unwrap()
            .enemies
            .iter()
            .any(|enemy| enemy.creature.hp > 0 && enemy.creature.power(power_id::REATTACH) > 0);
        let mut participants: Vec<_> = (0..count)
            .filter(|&index| {
                let enemy = &self.combat().unwrap().enemies[index];
                enemy.creature.hp > 0
                    || primary_alive
                        && enemy.creature.power(power_id::ILLUSION) > 0
                        && enemy.move_index == 1
                    || segment_alive
                        && enemy.creature.power(power_id::REATTACH) > 0
                        && matches!(enemy.move_index, 3 | 4)
                    || enemy.creature.power(power_id::ADAPTABLE) > 0 && enemy.move_index == 0
            })
            .collect();
        if obscura {
            participants.sort_by_key(|&index| {
                content.enemies[self.combat().unwrap().enemies[index].creature.id as usize].id
                    != "MONSTER.PARAFRIGHT"
            });
        }
        for &index in &participants {
            self.poison(content, Actor::Enemy(index));
            self.trigger(content, Trigger::TurnStart, Actor::Enemy(index), 0);
            self.resolve(content);
        }
        for index in participants {
            let enemy = &self.combat().unwrap().enemies[index];
            let reviving = enemy.creature.hp <= 0
                && (enemy.creature.power(power_id::ILLUSION) > 0
                    && enemy.move_index == 1
                    && self.combat().unwrap().enemies.iter().any(|other| {
                        other.creature.hp > 0 && other.creature.power(power_id::MINION) == 0
                    })
                    || enemy.creature.power(power_id::REATTACH) > 0
                        && matches!(enemy.move_index, 3 | 4)
                        && self.combat().unwrap().enemies.iter().any(|other| {
                            other.creature.hp > 0 && other.creature.power(power_id::REATTACH) > 0
                        })
                    || enemy.creature.power(power_id::ADAPTABLE) > 0 && enemy.move_index == 0);
            if enemy.creature.hp <= 0 && !reviving {
                continue;
            }
            if self.combat().unwrap().enemies[index]
                .creature
                .kind(content, PowerKind::BlockRetain)
                == 0
                && self.combat().unwrap().enemies[index]
                    .creature
                    .kind(content, PowerKind::BlockRetainOnce)
                    == 0
            {
                self.combat_mut().unwrap().enemies[index].creature.block = 0;
            }
            if let Some(power) = self.combat().unwrap().enemies[index]
                .creature
                .powers
                .iter()
                .find(|power| content.powers[power.id as usize].kind == PowerKind::BlockRetainOnce)
                .copied()
            {
                self.combat_mut().unwrap().enemies[index]
                    .creature
                    .consume_power(power.id);
            }
            let move_index = self.combat().unwrap().enemies[index].move_index;
            let id = self.combat().unwrap().enemies[index].creature.id;
            let stunned = self.combat().unwrap().enemies[index].stunned;
            if !stunned
                && content.enemies[id as usize].id == "MONSTER.THIEVING_HOPPER"
                && move_index == 0
            {
                self.thieving_hopper(content, index);
            }
            let effects = content.enemies[id as usize].moves[move_index].effects;
            let context = Context {
                source: Actor::Enemy(index),
                target: None,
                card: None,
                upgraded: false,
                x: 0,
                event: 0,
                orb: false,
                orb_id: None,
                pen_nib: false,
            };
            if stunned {
            } else if content.enemies[id as usize].id == "MONSTER.WATERFALL_GIANT"
                && move_index == 4
            {
                let amount = if self.run.ascension >= 9 { 23 } else { 20 }
                    + self.combat().unwrap().enemies[index].value;
                self.damage(
                    content,
                    Actor::Enemy(index),
                    Actor::Player,
                    amount,
                    DamageKind::Attack,
                    None,
                );
                self.combat_mut().unwrap().enemies[index].value += 5;
                self.apply_power(content, Actor::Enemy(index), power_id::STEAM_ERUPTION, 3);
            } else if content.enemies[id as usize].id == "MONSTER.WATERFALL_GIANT"
                && move_index == 6
            {
                let amount = self.combat().unwrap().enemies[index].value;
                self.damage(
                    content,
                    Actor::Enemy(index),
                    Actor::Player,
                    amount,
                    DamageKind::Attack,
                    None,
                );
                self.kill_actor(content, Actor::Enemy(index));
            } else if content.enemies[id as usize].id == "MONSTER.KNOWLEDGE_DEMON"
                && move_index == 0
            {
                let stage = self.combat().unwrap().enemies[index]
                    .move_history
                    .iter()
                    .filter(|&&prior| prior == 0)
                    .count()
                    .saturating_sub(1)
                    .min(2);
                self.combat_mut().unwrap().offer = vec![
                    Card {
                        id: card_id::DISINTEGRATION,
                        value: 6 + stage as i16,
                        ..Card::default()
                    },
                    Card {
                        id: [card_id::MIND_ROT, card_id::SLOTH, card_id::WASTE_AWAY][stage],
                        ..Card::default()
                    },
                ];
                self.select(
                    content,
                    Pile::Offer,
                    CardFilter::Any,
                    CardOp::TakeOffer,
                    1,
                    false,
                    false,
                );
                return;
            } else if content.enemies[id as usize].id == "MONSTER.TEST_SUBJECT" && move_index == 0 {
                let first = self.creature(Actor::Enemy(index)).max_hp < 200;
                let hp = match (first, self.run.ascension >= 8) {
                    (true, false) => 200,
                    (true, true) => 212,
                    (false, false) => 300,
                    (false, true) => 313,
                };
                let creature = self.creature_mut(Actor::Enemy(index));
                creature.hp = hp;
                creature.max_hp = hp;
                if first {
                    creature.add_power(power_id::PAINFUL_STABS, 1);
                } else {
                    creature.powers.retain(|power| {
                        !matches!(power.id, power_id::ADAPTABLE | power_id::PAINFUL_STABS)
                    });
                    creature.add_power(power_id::NEMESIS, 1);
                }
            } else if content.enemies[id as usize].id == "MONSTER.TEST_SUBJECT" && move_index == 3 {
                let hits = 2 + self.combat().unwrap().enemies[index]
                    .move_history
                    .iter()
                    .filter(|&&prior| prior == 3)
                    .count();
                let amount = if self.run.ascension >= 9 { 11 } else { 10 };
                for _ in 0..hits {
                    self.damage(
                        content,
                        Actor::Enemy(index),
                        Actor::Player,
                        amount,
                        DamageKind::Attack,
                        None,
                    );
                }
            } else if content.enemies[id as usize].id == "MONSTER.TEST_SUBJECT" && move_index == 6 {
                for _ in 0..if self.run.ascension >= 9 { 5 } else { 3 } {
                    self.add_generated(
                        content,
                        Pile::Discard,
                        Card {
                            id: card_id::BURN,
                            ..Card::default()
                        },
                    );
                }
                self.apply_power(
                    content,
                    Actor::Enemy(index),
                    power_id::STRENGTH,
                    if self.run.ascension >= 9 { 3 } else { 2 },
                );
            } else if content.enemies[id as usize].id == "MONSTER.OVICOPTER" && move_index == 0 {
                let eggs = self
                    .combat()
                    .unwrap()
                    .enemies
                    .iter()
                    .filter(|enemy| {
                        enemy.creature.hp > 0
                            && content.enemies[enemy.creature.id as usize].id == "MONSTER.TOUGH_EGG"
                    })
                    .count();
                for _ in eggs..3 {
                    let egg = self.spawn_enemy(content, "MONSTER.TOUGH_EGG");
                    self.apply_power(content, Actor::Enemy(egg), power_id::MINION, 1);
                    self.apply_power(content, Actor::Enemy(egg), power_id::HATCH, 1);
                }
            } else if content.enemies[id as usize].id == "MONSTER.TOUGH_EGG" && move_index == 0 {
                let range = if self.run.ascension >= 8 {
                    20..=22
                } else {
                    19..=21
                };
                let hp = self.rngs.niche.range(*range.start(), *range.end());
                let egg = &mut self.combat_mut().unwrap().enemies[index].creature;
                egg.hp = hp;
                egg.max_hp = hp;
                egg.powers.retain(|power| power.id == power_id::MINION);
            } else if content.enemies[id as usize].id == "MONSTER.FOGMOG" && move_index == 0 {
                self.spawn_enemy(content, "MONSTER.EYE_WITH_TEETH");
            } else if content.enemies[id as usize].id == "MONSTER.LIVING_FOG" && move_index == 1 {
                self.spawn_enemy(content, "MONSTER.GAS_BOMB");
                push_effects(&mut self.combat_mut().unwrap().queue, effects, context);
            } else if content.enemies[id as usize].id == "MONSTER.FABRICATOR" && move_index < 2 {
                if move_index == 1 {
                    push_effects(&mut self.combat_mut().unwrap().queue, effects, context);
                    self.resolve(content);
                } else {
                    let bot = ["MONSTER.GUARDBOT", "MONSTER.NOISEBOT"]
                        [self.rngs.monster_ai.below(2) as usize];
                    let bot = self.spawn_enemy(content, bot);
                    self.apply_power(content, Actor::Enemy(bot), power_id::MINION, 1);
                }
                let bot =
                    ["MONSTER.ZAPBOT", "MONSTER.STABBOT"][self.rngs.monster_ai.below(2) as usize];
                let bot = self.spawn_enemy(content, bot);
                self.apply_power(content, Actor::Enemy(bot), power_id::MINION, 1);
            } else if content.enemies[id as usize].id == "MONSTER.GUARDBOT" {
                if let Some(fabricator) = self.combat().unwrap().enemies.iter().position(|enemy| {
                    enemy.creature.hp > 0
                        && content.enemies[enemy.creature.id as usize].id == "MONSTER.FABRICATOR"
                }) {
                    let creature = &mut self.combat_mut().unwrap().enemies[fabricator].creature;
                    creature.block = creature.block.saturating_add(15);
                }
            } else if content.enemies[id as usize].id == "MONSTER.NOISEBOT" {
                self.add_generated(
                    content,
                    Pile::Discard,
                    Card {
                        id: card_id::DAZED,
                        ..Card::default()
                    },
                );
                self.add_random(
                    content,
                    Pile::Draw,
                    Card {
                        id: card_id::DAZED,
                        ..Card::default()
                    },
                );
            } else if content.enemies[id as usize].id == "MONSTER.TWO_TAILED_RAT"
                && move_index == 3
                && self
                    .combat()
                    .unwrap()
                    .enemies
                    .iter()
                    .filter(|enemy| {
                        content.enemies[enemy.creature.id as usize].id == "MONSTER.TWO_TAILED_RAT"
                    })
                    .count()
                    < 5
            {
                let rat = self.spawn_enemy(content, "MONSTER.TWO_TAILED_RAT");
                self.combat_mut().unwrap().enemies[rat].move_index =
                    self.rngs.monster_ai.below(3) as usize;
            } else if content.enemies[id as usize].id == "MONSTER.THE_OBSCURA" && move_index == 0 {
                self.spawn_enemy(content, "MONSTER.PARAFRIGHT");
            } else if content.enemies[id as usize].id == "MONSTER.ENTOMANCER" && move_index == 2 {
                let hive = self
                    .creature(Actor::Enemy(index))
                    .power(power_id::PERSONAL_HIVE);
                if hive < 3 {
                    self.apply_power(content, Actor::Enemy(index), power_id::PERSONAL_HIVE, 1);
                }
                self.apply_power(
                    content,
                    Actor::Enemy(index),
                    power_id::STRENGTH,
                    if hive < 3 { 1 } else { 2 },
                );
            } else {
                push_effects(&mut self.combat_mut().unwrap().queue, effects, context);
            }
            self.resolve(content);
            if content.enemies[id as usize].id == "MONSTER.AXEBOT" && move_index == 0 {
                let stock = self.creature(Actor::Enemy(index)).power(power_id::STOCK);
                let strength = if self.run.ascension >= 9 { 4 } else { 3 } * (2 - stock);
                self.apply_power(content, Actor::Enemy(index), power_id::STRENGTH, strength);
            }
            if content.enemies[id as usize].id == "MONSTER.GREMLIN_MERC" {
                let stolen = self.run.gold.min(
                    self.creature(Actor::Enemy(index))
                        .power(power_id::THIEVERY)
                        .max(0) as i32,
                );
                self.run.gold -= stolen;
                if let Some(power) = self
                    .creature_mut(Actor::Enemy(index))
                    .powers
                    .iter_mut()
                    .find(|power| power.id == power_id::THIEVERY)
                {
                    power.value = power.value.saturating_add(stolen as i16);
                }
            }
            if content.enemies[id as usize].id == "MONSTER.LAGAVULIN_MATRIARCH"
                && self.creature(Actor::Enemy(index)).power(power_id::ASLEEP) == 1
            {
                self.creature_mut(Actor::Enemy(index))
                    .powers
                    .retain(|power| power.id != 79);
            }
            if content.enemies[id as usize].id == "MONSTER.GAS_BOMB"
                && self.creature(Actor::Enemy(index)).hp > 0
            {
                self.kill_actor(content, Actor::Enemy(index));
            }
            self.trigger(content, Trigger::TurnEnd, Actor::Enemy(index), 0);
            self.resolve(content);
            if content.enemies[id as usize].id == "MONSTER.LAGAVULIN_MATRIARCH" {
                self.creature_mut(Actor::Enemy(index))
                    .consume_power(power_id::ASLEEP);
            }
            if content.enemies[id as usize].id == "MONSTER.SLUMBERING_BEETLE" {
                self.creature_mut(Actor::Enemy(index))
                    .consume_power(power_id::SLUMBER);
                if self.creature(Actor::Enemy(index)).power(power_id::SLUMBER) == 0 {
                    self.creature_mut(Actor::Enemy(index))
                        .powers
                        .retain(|power| power.id != 79);
                }
            }
            if (1..=3).contains(&self.event_combat) {
                let timer = self
                    .creature(Actor::Enemy(index))
                    .power(power_id::TIME_LIMIT);
                if timer > 1 {
                    self.creature_mut(Actor::Enemy(index))
                        .add_power(power_id::TIME_LIMIT, -1);
                } else if timer == 1 {
                    self.creature_mut(Actor::Enemy(index)).hp = 0;
                    self.event_combat |= 128;
                }
            }
            if self
                .creature(Actor::Enemy(index))
                .power(power_id::ESCAPE_ARTIST)
                > 1
            {
                self.creature_mut(Actor::Enemy(index))
                    .consume_power(power_id::ESCAPE_ARTIST);
            }
            if content.enemies[id as usize].id == "MONSTER.THIEVING_HOPPER" && move_index == 4 {
                self.creature_mut(Actor::Enemy(index)).hp = 0;
            }
            if content.enemies[id as usize].id == "MONSTER.FAT_GREMLIN" && move_index == 1 {
                self.creature_mut(Actor::Enemy(index)).hp = 0;
            }
        }
        self.finish_enemy_turn(content, count);
    }

    fn finish_enemy_turn(&mut self, content: &Content, count: usize) {
        let disintegration = self.creature(Actor::Player).power(power_id::DISINTEGRATION);
        if disintegration > 0 {
            self.damage(
                content,
                Actor::Player,
                Actor::Player,
                disintegration,
                DamageKind::Unpowered,
                None,
            );
        }
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::TAINTED);
        self.decay(content, Actor::Player);
        self.decay(content, Actor::Osty);
        for index in 0..count {
            self.decay(content, Actor::Enemy(index));
        }
        for enemy in &mut self.combat_mut().unwrap().enemies {
            if enemy.creature.hp > 0 && enemy.creature.power(power_id::NEMESIS) > 0 {
                enemy.creature.add_power(power_id::INTANGIBLE, 1);
            }
            enemy.creature.consume_power(power_id::HATCH);
        }
        self.combat_mut().unwrap().enemy_turn = false;
        let enemies = (0..count).map(Actor::Enemy).collect();
        self.doom_kill(content, enemies);
        self.resolve(content);
        self.creature_mut(Actor::Player)
            .consume_power(power_id::COLOSSUS);
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::FLAME_BARRIER);
        if self.combat().unwrap().player.hp > 0
            && self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .any(|x| x.creature.hp > 0)
        {
            self.start_turn(content);
        }
    }

    fn tender(&mut self) {
        let player = self.creature_mut(Actor::Player);
        let Some(power) = player
            .powers
            .iter_mut()
            .find(|power| power.id == power_id::TENDER)
        else {
            return;
        };
        power.amount += 1;
        player.add_power(power_id::STRENGTH, -1);
        player.add_power(power_id::DEXTERITY, -1);
    }

    fn reset_tender(&mut self) {
        let player = self.creature_mut(Actor::Player);
        let Some(power) = player
            .powers
            .iter_mut()
            .find(|power| power.id == power_id::TENDER)
        else {
            return;
        };
        let amount = power.amount;
        power.amount = 0;
        player.add_power(power_id::STRENGTH, amount);
        player.add_power(power_id::DEXTERITY, amount);
    }

    fn thieving_hopper(&mut self, content: &Content, enemy: usize) {
        let priorities: &[&[CardRarity]] = &[
            &[CardRarity::Uncommon],
            &[CardRarity::Common, CardRarity::Rare, CardRarity::Event],
            &[CardRarity::Basic],
            &[CardRarity::Ancient],
        ];
        for rarities in priorities {
            let deck = &self.run.deck;
            let mut candidates = Vec::new();
            for pile in [Pile::Draw, Pile::Discard] {
                let cards = cards(self.combat().unwrap(), pile);
                for offset in 0..cards.len() {
                    let index = if pile == Pile::Draw {
                        cards.len() - 1 - offset
                    } else {
                        offset
                    };
                    let card = cards[index];
                    if deck.iter().any(|deck| deck.instance == card.instance)
                        && rarities.contains(&content.cards[card.id as usize].rarity)
                    {
                        candidates.push((pile, index));
                    }
                }
            }
            if !candidates.is_empty() {
                let (pile, index) = candidates[self
                    .rngs
                    .combat_card_generation
                    .below(candidates.len() as u32)
                    as usize];
                let card = if pile == Pile::Draw {
                    self.combat_mut().unwrap().remove_draw(index)
                } else {
                    self.pile_mut(pile).remove(index)
                };
                if let Some(index) = self
                    .run
                    .deck
                    .iter()
                    .position(|candidate| candidate.instance == card.instance)
                {
                    self.run.deck.remove(index);
                }
                self.creature_mut(Actor::Enemy(enemy))
                    .add_power(power_id::SWIPE, 1);
                break;
            }
        }
    }

    fn roll_moves(&mut self, content: &Content) {
        let Phase::Combat(combat) = &mut self.phase else {
            return;
        };
        let living = combat
            .enemies
            .iter()
            .filter(|enemy| enemy.creature.hp > 0)
            .count();
        let torch_alive = combat.enemies.iter().any(|enemy| {
            enemy.creature.hp > 0
                && content.enemies[enemy.creature.id as usize].id == "MONSTER.TORCH_HEAD_AMALGAM"
        });
        let rat_count = combat
            .enemies
            .iter()
            .filter(|enemy| {
                content.enemies[enemy.creature.id as usize].id == "MONSTER.TWO_TAILED_RAT"
            })
            .count();
        let rat_calls = combat
            .enemies
            .iter()
            .flat_map(|enemy| &enemy.move_history)
            .filter(|&&move_index| move_index == 3)
            .count();
        let mut rat_summon_queued = false;
        for (slot, enemy) in combat.enemies.iter_mut().enumerate() {
            if enemy.creature.hp <= 0 {
                continue;
            }
            if enemy.stunned {
                enemy.stunned = false;
                continue;
            }
            let moves = content.enemies[enemy.creature.id as usize].moves;
            if content.enemies[enemy.creature.id as usize].id == "MONSTER.WRIGGLER" {
                let chosen = match enemy.last_move {
                    0 if slot % 2 == 0 => 1,
                    0 => 2,
                    1 => 2,
                    _ => 1,
                };
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if content.enemies[enemy.creature.id as usize].id == "MONSTER.LIVING_SHIELD"
                && enemy.last_move == 0
            {
                let chosen = (living == 1) as usize;
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if content.enemies[enemy.creature.id as usize].id == "MONSTER.QUEEN"
                && matches!(enemy.last_move, 1 | 2)
            {
                let chosen = if torch_alive { 2 } else { 3 };
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if content.enemies[enemy.creature.id as usize].id == "MONSTER.OVICOPTER"
                && enemy.last_move == 3
            {
                let teammates = living.saturating_sub(1);
                let chosen = if teammates <= 3 { 0 } else { 1 };
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if enemy.last_move == usize::MAX {
                enemy.last_move = enemy.move_index;
                enemy.repeats = 1;
                enemy.move_history.push(enemy.move_index);
                continue;
            }
            let id = content.enemies[enemy.creature.id as usize].id;
            if id == "MONSTER.FABRICATOR" {
                let chosen = if living.saturating_sub(1) < 4 {
                    self.rngs.monster_ai.below(2) as usize
                } else {
                    2
                };
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if id == "MONSTER.LAGAVULIN_MATRIARCH" && enemy.last_move == 0 {
                let chosen = if enemy.creature.power(power_id::ASLEEP) > 0 {
                    0
                } else {
                    1
                };
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if id == "MONSTER.TEST_SUBJECT" && enemy.last_move == 0 {
                let chosen = if enemy.creature.max_hp >= 300 { 4 } else { 3 };
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if id == "MONSTER.KNOWLEDGE_DEMON" && enemy.last_move == 3 {
                let chosen = usize::from(
                    enemy
                        .move_history
                        .iter()
                        .filter(|&&move_index| move_index == 0)
                        .count()
                        >= 3,
                );
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if id == "MONSTER.SLUMBERING_BEETLE" && enemy.last_move == 0 {
                let chosen = usize::from(enemy.creature.power(power_id::SLUMBER) == 0);
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if id == "MONSTER.FROG_KNIGHT" && enemy.last_move == 0 {
                let charged = enemy.move_history.contains(&3);
                let chosen = if charged || enemy.creature.hp * 2 >= enemy.creature.max_hp {
                    2
                } else {
                    3
                };
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if id == "MONSTER.TWO_TAILED_RAT" {
                let basic_moves = enemy
                    .move_history
                    .iter()
                    .filter(|&&move_index| move_index < 3)
                    .count();
                let can_summon = basic_moves >= 2
                    && rat_calls < 3
                    && rat_count < 5
                    && !rat_summon_queued
                    && !enemy.move_history.contains(&3);
                let mut choices: Vec<_> = (0..if can_summon { 4 } else { 3 })
                    .filter(|&move_index| move_index != enemy.last_move)
                    .collect();
                let total: u16 = choices.iter().map(|&i| moves[i].weight as u16).sum();
                let mut roll = self.rngs.monster_ai.below(total as u32) as u16;
                let mut chosen = choices.remove(0);
                for i in choices {
                    if roll < moves[chosen].weight as u16 {
                        break;
                    }
                    roll -= moves[chosen].weight as u16;
                    chosen = i;
                }
                rat_summon_queued |= chosen == 3;
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            let next = moves[enemy.last_move].next;
            let random = next.len() > 1
                || next.is_empty() && moves.len() > 1
                || id == "MONSTER.LEAF_SLIME_S";
            let mut choices: Vec<_> = if next.is_empty() {
                (0..moves.len()).collect()
            } else {
                next.to_vec()
            };
            choices.retain(|&i| i != enemy.last_move || enemy.repeats < moves[i].max_repeats);
            if id == "MONSTER.MAWLER" && enemy.move_history.contains(&1) {
                choices.retain(|&i| i != 1);
            }
            choices.retain(|&i| {
                let cooldown = match (id, i) {
                    ("MONSTER.FLYCONID", 0) => 3,
                    ("MONSTER.FLYCONID", 1) => 2,
                    _ => 0,
                };
                !enemy
                    .move_history
                    .iter()
                    .rev()
                    .take(cooldown)
                    .any(|&prior| prior == i)
            });
            if choices.is_empty() {
                if next.is_empty() {
                    choices.push(enemy.last_move);
                } else {
                    choices.extend_from_slice(next);
                }
            }
            let mut chosen = choices[0];
            if random && self.expectation.is_none() {
                let total: u16 = choices.iter().map(|&i| moves[i].weight as u16).sum();
                let mut roll = self.rngs.monster_ai.below(total.max(1) as u32) as u16;
                for &i in &choices {
                    if roll < moves[i].weight as u16 {
                        chosen = i;
                        break;
                    }
                    roll -= moves[i].weight as u16;
                }
            }
            if chosen == enemy.last_move {
                enemy.repeats += 1;
            } else {
                enemy.last_move = chosen;
                enemy.repeats = 1;
            }
            enemy.move_index = chosen;
            enemy.move_history.push(chosen);
        }
    }

    fn draw(
        &mut self,
        content: &Content,
        count: u8,
        context: Context,
        hand_draw: bool,
        draw_flags: u16,
    ) -> Option<Card> {
        if !hand_draw
            && !self.combat().unwrap().enemy_turn
            && self.has_relic(content, "RELIC.FIDDLE")
        {
            return None;
        }
        if self
            .combat()
            .unwrap()
            .player
            .kind(content, PowerKind::NoDraw)
            > 0
        {
            return None;
        }
        if count == 0 || self.combat().unwrap().hand.len() >= 10 {
            return None;
        }
        let shuffled = self.combat().unwrap().draw.is_empty();
        if shuffled {
            if self.combat().unwrap().discard.is_empty() {
                return None;
            }
            self.shuffle_discard_into_draw(content);
            self.trigger(content, Trigger::Shuffle, Actor::Player, 0);
        }
        let mut card = self.take_draw().unwrap();
        self.expectation_draw();
        if card.enchantment == Some(Enchantment::Slither) {
            self.expectation_unknown();
            card.cost_override = Some(if self.expectation.is_some() {
                0
            } else {
                self.rngs.combat_energy_costs.below(4) as i8
            });
        }
        if (self.has_relic(content, "RELIC.SNECKO_EYE")
            || self.has_relic(content, "RELIC.FAKE_SNECKO_EYE"))
            && content.cards[card.id as usize].cost[card.upgrades.min(1) as usize] >= 0
        {
            self.expectation_unknown();
            card.cost_override = Some(if self.expectation.is_some() {
                0
            } else {
                self.rngs.combat_energy_costs.below(4) as i8
            });
        }
        card.turn_flags |= draw_flags;
        let combat = self.combat_mut().unwrap();
        let hellraiser = combat.player.power(power_id::HELLRAISER) > 0
            && content.cards[card.id as usize].tags & STRIKE_TAG != 0;
        if !hellraiser {
            combat.hand.push(card);
        }
        combat.drawn = combat.drawn.saturating_add(1);
        if !hand_draw {
            combat.history.extra_drawn = combat.history.extra_drawn.saturating_add(1);
        }
        if count > 1 {
            combat.queue.push(Pending {
                effect: if hand_draw {
                    Effect::HandDraw(count - 1)
                } else {
                    Effect::Draw(count - 1)
                },
                context,
            });
        }
        let pagestorm = combat
            .player
            .power(power_id::PAGESTORM)
            .clamp(0, u8::MAX as i16) as u8;
        let flags = card.flags(content.cards[card.id as usize]);
        if pagestorm > 0 && flags & ETHEREAL != 0 {
            combat.queue.push(Pending {
                effect: Effect::Draw(pagestorm),
                context,
            });
        }
        let powers = self.creature(Actor::Player).powers.clone();
        self.dispatch(
            content,
            Trigger::CardDrawn,
            Actor::Player,
            Context {
                source: Actor::Player,
                target: None,
                card: Some(card.id),
                upgraded: card.upgrades > 0,
                x: 0,
                event: 1,
                orb: false,
                orb_id: None,
                pen_nib: false,
            },
            powers,
        );
        self.trigger_card(content, card, Trigger::CardDrawn);
        self.tick_automation();
        if hellraiser {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::AutoPlay(card),
                context: Context::card(card),
            });
        }
        Some(card)
    }

    fn tick_automation(&mut self) {
        let blocked = self.creature(Actor::Player).power(power_id::NO_ENERGY_GAIN) > 0;
        let mut energy = 0;
        for automation in &mut self.combat_mut().unwrap().automation {
            automation.0 -= 1;
            if automation.0 == 0 {
                automation.0 = 10;
                energy += automation.1;
            }
        }
        if !blocked {
            self.combat_mut().unwrap().energy =
                self.combat().unwrap().energy.saturating_add(energy);
        }
    }

    fn resolve(&mut self, content: &Content) {
        while self.combat().is_some_and(|x| x.choice.is_none()) {
            let Some(pending) = self.combat_mut().and_then(|x| x.queue.pop()) else {
                break;
            };
            self.apply(content, pending);
        }
    }

    fn apply(&mut self, content: &Content, pending: Pending) {
        self.combat_mut().unwrap().pen_nib = pending.context.pen_nib;
        let mut amount = self.amount(
            content,
            pending.context,
            match pending.effect {
                Effect::Attack(_, x, _)
                | Effect::AttackMany(_, x, _)
                | Effect::OstyAttack(_, x, _)
                | Effect::OstyAttackMany(_, x, _)
                | Effect::MoveDamage(_, x)
                | Effect::Damage(_, x)
                | Effect::LoseHp(_, x)
                | Effect::Block(_, x)
                | Effect::BlockNextTurn(x)
                | Effect::DodgeRoll(x)
                | Effect::DrawBlockIf(_, x)
                | Effect::RawBlock(_, x)
                | Effect::Heal(_, x)
                | Effect::HealPercent(_, x)
                | Effect::MaxHp(x)
                | Effect::Gold(x)
                | Effect::Bomb(x)
                | Effect::Panache(x)
                | Effect::RollingBoulder(x)
                | Effect::ToricToughness(x)
                | Effect::ApplyPower(_, _, x)
                | Effect::ApplyDebuff(_, _, x)
                | Effect::StackPower(_, _, x)
                | Effect::Misery(x)
                | Effect::FlakCannon(x)
                | Effect::TemporaryStrength(_, _, x)
                | Effect::RandomColorless(_, x, _)
                | Effect::RandomColorlessOther(_, x)
                | Effect::RandomCharacter(_, x, _)
                | Effect::RandomCharacterCost0(_, x, _)
                | Effect::RandomCardOp(_, _, _, x)
                | Effect::AutoPlayRandom(_, _, x)
                | Effect::Aggression(x)
                | Effect::AutoPlayDraw(x, _)
                | Effect::DrawAmount(x)
                | Effect::ChooseDraw(x)
                | Effect::Repeat(x, _)
                | Effect::EvokeMany(x)
                | Effect::EvokeLast(x)
                | Effect::GrowCard(x)
                | Effect::GrowDrawn(x)
                | Effect::GrowAll(_, x)
                | Effect::PersistCard(x)
                | Effect::ExhaustForBlock(x)
                | Effect::ExhaustForAttack(_, x)
                | Effect::ExhaustAttackStep(_, x, _)
                | Effect::Stars(x)
                | Effect::Forge(x)
                | Effect::Summon(x) => x,
                Effect::SelectAmount(_, _, x, _, _) => x,
                _ => Amount::fixed(0, 0),
            },
        );
        if pending.context.orb_id == Some(orb_id::LIGHTNING)
            && self.has_relic(content, "RELIC.INFUSED_CORE")
            && matches!(pending.effect, Effect::Damage(..))
        {
            amount = amount.saturating_add(1);
        }
        match pending.effect {
            Effect::Attack(target, _, hits) => {
                let mut total = 0;
                for actor in self.targets(target, pending.context) {
                    let mut lost = 0;
                    for _ in 0..hits {
                        let result = self.damage(
                            content,
                            pending.context.source,
                            actor,
                            amount,
                            DamageKind::Attack,
                            pending.context.card,
                        );
                        lost += result.lost;
                        total += result.resolved;
                    }
                    self.reaper(content, pending.context.source, actor, lost);
                }
                self.combat_mut().unwrap().last_damage = total;
                if pending.context.source == Actor::Player {
                    self.creature_mut(Actor::Player)
                        .powers
                        .retain(|power| power.id != power_id::VIGOR);
                    if self
                        .creature(Actor::Player)
                        .power(power_id::GIGANTIFICATION)
                        > 0
                    {
                        self.creature_mut(Actor::Player)
                            .consume_power(power_id::GIGANTIFICATION);
                    }
                }
            }
            Effect::AttackMany(target, _, hits) => {
                let hits = self.amount(content, pending.context, hits).max(0);
                let mut total = 0;
                for actor in self.targets(target, pending.context) {
                    let mut lost = 0;
                    for _ in 0..hits {
                        let result = self.damage(
                            content,
                            pending.context.source,
                            actor,
                            amount,
                            DamageKind::Attack,
                            pending.context.card,
                        );
                        lost += result.lost;
                        total += result.resolved;
                    }
                    self.reaper(content, pending.context.source, actor, lost);
                }
                self.combat_mut().unwrap().last_damage = total;
                if pending.context.source == Actor::Player {
                    self.creature_mut(Actor::Player)
                        .powers
                        .retain(|power| power.id != power_id::VIGOR);
                    if self
                        .creature(Actor::Player)
                        .power(power_id::GIGANTIFICATION)
                        > 0
                    {
                        self.creature_mut(Actor::Player)
                            .consume_power(power_id::GIGANTIFICATION);
                    }
                }
            }
            Effect::OstyAttack(target, _, hits) => {
                if self.creature(Actor::Osty).hp <= 0 {
                    return;
                }
                self.combat_mut().unwrap().history.osty_attacks += 1;
                for actor in self.targets(target, pending.context) {
                    let mut dealt = 0;
                    for _ in 0..hits {
                        dealt += self
                            .damage(
                                content,
                                Actor::Osty,
                                actor,
                                amount,
                                DamageKind::Attack,
                                pending.context.card,
                            )
                            .lost;
                    }
                    self.reaper(content, Actor::Osty, actor, dealt);
                    self.sic_em(actor);
                }
                if self.has_relic(content, "RELIC.BONE_FLUTE") {
                    self.gain_block(content, Actor::Player, 2, None, false);
                }
            }
            Effect::OstyAttackMany(target, _, hits) => {
                if self.creature(Actor::Osty).hp <= 0 {
                    return;
                }
                let hits = self.amount(content, pending.context, hits).max(0);
                self.combat_mut().unwrap().history.osty_attacks += 1;
                for actor in self.targets(target, pending.context) {
                    let mut dealt = 0;
                    for _ in 0..hits {
                        dealt += self
                            .damage(
                                content,
                                Actor::Osty,
                                actor,
                                amount,
                                DamageKind::Attack,
                                pending.context.card,
                            )
                            .lost;
                    }
                    self.reaper(content, Actor::Osty, actor, dealt);
                    self.sic_em(actor);
                }
                if self.has_relic(content, "RELIC.BONE_FLUTE") {
                    self.gain_block(content, Actor::Player, 2, None, false);
                }
            }
            Effect::MoveDamage(target, _) => {
                for actor in self.targets(target, pending.context) {
                    self.damage(
                        content,
                        pending.context.source,
                        actor,
                        amount,
                        DamageKind::Move,
                        pending.context.card,
                    );
                }
            }
            Effect::Damage(target, _) => {
                let targets = self.targets(target, pending.context);
                for &actor in &targets {
                    self.damage(
                        content,
                        pending.context.source,
                        actor,
                        amount,
                        DamageKind::Unpowered,
                        pending.context.card,
                    );
                }
                if pending.context.orb {
                    self.thunder(content, &targets);
                }
            }
            Effect::Kill(target) => {
                for actor in self.targets(target, pending.context) {
                    self.kill_actor(content, actor);
                }
            }
            Effect::Stun(target) => {
                for actor in self.targets(target, pending.context) {
                    if let Actor::Enemy(index) = actor {
                        self.combat_mut().unwrap().enemies[index].stunned = true;
                    }
                }
            }
            Effect::DoomKill => {
                let enemies = self.targets(Target::AllEnemies, pending.context);
                self.doom_kill(content, enemies);
            }
            Effect::LoseHp(target, _) => {
                for actor in self.targets(target, pending.context) {
                    let before = self.creature(actor).hp.max(0);
                    let lost = amount.max(0).min(before);
                    self.creature_mut(actor).hp = before - lost;
                    if actor == Actor::Player {
                        self.combat_mut().unwrap().history.hp_lost += lost;
                        if lost > 0 {
                            self.combat_mut().unwrap().history.hp_loss_events += 1;
                        }
                    }
                    if lost > 0 {
                        self.trigger(content, Trigger::HpLost, actor, lost);
                        if actor == Actor::Player {
                            self.inferno();
                        }
                        if actor == Actor::Osty {
                            self.necro_mastery(lost);
                        }
                    }
                    if self.creature(actor).hp <= 0 {
                        self.died(content, actor);
                    }
                }
            }
            Effect::Block(target, _) => {
                let targets = self.targets(target, pending.context);
                for &actor in &targets {
                    self.gain_block(content, actor, amount, pending.context.card, true);
                }
                if pending.context.orb {
                    self.thunder(content, &targets);
                }
            }
            Effect::BlockNextTurn(_) => {
                let amount = self.modified_block(content, Actor::Player, amount, false);
                self.apply_power(content, Actor::Player, power_id::BLOCK_NEXT_TURN, amount);
            }
            Effect::ToricToughness(_) => {
                self.creature_mut(Actor::Player).powers.push(Power {
                    id: power_id::TORIC_TOUGHNESS,
                    amount: 2,
                    skip_next_decay: false,
                    value: amount,
                });
            }
            Effect::DodgeRoll(_) => {
                let amount =
                    self.gain_block(content, Actor::Player, amount, pending.context.card, true);
                self.apply_power(content, Actor::Player, power_id::BLOCK_NEXT_TURN, amount);
            }
            Effect::DrawBlockIf(kind, _) => {
                if self
                    .draw(content, 1, pending.context, false, 0)
                    .is_some_and(|card| content.cards[card.id as usize].card_type == kind)
                {
                    self.gain_block(content, Actor::Player, amount, pending.context.card, true);
                }
            }
            Effect::RawBlock(target, _) => {
                for actor in self.targets(target, pending.context) {
                    let amount = amount.max(0);
                    self.creature_mut(actor).block =
                        self.creature(actor).block.saturating_add(amount);
                    if amount > 0 {
                        self.trigger(content, Trigger::BlockGained, actor, amount);
                    }
                }
            }
            Effect::Heal(target, _) => {
                for actor in self.targets(target, pending.context) {
                    let x = self.creature_mut(actor);
                    x.hp = (x.hp + amount.max(0)).min(x.max_hp)
                }
                self.sync_red_skull(content);
            }
            Effect::HealPercent(target, _) => {
                for actor in self.targets(target, pending.context) {
                    let heal = self.creature(actor).max_hp.saturating_mul(amount.max(0)) / 100;
                    let creature = self.creature_mut(actor);
                    creature.hp = creature.hp.saturating_add(heal).min(creature.max_hp);
                }
                self.sync_red_skull(content);
            }
            Effect::MaxHp(_) => {
                if amount < 0 {
                    let max_hp = self
                        .creature(Actor::Player)
                        .max_hp
                        .saturating_add(amount)
                        .max(1);
                    let excess = self.creature(Actor::Player).hp.saturating_sub(max_hp);
                    if excess > 0 {
                        self.damage(
                            content,
                            Actor::Player,
                            Actor::Player,
                            excess,
                            DamageKind::Unpowered,
                            pending.context.card,
                        );
                    }
                    self.creature_mut(Actor::Player).max_hp = max_hp;
                } else {
                    let player = self.creature_mut(Actor::Player);
                    player.max_hp = player.max_hp.saturating_add(amount);
                    player.hp = player.hp.saturating_add(amount).min(player.max_hp);
                }
                self.run.max_hp = self.creature(Actor::Player).max_hp;
                self.run.hp = self.creature(Actor::Player).hp;
                self.sync_red_skull(content);
            }
            Effect::Gold(_) => self.gain_gold(content, amount.max(0) as i32),
            Effect::Bomb(_) => self.combat_mut().unwrap().bombs.push((3, amount.max(0))),
            Effect::Automation(energy) => {
                self.combat_mut().unwrap().automation.push((10, energy));
                self.creature_mut(Actor::Player)
                    .add_power(power_id::AUTOMATION, energy);
            }
            Effect::Panache(_) => {
                let cards = self.combat().unwrap().history.cards;
                self.combat_mut()
                    .unwrap()
                    .panache
                    .push((5, amount.max(0), cards));
                self.creature_mut(Actor::Player)
                    .add_power(power_id::PANACHE, amount);
            }
            Effect::RollingBoulder(_) => {
                self.combat_mut().unwrap().boulders.push(amount.max(0));
                self.creature_mut(Actor::Player)
                    .add_power(power_id::ROLLING_BOULDER, amount);
            }
            Effect::RandomPotion => {
                self.expectation_unknown();
                if self.expectation.is_none() {
                    let pool = self.potion_pool(content);
                    let potion = self.random_potions(&pool, 1, true, true).pop();
                    if !self.has_relic(content, "RELIC.SOZU")
                        && let Some(slot) = self.run.potions.iter().position(Option::is_none)
                    {
                        self.run.potions[slot] = potion;
                    }
                }
                self.sync_belt_buckle(content);
            }
            Effect::RandomizeHandCosts => {
                self.expectation_unknown();
                let len = self.combat().unwrap().hand.len();
                for index in 0..len {
                    let card = self.combat().unwrap().hand[index];
                    let def = content.cards[card.id as usize];
                    if def.cost[card.upgrades.min(1) as usize] >= 0 {
                        let cost = if self.expectation.is_some() {
                            0
                        } else {
                            self.rngs.combat_energy_costs.below(4) as i8
                        };
                        self.combat_mut().unwrap().hand[index].cost_override = Some(cost);
                    }
                }
            }
            Effect::ReplayTagged(tag, count) => {
                let combat = self.combat_mut().unwrap();
                for card in combat
                    .draw
                    .iter_mut()
                    .chain(&mut combat.hand)
                    .chain(&mut combat.discard)
                    .chain(&mut combat.exhaust)
                {
                    if content.cards[card.id as usize].tags & tag != 0 {
                        card.replays = card.replays.saturating_add(count);
                    }
                }
            }
            Effect::ChannelSlots(id) => {
                for _ in 0..self.combat().unwrap().orb_slots {
                    self.channel(content, id);
                }
            }
            Effect::DistinctColorless(pile, count, upgraded) => {
                self.expectation_unknown();
                let mut pool = content.colorless.clone();
                pool.retain(|&id| {
                    !matches!(
                        content.cards[id as usize].id,
                        "CARD.ALCHEMIZE" | "CARD.HAND_OF_GREED" | "CARD.HIDDEN_GEM"
                    )
                });
                pool.sort_unstable_by_key(|&id| content.cards[id as usize].id);
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                for id in pool.into_iter().take(count as usize) {
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            upgrades: upgraded as u8,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::ApplyPower(target, id, _) => {
                for actor in self.targets(target, pending.context) {
                    self.apply_power(content, actor, id, amount);
                    if id == power_id::CONSTRICT
                        && let Actor::Enemy(source) = pending.context.source
                        && let Some(power) = self
                            .creature_mut(actor)
                            .powers
                            .iter_mut()
                            .find(|power| power.id == id)
                    {
                        power.value = source as i16 + 1;
                    }
                }
            }
            Effect::ApplyDebuff(target, id, _) => {
                for actor in self.targets(target, pending.context) {
                    if self.apply_debuff(content, actor, id, amount)
                        && let Actor::Enemy(source) = pending.context.source
                        && matches!(actor, Actor::Player)
                    {
                        let possess = if id == power_id::STRENGTH {
                            power_id::POSSESS_STRENGTH
                        } else if id == power_id::DEXTERITY {
                            power_id::POSSESS_SPEED
                        } else {
                            continue;
                        };
                        if let Some(power) = self.combat_mut().unwrap().enemies[source]
                            .creature
                            .powers
                            .iter_mut()
                            .find(|power| power.id == possess)
                        {
                            power.value = power.value.saturating_sub(amount);
                        }
                    }
                }
            }
            Effect::StackPower(target, id, _) => {
                for actor in self.targets(target, pending.context) {
                    self.creature_mut(actor).add_power(id, amount);
                }
            }
            Effect::DoublePower(target, id, minimum) => {
                for actor in self.targets(target, pending.context) {
                    let amount = self.creature(actor).power(id).max(minimum);
                    self.apply_debuff(content, actor, id, amount);
                }
            }
            Effect::Misery(_) => {
                let Some(target) = pending.context.target else {
                    return;
                };
                let target = Actor::Enemy(target);
                let debuffs: Vec<_> = self
                    .creature(target)
                    .powers
                    .iter()
                    .copied()
                    .filter(|power| {
                        content.powers[power.id as usize].debuff
                            || power.id == power_id::STRENGTH && power.amount < 0
                    })
                    .collect();
                self.damage(
                    content,
                    Actor::Player,
                    target,
                    amount,
                    DamageKind::Attack,
                    pending.context.card,
                );
                let others: Vec<_> = self
                    .combat()
                    .unwrap()
                    .enemies
                    .iter()
                    .enumerate()
                    .filter(|(index, enemy)| {
                        Actor::Enemy(*index) != target && enemy.creature.hp > 0
                    })
                    .map(|(index, _)| Actor::Enemy(index))
                    .collect();
                for actor in others {
                    for power in &debuffs {
                        if matches!(power.id, power_id::ENFEEBLING_TOUCH | power_id::MANGLE) {
                            if let Some(artifact) = self
                                .creature(actor)
                                .powers
                                .iter()
                                .find(|x| content.powers[x.id as usize].kind == PowerKind::Artifact)
                                .copied()
                            {
                                self.creature_mut(actor).consume_power(artifact.id);
                            } else {
                                self.creature_mut(actor).add_power(power.id, power.amount);
                            }
                        } else {
                            self.apply_debuff(content, actor, power.id, power.amount);
                        }
                    }
                }
            }
            Effect::RemovePower(target, id) => {
                for actor in self.targets(target, pending.context) {
                    self.creature_mut(actor).powers.retain(|x| x.id != id)
                }
            }
            Effect::TemporaryStrength(target, restore, _) => {
                for actor in self.targets(target, pending.context) {
                    if self.apply_debuff(content, actor, power_id::STRENGTH, -amount) {
                        self.creature_mut(actor).add_power(restore, amount);
                    }
                }
            }
            Effect::Draw(count) => {
                self.draw(content, count, pending.context, false, 0);
            }
            Effect::HandDraw(count) => {
                self.draw(content, count, pending.context, true, 0);
            }
            Effect::DrawAmount(_) => {
                self.draw(
                    content,
                    amount.clamp(0, u8::MAX as i16) as u8,
                    pending.context,
                    false,
                    0,
                );
            }
            Effect::DrawTo(size) => {
                let count = size[pending.context.upgraded as usize]
                    .saturating_sub(self.combat().unwrap().hand.len() as u8);
                self.draw(content, count, pending.context, false, 0);
            }
            Effect::DrawUntilNot(kind) => {
                let base = self.combat().unwrap().queue.len();
                if self
                    .draw(content, 1, pending.context, false, 0)
                    .is_some_and(|card| content.cards[card.id as usize].card_type == kind)
                {
                    self.combat_mut().unwrap().queue.insert(
                        base,
                        Pending {
                            effect: Effect::DrawUntilNot(kind),
                            context: pending.context,
                        },
                    );
                }
            }
            Effect::ChooseDraw(_) => {
                let count = amount.clamp(0, u8::MAX as i16) as u8;
                if self.combat().unwrap().draw.len() < count as usize
                    && !self.combat().unwrap().discard.is_empty()
                {
                    let combat = self.combat_mut().unwrap();
                    combat.draw.append(&mut combat.discard);
                    self.shuffle_draw();
                    self.trigger(content, Trigger::Shuffle, Actor::Player, 0);
                }
                self.select(
                    content,
                    Pile::Draw,
                    CardFilter::Any,
                    CardOp::Move(Pile::Hand),
                    count,
                    false,
                    false,
                );
            }
            Effect::FreeHand => {
                for card in &mut self.combat_mut().unwrap().hand {
                    card.free = true;
                }
            }
            Effect::Energy(amount) => {
                if amount > 0 && self.creature(Actor::Player).power(power_id::NO_ENERGY_GAIN) > 0 {
                    return;
                }
                self.combat_mut().unwrap().energy =
                    self.combat().unwrap().energy.saturating_add(amount as i16);
                if pending.context.orb {
                    self.thunder(content, &[Actor::Player]);
                }
            }
            Effect::DoubleEnergy => {
                let combat = self.combat_mut().unwrap();
                combat.energy = combat.energy.saturating_mul(2);
            }
            Effect::AddCard(pile, id, count) => {
                let value = if id == card_id::WITHER {
                    let combat = self.combat().unwrap();
                    combat
                        .draw
                        .iter()
                        .chain(&combat.hand)
                        .chain(&combat.discard)
                        .chain(&combat.exhaust)
                        .filter(|card| card.id == id)
                        .map(|card| card.value)
                        .max()
                        .unwrap_or_default()
                } else {
                    0
                };
                for _ in 0..count {
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            value,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::AddUpgradedCard(pile, id, count) => {
                for _ in 0..count {
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            upgrades: 1,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::AddFlaggedCard(pile, id, count, flags) => {
                for _ in 0..count {
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            flags,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::RandomCard(pile, kind, count, free) => {
                self.expectation_unknown();
                let pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let def = content.cards[id as usize];
                        def.card_type == kind
                            && matches!(
                                def.rarity,
                                CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                            )
                            && def.flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                for _ in 0..count {
                    if pool.is_empty() {
                        break;
                    }
                    let id = if self.expectation.is_some() {
                        pool[0]
                    } else {
                        pool[self.rngs.combat_card_generation.below(pool.len() as u32) as usize]
                    };
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            free,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::RandomColorless(pile, _, upgrade) => {
                self.expectation_unknown();
                let mut pool: Vec<_> = content
                    .colorless
                    .iter()
                    .copied()
                    .filter(|&id| content.cards[id as usize].flags[0] & NO_GENERATE == 0)
                    .collect();
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(amount.max(0) as usize);
                for id in pool {
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            upgrades: (upgrade && pending.context.upgraded) as u8,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::RandomColorlessOther(pile, _) => {
                self.expectation_unknown();
                let mut pool: Vec<_> = content
                    .colorless
                    .iter()
                    .copied()
                    .filter(|&id| {
                        Some(id) != pending.context.card
                            && content.cards[id as usize].flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                pool.sort_by_key(|&id| content.cards[id as usize].id);
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(amount.max(0) as usize);
                for id in pool {
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::OfferColorless(count, upgrade, free) => {
                self.expectation_unknown();
                let mut pool: Vec<_> = content
                    .colorless
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let card = content.cards[id as usize];
                        matches!(
                            card.rarity,
                            CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                        ) && card.flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                pool.sort_by_key(|&id| content.cards[id as usize].id);
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(count as usize);
                self.combat_mut().unwrap().offer = pool
                    .into_iter()
                    .map(|id| Card {
                        id,
                        upgrades: (upgrade && pending.context.upgraded) as u8,
                        free,
                        ..Card::default()
                    })
                    .collect();
                self.select(
                    content,
                    Pile::Offer,
                    CardFilter::Any,
                    CardOp::TakeOffer,
                    1,
                    false,
                    true,
                );
            }
            Effect::OfferCharacter(count, free) => {
                self.expectation_unknown();
                let mut pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let card = content.cards[id as usize];
                        matches!(
                            card.rarity,
                            CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                        ) && card.flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                pool.sort_by_key(|&id| content.cards[id as usize].id);
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(count as usize);
                self.combat_mut().unwrap().offer = pool
                    .into_iter()
                    .map(|id| Card {
                        id,
                        free,
                        ..Card::default()
                    })
                    .collect();
                self.select(
                    content,
                    Pile::Offer,
                    CardFilter::Any,
                    CardOp::TakeOffer,
                    1,
                    false,
                    true,
                );
            }
            Effect::OfferCharacterRetain(count) => {
                self.expectation_unknown();
                let mut pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let card = content.cards[id as usize];
                        matches!(
                            card.rarity,
                            CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                        ) && card.flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                pool.sort_by_key(|&id| content.cards[id as usize].id);
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(count as usize);
                self.combat_mut().unwrap().offer = pool
                    .into_iter()
                    .map(|id| Card {
                        id,
                        flags: RETAIN,
                        ..Card::default()
                    })
                    .collect();
                self.select(
                    content,
                    Pile::Offer,
                    CardFilter::Any,
                    CardOp::TakeOffer,
                    1,
                    false,
                    false,
                );
            }
            Effect::OfferCharacterType(kind, count) => {
                self.expectation_unknown();
                let mut pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let def = content.cards[id as usize];
                        def.card_type == kind
                            && matches!(
                                def.rarity,
                                CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                            )
                            && def.flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                pool.sort_by_key(|&id| content.cards[id as usize].id);
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(count as usize);
                self.combat_mut().unwrap().offer = pool
                    .into_iter()
                    .map(|id| Card {
                        id,
                        free: true,
                        ..Card::default()
                    })
                    .collect();
                self.select(
                    content,
                    Pile::Offer,
                    CardFilter::Any,
                    CardOp::TakeOffer,
                    1,
                    false,
                    true,
                );
            }
            Effect::OfferOtherCharacter(kind, count, upgrade) => {
                self.expectation_unknown();
                let mut pool: Vec<_> = [0, 2, 3, 4, 1]
                    .into_iter()
                    .filter(|&index| index != self.run.character as usize)
                    .flat_map(|index| {
                        let mut cards = content.characters[index].cards.to_vec();
                        cards.sort_by_key(|&id| content.cards[id as usize].id);
                        cards
                    })
                    .filter(|&id| {
                        let def = content.cards[id as usize];
                        def.card_type == kind
                            && matches!(
                                def.rarity,
                                CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                            )
                            && def.flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(count as usize);
                self.combat_mut().unwrap().offer = pool
                    .into_iter()
                    .map(|id| Card {
                        id,
                        upgrades: (upgrade && pending.context.upgraded) as u8,
                        free: true,
                        ..Card::default()
                    })
                    .collect();
                self.select(
                    content,
                    Pile::Offer,
                    CardFilter::Any,
                    CardOp::TakeOffer,
                    1,
                    false,
                    true,
                );
            }
            Effect::RandomCharacter(pile, _, flags) => {
                self.expectation_unknown();
                let pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let card = content.cards[id as usize];
                        card.flags[0] & NO_GENERATE == 0
                            && !matches!(card.rarity, CardRarity::Basic | CardRarity::Ancient)
                    })
                    .collect();
                let mut pool = pool;
                pool.sort_by_key(|&id| content.cards[id as usize].id);
                for _ in 0..amount.max(0) {
                    if pool.is_empty() {
                        break;
                    }
                    let id = if self.expectation.is_some() {
                        pool[0]
                    } else {
                        pool[self.rngs.combat_card_generation.below(pool.len() as u32) as usize]
                    };
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            flags,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::DistinctCharacter(pile, count, free) => {
                self.expectation_unknown();
                let mut pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let card = content.cards[id as usize];
                        card.flags[0] & NO_GENERATE == 0
                            && matches!(
                                card.rarity,
                                CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                            )
                    })
                    .collect();
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                for id in pool.into_iter().take(count as usize) {
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            free,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::RandomCharacterCost0(pile, _, upgrade) => {
                self.expectation_unknown();
                let pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let def = content.cards[id as usize];
                        def.cost[0] == 0 && def.flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                for _ in 0..amount.max(0) {
                    if pool.is_empty() {
                        break;
                    }
                    let id = if self.expectation.is_some() {
                        pool[0]
                    } else {
                        pool[self.rngs.combat_card_generation.below(pool.len() as u32) as usize]
                    };
                    self.add_generated(
                        content,
                        pile,
                        Card {
                            id,
                            upgrades: (upgrade && pending.context.upgraded) as u8,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::RandomCardOp(pile, filter, op, _) => {
                let mut candidates: Vec<_> = cards(self.combat().unwrap(), pile)
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| eligible(content, card, filter, op))
                    .map(|(index, _)| index)
                    .collect();
                let count = amount.max(0) as usize;
                if self.expectation.is_some() && count < candidates.len() {
                    let mut selected = Vec::new();
                    for _ in 0..count {
                        let choice = self.expectation_choice(candidates.len()).unwrap_or(0);
                        selected.push(candidates.swap_remove(choice));
                    }
                    candidates = selected;
                } else if count == 1 && !candidates.is_empty() {
                    let selected = self
                        .rngs
                        .combat_card_selection
                        .below(candidates.len() as u32) as usize;
                    candidates = vec![candidates[selected]];
                } else {
                    if self.expectation.is_none() {
                        self.rngs.combat_card_selection.shuffle(&mut candidates);
                    }
                    candidates.truncate(count);
                }
                candidates.sort_unstable_by(|a, b| b.cmp(a));
                for index in candidates {
                    self.apply_card_op(content, pile, index, op);
                }
            }
            Effect::AutoPlayRandom(pile, filter, _) => {
                let mut candidates: Vec<_> = cards(self.combat().unwrap(), pile)
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| eligible(content, card, filter, CardOp::AutoPlay(1)))
                    .map(|(index, card)| (index, *card))
                    .collect();
                if candidates.is_empty() && filter == CardFilter::PlayableOrAny {
                    candidates = cards(self.combat().unwrap(), pile)
                        .iter()
                        .copied()
                        .enumerate()
                        .collect();
                }
                let count = amount.max(0) as usize;
                if self.expectation.is_some() {
                    let mut selected = Vec::new();
                    for _ in 0..count.min(candidates.len()) {
                        let choice = self.expectation_choice(candidates.len()).unwrap_or(0);
                        selected.push(candidates.swap_remove(choice));
                    }
                    candidates = selected;
                } else {
                    self.rngs.combat_card_selection.shuffle(&mut candidates);
                    candidates.truncate(count);
                }
                let selected: Vec<_> = candidates.iter().map(|(_, card)| *card).collect();
                candidates.sort_unstable_by_key(|candidate| std::cmp::Reverse(candidate.0));
                for (index, _) in candidates {
                    self.remove_pile(pile, index);
                }
                for card in selected.into_iter().rev() {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlay(card),
                        context: Context::card(card),
                    });
                }
            }
            Effect::ChooseRandomDraw(count) => {
                self.expectation_unknown();
                let mut candidates: Vec<_> = (0..self.combat().unwrap().draw.len()).collect();
                self.rngs.combat_card_selection.shuffle(&mut candidates);
                candidates.truncate(count as usize);
                for index in candidates {
                    self.combat_mut().unwrap().draw[index].turn_flags |= FETCHED;
                }
                self.select(
                    content,
                    Pile::Draw,
                    CardFilter::Flag(FETCHED),
                    CardOp::TakeFetched,
                    1,
                    false,
                    false,
                );
            }
            Effect::AddRandomCard(id, count, upgrade) => {
                for _ in 0..count[pending.context.upgraded as usize] {
                    self.add_random(
                        content,
                        Pile::Draw,
                        Card {
                            id,
                            upgrades: (upgrade && pending.context.upgraded) as u8,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::AddRandom(pile, id, count) => {
                for _ in 0..count {
                    self.add_random(
                        content,
                        pile,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::FranticEscape => {
                if let Some(index) = self
                    .combat()
                    .unwrap()
                    .enemies
                    .iter()
                    .position(|enemy| enemy.creature.power(power_id::SANDPIT) != 0)
                {
                    self.apply_power(content, Actor::Enemy(index), power_id::SANDPIT, 1);
                }
            }
            Effect::Aggression(_) => {
                self.expectation_unknown();
                let mut attacks: Vec<_> = self
                    .combat()
                    .unwrap()
                    .discard
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        content.cards[card.id as usize].card_type == CardType::Attack
                    })
                    .map(|(index, _)| index)
                    .collect();
                self.rngs.combat_card_selection.shuffle(&mut attacks);
                attacks.truncate(amount.max(0) as usize);
                attacks.sort_unstable_by(|a, b| b.cmp(a));
                for index in attacks {
                    let mut card = self.combat_mut().unwrap().discard.remove(index);
                    card.upgrades = 1;
                    self.add_to_hand(card);
                }
            }
            Effect::AutoPlayDraw(_, exhaust) => {
                let count = amount.max(0) as usize;
                let mut cards = Vec::new();
                for _ in 0..count {
                    if self.combat().unwrap().draw.is_empty()
                        && !self.combat().unwrap().discard.is_empty()
                    {
                        self.shuffle_discard_into_draw(content);
                        self.trigger(content, Trigger::Shuffle, Actor::Player, 0);
                    }
                    let Some(mut card) = self.take_draw() else {
                        break;
                    };
                    self.expectation_draw();
                    if exhaust {
                        card.flags |= EXHAUST;
                    }
                    cards.push(card);
                }
                for card in cards.into_iter().rev() {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlay(card),
                        context: Context::card(card),
                    });
                }
            }
            Effect::Stoke => {
                self.expectation_unknown();
                let count = self.combat().unwrap().hand.len();
                while !self.combat().unwrap().hand.is_empty() {
                    self.apply_card_op(content, Pile::Hand, 0, CardOp::Move(Pile::Exhaust));
                }
                self.resolve(content);
                let pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let card = content.cards[id as usize];
                        card.flags[0] & NO_GENERATE == 0
                            && !matches!(card.rarity, CardRarity::Basic | CardRarity::Ancient)
                    })
                    .collect();
                let mut pool = pool;
                pool.sort_by_cached_key(|&id| content.cards[id as usize].id.replace('_', " "));
                for _ in 0..count {
                    if pool.is_empty() {
                        break;
                    }
                    let id =
                        pool[self.rngs.combat_card_generation.below(pool.len() as u32) as usize];
                    self.add_generated(
                        content,
                        Pile::Hand,
                        Card {
                            id,
                            upgrades: pending.context.upgraded as u8,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::Stampede => {
                self.expectation_unknown();
                let count = self
                    .creature(Actor::Player)
                    .power(power_id::STAMPEDE)
                    .max(0);
                for _ in 0..count {
                    let attacks: Vec<_> = self
                        .combat()
                        .unwrap()
                        .hand
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            let def = content.cards[card.id as usize];
                            def.card_type == CardType::Attack && card.flags(def) & UNPLAYABLE == 0
                        })
                        .map(|(index, _)| index)
                        .collect();
                    if attacks.is_empty() {
                        break;
                    }
                    let index = attacks[self.rngs.shuffle.below(attacks.len() as u32) as usize];
                    let card = self.combat_mut().unwrap().hand.remove(index);
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlay(card),
                        context: Context::card(card),
                    });
                }
            }
            Effect::ContinueEndTurn => self.continue_end_turn(content),
            Effect::FlakCannon(_) => {
                let base = self.combat().unwrap().queue.len();
                let mut count = 0;
                for pile in [Pile::Draw, Pile::Hand, Pile::Discard] {
                    let mut index = 0;
                    while index < cards(self.combat().unwrap(), pile).len() {
                        let card = cards(self.combat().unwrap(), pile)[index];
                        if content.cards[card.id as usize].card_type == CardType::Status {
                            self.apply_card_op(content, pile, index, CardOp::Move(Pile::Exhaust));
                            count += 1;
                        } else {
                            index += 1;
                        }
                    }
                }
                self.combat_mut().unwrap().queue.insert(
                    base,
                    Pending {
                        effect: Effect::AttackMany(
                            Target::RandomEnemy,
                            Amount::fixed(amount, amount),
                            Amount::fixed(count, count),
                        ),
                        context: pending.context,
                    },
                );
            }
            Effect::CopyCard(pile, count) => {
                let combat = self.combat().unwrap();
                let mut card = combat
                    .auto_plays
                    .last()
                    .map(|play| play.card)
                    .or(combat.playing)
                    .unwrap_or(Card {
                        id: pending.context.card.unwrap(),
                        upgrades: pending.context.upgraded as u8,
                        ..Card::default()
                    });
                card.instance = 0;
                for _ in 0..count {
                    self.add_generated(content, pile, card);
                }
            }
            Effect::Discard(count, random) => self.select(
                content,
                Pile::Hand,
                CardFilter::Any,
                CardOp::Move(Pile::Discard),
                count,
                random,
                false,
            ),
            Effect::DiscardHandDraw => {
                let count = self.combat().unwrap().hand.len().min(u8::MAX as usize) as u8;
                self.discard_hand(content);
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Draw(count),
                    context: pending.context,
                });
            }
            Effect::DiscardHandAdd(id) => {
                let count = self.combat().unwrap().hand.len().min(u8::MAX as usize) as u8;
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::AddCard(Pile::Hand, id, count),
                    context: pending.context,
                });
                self.discard_hand(content);
            }
            Effect::PlayExhaustedShivs => {
                let mut shivs = Vec::new();
                self.combat_mut().unwrap().exhaust.retain(|card| {
                    if content.cards[card.id as usize].tags & SHIV_TAG != 0 {
                        let mut card = *card;
                        if pending.context.upgraded {
                            card.upgrades = 1;
                        }
                        shivs.push(card);
                        false
                    } else {
                        true
                    }
                });
                for card in shivs.into_iter().rev() {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlay(card),
                        context: pending.context,
                    });
                }
            }
            Effect::AutoPlay(card) => self.begin_auto_play(content, card, pending.context),
            Effect::FinishAutoPlay => self.finish_auto_play(content),
            Effect::WhisperingEarring(played) => {
                if self.combat().unwrap().playing.is_some() {
                    self.finish_play(content);
                    self.combat_mut().unwrap().queue.insert(
                        0,
                        Pending {
                            effect: Effect::WhisperingEarring(played),
                            context: pending.context,
                        },
                    );
                } else if played < 13
                    && let Some(Action::Play { hand, target }) = self
                        .actions(content)
                        .into_iter()
                        .find(|action| matches!(action, Action::Play { .. }))
                {
                    let manual_cards = self.combat().unwrap().history.manual_cards;
                    let void_form = self.creature(Actor::Player).power(power_id::VOID_FORM);
                    self.combat_mut().unwrap().history.manual_cards = manual_cards.max(void_form);
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::WhisperingEarring(played + 1),
                        context: pending.context,
                    });
                    self.play(content, hand, target);
                    let plays = self.combat().unwrap().card_plays as i16;
                    self.combat_mut().unwrap().history.manual_cards = manual_cards;
                    self.combat_mut().unwrap().history.manual_plays -= plays;
                }
            }
            Effect::MoveAll(from, filter, to) => {
                let mut index = 0;
                let mut remaining = cards(self.combat().unwrap(), from).len();
                while remaining > 0 {
                    remaining -= 1;
                    let card = &cards(self.combat().unwrap(), from)[index];
                    if eligible(content, &card, filter, CardOp::Move(to)) {
                        self.apply_card_op(content, from, index, CardOp::Move(to));
                    } else {
                        index += 1;
                    }
                }
            }
            Effect::DrawFiltered(count, filter) => {
                let combat = self.combat_mut().unwrap();
                combat.queue.push(Pending {
                    effect: Effect::FinishDrawFiltered(filter),
                    context: pending.context,
                });
                if count > 0 {
                    combat.queue.push(Pending {
                        effect: Effect::DrawFilteredStep(count, filter),
                        context: pending.context,
                    });
                }
            }
            Effect::DrawFilteredStep(count, filter) => {
                let base = self.combat().unwrap().queue.len();
                if self
                    .draw(content, 1, pending.context, false, FILTERED_DRAW)
                    .is_some()
                    && count > 1
                {
                    self.combat_mut().unwrap().queue.insert(
                        base,
                        Pending {
                            effect: Effect::DrawFilteredStep(count - 1, filter),
                            context: pending.context,
                        },
                    );
                }
            }
            Effect::FinishDrawFiltered(filter) => {
                let mut index = 0;
                while index < self.combat().unwrap().hand.len() {
                    let card = &self.combat().unwrap().hand[index];
                    let marked = card.turn_flags & FILTERED_DRAW != 0;
                    let discard =
                        marked && !eligible(content, &card, filter, CardOp::Move(Pile::Discard));
                    if marked {
                        self.combat_mut().unwrap().hand[index].turn_flags &= !FILTERED_DRAW;
                    }
                    if discard {
                        self.apply_card_op(content, Pile::Hand, index, CardOp::Move(Pile::Discard));
                    } else {
                        index += 1;
                    }
                }
                let combat = self.combat_mut().unwrap();
                for pile in [&mut combat.draw, &mut combat.discard, &mut combat.exhaust] {
                    for card in pile {
                        card.turn_flags &= !FILTERED_DRAW;
                    }
                }
            }
            Effect::Exhaust(count, random) => self.select(
                content,
                Pile::Hand,
                CardFilter::Any,
                CardOp::Move(Pile::Exhaust),
                count,
                random,
                false,
            ),
            Effect::ExhaustForBlock(value) => {
                let index =
                    self.combat().unwrap().hand.iter().position(|card| {
                        content.cards[card.id as usize].card_type != CardType::Attack
                    });
                if let Some(index) = index {
                    let combat = self.combat_mut().unwrap();
                    combat.queue.push(Pending {
                        effect: pending.effect,
                        context: pending.context,
                    });
                    combat.queue.push(Pending {
                        effect: Effect::Block(Target::Player, value),
                        context: pending.context,
                    });
                    self.apply_card_op(content, Pile::Hand, index, CardOp::Move(Pile::Exhaust));
                }
            }
            Effect::ExhaustForAttack(target, value) => {
                let count = self.combat().unwrap().hand.len().min(u8::MAX as usize) as u8;
                let mut context = pending.context;
                context.event = count as i16;
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::ExhaustAttackStep(target, value, count),
                    context,
                });
            }
            Effect::ExhaustAttackStep(target, value, remaining) => {
                if remaining > 0 {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::ExhaustAttackStep(target, value, remaining - 1),
                        context: pending.context,
                    });
                    self.apply_card_op(content, Pile::Hand, 0, CardOp::Move(Pile::Exhaust));
                } else {
                    for _ in 0..pending.context.event {
                        self.combat_mut().unwrap().queue.push(Pending {
                            effect: Effect::Attack(target, value, 1),
                            context: pending.context,
                        });
                    }
                }
            }
            Effect::Upgrade(pile, count, random) => self.select(
                content,
                pile,
                CardFilter::Any,
                CardOp::Upgrade,
                count,
                random,
                false,
            ),
            Effect::TransformHand(id, upgrades) => {
                let attacks: Vec<_> = self
                    .combat()
                    .unwrap()
                    .hand
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        content.cards[card.id as usize].card_type == CardType::Attack
                    })
                    .map(|(index, _)| index)
                    .collect();
                for index in attacks {
                    self.combat_mut().unwrap().hand[index] = Card {
                        id,
                        instance: self.next_card,
                        upgrades: upgrades[pending.context.upgraded as usize],
                        ..Card::default()
                    };
                    self.next_card = self.next_card.saturating_add(1);
                }
            }
            Effect::If(condition, yes, no) => {
                let effects = if self.condition(content, pending.context, condition) {
                    yes
                } else {
                    no
                };
                push_effects(
                    &mut self.combat_mut().unwrap().queue,
                    effects,
                    pending.context,
                );
            }
            Effect::Repeat(_, effects) => {
                for _ in 0..amount.max(0) {
                    push_effects(
                        &mut self.combat_mut().unwrap().queue,
                        effects,
                        pending.context,
                    )
                }
            }
            Effect::Random(count, effects) => {
                for _ in 0..count {
                    if !effects.is_empty() {
                        let i = if self.expectation.is_some() {
                            self.expectation_choice(effects.len()).unwrap_or(0)
                        } else {
                            self.rngs.niche.below(effects.len() as u32) as usize
                        };
                        self.combat_mut().unwrap().queue.push(Pending {
                            effect: effects[i],
                            context: pending.context,
                        })
                    }
                }
            }
            Effect::Channel(id, count) => {
                for _ in 0..count {
                    self.channel(content, id)
                }
            }
            Effect::RandomOrb(count) => {
                self.expectation_unknown();
                for _ in 0..count[pending.context.upgraded as usize] {
                    let combat = self.combat().unwrap();
                    if combat.enemies.iter().all(|enemy| enemy.creature.hp <= 0)
                        && combat
                            .enemies
                            .iter()
                            .all(|enemy| enemy.creature.power(power_id::ADAPTABLE) == 0)
                    {
                        break;
                    }
                    let id = self
                        .rngs
                        .combat_orb_generation
                        .below(content.orbs.len() as u32) as Id;
                    self.channel(content, id)
                }
            }
            Effect::Evoke(dequeue) => self.evoke(content, dequeue),
            Effect::EvokeMany(_) => {
                for i in 0..amount.max(0) {
                    self.evoke(content, i == amount - 1)
                }
            }
            Effect::EvokeLast(_) => {
                if amount <= 0 {
                    return;
                }
                let Some(orb) = self.combat_mut().unwrap().orbs.pop() else {
                    return;
                };
                let mut context = pending.context;
                context.event = orb.value;
                context.orb = true;
                context.orb_id = Some(orb.id);
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::EvokeLast(Amount::fixed(amount - 1, amount - 1)),
                    context: pending.context,
                });
                push_effects(
                    &mut self.combat_mut().unwrap().queue,
                    content.orbs[orb.id as usize].evoke,
                    context,
                );
            }
            Effect::EvokeAll(times) => {
                if !self.combat().unwrap().orbs.is_empty() {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::EvokeAll(times),
                        context: pending.context,
                    });
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::EvokeMany(Amount::fixed(times as i16, times as i16)),
                        context: pending.context,
                    });
                }
            }
            Effect::PassiveFirst(count) => {
                for _ in 0..count {
                    self.passive_orb(content, Some(0));
                }
            }
            Effect::PassiveLast(count) => {
                for _ in 0..count {
                    self.passive_orb(content, self.combat().unwrap().orbs.len().checked_sub(1));
                }
            }
            Effect::PassiveAll => {
                for index in (0..self.combat().unwrap().orbs.len()).rev() {
                    self.passive_orb(content, Some(index));
                }
            }
            Effect::OrbSlots(amount) => {
                let combat = self.combat_mut().unwrap();
                combat.orb_slots = (combat.orb_slots as i16 + amount as i16).clamp(0, 10) as u8;
                while self.combat().unwrap().orbs.len() > self.combat().unwrap().orb_slots as usize
                {
                    self.evoke(content, true);
                }
            }
            Effect::Stars(_) => {
                let combat = self.combat_mut().unwrap();
                combat.stars = combat.stars.saturating_add(amount).max(0);
                if amount > 0 {
                    combat.history.stars_gained =
                        combat.history.stars_gained.saturating_add(amount);
                }
                let damage = combat.player.power(power_id::BLACK_HOLE);
                if amount > 0 && damage > 0 {
                    combat.queue.push(Pending {
                        effect: Effect::Damage(Target::AllEnemies, Amount::fixed(damage, damage)),
                        context: pending.context,
                    });
                }
            }
            Effect::Forge(_) => {
                let unexhausted = {
                    let combat = self.combat().unwrap();
                    combat
                        .draw
                        .iter()
                        .chain(&combat.hand)
                        .chain(&combat.discard)
                        .chain(combat.playing.iter())
                        .chain(combat.auto_plays.iter().map(|play| &play.card))
                        .any(|card| card.id == card_id::SOVEREIGN_BLADE)
                };
                if !unexhausted {
                    self.add_generated(
                        content,
                        Pile::Hand,
                        Card {
                            id: card_id::SOVEREIGN_BLADE,
                            ..Card::default()
                        },
                    );
                }
                let combat = self.combat_mut().unwrap();
                for card in combat
                    .draw
                    .iter_mut()
                    .chain(&mut combat.hand)
                    .chain(&mut combat.discard)
                    .chain(&mut combat.exhaust)
                    .chain(combat.playing.iter_mut())
                    .chain(combat.auto_plays.iter_mut().map(|play| &mut play.card))
                    .filter(|card| card.id == card_id::SOVEREIGN_BLADE)
                {
                    card.value = card.value.saturating_add(amount);
                }
            }
            Effect::Summon(_) => {
                let osty = &mut self.combat_mut().unwrap().osty;
                if osty.hp > 0 {
                    osty.max_hp = osty.max_hp.saturating_add(amount.max(0));
                    osty.hp = osty.hp.saturating_add(amount.max(0));
                } else if amount > 0 {
                    osty.max_hp = amount;
                    osty.hp = amount;
                    osty.block = 0;
                }
            }
            Effect::MaxEnergy(amount) => {
                let combat = self.combat_mut().unwrap();
                combat.max_energy = (combat.max_energy + amount as i16).max(0);
            }
            Effect::GrowCard(_) => {
                let instance = {
                    let combat = self.combat_mut().unwrap();
                    combat
                        .auto_plays
                        .last_mut()
                        .map(|play| &mut play.card)
                        .or(combat.playing.as_mut())
                        .and_then(|card| {
                            card.value = card.value.saturating_add(amount);
                            (card.id == card_id::THE_SCYTHE && card.instance != 0)
                                .then_some(card.instance)
                        })
                };
                if let Some(deck) = instance.and_then(|instance| {
                    self.run
                        .deck
                        .iter_mut()
                        .find(|deck| deck.instance == instance)
                }) {
                    deck.value = deck.value.saturating_add(amount);
                }
            }
            Effect::GrowDrawn(_) => {
                let combat = self.combat_mut().unwrap();
                if let Some(card) = combat
                    .hand
                    .iter_mut()
                    .rev()
                    .find(|card| Some(card.id) == pending.context.card)
                {
                    card.value = card.value.saturating_add(amount);
                }
            }
            Effect::GrowAll(id, _) => {
                let combat = self.combat_mut().unwrap();
                for pile in [
                    &mut combat.draw,
                    &mut combat.hand,
                    &mut combat.discard,
                    &mut combat.exhaust,
                ] {
                    for card in pile.iter_mut().filter(|card| card.id == id) {
                        card.value = card.value.saturating_add(amount);
                    }
                }
                if let Some(card) = combat.playing.as_mut().filter(|card| card.id == id) {
                    card.value = card.value.saturating_add(amount);
                }
                for card in combat
                    .auto_plays
                    .iter_mut()
                    .map(|play| &mut play.card)
                    .filter(|card| card.id == id)
                {
                    card.value = card.value.saturating_add(amount);
                }
            }
            Effect::PersistCard(_) => {
                let persisted = {
                    let combat = self.combat_mut().unwrap();
                    combat
                        .auto_plays
                        .last_mut()
                        .map(|play| &mut play.card)
                        .or(combat.playing.as_mut())
                        .and_then(|card| {
                            card.value = card.value.saturating_add(amount);
                            (card.instance != 0).then_some((card.instance, card.value))
                        })
                };
                if let Some((instance, value)) = persisted {
                    if let Some(master) = self
                        .run
                        .deck
                        .iter_mut()
                        .find(|master| master.instance == instance)
                    {
                        master.value = value;
                    }
                }
            }
            Effect::SetCardCost(cost) => {
                let combat = self.combat_mut().unwrap();
                if let Some(card) = combat
                    .auto_plays
                    .last_mut()
                    .map(|play| &mut play.card)
                    .or(combat.playing.as_mut())
                {
                    let base = content.cards[card.id as usize].cost[card.upgrades.min(1) as usize];
                    card.cost_delta = if base < 0 { 0 } else { cost - base };
                }
            }
            Effect::ReduceCardCost(amount) => {
                let combat = self.combat_mut().unwrap();
                if let Some(card) = combat
                    .auto_plays
                    .last_mut()
                    .map(|play| &mut play.card)
                    .or(combat.playing.as_mut())
                {
                    card.cost_delta = card.cost_delta.saturating_sub(amount);
                }
            }
            Effect::CapHandCosts => {
                let combat = self.combat_mut().unwrap();
                for card in &mut combat.hand {
                    let def = content.cards[card.id as usize];
                    if card_cost(*card, def, combat.energy) > 1 {
                        if pending.context.upgraded {
                            card.cost_delta = 1 - def.cost[card.upgrades.min(1) as usize];
                        } else {
                            card.cost_override = Some(1);
                        }
                    }
                }
            }
            Effect::RecycleHand(draw) => {
                let combat = self.combat_mut().unwrap();
                let hand = std::mem::take(&mut combat.hand);
                combat.prepend_draw(hand);
                combat.queue.push(Pending {
                    effect: Effect::Draw(draw[pending.context.upgraded as usize]),
                    context: pending.context,
                });
            }
            Effect::ShuffleHandDraw(count) => {
                let combat = self.combat_mut().unwrap();
                let hand = std::mem::take(&mut combat.hand);
                combat.draw.extend(hand);
                self.shuffle_draw();
                self.trigger(content, Trigger::Shuffle, Actor::Player, 0);
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Draw(count),
                    context: pending.context,
                });
            }
            Effect::FillPotions => {
                let pool = self.potion_pool(content);
                let count = self
                    .run
                    .potions
                    .iter()
                    .filter(|slot| slot.is_none())
                    .count();
                let potions = self.random_potions(&pool, count, false, false);
                if !self.has_relic(content, "RELIC.SOZU") {
                    for (slot, potion) in self
                        .run
                        .potions
                        .iter_mut()
                        .filter(|slot| slot.is_none())
                        .zip(potions)
                    {
                        *slot = Some(potion);
                    }
                }
                self.sync_belt_buckle(content);
            }
            Effect::EndTurn => self.combat_mut().unwrap().force_end = true,
            Effect::Select(pile, filter, count, random, optional, op) => self.select(
                content,
                pile,
                filter,
                op,
                count[pending.context.upgraded as usize],
                random,
                optional,
            ),
            Effect::SelectAmount(pile, filter, _, optional, op) => self.select(
                content,
                pile,
                filter,
                op,
                amount.clamp(0, u8::MAX as i16) as u8,
                false,
                optional,
            ),
        }
    }

    pub(crate) fn incoming_damage_amount(
        &self,
        content: &Content,
        source: Actor,
        base: i16,
        kind: DamageKind,
    ) -> i16 {
        let attack = matches!(kind, DamageKind::Attack);
        let powered = matches!(kind, DamageKind::Attack | DamageKind::Move);
        let target = Actor::Player;
        let mut amount = base.max(0);
        if powered {
            amount =
                amount.saturating_add(self.creature(source).kind(content, PowerKind::Strength));
            if attack {
                amount = amount.saturating_add(self.creature(target).power(power_id::TAINTED));
            }
            if self.creature(target).kind(content, PowerKind::Vulnerable) > 0 {
                let vulnerable = if self.creature(target).power(power_id::DEBILITATE) > 0 {
                    200
                } else {
                    150
                };
                amount = amount.saturating_mul(vulnerable) / 100;
            }
            if self.creature(source).kind(content, PowerKind::Weak) > 0 {
                let multiplier = if self.creature(source).power(power_id::DEBILITATE) > 0 {
                    50
                } else {
                    75
                };
                let paper_krane = 15
                    * (matches!(source, Actor::Enemy(_))
                        && self.has_relic(content, "RELIC.PAPER_KRANE"))
                        as i16;
                amount = amount.saturating_mul(multiplier - paper_krane) / 100;
            }
            if matches!(source, Actor::Enemy(_))
                && self.creature(source).hp <= self.creature(source).power(power_id::DOOM)
                && self.has_relic(content, "RELIC.UNDYING_SIGIL")
            {
                amount /= 2;
            }
            if self.combat().unwrap().diamond_diadem {
                amount /= 2;
            }
            amount = amount.saturating_mul(100 + self.creature(target).power(power_id::SLOW)) / 100;
            for _ in 0..self.creature(source).power(power_id::DOUBLE_DAMAGE).max(0) {
                amount = amount.saturating_mul(2);
            }
            if let Actor::Enemy(index) = source
                && let Some(surrounded) = self
                    .creature(target)
                    .powers
                    .iter()
                    .find(|power| power.id == power_id::SURROUNDED)
            {
                let back = if surrounded.value == 0 {
                    power_id::BACK_ATTACK_LEFT
                } else {
                    power_id::BACK_ATTACK_RIGHT
                };
                if self.creature(Actor::Enemy(index)).power(back) > 0 {
                    amount = amount.saturating_mul(3) / 2;
                }
            }
            if matches!(source, Actor::Enemy(_))
                && self.creature(source).kind(content, PowerKind::Vulnerable) > 0
                && self.creature(target).power(power_id::COLOSSUS) > 0
            {
                amount /= 2;
            }
            if self.creature(source).power(power_id::SHRINK) != 0 {
                amount = amount.saturating_mul(70) / 100;
            }
        }
        if attack && self.creature(target).power(power_id::FLUTTER) > 0 {
            amount /= 2;
        }
        let cap = self.creature(target).power(power_id::HARD_TO_KILL);
        if cap > 0 {
            amount = amount.min(cap);
        }
        if self.creature(target).kind(content, PowerKind::Intangible) > 0 {
            amount = amount.min(1);
        }
        amount
    }

    fn damage(
        &mut self,
        content: &Content,
        source: Actor,
        target: Actor,
        base: i16,
        kind: DamageKind,
        card: Option<Id>,
    ) -> DamageResult {
        if self.creature(target).hp <= 0 {
            return DamageResult::default();
        }
        let attack = matches!(kind, DamageKind::Attack);
        let powered = matches!(kind, DamageKind::Attack | DamageKind::Move);
        if attack
            && source == Actor::Player
            && let Actor::Enemy(index) = target
        {
            self.combat_mut().unwrap().hits[index] += 1;
        }
        let mut amount = base.max(0);
        let played = self
            .combat()
            .unwrap()
            .auto_plays
            .last()
            .map(|play| play.card)
            .or(self.combat().unwrap().playing)
            .filter(|played| card == Some(played.id));
        if powered && source == Actor::Player {
            match played.and_then(|card| card.enchantment.map(|enchantment| (card, enchantment))) {
                Some((_, Enchantment::Corrupted)) => amount = amount.saturating_mul(3) / 2,
                Some((_, Enchantment::Instinct)) => amount = amount.saturating_mul(2),
                Some((card, Enchantment::Sharp)) => {
                    amount = amount.saturating_add(card.enchantment_amount)
                }
                Some((card, Enchantment::Vigorous)) if card.enchantment_value > 0 => {
                    amount = amount.saturating_add(card.enchantment_amount)
                }
                Some((card, Enchantment::Momentum)) => {
                    amount = amount.saturating_add(card.enchantment_value)
                }
                Some((_, Enchantment::TezcatarasEmber)) => amount = amount.saturating_add(3),
                _ => {}
            }
            if let Some(played) = played {
                if played.upgrades > 0 && self.has_relic(content, "RELIC.MINIATURE_CANNON") {
                    amount = amount.saturating_add(3);
                }
                if played.enchantment.is_some() && self.has_relic(content, "RELIC.MYSTIC_LIGHTER") {
                    amount = amount.saturating_add(9);
                }
            }
        }
        if attack {
            if source == Actor::Player
                && card.is_some_and(|id| content.cards[id as usize].tags & SHIV_TAG != 0)
            {
                amount = amount.saturating_add(self.creature(source).power(power_id::ACCURACY));
                if self.combat().unwrap().history.shivs == 0 {
                    amount = amount
                        .saturating_add(self.creature(source).power(power_id::PHANTOM_BLADES));
                }
            }
            if source == Actor::Player
                && played
                    .is_some_and(|card| card.flags(content.cards[card.id as usize]) & INKY != 0)
            {
                amount = amount.saturating_add(1);
            }
            if source == Actor::Player {
                amount = amount.saturating_add(self.creature(source).power(power_id::VIGOR));
            }
            if card == Some(card_id::HANG) {
                amount = amount.saturating_mul(self.creature(target).power(power_id::HANG).max(1));
            }
            if source == Actor::Player && self.combat().unwrap().history.attacks == 1 {
                amount = amount
                    .saturating_mul(100 + self.creature(source).power(power_id::LETHALITY))
                    / 100;
            }
            if source == Actor::Osty {
                amount =
                    amount.saturating_add(self.creature(Actor::Player).power(power_id::CALCIFY));
            }
        }
        if powered {
            if attack
                && matches!(source, Actor::Player | Actor::Osty)
                && card.is_some_and(|id| content.cards[id as usize].tags & STRIKE_TAG != 0)
            {
                amount = amount
                    .saturating_add(3 * self.has_relic(content, "RELIC.STRIKE_DUMMY") as i16)
                    .saturating_add(self.has_relic(content, "RELIC.FAKE_STRIKE_DUMMY") as i16);
            }
            amount =
                amount.saturating_add(self.creature(source).kind(content, PowerKind::Strength));
            if matches!(source, Actor::Player | Actor::Osty)
                && card.is_some_and(|id| content.cards[id as usize].tags & MINION_TAG != 0)
                && self.has_relic(content, "RELIC.VITRUVIAN_MINION")
            {
                amount = amount.saturating_mul(2);
            }
            if attack
                && matches!(source, Actor::Player | Actor::Osty)
                && self.combat().unwrap().pen_nib
                && (self
                    .combat()
                    .unwrap()
                    .auto_plays
                    .last()
                    .is_some_and(|play| card == Some(play.card.id))
                    || self
                        .combat()
                        .unwrap()
                        .playing
                        .is_some_and(|played| card == Some(played.id)))
            {
                amount = amount.saturating_mul(2);
            }
            if attack {
                amount = amount.saturating_add(self.creature(target).power(power_id::TAINTED));
            }
            if self.creature(target).kind(content, PowerKind::Vulnerable) > 0 {
                let vulnerable = if self.creature(target).power(power_id::DEBILITATE) > 0 {
                    200
                } else {
                    150
                };
                let cruelty = if matches!(source, Actor::Player | Actor::Osty)
                    && matches!(target, Actor::Enemy(_))
                {
                    self.creature(Actor::Player).power(power_id::CRUELTY)
                } else {
                    0
                };
                let paper_phrog = 25
                    * (matches!(source, Actor::Player | Actor::Osty)
                        && matches!(target, Actor::Enemy(_))
                        && self.has_relic(content, "RELIC.PAPER_PHROG"))
                        as i16;
                amount = amount.saturating_mul(vulnerable + cruelty + paper_phrog) / 100;
            }
            if self.creature(source).kind(content, PowerKind::Weak) > 0 {
                let multiplier = if self.creature(source).power(power_id::DEBILITATE) > 0 {
                    50
                } else {
                    75
                };
                let paper_krane = 15
                    * (target == Actor::Player
                        && matches!(source, Actor::Enemy(_))
                        && self.has_relic(content, "RELIC.PAPER_KRANE"))
                        as i16;
                amount = amount.saturating_mul(multiplier - paper_krane) / 100;
            }
            if target == Actor::Player
                && matches!(source, Actor::Enemy(_))
                && self.creature(source).hp <= self.creature(source).power(power_id::DOOM)
                && self.has_relic(content, "RELIC.UNDYING_SIGIL")
            {
                amount /= 2;
            }
            if target == Actor::Player && self.combat().unwrap().diamond_diadem {
                amount /= 2;
            }
            amount = amount.saturating_mul(100 + self.creature(target).power(power_id::SLOW)) / 100;
            for _ in 0..self.creature(source).power(power_id::DOUBLE_DAMAGE).max(0) {
                amount = amount.saturating_mul(2);
            }
            if target == Actor::Player
                && let Actor::Enemy(index) = source
                && let Some(surrounded) = self
                    .creature(Actor::Player)
                    .powers
                    .iter()
                    .find(|power| power.id == power_id::SURROUNDED)
            {
                let back = if surrounded.value == 0 {
                    power_id::BACK_ATTACK_LEFT
                } else {
                    power_id::BACK_ATTACK_RIGHT
                };
                if self.creature(Actor::Enemy(index)).power(back) > 0 {
                    amount = amount.saturating_mul(3) / 2;
                }
            }
            if target == Actor::Player
                && matches!(source, Actor::Enemy(_))
                && self.creature(source).kind(content, PowerKind::Vulnerable) > 0
                && self.creature(target).power(power_id::COLOSSUS) > 0
            {
                amount /= 2;
            }
            if self.creature(source).power(power_id::SHRINK) != 0 {
                amount = amount.saturating_mul(70) / 100;
            }
        }
        if attack && self.creature(target).power(power_id::FLUTTER) > 0 {
            amount /= 2;
        }
        let cap = self.creature(target).power(power_id::HARD_TO_KILL);
        if cap > 0 {
            amount = amount.min(cap);
        }
        if attack
            && matches!(source, Actor::Player | Actor::Osty)
            && card.is_some()
            && self.creature(target).power(power_id::SOAR) > 0
        {
            amount /= 2;
        }
        if attack {
            if source == Actor::Player && self.creature(target).kind(content, PowerKind::Weak) > 0 {
                amount =
                    amount.saturating_mul(self.creature(source).power(power_id::TRACKING).max(1));
            }
            if source == Actor::Player
                && card == Some(card_id::SOVEREIGN_BLADE)
                && self.creature(target).power(power_id::CONQUEROR) > 0
            {
                amount = amount.saturating_mul(2);
            }
            if source == Actor::Player
                && card.is_some()
                && self.creature(source).power(power_id::GIGANTIFICATION) > 0
            {
                amount = amount.saturating_mul(3);
            }
            self.trigger(content, Trigger::Attacked, target, amount);
            if source == Actor::Player
                && card.is_some()
                && let Actor::Enemy(index) = target
            {
                let curl = self.creature(target).power(power_id::CURL_UP);
                if curl > 0 {
                    self.creature_mut(target)
                        .powers
                        .retain(|power| power.id != power_id::CURL_UP);
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::RawBlock(Target::Source, Amount::fixed(curl, curl)),
                        context: Context {
                            source: target,
                            target: Some(index),
                            card,
                            upgraded: false,
                            x: 0,
                            event: 0,
                            orb: false,
                            orb_id: None,
                            pen_nib: false,
                        },
                    });
                }
            }
        }
        if self.creature(target).kind(content, PowerKind::Intangible) > 0 {
            amount = amount.min(1);
        }
        if target == Actor::Player && matches!(source, Actor::Enemy(_)) {
            let expected = self.incoming_damage_amount(content, source, base, kind);
            debug_assert_eq!(amount, expected);
        }
        let blocked = if matches!(kind, DamageKind::Unblockable) {
            0
        } else {
            amount.min(self.creature(target).block)
        };
        self.creature_mut(target).block -= blocked;
        if blocked > 0
            && self.creature(target).block == 0
            && matches!(target, Actor::Enemy(_))
            && matches!(source, Actor::Player | Actor::Osty)
            && self.has_relic(content, "RELIC.HAND_DRILL")
        {
            self.apply_power(content, target, power_id::VULNERABLE, 2);
        }
        if blocked > 0
            && self.creature(target).block == 0
            && self.creature(target).power(power_id::BURROWED) > 0
        {
            self.creature_mut(target).consume_power(power_id::BURROWED);
            if let Actor::Enemy(index) = target
                && content.enemies[self.creature(target).id as usize].id == "MONSTER.TUNNELER"
            {
                let enemy = &mut self.combat_mut().unwrap().enemies[index];
                enemy.move_index = 3;
                enemy.last_move = 3;
                enemy.repeats = 1;
            }
        }
        let mut lost = amount - blocked;
        let shell = self.creature(target).power(power_id::HARDENED_SHELL);
        if shell > 0 {
            let received = self
                .creature(target)
                .powers
                .iter()
                .find(|power| power.id == power_id::HARDENED_SHELL)
                .unwrap()
                .value;
            lost = lost.min((shell - received).max(0));
        }
        let hp_target = if attack
            && target == Actor::Player
            && matches!(source, Actor::Enemy(_))
            && self.creature(Actor::Osty).hp > 0
        {
            Actor::Osty
        } else {
            target
        };
        let slippery = lost > 0 && self.creature(hp_target).power(power_id::SLIPPERY) > 0;
        if slippery {
            lost = 1;
        }
        if lost > 0 && self.creature(hp_target).kind(content, PowerKind::Buffer) > 0 {
            let id = self
                .creature(hp_target)
                .powers
                .iter()
                .find(|power| content.powers[power.id as usize].kind == PowerKind::Buffer)
                .unwrap()
                .id;
            self.creature_mut(hp_target).consume_power(id);
            lost = 0;
        }
        if lost > 0 && hp_target == Actor::Player && self.has_relic(content, "RELIC.TUNGSTEN_ROD") {
            lost -= 1;
        }
        if lost > 0
            && hp_target == Actor::Player
            && self.has_relic(content, "RELIC.BEATING_REMNANT")
        {
            lost = lost.min((20 - self.combat().unwrap().history.hp_lost).max(0));
        }
        if attack
            && (1..5).contains(&lost)
            && matches!(source, Actor::Player | Actor::Osty)
            && matches!(hp_target, Actor::Enemy(_))
            && self.has_relic(content, "RELIC.THE_BOOT")
        {
            lost = 5;
        }
        lost = lost.min(self.creature(hp_target).hp.max(0));
        self.creature_mut(hp_target).hp -= lost;
        if lost > 0
            && hp_target == Actor::Player
            && !self.combat().unwrap().enemy_turn
            && self.has_relic(content, "RELIC.DEMON_TONGUE")
            && !self.combat().unwrap().demon_tongue
        {
            self.combat_mut().unwrap().demon_tongue = true;
            self.creature_mut(hp_target).hp =
                (self.creature(hp_target).hp + lost).min(self.creature(hp_target).max_hp);
        }
        self.sync_red_skull(content);
        if lost > 0
            && hp_target == Actor::Player
            && self.has_relic(content, "RELIC.CENTENNIAL_PUZZLE")
            && !self.combat().unwrap().centennial_puzzle
        {
            self.combat_mut().unwrap().centennial_puzzle = true;
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Draw(3),
                context: Context::player(),
            });
        }
        if lost > 0
            && hp_target == Actor::Player
            && self.has_relic(content, "RELIC.SELF_FORMING_CLAY")
        {
            self.apply_power(content, Actor::Player, power_id::BLOCK_NEXT_TURN, 3);
        }
        if lost > 0
            && let Some(power) = self
                .creature_mut(target)
                .powers
                .iter_mut()
                .find(|power| power.id == power_id::HARDENED_SHELL)
        {
            power.value = power.value.saturating_add(lost);
        }
        if lost > 0
            && let Actor::Enemy(index) = hp_target
            && self.creature(hp_target).hp > 0
            && self.creature(hp_target).power(power_id::SHRIEK) > 0
            && self.creature(hp_target).hp <= self.creature(hp_target).power(power_id::SHRIEK)
        {
            self.creature_mut(hp_target)
                .powers
                .retain(|power| power.id != power_id::SHRIEK);
            let enemy = &mut self.combat_mut().unwrap().enemies[index];
            enemy.move_index = 2;
            enemy.last_move = 2;
            enemy.repeats = 1;
            enemy.move_history.push(2);
        }
        if lost > 0
            && let Actor::Enemy(index) = hp_target
            && content.enemies[self.combat().unwrap().enemies[index].creature.id as usize].id
                == "MONSTER.SLUMBERING_BEETLE"
            && self.creature(hp_target).power(power_id::SLUMBER) > 0
        {
            self.creature_mut(hp_target)
                .consume_power(power_id::SLUMBER);
            if self.creature(hp_target).power(power_id::SLUMBER) == 0 {
                self.creature_mut(hp_target)
                    .powers
                    .retain(|power| power.id != 79);
                let enemy = &mut self.combat_mut().unwrap().enemies[index];
                enemy.move_index = 2;
                enemy.last_move = usize::MAX;
                enemy.repeats = 0;
            }
        }
        if lost > 0
            && let Actor::Enemy(index) = hp_target
            && content.enemies[self.combat().unwrap().enemies[index].creature.id as usize].id
                == "MONSTER.LAGAVULIN_MATRIARCH"
            && self.creature(hp_target).power(power_id::ASLEEP) > 0
        {
            self.creature_mut(hp_target)
                .powers
                .retain(|power| !matches!(power.id, 79 | power_id::ASLEEP));
            let enemy = &mut self.combat_mut().unwrap().enemies[index];
            enemy.move_index = 5;
            enemy.last_move = usize::MAX;
            enemy.repeats = 0;
        }
        if lost > 0
            && let Actor::Enemy(index) = hp_target
            && self.creature(hp_target).hp <= self.creature(hp_target).power(power_id::PLOW)
            && self.creature(hp_target).power(power_id::PLOW) > 0
        {
            self.creature_mut(hp_target).powers.retain(|power| {
                !matches!(
                    power.id,
                    power_id::PLOW
                        | power_id::STRENGTH
                        | power_id::FLEX_POTION
                        | power_id::FEEDING_FRENZY
                )
            });
            let enemy = &mut self.combat_mut().unwrap().enemies[index];
            enemy.move_index = 2;
            enemy.last_move = 2;
            enemy.repeats = 1;
            enemy.move_history.push(2);
        }
        if slippery && lost > 0 {
            self.creature_mut(hp_target)
                .consume_power(power_id::SLIPPERY);
        }
        if attack
            && lost > 0
            && matches!(source, Actor::Player | Actor::Osty)
            && card.is_some()
            && self.creature(target).hp > 0
            && let Some(skittish) = self
                .creature_mut(target)
                .powers
                .iter_mut()
                .find(|power| power.id == power_id::SKITTISH && power.value == 0)
        {
            skittish.value = 1;
            let amount = skittish.amount;
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::RawBlock(Target::Source, Amount::fixed(amount, amount)),
                context: Context {
                    source: target,
                    target: match target {
                        Actor::Enemy(index) => Some(index),
                        _ => None,
                    },
                    card,
                    upgraded: false,
                    x: 0,
                    event: 0,
                    orb: false,
                    orb_id: None,
                    pen_nib: false,
                },
            });
        }
        if attack
            && amount > 0
            && blocked == amount
            && let Actor::Enemy(index) = source
            && self.creature(source).power(power_id::IMBALANCED) > 0
        {
            let enemy = &mut self.combat_mut().unwrap().enemies[index];
            if content.enemies[enemy.creature.id as usize].id == "MONSTER.BOWLBUG_ROCK" {
                enemy.move_index = 1;
                enemy.last_move = usize::MAX;
                enemy.repeats = 0;
            }
        }
        if attack && hp_target == Actor::Player && lost > 0 {
            let paper_cuts = self.creature(source).power(power_id::PAPER_CUTS).max(0);
            if paper_cuts > 0 {
                let player = self.creature_mut(Actor::Player);
                player.max_hp = (player.max_hp - paper_cuts).max(1);
                player.hp = player.hp.min(player.max_hp);
                self.run.max_hp = player.max_hp;
            }
            for _ in 0..self.creature(source).power(power_id::PAINFUL_STABS).max(0) {
                self.add_generated(
                    content,
                    Pile::Discard,
                    Card {
                        id: card_id::WOUND,
                        ..Card::default()
                    },
                );
            }
        }
        if attack && lost > 0 && self.creature(target).power(power_id::FLUTTER) > 0 {
            self.creature_mut(target).consume_power(power_id::FLUTTER);
            if self.creature(target).power(power_id::FLUTTER) == 0
                && let Actor::Enemy(index) = target
                && content.enemies[self.creature(target).id as usize].id
                    == "MONSTER.THIEVING_HOPPER"
            {
                let enemy = &mut self.combat_mut().unwrap().enemies[index];
                enemy.move_index = 5;
                enemy.last_move = 5;
                enemy.repeats = 1;
            }
        }
        if hp_target == Actor::Osty && lost > 0 {
            self.necro_mastery(lost);
        }
        if hp_target == Actor::Player {
            self.combat_mut().unwrap().history.hp_lost += lost;
            if lost > 0 {
                self.combat_mut().unwrap().history.hp_loss_events += 1;
                if !matches!(kind, DamageKind::Unblockable) {
                    self.damage_taken = true;
                }
            }
        }
        if lost > 0 {
            self.trigger(content, Trigger::HpLost, hp_target, lost);
            if hp_target == Actor::Player {
                self.inferno();
            }
        }
        if blocked > 0 {
            self.trigger(content, Trigger::Blocked, target, blocked);
        }
        if attack
            && target == Actor::Player
            && matches!(source, Actor::Enemy(_))
            && blocked > 0
            && self.creature(target).power(power_id::REFLECT) > 0
        {
            self.damage(
                content,
                target,
                source,
                blocked,
                DamageKind::Unpowered,
                None,
            );
        }
        if lost > 0 {
            self.trigger(content, Trigger::Damaged, hp_target, lost);
        }
        if attack && lost > 0 {
            let poison = self.creature(source).power(power_id::ENVENOM);
            if poison > 0 {
                self.apply_power(content, hp_target, power_id::POISON, poison);
            }
            let suck = self.creature(source).power(power_id::SUCK);
            if suck > 0 {
                self.apply_power(content, source, power_id::STRENGTH, suck);
            }
        }
        if attack && source == Actor::Player {
            let loss = self.creature(source).power(power_id::MONARCHS_GAZE);
            if loss > 0 && self.apply_debuff(content, target, power_id::STRENGTH, -loss) {
                self.creature_mut(target)
                    .add_power(power_id::RESTORE_STRENGTH, loss);
            }
        }
        if attack && target == Actor::Player && matches!(source, Actor::Enemy(_)) {
            let barrier = self.creature(Actor::Player).power(power_id::FLAME_BARRIER);
            if barrier > 0 {
                self.damage(
                    content,
                    Actor::Player,
                    source,
                    barrier,
                    DamageKind::Unpowered,
                    None,
                );
            }
        }
        if attack {
            let thorns = self.creature(target).kind(content, PowerKind::Thorns);
            if thorns > 0 {
                self.damage(content, target, source, thorns, DamageKind::Unpowered, None);
            }
        }
        if attack && matches!(source, Actor::Player | Actor::Osty) {
            for _ in 0..self.creature(target).power(power_id::PERSONAL_HIVE) {
                self.add_random(
                    content,
                    Pile::Draw,
                    Card {
                        id: card_id::DAZED,
                        ..Card::default()
                    },
                );
            }
        }
        if attack
            && hp_target == Actor::Player
            && matches!(source, Actor::Enemy(_))
            && lost > 0
            && self.creature(Actor::Player).power(power_id::THE_GAMBIT) > 0
        {
            self.creature_mut(Actor::Player)
                .powers
                .retain(|power| power.id != power_id::THE_GAMBIT);
            self.creature_mut(Actor::Player).hp = 0;
        }
        if self.creature(hp_target).hp <= 0 {
            self.died(content, hp_target);
        }
        DamageResult {
            lost,
            resolved: amount,
        }
    }

    fn kill_actor(&mut self, content: &Content, actor: Actor) {
        let lost = self.creature(actor).hp.max(0);
        if lost == 0 {
            return;
        }
        self.creature_mut(actor).hp = 0;
        if actor == Actor::Osty {
            self.necro_mastery(lost);
        }
        self.died(content, actor);
    }

    fn doom_kill(&mut self, content: &Content, actors: Vec<Actor>) {
        let mut killed = 0;
        for actor in actors {
            if self.creature(actor).hp > 0
                && self.creature(actor).hp <= self.creature(actor).power(power_id::DOOM)
            {
                self.kill_actor(content, actor);
                killed += matches!(actor, Actor::Enemy(_)) as i16;
            }
        }
        if killed > 0 && self.has_relic(content, "RELIC.BOOK_REPAIR_KNIFE") {
            let player = self.creature_mut(Actor::Player);
            player.hp = (player.hp + 3 * killed).min(player.max_hp);
            self.sync_red_skull(content);
        }
    }

    fn died(&mut self, content: &Content, actor: Actor) {
        if actor == Actor::Player
            && let Some(slot) = self
                .run
                .potions
                .iter()
                .position(|potion| *potion == Some(potion_id::FAIRY_IN_A_BOTTLE))
        {
            self.run.potions[slot] = None;
            self.creature_mut(actor).hp = (self.creature(actor).max_hp * 3 / 10).max(1);
            return;
        }
        if actor == Actor::Player
            && !self.lizard_tail
            && self.has_relic(content, "RELIC.LIZARD_TAIL")
        {
            self.lizard_tail = true;
            self.creature_mut(actor).hp = (self.creature(actor).max_hp / 2).max(1);
            return;
        }
        let infested = self.creature(actor).power(power_id::INFESTED);
        let (stock, surprise, stolen, heist) = match actor {
            Actor::Enemy(_) => (
                self.creature(actor).power(power_id::STOCK),
                self.creature(actor).power(power_id::SURPRISE) > 0,
                self.creature(actor)
                    .powers
                    .iter()
                    .filter(|power| power.id == power_id::THIEVERY)
                    .map(|power| power.value)
                    .sum(),
                self.creature(actor).power(power_id::HEIST).max(0) as i32,
            ),
            _ => (0, false, 0, 0),
        };
        if let Actor::Enemy(dead) = actor {
            if heist > 0 {
                self.reward_gold_parts.push(heist);
            }
            let ravenous: Vec<_> = self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .filter(|(index, enemy)| {
                    *index != dead
                        && enemy.creature.hp > 0
                        && enemy.creature.power(power_id::RAVENOUS) > 0
                })
                .map(|(index, _)| index)
                .collect();
            for index in ravenous {
                let amount = self.creature(Actor::Enemy(index)).power(power_id::RAVENOUS);
                self.creature_mut(Actor::Enemy(index))
                    .add_power(power_id::STRENGTH, amount);
                self.combat_mut().unwrap().enemies[index].stunned = true;
            }
            let steam = self.creature(actor).power(power_id::STEAM_ERUPTION);
            if steam > 0
                && content.enemies[self.combat().unwrap().enemies[dead].creature.id as usize].id
                    == "MONSTER.WATERFALL_GIANT"
            {
                let creature = self.creature_mut(actor);
                creature.hp = i16::MAX;
                creature.max_hp = i16::MAX;
                creature
                    .powers
                    .retain(|power| power.id != power_id::STEAM_ERUPTION);
                let enemy = &mut self.combat_mut().unwrap().enemies[dead];
                enemy.move_index = 7;
                enemy.last_move = 7;
                enemy.repeats = 1;
                enemy.move_history.push(7);
                enemy.value = steam;
                return;
            }
            let enraged: Vec<_> = self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .filter(|(index, enemy)| {
                    *index != dead
                        && enemy.creature.hp > 0
                        && enemy.creature.power(power_id::CRAB_RAGE) > 0
                })
                .map(|(index, _)| index)
                .collect();
            for index in enraged {
                let creature = self.creature_mut(Actor::Enemy(index));
                creature.add_power(power_id::STRENGTH, 6);
                creature.block = creature.block.saturating_add(99);
                creature
                    .powers
                    .retain(|power| power.id != power_id::CRAB_RAGE);
            }
        }
        if let Actor::Enemy(index) = actor
            && self.creature(actor).power(power_id::ADAPTABLE) > 0
            && content.enemies[self.combat().unwrap().enemies[index].creature.id as usize].id
                == "MONSTER.TEST_SUBJECT"
        {
            let creature = &mut self.combat_mut().unwrap().enemies[index].creature;
            creature.block = 0;
            creature
                .powers
                .retain(|power| matches!(power.id, power_id::ADAPTABLE | power_id::PAINFUL_STABS));
            let enemy = &mut self.combat_mut().unwrap().enemies[index];
            enemy.move_index = 0;
            enemy.last_move = 0;
            enemy.repeats = 1;
            enemy.move_history.push(0);
        }
        if let Actor::Enemy(index) = actor
            && self.creature(actor).power(power_id::ILLUSION) > 0
        {
            let creature = &mut self.combat_mut().unwrap().enemies[index].creature;
            creature.block = 0;
            creature.powers.retain(|power| {
                !content.powers[power.id as usize].debuff
                    && (power.id != power_id::STRENGTH || power.amount > 0)
            });
            let enemy = &mut self.combat_mut().unwrap().enemies[index];
            enemy.move_index = 1;
            enemy.last_move = 1;
            enemy.repeats = 1;
        }
        if let Actor::Enemy(index) = actor
            && self.creature(actor).power(power_id::REATTACH) > 0
        {
            let revive = self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .any(|(other, enemy)| {
                    other != index
                        && enemy.creature.hp > 0
                        && enemy.creature.power(power_id::REATTACH) > 0
                });
            let creature = &mut self.combat_mut().unwrap().enemies[index].creature;
            creature.block = 0;
            creature
                .powers
                .retain(|power| power.id == power_id::REATTACH);
            if revive {
                let enemy = &mut self.combat_mut().unwrap().enemies[index];
                enemy.move_index = 3;
                enemy.last_move = 3;
                enemy.repeats = 1;
            }
        }
        let combat = self.combat_mut().unwrap();
        for pile in [
            &mut combat.draw,
            &mut combat.hand,
            &mut combat.discard,
            &mut combat.exhaust,
        ] {
            for card in pile
                .iter_mut()
                .filter(|card| card.id == card_id::MELANCHOLY)
            {
                card.cost_delta = card.cost_delta.saturating_sub(1);
            }
        }
        if matches!(actor, Actor::Enemy(_)) {
            if let Actor::Enemy(index) = actor {
                let enemy_id =
                    content.enemies[self.combat().unwrap().enemies[index].creature.id as usize].id;
                if enemy_id == "MONSTER.SPECTRAL_KNIGHT" {
                    self.creature_mut(Actor::Player)
                        .powers
                        .retain(|power| power.id != power_id::HEX);
                } else if enemy_id == "MONSTER.MAGI_KNIGHT"
                    && !self
                        .combat()
                        .unwrap()
                        .enemies
                        .iter()
                        .enumerate()
                        .any(|(other, enemy)| {
                            other != index
                                && enemy.creature.hp > 0
                                && content.enemies[enemy.creature.id as usize].id
                                    == "MONSTER.MAGI_KNIGHT"
                        })
                {
                    let dampened = std::mem::take(&mut self.combat_mut().unwrap().dampened);
                    let combat = self.combat_mut().unwrap();
                    for pile in [
                        &mut combat.draw,
                        &mut combat.hand,
                        &mut combat.discard,
                        &mut combat.exhaust,
                    ] {
                        for card in pile {
                            if let Some((_, upgrades)) = dampened
                                .iter()
                                .find(|(instance, _)| *instance == card.instance)
                            {
                                card.upgrades = *upgrades;
                            }
                        }
                    }
                    combat
                        .player
                        .powers
                        .retain(|power| power.id != power_id::DAMPEN);
                } else if enemy_id == "MONSTER.TORCH_HEAD_AMALGAM" {
                    if let Some(queen) =
                        self.combat_mut().unwrap().enemies.iter_mut().find(|enemy| {
                            enemy.creature.hp > 0
                                && content.enemies[enemy.creature.id as usize].id == "MONSTER.QUEEN"
                        })
                        && queen.move_index == 2
                    {
                        queen.move_index = 5;
                        queen.last_move = 5;
                        queen.repeats = 1;
                    }
                }
                let strength = self.combat().unwrap().enemies[index]
                    .creature
                    .powers
                    .iter()
                    .find(|power| power.id == power_id::POSSESS_STRENGTH)
                    .map_or(0, |power| power.value);
                let dexterity = self.combat().unwrap().enemies[index]
                    .creature
                    .powers
                    .iter()
                    .find(|power| power.id == power_id::POSSESS_SPEED)
                    .map_or(0, |power| power.value);
                self.creature_mut(Actor::Player)
                    .add_power(power_id::STRENGTH, strength);
                self.creature_mut(Actor::Player)
                    .add_power(power_id::DEXTERITY, dexterity);
                self.creature_mut(Actor::Player).powers.retain(|power| {
                    power.id != power_id::CONSTRICT || power.value != index as i16 + 1
                });
            }
            self.trigger(content, Trigger::EnemyDied, actor, 0);
            if self.has_relic(content, "RELIC.GREMLIN_HORN") {
                push_effects(
                    &mut self.combat_mut().unwrap().queue,
                    &[Effect::Energy(1), Effect::Draw(1)],
                    Context::player(),
                );
            }
            if stock > 0 {
                let spawned = self.spawn_enemy(content, "MONSTER.AXEBOT");
                let creature = self.creature_mut(Actor::Enemy(spawned));
                creature.powers.retain(|power| power.id != power_id::STOCK);
                if stock > 1 {
                    creature.add_power(power_id::STOCK, stock - 1);
                }
                let enemy = &mut self.combat_mut().unwrap().enemies[spawned];
                enemy.move_index = 0;
                enemy.last_move = usize::MAX;
                enemy.repeats = 0;
                enemy.move_history.clear();
            }
            if surprise {
                self.spawn_enemy(content, "MONSTER.SNEAKY_GREMLIN");
                let fat = self.spawn_enemy(content, "MONSTER.FAT_GREMLIN");
                if stolen > 0 {
                    self.creature_mut(Actor::Enemy(fat))
                        .add_power(power_id::HEIST, stolen);
                }
            }
            if infested > 0 {
                self.spawn_wrigglers(content, infested);
            }
            if let Some(target) = self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .position(|enemy| enemy.creature.hp > 0)
            {
                self.face_target(Some(target));
            }
        }
    }

    pub(crate) fn spawn_wrigglers(&mut self, content: &Content, count: i16) {
        let id = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.WRIGGLER")
            .unwrap() as Id;
        let range = content.enemies[id as usize].hp(self.run.ascension);
        for _ in 0..count {
            let mut instance = 2;
            while self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .any(|enemy| enemy.instance == instance)
            {
                instance += 1;
            }
            let candidates: Vec<_> = range
                .clone()
                .filter(|hp| {
                    self.combat()
                        .unwrap()
                        .enemies
                        .iter()
                        .all(|enemy| enemy.creature.max_hp != *hp)
                })
                .collect();
            let hp = if candidates.is_empty() {
                self.rngs.niche.range(*range.start(), *range.end())
            } else {
                candidates[self.rngs.niche.below(candidates.len() as u32) as usize]
            };
            let fur_coat = self.fur_coat_active();
            self.insert_enemy(Enemy {
                instance,
                creature: Creature {
                    id,
                    hp: if fur_coat { 1 } else { hp },
                    max_hp: hp,
                    block: 0,
                    powers: vec![],
                },
                move_index: 0,
                last_move: 0,
                repeats: 1,
                move_history: vec![0],
                stunned: false,
                value: 0,
            });
        }
    }

    fn spawn_enemy(&mut self, content: &Content, name: &str) -> usize {
        let id = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == name)
            .unwrap() as Id;
        let def = &content.enemies[id as usize];
        let range = def.hp(self.run.ascension);
        let hp = self.rngs.niche.range(*range.start(), *range.end());
        let instance = self
            .combat()
            .unwrap()
            .enemies
            .iter()
            .map(|enemy| enemy.instance)
            .max()
            .unwrap_or_default()
            + 1;
        let powers = def
            .powers(self.run.ascension)
            .into_iter()
            .map(|(id, amount)| Power {
                id,
                amount,
                skip_next_decay: false,
                value: 0,
            })
            .collect();
        let fur_coat = self.fur_coat_active();
        let enemy = Enemy {
            instance,
            creature: Creature {
                id,
                hp: if fur_coat { 1 } else { hp },
                max_hp: hp,
                block: 0,
                powers,
            },
            move_index: def.start,
            last_move: usize::MAX,
            repeats: 0,
            move_history: vec![],
            stunned: false,
            value: 0,
        };
        self.insert_enemy(enemy)
    }

    fn insert_enemy(&mut self, enemy: Enemy) -> usize {
        let combat = self.combat_mut().unwrap();
        let index = combat
            .enemies
            .iter()
            .position(|enemy| {
                enemy.creature.hp <= 0
                    && enemy.creature.power(power_id::ADAPTABLE) == 0
                    && enemy.creature.power(power_id::ILLUSION) == 0
                    && enemy.creature.power(power_id::REATTACH) == 0
            })
            .unwrap_or(combat.enemies.len());
        if index < combat.enemies.len() {
            combat.enemies[index] = enemy;
        } else {
            combat.enemies.push(enemy);
        }
        combat.hits.resize(combat.enemies.len(), 0);
        combat.hits[index] = 0;
        if !combat.enemy_power_snapshot.is_empty() {
            combat
                .enemy_power_snapshot
                .resize(combat.enemies.len(), vec![]);
            combat.enemy_power_snapshot[index].clear();
        }
        index
    }

    fn fur_coat_active(&self) -> bool {
        self.fur_coat_act == Some(self.run.act)
            && self.map.current.is_some_and(|current| {
                let node = &self.map.nodes[current];
                self.fur_coat.contains(&(node.lane, node.floor))
            })
    }

    fn necro_mastery(&mut self, lost: i16) {
        let amount =
            lost.saturating_mul(self.creature(Actor::Player).power(power_id::NECRO_MASTERY));
        if amount > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::LoseHp(Target::AllEnemies, Amount::fixed(amount, amount)),
                context: Context::player(),
            });
        }
    }

    fn reaper(&mut self, content: &Content, source: Actor, target: Actor, dealt: i16) {
        if matches!(source, Actor::Player | Actor::Osty) && dealt > 0 {
            let doom =
                dealt.saturating_mul(self.creature(Actor::Player).power(power_id::REAPER_FORM));
            self.apply_power(content, target, power_id::DOOM, doom);
        }
    }

    fn sic_em(&mut self, target: Actor) {
        let amount = self.creature(target).power(power_id::SIC_EM);
        if amount > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Summon(Amount::fixed(amount, amount)),
                context: Context::player(),
            });
        }
    }

    fn inferno(&mut self) {
        let amount = self.creature(Actor::Player).power(power_id::INFERNO);
        if amount > 0 && !self.combat().unwrap().enemy_turn {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(amount, amount)),
                context: Context::player(),
            });
        }
    }

    fn sync_red_skull(&mut self, content: &Content) {
        if self.combat().is_none() || !self.has_relic(content, "RELIC.RED_SKULL") {
            return;
        }
        let active = self.creature(Actor::Player).hp * 2 <= self.creature(Actor::Player).max_hp;
        if active != self.combat().unwrap().red_skull {
            self.combat_mut().unwrap().red_skull = active;
            self.apply_power(
                content,
                Actor::Player,
                power_id::STRENGTH,
                if active { 3 } else { -3 },
            );
        }
    }

    fn sync_belt_buckle(&mut self, content: &Content) {
        if self.combat().is_none() || !self.has_relic(content, "RELIC.BELT_BUCKLE") {
            return;
        }
        let active = self.run.potions.iter().all(Option::is_none);
        if active != self.combat().unwrap().belt_buckle {
            self.combat_mut().unwrap().belt_buckle = active;
            self.apply_power(
                content,
                Actor::Player,
                power_id::DEXTERITY,
                if active { 2 } else { -2 },
            );
        }
    }

    fn thunder(&mut self, content: &Content, targets: &[Actor]) {
        let amount = self.creature(Actor::Player).power(power_id::THUNDER);
        for &target in targets {
            self.damage(
                content,
                Actor::Player,
                target,
                amount,
                DamageKind::Unpowered,
                None,
            );
        }
    }

    fn gain_block(
        &mut self,
        content: &Content,
        actor: Actor,
        base: i16,
        card: Option<Id>,
        powered: bool,
    ) -> i16 {
        if actor == Actor::Player
            && card.is_some()
            && self.creature(actor).power(power_id::NO_BLOCK) > 0
        {
            return 0;
        }
        let fasten = card
            .filter(|&id| content.cards[id as usize].tags & DEFEND_TAG != 0)
            .map_or(0, |_| self.creature(actor).power(power_id::FASTEN));
        let enchantment = (actor == Actor::Player)
            .then(|| {
                self.combat()
                    .unwrap()
                    .auto_plays
                    .last()
                    .map(|play| play.card)
                    .or(self.combat().unwrap().playing)
            })
            .flatten()
            .filter(|played| card == Some(played.id))
            .map_or(0, |card| match card.enchantment {
                Some(Enchantment::Nimble) => card.enchantment_amount,
                Some(Enchantment::Goopy) => card.enchantment_amount.saturating_sub(1),
                _ => 0,
            });
        let mut amount = self.modified_block(
            content,
            actor,
            base.saturating_add(fasten).saturating_add(enchantment),
            powered,
        );
        if actor == Actor::Player
            && card.is_some_and(|id| content.cards[id as usize].tags & MINION_TAG != 0)
            && self.has_relic(content, "RELIC.VITRUVIAN_MINION")
        {
            amount = amount.saturating_mul(2);
        }
        let combat = self.combat().unwrap();
        let prior_card_blocks = combat.history.block_gains
            - if combat.history.block_card == combat.history.cards {
                combat.history.block_card_gains
            } else {
                0
            };
        if actor == Actor::Player
            && card.is_some()
            && prior_card_blocks == 0
            && self.has_relic(content, "RELIC.VAMBRACE")
        {
            amount = amount.saturating_mul(2);
        }
        if actor == Actor::Player
            && card.is_some()
            && combat.paels_legion == 0
            && self.has_relic(content, "RELIC.PAELS_LEGION")
        {
            amount = amount.saturating_mul(2);
        }
        self.creature_mut(actor).block = self.creature(actor).block.saturating_add(amount);
        if amount > 0 {
            if actor == Actor::Player && card.is_some() {
                let history = &mut self.combat_mut().unwrap().history;
                if history.block_card != history.cards {
                    history.block_card = history.cards;
                    history.block_card_gains = 0;
                }
                history.block_gains += 1;
                history.block_card_gains += 1;
            }
            self.trigger(content, Trigger::BlockGained, actor, amount);
        }
        amount
    }

    pub(crate) fn modified_block(
        &self,
        content: &Content,
        actor: Actor,
        base: i16,
        card_or_move: bool,
    ) -> i16 {
        let mut amount = base
            .saturating_add(if card_or_move {
                self.creature(actor).kind(content, PowerKind::Dexterity)
            } else {
                0
            })
            .max(0);
        for _ in 0..self.creature(actor).power(power_id::SHADOWMELD).max(0) {
            amount = amount.saturating_mul(2);
        }
        if actor == Actor::Player
            && card_or_move
            && self.combat().unwrap().history.block_gains
                - if self.combat().unwrap().history.block_card
                    == self.combat().unwrap().history.cards
                {
                    self.combat().unwrap().history.block_card_gains
                } else {
                    0
                }
                < self.creature(actor).power(power_id::UNMOVABLE)
        {
            amount = amount.saturating_mul(2);
        }
        if card_or_move && self.creature(actor).kind(content, PowerKind::Frail) > 0 {
            amount = amount * 3 / 4;
        }
        amount
    }

    fn apply_power(&mut self, content: &Content, actor: Actor, id: Id, mut amount: i16) {
        if actor == Actor::Player
            && id == power_id::STRENGTH
            && amount > 0
            && self.has_relic(content, "RELIC.RUINED_HELMET")
            && !self.combat().unwrap().ruined_helmet
        {
            amount *= 2;
            self.combat_mut().unwrap().ruined_helmet = true;
        }
        if matches!(actor, Actor::Enemy(_))
            && (amount < 0 || content.powers[id as usize].debuff && amount > 0)
            && self.has_relic(content, "RELIC.UNSETTLING_LAMP")
            && !self.combat().unwrap().unsettling_used
            && !self.combat().unwrap().enemy_turn
            && let Some(card) = self
                .combat()
                .unwrap()
                .auto_plays
                .last()
                .map(|play| play.card.id)
                .or(self.combat().unwrap().playing.map(|card| card.id))
        {
            if self.combat().unwrap().unsettling_lamp.is_none() {
                self.combat_mut().unwrap().unsettling_lamp = Some(card);
            }
            if self.combat().unwrap().unsettling_lamp == Some(card) {
                amount *= 2;
            }
        }
        if id == power_id::POISON
            && matches!(actor, Actor::Enemy(_))
            && amount > 0
            && self.has_relic(content, "RELIC.SNECKO_SKULL")
        {
            amount += 1;
        }
        if content.powers[id as usize].debuff {
            self.apply_debuff(content, actor, id, amount);
        } else {
            let energy = actor == Actor::Player
                && matches!(
                    id,
                    power_id::DEMESNE | power_id::FRIENDSHIP | power_id::PYRE
                );
            let before = self.creature(actor).power(id);
            if actor == Actor::Player && id == power_id::ORBIT && amount != 0 {
                self.creature_mut(actor).powers.push(Power {
                    id,
                    amount,
                    skip_next_decay: false,
                    value: 0,
                });
            } else {
                self.creature_mut(actor).add_power(id, amount);
            }
            if id == power_id::SANDPIT
                && matches!(actor, Actor::Enemy(_))
                && before > 0
                && self.creature(actor).power(id) == 0
            {
                self.creature_mut(Actor::Player).hp = 0;
                self.creature_mut(Actor::Osty).hp = 0;
            }
            if id == power_id::CRIMSON_MANTLE
                && let Some(power) = self
                    .creature_mut(actor)
                    .powers
                    .iter_mut()
                    .find(|power| power.id == id)
            {
                power.value += 1;
            }
            if energy {
                let delta = self.creature(actor).power(id).saturating_sub(before);
                let max_energy = self.combat().unwrap().max_energy.saturating_add(delta);
                self.combat_mut().unwrap().max_energy = max_energy;
            }
            if actor == Actor::Player && id == power_id::PHANTOM_BLADES {
                self.retain_shivs(content);
            }
        }
    }

    fn apply_debuff(&mut self, content: &Content, actor: Actor, id: Id, amount: i16) -> bool {
        if amount == 0 {
            return false;
        }
        if let Some(artifact) = self
            .creature(actor)
            .powers
            .iter()
            .find(|power| content.powers[power.id as usize].kind == PowerKind::Artifact)
            .copied()
        {
            self.creature_mut(actor).consume_power(artifact.id);
            false
        } else {
            if id == power_id::TENDER
                && !self
                    .creature(actor)
                    .powers
                    .iter()
                    .any(|power| power.id == id)
            {
                self.creature_mut(actor).powers.push(Power {
                    id,
                    amount: 0,
                    skip_next_decay: false,
                    value: 0,
                });
                return true;
            }
            let skip_next_decay = actor == Actor::Player
                && self.combat().unwrap().enemy_turn
                && matches!(
                    content.powers[id as usize].kind,
                    PowerKind::Weak | PowerKind::Vulnerable | PowerKind::Frail
                );
            let new = self.creature(actor).power(id) == 0;
            self.creature_mut(actor)
                .add_power_with_skip_next_decay(id, amount, skip_next_decay);
            if actor == Actor::Player && id == power_id::DAMPEN && new {
                let combat = self.combat_mut().unwrap();
                for pile in [
                    &mut combat.draw,
                    &mut combat.hand,
                    &mut combat.discard,
                    &mut combat.exhaust,
                ] {
                    for card in pile.iter_mut().filter(|card| card.upgrades > 0) {
                        combat.dampened.push((card.instance, card.upgrades));
                        card.upgrades = 0;
                    }
                }
            }
            if matches!(id, power_id::ENFEEBLING_TOUCH | power_id::MANGLE) {
                self.creature_mut(actor)
                    .add_power(power_id::STRENGTH, -amount);
            }
            if id == power_id::DOOM && amount > 0 {
                self.combat_mut().unwrap().history.doom_applied += 1;
                let block = self.creature(Actor::Player).power(power_id::SHROUD).max(0);
                if block > 0 {
                    self.creature_mut(Actor::Player).block =
                        self.creature(Actor::Player).block.saturating_add(block);
                    self.trigger(content, Trigger::BlockGained, Actor::Player, block);
                }
            }
            if matches!(actor, Actor::Enemy(_))
                && !matches!(id, power_id::ENFEEBLING_TOUCH | power_id::MANGLE)
            {
                let damage = self
                    .creature(Actor::Player)
                    .power(power_id::SLEIGHT_OF_FLESH);
                if damage > 0 {
                    self.damage(
                        content,
                        Actor::Player,
                        actor,
                        damage,
                        DamageKind::Unpowered,
                        None,
                    );
                }
            }
            if id == power_id::VULNERABLE && amount > 0 && matches!(actor, Actor::Enemy(_)) {
                let draw = self
                    .creature(Actor::Player)
                    .power(power_id::VICIOUS)
                    .clamp(0, u8::MAX as i16) as u8;
                if draw > 0 {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::Draw(draw),
                        context: Context::player(),
                    });
                }
            }
            if id == power_id::POISON && amount > 0 && matches!(actor, Actor::Enemy(_)) {
                self.outbreak(content);
            }
            true
        }
    }

    fn outbreak(&mut self, content: &Content) {
        let amount = self.creature(Actor::Player).power(power_id::OUTBREAK);
        if amount <= 0 {
            return;
        }
        let combat = self.combat_mut().unwrap();
        combat.poisoned += 1;
        if combat.poisoned < 3 {
            return;
        }
        combat.poisoned %= 3;
        for index in 0..combat.enemies.len() {
            self.damage(
                content,
                Actor::Player,
                Actor::Enemy(index),
                amount,
                DamageKind::Unpowered,
                None,
            );
        }
    }

    fn retain_shivs(&mut self, content: &Content) {
        let combat = self.combat_mut().unwrap();
        for pile in [
            &mut combat.draw,
            &mut combat.hand,
            &mut combat.discard,
            &mut combat.exhaust,
        ] {
            for card in pile {
                if content.cards[card.id as usize].tags & SHIV_TAG != 0 {
                    card.flags |= RETAIN;
                }
            }
        }
    }

    fn poison(&mut self, content: &Content, actor: Actor) {
        let Some(power) = self
            .creature(actor)
            .powers
            .iter()
            .find(|x| content.powers[x.id as usize].kind == PowerKind::Poison)
            .copied()
        else {
            return;
        };
        self.damage(
            content,
            actor,
            actor,
            power.amount,
            DamageKind::Unblockable,
            None,
        );
        self.resolve(content);
        if self.creature(actor).hp > 0 && self.creature(actor).power(power.id) > 0 {
            self.creature_mut(actor).consume_power(power.id);
        }
    }

    fn channel(&mut self, content: &Content, id: Id) {
        let combat = self.combat().unwrap();
        if combat.enemies.iter().all(|enemy| enemy.creature.hp <= 0)
            && combat
                .enemies
                .iter()
                .all(|enemy| enemy.creature.power(power_id::ADAPTABLE) == 0)
        {
            return;
        }
        self.combat_mut().unwrap().orb_slots = self.combat().unwrap().orb_slots.max(1);
        if id == orb_id::LIGHTNING {
            let combat = self.combat_mut().unwrap();
            combat.lightning_channeled = combat.lightning_channeled.saturating_add(1);
        }
        if self.combat().unwrap().orbs.len() == self.combat().unwrap().orb_slots as usize {
            let mut pending = std::mem::take(&mut self.combat_mut().unwrap().queue);
            self.evoke(content, true);
            self.resolve(content);
            pending.append(&mut self.combat_mut().unwrap().queue);
            self.combat_mut().unwrap().queue = pending;
        }
        self.combat_mut().unwrap().orbs.push(Orb {
            id,
            value: content.orbs[id as usize].initial_value,
        });
        let combat = self.combat_mut().unwrap();
        combat.orbs_channeled = combat.orbs_channeled.saturating_add(1);
        if self.combat().unwrap().orbs_channeled == 7 && self.has_relic(content, "RELIC.METRONOME")
        {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(30, 30)),
                context: Context::player(),
            });
        }
    }

    fn evoke(&mut self, content: &Content, dequeue: bool) {
        if self.combat().unwrap().orbs.is_empty() {
            return;
        }
        let orb = if dequeue {
            self.combat_mut().unwrap().orbs.remove(0)
        } else {
            self.combat().unwrap().orbs[0]
        };
        let def = content.orbs[orb.id as usize];
        let context = Context {
            source: Actor::Player,
            target: None,
            card: None,
            upgraded: false,
            x: 0,
            event: orb.value,
            orb: true,
            orb_id: Some(orb.id),
            pen_nib: false,
        };
        push_effects(&mut self.combat_mut().unwrap().queue, def.evoke, context);
    }

    fn passive_orbs(&mut self, content: &Content, timing: OrbTiming) {
        let orbs = self.combat().unwrap().orbs.clone();
        for (index, orb) in orbs.into_iter().enumerate().rev() {
            if content.orbs[orb.id as usize].timing == timing {
                self.passive_orb(content, Some(index));
            }
        }
    }

    fn passive_orb(&mut self, content: &Content, index: Option<usize>) {
        let Some(index) = index else { return };
        let Some(orb) = self.combat().unwrap().orbs.get(index).copied() else {
            return;
        };
        let def = content.orbs[orb.id as usize];
        let focus = self.creature(Actor::Player).kind(content, PowerKind::Focus);
        let triggers =
            1 + (index == 0 && self.has_relic(content, "RELIC.GOLD_PLATED_CABLES")) as usize;
        let mut contexts = Vec::with_capacity(triggers);
        for _ in 0..triggers {
            let value = self.combat().unwrap().orbs[index].value.saturating_add(
                (def.passive_value + if def.focus_passive { focus } else { 0 }).max(0),
            );
            self.combat_mut().unwrap().orbs[index].value = value;
            let event = self.combat().unwrap().orbs[index].value;
            if def.passive_decay == 0 || event.saturating_add(focus) > 0 {
                self.combat_mut().unwrap().orbs[index].value = (event - def.passive_decay).max(0);
            }
            contexts.push(Context {
                source: Actor::Player,
                target: None,
                card: None,
                upgraded: false,
                x: 0,
                event,
                orb: false,
                orb_id: Some(orb.id),
                pen_nib: false,
            });
        }
        for context in contexts.into_iter().rev() {
            push_effects(&mut self.combat_mut().unwrap().queue, def.passive, context);
        }
    }

    fn decay(&mut self, content: &Content, actor: Actor) {
        let powers: Vec<_> = self
            .creature(actor)
            .powers
            .iter()
            .filter(|x| {
                matches!(
                    content.powers[x.id as usize].kind,
                    PowerKind::Weak
                        | PowerKind::Vulnerable
                        | PowerKind::Frail
                        | PowerKind::Intangible
                ) || x.id == power_id::SHRINK && x.amount > 0
            })
            .map(|power| (power.id, power.skip_next_decay))
            .collect();
        for (id, skip) in powers {
            if skip {
                self.creature_mut(actor)
                    .powers
                    .iter_mut()
                    .find(|power| power.id == id)
                    .unwrap()
                    .skip_next_decay = false;
            } else {
                self.creature_mut(actor).consume_power(id);
            }
        }
        if matches!(actor, Actor::Enemy(_)) {
            self.creature_mut(actor).consume_power(power_id::CONQUEROR);
            self.creature_mut(actor).consume_power(power_id::DEBILITATE);
            self.creature_mut(actor)
                .powers
                .retain(|power| power.id != power_id::SIC_EM);
        }
    }

    fn trigger(&mut self, content: &Content, trigger: Trigger, owner: Actor, event: i16) {
        if trigger == Trigger::CardExhausted
            && owner == Actor::Player
            && self.has_relic(content, "RELIC.JOSS_PAPER")
        {
            self.joss_paper = (self.joss_paper + 1) % 5;
            if self.joss_paper == 0 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Draw(1),
                    context: Context::player(),
                });
            }
        }
        let target = match owner {
            Actor::Player | Actor::Osty => None,
            Actor::Enemy(i) => Some(i),
        };
        let context = Context {
            source: owner,
            target,
            card: None,
            upgraded: false,
            x: 0,
            event,
            orb: false,
            orb_id: None,
            pen_nib: false,
        };
        let powers = self.creature(owner).powers.clone();
        self.dispatch(content, trigger, owner, context, powers);
    }

    fn dispatch(
        &mut self,
        content: &Content,
        trigger: Trigger,
        owner: Actor,
        context: Context,
        powers: Vec<Power>,
    ) {
        for power in powers.into_iter().rev() {
            if power.id == 79
                && trigger == Trigger::TurnStart
                && self.combat().is_some_and(|combat| combat.turn == 1)
            {
                continue;
            }
            if trigger == Trigger::CardPlayed
                && power.id == power_id::ENRAGE
                && context
                    .card
                    .is_some_and(|card| content.cards[card as usize].card_type == CardType::Skill)
            {
                self.apply_power(content, owner, power_id::STRENGTH, power.amount);
            }
            if trigger == Trigger::CardPlayed
                && power.id == power_id::GALVANIC
                && context
                    .card
                    .is_some_and(|card| content.cards[card as usize].card_type == CardType::Power)
            {
                self.damage(
                    content,
                    owner,
                    Actor::Player,
                    power.amount,
                    DamageKind::Unpowered,
                    None,
                );
            }
            if trigger == Trigger::CardPlayed && power.id == power_id::WITHERING_PRESENCE {
                let generate = {
                    let power = self
                        .creature_mut(owner)
                        .powers
                        .iter_mut()
                        .find(|x| x.id == power.id)
                        .unwrap();
                    if power.value <= 0 {
                        power.value = power.amount;
                    }
                    power.value -= 1;
                    if power.value == 0 {
                        power.value = 6;
                        true
                    } else {
                        false
                    }
                };
                if generate {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AddCard(Pile::Hand, card_id::WITHER, 1),
                        context,
                    });
                }
            }
            if trigger == Trigger::TurnEnd
                && matches!(
                    power.id,
                    power_id::FOCUSED_STRIKE | power_id::HOTFIX | power_id::SYNCHRONIZE
                )
            {
                self.creature_mut(owner)
                    .add_power(power_id::FOCUS, -power.amount);
            }
            if matches!(content.powers[power.id as usize].kind, PowerKind::OneShot(at) if at == trigger)
            {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::RemovePower(Target::Source, power.id),
                    context,
                });
            }
            for hook in content.powers[power.id as usize]
                .hooks
                .iter()
                .rev()
                .filter(|x| x.trigger == trigger)
            {
                let mut context = context;
                if matches!(
                    trigger,
                    Trigger::PowerPlayed | Trigger::CardDrawn | Trigger::CardGenerated
                ) {
                    context.event = power.amount;
                }
                push_effects(&mut self.combat_mut().unwrap().queue, hook.effects, context);
            }
        }
        if owner == Actor::Player {
            for i in (0..self.run.relics.len()).rev() {
                if self.melted_relics.contains(&i) {
                    continue;
                }
                let id = self.run.relics[i];
                for hook in content.relics[id as usize]
                    .hooks
                    .iter()
                    .rev()
                    .filter(|x| x.trigger == trigger)
                {
                    push_effects(&mut self.combat_mut().unwrap().queue, hook.effects, context);
                }
            }
        }
    }

    fn trigger_card(&mut self, content: &Content, card: Card, trigger: Trigger) {
        if trigger == Trigger::CardExhausted
            && !self.combat().unwrap().burning_sticks
            && self.has_relic(content, "RELIC.BURNING_STICKS")
            && card.card_type(content.cards[card.id as usize]) == CardType::Skill
        {
            self.combat_mut().unwrap().burning_sticks = true;
            self.add_generated(
                content,
                Pile::Hand,
                Card {
                    instance: 0,
                    ..card
                },
            );
        }
        let context = Context {
            event: card.value,
            ..Context::card(card)
        };
        for hook in content.cards[card.id as usize]
            .hooks
            .iter()
            .rev()
            .filter(|hook| hook.trigger == trigger)
        {
            push_effects(&mut self.combat_mut().unwrap().queue, hook.effects, context);
        }
    }

    fn trigger_generated(&mut self, content: &Content, card: Id) {
        self.combat_mut().unwrap().history.generated += 1;
        if content.cards[card as usize].tags & SHIV_TAG != 0
            && self.creature(Actor::Player).power(power_id::PHANTOM_BLADES) > 0
        {
            self.retain_shivs(content);
        }
        let powers = self.creature(Actor::Player).powers.clone();
        self.dispatch(
            content,
            Trigger::CardGenerated,
            Actor::Player,
            Context {
                source: Actor::Player,
                target: None,
                card: Some(card),
                upgraded: false,
                x: 0,
                event: 1,
                orb: false,
                orb_id: None,
                pen_nib: false,
            },
            powers,
        );
    }

    pub(crate) fn amount(&self, content: &Content, context: Context, amount: Amount) -> i16 {
        let combat = self.combat().unwrap();
        let source = self.creature(context.source);
        let scale = match amount.scale {
            Scale::None => 0,
            Scale::X => context.x,
            Scale::Block => source.block,
            Scale::Energy => combat.energy,
            Scale::MissingHp => source.max_hp - source.hp,
            Scale::DrawSize => combat.draw.len() as i16,
            Scale::DiscardSize => combat.discard.len() as i16,
            Scale::ExhaustSize => combat.exhaust.len() as i16,
            Scale::ExhaustId(id) => {
                combat.exhaust.iter().filter(|card| card.id == id).count() as i16
            }
            Scale::HandSize => combat.hand.len() as i16,
            Scale::CardsPlayed => combat.history.cards,
            Scale::AttacksPlayed => combat.history.attacks,
            Scale::PriorAttacks => (combat.history.attacks - 1).max(0),
            Scale::SkillsPlayed => combat.history.skills,
            Scale::EnergySpent => combat.history.energy,
            Scale::Exhausted => combat.history.exhausted,
            Scale::HpLost => combat.history.hp_lost,
            Scale::HpLossEvents => combat.history.hp_loss_events,
            Scale::CardValue => combat
                .auto_plays
                .last()
                .map(|play| play.card)
                .or(combat.playing)
                .map_or(context.event, |card| card.value),
            Scale::LivingEnemies => {
                combat.enemies.iter().filter(|x| x.creature.hp > 0).count() as i16
            }
            Scale::Orbs => combat.orbs.len() as i16,
            Scale::OrbTypes => {
                let mut ids: Vec<_> = combat.orbs.iter().map(|orb| orb.id).collect();
                ids.sort_unstable();
                ids.dedup();
                ids.len() as i16
            }
            Scale::OrbTypesPower(id) => {
                let mut ids: Vec<_> = combat.orbs.iter().map(|orb| orb.id).collect();
                ids.sort_unstable();
                ids.dedup();
                (ids.len() as i16).saturating_mul(source.power(id))
            }
            Scale::Stars => combat.stars,
            Scale::TargetPower(id) => context
                .target
                .map_or(0, |target| self.creature(Actor::Enemy(target)).power(id)),
            Scale::TargetPowerDiv(id, divisor) => context.target.map_or(0, |target| {
                self.creature(Actor::Enemy(target)).power(id) / divisor.max(1)
            }),
            Scale::Tagged(tag) => {
                let piles = combat
                    .draw
                    .iter()
                    .chain(&combat.hand)
                    .chain(&combat.discard)
                    .chain(&combat.exhaust);
                piles
                    .filter(|card| content.cards[card.id as usize].tags & tag != 0)
                    .count() as i16
                    + context
                        .card
                        .is_some_and(|id| content.cards[id as usize].tags & tag != 0)
                        as i16
            }
            Scale::Power(power_id::CRIMSON_MANTLE_DAMAGE) => source
                .powers
                .iter()
                .find(|power| power.id == power_id::CRIMSON_MANTLE)
                .map_or(0, |power| power.value),
            Scale::Power(id) => source.power(id),
            Scale::Event => context.event,
            Scale::EventPower(id) => context.event.saturating_add(source.power(id)),
            Scale::HandType(kind) => combat
                .hand
                .iter()
                .filter(|card| content.cards[card.id as usize].card_type == kind)
                .count() as i16,
            Scale::EnemyPowerTotal(id) => combat
                .enemies
                .iter()
                .filter(|enemy| enemy.creature.hp > 0)
                .map(|enemy| enemy.creature.power(id))
                .sum(),
            Scale::TargetDebuffs => context.target.map_or(0, |target| {
                let creature = self.creature(Actor::Enemy(target));
                creature
                    .powers
                    .iter()
                    .filter(|power| {
                        content.powers[power.id as usize].debuff
                            || power.id == power_id::STRENGTH
                                && power.amount < 0
                                && creature.power(power_id::RESTORE_STRENGTH) == 0
                    })
                    .count() as i16
            }),
            Scale::Discarded => combat.history.discarded,
            Scale::DrawnCombat => combat.drawn,
            Scale::StarCards => combat
                .draw
                .iter()
                .chain(&combat.hand)
                .chain(&combat.discard)
                .chain(&combat.exhaust)
                .chain(combat.playing.iter())
                .chain(combat.auto_plays.iter().map(|play| &play.card))
                .filter(|card| {
                    content.cards[card.id as usize].star_cost[card.upgrades.min(1) as usize] != -1
                })
                .count() as i16,
            Scale::StarsGained => combat.history.stars_gained,
            Scale::Generated => combat.history.generated,
            Scale::PriorTargetHits => context
                .target
                .map_or(0, |target| combat.hits[target].saturating_sub(1)),
            Scale::OstyHp => combat.osty.hp.max(0),
            Scale::OstyMaxHp => combat.osty.max_hp.max(0),
            Scale::OstyAttacks => combat.history.osty_attacks,
            Scale::EtherealPlayed => combat.history.ethereal,
            Scale::LightningChanneled => combat.lightning_channeled,
            Scale::LastDamage => combat.last_damage,
            Scale::ExtraDrawn => combat.history.extra_drawn,
            Scale::TurnDiv(divisor) => combat.turn as i16 / divisor.max(1),
        };
        let base = if context.upgraded
            || matches!(context.source, Actor::Enemy(_))
                && amount.ascension > 0
                && self.run.ascension >= amount.ascension
        {
            amount.upgraded
        } else {
            amount.base
        };
        base.saturating_add(scale.saturating_mul(amount.multiplier)) / amount.divisor.max(1)
    }

    pub(crate) fn condition(
        &self,
        content: &Content,
        context: Context,
        condition: Condition,
    ) -> bool {
        match condition {
            Condition::Always => true,
            Condition::Upgraded => context.upgraded,
            Condition::TargetAlive => context
                .target
                .is_some_and(|x| self.creature(Actor::Enemy(x)).hp > 0),
            Condition::TargetHasPower(id) => context
                .target
                .is_some_and(|x| self.creature(Actor::Enemy(x)).power(id) != 0),
            Condition::SourceHasPower(id) => self.creature(context.source).power(id) != 0,
            Condition::HandAtMost(count) => self.combat().unwrap().hand.len() <= count as usize,
            Condition::HandAtLeast(count) => self.combat().unwrap().hand.len() >= count as usize,
            Condition::HandWithout(kind) => self
                .combat()
                .unwrap()
                .hand
                .iter()
                .all(|card| content.cards[card.id as usize].card_type != kind),
            Condition::HandEmpty => self.combat().unwrap().hand.is_empty(),
            Condition::EventAtLeast(value) => context.event >= value,
            Condition::ExhaustAtLeast(count) => {
                self.combat().unwrap().exhaust.len() >= count as usize
            }
            Condition::ExhaustedThisTurn => self.combat().unwrap().history.exhausted > 0,
            Condition::HpLostThisTurn => self.combat().unwrap().history.hp_lost > 0,
            Condition::TargetDead => context
                .target
                .is_some_and(|x| self.creature(Actor::Enemy(x)).hp <= 0),
            Condition::FirstTurn => self.combat().unwrap().turn == 1,
            Condition::HasOrb(id) => self.combat().unwrap().orbs.iter().any(|orb| orb.id == id),
            Condition::CardType(kind) => context
                .card
                .is_some_and(|id| content.cards[id as usize].card_type == kind),
            Condition::PriorCardsBelow(count) => {
                self.combat().unwrap().history.cards - 1 < count[context.upgraded as usize] as i16
            }
            Condition::TargetAttacking => context.target.is_some_and(|target| {
                let enemy = &self.combat().unwrap().enemies[target];
                effects_attack(
                    content.enemies[enemy.creature.id as usize].moves[enemy.move_index].effects,
                )
            }),
            Condition::XAtLeast(value) => context.x >= value,
            Condition::OstyAlive => self.creature(Actor::Osty).hp > 0,
            Condition::DoomApplied => self.combat().unwrap().history.doom_applied > 0,
            Condition::Card(id) => context.card == Some(id),
        }
    }

    fn targets(&mut self, target: Target, context: Context) -> Vec<Actor> {
        match target {
            Target::Source => vec![context.source],
            Target::Player => vec![Actor::Player],
            Target::Osty => vec![Actor::Osty],
            Target::ChosenEnemy
                if context.card == Some(card_id::SOVEREIGN_BLADE)
                    && self.creature(Actor::Player).power(power_id::SEEKING_EDGE) > 0 =>
            {
                self.combat()
                    .unwrap()
                    .enemies
                    .iter()
                    .enumerate()
                    .filter(|(_, enemy)| enemy.creature.hp > 0)
                    .map(|(i, _)| Actor::Enemy(i))
                    .collect()
            }
            Target::ChosenEnemy => context
                .target
                .filter(|&x| {
                    self.combat()
                        .unwrap()
                        .enemies
                        .get(x)
                        .is_some_and(|enemy| enemy.creature.hp > 0)
                })
                .map(|x| vec![Actor::Enemy(x)])
                .unwrap_or_default(),
            Target::ChosenEnemyOrDead => context
                .target
                .filter(|&index| self.combat().unwrap().enemies.get(index).is_some())
                .map(|index| vec![Actor::Enemy(index)])
                .unwrap_or_default(),
            Target::AllEnemies => self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, x)| x.creature.hp > 0)
                .map(|(i, _)| Actor::Enemy(i))
                .collect(),
            Target::OtherEnemies => self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .filter(|(i, enemy)| {
                    enemy.creature.hp > 0
                        && Some(*i)
                            != match context.source {
                                Actor::Enemy(source) => Some(source),
                                _ => context.target,
                            }
                })
                .map(|(i, _)| Actor::Enemy(i))
                .collect(),
            Target::RandomEnemy => {
                let alive: Vec<_> = self
                    .combat()
                    .unwrap()
                    .enemies
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| x.creature.hp > 0)
                    .map(|(i, _)| Actor::Enemy(i))
                    .collect();
                if alive.is_empty() {
                    vec![]
                } else {
                    let choice = if self.expectation.is_some() {
                        self.expectation_choice(alive.len()).unwrap_or(0)
                    } else {
                        self.rngs.combat_targets.below(alive.len() as u32) as usize
                    };
                    vec![alive[choice]]
                }
            }
            Target::LowestHpEnemy => self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, enemy)| enemy.creature.hp > 0)
                .min_by_key(|(_, enemy)| enemy.creature.hp)
                .map(|(i, _)| vec![Actor::Enemy(i)])
                .unwrap_or_default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn select(
        &mut self,
        content: &Content,
        pile: Pile,
        filter: CardFilter,
        op: CardOp,
        count: u8,
        random: bool,
        optional: bool,
    ) {
        let random = random && count != u8::MAX;
        let candidates: Vec<_> = cards(self.combat().unwrap(), pile)
            .iter()
            .enumerate()
            .filter(|(_, card)| eligible(content, card, filter, op))
            .map(|(i, _)| i)
            .collect();
        let count = count.min(candidates.len() as u8);
        if random {
            let mut candidates = candidates;
            for _ in 0..count {
                let at = if self.expectation.is_some() {
                    self.expectation_choice(candidates.len()).unwrap_or(0)
                } else {
                    self.rngs
                        .combat_card_selection
                        .below(candidates.len() as u32) as usize
                };
                let index = candidates.remove(at);
                let moved = self.apply_card_op(content, pile, index, op);
                if moved {
                    for candidate in &mut candidates {
                        if *candidate > index {
                            *candidate -= 1;
                        }
                    }
                }
            }
        } else if !optional && count as usize == candidates.len() {
            for &index in candidates.iter().rev() {
                self.apply_card_op(content, pile, index, op);
            }
        } else if count > 0 {
            self.combat_mut().unwrap().choice = Some(Choice {
                pile,
                filter,
                op,
                remaining: count,
                optional,
            });
        }
    }

    fn apply_card_op(&mut self, content: &Content, pile: Pile, index: usize, op: CardOp) -> bool {
        match op {
            CardOp::Move(to) => {
                let card = self.remove_pile(pile, index);
                let sly = pile == Pile::Hand && to == Pile::Discard && self.is_sly(content, card);
                if pile == Pile::Hand && to == Pile::Discard {
                    self.expectation_discard(1);
                    self.combat_mut().unwrap().history.discarded += 1;
                    self.after_discard(content, 1);
                }
                if !sly {
                    if to == Pile::Hand {
                        self.add_to_hand(card);
                    } else {
                        self.push_pile(to, card);
                    }
                }
                if sly {
                    self.sly(content, card);
                }
                if pile == Pile::Hand {
                    self.unceasing_top(content);
                }
                if to == Pile::Exhaust {
                    self.combat_mut().unwrap().history.exhausted += 1;
                    self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
                    self.trigger_card(content, card, Trigger::CardExhausted);
                }
                return true;
            }
            CardOp::Upgrade => self.pile_mut(pile)[index].upgrades = 1,
            CardOp::Cost(delta) => {
                let card = &mut self.pile_mut(pile)[index];
                card.cost_delta = card.cost_delta.saturating_add(delta);
            }
            CardOp::SetCost(cost) => {
                let card = self.pile_mut(pile)[index];
                let base = content.cards[card.id as usize].cost[card.upgrades.min(1) as usize];
                self.pile_mut(pile)[index].cost_delta = if base < 0 { 0 } else { cost - base };
            }
            CardOp::Flag(flag) => self.pile_mut(pile)[index].flags |= flag,
            CardOp::TurnFlag(flag) => self.pile_mut(pile)[index].turn_flags |= flag,
            CardOp::CopyNextTurn(count) => {
                let mut card = self.pile_mut(pile)[index];
                card.instance = 0;
                card.free = false;
                card.turn_flags = 0;
                self.combat_mut().unwrap().nightmares.push((card, count));
            }
            CardOp::Transform(id, upgrades) => {
                self.pile_mut(pile)[index] = Card {
                    id,
                    upgrades,
                    ..Card::default()
                }
            }
            CardOp::AutoPlay(count) => {
                let card = self.remove_pile(pile, index);
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::AutoPlay(card),
                    context: Context::card(card),
                });
                for _ in 1..count {
                    let mut replay = card;
                    replay.flags |= REPLAY;
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlay(replay),
                        context: Context::card(card),
                    });
                }
                return true;
            }
            CardOp::CopySelected(count) => {
                let mut card = self.pile_mut(pile)[index];
                card.instance = 0;
                for _ in 0..count {
                    self.add_generated(content, Pile::Hand, card);
                }
            }
            CardOp::TakeOffer => {
                let card = self.remove_pile(pile, index);
                self.combat_mut().unwrap().offer.clear();
                self.add_to_hand(card);
                self.trigger_generated(content, card.id);
                return true;
            }
            CardOp::Transfigure => {
                let card = &mut self.pile_mut(pile)[index];
                if content.cards[card.id as usize].cost[card.upgrades.min(1) as usize] >= 0 {
                    card.cost_delta = card.cost_delta.saturating_add(1);
                }
                card.replays = card.replays.saturating_add(1);
            }
            CardOp::Replay(count) => {
                let card = &mut self.pile_mut(pile)[index];
                card.replays = card.replays.saturating_add(count);
            }
            CardOp::TakeFetched => {
                let mut card = self.remove_pile(pile, index);
                card.turn_flags &= !FETCHED;
                for card in &mut self.combat_mut().unwrap().draw {
                    card.turn_flags &= !FETCHED;
                }
                self.add_to_hand(card);
                return true;
            }
            CardOp::TransformRandom => {
                self.expectation_unknown();
                let old = self.pile_mut(pile)[index];
                let pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        id != old.id && content.cards[id as usize].flags[0] & NO_GENERATE == 0
                    })
                    .collect();
                if !pool.is_empty() {
                    let id =
                        pool[self.rngs.combat_card_generation.below(pool.len() as u32) as usize];
                    self.pile_mut(pile)[index] = Card {
                        id,
                        upgrades: old.upgrades,
                        ..Card::default()
                    };
                }
            }
            CardOp::DiscardDraw => {
                let card = self.remove_pile(pile, index);
                self.expectation_discard(1);
                self.combat_mut().unwrap().discard.push(card);
                self.combat_mut().unwrap().history.discarded += 1;
                self.after_discard(content, 1);
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Draw(1),
                    context: Context::player(),
                });
                return true;
            }
            CardOp::MoveFree(to) => {
                let mut card = self.remove_pile(pile, index);
                card.free = true;
                self.push_pile(to, card);
                return true;
            }
            CardOp::FreeCombat => self.pile_mut(pile)[index].flags |= FREE_COMBAT,
        }
        false
    }

    fn sly(&mut self, _content: &Content, card: Card) {
        self.combat_mut().unwrap().queue.push(Pending {
            effect: Effect::AutoPlay(card),
            context: Context::card(card),
        });
    }

    fn discard_hand(&mut self, content: &Content) {
        let hand = std::mem::take(&mut self.combat_mut().unwrap().hand);
        self.expectation_discard(hand.len());
        self.combat_mut().unwrap().history.discarded += hand.len() as i16;
        self.after_discard(content, hand.len());
        let discarded: Vec<_> = hand
            .iter()
            .copied()
            .filter(|&card| !self.is_sly(content, card))
            .collect();
        self.combat_mut().unwrap().discard.extend(discarded);
        for card in hand.into_iter().rev() {
            if self.is_sly(content, card) {
                self.sly(content, card);
            }
        }
        self.unceasing_top(content);
    }

    fn after_discard(&mut self, content: &Content, count: usize) {
        if self.combat().unwrap().enemy_turn {
            return;
        }
        for _ in 0..count {
            if self.has_relic(content, "RELIC.TOUGH_BANDAGES") {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::RawBlock(Target::Player, Amount::fixed(3, 3)),
                    context: Context::player(),
                });
            }
            if self.has_relic(content, "RELIC.TINGSHA") {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Damage(Target::RandomEnemy, Amount::fixed(3, 3)),
                    context: Context::player(),
                });
            }
        }
    }

    fn unceasing_top(&mut self, content: &Content) {
        if self.combat().unwrap().hand.is_empty()
            && !self.combat().unwrap().enemy_turn
            && self.has_relic(content, "RELIC.UNCEASING_TOP")
        {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Draw(1),
                context: Context::player(),
            });
        }
    }

    fn mummified_hand(&mut self, content: &Content) {
        let spiked = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
        let combat = self.combat().unwrap();
        let base_costs = |card: &Card| {
            let def = content.cards[card.id as usize];
            def.cost[card.upgrades.min(1) as usize] > 0
                || def.star_cost[card.upgrades.min(1) as usize] > 0
        };
        let costs = |card: &Card| {
            let def = content.cards[card.id as usize];
            energy_cost(combat, *card, def, spiked) > 0 || star_cost(combat, *card, def) > 0
        };
        let mut choices: Vec<_> = combat
            .hand
            .iter()
            .enumerate()
            .filter(|(_, card)| base_costs(card) && costs(card))
            .map(|(index, _)| index)
            .collect();
        if choices.is_empty() {
            choices.extend(
                combat
                    .hand
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| costs(card))
                    .map(|(index, _)| index),
            );
        }
        if choices.is_empty() {
            choices.extend(
                combat
                    .hand
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| base_costs(card))
                    .map(|(index, _)| index),
            );
        }
        if choices.is_empty() {
            choices.extend(0..combat.hand.len());
        }
        if !choices.is_empty() {
            let choice =
                choices[self.rngs.combat_card_selection.below(choices.len() as u32) as usize];
            self.combat_mut().unwrap().hand[choice].free = true;
        }
    }

    fn begin_auto_play(&mut self, content: &Content, mut card: Card, mut context: Context) {
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        let throwing_axe =
            self.has_relic(content, "RELIC.THROWING_AXE") && !self.combat().unwrap().throwing_axe;
        self.combat_mut().unwrap().throwing_axe |= throwing_axe;
        let pen_nib = card_type == CardType::Attack && self.has_relic(content, "RELIC.PEN_NIB");
        let mut pen_nib_counter = self.pen_nib;
        if card.target(def) == Target::ChosenEnemy
            && !(card.id == card_id::SOVEREIGN_BLADE
                && self.creature(Actor::Player).power(power_id::SEEKING_EDGE) > 0)
            && context.target.is_none()
        {
            let alive: Vec<_> = self
                .combat()
                .unwrap()
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, enemy)| enemy.creature.hp > 0)
                .map(|(index, _)| index)
                .collect();
            if !alive.is_empty() {
                let choice = if self.expectation.is_some() {
                    self.expectation_choice(alive.len()).unwrap_or(0)
                } else {
                    self.rngs.combat_targets.below(alive.len() as u32) as usize
                };
                context.target = Some(alive[choice]);
            }
        }
        context.source = Actor::Player;
        context.card = Some(card.id);
        context.upgraded = card.upgrades > 0;
        context.pen_nib = false;
        context.x = if def.cost[card.upgrades.min(1) as usize] < 0 {
            self.combat().unwrap().energy
        } else {
            0
        };
        if matches!(card_type, CardType::Attack | CardType::Skill)
            && self.combat().unwrap().history.attacks + self.combat().unwrap().history.skills
                < self.creature(Actor::Player).power(power_id::NOSTALGIA)
        {
            card.flags |= RETURN_TO_DRAW;
        }
        let combat = self.combat_mut().unwrap();
        let throne = combat.player.power(power_id::THE_SEALED_THRONE);
        let duplicate = (combat.player.power(power_id::DUPLICATION) > 0) as u8;
        if duplicate > 0 {
            combat.player.consume_power(power_id::DUPLICATION);
        }
        if throne > 0 {
            combat.stars = combat.stars.saturating_add(throne);
            combat.history.stars_gained = combat.history.stars_gained.saturating_add(throne);
        }
        let plays = 1
            + throwing_axe as u8
            + card.replays
            + duplicate
            + if card.id == card_id::SOVEREIGN_BLADE {
                combat.player.power(power_id::SWORD_SAGE).max(0) as u8
            } else {
                0
            };
        combat.auto_plays.push(AutoPlay {
            card,
            powers: combat.player.powers.clone(),
            enemy_powers: combat
                .enemies
                .iter()
                .map(|enemy| enemy.creature.powers.clone())
                .collect(),
            plays,
        });
        combat.history.cards += plays as i16;
        match card_type {
            CardType::Attack => combat.history.attacks += plays as i16,
            CardType::Skill => combat.history.skills += plays as i16,
            CardType::Power => combat.history.powers += plays as i16,
            _ => {}
        }
        combat.queue.push(Pending {
            effect: Effect::FinishAutoPlay,
            context,
        });
        let mut pen_nibs = vec![false; plays as usize];
        for doubled in &mut pen_nibs {
            if pen_nib {
                pen_nib_counter = (pen_nib_counter + 1) % 10;
                *doubled = pen_nib_counter == 0;
            }
        }
        for play in (0..plays).rev() {
            let context = Context {
                pen_nib: pen_nibs[play as usize],
                ..context
            };
            push_card_effects(&mut combat.queue, card, def, context);
            if card.flags & INKY != 0 {
                combat.queue.push(Pending {
                    effect: Effect::ApplyPower(
                        Target::ChosenEnemy,
                        power_id::WEAK,
                        Amount::fixed(1, 1),
                    ),
                    context,
                });
            }
        }
        let black_hole = combat.player.power(power_id::BLACK_HOLE);
        if throne > 0 && black_hole > 0 {
            combat.queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(black_hole, black_hole)),
                context,
            });
        }
        if card.id != card_id::BOMBARDMENT && !self.replaying {
            self.queue_bombardments();
        }
        self.pen_nib = pen_nib_counter;
    }

    fn finish_card_destination(
        &mut self,
        content: &Content,
        mut card: Card,
        replay: bool,
        feral: bool,
    ) -> (Card, bool) {
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        let corruption = card_type == CardType::Skill
            && self.creature(Actor::Player).power(power_id::CORRUPTION) > 0;
        if card_type == CardType::Skill
            && self.creature(Actor::Player).power(power_id::MASTER_PLANNER) > 0
        {
            card.flags |= SLY;
        }
        let flags = card.flags(def);
        card.cost_override = None;
        card.free = false;
        let combat = self.combat_mut().unwrap();
        if flags & ETHEREAL != 0 {
            combat.history.ethereal += 1;
        }
        if def.tags & SHIV_TAG != 0 {
            combat.history.shivs += 1;
        }
        let exhausted = if replay {
            false
        } else if feral && combat.history.feral_returns < combat.player.power(power_id::FERAL) {
            if combat.hand.len() < 10 {
                combat.hand.push(card);
                combat.history.feral_returns += 1;
            } else {
                combat.discard.push(card);
            }
            false
        } else if flags & RETURN_TO_HAND != 0 {
            if combat.hand.len() < 10 {
                combat.hand.push(card);
            } else {
                combat.discard.push(card);
            }
            false
        } else if flags & RETURN_TO_DRAW != 0 {
            combat.push_draw(card);
            false
        } else if flags & EXHAUST != 0 || corruption {
            combat.exhaust.push(card);
            true
        } else {
            if card_type != CardType::Power {
                combat.discard.push(card);
            }
            false
        };
        if exhausted {
            combat.history.exhausted += 1;
        }
        (card, exhausted)
    }

    fn queue_card_triggers(
        &mut self,
        content: &Content,
        card: Card,
        powers: Vec<Power>,
        enemies: Vec<Vec<Power>>,
    ) -> Context {
        self.tender();
        let trigger = match card.card_type(content.cards[card.id as usize]) {
            CardType::Attack => Trigger::AttackPlayed,
            CardType::Skill => Trigger::SkillPlayed,
            CardType::Power => Trigger::PowerPlayed,
            _ => Trigger::CardPlayed,
        };
        let context = Context {
            event: 1,
            ..Context::card(card)
        };
        if content.cards[card.id as usize].tags & SHIV_TAG != 0
            && self.has_relic(content, "RELIC.HELICAL_DART")
        {
            self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
            self.apply_power(content, Actor::Player, power_id::HELICAL_DART, 1);
        }
        self.dispatch(content, trigger, Actor::Player, context, powers.clone());
        self.dispatch(content, Trigger::CardPlayed, Actor::Player, context, powers);
        for (index, powers) in enemies.into_iter().enumerate() {
            self.dispatch(
                content,
                Trigger::CardPlayed,
                Actor::Enemy(index),
                Context {
                    source: Actor::Enemy(index),
                    target: Some(index),
                    ..context
                },
                powers,
            );
        }
        context
    }

    fn finish_auto_play(&mut self, content: &Content) {
        let AutoPlay {
            mut card,
            powers,
            enemy_powers: enemies,
            plays,
        } = self.combat_mut().unwrap().auto_plays.pop().unwrap();
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        let replay = card.flags & REPLAY != 0;
        card.flags &= !REPLAY;
        if card.upgrades == 0
            && matches!(card_type, CardType::Attack | CardType::Skill)
            && self.has_relic(content, "RELIC.RAZOR_TOOTH")
        {
            card.upgrades = 1;
        }
        let feral = card_type == CardType::Attack;
        let (card, exhausted) = self.finish_card_destination(content, card, replay, feral);
        if matches!(card_type, CardType::Attack | CardType::Skill) && card.flags & DUPE == 0 {
            self.combat_mut().unwrap().history_course = Some(card);
        }
        if self.combat().unwrap().unsettling_lamp == Some(card.id) {
            self.combat_mut().unwrap().unsettling_lamp = None;
            self.combat_mut().unwrap().unsettling_used = true;
        }
        if card_type == CardType::Attack
            && self.has_relic(content, "RELIC.MUSIC_BOX")
            && !self.combat().unwrap().music_box
        {
            let mut copy = card;
            copy.instance = 0;
            copy.flags |= ETHEREAL;
            self.combat_mut().unwrap().music_box = true;
            self.add_generated(content, Pile::Hand, copy);
        }
        if card_type == CardType::Power
            && self.has_relic(content, "RELIC.PERMAFROST")
            && !self.combat().unwrap().permafrost
        {
            self.combat_mut().unwrap().permafrost = true;
            self.gain_block(content, Actor::Player, 7, None, false);
        }
        if card_type == CardType::Power && self.has_relic(content, "RELIC.MUMMIFIED_HAND") {
            for _ in 0..plays {
                self.mummified_hand(content);
            }
        }
        if self.combat().unwrap().paels_legion == 0
            && self.has_relic(content, "RELIC.PAELS_LEGION")
            && self.combat().unwrap().history.block_card == self.combat().unwrap().history.cards
        {
            self.combat_mut().unwrap().paels_legion = 2;
        }
        let old_attacks = self.combat().unwrap().history.attacks
            - (card_type == CardType::Attack) as i16 * plays as i16;
        let old_skills = self.combat().unwrap().history.skills
            - (card_type == CardType::Skill) as i16 * plays as i16;
        if card_type == CardType::Attack {
            let attacks = self.combat().unwrap().history.attacks;
            if old_attacks < 3 && attacks >= 3 {
                let copies = self
                    .creature(Actor::Player)
                    .power(power_id::JUGGLING)
                    .max(0) as u8;
                for _ in 0..copies {
                    self.add_generated(
                        content,
                        Pile::Hand,
                        Card {
                            instance: 0,
                            ..card
                        },
                    );
                }
            }
            if self.has_relic(content, "RELIC.ORNAMENTAL_FAN") {
                for _ in 0..attacks / 3 - old_attacks / 3 {
                    self.gain_block(content, Actor::Player, 4, None, false);
                }
            }
            for _ in 0..attacks / 3 - old_attacks / 3 {
                if self.has_relic(content, "RELIC.KUNAI") {
                    self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
                }
                if self.has_relic(content, "RELIC.SHURIKEN") {
                    self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
                }
            }
            if self.has_relic(content, "RELIC.NUNCHAKU") {
                let attacks = self.nunchaku as i16 + plays as i16;
                self.combat_mut().unwrap().energy += attacks / 10;
                self.nunchaku = (attacks % 10) as u8;
            }
            if self.has_relic(content, "RELIC.KUSARIGAMA") {
                let attacks = self.combat().unwrap().kusarigama as i16 + plays as i16;
                self.combat_mut().unwrap().kusarigama = (attacks % 3) as u8;
                for _ in 0..attacks / 3 {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::Damage(Target::RandomEnemy, Amount::fixed(6, 6)),
                        context: Context::player(),
                    });
                }
            }
        }
        if card_type == CardType::Skill && self.has_relic(content, "RELIC.LETTER_OPENER") {
            for _ in 0..self.combat().unwrap().history.skills / 3 - old_skills / 3 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Damage(Target::AllEnemies, Amount::fixed(5, 5)),
                    context: Context::player(),
                });
            }
        }
        if self.has_relic(content, "RELIC.IRON_CLUB") {
            let cards = self.iron_club as i16 + plays as i16;
            self.iron_club = (cards % 4) as u8;
            for _ in 0..cards / 4 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Draw(1),
                    context: Context::player(),
                });
            }
        }
        if card_type == CardType::Skill && self.has_relic(content, "RELIC.TUNING_FORK") {
            let skills = self.tuning_fork as i16 + plays as i16;
            self.tuning_fork = (skills % 10) as u8;
            for _ in 0..skills / 10 {
                self.gain_block(content, Actor::Player, 7, None, false);
            }
        }
        if !self.combat().unwrap().rainbow_ring
            && self.has_relic(content, "RELIC.RAINBOW_RING")
            && self.combat().unwrap().history.attacks > 0
            && self.combat().unwrap().history.skills > 0
            && self.combat().unwrap().history.powers > 0
        {
            self.combat_mut().unwrap().rainbow_ring = true;
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
            self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
        }
        if exhausted {
            self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
            self.trigger_card(content, card, Trigger::CardExhausted);
        }
        for _ in 0..plays {
            self.queue_card_triggers(content, card, powers.clone(), enemies.clone());
        }
        if card_type == CardType::Skill {
            self.return_make_it_so(old_skills);
        }
        self.tick_panache();
        self.auto_play_top();
    }

    fn return_make_it_so(&mut self, old_skills: i16) {
        if self.combat().unwrap().history.skills / 3 == old_skills / 3 {
            return;
        }
        for pile in [Pile::Draw, Pile::Discard, Pile::Exhaust] {
            let mut cards = Vec::new();
            if pile == Pile::Draw {
                let indices = self
                    .combat()
                    .unwrap()
                    .draw
                    .iter()
                    .enumerate()
                    .filter_map(|(index, card)| (card.id == card_id::MAKE_IT_SO).then_some(index))
                    .collect::<Vec<_>>();
                for index in indices.into_iter().rev() {
                    cards.push(self.combat_mut().unwrap().remove_draw(index));
                }
                cards.reverse();
            } else {
                self.pile_mut(pile).retain(|card| {
                    let remove = card.id == card_id::MAKE_IT_SO;
                    if remove {
                        cards.push(*card);
                    }
                    !remove
                });
            }
            for card in cards {
                self.add_to_hand(card);
            }
        }
    }

    fn queue_bombardments(&mut self) {
        let mut cards = Vec::new();
        self.combat_mut().unwrap().exhaust.retain(|card| {
            if card.id == card_id::BOMBARDMENT {
                cards.push(*card);
                false
            } else {
                true
            }
        });
        for card in cards {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::AutoPlay(card),
                context: Context::card(card),
            });
        }
    }

    fn auto_play_top(&mut self) {
        if self
            .combat()
            .unwrap()
            .draw
            .last()
            .is_none_or(|card| card.id != card_id::I_AM_INVINCIBLE)
        {
            return;
        }
        let card = self.combat_mut().unwrap().pop_draw().unwrap();
        self.combat_mut().unwrap().queue.insert(
            0,
            Pending {
                effect: Effect::AutoPlay(card),
                context: Context::card(card),
            },
        );
    }

    fn is_sly(&self, content: &Content, card: Card) -> bool {
        let def = content.cards[card.id as usize];
        card.flags(def) & SLY != 0
    }

    fn add_to_hand(&mut self, card: Card) {
        if self.combat().unwrap().hand.len() >= 10 {
            self.expectation_discard(1);
        }
        let combat = self.combat_mut().unwrap();
        if combat.hand.len() < 10 {
            combat.hand.push(card);
        } else {
            combat.discard.push(card);
        }
    }

    fn add_generated(&mut self, content: &Content, pile: Pile, mut card: Card) {
        let def = content.cards[card.id as usize];
        if self.has_relic(content, "RELIC.GHOST_SEED")
            && def.rarity == CardRarity::Basic
            && def.tags & (STRIKE_TAG | DEFEND_TAG) != 0
        {
            card.flags |= ETHEREAL;
        }
        if card.instance == 0 {
            card.instance = self.next_card;
            self.next_card = self.next_card.saturating_add(1);
        }
        if pile == Pile::Hand {
            self.add_to_hand(card);
        } else if pile == Pile::Draw {
            self.combat_mut().unwrap().push_draw(card);
        } else {
            self.pile_mut(pile).push(card);
        }
        self.trigger_generated(content, card.id);
    }

    fn add_random(&mut self, content: &Content, pile: Pile, mut card: Card) {
        self.expectation_unknown();
        card.instance = self.next_card;
        self.next_card = self.next_card.saturating_add(1);
        let len = cards(self.combat().unwrap(), pile).len();
        let roll = self.rngs.shuffle.below(len as u32 + 1) as usize;
        let index = if pile == Pile::Draw { len - roll } else { roll };
        if pile == Pile::Draw {
            self.combat_mut().unwrap().insert_unknown_draw(index, card);
        } else {
            self.pile_mut(pile).insert(index, card);
        }
        self.trigger_generated(content, card.id);
    }

    fn pile_mut(&mut self, pile: Pile) -> &mut Vec<Card> {
        let combat = self.combat_mut().unwrap();
        match pile {
            Pile::Draw => &mut combat.draw,
            Pile::Hand => &mut combat.hand,
            Pile::Discard => &mut combat.discard,
            Pile::Exhaust => &mut combat.exhaust,
            Pile::Offer => &mut combat.offer,
        }
    }

    fn remove_pile(&mut self, pile: Pile, index: usize) -> Card {
        if pile == Pile::Draw {
            self.combat_mut().unwrap().remove_draw(index)
        } else {
            self.pile_mut(pile).remove(index)
        }
    }

    fn push_pile(&mut self, pile: Pile, card: Card) {
        if pile == Pile::Draw {
            self.combat_mut().unwrap().push_draw(card);
        } else {
            self.pile_mut(pile).push(card);
        }
    }

    fn shuffle_draw(&mut self) {
        let Phase::Combat(combat) = &mut self.phase else {
            return;
        };
        if self.expectation.is_none() {
            self.rngs.shuffle.shuffle(&mut combat.draw);
        }
        combat.forget_draw_order();
    }

    fn shuffle_discard_into_draw(&mut self, content: &Content) {
        let Phase::Combat(combat) = &mut self.phase else {
            return;
        };
        combat
            .discard
            .sort_by_key(|card| (content.cards[card.id as usize].id, card.upgrades));
        combat.draw.append(&mut combat.discard);
        if self.expectation.is_none() {
            self.rngs.shuffle.shuffle(&mut combat.draw);
        }
        combat.draw.reverse();
        combat.forget_draw_order();
        if let Some(index) = combat
            .draw
            .iter()
            .position(|card| card.enchantment == Some(Enchantment::PerfectFit))
        {
            let card = combat.draw.remove(index);
            combat.draw.push(card);
            combat.known_draw_top = 1;
        }
    }

    fn take_draw(&mut self) -> Option<Card> {
        let combat = self.combat()?;
        let unknown = combat
            .draw
            .len()
            .saturating_sub(combat.known_draw_bottom + combat.known_draw_top);
        if self.expectation.is_some() && combat.known_draw_top == 0 && unknown > 0 {
            let bottom = combat.known_draw_bottom;
            let choice = self.expectation_choice(unknown).unwrap_or(0);
            Some(self.combat_mut().unwrap().remove_draw(bottom + choice))
        } else {
            self.combat_mut().unwrap().pop_draw()
        }
    }

    fn creature(&self, actor: Actor) -> &Creature {
        let combat = self.combat().unwrap();
        match actor {
            Actor::Player => &combat.player,
            Actor::Osty => &combat.osty,
            Actor::Enemy(i) => &combat.enemies[i].creature,
        }
    }

    fn creature_mut(&mut self, actor: Actor) -> &mut Creature {
        let combat = self.combat_mut().unwrap();
        match actor {
            Actor::Player => &mut combat.player,
            Actor::Osty => &mut combat.osty,
            Actor::Enemy(i) => &mut combat.enemies[i].creature,
        }
    }

    fn finish_combat(&mut self, content: &Content) {
        let Some(combat) = self.combat() else {
            return;
        };
        if combat.player.hp <= 0 {
            self.run.hp = 0;
            self.phase = Phase::Dead;
        } else if combat.enemies.iter().all(|x| x.creature.hp <= 0)
            && combat
                .enemies
                .iter()
                .all(|x| x.creature.power(power_id::ADAPTABLE) == 0)
            && combat.choice.is_none()
            && combat.queue.is_empty()
            && combat.playing.is_none()
        {
            if self.has_relic(content, "RELIC.MEAT_ON_THE_BONE")
                && self.creature(Actor::Player).hp * 2 <= self.creature(Actor::Player).max_hp
            {
                let player = self.creature_mut(Actor::Player);
                player.hp = (player.hp + 12).min(player.max_hp);
            }
            self.trigger(content, Trigger::Victory, Actor::Player, 0);
            self.resolve(content);
            if self.has_relic(content, "RELIC.CHOSEN_CHEESE") {
                self.creature_mut(Actor::Player).hp += 1;
                self.creature_mut(Actor::Player).max_hp += 1;
            }
            let improvement = self
                .combat()
                .unwrap()
                .player
                .power(power_id::IMPROVEMENT)
                .max(0);
            for card in &mut self.run.deck {
                if content.cards[card.id as usize].id == "CARD.GUILTY" {
                    card.value += 1;
                }
            }
            self.run.deck.retain(|card| {
                content.cards[card.id as usize].id != "CARD.GUILTY" || card.value < 5
            });
            let mut upgradable: Vec<_> = self
                .run
                .deck
                .iter()
                .enumerate()
                .filter(|(_, card)| {
                    card.upgrades == 0
                        && !matches!(
                            card.card_type(content.cards[card.id as usize]),
                            CardType::Status | CardType::Curse | CardType::Quest
                        )
                })
                .map(|(index, _)| index)
                .collect();
            for _ in 0..improvement {
                if upgradable.is_empty() {
                    break;
                }
                let choice = self
                    .rngs
                    .combat_card_selection
                    .below(upgradable.len() as u32);
                self.run.deck[upgradable.remove(choice as usize)].upgrades = 1;
            }
            if let Some(combats) = &mut self.wongo_combats {
                *combats = combats.saturating_add(1);
            }
            if self.has_relic(content, "RELIC.LASTING_CANDY") {
                self.lasting_candy = self.lasting_candy.saturating_add(1);
            }
            if self.room == Room::Combat && self.has_relic(content, "RELIC.FISHING_ROD") {
                self.fishing_rod = (self.fishing_rod + 1) % 3;
                if self.fishing_rod == 0 {
                    let cards: Vec<_> = self
                        .run
                        .deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            card.upgrades == 0
                                && !matches!(
                                    card.card_type(content.cards[card.id as usize]),
                                    CardType::Status | CardType::Curse | CardType::Quest
                                )
                        })
                        .map(|(index, _)| index)
                        .collect();
                    if !cards.is_empty() {
                        let choice = self.rngs.niche.below(cards.len() as u32) as usize;
                        self.run.deck[cards[choice]].upgrades = 1;
                    }
                }
            }
            if self.room == Room::Elite && self.has_relic(content, "RELIC.WAR_HAMMER") {
                let mut cards: Vec<_> = self
                    .run
                    .deck
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        card.upgrades == 0
                            && !matches!(
                                card.card_type(content.cards[card.id as usize]),
                                CardType::Status | CardType::Curse | CardType::Quest
                            )
                    })
                    .map(|(index, _)| index)
                    .collect();
                self.rngs.niche.shuffle(&mut cards);
                for index in cards.into_iter().take(4) {
                    self.run.deck[index].upgrades = 1;
                }
            }
            if self.room == Room::Elite
                && let Some(index) = self
                    .run
                    .relics
                    .iter()
                    .position(|&id| content.relics[id as usize].id == "RELIC.SWORD_OF_STONE")
            {
                self.sword_of_stone += 1;
                if self.sword_of_stone >= 5 {
                    self.run.relics[index] = content.relic_id("RELIC.SWORD_OF_JADE").unwrap();
                }
            }
            if !self.paels_cards.is_empty() {
                let index = self.rngs.rewards.below(self.paels_cards.len() as u32) as usize;
                let mut card = self.paels_cards.remove(index);
                card.instance = 0;
                card.upgrades = card.upgrades.saturating_add(1);
                self.add_card(content, card);
            }
            if self.pumpkin_candle > 0 {
                self.pumpkin_candle -= 1;
            }
            if self.has_relic(content, "RELIC.TOY_BOX") && self.toy_box_combats < 12 {
                self.toy_box_combats += 1;
                if self.toy_box_combats % 3 == 0
                    && let Some(index) = self
                        .wax_relics
                        .iter()
                        .copied()
                        .find(|index| !self.melted_relics.contains(index))
                {
                    self.melted_relics.push(index);
                    if matches!(
                        content.relics[self.run.relics[index] as usize].id,
                        "RELIC.BLESSED_ANTLER"
                            | "RELIC.ECTOPLASM"
                            | "RELIC.PHILOSOPHERS_STONE"
                            | "RELIC.PRISMATIC_GEM"
                            | "RELIC.SOZU"
                            | "RELIC.SPIKED_GAUNTLETS"
                            | "RELIC.VELVET_CHOKER"
                            | "RELIC.WHISPERING_EARRING"
                    ) {
                        self.run.energy = self.run.energy.saturating_sub(1);
                    }
                }
            }
            let player = &self.combat().unwrap().player;
            let (hp, max_hp) = (player.hp, player.max_hp);
            let royalties = player.power(power_id::ROYALTIES) as i32;
            let removals = player.power(power_id::FORBIDDEN_GRIMOIRE).max(0) as u8;
            self.run.hp = hp;
            self.run.max_hp = max_hp;
            if self.event_combat != 0 {
                self.phase = self.finish_event_combat(content, royalties, removals);
                return;
            }
            self.phase = if self.room == Room::Boss && self.run.act == 3 {
                if self.run.ascension >= 10 && self.bosses_visited == 1 {
                    Phase::Map
                } else {
                    Phase::Won
                }
            } else if content.acts.get(self.act as usize).is_some() {
                let mut rewards = self.rewards(content);
                rewards.gold += royalties;
                rewards.removals = removals;
                Phase::Rewards(rewards)
            } else {
                Phase::Map
            };
        }
    }
}

fn can_enchant(
    content: &Content,
    card: &Card,
    enchantment: Enchantment,
    card_type: Option<CardType>,
) -> bool {
    let def = content.cards[card.id as usize];
    if card.enchantment.is_some()
        || matches!(
            def.card_type,
            CardType::Status | CardType::Curse | CardType::Quest
        )
        || card.flags(def) & UNPLAYABLE != 0
        || card_type.is_some_and(|card_type| def.card_type != card_type)
    {
        return false;
    }
    match enchantment {
        Enchantment::Corrupted
        | Enchantment::Instinct
        | Enchantment::Sharp
        | Enchantment::Vigorous => def.card_type == CardType::Attack,
        Enchantment::Goopy => def.tags & DEFEND_TAG != 0,
        Enchantment::Imbued => def.card_type == CardType::Skill,
        Enchantment::Nimble => effects_gain_block(def.effects),
        Enchantment::Slither => def.cost[card.upgrades.min(1) as usize] >= 0,
        Enchantment::SoulsPower => card.flags(def) & EXHAUST != 0,
        Enchantment::Spiral => {
            def.rarity == CardRarity::Basic && def.tags & (STRIKE_TAG | DEFEND_TAG) != 0
        }
        _ => true,
    }
}

fn upgrade_type(content: &Content, cards: &mut [Card], card_type: CardType) {
    for card in cards {
        if card.card_type(content.cards[card.id as usize]) == card_type {
            card.upgrades = 1;
        }
    }
}

fn upgrade_all(content: &Content, cards: &mut [Card]) {
    for card in cards {
        if card.upgrades == 0
            && !matches!(
                card.card_type(content.cards[card.id as usize]),
                CardType::Status | CardType::Curse | CardType::Quest
            )
        {
            card.upgrades = 1;
        }
    }
}

fn enchant_all(content: &Content, cards: &mut [Card], enchantment: Enchantment, amount: i16) {
    for card in cards {
        if can_enchant(content, card, enchantment, None) {
            card.enchantment = Some(enchantment);
            card.enchantment_amount = amount;
        }
    }
}

fn effects_gain_block(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| match effect {
        Effect::Block(..) | Effect::DodgeRoll(_) | Effect::ToricToughness(_) => true,
        Effect::If(_, yes, no) => effects_gain_block(yes) || effects_gain_block(no),
        Effect::Repeat(_, effects) | Effect::Random(_, effects) => effects_gain_block(effects),
        _ => false,
    })
}

pub(crate) fn eligible(content: &Content, card: &&Card, filter: CardFilter, op: CardOp) -> bool {
    let def = content.cards[card.id as usize];
    let matches = match filter {
        CardFilter::Any => true,
        CardFilter::Type(kind) => def.card_type == kind,
        CardFilter::AttackOrPower => {
            matches!(def.card_type, CardType::Attack | CardType::Power)
        }
        CardFilter::TypeWithoutTurnFlag(kind, flag) => {
            def.card_type == kind && card.turn_flags & flag == 0
        }
        CardFilter::WithoutFlag(flag) => card.flags(def) & flag == 0,
        CardFilter::NotType(kind) => def.card_type != kind,
        CardFilter::Id(id) => card.id == id,
        CardFilter::Cost(cost) => card_cost(**card, def, 0) == cost as i16,
        CardFilter::PlayableCost(cost) => {
            matches!(
                def.card_type,
                CardType::Attack | CardType::Skill | CardType::Power
            ) && def.cost[card.upgrades.min(1) as usize] >= 0
                && card_cost(**card, def, 0) == cost as i16
        }
        CardFilter::Flag(flag) => card.flags(def) & flag != 0,
        CardFilter::Upgradable => {
            card.upgrades == 0
                && !matches!(
                    def.card_type,
                    CardType::Status | CardType::Curse | CardType::Quest
                )
        }
        CardFilter::Colorless => content.colorless.contains(&card.id),
        CardFilter::Rare => def.rarity == CardRarity::Rare,
        CardFilter::NoReplay => {
            card.replays == 0
                && matches!(
                    def.card_type,
                    CardType::Attack | CardType::Skill | CardType::Power
                )
                && card.flags(def) & UNPLAYABLE == 0
        }
        CardFilter::PlayableOrAny => card.flags(def) & UNPLAYABLE == 0,
        CardFilter::CostsResource => {
            def.cost[card.upgrades.min(1) as usize] != 0
                || def.star_cost[card.upgrades.min(1) as usize] > 0
        }
    };
    matches
        && (!matches!(op, CardOp::Upgrade)
            || card.upgrades == 0
                && !matches!(
                    def.card_type,
                    CardType::Status | CardType::Curse | CardType::Quest
                ))
}

pub(crate) fn card_cost(card: Card, def: CardDef, energy: i16) -> i16 {
    if card.free
        || card.flags & FREE_COMBAT != 0
        || card.enchantment == Some(Enchantment::TezcatarasEmber)
    {
        return 0;
    }
    if let Some(cost) = card.cost_override {
        return cost as i16;
    }
    let cost = def.cost[card.upgrades.min(1) as usize];
    if cost < 0 {
        energy
    } else {
        (cost as i16 + card.cost_delta as i16).max(0)
    }
}

pub(crate) fn energy_cost(
    combat: &Combat,
    card: Card,
    def: CardDef,
    spiked_gauntlets: bool,
) -> i16 {
    let card_type = card.card_type(def);
    let void_free = combat.history.manual_cards < combat.player.power(power_id::VOID_FORM);
    if void_free
        || card_type == CardType::Power && combat.player.power(power_id::FREE_POWER) > 0
        || card_type == CardType::Skill && combat.player.power(power_id::FREE_SKILL) > 0
        || card_type == CardType::Skill && combat.player.power(power_id::CORRUPTION) > 0
    {
        return 0;
    }
    let cost = card_cost(card, def, combat.energy)
        + combat.player.power(power_id::BORROWED_TIME)
        + (card_type == CardType::Attack) as i16 * combat.player.power(power_id::TANGLED)
        - (card_type == CardType::Power) as i16 * combat.player.power(power_id::CURIOUS)
        + (card_type == CardType::Power && spiked_gauntlets) as i16;
    if card.id == card_id::PINPOINT {
        (cost - combat.history.skills).max(0)
    } else if card.id == card_id::BANSHEES_CRY {
        (cost - combat.history.ethereal * 2).max(0)
    } else if card.id == card_id::FLATTEN && combat.history.osty_attacks > 0
        || card.flags(def) & ETHEREAL != 0 && combat.player.power(power_id::VEILPIERCER) > 0
        || card_type == CardType::Attack && combat.player.power(power_id::FREE_ATTACK) > 0
    {
        0
    } else {
        cost.max(0)
    }
}

pub(crate) fn star_cost(combat: &Combat, card: Card, def: CardDef) -> i16 {
    if card.free
        || card.flags & FREE_COMBAT != 0
        || card.enchantment == Some(Enchantment::TezcatarasEmber)
        || combat.history.manual_cards < combat.player.power(power_id::VOID_FORM)
    {
        return 0;
    }
    match def.star_cost[card.upgrades.min(1) as usize] {
        -2 => combat.stars,
        cost => cost.max(0) as i16,
    }
}

pub(crate) fn trigger_orbit(combat: &mut Combat, cost: i16) {
    if cost <= 0 {
        return;
    }
    let mut gain: i16 = 0;
    for power in combat
        .player
        .powers
        .iter_mut()
        .filter(|power| power.id == power_id::ORBIT)
    {
        let before = power.value;
        power.value = power.value.saturating_add(cost);
        gain = gain.saturating_add(power.amount.saturating_mul(power.value / 4 - before / 4));
    }
    combat.orbit_spent = combat
        .player
        .powers
        .iter()
        .filter(|power| power.id == power_id::ORBIT)
        .map(|power| power.value)
        .max()
        .unwrap_or(0);
    combat.energy = combat.energy.saturating_add(gain);
}

fn cards(combat: &Combat, pile: Pile) -> &[Card] {
    match pile {
        Pile::Draw => &combat.draw,
        Pile::Hand => &combat.hand,
        Pile::Discard => &combat.discard,
        Pile::Exhaust => &combat.exhaust,
        Pile::Offer => &combat.offer,
    }
}

fn price(item: &ShopItem) -> i32 {
    match item {
        ShopItem::Card(_, x)
        | ShopItem::Relic(_, x)
        | ShopItem::Potion(_, x)
        | ShopItem::Remove(x) => *x,
    }
}

fn price_mut(item: &mut ShopItem) -> &mut i32 {
    match item {
        ShopItem::Card(_, x)
        | ShopItem::Relic(_, x)
        | ShopItem::Potion(_, x)
        | ShopItem::Remove(x) => x,
    }
}

fn with_price(mut item: ShopItem, price: i32) -> ShopItem {
    *price_mut(&mut item) = price;
    item
}

pub(crate) fn relic_group(id: Id) -> Option<usize> {
    [COMMON_RELICS, UNCOMMON_RELICS, RARE_RELICS, SHOP_RELICS]
        .iter()
        .position(|relics| relics.contains(&id))
}

pub(crate) fn event_page(id: Id, actions: &[u8]) -> Phase {
    Phase::Event(
        id,
        actions
            .iter()
            .map(|action| EventOption {
                requirement: Requirement::Always,
                effects: match action {
                    0 => &[RunEffect::EventAction(0)],
                    1 => &[RunEffect::EventAction(1)],
                    2 => &[RunEffect::EventAction(2)],
                    3 => &[RunEffect::EventAction(3)],
                    4 => &[RunEffect::EventAction(4)],
                    5 => &[RunEffect::EventAction(5)],
                    6 => &[RunEffect::EventAction(6)],
                    7 => &[RunEffect::EventAction(7)],
                    8 => &[RunEffect::EventAction(8)],
                    10 => &[RunEffect::EventAction(10)],
                    11 => &[RunEffect::EventAction(11)],
                    12 => &[RunEffect::EventAction(12)],
                    _ => &[],
                },
            })
            .collect(),
    )
}

fn relic_rarity(roll: f32) -> usize {
    if roll < 0.5 {
        0
    } else if roll < 0.83 {
        1
    } else {
        2
    }
}

pub(crate) fn relic_deques(values: impl Iterator<Item = Id>, rng: &mut Rng) -> [Vec<Id>; 4] {
    let mut deques: [Vec<Id>; 4] = std::array::from_fn(|_| vec![]);
    for id in values {
        if let Some(rarity) = relic_group(id) {
            deques[rarity].push(id);
        }
    }
    for rarity in [1, 0, 2, 3] {
        rng.shuffle(&mut deques[rarity]);
    }
    deques
}

fn relic_allowed_in_shop(id: Id) -> bool {
    ![21, 44, 135, 161, 247].contains(&id)
}

fn push_effects(queue: &mut Vec<Pending>, effects: &'static [Effect], context: Context) {
    queue.extend(
        effects
            .iter()
            .rev()
            .map(|&effect| Pending { effect, context }),
    );
}

fn push_card_effects(queue: &mut Vec<Pending>, card: Card, def: CardDef, context: Context) {
    if def.id != "CARD.MAD_SCIENCE" {
        push_effects(queue, def.effects, context);
        return;
    }
    let rider = card.variant % 10;
    let mut effects = match card.card_type(def) {
        CardType::Attack => vec![Effect::Attack(
            Target::ChosenEnemy,
            Amount::fixed(12, 12),
            if rider == 2 { 3 } else { 1 },
        )],
        CardType::Skill => vec![Effect::Block(Target::Player, Amount::fixed(8, 8))],
        CardType::Power => match rider {
            7 => vec![
                Effect::ApplyPower(Target::Player, power_id::STRENGTH, Amount::fixed(2, 2)),
                Effect::ApplyPower(Target::Player, power_id::DEXTERITY, Amount::fixed(2, 2)),
            ],
            8 => vec![Effect::ApplyPower(
                Target::Player,
                power_id::CURIOUS,
                Amount::fixed(1, 1),
            )],
            9 => vec![Effect::ApplyPower(
                Target::Player,
                power_id::IMPROVEMENT,
                Amount::fixed(1, 1),
            )],
            _ => vec![],
        },
        _ => vec![],
    };
    match rider {
        1 => effects.extend([
            Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(2, 2)),
            Effect::ApplyPower(
                Target::ChosenEnemy,
                power_id::VULNERABLE,
                Amount::fixed(2, 2),
            ),
        ]),
        3 => effects.push(Effect::ApplyPower(
            Target::ChosenEnemy,
            power_id::STRANGLE,
            Amount::fixed(6, 6),
        )),
        4 => effects.push(Effect::Energy(2)),
        5 => effects.push(Effect::Draw(3)),
        6 => effects.push(Effect::DistinctCharacter(Pile::Hand, 1, true)),
        _ => {}
    }
    queue.extend(
        effects
            .into_iter()
            .rev()
            .map(|effect| Pending { effect, context }),
    );
}

fn effects_attack(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| match effect {
        Effect::Attack(..) => true,
        Effect::If(_, yes, no) => effects_attack(yes) || effects_attack(no),
        Effect::Repeat(_, effects) | Effect::Random(_, effects) => effects_attack(effects),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_test_subject_keeps_its_revive_move() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let id = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.TEST_SUBJECT")
            .unwrap() as Id;
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.enemies.truncate(1);
        let enemy = &mut combat.enemies[0];
        enemy.creature.id = id;
        enemy.creature.hp = 0;
        enemy.creature.max_hp = 100;
        enemy.creature.add_power(power_id::ADAPTABLE, 1);
        enemy.move_index = 0;
        enemy.last_move = 0;

        game.roll_moves(&content);
        assert_eq!(game.combat().unwrap().enemies[0].move_index, 0);
        game.step(&content, Action::EndTurn).unwrap();
        assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 200);
    }

    #[test]
    fn corpse_slug_glomp_attacks_and_applies_weak() {
        let content = foundation_content();
        let slug = content
            .enemies
            .iter()
            .find(|enemy| enemy.id == "MONSTER.CORPSE_SLUG")
            .unwrap();
        assert!(matches!(
            slug.moves[1].effects,
            [
                Effect::Attack(Target::Player, ..),
                Effect::ApplyPower(
                    Target::Player,
                    power_id::WEAK,
                    Amount {
                        base: 1,
                        upgraded: 1,
                        ..
                    }
                ),
            ]
        ));
    }

    #[test]
    fn cinder_attacks_then_uses_one_card_selection_draw() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 2, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let cinder = content.card_id("CARD.CINDER").unwrap();
        let strike = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
        let cards = [11, 12, 13].map(|instance| Card {
            id: strike,
            instance,
            ..Card::default()
        });
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = std::iter::once(Card {
            id: cinder,
            instance: 10,
            ..Card::default()
        })
        .chain(cards)
        .collect();
        combat.discard.clear();
        combat.exhaust.clear();
        combat.queue.clear();
        combat.player.powers.clear();
        combat.enemies.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.enemies[0].creature.max_hp = 100;
        combat.enemies[0].creature.block = 0;
        combat.enemies[0].creature.powers.clear();
        combat.energy = 10;
        let mut expected_rng = game.rngs.combat_card_selection.clone();
        let expected = cards[expected_rng.below(cards.len() as u32) as usize].instance;
        let draws = game.rngs.combat_card_selection.1;

        game.step(
            &content,
            Action::Play {
                hand: 0,
                target: Some(0),
            },
        )
        .unwrap();

        let combat = game.combat().unwrap();
        assert_eq!(combat.enemies[0].creature.hp, 82);
        assert_eq!(combat.exhaust.len(), 1);
        assert_eq!(combat.exhaust[0].instance, expected);
        assert_eq!(game.rngs.combat_card_selection.1, draws + 1);
    }

    #[test]
    fn leaving_play_clears_temporary_cost_state() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 3, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let card = Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            free: true,
            cost_override: Some(0),
            ..Card::default()
        };

        let (card, exhausted) = game.finish_card_destination(&content, card, false, false);

        assert!(!exhausted);
        assert!(!card.free);
        assert_eq!(card.cost_override, None);
        let card = game.combat().unwrap().discard.last().unwrap();
        assert!(!card.free);
        assert_eq!(card.cost_override, None);
    }

    #[test]
    fn fresh_runs_start_in_a_real_first_act_at_neow() {
        let content = foundation_content();
        let acts = ["ACT.THE_OVERGROWTH", "ACT.THE_UNDERDOCKS"]
            .map(|name| content.acts.iter().position(|act| act.id == name).unwrap() as Id);
        let mut seen = [false; 2];
        for seed in 1..=8 {
            let mut game = Game::new_character_ascension(&content, seed, 0, 10).unwrap();
            let selected = game.rngs.up_front.clone().below(2) as usize;
            game.begin_run(&content).unwrap();
            seen[selected] = true;
            assert_eq!(game.act, acts[selected]);
            assert_eq!(game.run.act, 1);
            assert_eq!(game.run.floor, 1);
            assert_eq!(game.room, Room::Event);
            assert_eq!(game.map.nodes[game.map.current.unwrap()].floor, 0);
            assert!(matches!(
                game.phase,
                Phase::Event(id, _) if content.events[id as usize].id == "EVENT.NEOW"
            ));
            assert!(
                game.actions(&content)
                    .iter()
                    .all(|action| matches!(action, Action::Event(_)))
            );
        }
        assert_eq!(seen, [true, true]);
    }

    #[test]
    fn single_player_neow_never_offers_massive_scroll() {
        let content = foundation_content();
        let massive_scroll = content.relic_id("RELIC.MASSIVE_SCROLL").unwrap() as i64 + 1;
        let template = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
        for seed in 1..=128 {
            let mut game = template.clone();
            game.rngs = Rngs::from_seed(seed);
            game.seed = seed as u32;
            game.begin_run(&content).unwrap();
            assert!(!game.event_data.contains(&massive_scroll));
        }
    }

    #[test]
    fn later_acts_reveal_their_floor_zero_ancient_deterministically() {
        let content = foundation_content();
        for (previous, name, allowed) in [
            (
                1,
                "ACT.THE_HIVE",
                [
                    "EVENT.OROBAS",
                    "EVENT.PAEL",
                    "EVENT.TEZCATARA",
                    "EVENT.DARV",
                ],
            ),
            (
                2,
                "ACT.THE_GLORY",
                ["EVENT.NONUPEIPE", "EVENT.TANX", "EVENT.VAKUU", "EVENT.DARV"],
            ),
        ] {
            let act = content.acts.iter().position(|act| act.id == name).unwrap() as Id;
            let mut first = Game::new_character_ascension(&content, 71, 0, 10).unwrap();
            first.run.act = previous;
            first.run.hp = 10;
            let mut second = first.clone();
            first.begin_act(&content, act).unwrap();
            second.begin_act(&content, act).unwrap();
            assert_eq!(first.run.floor, 0);
            assert_eq!(first.run.hp, 10);
            let actions = first.actions(&content);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions, second.actions(&content));
            assert!(matches!(actions[0], Action::Path(index) if first.map.nodes[index].floor == 0));

            let draws = first.rngs.up_front.1;
            first.step(&content, actions[0].clone()).unwrap();
            second.step(&content, actions[0].clone()).unwrap();
            let Phase::Event(event, _) = first.phase else {
                panic!("ancient event did not start")
            };
            assert!(allowed.contains(&content.events[event as usize].id));
            assert_eq!(first.run.floor, 1);
            assert_eq!(first.run.hp, 66);
            assert_eq!(first.rngs.up_front.1, draws + 1);
            assert_eq!(format!("{:?}", first), format!("{:?}", second));
        }
    }

    #[test]
    fn standard_acts_use_ordinal_map_profiles() {
        let content = foundation_content();
        for (number, name, boss_floor, rooms) in [
            (1, "ACT.THE_OVERGROWTH", 14, [21, 11, 5, 8, 3]),
            (1, "ACT.THE_UNDERDOCKS", 14, [21, 11, 5, 8, 3]),
            (2, "ACT.THE_HIVE", 15, [25, 10, 5, 11, 3]),
            (3, "ACT.THE_GLORY", 16, [23, 13, 5, 9, 3]),
        ] {
            let act = content.acts.iter().position(|act| act.id == name).unwrap() as Id;
            let mut game = Game::new_character_ascension(&content, 73, 0, 0).unwrap();
            game.run.act = number - 1;
            game.begin_act(&content, act).unwrap();
            assert_eq!(game.run.act, number);
            assert_eq!(
                game.map
                    .nodes
                    .iter()
                    .filter(|node| node.room == Room::Boss)
                    .map(|node| node.floor)
                    .max(),
                Some(boss_floor)
            );
            assert_eq!(
                [
                    Room::Combat,
                    Room::Unknown,
                    Room::Elite,
                    Room::Rest,
                    Room::Shop
                ]
                .map(|room| game
                    .map
                    .nodes
                    .iter()
                    .filter(|node| node.room == room)
                    .count()),
                rooms
            );
            if number == 3 {
                let boss = game
                    .map
                    .nodes
                    .iter()
                    .position(|node| node.room == Room::Boss)
                    .unwrap();
                game.enter_room(&content, boss);
                let combat = game.combat_mut().unwrap();
                combat.enemies.iter_mut().for_each(|enemy| {
                    enemy.creature.hp = 0;
                    enemy.creature.powers.clear();
                });
                combat.queue.clear();
                game.finish_combat(&content);
                assert!(matches!(game.phase, Phase::Won));
                assert_eq!((game.run.act - 1) as u16 * 17 + game.run.floor as u16, 51);
            }
        }
    }

    #[test]
    fn forced_full_run_reaches_both_a10_bosses_and_wins() {
        fn win_combat(game: &mut Game, content: &Content) {
            let combat = game.combat_mut().unwrap();
            combat.enemies.iter_mut().for_each(|enemy| {
                enemy.creature.hp = 0;
                enemy.creature.powers.clear();
            });
            combat.choice = None;
            combat.queue.clear();
            combat.playing = None;
            game.finish_combat(content);
        }

        fn enter_first_boss(game: &mut Game, content: &Content) {
            let boss = game
                .map
                .nodes
                .iter()
                .position(|node| {
                    node.room == Room::Boss && (game.run.act < 3 || !node.next.is_empty())
                })
                .unwrap();
            game.enter_room(content, boss);
            win_combat(game, content);
        }

        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 72, 0, 10).unwrap();
        game.begin_run(&content).unwrap();
        assert!(matches!(
            content.acts[game.act as usize].id,
            "ACT.THE_OVERGROWTH" | "ACT.THE_UNDERDOCKS"
        ));
        assert!(matches!(
            game.phase,
            Phase::Event(id, _) if content.events[id as usize].id == "EVENT.NEOW"
        ));

        for (act, ancient) in [
            (
                2,
                [
                    "EVENT.OROBAS",
                    "EVENT.PAEL",
                    "EVENT.TEZCATARA",
                    "EVENT.DARV",
                ]
                .as_slice(),
            ),
            (
                3,
                ["EVENT.NONUPEIPE", "EVENT.TANX", "EVENT.VAKUU", "EVENT.DARV"].as_slice(),
            ),
        ] {
            enter_first_boss(&mut game, &content);
            assert!(matches!(game.phase, Phase::Rewards(_)));
            game.step(&content, Action::Leave).unwrap();
            assert_eq!(game.run.act, act);
            let path = game
                .actions(&content)
                .into_iter()
                .find(|action| matches!(action, Action::Path(_)))
                .unwrap();
            game.step(&content, path).unwrap();
            let Phase::Event(id, _) = game.phase else {
                panic!("ancient event did not start")
            };
            assert!(ancient.contains(&content.events[id as usize].id));
        }

        enter_first_boss(&mut game, &content);
        assert_eq!(game.bosses_visited, 1);
        assert!(matches!(game.phase, Phase::Map));
        let second = game
            .actions(&content)
            .into_iter()
            .find(|action| matches!(action, Action::Path(_)))
            .unwrap();
        game.step(&content, second).unwrap();
        assert_eq!(game.bosses_visited, 2);
        win_combat(&mut game, &content);
        assert!(matches!(game.phase, Phase::Won));
    }

    #[test]
    fn forced_run_choice_finishes_when_candidates_run_out() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        for card in &mut game.run.deck {
            card.upgrades = 1;
        }
        game.run.deck[0].upgrades = 0;
        game.phase = Phase::Rewards(Rewards {
            gold: 0,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![content.relic_id("RELIC.YUMMY_COOKIE").unwrap()],
            potions: vec![],
            removals: 0,
        });

        game.step(&content, Action::RewardRelic(0)).unwrap();
        assert!(matches!(game.phase, Phase::UpgradeCards(4, false)));
        game.step(&content, Action::Smith(0)).unwrap();

        assert_eq!(game.run.deck[0].upgrades, 1);
        assert!(matches!(game.phase, Phase::Rewards(_)));
        assert!(!game.actions(&content).is_empty());
    }

    #[test]
    fn stampede_autoplay_choice_pauses_end_turn() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let dagger = content.card_id("CARD.DAGGER_THROW").unwrap();
        let defend = content.card_id("CARD.DEFEND_IRONCLAD").unwrap();
        let combat = game.combat_mut().unwrap();
        combat.hand = vec![
            Card {
                id: dagger,
                upgrades: 1,
                instance: 1,
                ..Card::default()
            },
            Card {
                id: defend,
                instance: 2,
                ..Card::default()
            },
        ];
        combat.draw = vec![Card {
            id: defend,
            instance: 3,
            ..Card::default()
        }];
        combat.discard.clear();
        combat.exhaust.clear();
        combat.queue.clear();
        combat.player.powers.clear();
        combat.player.add_power(power_id::STAMPEDE, 1);
        combat.enemies.truncate(1);
        combat.enemies[0].creature.hp = 200;
        combat.enemies[0].creature.max_hp = 200;
        combat.enemies[0].creature.powers.clear();

        game.step(&content, Action::EndTurn).unwrap();

        let combat = game.combat().unwrap();
        assert!(combat.choice.is_some());
        assert_eq!(combat.hand.len(), 2);
        assert!(
            combat
                .queue
                .iter()
                .any(|pending| matches!(pending.effect, Effect::ContinueEndTurn))
        );
        assert_eq!(
            game.actions(&content),
            [Action::Choose(0), Action::Choose(1)]
        );

        game.step(&content, Action::Choose(0)).unwrap();

        let combat = game.combat().unwrap();
        assert!(combat.choice.is_none());
        assert!(combat.queue.is_empty());
        assert!(combat.auto_plays.is_empty());
        assert_eq!(combat.turn, 2);
        assert!(!game.actions(&content).is_empty());
    }

    #[test]
    fn smoggy_allows_only_one_skill_each_turn() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![
            Card {
                id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
                ..Card::default()
            },
            Card {
                id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
                ..Card::default()
            },
        ];
        combat.energy = 10;
        combat.player.add_power(power_id::SMOGGY, 1);
        assert!(game.actions(&content).contains(&Action::Play {
            hand: 0,
            target: None
        }));

        game.combat_mut().unwrap().history.skills = 1;
        let actions = game.actions(&content);
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::Play {
                hand: 0,
                target: None
            }
        )));
        assert!(actions.contains(&Action::Play {
            hand: 1,
            target: Some(0)
        }));
    }

    #[test]
    fn fishing_rod_upgrades_every_third_combat() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run
            .relics
            .push(content.relic_id("RELIC.FISHING_ROD").unwrap());
        for card in &mut game.run.deck {
            card.upgrades = 1;
        }
        game.run.deck[0].upgrades = 0;
        game.fishing_rod = 2;
        let combat = game.combat_mut().unwrap();
        for enemy in &mut combat.enemies {
            enemy.creature.hp = 0;
            enemy
                .creature
                .powers
                .retain(|power| power.id != power_id::ADAPTABLE);
        }
        combat.queue.clear();
        combat.choice = None;
        combat.playing = None;

        game.finish_combat(&content);

        assert_eq!(game.fishing_rod, 0);
        assert_eq!(game.run.deck[0].upgrades, 1);
        assert!(matches!(game.phase, Phase::Rewards(_)));
    }

    #[test]
    fn spawned_enemies_reuse_dead_slots_and_clear_slot_state() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let combat = game.combat_mut().unwrap();
        combat.enemies.truncate(1);
        combat.hits.truncate(1);
        combat.enemies[0].creature.hp = 0;
        combat.hits[0] = 9;
        combat.enemy_power_snapshot = vec![vec![Power {
            id: power_id::STRENGTH,
            amount: 2,
            skip_next_decay: false,
            value: 0,
        }]];

        let slot = game.spawn_enemy(&content, "MONSTER.TOUGH_EGG");

        let combat = game.combat().unwrap();
        assert_eq!(slot, 0);
        assert_eq!(combat.enemies.len(), 1);
        assert_eq!(combat.hits, [0]);
        assert!(combat.enemy_power_snapshot[0].is_empty());
        assert_eq!(
            content.enemies[combat.enemies[0].creature.id as usize].id,
            "MONSTER.TOUGH_EGG"
        );
    }

    #[test]
    fn death_spawns_use_the_slots_they_return() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let template = game.combat().unwrap().enemies[0].clone();
        let axebot = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.AXEBOT")
            .unwrap() as Id;
        let merc = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.GREMLIN_MERC")
            .unwrap() as Id;
        let combat = game.combat_mut().unwrap();
        combat.enemies = vec![template.clone(), template.clone(), template];
        combat.hits = vec![9, 8, 7];
        combat.enemy_power_snapshot = vec![
            vec![Power {
                id: power_id::STRENGTH,
                amount: 1,
                skip_next_decay: false,
                value: 0,
            }];
            3
        ];
        combat.enemies[0].creature.hp = 0;
        combat.enemies[1].creature.id = merc;
        combat.enemies[1].creature.hp = 1;
        combat.enemies[1].creature.powers = vec![
            Power {
                id: power_id::SURPRISE,
                amount: 1,
                skip_next_decay: false,
                value: 0,
            },
            Power {
                id: power_id::THIEVERY,
                amount: 1,
                skip_next_decay: false,
                value: 17,
            },
        ];

        game.kill_actor(&content, Actor::Enemy(1));

        let combat = game.combat().unwrap();
        assert_eq!(combat.enemies.len(), 3);
        assert_eq!(combat.hits[..2], [0, 0]);
        assert!(combat.enemy_power_snapshot[..2].iter().all(Vec::is_empty));
        assert_eq!(
            content.enemies[combat.enemies[0].creature.id as usize].id,
            "MONSTER.SNEAKY_GREMLIN"
        );
        assert_eq!(
            content.enemies[combat.enemies[1].creature.id as usize].id,
            "MONSTER.FAT_GREMLIN"
        );
        assert_eq!(combat.enemies[1].creature.power(power_id::HEIST), 17);
        assert_ne!(combat.enemies[2].creature.id, axebot);
        assert_eq!(combat.enemies[2].creature.power(power_id::HEIST), 0);
    }

    #[test]
    fn stock_and_wrigglers_reuse_dead_slots() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let template = game.combat().unwrap().enemies[0].clone();
        let combat = game.combat_mut().unwrap();
        combat.enemies = vec![template.clone(), template];
        combat.hits = vec![9, 8];
        combat.enemy_power_snapshot = vec![vec![], vec![]];
        combat.enemies[0].creature.hp = 1;
        combat.enemies[0].creature.powers.clear();
        combat.enemies[0].creature.add_power(power_id::STOCK, 2);

        game.kill_actor(&content, Actor::Enemy(0));

        {
            let combat = game.combat().unwrap();
            assert_eq!(combat.enemies.len(), 2);
            assert_eq!(
                content.enemies[combat.enemies[0].creature.id as usize].id,
                "MONSTER.AXEBOT"
            );
            assert_eq!(combat.enemies[0].creature.power(power_id::STOCK), 1);
        }
        game.combat_mut().unwrap().enemies[0].creature.hp = 0;
        game.spawn_wrigglers(&content, 1);
        let combat = game.combat().unwrap();
        assert_eq!(combat.enemies.len(), 2);
        assert_eq!(combat.hits[0], 0);
        assert_eq!(
            content.enemies[combat.enemies[0].creature.id as usize].id,
            "MONSTER.WRIGGLER"
        );
    }

    #[test]
    fn hidden_event_outcomes_are_drawn_at_choice_time() {
        let content = foundation_content();
        for name in ["EVENT.STONE_OF_ALL_TIME", "EVENT.THE_FUTURE_OF_POTIONS"] {
            let id = content
                .events
                .iter()
                .position(|event| event.id == name)
                .unwrap() as Id;
            let mut first = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
            first.begin_act(&content, 0).unwrap();
            first.run.potions = vec![Some(UNCOMMON_POTIONS[0]), Some(UNCOMMON_POTIONS[1]), None];
            first.phase = Phase::Event(id, content.events[id as usize].options.to_vec());
            first.event_rng = Some(Rng::from_seed(42));
            first.event_data = [1; 4];
            let mut second = first.clone();
            second.event_data = [3; 4];

            let action = Action::Event((name == "EVENT.THE_FUTURE_OF_POTIONS") as usize);
            first.step(&content, action.clone()).unwrap();
            second.step(&content, action).unwrap();
            assert_eq!(first.run.potions, second.run.potions);
            assert_eq!(first.run.hp, second.run.hp);
            assert_eq!(first.run.max_hp, second.run.max_hp);
            assert_eq!(format!("{:?}", first.phase), format!("{:?}", second.phase));
        }
    }

    #[test]
    fn hp_loss_events_persist_across_turns() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.combat_mut().unwrap().history.hp_loss_events = 3;

        game.start_turn(&content);

        assert_eq!(game.combat().unwrap().history.hp_loss_events, 3);
    }

    #[test]
    fn generated_encounters_align_enemy_hit_counters() {
        let content = foundation_content();
        let encounter = content
            .encounters
            .iter()
            .position(|encounter| encounter.id.ends_with("SLITHERING_STRANGLER_NORMAL"))
            .unwrap() as Id;
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, encounter).unwrap();
        let target = game.combat().unwrap().enemies.len() - 1;
        assert_eq!(game.combat().unwrap().hits.len(), target + 1);
        game.damage(
            &content,
            Actor::Player,
            Actor::Enemy(target),
            1,
            DamageKind::Attack,
            None,
        );
        assert_eq!(game.combat().unwrap().hits[target], 1);
    }

    #[test]
    fn channeling_with_no_slots_creates_one() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let combat = game.combat_mut().unwrap();
        combat.orb_slots = 0;
        combat.orbs.clear();
        combat.queue.push(Pending {
            effect: Effect::Channel(orb_id::LIGHTNING, 1),
            context: Context::player(),
        });

        game.resolve(&content);

        let combat = game.combat().unwrap();
        assert_eq!(combat.orb_slots, 1);
        assert_eq!(combat.orbs.len(), 1);
        assert_eq!(
            (combat.orbs[0].id, combat.orbs[0].value),
            (orb_id::LIGHTNING, 0)
        );
    }

    #[test]
    fn channel_slots_resolves_each_overflow_before_the_next() {
        let content = foundation_content();
        let frost = content
            .orbs
            .iter()
            .position(|orb| orb.id == "ORB.FROST_ORB")
            .unwrap() as Id;
        let dark = content
            .orbs
            .iter()
            .position(|orb| orb.id == "ORB.DARK_ORB")
            .unwrap() as Id;
        for (hp, expected_hp, block, orbs) in [
            (1_000, 986, 5, [dark, dark, dark]),
            (5, 0, 0, [frost, dark, dark]),
        ] {
            let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
            game.begin_act(&content, 0).unwrap();
            game.start_combat(&content, content.acts[0].encounters[0])
                .unwrap();
            let combat = game.combat_mut().unwrap();
            combat.enemies.truncate(1);
            combat.hits.truncate(1);
            combat.enemy_power_snapshot.truncate(1);
            combat.enemies[0].creature.hp = hp;
            combat.enemies[0].creature.max_hp = 1_000;
            combat.enemies[0].creature.powers.clear();
            combat.player.block = 0;
            combat.orb_slots = 3;
            combat.orbs = vec![
                Orb {
                    id: orb_id::LIGHTNING,
                    value: 0,
                },
                Orb {
                    id: frost,
                    value: 0,
                },
                Orb { id: dark, value: 6 },
            ];
            combat.queue.clear();
            combat.queue.push(Pending {
                effect: Effect::ChannelSlots(dark),
                context: Context::player(),
            });

            game.resolve(&content);

            let combat = game.combat().unwrap();
            assert_eq!(combat.enemies[0].creature.hp, expected_hp);
            assert_eq!(combat.player.block, block);
            assert_eq!(
                combat.orbs.iter().map(|orb| orb.id).collect::<Vec<_>>(),
                orbs
            );
        }
    }

    #[test]
    fn negative_focus_does_not_reduce_stored_orb_values() {
        let content = foundation_content();
        let dark = content
            .orbs
            .iter()
            .position(|orb| orb.id == "ORB.DARK_ORB")
            .unwrap() as Id;
        let glass = content
            .orbs
            .iter()
            .position(|orb| orb.id == "ORB.GLASS_ORB")
            .unwrap() as Id;
        let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let combat = game.combat_mut().unwrap();
        combat.enemies.truncate(1);
        combat.enemies[0].creature.hp = 100;
        combat.player.add_power(power_id::FOCUS, -10);
        combat.orbs = vec![
            Orb { id: dark, value: 6 },
            Orb {
                id: glass,
                value: 4,
            },
        ];

        game.passive_orb(&content, Some(0));
        game.passive_orb(&content, Some(1));
        game.resolve(&content);

        let combat = game.combat().unwrap();
        assert_eq!(combat.enemies[0].creature.hp, 100);
        assert_eq!(
            combat.orbs.iter().map(|orb| orb.value).collect::<Vec<_>>(),
            [6, 4]
        );
    }

    #[test]
    fn divine_destiny_replaces_divine_right_and_grants_six_stars() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.obtain_relic(&content, content.relic_id("RELIC.TOUCH_OF_OROBAS").unwrap());
        assert!(game.has_relic(&content, "RELIC.DIVINE_DESTINY"));
        assert!(!game.has_relic(&content, "RELIC.DIVINE_RIGHT"));

        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();

        assert_eq!(game.combat().unwrap().stars, 6);
        game.start_turn(&content);
        assert_eq!(game.combat().unwrap().stars, 6);
    }

    #[test]
    fn orbit_instances_keep_independent_energy_counters() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.combat_mut().unwrap().energy = 0;

        game.apply_power(&content, Actor::Player, power_id::ORBIT, 2);
        trigger_orbit(game.combat_mut().unwrap(), 3);
        game.apply_power(&content, Actor::Player, power_id::ORBIT, 3);
        trigger_orbit(game.combat_mut().unwrap(), 1);
        assert_eq!(game.combat().unwrap().energy, 2);
        trigger_orbit(game.combat_mut().unwrap(), 3);

        let combat = game.combat().unwrap();
        let orbit: Vec<_> = combat
            .player
            .powers
            .iter()
            .filter(|power| power.id == power_id::ORBIT)
            .map(|power| (power.amount, power.value))
            .collect();
        assert_eq!(orbit, [(2, 7), (3, 4)]);
        assert_eq!(combat.energy, 5);
    }

    #[test]
    fn mini_regent_triggers_once_each_turn() {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.run
            .relics
            .push(content.relic_id("RELIC.MINI_REGENT").unwrap());
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let falling_star = content.card_id("CARD.FALLING_STAR").unwrap();
        for expected in [1, 2] {
            let combat = game.combat_mut().unwrap();
            combat.hand = vec![Card {
                id: falling_star,
                ..Card::default()
            }];
            combat.stars = 3;
            combat.enemies[0].creature.hp = 1_000;
            combat.enemies[0].creature.max_hp = 1_000;
            game.step(
                &content,
                Action::Play {
                    hand: 0,
                    target: Some(0),
                },
            )
            .unwrap();
            assert_eq!(
                game.creature(Actor::Player).power(power_id::STRENGTH),
                expected
            );
            if expected == 1 {
                game.start_turn(&content);
            }
        }
    }
}
