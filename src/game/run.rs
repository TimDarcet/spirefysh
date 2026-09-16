use super::*;

impl Game {
    pub(super) fn expectation_choice(&mut self, count: usize) -> Option<usize> {
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

    pub(super) fn expectation_unknown(&mut self) {
        if let Some(expectation) = &mut self.expectation {
            expectation.unknown = true;
        }
    }

    pub(super) fn expectation_discard(&mut self, count: usize) {
        if let Some(expectation) = &mut self.expectation {
            expectation.discarded = expectation.discarded.saturating_add(count as i16);
        }
    }

    pub(super) fn expectation_draw(&mut self) {
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

    pub(super) fn combat_mut(&mut self) -> Option<&mut Combat> {
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
                || def.id == "CARD.SHIV" && combat.player.power(power_id::FAN_OF_KNIVES) > 0
            {
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
        let hidden_event_choice = matches!(self.phase, Phase::Event(..))
            && matches!(action, Action::EventRelic(..) | Action::EventCard(..));
        if !self.actions(content).contains(&action) && !hidden_event_choice {
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

    pub fn step_with_rng(&mut self, content: &Content, action: Action) -> Result<bool, Error> {
        let before = (self.rngs.positions(), self.event_rng.map(|rng| rng.1));
        self.step(content, action)?;
        Ok(before != (self.rngs.positions(), self.event_rng.map(|rng| rng.1)))
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
            for slot in &mut grid[row] {
                let Some(stray) = slot.filter(|&id| id != treasure) else {
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
                *slot = None;
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
            unassigned.extend(
                grid.iter()
                    .take(length)
                    .flatten()
                    .filter_map(|&slot| slot.filter(|&id| nodes[id].room.is_none())),
            );
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
            for id in row.iter().flatten() {
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
                for row in grid.iter_mut().take(length) {
                    let old = *row;
                    *row = [None; 7];
                    for id in old.into_iter().flatten() {
                        let col = (nodes[id].col as i8 + shift) as usize;
                        nodes[id].col = col;
                        row[col] = Some(id);
                    }
                }
            }
            for row in grid.iter_mut().take(length) {
                let row_nodes: Vec<_> = row.iter().flatten().copied().collect();
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
                            .filter(|&col| col == old || row[col].is_none())
                            .max_by_key(|&col| (gap(col), std::cmp::Reverse(col)))
                            .unwrap_or(old);
                        if gap(next) > gap(old) {
                            row[old] = None;
                            row[next] = Some(id);
                            nodes[id].col = next;
                            moved = true;
                        }
                    }
                    if !moved {
                        break;
                    }
                }
            }
            for row in grid.iter_mut().take(length) {
                for col in 0..7 {
                    let Some(id) = row[col] else { continue };
                    if nodes[id].parents.len() != 1 || nodes[id].children.len() != 1 {
                        continue;
                    }
                    let parent = nodes[id].parents[0];
                    let child = nodes[id].children[0];
                    let next = if nodes[id].col < nodes[parent].col
                        && nodes[id].col < nodes[child].col
                    {
                        (col < 6 && row[col + 1].is_none()).then_some(col + 1)
                    } else if nodes[id].col > nodes[parent].col && nodes[id].col > nodes[child].col
                    {
                        (col > 0 && row[col - 1].is_none()).then_some(col - 1)
                    } else {
                        None
                    };
                    if let Some(next) = next {
                        row[col] = None;
                        row[next] = Some(id);
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
        for row in grid.iter().take(length).skip(1) {
            ids.extend(row.iter().flatten().copied());
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
                .collect::<Vec<_>>()
                .into(),
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
            nodes: nodes.into(),
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

    pub(crate) fn event_allowed(&self, content: &Content, id: Id) -> bool {
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
                if self.paels_wing.is_multiple_of(2) {
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

    pub(super) fn enter_room(&mut self, content: &Content, node: usize) {
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
            let next = &self.map.nodes[node].next;
            let shop_allowed = previous != Room::Shop
                && (next.is_empty()
                    || !next
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

    pub(super) fn start_event_combat(&mut self, content: &Content, encounter: &str, event: u8) {
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

    pub(super) fn rewards(&mut self, content: &Content) -> Rewards {
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

    pub(super) fn finish_event_combat(
        &mut self,
        content: &Content,
        royalties: i32,
        removals: u8,
    ) -> Phase {
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
        if self.lasting_candy > 0 && self.lasting_candy.is_multiple_of(2) {
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

    fn modify_reward_cards(&mut self, content: &Content, cards: &mut [Card]) {
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

    pub(super) fn has_relic(&self, content: &Content, wanted: &str) -> bool {
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
                let relic = content.relics[**id as usize].id;
                !self.melted_relics.contains(index)
                    && relic_group(**id).is_some()
                    && !matches!(
                        relic,
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
                    && (relic != "RELIC.MAW_BANK" || !self.maw_bank)
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

    pub(super) fn obtain_relic(&mut self, content: &Content, id: Id) {
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
                        && let Some(id) = content.card_id("CARD.CLAW")
                    {
                        bundles.push(vec![
                            Card {
                                id,
                                ..Card::default()
                            };
                            3
                        ]);
                        continue;
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

    pub(super) fn gain_gold(&mut self, content: &Content, amount: i32) {
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

    pub(super) fn add_card(&mut self, content: &Content, mut card: Card) {
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

    pub(super) fn potion_pool(&self, content: &Content) -> Vec<Id> {
        let mut potions = content.characters[self.run.character as usize]
            .potion_pool
            .to_vec();
        potions.sort_unstable_by_key(|&id| content.potions[id as usize].id);
        let mut shared = content.acts[self.act as usize].potions.to_vec();
        shared.sort_unstable_by_key(|&id| content.potions[id as usize].id);
        potions.extend(shared);
        potions
    }

    pub(super) fn random_potions(
        &mut self,
        pool: &[Id],
        count: usize,
        combat: bool,
        unique: bool,
    ) -> Vec<Id> {
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

    pub(super) fn face_target(&mut self, target: Option<usize>) {
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
}
