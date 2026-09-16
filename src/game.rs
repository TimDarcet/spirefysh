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

mod combat;
mod run;

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
mod tests;
