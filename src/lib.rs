//! Deterministic Slay the Spire 2 simulation.
//!
//! The learning module exposes an optional player-visible value model.

#[cfg(feature = "python")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{collections::HashSet, ops::RangeInclusive};

mod foundation;
mod game;
mod learning;
mod replay;

pub use foundation::{foundation_content, foundation_game};
pub use learning::ValueModel;
pub use replay::{Divergence, ReplayReport, replay_trace};

pub type Id = u16;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Pile {
    Draw,
    Hand,
    Discard,
    Exhaust,
    Offer,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Target {
    Source,
    Player,
    Osty,
    ChosenEnemy,
    ChosenEnemyOrDead,
    AllEnemies,
    OtherEnemies,
    RandomEnemy,
    LowestHpEnemy,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CardType {
    Attack,
    Skill,
    Power,
    Status,
    Curse,
    Quest,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CardRarity {
    Basic,
    Common,
    Uncommon,
    Rare,
    Ancient,
    Event,
    Quest,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Enchantment {
    Adroit,
    Clone,
    Corrupted,
    Glam,
    Goopy,
    Imbued,
    Inky,
    Instinct,
    Momentum,
    Nimble,
    PerfectFit,
    RoyallyApproved,
    Sharp,
    Slither,
    SlumberingEssence,
    SoulsPower,
    Sown,
    Spiral,
    Steady,
    Swift,
    TezcatarasEmber,
    Vigorous,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum PowerKind {
    Other,
    OneShot(Trigger),
    Strength,
    Dexterity,
    Weak,
    Vulnerable,
    Frail,
    Intangible,
    Artifact,
    Poison,
    Thorns,
    Focus,
    NoDraw,
    BlockRetain,
    BlockRetainOnce,
    Buffer,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Trigger {
    CombatStart,
    TurnStart,
    TurnEnd,
    CardPlayed,
    AttackPlayed,
    SkillPlayed,
    PowerPlayed,
    Attacked,
    Damaged,
    Blocked,
    BlockGained,
    HpLost,
    CardDrawn,
    CardGenerated,
    CardExhausted,
    Shuffle,
    EnemyDied,
    Victory,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Scale {
    None,
    X,
    Block,
    Energy,
    MissingHp,
    DrawSize,
    DiscardSize,
    ExhaustSize,
    ExhaustId(Id),
    HandSize,
    CardsPlayed,
    AttacksPlayed,
    PriorAttacks,
    SkillsPlayed,
    EnergySpent,
    Exhausted,
    HpLost,
    HpLossEvents,
    CardValue,
    LivingEnemies,
    Orbs,
    OrbTypes,
    OrbTypesPower(Id),
    Stars,
    TargetPower(Id),
    TargetPowerDiv(Id, i16),
    Tagged(u16),
    Power(Id),
    Event,
    EventPower(Id),
    HandType(CardType),
    EnemyPowerTotal(Id),
    TargetDebuffs,
    Discarded,
    DrawnCombat,
    StarCards,
    StarsGained,
    Generated,
    PriorTargetHits,
    OstyHp,
    OstyMaxHp,
    OstyAttacks,
    EtherealPlayed,
    LightningChanneled,
    LastDamage,
    ExtraDrawn,
    TurnDiv(i16),
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Amount {
    pub base: i16,
    pub upgraded: i16,
    pub ascension: u8,
    pub scale: Scale,
    pub multiplier: i16,
    pub divisor: i16,
}

impl Amount {
    pub const fn fixed(base: i16, upgraded: i16) -> Self {
        Self {
            base,
            upgraded,
            ascension: 0,
            scale: Scale::None,
            multiplier: 0,
            divisor: 1,
        }
    }

    pub const fn scaled(scale: Scale, multiplier: i16) -> Self {
        Self {
            base: 0,
            upgraded: 0,
            ascension: 0,
            scale,
            multiplier,
            divisor: 1,
        }
    }

    pub const fn ascended(base: i16, ascended: i16) -> Self {
        Self {
            ascension: 9,
            ..Self::fixed(base, ascended)
        }
    }

    pub const fn tough(base: i16, ascended: i16) -> Self {
        Self {
            ascension: 8,
            ..Self::fixed(base, ascended)
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Condition {
    Always,
    Upgraded,
    TargetAlive,
    TargetHasPower(Id),
    SourceHasPower(Id),
    HandAtMost(u8),
    HandAtLeast(u8),
    HandWithout(CardType),
    HandEmpty,
    EventAtLeast(i16),
    ExhaustAtLeast(u8),
    ExhaustedThisTurn,
    HpLostThisTurn,
    TargetDead,
    FirstTurn,
    HasOrb(Id),
    CardType(CardType),
    PriorCardsBelow([u8; 2]),
    TargetAttacking,
    XAtLeast(i16),
    OstyAlive,
    DoomApplied,
    Card(Id),
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum PlayCondition {
    Always,
    AttacksOnly,
    EmptyDrawPile,
    OstyAlive,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CardFilter {
    Any,
    Type(CardType),
    AttackOrPower,
    TypeWithoutTurnFlag(CardType, u16),
    WithoutFlag(u16),
    NotType(CardType),
    Id(Id),
    Cost(i8),
    PlayableCost(i8),
    Flag(u16),
    Upgradable,
    Colorless,
    Rare,
    NoReplay,
    PlayableOrAny,
    CostsResource,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CardOp {
    Move(Pile),
    Upgrade,
    Cost(i8),
    SetCost(i8),
    Flag(u16),
    TurnFlag(u16),
    CopyNextTurn(u8),
    Transform(Id, u8),
    AutoPlay(u8),
    CopySelected(u8),
    TakeOffer,
    Transfigure,
    Replay(u8),
    TakeFetched,
    TransformRandom,
    DiscardDraw,
    MoveFree(Pile),
    FreeCombat,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Effect {
    Attack(Target, Amount, u8),
    AttackMany(Target, Amount, Amount),
    OstyAttack(Target, Amount, u8),
    OstyAttackMany(Target, Amount, Amount),
    MoveDamage(Target, Amount),
    Damage(Target, Amount),
    Kill(Target),
    Stun(Target),
    DoomKill,
    LoseHp(Target, Amount),
    Block(Target, Amount),
    BlockNextTurn(Amount),
    DodgeRoll(Amount),
    DrawBlockIf(CardType, Amount),
    RawBlock(Target, Amount),
    Heal(Target, Amount),
    HealPercent(Target, Amount),
    MaxHp(Amount),
    Gold(Amount),
    RandomPotion,
    Bomb(Amount),
    Automation(i16),
    Panache(Amount),
    RollingBoulder(Amount),
    ToricToughness(Amount),
    ApplyPower(Target, Id, Amount),
    ApplyDebuff(Target, Id, Amount),
    StackPower(Target, Id, Amount),
    DoublePower(Target, Id, i16),
    Misery(Amount),
    RemovePower(Target, Id),
    TemporaryStrength(Target, Id, Amount),
    Draw(u8),
    HandDraw(u8),
    DrawAmount(Amount),
    DrawTo([u8; 2]),
    DrawUntilNot(CardType),
    ChooseDraw(Amount),
    FreeHand,
    Energy(i8),
    DoubleEnergy,
    AddCard(Pile, Id, u8),
    AddUpgradedCard(Pile, Id, u8),
    AddFlaggedCard(Pile, Id, u8, u16),
    RandomCard(Pile, CardType, u8, bool),
    RandomColorless(Pile, Amount, bool),
    RandomColorlessOther(Pile, Amount),
    OfferColorless(u8, bool, bool),
    OfferCharacter(u8, bool),
    OfferCharacterRetain(u8),
    OfferCharacterType(CardType, u8),
    OfferOtherCharacter(CardType, u8, bool),
    RandomCharacter(Pile, Amount, u16),
    DistinctCharacter(Pile, u8, bool),
    RandomCharacterCost0(Pile, Amount, bool),
    RandomCardOp(Pile, CardFilter, CardOp, Amount),
    AutoPlayRandom(Pile, CardFilter, Amount),
    ChooseRandomDraw(u8),
    SelectAmount(Pile, CardFilter, Amount, bool, CardOp),
    ShuffleHandDraw(u8),
    FillPotions,
    RandomizeHandCosts,
    ReplayTagged(u16, u8),
    ChannelSlots(Id),
    DistinctColorless(Pile, u8, bool),
    AddRandomCard(Id, [u8; 2], bool),
    AddRandom(Pile, Id, u8),
    FranticEscape,
    Aggression(Amount),
    AutoPlayDraw(Amount, bool),
    Stoke,
    Stampede,
    ContinueEndTurn,
    FlakCannon(Amount),
    CopyCard(Pile, u8),
    Discard(u8, bool),
    DiscardHandDraw,
    DiscardHandAdd(Id),
    PlayExhaustedShivs,
    AutoPlay(Card),
    FinishAutoPlay,
    WhisperingEarring(u8),
    MoveAll(Pile, CardFilter, Pile),
    DrawFiltered(u8, CardFilter),
    DrawFilteredStep(u8, CardFilter),
    FinishDrawFiltered(CardFilter),
    Exhaust(u8, bool),
    ExhaustForBlock(Amount),
    ExhaustForAttack(Target, Amount),
    ExhaustAttackStep(Target, Amount, u8),
    Upgrade(Pile, u8, bool),
    TransformHand(Id, [u8; 2]),
    If(Condition, &'static [Effect], &'static [Effect]),
    Repeat(Amount, &'static [Effect]),
    Random(u8, &'static [Effect]),
    Channel(Id, u8),
    RandomOrb([u8; 2]),
    Evoke(bool),
    EvokeMany(Amount),
    EvokeLast(Amount),
    EvokeAll(u8),
    PassiveFirst(u8),
    PassiveLast(u8),
    PassiveAll,
    OrbSlots(i8),
    Stars(Amount),
    Forge(Amount),
    Summon(Amount),
    MaxEnergy(i8),
    GrowCard(Amount),
    GrowDrawn(Amount),
    GrowAll(Id, Amount),
    PersistCard(Amount),
    SetCardCost(i8),
    ReduceCardCost(i8),
    CapHandCosts,
    RecycleHand([u8; 2]),
    EndTurn,
    Select(Pile, CardFilter, [u8; 2], bool, bool, CardOp),
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct Hook {
    pub trigger: Trigger,
    pub effects: &'static [Effect],
}

pub const EXHAUST: u16 = 1;
pub const ETHEREAL: u16 = 2;
pub const RETAIN: u16 = 4;
pub const INNATE: u16 = 8;
pub const UNPLAYABLE: u16 = 16;
pub const ETERNAL: u16 = 32;
pub const LIMIT_THREE: u16 = 64;
pub const SLY: u16 = 128;
pub const INKY: u16 = 256;
const REPLAY: u16 = 512;
const DUPE: u16 = 1024;
const RETURN_TO_HAND: u16 = 1024;
const RETURN_TO_DRAW: u16 = 2048;
const NO_GENERATE: u16 = 4096;
const FETCHED: u16 = 8192;
const FREE_COMBAT: u16 = 16384;
const FILTERED_DRAW: u16 = 32768;
pub const STRIKE_TAG: u16 = 1;
pub const DEFEND_TAG: u16 = 2;
pub const SHIV_TAG: u16 = 4;
pub const OSTY_ATTACK_TAG: u16 = 8;
pub const MINION_TAG: u16 = 16;

mod foundation_id {
    pub(crate) mod card {
        use crate::Id;

        pub const DAZED: Id = 58;
        pub const WOUND: Id = 60;
        pub const BURN: Id = 62;
        pub const PINPOINT: Id = 263;
        pub const SOVEREIGN_BLADE: Id = 279;
        pub const I_AM_INVINCIBLE: Id = 311;
        pub const MAKE_IT_SO: Id = 316;
        pub const BOMBARDMENT: Id = 356;
        pub const BANSHEES_CRY: Id = 360;
        pub const FETCH: Id = 388;
        pub const FLATTEN: Id = 389;
        pub const HANG: Id = 395;
        pub const MELANCHOLY: Id = 401;
        pub const RIGHT_HAND_HAND: Id = 419;
        pub const THE_SCYTHE: Id = 437;
        pub const TORIC_TOUGHNESS: Id = 518;
        pub const WITHER: Id = 521;
        pub const SLIMED: Id = 3;
        pub const TOXIC: Id = 542;
        pub const DISINTEGRATION: Id = 543;
        pub const MIND_ROT: Id = 544;
        pub const SLOTH: Id = 545;
        pub const WASTE_AWAY: Id = 546;
        pub const BECKON: Id = 548;
        pub const CORRUPTION: Id = 550;
        pub const MAUL: Id = 554;
    }

    pub(crate) mod power {
        use crate::Id;

        pub const STRENGTH: Id = 0;
        pub const DEXTERITY: Id = 1;
        pub const WEAK: Id = 2;
        pub const VULNERABLE: Id = 3;
        pub const STRANGLE: Id = 55;
        pub const FRAIL: Id = 4;
        pub const MINION: Id = 16;
        pub const INTANGIBLE: Id = 5;
        pub const ASLEEP: Id = 166;
        pub const BACK_ATTACK_LEFT: Id = 167;
        pub const BACK_ATTACK_RIGHT: Id = 269;
        pub const ADAPTABLE: Id = 164;
        pub const DISINTEGRATION: Id = 185;
        pub const CORRUPTION: Id = 175;
        pub const CRAB_RAGE: Id = 177;
        pub const DRAW_CARDS_NEXT_TURN: Id = 187;
        pub const ENRAGE: Id = 190;
        pub const FEEDING_FRENZY: Id = 192;
        pub const HATCH: Id = 204;
        pub const HARD_TO_KILL: Id = 203;
        pub const HARDENED_SHELL: Id = 202;
        pub const NEMESIS: Id = 221;
        pub const MIND_ROT: Id = 219;
        pub const PAINFUL_STABS: Id = 223;
        pub const RAVENOUS: Id = 231;
        pub const SLUMBER: Id = 244;
        pub const SKITTISH: Id = 240;
        pub const SLOTH: Id = 242;
        pub const SURROUNDED: Id = 253;
        pub const STEAM_ERUPTION: Id = 249;
        pub const STOCK: Id = 250;
        pub const SHRIEK: Id = 239;
        pub const WASTE_AWAY: Id = 266;
        pub const POISON: Id = 7;
        pub const ACCURACY: Id = 21;
        pub const FERAL: Id = 33;
        pub const FREE_POWER: Id = 35;
        pub const THUNDER: Id = 38;
        pub const SIGNAL_BOOST: Id = 39;
        pub const ECHO_FORM: Id = 40;
        pub const BLOCK_NEXT_TURN: Id = 41;
        pub const RESTORE_STRENGTH: Id = 42;
        pub const HELICAL_DART: Id = 206;
        pub const REPTILE_TRINKET: Id = 234;
        pub const FREE_SKILL: Id = 44;
        pub const BURST: Id = 47;
        pub const SHADOWMELD: Id = 49;
        pub const DOUBLE_DAMAGE: Id = 51;
        pub const ENVENOM: Id = 58;
        pub const FAN_OF_KNIVES: Id = 59;
        pub const TOOLS_OF_THE_TRADE: Id = 60;
        pub const MASTER_PLANNER: Id = 61;
        pub const PHANTOM_BLADES: Id = 62;
        pub const TRACKING: Id = 64;
        pub const OUTBREAK: Id = 65;
        pub const WELL_LAID_PLANS: Id = 66;
        pub const BLACK_HOLE: Id = 68;
        pub const CURIOUS: Id = 179;
        pub const IMPROVEMENT: Id = 213;
        pub const TIME_LIMIT: Id = 263;
        pub const CHILD_OF_THE_STARS: Id = 69;
        pub const CONQUEROR: Id = 70;
        pub const RETAIN_HAND: Id = 71;
        pub const MONARCHS_GAZE: Id = 76;
        pub const ORBIT: Id = 80;
        pub const PALE_BLUE_DOT: Id = 81;
        pub const VIGOR: Id = 83;
        pub const REFLECT: Id = 85;
        pub const ROYALTIES: Id = 86;
        pub const SEEKING_EDGE: Id = 87;
        pub const SWORD_SAGE: Id = 89;
        pub const THE_SEALED_THRONE: Id = 90;
        pub const TYRANNY: Id = 91;
        pub const VOID_FORM: Id = 92;
        pub const DOOM: Id = 94;
        pub const BORROWED_TIME: Id = 95;
        pub const CALCIFY: Id = 96;
        pub const DANSE_MACABRE: Id = 99;
        pub const DEBILITATE: Id = 100;
        pub const DEMESNE: Id = 101;
        pub const ENFEEBLING_TOUCH: Id = 103;
        pub const FORBIDDEN_GRIMOIRE: Id = 104;
        pub const FRIENDSHIP: Id = 105;
        pub const HANG: Id = 106;
        pub const LETHALITY: Id = 109;
        pub const NECRO_MASTERY: Id = 110;
        pub const OBLIVION: Id = 112;
        pub const PAGESTORM: Id = 113;
        pub const REAPER_FORM: Id = 114;
        pub const SHROUD: Id = 116;
        pub const SIC_EM: Id = 117;
        pub const SLEIGHT_OF_FLESH: Id = 118;
        pub const SPIRIT_OF_ASH: Id = 119;
        pub const VEILPIERCER: Id = 120;
        pub const COLOSSUS: Id = 122;
        pub const CRIMSON_MANTLE: Id = 123;
        pub const CRIMSON_MANTLE_DAMAGE: Id = 124;
        pub const CRUELTY: Id = 125;
        pub const NO_ENERGY_GAIN: Id = 126;
        pub const FLAME_BARRIER: Id = 127;
        pub const FOCUS: Id = 17;
        pub const FLEX_POTION: Id = 194;
        pub const FOCUSED_STRIKE: Id = 196;
        pub const HOTFIX: Id = 210;
        pub const ILLUSION: Id = 211;
        pub const IMBALANCED: Id = 212;
        pub const HELLRAISER: Id = 128;
        pub const INFERNO: Id = 129;
        pub const JUGGLING: Id = 131;
        pub const MANGLE: Id = 132;
        pub const ONE_TWO_PUNCH: Id = 133;
        pub const PYRE: Id = 134;
        pub const STAMPEDE: Id = 137;
        pub const UNMOVABLE: Id = 138;
        pub const FREE_ATTACK: Id = 139;
        pub const VICIOUS: Id = 140;
        pub const NO_BLOCK: Id = 144;
        pub const THE_GAMBIT: Id = 145;
        pub const AUTOMATION: Id = 146;
        pub const FASTEN: Id = 149;
        pub const NOSTALGIA: Id = 151;
        pub const PANACHE: Id = 152;
        pub const ROLLING_BOULDER: Id = 154;
        pub const SHRINK: Id = 156;
        pub const SPEED_POTION: Id = 248;
        pub const SLIPPERY: Id = 241;
        pub const TANGLED: Id = 257;
        pub const CLARITY: Id = 157;
        pub const CHAINS_OF_BINDING: Id = 171;
        pub const WITHERING_PRESENCE: Id = 268;
        pub const DUPLICATION: Id = 158;
        pub const GIGANTIFICATION: Id = 159;
        pub const BURROWED: Id = 170;
        pub const RITUAL: Id = 160;
        pub const CURL_UP: Id = 180;
        pub const GALVANIC: Id = 197;
        pub const PLATING: Id = 79;
        pub const SOAR: Id = 247;
        pub const CONSTRICT: Id = 173;
        pub const CRUSH_UNDER: Id = 178;
        pub const DARK_SHACKLES: Id = 182;
        pub const DAMPEN: Id = 181;
        pub const DYING_STAR: Id = 189;
        pub const ESCAPE_ARTIST: Id = 191;
        pub const FLUTTER: Id = 195;
        pub const HIGH_VOLTAGE: Id = 209;
        pub const HEX: Id = 208;
        pub const INFESTED: Id = 214;
        pub const PERSONAL_HIVE: Id = 225;
        pub const PLOW: Id = 227;
        pub const PIERCING_WAIL: Id = 226;
        pub const POSSESS_SPEED: Id = 228;
        pub const POSSESS_STRENGTH: Id = 229;
        pub const RAMPART: Id = 230;
        pub const REATTACH: Id = 232;
        pub const RINGING: Id = 235;
        pub const SANDPIT: Id = 236;
        pub const SHACKLING_POTION: Id = 238;
        pub const SLOW: Id = 243;
        pub const SMOGGY: Id = 245;
        pub const SUCK: Id = 251;
        pub const SWIPE: Id = 254;
        pub const SYNCHRONIZE: Id = 255;
        pub const SURPRISE: Id = 252;
        pub const TENDER: Id = 259;
        pub const TERRITORIAL: Id = 260;
        pub const THIEVERY: Id = 262;
        pub const HEIST: Id = 205;
        pub const TORIC_TOUGHNESS: Id = 264;
        pub const PAPER_CUTS: Id = 224;
        pub const TAINTED: Id = 267;
    }

    pub(crate) mod potion {
        use crate::Id;

        pub const ENTROPIC_BREW: Id = 20;
        pub const FAIRY_IN_A_BOTTLE: Id = 21;
        pub const FRUIT_JUICE: Id = 24;
        pub const BLOOD: Id = 45;
    }

    pub(crate) mod orb {
        use crate::Id;

        pub const LIGHTNING: Id = 0;
    }
}

use foundation_id::{card as card_id, orb as orb_id, potion as potion_id, power as power_id};

#[derive(Clone, Copy, Debug, Hash)]
pub struct CardDef {
    pub id: &'static str,
    pub card_type: CardType,
    pub rarity: CardRarity,
    pub cost: [i8; 2],
    pub star_cost: [i8; 2],
    pub target: Target,
    pub flags: [u16; 2],
    pub playable: PlayCondition,
    pub effects: &'static [Effect],
    pub hooks: &'static [Hook],
    pub tags: u16,
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct PowerDef {
    pub id: &'static str,
    pub kind: PowerKind,
    pub debuff: bool,
    pub hooks: &'static [Hook],
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct RelicDef {
    pub id: &'static str,
    pub hooks: &'static [Hook],
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct PotionDef {
    pub id: &'static str,
    pub target: Target,
    pub effects: &'static [Effect],
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct MoveDef {
    pub intent: &'static str,
    pub weight: u8,
    pub max_repeats: u8,
    pub next: &'static [usize],
    pub effects: &'static [Effect],
}

#[derive(Clone, Debug, Hash)]
pub struct EnemyDef {
    pub id: &'static str,
    pub hp: RangeInclusive<i16>,
    pub start: usize,
    pub moves: &'static [MoveDef],
    pub powers: &'static [(Id, i16)],
}

impl EnemyDef {
    fn hp(&self, ascension: u8) -> RangeInclusive<i16> {
        let profiles = match self.id {
            "MONSTER.LEAF_SLIME_S" => [11..=15, 12..=16],
            "MONSTER.LEAF_SLIME_M" => [32..=35, 33..=36],
            "MONSTER.KIN_FOLLOWER" => [58..=59, 62..=63],
            "MONSTER.KIN_PRIEST" => [190..=190, 199..=199],
            "MONSTER.AXEBOT" => [70..=78, 76..=86],
            "MONSTER.BATTLE_FRIEND_V1_0" => [75..=75, 75..=75],
            "MONSTER.BATTLE_FRIEND_V2_0" => [150..=150, 150..=150],
            "MONSTER.BATTLE_FRIEND_V3_0" => [300..=300, 300..=300],
            "MONSTER.BOWLBUG_EGG" => [21..=22, 23..=24],
            "MONSTER.BOWLBUG_NECTAR" => [35..=38, 36..=39],
            "MONSTER.BOWLBUG_ROCK" => [45..=48, 46..=49],
            "MONSTER.BOWLBUG_SILK" => [40..=43, 41..=44],
            "MONSTER.BYGONE_EFFIGY" => [127..=127, 132..=132],
            "MONSTER.BYRDONIS" => [81..=84, 90..=90],
            "MONSTER.BYRDPIP" => [9999..=9999, 9999..=9999],
            "MONSTER.CALCIFIED_CULTIST" => [38..=41, 39..=42],
            "MONSTER.CEREMONIAL_BEAST" => [252..=252, 262..=262],
            "MONSTER.CHOMPER" => [60..=64, 63..=67],
            "MONSTER.CORPSE_SLUG" => [25..=27, 27..=29],
            "MONSTER.CRUSHER" => [209..=209, 219..=219],
            "MONSTER.CUBEX_CONSTRUCT" => [65..=65, 70..=70],
            "MONSTER.DAMP_CULTIST" => [51..=53, 52..=54],
            "MONSTER.DEVOTED_SCULPTOR" => [162..=162, 172..=172],
            "MONSTER.DOORMAKER" => [489..=489, 512..=512],
            "MONSTER.AEONGLASS" => [512..=512, 535..=535],
            "MONSTER.ENTOMANCER" => [145..=145, 155..=155],
            "MONSTER.EXOSKELETON" => [24..=28, 25..=29],
            "MONSTER.EYE_WITH_TEETH" => [6..=6, 6..=6],
            "MONSTER.FABRICATOR" => [150..=150, 155..=155],
            "MONSTER.FAT_GREMLIN" => [13..=17, 14..=18],
            "MONSTER.FLAIL_KNIGHT" => [101..=101, 108..=108],
            "MONSTER.FLYCONID" => [47..=49, 51..=53],
            "MONSTER.FOGMOG" => [74..=74, 78..=78],
            "MONSTER.FOSSIL_STALKER" => [51..=53, 54..=56],
            "MONSTER.FROG_KNIGHT" => [191..=191, 199..=199],
            "MONSTER.FAKE_MERCHANT_MONSTER" => [165..=165, 175..=175],
            "MONSTER.FUZZY_WURM_CRAWLER" => [55..=57, 58..=59],
            "MONSTER.GAS_BOMB" => [7..=7, 8..=8],
            "MONSTER.GLOBE_HEAD" => [148..=148, 158..=158],
            "MONSTER.GREMLIN_MERC" => [47..=49, 51..=53],
            "MONSTER.GUARDBOT" => [16..=20, 17..=21],
            "MONSTER.HATCHLING_MINION" => [19..=21, 20..=22],
            "MONSTER.HAUNTED_SHIP" => [63..=63, 67..=67],
            "MONSTER.HUNTER_KILLER" => [121..=121, 126..=126],
            "MONSTER.INFESTED_PRISM" => [161..=161, 171..=171],
            "MONSTER.INKLET" => [11..=17, 12..=18],
            "MONSTER.KNOWLEDGE_DEMON" => [379..=379, 399..=399],
            "MONSTER.LAGAVULIN_MATRIARCH" => [222..=222, 233..=233],
            "MONSTER.LIVING_FOG" => [80..=80, 82..=82],
            "MONSTER.LIVING_SHIELD" => [55..=55, 65..=65],
            "MONSTER.LOUSE_PROGENITOR" => [134..=136, 138..=141],
            "MONSTER.MAGI_KNIGHT" => [82..=82, 89..=89],
            "MONSTER.MAWLER" => [72..=72, 76..=76],
            "MONSTER.MECHA_KNIGHT" => [300..=300, 320..=320],
            "MONSTER.MYTE" => [61..=67, 64..=69],
            "MONSTER.MYSTERIOUS_KNIGHT" => [101..=101, 108..=108],
            "MONSTER.NIBBIT" => [42..=46, 44..=48],
            "MONSTER.NOISEBOT" => [18..=23, 19..=24],
            "MONSTER.OSTY" => [1..=1, 1..=1],
            "MONSTER.OWL_MAGISTRATE" => [231..=231, 247..=247],
            "MONSTER.OVICOPTER" => [124..=130, 126..=132],
            "MONSTER.PAEL_S_LEGION" => [9999..=9999, 9999..=9999],
            "MONSTER.PARAFRIGHT" | "MONSTER.PARAFRIGHT_MINION" => [21..=21, 21..=21],
            "MONSTER.PHANTASMAL_GARDENER" => [26..=31, 27..=32],
            "MONSTER.PHROG_PARASITE" => [61..=64, 66..=68],
            "MONSTER.PUNCH_CONSTRUCT" => [55..=55, 60..=60],
            "MONSTER.QUEEN" => [400..=400, 419..=419],
            "MONSTER.ROCKET" => [199..=199, 209..=209],
            "MONSTER.ASSASSIN_RUBY_RAIDER" => [18..=23, 19..=24],
            "MONSTER.AXE_RUBY_RAIDER" => [20..=22, 21..=23],
            "MONSTER.BRUTE_RUBY_RAIDER" => [30..=33, 31..=34],
            "MONSTER.CROSSBOW_RUBY_RAIDER" => [18..=21, 19..=22],
            "MONSTER.TRACKER_RUBY_RAIDER" => [21..=25, 22..=26],
            "MONSTER.SCROLL_OF_BITING" => [30..=37, 33..=39],
            "MONSTER.SEAPUNK" => [44..=46, 47..=49],
            "MONSTER.SEWER_CLAM" => [56..=56, 58..=58],
            "MONSTER.SHRINKER_BEETLE" => [38..=40, 40..=42],
            "MONSTER.SKULKING_COLONY" => [75..=75, 80..=80],
            "MONSTER.SLIMED_BERSERKER" => [261..=261, 281..=281],
            "MONSTER.SLITHERING_STRANGLER" => [53..=55, 54..=56],
            "MONSTER.SLUDGE_SPINNER" => [37..=39, 41..=42],
            "MONSTER.SLUMBERING_BEETLE" => [86..=86, 89..=89],
            "MONSTER.SNAPPING_JAXFRUIT" => [31..=33, 34..=36],
            "MONSTER.SNEAKY_GREMLIN" => [10..=14, 11..=15],
            "MONSTER.SOUL_FYSH" => [211..=211, 221..=221],
            "MONSTER.SOUL_NEXUS" => [234..=234, 254..=254],
            "MONSTER.SPECTRAL_KNIGHT" => [93..=93, 97..=97],
            "MONSTER.SPINY_TOAD" => [116..=119, 121..=124],
            "MONSTER.STABBOT" => [18..=23, 19..=24],
            "MONSTER.TERROR_EEL" => [140..=140, 150..=150],
            "MONSTER.THE_ADVERSARY_MK_1" => [100..=100, 100..=100],
            "MONSTER.THE_ADVERSARY_MK_2" => [200..=200, 200..=200],
            "MONSTER.THE_ADVERSARY_MK_3" => [300..=300, 300..=300],
            "MONSTER.THE_ARCHITECT" => [9999..=9999, 9999..=9999],
            "MONSTER.THE_FORGOTTEN" => [106..=106, 111..=111],
            "MONSTER.THE_INSATIABLE" => [321..=321, 341..=341],
            "MONSTER.THE_LOST" => [93..=93, 99..=99],
            "MONSTER.THE_OBSCURA" => [123..=123, 129..=129],
            "MONSTER.THIEVING_HOPPER" => [79..=79, 84..=84],
            "MONSTER.TOADPOLE" => [21..=25, 22..=26],
            "MONSTER.TORCH_HEAD_AMALGAM" => [199..=199, 211..=211],
            "MONSTER.TOUGH_EGG" => [14..=18, 15..=19],
            "MONSTER.TUNNELER" => [87..=87, 92..=92],
            "MONSTER.TURRET_OPERATOR" => [41..=41, 51..=51],
            "MONSTER.TWIG_SLIME_M" => [26..=28, 27..=29],
            "MONSTER.TWIG_SLIME_S" => [7..=11, 8..=12],
            "MONSTER.TWO_TAILED_RAT" => [17..=21, 18..=22],
            "MONSTER.VANTOM" => [173..=173, 183..=183],
            "MONSTER.VINE_SHAMBLER" => [61..=61, 64..=64],
            "MONSTER.WATERFALL_GIANT" => [240..=240, 250..=250],
            "MONSTER.WRIGGLER" => [17..=21, 18..=22],
            "MONSTER.ZAPBOT" => [18..=23, 19..=24],
            "MONSTER.DECIMILLIPEDE_SEGMENT_FRONT"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_MIDDLE"
            | "MONSTER.DECIMILLIPEDE_SEGMENT_BACK" => [40..=46, 46..=52],
            "MONSTER.DECIMILLIPEDE_3_SEGMENTS" => [40..=46, 46..=52],
            "MONSTER.DOOR" => [165..=165, 165..=165],
            "MONSTER.STAGE_1"
            | "MONSTER.STAGE_2"
            | "MONSTER.STAGE_3"
            | "MONSTER.TEST_SUBJECT_C_COUNT" => [1..=1, 1..=1],
            "MONSTER.TEST_SUBJECT" => [100..=100, 111..=111],
            _ => return self.hp.clone(),
        };
        profiles[(ascension >= 8) as usize].clone()
    }

    fn powers(&self, ascension: u8) -> Vec<(Id, i16)> {
        let profiles: &[(Id, i16, i16, u8)] = match self.id {
            "MONSTER.AXEBOT" => &[(250, 2, 2, 0)],
            "MONSTER.BATTLE_FRIEND_V1_0"
            | "MONSTER.BATTLE_FRIEND_V2_0"
            | "MONSTER.BATTLE_FRIEND_V3_0" => &[(power_id::TIME_LIMIT, 3, 3, 0)],
            "MONSTER.CORPSE_SLUG" => &[(231, 4, 5, 9)],
            "MONSTER.FROG_KNIGHT" => &[(79, 15, 19, 8)],
            "MONSTER.GLOBE_HEAD" => &[(power_id::GALVANIC, 6, 6, 0)],
            "MONSTER.INFESTED_PRISM" => &[(265, 2, 3, 9)],
            "MONSTER.LOUSE_PROGENITOR" => &[(power_id::CURL_UP, 14, 18, 8)],
            "MONSTER.PHANTASMAL_GARDENER" => &[(240, 6, 7, 8)],
            "MONSTER.SLUMBERING_BEETLE" => &[(79, 15, 18, 8)],
            "MONSTER.SEWER_CLAM" => &[(power_id::PLATING, 8, 9, 8)],
            "MONSTER.TEST_SUBJECT" => &[(power_id::ENRAGE, 2, 3, 9)],
            "MONSTER.TERROR_EEL" => &[(239, 70, 75, 8)],
            "MONSTER.VANTOM" => &[(241, 8, 9, 8)],
            "MONSTER.AEONGLASS" => &[(6, 3, 3, 0), (power_id::WITHERING_PRESENCE, 6, 6, 0)],
            _ => &[],
        };
        let mut powers = self.powers.to_vec();
        for &(id, base, high, level) in profiles {
            let amount = if ascension >= level { high } else { base };
            if let Some(power) = powers.iter_mut().find(|power| power.0 == id) {
                power.1 = amount;
            } else {
                powers.push((id, amount));
            }
        }
        powers
    }
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct EncounterDef {
    pub id: &'static str,
    pub enemies: &'static [Id],
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct OrbDef {
    pub id: &'static str,
    pub timing: OrbTiming,
    pub initial_value: i16,
    pub passive_value: i16,
    pub passive_decay: i16,
    pub focus_passive: bool,
    pub passive: &'static [Effect],
    pub evoke: &'static [Effect],
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum OrbTiming {
    TurnStart,
    TurnEnd,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Room {
    Combat,
    Elite,
    Boss,
    Unknown,
    Event,
    Shop,
    Rest,
    Treasure,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Requirement {
    Always,
    Gold(i32),
    Hp(i16),
    Deck,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum RunEffect {
    Gold(i32),
    RandomGold(i32, i32),
    LoseAllGold,
    Heal(i16),
    HealPercent(u8),
    FullHeal,
    LoseHp(i16),
    MaxHp(i16),
    MaxHpTo(i16),
    AddCard(&'static str, u8),
    AddRelic(&'static str),
    AddPotion(&'static str),
    PotionRewards(&'static str, u8),
    RandomPotionReward(bool),
    NextRelics(u8),
    RelicOfRarity(u8),
    EventRelic,
    RandomCard(&'static [&'static str]),
    RandomRelic(&'static [&'static str]),
    DiscardPotion(u8),
    DiscardRandomPotion,
    RemoveCards(u8, u16),
    UpgradeCards(u8),
    UpgradeRandom(u8),
    UpgradeShuffled(u8),
    UpgradeAll,
    DowngradeRandom(u8),
    CloneDeck,
    TransformCards(Option<&'static str>, u8),
    EnchantCards(Enchantment, i16, u8, Option<CardType>),
    SkipEventRng(u8),
    ChooseCommonCards(u8, u8),
    ChooseRewardCards(u8, u8),
    RemoveRandomCard,
    Options(&'static [EventOption]),
    EventAction(u8),
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct EventOption {
    pub requirement: Requirement,
    pub effects: &'static [RunEffect],
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct EventDef {
    pub id: &'static str,
    pub options: &'static [EventOption],
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct ActDef {
    pub id: &'static str,
    pub encounters: &'static [Id],
    pub elites: &'static [Id],
    pub bosses: &'static [Id],
    pub events: &'static [Id],
    pub cards: &'static [Id],
    pub relics: &'static [Id],
    pub potions: &'static [Id],
}

#[derive(Clone, Copy, Debug, Hash)]
pub struct CharacterDef {
    pub id: &'static str,
    pub hp: i16,
    pub gold: i32,
    pub energy: u8,
    pub draw: u8,
    pub orb_slots: u8,
    pub cards: &'static [Id],
    pub relic_pool: &'static [Id],
    pub potion_pool: &'static [Id],
    pub deck: &'static [(Id, u8)],
    pub relics: &'static [Id],
}

#[derive(Hash)]
pub struct Content {
    cards: Vec<CardDef>,
    powers: Vec<PowerDef>,
    relics: Vec<RelicDef>,
    potions: Vec<PotionDef>,
    enemies: Vec<EnemyDef>,
    encounters: Vec<EncounterDef>,
    orbs: Vec<OrbDef>,
    events: Vec<EventDef>,
    acts: Vec<ActDef>,
    characters: Vec<CharacterDef>,
    colorless: Vec<Id>,
}

impl Content {
    pub fn card_id(&self, id: &str) -> Option<Id> {
        self.cards.iter().position(|x| x.id == id).map(|x| x as Id)
    }

    pub fn relic_id(&self, id: &str) -> Option<Id> {
        self.relics.iter().position(|x| x.id == id).map(|x| x as Id)
    }

    pub fn potion_id(&self, id: &str) -> Option<Id> {
        self.potions
            .iter()
            .position(|x| x.id == id)
            .map(|x| x as Id)
    }

    pub fn cards(&self) -> &[CardDef] {
        &self.cards
    }

    pub fn powers(&self) -> &[PowerDef] {
        &self.powers
    }

    pub fn relics(&self) -> &[RelicDef] {
        &self.relics
    }

    pub fn potions(&self) -> &[PotionDef] {
        &self.potions
    }

    pub fn enemies(&self) -> &[EnemyDef] {
        &self.enemies
    }

    pub fn encounters(&self) -> &[EncounterDef] {
        &self.encounters
    }

    pub fn orbs(&self) -> &[OrbDef] {
        &self.orbs
    }

    pub fn events(&self) -> &[EventDef] {
        &self.events
    }

    pub fn acts(&self) -> &[ActDef] {
        &self.acts
    }

    pub fn characters(&self) -> &[CharacterDef] {
        &self.characters
    }

    pub fn colorless(&self) -> &[Id] {
        &self.colorless
    }

    pub fn validate(&self) -> Result<(), String> {
        unique("cards", self.cards.iter().map(|x| x.id))?;
        unique("powers", self.powers.iter().map(|x| x.id))?;
        unique("relics", self.relics.iter().map(|x| x.id))?;
        unique("potions", self.potions.iter().map(|x| x.id))?;
        unique("enemies", self.enemies.iter().map(|x| x.id))?;
        unique("encounters", self.encounters.iter().map(|x| x.id))?;
        unique("orbs", self.orbs.iter().map(|x| x.id))?;
        unique("events", self.events.iter().map(|x| x.id))?;
        unique("acts", self.acts.iter().map(|x| x.id))?;
        unique("characters", self.characters.iter().map(|x| x.id))?;
        for (index, card) in self.cards.iter().enumerate() {
            if !self.valid_effects(card.effects) {
                return Err(format!("cards[{index}].effects"));
            }
            if card
                .hooks
                .iter()
                .any(|hook| !self.valid_effects(hook.effects))
            {
                return Err(format!("cards[{index}].hooks"));
            }
        }
        for (index, power) in self.powers.iter().enumerate() {
            if power
                .hooks
                .iter()
                .any(|hook| !self.valid_effects(hook.effects))
            {
                return Err(format!("powers[{index}].hooks"));
            }
        }
        for (index, relic) in self.relics.iter().enumerate() {
            if relic
                .hooks
                .iter()
                .any(|hook| !self.valid_effects(hook.effects))
            {
                return Err(format!("relics[{index}].hooks"));
            }
        }
        for (index, potion) in self.potions.iter().enumerate() {
            if !self.valid_effects(potion.effects) {
                return Err(format!("potions[{index}].effects"));
            }
        }
        for (index, enemy) in self.enemies.iter().enumerate() {
            if enemy.hp.is_empty()
                || enemy.moves.get(enemy.start).is_none()
                || enemy.powers.iter().any(|&(id, _)| !self.valid_power(id))
                || enemy.moves.iter().any(|x| {
                    x.next.iter().any(|&i| i >= enemy.moves.len()) || !self.valid_effects(x.effects)
                })
            {
                return Err(format!("enemies[{index}]"));
            }
        }
        for (index, encounter) in self.encounters.iter().enumerate() {
            if encounter
                .enemies
                .iter()
                .any(|&id| id as usize >= self.enemies.len())
            {
                return Err(format!("encounters[{index}].enemies"));
            }
        }
        for (index, orb) in self.orbs.iter().enumerate() {
            if !self.valid_effects(orb.passive) || !self.valid_effects(orb.evoke) {
                return Err(format!("orbs[{index}].effects"));
            }
        }
        for (event_index, event) in self.events.iter().enumerate() {
            for (option_index, option) in event.options.iter().enumerate() {
                for (effect_index, effect) in option.effects.iter().enumerate() {
                    let valid = match effect {
                        RunEffect::AddCard(id, _) => self.card_id(id).is_some(),
                        RunEffect::AddRelic(id) => self.relic_id(id).is_some(),
                        RunEffect::AddPotion(id) => self.potion_id(id).is_some(),
                        RunEffect::PotionRewards(id, _) => self.potion_id(id).is_some(),
                        RunEffect::RandomCard(ids) => {
                            ids.iter().all(|id| self.card_id(id).is_some())
                        }
                        RunEffect::RandomRelic(ids) => {
                            ids.iter().all(|id| self.relic_id(id).is_some())
                        }
                        _ => true,
                    };
                    if !valid {
                        return Err(format!(
                            "events[{event_index}].options[{option_index}].effects[{effect_index}]"
                        ));
                    }
                }
            }
        }
        for (index, act) in self.acts.iter().enumerate() {
            if act
                .encounters
                .iter()
                .chain(act.elites)
                .chain(act.bosses)
                .any(|&id| id as usize >= self.encounters.len())
                || act
                    .events
                    .iter()
                    .any(|&id| id as usize >= self.events.len())
                || act.cards.iter().any(|&id| !self.valid_card(id))
                || act
                    .relics
                    .iter()
                    .any(|&id| id as usize >= self.relics.len())
                || act
                    .potions
                    .iter()
                    .any(|&id| id as usize >= self.potions.len())
            {
                return Err(format!("acts[{index}]"));
            }
        }
        for (index, character) in self.characters.iter().enumerate() {
            if character.cards.iter().any(|&id| !self.valid_card(id))
                || character.deck.iter().any(|&(id, _)| !self.valid_card(id))
                || character
                    .relic_pool
                    .iter()
                    .chain(character.relics)
                    .any(|&id| id as usize >= self.relics.len())
                || character
                    .potion_pool
                    .iter()
                    .any(|&id| id as usize >= self.potions.len())
            {
                return Err(format!("characters[{index}]"));
            }
        }
        if let Some(index) = self.colorless.iter().position(|&id| !self.valid_card(id)) {
            return Err(format!("colorless[{index}]"));
        }
        Ok(())
    }

    fn valid_card(&self, id: Id) -> bool {
        (id as usize) < self.cards.len()
    }

    fn valid_power(&self, id: Id) -> bool {
        (id as usize) < self.powers.len()
    }

    fn valid_amount(&self, amount: Amount) -> bool {
        match amount.scale {
            Scale::ExhaustId(id) => self.valid_card(id),
            Scale::OrbTypesPower(id)
            | Scale::TargetPower(id)
            | Scale::TargetPowerDiv(id, _)
            | Scale::Power(id)
            | Scale::EventPower(id)
            | Scale::EnemyPowerTotal(id) => self.valid_power(id),
            _ => true,
        }
    }

    fn valid_condition(&self, condition: Condition) -> bool {
        match condition {
            Condition::TargetHasPower(id) | Condition::SourceHasPower(id) => self.valid_power(id),
            Condition::HasOrb(id) => (id as usize) < self.orbs.len(),
            Condition::Card(id) => self.valid_card(id),
            _ => true,
        }
    }

    fn valid_filter(&self, filter: CardFilter) -> bool {
        !matches!(filter, CardFilter::Id(id) if !self.valid_card(id))
    }

    fn valid_op(&self, op: CardOp) -> bool {
        !matches!(op, CardOp::Transform(id, _) if !self.valid_card(id))
    }

    fn valid_effects(&self, effects: &[Effect]) -> bool {
        effects.iter().all(|effect| match *effect {
            Effect::Attack(_, amount, _)
            | Effect::OstyAttack(_, amount, _)
            | Effect::MoveDamage(_, amount)
            | Effect::Damage(_, amount)
            | Effect::LoseHp(_, amount)
            | Effect::Block(_, amount)
            | Effect::BlockNextTurn(amount)
            | Effect::DodgeRoll(amount)
            | Effect::DrawBlockIf(_, amount)
            | Effect::RawBlock(_, amount)
            | Effect::Heal(_, amount)
            | Effect::HealPercent(_, amount)
            | Effect::MaxHp(amount)
            | Effect::Gold(amount)
            | Effect::Bomb(amount)
            | Effect::Panache(amount)
            | Effect::RollingBoulder(amount)
            | Effect::ToricToughness(amount)
            | Effect::Misery(amount)
            | Effect::DrawAmount(amount)
            | Effect::ChooseDraw(amount)
            | Effect::RandomColorless(_, amount, _)
            | Effect::RandomColorlessOther(_, amount)
            | Effect::RandomCharacter(_, amount, _)
            | Effect::RandomCharacterCost0(_, amount, _)
            | Effect::Aggression(amount)
            | Effect::AutoPlayDraw(amount, _)
            | Effect::FlakCannon(amount)
            | Effect::ExhaustForBlock(amount)
            | Effect::ExhaustForAttack(_, amount)
            | Effect::ExhaustAttackStep(_, amount, _)
            | Effect::EvokeMany(amount)
            | Effect::EvokeLast(amount)
            | Effect::Stars(amount)
            | Effect::Forge(amount)
            | Effect::Summon(amount)
            | Effect::GrowCard(amount)
            | Effect::GrowDrawn(amount)
            | Effect::PersistCard(amount) => self.valid_amount(amount),
            Effect::AttackMany(_, amount, hits) | Effect::OstyAttackMany(_, amount, hits) => {
                self.valid_amount(amount) && self.valid_amount(hits)
            }
            Effect::ApplyPower(_, id, amount)
            | Effect::ApplyDebuff(_, id, amount)
            | Effect::StackPower(_, id, amount) => {
                self.valid_power(id) && self.valid_amount(amount)
            }
            Effect::TemporaryStrength(_, id, amount) => {
                self.valid_power(id) && self.valid_amount(amount)
            }
            Effect::DoublePower(_, id, _) | Effect::RemovePower(_, id) => self.valid_power(id),
            Effect::AddCard(_, id, _)
            | Effect::AddUpgradedCard(_, id, _)
            | Effect::AddFlaggedCard(_, id, _, _)
            | Effect::AddRandomCard(id, _, _)
            | Effect::AddRandom(_, id, _)
            | Effect::DiscardHandAdd(id)
            | Effect::TransformHand(id, _) => self.valid_card(id),
            Effect::GrowAll(id, amount) => self.valid_card(id) && self.valid_amount(amount),
            Effect::RandomCardOp(_, filter, op, amount) => {
                self.valid_filter(filter) && self.valid_op(op) && self.valid_amount(amount)
            }
            Effect::AutoPlayRandom(_, filter, amount) => {
                self.valid_filter(filter) && self.valid_amount(amount)
            }
            Effect::SelectAmount(_, filter, amount, _, op) => {
                self.valid_filter(filter) && self.valid_amount(amount) && self.valid_op(op)
            }
            Effect::ChannelSlots(id) | Effect::Channel(id, _) => (id as usize) < self.orbs.len(),
            Effect::AutoPlay(card) => self.valid_card(card.id),
            Effect::MoveAll(_, filter, _)
            | Effect::DrawFiltered(_, filter)
            | Effect::DrawFilteredStep(_, filter)
            | Effect::FinishDrawFiltered(filter) => self.valid_filter(filter),
            Effect::If(condition, yes, no) => {
                self.valid_condition(condition) && self.valid_effects(yes) && self.valid_effects(no)
            }
            Effect::Repeat(amount, nested) => {
                self.valid_amount(amount) && self.valid_effects(nested)
            }
            Effect::Random(_, nested) => self.valid_effects(nested),
            Effect::Select(_, filter, _, _, _, op) => {
                self.valid_filter(filter) && self.valid_op(op)
            }
            Effect::Kill(_)
            | Effect::Stun(_)
            | Effect::DoomKill
            | Effect::RandomPotion
            | Effect::Automation(_)
            | Effect::Draw(_)
            | Effect::HandDraw(_)
            | Effect::DrawTo(_)
            | Effect::DrawUntilNot(_)
            | Effect::FreeHand
            | Effect::Energy(_)
            | Effect::DoubleEnergy
            | Effect::RandomCard(..)
            | Effect::OfferColorless(..)
            | Effect::OfferCharacter(..)
            | Effect::OfferCharacterRetain(..)
            | Effect::OfferCharacterType(..)
            | Effect::OfferOtherCharacter(..)
            | Effect::ChooseRandomDraw(_)
            | Effect::ShuffleHandDraw(_)
            | Effect::FillPotions
            | Effect::RandomizeHandCosts
            | Effect::ReplayTagged(..)
            | Effect::DistinctColorless(..)
            | Effect::FranticEscape
            | Effect::Stoke
            | Effect::Stampede
            | Effect::ContinueEndTurn
            | Effect::CopyCard(..)
            | Effect::Discard(..)
            | Effect::DiscardHandDraw
            | Effect::PlayExhaustedShivs
            | Effect::FinishAutoPlay
            | Effect::WhisperingEarring(_)
            | Effect::Exhaust(..)
            | Effect::Upgrade(..)
            | Effect::RandomOrb(_)
            | Effect::Evoke(_)
            | Effect::EvokeAll(_)
            | Effect::PassiveFirst(_)
            | Effect::PassiveLast(_)
            | Effect::PassiveAll
            | Effect::OrbSlots(_)
            | Effect::MaxEnergy(_)
            | Effect::SetCardCost(_)
            | Effect::ReduceCardCost(_)
            | Effect::CapHandCosts
            | Effect::RecycleHand(_)
            | Effect::EndTurn => true,
            Effect::DistinctCharacter(_, _, _) => true,
        })
    }
}

fn unique<'a>(kind: &str, ids: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(format!("{kind}: duplicate id {id}"));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct Card {
    pub id: Id,
    pub instance: u32,
    pub upgrades: u8,
    pub cost_delta: i8,
    pub flags: u16,
    pub turn_flags: u16,
    pub value: i16,
    pub replays: u8,
    pub free: bool,
    pub cost_override: Option<i8>,
    pub enchantment: Option<Enchantment>,
    pub enchantment_amount: i16,
    pub enchantment_value: i16,
    pub variant: u8,
}

fn card_key(card: &Card) -> [i64; 13] {
    [
        card.id as i64,
        card.upgrades as i64,
        card.cost_delta as i64,
        card.flags as i64,
        card.turn_flags as i64,
        card.value as i64,
        card.replays as i64,
        card.free as i64,
        card.cost_override.unwrap_or(-1) as i64,
        card.enchantment.map_or(0, |x| x as i64 + 1),
        card.enchantment_amount as i64,
        card.enchantment_value as i64,
        card.variant as i64,
    ]
}

impl Card {
    fn flags(self, def: CardDef) -> u16 {
        let mut flags = def.flags[self.upgrades.min(1) as usize] | self.flags | self.turn_flags;
        match self.enchantment {
            Some(Enchantment::Goopy) => flags |= EXHAUST,
            Some(Enchantment::Inky) => flags |= INKY,
            Some(Enchantment::RoyallyApproved) => flags |= INNATE | RETAIN,
            Some(Enchantment::SoulsPower) => flags &= !EXHAUST,
            Some(Enchantment::Steady) => flags |= RETAIN,
            Some(Enchantment::TezcatarasEmber) => flags |= ETERNAL,
            _ => {}
        }
        flags
    }

    fn card_type(self, def: CardDef) -> CardType {
        if def.id != "CARD.MAD_SCIENCE" {
            return def.card_type;
        }
        match self.variant / 10 {
            1 => CardType::Attack,
            3 => CardType::Power,
            _ => CardType::Skill,
        }
    }

    fn target(self, def: CardDef) -> Target {
        if def.id == "CARD.MAD_SCIENCE" && self.card_type(def) == CardType::Attack {
            Target::ChosenEnemy
        } else {
            def.target
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Power {
    pub id: Id,
    pub amount: i16,
    pub skip_next_decay: bool,
    pub value: i16,
}

#[derive(Clone, Debug)]
pub struct Creature {
    pub id: Id,
    pub hp: i16,
    pub max_hp: i16,
    pub block: i16,
    pub powers: Vec<Power>,
}

impl Creature {
    fn power(&self, id: Id) -> i16 {
        self.powers
            .iter()
            .find(|x| x.id == id)
            .map_or(0, |x| x.amount)
    }

    fn kind(&self, content: &Content, kind: PowerKind) -> i16 {
        self.powers
            .iter()
            .filter(|x| content.powers[x.id as usize].kind == kind)
            .map(|x| x.amount)
            .sum()
    }

    fn add_power(&mut self, id: Id, amount: i16) {
        self.add_power_with_skip_next_decay(id, amount, false);
    }

    fn consume_power(&mut self, id: Id) {
        if self.power(id) > 0 {
            self.add_power(id, -1);
        }
    }

    fn add_power_with_skip_next_decay(&mut self, id: Id, amount: i16, skip_next_decay: bool) {
        if let Some(power) = self.powers.iter_mut().find(|x| x.id == id) {
            power.amount = power.amount.saturating_add(amount);
            if power.amount == 0 {
                self.powers.retain(|power| power.id != id);
            }
        } else if amount != 0 {
            self.powers.push(Power {
                id,
                amount,
                skip_next_decay,
                value: 0,
            });
        }
    }
}

#[derive(Clone, Debug)]
pub struct Enemy {
    pub instance: u32,
    pub creature: Creature,
    pub move_index: usize,
    pub last_move: usize,
    pub repeats: u8,
    pub move_history: Vec<usize>,
    pub stunned: bool,
    pub value: i16,
}

#[derive(Clone, Copy, Debug)]
pub struct Orb {
    pub id: Id,
    pub value: i16,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct History {
    pub cards: i16,
    pub manual_cards: i16,
    pub manual_plays: i16,
    pub attacks: i16,
    pub skills: i16,
    pub powers: i16,
    pub energy: i16,
    pub exhausted: i16,
    pub discarded: i16,
    pub shivs: i16,
    pub stars_gained: i16,
    pub generated: i16,
    pub ethereal: i16,
    pub extra_drawn: i16,
    pub doom_applied: i16,
    pub osty_attacks: i16,
    pub block_gains: i16,
    block_card: i16,
    block_card_gains: i16,
    pub hp_lost: i16,
    pub hp_loss_events: i16,
    pub feral_returns: i16,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Actor {
    Player,
    Osty,
    Enemy(usize),
}

#[derive(Clone, Copy, Debug, Hash)]
struct Context {
    source: Actor,
    target: Option<usize>,
    card: Option<Id>,
    upgraded: bool,
    x: i16,
    event: i16,
    orb: bool,
    orb_id: Option<Id>,
    pen_nib: bool,
}

impl Context {
    const fn player() -> Self {
        Self {
            source: Actor::Player,
            target: None,
            card: None,
            upgraded: false,
            x: 0,
            event: 0,
            orb: false,
            orb_id: None,
            pen_nib: false,
        }
    }

    const fn card(card: Card) -> Self {
        Self {
            card: Some(card.id),
            upgraded: card.upgrades > 0,
            ..Self::player()
        }
    }
}

#[derive(Clone, Copy, Debug, Hash)]
struct Pending {
    effect: Effect,
    context: Context,
}

#[derive(Clone, Debug, Hash)]
struct AutoPlay {
    card: Card,
    powers: Vec<Power>,
    enemy_powers: Vec<Vec<Power>>,
    plays: u8,
}

#[derive(Clone, Debug)]
pub struct Combat {
    pub player: Creature,
    pub osty: Creature,
    pub enemies: Vec<Enemy>,
    pub draw: Vec<Card>,
    known_draw_top: usize,
    known_draw_bottom: usize,
    pub hand: Vec<Card>,
    pub discard: Vec<Card>,
    pub exhaust: Vec<Card>,
    offer: Vec<Card>,
    pub energy: i16,
    pub max_energy: i16,
    pub draw_per_turn: u8,
    pub stars: i16,
    pub turn: u16,
    pub orbs: Vec<Orb>,
    pub orb_slots: u8,
    pub history: History,
    last_cards: i16,
    orbit_spent: i16,
    hits: Vec<i16>,
    last_damage: i16,
    drawn: i16,
    lightning_channeled: i16,
    orbs_channeled: u8,
    poisoned: u8,
    nightmares: Vec<(Card, u8)>,
    bombs: Vec<(u8, i16)>,
    automation: Vec<(u8, i16)>,
    panache: Vec<(u8, i16, i16)>,
    boulders: Vec<i16>,
    dampened: Vec<(u32, u8)>,
    paels_tears: bool,
    ending: bool,
    playing: Option<Card>,
    auto_plays: Vec<AutoPlay>,
    queue: Vec<Pending>,
    choice: Option<Choice>,
    power_snapshot: Vec<Power>,
    enemy_power_snapshot: Vec<Vec<Power>>,
    card_energy: i16,
    card_stars: i16,
    card_plays: u8,
    force_end: bool,
    enemy_turn: bool,
    centennial_puzzle: bool,
    demon_tongue: bool,
    permafrost: bool,
    pen_nib: bool,
    ruined_helmet: bool,
    music_box: bool,
    mini_regent: bool,
    rainbow_ring: bool,
    kusarigama: u8,
    unsettling_lamp: Option<Id>,
    unsettling_used: bool,
    diamond_diadem: bool,
    belt_buckle: bool,
    burning_sticks: bool,
    red_skull: bool,
    throwing_axe: bool,
    paels_eye: bool,
    paels_eye_extra: bool,
    paels_legion: u8,
    history_course: Option<Card>,
}

#[derive(Clone, Copy, Debug)]
struct Choice {
    pile: Pile,
    filter: CardFilter,
    op: CardOp,
    remaining: u8,
    optional: bool,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub ascension: u8,
    pub character: Id,
    pub hp: i16,
    pub max_hp: i16,
    pub gold: i32,
    pub deck: Vec<Card>,
    pub relics: Vec<Id>,
    pub potions: Vec<Option<Id>>,
    pub act: u8,
    pub floor: u8,
    pub energy: u8,
    pub draw: u8,
    pub orb_slots: u8,
    pub card_shop_removals: u16,
}

#[derive(Clone, Debug)]
pub struct MapNode {
    pub floor: u8,
    pub lane: u8,
    pub room: Room,
    pub next: Vec<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Map {
    pub nodes: Vec<MapNode>,
    pub current: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Rewards {
    pub gold: i32,
    pub cards: Vec<Card>,
    pub card_rewards: Vec<CardReward>,
    pub relics: Vec<Id>,
    pub potions: Vec<Id>,
    pub removals: u8,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CardReward {
    Standard(Room),
    Fixed(Id, CardRarity),
    Kaleidoscope,
    Crystal(CardRarity),
}

#[derive(Clone, Debug)]
struct CrystalSphere {
    cells: Vec<Option<usize>>,
    clear: Vec<bool>,
    items: Vec<(u8, u8, u8, u8, u8)>,
    revealed: Vec<usize>,
    remaining: u8,
    big: bool,
}

#[derive(Clone, Debug)]
pub enum ShopItem {
    Card(Card, i32),
    Relic(Id, i32),
    Potion(Id, i32),
    Remove(i32),
}

#[derive(Clone, Debug)]
pub enum Phase {
    Map,
    Combat(Box<Combat>),
    Rewards(Rewards),
    Shop(Vec<ShopItem>),
    Rest,
    Event(Id, Vec<EventOption>),
    RemoveCards(u8, u16, bool),
    UpgradeCards(u8, bool),
    TransformCards(Option<Id>, u8, bool),
    EnchantCards(Enchantment, i16, u8, Option<CardType>, bool),
    ChooseCards(Vec<Card>, u8, bool),
    ChooseBundles(Vec<Vec<Card>>),
    Won,
    Dead,
}

#[derive(Clone, Copy, Debug)]
enum DamageKind {
    Attack,
    Move,
    Unpowered,
    Unblockable,
}

#[derive(Clone, Copy, Debug, Default)]
struct DamageResult {
    lost: i16,
    resolved: i16,
}

#[derive(Clone, Copy, Debug)]
struct Rng([u64; 4], u64);

impl Rng {
    fn from_seed(mut seed: u64) -> Self {
        fn split_mix(seed: &mut u64) -> u64 {
            *seed = seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = *seed;
            value = (value ^ value >> 30).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ value >> 27).wrapping_mul(0x94d049bb133111eb);
            value ^ value >> 31
        }
        Self(
            [
                split_mix(&mut seed),
                split_mix(&mut seed),
                split_mix(&mut seed),
                split_mix(&mut seed),
            ],
            0,
        )
    }

    fn next(&mut self) -> u64 {
        self.1 += 1;
        let result = self.0[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let shifted = self.0[1] << 17;
        self.0[2] ^= self.0[0];
        self.0[3] ^= self.0[1];
        self.0[1] ^= self.0[2];
        self.0[0] ^= self.0[3];
        self.0[2] ^= shifted;
        self.0[3] = self.0[3].rotate_left(45);
        result
    }

    fn below(&mut self, bound: u32) -> u32 {
        assert!(bound > 0);
        (self.double() * bound as f64) as u32
    }

    fn range(&mut self, min: i16, max: i16) -> i16 {
        min + self.below((max - min + 1) as u32) as i16
    }

    fn single(&mut self) -> f32 {
        self.double() as f32
    }

    fn float(&mut self, min: f32, max: f32) -> f32 {
        (self.double() * (max - min) as f64 + min as f64) as f32
    }

    fn double(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for i in (1..values.len()).rev() {
            values.swap(i, self.below((i + 1) as u32) as usize);
        }
    }

    fn forward(&mut self, count: u64) {
        for _ in 0..count {
            self.next();
        }
    }

    fn gaussian(&mut self, mean: f64, deviation: f64, min: i16, max: i16) -> i16 {
        loop {
            let radius = (-2.0 * (1.0 - self.double()).ln()).sqrt();
            let angle = std::f64::consts::TAU * (1.0 - self.double());
            let value = (mean + deviation * radius * angle.sin()).round_ties_even() as i16;
            if (min..=max).contains(&value) {
                return value;
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Rngs {
    combat_card_generation: Rng,
    combat_card_selection: Rng,
    combat_energy_costs: Rng,
    combat_orb_generation: Rng,
    combat_potion_generation: Rng,
    combat_targets: Rng,
    monster_ai: Rng,
    niche: Rng,
    shuffle: Rng,
    treasure_room_relics: Rng,
    unknown_map_point: Rng,
    up_front: Rng,
    rewards: Rng,
    shops: Rng,
    transformations: Rng,
}

impl Rngs {
    fn from_seed(seed: u64) -> Self {
        let named = |name| Rng::from_seed((seed as u32).wrapping_add(hash(name)) as u64);
        Self {
            combat_card_generation: named("combat_card_generation"),
            combat_card_selection: named("combat_card_selection"),
            combat_energy_costs: named("combat_energy_costs"),
            combat_orb_generation: named("combat_orb_generation"),
            combat_potion_generation: named("combat_potion_generation"),
            combat_targets: named("combat_targets"),
            monster_ai: named("monster_ai"),
            niche: named("niche"),
            shuffle: named("shuffle"),
            treasure_room_relics: named("treasure_room_relics"),
            unknown_map_point: named("unknown_map_point"),
            up_front: named("up_front"),
            rewards: named("rewards"),
            shops: named("shops"),
            transformations: named("transformations"),
        }
    }
}

fn hash(value: &str) -> u32 {
    let mut a = 352_654_597u32;
    let mut b = a;
    let mut chars = value.encode_utf16();
    while let Some(first) = chars.next() {
        a = a.wrapping_mul(33) ^ first as u32;
        if let Some(second) = chars.next() {
            b = b.wrapping_mul(33) ^ second as u32;
        }
    }
    a.wrapping_add(b.wrapping_mul(1_566_083_941))
}

fn encounter_tags(id: &str) -> u16 {
    [
        ("SCROLLS_OF_BITING", 1),
        ("BOWLBUG", 2),
        ("SLUMBER", 2),
        ("EXOSKELETON", 4),
        ("OVERGROWTH_FLORA", 8 | 16),
        ("SHROOM", 8 | 16),
        ("SLIME", 16),
        ("FUZZY_WURM_CRAWLER", 32),
        ("OVERGROWTH_CRAWLERS", 32 | 64),
        ("SHRINKER", 64),
        ("NIBBIT", 128),
        ("CORPSE_SLUGS", 256),
        ("SEAPUNK", 512),
        ("UNDERDOCKS_WILDLIFE", 512),
    ]
    .into_iter()
    .filter_map(|(needle, tag)| id.contains(needle).then_some(tag))
    .fold(0, |tags, tag| tags | tag)
}

fn encounter_entry(id: &str) -> &str {
    match id {
        "ENCOUNTER.THE_HIVE_NORMAL_BOWLBUG_SWARM" => "BOWLBUGS_NORMAL",
        "ENCOUNTER.THE_HIVE_WEAK_BOWLBUGS" => "BOWLBUGS_WEAK",
        "ENCOUNTER.THE_HIVE_ELITE_THE_DECIMILLIPEDE" => "DECIMILLIPEDE_ELITE",
        "ENCOUNTER.THE_HIVE_NORMAL_MANY_EXOSKELETONS" => "EXOSKELETONS_NORMAL",
        "ENCOUNTER.THE_HIVE_NORMAL_MASS_OF_MYTES" => "MYTES_NORMAL",
        "ENCOUNTER.THE_HIVE_WEAK_EXOSKELETONS" => "EXOSKELETONS_WEAK",
        "ENCOUNTER.THE_OVERGROWTH_NORMAL_OVERGROWTH_FLORA" => "FLYCONID_NORMAL",
        "ENCOUNTER.THE_OVERGROWTH_NORMAL_RUBY_RAIDERS" => "RUBY_RAIDERS_NORMAL",
        "ENCOUNTER.THE_OVERGROWTH_NORMAL_INKLETS" => "INKLETS_NORMAL",
        "ENCOUNTER.THE_OVERGROWTH_NORMAL_SWARM_OF_SLIMES" => "SLIMES_NORMAL",
        "ENCOUNTER.THE_OVERGROWTH_WEAK_GROUP_OF_SLIMES" => "SLIMES_WEAK",
        "ENCOUNTER.THE_OVERGROWTH_NORMAL_STRANGLER_AND_FRIEND" => "SLITHERING_STRANGLER_NORMAL",
        "ENCOUNTER.THE_GLORY_NORMAL_MANY_SCROLLS_OF_BITING" => "SCROLLS_OF_BITING_NORMAL",
        "ENCOUNTER.THE_GLORY_WEAK_SCROLLS_OF_BITING" => "SCROLLS_OF_BITING_WEAK",
        "ENCOUNTER.THE_UNDERDOCKS_NORMAL_MANY_CORPSE_SLUGS" => "CORPSE_SLUGS_NORMAL",
        "ENCOUNTER.THE_UNDERDOCKS_WEAK_CORPSE_SLUGS" => "CORPSE_SLUGS_WEAK",
        "ENCOUNTER.THE_UNDERDOCKS_NORMAL_TWO_TAILED_RATS" => "TWO_TAILED_RATS_NORMAL",
        "ENCOUNTER.THE_UNDERDOCKS_ELITE_PHANTASMAL_GARDENERS" => "PHANTASMAL_GARDENERS_ELITE",
        "ENCOUNTER.EVENT_ENCOUNTERS_NORMAL_PUNCH_CONSTRUCTS" => "PUNCH_OFF_EVENT_ENCOUNTER",
        "ENCOUNTER.PUNCH_OFF_EVENT_ENCOUNTER" => "PUNCH_OFF_EVENT_ENCOUNTER",
        _ => id.strip_prefix("ENCOUNTER.").unwrap_or(id),
    }
}

#[derive(Clone, Debug, Default)]
struct Expectation {
    choices: Vec<usize>,
    used: usize,
    next: Option<usize>,
    unknown: bool,
    drawn: i16,
    discarded: i16,
}

#[derive(Clone, Debug)]
pub struct Game {
    run: Run,
    map: Map,
    phase: Phase,
    act: Id,
    room: Room,
    run_queue: Vec<RunEffect>,
    resume: Option<Phase>,
    rngs: Rngs,
    next_card: u32,
    rarity_offset: i16,
    potion_odds: i8,
    bosses: [Option<Id>; 2],
    bosses_visited: u8,
    encounters: Vec<Id>,
    elites: Vec<Id>,
    weak_encounters_left: u8,
    regular_encounters_left: u8,
    elite_encounters_left: u8,
    last_encounter: Option<Id>,
    last_elite: Option<Id>,
    events: Vec<Id>,
    visited_events: Vec<Id>,
    enemy_starts: Vec<usize>,
    seed: u32,
    unknown_odds: [i16; 4],
    happy_flower: u8,
    tea_set: u8,
    replacing_potion: bool,
    pending_potion: Option<Id>,
    removal_price: i32,
    event_rng: Option<Rng>,
    event_relic: Option<Id>,
    event_data: [i64; 4],
    event_cards: Vec<u32>,
    relic_queue: Vec<Id>,
    relic_deques: [Vec<Id>; 4],
    shared_relic_deques: [Vec<Id>; 4],
    pending_curse: bool,
    wongo_combats: Option<u8>,
    spoils: Option<(u8, u8)>,
    event_combat: u8,
    damage_taken: bool,
    lasting_candy: u8,
    paels_wing: u8,
    silver_crucible: u8,
    silken_tress: bool,
    rerolled_cards: bool,
    maw_bank: bool,
    silver_treasures: u8,
    golden_compass: Option<u8>,
    winged_boots: u8,
    rest_used: u8,
    girya: u8,
    pumpkin_candle: u8,
    fishing_rod: u8,
    cooking: bool,
    astrolabe: bool,
    transform_niche: bool,
    paels_tooth: bool,
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
    crystal: Option<CrystalSphere>,
    reward_gold_parts: Vec<i32>,
    fake_merchant: Vec<Id>,
    fake_shop: bool,
    fake_happy_flower: u8,
    lizard_tail: bool,
    nunchaku: u8,
    pendulum: u8,
    pen_nib: u8,
    iron_club: u8,
    joss_paper: u8,
    tuning_fork: u8,
    bone_tea: bool,
    tea_of_discourtesy: bool,
    galactic_dust: u8,
    book_of_five_rings: u8,
    ember_tea: u8,
    sword_of_stone: u8,
    replaying: bool,
    expectation: Option<Expectation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Play { hand: usize, target: Option<usize> },
    Potion { slot: usize, target: Option<usize> },
    DiscardPotion(usize),
    Choose(usize),
    EndTurn,
    Path(usize),
    RewardGold,
    RewardCard(usize),
    RewardRelic(usize),
    RewardPotion(usize),
    RewardRemove,
    RerollCards,
    SacrificeCards,
    Buy(usize),
    Rest,
    Hatch,
    Lift,
    Cook,
    Kindle,
    Dig,
    Smith(usize),
    Event(usize),
    CrystalCell(u8, u8),
    CrystalTool(bool),
    EventRelic(usize, Id),
    EventCard(usize, Card),
    Enchant(usize),
    RemoveCard(usize),
    Clone,
    Cancel,
    Done,
    Leave,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidAction,
    InvalidContent,
    CombatActive,
    NoCombat,
}
