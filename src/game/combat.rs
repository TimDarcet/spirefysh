use super::*;

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

fn push_play(queue: &mut Vec<Pending>, card: Card, def: CardDef, fan_shiv: bool, context: Context) {
    if card.flags(def) & INKY != 0 {
        let target = if fan_shiv || def.target == Target::AllEnemies {
            Target::AllEnemies
        } else {
            Target::ChosenEnemy
        };
        queue.push(Pending {
            effect: Effect::ApplyPower(target, power_id::WEAK, Amount::fixed(1, 1)),
            context,
        });
    }
    if fan_shiv {
        queue.push(Pending {
            effect: Effect::Attack(Target::AllEnemies, Amount::fixed(4, 6), 1),
            context,
        });
        return;
    }
    if def.id == "CARD.COSMIC_INDIFFERENCE" {
        queue.push(Pending {
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
    push_card_effects(queue, card, def, context);
}

/// Player powers that act in AfterSideTurnEnd, after the Ethereal exhausts: Consuming Shadow
/// and the Temporary Focus powers.
const AFTER_SIDE_TURN_END: [Id; 4] = [
    power_id::CONSUMING_SHADOW,
    power_id::FOCUSED_STRIKE,
    power_id::HOTFIX,
    power_id::SYNCHRONIZE,
];

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
    pub(super) fn play(&mut self, content: &Content, hand: usize, target: Option<usize>) {
        let spiked_gauntlets = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
        let chemical_x = self.has_relic(content, "RELIC.CHEMICAL_X");
        let music_box = self.has_relic(content, "RELIC.MUSIC_BOX");
        let brilliant_scarf = self.has_relic(content, "RELIC.BRILLIANT_SCARF")
            && self.combat().unwrap().history.manual_plays == 4;
        let throwing_axe =
            self.has_relic(content, "RELIC.THROWING_AXE") && !self.combat().unwrap().throwing_axe;
        let pen_nib = self.has_relic(content, "RELIC.PEN_NIB")
            && self.combat().unwrap().hand[hand]
                .card_type(content.cards[self.combat().unwrap().hand[hand].id as usize])
                == CardType::Attack;
        let mut pen_nib_counter = self.pen_nib;
        let razor_tooth = self.razor_tooth(content, self.combat().unwrap().hand[hand]);
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
        combat.bound_played |= card.affliction == Affliction::Bound;
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
        // The result pile is chosen when the play starts, so a mid-play upgrade keeps Exhaust and
        // Corruption only exhausts skills that start after it (not the Havoc that played it).
        card.turn_flags |= card.flags(def) & EXHAUST;
        if card_type == CardType::Skill && combat.player.power(power_id::CORRUPTION) > 0 {
            card.turn_flags |= EXHAUST;
        }
        combat.mark_rebound(def, &card);
        combat.playing = Some(card);
        if music_box
            && card_type == CardType::Attack
            && !combat.music_box
            && combat.music_box_card.is_none()
        {
            combat.music_box_card = Some(card.instance);
        }
        // Free Skill and Free Power spend themselves in BeforeCardPlayed even when Void Form
        // already frees the card.
        let free = match card_type {
            CardType::Power if combat.player.power(power_id::FREE_POWER) > 0 => {
                Some(power_id::FREE_POWER)
            }
            CardType::Skill if combat.player.power(power_id::FREE_SKILL) > 0 => {
                Some(power_id::FREE_SKILL)
            }
            _ => None,
        };
        let veil = is_ethereal(combat, card, def) && combat.player.power(power_id::VEILPIERCER) > 0;
        let free_attack =
            card_type == CardType::Attack && combat.player.power(power_id::FREE_ATTACK) > 0;
        // Brilliant Scarf modifies costs, which an X cost ignores (GetAmountToSpend).
        let cost = if brilliant_scarf && def.cost[card.upgrades.min(1) as usize] >= 0 {
            0
        } else {
            energy_cost(combat, card, def, spiked_gauntlets, true)
        };
        let consumed: Vec<Id> = free
            .into_iter()
            .chain(veil.then_some(power_id::VEILPIERCER))
            .chain(free_attack.then_some(power_id::FREE_ATTACK))
            .collect();
        // Danse Macabre resolves the cost in BeforeCardPlayed, after earlier powers there have
        // spent themselves (a Veilpiercer listed first no longer zeroes the cost).
        let danse = combat
            .player
            .powers
            .iter()
            .position(|power| power.id == power_id::DANSE_MACABRE);
        let danse_cost = match danse {
            Some(danse) if !consumed.is_empty() && cost == 0 && !brilliant_scarf => {
                let mut later = combat.clone();
                for &id in &consumed {
                    if later.player.powers.iter().position(|power| power.id == id) < Some(danse) {
                        later.player.consume_power(id);
                    }
                }
                energy_cost(&later, card, def, spiked_gauntlets, true)
            }
            _ => cost,
        };
        for &id in &consumed {
            combat.player.consume_power(id);
        }
        let x_cost = def.cost[card.upgrades.min(1) as usize] < 0;
        // Whispering Earring spends energy before its auto-play, which then sees an X cost as 0.
        let whispered = x_cost
            && combat.queue.iter().any(
                |pending| matches!(pending.effect, Effect::WhisperingEarring(played) if played > 0),
            );
        let energy_value = if whispered { 0 } else { cost };
        let x = if x_cost { combat.energy } else { 0 } + 2 * (x_cost && chemical_x) as i16;
        combat.energy -= cost;
        if card_type == CardType::Attack {
            combat.discount_stomps(content);
        }
        trigger_orbit(combat, cost);
        let stars = if brilliant_scarf && def.star_cost[card.upgrades.min(1) as usize] != -2 {
            0
        } else {
            star_cost(combat, card, def)
        };
        combat.stars -= stars;
        // The Sealed Throne's stars come in BeforeCardPlayed, after the play's resources are spent.
        let throne = combat.player.power(power_id::THE_SEALED_THRONE);
        if throne > 0 {
            combat.stars = combat.stars.saturating_add(throne);
            combat.history.stars_gained = combat.history.stars_gained.saturating_add(throne);
        }
        combat.card_energy = energy_value;
        combat.card_stars = stars;
        combat.mark_feral(card_type, energy_value, &card);
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
        let signal_boost =
            card_type == CardType::Power && combat.player.power(power_id::SIGNAL_BOOST) > 0;
        let plays = 1
            + throwing_axe as u8
            + card.replays
            + matches!(card.enchantment, Some(Enchantment::Spiral)) as u8
            + (card.enchantment == Some(Enchantment::Glam) && card.enchantment_value > 0) as u8
            + (combat.history.first_plays < combat.player.power(power_id::ECHO_FORM)) as u8
            + signal_boost as u8
            + burst as u8
            + punch as u8
            + duplicate;
        if signal_boost {
            combat.player.consume_power(power_id::SIGNAL_BOOST);
        }
        if punch {
            combat.player.consume_power(power_id::ONE_TWO_PUNCH);
        }
        combat.history.energy += cost;
        combat.card_plays = plays;
        combat.history.cards += plays as i16;
        combat.history.first_plays += 1;
        combat.total_cards += plays as i16;
        combat.history.manual_cards += 1;
        combat.history.manual_plays += plays as i16;
        combat.history.zero_attacks +=
            (card_type == CardType::Attack && energy_value == 0) as i16 * plays as i16;
        let first_attack = card_type == CardType::Attack && combat.history.attacks == 0;
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
            event: stars
                + 2 * (chemical_x && def.star_cost[card.upgrades.min(1) as usize] == -2) as i16,
            orb: false,
            orb_id: None,
            pen_nib: false,
            lethal: false,
            hook: false,
            off_pile: false,
            target_instance: target.map(|slot| combat.enemies[slot].instance),
        };
        let fan_shiv = def.id == "CARD.SHIV" && combat.player.power(power_id::FAN_OF_KNIVES) > 0;
        if card.enchantment == Some(Enchantment::Sown) && card.enchantment_value > 0 {
            if combat.player.power(power_id::NO_ENERGY_GAIN) == 0 {
                combat.energy = combat.energy.saturating_add(card.enchantment_amount);
            }
            combat.playing.as_mut().unwrap().enchantment_value = 0;
        }
        if fetch {
            combat.queue.push(Pending {
                effect: Effect::Draw(1),
                context,
            });
        }
        // Pen Nib counts each play at its own start (BeforeCardPlayed): the later plays of a
        // series advance it when they start, after any auto-play the earlier ones caused.
        if pen_nib {
            pen_nib_counter = (pen_nib_counter + 1) % 10;
        }
        // Each extra play starts once the previous play and its card-played triggers resolve.
        let contexts: Vec<_> = (0..plays)
            .map(|play| Context {
                pen_nib: play == 0 && pen_nib && pen_nib_counter == 0,
                lethal: first_attack && play == 0,
                upgraded: context.upgraded || razor_tooth && play > 0,
                ..context
            })
            .collect();
        // Enchantments act after the card's own effects (queued first, so they resolve last).
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
                let draw = card.enchantment_amount.max(0) as u8;
                if def.flags[card.upgrades.min(1) as usize] & RETURN_TO_DRAW != 0 {
                    combat.swift_after_return = draw;
                } else {
                    combat.queue.push(Pending {
                        effect: Effect::Draw(draw),
                        context,
                    });
                }
            }
            _ => {}
        }
        push_play(&mut combat.queue, card, def, fan_shiv, contexts[0]);
        combat.pending_plays = contexts[1..].to_vec();
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
        self.queue_before_play_blocks(content, card, energy_value, danse_cost, context);
        // Star-spending relics fire when resources are spent, before the card resolves.
        let card_stars = self.combat().unwrap().card_stars;
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
        self.pen_nib = pen_nib_counter;
    }

    /// Vambrace and Pael's Legion double one card play's block; the game spends them in
    /// AfterCardPlayed, so a replayed card (Spiral) is doubled only once.
    /// Vambrace and Pael's Legion are spent once the play of the card they doubled finishes.
    fn spend_block_doublers(&mut self, content: &Content, instance: u32) {
        if self.combat().unwrap().block_doubler != Some(instance) {
            return;
        }
        self.combat_mut().unwrap().block_doubler = None;
        if self.has_relic(content, "RELIC.VAMBRACE") {
            self.combat_mut().unwrap().vambrace = true;
        }
        if self.combat().unwrap().paels_legion == 0 && self.has_relic(content, "RELIC.PAELS_LEGION")
        {
            self.combat_mut().unwrap().paels_legion = 2;
        }
    }

    pub(super) fn finish_play(&mut self, content: &Content) {
        let combat = self.combat().unwrap();
        if combat.choice.is_none()
            && combat.queue.is_empty()
            && combat.playing.is_some()
            && !combat.pending_plays.is_empty()
        {
            let mut card = combat.playing.unwrap();
            self.spend_block_doublers(content, card.instance);
            // Maul grows as each play resolves (OnPlay), so its next play hits harder.
            self.grow_maul(&mut card);
            // Vigorous wears off after each play (AfterCardPlayed), so replays don't get it;
            // Goopy grows with each play.
            match card.enchantment {
                Some(Enchantment::Vigorous) => card.enchantment_value = 0,
                Some(Enchantment::Goopy | Enchantment::Momentum) => {
                    self.spend_enchantment(&mut card)
                }
                _ => {}
            }
            self.combat_mut().unwrap().playing = Some(card);
            // Each play of a series spends a Free Attack, Skill or Power (BeforeCardPlayed), e.g.
            // the One-Two Punch replay of Unrelenting spends the one it just granted.
            let free = match card.card_type(content.cards[card.id as usize]) {
                CardType::Attack => Some(power_id::FREE_ATTACK),
                CardType::Skill => Some(power_id::FREE_SKILL),
                CardType::Power => Some(power_id::FREE_POWER),
                _ => None,
            };
            if let Some(free) = free {
                self.creature_mut(Actor::Player).consume_power(free);
            }
            // Veilpiercer is spent by each play of an Ethereal card (BeforeCardPlayed).
            let combat = self.combat_mut().unwrap();
            if is_ethereal(combat, card, content.cards[card.id as usize]) {
                combat.player.consume_power(power_id::VEILPIERCER);
            }
            // Unsettling Lamp finishes after the first play of its card (AfterCardPlayed).
            if self.combat().unwrap().unsettling_lamp == Some(card.id) {
                let combat = self.combat_mut().unwrap();
                combat.unsettling_lamp = None;
                combat.unsettling_used = true;
            }
        }
        let combat = self.combat().unwrap();
        if combat.choice.is_none()
            && combat.queue.is_empty()
            && let Some(card) = combat.playing
            && !combat.pending_plays.is_empty()
        {
            let finished = combat.card_plays as usize - combat.pending_plays.len();
            self.juggle(
                content,
                self.sharpened(content, card, finished),
                finished,
                false,
            );
            // Rupture's Strength from this play's own HP loss lands as it finishes (Glam replays).
            self.release_rupture(content);
            // Music Box copies its attack once that play finishes (AfterCardPlayed), before the
            // series' next play.
            if self.combat().unwrap().music_box_card == Some(card.instance) {
                let mut copy = card;
                copy.instance = 0;
                copy.flags |= ETHEREAL;
                // Razor Tooth has upgraded the card by then if it comes first in relic order.
                if self.razor_tooth(content, card)
                    && self.relic_first(content, "RELIC.RAZOR_TOOTH", "RELIC.MUSIC_BOX")
                {
                    copy.upgrades = 1;
                }
                let combat = self.combat_mut().unwrap();
                combat.music_box = true;
                combat.music_box_card = None;
                self.add_generated(content, Pile::Hand, copy);
                // Its generation hooks (Arsenal) resolve before the next play.
                self.resolve(content);
            }
            let card_type = card.card_type(content.cards[card.id as usize]);
            let pen_nib = card_type == CardType::Attack && self.has_relic(content, "RELIC.PEN_NIB");
            if pen_nib {
                self.pen_nib = (self.pen_nib + 1) % 10;
            }
            let doubled = pen_nib && self.pen_nib == 0;
            let combat = self.combat_mut().unwrap();
            let mut context = combat.pending_plays.remove(0);
            context.pen_nib = doubled;
            let fan_shiv = content.cards[card.id as usize].id == "CARD.SHIV"
                && combat.player.power(power_id::FAN_OF_KNIVES) > 0;
            // Adroit and Corrupted act on every play, after the card's own effects.
            let amount = card.enchantment_amount;
            match card.enchantment {
                Some(Enchantment::Adroit) => combat.queue.push(Pending {
                    effect: Effect::Block(Target::Player, Amount::fixed(amount, amount)),
                    context,
                }),
                Some(Enchantment::Corrupted) => combat.queue.push(Pending {
                    effect: Effect::LoseHp(Target::Player, Amount::fixed(2, 2)),
                    context,
                }),
                _ => {}
            }
            push_play(
                &mut combat.queue,
                card,
                content.cards[card.id as usize],
                fan_shiv,
                context,
            );
            // Each play of a series snapshots its listeners at its own start (BeforeCardPlayed),
            // so a power applied by an earlier play (Storm) sees the later ones.
            let powers =
                std::mem::replace(&mut combat.power_snapshot, combat.player.powers.clone());
            let enemies = std::mem::replace(
                &mut combat.enemy_power_snapshot,
                combat
                    .enemies
                    .iter()
                    .map(|enemy| enemy.creature.powers.clone())
                    .collect(),
            );
            let skills = self.finished_plays(content, CardType::Skill) + finished as i16;
            self.queue_return_late(card_type, (skills - 1, skills), 0);
            // Each finished play of a Shiv counts (Phantom Blades), not just the last.
            if content.cards[card.id as usize].tags & SHIV_TAG != 0 {
                self.combat_mut().unwrap().history.shivs += 1;
            }
            // Once the fight is won the combat ends during a card's remaining plays, before
            // their AfterCardPlayed hooks (Calamity).
            if !self.enemies_defeated() {
                self.queue_card_triggers(content, card, powers, enemies, 1);
                self.rainbow_ring(content, card_type, finished as i16 + 1);
            }
            // Danse Macabre reads the card's cost as it resolves for this play (Flatten made free
            // by the first play's Osty attack); an X cost keeps the energy spent.
            let energy = self.combat().unwrap().card_energy;
            let def = content.cards[card.id as usize];
            let cost = if def.cost[card.upgrades.min(1) as usize] < 0 {
                energy
            } else {
                let spiked = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
                energy_cost(self.combat().unwrap(), card, def, spiked, false)
            };
            self.queue_before_play_blocks(content, card, energy, cost, context);
            self.queue_sealed_throne(context);
            // Each replay is its own play: Stomp's BeforeCardPlayed discount applies again.
            if card_type == CardType::Attack {
                self.combat_mut().unwrap().discount_stomps(content);
            }
            self.tick_panache();
            self.resolve(content);
            return self.finish_play(content);
        }
        let (mut card, card_energy) = {
            let combat = self.combat_mut().unwrap();
            if combat.choice.is_some() || !combat.queue.is_empty() || combat.playing.is_none() {
                return;
            }
            (combat.playing.take().unwrap(), combat.card_energy)
        };
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        self.spend_enchantment(&mut card);
        let feral = self.combat_mut().unwrap().take_feral(&card);
        self.grow_maul(&mut card);
        // Juggling (a power) clones the card before Razor Tooth (a relic) upgrades it.
        let unsharpened = card;
        if self.razor_tooth(content, card) {
            card.upgrades = 1;
        }
        let card = self.finish_card(content, card);
        // The hand is checked once the card is placed and its exhaust hooks resolved.
        self.combat_mut().unwrap().queue.push(Pending {
            effect: Effect::CheckEmptyHand,
            context: Context::player(),
        });
        self.place_played(content, card, false, feral);
        let skills = self.finished_plays(content, CardType::Skill);
        self.queue_return_late(card_type, (skills - 1, skills), card_energy);
        if matches!(card_type, CardType::Attack | CardType::Skill) && card.flags & DUPE == 0 {
            self.combat_mut().unwrap().history_course = Some(card);
        }
        if self.combat().unwrap().unsettling_lamp == Some(card.id) {
            self.combat_mut().unwrap().unsettling_lamp = None;
            self.combat_mut().unwrap().unsettling_used = true;
        }
        if self.combat().unwrap().music_box_card == Some(card.instance) {
            let mut copy = card;
            // Music Box copies the card before Razor Tooth upgrades it unless Razor Tooth comes first.
            if !self.relic_first(content, "RELIC.RAZOR_TOOTH", "RELIC.MUSIC_BOX") {
                copy.upgrades = unsharpened.upgrades;
            }
            copy.instance = 0;
            copy.flags |= ETHEREAL;
            let combat = self.combat_mut().unwrap();
            combat.music_box = true;
            combat.music_box_card = None;
            // A relic hook: it follows the player's powers' hooks (Calamity).
            combat.music_box_copy = Some(copy);
        }
        if card_type == CardType::Power
            && self.has_relic(content, "RELIC.PERMAFROST")
            && !self.combat().unwrap().permafrost
        {
            self.combat_mut().unwrap().permafrost = true;
            self.gain_block(content, Actor::Player, 7, None, false);
        }
        self.spend_block_doublers(content, card.instance);
        let plays = self.combat().unwrap().card_plays as usize;
        self.juggle(
            content,
            self.sharpened(content, unsharpened, plays),
            0,
            true,
        );
        self.rainbow_ring(content, CardType::Attack, 0);
        if card_energy >= 3 && self.has_relic(content, "RELIC.IVORY_TILE") {
            self.gain_energy(self.combat().unwrap().card_plays as i16);
        }
        self.release_rupture(content);
        let powers = std::mem::take(&mut self.combat_mut().unwrap().power_snapshot);
        let enemies = std::mem::take(&mut self.combat_mut().unwrap().enemy_power_snapshot);
        // Earlier plays of a series were counted as they finished.
        let context = self.queue_card_triggers(content, card, powers, enemies, 1);
        let damage = self.combat().unwrap().player.power(power_id::BLACK_HOLE);
        if self.combat().unwrap().card_stars > 0 && damage > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(damage, damage)),
                context,
            });
        }
        self.tick_panache();
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
            source: Actor::Player,
            target,
            card: None,
            upgraded: false,
            x: 0,
            event: 0,
            orb: false,
            orb_id: None,
            pen_nib: false,
            lethal: false,
            hook: false,
            off_pile: false,
            target_instance: None,
        };
        // The game checks for an empty hand (Unceasing Top) only once the potion is done.
        let combat = self.combat_mut().unwrap();
        combat.potions += 1;
        combat.queue.push(Pending {
            effect: Effect::CheckEmptyHand,
            context,
        });
        // Belt Buckle's and Reptile Trinket's AfterPotionUsed resolve after the potion itself, in
        // relic order (queue is a stack, so the later relic is pushed first).
        let buckle = self.has_relic(content, "RELIC.BELT_BUCKLE")
            && self.run.potions.iter().all(Option::is_none)
            && !self.combat().unwrap().belt_buckle;
        let trinkets = self.relic_count(content, "RELIC.REPTILE_TRINKET");
        let buckle_first = self.relic_first(content, "RELIC.BELT_BUCKLE", "RELIC.REPTILE_TRINKET");
        let combat = self.combat_mut().unwrap();
        let push_buckle = |combat: &mut Combat| {
            combat.belt_buckle = true;
            combat.queue.push(Pending {
                effect: Effect::ApplyPower(
                    Target::Player,
                    power_id::DEXTERITY,
                    Amount::fixed(2, 2),
                ),
                context,
            });
        };
        if buckle && !buckle_first {
            push_buckle(combat);
        }
        for _ in 0..trinkets {
            for power in [power_id::REPTILE_TRINKET, power_id::STRENGTH] {
                combat.queue.push(Pending {
                    effect: Effect::ApplyPower(Target::Player, power, Amount::fixed(3, 3)),
                    context,
                });
            }
        }
        if buckle && buckle_first {
            push_buckle(combat);
        }
        push_effects(
            &mut self.combat_mut().unwrap().queue,
            content.potions[id as usize].effects,
            context,
        );
    }

    pub(super) fn choose(&mut self, content: &Content, index: usize) {
        let choice = self.combat().unwrap().choice.unwrap();
        let knowledge = (choice.pile == Pile::Offer
            && self.combat().unwrap().enemy_turn
            && matches!(
                self.combat().unwrap().offer[index].id,
                card_id::DISINTEGRATION | card_id::MIND_ROT | card_id::SLOTH | card_id::WASTE_AWAY
            ))
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
            }
            let combat = self.combat_mut().unwrap();
            combat.offer.clear();
            combat.choice = None;
            let enemies = (0..self.combat().unwrap().enemies.len())
                .map(Actor::Enemy)
                .collect();
            self.doom_kill(content, enemies);
            if self.creature(Actor::Enemy(enemy)).hp > 0 {
                self.trigger(content, Trigger::TurnEnd, Actor::Enemy(enemy), 0);
            }
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
        if choice.op == CardOp::DiscardDraw && self.combat().unwrap().choice.is_none() {
            self.finish_discard_draw(content);
        }
    }

    /// CardCmd.DiscardAndDraw: each discard's hooks run first, then as many cards are drawn,
    /// then the Sly ones auto-play from the discard pile; all before anything queued earlier.
    pub(super) fn finish_discard_draw(&mut self, content: &Content) {
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
        queue.insert(
            start + sly.len(),
            Pending {
                effect: Effect::Draw(cards.len().min(u8::MAX as usize) as u8),
                context: Context::player(),
            },
        );
    }

    pub(super) fn start_turn(&mut self, content: &Content) {
        self.roll_moves(content);
        self.start_player_turn(content);
    }

    /// An extra turn (Pael's Eye) keeps the enemies' intents.
    /// Toasty Mittens exhausts the draw pile's top card (turn 1: the top non-Innate one), then
    /// gives 1 Strength.
    /// Queued (behind a pick), it leaves its exhaust hooks and Strength on the queue rather than
    /// resolving the rest of the turn setup first.
    fn toasty_mittens(&mut self, content: &Content, queued: bool) {
        if queued {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::ApplyPower(Target::Player, power_id::STRENGTH, Amount::fixed(1, 1)),
                context: Context::player(),
            });
        }
        if self.combat().unwrap().draw.is_empty() && !self.combat().unwrap().discard.is_empty() {
            self.shuffle_discard_into_draw(content);
            self.after_shuffle(content);
            if !queued {
                self.resolve(content);
            }
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
            self.combat_mut().unwrap().history.exhausted += 1;
            self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
            self.trigger_card(content, card, Trigger::CardExhausted);
            if !queued {
                self.resolve(content);
            }
        }
        if !queued {
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
        }
    }

    pub(super) fn start_player_turn(&mut self, content: &Content) {
        self.combat_mut().unwrap().setup = true;
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
        let sparkling_rouge = self.has_relic(content, "RELIC.SPARKLING_ROUGE");
        let bread = self.has_relic(content, "RELIC.BREAD");
        let art_of_war = self.relic_count(content, "RELIC.ART_OF_WAR");
        let captains_wheel = self.relic_count(content, "RELIC.CAPTAINS_WHEEL");
        let horn_cleat = self.relic_count(content, "RELIC.HORN_CLEAT");
        let ice_cream = self.has_relic(content, "RELIC.ICE_CREAM");
        let sturdy_clamp = self.has_relic(content, "RELIC.STURDY_CLAMP");
        let paels_flesh = self.relic_count(content, "RELIC.PAELS_FLESH");
        let tea_set = self.tea_set as i16;
        self.tea_set = 0;
        let seal_of_gold = self.has_relic(content, "RELIC.SEAL_OF_GOLD") && self.run.gold >= 5;
        let combat = self.combat_mut().unwrap();
        let mut generated = Vec::new();
        combat.turn += 1;
        combat.demon_tongue = false;
        combat.mini_regent = false;
        combat.music_box = false;
        combat.music_box_card = None;
        combat.rainbow_ring = false;
        combat.attacks_mod3 = 0;
        combat.bound_this_turn = 0;
        let prior_attacks = combat.history.attacks;
        let prior_hits = combat.history.hp_lost + combat.history.unblocked_hits;
        if combat.turn == 3 {
            combat.max_energy += paels_flesh;
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
                card.free_stars = false;
                card.turn_flags = 0;
                if card.until_played {
                    card.cost_override = None;
                    card.until_played = false;
                }
                if card.id == card_id::FLATTEN || content.cards[card.id as usize].id == "CARD.STOMP"
                {
                    card.value = 0;
                }
            }
        }
        let nightmare = power_id::NIGHTMARE;
        // Nightmare, Hello World and Infinite Blades add cards BeforeHandDraw, in power order.
        let position = |id: &str| {
            combat
                .player
                .powers
                .iter()
                .position(|power| content.powers[power.id as usize].id == id)
        };
        let after_hello_world = ["POWER.HELLO_WORLD_POWER", "POWER.INFINITE_BLADES_POWER"]
            .into_iter()
            .any(|id| {
                position(id)
                    .zip(position("POWER.NIGHTMARE_POWER"))
                    .is_some_and(|(earlier, nightmare)| earlier < nightmare)
            });
        // Aggression fetches BeforeSideTurnStart, ahead of every BeforeHandDraw card.
        let aggression = combat.player.power(121) > 0;
        combat.player.powers.retain(|power| power.id != nightmare);
        let nightmares = std::mem::take(&mut combat.nightmares);
        let (nightmares, mid_nightmares, late_nightmares) = if after_hello_world {
            (Vec::new(), Vec::new(), nightmares)
        } else if aggression {
            (Vec::new(), nightmares, Vec::new())
        } else {
            (nightmares, Vec::new(), Vec::new())
        };
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
            + art_of_war * (combat.turn > 1 && prior_attacks == 0) as i16
            + seal_of_gold as i16
            // Bread's +1 max energy starts on turn 2; its turn-1 loss comes in AfterSideTurnStart.
            - (bread && combat.turn == 1) as i16;
        combat.last_cards = combat.history.cards;
        combat.hits.fill(0);
        // HP-loss events, ethereal plays (Pull From Below) and generated cards (Supermassive)
        // count for the whole combat.
        let (hp_loss_events, ethereal, generated_cards) = (
            combat.history.hp_loss_events,
            combat.history.ethereal,
            combat.history.generated,
        );
        combat.history = History::default();
        combat.history.hp_loss_events = hp_loss_events;
        combat.history.ethereal = ethereal;
        combat.history.generated = generated_cards;
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
        // AfterBlockCleared fires every turn start, even when block is retained.
        let mut toric_blocks = Vec::new();
        {
            for power in &mut combat.player.powers {
                if power.id == power_id::TORIC_TOUGHNESS {
                    toric_blocks.push(power.value);
                    power.amount -= 1;
                }
                if power.id == power_id::SELF_FORMING_CLAY {
                    toric_blocks.push(power.amount);
                    power.amount = 0;
                }
            }
            combat
                .player
                .powers
                .retain(|power| power.id != power_id::SELF_FORMING_CLAY);
            combat
                .player
                .powers
                .retain(|power| power.id != power_id::TORIC_TOUGHNESS || power.amount != 0);
            if combat.turn == 3 {
                toric_blocks.extend((0..captains_wheel).map(|_| 18));
            }
            if combat.turn == 2 {
                toric_blocks.extend((0..horn_cleat).map(|_| 14));
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
        // Nightmare's copies have the player as creator, so even a Wither fires Regalite.
        for id in generated {
            self.trigger_created(content, id);
        }
        for block in toric_blocks {
            self.gain_block(content, Actor::Player, block, None, false);
        }
        self.poison(content, Actor::Player);
        // Aggression fetches its attacks BeforeSideTurnStart, ahead of the BeforeHandDraw cards
        // (Infinite Blades).
        if self.creature(Actor::Player).power(121) > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Aggression(Amount::scaled(Scale::Power(121), 1)),
                context: Context::player(),
            });
            self.resolve(content);
        }
        for (card, count) in mid_nightmares {
            for _ in 0..count {
                let card = self.add_to_pile(
                    content,
                    Pile::Hand,
                    Card {
                        instance: 0,
                        free: false,
                        turn_flags: 0,
                        ..card
                    },
                );
                self.trigger_created(content, card.id);
            }
        }
        // Pollinous Core counts turns BeforeSideTurnStart, even one the fight ends in.
        if self.has_relic(content, "RELIC.POLLINOUS_CORE") {
            self.pollinous_core += 1;
        }
        // Spinner, Lightning Rod and Genesis act after the energy reset (AfterEnergyReset), in
        // power order, before turn-start hooks (Loop). The queue resolves its last entry first.
        let powers = &self.creature(Actor::Player).powers;
        let position = |id: &str| {
            powers
                .iter()
                .position(|power| content.powers[power.id as usize].id == id)
        };
        let spinner_first = position("POWER.SPINNER_POWER") < position("POWER.LIGHTNING_ROD_POWER");
        if spinner_first {
            self.trigger(content, Trigger::EnergyReset, Actor::Player, 0);
        }
        for _ in 0..self.creature(Actor::Player).power(power_id::SPINNER) {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Channel(4, 1),
                context: Context::player(),
            });
        }
        if !spinner_first {
            self.trigger(content, Trigger::EnergyReset, Actor::Player, 0);
        }
        self.resolve(content);
        // Genesis' stars can win the fight through Black Hole before any other turn-start hook.
        if self.enemies_defeated() {
            return;
        }
        self.trigger(content, Trigger::TurnStart, Actor::Player, 0);
        self.resolve(content);
        // Power Cell (BeforeSideTurnStart, so before the innate reorder and hand draw) moves two
        // 0-cost draw-pile cards to the hand: a StableShuffle of the pile listed from its top.
        let first_turn = self.combat().unwrap().turn == 1;
        for _ in 0..if first_turn {
            self.relic_count(content, "RELIC.POWER_CELL")
        } else {
            0
        } {
            let mut picks: Vec<_> = self
                .combat()
                .unwrap()
                .draw
                .iter()
                .rev()
                .filter(|card| {
                    eligible(
                        content,
                        card,
                        CardFilter::PlayableCost(0),
                        CardOp::Move(Pile::Hand),
                    )
                })
                .copied()
                .collect();
            sort_for_shuffle(&mut picks, content);
            if self.expectation.is_none() {
                self.rngs.combat_card_selection.shuffle(&mut picks);
            }
            for card in picks.into_iter().take(2) {
                let draw = &self.combat().unwrap().draw;
                let index = draw
                    .iter()
                    .position(|other| other.instance == card.instance)
                    .unwrap();
                let card = self.combat_mut().unwrap().remove_draw(index);
                self.add_to_hand(card);
            }
        }
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
        let mut relic_draw = 0;
        if turn == 1 {
            relic_draw -= 2 * self.relic_count(content, "RELIC.BIG_MUSHROOM");
            relic_draw += 2 * self.relic_count(content, "RELIC.BAG_OF_PREPARATION");
            relic_draw += 2 * self.relic_count(content, "RELIC.RING_OF_THE_SNAKE");
            relic_draw += 2
                * self.relic_count(content, "RELIC.BOOMING_CONCH")
                * (self.room == Room::Elite) as i16;
        }
        if turn <= 3 {
            relic_draw += 2 * self.relic_count(content, "RELIC.RING_OF_THE_DRAKE");
        }
        relic_draw += 2 * self.relic_count(content, "RELIC.FIDDLE");
        relic_draw += self.relic_count(content, "RELIC.PAELS_BLOOD");
        relic_draw += 2 * self.relic_count(content, "RELIC.SNECKO_EYE");
        if turn > 1 && self.combat().unwrap().last_cards <= 3 {
            relic_draw += 3 * self.relic_count(content, "RELIC.POCKETWATCH");
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
            // Machine Learning raises the hand draw (ModifyHandDraw).
            + self
                .creature(Actor::Player)
                .powers
                .iter()
                .filter(|power| content.powers[power.id as usize].id == "POWER.MACHINE_LEARNING_POWER")
                .map(|power| power.amount)
                .sum::<i16>()
            - self.creature(Actor::Player).power(power_id::MIND_ROT))
        .clamp(0, u8::MAX as i16) as u8;
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::DRAW_CARDS_NEXT_TURN);
        // BeforeHandDraw relics that touch the draw pile run in relic order, before the turn-1
        // reorder: Toasty Mittens exhausts a Soul that an earlier Funerary Mask put on top.
        let relics: Vec<_> = (self.run.relics.iter().enumerate())
            .filter(|(index, _)| !self.melted_relics.contains(index))
            .map(|(_, &id)| content.relics[id as usize].id)
            .collect();
        let mut mittens = 0;
        for relic in relics {
            let added = match relic {
                "RELIC.BLESSED_ANTLER" if turn == 1 => content.card_id("CARD.DAZED"),
                "RELIC.FUNERARY_MASK" if turn == 1 => content.card_id("CARD.SOUL"),
                _ => None,
            };
            if let Some(id) = added {
                for _ in 0..3 {
                    self.add_random(
                        content,
                        Pile::Draw,
                        Card {
                            id,
                            ..Card::default()
                        },
                    );
                }
            }
            if relic != "RELIC.TOASTY_MITTENS" {
                continue;
            }
            // A power's BeforeHandDraw pick (Foregone Conclusion) comes first.
            if self.combat().unwrap().choice.is_some() {
                mittens += 1;
            } else {
                self.toasty_mittens(content, false);
            }
        }
        let hello_world = self
            .creature(Actor::Player)
            .powers
            .iter()
            .find(|power| content.powers[power.id as usize].id == "POWER.HELLO_WORLD_POWER")
            .map_or(0, |power| power.amount.max(0) as usize);
        if hello_world > 0 {
            self.expectation_unknown();
            let mut pool: Vec<_> = content.characters[self.run.character as usize]
                .cards
                .iter()
                .copied()
                .filter(|&id| {
                    let card = content.cards[id as usize];
                    card.rarity == CardRarity::Common && card.flags[0] & NO_GENERATE == 0
                })
                .collect();
            if self.expectation.is_none() {
                self.rngs.combat_card_generation.shuffle(&mut pool);
            }
            for id in pool.into_iter().take(hello_world) {
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
        // Call of the Void generates after an earlier Hello World (both BeforeHandDraw, in power
        // order; the TurnStart dispatch skips it then).
        if self.hello_world_before_void(content) {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::RandomCharacter(
                    Pile::Hand,
                    Amount::scaled(Scale::Power(97), 1),
                    ETHEREAL,
                ),
                context: Context::player(),
            });
            self.resolve(content);
        }
        for (card, count) in late_nightmares {
            for _ in 0..count {
                let card = self.add_to_pile(
                    content,
                    Pile::Hand,
                    Card {
                        instance: 0,
                        free: false,
                        turn_flags: 0,
                        ..card
                    },
                );
                self.trigger_created(content, card.id);
            }
        }
        // Cards' BeforeHandDraw hooks (Bolas, Thrumming Hatchet) run after powers' (Hello World)
        // and relics'.
        self.return_bolas(content);
        let mut turn_effects = vec![Effect::ToastyMittens; mittens];
        for id in self.run.relics.clone() {
            match content.relics[id as usize].id {
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
                // After Toolbox's pick when Toolbox comes first (both BeforeHandDraw).
                "RELIC.NINJA_SCROLL" if turn == 1 => {
                    if let Some(shiv) = content.card_id("CARD.SHIV") {
                        turn_effects.push(Effect::AddCard(Pile::Hand, shiv, 3));
                    }
                }
                _ => {}
            }
        }
        turn_effects.push(Effect::HandDraw(count));
        // After the hand draw: AfterPlayerTurnStart (powers, then relics in relic order), its Late
        // pass, AfterSideTurnStart, then the AutoPrePlay phase.
        let mut boulders = false;
        for power in &self.creature(Actor::Player).powers {
            let amount = power.amount.clamp(0, u8::MAX as i16) as u8;
            match power.id {
                // One tick rolls every boulder, however many power entries track them.
                power_id::ROLLING_BOULDER if !std::mem::replace(&mut boulders, true) => {
                    turn_effects.push(Effect::BoulderTick)
                }
                power_id::TOOLS_OF_THE_TRADE => turn_effects.push(Effect::Discard(amount, false)),
                // Their HP loss comes after the hand draw, so Rupture's Strength misses the cards
                // Hellraiser auto-played meanwhile.
                power_id::CRIMSON_MANTLE => turn_effects.extend([
                    Effect::LoseHp(
                        Target::Player,
                        Amount::scaled(Scale::Power(power_id::CRIMSON_MANTLE_DAMAGE), 1),
                    ),
                    Effect::RawBlock(
                        Target::Player,
                        Amount::scaled(Scale::Power(power_id::CRIMSON_MANTLE), 1),
                    ),
                ]),
                power_id::INFERNO => turn_effects.push(Effect::LoseHp(
                    Target::Player,
                    Amount::scaled(Scale::Power(power_id::INFERNO_DAMAGE), 1),
                )),
                power_id::TYRANNY => turn_effects.push(Effect::Exhaust(amount, false)),
                // Loop's passives come here, ahead of Biased Cognition's Focus loss.
                _ if content.powers[power.id as usize].id == "POWER.LOOP_POWER" => turn_effects
                    .extend(std::iter::repeat_n(
                        Effect::PassiveFirst(1),
                        amount as usize,
                    )),
                // The cards are picked from the hand (AutoSlay's untraced selector in sampled runs).
                power_id::ENTROPY => turn_effects.push(Effect::SelectAmount(
                    Pile::Hand,
                    CardFilter::Any,
                    Amount::fixed(power.amount, power.amount),
                    false,
                    CardOp::TransformRandom,
                )),
                _ => {}
            }
        }
        let relics: Vec<_> = (self.run.relics.iter().enumerate())
            .filter(|(index, _)| !self.melted_relics.contains(index))
            .map(|(_, &id)| content.relics[id as usize].id)
            .collect();
        for &relic in &relics {
            match relic {
                "RELIC.MR_STRUGGLES" => turn_effects.push(Effect::Damage(
                    Target::AllEnemies,
                    Amount::fixed(turn as i16, turn as i16),
                )),
                // Festive Popper hits on turn 1's AfterPlayerTurnStart, after Fur Coat's HP set.
                "RELIC.FESTIVE_POPPER" if turn == 1 => {
                    turn_effects.push(Effect::Damage(Target::AllEnemies, Amount::fixed(9, 9)))
                }
                "RELIC.MERCURY_HOURGLASS" => {
                    turn_effects.push(Effect::Damage(Target::AllEnemies, Amount::fixed(3, 3)))
                }
                "RELIC.BELLOWS" if turn == 1 => {
                    turn_effects.push(Effect::Upgrade(Pile::Hand, u8::MAX, false))
                }
                "RELIC.EMOTION_CHIP" if prior_hits > 0 => turn_effects.push(Effect::PassiveAll),
                "RELIC.PENDULUM" => {
                    self.pendulum = (self.pendulum + 1) % 3;
                    if self.pendulum == 0 {
                        turn_effects.push(Effect::Draw(1));
                    }
                }
                "RELIC.ROYAL_POISON" if turn == 1 => {
                    turn_effects.push(Effect::LoseHp(Target::Player, Amount::fixed(4, 4)))
                }
                "RELIC.VEXING_PUZZLEBOX" if turn == 1 => {
                    turn_effects.push(Effect::DistinctCharacter(Pile::Hand, 1, true))
                }
                "RELIC.CHOICES_PARADOX" if turn == 1 => {
                    turn_effects.push(Effect::OfferCharacterRetain(5))
                }
                // Its discard-and-redraw runs in relic order: Vexing Puzzlebox's card comes after
                // it when the Puzzlebox is held later.
                "RELIC.GAMBLING_CHIP" if turn == 1 => turn_effects.push(Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [u8::MAX, u8::MAX],
                    false,
                    true,
                    CardOp::DiscardDraw,
                )),
                _ => {}
            }
        }
        for &relic in &relics {
            let heal = match relic {
                "RELIC.BLOOD_VIAL" => 2,
                "RELIC.FAKE_BLOOD_VIAL" => 1,
                _ => 0,
            };
            if turn == 1 && heal > 0 {
                turn_effects.push(Effect::Heal(Target::Player, Amount::fixed(heal, heal)));
            }
        }
        // Furnace, Prep Time and Demon Form act after the hand draw (AfterSideTurnStart), so
        // Hellraiser's drawn Strikes don't get their Vigor or Strength.
        let mut side = Vec::new();
        for power in &self.creature(Actor::Player).powers {
            let amount = Amount::fixed(power.amount, power.amount);
            match power.id {
                power_id::FURNACE if power.amount > 0 => side.push(Effect::Forge(amount)),
                power_id::NOXIOUS_FUMES if power.amount > 0 => side.push(Effect::ApplyPower(
                    Target::AllEnemies,
                    power_id::POISON,
                    amount,
                )),
                power_id::PREP_TIME if power.amount > 0 => {
                    side.push(Effect::ApplyPower(Target::Player, power_id::VIGOR, amount))
                }
                power_id::COUNTDOWN if power.amount > 0 => side.push(Effect::ApplyPower(
                    Target::RandomEnemy,
                    power_id::DOOM,
                    amount,
                )),
                power_id::DEMON_FORM if power.amount > 0 => side.push(Effect::ApplyPower(
                    Target::Player,
                    power_id::STRENGTH,
                    amount,
                )),
                _ if content.powers[power.id as usize].id == "POWER.BIASED_COGNITION_POWER" => side
                    .push(Effect::ApplyPower(
                        Target::Player,
                        power_id::FOCUS,
                        Amount::fixed(-power.amount, -power.amount),
                    )),
                _ if content.powers[power.id as usize].id == "POWER.WRAITH_FORM_POWER" => side
                    .push(Effect::ApplyPower(
                        Target::Player,
                        power_id::DEXTERITY,
                        Amount::fixed(-power.amount, -power.amount),
                    )),
                // Shadow Step turns into Double Damage only now, so Sly cards that Tools of the
                // Trade discarded aren't doubled.
                _ if content.powers[power.id as usize].id == "POWER.SHADOW_STEP_POWER" => {
                    side.push(Effect::ApplyPower(
                        Target::Player,
                        power_id::DOUBLE_DAMAGE,
                        amount,
                    ));
                    side.push(Effect::RemovePower(Target::Player, power.id));
                }
                _ => {}
            }
        }
        for &relic in &relics {
            match relic {
                // With Gambling Chip (AfterPlayerTurnStart) it upgrades the redrawn hand instead.
                "RELIC.BONE_TEA"
                    if turn == 1 && !self.bone_tea && !relics.contains(&"RELIC.GAMBLING_CHIP") =>
                {
                    self.bone_tea = true;
                    side.push(Effect::Upgrade(Pile::Hand, u8::MAX, false));
                }
                "RELIC.CROSSBOW" => {
                    side.push(Effect::RandomCard(Pile::Hand, CardType::Attack, 1, true))
                }
                "RELIC.SAI" => side.push(Effect::RawBlock(Target::Player, Amount::fixed(7, 7))),
                // Brimstone's Strength comes after the hand draw, so Hellraiser's Strikes miss it.
                "RELIC.BRIMSTONE" => {
                    side.push(Effect::ApplyPower(
                        Target::Player,
                        power_id::STRENGTH,
                        Amount::fixed(2, 2),
                    ));
                    side.push(Effect::ApplyPower(
                        Target::AllEnemies,
                        power_id::STRENGTH,
                        Amount::fixed(1, 1),
                    ));
                }
                // With Gambling Chip (AfterPlayerTurnStart) its Vigor comes after the redraw.
                "RELIC.AKABEKO" if turn == 1 && !relics.contains(&"RELIC.GAMBLING_CHIP") => side
                    .push(Effect::ApplyPower(
                        Target::Player,
                        power_id::VIGOR,
                        Amount::fixed(8, 8),
                    )),
                // Its gold goes in AfterSideTurnStart: a fight won before that phase costs none,
                // while the phase's hooks all run even if one wins it (Noxious Fumes), so it's
                // charged first.
                "RELIC.SEAL_OF_GOLD" if self.run.gold >= 5 => {
                    side.insert(0, Effect::Gold(Amount::fixed(-5, -5)))
                }
                "RELIC.ORANGE_DOUGH" if turn == 1 => {
                    side.push(Effect::DistinctColorless(Pile::Hand, 2, false))
                }
                "RELIC.FENCING_MANUAL" if turn == 1 => {
                    side.push(Effect::Forge(Amount::fixed(10, 10)))
                }
                // Energy relics act after the hand draw (AfterSideTurnStart).
                "RELIC.LANTERN" if turn == 1 => side.push(Effect::Energy(1)),
                "RELIC.VERY_HOT_COCOA" if turn == 1 => side.push(Effect::Energy(4)),
                "RELIC.CANDELABRA" if turn == 2 => side.push(Effect::Energy(2)),
                "RELIC.CHANDELIER" if turn == 3 => side.push(Effect::Energy(3)),
                "RELIC.BOOMING_CONCH" if turn == 1 && self.room == Room::Elite => {
                    side.push(Effect::Energy(1))
                }
                "RELIC.BREAD" if turn == 1 => side.push(Effect::Energy(-2)),
                // After cards that Tools of the Trade's discard auto-played, which still count
                // toward the previous turn's Letter Opener skills.
                "RELIC.PAELS_LEGION" | "RELIC.LETTER_OPENER"
                    if !side.contains(&Effect::SideTurnStart) =>
                {
                    side.push(Effect::SideTurnStart)
                }
                _ => {}
            }
        }
        let mayhem = self.creature(Actor::Player).power(power_id::MAYHEM);
        for effect in turn_effects.into_iter().rev() {
            self.combat_mut().unwrap().queue.push(Pending {
                effect,
                context: Context::player(),
            });
        }
        self.resolve(content);
        // AfterSideTurnStart runs once the setup completes (card choices never pause it in sampled
        // runs, since AutoSlay's selector answers them), unless the setup won the fight (Crossbow
        // doesn't roll then). Rampart (an enemy power) shields the Turret Operators last, except on
        // extra turns; the setup ends with it.
        if !self.enemies_defeated() {
            side.push(Effect::Rampart);
            // Behind a pending choice they wait at the bottom of the queue (resolved last).
            let pending = self.combat().unwrap().choice.is_some();
            let queue = &mut self.combat_mut().unwrap().queue;
            let at = if pending { 0 } else { queue.len() };
            queue.splice(
                at..at,
                side.into_iter().rev().map(|effect| Pending {
                    effect,
                    context: Context::player(),
                }),
            );
            self.resolve(content);
        }
        // Big Hat's Ethereal cards arrive after the hand draw (AfterSideTurnStart).
        for _ in 0..(turn == 1) as i16 * self.relic_count(content, "RELIC.BIG_HAT") {
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
        // Turn-start orb passives (Plasma) follow AfterSideTurnStart (OrbQueue.AfterTurnStart),
        // ahead of the AutoPrePlay phase.
        self.combat_mut().unwrap().queue.insert(
            0,
            Pending {
                effect: Effect::TurnStartOrbs,
                context: Context::player(),
            },
        );
        // Bombardment plays itself in the AutoPrePlay phase, after the turn setup, including
        // setup effects still queued behind a pending choice (Tyranny may exhaust another).
        self.combat_mut().unwrap().queue.insert(
            0,
            Pending {
                effect: Effect::Bombardments,
                context: Context::player(),
            },
        );
        // Mayhem, then History Course, auto-play after Bombardment (AfterAutoPrePlayPhaseEntered
        // follows its Early pass).
        if mayhem > 0 {
            self.combat_mut().unwrap().queue.insert(
                0,
                Pending {
                    effect: Effect::AutoPlayDraw(Amount::fixed(mayhem, mayhem), false),
                    context: Context::player(),
                },
            );
        }
        if let Some(mut card) = history_course {
            // It dupes the card as it is now (CreateDupe): a Dampen applied since has downgraded it.
            let combat = self.combat().unwrap();
            if let Some(current) = combat
                .draw
                .iter()
                .chain(&combat.hand)
                .chain(&combat.discard)
                .chain(&combat.exhaust)
                .find(|other| other.instance == card.instance)
            {
                card.upgrades = current.upgrades;
            }
            card.instance = 0;
            // It clones the card as it is when played: last turn's discounts are gone (a Stomp
            // gets this turn's in begin_auto_play).
            card.free = false;
            card.flags |= REPLAY | DUPE;
            self.combat_mut().unwrap().queue.insert(
                0,
                Pending {
                    effect: Effect::AutoPlay(card),
                    context: Context::player(),
                },
            );
        }
        self.resolve(content);
    }

    fn hello_world_before_void(&self, content: &Content) -> bool {
        let powers = &self.creature(Actor::Player).powers;
        let position = |id: &str| {
            powers
                .iter()
                .position(|power| content.powers[power.id as usize].id == id)
        };
        position("POWER.HELLO_WORLD_POWER") < position("POWER.CALL_OF_THE_VOID_POWER")
            && position("POWER.HELLO_WORLD_POWER").is_some()
    }

    /// Turn 1's AutoPrePlay phase, after every combat-start effect: Imbued cards play
    /// themselves, then Whispering Earring plays the hand (AfterAutoPrePlayPhaseEnteredLate).
    pub(super) fn auto_pre_play(&mut self, content: &Content) {
        let mut effects = Vec::new();
        // Hook listeners visit hand, draw (from the top), discard, then exhaust (an Imbued card
        // Toasty Mittens exhausted still plays). Each leaves its pile only when its play starts.
        for pile in [Pile::Hand, Pile::Draw, Pile::Discard, Pile::Exhaust] {
            let mut imbued: Vec<_> = cards(self.combat().unwrap(), pile)
                .iter()
                .filter(|card| card.enchantment == Some(Enchantment::Imbued))
                .map(|&card| Effect::AutoPlay(card))
                .collect();
            if pile == Pile::Draw {
                imbued.reverse();
            }
            effects.extend(imbued);
        }
        if self.has_relic(content, "RELIC.WHISPERING_EARRING") {
            effects.push(Effect::WhisperingEarring(0));
        }
        for effect in effects.into_iter().rev() {
            self.combat_mut().unwrap().queue.push(Pending {
                effect,
                context: Context::player(),
            });
        }
        self.resolve(content);
    }

    /// Each boulder hits the enemies one at a time, left to right, so a death in between
    /// resolves before the next hit (Crab Rage's block).
    pub(super) fn tick_boulders(&mut self, content: &Content) {
        let mut damage = Vec::new();
        for boulder in &mut self.combat_mut().unwrap().boulders {
            damage.push(*boulder);
            *boulder = boulder.saturating_add(5);
        }
        self.creature_mut(Actor::Player)
            .add_power(power_id::ROLLING_BOULDER, 5 * damage.len() as i16);
        let enemies = self.targets(content, Target::AllEnemies, Context::player());
        for amount in damage.into_iter().rev() {
            for &enemy in enemies.iter().rev() {
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
    }

    pub(super) fn end_turn(&mut self, content: &Content) {
        while self
            .combat()
            .unwrap()
            .draw
            .last()
            .is_some_and(|card| card.id == card_id::I_AM_INVINCIBLE)
        {
            self.auto_play_top();
            self.resolve(content);
        }
        // BeforeSideTurnEnd, ahead of the orb passives.
        self.combat_mut().unwrap().paels_tears =
            self.combat().unwrap().energy > 0 && self.has_relic(content, "RELIC.PAELS_TEARS");
        // Stampede auto-plays when the play phase ends (AfterAutoPostPlayPhaseEntered), before
        // the BeforeTurnEnd hooks.
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
        // Howl from Beyond plays itself from the exhaust pile when the play phase ends.
        for card in howls.into_iter().rev() {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::AutoPlay(card),
                context: Context {
                    off_pile: true,
                    ..Context::card(card)
                },
            });
        }
        self.resolve(content);
        // BeforeSideTurnEndVeryEarly: Orichalcum notes whether the player has no block.
        let orichalcum = self.creature(Actor::Player).block == 0;
        // BeforeSideTurnEndEarly: Plating, then Pael's Eye.
        let plating = self.creature(Actor::Player).power(power_id::PLATING);
        if plating > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::RawBlock(Target::Player, Amount::fixed(plating, plating)),
                context: Context::player(),
            });
            self.resolve(content);
        }
        if self.has_relic(content, "RELIC.PAELS_EYE")
            && !self.combat().unwrap().paels_eye
            && self.combat().unwrap().history.manual_cards == 0
            && !(self.combat().unwrap().turn == 1
                && self.has_relic(content, "RELIC.WHISPERING_EARRING"))
        {
            let cards = std::mem::take(&mut self.combat_mut().unwrap().hand);
            let combat = self.combat_mut().unwrap();
            combat.paels_eye = true;
            combat
                .control
                .push(ControlContinuation::StartExtraPlayerTurnAfterResolution);
            self.combat_mut().unwrap().history.exhausted += cards.len() as i16;
            for card in cards {
                self.combat_mut().unwrap().exhaust.push(card);
                self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
                self.trigger_card(content, card, Trigger::CardExhausted);
            }
            self.resolve(content);
        }
        // BeforeSideTurnEnd: the player's powers (Doom, The Bomb, Hailstorm), then the relics in
        // relic order.
        self.doom_kill(content, vec![Actor::Player, Actor::Osty]);
        self.resolve(content);
        if self.creature(Actor::Player).hp <= 0 {
            return;
        }
        self.tick_bombs(content);
        self.resolve(content);
        // Hailstorm hits before the orbs' turn-end passives.
        let hailstorm = self.creature(Actor::Player).power(power_id::HAILSTORM);
        if hailstorm > 0
            && !self.enemies_defeated()
            && self
                .combat()
                .unwrap()
                .orbs
                .iter()
                .any(|orb| orb.id == orb_id::FROST)
        {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::AllEnemies, Amount::fixed(hailstorm, hailstorm)),
                context: Context::player(),
            });
            self.resolve(content);
        }
        for i in 0..self.run.relics.len() {
            if self.melted_relics.contains(&i) {
                continue;
            }
            let effect = match content.relics[self.run.relics[i] as usize].id {
                "RELIC.ORICHALCUM" | "RELIC.FAKE_ORICHALCUM" if orichalcum => {
                    let block =
                        if content.relics[self.run.relics[i] as usize].id == "RELIC.ORICHALCUM" {
                            6
                        } else {
                            3
                        };
                    self.gain_block(content, Actor::Player, block, None, false);
                    None
                }
                "RELIC.SCREAMING_FLAGON" if self.combat().unwrap().hand.is_empty() => {
                    Some(Effect::Damage(Target::AllEnemies, Amount::fixed(20, 20)))
                }
                "RELIC.DIAMOND_DIADEM" if self.combat().unwrap().history.cards <= 2 => {
                    self.apply_power(content, Actor::Player, power_id::DIAMOND_DIADEM, 1);
                    None
                }
                "RELIC.RIPPLE_BASIN" if self.combat().unwrap().history.attacks == 0 => {
                    self.gain_block(content, Actor::Player, 4, None, false);
                    None
                }
                "RELIC.STONE_CALENDAR" if self.combat().unwrap().turn == 7 => {
                    Some(Effect::Damage(Target::AllEnemies, Amount::fixed(52, 52)))
                }
                "RELIC.CLOAK_CLASP" => {
                    let hand = self.combat().unwrap().hand.len() as i16;
                    Some(Effect::RawBlock(Target::Player, Amount::fixed(hand, hand)))
                }
                _ => None,
            };
            if let Some(effect) = effect {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect,
                    context: Context::player(),
                });
                self.resolve(content);
            }
        }
        self.resolve(content);
        if self.enemies_defeated() {
            return;
        }
        self.passive_orbs(content, OrbTiming::TurnEnd);
        self.resolve(content);
        if self.enemies_defeated() {
            return;
        }
        // Temporary Strength wears off at AfterSideTurnEnd, after the hand is flushed (below).
        let temporary: Vec<(usize, Power)> = {
            let powers = &mut self.creature_mut(Actor::Player).powers;
            let mut taken = Vec::new();
            let mut index = 0;
            while index < powers.len() {
                if TEMPORARY_STRENGTH.contains(&powers[index].id) {
                    taken.push((index + taken.len(), powers.remove(index)));
                } else {
                    index += 1;
                }
            }
            taken
        };
        self.trigger(content, Trigger::TurnEnd, Actor::Player, 0);
        for (index, power) in temporary {
            let powers = &mut self.creature_mut(Actor::Player).powers;
            powers.insert(index.min(powers.len()), power);
        }
        self.resolve(content);
        for panache in &mut self.combat_mut().unwrap().panache {
            panache.0 = 5;
        }
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| !matches!(power.id, power_id::DUPLICATION | power_id::REBOUND));
        self.combat_mut().unwrap().rebounds.clear();
        self.combat_mut().unwrap().ferals.clear();
        // DoTurnEnd: Ethereal cards leave the hand first (their exhaust hooks run later), then each
        // turn-end card moves to the play pile for its effect and on to the discard pile, so the
        // effects see the smaller hand (Regret, Centennial Puzzle's draws).
        let hexed = self.creature(Actor::Player).power(power_id::HEX) > 0;
        let turn_end = |card: &Card| {
            content.cards[card.id as usize]
                .hooks
                .iter()
                .any(|hook| hook.trigger == Trigger::TurnEnd)
        };
        let ethereal =
            |card: &Card| card.flags(content.cards[card.id as usize]) & ETHEREAL != 0 || hexed;
        let combat = self.combat_mut().unwrap();
        // Regret counts the hand at BeforeSideTurnEnd, before any card leaves it.
        let hand_size = combat.hand.len() as i16;
        let turn_end_cards: Vec<_> = combat
            .hand
            .iter()
            .copied()
            .filter(|card| turn_end(card))
            .collect();
        let (ethereal_cards, hand): (Vec<_>, Vec<_>) = std::mem::take(&mut combat.hand)
            .into_iter()
            .partition(|card| ethereal(card) && !turn_end(card));
        combat.hand = hand;
        combat.ethereal_ending = Some(ethereal_cards);
        for card in turn_end_cards {
            let combat = self.combat_mut().unwrap();
            let Some(index) = combat.hand.iter().position(|other| *other == card) else {
                continue;
            };
            combat.hand.remove(index);
            if content.card_id("CARD.REGRET") == Some(card.id) {
                combat.queue.push(Pending {
                    effect: Effect::LoseHp(Target::Player, Amount::fixed(hand_size, hand_size)),
                    context: Context::card(card),
                });
            } else {
                self.trigger_card(content, card, Trigger::TurnEnd);
            }
            self.resolve(content);
            let combat = self.combat_mut().unwrap();
            if ethereal(&card) {
                combat.ethereal_ending.get_or_insert_default().push(card);
            } else {
                combat.discard.push(card);
            }
        }
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::CORROSIVE_WAVE);
        // Regen heals in AfterSideTurnEnd, after cards' end-of-turn effects in hand (Bad Luck).
        let regen = self.creature(Actor::Player).power(power_id::REGEN);
        if regen > 0 && self.creature(Actor::Player).hp > 0 {
            for effect in [
                Effect::ApplyPower(Target::Player, power_id::REGEN, Amount::fixed(-1, -1)),
                Effect::Heal(Target::Player, Amount::fixed(regen, regen)),
            ] {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect,
                    context: Context::player(),
                });
            }
            self.resolve(content);
        }
        // Lunar Pastry gains its star in AfterSideTurnEnd, after the orbs; it counts as gaining
        // stars (Black Hole).
        for _ in 0..self.relic_count(content, "RELIC.LUNAR_PASTRY") {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Stars(Amount::fixed(1, 1)),
                context: Context::player(),
            });
        }
        self.resolve(content);
        if self.enemies_defeated() {
            return;
        }
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
        for power in self
            .creature_mut(Actor::Player)
            .powers
            .iter_mut()
            .filter(|power| power.id == power_id::THE_BOMB)
        {
            power.amount -= 1;
        }
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::THE_BOMB || power.amount > 0);
        // Each explosion is one damage command: every enemy is hit before deaths resolve.
        for amount in explosions {
            let previous = self.combat_mut().unwrap().dying.replace(vec![]);
            for target in self.targets(content, Target::AllEnemies, Context::player()) {
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
        let combat = self.combat_mut().unwrap();
        combat.bound_played = false;
        for card in combat
            .hand
            .iter_mut()
            .chain(&mut combat.draw)
            .chain(&mut combat.discard)
            .chain(&mut combat.exhaust)
        {
            // Bound, Ringing, Smog and Entangled end with the player's turn.
            if matches!(
                card.affliction,
                Affliction::Bound | Affliction::Ringing | Affliction::Smog | Affliction::Entangled
            ) {
                card.affliction = Affliction::None;
            }
        }
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::RINGING);
        self.reset_tender();
        let (count, exhausted) = {
            let combat = self.combat_mut().unwrap();
            let ethereal_ending = combat.ethereal_ending.take();
            let mut hand = Vec::new();
            let mut exhausted = ethereal_ending.clone().unwrap_or_default();
            let mut discarded = Vec::new();
            let hexed = combat.player.power(power_id::HEX) > 0;
            let turn_end = |card: &Card| {
                content.cards[card.id as usize]
                    .hooks
                    .iter()
                    .any(|hook| hook.trigger == Trigger::TurnEnd)
            };
            for card in std::mem::take(&mut combat.hand) {
                let def = content.cards[card.id as usize];
                let flags = card.flags(def);
                if (flags & ETHEREAL != 0 || hexed) && ethereal_ending.is_none() {
                    exhausted.push(card);
                } else if flags & RETAIN != 0 && !turn_end(&card) {
                    hand.push(card);
                } else {
                    discarded.push(card);
                }
            }
            combat.discard.extend(discarded);
            // Turn-end cards (Doubt) exhaust after their effect, so after the other ethereal cards.
            exhausted.sort_by_key(turn_end);
            combat.exhaust.extend(&exhausted);
            combat.hand = hand;
            combat.history.exhausted += exhausted.len() as i16;
            (combat.enemies.len(), exhausted)
        };
        // Dark Embrace defers its draws for these Ethereal exhausts to AfterSideTurnEnd.
        let dark_embrace = self
            .creature(Actor::Player)
            .powers
            .iter()
            .position(|power| power.id == power_id::DARK_EMBRACE);
        let dark_embrace = dark_embrace
            .map(|index| (index, self.creature_mut(Actor::Player).powers.remove(index)));
        let ethereal = exhausted.len() as i16;
        self.joss_ethereal = Some(0);
        self.turn_end_exhaust = true;
        for card in exhausted {
            self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
            self.trigger_card(content, card, Trigger::CardExhausted);
        }
        let joss_ethereal = self.joss_ethereal.take().unwrap_or(0);
        if let Some((index, power)) = dark_embrace {
            self.creature_mut(Actor::Player).powers.insert(index, power);
        }
        // Exhaust triggers (Forgotten Soul) resolve before AfterSideTurnEnd: Consuming Shadow,
        // then Parrying Shield.
        self.resolve(content);
        self.turn_end_exhaust = false;
        // Power order: Consuming Shadow evokes, Temporary Focus and Strength wear off. Dark
        // Embrace's draw (below) comes before the temporary Strength listed after it.
        let mut after_side = Vec::new();
        let mut late_strength = Vec::new();
        for (index, power) in self.creature(Actor::Player).powers.iter().enumerate() {
            let amount = Amount::fixed(power.amount, power.amount);
            if TEMPORARY_STRENGTH.contains(&power.id) {
                let effects = [
                    Effect::RemovePower(Target::Player, power.id),
                    Effect::ApplyPower(
                        Target::Player,
                        power_id::STRENGTH,
                        Amount::fixed(-power.amount, -power.amount),
                    ),
                ];
                if ethereal > 0 && dark_embrace.is_some_and(|(at, _)| at < index) {
                    late_strength.extend(effects);
                } else {
                    after_side.extend(effects);
                }
                continue;
            }
            match power.id {
                power_id::CONSUMING_SHADOW if !self.enemies_defeated() => {
                    after_side.push(Effect::EvokeLast(amount))
                }
                power_id::CONSUMING_SHADOW => {}
                id if AFTER_SIDE_TURN_END.contains(&id) => after_side.extend([
                    Effect::StackPower(
                        Target::Player,
                        power_id::FOCUS,
                        Amount::fixed(-power.amount, -power.amount),
                    ),
                    Effect::RemovePower(Target::Player, id),
                ]),
                _ => {}
            }
        }
        for effect in after_side.into_iter().rev() {
            self.combat_mut().unwrap().queue.push(Pending {
                effect,
                context: Context::player(),
            });
        }
        self.resolve(content);
        if self.has_relic(content, "RELIC.PARRYING_SHIELD")
            && self.creature(Actor::Player).block >= 10
        {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Damage(Target::RandomEnemy, Amount::fixed(6, 6)),
                context: Context::player(),
            });
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
                        && energy_cost(self.combat().unwrap(), **card, def, spiked, true) > 0
                })
                .map(|(index, _)| index)
                .collect();
            if !choices.is_empty() {
                let choice =
                    choices[self.rngs.combat_card_selection.below(choices.len() as u32) as usize];
                let card = self.combat().unwrap().hand[choice];
                let def = content.cards[card.id as usize];
                let cost = energy_cost(self.combat().unwrap(), card, def, spiked, true) - 1;
                // Bookmark's discount lasts until played (AddUntilPlayed).
                let card = &mut self.combat_mut().unwrap().hand[choice];
                card.cost_override = Some(cost as i8);
                card.when_played = true;
            }
        }
        // AfterSideTurnEnd in power order: a No Draw applied after Dark Embrace still blocks it.
        let no_draw_later = dark_embrace.is_some()
            && self
                .creature(Actor::Player)
                .powers
                .iter()
                .skip_while(|power| power.id != power_id::DARK_EMBRACE)
                .skip(1)
                .any(|power| {
                    power.amount > 0 && content.powers[power.id as usize].kind == PowerKind::NoDraw
                });
        self.creature_mut(Actor::Player).powers.retain(|power| {
            power.amount <= 0 || content.powers[power.id as usize].kind != PowerKind::NoDraw
        });
        if let Some((_, power)) = dark_embrace
            && ethereal > 0
            && !no_draw_later
        {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::DrawAmount(Amount::fixed(
                    power.amount * ethereal,
                    power.amount * ethereal,
                )),
                context: Context::player(),
            });
            self.resolve(content);
        }
        for effect in late_strength.into_iter().rev() {
            self.combat_mut().unwrap().queue.push(Pending {
                effect,
                context: Context::player(),
            });
        }
        self.resolve(content);
        // Joss Paper counts the Ethereal exhausts at AfterSideTurnEnd, once No Draw is gone.
        if joss_ethereal > 0 {
            let total = self.joss_paper + joss_ethereal;
            self.joss_paper = total % 5;
            if total >= 5 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::DrawAmount(Amount::fixed(
                        (total / 5) as i16,
                        (total / 5) as i16,
                    )),
                    context: Context::player(),
                });
                self.resolve(content);
            }
        }
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
        if let Some(power) = self
            .combat_mut()
            .unwrap()
            .player
            .powers
            .iter_mut()
            .find(|power| power.id == power_id::HELLRAISER)
        {
            power.value = 0;
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
        }
        if self.combat().unwrap().control.last()
            == Some(&ControlContinuation::StartExtraPlayerTurnAfterResolution)
        {
            self.combat_mut().unwrap().control.pop();
            self.combat_mut().unwrap().extra_turn = true;
            self.start_player_turn(content);
            self.combat_mut().unwrap().extra_turn = false;
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
        participants.sort_by_key(|&index| self.slot_order(content, index));
        // Every enemy's block clears when the enemy side's turn starts, before any of them acts.
        for &index in &participants {
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
        }
        for &index in &participants {
            self.poison(content, Actor::Enemy(index));
            self.trigger(content, Trigger::TurnStart, Actor::Enemy(index), 0);
            self.resolve(content);
        }
        let mut ended = Vec::new();
        // A creature spawned this turn into a dead participant's slot waits for its next turn.
        let instances: Vec<_> = participants
            .iter()
            .map(|&index| self.combat().unwrap().enemies[index].instance)
            .collect();
        for (index, instance) in participants.into_iter().zip(instances) {
            if self.creature(Actor::Player).hp <= 0 {
                break;
            }
            let enemy = &self.combat().unwrap().enemies[index];
            if enemy.instance != instance {
                continue;
            }
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
                lethal: false,
                hook: false,
                off_pile: false,
                target_instance: None,
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
                && move_index == 7
            {
                let creature = &mut self.combat_mut().unwrap().enemies[index].creature;
                let steam = creature.power(power_id::STEAM_ERUPTION);
                creature
                    .powers
                    .retain(|power| power.id != power_id::STEAM_ERUPTION);
                self.combat_mut().unwrap().enemies[index].value = steam;
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
                // Three eggs, each into the last free of the five egg slots.
                for _ in 0..3.min(5usize.saturating_sub(eggs)) {
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
                    self.spawn_bot(content, index, 2);
                } else {
                    self.spawn_bot(content, index, 0);
                    self.spawn_bot(content, index, 2);
                }
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
                // A rat called for backup picks its first move with the next intents.
                self.combat_mut().unwrap().enemies[rat].value = 1;
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
            if !stunned && content.enemies[id as usize].id == "MONSTER.AXEBOT" && move_index == 0 {
                let stock = self.creature(Actor::Enemy(index)).power(power_id::STOCK);
                let strength = if self.run.ascension >= 9 { 4 } else { 3 } * (2 - stock);
                self.apply_power(content, Actor::Enemy(index), power_id::STRENGTH, strength);
            }
            if !stunned
                && content.enemies[id as usize].id == "MONSTER.GREMLIN_MERC"
                && self.creature(Actor::Player).hp > 0
            {
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
            if !stunned
                && content.enemies[id as usize].id == "MONSTER.GAS_BOMB"
                && self.creature(Actor::Enemy(index)).hp > 0
            {
                self.kill_actor(content, Actor::Enemy(index));
            }
            ended.push(index);
            if !stunned
                && content.enemies[id as usize].id == "MONSTER.THIEVING_HOPPER"
                && move_index == 4
            {
                self.creature_mut(Actor::Enemy(index)).hp = 0;
            }
            if !stunned
                && content.enemies[id as usize].id == "MONSTER.FAT_GREMLIN"
                && move_index == 1
            {
                self.creature_mut(Actor::Enemy(index)).hp = 0;
            }
        }
        // Doom kills before the enemies' turn-end effects (BeforeSideTurnEnd); the doomed skip them.
        let alive: Vec<_> = (0..self.combat().unwrap().enemies.len())
            .map(|index| self.creature(Actor::Enemy(index)).hp > 0)
            .collect();
        let enemies = (0..alive.len()).map(Actor::Enemy).collect();
        self.doom_kill(content, enemies);
        self.resolve(content);
        ended.retain(|&index| !alive[index] || self.creature(Actor::Enemy(index)).hp > 0);
        // Enemy turn-end effects resolve once every enemy has acted.
        for index in ended {
            let id = self.combat().unwrap().enemies[index].creature.id;
            if content.enemies[id as usize].id == "MONSTER.LAGAVULIN_MATRIARCH"
                && self.creature(Actor::Enemy(index)).power(power_id::ASLEEP) == 1
            {
                self.creature_mut(Actor::Enemy(index))
                    .powers
                    .retain(|power| power.id != 79);
            }
            // Nemesis precedes Demise among the Test Subject's powers, so its Intangible toggle
            // already applies to Demise's damage (the toggle itself completes below).
            let creature = self.creature_mut(Actor::Enemy(index));
            if let Some(nemesis) = creature
                .powers
                .iter()
                .find(|power| power.id == power_id::NEMESIS)
            {
                if nemesis.value == 1 {
                    creature
                        .powers
                        .retain(|power| power.id != power_id::INTANGIBLE);
                } else if creature.power(power_id::INTANGIBLE) == 0 {
                    creature.add_power(power_id::INTANGIBLE, 1);
                }
            }
            // Slumber and Demise both tick in AfterSideTurnEnd, in power order: Slumber ticking first
            // lets Demise's HP loss end it with a stun instead of the immediate wake-up.
            let powers = &self.creature(Actor::Enemy(index)).powers;
            let position = |id| powers.iter().position(|power| power.id == id);
            let slumber_first = content.enemies[id as usize].id == "MONSTER.SLUMBERING_BEETLE"
                && position(power_id::SLUMBER).is_some_and(|slumber| {
                    position(power_id::DEMISE).is_some_and(|demise| slumber < demise)
                });
            // Plating's block (BeforeSideTurnEndEarly) still lands before a wake-up drops it.
            let woke = slumber_first && {
                let creature = self.creature_mut(Actor::Enemy(index));
                creature.consume_power(power_id::SLUMBER);
                creature.power(power_id::SLUMBER) == 0
            };
            if woke {
                let enemy = &mut self.combat_mut().unwrap().enemies[index];
                enemy.move_index = 2;
                enemy.last_move = usize::MAX;
                enemy.repeats = 0;
            }
            self.trigger(content, Trigger::TurnEnd, Actor::Enemy(index), 0);
            self.resolve(content);
            if woke {
                self.creature_mut(Actor::Enemy(index))
                    .powers
                    .retain(|power| power.id != 79);
            }
            if content.enemies[id as usize].id == "MONSTER.LAGAVULIN_MATRIARCH" {
                self.creature_mut(Actor::Enemy(index))
                    .consume_power(power_id::ASLEEP);
            }
            if content.enemies[id as usize].id == "MONSTER.SLUMBERING_BEETLE" && !slumber_first {
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
        }
        self.finish_enemy_turn(content, count);
    }

    fn finish_enemy_turn(&mut self, content: &Content, count: usize) {
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::TAINTED);
        self.decay(content, Actor::Player);
        // Diamond Diadem's power ends at the enemy side's turn end.
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::DIAMOND_DIADEM);
        // No Block decrements at the enemy side's turn end.
        self.creature_mut(Actor::Player)
            .consume_power(power_id::NO_BLOCK);
        self.decay(content, Actor::Osty);
        for index in 0..count {
            self.decay(content, Actor::Enemy(index));
        }
        for enemy in &mut self.combat_mut().unwrap().enemies {
            enemy
                .creature
                .powers
                .retain(|power| power.id != power_id::STRANGLE);
            // Nemesis alternates: Intangible one round, none the next.
            if enemy.creature.hp > 0
                && let Some(nemesis) = enemy
                    .creature
                    .powers
                    .iter_mut()
                    .find(|power| power.id == power_id::NEMESIS)
            {
                nemesis.value ^= 1;
                if nemesis.value == 1 {
                    let intangible = enemy.creature.power(power_id::INTANGIBLE);
                    enemy
                        .creature
                        .add_power(power_id::INTANGIBLE, 1 - intangible);
                } else {
                    enemy
                        .creature
                        .powers
                        .retain(|power| power.id != power_id::INTANGIBLE);
                }
            }
            enemy.creature.consume_power(power_id::HATCH);
        }
        self.combat_mut().unwrap().enemy_turn = false;
        self.resolve(content);
        self.creature_mut(Actor::Player)
            .consume_power(power_id::COLOSSUS);
        self.creature_mut(Actor::Player)
            .powers
            .retain(|power| power.id != power_id::FLAME_BARRIER);
        // Minions (Parafright) don't keep the fight going once the rest are dead.
        if self.combat().unwrap().player.hp > 0 && !self.enemies_defeated() {
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
                // After the spawn stun, wriggler1/3 bite and wriggler2/4 wriggle (by slot name,
                // kept in `value`); older states fall back to list parity.
                let biter = if enemy.value > 0 {
                    enemy.value % 2 == 1
                } else {
                    slot % 2 == 0
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
        // CardPileCmd.Draw stops once the combat is ending (e.g. Speedster killed the last enemy).
        if count == 0 || self.combat().unwrap().hand.len() >= 10 || self.enemies_defeated() {
            return None;
        }
        let shuffled = self.combat().unwrap().draw.is_empty();
        if shuffled {
            if self.combat().unwrap().discard.is_empty() {
                return None;
            }
            self.shuffle_discard_into_draw(content);
            self.after_shuffle(content);
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
            self.expectation_unknown();
            card.cost_override = Some(if self.expectation.is_some() {
                0
            } else {
                self.rngs.combat_energy_costs.below(4) as i8
            });
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
            if let Some(cost) = self.confused_cost()
                && !x_cost
            {
                confuse(&mut card, cost);
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
            match &mut card.cost_override {
                Some(cost) => *cost = cost.saturating_sub(1),
                None => card.cost_delta = card.cost_delta.saturating_sub(1),
            }
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
        if pagestorm > 0 && is_ethereal(combat, card, content.cards[card.id as usize]) {
            combat.queue.push(Pending {
                effect: Effect::Draw(pagestorm),
                context,
            });
        }
        let speedster = !hand_draw && !self.combat().unwrap().enemy_turn;
        let mut powers = self.creature(Actor::Player).powers.clone();
        powers.retain(|power| speedster || power.id != power_id::SPEEDSTER);
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
                lethal: false,
                hook: false,
                off_pile: false,
                target_instance: None,
            },
            powers,
        );
        self.trigger_card(content, card, Trigger::CardDrawn);
        self.tick_automation();
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

    /// Confused's cost roll; None while the expectation search hides it.
    fn confused_cost(&mut self) -> Option<i8> {
        self.expectation_unknown();
        Some(if self.expectation.is_some() {
            0
        } else {
            self.rngs.combat_energy_costs.below(4) as i8
        })
    }

    /// PlayerCmd.GainEnergy: No Energy Gain blocks every energy gain.
    fn gain_energy(&mut self, amount: i16) {
        if amount > 0 && self.creature(Actor::Player).power(power_id::NO_ENERGY_GAIN) == 0 {
            let combat = self.combat_mut().unwrap();
            combat.energy = combat.energy.saturating_add(amount);
        }
    }

    fn tick_automation(&mut self) {
        let mut energy = 0;
        for automation in &mut self.combat_mut().unwrap().automation {
            automation.0 -= 1;
            if automation.0 == 0 {
                automation.0 = 10;
                energy += automation.1;
            }
        }
        self.gain_energy(energy);
    }

    pub(crate) fn resolve(&mut self, content: &Content) {
        while self.combat().is_some_and(|x| x.choice.is_none()) {
            let Some(pending) = self.combat_mut().and_then(|x| x.queue.pop()) else {
                // Turn 1's AutoPrePlay phase waits for every combat-start effect and choice.
                if std::mem::take(&mut self.combat_mut().unwrap().auto_pre_play) {
                    self.auto_pre_play(content);
                }
                break;
            };
            self.apply(content, pending);
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
                let attacker = |game: &Self| match source {
                    Actor::Enemy(index) => Some(game.combat().unwrap().enemies[index].instance),
                    _ => None,
                };
                let attacking = attacker(self);
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
                    let previous = area.then(|| self.combat_mut().unwrap().dying.replace(vec![]));
                    for actor in self.targets(content, target, pending.context) {
                        let instance = |game: &Self| match actor {
                            Actor::Enemy(index) => {
                                Some(game.combat().unwrap().enemies[index].instance)
                            }
                            _ => None,
                        };
                        let targeted = instance(self);
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
                            if instance(self) != targeted {
                                break;
                            }
                            let result = self.damage_with_bonus(
                                content,
                                actor,
                                amount,
                                DamageKind::Attack,
                                pending.context,
                            );
                            // Reaper Form applies Doom per hit (AfterDamageGiven).
                            self.reaper(content, source, actor, result.lost + result.blocked);
                            total += if overkill {
                                result.dealt
                            } else {
                                result.blocked + result.lost
                            };
                            suck_hits += (result.lost > 0) as i16;
                            // A respawn (Axebot's Stock) refills the slot, but the target was still killed.
                            // The Waterfall Giant's death leaves it at i16::MAX HP, still a kill.
                            killed |= result.lost > 0
                                && (instance(self) != targeted
                                    || self.creature(actor).hp <= 0
                                    || self.creature(actor).hp == i16::MAX);
                            if self.creature(source).hp <= 0 || attacker(self) != attacking {
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
            Effect::OstyAttack(target, _, hits) => {
                if self.creature(Actor::Osty).hp <= 0 {
                    return;
                }
                self.combat_mut().unwrap().history.osty_attacks += 1;
                self.mark_flattens();
                // An area hit strikes every target before deaths resolve (e.g. Crab Rage).
                let previous = (target == Target::AllEnemies)
                    .then(|| self.combat_mut().unwrap().dying.replace(vec![]));
                for actor in self.targets(content, target, pending.context) {
                    for _ in 0..hits {
                        let result = self.damage_with_bonus(
                            content,
                            actor,
                            amount,
                            DamageKind::Attack,
                            Context {
                                source: Actor::Osty,
                                ..pending.context
                            },
                        );
                        self.reaper(content, Actor::Osty, actor, result.lost + result.blocked);
                        if self.creature(Actor::Osty).hp <= 0 {
                            break;
                        }
                    }
                    self.sic_em(actor);
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
            Effect::OstyAttackMany(target, _, hits) => {
                if self.creature(Actor::Osty).hp <= 0 {
                    return;
                }
                let hits = self.amount(content, pending.context, hits).max(0);
                self.combat_mut().unwrap().history.osty_attacks += 1;
                self.mark_flattens();
                // An area hit strikes every target before deaths resolve (e.g. Crab Rage).
                let previous = (target == Target::AllEnemies)
                    .then(|| self.combat_mut().unwrap().dying.replace(vec![]));
                for actor in self.targets(content, target, pending.context) {
                    for _ in 0..hits {
                        let result = self.damage_with_bonus(
                            content,
                            actor,
                            amount,
                            DamageKind::Attack,
                            Context {
                                source: Actor::Osty,
                                ..pending.context
                            },
                        );
                        self.reaper(content, Actor::Osty, actor, result.lost + result.blocked);
                        if self.creature(Actor::Osty).hp <= 0 {
                            break;
                        }
                    }
                    self.sic_em(actor);
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
                for actor in self.targets(content, target, pending.context) {
                    // A death can refill the target's slot (Surprise's gremlins), so the splash
                    // skips the target by instance.
                    let instance = |game: &Self, actor| match actor {
                        Actor::Enemy(index) => Some(game.combat().unwrap().enemies[index].instance),
                        _ => None,
                    };
                    let hit = instance(self, actor);
                    let result = self.damage_with_bonus(
                        content,
                        actor,
                        amount,
                        DamageKind::Move,
                        pending.context,
                    );
                    self.reaper(
                        content,
                        pending.context.source,
                        actor,
                        result.lost + result.blocked,
                    );
                    let dealt = result.dealt;
                    // A refilled slot (Surprise's gremlins) means the hit creature died.
                    kills += (instance(self, actor) != hit || self.creature(actor).hp <= 0) as i16;
                    if card == Some("CARD.OMNISLICE") {
                        for other in self.targets(content, Target::AllEnemies, pending.context) {
                            if instance(self, other) != hit {
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
                let targets = self.targets(content, target, pending.context);
                let previous = self.combat_mut().unwrap().dying.replace(vec![]);
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
                for actor in self.targets(content, target, pending.context) {
                    self.kill_actor(content, actor);
                }
            }
            Effect::Stun(target) => {
                for actor in self.targets(content, target, pending.context) {
                    if let Actor::Enemy(index) = actor {
                        self.combat_mut().unwrap().enemies[index].stunned = true;
                    }
                }
            }
            Effect::DoomKill => {
                let enemies = self.targets(content, Target::AllEnemies, pending.context);
                self.doom_kill(content, enemies);
            }
            Effect::LoseHp(target, _) => {
                for actor in self.targets(content, target, pending.context) {
                    let before = self.creature(actor).hp.max(0);
                    // ModifyHpLostAfterOsty runs powers (Intangible) before relics (Tungsten Rod,
                    // Beating Remnant); Buffer comes last.
                    let mut lost = amount.max(0);
                    if self.creature(actor).kind(content, PowerKind::Intangible) > 0 {
                        lost = lost.min(1);
                    }
                    // Direct HP loss is damage in the game, so Hard To Kill's cap applies (Demise).
                    let cap = self.creature(actor).power(power_id::HARD_TO_KILL);
                    if cap > 0 {
                        lost = lost.min(cap);
                    }
                    // Hardened Shell caps every HP loss it sees, Strangle and Poison included.
                    if let Some(shell) = self
                        .creature(actor)
                        .powers
                        .iter()
                        .find(|power| power.id == power_id::HARDENED_SHELL)
                    {
                        lost = lost.min((shell.amount - shell.value).max(0));
                    }
                    let slippery = lost > 0 && self.creature(actor).power(power_id::SLIPPERY) > 0;
                    if slippery {
                        lost = 1;
                    }
                    if actor == Actor::Player {
                        lost = self.modify_player_hp_lost(content, lost);
                    }
                    let lost = self.buffer(content, actor, lost).min(before);
                    self.creature_mut(actor).hp = before - lost;
                    if let Some(shell) = self
                        .creature_mut(actor)
                        .powers
                        .iter_mut()
                        .find(|power| power.id == power_id::HARDENED_SHELL)
                    {
                        shell.value = shell.value.saturating_add(lost);
                    }
                    if slippery && lost > 0 {
                        self.creature_mut(actor).consume_power(power_id::SLIPPERY);
                    }
                    // Direct HP loss is damage in the game, so it can break Plow (Demise).
                    if lost > 0 {
                        self.wake_on_hp_loss(content, actor);
                        self.break_plow(actor);
                    }
                    if actor == Actor::Player {
                        self.after_player_hp_loss(content, lost);
                        self.sync_red_skull(content);
                        // Unblockable damage is never fully blocked, even when no HP is lost (Emotion Chip).
                        if amount > 0 {
                            self.combat_mut().unwrap().history.unblocked_hits += 1;
                        }
                        self.combat_mut().unwrap().history.hp_lost += lost;
                        if self.creature(Actor::Player).hp > 0 {
                            self.combat_mut().unwrap().history.remnant_lost += lost;
                        }
                        if lost > 0 {
                            self.combat_mut().unwrap().history.hp_loss_events += 1;
                        }
                    }
                    if lost > 0 {
                        // Rupture waits until the card that cost the HP finishes.
                        let combat = self.combat_mut().unwrap();
                        let playing = combat.playing.is_some() || !combat.auto_plays.is_empty();
                        let rupture =
                            (actor == Actor::Player && pending.context.card.is_some() && playing)
                                .then(|| {
                                    combat
                                        .player
                                        .powers
                                        .iter()
                                        .position(|power| power.id == power_id::RUPTURE)
                                })
                                .flatten()
                                .map(|index| (index, combat.player.powers.remove(index)));
                        self.trigger(content, Trigger::HpLost, actor, lost);
                        if let Some((index, mut power)) = rupture {
                            power.value += power.amount;
                            let powers = &mut self.combat_mut().unwrap().player.powers;
                            powers.insert(index.min(powers.len()), power);
                        }
                        if actor == Actor::Player {
                            self.inferno();
                        }
                        if actor == Actor::Osty {
                            self.necro_mastery(content, lost);
                        }
                    }
                    // Only a creature that was alive can die (Strangle on an already dead enemy).
                    if before > 0 && self.creature(actor).hp <= 0 {
                        self.died(content, actor);
                    }
                }
            }
            Effect::Block(target, _) => {
                let targets = self.targets(content, target, pending.context);
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
                } else if pending.context.card.is_some()
                    && self.creature(Actor::Player).power(power_id::NO_BLOCK) > 0
                {
                    0
                } else {
                    let factor = if pending.context.card.is_some() {
                        self.relic_block_multiplier(content)
                    } else {
                        1
                    };
                    self.scaled_block(
                        content,
                        Actor::Player,
                        amount + self.block_enchantment(pending.context.card),
                        true,
                        true,
                        factor,
                    )
                };
                if amount > 0 {
                    self.apply_power(content, Actor::Player, power_id::BLOCK_NEXT_TURN, amount);
                }
            }
            Effect::Rampart => {
                self.combat_mut().unwrap().setup = false;
                if self.combat().unwrap().extra_turn {
                    return;
                }
                let rampart: i16 = self
                    .combat()
                    .unwrap()
                    .enemies
                    .iter()
                    .filter(|enemy| enemy.creature.hp > 0)
                    .map(|enemy| enemy.creature.power(power_id::RAMPART).max(0))
                    .sum();
                for enemy in &mut self.combat_mut().unwrap().enemies {
                    if content.enemies[enemy.creature.id as usize].id == "MONSTER.TURRET_OPERATOR"
                        && enemy.creature.hp > 0
                    {
                        enemy.creature.block += rampart;
                    }
                }
            }
            Effect::ToricToughness(_) => {
                // It stores the block the card gained, so none under No Block.
                let value = if self.creature(Actor::Player).power(power_id::NO_BLOCK) > 0 {
                    0
                } else {
                    amount
                };
                self.creature_mut(Actor::Player).powers.push(Power {
                    id: power_id::TORIC_TOUGHNESS,
                    amount: 2,
                    skip_next_decay: false,
                    value,
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
                // Negative raw block strips block (Expose); Shadowmeld doubles any block gain.
                for actor in self.targets(content, target, pending.context) {
                    let mut amount = amount;
                    for _ in 0..if amount > 0 {
                        self.creature(actor).power(power_id::SHADOWMELD).max(0)
                    } else {
                        0
                    } {
                        amount = amount.saturating_mul(2);
                    }
                    let had_block = self.creature(actor).block > 0;
                    self.creature_mut(actor).block =
                        self.creature(actor).block.saturating_add(amount).max(0);
                    // Losing all block (Expose) breaks it too (CreatureCmd.LoseBlock).
                    if had_block && self.creature(actor).block == 0 {
                        self.break_burrow(content, actor);
                    }
                    if amount > 0 {
                        self.trigger(content, Trigger::BlockGained, actor, amount);
                    }
                }
            }
            Effect::Heal(target, _) => {
                for actor in self.targets(content, target, pending.context) {
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
                for actor in self.targets(content, target, pending.context) {
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
                self.combat_mut().unwrap().bombs.push((3, amount.max(0)));
                self.creature_mut(Actor::Player).powers.push(Power {
                    id: power_id::THE_BOMB,
                    amount: 3,
                    skip_next_decay: false,
                    value: 0,
                });
            }
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
                        let card = &mut self.combat_mut().unwrap().hand[index];
                        card.cost_override = Some(cost);
                        card.until_played = true;
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
                for actor in self.targets(content, target, pending.context) {
                    self.creature_mut(actor).add_power(power_id::SLOW, amount);
                }
            }
            Effect::ApplyPower(target, id, raw) => {
                // Blight Strike's Doom goes to the creature its attack hit: one it killed stays
                // dead even when another Axebot respawns into its slot.
                // The Waterfall Giant outlives its death (i16::MAX HP) and still takes it.
                if raw.scale == Scale::LastDamage
                    && target == Target::ChosenEnemy
                    && self.combat().unwrap().last_killed
                    && !pending
                        .context
                        .target
                        .is_some_and(|x| self.creature(Actor::Enemy(x)).hp == i16::MAX)
                {
                    return;
                }
                for actor in self.targets(content, target, pending.context) {
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
                for actor in self.targets(content, target, pending.context) {
                    let amount = self.unsettling_lamp(content, actor, id, amount);
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
                for actor in self.targets(content, target, pending.context) {
                    self.apply_power(content, actor, id, amount);
                }
            }
            Effect::DoublePower(target, id, minimum) => {
                for actor in self.targets(content, target, pending.context) {
                    let amount = self.creature(actor).power(id).max(minimum);
                    let amount = self.unsettling_lamp(content, actor, id, amount);
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
                let result = self.damage_with_bonus(
                    content,
                    target,
                    amount,
                    DamageKind::Attack,
                    Context {
                        source: Actor::Player,
                        ..pending.context
                    },
                );
                self.reaper(content, Actor::Player, target, result.lost + result.blocked);
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
                                self.unsettling_lamp(content, actor, power.id, power.amount);
                            self.apply_debuff(content, actor, power.id, amount);
                        }
                    }
                }
            }
            Effect::RemovePower(target, id) => {
                for actor in self.targets(content, target, pending.context) {
                    self.creature_mut(actor).powers.retain(|x| x.id != id)
                }
            }
            Effect::TemporaryStrength(target, restore, _) => {
                for actor in self.targets(content, target, pending.context) {
                    let amount = -self.unsettling_lamp(content, actor, power_id::STRENGTH, -amount);
                    if self.apply_debuff(content, actor, power_id::STRENGTH, -amount) {
                        self.creature_mut(actor).add_power(restore, amount);
                    }
                }
            }
            Effect::Draw(count) => {
                self.draw(content, count, pending.context, false, 0);
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
                if !self.enemies_defeated()
                    && self
                        .draw(content, 1, pending.context, false, 0)
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
                    self.shuffle_discard_into_draw(content);
                    self.after_shuffle(content);
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
                for id in pool.into_iter().take(count as usize) {
                    let card = Card {
                        id,
                        free,
                        ..Card::default()
                    };
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
                for id in pool.into_iter().take(count as usize) {
                    let card = Card {
                        id,
                        free,
                        ..Card::default()
                    };
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
                while self
                    .combat()
                    .is_some_and(|combat| combat.choice.is_none() && combat.queue.len() > base)
                {
                    let pending = self.combat_mut().unwrap().queue.pop().unwrap();
                    self.apply(content, pending);
                }
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
            Effect::ContinueEndTurn => self.continue_end_turn(content),
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
                // A later play of the series clones the card as Razor Tooth upgraded it (Anger).
                if pending.context.upgraded {
                    card.upgrades = card.upgrades.max(1);
                }
                // Adaptive Strike's copy costs 0 for the rest of combat.
                if content.cards[card.id as usize].id == "CARD.ADAPTIVE_STRIKE" {
                    card.flags |= FREE_COMBAT;
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
            Effect::Bombardments => self.queue_bombardments(),
            Effect::TurnStartOrbs => self.passive_orbs(content, OrbTiming::TurnStart),
            Effect::ToastyMittens => self.toasty_mittens(content, true),
            Effect::BurningSticks(card) => {
                // The game exhausts these before discarding the hand, which sweeps the copy away.
                let pile = if self.turn_end_exhaust {
                    Pile::Discard
                } else {
                    Pile::Hand
                };
                self.add_generated(content, pile, card);
            }
            Effect::Juggle => {
                for card in std::mem::take(&mut self.combat_mut().unwrap().juggled) {
                    self.add_generated(content, Pile::Hand, card);
                }
            }
            // Its hooks don't run once the combat is over (Hellraiser's Strike won the fight).
            Effect::ConfusedRoll(card) => {
                if !self.enemies_defeated()
                    && let Some(cost) = self.confused_cost()
                    && content.cards[card.id as usize].cost[card.upgrades.min(1) as usize] >= 0
                {
                    let combat = self.combat_mut().unwrap();
                    for other in combat
                        .hand
                        .iter_mut()
                        .chain(&mut combat.draw)
                        .chain(&mut combat.discard)
                        .chain(&mut combat.exhaust)
                        .filter(|other| other.instance == card.instance)
                    {
                        confuse(other, cost);
                    }
                }
            }
            Effect::BoulderTick => self.tick_boulders(content),
            Effect::FinishAutoPlay => self.finish_auto_play(content),
            Effect::AutoPlayStep => {
                if !self.enemies_defeated() {
                    let auto = self.combat_mut().unwrap().auto_plays.last_mut().unwrap();
                    auto.counted += 1;
                    if auto.card.enchantment == Some(Enchantment::Momentum) {
                        auto.card.enchantment_value = auto
                            .card
                            .enchantment_value
                            .saturating_add(auto.card.enchantment_amount);
                    }
                    // The next play snapshots its listeners at its own start (Subroutine applied
                    // by an earlier play counts for the later ones).
                    let card = auto.card;
                    let combat = self.combat_mut().unwrap();
                    let (now, enemies_now) = (
                        combat.player.powers.clone(),
                        combat
                            .enemies
                            .iter()
                            .map(|enemy| enemy.creature.powers.clone())
                            .collect(),
                    );
                    let auto = combat.auto_plays.last_mut().unwrap();
                    let powers = std::mem::replace(&mut auto.powers, now);
                    let enemies = std::mem::replace(&mut auto.enemy_powers, enemies_now);
                    self.spend_block_doublers(content, card.instance);
                    self.queue_card_triggers(content, card, powers, enemies, 1);
                    let card_type = card.card_type(content.cards[card.id as usize]);
                    let counted = self.combat().unwrap().auto_plays.last().unwrap().counted as i16;
                    // Juggling copies the turn's third finished attack play, which can be an
                    // earlier play of the series (before the next play counts the copies).
                    self.juggle(
                        content,
                        self.sharpened(content, card, counted as usize),
                        counted as usize,
                        true,
                    );
                    self.rainbow_ring(content, card_type, counted);
                    self.tick_panache();
                }
            }
            Effect::PlacePlayed(card, replay, feral) => {
                if self.place_card(content, card, replay, feral) {
                    self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
                    self.trigger_card(content, card, Trigger::CardExhausted);
                }
            }
            Effect::CheckEmptyHand => self.unceasing_top(content),
            Effect::MummifiedHand => self.mummified_hand(content),
            Effect::SideTurnStart => {
                let combat = self.combat_mut().unwrap();
                combat.paels_legion = combat.paels_legion.saturating_sub(1);
                if combat.turn > 1 {
                    combat.letter_opener = 0;
                }
            }
            Effect::ReturnLate(make_it_so, energy) => {
                if make_it_so {
                    self.return_make_it_so();
                }
                self.return_right_hands(energy);
            }
            // Each copy plays up to 13 cards once the previous copy stops, so the cap adds up.
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
                } else if played < 13 * self.relic_count(content, "RELIC.WHISPERING_EARRING") as u8
                    && !self.combat().unwrap().force_end
                    && !self.enemies_defeated()
                    && let Some(&hand) = self.playable_cards(content).first()
                {
                    // The first playable hand card, aimed at the leftmost hittable enemy.
                    let target =
                        self.actions(content)
                            .into_iter()
                            .find_map(|action| match action {
                                Action::Play { hand: h, target } if h == hand => Some(target),
                                _ => None,
                            });
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
                            combat.history.exhausted += 1;
                            self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
                            self.trigger_card(content, card, Trigger::CardExhausted);
                        } else {
                            combat.discard.push(card);
                        }
                        return;
                    };
                    // Void Form frees these hand cards too, but only manual plays use it up.
                    let manual_cards = self.combat().unwrap().history.manual_cards;
                    self.play(content, hand, target);
                    // Its resources were spent before CardCmd.AutoPlay, whose CardPlay records
                    // none: Black Hole sees no stars spent.
                    self.combat_mut().unwrap().card_stars = 0;
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
                let osty_attacked = combat.history.osty_attacks > 0;
                for card in &mut combat.hand {
                    let def = content.cards[card.id as usize];
                    // Reduce only: a Flatten made free by Osty's attack stays free.
                    let flatten_free = card.id == card_id::FLATTEN
                        && osty_attacked
                        && card.cost_override.is_none();
                    if card_cost(*card, def, combat.energy) > 1 && !flatten_free {
                        if pending.context.upgraded {
                            card.cost_delta = 1 - def.cost[card.upgrades.min(1) as usize];
                        } else {
                            // This turn or until played.
                            card.cost_override = Some(1);
                            card.until_played = true;
                        }
                    }
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
                self.shuffle_discard_into_draw(content);
                self.after_shuffle(content);
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
        let mut ratio = (1i64, 1i64);
        let mut scale = |num: i16, den: i64| {
            ratio.0 *= num as i64;
            ratio.1 *= den;
        };
        if powered {
            amount = amount.saturating_add(self.creature(source).power(power_id::VIGOR));
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
                scale(vulnerable, 100);
            }
            if self.creature(source).kind(content, PowerKind::Weak) > 0 {
                let paper_krane = 15
                    * (matches!(source, Actor::Enemy(_))
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
            if matches!(source, Actor::Enemy(_))
                && self.creature(source).hp <= self.creature(source).power(power_id::DOOM)
                && self.has_relic(content, "RELIC.UNDYING_SIGIL")
            {
                scale(1, 2);
            }
            if self.creature(target).power(power_id::DIAMOND_DIADEM) > 0 {
                scale(1, 2);
            }
            scale(100 + self.creature(target).power(power_id::SLOW), 100);
            if self.creature(source).power(power_id::DOUBLE_DAMAGE) > 0 {
                scale(2, 1);
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
                    scale(3, 2);
                }
            }
            if matches!(source, Actor::Enemy(_))
                && self.creature(source).kind(content, PowerKind::Vulnerable) > 0
                && self.creature(target).power(power_id::COLOSSUS) > 0
            {
                scale(1, 2);
            }
            if self.creature(source).power(power_id::SHRINK) != 0 {
                scale(70, 100);
            }
        }
        if attack && self.creature(target).power(power_id::FLUTTER) > 0 {
            scale(1, 2);
        }
        amount = (amount as i64 * ratio.0 / ratio.1).clamp(0, i16::MAX as i64) as i16;
        let cap = self.creature(target).power(power_id::HARD_TO_KILL);
        if cap > 0 {
            amount = amount.min(cap);
        }
        if self.creature(target).kind(content, PowerKind::Intangible) > 0 {
            amount = amount.min(1);
        }
        amount.max(0)
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

    /// Unblocked damage, direct HP loss included, wakes sleepers and ends Shriek.
    fn wake_on_hp_loss(&mut self, content: &Content, actor: Actor) {
        if let Actor::Enemy(index) = actor
            && self.creature(actor).hp > 0
            && self.creature(actor).power(power_id::SHRIEK) > 0
            && self.creature(actor).hp <= self.creature(actor).power(power_id::SHRIEK)
        {
            self.creature_mut(actor)
                .powers
                .retain(|power| power.id != power_id::SHRIEK);
            let combat = self.combat_mut().unwrap();
            // Stunned on its own turn, it keeps the stun past this turn's move roll.
            let held = combat.enemy_turn;
            let enemy = &mut combat.enemies[index];
            enemy.move_index = 2;
            enemy.last_move = 2;
            enemy.repeats = 1;
            enemy.move_history.push(2);
            enemy.stunned |= held;
        }
        if let Actor::Enemy(index) = actor
            && content.enemies[self.combat().unwrap().enemies[index].creature.id as usize].id
                == "MONSTER.SLUMBERING_BEETLE"
            && self.creature(actor).power(power_id::SLUMBER) > 0
        {
            self.creature_mut(actor).consume_power(power_id::SLUMBER);
            if self.creature(actor).power(power_id::SLUMBER) == 0 {
                let enemy = &mut self.combat_mut().unwrap().enemies[index];
                enemy.move_index = 2;
                enemy.last_move = usize::MAX;
                enemy.repeats = 0;
            }
        }
        if let Actor::Enemy(index) = actor
            && content.enemies[self.combat().unwrap().enemies[index].creature.id as usize].id
                == "MONSTER.LAGAVULIN_MATRIARCH"
            && self.creature(actor).power(power_id::ASLEEP) > 0
        {
            self.creature_mut(actor)
                .powers
                .retain(|power| !matches!(power.id, 79 | power_id::ASLEEP));
            let enemy = &mut self.combat_mut().unwrap().enemies[index];
            enemy.move_index = 5;
            enemy.last_move = usize::MAX;
            enemy.repeats = 0;
        }
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
        // Calculated damage (Precise Cut) may go negative; modifiers apply before the final clamp.
        let mut amount = base;
        let played = self
            .combat()
            .unwrap()
            .auto_plays
            .last()
            .map(|play| play.card)
            .or(self.combat().unwrap().playing)
            .filter(|played| card == Some(played.id));
        // Corrupted's ×1.5 keeps its half point until the single final floor.
        let mut half = 0i64;
        if powered && matches!(source, Actor::Player | Actor::Osty) {
            match played.and_then(|card| card.enchantment.map(|enchantment| (card, enchantment))) {
                Some((_, Enchantment::Corrupted)) => {
                    half = (amount as i64 * 3).rem_euclid(2);
                    amount = (amount as i64 * 3).div_euclid(2) as i16;
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
            scale(self.creature(target).power(power_id::HANG).max(1), 1);
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
                amount = amount.saturating_add(self.creature(target).power(power_id::TAINTED));
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
            if self.creature(target).kind(content, PowerKind::Vulnerable) > 0 {
                // Cruelty and Paper Phrog read the dealer's own powers and relics, so Osty's
                // attacks get neither.
                let cruelty = if source == Actor::Player && matches!(target, Actor::Enemy(_)) {
                    self.creature(Actor::Player).power(power_id::CRUELTY)
                } else {
                    0
                };
                let paper_phrog = 25
                    * (source == Actor::Player
                        && matches!(target, Actor::Enemy(_))
                        && self.has_relic(content, "RELIC.PAPER_PHROG"))
                        as i16;
                // Debilitate doubles the whole bonus, Cruelty's and Paper Phrog's included.
                let bonus = (50 + cruelty + paper_phrog)
                    * (1 + (self.creature(target).power(power_id::DEBILITATE) > 0) as i16);
                scale(100 + bonus, 100);
            }
            if self.creature(source).kind(content, PowerKind::Weak) > 0 {
                let paper_krane = 15
                    * (target == Actor::Player
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
            if target == Actor::Player
                && matches!(source, Actor::Enemy(_))
                && self.creature(source).hp <= self.creature(source).power(power_id::DOOM)
                && self.has_relic(content, "RELIC.UNDYING_SIGIL")
            {
                scale(1, 2);
            }
            if self.creature(target).power(power_id::DIAMOND_DIADEM) > 0 {
                scale(1, 2);
            }
            scale(100 + self.creature(target).power(power_id::SLOW), 100);
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
                    scale(3, 2);
                }
            }
            if target == Actor::Player
                && matches!(source, Actor::Enemy(_))
                && self.creature(source).kind(content, PowerKind::Vulnerable) > 0
                && self.creature(target).power(power_id::COLOSSUS) > 0
            {
                scale(1, 2);
            }
            if self.creature(source).power(power_id::SHRINK) != 0 {
                scale(70, 100);
            }
        }
        // Flutter and Soar halve any powered attack, move damage included (IsPoweredAttack).
        if powered && self.creature(target).power(power_id::FLUTTER) > 0 {
            scale(1, 2);
        }
        if powered && self.creature(target).power(power_id::SOAR) > 0 {
            scale(1, 2);
        }
        // Tracking doubles any powered attack from a card on a Weak enemy, move damage included.
        if powered
            && matches!(source, Actor::Player | Actor::Osty)
            && card.is_some()
            && self.creature(target).kind(content, PowerKind::Weak) > 0
        {
            scale(
                self.creature(Actor::Player)
                    .power(power_id::TRACKING)
                    .max(1),
                1,
            );
        }
        if attack {
            if source == Actor::Player
                && card == Some(card_id::SOVEREIGN_BLADE)
                && self.creature(target).power(power_id::CONQUEROR) > 0
            {
                scale(2, 1);
            }
        }
        // Gigantification triples any powered attack from a card, move damage included.
        if powered
            && matches!(source, Actor::Player | Actor::Osty)
            && card.is_some()
            && self
                .creature(Actor::Player)
                .power(power_id::GIGANTIFICATION)
                > 0
        {
            scale(3, 1);
        }
        amount =
            ((amount as i64 * 2 + half) * ratio.0 / (ratio.1 * 2)).clamp(0, i16::MAX as i64) as i16;
        let cap = self.creature(target).power(power_id::HARD_TO_KILL);
        if !overflow_kind && cap > 0 {
            amount = amount.min(cap);
        }
        amount = amount.max(0);
        if attack {
            self.trigger(content, Trigger::Attacked, target, amount);
            if matches!(source, Actor::Player | Actor::Osty)
                && card.is_some()
                && let Actor::Enemy(index) = target
            {
                // Curl Up's block comes once the card that hit it (Osty's too) finishes.
                let combat = self.combat_mut().unwrap();
                if combat.enemies[index].creature.power(power_id::CURL_UP) > 0
                    && !combat.curled.contains(&index)
                {
                    combat.curled.push(index);
                }
            }
        }
        if !overflow_kind && self.creature(target).kind(content, PowerKind::Intangible) > 0 {
            amount = amount.min(1);
        }
        if target == Actor::Player && matches!(source, Actor::Enemy(_)) {
            let expected = self.incoming_damage_amount(content, source, base, kind);
            debug_assert_eq!(amount, expected);
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
        if blocked > 0
            && self.creature(target).block == 0
            && matches!(target, Actor::Enemy(_))
            && matches!(source, Actor::Player | Actor::Osty)
            && self.has_relic(content, "RELIC.HAND_DRILL")
        {
            // A relic hook with no card source: Unsettling Lamp leaves it alone.
            let hook = std::mem::replace(&mut self.combat_mut().unwrap().applying_hook, true);
            self.apply_power(content, target, power_id::VULNERABLE, 2);
            self.combat_mut().unwrap().applying_hook = hook;
        }
        if blocked > 0 && self.creature(target).block == 0 {
            self.break_burrow(content, target);
        }
        let mut lost = amount - blocked;
        if target == Actor::Player
            && (matches!(kind, DamageKind::Unblockable)
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
        let slippery = lost > 0 && self.creature(hp_target).power(power_id::SLIPPERY) > 0;
        if slippery {
            lost = 1;
        }
        if lost > 0 && hp_target == Actor::Player {
            lost = self.modify_player_hp_lost(content, lost);
        }
        lost = self.buffer(content, hp_target, lost);
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
        if hp_target == Actor::Player {
            self.after_player_hp_loss(content, lost);
        }
        self.sync_red_skull(content);
        if lost != 0
            && let Some(power) = self
                .creature_mut(target)
                .powers
                .iter_mut()
                .find(|power| power.id == power_id::HARDENED_SHELL)
        {
            power.value = power.value.saturating_add(lost);
        }
        if lost > 0 {
            self.wake_on_hp_loss(content, hp_target);
        }
        if slippery && lost > 0 {
            self.creature_mut(hp_target)
                .consume_power(power_id::SLIPPERY);
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
                    upgraded: false,
                    x: 0,
                    event: 0,
                    orb: false,
                    orb_id: None,
                    pen_nib: false,
                    lethal: false,
                    hook: false,
                    off_pile: false,
                    target_instance: None,
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
        if (attack || overflow_kind) && hp_target == Actor::Player && lost > 0 {
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
        if powered && lost > 0 && self.creature(target).power(power_id::FLUTTER) > 0 {
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
        if lost > 0 {
            self.trigger(content, Trigger::HpLost, hp_target, lost);
            if hp_target == Actor::Player {
                self.inferno();
            }
        }
        if blocked > 0 {
            self.trigger(content, Trigger::Blocked, block_target, blocked);
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
        if powered && lost > 0 {
            let poison = self.creature(source).power(power_id::ENVENOM);
            if poison > 0 && self.combat().unwrap().dying.is_some() {
                self.combat_mut()
                    .unwrap()
                    .envenomed
                    .push((hp_target, poison));
            } else if poison > 0 {
                self.envenom(content, hp_target, poison);
            }
        }
        // A creature the hit killed can't receive Monarch's Gaze (CanReceivePowers).
        if powered && source == Actor::Player && self.creature(target).hp > 0 {
            let loss = self.creature(source).power(power_id::MONARCHS_GAZE);
            if loss > 0 && self.apply_debuff(content, target, power_id::STRENGTH, -loss) {
                self.creature_mut(target)
                    .add_power(power_id::MONARCHS_GAZE_STRENGTH_DOWN, loss);
            }
        }
        // Plow breaks in AfterDamageReceived, after the dealer's AfterDamageGiven (Monarch's Gaze).
        if lost > 0 {
            self.break_plow(hp_target);
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
        if powered || card.is_some_and(|card| content.cards[card as usize].id == "CARD.OMNISLICE") {
            let thorns = self.creature(target).kind(content, PowerKind::Thorns);
            if thorns > 0 {
                self.damage(content, target, source, thorns, DamageKind::Unpowered, None);
            }
        }
        // A killed hive no longer reacts: its powers are gone by AfterDamageReceived.
        if powered && matches!(source, Actor::Player | Actor::Osty) && self.creature(target).hp > 0
        {
            // Personal Hive's Dazed has no creator, so generation hooks and Supermassive ignore it.
            for _ in 0..self.creature(target).power(power_id::PERSONAL_HIVE) {
                self.insert_random(
                    Pile::Draw,
                    Card {
                        id: card_id::DAZED,
                        ..Card::default()
                    },
                );
            }
        }
        if (attack || overflow_kind)
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
            match &mut self.combat_mut().unwrap().dying {
                Some(dying) => dying.push(hp_target),
                None => self.died(content, hp_target),
            }
        }
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
            // It's used like any potion, so AfterPotionUsed relics fire (Reptile Trinket).
            for _ in 0..self.relic_count(content, "RELIC.REPTILE_TRINKET") {
                for power in [power_id::REPTILE_TRINKET, power_id::STRENGTH] {
                    self.apply_power(content, Actor::Player, power, 3);
                }
            }
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
        // Crab Rage is an enemy AfterDeath hook, after the player's relics (Gremlin Horn).
        let mut enraged = Vec::new();
        if let Actor::Enemy(dead) = actor {
            if heist > 0 {
                self.reward_gold_parts.push(heist);
            }
            if content.enemies[self.creature(actor).id as usize].id == "MONSTER.SHRINKER_BEETLE" {
                self.creature_mut(Actor::Player)
                    .powers
                    .retain(|power| power.id != power_id::SHRINK || power.amount > 0);
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
                // Death strips its other powers; Steam Eruption stays until the About to Blow
                // move stores it as the blast.
                let creature = self.creature_mut(actor);
                creature
                    .powers
                    .retain(|power| power.id == power_id::STEAM_ERUPTION);
                creature.hp = i16::MAX;
                creature.max_hp = i16::MAX;
                let enemy = &mut self.combat_mut().unwrap().enemies[dead];
                enemy.move_index = 7;
                enemy.last_move = 7;
                enemy.repeats = 1;
                enemy.move_history.push(7);
                enemy.value = steam;
                // AfterDeath still fires with the removal prevented.
                self.gremlin_horn(content);
                return;
            }
            enraged = self
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
            // Illusions keep buffs (Strength at any amount) and temporary debuffs.
            creature.powers.retain(|power| {
                !content.powers[power.id as usize].debuff
                    || TEMPORARY_STRENGTH_DOWN.contains(&power.id)
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
                    // Its restoring upgrades are skipped once combat is ending (CardCmd.Upgrade).
                    let dampened = std::mem::take(&mut self.combat_mut().unwrap().dampened);
                    let dampened = if self.enemies_defeated() {
                        Vec::new()
                    } else {
                        dampened
                    };
                    let combat = self.combat_mut().unwrap();
                    for card in combat
                        .draw
                        .iter_mut()
                        .chain(&mut combat.hand)
                        .chain(&mut combat.discard)
                        .chain(&mut combat.exhaust)
                        .chain(played(&mut combat.queue))
                        .chain(&mut combat.playing)
                        .chain(combat.auto_plays.iter_mut().map(|auto| &mut auto.card))
                    {
                        if let Some((_, upgrades)) = dampened
                            .iter()
                            .find(|(instance, _)| *instance == card.instance)
                        {
                            card.upgrades = *upgrades;
                        }
                    }
                    // The card being played reads its restored upgrade for its remaining
                    // effects (DynamicVars are read when each effect runs).
                    if let Some(card) = combat.playing.filter(|card| card.upgrades > 0) {
                        for pending in combat.queue.iter_mut().filter(|pending| {
                            pending.context.card == Some(card.id) && !pending.context.hook
                        }) {
                            pending.context.upgraded = true;
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
            // Its AfterDeath spawns (Stock, Surprise) still see it among the enemies.
            self.corpse_hp = Some(self.creature(actor).max_hp);
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
            self.corpse_hp = None;
            if infested > 0 {
                self.spawn_wrigglers(content, infested);
            }
            self.gremlin_horn(content);
            for index in enraged {
                let creature = self.creature_mut(Actor::Enemy(index));
                creature.add_power(power_id::STRENGTH, 6);
                creature.block = creature.block.saturating_add(99);
                creature
                    .powers
                    .retain(|power| power.id != power_id::CRAB_RAGE);
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
            self.insert_enemy(
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
                    last_move: 0,
                    repeats: 1,
                    move_history: vec![0],
                    stunned: false,
                    value: wriggler,
                    slot: 0,
                },
                false,
            );
        }
    }

    pub(super) fn spawn_enemy(&mut self, content: &Content, name: &str) -> usize {
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
                skip_next_decay: false,
                value: 0,
            })
            .collect();
        let fur_coat = self.fur_coat_active();
        let enemies = &self.combat().unwrap().enemies;
        let free = |slot: &u8| {
            enemies
                .iter()
                .all(|e| e.creature.hp <= 0 || e.slot != *slot)
        };
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
            // A rat called for backup or a laid egg takes the last free slot.
            slot: if matches!(name, "MONSTER.TWO_TAILED_RAT" | "MONSTER.TOUGH_EGG") {
                let last = enemies.iter().map(|enemy| enemy.slot).max();
                (1..=last.unwrap_or_default().max(5))
                    .rev()
                    .find(free)
                    .unwrap_or(0)
            } else if FABRICATOR_BOTS.contains(&name) {
                // A fabricated bot takes the first free slot (bot1, bot2, fabricator, bot3, bot4).
                (1..=5).find(free).unwrap_or(0)
            } else {
                0
            },
        };
        self.insert_enemy(enemy, true)
    }

    /// Summons the game slots in front of their summoner, so they come first in its enemy list.
    fn slot_order(&self, content: &Content, index: usize) -> (bool, u8) {
        let enemy = &self.combat().unwrap().enemies[index];
        let front = matches!(
            content.enemies[enemy.creature.id as usize].id,
            "MONSTER.GAS_BOMB"
                | "MONSTER.EYE_WITH_TEETH"
                | "MONSTER.PARAFRIGHT"
                | "MONSTER.TOUGH_EGG"
        );
        (!front, enemy.slot)
    }

    /// Osty's first attack this turn sets every Flatten's cost to 0 for the turn.
    fn mark_flattens(&mut self) {
        let combat = self.combat_mut().unwrap();
        if combat.history.osty_attacks != 1 {
            return;
        }
        for pile in [
            &mut combat.draw,
            &mut combat.hand,
            &mut combat.discard,
            &mut combat.exhaust,
        ] {
            for card in pile.iter_mut().filter(|card| card.id == card_id::FLATTEN) {
                card.value = card.cost_delta as i16 + 1;
            }
        }
    }

    fn alive_in_slot_order(&self, content: &Content) -> Vec<usize> {
        let mut alive: Vec<_> = (0..self.combat().unwrap().enemies.len())
            .filter(|&index| self.combat().unwrap().enemies[index].creature.hp > 0)
            .collect();
        alive.sort_by_key(|&index| self.slot_order(content, index));
        alive
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

    // HP-loss modifiers (ModifyHpLostAfterOsty) apply once per relic copy, in relic order.
    fn modify_player_hp_lost(&self, content: &Content, mut lost: i16) -> i16 {
        let remaining = (20 - self.combat().unwrap().history.remnant_lost).max(0);
        for (index, &id) in self.run.relics.iter().enumerate() {
            if self.melted_relics.contains(&index) {
                continue;
            }
            match content.relics[id as usize].id {
                "RELIC.TUNGSTEN_ROD" => lost = (lost - 1).max(0),
                "RELIC.BEATING_REMNANT" => lost = lost.min(remaining),
                _ => {}
            }
        }
        lost
    }

    /// Relic counters that tick as each play finishes (AfterCardPlayed).
    fn count_finished_play(&mut self, content: &Content, card_type: CardType, plays: i16) {
        if card_type == CardType::Attack {
            self.every_third_attack(content, plays);
            if self.has_relic(content, "RELIC.NUNCHAKU") {
                let attacks = self.nunchaku as i16 + plays;
                self.gain_energy(attacks / 10);
                self.nunchaku = (attacks % 10) as u8;
            }
        }
        if card_type == CardType::Skill && self.has_relic(content, "RELIC.LETTER_OPENER") {
            let skills = self.combat().unwrap().letter_opener as i16 + plays;
            self.combat_mut().unwrap().letter_opener = (skills % 3) as u8;
            for _ in 0..skills / 3 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Damage(Target::AllEnemies, Amount::fixed(5, 5)),
                    context: Context::player(),
                });
            }
        }
        if self.has_relic(content, "RELIC.IRON_CLUB") {
            let cards = self.iron_club as i16 + plays;
            self.iron_club = (cards % 4) as u8;
            for _ in 0..cards / 4 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Draw(1),
                    context: Context::player(),
                });
            }
        }
        if card_type == CardType::Skill && self.has_relic(content, "RELIC.TUNING_FORK") {
            let skills = self.tuning_fork as i16 + plays;
            self.tuning_fork = (skills % 10) as u8;
            for _ in 0..skills / 10 {
                self.gain_block(content, Actor::Player, 7, None, false);
            }
        }
    }

    /// Ornamental Fan, Kunai, Shuriken and Kusarigama count finished attack plays this turn.
    fn every_third_attack(&mut self, content: &Content, plays: i16) {
        let attacks = self.combat().unwrap().attacks_mod3 as i16 + plays;
        self.combat_mut().unwrap().attacks_mod3 = (attacks % 3) as u8;
        for _ in 0..attacks / 3 {
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

    // Every HP loss is damage in the game, so these relics react to self-inflicted loss too.
    fn after_player_hp_loss(&mut self, content: &Content, lost: i16) {
        if lost <= 0 {
            return;
        }
        if !self.combat().unwrap().enemy_turn
            && self.has_relic(content, "RELIC.DEMON_TONGUE")
            && !self.combat().unwrap().demon_tongue
        {
            self.combat_mut().unwrap().demon_tongue = true;
            let player = self.creature_mut(Actor::Player);
            player.hp = (player.hp + lost).min(player.max_hp);
        }
        if self.has_relic(content, "RELIC.CENTENNIAL_PUZZLE")
            && !self.combat().unwrap().centennial_puzzle
        {
            self.combat_mut().unwrap().centennial_puzzle = true;
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Draw(3),
                context: Context::player(),
            });
        }
        for _ in 0..self.relic_count(content, "RELIC.SELF_FORMING_CLAY") {
            self.apply_power(content, Actor::Player, power_id::SELF_FORMING_CLAY, 3);
        }
    }

    // Buffer (ModifyHpLostAfterOstyLate) cancels HP loss left after the other modifiers.
    fn buffer(&mut self, content: &Content, actor: Actor, lost: i16) -> i16 {
        let Some(id) = self
            .creature(actor)
            .powers
            .iter()
            .find(|power| {
                content.powers[power.id as usize].kind == PowerKind::Buffer && power.amount > 0
            })
            .map(|power| power.id)
        else {
            return lost;
        };
        if lost > 0 {
            self.creature_mut(actor).consume_power(id);
            return 0;
        }
        lost
    }

    /// Fabricator spawns a bot from FABRICATOR_BOTS[set..set + 2], never repeating its last spawn;
    /// its `value` holds 1 + the last bot's index.
    fn spawn_bot(&mut self, content: &Content, fabricator: usize, set: usize) {
        let last = self.combat().unwrap().enemies[fabricator].value as usize;
        let options: Vec<_> = (set..set + 2).filter(|&bot| bot + 1 != last).collect();
        let bot = options[self.rngs.monster_ai.below(options.len() as u32) as usize];
        self.combat_mut().unwrap().enemies[fabricator].value = bot as i16 + 1;
        let spawned = self.spawn_enemy(content, FABRICATOR_BOTS[bot]);
        self.apply_power(content, Actor::Enemy(spawned), power_id::MINION, 1);
    }

    fn reaper(&mut self, content: &Content, source: Actor, target: Actor, dealt: i16) {
        if matches!(source, Actor::Player | Actor::Osty) && dealt > 0 {
            let doom =
                dealt.saturating_mul(self.creature(Actor::Player).power(power_id::REAPER_FORM));
            // Reaper Form's Doom has no card source, so Unsettling Lamp doesn't double it.
            let hook = std::mem::replace(&mut self.combat_mut().unwrap().applying_hook, true);
            self.apply_power(content, target, power_id::DOOM, doom);
            self.combat_mut().unwrap().applying_hook = hook;
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

    /// The played card's own type (a Mad Science power); it waits in the queue for its pile.
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
            .chain(combat.playing.iter_mut())
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

    fn played_card_type(&self, content: &Content, card: Id) -> CardType {
        let def = content.cards[card as usize];
        self.combat()
            .and_then(|combat| {
                combat
                    .queue
                    .iter()
                    .rev()
                    .find_map(|pending| match pending.effect {
                        Effect::PlacePlayed(played, ..) if played.id == card => Some(played),
                        _ => None,
                    })
            })
            .map_or(def.card_type, |played| played.card_type(def))
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
            let combat = self.combat_mut().unwrap();
            let held = combat.enemy_turn;
            let enemy = &mut combat.enemies[index];
            enemy.move_index = 3;
            enemy.last_move = 3;
            enemy.repeats = 1;
            enemy.stunned |= held;
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
        // No Block stops powered card block only (Entrench's unpowered block still lands).
        if actor == Actor::Player
            && card.is_some()
            && powered
            && self.creature(actor).power(power_id::NO_BLOCK) > 0
        {
            return 0;
        }
        // Fasten adds to powered Defend block only.
        let fasten = card
            .filter(|&id| powered && content.cards[id as usize].tags & DEFEND_TAG != 0)
            .map_or(0, |_| self.creature(actor).power(power_id::FASTEN));
        let enchantment = if actor == Actor::Player {
            self.block_enchantment(card)
        } else {
            0
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
        let amount = self.scaled_block(
            content,
            actor,
            base.saturating_add(fasten).saturating_add(enchantment),
            powered,
            powered || card.is_some(),
            factor,
        );
        self.creature_mut(actor).block = self.creature(actor).block.saturating_add(amount);
        // The game's block is a decimal floored only when added: Frail turning 1 block into 0.75
        // still records a block gain (Unmovable) and fires AfterBlockGained (Juggernaut).
        let dexterity = if powered {
            self.creature(actor).kind(content, PowerKind::Dexterity)
        } else {
            0
        };
        let positive = amount > 0
            || factor > 0
                && base
                    .saturating_add(fasten)
                    .saturating_add(enchantment)
                    .saturating_add(dexterity)
                    > 0;
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
            self.trigger(content, Trigger::BlockGained, actor, amount);
        }
        amount
    }

    /// Vambrace doubles the block of the first card that gains block; Pael's Legion doubles all
    /// card block (a Sly card auto-played by it included) until that first card's play finishes.
    fn relic_block_multiplier(&mut self, content: &Content) -> i16 {
        let combat = self.combat().unwrap();
        let instance = combat
            .auto_plays
            .last()
            .map(|play| play.card)
            .or(combat.playing)
            .map_or(0, |card| card.instance);
        let first = combat.block_doubler.is_none_or(|card| card == instance);
        let vambrace = first && !combat.vambrace && self.has_relic(content, "RELIC.VAMBRACE");
        let legion = combat.paels_legion == 0 && self.has_relic(content, "RELIC.PAELS_LEGION");
        if (vambrace || legion) && combat.block_doubler.is_none() {
            self.combat_mut().unwrap().block_doubler = Some(instance);
        }
        (1 + vambrace as i16) * (1 + legion as i16)
    }

    fn block_enchantment(&self, card: Option<Id>) -> i16 {
        let combat = self.combat().unwrap();
        combat
            .auto_plays
            .last()
            .map(|play| play.card)
            .or(combat.playing)
            .filter(|played| card == Some(played.id))
            .map_or(0, |card| match card.enchantment {
                Some(Enchantment::Nimble) => card.enchantment_amount,
                Some(Enchantment::Goopy) => card.enchantment_amount.saturating_sub(1),
                _ => 0,
            })
    }

    pub(crate) fn modified_block(
        &self,
        content: &Content,
        actor: Actor,
        base: i16,
        card_or_move: bool,
    ) -> i16 {
        self.scaled_block(content, actor, base, card_or_move, card_or_move, 1)
    }

    /// Dexterity and Frail need powered block; Unmovable any card or move block (Entrench).
    fn scaled_block(
        &self,
        content: &Content,
        actor: Actor,
        base: i16,
        powered: bool,
        card_or_move: bool,
        factor: i16,
    ) -> i16 {
        let mut amount = base
            .saturating_add(if powered {
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
                - if self.combat().unwrap().history.block_card == self.play_key() {
                    self.combat().unwrap().history.block_card_gains
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
        amount
    }

    /// Identifies the current card play: each replay of a series is its own play (CardPlay).
    fn play_key(&self) -> i32 {
        let combat = self.combat().unwrap();
        let index = combat.auto_plays.last().map_or(
            combat.card_plays as i32 - combat.pending_plays.len() as i32,
            |auto| 256 * combat.auto_plays.len() as i32 + auto.counted as i32,
        );
        combat.history.cards as i32 * 4096 + index
    }

    pub(super) fn apply_power(&mut self, content: &Content, actor: Actor, id: Id, mut amount: i16) {
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
                .chain(combat.playing.iter_mut())
                .chain(combat.auto_plays.iter_mut().map(|play| &mut play.card))
                .chain(played(&mut combat.queue))
                .filter(|card| card.id == card_id::SOVEREIGN_BLADE)
            {
                card.replays = (card.replays as i16 + amount).clamp(0, u8::MAX as i16) as u8;
            }
        }
        if actor == Actor::Player
            && id == power_id::STRENGTH
            && amount > 0
            && self.has_relic(content, "RELIC.RUINED_HELMET")
            && !self.combat().unwrap().ruined_helmet
        {
            amount *= 2;
            self.combat_mut().unwrap().ruined_helmet = true;
        }
        // Snecko Skull is additive, so it comes before Unsettling Lamp's doubling.
        if id == power_id::POISON
            && matches!(actor, Actor::Enemy(_))
            && amount > 0
            && self.has_relic(content, "RELIC.SNECKO_SKULL")
        {
            amount += 1;
        }
        amount = self.unsettling_lamp(content, actor, id, amount);
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
            } else if id == power_id::RITUAL && before == 0 {
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

    /// Unsettling Lamp doubles every debuff the first debuffing card of the combat applies.
    fn unsettling_lamp(&mut self, content: &Content, actor: Actor, id: Id, amount: i16) -> i16 {
        // Power and relic hooks apply their debuffs with no card source.
        if !self.combat().unwrap().applying_hook
            && matches!(actor, Actor::Enemy(_))
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
                    self.creature_mut(Actor::Player).block =
                        self.creature(Actor::Player).block.saturating_add(block);
                    self.trigger(content, Trigger::BlockGained, Actor::Player, block);
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
            self.damage(content, actor, actor, amount, DamageKind::Unblockable, None);
            self.resolve(content);
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
            if let Some(card) = combat.playing.filter(|card| card.upgrades > 0) {
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
            let previous = self.combat_mut().unwrap().dying.replace(vec![]);
            for enemy in self.targets(content, Target::AllEnemies, Context::player()) {
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
            source: Actor::Player,
            target: None,
            card: None,
            upgraded: false,
            x: 0,
            event: orb.value,
            orb: true,
            orb_id: Some(orb.id),
            pen_nib: false,
            lethal: false,
            hook: false,
            off_pile: false,
            target_instance: None,
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
                source: Actor::Player,
                target,
                card: None,
                upgraded: false,
                x: 0,
                event,
                orb: target.is_some(),
                orb_id: Some(orb.id),
                pen_nib: false,
                lethal: false,
                hook: false,
                off_pile: false,
                target_instance: None,
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
        if trigger == Trigger::CardExhausted && owner == Actor::Player {
            let combat = self.combat_mut().unwrap();
            combat.exhaust_queue = Some((combat.queue.len(), None));
        }
        if trigger == Trigger::CardExhausted
            && owner == Actor::Player
            && self.has_relic(content, "RELIC.JOSS_PAPER")
        {
            if let Some(count) = &mut self.joss_ethereal {
                *count += 1;
            } else {
                self.joss_paper = (self.joss_paper + 1) % 5;
                if self.joss_paper == 0 {
                    let combat = self.combat_mut().unwrap();
                    let joss = combat.queue.len();
                    if let Some((_, draw)) = &mut combat.exhaust_queue {
                        *draw = Some(joss);
                    }
                    combat.queue.push(Pending {
                        effect: Effect::Draw(1),
                        context: Context::player(),
                    });
                }
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
            lethal: false,
            hook: false,
            off_pile: false,
            target_instance: None,
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
            if trigger == Trigger::TurnStart
                && matches!(
                    power.id,
                    power_id::FURNACE
                        | power_id::PREP_TIME
                        | power_id::DEMON_FORM
                        | power_id::CRIMSON_MANTLE
                        | power_id::INFERNO
                )
                || trigger == Trigger::TurnStart
                    && owner == Actor::Player
                    && power.id == 97
                    && self.hello_world_before_void(content)
                || trigger == Trigger::TurnEnd && AFTER_SIDE_TURN_END.contains(&power.id)
                // The player's Plating blocks earlier (BeforeSideTurnEndEarly, in end_turn).
                || trigger == Trigger::TurnEnd
                    && owner == Actor::Player
                    && power.id == power_id::PLATING
            {
                continue;
            }
            // Generation hooks only count cards the player creates; enemies add theirs on their turn.
            if trigger == Trigger::CardGenerated && self.combat().unwrap().enemy_turn {
                continue;
            }
            if trigger == Trigger::CardDrawn
                && content.powers[power.id as usize].id == "POWER.ITERATION_POWER"
                && self.combat().unwrap().history.statuses_drawn > 1
            {
                continue;
            }
            if trigger == Trigger::HpLost
                && power.id == power_id::RUPTURE
                && self.combat().unwrap().enemy_turn
            {
                continue;
            }
            if trigger == Trigger::TurnEnd && power.id == power_id::MONARCHS_GAZE_STRENGTH_DOWN {
                // A dead Illusion keeps the power but gets no Strength back.
                if self.creature(owner).hp > 0 {
                    self.creature_mut(owner)
                        .add_power(power_id::STRENGTH, power.amount);
                }
                self.creature_mut(owner)
                    .powers
                    .retain(|current| current.id != power.id);
                continue;
            }
            if trigger == Trigger::TurnEnd && power.id == power_id::RITUAL && power.skip_next_decay
            {
                self.creature_mut(owner)
                    .powers
                    .iter_mut()
                    .find(|current| current.id == power.id)
                    .unwrap()
                    .skip_next_decay = false;
                continue;
            }
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
                    .is_some_and(|card| self.played_card_type(content, card) == CardType::Skill)
            {
                self.apply_power(content, owner, power_id::STRENGTH, power.amount);
            }
            // Galvanic hits for each played card carrying its Galvanized affliction.
            if trigger == Trigger::CardPlayed
                && power.id == power_id::GALVANIC
                && context.card.is_some_and(|card| {
                    self.played_card(card).map_or(
                        self.played_card_type(content, card) == CardType::Power,
                        |played| played.affliction == Affliction::Galvanized,
                    )
                })
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
            // Vital Spark applies Tainted for each played card carrying its Tainted affliction.
            if trigger == Trigger::CardPlayed
                && power.id == power_id::VITAL_SPARK
                && context.card.is_some_and(|card| {
                    self.played_card(card).map_or(
                        self.played_card_type(content, card) == CardType::Skill,
                        |played| played.affliction == Affliction::Tainted,
                    )
                })
            {
                self.apply_power(content, Actor::Player, power_id::TAINTED, power.amount);
            }
            // Smoggy afflicts every unafflicted skill with Smog once a skill finishes playing.
            if trigger == Trigger::CardPlayed
                && owner == Actor::Player
                && power.id == power_id::SMOGGY
                && context
                    .card
                    .is_some_and(|card| self.played_card_type(content, card) == CardType::Skill)
            {
                self.afflict_all(content, Affliction::Smog, &[CardType::Skill]);
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
            if trigger == Trigger::TurnEnd && power.id == power_id::DOUBLE_DAMAGE {
                self.creature_mut(owner).consume_power(power.id);
            }
            // Corrosive Wave lasts through the turn-end card effects (AfterSideTurnEnd), so their
            // draws still poison.
            if matches!(content.powers[power.id as usize].kind, PowerKind::OneShot(at) if at == trigger)
                && !(trigger == Trigger::TurnEnd && power.id == power_id::CORROSIVE_WAVE)
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
                // Strangle, Serpent Form, Monologue, Afterimage and Oblivion use their stacks from
                // when the card started (snapshot), not after it stacked. A temporary Strength power
                // restores its amount even when Plow's break removed it earlier in the same turn end
                // (the hook list is a snapshot).
                if matches!(
                    trigger,
                    Trigger::PowerPlayed | Trigger::CardDrawn | Trigger::CardGenerated
                ) || matches!(
                    power.id,
                    power_id::STRANGLE
                        | power_id::SERPENT_FORM
                        | power_id::MONOLOGUE
                        | power_id::AFTERIMAGE
                        | power_id::OBLIVION
                        | power_id::FLEX_POTION
                        | power_id::FEEDING_FRENZY
                        | power_id::REPTILE_TRINKET
                ) || TEMPORARY_STRENGTH_DOWN.contains(&power.id)
                {
                    context.event = power.amount;
                }
                context.hook = true;
                push_effects(&mut self.combat_mut().unwrap().queue, hook.effects, context);
            }
        }
        // Relics too only count cards the player creates (Regalite).
        if owner == Actor::Player
            && !(trigger == Trigger::CardGenerated && self.combat().unwrap().enemy_turn)
        {
            for i in (0..self.run.relics.len()).rev() {
                if self.melted_relics.contains(&i) {
                    continue;
                }
                let id = self.run.relics[i];
                // Cloak Clasp blocks in BeforeSideTurnEnd, ahead of the orbs (in end_turn).
                if trigger == Trigger::TurnEnd
                    && content.relics[id as usize].id == "RELIC.CLOAK_CLASP"
                {
                    continue;
                }
                for hook in content.relics[id as usize]
                    .hooks
                    .iter()
                    .rev()
                    .filter(|x| x.trigger == trigger)
                {
                    let context = Context {
                        hook: true,
                        off_pile: false,
                        ..context
                    };
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
            let copy = Card {
                instance: 0,
                ..card
            };
            // The copy comes after the powers' exhaust hooks (Dark Embrace) and, in relic order,
            // before or after Joss Paper's draw.
            let position = |id| {
                (self.run.relics.iter()).position(|&relic| content.relics[relic as usize].id == id)
            };
            let joss_first = position("RELIC.JOSS_PAPER") < position("RELIC.BURNING_STICKS");
            let len = self.combat().unwrap().queue.len();
            match self.combat().unwrap().exhaust_queue {
                Some((base, draw)) if base <= len => {
                    let index = draw.map_or(base, |draw| draw + !joss_first as usize);
                    self.combat_mut().unwrap().queue.insert(
                        index.min(len),
                        Pending {
                            effect: Effect::BurningSticks(copy),
                            context: Context::player(),
                        },
                    );
                }
                _ => self.add_generated(content, Pile::Hand, copy),
            }
        }
        self.combat_mut().unwrap().exhaust_queue = None;
        // Drum of Battle's exhaust energy goes through GeneratePlayCount: replays and play-count
        // powers (Duplication, Burst, Echo Form, Throwing Axe) multiply it and are spent.
        if trigger == Trigger::CardExhausted
            && content.cards[card.id as usize].id == "CARD.DRUM_OF_BATTLE"
        {
            let throwing_axe = self.has_relic(content, "RELIC.THROWING_AXE")
                && !self.combat().unwrap().throwing_axe;
            let combat = self.combat_mut().unwrap();
            combat.throwing_axe |= throwing_axe;
            let mut plays = 1
                + throwing_axe as u8
                + card.replays
                + matches!(card.enchantment, Some(Enchantment::Spiral)) as u8
                + (card.enchantment == Some(Enchantment::Glam) && card.enchantment_value > 0) as u8
                + (combat.history.first_plays < combat.player.power(power_id::ECHO_FORM)) as u8;
            for id in [power_id::DUPLICATION, power_id::BURST] {
                if combat.player.power(id) > 0 {
                    combat.player.consume_power(id);
                    plays += 1;
                }
            }
            let energy = if card.upgrades > 0 { 3 } else { 2 };
            for _ in 0..plays {
                combat.queue.push(Pending {
                    effect: Effect::Energy(energy),
                    context: Context::player(),
                });
            }
            return;
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
        // Rocket Punch costs 0 until played once its owner creates a Status card.
        if content.cards[card as usize].card_type == CardType::Status
            && !self.combat().unwrap().enemy_turn
            && let Some(rocket_punch) = content.card_id("CARD.ROCKET_PUNCH")
        {
            let combat = self.combat_mut().unwrap();
            for pile in [
                &mut combat.hand,
                &mut combat.draw,
                &mut combat.discard,
                &mut combat.exhaust,
            ] {
                for other in pile.iter_mut().filter(|other| other.id == rocket_punch) {
                    other.cost_override = Some(0);
                    other.when_played = true;
                }
            }
        }
        if content.cards[card as usize].tags & SHIV_TAG != 0
            && self.creature(Actor::Player).power(power_id::PHANTOM_BLADES) > 0
        {
            self.retain_shivs(content);
        }
        // Cards added once the combat is ending fire no generation hooks.
        if self.enemies_defeated() {
            return;
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
                lethal: false,
                hook: false,
                off_pile: false,
                target_instance: None,
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
                let unfinished = if combat.auto_plays.is_empty() {
                    0
                } else {
                    combat.auto_plays.len() as i16 - 1 + combat.playing.is_some() as i16
                };
                combat.total_cards - combat.pending_plays.len() as i16 - unfinished
            }
            Scale::CardsPlayed => combat.history.cards,
            Scale::AttacksPlayed => combat.history.attacks,
            // Finished attack plays, the playing series' earlier ones included (Duplication).
            Scale::PriorAttacks => {
                self.finished_plays(content, CardType::Attack)
                    + combat
                        .playing
                        .filter(|card| {
                            card.card_type(content.cards[card.id as usize]) == CardType::Attack
                        })
                        .map_or(0, |_| {
                            combat.card_plays as i16 - 1 - combat.pending_plays.len() as i16
                        })
            }
            // Lunar Blast counts finished skill plays, so skills still resolving don't count.
            Scale::SkillsPlayed => self.finished_plays(content, CardType::Skill),
            // Helix Drill leaves out its own cost while it's being played.
            Scale::EnergySpent => {
                combat.history.energy
                    - combat
                        .auto_plays
                        .last()
                        .map(|play| play.card)
                        .or(combat.playing)
                        .map_or(0, |card| {
                            card_cost(card, content.cards[card.id as usize], combat.energy)
                        })
            }
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
                // Cards waiting in the play pile (Cascade's picks, played cards not yet placed)
                // count too; the card being played is counted below.
                let playing = combat.playing.map(|card| card.instance);
                let play_pile = combat
                    .queue
                    .iter()
                    .filter_map(|pending| match pending.effect {
                        Effect::AutoPlay(card) | Effect::PlacePlayed(card, ..)
                            if card.flags & DUPE == 0
                                && Some(card.instance) != playing
                                && !combat
                                    .auto_plays
                                    .iter()
                                    .any(|play| play.card.instance == card.instance) =>
                        {
                            Some(card)
                        }
                        _ => None,
                    });
                // So do plays still resolving under the current one (Pommel Strike whose draw let
                // Hellraiser auto-play this card).
                let current = combat
                    .auto_plays
                    .last()
                    .map(|play| play.card.instance)
                    .or(playing);
                let resolving = combat
                    .auto_plays
                    .iter()
                    .map(|play| play.card)
                    .chain(combat.playing)
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
                    .chain(combat.playing.iter())
                    .chain(combat.auto_plays.iter().map(|play| &play.card))
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
                let combat = self.combat().unwrap();
                let target = &combat.enemies[x as usize].creature;
                let reattach_alive = target.power(power_id::REATTACH) > 0
                    && combat.enemies.iter().enumerate().any(|(i, enemy)| {
                        i != x as usize
                            && enemy.creature.hp > 0
                            && enemy.creature.power(power_id::REATTACH) > 0
                    });
                combat.last_killed
                    && !(context.card.is_some_and(|card| {
                        matches!(
                            content.cards[card as usize].id,
                            "CARD.FEED" | "CARD.THE_HUNT" | "CARD.HAND_OF_GREED"
                        )
                    }) && (target.power(power_id::MINION) > 0 || reattach_alive))
            }),
            Condition::FirstTurn => self.combat().unwrap().turn == 1,
            Condition::HasOrb(id) => self.combat().unwrap().orbs.iter().any(|orb| orb.id == id),
            Condition::CardType(kind) => context
                .card
                .is_some_and(|id| content.cards[id as usize].card_type == kind),
            // FTL counts finished plays: not the card playing it (Beat Down) nor itself.
            Condition::PriorCardsBelow(count) => {
                let combat = self.combat().unwrap();
                let unfinished = combat
                    .playing
                    .map_or(0, |_| combat.pending_plays.len() as i16 + 1)
                    + combat
                        .auto_plays
                        .iter()
                        .map(|auto| auto.plays as i16)
                        .sum::<i16>();
                combat.history.cards - unfinished < count[context.upgraded as usize] as i16
            }
            Condition::TargetAttacking => context.target.is_some_and(|target| {
                let enemy = &self.combat().unwrap().enemies[target];
                let def = &content.enemies[enemy.creature.id as usize];
                !enemy.stunned
                    && (def.moves[enemy.move_index].intent.starts_with("Attack")
                        || effects_attack(def.moves[enemy.move_index].effects))
            }),
            Condition::XAtLeast(value) => context.x >= value,
            Condition::OstyAlive => self.creature(Actor::Osty).hp > 0,
            Condition::DoomApplied => self.combat().unwrap().history.doom_applied > 0,
            Condition::Card(id) => context.card == Some(id),
        }
    }

    fn targets(&mut self, content: &Content, target: Target, context: Context) -> Vec<Actor> {
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
            // Reviving enemies cannot receive powers.
            Target::ChosenEnemyOrDead => context
                .target
                .filter(|&index| {
                    self.combat()
                        .unwrap()
                        .enemies
                        .get(index)
                        .is_some_and(|enemy| {
                            enemy.creature.hp > 0
                                || [power_id::ILLUSION, power_id::REATTACH, power_id::ADAPTABLE]
                                    .iter()
                                    .all(|&power| enemy.creature.power(power) == 0)
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
                    return if alive {
                        vec![Actor::Enemy(target)]
                    } else {
                        vec![]
                    };
                }
                let alive: Vec<_> = self
                    .alive_in_slot_order(content)
                    .into_iter()
                    .map(Actor::Enemy)
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
                    self.combat_mut().unwrap().history.exhausted += 1;
                    self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
                    self.trigger_card(content, card, Trigger::CardExhausted);
                }
                return true;
            }
            // CardCmd.Upgrade does nothing once combat is ending.
            CardOp::Upgrade if self.enemies_defeated() => {}
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
                self.creature_mut(Actor::Player)
                    .add_power(power_id::NIGHTMARE, count as i16);
            }
            // Combat transforms count as generating the new card (Regalite, ...).
            CardOp::Transform(id, upgrades) => {
                let mut card = Card {
                    id,
                    upgrades,
                    ..Card::default()
                };
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
                let razor_tooth = self.razor_tooth(content, card);
                for _ in 1..count {
                    let mut replay = card;
                    replay.flags |= REPLAY;
                    if razor_tooth {
                        replay.upgrades = 1;
                    }
                    if card.until_played || card.when_played {
                        replay.cost_override = None;
                        replay.until_played = false;
                        replay.when_played = false;
                    }
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AutoPlay(replay),
                        context,
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
                self.afflict_new(content, &mut card);
                self.add_to_hand(card);
                self.trigger_generated(content, card.id);
                return true;
            }
            CardOp::Transfigure => {
                let card = &mut self.pile_mut(pile)[index];
                if content.cards[card.id as usize].cost[card.upgrades.min(1) as usize] >= 0 {
                    card.cost_delta = card.cost_delta.saturating_add(1);
                    // The +1 lands on top of an absolute cost set earlier (Mummified Hand's 0).
                    if card.free {
                        card.free = false;
                        card.cost_override = Some(1);
                        card.until_played = true;
                    } else if let Some(cost) = card.cost_override {
                        card.cost_override = Some(cost.saturating_add(1));
                    }
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
                card.free = true;
                self.push_pile(to, card);
                return true;
            }
            CardOp::FreeCombat => self.pile_mut(pile)[index].flags |= FREE_COMBAT,
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
            self.combat_mut().unwrap().hand[choice].free = true;
        }
    }

    /// Relic hooks run in relic order, so the earlier-held of two relics acts first.
    fn relic_first(&self, content: &Content, first: &str, second: &str) -> bool {
        let position = |id| {
            content
                .relic_id(id)
                .and_then(|id| self.run.relics.iter().position(|&relic| relic == id))
        };
        position(first) < position(second)
    }

    /// The card as its `play`th play's AfterCardPlayed hooks see it: Juggling (a power) runs
    /// before Razor Tooth (a relic), which upgraded the card after its first play.
    fn sharpened(&self, content: &Content, mut card: Card, play: usize) -> Card {
        if play > 1 && self.razor_tooth(content, card) {
            card.upgrades = 1;
        }
        card
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
            let combat = self.combat_mut().unwrap();
            let piles = [
                &mut combat.discard,
                &mut combat.draw,
                &mut combat.hand,
                &mut combat.exhaust,
            ];
            for pile in piles {
                if let Some(index) = pile
                    .iter()
                    .rposition(|other| other.instance == card.instance && other.id == card.id)
                {
                    card = pile.remove(index);
                    // Each play picks its own result pile.
                    card.turn_flags &= !EXHAUST;
                    break;
                }
            }
        }
        let def = content.cards[card.id as usize];
        // History Course's Stomp copy is cloned now, with this turn's attack discounts.
        if def.id == "CARD.STOMP" && card.flags & (REPLAY | DUPE) == REPLAY | DUPE {
            card.value = self.combat().unwrap().history.attacks;
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
            let alive = self.alive_in_slot_order(content);
            if card.target(def) == Target::ChosenEnemy
                && !fan_shiv
                && context.target.is_none()
                && !alive.is_empty()
            {
                let choice = if self.expectation.is_some() {
                    self.expectation_choice(alive.len()).unwrap_or(0)
                } else {
                    self.rngs.combat_targets.below(alive.len() as u32) as usize
                };
                context.target = Some(alive[choice]);
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
            && self.alive_in_slot_order(content).is_empty();
        // Normality, Velvet Choker, Sloth and Ringing stop auto-plays too (Hook.ShouldPlay).
        let sloth = combat.player.power(power_id::SLOTH);
        // Velvet Choker counts finished plays: not the card this auto-play happens inside.
        let finished = combat.history.cards
            - combat
                .playing
                .map_or(0, |_| combat.pending_plays.len() as i16 + 1)
            - combat
                .auto_plays
                .iter()
                .map(|auto| auto.plays as i16)
                .sum::<i16>();
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
                combat.history.exhausted += 1;
                self.trigger(content, Trigger::CardExhausted, Actor::Player, 1);
                self.trigger_card(content, card, Trigger::CardExhausted);
            } else {
                self.combat_mut().unwrap().discard.push(card);
            }
            return;
        }
        if flags & DUPE == 0 {
            self.combat_mut().unwrap().bound_played |= card.affliction == Affliction::Bound;
        }
        let card_type = card.card_type(def);
        let throwing_axe =
            self.has_relic(content, "RELIC.THROWING_AXE") && !self.combat().unwrap().throwing_axe;
        self.combat_mut().unwrap().throwing_axe |= throwing_axe;
        if card_type == CardType::Attack
            && self.has_relic(content, "RELIC.MUSIC_BOX")
            && !self.combat().unwrap().music_box
            && self.combat().unwrap().music_box_card.is_none()
        {
            self.combat_mut().unwrap().music_box_card = Some(card.instance);
        }
        let pen_nib = card_type == CardType::Attack && self.has_relic(content, "RELIC.PEN_NIB");
        let mut pen_nib_counter = self.pen_nib;
        let razor_tooth = self.razor_tooth(content, card);
        if card.target(def) == Target::ChosenEnemy && !fan_shiv && context.target.is_none() {
            let alive = self.alive_in_slot_order(content);
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
        context.target_instance = context
            .target
            .map(|slot| self.combat().unwrap().enemies[slot].instance);
        context.x = if def.cost[card.upgrades.min(1) as usize] < 0 {
            self.combat().unwrap().energy + 2 * self.has_relic(content, "RELIC.CHEMICAL_X") as i16
        } else {
            0
        };
        // A star X-cost auto-play counts the current stars without spending them (Stardust).
        if def.star_cost[card.upgrades.min(1) as usize] == -2 {
            context.event = self.combat().unwrap().stars
                + 2 * self.has_relic(content, "RELIC.CHEMICAL_X") as i16;
        }
        if matches!(card_type, CardType::Attack | CardType::Skill)
            && self.combat().unwrap().history.attacks + self.combat().unwrap().history.skills
                < self.creature(Actor::Player).power(power_id::NOSTALGIA)
        {
            card.flags |= RETURN_TO_DRAW;
        }
        // An auto-play's energy value is its modified cost (Borrowed Time, Corruption, ...), but
        // Void Form only frees cards in the hand or play pile (Mayhem, Havoc and Cascade move
        // theirs there first).
        let spiked_gauntlets = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
        let combat = self.combat().unwrap();
        // Brilliant Scarf frees cards in the hand or play pile while its 5th manual play resolves
        // (an X cost still counts the current energy).
        let finished_manual = combat.history.manual_plays
            - combat
                .playing
                .map_or(0, |_| combat.pending_plays.len() as i16 + 1);
        let energy = if self.has_relic(content, "RELIC.BRILLIANT_SCARF")
            && finished_manual == 4
            && !context.off_pile
            && def.cost[card.upgrades.min(1) as usize] >= 0
        {
            0
        } else if context.off_pile
            && combat.history.manual_cards < combat.player.power(power_id::VOID_FORM)
        {
            card_cost(card, def, combat.energy) + combat.player.power(power_id::BORROWED_TIME)
        } else {
            energy_cost(combat, card, def, spiked_gauntlets, !context.off_pile)
        };
        // The energy value is taken where the card sits; then it moves to the play pile, where
        // BeforeCardPlayed spends a Free Attack, Skill or Power, and a Veilpiercer if Ethereal.
        let veil = is_ethereal(combat, card, def);
        if let Some(free) = match card_type {
            CardType::Attack => Some(power_id::FREE_ATTACK),
            CardType::Skill => Some(power_id::FREE_SKILL),
            CardType::Power => Some(power_id::FREE_POWER),
            _ => None,
        } {
            self.creature_mut(Actor::Player).consume_power(free);
        }
        // BeforeCardPlayed sees an auto-play's cost as its energy value (Intimidating Helmet),
        // and Danse Macabre its resolved cost (an X cost captures the current energy).
        let x = def.cost[card.upgrades.min(1) as usize] < 0;
        let before_blocks = self.before_play_blocks(
            content,
            card,
            energy,
            if x {
                self.combat().unwrap().energy
            } else {
                energy
            },
        );
        card.turn_flags |= card.flags(def) & EXHAUST;
        if card.card_type(def) == CardType::Skill
            && self.creature(Actor::Player).power(power_id::CORRUPTION) > 0
        {
            card.turn_flags |= EXHAUST;
        }
        let combat = self.combat_mut().unwrap();
        combat.mark_rebound(def, &card);
        combat.mark_feral(card_type, energy, &card);
        if veil {
            combat.player.consume_power(power_id::VEILPIERCER);
        }
        if card_type == CardType::Attack {
            combat.discount_stomps(content);
        }
        let throne = combat.player.power(power_id::THE_SEALED_THRONE);
        let duplicate = (combat.player.power(power_id::DUPLICATION) > 0) as u8;
        if duplicate > 0 {
            combat.player.consume_power(power_id::DUPLICATION);
        }
        // Play-count powers (ModifyCardPlayCount) apply to auto-plays too.
        let mut extra = 0;
        for (kind, id) in [
            (CardType::Skill, power_id::BURST),
            (CardType::Attack, power_id::ONE_TWO_PUNCH),
            (CardType::Power, power_id::SIGNAL_BOOST),
        ] {
            if card_type == kind && combat.player.power(id) > 0 {
                combat.player.consume_power(id);
                extra += 1;
            }
        }
        if throne > 0 {
            combat.stars = combat.stars.saturating_add(throne);
            combat.history.stars_gained = combat.history.stars_gained.saturating_add(throne);
        }
        let plays = 1
            + throwing_axe as u8
            + card.replays
            + matches!(card.enchantment, Some(Enchantment::Spiral)) as u8
            + (card.enchantment == Some(Enchantment::Glam) && card.enchantment_value > 0) as u8
            + (combat.history.first_plays < combat.player.power(power_id::ECHO_FORM)) as u8
            + duplicate
            + extra;
        combat.auto_plays.push(AutoPlay {
            card,
            powers: combat.player.powers.clone(),
            enemy_powers: combat
                .enemies
                .iter()
                .map(|enemy| enemy.creature.powers.clone())
                .collect(),
            plays,
            counted: 0,
            energy,
        });
        combat.history.cards += plays as i16;
        combat.history.first_plays += 1;
        combat.history.zero_attacks +=
            (card_type == CardType::Attack && energy == 0) as i16 * plays as i16;
        combat.total_cards += plays as i16;
        let first_attack = card_type == CardType::Attack && combat.history.attacks == 0;
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
            if play + 1 < plays {
                combat.queue.push(Pending {
                    effect: Effect::AutoPlayStep,
                    context,
                });
            }
            let context = Context {
                pen_nib: pen_nibs[play as usize],
                lethal: first_attack && play == 0,
                upgraded: context.upgraded || razor_tooth && play > 0,
                ..context
            };
            // Each play's enchantment acts after the card's own effects.
            let amount = card.enchantment_amount;
            let enchantment = match card.enchantment {
                Some(Enchantment::Adroit) => {
                    Some(Effect::Block(Target::Player, Amount::fixed(amount, amount)))
                }
                Some(Enchantment::Corrupted) => {
                    Some(Effect::LoseHp(Target::Player, Amount::fixed(2, 2)))
                }
                Some(Enchantment::Swift) if play == 0 && card.enchantment_value > 0 => {
                    combat.auto_plays.last_mut().unwrap().card.enchantment_value = 0;
                    Some(Effect::Draw(amount.max(0) as u8))
                }
                Some(Enchantment::Sown) if play == 0 && card.enchantment_value > 0 => {
                    combat.auto_plays.last_mut().unwrap().card.enchantment_value = 0;
                    Some(Effect::Energy(amount as i8))
                }
                _ => None,
            };
            if let Some(effect) = enchantment {
                combat.queue.push(Pending { effect, context });
            }
            // Fetch draws after Osty's attack on its first finished play this turn.
            if play == 0
                && card.id == card_id::FETCH
                && card.turn_flags & FETCHED == 0
                && combat.osty.hp > 0
            {
                combat.auto_plays.last_mut().unwrap().card.turn_flags |= FETCHED;
                combat.queue.push(Pending {
                    effect: Effect::Draw(1),
                    context,
                });
            }
            let fan_shiv =
                def.id == "CARD.SHIV" && combat.player.power(power_id::FAN_OF_KNIVES) > 0;
            push_play(&mut combat.queue, card, def, fan_shiv, context);
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
            for block in before_blocks.into_iter().filter(|&block| block > 0) {
                combat.queue.push(Pending {
                    effect: Effect::RawBlock(Target::Player, Amount::fixed(block, block)),
                    context,
                });
            }
            // The Sealed Throne's BeforeCardPlayed stars come again before each later play.
            if play > 0 && throne > 0 {
                combat.queue.push(Pending {
                    effect: Effect::Stars(Amount::fixed(throne, throne)),
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

    /// CardPlayFinished bookkeeping, before the AfterCardPlayed hooks.
    pub(super) fn finish_card(&mut self, content: &Content, mut card: Card) -> Card {
        let def = content.cards[card.id as usize];
        if matches!(def.id, "CARD.BOLAS" | "CARD.THRUMMING_HATCHET") {
            card.value = 1;
        }
        if card.card_type(def) == CardType::Skill
            && self.creature(Actor::Player).power(power_id::MASTER_PLANNER) > 0
        {
            card.flags |= SLY;
        }
        // Only "until played" costs wear off once played; combat-long ones (Confused, Slither,
        // Touch of Insanity) stay.
        if card.until_played || card.when_played {
            card.cost_override = None;
            card.until_played = false;
            card.when_played = false;
        }
        card.free = false;
        let combat = self.combat_mut().unwrap();
        if is_ethereal(combat, card, def) {
            combat.history.ethereal += 1;
            // Each finished ethereal play makes every Banshee's Cry 2 cheaper this combat.
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
                other.cost_delta = other.cost_delta.saturating_sub(2);
            }
        }
        if def.tags & SHIV_TAG != 0 {
            combat.history.shivs += 1;
        }
        card
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
        let combat = self.combat_mut().unwrap();
        // A dupe (History Course's copy) has no result pile, but Feral still sends it to the hand
        // and Corruption still exhausts a skill.
        let dupe = card.flags & DUPE != 0
            && !(card_type == CardType::Skill && combat.player.power(power_id::CORRUPTION) > 0);
        let exhausted = if replay && card.flags & DUPE == 0 {
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
            false
        } else if card.flags & DUPE != 0 {
            // Corruption's skill dupe (DUPE shares RETURN_TO_HAND's bit).
            combat.exhaust.push(card);
            true
        } else if flags & RETURN_TO_HAND != 0 {
            if combat.hand.len() < 10 {
                combat.hand.push(card);
            } else {
                combat.discard.push(card);
            }
            false
        } else if card_type != CardType::Power && flags & EXHAUST != 0 {
            card.flags &= !RETURN_TO_DRAW;
            combat.exhaust.push(card);
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
        };
        if exhausted {
            combat.history.exhausted += 1;
        }
        exhausted
    }

    /// Queues a finished play's card-played hooks in the game's listener order: the player's
    /// powers, then relics (the `plays` counted by relic counters), then enemies.
    /// Rainbow Ring fires once per turn after a play finishes with an attack, a skill and a power
    /// all finished this turn (AfterCardPlayed), so mid-series once that play completes the set.
    /// `finished` counts plays of `card_type` that finished in a series still marked as playing.
    fn rainbow_ring(&mut self, content: &Content, card_type: CardType, finished: i16) {
        if !self.combat().unwrap().rainbow_ring
            && self.has_relic(content, "RELIC.RAINBOW_RING")
            && [CardType::Attack, CardType::Skill, CardType::Power]
                .into_iter()
                .all(|kind| {
                    self.finished_plays(content, kind)
                        + if kind == card_type { finished } else { 0 }
                        > 0
                })
        {
            self.combat_mut().unwrap().rainbow_ring = true;
            self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
            self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
        }
    }

    fn queue_card_triggers(
        &mut self,
        content: &Content,
        card: Card,
        powers: Vec<Power>,
        enemies: Vec<Vec<Power>>,
        plays: u8,
    ) -> Context {
        self.tender();
        let trigger = match card.card_type(content.cards[card.id as usize]) {
            CardType::Attack => Some(Trigger::AttackPlayed),
            CardType::Skill => Some(Trigger::SkillPlayed),
            CardType::Power => Some(Trigger::PowerPlayed),
            _ => None,
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
        for index in std::mem::take(&mut self.combat_mut().unwrap().curled) {
            let creature = &mut self.combat_mut().unwrap().enemies[index].creature;
            let curl = creature.power(power_id::CURL_UP);
            creature
                .powers
                .retain(|power| power.id != power_id::CURL_UP);
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::RawBlock(Target::Source, Amount::fixed(curl, curl)),
                context: Context {
                    source: Actor::Enemy(index),
                    target: Some(index),
                    hook: true,
                    ..context
                },
            });
        }
        // The queue is a stack: enemies' hooks are pushed first so the player's resolve first.
        for (index, powers) in enemies.into_iter().enumerate().rev() {
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
        if plays > 0 {
            let card_type = card.card_type(content.cards[card.id as usize]);
            self.count_finished_play(content, card_type, plays as i16);
        }
        if let Some(copy) = self.combat_mut().unwrap().music_box_copy.take() {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::BurningSticks(copy),
                context,
            });
        }
        if let Some(trigger) = trigger {
            self.dispatch(content, trigger, Actor::Player, context, powers.clone());
        }
        self.dispatch(content, Trigger::CardPlayed, Actor::Player, context, powers);
        context
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

    // Uproar plays a random attack from the draw pile once its own attack resolves.
    /// Block from BeforeCardPlayed hooks, gained before each play of a card: Intimidating Helmet
    /// checks its energy value, Danse Macabre its resolved cost, Spirit of Ash its Ethereal keyword.
    fn before_play_blocks(
        &self,
        content: &Content,
        card: Card,
        energy: i16,
        cost: i16,
    ) -> [i16; 3] {
        let ethereal = is_ethereal(
            self.combat().unwrap(),
            card,
            content.cards[card.id as usize],
        );
        let player = self.creature(Actor::Player);
        [
            4 * (energy >= 2 && self.has_relic(content, "RELIC.INTIMIDATING_HELMET")) as i16,
            player.power(power_id::DANSE_MACABRE) * (cost >= 2) as i16,
            player.power(power_id::SPIRIT_OF_ASH) * ethereal as i16,
        ]
    }

    fn queue_sealed_throne(&mut self, context: Context) {
        let throne = self
            .creature(Actor::Player)
            .power(power_id::THE_SEALED_THRONE);
        if throne > 0 {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::Stars(Amount::fixed(throne, throne)),
                context,
            });
        }
    }

    fn queue_before_play_blocks(
        &mut self,
        content: &Content,
        card: Card,
        energy: i16,
        cost: i16,
        context: Context,
    ) {
        for block in self.before_play_blocks(content, card, energy, cost) {
            if block > 0 {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::RawBlock(Target::Player, Amount::fixed(block, block)),
                    context,
                });
            }
        }
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

    /// Plow breaks when HP loss drops its owner to the Plow amount: Strength goes and it is stunned.
    fn break_plow(&mut self, actor: Actor) {
        if let Actor::Enemy(index) = actor
            && self.creature(actor).power(power_id::PLOW) > 0
            && self.creature(actor).hp <= self.creature(actor).power(power_id::PLOW)
        {
            // It loses Strength and every temporary Strength power (TemporaryStrengthPower).
            self.creature_mut(actor).powers.retain(|power| {
                !matches!(
                    power.id,
                    power_id::PLOW
                        | power_id::STRENGTH
                        | power_id::FLEX_POTION
                        | power_id::FEEDING_FRENZY
                        | power_id::CRUSH_UNDER
                        | power_id::DARK_SHACKLES
                        | power_id::ENFEEBLING_TOUCH
                        | power_id::DYING_STAR
                        | power_id::MANGLE
                        | power_id::MONARCHS_GAZE_STRENGTH_DOWN
                        | power_id::PIERCING_WAIL
                        | power_id::REPTILE_TRINKET
                        | power_id::SHACKLING_POTION
                )
            });
            let enemy = &mut self.combat_mut().unwrap().enemies[index];
            enemy.move_index = 2;
            enemy.last_move = 2;
            enemy.repeats = 1;
            enemy.move_history.push(2);
        }
    }

    /// One damage command hits every target before any of their deaths resolve
    /// (CreatureCmd.Damage), so e.g. Crab Rage's block can't shield the others.
    /// Gremlin Horn resolves within AfterDeath, so its draw sees enemies whose deaths come later
    /// in the same attack as still alive.
    fn gremlin_horn(&mut self, content: &Content) {
        let base = self.combat().unwrap().queue.len();
        for _ in 0..self.relic_count(content, "RELIC.GREMLIN_HORN") {
            for effect in [Effect::Energy(1), Effect::Draw(1)] {
                self.apply(
                    content,
                    Pending {
                        effect,
                        context: Context::player(),
                    },
                );
            }
        }
        // Its draw's hooks (Speedster) resolve within AfterDeath, before later death hooks.
        while self
            .combat()
            .is_some_and(|combat| combat.choice.is_none() && combat.queue.len() > base)
        {
            let pending = self.combat_mut().unwrap().queue.pop().unwrap();
            self.apply(content, pending);
        }
    }

    /// Envenom's poison has no card source, so Unsettling Lamp doesn't double it.
    fn envenom(&mut self, content: &Content, target: Actor, poison: i16) {
        let hook = std::mem::replace(&mut self.combat_mut().unwrap().applying_hook, true);
        self.apply_power(content, target, power_id::POISON, poison);
        self.combat_mut().unwrap().applying_hook = hook;
    }

    fn resolve_deaths(&mut self, content: &Content, previous: Option<Vec<Actor>>) {
        for (target, poison) in std::mem::take(&mut self.combat_mut().unwrap().envenomed) {
            self.envenom(content, target, poison);
        }
        let dying = std::mem::replace(&mut self.combat_mut().unwrap().dying, previous);
        for actor in dying.unwrap_or_default() {
            self.died(content, actor);
        }
    }

    /// A played Maul makes every Maul hit harder, auto-plays included.
    /// Thrash gains the damage of the attack it exhausts, modified by the player's damage
    /// hooks with no target (Hook.ModifyDamage: Strength, Weak, Double Damage, Shrink).
    fn thrash_gain(&mut self, content: &Content, exhausted: Card) {
        let def = content.cards[exhausted.id as usize];
        if exhausted.card_type(def) != CardType::Attack {
            return;
        }
        let combat = self.combat().unwrap();
        let Some(thrash) = combat
            .auto_plays
            .last()
            .map(|play| play.card)
            .or(combat.playing)
        else {
            return;
        };
        if content.cards[thrash.id as usize].id != "CARD.THRASH" {
            return;
        }
        let Some(amount) = def.effects.iter().find_map(|effect| match effect {
            Effect::Attack(_, amount, _)
            | Effect::AttackMany(_, amount, _)
            | Effect::ExhaustForAttack(_, amount)
            | Effect::MoveDamage(_, amount)
            | Effect::OstyAttack(_, amount, _)
            | Effect::OstyAttackMany(_, amount, _) => Some(*amount),
            _ => None,
        }) else {
            return;
        };
        let mut damage = if exhausted.upgrades > 0 {
            amount.upgraded
        } else {
            amount.base
        };
        if amount.scale == Scale::CardValue {
            damage += exhausted.value;
        }
        let player = self.creature(Actor::Player);
        let mut ratio = (1i64, 1i64);
        if player.kind(content, PowerKind::Weak) > 0 {
            ratio.0 *= if player.power(power_id::DEBILITATE) > 0 {
                50
            } else {
                75
            };
            ratio.1 *= 100;
        }
        if player.power(power_id::DOUBLE_DAMAGE) > 0 {
            ratio.0 *= 2;
        }
        if player.power(power_id::SHRINK) != 0 {
            ratio.0 *= 70;
            ratio.1 *= 100;
        }
        let damage = ((damage + player.kind(content, PowerKind::Strength)).max(0) as i64 * ratio.0
            / ratio.1) as i16;
        let combat = self.combat_mut().unwrap();
        match combat.auto_plays.last_mut() {
            Some(play) => play.card.value += damage,
            None => combat.playing.as_mut().unwrap().value += damage,
        }
    }

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
        for other in &mut self.run.deck {
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

    fn finish_auto_play(&mut self, content: &Content) {
        let AutoPlay {
            mut card,
            powers,
            enemy_powers: enemies,
            plays,
            counted,
            energy,
        } = self.combat_mut().unwrap().auto_plays.pop().unwrap();
        let def = content.cards[card.id as usize];
        let card_type = card.card_type(def);
        let replay = card.flags & REPLAY != 0;
        card.flags &= !REPLAY;
        // An auto-play's energy value is its cost (Ivory Tile).
        if energy >= 3 && self.has_relic(content, "RELIC.IVORY_TILE") {
            self.gain_energy(plays as i16);
        }
        self.spend_enchantment(&mut card);
        self.grow_maul(&mut card);
        // Juggling (a power) clones the card before Razor Tooth (a relic) upgrades it.
        let unsharpened = card;
        if self.razor_tooth(content, card) {
            card.upgrades = 1;
        }
        self.release_rupture(content);
        let feral = self.combat_mut().unwrap().take_feral(&card);
        let card = self.finish_card(content, card);
        // An auto-play checks for an empty hand too, unless it ran inside another card or potion
        // or during the turn's setup (CheckForEmptyHand, UnceasingTop.IsValidPhase).
        let combat = self.combat_mut().unwrap();
        if combat.playing.is_none()
            && combat.auto_plays.is_empty()
            && combat.potions == 0
            && !combat.setup
        {
            combat.queue.push(Pending {
                effect: Effect::CheckEmptyHand,
                context: Context::player(),
            });
        }
        self.place_played(content, card, replay, feral);
        let skills = self.finished_plays(content, CardType::Skill);
        self.queue_return_late(card_type, (skills - plays as i16, skills), energy);
        if matches!(card_type, CardType::Attack | CardType::Skill) && card.flags & DUPE == 0 {
            self.combat_mut().unwrap().history_course = Some(card);
        }
        if self.combat().unwrap().unsettling_lamp == Some(card.id) {
            self.combat_mut().unwrap().unsettling_lamp = None;
            self.combat_mut().unwrap().unsettling_used = true;
        }
        if self.combat().unwrap().music_box_card == Some(card.instance) {
            let mut copy = card;
            // Music Box copies the card before Razor Tooth upgrades it unless Razor Tooth comes first.
            if !self.relic_first(content, "RELIC.RAZOR_TOOTH", "RELIC.MUSIC_BOX") {
                copy.upgrades = unsharpened.upgrades;
            }
            copy.instance = 0;
            copy.flags |= ETHEREAL;
            let combat = self.combat_mut().unwrap();
            combat.music_box = true;
            combat.music_box_card = None;
            // A relic hook: it follows the player's powers' hooks (Calamity).
            combat.music_box_copy = Some(copy);
        }
        if card_type == CardType::Power
            && self.has_relic(content, "RELIC.PERMAFROST")
            && !self.combat().unwrap().permafrost
        {
            self.combat_mut().unwrap().permafrost = true;
            self.gain_block(content, Actor::Player, 7, None, false);
        }
        self.spend_block_doublers(content, card.instance);
        // Juggling counts finished attack plays (AfterCardPlayed).
        if card_type == CardType::Attack {
            let attacks = self.finished_plays(content, CardType::Attack);
            // Earlier plays of the series checked as they finished (AutoPlayStep).
            if attacks == 3 {
                let copies = self
                    .creature(Actor::Player)
                    .power(power_id::JUGGLING)
                    .max(0) as u8;
                let copy = Card {
                    instance: 0,
                    flags: unsharpened.flags & !EXHAUST,
                    turn_flags: unsharpened.turn_flags & !EXHAUST,
                    ..self.sharpened(content, unsharpened, plays as usize)
                };
                self.combat_mut().unwrap().juggled = vec![copy; copies as usize];
            }
        }
        self.rainbow_ring(content, CardType::Attack, 0);
        // Earlier plays ran their AfterCardPlayed hooks as they finished (AutoPlayStep); once the
        // fight is won those steps are skipped (Calamity), but the plays still count.
        self.queue_card_triggers(content, card, powers, enemies, plays - counted);
        self.tick_panache();
    }

    /// Juggling copies the turn's third finished attack play (AfterCardPlayed); `play` counts
    /// the finished plays of the series still playing. `defer` leaves the copies for Juggling's
    /// AttackPlayed hook, so they follow earlier powers' hooks (Calamity).
    fn juggle(&mut self, content: &Content, card: Card, play: usize, defer: bool) {
        if card.card_type(content.cards[card.id as usize]) != CardType::Attack
            || self.finished_plays(content, CardType::Attack) + play as i16 != 3
        {
            return;
        }
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
            if defer {
                self.combat_mut().unwrap().juggled.push(copy);
            } else {
                self.add_generated(content, Pile::Hand, copy);
            }
        }
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
            - combat
                .playing
                .filter(|&card| of_type(card))
                .map_or(0, |_| combat.card_plays as i16)
            - combat
                .auto_plays
                .iter()
                .filter(|auto| of_type(auto.card))
                .map(|auto| auto.plays as i16)
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

    fn return_bolas(&mut self, content: &Content) {
        let mut returned = Vec::new();
        for pile in [Pile::Draw, Pile::Discard, Pile::Exhaust] {
            let mut index = 0;
            while index < cards(self.combat().unwrap(), pile).len() {
                let card = cards(self.combat().unwrap(), pile)[index];
                if matches!(
                    content.cards[card.id as usize].id,
                    "CARD.BOLAS" | "CARD.THRUMMING_HATCHET"
                ) && card.value > 0
                {
                    returned.push(self.remove_pile(pile, index));
                } else {
                    index += 1;
                }
            }
        }
        for mut card in returned {
            card.value = 0;
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

    fn add_generated(&mut self, content: &Content, pile: Pile, card: Card) {
        let card = self.add_to_pile(content, pile, card);
        self.trigger_generated(content, card.id);
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
        if content.cards[card.id as usize].id == "CARD.STOMP" {
            card.value = self.combat().unwrap().history.attacks;
        }
        if card.id == card_id::BANSHEES_CRY {
            let ethereal = self.combat().unwrap().history.ethereal;
            card.cost_delta = card
                .cost_delta
                .saturating_sub((2 * ethereal).clamp(0, 127) as i8);
        }
        if card.instance == 0 {
            card.instance = self.next_card;
            self.next_card = self.next_card.saturating_add(1);
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
        // Biiig Hug's Soot lands in the draw pile before the shuffle's draw continues.
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
        self.trigger(content, Trigger::Shuffle, Actor::Player, 0);
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

    fn enemies_defeated(&self) -> bool {
        let enemies = &self.combat().unwrap().enemies;
        enemies
            .iter()
            .all(|x| x.creature.hp <= 0 || x.creature.power(power_id::MINION) > 0)
            && enemies
                .iter()
                .all(|x| x.creature.power(power_id::ADAPTABLE) == 0)
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
            && combat.playing.is_none()
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
            self.trigger(content, Trigger::Victory, Actor::Player, 0);
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
            if self.room == Room::Elite {
                for _ in 0..self.relic_count(content, "RELIC.WAR_HAMMER") {
                    self.upgrade_random(content, 4, None, false);
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
                let mut rewards = self.rewards(content, hunts);
                rewards.gold += royalties;
                rewards.removals = removals;
                Phase::Rewards(rewards)
            } else {
                Phase::Map
            };
        }
    }
}

/// Confused sets the energy cost this combat (EnergyCost.SetThisCombat), overriding an earlier
/// SetToFreeThisCombat's energy part; its star part stays free.
fn confuse(card: &mut Card, cost: i8) {
    card.cost_override = Some(cost);
    if card.flags & FREE_COMBAT != 0 {
        card.flags &= !FREE_COMBAT;
        card.free_stars_combat = true;
    }
}
