use super::*;

/// The listeners the sim handles in code rather than data hooks, per game pass (`hook-listeners.txt`).
const LISTENERS: &[(Trigger, &[&str])] = &[
    (
        Trigger::AfterRoomEntered,
        &[
            "RELIC.EMBER_TEA",
            "RELIC.GIRYA",
            "RELIC.GORGET",
            "RELIC.PHILOSOPHERS_STONE",
            "RELIC.RED_SKULL",
            "RELIC.SLING_OF_COURAGE",
            "RELIC.STONE_CRACKER",
            "RELIC.SWORD_OF_JADE",
        ],
    ),
    (
        Trigger::BeforeCombatStart,
        &[
            "POWER.GALVANIC_POWER",
            "POWER.VITAL_SPARK_POWER",
            "RELIC.BELT_BUCKLE",
            "RELIC.DELICATE_FROND",
            "RELIC.FAKE_SNECKO_EYE",
            "RELIC.SNECKO_EYE",
            "RELIC.TEA_OF_DISCOURTESY",
        ],
    ),
    (Trigger::BeforeCombatStartLate, &["RELIC.PETRIFIED_TOAD"]),
    (
        Trigger::BeforeSideTurnStart,
        &[
            "POWER.AGGRESSION_POWER",
            "POWER.HARDENED_SHELL_POWER",
            "RELIC.POLLINOUS_CORE",
            "RELIC.POWER_CELL",
        ],
    ),
    (
        Trigger::AfterBlockCleared,
        &[
            "POWER.SELF_FORMING_CLAY_POWER",
            "POWER.TORIC_TOUGHNESS_POWER",
            "RELIC.CAPTAINS_WHEEL",
            "RELIC.HORN_CLEAT",
            "RELIC.SPARKLING_ROUGE",
        ],
    ),
    (
        Trigger::AfterEnergyReset,
        &[
            "POWER.SPINNER_POWER",
            "RELIC.ART_OF_WAR",
            "RELIC.FAKE_VENERABLE_TEA_SET",
            "RELIC.VENERABLE_TEA_SET",
        ],
    ),
    (
        Trigger::BeforeHandDraw,
        &[
            "CARD.BOLAS",
            "CARD.THRUMMING_HATCHET",
            "POWER.HELLO_WORLD_POWER",
            "POWER.NIGHTMARE_POWER",
            "RELIC.BLESSED_ANTLER",
            "RELIC.FUNERARY_MASK",
            "RELIC.JEWELED_MASK",
            "RELIC.NINJA_SCROLL",
            "RELIC.RADIANT_PEARL",
            "RELIC.TOASTY_MITTENS",
            "RELIC.TOOLBOX",
        ],
    ),
    (
        Trigger::AfterPlayerTurnStart,
        &[
            "POWER.ENTROPY_POWER",
            "POWER.LOOP_POWER",
            "POWER.ROLLING_BOULDER_POWER",
            "POWER.TOOLS_OF_THE_TRADE_POWER",
            "POWER.TYRANNY_POWER",
            "RELIC.BELLOWS",
            "RELIC.CHOICES_PARADOX",
            "RELIC.EMOTION_CHIP",
            "RELIC.FESTIVE_POPPER",
            "RELIC.GAMBLING_CHIP",
            "RELIC.MERCURY_HOURGLASS",
            "RELIC.MR_STRUGGLES",
            "RELIC.PENDULUM",
            "RELIC.ROYAL_POISON",
            "RELIC.VEXING_PUZZLEBOX",
        ],
    ),
    (
        Trigger::AfterPlayerTurnStartLate,
        &["RELIC.BLOOD_VIAL", "RELIC.FAKE_BLOOD_VIAL"],
    ),
    (
        Trigger::AfterSideTurnStart,
        &[
            "POWER.BIASED_COGNITION_POWER",
            "POWER.COUNTDOWN_POWER",
            "POWER.NOXIOUS_FUMES_POWER",
            "POWER.POISON_POWER",
            "POWER.RAMPART_POWER",
            "POWER.SHADOW_STEP_POWER",
            "POWER.SLOW_POWER",
            "POWER.WRAITH_FORM_POWER",
            "RELIC.AKABEKO",
            "RELIC.BIG_HAT",
            "RELIC.BONE_TEA",
            "RELIC.BOOMING_CONCH",
            "RELIC.BREAD",
            "RELIC.BRIMSTONE",
            "RELIC.CANDELABRA",
            "RELIC.CHANDELIER",
            "RELIC.CROSSBOW",
            "RELIC.FAKE_HAPPY_FLOWER",
            "RELIC.FENCING_MANUAL",
            "RELIC.HAPPY_FLOWER",
            "RELIC.LANTERN",
            "RELIC.LETTER_OPENER",
            "RELIC.ORANGE_DOUGH",
            "RELIC.PAELS_LEGION",
            "RELIC.PAELS_TEARS",
            "RELIC.SAI",
            "RELIC.SEAL_OF_GOLD",
            "RELIC.VERY_HOT_COCOA",
        ],
    ),
    (
        Trigger::AutoPrePlay,
        &["POWER.MAYHEM_POWER", "RELIC.HISTORY_COURSE"],
    ),
    (Trigger::AutoPrePlayLate, &["RELIC.WHISPERING_EARRING"]),
    (
        Trigger::BeforeSideTurnEndVeryEarly,
        &[
            "POWER.ASLEEP_POWER",
            "RELIC.FAKE_ORICHALCUM",
            "RELIC.ORICHALCUM",
        ],
    ),
    (Trigger::BeforeSideTurnEndEarly, &["RELIC.PAELS_EYE"]),
    (
        Trigger::BeforeSideTurnEnd,
        &[
            "POWER.DOOM_POWER",
            "POWER.HAILSTORM_POWER",
            "POWER.THE_BOMB_POWER",
            "RELIC.DIAMOND_DIADEM",
            "RELIC.FAKE_ORICHALCUM",
            "RELIC.ORICHALCUM",
            "RELIC.PAELS_TEARS",
            "RELIC.RIPPLE_BASIN",
            "RELIC.SCREAMING_FLAGON",
            "RELIC.STONE_CALENDAR",
        ],
    ),
    (
        Trigger::AfterSideTurnEnd,
        &[
            "POWER.ASLEEP_POWER",
            "POWER.BATTLEWORN_DUMMY_TIME_LIMIT_POWER",
            "POWER.COLOSSUS_POWER",
            "POWER.CONQUEROR_POWER",
            "POWER.DARK_EMBRACE_POWER",
            "POWER.DEBILITATE_POWER",
            "POWER.DIAMOND_DIADEM_POWER",
            "POWER.DOUBLE_DAMAGE_POWER",
            "POWER.ESCAPE_ARTIST_POWER",
            "POWER.FLAME_BARRIER_POWER",
            "POWER.FRAIL_POWER",
            "POWER.HATCH_POWER",
            "POWER.HELLRAISER_POWER",
            "POWER.INTANGIBLE_POWER",
            "POWER.MONARCHS_GAZE_STRENGTH_DOWN_POWER",
            "POWER.NEMESIS_POWER",
            "POWER.NO_BLOCK_POWER",
            "POWER.NO_DRAW_POWER",
            "POWER.NO_ENERGY_GAIN_POWER",
            "POWER.OBLIVION_POWER",
            "POWER.PANACHE_POWER",
            "POWER.REGEN_POWER",
            "POWER.SHRINK_POWER",
            "POWER.SIC_EM_POWER",
            "POWER.SKITTISH_POWER",
            "POWER.SLUMBER_POWER",
            "POWER.STRANGLE_POWER",
            "POWER.TAINTED_POWER",
            "POWER.VULNERABLE_POWER",
            "POWER.WEAK_POWER",
            "RELIC.JOSS_PAPER",
            "RELIC.LUNAR_PASTRY",
            "RELIC.PARRYING_SHIELD",
        ],
    ),
    (
        Trigger::AfterSideTurnEndLate,
        &["POWER.DISINTEGRATION_POWER"],
    ),
    (
        Trigger::AfterCardExhausted,
        &[
            "CARD.DRUM_OF_BATTLE",
            "RELIC.BURNING_STICKS",
            "RELIC.JOSS_PAPER",
        ],
    ),
    (
        Trigger::AfterCardPlayed,
        &[
            "POWER.CURL_UP_POWER",
            "POWER.GALVANIC_POWER",
            "POWER.PANACHE_POWER",
            "POWER.TENDER_POWER",
            "POWER.VITAL_SPARK_POWER",
            "POWER.WITHERING_PRESENCE_POWER",
            "RELIC.HELICAL_DART",
            "RELIC.IRON_CLUB",
            "RELIC.RAINBOW_RING",
            "RELIC.RAZOR_TOOTH",
            "RELIC.UNSETTLING_LAMP",
        ],
    ),
    (Trigger::AfterCardDrawn, &["POWER.AUTOMATION_POWER"]),
    (
        Trigger::BeforeCardPlayed,
        &[
            "POWER.DANSE_MACABRE_POWER",
            "POWER.FREE_ATTACK_POWER",
            "POWER.FREE_POWER_POWER",
            "POWER.FREE_SKILL_POWER",
            "POWER.SPIRIT_OF_ASH_POWER",
            "POWER.THE_SEALED_THRONE_POWER",
            "POWER.VEILPIERCER_POWER",
            "RELIC.INTIMIDATING_HELMET",
            "RELIC.MUSIC_BOX",
            "RELIC.PEN_NIB",
        ],
    ),
    (
        Trigger::AttackPlayed,
        &["RELIC.MUSIC_BOX", "RELIC.NUNCHAKU"],
    ),
    (
        Trigger::SkillPlayed,
        &[
            "POWER.ENRAGE_POWER",
            "POWER.SMOGGY_POWER",
            "RELIC.LETTER_OPENER",
            "RELIC.TUNING_FORK",
        ],
    ),
    (Trigger::PowerPlayed, &["RELIC.PERMAFROST"]),
    (Trigger::AfterCardGeneratedForCombat, &["CARD.ROCKET_PUNCH"]),
    (
        Trigger::AfterPotionUsed,
        &["RELIC.BELT_BUCKLE", "RELIC.REPTILE_TRINKET"],
    ),
    (
        Trigger::AfterDamageGiven,
        &[
            "POWER.ENVENOM_POWER",
            "POWER.IMBALANCED_POWER",
            "POWER.MONARCHS_GAZE_POWER",
            "POWER.PAPER_CUTS_POWER",
            "POWER.REAPER_FORM_POWER",
            "POWER.SIC_EM_POWER",
            "RELIC.HAND_DRILL",
        ],
    ),
    (
        Trigger::AfterDamageReceived,
        &[
            "POWER.ASLEEP_POWER",
            "POWER.CURL_UP_POWER",
            "POWER.FLAME_BARRIER_POWER",
            "POWER.FLUTTER_POWER",
            "POWER.HARDENED_SHELL_POWER",
            "POWER.INFERNO_POWER",
            "POWER.PERSONAL_HIVE_POWER",
            "POWER.PLOW_POWER",
            "POWER.REFLECT_POWER",
            "POWER.RUPTURE_POWER",
            "POWER.SHRIEK_POWER",
            "POWER.SLIPPERY_POWER",
            "POWER.SLUMBER_POWER",
            "POWER.THE_GAMBIT_POWER",
            "RELIC.CENTENNIAL_PUZZLE",
            "RELIC.DEMON_TONGUE",
            "RELIC.SELF_FORMING_CLAY",
        ],
    ),
    (
        Trigger::AfterDeath,
        &[
            "CARD.MELANCHOLY",
            "POWER.ADAPTABLE_POWER",
            "POWER.CONSTRICT_POWER",
            "POWER.CRAB_RAGE_POWER",
            "POWER.DAMPEN_POWER",
            "POWER.HEX_POWER",
            "POWER.ILLUSION_POWER",
            "POWER.INFESTED_POWER",
            "POWER.POSSESS_SPEED_POWER",
            "POWER.POSSESS_STRENGTH_POWER",
            "POWER.RAVENOUS_POWER",
            "POWER.REATTACH_POWER",
            "POWER.SHRINK_POWER",
            "POWER.STEAM_ERUPTION_POWER",
            "POWER.STOCK_POWER",
            "POWER.SURPRISE_POWER",
            "RELIC.GREMLIN_HORN",
        ],
    ),
];

/// The powers that remember each play's amount in BeforeCardPlayed for its AfterCardPlayed
/// (`amountsForPlayedCards`), so one applied mid-play doesn't hear that play.
const RECORDERS: &[&str] = &[
    "POWER.AFTERIMAGE_POWER",
    "POWER.CALAMITY_POWER",
    "POWER.GRAVITY_POWER",
    "POWER.MONOLOGUE_POWER",
    "POWER.OBLIVION_POWER",
    "POWER.SERPENT_FORM_POWER",
    "POWER.STORM_POWER",
    "POWER.STRANGLE_POWER",
    "POWER.SUBROUTINE_POWER",
];

pub(crate) const COMBAT_START: &[Effect] = &[
    Effect::Fire(Trigger::AfterRoomEntered),
    Effect::Fire(Trigger::BeforeCombatStart),
    Effect::Fire(Trigger::BeforeCombatStartLate),
];

/// CombatManager.StartTurn and SetupPlayerTurn, the player side.
pub(crate) const PLAYER_TURN: &[Effect] = &[
    Effect::Step(Step::TurnStart),
    Effect::Fire(Trigger::BeforeSideTurnStart),
    Effect::Step(Step::RollMoves),
    Effect::Step(Step::ClearBlock),
    Effect::Fire(Trigger::AfterBlockCleared),
    Effect::Step(Step::ResetEnergy),
    Effect::Fire(Trigger::AfterEnergyReset),
    Effect::Fire(Trigger::AfterEnergyResetLate),
    Effect::Fire(Trigger::BeforeHandDraw),
    Effect::Step(Step::HandDraw),
    Effect::Fire(Trigger::AfterPlayerTurnStart),
    Effect::Fire(Trigger::AfterPlayerTurnStartLate),
    Effect::Fire(Trigger::AfterSideTurnStart),
    Effect::Fire(Trigger::AfterSideTurnStartLate),
    Effect::Step(Step::TurnStartOrbs),
    Effect::Step(Step::Bombardments),
    Effect::Fire(Trigger::AutoPrePlay),
    Effect::Fire(Trigger::AutoPrePlayLate),
];

/// CombatManager.EndPlayerTurnPhaseOne: AfterAutoPostPlayPhaseEntered, BeforeTurnEnd, DoTurnEnd
/// and BeforeFlush; then phase two (Flush: the hand flush, AfterTurnEnd and the switch of sides).
pub(crate) const END_PLAYER_TURN: &[Effect] = &[
    Effect::Step(Step::IAmInvincible),
    Effect::Step(Step::Stampede),
    Effect::Step(Step::HowlFromBeyond),
    Effect::Fire(Trigger::BeforeSideTurnEndVeryEarly),
    Effect::Fire(Trigger::BeforeSideTurnEndEarly),
    Effect::Fire(Trigger::BeforeSideTurnEnd),
    Effect::Step(Step::TurnEndOrbs),
    Effect::Step(Step::DoTurnEnd),
    Effect::Step(Step::Retain),
    Effect::Step(Step::Flush),
    Effect::Step(Step::Bookmark),
    Effect::Fire(Trigger::AfterSideTurnEnd),
    Effect::Fire(Trigger::AfterSideTurnEndLate),
    Effect::Step(Step::SwitchSide),
];

/// CombatManager.StartTurn, the enemy side, then its moves (ExecuteEnemyTurn).
pub(crate) const ENEMY_TURN: &[Effect] = &[
    Effect::Fire(Trigger::BeforeSideTurnStart),
    Effect::Step(Step::ClearBlock),
    Effect::Fire(Trigger::AfterSideTurnStart),
    Effect::Fire(Trigger::AfterSideTurnStartLate),
    Effect::Step(Step::EnemyMoves),
];

/// CombatManager.EndEnemyTurnInternal (BeforeTurnEnd and AfterTurnEnd), then the switch of sides.
pub(crate) const END_ENEMY_TURN: &[Effect] = &[
    Effect::Fire(Trigger::BeforeSideTurnEndVeryEarly),
    Effect::Fire(Trigger::BeforeSideTurnEndEarly),
    Effect::Fire(Trigger::BeforeSideTurnEnd),
    Effect::Fire(Trigger::AfterSideTurnEnd),
    Effect::Fire(Trigger::AfterSideTurnEndLate),
    Effect::Step(Step::EnemyTurnEnd),
];

pub(crate) fn listens(trigger: Trigger, id: &str, hooks: &[Hook]) -> bool {
    hooks.iter().any(|hook| hears(hook.trigger, trigger)) || in_code(trigger, id)
}

fn in_code(trigger: Trigger, id: &str) -> bool {
    LISTENERS
        .iter()
        .any(|&(at, ids)| hears(at, trigger) && ids.contains(&id))
}

/// A pass runs the listeners' hooks for it; a played card's pass (an attack's AttackPlayed) also
/// runs those for any played card (AfterCardPlayed), in the listener's own hook order.
fn hears(hook: Trigger, pass: Trigger) -> bool {
    hook == pass
        || hook == Trigger::AfterCardPlayed
            && matches!(
                pass,
                Trigger::AttackPlayed | Trigger::SkillPlayed | Trigger::PowerPlayed
            )
}

impl Game {
    pub(crate) fn trigger(
        &mut self,
        content: &Content,
        trigger: Trigger,
        owner: Actor,
        event: i16,
    ) {
        let target = match owner {
            Actor::Player | Actor::Osty => None,
            Actor::Enemy(i) => Some(i),
        };
        let context = Context {
            source: owner,
            target,
            event,
            ..Context::player()
        };
        let powers = self.creature(owner).powers.clone();
        self.fire(content, trigger, context, powers);
    }

    /// CardCmd.Exhaust's AfterCardExhausted pass (causedByEthereal for a turn-end Ethereal).
    pub(super) fn exhausted(&mut self, content: &Content, card: Card, ethereal: bool) {
        let context = Context {
            event: 1,
            ethereal,
            subject: Some(card.instance),
            ..Context::player()
        };
        let powers = self.creature(Actor::Player).powers.clone();
        self.fire(content, Trigger::AfterCardExhausted, context, powers);
    }

    /// One play's BeforeCardPlayed pass (the player's powers, then the relics), as the powers
    /// that remember plays record their amounts for its AfterCardPlayed pass.
    pub(super) fn before_card_played(&mut self, content: &Content) {
        let actors = (0..self.combat().unwrap().enemies.len()).map(Actor::Enemy);
        let recorded = (std::iter::once(Actor::Player).chain(actors))
            .flat_map(|actor| {
                let powers = self.creature(actor).powers.iter();
                let powers =
                    powers.filter(|x| RECORDERS.contains(&content.powers[x.id as usize].id));
                powers.map(move |power| (actor, *power))
            })
            .collect();
        let play = self.combat_mut().unwrap().plays.last_mut().unwrap();
        play.recorded = recorded;
        let context = play.context;
        let powers = self.creature(Actor::Player).powers.clone();
        self.fire(content, Trigger::BeforeCardPlayed, context, powers);
    }

    /// One play's AfterCardPlayed pass, typed by the card (`hears`), in listener order: the
    /// player's powers, the relics, then each enemy's powers, as they are when the pass starts;
    /// the powers that remember plays hear only those they recorded, with the recorded amount.
    /// Kunai, Shuriken, Ornamental Fan and Kusarigama's shared attack count resolves before the
    /// relics.
    pub(crate) fn card_played(
        &mut self,
        content: &Content,
        card: Card,
        recorded: Vec<(Actor, Power)>,
    ) -> Context {
        let card_type = card.card_type(content.cards[card.id as usize]);
        let trigger = match card_type {
            CardType::Attack => Trigger::AttackPlayed,
            CardType::Skill => Trigger::SkillPlayed,
            CardType::Power => Trigger::PowerPlayed,
            _ => Trigger::AfterCardPlayed,
        };
        let context = Context {
            event: 1,
            subject: Some(card.instance),
            ..Context::card(card)
        };
        let hearing = |game: &Self, actor| -> Vec<_> {
            (game.creature(actor).powers.iter())
                .filter_map(|&power| {
                    let def = content.powers[power.id as usize];
                    if !listens(trigger, def.id, def.hooks) {
                        None
                    } else if RECORDERS.contains(&def.id) {
                        let old = recorded.iter().find(|(owner, x)| {
                            *owner == actor && x.id == power.id && x.instance == power.instance
                        });
                        let amount = old.map(|&(_, x)| x.amount);
                        amount.map(|amount| Listener::Power(actor, Power { amount, ..power }))
                    } else {
                        Some(Listener::Power(actor, power))
                    }
                })
                .collect()
        };
        let enemies = (0..self.combat().unwrap().enemies.len()).map(Actor::Enemy);
        let enemies = enemies.flat_map(|actor| hearing(self, actor)).collect();
        self.queue_pass(trigger, enemies, context);
        if card_type == CardType::Attack {
            self.every_third_attack(content);
        }
        let mut allies = hearing(self, Actor::Player);
        allies.extend(self.relic_listeners(content, trigger));
        self.queue_pass(trigger, allies, context);
        context
    }

    /// The card being played is of this type.
    fn playing(&self, content: &Content, card_type: CardType) -> bool {
        let card = self.combat().unwrap().plays.last().unwrap().card;
        card.card_type(content.cards[card.id as usize]) == card_type
    }

    fn playing_ethereal(&self, content: &Content) -> bool {
        let combat = self.combat().unwrap();
        let card = combat.plays.last().unwrap().card;
        is_ethereal(combat, card, content.cards[card.id as usize])
    }

    /// The pass's listeners pushed in reverse, so the first one resolves first.
    fn queue_pass(&mut self, trigger: Trigger, listeners: Vec<Listener>, context: Context) {
        let queue = &mut self.combat_mut().unwrap().queue;
        queue.extend(listeners.into_iter().rev().map(|listener| Pending {
            effect: Effect::Listen(trigger, listener),
            context,
        }));
    }

    /// The player's live relics that hear the pass, in relic order.
    fn relic_listeners(&self, content: &Content, trigger: Trigger) -> Vec<Listener> {
        let relics = (0..self.run.relics.len()).filter(|&i| {
            let relic = content.relics[self.run.relics[i] as usize];
            !self.melted_relics.contains(&i) && listens(trigger, relic.id, relic.hooks)
        });
        relics.map(|i| Listener::Relic(i as u8)).collect()
    }

    /// One hook pass, pushed in reverse so the first listener resolves first. Most passes don't
    /// start once the combat is over or ending (`Hook.IterateCombatHookListeners`).
    pub(super) fn fire(
        &mut self,
        content: &Content,
        trigger: Trigger,
        context: Context,
        powers: Vec<Power>,
    ) {
        let turn = (PLAYER_TURN.iter().chain(END_PLAYER_TURN)).any(|x| *x == Effect::Fire(trigger));
        if (turn
            || matches!(
                trigger,
                Trigger::AfterBlockGained
                    | Trigger::AfterCardDrawn
                    | Trigger::AfterCardGeneratedForCombat
                    | Trigger::AfterCardExhausted
                    | Trigger::AfterShuffle
            ))
            && (self.enemies_defeated() || self.creature(Actor::Player).hp <= 0)
        {
            return;
        }
        let listeners = self.listeners(content, trigger, context, powers);
        self.queue_pass(trigger, listeners, context);
    }

    /// The pass's listeners in game order (CombatState.IterateHookListeners): the player's powers,
    /// relics and cards, then Osty and the enemies, which hear the player's passes only in code
    /// (after combat start). The enemy side's turn passes go over its participants, after the
    /// player's and Osty's in-code turn-end listeners; any other pass over the source's snapshotted
    /// powers, plus the relics for the player. Generation hooks only count the player's cards.
    fn listeners(
        &self,
        content: &Content,
        trigger: Trigger,
        context: Context,
        powers: Vec<Power>,
    ) -> Vec<Listener> {
        if trigger == Trigger::AfterCardGeneratedForCombat && self.combat().unwrap().enemy_turn {
            return vec![];
        }
        let relics = self.relic_listeners(content, trigger).into_iter();
        if self.combat().unwrap().enemy_turn
            && (ENEMY_TURN.iter().chain(END_ENEMY_TURN)).any(|x| *x == Effect::Fire(trigger))
        {
            let end = END_ENEMY_TURN.contains(&Effect::Fire(trigger));
            let allies = [Actor::Player, Actor::Osty].into_iter().filter(|_| end);
            let enemies = self.participants(content).into_iter().map(Actor::Enemy);
            let listening = |actor: Actor, power: &Power| {
                let def = content.powers[power.id as usize];
                let hears = if matches!(actor, Actor::Enemy(_)) {
                    listens(trigger, def.id, def.hooks)
                        || end && def.kind == PowerKind::OneShot(trigger)
                } else {
                    in_code(trigger, def.id)
                };
                hears.then_some(Listener::Power(actor, *power))
            };
            return (allies.chain(enemies))
                .flat_map(|actor| {
                    (self.creature(actor).powers.iter())
                        .filter_map(move |power| listening(actor, power))
                })
                .collect();
        }
        let combat_start = COMBAT_START.contains(&Effect::Fire(trigger));
        let card_pass = matches!(
            trigger,
            Trigger::AfterCardDrawn
                | Trigger::AfterCardExhausted
                | Trigger::AfterCardGeneratedForCombat
                | Trigger::AfterShuffle
                | Trigger::AfterPotionUsed
        );
        let hit = matches!(
            trigger,
            Trigger::AfterDamageGiven | Trigger::AfterDamageReceived | Trigger::AfterDeath
        );
        if !combat_start
            && !card_pass
            && !hit
            && !(context.source == Actor::Player
                && (PLAYER_TURN.iter().chain(END_PLAYER_TURN)).any(|x| *x == Effect::Fire(trigger)))
        {
            let powers = powers.into_iter();
            let powers = powers.map(|power| Listener::Power(context.source, power));
            return powers
                .chain(relics.filter(|_| context.source == Actor::Player))
                .collect();
        }
        // A card pass keeps the caller's snapshot of the player's powers (Speedster on a hand draw).
        let player = if card_pass {
            powers
        } else {
            self.creature(Actor::Player).powers.clone()
        };
        let hearing = |actor, powers: Vec<Power>| {
            let powers = powers.into_iter().filter(move |power| {
                let def = content.powers[power.id as usize];
                if combat_start || actor == Actor::Player {
                    listens(trigger, def.id, def.hooks) || def.kind == PowerKind::OneShot(trigger)
                } else {
                    in_code(trigger, def.id)
                }
            });
            powers
                .map(move |power| Listener::Power(actor, power))
                .collect::<Vec<_>>()
        };
        let creature = |actor| self.creature(actor).powers.clone();
        let enemies = (0..self.combat().unwrap().enemies.len()).map(Actor::Enemy);
        (hearing(Actor::Player, player).into_iter().chain(relics))
            .chain(self.card_listeners(content, trigger, context))
            .chain(hearing(Actor::Osty, creature(Actor::Osty)))
            .chain(enemies.flat_map(|actor| hearing(actor, creature(actor))))
            .collect()
    }

    /// The player's cards that hear the pass (PlayerCombatState.AllCards: hand, draw pile from
    /// the top, discard and exhaust piles). A card pass is about one card, and only that card
    /// hears its own draw or exhaust (Drum of Battle, Void); an enchantment hears with its card.
    fn card_listeners(
        &self,
        content: &Content,
        trigger: Trigger,
        context: Context,
    ) -> Vec<Listener> {
        let combat = self.combat().unwrap();
        let cards = (combat.hand.iter())
            .chain(combat.draw.iter().rev())
            .chain(&combat.discard)
            .chain(&combat.exhaust);
        let hears = |card: &&Card| {
            let def = content.cards[card.id as usize];
            match context.subject {
                Some(instance) => card.instance == instance && listens(trigger, def.id, def.hooks),
                None => {
                    in_code(trigger, def.id)
                        || trigger == Trigger::AutoPrePlay
                            && card.enchantment == Some(Enchantment::Imbued)
                }
            }
        };
        cards
            .filter(hears)
            .map(|&card| Listener::Card(card))
            .collect()
    }

    /// CreatureCmd.Damage's hooks for one hit: AfterDamageGiven, then AfterDamageReceived unless
    /// the hit killed its target, each listener's reaction resolved before the next one's.
    pub(super) fn hit_hooks(&mut self, content: &Content, hit: Hit) {
        for trigger in [Trigger::AfterDamageGiven, Trigger::AfterDamageReceived] {
            if trigger == Trigger::AfterDamageReceived && self.creature(hit.target).hp <= 0 {
                return;
            }
            let base = self.combat().unwrap().queue.len();
            for listener in self.listeners(content, trigger, Context::player(), vec![]) {
                let hook = std::mem::replace(&mut self.combat_mut().unwrap().applying_hook, true);
                self.hit_hook(content, trigger, listener, hit);
                self.combat_mut().unwrap().applying_hook = hook;
                self.settle(content, base);
            }
        }
    }

    fn set_move(&mut self, index: usize, next: usize, last: usize, repeats: u8) -> &mut Enemy {
        let enemy = &mut self.combat_mut().unwrap().enemies[index];
        enemy.move_index = next;
        enemy.last_move = last;
        enemy.repeats = repeats;
        enemy
    }

    fn hit_hook(&mut self, content: &Content, trigger: Trigger, listener: Listener, hit: Hit) {
        let (owner, id, amount) = match listener {
            Listener::Power(owner, power) => (
                owner,
                content.powers[power.id as usize].id,
                self.creature(owner).power(power.id),
            ),
            Listener::Relic(i) => (
                Actor::Player,
                content.relics[self.run.relics[i as usize] as usize].id,
                0,
            ),
            Listener::Card(_) => return,
        };
        let attack = matches!(hit.kind, DamageKind::Attack);
        let powered = matches!(hit.kind, DamageKind::Attack | DamageKind::Move);
        let mine = matches!(hit.dealer, Actor::Player | Actor::Osty);
        let (dealt, received, lost) = (hit.dealer == owner, hit.target == owner, hit.lost > 0);
        let hurt = hit.receiver == owner && lost;
        let player_turn = !self.combat().unwrap().enemy_turn;
        match (trigger, id) {
            (Trigger::AfterDamageGiven, "POWER.ENVENOM_POWER") if dealt && powered && lost => {
                self.apply_power(content, hit.receiver, power_id::POISON, amount)
            }
            (Trigger::AfterDamageGiven, "POWER.IMBALANCED_POWER") if dealt && hit.fully_blocked => {
                if let Actor::Enemy(index) = owner
                    && content.enemies[self.creature(owner).id as usize].id
                        == "MONSTER.BOWLBUG_ROCK"
                {
                    self.set_move(index, 1, usize::MAX, 0);
                }
            }
            // A creature the hit killed can't receive Monarch's Gaze (CanReceivePowers).
            (Trigger::AfterDamageGiven, "POWER.MONARCHS_GAZE_POWER")
                if dealt
                    && powered
                    && owner == Actor::Player
                    && self.creature(hit.target).hp > 0 =>
            {
                if self.apply_debuff(content, hit.target, power_id::STRENGTH, -amount) {
                    (self.creature_mut(hit.target))
                        .add_power(power_id::MONARCHS_GAZE_STRENGTH_DOWN, amount);
                }
            }
            (Trigger::AfterDamageGiven, "POWER.PAPER_CUTS_POWER")
                if dealt
                    && (attack || matches!(hit.kind, DamageKind::Overflow))
                    && hit.receiver == Actor::Player
                    && lost =>
            {
                let player = self.creature_mut(Actor::Player);
                player.max_hp = (player.max_hp - amount.max(0)).max(1);
                player.hp = player.hp.min(player.max_hp);
                self.run.max_hp = player.max_hp;
            }
            (Trigger::AfterDamageGiven, "POWER.REAPER_FORM_POWER")
                if mine && powered && hit.blocked + hit.lost > 0 =>
            {
                let doom = (hit.blocked + hit.lost).saturating_mul(amount);
                self.apply_power(content, hit.target, power_id::DOOM, doom)
            }
            (Trigger::AfterDamageGiven, "POWER.SIC_EM_POWER")
                if received && hit.dealer == Actor::Osty =>
            {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Summon(Amount::fixed(amount, amount)),
                    context: Context::player(),
                })
            }
            (Trigger::AfterDamageGiven, "RELIC.HAND_DRILL")
                if mine
                    && matches!(hit.target, Actor::Enemy(_))
                    && hit.blocked > 0
                    && self.creature(hit.target).block == 0 =>
            {
                self.apply_power(content, hit.target, power_id::VULNERABLE, 2)
            }
            (Trigger::AfterDamageReceived, "POWER.FLAME_BARRIER_POWER")
                if received && attack && matches!(hit.dealer, Actor::Enemy(_)) =>
            {
                self.damage(
                    content,
                    owner,
                    hit.dealer,
                    amount,
                    DamageKind::Unpowered,
                    None,
                );
            }
            (Trigger::AfterDamageReceived, "POWER.REFLECT_POWER")
                if received
                    && attack
                    && matches!(hit.dealer, Actor::Enemy(_))
                    && hit.blocked > 0 =>
            {
                self.damage(
                    content,
                    owner,
                    hit.dealer,
                    hit.blocked,
                    DamageKind::Unpowered,
                    None,
                );
            }
            (Trigger::AfterDamageReceived, "POWER.INFERNO_POWER") if hurt && player_turn => {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Damage(Target::AllEnemies, Amount::fixed(amount, amount)),
                    context: Context::player(),
                })
            }
            // Strength from HP a played card cost waits until that card finishes.
            (Trigger::AfterDamageReceived, "POWER.RUPTURE_POWER") if hurt && player_turn => {
                let combat = self.combat_mut().unwrap();
                let playing = !combat.plays.is_empty();
                let rupture = (combat.player.powers.iter_mut())
                    .find(|power| power.id == power_id::RUPTURE)
                    .filter(|_| hit.card.is_some() && playing);
                match rupture {
                    Some(rupture) => rupture.value += amount,
                    None => self.apply_power(content, owner, power_id::STRENGTH, amount),
                }
            }
            (Trigger::AfterDamageReceived, "POWER.THE_GAMBIT_POWER")
                if hurt
                    && (attack || matches!(hit.kind, DamageKind::Overflow))
                    && matches!(hit.dealer, Actor::Enemy(_)) =>
            {
                (self.creature_mut(owner).powers).retain(|power| power.id != power_id::THE_GAMBIT);
                self.kill_actor(content, owner);
            }
            // Curl Up's block comes once the card that hit it (Osty's too) finishes.
            (Trigger::AfterDamageReceived, "POWER.CURL_UP_POWER")
                if received && powered && mine && hit.card.is_some() =>
            {
                let Actor::Enemy(index) = owner else { return };
                let curled = &mut self.combat_mut().unwrap().curled;
                if !curled.contains(&index) {
                    curled.push(index);
                }
            }
            (Trigger::AfterDamageReceived, "POWER.FLUTTER_POWER")
                if received && powered && lost =>
            {
                self.creature_mut(owner).consume_power(power_id::FLUTTER);
                if self.creature(owner).power(power_id::FLUTTER) == 0
                    && let Actor::Enemy(index) = owner
                    && content.enemies[self.creature(owner).id as usize].id
                        == "MONSTER.THIEVING_HOPPER"
                {
                    self.set_move(index, 5, 5, 1);
                }
            }
            (Trigger::AfterDamageReceived, "POWER.HARDENED_SHELL_POWER")
                if received && hit.lost != 0 =>
            {
                let powers = &mut self.creature_mut(owner).powers;
                let shell = powers.iter_mut().find(|x| x.id == power_id::HARDENED_SHELL);
                let shell = shell.unwrap();
                shell.value = shell.value.saturating_add(hit.lost);
            }
            // Personal Hive's Dazed has no creator, so generation hooks and Supermassive ignore it.
            (Trigger::AfterDamageReceived, "POWER.PERSONAL_HIVE_POWER")
                if received && powered && mine =>
            {
                for _ in 0..amount {
                    let dazed = Card {
                        id: card_id::DAZED,
                        ..Card::default()
                    };
                    self.insert_random(Pile::Draw, dazed);
                }
            }
            // It loses Strength and every temporary Strength power (TemporaryStrengthPower).
            (Trigger::AfterDamageReceived, "POWER.PLOW_POWER")
                if hurt && self.creature(owner).hp <= amount =>
            {
                let Actor::Enemy(index) = owner else { return };
                self.creature_mut(owner).powers.retain(|power| {
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
                self.set_move(index, 2, 2, 1).move_history.push(2);
            }
            (Trigger::AfterDamageReceived, "POWER.SHRIEK_POWER")
                if hurt && self.creature(owner).hp > 0 && self.creature(owner).hp <= amount =>
            {
                let Actor::Enemy(index) = owner else { return };
                (self.creature_mut(owner).powers).retain(|power| power.id != power_id::SHRIEK);
                // Stunned on its own turn, it keeps the stun past this turn's move roll.
                let held = self.combat().unwrap().enemy_turn;
                let enemy = self.set_move(index, 2, 2, 1);
                enemy.move_history.push(2);
                enemy.stunned |= held;
            }
            (Trigger::AfterDamageReceived, "POWER.SLIPPERY_POWER") if hurt => {
                self.creature_mut(owner).consume_power(power_id::SLIPPERY)
            }
            (Trigger::AfterDamageReceived, "POWER.SLUMBER_POWER")
                if hurt
                    && content.enemies[self.creature(owner).id as usize].id
                        == "MONSTER.SLUMBERING_BEETLE" =>
            {
                let Actor::Enemy(index) = owner else { return };
                self.creature_mut(owner).consume_power(power_id::SLUMBER);
                if self.creature(owner).power(power_id::SLUMBER) == 0 {
                    self.set_move(index, 2, usize::MAX, 0);
                }
            }
            (Trigger::AfterDamageReceived, "POWER.ASLEEP_POWER")
                if hurt
                    && content.enemies[self.creature(owner).id as usize].id
                        == "MONSTER.LAGAVULIN_MATRIARCH" =>
            {
                let Actor::Enemy(index) = owner else { return };
                (self.creature_mut(owner).powers)
                    .retain(|power| !matches!(power.id, power_id::PLATING | power_id::ASLEEP));
                self.set_move(index, 5, usize::MAX, 0);
            }
            (Trigger::AfterDamageReceived, "RELIC.DEMON_TONGUE")
                if hurt && player_turn && !self.combat().unwrap().demon_tongue =>
            {
                self.combat_mut().unwrap().demon_tongue = true;
                let player = self.creature_mut(Actor::Player);
                player.hp = (player.hp + hit.lost).min(player.max_hp);
            }
            (Trigger::AfterDamageReceived, "RELIC.CENTENNIAL_PUZZLE")
                if hurt && !self.combat().unwrap().centennial_puzzle =>
            {
                self.combat_mut().unwrap().centennial_puzzle = true;
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Draw(3),
                    context: Context::player(),
                });
            }
            (Trigger::AfterDamageReceived, "RELIC.SELF_FORMING_CLAY") if hurt => {
                self.apply_power(content, Actor::Player, power_id::SELF_FORMING_CLAY, 3)
            }
            _ => {}
        }
    }

    /// AfterDeath over every creature's listeners, each resolved before the next.
    pub(super) fn death_hooks(&mut self, content: &Content, dead: Actor, prevented: bool) {
        let base = self.combat().unwrap().queue.len();
        for listener in self.listeners(content, Trigger::AfterDeath, Context::player(), vec![]) {
            self.death_hook(content, listener, dead, prevented);
            self.settle(content, base);
        }
    }

    /// One AfterDeath listener's reaction to `dead`'s death, `prevented` its removal.
    fn death_hook(&mut self, content: &Content, listener: Listener, dead: Actor, prevented: bool) {
        let enemy_id = |game: &Self, actor| content.enemies[game.creature(actor).id as usize].id;
        let living = |game: &Self, id| {
            (game.combat().unwrap().enemies.iter()).any(|enemy| {
                enemy.creature.hp > 0 && content.enemies[enemy.creature.id as usize].id == id
            })
        };
        let (owner, power) = match listener {
            Listener::Power(owner, power) => (owner, power),
            Listener::Relic(i) => {
                let id = content.relics[self.run.relics[i as usize] as usize].id;
                if id == "RELIC.GREMLIN_HORN" && matches!(dead, Actor::Enemy(_)) {
                    for effect in [Effect::Energy(1), Effect::Draw(1)] {
                        let context = Context::player();
                        self.apply(content, Pending { effect, context });
                    }
                }
                return;
            }
            Listener::Card(card) => {
                let combat = self.combat_mut().unwrap();
                let piles = (combat.draw.iter_mut())
                    .chain(&mut combat.hand)
                    .chain(&mut combat.discard)
                    .chain(&mut combat.exhaust);
                for card in piles.filter(|other| other.instance == card.instance && !prevented) {
                    card.cost_delta = card.cost_delta.saturating_sub(1);
                }
                return;
            }
        };
        let Actor::Enemy(index) = dead else {
            return;
        };
        let mine = owner == dead;
        let other = !mine && self.creature(owner).hp > 0 && matches!(owner, Actor::Enemy(_));
        let strip = |game: &mut Self, keep: &dyn Fn(&Power) -> bool| {
            let creature = &mut game.combat_mut().unwrap().enemies[index].creature;
            creature.block = 0;
            creature.powers.retain(|power| keep(power));
        };
        match content.powers[power.id as usize].id {
            "POWER.SHRINK_POWER" if enemy_id(self, dead) == "MONSTER.SHRINKER_BEETLE" => {
                (self.creature_mut(Actor::Player).powers)
                    .retain(|power| power.id != power_id::SHRINK || power.amount > 0)
            }
            "POWER.HEX_POWER" if enemy_id(self, dead) == "MONSTER.SPECTRAL_KNIGHT" => {
                (self.creature_mut(Actor::Player).powers).retain(|power| power.id != power_id::HEX)
            }
            "POWER.DAMPEN_POWER"
                if enemy_id(self, dead) == "MONSTER.MAGI_KNIGHT"
                    && !living(self, "MONSTER.MAGI_KNIGHT") =>
            {
                // Its restoring upgrades are skipped once combat is ending (CardCmd.Upgrade).
                let mut dampened = std::mem::take(&mut self.combat_mut().unwrap().dampened);
                if self.enemies_defeated() {
                    dampened.clear();
                }
                let combat = self.combat_mut().unwrap();
                for card in combat
                    .draw
                    .iter_mut()
                    .chain(&mut combat.hand)
                    .chain(&mut combat.discard)
                    .chain(&mut combat.exhaust)
                    .chain(played(&mut combat.queue))
                    .chain(combat.plays.iter_mut().map(|play| &mut play.card))
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
                if let Some(card) =
                    (combat.plays.first().map(|play| play.card)).filter(|card| card.upgrades > 0)
                {
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
            }
            "POWER.CONSTRICT_POWER" => (self.creature_mut(Actor::Player).powers)
                .retain(|power| power.id != power_id::CONSTRICT || power.value != index as i16 + 1),
            "POWER.RAVENOUS_POWER" if other && !prevented => {
                self.creature_mut(owner)
                    .add_power(power_id::STRENGTH, power.amount);
                let Actor::Enemy(owner) = owner else { return };
                self.combat_mut().unwrap().enemies[owner].stunned = true;
            }
            "POWER.CRAB_RAGE_POWER" if other => {
                let creature = self.creature_mut(owner);
                creature.add_power(power_id::STRENGTH, 6);
                creature.block = creature.block.saturating_add(99);
                creature
                    .powers
                    .retain(|power| power.id != power_id::CRAB_RAGE);
            }
            // Death strips its other powers; Steam Eruption stays until the About to Blow move
            // stores it as the blast.
            "POWER.STEAM_ERUPTION_POWER" if mine && prevented => {
                let creature = self.creature_mut(dead);
                creature
                    .powers
                    .retain(|power| power.id == power_id::STEAM_ERUPTION);
                creature.hp = i16::MAX;
                creature.max_hp = i16::MAX;
                let enemy = self.set_move(index, 7, 7, 1);
                enemy.move_history.push(7);
                enemy.value = power.amount;
            }
            "POWER.ADAPTABLE_POWER" if mine && enemy_id(self, dead) == "MONSTER.TEST_SUBJECT" => {
                strip(self, &|power| {
                    matches!(power.id, power_id::ADAPTABLE | power_id::PAINFUL_STABS)
                });
                self.set_move(index, 0, 0, 1).move_history.push(0);
            }
            // Illusions keep buffs (Strength at any amount) and temporary debuffs.
            "POWER.ILLUSION_POWER" if mine => {
                strip(self, &|power| {
                    !content.powers[power.id as usize].debuff
                        || TEMPORARY_STRENGTH_DOWN.contains(&power.id)
                });
                self.set_move(index, 1, 1, 1);
            }
            "POWER.REATTACH_POWER" if mine => {
                strip(self, &|power| power.id == power_id::REATTACH);
                if (self.combat().unwrap().enemies.iter().enumerate()).any(|(other, enemy)| {
                    other != index
                        && enemy.creature.hp > 0
                        && enemy.creature.power(power_id::REATTACH) > 0
                }) {
                    self.set_move(index, 3, 3, 1);
                }
            }
            "POWER.POSSESS_STRENGTH_POWER" if mine => self
                .creature_mut(Actor::Player)
                .add_power(power_id::STRENGTH, power.value),
            "POWER.POSSESS_SPEED_POWER" if mine => self
                .creature_mut(Actor::Player)
                .add_power(power_id::DEXTERITY, power.value),
            // Its spawns still see it among the enemies (unique HP rolls).
            "POWER.STOCK_POWER" if mine && !prevented && power.amount > 0 => {
                self.corpse_hp = Some(self.creature(dead).max_hp);
                let spawned = self.spawn_enemy(content, "MONSTER.AXEBOT");
                self.corpse_hp = None;
                let creature = self.creature_mut(Actor::Enemy(spawned));
                creature.powers.retain(|power| power.id != power_id::STOCK);
                if power.amount > 1 {
                    creature.add_power(power_id::STOCK, power.amount - 1);
                }
                self.set_move(spawned, 0, usize::MAX, 0)
                    .move_history
                    .clear();
            }
            "POWER.SURPRISE_POWER" if mine && !prevented => {
                let stolen = (self.creature(dead).powers.iter())
                    .filter(|power| power.id == power_id::THIEVERY)
                    .map(|power| power.value)
                    .sum();
                self.corpse_hp = Some(self.creature(dead).max_hp);
                self.spawn_enemy(content, "MONSTER.SNEAKY_GREMLIN");
                let fat = self.spawn_enemy(content, "MONSTER.FAT_GREMLIN");
                self.corpse_hp = None;
                if stolen > 0 {
                    self.creature_mut(Actor::Enemy(fat))
                        .add_power(power_id::HEIST, stolen);
                }
            }
            "POWER.INFESTED_POWER" if mine && !prevented => {
                self.spawn_wrigglers(content, power.amount)
            }
            _ => {}
        }
    }

    pub(super) fn listen(
        &mut self,
        content: &Content,
        trigger: Trigger,
        listener: Listener,
        context: Context,
    ) {
        let (owner, power) = match listener {
            Listener::Power(owner, power) => (owner, power),
            Listener::Card(card) => {
                match (trigger, content.cards[card.id as usize].id) {
                    // Played last turn, it returns to the hand from wherever it is.
                    (Trigger::BeforeHandDraw, "CARD.BOLAS" | "CARD.THRUMMING_HATCHET")
                        if card.value > 0 =>
                    {
                        let combat = self.combat().unwrap();
                        let pile = [Pile::Draw, Pile::Discard, Pile::Exhaust]
                            .into_iter()
                            .find_map(|pile| {
                                (cards(combat, pile).iter())
                                    .position(|other| other.instance == card.instance)
                                    .map(|index| (pile, index))
                            });
                        if let Some((pile, index)) = pile {
                            let card = self.remove_pile(pile, index);
                            self.add_to_hand(Card { value: 0, ..card });
                        }
                    }
                    // Turn 1's Imbued cards play themselves, each leaving its pile as it starts.
                    (Trigger::AutoPrePlay, _) if self.combat().unwrap().turn == 1 => {
                        self.combat_mut().unwrap().queue.push(Pending {
                            effect: Effect::AutoPlay(card),
                            context: Context::player(),
                        })
                    }
                    // Its exhaust energy goes through GeneratePlayCount: replays and play-count
                    // powers (Duplication, Burst, Echo Form, Throwing Axe) multiply it and are spent.
                    (Trigger::AfterCardExhausted, "CARD.DRUM_OF_BATTLE") => {
                        let throwing_axe = self.has_relic(content, "RELIC.THROWING_AXE")
                            && !self.combat().unwrap().throwing_axe;
                        let combat = self.combat_mut().unwrap();
                        combat.throwing_axe |= throwing_axe;
                        let plays = combat.play_count(content, card, throwing_axe);
                        let energy = if card.upgrades > 0 { 3 } else { 2 };
                        combat.queue.extend((0..plays).map(|_| Pending {
                            effect: Effect::Energy(energy),
                            context: Context::player(),
                        }));
                    }
                    // It costs 0 until played once its owner creates a Status card.
                    (Trigger::AfterCardGeneratedForCombat, "CARD.ROCKET_PUNCH") => {
                        let generated = context.card.map(|id| content.cards[id as usize]);
                        if generated.is_some_and(|def| def.card_type == CardType::Status) {
                            let combat = self.combat_mut().unwrap();
                            let pile = (combat.hand.iter_mut())
                                .chain(&mut combat.draw)
                                .chain(&mut combat.discard)
                                .chain(&mut combat.exhaust)
                                .find(|other| other.instance == card.instance);
                            if let Some(card) = pile {
                                card.cost_override = Some(0);
                                card.when_played = true;
                            }
                        }
                    }
                    (Trigger::AutoPrePlay | Trigger::BeforeHandDraw, _) => {}
                    _ => self.trigger_card(content, card, trigger),
                }
                return;
            }
            Listener::Relic(i) => {
                let id = self.run.relics[i as usize];
                let turn = self.combat().unwrap().turn;
                let mut effects = Vec::new();
                match (trigger, content.relics[id as usize].id) {
                    (Trigger::AfterRoomEntered, "RELIC.EMBER_TEA") if self.ember_tea < 5 => {
                        self.ember_tea += 1;
                        self.apply_power(content, Actor::Player, power_id::STRENGTH, 2);
                    }
                    (Trigger::AfterRoomEntered, "RELIC.GIRYA") if self.girya > 0 => {
                        let lifts = self.girya as i16;
                        self.apply_power(content, Actor::Player, power_id::STRENGTH, lifts);
                    }
                    (Trigger::AfterRoomEntered, "RELIC.GORGET") => {
                        self.apply_power(content, Actor::Player, power_id::PLATING, 4)
                    }
                    (Trigger::AfterRoomEntered, "RELIC.PHILOSOPHERS_STONE") => {
                        for enemy in &mut self.combat_mut().unwrap().enemies {
                            enemy.creature.add_power(power_id::STRENGTH, 1);
                        }
                    }
                    (Trigger::AfterRoomEntered, "RELIC.RED_SKULL")
                        if self.creature(Actor::Player).hp * 2
                            <= self.creature(Actor::Player).max_hp =>
                    {
                        self.combat_mut().unwrap().red_skull = true;
                        self.apply_power(content, Actor::Player, power_id::STRENGTH, 3);
                    }
                    (Trigger::AfterRoomEntered, "RELIC.SLING_OF_COURAGE")
                        if self.room == Room::Elite =>
                    {
                        self.apply_power(content, Actor::Player, power_id::STRENGTH, 2)
                    }
                    (Trigger::AfterRoomEntered, "RELIC.STONE_CRACKER") => {
                        let mut cards: Vec<_> = (self.combat().unwrap().draw.iter().rev())
                            .copied()
                            .filter(|card| {
                                card.upgrades == 0
                                    && !matches!(
                                        card.card_type(content.cards[card.id as usize]),
                                        CardType::Status | CardType::Curse | CardType::Quest
                                    )
                            })
                            .collect();
                        sort_for_shuffle(&mut cards, content);
                        self.rngs.combat_card_selection.shuffle(&mut cards);
                        for card in cards.into_iter().take(2) {
                            let draw = &mut self.combat_mut().unwrap().draw;
                            let card = draw.iter_mut().find(|x| x.instance == card.instance);
                            card.unwrap().upgrades = 1;
                        }
                    }
                    (Trigger::AfterRoomEntered, "RELIC.SWORD_OF_JADE") => {
                        self.apply_power(content, Actor::Player, power_id::STRENGTH, 3)
                    }
                    (Trigger::BeforeCombatStart, "RELIC.BELT_BUCKLE")
                        if self.run.potions.iter().all(Option::is_none) =>
                    {
                        self.combat_mut().unwrap().belt_buckle = true;
                        self.apply_power(content, Actor::Player, power_id::DEXTERITY, 2);
                    }
                    (Trigger::BeforeCombatStart, "RELIC.DELICATE_FROND") => {
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
                    (Trigger::BeforeCombatStart, "RELIC.SNECKO_EYE" | "RELIC.FAKE_SNECKO_EYE") => {
                        self.apply_power(content, Actor::Player, power_id::CONFUSED, 1)
                    }
                    (Trigger::BeforeCombatStart, "RELIC.TEA_OF_DISCOURTESY")
                        if !self.tea_of_discourtesy =>
                    {
                        self.tea_of_discourtesy = true;
                        for _ in 0..2 {
                            let dazed = Card {
                                id: card_id::DAZED,
                                ..Card::default()
                            };
                            self.add_random(content, Pile::Draw, dazed);
                        }
                    }
                    (Trigger::BeforeSideTurnStart, "RELIC.POLLINOUS_CORE") => {
                        self.pollinous_core += 1
                    }
                    // Two 0-cost draw-pile cards to the hand: a StableShuffle of the pile listed from
                    // its top.
                    (Trigger::BeforeSideTurnStart, "RELIC.POWER_CELL") if turn == 1 => {
                        let mut picks: Vec<_> = (self.combat().unwrap().draw.iter().rev())
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
                            let index = draw.iter().position(|x| x.instance == card.instance);
                            let card = self.combat_mut().unwrap().remove_draw(index.unwrap());
                            self.add_to_hand(card);
                        }
                    }
                    (Trigger::AfterBlockCleared, "RELIC.CAPTAINS_WHEEL") if turn == 3 => {
                        self.gain_block(content, Actor::Player, 18, None, false);
                    }
                    (Trigger::AfterBlockCleared, "RELIC.HORN_CLEAT") if turn == 2 => {
                        self.gain_block(content, Actor::Player, 14, None, false);
                    }
                    (Trigger::AfterBlockCleared, "RELIC.SPARKLING_ROUGE") if turn == 3 => {
                        let player = self.creature_mut(Actor::Player);
                        player.add_power(power_id::STRENGTH, 1);
                        player.add_power(power_id::DEXTERITY, 1);
                    }
                    (
                        Trigger::AfterEnergyReset,
                        "RELIC.VENERABLE_TEA_SET" | "RELIC.FAKE_VENERABLE_TEA_SET",
                    ) => {
                        let energy = std::mem::take(&mut self.tea_set) as i16;
                        self.gain_energy(energy);
                    }
                    (Trigger::AfterEnergyReset, "RELIC.ART_OF_WAR")
                        if turn > 1 && self.combat().unwrap().last_turn.attacks == 0 =>
                    {
                        self.gain_energy(1)
                    }
                    (
                        Trigger::BeforeHandDraw,
                        relic @ ("RELIC.BLESSED_ANTLER" | "RELIC.FUNERARY_MASK"),
                    ) if turn == 1 => {
                        let card = if relic == "RELIC.BLESSED_ANTLER" {
                            "CARD.DAZED"
                        } else {
                            "CARD.SOUL"
                        };
                        if let Some(id) = content.card_id(card) {
                            for _ in 0..3 {
                                let card = Card {
                                    id,
                                    ..Card::default()
                                };
                                self.add_random(content, Pile::Draw, card);
                            }
                        }
                    }
                    (Trigger::BeforeHandDraw, "RELIC.TOASTY_MITTENS") => {
                        self.toasty_mittens(content)
                    }
                    (Trigger::BeforeHandDraw, "RELIC.JEWELED_MASK") if turn == 1 => {
                        effects.push(Effect::RandomCardOp(
                            Pile::Draw,
                            CardFilter::Type(CardType::Power),
                            CardOp::MoveFree(Pile::Hand),
                            Amount::fixed(1, 1),
                        ))
                    }
                    (Trigger::BeforeHandDraw, "RELIC.RADIANT_PEARL") if turn == 1 => effects
                        .extend(
                            (content.card_id("CARD.LUMINESCE"))
                                .map(|card| Effect::AddCard(Pile::Hand, card, 1)),
                        ),
                    (Trigger::BeforeHandDraw, "RELIC.TOOLBOX") if turn == 1 => {
                        effects.push(Effect::OfferColorless(3, false, false))
                    }
                    (Trigger::BeforeHandDraw, "RELIC.NINJA_SCROLL") if turn == 1 => effects.extend(
                        (content.card_id("CARD.SHIV"))
                            .map(|shiv| Effect::AddCard(Pile::Hand, shiv, 3)),
                    ),
                    (Trigger::AfterPlayerTurnStart, "RELIC.MR_STRUGGLES") => effects.push(
                        Effect::Damage(Target::AllEnemies, Amount::fixed(turn as i16, turn as i16)),
                    ),
                    // On turn 1's AfterPlayerTurnStart, after Fur Coat's HP set.
                    (Trigger::AfterPlayerTurnStart, "RELIC.FESTIVE_POPPER") if turn == 1 => {
                        effects.push(Effect::Damage(Target::AllEnemies, Amount::fixed(9, 9)))
                    }
                    (Trigger::AfterPlayerTurnStart, "RELIC.MERCURY_HOURGLASS") => {
                        effects.push(Effect::Damage(Target::AllEnemies, Amount::fixed(3, 3)))
                    }
                    (Trigger::AfterPlayerTurnStart, "RELIC.BELLOWS") if turn == 1 => {
                        effects.push(Effect::Upgrade(Pile::Hand, u8::MAX, false))
                    }
                    (Trigger::AfterPlayerTurnStart, "RELIC.EMOTION_CHIP")
                        if self.combat().unwrap().last_turn.hp_lost
                            + self.combat().unwrap().last_turn.unblocked_hits
                            > 0 =>
                    {
                        effects.push(Effect::PassiveAll)
                    }
                    (Trigger::AfterPlayerTurnStart, "RELIC.PENDULUM") => {
                        self.pendulum = (self.pendulum + 1) % 3;
                        if self.pendulum == 0 {
                            effects.push(Effect::Draw(1));
                        }
                    }
                    (Trigger::AfterPlayerTurnStart, "RELIC.ROYAL_POISON") if turn == 1 => {
                        effects.push(Effect::LoseHp(Target::Player, Amount::fixed(4, 4)))
                    }
                    (Trigger::AfterPlayerTurnStart, "RELIC.VEXING_PUZZLEBOX") if turn == 1 => {
                        effects.push(Effect::DistinctCharacter(Pile::Hand, 1, true))
                    }
                    (Trigger::AfterPlayerTurnStart, "RELIC.CHOICES_PARADOX") if turn == 1 => {
                        effects.push(Effect::OfferCharacterRetain(5))
                    }
                    (Trigger::AfterPlayerTurnStart, "RELIC.GAMBLING_CHIP") if turn == 1 => effects
                        .push(Effect::Select(
                            Pile::Hand,
                            CardFilter::Any,
                            [u8::MAX, u8::MAX],
                            false,
                            true,
                            CardOp::DiscardDraw,
                        )),
                    (
                        Trigger::AfterPlayerTurnStartLate,
                        relic @ ("RELIC.BLOOD_VIAL" | "RELIC.FAKE_BLOOD_VIAL"),
                    ) if turn == 1 => {
                        let heal = if relic == "RELIC.BLOOD_VIAL" { 2 } else { 1 };
                        effects.push(Effect::Heal(Target::Player, Amount::fixed(heal, heal)))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.BONE_TEA")
                        if turn == 1 && !self.bone_tea =>
                    {
                        self.bone_tea = true;
                        effects.push(Effect::Upgrade(Pile::Hand, u8::MAX, false));
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.CROSSBOW") => {
                        effects.push(Effect::RandomCard(Pile::Hand, CardType::Attack, 1, true))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.SAI") => {
                        effects.push(Effect::RawBlock(Target::Player, Amount::fixed(7, 7)))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.BRIMSTONE") => effects.extend([
                        Effect::ApplyPower(Target::Player, power_id::STRENGTH, Amount::fixed(2, 2)),
                        Effect::ApplyPower(
                            Target::AllEnemies,
                            power_id::STRENGTH,
                            Amount::fixed(1, 1),
                        ),
                    ]),
                    (Trigger::AfterSideTurnStart, "RELIC.AKABEKO") if turn == 1 => effects.push(
                        Effect::ApplyPower(Target::Player, power_id::VIGOR, Amount::fixed(8, 8)),
                    ),
                    (Trigger::AfterSideTurnStart, "RELIC.SEAL_OF_GOLD") if self.run.gold >= 5 => {
                        effects.extend([Effect::Gold(Amount::fixed(-5, -5)), Effect::Energy(1)])
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.ORANGE_DOUGH") if turn == 1 => {
                        effects.push(Effect::DistinctColorless(Pile::Hand, 2, false))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.FENCING_MANUAL") if turn == 1 => {
                        effects.push(Effect::Forge(Amount::fixed(10, 10)))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.LANTERN") if turn == 1 => {
                        effects.push(Effect::Energy(1))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.VERY_HOT_COCOA") if turn == 1 => {
                        effects.push(Effect::Energy(4))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.CANDELABRA") if turn == 2 => {
                        effects.push(Effect::Energy(2))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.CHANDELIER") if turn == 3 => {
                        effects.push(Effect::Energy(3))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.BOOMING_CONCH")
                        if turn == 1 && self.room == Room::Elite =>
                    {
                        effects.push(Effect::Energy(1))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.BREAD") if turn == 1 => {
                        effects.push(Effect::Energy(-2))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.PAELS_LEGION") => {
                        let combat = self.combat_mut().unwrap();
                        combat.paels_legion = combat.paels_legion.saturating_sub(1);
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.LETTER_OPENER") if turn > 1 => {
                        self.combat_mut().unwrap().letter_opener = 0
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.HAPPY_FLOWER") => {
                        self.happy_flower = (self.happy_flower + 1) % 3;
                        if self.happy_flower == 0 {
                            effects.push(Effect::Energy(1));
                        }
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.FAKE_HAPPY_FLOWER") => {
                        self.fake_happy_flower = (self.fake_happy_flower + 1) % 5;
                        if self.fake_happy_flower == 0 {
                            effects.push(Effect::Energy(1));
                        }
                    }
                    (Trigger::BeforeSideTurnEnd, "RELIC.PAELS_TEARS") => {
                        let energy = self.combat().unwrap().energy;
                        self.combat_mut().unwrap().paels_tears = energy > 0;
                    }
                    // Orichalcum notes whether the player has no block.
                    (
                        Trigger::BeforeSideTurnEndVeryEarly,
                        "RELIC.ORICHALCUM" | "RELIC.FAKE_ORICHALCUM",
                    ) => {
                        let block = self.creature(Actor::Player).block;
                        self.combat_mut().unwrap().orichalcum = block == 0;
                    }
                    (Trigger::BeforeSideTurnEnd, "RELIC.ORICHALCUM")
                        if self.combat().unwrap().orichalcum =>
                    {
                        self.gain_block(content, Actor::Player, 6, None, false);
                    }
                    (Trigger::BeforeSideTurnEnd, "RELIC.FAKE_ORICHALCUM")
                        if self.combat().unwrap().orichalcum =>
                    {
                        self.gain_block(content, Actor::Player, 3, None, false);
                    }
                    (Trigger::BeforeSideTurnEnd, "RELIC.SCREAMING_FLAGON")
                        if self.combat().unwrap().hand.is_empty() =>
                    {
                        effects.push(Effect::Damage(Target::AllEnemies, Amount::fixed(20, 20)))
                    }
                    (Trigger::BeforeSideTurnEnd, "RELIC.DIAMOND_DIADEM")
                        if self.combat().unwrap().history.cards <= 2 =>
                    {
                        self.apply_power(content, Actor::Player, power_id::DIAMOND_DIADEM, 1)
                    }
                    (Trigger::BeforeSideTurnEnd, "RELIC.RIPPLE_BASIN")
                        if self.combat().unwrap().history.attacks == 0 =>
                    {
                        self.gain_block(content, Actor::Player, 4, None, false);
                    }
                    (Trigger::BeforeSideTurnEnd, "RELIC.STONE_CALENDAR") if turn == 7 => {
                        effects.push(Effect::Damage(Target::AllEnemies, Amount::fixed(52, 52)))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.PAELS_TEARS")
                        if self.combat().unwrap().paels_tears =>
                    {
                        effects.push(Effect::Energy(2))
                    }
                    (Trigger::AfterSideTurnStart, "RELIC.BIG_HAT") if turn == 1 => {
                        let mut pool: Vec<_> = (content.characters[self.run.character as usize]
                            .cards)
                            .iter()
                            .copied()
                            .filter(|&id| {
                                let card = content.cards[id as usize];
                                card.flags[0] & ETHEREAL != 0
                                    && card.flags[0] & NO_GENERATE == 0
                                    && matches!(
                                        card.rarity,
                                        CardRarity::Common
                                            | CardRarity::Uncommon
                                            | CardRarity::Rare
                                    )
                            })
                            .collect();
                        self.rngs.combat_card_generation.shuffle(&mut pool);
                        for id in pool.into_iter().take(2) {
                            let card = Card {
                                id,
                                ..Card::default()
                            };
                            self.add_generated(content, Pile::Hand, card);
                        }
                    }
                    (Trigger::AutoPrePlay, "RELIC.HISTORY_COURSE") => {
                        if let Some(mut card) = self.combat_mut().unwrap().last_turn_card.take() {
                            // It dupes the card as it is now (CreateDupe): a Dampen applied since
                            // has downgraded it.
                            let combat = self.combat().unwrap();
                            if let Some(current) = (combat.draw.iter())
                                .chain(&combat.hand)
                                .chain(&combat.discard)
                                .chain(&combat.exhaust)
                                .find(|other| other.instance == card.instance)
                            {
                                card.upgrades = current.upgrades;
                            }
                            card.instance = 0;
                            // It clones the card as it is when played: last turn's discounts are
                            // gone (a Stomp gets this turn's in begin_auto_play).
                            card.free = false;
                            card.flags |= REPLAY | DUPE;
                            effects.push(Effect::AutoPlay(card));
                        }
                    }
                    (Trigger::AutoPrePlayLate, "RELIC.WHISPERING_EARRING") if turn == 1 => {
                        effects.push(Effect::WhisperingEarring(0))
                    }
                    (Trigger::BeforeSideTurnEndEarly, "RELIC.PAELS_EYE") => {
                        if self.takes_extra_turn(content) {
                            let combat = self.combat_mut().unwrap();
                            let cards = std::mem::take(&mut combat.hand);
                            combat.history.exhausted += cards.len() as i16;
                            for card in cards {
                                self.combat_mut().unwrap().exhaust.push(card);
                                self.exhausted(content, card, false);
                            }
                        }
                    }
                    (Trigger::AfterSideTurnEnd, "RELIC.JOSS_PAPER") => {
                        let total = self.joss_paper + std::mem::take(&mut self.joss_paper_ethereal);
                        self.joss_paper = total % 5;
                        if total >= 5 {
                            let draws = (total / 5) as i16;
                            effects.push(Effect::DrawAmount(Amount::fixed(draws, draws)));
                        }
                    }
                    (Trigger::AfterSideTurnEnd, "RELIC.LUNAR_PASTRY") => {
                        effects.push(Effect::Stars(Amount::fixed(1, 1)))
                    }
                    (Trigger::AfterSideTurnEnd, "RELIC.PARRYING_SHIELD")
                        if self.creature(Actor::Player).block >= 10 =>
                    {
                        effects.push(Effect::Damage(Target::RandomEnemy, Amount::fixed(6, 6)))
                    }
                    (Trigger::AfterCardExhausted, "RELIC.JOSS_PAPER") if context.ethereal => {
                        self.joss_paper_ethereal += 1
                    }
                    (Trigger::AfterCardExhausted, "RELIC.JOSS_PAPER") => {
                        self.joss_paper = (self.joss_paper + 1) % 5;
                        if self.joss_paper == 0 {
                            effects.push(Effect::Draw(1));
                        }
                    }
                    (Trigger::AfterCardExhausted, "RELIC.BURNING_STICKS")
                        if !self.combat().unwrap().burning_sticks =>
                    {
                        let combat = self.combat().unwrap();
                        let card = (combat.exhaust.iter())
                            .chain(&combat.hand)
                            .chain(&combat.draw)
                            .chain(&combat.discard)
                            .find(|card| Some(card.instance) == context.subject)
                            .copied();
                        if let Some(card) = card.filter(|card| {
                            card.card_type(content.cards[card.id as usize]) == CardType::Skill
                        }) {
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
                    }
                    (Trigger::AfterPotionUsed, "RELIC.BELT_BUCKLE")
                        if self.run.potions.iter().all(Option::is_none)
                            && !self.combat().unwrap().belt_buckle =>
                    {
                        self.combat_mut().unwrap().belt_buckle = true;
                        effects.push(Effect::ApplyPower(
                            Target::Player,
                            power_id::DEXTERITY,
                            Amount::fixed(2, 2),
                        ));
                    }
                    (Trigger::AfterPotionUsed, "RELIC.REPTILE_TRINKET") => {
                        effects.extend([power_id::STRENGTH, power_id::REPTILE_TRINKET].map(
                            |power| Effect::ApplyPower(Target::Player, power, Amount::fixed(3, 3)),
                        ))
                    }
                    (Trigger::BeforeCardPlayed, "RELIC.INTIMIDATING_HELMET")
                        if self.combat().unwrap().plays.last().unwrap().energy >= 2 =>
                    {
                        effects.push(Effect::RawBlock(Target::Player, Amount::fixed(4, 4)))
                    }
                    (Trigger::BeforeCardPlayed, "RELIC.PEN_NIB")
                        if self.playing(content, CardType::Attack) =>
                    {
                        self.pen_nib = (self.pen_nib + 1) % 10
                    }
                    (Trigger::BeforeCardPlayed, "RELIC.MUSIC_BOX")
                        if self.playing(content, CardType::Attack)
                            && !self.combat().unwrap().music_box
                            && self.combat().unwrap().music_box_card.is_none() =>
                    {
                        let combat = self.combat_mut().unwrap();
                        combat.music_box_card = Some(combat.plays.last().unwrap().card.instance);
                    }
                    // It finishes after the first play of the card it doubled.
                    (_, "RELIC.UNSETTLING_LAMP")
                        if context.card.is_some()
                            && self.combat().unwrap().unsettling_lamp == context.card =>
                    {
                        let combat = self.combat_mut().unwrap();
                        combat.unsettling_lamp = None;
                        combat.unsettling_used = true;
                    }
                    // It copies its attack as the play left it so far (after Razor Tooth's
                    // upgrade if that relic comes first).
                    (Trigger::AttackPlayed, "RELIC.MUSIC_BOX")
                        if context.subject.is_some()
                            && self.combat().unwrap().music_box_card == context.subject =>
                    {
                        let combat = self.combat_mut().unwrap();
                        combat.music_box = true;
                        combat.music_box_card = None;
                        if let Some(&mut card) = self.played_mut(context.subject.unwrap()) {
                            let copy = Card {
                                instance: 0,
                                flags: card.flags | ETHEREAL,
                                ..card
                            };
                            self.add_generated(content, Pile::Hand, copy);
                        }
                    }
                    // It upgrades an attack or skill as each of its plays finishes.
                    (_, "RELIC.RAZOR_TOOTH") => {
                        if let Some(card) = context.subject.and_then(|x| self.played_mut(x))
                            && card.upgrades == 0
                            && matches!(
                                card.card_type(content.cards[card.id as usize]),
                                CardType::Attack | CardType::Skill
                            )
                        {
                            card.upgrades = 1;
                        }
                    }
                    (_, "RELIC.HELICAL_DART")
                        if context.card.is_some_and(|card| {
                            content.cards[card as usize].tags & SHIV_TAG != 0
                        }) =>
                    {
                        self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
                        self.apply_power(content, Actor::Player, power_id::HELICAL_DART, 1);
                    }
                    (Trigger::AttackPlayed, "RELIC.NUNCHAKU") => {
                        self.nunchaku = (self.nunchaku + 1) % 10;
                        if self.nunchaku == 0 {
                            self.gain_energy(1);
                        }
                    }
                    (Trigger::SkillPlayed, "RELIC.LETTER_OPENER") => {
                        let combat = self.combat_mut().unwrap();
                        combat.letter_opener = (combat.letter_opener + 1) % 3;
                        if combat.letter_opener == 0 {
                            effects.push(Effect::Damage(Target::AllEnemies, Amount::fixed(5, 5)));
                        }
                    }
                    (_, "RELIC.IRON_CLUB") => {
                        self.iron_club = (self.iron_club + 1) % 4;
                        if self.iron_club == 0 {
                            effects.push(Effect::Draw(1));
                        }
                    }
                    (Trigger::SkillPlayed, "RELIC.TUNING_FORK") => {
                        self.tuning_fork = (self.tuning_fork + 1) % 10;
                        if self.tuning_fork == 0 {
                            self.gain_block(content, Actor::Player, 7, None, false);
                        }
                    }
                    // Once per turn, after an attack, a skill and a power have all finished.
                    (_, "RELIC.RAINBOW_RING")
                        if !self.combat().unwrap().rainbow_ring
                            && [CardType::Attack, CardType::Skill, CardType::Power]
                                .into_iter()
                                .all(|kind| self.played_this_turn(content, kind) > 0) =>
                    {
                        self.combat_mut().unwrap().rainbow_ring = true;
                        self.apply_power(content, Actor::Player, power_id::STRENGTH, 1);
                        self.apply_power(content, Actor::Player, power_id::DEXTERITY, 1);
                    }
                    (Trigger::PowerPlayed, "RELIC.PERMAFROST")
                        if !self.combat().unwrap().permafrost =>
                    {
                        self.combat_mut().unwrap().permafrost = true;
                        self.gain_block(content, Actor::Player, 7, None, false);
                    }
                    (Trigger::BeforeCombatStartLate, "RELIC.PETRIFIED_TOAD")
                        if !self.has_relic(content, "RELIC.SOZU") =>
                    {
                        if let Some(id) = content.potion_id("POTION.POTION_SHAPED_ROCK")
                            && let Some(slot) =
                                self.run.potions.iter_mut().find(|slot| slot.is_none())
                        {
                            *slot = Some(id);
                        }
                    }
                    _ => {}
                }
                let queue = &mut self.combat_mut().unwrap().queue;
                push_effects(queue, &effects, Context::player());
                for hook in content.relics[id as usize]
                    .hooks
                    .iter()
                    .rev()
                    .filter(|x| hears(x.trigger, trigger))
                {
                    let context = Context {
                        hook: true,
                        off_pile: false,
                        ..context
                    };
                    push_effects(&mut self.combat_mut().unwrap().queue, hook.effects, context);
                }
                return;
            }
        };
        let turn = self.combat().unwrap().turn;
        let amount = power.amount.clamp(0, u8::MAX as i16) as u8;
        let fixed = Amount::fixed(power.amount, power.amount);
        let player = owner == Actor::Player;
        // The owner's side is starting or ending its turn (the participants of a side-turn pass).
        let participant = matches!(owner, Actor::Enemy(_)) == self.combat().unwrap().enemy_turn;
        let context = match owner {
            Actor::Enemy(i) if context.source != owner => Context {
                source: owner,
                target: Some(i),
                ..context
            },
            _ => context,
        };
        let mut effects = Vec::new();
        match (trigger, content.powers[power.id as usize].id) {
            (Trigger::BeforeSideTurnStart, "POWER.HARDENED_SHELL_POWER") => {
                let powers = &mut self.creature_mut(owner).powers;
                if let Some(shell) = powers.iter_mut().find(|x| x.id == power.id) {
                    shell.value = 0;
                }
            }
            (Trigger::BeforeSideTurnStart, "POWER.AGGRESSION_POWER") if player => effects.push(
                Effect::Aggression(Amount::scaled(Scale::Power(power.id), 1)),
            ),
            (Trigger::AfterBlockCleared, "POWER.TORIC_TOUGHNESS_POWER") => {
                let powers = &mut self.creature_mut(owner).powers;
                if let Some(index) = powers.iter().position(|x| *x == power) {
                    powers[index].amount -= 1;
                    if powers[index].amount == 0 {
                        powers.remove(index);
                    }
                }
                self.gain_block(content, owner, power.value, None, false);
            }
            (Trigger::AfterBlockCleared, "POWER.SELF_FORMING_CLAY_POWER") => {
                self.creature_mut(owner).powers.retain(|x| *x != power);
                self.gain_block(content, owner, power.amount, None, false);
            }
            (Trigger::AfterEnergyReset, "POWER.SPINNER_POWER") => {
                effects.extend(std::iter::repeat_n(Effect::Channel(4, 1), amount as usize))
            }
            (Trigger::BeforeHandDraw, "POWER.NIGHTMARE_POWER") => {
                self.creature_mut(owner).powers.retain(|x| x.id != power.id);
                for (card, count) in std::mem::take(&mut self.combat_mut().unwrap().nightmares) {
                    for _ in 0..count {
                        let card = Card {
                            instance: 0,
                            free: false,
                            turn_flags: 0,
                            ..card
                        };
                        // Its copies have the player as creator, so even a Wither fires Regalite.
                        let card = self.add_to_pile(content, Pile::Hand, card);
                        self.trigger_created(content, card.id);
                    }
                }
            }
            (Trigger::BeforeHandDraw, "POWER.HELLO_WORLD_POWER") if power.amount > 0 => {
                self.expectation_unknown();
                let mut pool: Vec<_> = (content.characters[self.run.character as usize].cards)
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
                for id in pool.into_iter().take(amount as usize) {
                    let card = Card {
                        id,
                        ..Card::default()
                    };
                    self.add_generated(content, Pile::Hand, card);
                }
            }
            (Trigger::AfterPlayerTurnStart, "POWER.ROLLING_BOULDER_POWER") => {
                self.tick_boulder(content, power)
            }
            (Trigger::AfterPlayerTurnStart, "POWER.TOOLS_OF_THE_TRADE_POWER") => {
                effects.push(Effect::Discard(amount, false))
            }
            (Trigger::AfterPlayerTurnStart, "POWER.TYRANNY_POWER") => {
                effects.push(Effect::Exhaust(amount, false))
            }
            (Trigger::AfterPlayerTurnStart, "POWER.LOOP_POWER") => effects.extend(
                std::iter::repeat_n(Effect::PassiveFirst(1), amount as usize),
            ),
            // The cards are picked from the hand (AutoSlay's untraced selector in sampled runs).
            (Trigger::AfterPlayerTurnStart, "POWER.ENTROPY_POWER") => {
                effects.push(Effect::SelectAmount(
                    Pile::Hand,
                    CardFilter::Any,
                    fixed,
                    false,
                    CardOp::TransformRandom,
                ))
            }
            (Trigger::AfterSideTurnStart, "POWER.POISON_POWER") if participant => {
                self.poison(content, owner)
            }
            (Trigger::AfterSideTurnStart, "POWER.SLOW_POWER") if participant => {
                let powers = &mut self.creature_mut(owner).powers;
                if let Some(slow) = powers.iter_mut().find(|x| x.id == power.id) {
                    slow.amount = 0;
                }
            }
            (Trigger::AfterSideTurnStart, "POWER.NOXIOUS_FUMES_POWER")
                if player && power.amount > 0 =>
            {
                effects.push(Effect::ApplyPower(
                    Target::AllEnemies,
                    power_id::POISON,
                    fixed,
                ))
            }
            (Trigger::AfterSideTurnStart, "POWER.COUNTDOWN_POWER")
                if player && power.amount > 0 =>
            {
                effects.push(Effect::ApplyPower(
                    Target::RandomEnemy,
                    power_id::DOOM,
                    fixed,
                ))
            }
            (Trigger::AfterSideTurnStart, "POWER.BIASED_COGNITION_POWER") if player => {
                let loss = Amount::fixed(-power.amount, -power.amount);
                effects.push(Effect::ApplyPower(Target::Player, power_id::FOCUS, loss))
            }
            (Trigger::AfterSideTurnStart, "POWER.WRAITH_FORM_POWER") if player => {
                let loss = Amount::fixed(-power.amount, -power.amount);
                effects.push(Effect::ApplyPower(
                    Target::Player,
                    power_id::DEXTERITY,
                    loss,
                ))
            }
            // Shadow Step turns into Double Damage only now, so Sly cards that Tools of the Trade
            // discarded aren't doubled.
            (Trigger::AfterSideTurnStart, "POWER.SHADOW_STEP_POWER") if player => effects.extend([
                Effect::ApplyPower(Target::Player, power_id::DOUBLE_DAMAGE, fixed),
                Effect::RemovePower(Target::Player, power.id),
            ]),
            // It shields the Turret Operators on the player's turn, except an extra one.
            (Trigger::AfterSideTurnStart, "POWER.RAMPART_POWER")
                if !participant
                    && !self.combat().unwrap().extra_turn
                    && self.creature(owner).hp > 0 =>
            {
                for enemy in &mut self.combat_mut().unwrap().enemies {
                    if content.enemies[enemy.creature.id as usize].id == "MONSTER.TURRET_OPERATOR"
                        && enemy.creature.hp > 0
                    {
                        enemy.creature.block += power.amount.max(0);
                    }
                }
            }
            // Dark Embrace draws for the Ethereal exhausts once the hand is flushed.
            (Trigger::AfterCardExhausted, "POWER.DARK_EMBRACE_POWER") if context.ethereal => {
                if let Some(power) = self.creature_mut(owner).instance_mut(power) {
                    power.value += 1;
                }
                return;
            }
            // Iteration draws for the turn's first Status drawn.
            (Trigger::AfterCardDrawn, "POWER.AUTOMATION_POWER") => {
                let Some(automation) = self.creature_mut(owner).instance_mut(power) else {
                    return;
                };
                automation.value -= 1;
                if automation.value <= 0 {
                    automation.value = 10;
                    self.gain_energy(power.amount);
                }
            }
            (Trigger::AfterCardDrawn, "POWER.ITERATION_POWER")
                if self.combat().unwrap().history.statuses_drawn > 1 =>
            {
                return;
            }
            // A Free Attack, Skill or Power and a Veilpiercer spend themselves on the card.
            (
                Trigger::BeforeCardPlayed,
                "POWER.FREE_ATTACK_POWER" | "POWER.FREE_SKILL_POWER" | "POWER.FREE_POWER_POWER",
            ) => {
                let card = self.combat().unwrap().plays.last().unwrap().card;
                let free = match card.card_type(content.cards[card.id as usize]) {
                    CardType::Attack => power_id::FREE_ATTACK,
                    CardType::Skill => power_id::FREE_SKILL,
                    CardType::Power => power_id::FREE_POWER,
                    _ => return,
                };
                if free == power.id {
                    self.creature_mut(owner).consume_power(free);
                }
            }
            (Trigger::BeforeCardPlayed, "POWER.VEILPIERCER_POWER")
                if self.playing_ethereal(content) =>
            {
                self.creature_mut(owner).consume_power(power.id)
            }
            (Trigger::BeforeCardPlayed, "POWER.SPIRIT_OF_ASH_POWER")
                if self.playing_ethereal(content) =>
            {
                effects.push(Effect::RawBlock(Target::Player, fixed))
            }
            // Danse Macabre reads the cost as it now resolves (EnergyCost.GetResolved): a Free
            // power or Veilpiercer listed before it has spent itself; an X cost is the energy
            // the play started with.
            (Trigger::BeforeCardPlayed, "POWER.DANSE_MACABRE_POWER") => {
                let spiked = self.has_relic(content, "RELIC.SPIKED_GAUNTLETS");
                let chemical_x = 2 * self.has_relic(content, "RELIC.CHEMICAL_X") as i16;
                let combat = self.combat().unwrap();
                let play = combat.plays.last().unwrap();
                let def = content.cards[play.card.id as usize];
                let cost = if def.cost[play.card.upgrades.min(1) as usize] < 0 {
                    play.context.x - chemical_x
                } else {
                    energy_cost(combat, play.card, def, spiked, true)
                };
                if cost >= 2 {
                    effects.push(Effect::RawBlock(Target::Player, fixed))
                }
            }
            (Trigger::BeforeCardPlayed, "POWER.THE_SEALED_THRONE_POWER") => {
                effects.push(Effect::Stars(fixed))
            }
            (Trigger::AfterSideTurnStart, "POWER.PLATING_POWER") if turn == 1 => return,
            (Trigger::AutoPrePlay, "POWER.MAYHEM_POWER") if player => {
                effects.push(Effect::AutoPlayDraw(fixed, false))
            }
            (Trigger::BeforeSideTurnEndVeryEarly, "POWER.ASLEEP_POWER")
                if participant && power.amount <= 1 =>
            {
                (self.creature_mut(owner).powers).retain(|x| x.id != power_id::PLATING)
            }
            // The side's first doomed creature kills them all (DoomKill).
            (Trigger::BeforeSideTurnEnd, "POWER.DOOM_POWER") if participant => {
                let side: Vec<_> = if matches!(owner, Actor::Enemy(_)) {
                    (0..self.combat().unwrap().enemies.len())
                        .map(Actor::Enemy)
                        .collect()
                } else {
                    vec![Actor::Player, Actor::Osty]
                };
                let doomed = side.iter().find(|&&actor| {
                    let creature = self.creature(actor);
                    creature.hp > 0 && creature.hp <= creature.power(power_id::DOOM)
                });
                if doomed == Some(&owner) {
                    self.doom_kill(content, side);
                }
            }
            (Trigger::BeforeSideTurnEnd, "POWER.THE_BOMB_POWER") if participant => {
                self.tick_bomb(content, power)
            }
            (Trigger::BeforeSideTurnEnd, "POWER.HAILSTORM_POWER")
                if participant
                    && (self.combat().unwrap().orbs.iter()).any(|orb| orb.id == orb_id::FROST) =>
            {
                effects.push(Effect::Damage(Target::AllEnemies, fixed))
            }
            // A dead Illusion keeps the power but gets no Strength back.
            (Trigger::AfterSideTurnEnd, "POWER.MONARCHS_GAZE_STRENGTH_DOWN_POWER")
                if participant =>
            {
                let creature = self.creature_mut(owner);
                if creature.hp > 0 {
                    creature.add_power(power_id::STRENGTH, power.amount);
                }
                creature.powers.retain(|current| current.id != power.id);
                return;
            }
            // Ritual skips the turn it was applied by an enemy (WasJustAppliedByEnemy).
            (Trigger::AfterSideTurnEnd, "POWER.RITUAL_POWER") if power.skip_next_decay => {
                let powers = &mut self.creature_mut(owner).powers;
                let ritual = powers.iter_mut().find(|current| current.id == power.id);
                ritual.unwrap().skip_next_decay = false;
                return;
            }
            _ => {}
        }
        let queue = &mut self.combat_mut().unwrap().queue;
        push_effects(queue, &effects, Context::player());
        let played = hears(Trigger::AfterCardPlayed, trigger);
        let afflicted = |game: &Self, affliction, card_type| {
            played
                && context.card.is_some_and(|card| {
                    game.played_card(card)
                        .map_or(content.cards[card as usize].card_type == card_type, |x| {
                            x.affliction == affliction
                        })
                })
        };
        match (trigger, power.id) {
            (Trigger::BeforeCombatStart, power_id::GALVANIC) => {
                self.afflict_all(content, Affliction::Galvanized, &[CardType::Power])
            }
            (Trigger::BeforeCombatStart, power_id::VITAL_SPARK) => {
                self.afflict_all(content, Affliction::Tainted, &[CardType::Skill])
            }
            (Trigger::SkillPlayed, power_id::ENRAGE) => {
                self.apply_power(content, owner, power_id::STRENGTH, power.amount)
            }
            // Galvanic hits for each played card carrying its Galvanized affliction.
            (_, power_id::GALVANIC) if afflicted(self, Affliction::Galvanized, CardType::Power) => {
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
            (_, power_id::VITAL_SPARK) if afflicted(self, Affliction::Tainted, CardType::Skill) => {
                self.apply_power(content, Actor::Player, power_id::TAINTED, power.amount)
            }
            // Smoggy afflicts every unafflicted skill with Smog once a skill finishes playing.
            (Trigger::SkillPlayed, power_id::SMOGGY) if player => {
                self.afflict_all(content, Affliction::Smog, &[CardType::Skill])
            }
            (_, power_id::WITHERING_PRESENCE) if played => {
                let power = (self.creature_mut(owner).powers.iter_mut())
                    .find(|x| x.id == power.id)
                    .unwrap();
                if power.value <= 0 {
                    power.value = power.amount;
                }
                power.value -= 1;
                if power.value == 0 {
                    power.value = 6;
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::AddCard(Pile::Hand, card_id::WITHER, 1),
                        context,
                    });
                }
            }
            // Each Panache counts the cards played after the one that applied it.
            (_, power_id::PANACHE) if played && player => {
                let Some(panache) = self.creature_mut(owner).instance_mut(power) else {
                    return;
                };
                if std::mem::take(&mut panache.skip_next_decay) {
                    return;
                }
                panache.value -= 1;
                if panache.value <= 0 {
                    panache.value = 5;
                    let damage = Amount::fixed(power.amount, power.amount);
                    self.combat_mut().unwrap().queue.push(Pending {
                        effect: Effect::Damage(Target::AllEnemies, damage),
                        context: Context::player(),
                    });
                }
            }
            (_, power_id::TENDER) if played && player => {
                let player = self.creature_mut(Actor::Player);
                player.instance_mut(power).unwrap().amount += 1;
                player.add_power(power_id::STRENGTH, -1);
                player.add_power(power_id::DEXTERITY, -1);
            }
            // Curled up by an earlier hit, it blocks once the play finishes.
            (_, power_id::CURL_UP) if played => {
                let Actor::Enemy(index) = owner else { return };
                let combat = self.combat_mut().unwrap();
                if let Some(at) = combat.curled.iter().position(|&curled| curled == index) {
                    combat.curled.remove(at);
                    let creature = &mut combat.enemies[index].creature;
                    let curl = creature.power(power.id);
                    creature.powers.retain(|current| current.id != power.id);
                    combat.queue.push(Pending {
                        effect: Effect::RawBlock(Target::Source, Amount::fixed(curl, curl)),
                        context: Context {
                            hook: true,
                            ..context
                        },
                    });
                }
            }
            (Trigger::AfterSideTurnEndLate, power_id::DISINTEGRATION) if participant && player => {
                let amount = power.amount;
                self.damage(content, owner, owner, amount, DamageKind::Unpowered, None);
            }
            _ => {}
        }
        if matches!(content.powers[power.id as usize].kind, PowerKind::OneShot(at) if at == trigger)
        {
            self.combat_mut().unwrap().queue.push(Pending {
                effect: Effect::RemovePower(Target::Source, power.id),
                context,
            });
        }
        if trigger == Trigger::AfterSideTurnEnd {
            self.after_side_turn_end(content, owner, power, participant);
        }
        for hook in content.powers[power.id as usize]
            .hooks
            .iter()
            .rev()
            .filter(|x| hears(x.trigger, trigger))
        {
            let mut context = context;
            // Strangle, Serpent Form, Monologue, Afterimage and Oblivion use their stacks from
            // when the card started (snapshot), not after it stacked. A temporary Strength power
            // restores its amount even when Plow's break removed it earlier in the same turn end
            // (the hook list is a snapshot).
            if matches!(
                hook.trigger,
                Trigger::PowerPlayed
                    | Trigger::AfterCardDrawn
                    | Trigger::AfterCardGeneratedForCombat
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

    /// AfterSideTurnEnd, in power order: the powers of the side ending its turn wear off or tick
    /// (participants), the durations tick at the enemy side's (CombatSide.Enemy).
    fn after_side_turn_end(
        &mut self,
        content: &Content,
        owner: Actor,
        power: Power,
        participant: bool,
    ) {
        let player = owner == Actor::Player;
        let enemy_side = self.combat().unwrap().enemy_turn;
        let amount = Amount::fixed(power.amount, power.amount);
        let timed = (1..=3).contains(&self.event_combat);
        let creature = self.creature_mut(owner);
        let mut effects = vec![];
        match power.id {
            power_id::WEAK | power_id::VULNERABLE | power_id::FRAIL | power_id::INTANGIBLE
                if enemy_side =>
            {
                match creature.powers.iter_mut().find(|x| x.id == power.id) {
                    Some(current) if current.skip_next_decay => current.skip_next_decay = false,
                    _ => creature.consume_power(power.id),
                }
            }
            power_id::NO_BLOCK | power_id::COLOSSUS if enemy_side => {
                creature.consume_power(power.id)
            }
            power_id::TAINTED | power_id::DIAMOND_DIADEM if enemy_side => {
                creature.powers.retain(|x| x.id != power.id)
            }
            power_id::FLAME_BARRIER if !participant => creature.powers.retain(|x| x.id != power.id),
            power_id::CONQUEROR
            | power_id::DEBILITATE
            | power_id::HATCH
            | power_id::ASLEEP
            | power_id::DOUBLE_DAMAGE
                if participant =>
            {
                creature.consume_power(power.id)
            }
            power_id::SHRINK if participant && power.amount > 0 => creature.consume_power(power.id),
            id if participant
                && (matches!(
                    id,
                    power_id::SIC_EM | power_id::STRANGLE | power_id::NO_ENERGY_GAIN
                ) || content.powers[id as usize].kind == PowerKind::NoDraw) =>
            {
                creature.powers.retain(|x| x.id != id)
            }
            // Slumber running out wakes the beetle (WakeUpMove): its Plating goes.
            power_id::SLUMBER if participant => {
                creature.consume_power(power.id);
                if creature.power(power.id) == 0 {
                    creature.powers.retain(|x| x.id != power_id::PLATING);
                }
            }
            power_id::NEMESIS if participant => {
                let nemesis = creature.powers.iter_mut().find(|x| x.id == power.id);
                let nemesis = nemesis.unwrap();
                nemesis.value ^= 1;
                if nemesis.value == 1 {
                    let intangible = creature.power(power_id::INTANGIBLE);
                    creature.add_power(power_id::INTANGIBLE, 1 - intangible);
                } else {
                    creature.powers.retain(|x| x.id != power_id::INTANGIBLE);
                }
            }
            power_id::ESCAPE_ARTIST if participant && power.amount > 1 => {
                creature.consume_power(power.id)
            }
            power_id::TIME_LIMIT if participant && timed => {
                if power.amount > 1 {
                    creature.add_power(power.id, -1);
                } else {
                    creature.hp = 0;
                    self.event_combat |= 128;
                }
            }
            power_id::PANACHE if participant => {
                if let Some(panache) = self.creature_mut(owner).instance_mut(power) {
                    panache.value = 5;
                }
            }
            power_id::FOCUSED_STRIKE | power_id::HOTFIX | power_id::SYNCHRONIZE
                if participant && player =>
            {
                let amount = Amount::fixed(-power.amount, -power.amount);
                effects = vec![Effect::StackPower(Target::Player, power_id::FOCUS, amount)];
            }
            power_id::DARK_EMBRACE if participant && player => {
                let current = creature.powers.iter_mut().find(|x| x.id == power.id);
                let draws = current.map_or(0, |x| power.amount * std::mem::take(&mut x.value));
                effects = (draws > 0)
                    .then(|| Effect::DrawAmount(Amount::fixed(draws, draws)))
                    .into_iter()
                    .collect();
            }
            power_id::REGEN if participant && player => {
                effects = vec![
                    Effect::ApplyPower(Target::Player, power_id::REGEN, Amount::fixed(-1, -1)),
                    Effect::Heal(Target::Player, amount),
                ]
            }
            // Hellraiser resets at its side's turn end, Skittish at the other side's.
            power_id::HELLRAISER | power_id::SKITTISH
                if (power.id == power_id::HELLRAISER) == participant =>
            {
                (creature.powers.iter_mut())
                    .filter(|x| x.id == power.id)
                    .for_each(|x| x.value = 0)
            }
            power_id::OBLIVION if !enemy_side && !player => {
                creature.powers.retain(|x| x.id != power.id)
            }
            _ => {}
        }
        let queue = &mut self.combat_mut().unwrap().queue;
        queue.extend(effects.into_iter().map(|effect| Pending {
            effect,
            context: Context::player(),
        }));
    }

    /// Creature.AfterTurnStart: the block clears unless a power keeps it (ShouldClearBlock). The
    /// player keeps the combat-start block (Anchor) on turn 1, and up to 10 with Sturdy Clamp.
    fn clear_block(&mut self, content: &Content, actor: Actor) {
        let player = actor == Actor::Player;
        let first = player && self.combat().unwrap().turn == 1;
        let kept = if player && self.has_relic(content, "RELIC.STURDY_CLAMP") {
            10
        } else {
            0
        };
        let creature = self.creature_mut(actor);
        if !first
            && creature.kind(content, PowerKind::BlockRetain) == 0
            && creature.kind(content, PowerKind::BlockRetainOnce) == 0
        {
            creature.block = creature.block.min(kept);
        }
        let once = (creature.powers.iter())
            .find(|power| content.powers[power.id as usize].kind == PowerKind::BlockRetainOnce);
        if let Some(power) = once.copied() {
            creature.consume_power(power.id);
        }
    }

    /// Pael's Eye: no card played by hand this turn (Whispering Earring plays turn 1's).
    fn takes_extra_turn(&self, content: &Content) -> bool {
        let combat = self.combat().unwrap();
        self.has_relic(content, "RELIC.PAELS_EYE")
            && !combat.paels_eye
            && combat.history.manual_cards == 0
            && !(combat.turn == 1 && self.has_relic(content, "RELIC.WHISPERING_EARRING"))
    }

    /// The enemies starting their side's turn, in slot order: the living ones and those about to
    /// revive (Illusion, Reattach, Adaptable).
    pub(super) fn participants(&self, content: &Content) -> Vec<usize> {
        let enemies = &self.combat().unwrap().enemies;
        let primary_alive = (enemies.iter())
            .any(|enemy| enemy.creature.hp > 0 && enemy.creature.power(power_id::MINION) == 0);
        let segment_alive = (enemies.iter())
            .any(|enemy| enemy.creature.hp > 0 && enemy.creature.power(power_id::REATTACH) > 0);
        let mut participants: Vec<_> = (0..enemies.len())
            .filter(|&index| {
                let enemy = &enemies[index];
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
        participants
    }

    pub(super) fn turn_step(&mut self, content: &Content, step: Step) {
        // An enemy's turn runs to its end even once the combat is over (the Fabricator still
        // builds its bot); the next enemy's doesn't start (CheckWinCondition).
        let own_turn = matches!(step, Step::EnemyMoveDone(..) | Step::FabricatorBot(_));
        if !own_turn && (self.enemies_defeated() || self.creature(Actor::Player).hp <= 0) {
            return;
        }
        match step {
            Step::TurnStart => {
                let paels_flesh = self.relic_count(content, "RELIC.PAELS_FLESH");
                let bread = self.has_relic(content, "RELIC.BREAD") as i16;
                let history_course = self.has_relic(content, "RELIC.HISTORY_COURSE");
                let combat = self.combat_mut().unwrap();
                combat.setup = true;
                combat.turn += 1;
                combat.demon_tongue = false;
                combat.mini_regent = false;
                combat.music_box = false;
                combat.music_box_card = None;
                combat.rainbow_ring = false;
                combat.attacks_mod3 = 0;
                combat.bound_this_turn = 0;
                if combat.turn == 3 {
                    combat.max_energy += paels_flesh;
                }
                // Bread's +1 max energy skips turn 1; its turn-1 loss comes in AfterSideTurnStart.
                if combat.turn == 2 {
                    combat.max_energy += bread;
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
                        if card.id == card_id::FLATTEN
                            || content.cards[card.id as usize].id == "CARD.STOMP"
                        {
                            card.value = 0;
                        }
                    }
                }
                combat.last_turn = combat.history;
                combat.last_turn_card = None;
                if combat.turn > 1 && history_course {
                    combat.last_turn_card = combat.history_course.take();
                }
                combat.hits.fill(0);
                // HP-loss events, ethereal plays (Pull From Below) and generated cards
                // (Supermassive) count for the whole combat. The combat start counts as round 1
                // (Divine Right's stars).
                if combat.turn > 1 {
                    combat.history = History {
                        hp_loss_events: combat.history.hp_loss_events,
                        ethereal: combat.history.ethereal,
                        generated: combat.history.generated,
                        ..History::default()
                    };
                }
            }
            // Enemies roll their intents after BeforeSideTurnStart, except on an extra turn.
            Step::RollMoves if !self.combat().unwrap().extra_turn => self.roll_moves(content),
            Step::RollMoves => {}
            Step::ClearBlock if self.combat().unwrap().enemy_turn => {
                for index in self.participants(content) {
                    self.clear_block(content, Actor::Enemy(index));
                }
            }
            Step::ClearBlock => self.clear_block(content, Actor::Player),
            Step::ResetEnergy => {
                let ice_cream = self.has_relic(content, "RELIC.ICE_CREAM");
                let combat = self.combat_mut().unwrap();
                let kept = if ice_cream && combat.turn > 1 {
                    combat.energy
                } else {
                    0
                };
                combat.energy = kept + combat.max_energy;
            }
            // Cards' BeforeHandDraw hooks (Bolas, Thrumming Hatchet) come after powers' and relics'.
            Step::HandDraw => {
                let turn = self.combat().unwrap().turn;
                let last_cards = self.combat().unwrap().last_turn.cards;
                let power = |game: &Self, id| game.creature(Actor::Player).power(id);
                let clarity = (power(self, power_id::CLARITY) > 0) as i16;
                if clarity > 0 {
                    self.creature_mut(Actor::Player)
                        .consume_power(power_id::CLARITY);
                }
                let mut count = self.combat().unwrap().draw_per_turn as i16
                    + clarity
                    + power(self, power_id::TOOLS_OF_THE_TRADE).max(0)
                    + (last_cards >= 5) as i16 * power(self, power_id::PALE_BLUE_DOT).max(0)
                    + power(self, power_id::TYRANNY).max(0)
                    + power(self, power_id::DRAW_CARDS_NEXT_TURN).max(0)
                    + power(self, power_id::DEMESNE).max(0)
                    - power(self, power_id::MIND_ROT);
                // Machine Learning raises the hand draw (ModifyHandDraw).
                count += (self.creature(Actor::Player).powers.iter())
                    .filter(|power| {
                        content.powers[power.id as usize].id == "POWER.MACHINE_LEARNING_POWER"
                    })
                    .map(|power| power.amount)
                    .sum::<i16>();
                let relics = |id| self.relic_count(content, id);
                if turn == 1 {
                    count -= 2 * relics("RELIC.BIG_MUSHROOM");
                    count += 2 * relics("RELIC.BAG_OF_PREPARATION");
                    count += 2 * relics("RELIC.RING_OF_THE_SNAKE");
                    count += 2 * relics("RELIC.BOOMING_CONCH") * (self.room == Room::Elite) as i16;
                }
                if turn <= 3 {
                    count += 2 * relics("RELIC.RING_OF_THE_DRAKE");
                }
                count += 2 * relics("RELIC.FIDDLE");
                count += relics("RELIC.PAELS_BLOOD");
                count += 2 * relics("RELIC.SNECKO_EYE");
                if turn > 1 && last_cards <= 3 {
                    count += 3 * relics("RELIC.POCKETWATCH");
                }
                if self.pollinous_core == 4 {
                    count += 2;
                    self.pollinous_core = 0;
                }
                self.creature_mut(Actor::Player)
                    .powers
                    .retain(|power| power.id != power_id::DRAW_CARDS_NEXT_TURN);
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::HandDraw(count.clamp(0, u8::MAX as i16) as u8),
                    context: Context::player(),
                });
            }
            // Turn-start orb passives (Plasma) follow AfterSideTurnStart (OrbQueue.AfterTurnStart);
            // the setup (PlayerTurnPhase.Start) ends with it.
            Step::TurnStartOrbs => {
                let combat = self.combat_mut().unwrap();
                combat.setup = false;
                combat.extra_turn = false;
                self.passive_orbs(content, OrbTiming::TurnStart);
            }
            Step::Bombardments => self.queue_bombardments(),
            // AfterAutoPostPlayPhaseEntered: I Am Invincible plays itself off the draw pile's top,
            // then Stampede, then Howl from Beyond from the exhaust pile.
            Step::IAmInvincible => {
                let combat = self.combat_mut().unwrap();
                if combat
                    .draw
                    .last()
                    .is_some_and(|card| card.id == card_id::I_AM_INVINCIBLE)
                {
                    let card = combat.pop_draw().unwrap();
                    combat.queue.push(Pending {
                        effect: Effect::Step(Step::IAmInvincible),
                        context: Context::player(),
                    });
                    combat.queue.push(Pending {
                        effect: Effect::AutoPlay(card),
                        context: Context::card(card),
                    });
                }
            }
            Step::Stampede if self.creature(Actor::Player).power(power_id::STAMPEDE) > 0 => {
                self.combat_mut().unwrap().queue.push(Pending {
                    effect: Effect::Stampede,
                    context: Context::player(),
                })
            }
            Step::Stampede => {}
            Step::HowlFromBeyond => {
                let howl = content.card_id("CARD.HOWL_FROM_BEYOND").unwrap();
                let combat = self.combat_mut().unwrap();
                let howls: Vec<_> = combat
                    .exhaust
                    .extract_if(.., |card| card.id == howl)
                    .collect();
                combat
                    .queue
                    .extend(howls.into_iter().rev().map(|card| Pending {
                        effect: Effect::AutoPlay(card),
                        context: Context {
                            off_pile: true,
                            ..Context::card(card)
                        },
                    }));
            }
            Step::TurnEndOrbs => self.passive_orbs(content, OrbTiming::TurnEnd),
            // DoTurnEnd: the Ethereal cards exhaust, then each turn-end card moves to the play pile
            // for its effect and on to the discard pile (or exhausts, if Ethereal), so the effects
            // see the smaller hand (Regret, Centennial Puzzle's draws).
            Step::DoTurnEnd => {
                let hexed = self.creature(Actor::Player).power(power_id::HEX) > 0;
                let combat = self.combat_mut().unwrap();
                combat.rebounds.clear();
                combat.ferals.clear();
                // Regret counts the hand at BeforeSideTurnEnd, before any card leaves it.
                let hand_size = combat.hand.len() as i16;
                let (turn_end_cards, others): (Vec<_>, Vec<_>) = combat
                    .hand
                    .iter()
                    .copied()
                    .partition(|card| turn_end(content, card));
                let ethereal_cards = others
                    .into_iter()
                    .filter(|card| ethereal(content, card, hexed));
                let steps = (ethereal_cards.map(Step::EtherealExhaust))
                    .chain(
                        turn_end_cards
                            .into_iter()
                            .map(|card| Step::TurnEndCard(card, hand_size)),
                    )
                    .collect::<Vec<_>>();
                combat
                    .queue
                    .extend(steps.into_iter().rev().map(|step| Pending {
                        effect: Effect::Step(step),
                        context: Context::player(),
                    }));
            }
            Step::EtherealExhaust(card) => {
                let combat = self.combat_mut().unwrap();
                let Some(index) = combat.hand.iter().position(|other| *other == card) else {
                    return;
                };
                combat.hand.remove(index);
                combat.exhaust.push(card);
                combat.history.exhausted += 1;
                self.exhausted(content, card, true);
            }
            Step::TurnEndCard(card, hand_size) => {
                let combat = self.combat_mut().unwrap();
                let Some(index) = combat.hand.iter().position(|other| *other == card) else {
                    return;
                };
                combat.hand.remove(index);
                combat.queue.push(Pending {
                    effect: Effect::Step(Step::TurnEndCardDone(card)),
                    context: Context::player(),
                });
                if content.card_id("CARD.REGRET") == Some(card.id) {
                    combat.queue.push(Pending {
                        effect: Effect::LoseHp(Target::Player, Amount::fixed(hand_size, hand_size)),
                        context: Context::card(card),
                    });
                } else {
                    self.trigger_card(content, card, Trigger::TurnEnd);
                }
            }
            Step::TurnEndCardDone(card) => {
                let hexed = self.creature(Actor::Player).power(power_id::HEX) > 0;
                let combat = self.combat_mut().unwrap();
                if ethereal(content, &card, hexed) {
                    combat.exhaust.push(card);
                    combat.history.exhausted += 1;
                    self.exhausted(content, card, true);
                } else {
                    combat.discard.push(card);
                }
            }
            Step::Retain => {
                let retain = (self.creature(Actor::Player).power(power_id::RETAIN_HAND) > 0)
                    || self.has_relic(content, "RELIC.RUNIC_PYRAMID")
                    || self.combat().unwrap().turn == 1
                        && self.has_relic(content, "RELIC.RINGING_TRIANGLE");
                if retain {
                    for card in &mut self.combat_mut().unwrap().hand {
                        card.turn_flags |= RETAIN;
                    }
                }
                self.creature_mut(Actor::Player)
                    .consume_power(power_id::RETAIN_HAND);
                let plans = self
                    .creature(Actor::Player)
                    .power(power_id::WELL_LAID_PLANS);
                if plans > 0 {
                    self.select(
                        content,
                        Pile::Hand,
                        CardFilter::WithoutFlag(RETAIN),
                        CardOp::TurnFlag(RETAIN),
                        plans.clamp(0, u8::MAX as i16) as u8,
                        false,
                        true,
                    );
                }
            }
            // FlushPlayerHand: retained cards stay, the rest are discarded.
            Step::Flush => {
                let combat = self.combat_mut().unwrap();
                combat.bound_played = false;
                for card in (combat.hand.iter_mut())
                    .chain(&mut combat.draw)
                    .chain(&mut combat.discard)
                    .chain(&mut combat.exhaust)
                {
                    // Bound, Ringing, Smog and Entangled end with the player's turn.
                    if matches!(
                        card.affliction,
                        Affliction::Bound
                            | Affliction::Ringing
                            | Affliction::Smog
                            | Affliction::Entangled
                    ) {
                        card.affliction = Affliction::None;
                    }
                }
                (combat.player.powers).retain(|power| power.id != power_id::RINGING);
                self.reset_tender();
                let combat = self.combat_mut().unwrap();
                for card in std::mem::take(&mut combat.hand) {
                    let flags = card.flags(content.cards[card.id as usize]);
                    if flags & RETAIN != 0 && !turn_end(content, &card) {
                        combat.hand.push(card);
                    } else {
                        combat.discard.push(card);
                    }
                }
            }
            // AfterFlush.
            Step::Bookmark if self.has_relic(content, "RELIC.BOOKMARK") => {
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
                    let choice = choices
                        [self.rngs.combat_card_selection.below(choices.len() as u32) as usize];
                    let card = self.combat().unwrap().hand[choice];
                    let def = content.cards[card.id as usize];
                    let cost = energy_cost(self.combat().unwrap(), card, def, spiked, true) - 1;
                    // Bookmark's discount lasts until played (AddUntilPlayed).
                    let card = &mut self.combat_mut().unwrap().hand[choice];
                    card.cost_override = Some(cost as i8);
                    card.when_played = true;
                }
            }
            Step::Bookmark => {}
            // An extra turn (Pael's Eye: ShouldTakeExtraTurn, then AfterTakingExtraTurn) or the
            // enemy side's turn.
            Step::SwitchSide if self.takes_extra_turn(content) => {
                let combat = self.combat_mut().unwrap();
                combat.paels_eye = true;
                combat.extra_turn = true;
                self.start_turn();
            }
            Step::SwitchSide => {
                let combat = self.combat_mut().unwrap();
                combat.enemy_turn = true;
                push_effects(&mut combat.queue, ENEMY_TURN, Context::player());
            }
            // The last step queued: the moves still resolve each enemy's move in turn.
            Step::EnemyMoves => self.enemy_moves(content),
            Step::EnemyMove(index, instance) => self.enemy_move(content, index as usize, instance),
            Step::EnemyMoveDone(index, move_index, stunned) => {
                self.enemy_move_done(content, index as usize, move_index as usize, stunned)
            }
            Step::FabricatorBot(index) => self.spawn_bot(content, index as usize, 2),
            Step::EnemyTurnEnd => {
                self.combat_mut().unwrap().enemy_turn = false;
                self.start_turn();
            }
        }
    }
}

/// A card with an effect at the end of the turn in hand (Regret, Doubt, Burn).
pub(super) fn turn_end(content: &Content, card: &Card) -> bool {
    (content.cards[card.id as usize].hooks.iter()).any(|hook| hook.trigger == Trigger::TurnEnd)
}

pub(super) fn ethereal(content: &Content, card: &Card, hexed: bool) -> bool {
    card.flags(content.cards[card.id as usize]) & ETHEREAL != 0 || hexed
}
