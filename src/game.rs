use crate::foundation::{
    ANYTIME_POTIONS, AUTOMATIC_POTIONS, COMMON_RELICS, NO_COMBAT_POTIONS, RARE_POTIONS,
    RARE_RELICS, SHOP_RELICS, UNCOMMON_POTIONS, UNCOMMON_RELICS,
};
use crate::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ActionOutcome {
    pub block: f32,
    pub draw: f32,
    pub discard: f32,
    pub exhaust: f32,
    pub hp_loss: Vec<f32>,
    pub deaths: Vec<f32>,
    pub player_hp: f32,
    pub energy: f32,
    pub stars: f32,
}

impl ActionOutcome {
    fn add(&mut self, other: &ActionOutcome, weight: f32) {
        for (left, right) in [
            (&mut self.block, other.block),
            (&mut self.draw, other.draw),
            (&mut self.discard, other.discard),
            (&mut self.exhaust, other.exhaust),
            (&mut self.player_hp, other.player_hp),
            (&mut self.energy, other.energy),
            (&mut self.stars, other.stars),
        ] {
            *left += right * weight;
        }
        for (left, right) in [(&mut self.hp_loss, &other.hp_loss), (&mut self.deaths, &other.deaths)] {
            left.resize(right.len(), 0.0);
            left.iter_mut().zip(right).for_each(|(left, right)| *left += right * weight);
        }
    }
}

const MAX_EXPECTATION_BRANCHES: usize = 64;

impl Combat {
    /// GeneratePlayCount: the card's own replays, then ModifyCardPlayCount (Throwing Axe, Echo
    /// Form, Duplication, Burst, One-Two Punch, Signal Boost), whose powers spend themselves.
    fn play_count(&mut self, content: &Content, card: Card, throwing_axe: bool) -> u8 {
        let card_type = card.card_type(content.cards[card.id as usize]);
        let mut plays = 1
            + card.replays
            + (card.enchantment == Some(Enchantment::Spiral)) as u8
            + (card.enchantment == Some(Enchantment::Glam) && card.enchantment_value > 0) as u8
            + throwing_axe as u8
            + (self.history.first_plays < self.player.power(power_id::ECHO_FORM)) as u8;
        for (kind, id) in [
            (None, power_id::DUPLICATION),
            (Some(CardType::Skill), power_id::BURST),
            (Some(CardType::Attack), power_id::ONE_TWO_PUNCH),
            (Some(CardType::Power), power_id::SIGNAL_BOOST),
        ] {
            if kind.is_none_or(|kind| kind == card_type) && self.player.power(id) > 0 {
                self.player.consume_power(id);
                plays += 1;
            }
        }
        plays
    }

    /// Void Form frees cards until that many hand plays have finished this turn.
    pub(crate) fn void_free(&self) -> bool {
        let unfinished = self.plays.iter().filter(|play| play.manual).count() as i16;
        self.history.manual_cards - unfinished < self.player.power(power_id::VOID_FORM)
    }

    /// Plays started but not finished, the current one included.
    fn unfinished(&self) -> i16 {
        (self.plays.iter())
            .map(|play| (play.plays - play.done) as i16)
            .sum()
    }

    /// Adds a cost modifier to every combat card `matches` picks, as its own hook would: in the
    /// piles, being played, or picked to auto-play (still in its pile in the game, Cascade's).
    fn add_costs(&mut self, matches: impl Fn(&Card) -> bool, amount: i8, kind: u8) {
        let piles = (self.draw.iter_mut())
            .chain(&mut self.hand)
            .chain(&mut self.discard)
            .chain(&mut self.exhaust)
            .chain(self.plays.iter_mut().map(|play| &mut play.card));
        let picked = self
            .queue
            .iter_mut()
            .filter_map(|pending| match &mut pending.effect {
                Effect::AutoPlay(card) => Some(card),
                _ => None,
            });
        for card in piles.chain(picked).filter(|card| matches(card)) {
            card.add_cost(amount, kind);
        }
    }

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

/// TemporaryStrengthPower subclasses that lower Strength, debuffs in the game.
pub(crate) const TEMPORARY_STRENGTH_DOWN: [Id; 8] = [
    power_id::CRUSH_UNDER,
    power_id::DARK_SHACKLES,
    power_id::DYING_STAR,
    power_id::ENFEEBLING_TOUCH,
    power_id::MANGLE,
    power_id::MONARCHS_GAZE_STRENGTH_DOWN,
    power_id::PIERCING_WAIL,
    power_id::SHACKLING_POTION,
];

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
        Enchantment::RoyallyApproved => matches!(def.card_type, CardType::Attack | CardType::Skill),
        // CardModel.GainsBlock: Mad Science only as a skill, Sovereign Blade only with Parry.
        Enchantment::Nimble => match def.id {
            "CARD.MAD_SCIENCE" => card.card_type(def) == CardType::Skill,
            "CARD.SOVEREIGN_BLADE" => false,
            _ => effects_gain_block(def.effects),
        },
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
        Effect::Block(..)
        | Effect::RawBlock(Target::Player, _)
        | Effect::DrawBlockIf(..)
        | Effect::ExhaustForBlock(_)
        | Effect::DodgeRoll(_)
        | Effect::ToricToughness(_) => true,
        Effect::If(_, yes, no) => effects_gain_block(yes) || effects_gain_block(no),
        Effect::Repeat(_, effects) | Effect::Random(_, effects) => effects_gain_block(effects),
        _ => false,
    })
}

pub(crate) fn eligible(content: &Content, card: &&Card, filter: CardFilter, op: CardOp) -> bool {
    let def = content.cards[card.id as usize];
    // A card's own type can differ from its definition's (Mad Science).
    let card_type = card.card_type(def);
    let matches = match filter {
        CardFilter::Any => true,
        CardFilter::Type(kind) => card_type == kind,
        CardFilter::AttackOrPower => matches!(card_type, CardType::Attack | CardType::Power),
        CardFilter::TypeWithoutTurnFlag(kind, flag) => {
            card_type == kind && card.turn_flags & flag == 0
        }
        CardFilter::WithoutFlag(flag) => card.flags(def) & flag == 0,
        CardFilter::NotType(kind) => card_type != kind,
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
        CardFilter::Colorless => {
            content.colorless.contains(&card.id) || VISUALLY_COLORLESS.contains(&def.id)
        }
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
        // CostsEnergyOrStars: a positive, non-X energy or star cost.
        CardFilter::CostsResource => {
            let stars = def.star_cost[card.upgrades.min(1) as usize];
            def.cost[card.upgrades.min(1) as usize] >= 0
                && card.local_energy(base_cost(**card, def)) > 0
                || stars >= 0 && card.star_override().unwrap_or(stars as i16) > 0
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

/// Hex afflicts every card in combat with Ethereal.
pub(crate) fn is_ethereal(combat: &Combat, card: Card, def: CardDef) -> bool {
    card.flags(def) & ETHEREAL != 0 || combat.player.power(power_id::HEX) > 0
}

pub(crate) fn card_cost(card: Card, def: CardDef, energy: i16) -> i16 {
    // X-cost cards always spend all energy, whatever their cost modifiers (GetAmountToSpend).
    if def.cost[card.upgrades.min(1) as usize] < 0 {
        return energy;
    }
    card.local_energy(base_cost(card, def)).max(0)
}

/// Tezcatara's Ember lowers the base cost to 0; later cost modifiers still apply on top.
pub(crate) fn base_cost(card: Card, def: CardDef) -> i16 {
    if card.enchantment == Some(Enchantment::TezcatarasEmber) {
        0
    } else {
        def.cost[card.upgrades.min(1) as usize] as i16
    }
}

/// `in_play`: the card is in the hand or play pile, where Free Attack, Free Skill, Free Power
/// and Veilpiercer apply.
pub(crate) fn energy_cost(
    combat: &Combat,
    card: Card,
    def: CardDef,
    spiked_gauntlets: bool,
    in_play: bool,
) -> i16 {
    // X costs spend all energy before any cost modifier applies (GetAmountToSpend).
    if def.cost[card.upgrades.min(1) as usize] < 0 {
        return combat.energy;
    }
    let card_type = card.card_type(def);
    if combat.void_free()
        || in_play && card_type == CardType::Power && combat.player.power(power_id::FREE_POWER) > 0
        || in_play && card_type == CardType::Skill && combat.player.power(power_id::FREE_SKILL) > 0
        || card_type == CardType::Skill && combat.player.power(power_id::CORRUPTION) > 0
    {
        return 0;
    }
    let cost = card.local_energy(base_cost(card, def))
        + combat.player.power(power_id::BORROWED_TIME)
        + (card.affliction == Affliction::Entangled) as i16
            * combat.player.power(power_id::TANGLED)
        + (card_type == CardType::Power && spiked_gauntlets) as i16;
    // Curious only lowers a power card that still costs something.
    let curious = combat.player.power(power_id::CURIOUS);
    let cost = cost - (card_type == CardType::Power && curious > 0 && cost > 0) as i16 * curious;
    if in_play
        && (is_ethereal(combat, card, def) && combat.player.power(power_id::VEILPIERCER) > 0
            || card_type == CardType::Attack && combat.player.power(power_id::FREE_ATTACK) > 0)
    {
        0
    } else {
        cost.max(0)
    }
}

pub(crate) fn star_cost(combat: &Combat, card: Card, def: CardDef) -> i16 {
    // Star X costs always take every star, even when the card is otherwise free.
    if def.star_cost[card.upgrades.min(1) as usize] == -2 {
        return combat.stars;
    }
    if card.enchantment == Some(Enchantment::TezcatarasEmber) || combat.void_free() {
        return 0;
    }
    let base = def.star_cost[card.upgrades.min(1) as usize] as i16;
    card.star_override().unwrap_or(base).max(0)
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
    if combat.player.power(power_id::NO_ENERGY_GAIN) == 0 {
        combat.energy = combat.energy.saturating_add(gain);
    }
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

pub(crate) fn price(item: &ShopItem) -> i32 {
    match item {
        ShopItem::Card(_, x)
        | ShopItem::Relic(_, x)
        | ShopItem::Potion(_, x)
        | ShopItem::Remove(x) => *x,
    }
}

pub(crate) fn price_mut(item: &mut ShopItem) -> &mut i32 {
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
    const GROUPS: [u8; 512] = {
        let (mut groups, lists) = (
            [4; 512],
            [COMMON_RELICS, UNCOMMON_RELICS, RARE_RELICS, SHOP_RELICS],
        );
        let mut group = lists.len();
        while group > 0 {
            group -= 1;
            let mut index = 0;
            while index < lists[group].len() {
                groups[lists[group][index] as usize] = group as u8;
                index += 1;
            }
        }
        groups
    };
    GROUPS
        .get(id as usize)
        .filter(|&&group| group < 4)
        .map(|&group| group as usize)
}

/// Event and token cards drawn with the colorless frame (CardModel.VisualCardPool.IsColorless).
const VISUALLY_COLORLESS: [&str; 31] = [
    "CARD.APOTHEOSIS",
    "CARD.APPARITION",
    "CARD.BRIGHTEST_FLAME",
    "CARD.BYRD_SWOOP",
    "CARD.ENLIGHTENMENT",
    "CARD.EXTERMINATE",
    "CARD.FEEDING_FRENZY",
    "CARD.MAD_SCIENCE",
    "CARD.MAUL",
    "CARD.METAMORPHOSIS",
    "CARD.NEOWS_FURY",
    "CARD.PECK",
    "CARD.RELAX",
    "CARD.SQUASH",
    "CARD.TORIC_TOUGHNESS",
    "CARD.WISH",
    "CARD.WHISTLE",
    "CARD.DISINTEGRATION",
    "CARD.FUEL",
    "CARD.GIANT_ROCK",
    "CARD.LUMINESCE",
    "CARD.MIND_ROT",
    "CARD.MINION_DIVE_BOMB",
    "CARD.MINION_SACRIFICE",
    "CARD.MINION_STRIKE",
    "CARD.SHIV",
    "CARD.SLOTH",
    "CARD.SOUL",
    "CARD.SOVEREIGN_BLADE",
    "CARD.SWEEPING_GAZE",
    "CARD.WASTE_AWAY",
];

/// StatusCardPool and CurseCardPool in pool order: a status or curse transforms into another of
/// its pool (in combat minus Frantic Escape and Soot, which can't be generated there).
const STATUS_POOL: [&str; 12] = [
    "CARD.BECKON",
    "CARD.BURN",
    "CARD.DAZED",
    "CARD.DEBRIS",
    "CARD.FRANTIC_ESCAPE",
    "CARD.INFECTION",
    "CARD.WITHER",
    "CARD.SLIMED",
    "CARD.SOOT",
    "CARD.TOXIC",
    "CARD.VOID",
    "CARD.WOUND",
];
const CURSE_POOL: [&str; 18] = [
    "CARD.ASCENDERS_BANE",
    "CARD.BAD_LUCK",
    "CARD.CLUMSY",
    "CARD.CURSE_OF_THE_BELL",
    "CARD.DEBT",
    "CARD.DECAY",
    "CARD.DOUBT",
    "CARD.ENTHRALLED",
    "CARD.FOLLY",
    "CARD.GREED",
    "CARD.GUILTY",
    "CARD.INJURY",
    "CARD.NORMALITY",
    "CARD.POOR_SLEEP",
    "CARD.REGRET",
    "CARD.SHAME",
    "CARD.SPORE_MIND",
    "CARD.WRITHE",
];

/// CardFactory.GetDefaultTransformationOptions: the card's own pool (a basic card's is its
/// character's; colorless for event, ancient and token cards), Common to Rare unless it is a
/// status or curse.
pub(crate) fn transform_options(content: &Content, old: Id) -> Vec<Id> {
    let def = content.cards[old as usize];
    let pool: Vec<Id> = match def.card_type {
        CardType::Status => STATUS_POOL
            .iter()
            .filter_map(|id| content.card_id(id))
            .collect(),
        CardType::Curse => CURSE_POOL
            .iter()
            .filter_map(|id| content.card_id(id))
            .collect(),
        _ if matches!(def.rarity, CardRarity::Event | CardRarity::Ancient) => {
            content.colorless().to_vec()
        }
        _ => content
            .characters
            .iter()
            .find(|character| {
                character.cards.contains(&old) || character.deck.iter().any(|&(id, _)| id == old)
            })
            .map_or_else(
                || content.colorless().to_vec(),
                |character| character.cards.to_vec(),
            ),
    };
    let any_rarity = matches!(def.card_type, CardType::Status | CardType::Curse);
    pool.into_iter()
        .filter(|&id| {
            id != old
                && (any_rarity
                    || matches!(
                        content.cards[id as usize].rarity,
                        CardRarity::Common | CardRarity::Uncommon | CardRarity::Rare
                    ))
        })
        .collect()
}

/// The Fabricator's defensive bots, then its aggressive ones.
pub(crate) const FABRICATOR_BOTS: [&str; 4] = [
    "MONSTER.GUARDBOT",
    "MONSTER.NOISEBOT",
    "MONSTER.ZAPBOT",
    "MONSTER.STABBOT",
];

/// Relics that raise max energy for the whole run.
pub(crate) const ENERGY_RELICS: [&str; 9] = [
    "RELIC.BLESSED_ANTLER",
    "RELIC.BLOOD_SOAKED_ROSE",
    "RELIC.ECTOPLASM",
    "RELIC.PHILOSOPHERS_STONE",
    "RELIC.PRISMATIC_GEM",
    "RELIC.SOZU",
    "RELIC.SPIKED_GAUNTLETS",
    "RELIC.VELVET_CHOKER",
    "RELIC.WHISPERING_EARRING",
];

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

fn push_effects(queue: &mut Vec<Pending>, effects: &[Effect], context: Context) {
    queue.extend(
        effects
            .iter()
            .rev()
            .map(|&effect| Pending { effect, context }),
    );
}

fn push_card_effects(queue: &mut Vec<Pending>, card: Card, def: CardDef, context: Context) {
    if def.id == "CARD.UPROAR" {
        queue.push(Pending {
            effect: Effect::Uproar,
            context,
        });
    }
    if def.id != "CARD.MAD_SCIENCE" {
        push_effects(queue, def.effects, context);
        return;
    }
    let rider = card.variant % 10;
    let mut effects = match card.card_type(def) {
        // Violence repeats the whole attack command, so Vigor only boosts the first.
        CardType::Attack => {
            vec![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(12, 12), 1);
                if rider == 2 { 3 } else { 1 }
            ]
        }
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
