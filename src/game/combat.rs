use super::*;

impl Game {
    pub(super) fn play(&mut self, content: &Content, hand: usize, target: Option<usize>) {
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

    pub(super) fn finish_play(&mut self, content: &Content) {
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

    pub(super) fn use_potion(&mut self, content: &Content, slot: usize, target: Option<usize>) {
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

    pub(super) fn choose(&mut self, content: &Content, index: usize) {
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

    pub(super) fn start_turn(&mut self, content: &Content) {
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
        if turn == 1
            && self.has_relic(content, "RELIC.BLESSED_ANTLER")
            && let Some(id) = content.card_id("CARD.DAZED")
        {
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
        if turn == 1
            && ninja_scroll
            && let Some(id) = content.card_id("CARD.SHIV")
        {
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

    pub(super) fn end_turn(&mut self, content: &Content) {
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

    pub(super) fn finish_end_turn(&mut self, content: &Content) {
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

    pub(super) fn roll_moves(&mut self, content: &Content) {
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

    pub(super) fn resolve(&mut self, content: &Content) {
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
                if let Some((instance, value)) = persisted
                    && let Some(master) = self
                        .run
                        .deck
                        .iter_mut()
                        .find(|master| master.instance == instance)
                {
                    master.value = value;
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

    pub(super) fn damage(
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

    pub(super) fn kill_actor(&mut self, content: &Content, actor: Actor) {
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
                } else if enemy_id == "MONSTER.TORCH_HEAD_AMALGAM"
                    && let Some(queen) =
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

    pub(super) fn spawn_enemy(&mut self, content: &Content, name: &str) -> usize {
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

    pub(super) fn fur_coat_active(&self) -> bool {
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

    pub(super) fn sync_belt_buckle(&mut self, content: &Content) {
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

    pub(super) fn apply_power(&mut self, content: &Content, actor: Actor, id: Id, mut amount: i16) {
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

    pub(super) fn passive_orb(&mut self, content: &Content, index: Option<usize>) {
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

    pub(super) fn trigger(
        &mut self,
        content: &Content,
        trigger: Trigger,
        owner: Actor,
        event: i16,
    ) {
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

    pub(super) fn finish_card_destination(
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

    pub(super) fn add_random(&mut self, content: &Content, pile: Pile, mut card: Card) {
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

    pub(super) fn creature(&self, actor: Actor) -> &Creature {
        let combat = self.combat().unwrap();
        match actor {
            Actor::Player => &combat.player,
            Actor::Osty => &combat.osty,
            Actor::Enemy(i) => &combat.enemies[i].creature,
        }
    }

    pub(super) fn creature_mut(&mut self, actor: Actor) -> &mut Creature {
        let combat = self.combat_mut().unwrap();
        match actor {
            Actor::Player => &mut combat.player,
            Actor::Osty => &mut combat.osty,
            Actor::Enemy(i) => &mut combat.enemies[i].creature,
        }
    }

    pub(super) fn finish_combat(&mut self, content: &Content) {
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
                if self.toy_box_combats.is_multiple_of(3)
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
