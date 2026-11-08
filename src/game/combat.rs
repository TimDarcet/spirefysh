use super::*;

pub(super) mod hooks;

pub(super) fn sort_for_shuffle(cards: &mut [Card], content: &Content) {
    fn compare(a: Card, b: Card, content: &Content) -> std::cmp::Ordering {
        (content.cards[a.id as usize].id, a.upgrades)
            .cmp(&(content.cards[b.id as usize].id, b.upgrades))
    }
    fn swap_if(cards: &mut [Card], content: &Content, a: usize, b: usize) {
        if compare(cards[a], cards[b], content).is_gt() {
            cards.swap(a, b);
        }
    }
    fn insertion(cards: &mut [Card], content: &Content) {
        for i in 0..cards.len() - 1 {
            let card = cards[i + 1];
            let mut j = i + 1;
            while j > 0 && compare(card, cards[j - 1], content).is_lt() {
                cards[j] = cards[j - 1];
                j -= 1;
            }
            cards[j] = card;
        }
    }
    fn partition(cards: &mut [Card], content: &Content) -> usize {
        let hi = cards.len() - 1;
        let middle = hi / 2;
        swap_if(cards, content, 0, middle);
        swap_if(cards, content, 0, hi);
        swap_if(cards, content, middle, hi);
        let pivot = cards[middle];
        cards.swap(middle, hi - 1);
        let (mut left, mut right) = (0, hi - 1);
        loop {
            left += 1;
            while compare(cards[left], pivot, content).is_lt() {
                left += 1;
            }
            right -= 1;
            while compare(pivot, cards[right], content).is_lt() {
                right -= 1;
            }
            if left >= right {
                break;
            }
            cards.swap(left, right);
        }
        cards.swap(left, hi - 1);
        left
    }
    fn heap(cards: &mut [Card], content: &Content) {
        fn down(cards: &mut [Card], content: &Content, mut i: usize, n: usize) {
            let card = cards[i - 1];
            while i <= n / 2 {
                let mut child = 2 * i;
                if child < n && compare(cards[child - 1], cards[child], content).is_lt() {
                    child += 1;
                }
                if !compare(card, cards[child - 1], content).is_lt() {
                    break;
                }
                cards[i - 1] = cards[child - 1];
                i = child;
            }
            cards[i - 1] = card;
        }
        for i in (1..=cards.len() / 2).rev() {
            down(cards, content, i, cards.len());
        }
        for i in (2..=cards.len()).rev() {
            cards.swap(0, i - 1);
            down(cards, content, 1, i - 1);
        }
    }
    fn intro(mut cards: &mut [Card], content: &Content, mut depth: usize) {
        while cards.len() > 1 {
            if cards.len() <= 16 {
                match cards.len() {
                    2 => swap_if(cards, content, 0, 1),
                    3 => {
                        swap_if(cards, content, 0, 1);
                        swap_if(cards, content, 0, 2);
                        swap_if(cards, content, 1, 2);
                    }
                    _ => insertion(cards, content),
                }
                return;
            }
            if depth == 0 {
                heap(cards, content);
                return;
            }
            depth -= 1;
            let pivot = partition(cards, content);
            let (left, right) = cards.split_at_mut(pivot + 1);
            intro(right, content, depth);
            cards = &mut left[..pivot];
        }
    }
    let depth = 2 * (usize::BITS - cards.len().leading_zeros()) as usize;
    intro(cards, content, depth);
}

/// Played cards waiting for their result pile (the game's play pile).
fn played(queue: &mut [Pending]) -> impl Iterator<Item = &mut Card> {
    queue
        .iter_mut()
        .filter_map(|pending| match &mut pending.effect {
            Effect::PlacePlayed(card, ..) => Some(card),
            _ => None,
        })
}

/// Cards with the game's Token rarity, which the sim's `CardRarity` doesn't model.
const TOKEN_CARDS: [&str; 10] = [
    "CARD.FUEL",
    "CARD.GIANT_ROCK",
    "CARD.LUMINESCE",
    "CARD.MINION_SACRIFICE",
    "CARD.MINION_DIVE_BOMB",
    "CARD.MINION_STRIKE",
    "CARD.SHIV",
    "CARD.SOUL",
    "CARD.SOVEREIGN_BLADE",
    "CARD.SWEEPING_GAZE",
];

impl Game {
    pub(super) fn play(
        &mut self,
        content: &Content,
        hand: usize,
        target: Option<usize>,
        manual: bool,
    ) {
        let spiked_gauntlets = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
        let chemical_x = self.has_relic(content, "RELIC.CHEMICAL_X");
        let brilliant_scarf = self.has_relic(content, "RELIC.BRILLIANT_SCARF")
            && self.combat().unwrap().history.manual_plays == 4;
        let combat = self.combat_mut().unwrap();
        let card = combat.hand.remove(hand);
        combat.bound_played |= card.affliction == Affliction::Bound;
        let def = content.cards[card.id as usize];
        // Brilliant Scarf modifies costs, which an X cost ignores (GetAmountToSpend).
        let cost = if brilliant_scarf && def.cost[card.upgrades.min(1) as usize] >= 0 {
            0
        } else {
            energy_cost(combat, card, def, spiked_gauntlets, true)
        };
        let x_cost = def.cost[card.upgrades.min(1) as usize] < 0;
        // Whispering Earring spends energy before its auto-play, which then sees an X cost as 0.
        let energy = if manual || !x_cost { cost } else { 0 };
        let x = if x_cost { combat.energy } else { 0 } + 2 * (x_cost && chemical_x) as i16;
        combat.energy -= cost;
        trigger_orbit(combat, cost);
        let stars = if brilliant_scarf && def.star_cost[card.upgrades.min(1) as usize] != -2 {
            0
        } else {
            star_cost(combat, card, def)
        };
        combat.stars -= stars;
        combat.history.energy += cost;
        let context = Context {
            target,
            x,
            event: stars
                + 2 * (chemical_x && def.star_cost[card.upgrades.min(1) as usize] == -2) as i16,
            target_instance: target.map(|slot| combat.enemies[slot].instance),
            ..Context::card(card)
        };
        let play = Play {
            card,
            plays: 0,
            done: 0,
            energy,
            stars,
            manual,
            context,
            recorded: Vec::new(),
        };
        self.start_play(content, play);
    }

    /// CardModel.OnPlayWrapper's start: the result pile, the play count, then the first play.
    fn start_play(&mut self, content: &Content, mut play: Play) {
        let axe = self
            .relic_mut(content, "RELIC.THROWING_AXE")
            .filter(|axe| axe.combat == 0);
        let throwing_axe = axe.map(|axe| axe.combat = 1).is_some();
        let combat = self.combat_mut().unwrap();
        let card = &mut play.card;
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        if matches!(card_type, CardType::Attack | CardType::Skill)
            && combat.history.attacks + combat.history.skills
                < combat.player.power(power_id::NOSTALGIA)
        {
            card.flags |= RETURN_TO_DRAW;
        }
        // The result pile is chosen when the play starts, so a mid-play upgrade keeps Exhaust and
        // Corruption only exhausts skills that start after it (not the Havoc that played it).
        card.turn_flags |= card.flags(def) & EXHAUST;
        if card_type == CardType::Skill && combat.player.power(power_id::CORRUPTION) > 0 {
            card.turn_flags |= EXHAUST;
        }
        // Rebound's redirect is chosen when a play starts
        // (ModifyCardPlayResultPileTypeAndPosition): a discard-bound card starting under it goes to
        // the top of the draw pile and uses one stack.
        if combat.player.power(power_id::REBOUND) > 0
            && card_type != CardType::Power
            && card.flags(def) & (EXHAUST | RETURN_TO_HAND | RETURN_TO_DRAW) == 0
        {
            combat.player.consume_power(power_id::REBOUND);
            combat.rebounds.push(card.instance);
        }
        // Feral's return is also chosen when a play starts: the first zero-energy attacks to start
        // this turn go back to the hand, even if one auto-played inside them finishes first.
        if card_type == CardType::Attack
            && play.energy == 0
            && combat.history.feral_returns < combat.player.power(power_id::FERAL)
        {
            combat.history.feral_returns += 1;
            combat.ferals.push(card.instance);
        }
        if card_type == CardType::Attack {
            let stomp = content.card_id("CARD.STOMP");
            combat.add_costs(|card| Some(card.id) == stomp, -1, THIS_TURN);
        }
        play.plays = combat.play_count(content, play.card, throwing_axe);
        let plays = play.plays as i16;
        let history = &mut combat.history;
        history.cards += plays;
        history.first_plays += 1;
        combat.total_cards += plays;
        if play.manual {
            history.manual_cards += 1;
            history.manual_plays += plays;
        }
        history.zero_attacks += (card_type == CardType::Attack && play.energy == 0) as i16 * plays;
        play.context.lethal = card_type == CardType::Attack && history.attacks == 0;
        match card_type {
            CardType::Attack => history.attacks += plays,
            CardType::Skill => history.skills += plays,
            CardType::Power => history.powers += plays,
            _ => {}
        }
        let (stars, context) = (play.stars, play.context);
        let context = Context {
            lethal: false,
            ..context
        };
        if !play.manual {
            // An auto-play's CardPlay records no spent stars (Black Hole).
            play.stars = 0;
        }
        combat.plays.push(play);
        combat.queue.push(Pending {
            effect: Effect::OnPlay,
            context,
        });
        let block = (combat.player.power(power_id::CHILD_OF_THE_STARS)).saturating_mul(stars);
        if block > 0 {
            combat.queue.push(Pending {
                effect: Effect::RawBlock(Target::Player, Amount::fixed(block, block)),
                context,
            });
        }
        self.before_card_played(content);
        // Star-spending relics fire when resources are spent, before the card resolves.
        let regent = self
            .relic_mut(content, "RELIC.MINI_REGENT")
            .filter(|regent| regent.combat == 0);
        if stars > 0
            && let Some(regent) = regent
        {
            regent.combat = 1;
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
        }
        if stars > 0
            && let Some(dust) = self.relic_mut(content, "RELIC.GALACTIC_DUST")
        {
            let stars = dust.counters[0] + stars as i32;
            dust.counters[0] = stars % 10;
            for _ in 0..stars / 10 {
                self.gain_block(content, Actor::Player, 10, None, false);
            }
        }
    }

    /// One play's OnPlay, enchantment and affliction, then its AfterPlay step.
    fn queue_play(&mut self, content: &Content) {
        let pen_nib = self
            .relic(content, "RELIC.PEN_NIB")
            .is_some_and(|nib| nib.counters[0] == 0);
        let play = self.combat().unwrap().plays.last().unwrap();
        let (card, base, index) = (play.card, play.context, play.done);
        let def = content.cards[card.id as usize];
        let attack = card.card_type(def) == CardType::Attack;
        let context = Context {
            pen_nib: pen_nib && attack,
            lethal: base.lethal && index == 0,
            upgraded: card.upgrades > 0,
            ..base
        };
        let combat = self.combat_mut().unwrap();
        let fan_shiv = def.id == "CARD.SHIV" && combat.player.power(power_id::FAN_OF_KNIVES) > 0;
        combat.queue.push(Pending {
            effect: Effect::AfterPlay,
            context,
        });
        // Enchantments act after the card's own effects (queued first, so they resolve last).
        let amount = card.enchantment_amount;
        let effect = match card.enchantment {
            Some(Enchantment::Adroit) => {
                Some(Effect::Block(Target::Player, Amount::fixed(amount, amount)))
            }
            Some(Enchantment::Corrupted) => {
                Some(Effect::LoseHp(Target::Player, Amount::fixed(2, 2)))
            }
            Some(Enchantment::Sown) if card.enchantment_value > 0 => {
                Some(Effect::Energy(amount as i8))
            }
            Some(Enchantment::Swift) if card.enchantment_value > 0 => {
                let draw = amount.max(0) as u8;
                if def.flags[card.upgrades.min(1) as usize] & RETURN_TO_DRAW != 0 {
                    combat.swift_after_return = draw;
                    None
                } else {
                    Some(Effect::Draw(draw))
                }
            }
            _ => None,
        };
        if matches!(
            card.enchantment,
            Some(Enchantment::Sown | Enchantment::Swift)
        ) {
            combat.plays.last_mut().unwrap().card.enchantment_value = 0;
        }
        if let Some(effect) = effect {
            combat.queue.push(Pending { effect, context });
        }
        // Fetch draws after Osty's attack on its first play this turn.
        if index == 0 && card.id == card_id::FETCH {
            let play = combat.plays.last_mut().unwrap();
            if card.turn_flags & FETCHED == 0 && combat.osty.hp > 0 {
                combat.queue.push(Pending {
                    effect: Effect::Draw(1),
                    context,
                });
            }
            play.card.turn_flags |= FETCHED;
        }
        if card.flags(def) & INKY != 0 {
            let target = if fan_shiv || def.target == Target::AllEnemies {
                Target::AllEnemies
            } else {
                Target::ChosenEnemy
            };
            combat.queue.push(Pending {
                effect: Effect::ApplyPower(target, power_id::WEAK, Amount::fixed(1, 1)),
                context,
            });
        }
        if fan_shiv {
            combat.queue.push(Pending {
                effect: Effect::Attack(Target::AllEnemies, Amount::fixed(4, 6), 1),
                context,
            });
            return;
        }
        if def.id == "CARD.COSMIC_INDIFFERENCE" {
            combat.queue.push(Pending {
                effect: Effect::Select(
                    Pile::Discard,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Draw),
                ),
                context,
            });
        }
        push_card_effects(&mut combat.queue, card, def, context);
    }

    /// A later play of a series starts once the previous one and its AfterCardPlayed hooks
    /// resolved: its BeforeCardPlayed pass, then its OnPlay.
    fn before_play(&mut self, content: &Content) {
        let combat = self.combat_mut().unwrap();
        let play = combat.plays.last().unwrap();
        let (card, context) = (play.card, play.context);
        combat.queue.push(Pending {
            effect: Effect::OnPlay,
            context,
        });
        if card.card_type(content.cards[card.id as usize]) == CardType::Attack {
            let stomp = content.card_id("CARD.STOMP");
            combat.add_costs(|card| Some(card.id) == stomp, -1, THIS_TURN);
        }
        self.before_card_played(content);
    }

    /// A play finished (CardPlayFinished, then AfterCardPlayed): the next play starts, or the
    /// card goes to its result pile.
    fn after_play(&mut self, content: &Content) {
        let combat = self.combat_mut().unwrap();
        let play = combat.plays.last_mut().unwrap();
        if play.done + 1 == play.plays {
            return self.finish_play(content);
        }
        let (mut card, energy) = (play.card, play.energy);
        let finished = play.done as i16 + 1;
        self.spend_block_doublers(content, card.instance);
        // Maul grows as each play resolves (OnPlay), so its next play hits harder.
        self.grow_maul(&mut card);
        // Vigorous wears off after each play (AfterCardPlayed), so replays don't get it;
        // Goopy grows with each play.
        match card.enchantment {
            Some(Enchantment::Vigorous) => card.enchantment_value = 0,
            Some(Enchantment::Goopy | Enchantment::Momentum) => self.spend_enchantment(&mut card),
            _ => {}
        }
        self.combat_mut().unwrap().plays.last_mut().unwrap().card = card;
        // Rupture's Strength from this play's own HP loss lands as it finishes (Glam replays).
        self.release_rupture(content);
        let card_type = card.card_type(content.cards[card.id as usize]);
        self.combat_mut().unwrap().queue.push(Pending {
            effect: Effect::BeforePlay,
            context: Context::card(card),
        });
        let skills = self.finished_plays(content, CardType::Skill) + finished;
        self.queue_return_late(card_type, (skills - 1, skills), energy);
        // Each finished play of a Shiv counts (Phantom Blades), not just the last.
        let combat = self.combat_mut().unwrap();
        if content.cards[card.id as usize].tags & SHIV_TAG != 0 {
            combat.history.shivs += 1;
        }
        let play = combat.plays.last_mut().unwrap();
        play.done += 1;
        let recorded = play.recorded.clone();
        // Once the fight is won the combat ends during a card's remaining plays, before
        // their AfterCardPlayed hooks (Calamity).
        if !self.enemies_defeated() {
            self.card_played(content, card, recorded);
        }
    }

    /// The last play finished: CardPlayFinished's bookkeeping, AfterCardPlayed, then the result
    /// pile and the empty-hand check.
    fn finish_play(&mut self, content: &Content) {
        let combat = self.combat_mut().unwrap();
        let play = combat.plays.pop().unwrap();
        let mut card = play.card;
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        let replay = card.flags & REPLAY != 0;
        card.flags &= !REPLAY;
        self.spend_enchantment(&mut card);
        let ferals = &mut self.combat_mut().unwrap().ferals;
        let feral = (ferals.iter().position(|&id| id == card.instance))
            .map(|index| ferals.swap_remove(index))
            .is_some();
        self.grow_maul(&mut card);
        if card_type == CardType::Skill
            && self.creature(Actor::Player).power(power_id::MASTER_PLANNER) > 0
        {
            card.flags |= SLY;
        }
        let combat = self.combat_mut().unwrap();
        if def.tags & SHIV_TAG != 0 {
            combat.history.shivs += 1;
        }
        // The hand is checked once the card is placed and its exhaust hooks resolved, unless the
        // play ran inside another card or potion or during the turn's setup (CheckForEmptyHand,
        // UnceasingTop.IsValidPhase).
        if combat.plays.is_empty() && combat.potions == 0 && !combat.setup {
            combat.queue.push(Pending {
                effect: Effect::CheckEmptyHand,
                context: Context::player(),
            });
        }
        self.place_played(content, card, replay, feral);
        let skills = self.finished_plays(content, CardType::Skill);
        self.queue_return_late(card_type, (skills - 1, skills), play.energy);
        self.spend_block_doublers(content, card.instance);
        if play.energy >= 3 && self.has_relic(content, "RELIC.IVORY_TILE") {
            self.gain_energy(play.plays as i16);
        }
        self.release_rupture(content);
        let context = self.card_played(content, card, play.recorded);
        let damage = self.combat().unwrap().player.power(power_id::BLACK_HOLE);
        if play.stars > 0 && damage > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(damage, damage)),
                context,
            });
        }
    }

    /// Vambrace and Pael's Legion double one card play's block; the game spends them in
    /// AfterCardPlayed, so a replayed card (Spiral) is doubled only once.
    fn spend_block_doublers(&mut self, content: &Content, instance: u32) {
        if self.combat().unwrap().block_doubler != Some(instance) {
            return;
        }
        self.combat_mut().unwrap().block_doubler = None;
        if let Some(vambrace) = self.relic_mut(content, "RELIC.VAMBRACE") {
            vambrace.combat = 1;
        }
        if let Some(legion) = self
            .relic_mut(content, "RELIC.PAELS_LEGION")
            .filter(|legion| legion.combat == 0)
        {
            legion.combat = 2;
        }
    }

    /// Right Hand Hand returns from the discard pile after any play worth 2+ energy, auto-plays too.
    /// With a full hand it goes back on top of the discard pile.
    fn return_right_hands(&mut self, energy: i16) {
        if energy < 2 {
            return;
        }
        let discard = &self.combat().unwrap().discard;
        let returning: Vec<_> = discard
            .iter()
            .filter(|card| card.id == card_id::RIGHT_HAND_HAND)
            .map(|card| card.instance)
            .collect();
        for instance in returning {
            let discard = &mut self.combat_mut().unwrap().discard;
            let index = discard
                .iter()
                .position(|card| card.instance == instance)
                .unwrap();
            let card = discard.remove(index);
            self.add_to_hand(card);
        }
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
            target,
            ..Context::player()
        };
        // The game checks for an empty hand (Unceasing Top) only once the potion is done.
        let combat = self.combat_mut().unwrap();
        combat.potions += 1;
        combat.queue.push(Pending {
            effect: Effect::CheckEmptyHand,
            context,
        });
        // Its AfterPotionUsed pass starts once the potion is done.
        combat.queue.push(Pending {
            effect: Effect::Fire(Trigger::AfterPotionUsed),
            context,
        });
        push_effects(
            &mut self.combat_mut().unwrap().queue,
            content.potions[id as usize].effects,
            context,
        );
    }

    pub(super) fn choose(&mut self, content: &Content, index: usize) {
        let combat = self.combat().unwrap();
        let choice = combat.choice.unwrap();
        let knowledge = (choice.pile == Pile::Offer
            && combat.enemy_turn
            && matches!(
                combat.offer[index].id,
                card_id::DISINTEGRATION | card_id::MIND_ROT | card_id::SLOTH | card_id::WASTE_AWAY
            )
            && combat.enemies.iter().any(|enemy| {
                enemy.move_index == 0
                    && content.enemies[enemy.creature.id as usize].id == "MONSTER.KNOWLEDGE_DEMON"
            }))
        .then(|| combat.offer[index]);
        if let Some(card) = knowledge {
            let (power, amount) = match card.id {
                card_id::DISINTEGRATION => (power_id::DISINTEGRATION, card.value),
                card_id::MIND_ROT => (power_id::MIND_ROT, 1),
                card_id::SLOTH => (power_id::SLOTH, 3),
                card_id::WASTE_AWAY => (power_id::WASTE_AWAY, 1),
                _ => unreachable!(),
            };
            self.apply_power(content, Actor::Player, power, amount);
            let combat = self.combat_mut().unwrap();
            combat.offer.clear();
            combat.choice = None;
            self.resolve(content);
            return;
        }
        // The picked cards go through one CardCmd.Discard (DiscardAndDraw without the draw).
        let discard = choice.pile == Pile::Hand && choice.op == CardOp::Move(Pile::Discard);
        let op = [choice.op, CardOp::DiscardDraw][discard as usize];
        self.apply_card_op(content, choice.pile, index, op);
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
        if op == CardOp::DiscardDraw && self.combat().unwrap().choice.is_none() {
            self.finish_discard_draw(content, !discard);
        }
    }

    /// CardCmd.DiscardAndDraw: each discard's hooks run first, then as many cards are drawn,
    /// then the Sly ones auto-play from the discard pile; all before anything queued earlier.
    pub(super) fn finish_discard_draw(&mut self, content: &Content, draw: bool) {
        let (start, cards) = std::mem::take(&mut self.combat_mut().unwrap().discard_draw);
        if cards.is_empty() {
            return;
        }
        let sly: Vec<_> = cards
            .iter()
            .copied()
            .filter(|&card| self.is_sly(content, card))
            .collect();
        let queue = &mut self.combat_mut().unwrap().queue;
        for &card in &sly {
            queue.insert(
                start,
                Pending {
                    effect: Effect::AutoPlay(card),
                    context: Context {
                        off_pile: true,
                        ..Context::card(card)
                    },
                },
            );
        }
        if draw {
            queue.insert(
                start + sly.len(),
                Pending {
                    effect: Effect::Draw(cards.len().min(u8::MAX as usize) as u8),
                    context: Context::player(),
                },
            );
        }
    }

    /// Queues the player's turn (CombatManager.StartTurn).
    pub(super) fn start_turn(&mut self) {
        push_effects(
            &mut self.combat_mut().unwrap().queue,
            hooks::PLAYER_TURN,
            Context::player(),
        );
    }

    /// Toasty Mittens exhausts the draw pile's top card (turn 1: the top non-Innate one), then
    /// gives 1 Strength.
    fn toasty_mittens(&mut self, content: &Content) {
        self.combat_mut().unwrap().queue.push(Pending {
            effect: Effect::ApplyPower(Target::Player, power_id::STRENGTH, Amount::fixed(1, 1)),
            context: Context::player(),
        });
        if self.combat().unwrap().draw.is_empty() && !self.combat().unwrap().discard.is_empty() {
            self.shuffle_discard_into_draw(content);
            self.after_shuffle(content);
        }
        let combat = self.combat().unwrap();
        let index = if combat.turn == 1 {
            (combat.draw.iter())
                .rposition(|card| card.flags(content.cards[card.id as usize]) & INNATE == 0)
        } else {
            combat.draw.len().checked_sub(1)
        };
        if let Some(index) = index {
            let card = self.combat_mut().unwrap().remove_draw(index);
            self.combat_mut().unwrap().exhaust.push(card);
            self.exhausted(content, card, false);
        }
    }

    /// Its hits land on the enemies one at a time, left to right, so a death in between
    /// resolves before the next hit (Crab Rage's block); then it grows by 5.
    pub(super) fn tick_boulder(&mut self, boulder: Power) {
        let Some(live) = self.creature_mut(Actor::Player).instance_mut(boulder) else {
            return;
        };
        let amount = live.amount;
        live.amount = amount.saturating_add(5);
        let enemies = self.targets(Target::AllEnemies, Context::player());
        for enemy in enemies.into_iter().rev() {
            let Actor::Enemy(index) = enemy else { continue };
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::ChosenEnemy, Amount::fixed(amount, amount)),
                context: Context {
                    target: Some(index),
                    ..Context::player()
                },
            });
        }
    }

    pub(super) fn end_turn(&mut self, content: &Content) {
        push_effects(
            &mut self.combat_mut().unwrap().queue,
            hooks::END_PLAYER_TURN,
            Context::player(),
        );
        self.resolve(content);
    }

    /// It counts down, then explodes as one damage command: every enemy is hit before deaths
    /// resolve.
    pub(super) fn tick_bomb(&mut self, content: &Content, bomb: Power) {
        let Some(live) = self.creature_mut(Actor::Player).instance_mut(bomb) else {
            return;
        };
        if live.amount > 1 {
            live.amount -= 1;
            return;
        }
        let amount = live.value;
        let previous = self.combat_mut().unwrap().pending_hits.replace(vec![]);
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
        self.resolve_deaths(content, previous);
        let powers = &mut self.creature_mut(Actor::Player).powers;
        powers.retain(|x| x.id != bomb.id || x.instance != bomb.instance);
    }

    /// ExecuteEnemyTurn and EndEnemyTurn: each enemy's move step in slot order, then the turn end.
    pub(super) fn enemy_moves(&mut self) {
        let participants = self.participants();
        let combat = self.combat_mut().unwrap();
        push_effects(&mut combat.queue, hooks::END_ENEMY_TURN, Context::player());
        // A creature spawned this turn into a dead participant's slot waits for its next turn.
        let moves: Vec<_> = (participants.into_iter())
            .map(|index| Effect::Step(Step::EnemyMove(index as u8, combat.enemies[index].instance)))
            .collect();
        push_effects(&mut combat.queue, &moves, Context::player());
    }

    pub(super) fn enemy_move(&mut self, content: &Content, index: usize, instance: u32) {
        let enemy = &self.combat().unwrap().enemies[index];
        let (move_index, id, stunned) = (enemy.move_index, enemy.creature.id, enemy.stunned > 0);
        if enemy.instance != instance || !self.participants().contains(&index) {
            return;
        }
        self.combat_mut().unwrap().queue.push(Pending {
            effect: Effect::Step(Step::EnemyMoveDone(index as u8, move_index as u8, stunned)),
            context: Context::player(),
        });
        let effects = content.enemies[id as usize].moves[move_index].effects;
        let context = Context {
            source: Actor::Enemy(index),
            ..Context::player()
        };
        if stunned {
            return;
        }
        let name = content.enemies[id as usize].id;
        if name == "MONSTER.THIEVING_HOPPER" && move_index == 0 {
            self.thieving_hopper(content, index);
        }
        match (name, move_index) {
            ("MONSTER.WATERFALL_GIANT", 4) => {
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
            }
            ("MONSTER.WATERFALL_GIANT", 7) => {
                let enemy = &mut self.combat_mut().unwrap().enemies[index];
                enemy.value = enemy.creature.power(power_id::STEAM_ERUPTION);
                (enemy.creature.powers).retain(|power| power.id != power_id::STEAM_ERUPTION);
            }
            ("MONSTER.WATERFALL_GIANT", 6) => {
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
            }
            ("MONSTER.KNOWLEDGE_DEMON", 0) => {
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
            }
            ("MONSTER.TEST_SUBJECT", 0) => {
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
            }
            ("MONSTER.TEST_SUBJECT", 3) => {
                // Three hits plus one per earlier claw (ExtraMultiClawCount), counted in value.
                let hits = 3 + self.combat().unwrap().enemies[index].value.max(0);
                self.combat_mut().unwrap().enemies[index].value += 1;
                let amount = if self.run.ascension >= 9 { 11 } else { 10 };
                // The attack stops once the Test Subject dies (Thorns).
                for _ in 0..hits {
                    if self.creature(Actor::Enemy(index)).hp <= 0 {
                        break;
                    }
                    self.damage(
                        content,
                        Actor::Enemy(index),
                        Actor::Player,
                        amount,
                        DamageKind::Attack,
                        None,
                    );
                }
            }
            ("MONSTER.TEST_SUBJECT", 6) => {
                for _ in 0..if self.run.ascension >= 9 { 5 } else { 3 } {
                    self.add_uncredited(
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
            }
            ("MONSTER.OVICOPTER", 0) => {
                // Three eggs, each into the last free of the five egg slots.
                for _ in 0..3 {
                    let Some(slot) = self.free_slot((1..=5).rev()) else {
                        break;
                    };
                    let egg = self.spawn_enemy(content, "MONSTER.TOUGH_EGG", slot);
                    self.apply_power(content, Actor::Enemy(egg), power_id::MINION, 1);
                    self.apply_power(content, Actor::Enemy(egg), power_id::HATCH, 1);
                }
            }
            ("MONSTER.TOUGH_EGG", 0) => {
                let low = if self.run.ascension >= 8 { 20 } else { 19 };
                let hp = self.rngs.niche.range(low, low + 2);
                let egg = &mut self.combat_mut().unwrap().enemies[index].creature;
                egg.hp = hp;
                egg.max_hp = hp;
                egg.powers.retain(|power| power.id == power_id::MINION);
            }
            ("MONSTER.FOGMOG", 0) => {
                self.spawn_enemy(content, "MONSTER.EYE_WITH_TEETH", 1);
            }
            ("MONSTER.LIVING_FOG", 1) => {
                if let Some(slot) = self.free_slot(1..=5) {
                    self.spawn_enemy(content, "MONSTER.GAS_BOMB", slot);
                }
                push_effects(&mut self.combat_mut().unwrap().queue, effects, context);
            }
            ("MONSTER.FABRICATOR", 0) => {
                self.spawn_bot(content, index, 0);
                self.spawn_bot(content, index, 2);
            }
            ("MONSTER.FABRICATOR", 1) => {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Step(Step::FabricatorBot(index as u8)),
                    context: Context::player(),
                });
                push_effects(&mut self.combat_mut().unwrap().queue, effects, context);
            }
            ("MONSTER.GUARDBOT", _) => {
                if let Some(fabricator) = self.combat().unwrap().enemies.iter().position(|enemy| {
                    enemy.creature.hp > 0
                        && content.enemies[enemy.creature.id as usize].id == "MONSTER.FABRICATOR"
                }) {
                    self.gain_block(content, Actor::Enemy(fabricator), 15, None, false);
                }
            }
            ("MONSTER.NOISEBOT", _) => {
                let dazed = Card {
                    id: card_id::DAZED,
                    ..Card::default()
                };
                self.add_uncredited(content, Pile::Discard, dazed);
                self.insert_random(Pile::Draw, dazed);
            }
            ("MONSTER.TWO_TAILED_RAT", 3) => {
                self.combat_mut().unwrap().rat_calls += 1;
                if let Some(slot) = self.free_slot((1..=5).rev()) {
                    let rat = self.spawn_enemy(content, "MONSTER.TWO_TAILED_RAT", slot);
                    // A rat called for backup picks its first move with the next intents.
                    self.combat_mut().unwrap().enemies[rat].value = 1;
                }
            }
            ("MONSTER.THE_OBSCURA", 0) => {
                self.spawn_enemy(content, "MONSTER.PARAFRIGHT", 1);
            }
            ("MONSTER.ENTOMANCER", 2) => {
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
            }
            _ => push_effects(&mut self.combat_mut().unwrap().queue, effects, context),
        }
    }

    pub(super) fn enemy_move_done(
        &mut self,
        content: &Content,
        index: usize,
        move_index: usize,
        stunned: bool,
    ) {
        if stunned {
            return;
        }
        let id = self.combat().unwrap().enemies[index].creature.id;
        match (content.enemies[id as usize].id, move_index) {
            ("MONSTER.AXEBOT", 0) => {
                let stock = self.creature(Actor::Enemy(index)).power(power_id::STOCK);
                let strength = if self.run.ascension >= 9 { 4 } else { 3 } * (2 - stock);
                self.apply_power(content, Actor::Enemy(index), power_id::STRENGTH, strength);
            }
            ("MONSTER.GREMLIN_MERC", _) if self.creature(Actor::Player).hp > 0 => {
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
            ("MONSTER.GAS_BOMB", _) if self.creature(Actor::Enemy(index)).hp > 0 => {
                self.kill_actor(content, Actor::Enemy(index))
            }
            ("MONSTER.THIEVING_HOPPER", 4) | ("MONSTER.FAT_GREMLIN", 1) => {
                self.creature_mut(Actor::Enemy(index)).hp = 0;
                self.combat_mut().unwrap().escaped.push(id);
            }
            _ => {}
        }
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
        // Imbued cards are only taken last, alongside Ancients.
        let priorities: &[&[CardRarity]] = &[
            &[CardRarity::Uncommon],
            &[CardRarity::Common, CardRarity::Rare, CardRarity::Event],
            &[CardRarity::Basic, CardRarity::Quest],
            &[CardRarity::Ancient],
        ];
        for (priority, rarities) in priorities.iter().enumerate() {
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
                    let imbued = card.enchantment == Some(Enchantment::Imbued);
                    if deck.iter().any(|deck| deck.instance == card.instance)
                        && (rarities.contains(&content.cards[card.id as usize].rarity) && !imbued
                            || priority == 3 && imbued)
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
                    let card = self.run.deck.remove(index);
                    let combat = self.combat_mut().unwrap();
                    combat.stolen.push((combat.enemies[enemy].instance, card));
                }
                self.creature_mut(Actor::Enemy(enemy))
                    .add_power(power_id::SWIPE, 1);
                break;
            }
        }
    }

    pub(super) fn roll_moves(&mut self, content: &Content) {
        if self.combat().is_none() {
            return;
        }
        let rat_slot_free = self.free_slot(1..=5).is_some();
        let Phase::Combat(combat) = &mut self.phase else {
            unreachable!()
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
        // PrepareForNextTurn rolls the enemies in slot order.
        let mut order: Vec<_> = (0..combat.enemies.len()).collect();
        order.sort_by_key(|&index| combat.enemies[index].slot);
        for index in order {
            // CanSummon: no other rat shows Call for Backup, whether just rolled or just performed.
            let rat_calling = (combat.enemies.iter().enumerate()).any(|(other, enemy)| {
                other != index
                    && enemy.creature.hp > 0
                    && enemy.move_index == 3
                    && content.enemies[enemy.creature.id as usize].id == "MONSTER.TWO_TAILED_RAT"
            });
            let enemy = &mut combat.enemies[index];
            // The dead don't roll, but a dead segment's Dead move goes on to Reattach.
            let reattaching = enemy.creature.power(power_id::REATTACH) > 0 && enemy.last_move == 3;
            if enemy.creature.hp <= 0 && !reattaching {
                continue;
            }
            if enemy.stunned > 0 {
                enemy.stunned -= 1;
                continue;
            }
            let moves = content.enemies[enemy.creature.id as usize].moves;
            if content.enemies[enemy.creature.id as usize].id == "MONSTER.WRIGGLER"
                && enemy.last_move != usize::MAX
            {
                // After the spawn stun, wriggler1/3 bite and wriggler2/4 wriggle (by slot name,
                // kept in `value`); older states fall back to list parity.
                let biter = if enemy.value > 0 {
                    enemy.value % 2 == 1
                } else {
                    index % 2 == 0
                };
                let chosen = match enemy.last_move {
                    0 if biter => 1,
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
                let chosen = if living <= 3 { 0 } else { 1 };
                enemy.last_move = chosen;
                enemy.move_index = chosen;
                enemy.repeats = 1;
                enemy.move_history.push(chosen);
                continue;
            }
            if content.enemies[enemy.creature.id as usize].id == "MONSTER.TWO_TAILED_RAT"
                && enemy.value == 1
            {
                enemy.value = 0;
                enemy.move_index = self.rngs.monster_ai.below(3) as usize;
            }
            if enemy.last_move == usize::MAX {
                enemy.last_move = enemy.move_index;
                enemy.repeats = 1;
                enemy.move_history.push(enemy.move_index);
                continue;
            }
            let id = content.enemies[enemy.creature.id as usize].id;
            if id == "MONSTER.FABRICATOR" {
                // CanFabricate counts the Fabricator itself among its living teammates.
                let chosen = if living < 4 {
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
                let chosen = if charged || enemy.creature.hp >= enemy.creature.max_hp / 2 {
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
                    && combat.rat_calls < 3
                    && rat_slot_free
                    && !rat_calling
                    && !enemy.move_history.contains(&3);
                // Screech has a 3-move cooldown; the others can't repeat.
                let screeched = enemy
                    .move_history
                    .iter()
                    .rev()
                    .take(3)
                    .any(|&prior| prior == 2);
                let mut choices: Vec<_> = (0..if can_summon { 4 } else { 3 })
                    .filter(|&move_index| move_index != enemy.last_move)
                    .filter(|&move_index| move_index != 2 || !screeched)
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
                // With every weight zeroed, RandomBranchState still draws and takes the first one.
                choices.push(next.first().copied().unwrap_or(0));
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
        again: Option<Effect>,
    ) -> Option<Card> {
        let next = |count| {
            if hand_draw {
                Effect::HandDraw(count)
            } else {
                Effect::Draw(count)
            }
        };
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
        // CardPileCmd.Draw stops once the combat is ending (e.g. Speedster killed the last enemy).
        if count == 0 || self.combat().unwrap().hand.len() >= 10 || self.enemies_defeated() {
            return None;
        }
        if self.combat().unwrap().draw.is_empty() {
            if self.combat().unwrap().discard.is_empty() {
                return None;
            }
            // CardPileCmd.Draw awaits the shuffle's AfterShuffle hooks (Stratagem's pick) before
            // drawing on, so the draw resumes beneath them.
            self.combat_mut().unwrap().queue.push(Pending {
                effect: again.unwrap_or(next(count)),
                context,
            });
            self.shuffle_discard_into_draw(content);
            self.after_shuffle(content);
            return None;
        }
        let mut card = self.take_draw().unwrap();
        self.expectation_draw();
        // Hellraiser (AfterCardDrawnEarly) auto-plays a drawn Strike before AfterCardDrawn hooks.
        let combat = self.combat_mut().unwrap();
        let early_hellraiser = combat.player.power(power_id::HELLRAISER) > 0
            && content.cards[card.id as usize].tags & STRIKE_TAG != 0;
        let mut hellraiser = early_hellraiser;
        if hellraiser {
            let infinite = combat
                .enemies
                .iter()
                .filter(|enemy| enemy.creature.hp > 0)
                .all(|enemy| {
                    enemy.creature.max_hp == i16::MAX
                        || enemy.creature.powers.iter().any(|power| {
                            power.id == power_id::HARDENED_SHELL && power.value >= power.amount
                        })
                });
            let power = combat
                .player
                .powers
                .iter_mut()
                .find(|power| power.id == power_id::HELLRAISER)
                .unwrap();
            if infinite {
                hellraiser = power.value < 9;
                power.value = power.value.saturating_add(1);
            } else {
                power.value = 0;
            }
        }
        // Slither only rerolls a card still in hand (a Strike Hellraiser played keeps its cost).
        if card.enchantment == Some(Enchantment::Slither) && !hellraiser {
            card.add_cost(self.roll_cost(), ABSOLUTE);
        }
        // Confused rerolls every drawn card with a canonical cost; X-cost cards (canonical cost
        // 0) roll too, though their cost stays X.
        let def = content.cards[card.id as usize];
        let x_cost =
            def.cost[card.upgrades.min(1) as usize] < 0 && card.flags(def) & UNPLAYABLE == 0;
        let confused = (self.has_relic(content, "RELIC.SNECKO_EYE")
            || self.has_relic(content, "RELIC.FAKE_SNECKO_EYE"))
            && (def.cost[card.upgrades.min(1) as usize] >= 0 || x_cost);
        if confused && !early_hellraiser {
            let cost = self.roll_cost();
            if !x_cost {
                card.add_cost(cost, ABSOLUTE);
            }
        }
        card.turn_flags |= draw_flags;
        let combat = self.combat_mut().unwrap();
        // Chains of Binding afflicts cards drawn on the player's turn, up to its amount per turn,
        // after Hellraiser has auto-played its Strikes.
        if !hellraiser
            && !combat.enemy_turn
            && card.affliction == Affliction::None
            && (combat.bound_this_turn as i16) < combat.player.power(power_id::CHAINS_OF_BINDING)
        {
            card.affliction = Affliction::Bound;
            combat.bound_this_turn += 1;
        }
        // Kingly Kick costs 1 less for the rest of combat each time it is drawn: a relative
        // modifier, so it also applies on top of an absolute one (Snecko Eye's roll above).
        if content.cards[card.id as usize].id == "CARD.KINGLY_KICK" {
            card.add_cost(-1, 0);
        }
        if !hellraiser {
            combat.hand.push(card);
        }
        combat.drawn = combat.drawn.saturating_add(1);
        if content.cards[card.id as usize].card_type == CardType::Status {
            combat.history.statuses_drawn += 1;
        }
        if !hand_draw {
            combat.history.extra_drawn = combat.history.extra_drawn.saturating_add(1);
        }
        if count > 1 {
            combat.queue.push(Pending {
                effect: next(count - 1),
                context,
            });
        }
        let pagestorm = combat
            .player
            .power(power_id::PAGESTORM)
            .clamp(0, u8::MAX as i16) as u8;
        if pagestorm > 0 && is_ethereal(combat, card, content.cards[card.id as usize]) {
            combat.queue.push(Pending {
                effect: Effect::Draw(pagestorm),
                context,
            });
        }
        let speedster = !hand_draw && !self.combat().unwrap().enemy_turn;
        let mut powers = self.creature(Actor::Player).powers.clone();
        powers.retain(|power| speedster || power.id != power_id::SPEEDSTER);
        self.fire(
            content,
            Trigger::AfterCardDrawn,
            Context {
                event: 1,
                subject: Some(card.instance),
                ..Context::card(card)
            },
            powers,
        );
        if confused && early_hellraiser {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::ConfusedRoll(card),
                context: Context::card(card),
            });
        }
        if hellraiser {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::AutoPlay(card),
                context: Context::card(card),
            });
        }
        Some(card)
    }

    /// A random 0-3 cost (Confused, Slither); 0 while the expectation search hides it.
    fn roll_cost(&mut self) -> i8 {
        self.expectation_unknown();
        if self.expectation.is_some() {
            0
        } else {
            self.rngs.combat_energy_costs.below(4) as i8
        }
    }

    /// PlayerCmd.GainEnergy: No Energy Gain blocks every energy gain.
    fn gain_energy(&mut self, amount: i16) {
        if amount > 0 && self.creature(Actor::Player).power(power_id::NO_ENERGY_GAIN) == 0 {
            let combat = self.combat_mut().unwrap();
            combat.energy = combat.energy.saturating_add(amount);
        }
    }

    pub(crate) fn resolve(&mut self, content: &Content) {
        // The player's death ends the combat on the spot: nothing queued after it resolves.
        while self.combat().is_some_and(|x| x.choice.is_none() && x.player.hp > 0) {
            let Some(pending) = self.combat_mut().and_then(|x| x.queue.pop()) else {
                break;
            };
            self.apply(content, pending);
        }
    }

    /// Resolves what was queued above `base`, as the game awaits a command's nested commands.
    pub(super) fn settle(&mut self, content: &Content, base: usize) {
        let hook = self.combat().unwrap().applying_hook;
        while self
            .combat()
            .is_some_and(|x| x.choice.is_none() && x.queue.len() > base)
        {
            let pending = self.combat_mut().unwrap().queue.pop().unwrap();
            self.apply(content, pending);
        }
        if let Some(combat) = self.combat_mut() {
            combat.applying_hook = hook;
        }
    }

    fn apply(&mut self, content: &Content, pending: Pending) {
        self.combat_mut().unwrap().applying_hook = pending.context.hook;
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
        // ModifyOrbValue runs powers first (Focus clamps at 0), then Infused Core's +1 per copy.
        if pending.context.orb_id == Some(orb_id::LIGHTNING)
            && self.has_relic(content, "RELIC.INFUSED_CORE")
            && matches!(pending.effect, Effect::Damage(..))
        {
            amount = amount
                .max(0)
                .saturating_add(self.relic_count(content, "RELIC.INFUSED_CORE") as i16);
        }
        match pending.effect {
            Effect::Attack(target, _, _) | Effect::AttackMany(target, _, _) => {
                let hits = match pending.effect {
                    Effect::AttackMany(_, _, hits) => self.amount(content, pending.context, hits),
                    Effect::Attack(_, _, hits) => hits as i16,
                    _ => unreachable!(),
                }
                .max(0);
                // Each hit strikes every target before deaths resolve; random attacks retarget per hit.
                let (rounds, hits) = if matches!(target, Target::RandomEnemy | Target::AllEnemies) {
                    (hits, 1)
                } else {
                    (1, hits)
                };
                let source = pending.context.source;
                // A respawn (Axebot's Stock) refills a killed attacker's slot; the attack still stops.
                let attacking = self.instance(source);
                let mut total = 0;
                // Blight Strike's Doom counts TotalDamage, without the overkill Fisticuffs adds.
                let overkill = pending.context.card != content.card_id("CARD.BLIGHT_STRIKE");
                let mut suck_hits = 0;
                let mut killed = false;
                'rounds: for _ in 0..rounds {
                    // An area hit strikes every target before deaths resolve (e.g. Shrink, Crab Rage),
                    // Seeking Edge's Sovereign Blade included.
                    let area = target == Target::AllEnemies
                        || target == Target::ChosenEnemy
                            && pending.context.card == Some(card_id::SOVEREIGN_BLADE)
                            && self.creature(Actor::Player).power(power_id::SEEKING_EDGE) > 0;
                    let previous =
                        area.then(|| self.combat_mut().unwrap().pending_hits.replace(vec![]));
                    for actor in self.targets(target, pending.context) {
                        let targeted = self.instance(actor);
                        // A later play of the series still targets the card's original creature, which a
                        // respawn (Axebot's Stock) has replaced.
                        if target == Target::ChosenEnemy
                            && pending
                                .context
                                .target_instance
                                .is_some_and(|played| Some(played) != targeted)
                        {
                            continue;
                        }
                        for _ in 0..hits {
                            // A slot refilled mid-attack (Axebot's Stock) holds a creature this attack never targeted.
                            if self.instance(actor) != targeted {
                                break;
                            }
                            let result = self.damage_with_bonus(
                                content,
                                actor,
                                amount,
                                DamageKind::Attack,
                                pending.context,
                            );
                            total += if overkill {
                                result.dealt
                            } else {
                                result.blocked + result.lost
                            };
                            suck_hits += (result.lost > 0) as i16;
                            // A respawn (Axebot's Stock) refills the slot, but the target was still killed.
                            // The Waterfall Giant's death leaves it at i16::MAX HP, still a kill.
                            killed |= result.lost > 0
                                && (self.instance(actor) != targeted
                                    || self.creature(actor).hp <= 0
                                    || self.creature(actor).hp == i16::MAX);
                            if self.creature(source).hp <= 0 || self.instance(source) != attacking {
                                if let Some(previous) = previous {
                                    self.resolve_deaths(content, previous);
                                }
                                break 'rounds;
                            }
                        }
                    }
                    if let Some(previous) = previous {
                        self.resolve_deaths(content, previous);
                    }
                }
                let suck = self.creature(pending.context.source).power(power_id::SUCK);
                if suck > 0 {
                    self.apply_power(
                        content,
                        pending.context.source,
                        power_id::STRENGTH,
                        suck * suck_hits,
                    );
                }
                self.combat_mut().unwrap().last_damage = total;
                self.combat_mut().unwrap().last_killed = killed;
                self.creature_mut(pending.context.source)
                    .powers
                    .retain(|power| power.id != power_id::VIGOR);
                if pending.context.source == Actor::Player {
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
            Effect::OstyAttack(target, ..) | Effect::OstyAttackMany(target, ..) => {
                if self.creature(Actor::Osty).hp <= 0 {
                    return;
                }
                let hits = match pending.effect {
                    Effect::OstyAttackMany(_, _, hits) => {
                        self.amount(content, pending.context, hits)
                    }
                    Effect::OstyAttack(_, _, hits) => hits as i16,
                    _ => unreachable!(),
                }
                .max(0);
                self.combat_mut().unwrap().history.osty_attacks += 1;
                // Each of Osty's attacks sets every Flatten's cost to 0 this turn (SetThisTurn).
                let flatten = |card: &Card| card.id == card_id::FLATTEN;
                (self.combat_mut().unwrap()).add_costs(flatten, 0, ABSOLUTE | THIS_TURN);
                // An area hit strikes every target before deaths resolve (e.g. Crab Rage).
                let previous = (target == Target::AllEnemies)
                    .then(|| self.combat_mut().unwrap().pending_hits.replace(vec![]));
                for actor in self.targets(target, pending.context) {
                    for _ in 0..hits {
                        self.damage_with_bonus(
                            content,
                            actor,
                            amount,
                            DamageKind::Attack,
                            Context {
                                source: Actor::Osty,
                                ..pending.context
                            },
                        );
                        if self.creature(Actor::Osty).hp <= 0 {
                            break;
                        }
                    }
                }
                if let Some(previous) = previous {
                    self.resolve_deaths(content, previous);
                }
                if pending.context.card.is_some() {
                    self.creature_mut(Actor::Player)
                        .consume_power(power_id::GIGANTIFICATION);
                }
                for _ in 0..self.relic_count(content, "RELIC.BONE_FLUTE") {
                    self.gain_block(content, Actor::Player, 2, None, false);
                }
            }
            Effect::MoveDamage(target, _) => {
                let card = pending
                    .context
                    .card
                    .map(|card| content.cards[card as usize].id);
                let mut kills = 0;
                for actor in self.targets(target, pending.context) {
                    // A death can refill the target's slot (Surprise's gremlins), so the splash
                    // skips the target by instance.
                    let hit = self.instance(actor);
                    let result = self.damage_with_bonus(
                        content,
                        actor,
                        amount,
                        DamageKind::Move,
                        pending.context,
                    );
                    let dealt = result.dealt;
                    // A refilled slot (Surprise's gremlins) means the hit creature died.
                    kills += (self.instance(actor) != hit || self.creature(actor).hp <= 0) as i16;
                    if card == Some("CARD.OMNISLICE") {
                        for other in self.targets(Target::AllEnemies, pending.context) {
                            if self.instance(other) != hit {
                                self.damage(
                                    content,
                                    pending.context.source,
                                    other,
                                    dealt,
                                    DamageKind::Unpowered,
                                    pending.context.card,
                                );
                            }
                        }
                    }
                }
                // Echoing Slash strikes again for every enemy it kills.
                if card == Some("CARD.ECHOING_SLASH") && kills > 0 {
                    for _ in 0..kills {
                        self.combat_mut().unwrap().queue.push(pending);
                    }
                } else if pending.context.source == Actor::Player {
                    let player = self.creature_mut(Actor::Player);
                    player.powers.retain(|power| power.id != power_id::VIGOR);
                    player.consume_power(power_id::GIGANTIFICATION);
                }
            }
            Effect::Damage(target, _) => {
                let targets = self.targets(target, pending.context);
                let previous = self.combat_mut().unwrap().pending_hits.replace(vec![]);
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
                self.resolve_deaths(content, previous);
                if pending.context.orb_id == Some(orb_id::LIGHTNING)
                    && pending.context.orb
                    && pending.context.target.is_none()
                {
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
                        self.combat_mut().unwrap().enemies[index].stunned = 1;
                    }
                }
            }
            Effect::DoomKill => {
                let enemies = self.targets(Target::AllEnemies, pending.context);
                self.doom_kill(content, enemies);
            }
            // HP loss is unblockable, unpowered damage (CreatureCmd.Damage).
            Effect::LoseHp(target, _) => {
                for actor in self.targets(target, pending.context) {
                    let kind = DamageKind::Unblockable;
                    self.damage_with_bonus(content, actor, amount, kind, pending.context);
                }
            }
            Effect::Block(target, _) => {
                let targets = self.targets(target, pending.context);
                for &actor in &targets {
                    let powered = pending.context.card.is_some()
                        || matches!(pending.context.source, Actor::Enemy(_));
                    self.gain_block(content, actor, amount, pending.context.card, powered);
                }
            }
            Effect::BlockNextTurn(x) => {
                // Prolong keeps the current block as is; Glitterstream's next-turn block goes
                // through ModifyBlock, relic multipliers included.
                let amount = if x.scale == Scale::Block {
                    amount
                } else {
                    let card = pending.context.card;
                    let amount = self
                        .modify_block(content, Actor::Player, amount, card, true)
                        .0;
                    self.claim_block_doubler(content, Actor::Player, amount, card);
                    amount
                };
                if amount > 0 {
                    self.apply_power(content, Actor::Player, power_id::BLOCK_NEXT_TURN, amount);
                }
            }
            Effect::ToricToughness(_) => {
                // It stores the block the card gained (Dexterity, Frail, No Block included).
                let card = pending.context.card;
                let value = self.modify_block(content, Actor::Player, amount, card, true).0;
                self.creature_mut(Actor::Player).powers.push(Power {
                    id: power_id::TORIC_TOUGHNESS,
                    amount: 2,
                    value,
                    ..Power::default()
                });
            }
            Effect::DodgeRoll(_) => {
                let amount =
                    self.gain_block(content, Actor::Player, amount, pending.context.card, true);
                self.apply_power(content, Actor::Player, power_id::BLOCK_NEXT_TURN, amount);
            }
            Effect::DrawBlockIf(kind, _) => {
                if self
                    .draw(content, 1, pending.context, false, 0, Some(pending.effect))
                    .is_some_and(|card| card.card_type(content.cards[card.id as usize]) == kind)
                {
                    self.gain_block(content, Actor::Player, amount, pending.context.card, true);
                }
            }
            // Entrench's own block is unpowered card block (Vambrace, Pael's Legion, Unmovable).
            Effect::RawBlock(Target::Player, x)
                if x.scale == Scale::Block && pending.context.card.is_some() =>
            {
                self.gain_block(content, Actor::Player, amount, pending.context.card, false);
            }
            Effect::RawBlock(target, _) => {
                for actor in self.targets(target, pending.context) {
                    if amount > 0 {
                        self.gain_block(content, actor, amount, None, false);
                        continue;
                    }
                    // Negative raw block strips block (Expose); losing all of it breaks it too
                    // (CreatureCmd.LoseBlock).
                    let had_block = self.creature(actor).block > 0;
                    let creature = self.creature_mut(actor);
                    creature.block = creature.block.saturating_add(amount).max(0);
                    if had_block && self.creature(actor).block == 0 {
                        self.break_burrow(content, actor);
                    }
                }
            }
            Effect::Heal(target, _) => {
                for actor in self.targets(target, pending.context) {
                    // Enemies don't heal once the combat is ending (Knowledge Demon's heal
                    // after Thorns killed it).
                    if actor != Actor::Player && self.enemies_defeated() {
                        continue;
                    }
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
                            DamageKind::Unblockable,
                            pending.context.card,
                        );
                    }
                    // Lowering max HP also caps current HP (SetMaxHpInternal).
                    let player = self.creature_mut(Actor::Player);
                    player.max_hp = max_hp;
                    player.hp = player.hp.min(max_hp);
                } else {
                    let player = self.creature_mut(Actor::Player);
                    player.max_hp = player.max_hp.saturating_add(amount);
                    player.hp = player.hp.saturating_add(amount).min(player.max_hp);
                }
                self.run.max_hp = self.creature(Actor::Player).max_hp;
                self.run.hp = self.creature(Actor::Player).hp;
                self.sync_red_skull(content);
            }
            Effect::Gold(_) if amount >= 0 => self.gain_gold(content, amount as i32),
            // Gold costs (Seal of Gold, Debt) don't apply once the fight is won.
            Effect::Gold(_) if self.enemies_defeated() => {}
            Effect::Gold(_) => self.run.gold = self.run.gold.saturating_sub(-amount as i32).max(0),
            Effect::Bomb(_) => {
                let player = self.creature_mut(Actor::Player);
                player.add_instance(power_id::THE_BOMB, 3, amount.max(0));
            }
            Effect::Automation(energy) => {
                let player = self.creature_mut(Actor::Player);
                player.add_instance(power_id::AUTOMATION, energy, 10);
            }
            Effect::Panache(_) => {
                let player = self.creature_mut(Actor::Player);
                player.add_instance(power_id::PANACHE, amount, 5);
                // It skips the play that applied it.
                player.powers.last_mut().unwrap().skip_next_decay = true;
            }
            Effect::RollingBoulder(_) => {
                let player = self.creature_mut(Actor::Player);
                player.add_instance(power_id::ROLLING_BOULDER, amount, 0);
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
                        let cost = self.roll_cost();
                        let card = &mut self.combat_mut().unwrap().hand[index];
                        card.add_cost(cost, ABSOLUTE | THIS_TURN | UNTIL_PLAYED);
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
                    .chain(played(&mut combat.queue))
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
            // Slow only counts card plays (SlowAmount); it isn't a debuff application.
            Effect::ApplyPower(target, power_id::SLOW, _) => {
                for actor in self.targets(target, pending.context) {
                    self.creature_mut(actor).add_power(power_id::SLOW, amount);
                }
            }
            Effect::ApplyPower(target, id, _) => {
                for actor in self.targets(target, pending.context) {
                    let new = self.creature(actor).power(id) == 0;
                    self.apply_power(content, actor, id, amount);
                    if new
                        && pending.context.card.is_some_and(|card| {
                            matches!(content.cards[card as usize].id, "CARD.DOUBT" | "CARD.SHAME")
                        })
                        && let Some(power) = self
                            .creature_mut(actor)
                            .powers
                            .iter_mut()
                            .find(|power| power.id == id)
                    {
                        power.skip_next_decay = true;
                    }
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
                    let amount = self.modify_power_amount(content, actor, id, amount);
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
            // Bubble Bubble applies its poison normally (Artifact, Unsettling Lamp, Snecko Skull).
            Effect::StackPower(target, id, _) => {
                for actor in self.targets(target, pending.context) {
                    self.apply_power(content, actor, id, amount);
                }
            }
            Effect::DoublePower(target, id, minimum) => {
                for actor in self.targets(target, pending.context) {
                    let amount = self.creature(actor).power(id).max(minimum);
                    let amount = self.modify_power_amount(content, actor, id, amount);
                    self.apply_debuff(content, actor, id, amount);
                }
            }
            Effect::Misery(_) => {
                let Some(target) = pending.context.target else {
                    return;
                };
                let target = Actor::Enemy(target);
                // A dead target (a replay after the first play's kill) lost its powers on death.
                let alive = self.creature(target).hp > 0;
                let debuffs: Vec<_> = self
                    .creature(target)
                    .powers
                    .iter()
                    .copied()
                    .filter(|power| {
                        alive && content.powers[power.id as usize].debuff
                            || power.id == power_id::STRENGTH && power.amount < 0
                            || TEMPORARY_STRENGTH_DOWN.contains(&power.id)
                    })
                    .collect();
                self.damage_with_bonus(
                    content,
                    target,
                    amount,
                    DamageKind::Attack,
                    Context {
                        source: Actor::Player,
                        ..pending.context
                    },
                );
                let player = self.creature_mut(Actor::Player);
                player.powers.retain(|power| power.id != power_id::VIGOR);
                player.consume_power(power_id::GIGANTIFICATION);
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
                        // Copied Strength already carries their loss (IgnoreNextInstance).
                        if TEMPORARY_STRENGTH_DOWN.contains(&power.id) {
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
                            let amount =
                                self.modify_power_amount(content, actor, power.id, power.amount);
                            self.apply_debuff(content, actor, power.id, amount);
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
                    let amount =
                        -self.modify_power_amount(content, actor, power_id::STRENGTH, -amount);
                    if self.apply_debuff(content, actor, power_id::STRENGTH, -amount) {
                        self.creature_mut(actor).add_power(restore, amount);
                    }
                }
            }
            Effect::Draw(count) => {
                self.draw(content, count, pending.context, false, 0, None);
            }
            Effect::HandDraw(mut count) => {
                if self.combat().unwrap().turn == 1 && !self.combat().unwrap().innate_ordered {
                    self.combat_mut().unwrap().innate_ordered = true;
                    // After BeforeHandDraw (Jeweled Mask, Toolbox), Imbued cards sink to the
                    // bottom, then innate cards move to the top one by one, reversing their order.
                    let rank = |card: &Card| {
                        if card.enchantment == Some(Enchantment::Imbued) {
                            0
                        } else {
                            1 + (card.flags(content.cards[card.id as usize]) & INNATE != 0) as u8
                        }
                    };
                    let draw = &mut self.combat_mut().unwrap().draw;
                    draw.sort_by_key(rank);
                    let start = draw
                        .iter()
                        .position(|card| rank(card) == 2)
                        .unwrap_or(draw.len());
                    draw[start..].reverse();
                    // The turn-1 hand draw takes every innate card, up to the hand limit.
                    count = count.max((draw.len() - start) as u8).min(10);
                }
                self.draw(content, count, pending.context, true, 0, None);
            }
            Effect::DrawAmount(_) => {
                let count = amount.clamp(0, u8::MAX as i16) as u8;
                self.draw(content, count, pending.context, false, 0, None);
            }
            Effect::DrawTo(size) => {
                let count = size[pending.context.upgraded as usize]
                    .saturating_sub(self.combat().unwrap().hand.len() as u8);
                self.draw(content, count, pending.context, false, 0, None);
            }
            Effect::DrawUntilNot(kind) => {
                let base = self.combat().unwrap().queue.len();
                if !self.enemies_defeated()
                    && self
                        .draw(content, 1, pending.context, false, 0, Some(pending.effect))
                        .is_some_and(|card| card.card_type(content.cards[card.id as usize]) == kind)
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
                // CardPileCmd.ShuffleIfNecessary: only an empty draw pile is reshuffled.
                if self.combat().unwrap().draw.is_empty()
                    && !self.combat().unwrap().discard.is_empty()
                {
                    self.combat_mut().unwrap().queue.push(pending);
                    self.shuffle_discard_into_draw(content);
                    self.after_shuffle(content);
                    return;
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
                    card.free_this_turn();
                }
            }
            Effect::Energy(amount) if amount < 0 => {
                let combat = self.combat_mut().unwrap();
                combat.energy = (combat.energy + amount as i16).max(0);
            }
            Effect::Energy(amount) => self.gain_energy(amount as i16),
            Effect::DoubleEnergy => {
                let combat = self.combat_mut().unwrap();
                combat.energy = combat.energy.saturating_mul(2);
            }
            Effect::AddCard(pile, id, count) => {
                // Crash Landing fills the hand with Debris; other cards its play adds (Withering
                // Presence's Wither) still overflow to the discard pile.
                let count = if pile == Pile::Hand
                    && content.cards[id as usize].id == "CARD.DEBRIS"
                    && pending
                        .context
                        .card
                        .is_some_and(|card| content.cards[card as usize].id == "CARD.CRASH_LANDING")
                {
                    count.min(10 - self.combat().unwrap().hand.len() as u8)
                } else {
                    count
                };
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
                let enemy = matches!(pending.context.source, Actor::Enemy(_));
                for _ in 0..count {
                    let card = Card {
                        id,
                        value,
                        ..Card::default()
                    };
                    if enemy {
                        self.add_uncredited(content, pile, card);
                    } else {
                        self.add_generated(content, pile, card);
                    }
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
                            flags: flags & !INKY,
                            enchantment: (flags & INKY != 0).then_some(Enchantment::Inky),
                            enchantment_amount: (flags & INKY != 0) as i16,
                            ..Card::default()
                        },
                    );
                }
            }
            Effect::RandomCard(pile, kind, count, free) => {
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
                // Calamity and Metamorphosis pick every card independently.
                if self.expectation.is_some() {
                } else if count > 1 || kind == CardType::Attack && !free && pile == Pile::Hand {
                    pool = (0..count)
                        .map(|_| {
                            pool[self.rngs.combat_card_generation.below(pool.len() as u32) as usize]
                        })
                        .collect();
                } else {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                // Free cards shuffled into the draw pile (Metamorphosis) stay free all combat.
                let mut blank = Card::default();
                if free && pile == Pile::Draw {
                    blank.free_this_combat();
                } else if free {
                    blank.free_this_turn();
                }
                for id in pool.into_iter().take(count as usize) {
                    let card = Card { id, ..blank };
                    // Cards generated into the draw pile land at random positions.
                    if pile == Pile::Draw {
                        self.add_random(content, pile, card);
                    } else {
                        self.add_generated(content, pile, card);
                    }
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
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(count as usize);
                let mut blank = Card::default();
                if free {
                    blank.free_this_turn();
                }
                self.combat_mut().unwrap().offer = pool
                    .into_iter()
                    .map(|id| Card {
                        id,
                        upgrades: (upgrade && pending.context.upgraded) as u8,
                        ..blank
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
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(count as usize);
                let mut blank = Card::default();
                if free {
                    blank.free_this_turn();
                }
                self.combat_mut().unwrap().offer =
                    pool.into_iter().map(|id| Card { id, ..blank }).collect();
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
                if self.expectation.is_none() {
                    self.rngs.combat_card_generation.shuffle(&mut pool);
                }
                pool.truncate(count as usize);
                let mut blank = Card::default();
                blank.free_this_turn();
                self.combat_mut().unwrap().offer =
                    pool.into_iter().map(|id| Card { id, ..blank }).collect();
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
                    .flat_map(|index| content.characters[index].cards.iter().copied())
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
                let mut blank = Card::default();
                blank.free_this_turn();
                self.combat_mut().unwrap().offer = pool
                    .into_iter()
                    .map(|id| Card {
                        id,
                        upgrades: (upgrade && pending.context.upgraded) as u8,
                        ..blank
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
                            && !matches!(
                                card.rarity,
                                CardRarity::Basic | CardRarity::Ancient | CardRarity::Event
                            )
                    })
                    .collect();
                // Each card heads a fresh full shuffle of the pool (GetDistinctForCombat).
                for _ in 0..amount.max(0) {
                    if pool.is_empty() {
                        break;
                    }
                    let mut shuffled = pool.clone();
                    if self.expectation.is_none() {
                        self.rngs.combat_card_generation.shuffle(&mut shuffled);
                    }
                    let id = shuffled[0];
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
                // Mad Science's Chaos adds its card without the generation hooks.
                let chaos = pending.context.card == content.card_id("CARD.MAD_SCIENCE");
                let mut blank = Card::default();
                if free {
                    blank.free_this_turn();
                }
                for id in pool.into_iter().take(count as usize) {
                    let card = Card { id, ..blank };
                    if chaos {
                        self.add_to_pile(content, pile, card);
                    } else {
                        self.add_generated(content, pile, card);
                    }
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
                        def.cost[0] == 0
                            && def.flags[0] & NO_GENERATE == 0
                            && matches!(
                                def.rarity,
                                CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                            )
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
                    // The game lists the draw pile from its top.
                    if pile == Pile::Draw {
                        candidates.reverse();
                    }
                    let selected = self
                        .rngs
                        .combat_card_selection
                        .below(candidates.len() as u32) as usize;
                    candidates = vec![candidates[selected]];
                } else if pile == Pile::Draw && self.expectation.is_none() {
                    // The game shuffles the draw pile from its top (Power Cell sorts first, as
                    // StableShuffle), takes what fits in hand and adds the cards in that order.
                    let draw = &self.combat().unwrap().draw;
                    let mut picked: Vec<_> =
                        candidates.iter().rev().map(|&index| draw[index]).collect();
                    if filter == CardFilter::PlayableCost(0) {
                        sort_for_shuffle(&mut picked, content);
                    }
                    self.rngs.combat_card_selection.shuffle(&mut picked);
                    let room =
                        if matches!(op, CardOp::Move(Pile::Hand) | CardOp::MoveFree(Pile::Hand)) {
                            10usize.saturating_sub(self.combat().unwrap().hand.len())
                        } else {
                            count
                        };
                    picked.truncate(count.min(room));
                    for card in picked {
                        if let Some(index) = self
                            .combat()
                            .unwrap()
                            .draw
                            .iter()
                            .position(|other| other.instance == card.instance)
                        {
                            self.apply_card_op(content, pile, index, op);
                        }
                    }
                    return;
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
                // The game lists the draw pile from its top.
                let mut candidates: Vec<_> = cards(self.combat().unwrap(), pile).to_vec();
                if pile == Pile::Draw {
                    candidates.reverse();
                }
                let all = candidates.clone();
                candidates.retain(|card| eligible(content, &card, filter, CardOp::AutoPlay(1)));
                if candidates.is_empty() && filter == CardFilter::PlayableOrAny {
                    candidates = all;
                }
                // Catastrophe reshuffles the draw pile before each card; Beat Down shuffles once.
                let count = if pile == Pile::Draw {
                    1
                } else {
                    amount.max(0) as usize
                };
                if self.expectation.is_some() {
                    let mut selected = Vec::new();
                    for _ in 0..count.min(candidates.len()) {
                        let choice = self.expectation_choice(candidates.len()).unwrap_or(0);
                        selected.push(candidates.swap_remove(choice));
                    }
                    candidates = selected;
                } else {
                    sort_for_shuffle(&mut candidates, content);
                    self.rngs.shuffle.shuffle(&mut candidates);
                    candidates.truncate(count);
                }
                // Beat Down's picks stay in the discard pile until each one is played.
                if pile == Pile::Draw {
                    for card in &candidates {
                        let index = cards(self.combat().unwrap(), pile)
                            .iter()
                            .position(|other| other == card)
                            .unwrap();
                        self.remove_pile(pile, index);
                    }
                }
                if pile == Pile::Draw && amount > 1 && !candidates.is_empty() {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlayRandom(
                            pile,
                            filter,
                            Amount::fixed(amount - 1, amount - 1),
                        ),
                        context: pending.context,
                    });
                }
                if pile != Pile::Draw {
                    let combat = self.combat_mut().unwrap();
                    combat
                        .beat_down
                        .extend(candidates.iter().map(|card| card.instance));
                }
                for card in candidates.into_iter().rev() {
                    self.auto_play_off_pile(card);
                }
            }
            Effect::ChooseRandomDraw(count) => {
                self.expectation_unknown();
                // StableShuffle of the draw pile listed from its top: sort, then shuffle.
                let mut candidates: Vec<_> =
                    self.combat().unwrap().draw.iter().rev().copied().collect();
                sort_for_shuffle(&mut candidates, content);
                self.rngs.combat_card_selection.shuffle(&mut candidates);
                candidates.truncate(count as usize);
                for card in &mut self.combat_mut().unwrap().draw {
                    if candidates
                        .iter()
                        .any(|picked| picked.instance == card.instance)
                    {
                        card.turn_flags |= FETCHED;
                    }
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
                let enemy = matches!(pending.context.source, Actor::Enemy(_));
                for _ in 0..count {
                    let card = Card {
                        id,
                        ..Card::default()
                    };
                    if enemy {
                        self.insert_random(pile, card);
                    } else {
                        self.add_random(content, pile, card);
                    }
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
                        card.card_type(content.cards[card.id as usize]) == CardType::Attack
                    })
                    .map(|(index, _)| index)
                    .collect();
                self.rngs.combat_card_selection.shuffle(&mut attacks);
                attacks.truncate(amount.max(0) as usize);
                // The picks reach the hand in shuffled order.
                let picked: Vec<_> = attacks
                    .iter()
                    .map(|&index| self.combat().unwrap().discard[index])
                    .collect();
                attacks.sort_unstable_by(|a, b| b.cmp(a));
                for index in attacks {
                    self.combat_mut().unwrap().discard.remove(index);
                }
                for mut card in picked {
                    card.upgrade(content);
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
                        self.after_shuffle(content);
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
                let base = self.combat().unwrap().queue.len();
                while !self.combat().unwrap().hand.is_empty() {
                    self.apply_card_op(content, Pile::Hand, 0, CardOp::Move(Pile::Exhaust));
                }
                // Exhaust triggers resolve first, but not effects queued before this card
                // (e.g. Whispering Earring's next play).
                self.settle(content, base);
                // CardFactory.GetForCombat over the character pool, in the pool's own order.
                let pool: Vec<_> = content.characters[self.run.character as usize]
                    .cards
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let card = content.cards[id as usize];
                        card.flags[0] & NO_GENERATE == 0
                            && !matches!(
                                card.rarity,
                                CardRarity::Basic | CardRarity::Ancient | CardRarity::Event
                            )
                    })
                    .collect();
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
                // Each pick sees the hand left by the previous auto-play (e.g. Cinder's exhaust);
                // the remaining picks ride in the event.
                let remaining = if pending.context.event > 0 {
                    pending.context.event
                } else {
                    self.creature(Actor::Player).power(power_id::STAMPEDE)
                };
                let attacks: Vec<_> = self
                    .combat()
                    .unwrap()
                    .hand
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| {
                        let def = content.cards[card.id as usize];
                        card.card_type(def) == CardType::Attack && card.flags(def) & UNPLAYABLE == 0
                    })
                    .map(|(index, _)| index)
                    .collect();
                if remaining > 0 && !attacks.is_empty() {
                    let index = attacks[self.rngs.shuffle.below(attacks.len() as u32) as usize];
                    let card = self.combat_mut().unwrap().hand.remove(index);
                    if remaining > 1 {
                        self.combat_mut().unwrap().queue.push(Pending {
                            effect: Effect::Stampede,
                            context: Context {
                                event: remaining - 1,
                                ..pending.context
                            },
                        });
                    }
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlay(card),
                        context: Context::card(card),
                    });
                }
            }
            Effect::FlakCannon(_) => {
                let base = self.combat().unwrap().queue.len();
                let mut count = 0;
                // PlayerCombatState.AllCards order: hand, draw pile from its top, discard pile.
                for pile in [Pile::Hand, Pile::Draw, Pile::Discard] {
                    let mut statuses: Vec<_> = cards(self.combat().unwrap(), pile)
                        .iter()
                        .filter(|card| {
                            content.cards[card.id as usize].card_type == CardType::Status
                        })
                        .map(|card| card.instance)
                        .collect();
                    if pile == Pile::Draw {
                        statuses.reverse();
                    }
                    for instance in statuses {
                        let index = cards(self.combat().unwrap(), pile)
                            .iter()
                            .position(|card| {
                                card.instance == instance
                                    && content.cards[card.id as usize].card_type == CardType::Status
                            })
                            .unwrap();
                        self.apply_card_op(content, pile, index, CardOp::Move(Pile::Exhaust));
                        count += 1;
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
                let mut card = combat.plays.last().map(|play| play.card).unwrap_or(Card {
                    id: pending.context.card.unwrap(),
                    upgrades: pending.context.upgraded as u8,
                    ..Card::default()
                });
                card.instance = 0;
                // A later play of the series clones the card as Razor Tooth upgraded it (Anger).
                if pending.context.upgraded {
                    card.upgrade(content);
                }
                // Adaptive Strike's copy costs 0 for the rest of combat.
                if content.cards[card.id as usize].id == "CARD.ADAPTIVE_STRIKE" {
                    card.add_cost(0, ABSOLUTE);
                }
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
                self.discard_hand(content, count);
            }
            Effect::DiscardHandAdd(id) => {
                let count = self.combat().unwrap().hand.len().min(u8::MAX as usize) as u8;
                // An upgraded Storm of Steel upgrades its Shivs.
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: if pending.context.upgraded {
                        Effect::AddUpgradedCard(Pile::Hand, id, count)
                    } else {
                        Effect::AddCard(Pile::Hand, id, count)
                    },
                    context: pending.context,
                });
                self.discard_hand(content, 0);
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
                        context: Context {
                            off_pile: true,
                            ..pending.context
                        },
                    });
                }
            }
            Effect::AutoPlay(mut card) => {
                let mut context = pending.context;
                // A card picked in advance is played from wherever it is by then.
                if card.instance != 0
                    && card.flags & (DUPE | REPLAY) == 0
                    && let Some((pile, index)) =
                        [Pile::Hand, Pile::Draw, Pile::Discard, Pile::Exhaust]
                            .into_iter()
                            .find_map(|pile| {
                                cards(self.combat().unwrap(), pile)
                                    .iter()
                                    .position(|other| other.instance == card.instance)
                                    .map(|index| (pile, index))
                            })
                {
                    context.off_pile = pile != Pile::Hand;
                    card = self.remove_pile(pile, index);
                }
                self.begin_auto_play(content, card, context)
            }
            Effect::Uproar => self.uproar(content),
            Effect::Fire(trigger) => {
                let context = pending.context;
                self.trigger(content, trigger, context.source, context.event)
            }
            Effect::Listen(trigger, listener) => {
                self.listen(content, trigger, listener, pending.context)
            }
            Effect::Step(step) => self.turn_step(content, step),
            // Juggling clones the turn's third finished attack as its play left it so far.
            Effect::Juggle => {
                let card = (pending.context.subject).and_then(|x| self.played_mut(x).copied());
                if let Some(card) = card.filter(|card| {
                    card.card_type(content.cards[card.id as usize]) == CardType::Attack
                        && self.played_this_turn(content, CardType::Attack) == 3
                }) {
                    for _ in 0..self
                        .creature(Actor::Player)
                        .power(power_id::JUGGLING)
                        .max(0)
                    {
                        // Havoc's forced exhaust belongs to the play, not the card.
                        let mut copy = card;
                        copy.instance = 0;
                        copy.flags &= !EXHAUST;
                        copy.turn_flags &= !EXHAUST;
                        self.add_generated(content, Pile::Hand, copy);
                    }
                }
            }
            // Its hooks don't run once the combat is over (Hellraiser's Strike won the fight).
            Effect::ConfusedRoll(card) => {
                if self.enemies_defeated() {
                    return;
                }
                let cost = self.roll_cost();
                if content.cards[card.id as usize].cost[card.upgrades.min(1) as usize] >= 0 {
                    let combat = self.combat_mut().unwrap();
                    for other in combat
                        .hand
                        .iter_mut()
                        .chain(&mut combat.draw)
                        .chain(&mut combat.discard)
                        .chain(&mut combat.exhaust)
                        .filter(|other| other.instance == card.instance)
                    {
                        other.add_cost(cost, ABSOLUTE);
                    }
                }
            }
            Effect::BeforePlay => self.before_play(content),
            Effect::OnPlay => self.queue_play(content),
            Effect::AfterPlay => self.after_play(content),
            Effect::PlacePlayed(card, replay, feral) => {
                if self.place_card(content, card, replay, feral) {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::Step(Step::PlayedCleanup(card.instance)),
                        context: Context::card(card),
                    });
                    self.exhausted(content, card, false);
                }
            }
            Effect::CheckEmptyHand => self.unceasing_top(content),
            Effect::MummifiedHand => self.mummified_hand(content),
            Effect::ReturnLate(make_it_so, energy) => {
                if make_it_so {
                    self.return_make_it_so();
                }
                self.return_right_hands(energy);
            }
            // Each copy plays up to 13 cards.
            Effect::WhisperingEarring(played) => {
                if played < 13
                    && !self.combat().unwrap().force_end
                    && !self.enemies_defeated()
                    && let Some(&hand) = self.playable_cards(content).first()
                {
                    // The first playable hand card, aimed at the leftmost hittable enemy.
                    let target = self.play_targets(content, hand).first().copied();
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::WhisperingEarring(played + 1),
                        context: pending.context,
                    });
                    let Some(target) = target else {
                        // Its resources are spent, but with every enemy reviving the auto-play
                        // finds no target and moves the card to its result pile unplayed.
                        let card = self.combat().unwrap().hand[hand];
                        let def = content.cards[card.id as usize];
                        let spiked = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
                        let combat = self.combat_mut().unwrap();
                        combat.energy -= energy_cost(combat, card, def, spiked, true);
                        combat.stars -= star_cost(combat, card, def);
                        combat.hand.remove(hand);
                        if card.flags(def) & EXHAUST != 0 {
                            combat.exhaust.push(card);
                            self.exhausted(content, card, false);
                        } else {
                            combat.discard.push(card);
                        }
                        return;
                    };
                    // Its resources are spent before CardCmd.AutoPlay; Void Form frees these hand
                    // cards too, but only manual plays use it up.
                    self.play(content, hand, target, false);
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
                    .draw(
                        content,
                        1,
                        pending.context,
                        false,
                        FILTERED_DRAW,
                        Some(pending.effect),
                    )
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
            // Scrape discards the drawn cards it rejects as one CardCmd.Discard.
            Effect::FinishDrawFiltered(filter) => {
                let mut rejected = Vec::new();
                for index in 0..self.combat().unwrap().hand.len() {
                    let card = self.combat().unwrap().hand[index];
                    if card.turn_flags & FILTERED_DRAW != 0 {
                        self.combat_mut().unwrap().hand[index].turn_flags &= !FILTERED_DRAW;
                        if !eligible(content, &&card, filter, CardOp::Move(Pile::Discard)) {
                            rejected.push(index);
                        }
                    }
                }
                self.discard_indices(content, &rejected);
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
            // Second Wind exhausts the non-attacks it saw in hand, not cards drawn meanwhile
            // (Joss Paper); the context's event counts the steps left plus one.
            Effect::ExhaustForBlock(value) => {
                let non_attack = |card: &Card| {
                    card.card_type(content.cards[card.id as usize]) != CardType::Attack
                };
                let hand = &self.combat().unwrap().hand;
                let remaining = if pending.context.event > 0 {
                    pending.context.event - 1
                } else {
                    hand.iter().filter(|card| non_attack(card)).count() as i16
                };
                let index = hand.iter().position(non_attack);
                if remaining > 0
                    && let Some(index) = index
                {
                    let combat = self.combat_mut().unwrap();
                    combat.queue.push(Pending {
                        effect: pending.effect,
                        context: Context {
                            event: remaining,
                            ..pending.context
                        },
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
                    // One attack command with a hit per exhausted card (Vigor covers every hit).
                    let hits = pending.context.event.clamp(0, u8::MAX as i16) as u8;
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::Attack(target, value, hits),
                        context: pending.context,
                    });
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
                        card.card_type(content.cards[card.id as usize]) == CardType::Attack
                    })
                    .map(|(index, _)| index)
                    .collect();
                for index in attacks {
                    let mut card = Card {
                        id,
                        instance: self.next_card,
                        upgrades: upgrades[pending.context.upgraded as usize],
                        ..Card::default()
                    };
                    self.afflict_new(content, &mut card);
                    self.combat_mut().unwrap().hand[index] = card;
                    self.next_card = self.next_card.saturating_add(1);
                    self.trigger_generated(content, id);
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
                if let [
                    Effect::Attack(
                        target @ (Target::ChosenEnemy | Target::RandomEnemy | Target::AllEnemies),
                        damage,
                        1,
                    ),
                ] = effects
                {
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::Attack(
                            *target,
                            *damage,
                            amount.clamp(0, u8::MAX as i16) as u8,
                        ),
                        context: pending.context,
                    });
                    return;
                }
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
                // Orbs are rolled even after the combat ends (channel then does nothing): Chaos's,
                // and Trash to Treasure's when another generation hook (Smokestack) won the fight.
                for _ in 0..count[pending.context.upgraded as usize] {
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
            Effect::PassiveLightning(count) => {
                for _ in 0..count {
                    for index in (0..self.combat().unwrap().orbs.len()).rev() {
                        if self.combat().unwrap().orbs[index].id == orb_id::LIGHTNING {
                            self.passive_orb_target(content, index, pending.context.target, 1);
                        }
                    }
                }
            }
            Effect::OrbSlots(amount) => {
                let combat = self.combat_mut().unwrap();
                combat.orb_slots = (combat.orb_slots as i16 + amount as i16).clamp(0, 10) as u8;
                // Lost slots drop the last orbs without evoking them (OrbQueue.RemoveCapacity).
                let slots = combat.orb_slots as usize;
                combat.orbs.truncate(slots);
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
                        .chain(combat.plays.iter().map(|play| &play.card))
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
                    .chain(combat.plays.iter_mut().map(|play| &mut play.card))
                    .chain(played(&mut combat.queue))
                    .filter(|card| card.id == card_id::SOVEREIGN_BLADE)
                {
                    card.value = card.value.saturating_add(amount);
                }
            }
            Effect::Summon(_) => {
                let combat = self.combat_mut().unwrap();
                // The first summon of a combat creates Osty as a new creature; later ones revive it.
                if combat.osty.max_hp == 0 && amount > 0 {
                    combat.next_creature += 1;
                }
                let osty = &mut combat.osty;
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
                        .plays
                        .last_mut()
                        .map(|play| &mut play.card)
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
                for card in combat
                    .plays
                    .iter_mut()
                    .map(|play| &mut play.card)
                    .chain(played(&mut combat.queue))
                    .filter(|card| card.id == id)
                {
                    card.value = card.value.saturating_add(amount);
                }
            }
            Effect::PersistCard(_) => {
                let persisted = {
                    let combat = self.combat_mut().unwrap();
                    combat
                        .plays
                        .last_mut()
                        .map(|play| &mut play.card)
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
                if let Some(card) = combat.plays.last_mut().map(|play| &mut play.card) {
                    card.add_cost(cost, ABSOLUTE);
                }
            }
            Effect::ReduceCardCost(amount) => {
                let combat = self.combat_mut().unwrap();
                if let Some(card) = combat.plays.last_mut().map(|play| &mut play.card) {
                    card.add_cost(-amount, 0);
                }
            }
            Effect::CapHandCosts => {
                let expiry = if pending.context.upgraded {
                    0
                } else {
                    THIS_TURN | UNTIL_PLAYED
                };
                for card in &mut self.combat_mut().unwrap().hand {
                    card.add_cost(1, ABSOLUTE | REDUCE_ONLY | expiry);
                }
            }
            Effect::RecycleHand(_) | Effect::ShuffleHandDraw(_) => {
                let count = match pending.effect {
                    Effect::RecycleHand(draw) => draw[pending.context.upgraded as usize],
                    Effect::ShuffleHandDraw(count) => count,
                    _ => unreachable!(),
                };
                let combat = self.combat_mut().unwrap();
                let hand = std::mem::take(&mut combat.hand);
                combat.draw.splice(0..0, hand.into_iter().rev());
                combat.queue.push(Pending {
                    effect: Effect::Draw(count),
                    context: pending.context,
                });
                self.shuffle_discard_into_draw(content);
                self.after_shuffle(content);
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

    /// Hook.ModifyDamage, then the caps. A played card's decimal damage (Thrash) keeps the
    /// fraction its integer base dropped.
    pub(crate) fn modify_damage(
        &self,
        content: &Content,
        target: Option<Actor>,
        base: i16,
        kind: DamageKind,
        context: Context,
        played: Option<Card>,
    ) -> i16 {
        let fraction = played
            .and_then(|card| content.cards[card.id as usize].damage_cents(card))
            .map_or(0, |cents| cents.rem_euclid(100));
        let cents = self.modify_damage_cents(
            content,
            target,
            base as i64 * 100 + fraction,
            kind,
            context,
            played,
        );
        let mut amount = cents.div_euclid(100).clamp(0, i16::MAX as i64) as i16;
        if let Some(target) = target
            && !matches!(kind, DamageKind::Overflow)
        {
            let cap = self.creature(target).power(power_id::HARD_TO_KILL);
            if cap > 0 {
                amount = amount.min(cap);
            }
            if self.creature(target).kind(content, PowerKind::Intangible) > 0 {
                amount = amount.min(1);
            }
        }
        amount
    }

    /// Hook.ModifyDamage in hundredths (the game's decimals): the card's enchantment, the
    /// additive then the multiplicative modifiers. With no target (Thrash) only the dealer's
    /// side applies.
    pub(crate) fn modify_damage_cents(
        &self,
        content: &Content,
        target: Option<Actor>,
        cents: i64,
        kind: DamageKind,
        context: Context,
        played: Option<Card>,
    ) -> i64 {
        let (source, card) = (context.source, context.card);
        let attack = matches!(kind, DamageKind::Attack);
        let powered = matches!(kind, DamageKind::Attack | DamageKind::Move);
        let on = |id| target.map_or(0, |target| self.creature(target).power(id));
        let on_kind =
            |power_kind| target.map_or(0, |target| self.creature(target).kind(content, power_kind));
        // Calculated damage (Precise Cut) may go negative; modifiers apply before the final clamp.
        // Bonuses add whole points; the fraction waits for the single final floor.
        let mut amount = cents.div_euclid(100) as i16;
        let mut fraction = cents.rem_euclid(100);
        if powered && matches!(source, Actor::Player | Actor::Osty) {
            match played.and_then(|card| card.enchantment.map(|enchantment| (card, enchantment))) {
                Some((_, Enchantment::Corrupted)) => {
                    let corrupted = (amount as i64 * 100 + fraction) * 3 / 2;
                    fraction = corrupted.rem_euclid(100);
                    amount = corrupted.div_euclid(100) as i16;
                }
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
                // Razor Tooth upgrades the card between the plays of a series (context.upgraded).
                if (played.upgrades > 0 || context.upgraded)
                    && self.has_relic(content, "RELIC.MINIATURE_CANNON")
                {
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
            amount = amount.saturating_add(self.creature(source).power(power_id::VIGOR));
            if source == Actor::Osty {
                amount =
                    amount.saturating_add(self.creature(Actor::Player).power(power_id::CALCIFY));
            }
        } else if powered && source == Actor::Player && card.is_some() {
            // Omnislice and Echoing Slash hit inside their card's attack, so Vigor adds to them.
            amount = amount.saturating_add(self.creature(source).power(power_id::VIGOR));
        }
        let mut ratio = (1i64, 1i64);
        let mut scale = |num: i16, den: i64| {
            ratio.0 *= num as i64;
            ratio.1 *= den;
        };
        // Hang multiplies after the additive bonuses (ModifyDamageMultiplicative).
        if powered && card == Some(card_id::HANG) {
            scale(on(power_id::HANG).max(1), 1);
        }
        if powered {
            if attack
                && matches!(source, Actor::Player | Actor::Osty)
                && card.is_some_and(|id| content.cards[id as usize].tags & STRIKE_TAG != 0)
            {
                amount = amount
                    .saturating_add(3 * self.relic_count(content, "RELIC.STRIKE_DUMMY"))
                    .saturating_add(self.relic_count(content, "RELIC.FAKE_STRIKE_DUMMY"));
            }
            amount =
                amount.saturating_add(self.creature(source).kind(content, PowerKind::Strength));
            if attack {
                amount = amount.saturating_add(on(power_id::TAINTED));
            }
            // Lethality boosts only the first play of the turn's first attack, Move damage included.
            if matches!(source, Actor::Player | Actor::Osty) && context.lethal {
                scale(
                    100 + self.creature(Actor::Player).power(power_id::LETHALITY),
                    100,
                );
            }
            if matches!(source, Actor::Player | Actor::Osty)
                && card.is_some_and(|id| content.cards[id as usize].tags & MINION_TAG != 0)
                && self.has_relic(content, "RELIC.VITRUVIAN_MINION")
            {
                scale(2, 1);
            }
            // Pen Nib doubles any powered attack of its card, Move damage (Omnislice) included.
            if matches!(source, Actor::Player | Actor::Osty) && context.pen_nib {
                scale(2, 1);
            }
            if on_kind(PowerKind::Vulnerable) > 0 {
                // Cruelty and Paper Phrog read the dealer's own powers and relics, so Osty's
                // attacks get neither.
                let cruelty = if source == Actor::Player && matches!(target, Some(Actor::Enemy(_)))
                {
                    self.creature(Actor::Player).power(power_id::CRUELTY)
                } else {
                    0
                };
                let paper_phrog = 25
                    * (source == Actor::Player
                        && matches!(target, Some(Actor::Enemy(_)))
                        && self.has_relic(content, "RELIC.PAPER_PHROG"))
                        as i16;
                // Debilitate doubles the whole bonus, Cruelty's and Paper Phrog's included.
                let bonus =
                    (50 + cruelty + paper_phrog) * (1 + (on(power_id::DEBILITATE) > 0) as i16);
                scale(100 + bonus, 100);
            }
            if self.creature(source).kind(content, PowerKind::Weak) > 0 {
                let paper_krane = 15
                    * (target == Some(Actor::Player)
                        && matches!(source, Actor::Enemy(_))
                        && self.has_relic(content, "RELIC.PAPER_KRANE"))
                        as i16;
                // Paper Krane lowers Weak's multiplier first; Debilitate then doubles the reduction.
                let weak = 75 - paper_krane;
                scale(
                    if self.creature(source).power(power_id::DEBILITATE) > 0 {
                        2 * weak - 100
                    } else {
                        weak
                    },
                    100,
                );
            }
            if target == Some(Actor::Player)
                && matches!(source, Actor::Enemy(_))
                && self.creature(source).hp <= self.creature(source).power(power_id::DOOM)
                && self.has_relic(content, "RELIC.UNDYING_SIGIL")
            {
                scale(1, 2);
            }
            if on(power_id::DIAMOND_DIADEM) > 0 {
                scale(1, 2);
            }
            scale(100 + on(power_id::SLOW), 100);
            // Double Damage also doubles the owner's pet (Osty) attacks from a card.
            let doubler = if source == Actor::Osty {
                Actor::Player
            } else {
                source
            };
            if self.creature(doubler).power(power_id::DOUBLE_DAMAGE) > 0
                && (source != Actor::Osty || card.is_some())
            {
                scale(2, 1);
            }
            if target == Some(Actor::Player)
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
                    scale(3, 2);
                }
            }
            if target == Some(Actor::Player)
                && matches!(source, Actor::Enemy(_))
                && self.creature(source).kind(content, PowerKind::Vulnerable) > 0
                && on(power_id::COLOSSUS) > 0
            {
                scale(1, 2);
            }
            if self.creature(source).power(power_id::SHRINK) != 0 {
                scale(70, 100);
            }
        }
        // Flutter and Soar halve any powered attack, move damage included (IsPoweredAttack).
        if powered && on(power_id::FLUTTER) > 0 {
            scale(1, 2);
        }
        if powered && on(power_id::SOAR) > 0 {
            scale(1, 2);
        }
        // Tracking (on a Weak enemy) and Gigantification multiply any powered attack from a card,
        // move damage included.
        if powered && matches!(source, Actor::Player | Actor::Osty) && card.is_some() {
            let player = self.creature(Actor::Player);
            if on_kind(PowerKind::Weak) > 0 {
                scale(player.power(power_id::TRACKING).max(1), 1);
            }
            if player.power(power_id::GIGANTIFICATION) > 0 {
                scale(3, 1);
            }
        }
        if attack {
            if source == Actor::Player
                && card == Some(card_id::SOVEREIGN_BLADE)
                && on(power_id::CONQUEROR) > 0
            {
                scale(2, 1);
            }
        }
        ((amount as i64 * 100 + fraction) * ratio.0).div_euclid(ratio.1)
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
        self.damage_with_bonus(
            content,
            target,
            base,
            kind,
            Context {
                source,
                card,
                ..Context::player()
            },
        )
    }

    fn damage_with_bonus(
        &mut self,
        content: &Content,
        target: Actor,
        base: i16,
        kind: DamageKind,
        context: Context,
    ) -> DamageResult {
        let (source, card) = (context.source, context.card);
        if self.creature(target).hp <= 0 {
            return DamageResult::default();
        }
        let overflow_kind = matches!(kind, DamageKind::Overflow);
        let attack = matches!(kind, DamageKind::Attack);
        let powered = matches!(kind, DamageKind::Attack | DamageKind::Move);
        if attack
            && source == Actor::Player
            && let Actor::Enemy(index) = target
        {
            self.combat_mut().unwrap().hits[index] += 1;
        }
        let played = self
            .combat()
            .unwrap()
            .plays
            .last()
            .map(|play| play.card)
            .filter(|played| card == Some(played.id));
        let amount = self.modify_damage(content, Some(target), base, kind, context, played);
        // Thorns strikes back in BeforeDamageReceived; the hit still lands.
        if powered || card.is_some_and(|card| content.cards[card as usize].id == "CARD.OMNISLICE") {
            let thorns = self.creature(target).kind(content, PowerKind::Thorns);
            if thorns > 0 {
                self.damage(content, target, source, thorns, DamageKind::Unpowered, None);
            }
        }
        let block_target = if target == Actor::Osty {
            Actor::Player
        } else {
            target
        };
        let blocked = if matches!(kind, DamageKind::Unblockable | DamageKind::Overflow) {
            0
        } else {
            amount.min(self.creature(block_target).block)
        };
        self.creature_mut(block_target).block -= blocked;
        let mut lost = amount - blocked;
        if target == Actor::Player
            && (matches!(kind, DamageKind::Unblockable) && base > 0
                || blocked == 0 && self.creature(Actor::Player).block == 0
                || lost > 0)
        {
            self.combat_mut().unwrap().history.unblocked_hits += 1;
        }
        let shell = if overflow_kind {
            0
        } else {
            self.creature(target).power(power_id::HARDENED_SHELL)
        };
        if shell > 0 {
            let received = self
                .creature(target)
                .powers
                .iter()
                .find(|power| power.id == power_id::HARDENED_SHELL)
                .unwrap()
                .value;
            // Not floored at 0: once The Boot pushed a hit past the cap, the next hit "loses"
            // negative HP and heals the excess back (LoseHpInternal).
            lost = lost.min(shell - received);
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
        // Hook.ModifyHpLost after Osty took the hit: Slippery, then the relics (Tungsten Rod,
        // Beating Remnant), then Buffer (Late) cancels what is left; The Boot raises a small hit.
        if lost > 0 && self.creature(hp_target).power(power_id::SLIPPERY) > 0 {
            lost = 1;
        }
        if lost > 0 && hp_target == Actor::Player {
            let remaining = (20 - self.combat().unwrap().history.remnant_lost).max(0);
            for (index, relic) in self.run.relics.iter().enumerate() {
                if self.melted_relics.contains(&index) {
                    continue;
                }
                match content.relics[relic.id as usize].id {
                    "RELIC.TUNGSTEN_ROD" => lost = (lost - 1).max(0),
                    "RELIC.BEATING_REMNANT" => lost = lost.min(remaining),
                    _ => {}
                }
            }
        }
        let buffer = self.creature(hp_target).powers.iter().find(|power| {
            content.powers[power.id as usize].kind == PowerKind::Buffer && power.amount > 0
        });
        if let Some(id) = buffer.map(|power| power.id)
            && lost > 0
        {
            self.creature_mut(hp_target).consume_power(id);
            lost = 0;
        }
        if attack
            && (1..5).contains(&lost)
            && matches!(source, Actor::Player | Actor::Osty)
            && matches!(hp_target, Actor::Enemy(_))
            && self.has_relic(content, "RELIC.THE_BOOT")
        {
            lost = 5;
        }
        let hp = self.creature(hp_target).hp.max(0);
        let overflow = (target == Actor::Player && hp_target == Actor::Osty)
            .then(|| lost.saturating_sub(hp))
            .unwrap_or(0);
        // The game's TotalDamage + OverkillDamage: blocked plus HP loss before the HP cap.
        let dealt = blocked + lost;
        lost = lost.min(hp);
        self.creature_mut(hp_target).hp -= lost;
        self.sync_red_skull(content);
        if blocked > 0 && self.creature(target).block == 0 {
            self.break_burrow(content, target);
        }
        // Skittish reacts to any hit of a card's attack command, Omnislice's splash included.
        if (matches!(kind, DamageKind::Attack | DamageKind::Move)
            || card.is_some_and(|card| content.cards[card as usize].id == "CARD.OMNISLICE"))
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
                    ..Context::player()
                },
            });
        }
        if (attack || overflow_kind) && hp_target == Actor::Player && lost > 0 {
            for _ in 0..self.creature(source).power(power_id::PAINFUL_STABS).max(0) {
                self.add_uncredited(
                    content,
                    Pile::Discard,
                    Card {
                        id: card_id::WOUND,
                        ..Card::default()
                    },
                );
            }
        }
        if hp_target == Actor::Osty && lost > 0 {
            self.necro_mastery(content, lost);
        }
        if hp_target == Actor::Player {
            self.combat_mut().unwrap().history.hp_lost += lost;
            if self.creature(Actor::Player).hp > 0 {
                self.combat_mut().unwrap().history.remnant_lost += lost;
            }
            if lost > 0 {
                self.combat_mut().unwrap().history.hp_loss_events += 1;
                if !matches!(kind, DamageKind::Unblockable) {
                    self.damage_taken = true;
                }
            }
        }
        let fully_blocked = !matches!(kind, DamageKind::Unblockable)
            && (blocked > 0 || self.creature(block_target).block > 0)
            && lost == 0;
        let hit = Hit {
            dealer: source,
            target,
            receiver: hp_target,
            card,
            kind,
            blocked,
            lost,
            fully_blocked,
            killed: false,
        };
        self.hit(content, hit);
        let overflow = if overflow > 0 {
            self.damage(
                content,
                source,
                Actor::Player,
                overflow,
                DamageKind::Overflow,
                card,
            )
            .lost
        } else {
            0
        };
        DamageResult {
            lost: lost.saturating_add(overflow),
            blocked,
            dealt,
        }
    }

    /// A hit's hooks and its receiver's death, or both later when one damage command hits
    /// several targets (CreatureCmd.Damage).
    fn hit(&mut self, content: &Content, hit: Hit) {
        let hit = Hit {
            killed: self.creature(hit.receiver).hp <= 0,
            ..hit
        };
        match &mut self.combat_mut().unwrap().pending_hits {
            Some(hits) => hits.push(hit),
            None => {
                self.hit_hooks(content, hit);
                if hit.killed {
                    self.died(content, hit.receiver);
                }
            }
        }
    }

    pub(super) fn kill_actor(&mut self, content: &Content, actor: Actor) {
        let lost = self.creature(actor).hp.max(0);
        if lost == 0 {
            return;
        }
        self.creature_mut(actor).hp = 0;
        if actor == Actor::Osty {
            self.necro_mastery(content, lost);
        }
        self.died(content, actor);
    }

    /// An enemy's death is fatal unless one of its powers denies it (ShouldOwnerDeathTriggerFatal):
    /// Minion always, Reattach while another segment lives.
    fn fatal(&self, index: usize) -> bool {
        let enemies = &self.combat().unwrap().enemies;
        let creature = &enemies[index].creature;
        creature.power(power_id::MINION) == 0
            && (creature.power(power_id::REATTACH) == 0
                || enemies.iter().enumerate().all(|(i, enemy)| {
                    i == index
                        || enemy.creature.hp <= 0
                        || enemy.creature.power(power_id::REATTACH) == 0
                }))
    }

    fn doom_kill(&mut self, content: &Content, actors: Vec<Actor>) {
        let mut doomed = Vec::new();
        for actor in actors {
            if self.creature(actor).hp > 0
                && self.creature(actor).hp <= self.creature(actor).power(power_id::DOOM)
            {
                self.kill_actor(content, actor);
                doomed.push(actor);
            }
        }
        // Book Repair Knife counts the deaths that were fatal once all the doomed died.
        let killed = (doomed.into_iter())
            .filter(|&actor| matches!(actor, Actor::Enemy(x) if self.fatal(x)))
            .count() as i16;
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
            // It's used like any potion, so AfterPotionUsed relics fire (Reptile Trinket).
            for _ in 0..self.relic_count(content, "RELIC.REPTILE_TRINKET") {
                for power in [power_id::REPTILE_TRINKET, power_id::STRENGTH] {
                    self.apply_power(content, Actor::Player, power, 3);
                }
            }
            return;
        }
        if actor == Actor::Player
            && let Some(tail) = self
                .relic_mut(content, "RELIC.LIZARD_TAIL")
                .filter(|tail| tail.counters[0] == 0)
        {
            tail.counters[0] = 1;
            self.creature_mut(actor).hp = (self.creature(actor).max_hp / 2).max(1);
            return;
        }
        if let Actor::Enemy(_) = actor {
            let heist = self.creature(actor).power(power_id::HEIST).max(0) as i32;
            if heist > 0 {
                self.reward_gold_parts.push(heist);
            }
        }
        // A Waterfall Giant with Steam Eruption prevents its removal, which most AfterDeath hooks
        // check (not Gremlin Horn or Crab Rage).
        let prevented = matches!(actor, Actor::Enemy(_))
            && self.creature(actor).power(power_id::STEAM_ERUPTION) > 0
            && content.enemies[self.creature(actor).id as usize].id == "MONSTER.WATERFALL_GIANT";
        if let Actor::Enemy(_) = actor
            && content.enemies[self.creature(actor).id as usize].id == "MONSTER.TORCH_HEAD_AMALGAM"
            && let Some(queen) = self.combat_mut().unwrap().enemies.iter_mut().find(|enemy| {
                enemy.creature.hp > 0
                    && content.enemies[enemy.creature.id as usize].id == "MONSTER.QUEEN"
            })
            && queen.move_index == 2
        {
            queen.move_index = 5;
            queen.last_move = 5;
            queen.repeats = 1;
        }
        let dead = self.instance(actor);
        self.death_hooks(content, actor, prevented);
        if let Actor::Enemy(_) = actor {
            // Its spawning powers go with its other powers once AfterDeath is done, unless a
            // spawn took its slot.
            if self.instance(actor) == dead {
                (self.creature_mut(actor).powers).retain(|power| {
                    !matches!(
                        power.id,
                        power_id::INFESTED | power_id::STOCK | power_id::SURPRISE
                    )
                });
            }
            // The last primary enemy's death takes its minions with it.
            let enemies = &self.combat().unwrap().enemies;
            let minions: Vec<_> = (0..enemies.len())
                .filter(|&i| enemies[i].creature.hp > 0)
                .collect();
            if self.creature(actor).power(power_id::MINION) == 0
                && !minions.is_empty()
                && minions
                    .iter()
                    .all(|&i| enemies[i].creature.power(power_id::MINION) > 0)
            {
                for i in minions {
                    self.kill_actor(content, Actor::Enemy(i));
                }
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

    /// A new creature's instance: the game's next creature id + 1.
    fn next_instance(&mut self) -> u32 {
        let combat = self.combat_mut().unwrap();
        let instance = combat
            .enemies
            .iter()
            .map(|enemy| enemy.instance)
            .max()
            .unwrap_or(1)
            .max(combat.next_creature)
            + 1;
        combat.next_creature = instance;
        instance
    }

    pub(crate) fn spawn_wrigglers(&mut self, content: &Content, count: i16) {
        let id = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == "MONSTER.WRIGGLER")
            .unwrap() as Id;
        let range = content.enemies[id as usize].hp(self.run.ascension);
        for wriggler in 1..=count {
            let instance = self.next_instance();
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
            let index = self.insert_enemy(
                Enemy {
                    instance,
                    creature: Creature {
                        id,
                        hp: if fur_coat { 1 } else { hp },
                        max_hp: hp,
                        block: 0,
                        powers: vec![],
                    },
                    move_index: 0,
                    last_move: usize::MAX,
                    repeats: 0,
                    move_history: vec![],
                    stunned: 0,
                    value: wriggler,
                    slot: wriggler as u8 + 1,
                },
                false,
            );
            self.start_move(index, 0);
        }
    }

    pub(super) fn spawn_enemy(&mut self, content: &Content, name: &str, slot: u8) -> usize {
        let id = content
            .enemies
            .iter()
            .position(|enemy| enemy.id == name)
            .unwrap() as Id;
        let def = &content.enemies[id as usize];
        let taken: Vec<_> = self
            .combat()
            .unwrap()
            .enemies
            .iter()
            .filter(|enemy| enemy.creature.hp > 0)
            .map(|enemy| enemy.creature.max_hp)
            .chain(self.corpse_hp)
            .collect();
        let hp = self.unique_hp(def.hp(self.run.ascension), &taken);
        let instance = self.next_instance();
        let powers = def
            .powers(self.run.ascension)
            .into_iter()
            .map(|(id, amount)| Power {
                id,
                amount,
                ..Power::default()
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
            stunned: 0,
            value: 0,
            slot,
        };
        let index = self.insert_enemy(enemy, true);
        self.start_move(index, def.start);
        index
    }

    /// A creature added on the player's turn rolls at once (AfterCreatureAdded), which keeps its
    /// first move; one added on the enemy turn keeps it through the next roll.
    pub(super) fn start_move(&mut self, index: usize, start: usize) {
        let rolled = !self.combat().unwrap().enemy_turn;
        let enemy = &mut self.combat_mut().unwrap().enemies[index];
        enemy.move_index = start;
        enemy.last_move = if rolled { start } else { usize::MAX };
        enemy.repeats = rolled as u8;
        enemy.move_history = if rolled { vec![start] } else { vec![] };
    }

    /// The first of `slots` no living enemy holds (Encounter.GetNextSlot).
    fn free_slot(&self, mut slots: impl Iterator<Item = u8>) -> Option<u8> {
        let enemies = &self.combat().unwrap().enemies;
        slots.find(|&slot| enemies.iter().all(|e| e.creature.hp <= 0 || e.slot != slot))
    }

    /// The living enemies in the game's order, sorted by slot (SortEnemiesBySlotName).
    fn alive_in_slot_order(&self) -> Vec<usize> {
        let enemies = &self.combat().unwrap().enemies;
        let mut alive: Vec<_> = (0..enemies.len())
            .filter(|&i| enemies[i].creature.hp > 0)
            .collect();
        alive.sort_by_key(|&index| enemies[index].slot);
        alive
    }

    fn random_enemy(&mut self) -> Option<usize> {
        let alive = self.alive_in_slot_order();
        if alive.is_empty() {
            return None;
        }
        let choice = if self.expectation.is_some() {
            self.expectation_choice(alive.len()).unwrap_or(0)
        } else {
            self.rngs.combat_targets.below(alive.len() as u32) as usize
        };
        Some(alive[choice])
    }

    fn insert_enemy(&mut self, enemy: Enemy, reuse_dead: bool) -> usize {
        let combat = self.combat_mut().unwrap();
        let index = combat
            .enemies
            .iter()
            .position(|enemy| {
                reuse_dead
                    && enemy.creature.hp <= 0
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
        for play in &mut combat.plays {
            play.recorded
                .retain(|(actor, _)| *actor != Actor::Enemy(index));
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

    fn necro_mastery(&mut self, content: &Content, lost: i16) {
        let amount =
            lost.saturating_mul(self.creature(Actor::Player).power(power_id::NECRO_MASTERY));
        if amount > 0 {
            self.apply(
                content,
                Pending {
                    effect: Effect::LoseHp(Target::AllEnemies, Amount::fixed(amount, amount)),
                    context: Context::player(),
                },
            );
        }
    }

    /// Relic counters that tick as each play finishes (AfterCardPlayed).
    /// Ornamental Fan, Kunai, Shuriken and Kusarigama count finished attack plays this turn.
    fn every_third_attack(&mut self, content: &Content) {
        let mut third = false;
        for relic in &mut self.run.relics {
            let id = content.relics[relic.id as usize].id;
            if matches!(
                id,
                "RELIC.ORNAMENTAL_FAN" | "RELIC.KUNAI" | "RELIC.SHURIKEN" | "RELIC.KUSARIGAMA"
            ) {
                relic.combat = (relic.combat + 1) % 3;
                third |= relic.combat == 0;
            }
        }
        if third {
            for _ in 0..self.relic_count(content, "RELIC.ORNAMENTAL_FAN") {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::RawBlock(Target::Player, Amount::fixed(4, 4)),
                    context: Context::player(),
                });
            }
            for _ in 0..self.relic_count(content, "RELIC.KUNAI") {
                self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
            }
            for _ in 0..self.relic_count(content, "RELIC.SHURIKEN") {
                self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
            }
            for _ in 0..self.relic_count(content, "RELIC.KUSARIGAMA") {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Damage(Target::RandomEnemy, Amount::fixed(6, 6)),
                    context: Context::player(),
                });
            }
        }
    }

    /// Fabricator spawns a bot from FABRICATOR_BOTS[set..set + 2], never repeating its last spawn;
    /// its `value` holds 1 + the last bot's index.
    fn spawn_bot(&mut self, content: &Content, fabricator: usize, set: usize) {
        let last = self.combat().unwrap().enemies[fabricator].value as usize;
        let options: Vec<_> = (set..set + 2).filter(|&bot| bot + 1 != last).collect();
        let bot = options[self.rngs.monster_ai.below(options.len() as u32) as usize];
        self.combat_mut().unwrap().enemies[fabricator].value = bot as i16 + 1;
        // The first free slot of bot1, bot2, fabricator, bot3, bot4.
        let slot = self.free_slot([1, 2, 4, 5].into_iter()).unwrap_or(0);
        let spawned = self.spawn_enemy(content, FABRICATOR_BOTS[bot], slot);
        self.apply_power(content, Actor::Enemy(spawned), power_id::MINION, 1);
    }

    fn sync_red_skull(&mut self, content: &Content) {
        if self.combat().is_none() {
            return;
        }
        let active = self.creature(Actor::Player).hp * 2 <= self.creature(Actor::Player).max_hp;
        let Some(skull) = self.relic_mut(content, "RELIC.RED_SKULL") else {
            return;
        };
        if active != (skull.combat != 0) {
            skull.combat = active as i32;
            self.apply_power(
                content,
                Actor::Player,
                power_id::STRENGTH,
                if active { 3 } else { -3 },
            );
        }
    }

    pub(super) fn sync_belt_buckle(&mut self, content: &Content) {
        if self.combat().is_none() {
            return;
        }
        let active = self.run.potions.iter().all(Option::is_none);
        let Some(buckle) = self.relic_mut(content, "RELIC.BELT_BUCKLE") else {
            return;
        };
        if active != (buckle.combat != 0) {
            buckle.combat = active as i32;
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

    /// The card being played (placed after its hooks), if `card` is it.
    fn played_card(&self, card: Id) -> Option<Card> {
        self.combat()?
            .queue
            .iter()
            .rev()
            .find_map(|pending| match pending.effect {
                Effect::PlacePlayed(played, ..) if played.id == card => Some(played),
                _ => None,
            })
    }

    /// The played card with this instance: in the play pile (still resolving or waiting for its
    /// result pile), or wherever its play moved it (Shining Strike).
    pub(super) fn played_mut(&mut self, instance: u32) -> Option<&mut Card> {
        let Combat {
            plays,
            queue,
            draw,
            hand,
            discard,
            exhaust,
            ..
        } = self.combat_mut()?;
        (plays.iter_mut().rev().map(|play| &mut play.card))
            .chain(played(queue))
            .chain(draw)
            .chain(hand)
            .chain(discard)
            .chain(exhaust)
            .find(|card| card.instance == instance)
    }

    /// CardCmd.Afflict on every combat card of a matching type that has no affliction yet.
    pub(super) fn afflict_all(
        &mut self,
        content: &Content,
        affliction: Affliction,
        types: &[CardType],
    ) {
        let combat = self.combat_mut().unwrap();
        for card in combat
            .hand
            .iter_mut()
            .chain(&mut combat.draw)
            .chain(&mut combat.discard)
            .chain(&mut combat.exhaust)
            .chain(combat.plays.first_mut().map(|play| &mut play.card))
            .filter(|card| {
                card.affliction == Affliction::None
                    && (types.is_empty()
                        || types.contains(&card.card_type(content.cards[card.id as usize])))
            })
        {
            card.affliction = affliction;
        }
    }

    /// AfterCardEnteredCombat: the affliction powers afflict a new card that has none.
    fn afflict_new(&self, content: &Content, card: &mut Card) {
        let combat = self.combat().unwrap();
        if card.affliction != Affliction::None {
            return;
        }
        let card_type = card.card_type(content.cards[card.id as usize]);
        let enemy_power = |id| {
            combat
                .enemies
                .iter()
                .any(|enemy| enemy.creature.hp > 0 && enemy.creature.power(id) > 0)
        };
        card.affliction = if card_type == CardType::Power && enemy_power(power_id::GALVANIC) {
            Affliction::Galvanized
        } else if card_type == CardType::Skill && enemy_power(power_id::VITAL_SPARK) {
            Affliction::Tainted
        } else if card_type == CardType::Attack && combat.player.power(power_id::TANGLED) > 0 {
            Affliction::Entangled
        } else if combat.player.power(power_id::RINGING) > 0 {
            Affliction::Ringing
        } else if card_type == CardType::Skill
            && combat.player.power(power_id::SMOGGY) > 0
            && combat.history.skills > 0
        {
            Affliction::Smog
        } else {
            Affliction::None
        };
    }

    /// Burrowed (Tunneler) ends once its block breaks, stunning it into its dizzy move.
    fn break_burrow(&mut self, content: &Content, target: Actor) {
        if self.creature(target).power(power_id::BURROWED) == 0 {
            return;
        }
        self.creature_mut(target).consume_power(power_id::BURROWED);
        if let Actor::Enemy(index) = target
            && content.enemies[self.creature(target).id as usize].id == "MONSTER.TUNNELER"
        {
            self.stun(index, 3);
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
        let (amount, positive) = self.modify_block(content, actor, base, card, powered);
        self.claim_block_doubler(content, actor, amount, card);
        self.creature_mut(actor).block = self.creature(actor).block.saturating_add(amount);
        if positive {
            if actor == Actor::Player && card.is_some() {
                let play = self.play_key();
                let history = &mut self.combat_mut().unwrap().history;
                if history.block_card != play {
                    history.block_card = play;
                    history.block_card_gains = 0;
                }
                history.block_gains += 1;
                history.block_card_gains += 1;
            }
            self.trigger(content, Trigger::AfterBlockGained, actor, amount);
        }
        amount
    }

    /// Hook.ModifyMaxEnergy over the run's max energy.
    pub(crate) fn max_energy(&self, content: &Content) -> i16 {
        let combat = self.combat().unwrap();
        let power = |id| combat.player.power(id);
        combat.max_energy
            + (combat.turn >= 2 && self.has_relic(content, "RELIC.BREAD")) as i16
            + (combat.turn >= 3) as i16 * self.relic_count(content, "RELIC.PAELS_FLESH")
            + self.pumpkin_candle(content) as i16
            + power(power_id::DEMESNE)
            + power(power_id::FRIENDSHIP)
            + power(power_id::PYRE)
            - power(power_id::WASTE_AWAY)
    }

    /// Hook.ModifyBlock: the block gained, and whether it counts as a gain. The game's block is
    /// a decimal floored only when added: Frail turning 1 block into 0.75 still records a block
    /// gain (Unmovable) and fires AfterBlockGained (Juggernaut).
    pub(crate) fn modify_block(
        &self,
        content: &Content,
        actor: Actor,
        base: i16,
        card: Option<Id>,
        powered: bool,
    ) -> (i16, bool) {
        // No Block stops powered card block only (Entrench's unpowered block still lands).
        if actor == Actor::Player
            && card.is_some()
            && powered
            && self.creature(actor).power(power_id::NO_BLOCK) > 0
        {
            return (0, false);
        }
        // Fasten adds to powered Defend block only.
        let fasten = card
            .filter(|&id| powered && content.cards[id as usize].tags & DEFEND_TAG != 0)
            .map_or(0, |_| self.creature(actor).power(power_id::FASTEN));
        let combat = self.combat().unwrap();
        let enchantment = match combat.plays.last().map(|play| play.card) {
            Some(played) if actor == Actor::Player && card == Some(played.id) => {
                match played.enchantment {
                    Some(Enchantment::Nimble) => played.enchantment_amount,
                    Some(Enchantment::Goopy) => played.enchantment_amount.saturating_sub(1),
                    _ => 0,
                }
            }
            _ => 0,
        };
        // Relic multipliers apply before Frail's floor, like the game's decimal block modifiers.
        let factor = if actor == Actor::Player && card.is_some() {
            let vitruvian = card
                .is_some_and(|id| content.cards[id as usize].tags & MINION_TAG != 0)
                && self.has_relic(content, "RELIC.VITRUVIAN_MINION");
            (1 + vitruvian as i16) * self.relic_block_multiplier(content)
        } else {
            1
        };
        let base = base.saturating_add(fasten).saturating_add(enchantment);
        // Dexterity and Frail need powered block; Unmovable any card or move block (Entrench).
        let dexterity = if powered {
            self.creature(actor).kind(content, PowerKind::Dexterity)
        } else {
            0
        };
        let mut amount = base.saturating_add(dexterity).max(0);
        for _ in 0..self.creature(actor).power(power_id::SHADOWMELD).max(0) {
            amount = amount.saturating_mul(2);
        }
        let history = &combat.history;
        if actor == Actor::Player
            && (powered || card.is_some())
            && history.block_gains
                - if history.block_card == self.play_key() {
                    history.block_card_gains
                } else {
                    0
                }
                < self.creature(actor).power(power_id::UNMOVABLE)
        {
            amount = amount.saturating_mul(2);
        }
        amount = amount.saturating_mul(factor);
        if powered && self.creature(actor).kind(content, PowerKind::Frail) > 0 {
            amount = amount * 3 / 4;
        }
        (
            amount,
            amount > 0 || factor > 0 && base.saturating_add(dexterity) > 0,
        )
    }

    /// Vambrace doubles the block of the first card that gains block; Pael's Legion doubles all
    /// card block (a Sly card auto-played by it included) until that first card's play finishes.
    fn relic_block_multiplier(&self, content: &Content) -> i16 {
        let combat = self.combat().unwrap();
        let instance = combat.plays.last().map_or(0, |play| play.card.instance);
        let first = combat.block_doubler.is_none_or(|card| card == instance);
        let unspent = |name| {
            self.relic(content, name)
                .is_some_and(|relic| relic.combat == 0)
        };
        let vambrace = first && unspent("RELIC.VAMBRACE");
        let legion = unspent("RELIC.PAELS_LEGION");
        (1 + vambrace as i16) * (1 + legion as i16)
    }

    /// AfterModifyingBlockAmount: the first card whose block a doubler raised claims it.
    fn claim_block_doubler(
        &mut self,
        content: &Content,
        actor: Actor,
        amount: i16,
        card: Option<Id>,
    ) {
        if amount > 0
            && actor == Actor::Player
            && card.is_some()
            && self.combat().unwrap().block_doubler.is_none()
            && self.relic_block_multiplier(content) > 1
        {
            let combat = self.combat_mut().unwrap();
            combat.block_doubler = Some(combat.plays.last().map_or(0, |play| play.card.instance));
        }
    }

    /// Identifies the current card play: each replay of a series is its own play (CardPlay).
    pub(crate) fn play_key(&self) -> i32 {
        let combat = self.combat().unwrap();
        let index = 256 * combat.plays.len() as i32
            + combat.plays.last().map_or(0, |play| play.done as i32);
        combat.history.cards as i32 * 4096 + index
    }

    pub(super) fn apply_power(&mut self, content: &Content, actor: Actor, id: Id, amount: i16) {
        // Dead enemies can't receive powers (e.g. Reaper Form's Doom on the creature it killed).
        if matches!(actor, Actor::Enemy(_)) && self.creature(actor).hp <= 0 {
            return;
        }
        // Feral counts the zero-energy attacks already started this turn (AfterApplied).
        if actor == Actor::Player && id == power_id::FERAL && self.creature(actor).power(id) == 0 {
            let history = &mut self.combat_mut().unwrap().history;
            history.feral_returns = history.zero_attacks;
        }
        // Tangled entangles every attack, Ringing every card, that has no affliction yet.
        if actor == Actor::Player && amount > 0 && self.creature(actor).power(id) == 0 {
            match id {
                power_id::TANGLED => {
                    self.afflict_all(content, Affliction::Entangled, &[CardType::Attack])
                }
                power_id::RINGING => self.afflict_all(content, Affliction::Ringing, &[]),
                _ => {}
            }
        }
        // Sword Sage adds replays to every Sovereign Blade in combat (TryAddReplays).
        if actor == Actor::Player && id == power_id::SWORD_SAGE {
            let combat = self.combat_mut().unwrap();
            for card in combat
                .draw
                .iter_mut()
                .chain(&mut combat.hand)
                .chain(&mut combat.discard)
                .chain(&mut combat.exhaust)
                .chain(combat.plays.iter_mut().map(|play| &mut play.card))
                .chain(played(&mut combat.queue))
                .filter(|card| card.id == card_id::SOVEREIGN_BLADE)
            {
                card.replays = (card.replays as i16 + amount).clamp(0, u8::MAX as i16) as u8;
            }
        }
        let amount = self.modify_power_amount(content, actor, id, amount);
        if content.powers[id as usize].debuff {
            self.apply_debuff(content, actor, id, amount);
        } else {
            let before = self.creature(actor).power(id);
            if actor == Actor::Player && id == power_id::ORBIT && amount != 0 {
                self.creature_mut(actor).powers.push(Power {
                    id,
                    amount,
                    ..Power::default()
                });
            } else if id == power_id::RITUAL && before == 0 && matches!(actor, Actor::Enemy(_)) {
                self.creature_mut(actor)
                    .add_power_with_skip_next_decay(id, amount, true);
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
            if actor == Actor::Player && id == power_id::PHANTOM_BLADES {
                self.retain_shivs(content);
            }
        }
    }

    /// Hook.ModifyPowerAmount: Ruined Helmet doubles the first Strength gain; Snecko Skull adds
    /// to Poison before Unsettling Lamp doubles every debuff of the combat's first debuffing card.
    fn modify_power_amount(&mut self, content: &Content, actor: Actor, id: Id, amount: i16) -> i16 {
        let mut amount = amount;
        if actor == Actor::Player
            && id == power_id::STRENGTH
            && amount > 0
            && let Some(helmet) = self
                .relic_mut(content, "RELIC.RUINED_HELMET")
                .filter(|helmet| helmet.combat == 0)
        {
            amount *= 2;
            helmet.combat = 1;
        }
        if id == power_id::POISON
            && matches!(actor, Actor::Enemy(_))
            && amount > 0
            && self.has_relic(content, "RELIC.SNECKO_SKULL")
        {
            amount += 1;
        }
        // Power and relic hooks apply their debuffs with no card source.
        if !self.combat().unwrap().applying_hook
            && matches!(actor, Actor::Enemy(_))
            && (amount < 0 || content.powers[id as usize].debuff && amount > 0)
            && self
                .relic(content, "RELIC.UNSETTLING_LAMP")
                .is_some_and(|lamp| lamp.combat == 0)
            && !self.combat().unwrap().enemy_turn
            && let Some(card) = self.combat().unwrap().plays.last().map(|play| play.card.id)
        {
            if self.combat().unwrap().unsettling_lamp.is_none() {
                self.combat_mut().unwrap().unsettling_lamp = Some(card);
            }
            if self.combat().unwrap().unsettling_lamp == Some(card) {
                return amount * 2;
            }
        }
        amount
    }

    fn apply_debuff(&mut self, content: &Content, actor: Actor, id: Id, amount: i16) -> bool {
        let amount = if id == power_id::SMOGGY {
            amount.saturating_sub(self.creature(actor).power(id))
        } else {
            amount
        };
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
                    ..Power::default()
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
                for card in combat
                    .draw
                    .iter_mut()
                    .chain(&mut combat.hand)
                    .chain(&mut combat.discard)
                    .chain(&mut combat.exhaust)
                    .chain(played(&mut combat.queue))
                    .filter(|card| card.upgrades > 0)
                {
                    combat.dampened.push((card.instance, card.upgrades));
                    card.upgrades = 0;
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
                    self.gain_block(content, Actor::Player, block, None, false);
                }
            }
            // Sleight of Flesh needs the player as applier; enemy-turn debuffs (Plow) come from enemies.
            // Temporary Strength downs (Enfeebling Touch, Mangle) count once, through their Strength.
            if matches!(actor, Actor::Enemy(_)) && !self.combat().unwrap().enemy_turn {
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
        // Accelerant makes enemy Poison tick extra times, at most once per stack.
        let accelerant = match actor {
            Actor::Enemy(_) => self
                .creature(Actor::Player)
                .power(power_id::ACCELERANT)
                .max(0),
            _ => 0,
        };
        for _ in 0..(1 + accelerant).min(power.amount) {
            let amount = self.creature(actor).power(power.id);
            if self.creature(actor).hp <= 0 || amount <= 0 {
                break;
            }
            // The hit's own hooks resolve before the decrement.
            let base = self.combat().unwrap().queue.len();
            self.damage(content, actor, actor, amount, DamageKind::Unblockable, None);
            self.settle(content, base);
            if self.creature(actor).hp > 0 && self.creature(actor).power(power.id) > 0 {
                self.creature_mut(actor).consume_power(power.id);
            }
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
        // Only characters without base orb slots get one; a Defect with no slots channels nothing.
        if content.characters[self.run.character as usize].orb_slots == 0 {
            self.combat_mut().unwrap().orb_slots = self.combat().unwrap().orb_slots.max(1);
        }
        if self.combat().unwrap().orb_slots == 0 {
            return;
        }
        if id == orb_id::LIGHTNING {
            let combat = self.combat_mut().unwrap();
            combat.lightning_channeled = combat.lightning_channeled.saturating_add(1);
        }
        if self.combat().unwrap().orbs.len() == self.combat().unwrap().orb_slots as usize {
            let mut pending = std::mem::take(&mut self.combat_mut().unwrap().queue);
            self.evoke(content, true);
            self.resolve(content);
            let combat = self.combat_mut().unwrap();
            // An evoke kill can restore Dampen's upgrades mid-play; the card's remaining effects
            // read the restored upgrade.
            if let Some(card) =
                (combat.plays.first().map(|play| play.card)).filter(|card| card.upgrades > 0)
            {
                for entry in pending
                    .iter_mut()
                    .filter(|entry| entry.context.card == Some(card.id) && !entry.context.hook)
                {
                    entry.context.upgraded = true;
                }
            }
            pending.append(&mut combat.queue);
            combat.queue = pending;
        }
        self.combat_mut().unwrap().orbs.push(Orb {
            id,
            value: content.orbs[id as usize].initial_value,
        });
        let combat = self.combat_mut().unwrap();
        combat.orbs_channeled = combat.orbs_channeled.saturating_add(1);
        // Metronome hits at once (AfterOrbChanneled), so a kill stops the remaining channels.
        // One damage command hits every enemy before any death resolves (Crab Rage).
        if self.combat().unwrap().orbs_channeled == 7 && self.has_relic(content, "RELIC.METRONOME")
        {
            let previous = self.combat_mut().unwrap().pending_hits.replace(vec![]);
            for enemy in self.targets(Target::AllEnemies, Context::player()) {
                self.damage(
                    content,
                    Actor::Player,
                    enemy,
                    30,
                    DamageKind::Unpowered,
                    None,
                );
            }
            self.resolve_deaths(content, previous);
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
            event: orb.value,
            orb: true,
            orb_id: Some(orb.id),
            ..Context::player()
        };
        push_effects(&mut self.combat_mut().unwrap().queue, def.evoke, context);
    }

    /// Only the turn's own orb passives (OrbQueue) trigger the first orb again for Gold-Plated Cables.
    fn passive_orbs(&mut self, content: &Content, timing: OrbTiming) {
        let orbs = self.combat().unwrap().orbs.clone();
        for (index, orb) in orbs.into_iter().enumerate().rev() {
            if content.orbs[orb.id as usize].timing == timing {
                let triggers = 1
                    + (index == 0) as usize
                        * self.relic_count(content, "RELIC.GOLD_PLATED_CABLES") as usize;
                self.passive_orb_target(content, index, None, triggers);
            }
        }
    }

    pub(super) fn passive_orb(&mut self, content: &Content, index: Option<usize>) {
        let Some(index) = index else { return };
        self.passive_orb_target(content, index, None, 1);
    }

    fn passive_orb_target(
        &mut self,
        content: &Content,
        index: usize,
        target: Option<usize>,
        triggers: usize,
    ) {
        let Some(orb) = self.combat().unwrap().orbs.get(index).copied() else {
            return;
        };
        let def = content.orbs[orb.id as usize];
        let focus = self.creature(Actor::Player).kind(content, PowerKind::Focus);
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
                target,
                event,
                orb: target.is_some(),
                orb_id: Some(orb.id),
                ..Context::player()
            });
        }
        for context in contexts.into_iter().rev() {
            push_effects(&mut self.combat_mut().unwrap().queue, def.passive, context);
        }
    }

    fn trigger_card(&mut self, content: &Content, card: Card, trigger: Trigger) {
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
        // Withering Presence creates Wither with no creator, so player generation hooks
        // (Regalite, Smokestack, ...) and the generated count (Supermassive) ignore it.
        if card == card_id::WITHER {
            return;
        }
        self.trigger_created(content, card);
    }

    /// Generation hooks for a card the player created (a transform into Wither included).
    fn trigger_created(&mut self, content: &Content, card: Id) {
        self.combat_mut().unwrap().history.generated += 1;
        if content.cards[card as usize].tags & SHIV_TAG != 0
            && self.creature(Actor::Player).power(power_id::PHANTOM_BLADES) > 0
        {
            self.retain_shivs(content);
        }
        let powers = self.creature(Actor::Player).powers.clone();
        self.fire(
            content,
            Trigger::AfterCardGeneratedForCombat,
            Context {
                card: Some(card),
                event: 1,
                ..Context::player()
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
            Scale::CardsPlayed
                if context
                    .card
                    .is_some_and(|card| content.cards[card as usize].id == "CARD.GOLD_AXE") =>
            {
                // Gold Axe counts finished plays; its own later replays haven't happened yet, nor
                // have the plays it's auto-played inside (Uproar, Havoc).
                combat.total_cards - (combat.unfinished() - 1).max(0)
            }
            Scale::CardsPlayed => combat.history.cards,
            Scale::AttacksPlayed => combat.history.attacks,
            // Finished attack plays, the playing series' earlier ones included (Duplication).
            Scale::PriorAttacks => self.played_this_turn(content, CardType::Attack),
            // Lunar Blast counts finished skill plays, so skills still resolving don't count.
            Scale::SkillsPlayed => self.finished_plays(content, CardType::Skill),
            // Helix Drill leaves out its own cost while it's being played.
            Scale::EnergySpent => {
                combat.history.energy
                    - combat.plays.last().map(|play| play.card).map_or(0, |card| {
                        card_cost(card, content.cards[card.id as usize], combat.energy)
                    })
            }
            Scale::Exhausted => combat.history.exhausted,
            Scale::HpLost => combat.history.hp_lost,
            Scale::HpLossEvents => combat.history.hp_loss_events,
            Scale::CardValue => combat
                .plays
                .last()
                .map(|play| play.card)
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
                // Cards waiting in the play pile (Cascade's picks, played cards not yet placed)
                // count too; the card being played is counted below.
                let play_pile = combat
                    .queue
                    .iter()
                    .filter_map(|pending| match pending.effect {
                        Effect::AutoPlay(card) | Effect::PlacePlayed(card, ..)
                            if card.flags & DUPE == 0
                                && !(combat.plays.iter())
                                    .any(|play| play.card.instance == card.instance) =>
                        {
                            Some(card)
                        }
                        _ => None,
                    });
                // So do plays still resolving under the current one (Pommel Strike whose draw let
                // Hellraiser auto-play this card).
                let current = combat.plays.last().map(|play| play.card.instance);
                let resolving = (combat.plays.iter().map(|play| play.card))
                    .filter(|card| Some(card.instance) != current && card.flags & DUPE == 0);
                let piles = combat
                    .draw
                    .iter()
                    .chain(&combat.hand)
                    .chain(&combat.discard)
                    .chain(&combat.exhaust)
                    .copied()
                    .chain(play_pile)
                    .chain(resolving);
                // A queued pick still sitting in its pile (Beat Down's later picks) counts once.
                let mut seen = Vec::new();
                piles
                    .filter(|card| {
                        content.cards[card.id as usize].tags & tag != 0 && {
                            let new = !seen.contains(&card.instance);
                            seen.push(card.instance);
                            new
                        }
                    })
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
                .filter(|card| card.card_type(content.cards[card.id as usize]) == kind)
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
                    // Temporary Strength downs count through their Strength alone.
                    .filter(|power| {
                        content.powers[power.id as usize].debuff
                            && !matches!(power.id, power_id::ENFEEBLING_TOUCH | power_id::MANGLE)
                            || power.id == power_id::STRENGTH && power.amount < 0
                    })
                    .count() as i16
            }),
            Scale::Discarded => combat.history.discarded,
            Scale::DrawnCombat => combat.drawn,
            Scale::StarCards => {
                // AllCards includes the play pile (cards waiting for their result pile) and cards
                // queued to auto-play that left their pile (Distilled Chaos's picks).
                let held: Vec<&Card> = combat
                    .draw
                    .iter()
                    .chain(&combat.hand)
                    .chain(&combat.discard)
                    .chain(&combat.exhaust)
                    .chain(combat.plays.iter().map(|play| &play.card))
                    .chain(
                        combat
                            .queue
                            .iter()
                            .filter_map(|pending| match &pending.effect {
                                Effect::PlacePlayed(card, ..) => Some(card),
                                _ => None,
                            }),
                    )
                    .collect();
                let queued = combat
                    .queue
                    .iter()
                    .filter_map(|pending| match &pending.effect {
                        Effect::AutoPlay(card)
                            if !held.iter().any(|other| other.instance == card.instance) =>
                        {
                            Some(card)
                        }
                        _ => None,
                    });
                held.iter()
                    .copied()
                    .chain(queued)
                    .filter(|card| {
                        content.cards[card.id as usize].star_cost[card.upgrades.min(1) as usize]
                            != -1
                    })
                    .count() as i16
            }
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
                .all(|card| card.card_type(content.cards[card.id as usize]) != kind),
            Condition::HandEmpty => self.combat().unwrap().hand.is_empty(),
            Condition::EventAtLeast(value) => context.event >= value,
            Condition::ExhaustAtLeast(count) => {
                self.combat().unwrap().exhaust.len() >= count as usize
            }
            Condition::ExhaustedThisTurn => self.combat().unwrap().history.exhausted > 0,
            Condition::HpLostThisTurn => self.combat().unwrap().history.hp_lost > 0,
            // Feed, The Hunt and Hand of Greed need a fatal kill, which Minion denies, and Reattach
            // denies unless every other segment is dead too.
            // The card's own attack must have landed the kill (a replay on a corpse doesn't).
            Condition::TargetDead => context.target.is_some_and(|x| {
                self.combat().unwrap().last_killed
                    && (self.fatal(x as usize)
                        || !context.card.is_some_and(|card| {
                            matches!(
                                content.cards[card as usize].id,
                                "CARD.FEED" | "CARD.THE_HUNT" | "CARD.HAND_OF_GREED"
                            )
                        }))
            }),
            Condition::FirstTurn => self.combat().unwrap().turn == 1,
            Condition::HasOrb(id) => self.combat().unwrap().orbs.iter().any(|orb| orb.id == id),
            Condition::CardType(kind) => context
                .card
                .is_some_and(|id| content.cards[id as usize].card_type == kind),
            // FTL counts finished plays: not the card playing it (Beat Down) nor itself.
            Condition::PriorCardsBelow(count) => {
                let combat = self.combat().unwrap();
                combat.history.cards - combat.unfinished() < count[context.upgraded as usize] as i16
            }
            Condition::TargetAttacking => context.target.is_some_and(|target| {
                let enemy = &self.combat().unwrap().enemies[target];
                let def = &content.enemies[enemy.creature.id as usize];
                enemy.stunned == 0
                    && (def.moves[enemy.move_index].intent.starts_with("Attack")
                        || effects_attack(def.moves[enemy.move_index].effects))
            }),
            Condition::XAtLeast(value) => context.x >= value,
            Condition::OstyAlive => self.creature(Actor::Osty).hp > 0,
            Condition::DoomApplied => self.combat().unwrap().history.doom_applied > 0,
            Condition::Card(id) => context.card == Some(id),
        }
    }

    fn targets(&mut self, target: Target, context: Context) -> Vec<Actor> {
        // A card's target is a creature, not a slot: a respawn into the slot (Axebot's Stock)
        // isn't the creature the card was played on.
        let targeted =
            |instance, context: Context| context.target_instance.is_none_or(|x| x == instance);
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
                    self.combat().unwrap().enemies.get(x).is_some_and(|enemy| {
                        enemy.creature.hp > 0 && targeted(enemy.instance, context)
                    })
                })
                .map(|x| vec![Actor::Enemy(x)])
                .unwrap_or_default(),
            // Reviving enemies cannot receive powers.
            Target::ChosenEnemyOrDead => context
                .target
                .filter(|&index| {
                    self.combat()
                        .unwrap()
                        .enemies
                        .get(index)
                        .is_some_and(|enemy| {
                            targeted(enemy.instance, context)
                                && (enemy.creature.hp > 0
                                    || [
                                        power_id::ILLUSION,
                                        power_id::REATTACH,
                                        power_id::ADAPTABLE,
                                    ]
                                    .iter()
                                    .all(|&power| enemy.creature.power(power) == 0))
                        })
                })
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
                // Orbs aimed by a card (Tesla Coil) hit that target even if it already died.
                if context.orb
                    && let Some(target) = context.target
                {
                    let alive = self.combat().unwrap().enemies[target].creature.hp > 0;
                    return Vec::from_iter(alive.then_some(Actor::Enemy(target)));
                }
                Vec::from_iter(self.random_enemy().map(Actor::Enemy))
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
    pub(super) fn select(
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
        // Dredge and Neow's Fury only pick what fits in hand.
        let count = if pile == Pile::Discard && op == CardOp::Move(Pile::Hand) && count > 1 {
            count.min(10u8.saturating_sub(self.combat().unwrap().hand.len() as u8))
        } else {
            count
        };
        let candidates: Vec<_> = cards(self.combat().unwrap(), pile)
            .iter()
            .enumerate()
            .filter(|(_, card)| eligible(content, card, filter, op))
            .map(|(i, _)| i)
            .collect();
        let count = count.min(candidates.len() as u8);
        let take_random = random && matches!(op, CardOp::Upgrade);
        let random = random && (count as usize != candidates.len() || count == 1 || take_random);
        if !random
            && pile != Pile::Offer
            && (self.enemies_defeated() || self.creature(Actor::Player).hp <= 0)
        {
            return;
        }
        if random {
            let mut candidates = candidates;
            if self.expectation.is_some() {
                let mut selected = Vec::new();
                for _ in 0..count {
                    let index = self.expectation_choice(candidates.len()).unwrap_or(0);
                    selected.push(candidates.swap_remove(index));
                }
                candidates = selected;
            } else if count == 1 && !take_random {
                let index = self
                    .rngs
                    .combat_card_selection
                    .below(candidates.len() as u32) as usize;
                candidates = vec![candidates[index]];
            } else {
                self.rngs.combat_card_selection.shuffle(&mut candidates);
                candidates.truncate(count as usize);
            }
            candidates.sort_unstable();
            let mut removed = 0;
            for index in candidates {
                removed += self.apply_card_op(content, pile, index - removed, op) as usize;
            }
        } else if count > 0
            && self.combat().unwrap().queue.iter().any(
                |pending| matches!(pending.effect, Effect::WhisperingEarring(played) if played > 0),
            )
        {
            // Whispering Earring's selector takes the first options; a draw pile is listed from
            // its top, then ordered by rarity and id (CardSelectCmd.FromCombatPile).
            let mut candidates = candidates;
            if pile == Pile::Draw {
                candidates.reverse();
                let pile_cards = cards(self.combat().unwrap(), pile);
                candidates.sort_by_key(|&index| {
                    let def = content.cards[pile_cards[index].id as usize];
                    let rarity = match def.card_type {
                        CardType::Status => 8,
                        CardType::Curse => 9,
                        _ if TOKEN_CARDS.contains(&def.id) => 7,
                        _ => match def.rarity {
                            CardRarity::Basic => 1,
                            CardRarity::Common => 2,
                            CardRarity::Uncommon => 3,
                            CardRarity::Rare => 4,
                            CardRarity::Ancient => 5,
                            CardRarity::Event => 6,
                            CardRarity::Quest => 10,
                        },
                    };
                    (rarity, def.id)
                });
            }
            candidates.truncate(count as usize);
            candidates.sort_unstable();
            // Several picked cards go through one CardCmd.Discard: every discard first, then the
            // Sly cards auto-play in hand order.
            if pile == Pile::Hand && op == CardOp::Move(Pile::Discard) && candidates.len() > 1 {
                self.discard_indices(content, &candidates);
                return;
            }
            let mut removed = 0;
            for index in candidates {
                removed += self.apply_card_op(content, pile, index - removed, op) as usize;
            }
        } else if !optional && count as usize == candidates.len() {
            if pile == Pile::Hand && op == CardOp::Move(Pile::Discard) && candidates.len() > 1 {
                let mut candidates = candidates;
                candidates.sort_unstable();
                self.discard_indices(content, &candidates);
            } else if pile == Pile::Draw {
                for index in candidates.into_iter().rev() {
                    self.apply_card_op(content, pile, index, op);
                }
            } else {
                let mut removed = 0;
                for index in candidates {
                    removed += self.apply_card_op(content, pile, index - removed, op) as usize;
                }
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
                let discarded = pile == Pile::Hand && to == Pile::Discard;
                if discarded {
                    self.expectation_discard(1);
                    self.combat_mut().unwrap().history.discarded += 1;
                }
                if !sly {
                    if to == Pile::Hand {
                        self.add_to_hand(card);
                    } else {
                        self.push_pile(to, card);
                    }
                }
                if sly {
                    self.auto_play_off_pile(card);
                }
                // AfterCardDiscarded (Tingsha, Tough Bandages) resolves before the Sly auto-play.
                if discarded {
                    self.after_discard(content, 1);
                }
                if to == Pile::Exhaust {
                    self.thrash_gain(content, card);
                    self.exhausted(content, card, false);
                }
                return true;
            }
            // CardCmd.Upgrade does nothing once combat is ending.
            CardOp::Upgrade if self.enemies_defeated() => {}
            CardOp::Upgrade => self.pile_mut(pile)[index].upgrade(content),
            CardOp::Cost(delta) => self.pile_mut(pile)[index].add_cost(delta, 0),
            CardOp::SetCost(cost) => self.pile_mut(pile)[index].add_cost(cost, ABSOLUTE),
            CardOp::Flag(flag) => self.pile_mut(pile)[index].flags |= flag,
            CardOp::TurnFlag(flag) => self.pile_mut(pile)[index].turn_flags |= flag,
            CardOp::CopyNextTurn(count) => {
                let mut card = self.pile_mut(pile)[index];
                card.instance = 0;
                card.expire_costs(THIS_TURN);
                card.turn_flags = 0;
                self.combat_mut().unwrap().nightmares.push((card, count));
                self.creature_mut(Actor::Player)
                    .add_power(power_id::NIGHTMARE, count as i16);
            }
            // Combat transforms count as generating the new card (Regalite, ...).
            CardOp::Transform(id, upgrades) => {
                let mut card = Card {
                    id,
                    upgrades,
                    instance: self.next_card,
                    ..Card::default()
                };
                self.next_card = self.next_card.saturating_add(1);
                self.afflict_new(content, &mut card);
                self.pile_mut(pile)[index] = card;
                self.trigger_generated(content, id);
            }
            CardOp::AutoPlay(count) => {
                let card = self.remove_pile(pile, index);
                let context = Context {
                    off_pile: pile != Pile::Hand,
                    ..Context::card(card)
                };
                // The repeats play the same card: a cost set until played (Snecko Oil) is gone
                // after the first play, a Confused cost lasts the combat, and Razor Tooth's
                // upgrade from the first play stays. The queue is a stack: repeats go under the first play.
                // They find the card in its result pile, outside the hand (Void Form, Free Skill).
                let razor_tooth = self.razor_tooth(content, card);
                for _ in 1..count {
                    let mut replay = card;
                    replay.flags |= REPLAY;
                    if razor_tooth {
                        replay.upgrade(content);
                    }
                    replay.expire_costs(UNTIL_PLAYED);
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlay(replay),
                        context: Context {
                            off_pile: true,
                            ..context
                        },
                    });
                }
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::AutoPlay(card),
                    context,
                });
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
                let mut card = self.remove_pile(pile, index);
                self.combat_mut().unwrap().offer.clear();
                card.instance = self.next_card;
                self.next_card = self.next_card.saturating_add(1);
                self.afflict_new(content, &mut card);
                self.add_to_hand(card);
                self.trigger_generated(content, card.id);
                return true;
            }
            CardOp::Transfigure => {
                let card = &mut self.pile_mut(pile)[index];
                if content.cards[card.id as usize].cost[card.upgrades.min(1) as usize] >= 0 {
                    card.add_cost(1, 0);
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
                // In combat only cards that can be generated (CanBeGeneratedInCombat); NO_GENERATE
                // also marks curses and statuses that only modifiers can't generate.
                let status_or_curse = matches!(
                    content.cards[old.id as usize].card_type,
                    CardType::Status | CardType::Curse
                );
                let pool: Vec<_> = transform_options(content, old.id)
                    .into_iter()
                    .filter(|&id| {
                        if status_or_curse {
                            !matches!(
                                content.cards[id as usize].id,
                                "CARD.FRANTIC_ESCAPE" | "CARD.SOOT"
                            )
                        } else {
                            content.cards[id as usize].flags[0] & NO_GENERATE == 0
                        }
                    })
                    .collect();
                if !pool.is_empty() {
                    let id = if self.expectation.is_some() {
                        pool[0]
                    } else {
                        pool[self.rngs.combat_card_selection.below(pool.len() as u32) as usize]
                    };
                    // CardFactory.CreateRandomCardForTransform: the new card is never upgraded.
                    let mut card = Card {
                        id,
                        ..Card::default()
                    };
                    self.afflict_new(content, &mut card);
                    self.pile_mut(pile)[index] = card;
                    self.trigger_created(content, id);
                }
            }
            CardOp::DiscardDraw => {
                let card = self.remove_pile(pile, index);
                self.expectation_discard(1);
                let combat = self.combat_mut().unwrap();
                combat.discard.push(card);
                combat.history.discarded += 1;
                if combat.discard_draw.1.is_empty() {
                    combat.discard_draw.0 = combat.queue.len();
                }
                combat.discard_draw.1.push(card);
                self.after_discard(content, 1);
                return true;
            }
            CardOp::MoveFree(to) => {
                let mut card = self.remove_pile(pile, index);
                card.free_this_turn();
                self.push_pile(to, card);
                return true;
            }
            CardOp::FreeCombat => self.pile_mut(pile)[index].free_this_combat(),
        }
        false
    }

    fn auto_play_off_pile(&mut self, card: Card) {
        self.combat_mut().unwrap().queue.push(Pending {
            effect: Effect::AutoPlay(card),
            context: Context {
                off_pile: true,
                ..Context::card(card)
            },
        });
    }

    /// CardCmd.DiscardAndDraw: every discard's hooks (Tingsha, Tough Bandages), then the draw,
    /// then the Sly cards auto-play in hand order.
    fn discard_hand(&mut self, content: &Content, draw: u8) {
        let hand = std::mem::take(&mut self.combat_mut().unwrap().hand);
        self.discard_cards(content, hand, draw);
    }

    /// Discards the hand cards at `indices` (ascending) as one CardCmd.Discard.
    fn discard_indices(&mut self, content: &Content, indices: &[usize]) {
        let mut cards: Vec<_> = indices
            .iter()
            .rev()
            .map(|&index| self.combat_mut().unwrap().hand.remove(index))
            .collect();
        cards.reverse();
        self.discard_cards(content, cards, 0);
    }

    fn discard_cards(&mut self, content: &Content, hand: Vec<Card>, draw: u8) {
        self.expectation_discard(hand.len());
        self.combat_mut().unwrap().history.discarded += hand.len() as i16;
        let count = hand.len();
        // Sly cards wait in the discard pile and auto-play from there in hand order, after
        // every discard's AfterCardDiscarded hooks (Tingsha, Tough Bandages).
        self.combat_mut()
            .unwrap()
            .discard
            .extend(hand.iter().copied());
        for card in hand.into_iter().rev() {
            if self.is_sly(content, card) {
                self.auto_play_off_pile(card);
            }
        }
        if draw > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Draw(draw),
                context: Context::player(),
            });
        }
        self.after_discard(content, count);
    }

    fn after_discard(&mut self, content: &Content, count: usize) {
        if self.combat().unwrap().enemy_turn {
            return;
        }
        for _ in 0..count {
            for _ in 0..self.relic_count(content, "RELIC.TOUGH_BANDAGES") {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::RawBlock(Target::Player, Amount::fixed(3, 3)),
                    context: Context::player(),
                });
            }
            for _ in 0..self.relic_count(content, "RELIC.TINGSHA") {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Damage(Target::RandomEnemy, Amount::fixed(3, 3)),
                    context: Context::player(),
                });
            }
        }
    }

    fn unceasing_top(&mut self, content: &Content) {
        let combat = self.combat_mut().unwrap();
        combat.potions = combat.potions.saturating_sub(1);
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
        // X-cost cards never count as costing energy here (CostsEnergyOrStars).
        let costs = |card: &Card| {
            let def = content.cards[card.id as usize];
            def.cost[card.upgrades.min(1) as usize] >= 0
                && energy_cost(combat, *card, def, spiked, true) > 0
                || star_cost(combat, *card, def) > 0
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
            self.combat_mut().unwrap().hand[choice].free_this_turn();
        }
    }

    fn razor_tooth(&self, content: &Content, card: Card) -> bool {
        card.upgrades == 0
            && matches!(
                card.card_type(content.cards[card.id as usize]),
                CardType::Attack | CardType::Skill
            )
            && self.has_relic(content, "RELIC.RAZOR_TOOTH")
    }

    fn begin_auto_play(&mut self, content: &Content, mut card: Card, mut context: Context) {
        // CardCmd.AutoPlay does nothing once the combat is ending.
        if self.enemies_defeated() {
            if card.flags & REPLAY == 0 {
                self.combat_mut().unwrap().discard.push(card);
            }
            return;
        }
        // A repeat (Decisions, Decisions) plays the same card again from wherever its earlier play
        // left it (CardCmd.AutoPlay moves it to the play pile, then to its result pile: an exhausting
        // card exhausts again).
        if card.flags & (REPLAY | DUPE) == REPLAY {
            let found = [Pile::Discard, Pile::Draw, Pile::Hand, Pile::Exhaust]
                .into_iter()
                .find_map(|pile| {
                    cards(self.combat().unwrap(), pile)
                        .iter()
                        .rposition(|other| other.instance == card.instance && other.id == card.id)
                        .map(|index| (pile, index))
                });
            if let Some((pile, index)) = found {
                card = self.remove_pile(pile, index);
                // Each play picks its own result pile.
                card.turn_flags &= !EXHAUST;
            }
        }
        let def = content.cards[card.id as usize];
        // History Course's Stomp copy is cloned now, with this turn's attack discounts.
        if def.id == "CARD.STOMP" && card.flags & (REPLAY | DUPE) == REPLAY | DUPE {
            card.retain_costs(|cost| cost.kind != THIS_TURN);
            let attacks = self.combat().unwrap().history.attacks;
            card.add_cost(-attacks.clamp(0, 127) as i8, THIS_TURN);
        }
        let flags = card.flags(def);
        // Fan of Knives Shivs and Seeking Edge's Sovereign Blade hit all enemies: no target roll.
        let fan_shiv = def.id == "CARD.SHIV"
            && self.creature(Actor::Player).power(power_id::FAN_OF_KNIVES) > 0
            || card.id == card_id::SOVEREIGN_BLADE
                && self.creature(Actor::Player).power(power_id::SEEKING_EDGE) > 0;
        if let Some(index) = self
            .combat()
            .unwrap()
            .beat_down
            .iter()
            .position(|&i| i == card.instance)
        {
            self.combat_mut().unwrap().beat_down.swap_remove(index);
            if card.target(def) == Target::ChosenEnemy && !fan_shiv && context.target.is_none() {
                context.target = self.random_enemy();
            }
        }
        // Smoggy afflicts skills only once a skill play has finished.
        let combat = self.combat().unwrap();
        let smogged = card.affliction == Affliction::Smog;
        let bound = card.affliction == Affliction::Bound && combat.bound_played;
        // An enemy-targeted auto-play with no hittable enemy goes to its pile unplayed.
        let no_target = card.target(def) == Target::ChosenEnemy
            && !fan_shiv
            && context.target.is_none()
            && self.alive_in_slot_order().is_empty();
        // Normality, Velvet Choker, Sloth and Ringing stop auto-plays too (Hook.ShouldPlay).
        let sloth = combat.player.power(power_id::SLOTH);
        // Velvet Choker counts finished plays: not the card this auto-play happens inside.
        let finished = combat.history.cards - combat.unfinished();
        let limited = card.affliction == Affliction::Ringing && combat.history.cards > 0
            || combat.history.cards >= 3
                && combat
                    .hand
                    .iter()
                    .any(|card| card.flags(content.cards[card.id as usize]) & LIMIT_THREE != 0)
            || finished >= 6 && self.has_relic(content, "RELIC.VELVET_CHOKER")
            || sloth > 0 && combat.history.cards >= sloth;
        if flags & UNPLAYABLE != 0 || smogged || bound || no_target || limited {
            // A repeat (Decisions, Decisions) is the same card, already placed by the first.
            if flags & DUPE != 0 || card.flags & REPLAY != 0 {
            } else if flags & EXHAUST != 0 {
                let combat = self.combat_mut().unwrap();
                combat.exhaust.push(card);
                self.exhausted(content, card, false);
            } else {
                self.combat_mut().unwrap().discard.push(card);
            }
            return;
        }
        if flags & DUPE == 0 {
            self.combat_mut().unwrap().bound_played |= card.affliction == Affliction::Bound;
        }
        if card.target(def) == Target::ChosenEnemy && !fan_shiv && context.target.is_none() {
            context.target = self.random_enemy();
        }
        context.source = Actor::Player;
        context.card = Some(card.id);
        context.upgraded = card.upgrades > 0;
        context.pen_nib = false;
        context.target_instance = context
            .target
            .map(|slot| self.combat().unwrap().enemies[slot].instance);
        let chemical_x = 2 * self.has_relic(content, "RELIC.CHEMICAL_X") as i16;
        let x = def.cost[card.upgrades.min(1) as usize] < 0;
        context.x = if x {
            self.combat().unwrap().energy + chemical_x
        } else {
            0
        };
        // A star X-cost auto-play counts the current stars without spending them (Stardust).
        if def.star_cost[card.upgrades.min(1) as usize] == -2 {
            context.event = self.combat().unwrap().stars + chemical_x;
        }
        // An auto-play's energy value is its modified cost (Borrowed Time, Corruption, ...), but
        // Void Form only frees cards in the hand or play pile (Mayhem, Havoc and Cascade move
        // theirs there first).
        let spiked_gauntlets = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
        let brilliant_scarf = self.has_relic(content, "RELIC.BRILLIANT_SCARF");
        let combat = self.combat_mut().unwrap();
        // Brilliant Scarf frees cards in the hand or play pile while its 5th manual play resolves
        // (an X cost still counts the current energy).
        let finished_manual = combat.history.manual_plays
            - (combat.plays.first())
                .filter(|play| play.manual)
                .map_or(0, |play| (play.plays - play.done) as i16);
        let energy = if brilliant_scarf && finished_manual == 4 && !context.off_pile && !x {
            0
        } else if context.off_pile && combat.void_free() {
            card_cost(card, def, combat.energy) + combat.player.power(power_id::BORROWED_TIME)
        } else {
            energy_cost(combat, card, def, spiked_gauntlets, !context.off_pile)
        };
        let play = Play {
            card,
            plays: 0,
            done: 0,
            energy,
            stars: 0,
            manual: false,
            context,
            recorded: Vec::new(),
        };
        self.start_play(content, play);
    }

    /// A played card reaches its result pile after its AfterCardPlayed hooks, except Shining
    /// Strike, which moves itself to the draw pile during its play.
    fn place_played(&mut self, content: &Content, card: Card, replay: bool, feral: bool) {
        let place = Pending {
            effect: Effect::PlacePlayed(card, replay, feral),
            context: Context::card(card),
        };
        if content.cards[card.id as usize].flags[card.upgrades.min(1) as usize] & RETURN_TO_DRAW
            != 0
        {
            self.apply(content, place);
            let combat = self.combat_mut().unwrap();
            let draw = std::mem::take(&mut combat.swift_after_return);
            if draw > 0 {
                combat.queue.push(Pending {
                    effect: Effect::Draw(draw),
                    context: Context::card(card),
                });
            }
        } else {
            self.combat_mut().unwrap().queue.push(place);
        }
    }

    /// Moves a finished card to its result pile; true when it was exhausted.
    pub(super) fn place_card(
        &mut self,
        content: &Content,
        mut card: Card,
        replay: bool,
        feral: bool,
    ) -> bool {
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        let flags = card.flags(def);
        // Played this turn, it returns next turn (not its AfterCardPlayed clones).
        if matches!(def.id, "CARD.BOLAS" | "CARD.THRUMMING_HATCHET") {
            card.value = 1;
        }
        let combat = self.combat_mut().unwrap();
        // An Ethereal play (as Razor Tooth left the card) makes every Banshee's Cry 2 cheaper.
        if is_ethereal(combat, card, def) {
            combat.history.ethereal += 1;
            for other in combat
                .draw
                .iter_mut()
                .chain(&mut combat.hand)
                .chain(&mut combat.discard)
                .chain(&mut combat.exhaust)
                .chain(std::iter::once(&mut card))
                .chain(played(&mut combat.queue))
                .filter(|other| other.id == card_id::BANSHEES_CRY)
            {
                other.add_cost(-2, 0);
            }
        }
        // Only "until played" costs wear off once the card is placed (AfterCardPlayedCleanup), so
        // the AfterCardPlayed clones (Juggling, Music Box) keep them, and an exhausted card keeps
        // them through its exhaust hooks (Step::PlayedCleanup); combat-long ones (Confused) stay.
        let unexpired = card;
        card.expire_costs(UNTIL_PLAYED);
        // History Course replays the card as its play left it (Razor Tooth's upgrade).
        if matches!(card_type, CardType::Attack | CardType::Skill) && card.flags & DUPE == 0 {
            combat.history_course = Some(card);
        }
        // A dupe (History Course's copy) has no result pile, but Feral still sends it to the hand
        // and Corruption still exhausts a skill.
        let dupe = card.flags & DUPE != 0
            && !(card_type == CardType::Skill && combat.player.power(power_id::CORRUPTION) > 0);
        if replay && card.flags & DUPE == 0 {
            false
        } else if feral {
            // Feral returns it to the top of the hand, which a full hand redirects to the top
            // (index 0) of the discard pile.
            if combat.hand.len() < 10 {
                combat.hand.insert(0, card);
            } else {
                combat.discard.insert(0, card);
            }
            false
        } else if dupe {
            // Shining Strike moves even a dupe (CreateDupe drops Exhaust) onto the draw pile.
            if def.flags[card.upgrades.min(1) as usize] & RETURN_TO_DRAW != 0 {
                card.flags &= !RETURN_TO_DRAW;
                combat.push_draw(card);
            }
            false
        } else if card.flags & DUPE != 0 {
            // Corruption's skill dupe (DUPE shares RETURN_TO_HAND's bit).
            combat.exhaust.push(unexpired);
            true
        } else if flags & RETURN_TO_HAND != 0 {
            if combat.hand.len() < 10 {
                combat.hand.push(card);
            } else {
                combat.discard.push(card);
            }
            false
        } else if card_type != CardType::Power && flags & EXHAUST != 0 {
            // This play's Exhaust (Havoc's ExhaustOnNextPlay included) doesn't carry to the next.
            combat.exhaust.push(Card {
                flags: unexpired.flags & !(RETURN_TO_DRAW | EXHAUST),
                turn_flags: unexpired.turn_flags & !EXHAUST,
                ..unexpired
            });
            true
        } else if flags & RETURN_TO_DRAW != 0 {
            // Nostalgia only redirects cards headed for the discard pile.
            card.flags &= !RETURN_TO_DRAW;
            combat.push_draw(card);
            false
        } else if let Some(index) = combat.rebounds.iter().position(|&id| id == card.instance) {
            combat.rebounds.swap_remove(index);
            combat.push_draw(card);
            false
        } else {
            if card_type != CardType::Power {
                combat.discard.push(card);
            }
            false
        }
    }

    fn release_rupture(&mut self, content: &Content) {
        let player = &mut self.combat_mut().unwrap().player;
        let Some(power) = player
            .powers
            .iter_mut()
            .find(|power| power.id == power_id::RUPTURE && power.value > 0)
        else {
            return;
        };
        let strength = std::mem::take(&mut power.value);
        self.apply_power(content, Actor::Player, power_id::STRENGTH, strength);
    }

    fn uproar(&mut self, content: &Content) {
        if self.creature(Actor::Player).hp <= 0 {
            return;
        }
        let draw = self.combat().unwrap().draw.iter().rev().copied();
        let mut attacks: Vec<_> = draw
            .filter(|card| card.card_type(content.cards[card.id as usize]) == CardType::Attack)
            .collect();
        let playable = |card: &Card| card.flags(content.cards[card.id as usize]) & UNPLAYABLE == 0;
        if attacks.iter().any(playable) {
            attacks.retain(playable);
        }
        sort_for_shuffle(&mut attacks, content);
        self.rngs.shuffle.shuffle(&mut attacks);
        if let Some(card) = attacks.first() {
            let index = self
                .combat()
                .unwrap()
                .draw
                .iter()
                .position(|candidate| candidate.instance == card.instance)
                .unwrap();
            let card = self.combat_mut().unwrap().remove_draw(index);
            self.auto_play_off_pile(card);
        }
    }

    fn resolve_deaths(&mut self, content: &Content, previous: Option<Vec<Hit>>) {
        let hits = std::mem::replace(&mut self.combat_mut().unwrap().pending_hits, previous);
        let hits = hits.unwrap_or_default();
        for &hit in &hits {
            self.hit_hooks(content, hit);
        }
        for hit in hits.iter().filter(|hit| hit.killed) {
            self.died(content, hit.receiver);
        }
    }

    /// Thrash gains the damage of the attack it exhausts, modified with no target
    /// (Hook.ModifyDamage, Move).
    fn thrash_gain(&mut self, content: &Content, exhausted: Card) {
        let def = content.cards[exhausted.id as usize];
        if exhausted.card_type(def) != CardType::Attack
            || (self.combat().unwrap().plays.last())
                .is_none_or(|play| content.cards[play.card.id as usize].id != "CARD.THRASH")
        {
            return;
        }
        let Some(damage) = def.damage_cents(exhausted) else {
            return;
        };
        let context = Context {
            card: Some(exhausted.id),
            ..Context::player()
        };
        // Thrash keeps the decimal (its damage is in hundredths).
        let damage = self.modify_damage_cents(
            content,
            None,
            damage,
            DamageKind::Move,
            context,
            Some(exhausted),
        );
        let combat = self.combat_mut().unwrap();
        let value = &mut combat.plays.last_mut().unwrap().card.value;
        *value = value.saturating_add(damage.clamp(0, i16::MAX as i64) as i16);
    }

    /// A played Maul makes every Maul in combat hit harder, auto-plays included (not the deck's).
    fn grow_maul(&mut self, card: &mut Card) {
        if card.id != card_id::MAUL {
            return;
        }
        let increase = 1 + card.upgrades.min(1) as i16;
        card.value += increase;
        let combat = self.combat_mut().unwrap();
        for other in combat
            .draw
            .iter_mut()
            .chain(&mut combat.hand)
            .chain(&mut combat.discard)
            .chain(&mut combat.exhaust)
            .chain(played(&mut combat.queue))
        {
            if other.id == card_id::MAUL {
                other.value += increase;
            }
        }
    }

    fn spend_enchantment(&mut self, card: &mut Card) {
        match card.enchantment {
            Some(Enchantment::Glam | Enchantment::Vigorous) => card.enchantment_value = 0,
            // Momentum's extra damage grows on every play (OnPlay).
            Some(Enchantment::Momentum) => {
                card.enchantment_value = card
                    .enchantment_value
                    .saturating_add(card.enchantment_amount)
            }
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
    }

    /// Plays of this type finished this turn, the finished plays of cards still resolving included.
    pub(super) fn played_this_turn(&self, content: &Content, card_type: CardType) -> i16 {
        let of_type = |card: Card| card.card_type(content.cards[card.id as usize]) == card_type;
        let plays = self.combat().unwrap().plays.iter();
        let done = plays
            .filter(|play| of_type(play.card))
            .map(|play| play.done as i16);
        self.finished_plays(content, card_type) + done.sum::<i16>()
    }

    /// Plays of this type finished this turn (AfterCardPlayed), excluding cards still resolving.
    fn finished_plays(&self, content: &Content, card_type: CardType) -> i16 {
        let combat = self.combat().unwrap();
        let of_type = |card: Card| card.card_type(content.cards[card.id as usize]) == card_type;
        let played = match card_type {
            CardType::Attack => combat.history.attacks,
            CardType::Skill => combat.history.skills,
            _ => combat.history.powers,
        };
        played
            - (combat.plays.iter())
                .filter(|play| of_type(play.card))
                .map(|play| play.plays as i16)
                .sum::<i16>()
    }

    /// Make It So and Right Hand Hand return after each play's card-played hooks
    /// (AfterCardPlayedLate), before the card reaches its pile; Make It So when the finished
    /// skill count crosses a multiple of 3 between `skills`.
    fn queue_return_late(&mut self, card_type: CardType, skills: (i16, i16), energy: i16) {
        let make_it_so = card_type == CardType::Skill && skills.0 / 3 != skills.1 / 3;
        self.combat_mut().unwrap().queue.push(Pending {
            effect: Effect::ReturnLate(make_it_so, energy),
            context: Context::player(),
        });
    }

    fn return_make_it_so(&mut self) {
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
                // Hook listeners visit the draw pile from its top.
                for index in indices.into_iter().rev() {
                    cards.push(self.combat_mut().unwrap().remove_draw(index));
                }
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
        // A Make It So still in the play pile (the skill that crossed the count) returns last.
        let queue = &mut self.combat_mut().unwrap().queue;
        let mut played = Vec::new();
        queue.retain(|pending| match pending.effect {
            Effect::PlacePlayed(card, ..) if card.id == card_id::MAKE_IT_SO => {
                played.push(card);
                false
            }
            _ => true,
        });
        for card in played.into_iter().rev() {
            self.add_to_hand(card);
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
        // They play in exhaust-pile order.
        for card in cards.into_iter().rev() {
            self.auto_play_off_pile(card);
        }
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

    /// CardPileCmd.AddGeneratedCardsToCombat: nothing once the fight is won.
    fn add_generated(&mut self, content: &Content, pile: Pile, card: Card) {
        if self.enemies_defeated() {
            return;
        }
        let card = self.add_to_pile(content, pile, card);
        self.trigger_generated(content, card.id);
    }

    /// A card an enemy adds has no creator: the player's generation hooks (and Supermassive's
    /// count) ignore it.
    fn add_uncredited(&mut self, content: &Content, pile: Pile, card: Card) {
        if !self.enemies_defeated() {
            self.add_to_pile(content, pile, card);
        }
    }

    /// Adds a new card to a pile without the generation hooks (CardPileCmd.Add).
    fn add_to_pile(&mut self, content: &Content, pile: Pile, mut card: Card) -> Card {
        let def = content.cards[card.id as usize];
        if card.id == card_id::SOVEREIGN_BLADE {
            let sage = self
                .creature(Actor::Player)
                .power(power_id::SWORD_SAGE)
                .max(0);
            card.replays = card.replays.saturating_add(sage as u8);
        }
        if self.has_relic(content, "RELIC.GHOST_SEED")
            && def.rarity == CardRarity::Basic
            && def.tags & (STRIKE_TAG | DEFEND_TAG) != 0
        {
            card.flags |= ETHEREAL;
        }
        // Stomp and Pinpoint enter combat with this turn's discounts (a copy has them already),
        // Banshee's Cry with the combat's, and Flatten free once Osty attacked this turn.
        let history = self.combat().unwrap().history;
        let discount = match def.id {
            "CARD.STOMP" => Some(history.attacks),
            _ => (card.id == card_id::PINPOINT).then_some(history.skills),
        };
        if let Some(discount) = discount {
            card.retain_costs(|cost| cost.kind != THIS_TURN);
            card.add_cost(-discount.clamp(0, 127) as i8, THIS_TURN);
        }
        if card.id == card_id::BANSHEES_CRY {
            card.add_cost(-(2 * history.ethereal).clamp(0, 127) as i8, 0);
        }
        if card.id == card_id::FLATTEN && history.osty_attacks > 0 {
            card.add_cost(0, ABSOLUTE | THIS_TURN);
        }
        // A new card, a clone too (CreateClone), has no plays of its own this turn (Fetch).
        if card.instance == 0 {
            card.instance = self.next_card;
            self.next_card = self.next_card.saturating_add(1);
            card.turn_flags &= !FETCHED;
        }
        self.afflict_new(content, &mut card);
        if pile == Pile::Hand {
            self.add_to_hand(card);
        } else if pile == Pile::Draw {
            self.combat_mut().unwrap().push_draw(card);
        } else {
            self.pile_mut(pile).push(card);
        }
        card
    }

    pub(super) fn add_random(&mut self, content: &Content, pile: Pile, mut card: Card) {
        if self.enemies_defeated() {
            return;
        }
        self.afflict_new(content, &mut card);
        let card = self.insert_random(pile, card);
        self.trigger_generated(content, card.id);
    }

    /// Shuffles a card into a random position of a pile without crediting the player as creator.
    fn insert_random(&mut self, pile: Pile, mut card: Card) -> Card {
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
        card
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

    fn after_shuffle(&mut self, content: &Content) {
        // Biiig Hug's Soot goes in ahead of the AfterShuffle pass (the game lists the relic
        // after the powers, so only a Stratagem pick could tell).
        for _ in 0..self.relic_count(content, "RELIC.BIIIG_HUG") {
            self.add_random(
                content,
                Pile::Draw,
                Card {
                    id: card_id::SOOT,
                    ..Card::default()
                },
            );
        }
        self.trigger(content, Trigger::AfterShuffle, Actor::Player, 0);
    }

    fn shuffle_discard_into_draw(&mut self, content: &Content) {
        let Phase::Combat(combat) = &mut self.phase else {
            return;
        };
        // The game shuffles discard followed by the draw pile (top first).
        let mut cards = std::mem::take(&mut combat.discard);
        cards.extend(combat.draw.drain(..).rev());
        sort_for_shuffle(&mut cards, content);
        if self.expectation.is_none() {
            self.rngs.shuffle.shuffle(&mut cards);
        }
        cards.reverse();
        combat.draw = cards;
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

    fn instance(&self, actor: Actor) -> Option<u32> {
        match actor {
            Actor::Enemy(index) => Some(self.combat().unwrap().enemies[index].instance),
            _ => None,
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

    /// No living primary enemy, unless a power stops the combat from ending
    /// (ShouldStopCombatFromEnding): a dying enemy's spawns, the Test Subject's revive.
    fn enemies_defeated(&self) -> bool {
        let enemies = &self.combat().unwrap().enemies;
        let stops = |power: &Power| {
            matches!(
                power.id,
                power_id::ADAPTABLE
                    | power_id::INFESTED
                    | power_id::STOCK
                    | power_id::SURPRISE
                    | power_id::STEAM_ERUPTION
            )
        };
        enemies
            .iter()
            .all(|x| x.creature.hp <= 0 || x.creature.power(power_id::MINION) > 0)
            && !enemies.iter().any(|x| x.creature.powers.iter().any(stops))
    }

    pub(crate) fn finish_combat(&mut self, content: &Content) {
        let Some(combat) = self.combat() else {
            return;
        };
        if combat.player.hp <= 0 {
            self.run.hp = 0;
            self.phase = Phase::Dead;
        } else if self.enemies_defeated()
            && combat.choice.is_none()
            && combat.queue.is_empty()
            && combat.plays.is_empty()
        {
            for enemy in &mut self.combat_mut().unwrap().enemies {
                if enemy.creature.power(power_id::MINION) > 0 {
                    enemy.creature.hp = 0;
                }
            }
            // Chosen Cheese's max HP lands first (AfterCombatEnd); Meat on the Bone checks HP next
            // (AfterCombatVictoryEarly), before the other victory relics heal.
            let cheese = self.relic_count(content, "RELIC.CHOSEN_CHEESE");
            self.creature_mut(Actor::Player).hp += cheese;
            self.creature_mut(Actor::Player).max_hp += cheese;
            for _ in 0..self.relic_count(content, "RELIC.MEAT_ON_THE_BONE") {
                let player = self.creature_mut(Actor::Player);
                if player.hp * 2 <= player.max_hp {
                    player.hp = (player.hp + 12).min(player.max_hp);
                }
            }
            self.trigger(content, Trigger::AfterCombatVictory, Actor::Player, 0);
            self.resolve(content);
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
            if let Some(ticket) = self.relic_mut(content, "RELIC.WONGOS_MYSTERY_TICKET")
                && ticket.counters[1] == 0
            {
                ticket.counters[0] += 1;
            }
            if let Some(candy) = self.relic_mut(content, "RELIC.LASTING_CANDY") {
                candy.counters[0] += 1;
            }
            let fishing = self.room == Room::Combat;
            let rod = self
                .relic_mut(content, "RELIC.FISHING_ROD")
                .filter(|_| fishing);
            if let Some(rod) = rod {
                rod.counters[0] = (rod.counters[0] + 1) % 3;
                if rod.counters[0] == 0 {
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
            if self.room == Room::Elite {
                for _ in 0..self.relic_count(content, "RELIC.WAR_HAMMER") {
                    self.upgrade_random(content, 4, None, false);
                }
            }
            if self.room == Room::Elite
                && let Some(sword) = (self.run.relics.iter_mut())
                    .find(|relic| content.relics[relic.id as usize].id == "RELIC.SWORD_OF_STONE")
            {
                sword.counters[0] += 1;
                if sword.counters[0] >= 5 {
                    *sword = Relic::new(content.relic_id("RELIC.SWORD_OF_JADE").unwrap());
                }
            }
            if !self.paels_cards.is_empty() {
                let index = self.rngs.rewards.below(self.paels_cards.len() as u32) as usize;
                let mut card = self.paels_cards.remove(index);
                card.instance = 0;
                card.upgrades = card.upgrades.saturating_add(1);
                self.add_card(content, card);
            }
            if let Some(candle) = self.relic_mut(content, "RELIC.PUMPKIN_CANDLE")
                && candle.counters[0] > 0
            {
                candle.counters[0] -= 1;
            }
            let toy_box = self
                .relic_mut(content, "RELIC.TOY_BOX")
                .filter(|toy_box| toy_box.counters[0] < 12);
            if let Some(toy_box) = toy_box {
                toy_box.counters[0] += 1;
                if toy_box.counters[0] % 3 == 0
                    && let Some(index) = self
                        .wax_relics
                        .iter()
                        .copied()
                        .find(|index| !self.melted_relics.contains(index))
                {
                    self.melted_relics.push(index);
                    if matches!(
                        content.relics[self.run.relics[index].id as usize].id,
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
            let hunts = player.power(power_id::THE_HUNT).max(0) as usize;
            self.run.hp = hp;
            self.run.max_hp = max_hp;
            if self.event_combat != 0 {
                self.phase = self.finish_event_combat(content, royalties, removals, hunts);
                return;
            }
            self.phase = if self.room == Room::Boss && self.run.act == 3 {
                if self.run.ascension >= 10 && self.bosses_visited == 1 {
                    Phase::Map
                } else {
                    Phase::Won
                }
            } else if content.acts.get(self.act as usize).is_some() {
                if royalties > 0 {
                    self.reward_gold_parts.push(royalties);
                }
                let mut rewards = self.rewards(content, hunts);
                rewards.removals = removals;
                Phase::Rewards(rewards)
            } else {
                Phase::Map
            };
        }
    }
}
