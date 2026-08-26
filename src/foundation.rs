use crate::*;

macro_rules! effects {
    ($($effect:expr),* $(,)?) => { const { &[$($effect),*] } };
}

const FEEL_NO_PAIN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardExhausted,
    effects: effects![Effect::RawBlock(
        Target::Source,
        Amount::scaled(Scale::Power(10), 1),
    )],
}];
const DARK_EMBRACE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardExhausted,
    effects: effects![Effect::DrawAmount(Amount::scaled(Scale::Power(11), 1))],
}];
const RUPTURE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::HpLost,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(12), 1),
    )],
}];
const JUGGERNAUT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::BlockGained,
    effects: effects![Effect::Damage(
        Target::RandomEnemy,
        Amount::scaled(Scale::Power(13), 1),
    )],
}];
const DEMON_FORM_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(14), 1),
    )],
}];
const TOXIC_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::Damage(Target::Player, Amount::fixed(5, 5))],
}];
const BECKON_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::LoseHp(Target::Player, Amount::fixed(6, 6))],
}];
const FEEDING_FRENZY_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        power_id::STRENGTH,
        Amount::scaled(Scale::Power(power_id::FEEDING_FRENZY), -1),
    )],
}];
const HIGH_VOLTAGE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(power_id::HIGH_VOLTAGE), 1),
    )],
}];
const TERRITORIAL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(power_id::TERRITORIAL), 1),
    )],
}];
const SLOW_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        power_id::SLOW,
        Amount::fixed(10, 10),
    )],
}];
const CONSTRICT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::Damage(
        Target::Source,
        Amount::scaled(Scale::Power(power_id::CONSTRICT), 1),
    )],
}];
const MACHINE_LEARNING_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Draw(1)],
}];
const NOXIOUS_FUMES_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::AllEnemies,
        7,
        Amount::scaled(Scale::Power(19), 1),
    )],
}];
const INFINITE_BLADES_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::AddCard(Pile::Hand, 137, 1)],
}];
const ENERGY_NEXT_TURN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Repeat(
        Amount::scaled(Scale::Power(22), 1),
        effects![Effect::Energy(1)],
    )],
}];
const STAR_NEXT_TURN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Stars(Amount::scaled(Scale::Power(72), 1))],
}];
const FOREGONE_CONCLUSION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ChooseDraw(Amount::scaled(Scale::Power(73), 1))],
}];
const FURNACE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Forge(Amount::scaled(Scale::Power(74), 1))],
}];
const GENESIS_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Stars(Amount::scaled(Scale::Power(75), 1))],
}];
const KINGLY_PUNCH_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardDrawn,
    effects: effects![Effect::GrowDrawn(Amount::fixed(4, 6))],
}];
const MONOLOGUE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![
        Effect::ApplyPower(Target::Player, 0, Amount::scaled(Scale::Event, 1)),
        Effect::ApplyPower(Target::Player, 78, Amount::scaled(Scale::Event, 1)),
    ],
}];
const MONOLOGUE_RESTORE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(78), -1),
    )],
}];
const PLATING_HOOKS: &[Hook] = &[
    Hook {
        trigger: Trigger::TurnEnd,
        effects: effects![Effect::RawBlock(
            Target::Source,
            Amount::scaled(Scale::Power(79), 1),
        )],
    },
    Hook {
        trigger: Trigger::TurnStart,
        effects: effects![Effect::ApplyPower(
            Target::Source,
            79,
            Amount::fixed(-1, -1),
        )],
    },
];
const RAMPART_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::RawBlock(
        Target::OtherEnemies,
        Amount::scaled(Scale::Power(power_id::RAMPART), 1),
    )],
}];
const PILLAR_OF_CREATION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardGenerated,
    effects: effects![Effect::RawBlock(
        Target::Source,
        Amount::scaled(Scale::Event, 1),
    )],
}];
const REFLECT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        85,
        Amount::fixed(-1, -1),
    )],
}];
const SPECTRUM_SHIFT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::RandomColorless(
        Pile::Hand,
        Amount::scaled(Scale::Power(93), 1),
        false,
    )],
}];
const CALL_OF_THE_VOID_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::RandomCharacter(
        Pile::Hand,
        Amount::scaled(Scale::Power(97), 1),
        ETHEREAL,
    )],
}];
const COUNTDOWN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::RandomEnemy,
        94,
        Amount::scaled(Scale::Power(98), 1),
    )],
}];
const DEVOUR_LIFE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![Effect::If(
        Condition::Card(368),
        &[Effect::Summon(Amount::scaled(Scale::Power(102), 1))],
        &[],
    )],
}];
const ENFEEBLING_TOUCH_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(103), 1),
    )],
}];
const HAUNT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![Effect::If(
        Condition::Card(368),
        &[Effect::LoseHp(
            Target::RandomEnemy,
            Amount::scaled(Scale::Power(107), 1),
        )],
        &[],
    )],
}];
const SUMMON_NEXT_TURN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Summon(Amount::scaled(Scale::Power(108), 1))],
}];
const NEUROSURGE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        94,
        Amount::scaled(Scale::Power(111), 1),
    )],
}];
const OBLIVION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        94,
        Amount::scaled(Scale::Power(112), 1),
    )],
}];
const SENTRY_MODE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Repeat(
        Amount::scaled(Scale::Power(115), 1),
        &[Effect::AddCard(Pile::Hand, 429, 1)],
    )],
}];
const AGGRESSION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Aggression(Amount::scaled(Scale::Power(121), 1))],
}];
const CRIMSON_MANTLE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![
        Effect::LoseHp(Target::Source, Amount::scaled(Scale::Power(124), 1)),
        Effect::RawBlock(Target::Source, Amount::scaled(Scale::Power(123), 1)),
    ],
}];
const INFERNO_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::LoseHp(
        Target::Source,
        Amount::scaled(Scale::Power(130), 1),
    )],
}];
const MANGLE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(132), 1),
    )],
}];
const RAGE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::AttackPlayed,
    effects: effects![Effect::RawBlock(
        Target::Source,
        Amount::scaled(Scale::Power(135), 1),
    )],
}];
const SETUP_STRIKE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(136), -1),
    )],
}];
const FLEX_POTION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        power_id::STRENGTH,
        Amount::scaled(Scale::Power(power_id::FLEX_POTION), -1),
    )],
}];
const SPEED_POTION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        power_id::DEXTERITY,
        Amount::scaled(Scale::Power(power_id::SPEED_POTION), -1),
    )],
}];
const TEMPORARY_FOCUS_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        17,
        Amount::scaled(Scale::Power(23), -1),
    )],
}];
const BLOCK_NEXT_TURN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::RawBlock(
        Target::Source,
        Amount::scaled(Scale::Power(41), 1),
    )],
}];
const RESTORE_STRENGTH_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(42), 1),
    )],
}];
const CRUSH_UNDER_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(power_id::CRUSH_UNDER), 1),
    )],
}];
const DARK_SHACKLES_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(power_id::DARK_SHACKLES), 1),
    )],
}];
const DYING_STAR_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(power_id::DYING_STAR), 1),
    )],
}];
const PIERCING_WAIL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(power_id::PIERCING_WAIL), 1),
    )],
}];
const SHACKLING_POTION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(power_id::SHACKLING_POTION), 1),
    )],
}];
const DRAW_NEXT_TURN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::DrawAmount(Amount::scaled(Scale::Power(43), 1))],
}];
const AFTERIMAGE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![Effect::Block(
        Target::Source,
        Amount::scaled(Scale::Power(46), 1),
    )],
}];
const CORROSIVE_WAVE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardDrawn,
    effects: effects![Effect::ApplyPower(
        Target::AllEnemies,
        7,
        Amount::scaled(Scale::Power(48), 1),
    )],
}];
const SHADOW_STEP_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        51,
        Amount::scaled(Scale::Power(50), 1),
    )],
}];
const WRAITH_FORM_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        1,
        Amount::scaled(Scale::Power(52), -1),
    )],
}];
const SERPENT_FORM_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![Effect::Damage(
        Target::RandomEnemy,
        Amount::scaled(Scale::Power(53), 1),
    )],
}];
const RESTORE_DEXTERITY_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        1,
        Amount::scaled(Scale::Power(54), -1),
    )],
}];
const HELICAL_DART_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        power_id::DEXTERITY,
        Amount::scaled(Scale::Power(power_id::HELICAL_DART), -1),
    )],
}];
const REPTILE_TRINKET_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        power_id::STRENGTH,
        Amount::scaled(Scale::Power(power_id::REPTILE_TRINKET), -1),
    )],
}];
const STRANGLE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![Effect::Damage(
        Target::Source,
        Amount::scaled(Scale::Power(55), 1),
    )],
}];
const SPEEDSTER_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardDrawn,
    effects: effects![Effect::Damage(
        Target::AllEnemies,
        Amount::scaled(Scale::Event, 1),
    )],
}];
const BIASED_COGNITION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        17,
        Amount::fixed(-1, -1),
    )],
}];
const COOLANT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::RawBlock(
        Target::Source,
        Amount::scaled(Scale::OrbTypesPower(25), 1),
    )],
}];
const HAILSTORM_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::If(
        Condition::HasOrb(1),
        &[Effect::Damage(
            Target::AllEnemies,
            Amount::scaled(Scale::Power(26), 1),
        )],
        &[],
    )],
}];
const LOOP_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Repeat(
        Amount::scaled(Scale::Power(27), 1),
        &[Effect::PassiveFirst(1)],
    )],
}];
const SPINNER_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Repeat(
        Amount::scaled(Scale::Power(28), 1),
        &[Effect::Channel(4, 1)],
    )],
}];
const CONSUMING_SHADOW_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::EvokeLast(Amount::scaled(Scale::Power(142), 1))],
}];
const LIGHTNING_ROD_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![
        Effect::Channel(0, 1),
        Effect::ApplyPower(Target::Player, 143, Amount::fixed(-1, -1)),
    ],
}];
const NO_BLOCK_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Player,
        144,
        Amount::fixed(-1, -1),
    )],
}];
const CALAMITY_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::AttackPlayed,
    effects: effects![Effect::Repeat(
        Amount::scaled(Scale::Power(147), 1),
        &[Effect::RandomCard(Pile::Hand, CardType::Attack, 1, false)],
    )],
}];
const ENTROPY_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::SelectAmount(
        Pile::Hand,
        CardFilter::Any,
        Amount::scaled(Scale::Power(148), 1),
        false,
        CardOp::TransformRandom,
    )],
}];
const MAYHEM_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::AutoPlayDraw(
        Amount::scaled(Scale::Power(150), 1),
        false,
    )],
}];
const PREP_TIME_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Player,
        83,
        Amount::scaled(Scale::Power(153), 1),
    )],
}];
const STRATAGEM_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::Shuffle,
    effects: effects![Effect::SelectAmount(
        Pile::Draw,
        CardFilter::Any,
        Amount::scaled(Scale::Power(155), 1),
        false,
        CardOp::Move(Pile::Hand),
    )],
}];
const RITUAL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        0,
        Amount::scaled(Scale::Power(160), 1),
    )],
}];
const DEMISE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::LoseHp(
        Target::Source,
        Amount::scaled(Scale::Power(161), 1),
    )],
}];
const RADIANCE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![
        Effect::Energy(1),
        Effect::ApplyPower(Target::Source, 162, Amount::fixed(-1, -1)),
    ],
}];
const REGEN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![
        Effect::Heal(Target::Source, Amount::scaled(Scale::Power(163), 1)),
        Effect::ApplyPower(Target::Source, 163, Amount::fixed(-1, -1)),
    ],
}];
const STORM_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::PowerPlayed,
    effects: effects![Effect::Repeat(
        Amount::scaled(Scale::Event, 1),
        effects![Effect::Channel(0, 1)],
    )],
}];
const SUBROUTINE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::PowerPlayed,
    effects: effects![Effect::Repeat(
        Amount::scaled(Scale::Event, 1),
        effects![Effect::Energy(1)]
    )],
}];
const ITERATION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardDrawn,
    effects: effects![Effect::If(
        Condition::CardType(CardType::Status),
        &[Effect::DrawAmount(Amount::scaled(Scale::Event, 1))],
        &[],
    )],
}];
const SMOKESTACK_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardGenerated,
    effects: effects![Effect::RawBlock(
        Target::Source,
        Amount::scaled(Scale::Power(34), 1),
    )],
}];
const CREATIVE_AI_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Repeat(
        Amount::scaled(Scale::Power(36), 1),
        &[Effect::RandomCard(Pile::Hand, CardType::Power, 1, false)],
    )],
}];
const TRASH_TO_TREASURE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardGenerated,
    effects: effects![Effect::If(
        Condition::CardType(CardType::Status),
        &[Effect::Repeat(
            Amount::scaled(Scale::Event, 1),
            &[Effect::RandomOrb([1, 1])],
        )],
        &[],
    )],
}];
const ARSENAL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardGenerated,
    effects: effects![Effect::ApplyPower(
        Target::Player,
        0,
        Amount::scaled(Scale::Event, 1),
    )],
}];
const ANCHOR_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::RawBlock(Target::Player, Amount::fixed(10, 10))],
}];
const FAKE_ANCHOR_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::RawBlock(Target::Player, Amount::fixed(4, 4))],
}];
const AKABEKO_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::ApplyPower(Target::Player, 83, Amount::fixed(8, 8))],
}];
const NEOW_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddRelic("RELIC.NEOWS_BONES")],
    },
];
const BAG_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::Draw(2)],
}];
const LANTERN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::Energy(1)],
}];
const VAJRA_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::ApplyPower(Target::Player, 0, Amount::fixed(1, 1))],
}];
const STONE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::ApplyPower(Target::Player, 1, Amount::fixed(1, 1))],
}];
const DIVINE_RIGHT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::Stars(Amount::fixed(3, 3))],
}];
const DIVINE_DESTINY_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::If(
        Condition::FirstTurn,
        effects![Effect::Stars(Amount::fixed(6, 6))],
        &[]
    )],
}];
const BOUND_PHYLACTERY_HOOKS: &[Hook] = &[
    Hook {
        trigger: Trigger::CombatStart,
        effects: effects![Effect::Summon(Amount::fixed(1, 1))],
    },
    Hook {
        trigger: Trigger::TurnStart,
        effects: effects![Effect::If(
            Condition::FirstTurn,
            &[],
            effects![Effect::Summon(Amount::fixed(1, 1))]
        )],
    },
];
const CRACKED_CORE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::Channel(0, 1)],
}];
const INFUSED_CORE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::If(
        Condition::FirstTurn,
        &[Effect::Channel(0, 3)],
        &[],
    )],
}];
const DATA_DISK_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::ApplyPower(Target::Player, 17, Amount::fixed(1, 1))],
}];
const RUNIC_CAPACITOR_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::If(
        Condition::FirstTurn,
        &[Effect::OrbSlots(3)],
        &[],
    )],
}];
const SYMBIOTIC_VIRUS_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::If(
        Condition::FirstTurn,
        &[Effect::Channel(2, 1)],
        &[],
    )],
}];
const BAG_OF_MARBLES_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::If(
        Condition::FirstTurn,
        &[Effect::ApplyPower(
            Target::AllEnemies,
            3,
            Amount::fixed(1, 1),
        )],
        &[],
    )],
}];
const BLOOD_VIAL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::If(
        Condition::FirstTurn,
        &[Effect::Heal(Target::Player, Amount::fixed(2, 2))],
        &[],
    )],
}];
const FAKE_BLOOD_VIAL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::If(
        Condition::FirstTurn,
        &[Effect::Heal(Target::Player, Amount::fixed(1, 1))],
        &[],
    )],
}];
const BRONZE_SCALES_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::ApplyPower(Target::Player, 8, Amount::fixed(3, 3))],
}];
const MERCURY_HOURGLASS_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::Damage(Target::AllEnemies, Amount::fixed(3, 3))],
}];
const RED_MASK_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::If(
        Condition::FirstTurn,
        &[Effect::ApplyPower(
            Target::AllEnemies,
            2,
            Amount::fixed(1, 1),
        )],
        &[],
    )],
}];
const BLACK_BLOOD_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::Victory,
    effects: &[Effect::Heal(Target::Player, Amount::fixed(12, 12))],
}];
const CLOAK_CLASP_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: &[Effect::RawBlock(Target::Player, Amount::fixed(1, 1))],
}];
const DAUGHTER_WIND_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::AttackPlayed,
    effects: &[Effect::RawBlock(Target::Player, Amount::fixed(1, 1))],
}];
const FENCING_MANUAL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: &[Effect::Forge(Amount::fixed(10, 10))],
}];
const FESTIVE_POPPER_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: &[Effect::Damage(Target::AllEnemies, Amount::fixed(9, 9))],
}];
const LOST_WISP_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::PowerPlayed,
    effects: &[Effect::Damage(Target::AllEnemies, Amount::fixed(8, 8))],
}];
const SAI_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: &[Effect::RawBlock(Target::Player, Amount::fixed(7, 7))],
}];
const BURN_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::Damage(Target::Player, Amount::fixed(2, 2))],
}];
const INFECTION_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::Damage(Target::Player, Amount::fixed(3, 3))],
}];
const WITHER_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::Damage(
        Target::Player,
        Amount {
            base: 3,
            upgraded: 3,
            ascension: 0,
            scale: Scale::CardValue,
            multiplier: 3,
            divisor: 1,
        },
    )],
}];
const DOUBT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(Target::Player, 2, Amount::fixed(1, 1))],
}];
const REGRET_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::Damage(
        Target::Player,
        Amount::scaled(Scale::HandSize, 1),
    )],
}];
const VITAL_SPARK_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardPlayed,
    effects: effects![Effect::If(
        Condition::CardType(CardType::Skill),
        effects![Effect::ApplyPower(
            Target::Player,
            267,
            Amount::scaled(Scale::Power(265), 1),
        )],
        &[],
    )],
}];
const SHAME_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::ApplyPower(Target::Player, 4, Amount::fixed(1, 1))],
}];
const BAD_LUCK_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::LoseHp(Target::Player, Amount::fixed(13, 13))],
}];
const DEBT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::Gold(Amount::fixed(-10, -10))],
}];
const DECAY_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnEnd,
    effects: effects![Effect::Damage(Target::Player, Amount::fixed(2, 2))],
}];
const VOID_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardDrawn,
    effects: effects![Effect::Energy(-1)],
}];
const DRUM_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardExhausted,
    effects: effects![Effect::If(
        Condition::Upgraded,
        effects![Effect::Energy(3)],
        effects![Effect::Energy(2)]
    )],
}];
const EMPTY_HOOKS: &[Hook] = &[];
const BURNING_BLOOD_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::Victory,
    effects: effects![Effect::Heal(Target::Player, Amount::fixed(6, 6))],
}];
const CHARONS_ASHES_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardExhausted,
    effects: effects![Effect::Damage(Target::AllEnemies, Amount::fixed(3, 3),)],
}];
const ABACUS_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::Shuffle,
    effects: effects![Effect::RawBlock(Target::Player, Amount::fixed(6, 6))],
}];
const TWISTED_FUNNEL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CombatStart,
    effects: effects![Effect::ApplyPower(
        Target::AllEnemies,
        power_id::POISON,
        Amount::fixed(4, 4),
    )],
}];
const FORGOTTEN_SOUL_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardExhausted,
    effects: effects![Effect::Damage(Target::RandomEnemy, Amount::fixed(1, 1),)],
}];
const REGALITE_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::CardGenerated,
    effects: effects![Effect::RawBlock(Target::Player, Amount::fixed(2, 2))],
}];
const SANDPIT_HOOKS: &[Hook] = &[Hook {
    trigger: Trigger::TurnStart,
    effects: effects![Effect::ApplyPower(
        Target::Source,
        power_id::SANDPIT,
        Amount::fixed(-1, -1),
    )],
}];
const WIKI_AXEBOT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Block, Buff",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Block(Target::Source, Amount::ascended(10, 15))],
    },
    MoveDef {
        intent: "Attack 9x2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(9, 10), 2)],
    },
    MoveDef {
        intent: "Attack 12, Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(12, 14), 1),
            Effect::ApplyPower(Target::Player, 2, Amount::fixed(2, 2)),
            Effect::ApplyPower(Target::Player, 4, Amount::fixed(2, 2)),
        ],
    },
];
const WIKI_BATTLE_FRIEND_V1_0_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Wait",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[],
}];
const WIKI_BATTLE_FRIEND_V2_0_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Wait",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[],
}];
const WIKI_BATTLE_FRIEND_V3_0_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Wait",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[],
}];
const WIKI_BOWLBUG_EGG_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Attack 7, Block 7",
    weight: 1,
    max_repeats: 1,
    next: &[0],
    effects: &[
        Effect::Attack(Target::Player, Amount::ascended(7, 8), 1),
        Effect::Block(Target::Source, Amount::ascended(7, 8)),
    ],
}];
const WIKI_BOWLBUG_NECTAR_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 3",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(3, 3), 1)],
    },
    MoveDef {
        intent: "Strength 15",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::ApplyPower(
            Target::Source,
            0,
            Amount::ascended(15, 16),
        )],
    },
    MoveDef {
        intent: "Attack 3",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(3, 3), 1)],
    },
];
const WIKI_BOWLBUG_ROCK_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(15, 16), 1)],
    },
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[],
    },
];
const WIKI_BOWLBUG_SILK_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 4x2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 5), 2)],
    },
    MoveDef {
        intent: "Weak 1",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(Target::Player, 2, Amount::fixed(1, 1))],
    },
];
const WIKI_BYGONE_EFFIGY_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Sleep",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
    MoveDef {
        intent: "Buff",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(10, 10))],
    },
    MoveDef {
        intent: "Sleep",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(13, 15), 1)],
    },
];
const WIKI_BYRDONIS_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 19",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(17, 19), 1)],
    },
    MoveDef {
        intent: "Attack 4x3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(3, 4), 3)],
    },
];
const WIKI_BYRDPIP_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Wait",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[],
}];
const WIKI_CALCIFIED_CULTIST_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Buff 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(Target::Source, 160, Amount::fixed(2, 2))],
    },
    MoveDef {
        intent: "Attack 9",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(9, 11), 1)],
    },
];
const WIKI_CEREMONIAL_BEAST_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Plow 150",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::PLOW,
            Amount::ascended(150, 160),
        )],
    },
    MoveDef {
        intent: "Attack 18, Strength 2",
        weight: 1,
        max_repeats: u8::MAX,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(18, 20), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[],
    },
    MoveDef {
        intent: "Ringing",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::ApplyPower(
            Target::Player,
            power_id::RINGING,
            Amount::fixed(1, 1),
        )],
    },
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 1,
        next: &[5],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(15, 17), 1)],
    },
    MoveDef {
        intent: "Attack 17, Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(17, 19), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::ascended(3, 4)),
        ],
    },
];
const WIKI_CHOMPER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 9x2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 2)],
    },
    MoveDef {
        intent: "StatusCard",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::AddCard(Pile::Discard, card_id::DAZED, 3)],
    },
];
const WIKI_CORPSE_SLUG_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 3x2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(3, 3), 2)],
    },
    MoveDef {
        intent: "Attack 8, Weak 1",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Player, power_id::WEAK, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(Target::Player, 4, Amount::fixed(2, 2))],
    },
];
const WIKI_CRUSHER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 12",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(12, 14), 1)],
    },
    MoveDef {
        intent: "Attack 4",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(4, 4), 1)],
    },
    MoveDef {
        intent: "Attack 6x2",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(6, 7), 2),
            Effect::ApplyPower(Target::Player, power_id::WEAK, Amount::fixed(2, 2)),
            Effect::ApplyPower(Target::Player, power_id::FRAIL, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::STRENGTH,
            Amount::ascended(2, 3),
        )],
    },
    MoveDef {
        intent: "Attack 12, Block 18",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(12, 14), 1),
            Effect::Block(Target::Source, Amount::fixed(18, 18)),
        ],
    },
];
const WIKI_CUBEX_CONSTRUCT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::STRENGTH,
            Amount::fixed(2, 2),
        )],
    },
    MoveDef {
        intent: "Attack 7, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(7, 8), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 7, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(7, 8), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 5x2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(5, 6), 2)],
    },
];
const WIKI_DAMP_CULTIST_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Buff 5",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(
            Target::Source,
            160,
            Amount::ascended(5, 6),
        )],
    },
    MoveDef {
        intent: "Attack 1",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(1, 3), 1)],
    },
];
const WIKI_DECIMILLIPEDE_3_SEGMENTS_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 5x2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(5, 6), 2)],
    },
    MoveDef {
        intent: "Attack 6, Buff 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(6, 7), 1),
            Effect::ApplyPower(Target::Source, 0, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 8, Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Player, 2, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Dead",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[],
    },
    MoveDef {
        intent: "Heal 25",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1, 2],
        effects: &[Effect::Heal(Target::Source, Amount::fixed(25, 25))],
    },
];
const WIKI_DEVOTED_SCULPTOR_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Ritual 9",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::RITUAL,
            Amount::fixed(9, 9),
        )],
    },
    MoveDef {
        intent: "Attack 12",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(12, 15), 1)],
    },
];
const WIKI_DOOR_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 25",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(25, 25), 1)],
    },
    MoveDef {
        intent: "Attack 15x2",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(15, 15), 2)],
    },
    MoveDef {
        intent: "Attack 20",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(20, 20), 1)],
    },
    MoveDef {
        intent: "Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(3, 3))],
    },
];
const WIKI_DOORMAKER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 31",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(31, 31), 1)],
    },
    MoveDef {
        intent: "Attack 40",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(40, 40), 1)],
    },
    MoveDef {
        intent: "Strength 5",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(5, 5))],
    },
];
const AEONGLASS_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 26, Block 33",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(26, 32), 1),
            Effect::Block(Target::Source, Amount::fixed(33, 33)),
        ],
    },
    MoveDef {
        intent: "Attack 11x2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(11, 12), 2)],
    },
    MoveDef {
        intent: "Status, Buff",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Repeat(
                Amount::ascended(1, 2),
                effects![Effect::AddCard(Pile::Discard, 521, 1)],
            ),
            Effect::GrowAll(521, Amount::fixed(1, 1)),
            Effect::ApplyPower(
                Target::Source,
                0,
                Amount {
                    base: 2,
                    upgraded: 3,
                    ascension: 9,
                    scale: Scale::TurnDiv(3),
                    multiplier: 1,
                    divisor: 1,
                },
            ),
        ],
    },
];
const WIKI_ENTOMANCER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 3x8",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::AttackMany(
            Target::Player,
            Amount::fixed(3, 3),
            Amount::ascended(7, 8),
        )],
    },
    MoveDef {
        intent: "Attack 20",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(18, 20), 1)],
    },
    MoveDef {
        intent: "Strength",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[],
    },
];
const WIKI_EXOSKELETON_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 1x3",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1],
        effects: &[Effect::AttackMany(
            Target::Player,
            Amount::fixed(1, 1),
            Amount::ascended(3, 4),
        )],
    },
    MoveDef {
        intent: "Attack 8",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 1)],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(2, 2))],
    },
];
const WIKI_EYE_WITH_TEETH_MINION_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Dazed 3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::AddCard(Pile::Discard, card_id::DAZED, 3)],
    },
    MoveDef {
        intent: "Heal",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Heal(Target::Source, Amount::fixed(6, 6))],
    },
];
const WIKI_FABRICATOR_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Summon",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 18, Summon",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(18, 21), 1)],
    },
    MoveDef {
        intent: "Attack 11",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(11, 13), 1)],
    },
];
const WIKI_FAT_GREMLIN_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
    MoveDef {
        intent: "Escape",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
];
const WIKI_FLAIL_KNIGHT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(3, 3))],
    },
    MoveDef {
        intent: "Attack 9x2",
        weight: 1,
        max_repeats: 2,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(9, 10), 2)],
    },
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 2,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(15, 17), 1)],
    },
];
const WIKI_FLYCONID_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Vulnerable 2",
        weight: 3,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::ApplyPower(
            Target::Player,
            power_id::VULNERABLE,
            Amount::fixed(2, 2),
        )],
    },
    MoveDef {
        intent: "Attack 9, Frail 2",
        weight: 2,
        max_repeats: 1,
        next: &[],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Player, power_id::FRAIL, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 12",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(11, 12), 1)],
    },
];
const WIKI_FOGMOG_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Summon",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 8, Strength 1",
        weight: 1,
        max_repeats: 1,
        next: &[2, 3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 8, Strength 1",
        weight: 2,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 14",
        weight: 3,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
];
const WIKI_FOSSIL_STALKER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 9, Frail 1",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(9, 11), 1),
            Effect::ApplyPower(Target::Player, power_id::FRAIL, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 12",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(12, 14), 1)],
    },
    MoveDef {
        intent: "Attack 3x2",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(3, 4), 2)],
    },
];
const WIKI_FROG_KNIGHT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Strength 5",
        weight: 1,
        max_repeats: 1,
        next: &[2, 3],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::STRENGTH,
            Amount::fixed(5, 5),
        )],
    },
    MoveDef {
        intent: "Attack 21",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(21, 23), 1)],
    },
    MoveDef {
        intent: "Attack 13, Frail 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(13, 14), 1),
            Effect::ApplyPower(Target::Player, power_id::FRAIL, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 35",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(35, 40), 1)],
    },
];
const WIKI_FUZZY_WURM_CRAWLER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 6, Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 6), 1)],
    },
    MoveDef {
        intent: "Strength 7",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(7, 7))],
    },
    MoveDef {
        intent: "Attack 6",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 6), 1)],
    },
];
const WIKI_GAS_BOMB_MINION_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Attack 8",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 1)],
}];
const WIKI_GLOBE_HEAD_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 13, Frail 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(13, 14), 1),
            Effect::ApplyPower(Target::Player, power_id::FRAIL, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 6x3",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(6, 7), 3)],
    },
    MoveDef {
        intent: "Attack 16, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(16, 17), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
        ],
    },
];
const WIKI_GREMLIN_MERC_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 7x2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::tough(7, 8), 2)],
    },
    MoveDef {
        intent: "Attack 6x2, Weak 2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::tough(6, 7), 2),
            Effect::ApplyPower(Target::Player, power_id::WEAK, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 8, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::tough(8, 9), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
        ],
    },
];
const WIKI_GUARDBOT_MINION_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Block 15",
    weight: 1,
    max_repeats: 1,
    next: &[0],
    effects: &[],
}];
const WIKI_HATCHLING_MINION_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Attack 4",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[Effect::Attack(Target::Player, Amount::fixed(4, 4), 1)],
}];
const WIKI_HAUNTED_SHIP_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 13",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(13, 14), 1)],
    },
    MoveDef {
        intent: "Attack 4x3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 5), 3)],
    },
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::ApplyPower(Target::Player, 2, Amount::fixed(3, 3)),
            Effect::AddCard(Pile::Discard, 58, 5),
        ],
    },
];
const WIKI_HUNTER_KILLER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2],
        effects: &[Effect::ApplyDebuff(
            Target::Player,
            259,
            Amount::fixed(1, 1),
        )],
    },
    MoveDef {
        intent: "Attack 19",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(17, 19), 1)],
    },
    MoveDef {
        intent: "Attack 8x3",
        weight: 2,
        max_repeats: u8::MAX,
        next: &[1, 2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(7, 8), 3)],
    },
];
const WIKI_INFESTED_PRISM_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(15, 17), 1)],
    },
    MoveDef {
        intent: "Attack 11, Block 11",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(11, 13), 1),
            Effect::Block(Target::Source, Amount::ascended(11, 13)),
        ],
    },
    MoveDef {
        intent: "Attack 5x3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(5, 6), 3)],
    },
    MoveDef {
        intent: "Attack 8, Block 20, Buff 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(8, 10), 1),
            Effect::Block(Target::Source, Amount::tough(20, 22)),
            Effect::ApplyPower(Target::Source, 265, Amount::ascended(2, 3)),
        ],
    },
];
const WIKI_INKLET_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 3",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(3, 4), 1)],
    },
    MoveDef {
        intent: "Attack 2x3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(2, 3), 3)],
    },
    MoveDef {
        intent: "Attack 10",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(10, 11), 1)],
    },
];
const WIKI_KNOWLEDGE_DEMON_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 17",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(17, 18), 1)],
    },
    MoveDef {
        intent: "Attack 8x3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 3)],
    },
    MoveDef {
        intent: "Attack 11, Heal 30, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(11, 13), 1),
            Effect::Heal(Target::Source, Amount::fixed(30, 30)),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::ascended(2, 3)),
        ],
    },
];
const WIKI_LAGAVULIN_MATRIARCH_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Sleep",
        weight: 1,
        max_repeats: u8::MAX,
        next: &[0, 1],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 19",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(19, 21), 1)],
    },
    MoveDef {
        intent: "Attack 12, Block 12",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(12, 14), 1),
            Effect::Block(Target::Source, Amount::tough(12, 14)),
        ],
    },
    MoveDef {
        intent: "Attack 9x2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(9, 10), 2)],
    },
    MoveDef {
        intent: "Strength -2, Dexterity -2, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::ApplyDebuff(Target::Player, power_id::STRENGTH, Amount::fixed(-2, -2)),
            Effect::ApplyDebuff(Target::Player, power_id::DEXTERITY, Amount::fixed(-2, -2)),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Wake",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
];
const WIKI_LIVING_FOG_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 8, Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Player, 245, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 5, Summon",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(5, 6), 1)],
    },
    MoveDef {
        intent: "Attack 8",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 1)],
    },
];
const WIKI_LIVING_SHIELD_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 6",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(6, 6), 1)],
    },
    MoveDef {
        intent: "Attack 16, Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(16, 18), 1),
            Effect::ApplyPower(Target::Source, 0, Amount::fixed(3, 3)),
        ],
    },
];
const WIKI_LOUSE_PROGENITOR_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 9",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(9, 10), 1),
            Effect::ApplyPower(Target::Player, power_id::FRAIL, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 14",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
    MoveDef {
        intent: "Block 14, Strength 5",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Block(Target::Source, Amount::tough(14, 18)),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(5, 5)),
        ],
    },
];
const WIKI_MAGI_KNIGHT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 6 Block 5",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(6, 7), 1),
            Effect::Block(Target::Source, Amount::tough(5, 9)),
        ],
    },
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::ApplyDebuff(
            Target::Player,
            power_id::DAMPEN,
            Amount::fixed(1, 1),
        )],
    },
    MoveDef {
        intent: "Attack 10",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(10, 11), 1)],
    },
    MoveDef {
        intent: "Block 5",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::Block(Target::Source, Amount::tough(5, 9))],
    },
    MoveDef {
        intent: "Attack 35",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(35, 40), 1)],
    },
];
const WIKI_MAWLER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 14",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
    MoveDef {
        intent: "Vulnerable 3",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::ApplyPower(
            Target::Player,
            power_id::VULNERABLE,
            Amount::fixed(3, 3),
        )],
    },
    MoveDef {
        intent: "Attack 4x2",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 5), 2)],
    },
];
const WIKI_MECHA_KNIGHT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 25",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(25, 30), 1)],
    },
    MoveDef {
        intent: "Status 4",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::AddCard(Pile::Hand, 62, 4)],
    },
    MoveDef {
        intent: "Block 15 Strength 5",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Block(Target::Source, Amount::fixed(15, 15)),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(5, 5)),
        ],
    },
    MoveDef {
        intent: "Attack 35",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(35, 40), 1)],
    },
];
const WIKI_MYTE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Toxic 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::AddCard(Pile::Hand, card_id::TOXIC, 2)],
    },
    MoveDef {
        intent: "Attack 13",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(13, 15), 1)],
    },
    MoveDef {
        intent: "Attack 4, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(4, 6), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::ascended(2, 3)),
        ],
    },
];
const WIKI_NIBBIT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 13",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(12, 13), 1)],
    },
    MoveDef {
        intent: "Attack 7, Block 6",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(6, 7), 1),
            Effect::Block(Target::Source, Amount::ascended(5, 6)),
        ],
    },
    MoveDef {
        intent: "Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::STRENGTH,
            Amount::ascended(2, 3),
        )],
    },
];
const WIKI_NOISEBOT_MINION_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Dazed 2",
    weight: 1,
    max_repeats: 1,
    next: &[0],
    effects: &[],
}];
const WIKI_OSTY_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Wait",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[],
}];
const WIKI_OWL_MAGISTRATE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(16, 17), 1)],
    },
    MoveDef {
        intent: "Attack 4x6",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(4, 4), 6)],
    },
    MoveDef {
        intent: "Soar",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::SOAR,
            Amount::fixed(1, 1),
        )],
    },
    MoveDef {
        intent: "Attack 33, Vulnerable 4",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(33, 36), 1),
            Effect::ApplyPower(Target::Player, power_id::VULNERABLE, Amount::fixed(4, 4)),
            Effect::RemovePower(Target::Source, power_id::SOAR),
        ],
    },
];
const WIKI_PAEL_S_LEGION_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Wait",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[],
}];
const WIKI_PARAFRIGHT_MINION_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(16, 17), 1)],
    },
    MoveDef {
        intent: "Heal",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Heal(Target::Source, Amount::fixed(21, 21))],
    },
];
const WIKI_PHANTASMAL_GARDENER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 5",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(5, 5), 1)],
    },
    MoveDef {
        intent: "Attack 7",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::tough(6, 7), 1)],
    },
    MoveDef {
        intent: "Attack 1x3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(1, 1), 3)],
    },
    MoveDef {
        intent: "Buff 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(
            Target::Source,
            0,
            Amount::ascended(2, 3),
        )],
    },
];
const WIKI_PHROG_PARASITE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Infection 3",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::AddCard(Pile::Discard, 519, 3)],
    },
    MoveDef {
        intent: "Attack 5x4",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 5), 4)],
    },
];
const WIKI_PUNCH_CONSTRUCT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Block 10",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Block(Target::Source, Amount::fixed(10, 10))],
    },
    MoveDef {
        intent: "Attack 5x2, Frail 1",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(5, 6), 2),
            Effect::ApplyPower(Target::Player, power_id::FRAIL, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 14",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
];
const WIKI_QUEEN_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyDebuff(
            Target::Player,
            power_id::CHAINS_OF_BINDING,
            Amount::fixed(3, 3),
        )],
    },
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::ApplyDebuff(Target::Player, power_id::FRAIL, Amount::fixed(99, 99)),
            Effect::ApplyDebuff(Target::Player, power_id::WEAK, Amount::fixed(99, 99)),
            Effect::ApplyDebuff(Target::Player, power_id::VULNERABLE, Amount::fixed(99, 99)),
        ],
    },
    MoveDef {
        intent: "Buff Block 20",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::ApplyPower(
                Target::OtherEnemies,
                power_id::STRENGTH,
                Amount::fixed(1, 1),
            ),
            Effect::Block(Target::Source, Amount::fixed(20, 20)),
        ],
    },
    MoveDef {
        intent: "Attack 3x5",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(3, 4), 5)],
    },
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 1,
        next: &[5],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(15, 18), 1)],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(2, 2))],
    },
];
const WIKI_ROCKET_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 3",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(3, 4), 1)],
    },
    MoveDef {
        intent: "Attack 18",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(18, 20), 1)],
    },
    MoveDef {
        intent: "Buff 2",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::ApplyPower(
            Target::Source,
            0,
            Amount::ascended(2, 3),
        )],
    },
    MoveDef {
        intent: "Attack 31",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(31, 35), 1)],
    },
    MoveDef {
        intent: "Sleep",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[],
    },
];
const WIKI_RUBY_RAIDER_ASSASSIN_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Attack 11",
    weight: 1,
    max_repeats: 1,
    next: &[0],
    effects: &[Effect::Attack(Target::Player, Amount::ascended(10, 11), 1)],
}];
const WIKI_RUBY_RAIDER_AXE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 5",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(5, 6), 1),
            Effect::Block(Target::Source, Amount::ascended(5, 6)),
        ],
    },
    MoveDef {
        intent: "Attack 5, Block 5",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(5, 6), 1),
            Effect::Block(Target::Source, Amount::ascended(5, 6)),
        ],
    },
    MoveDef {
        intent: "Attack 12",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(12, 13), 1)],
    },
];
const WIKI_RUBY_RAIDER_BRUTE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 7",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(7, 8), 1)],
    },
    MoveDef {
        intent: "Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(3, 3))],
    },
];
const WIKI_RUBY_RAIDER_CROSSBOW_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Block 3",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Block(Target::Source, Amount::fixed(3, 3))],
    },
    MoveDef {
        intent: "Attack 14",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
];
const WIKI_RUBY_RAIDER_TRACKER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(Target::Player, 4, Amount::fixed(2, 2))],
    },
    MoveDef {
        intent: "Attack 1x8",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::AttackMany(
            Target::Player,
            Amount::fixed(1, 1),
            Amount::ascended(8, 9),
        )],
    },
];
const WIKI_SCROLL_OF_BITING_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 14",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
    MoveDef {
        intent: "Attack 5x2",
        weight: 2,
        max_repeats: u8::MAX,
        next: &[0, 1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(5, 6), 2)],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(2, 2))],
    },
];
const WIKI_SEAPUNK_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 11",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(11, 13), 1)],
    },
    MoveDef {
        intent: "Attack 2x4",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(2, 2), 4)],
    },
    MoveDef {
        intent: "Block 7, Strength 1",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Block(Target::Source, Amount::tough(7, 8)),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::ascended(1, 2)),
        ],
    },
];
const WIKI_SEWER_CLAM_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 10",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(10, 11), 1)],
    },
    MoveDef {
        intent: "Strength 4",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::STRENGTH,
            Amount::fixed(4, 4),
        )],
    },
];
const WIKI_SHRINKER_BEETLE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Shrink -1",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(
            Target::Player,
            156,
            Amount::fixed(-1, -1),
        )],
    },
    MoveDef {
        intent: "Attack 8",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(7, 8), 1)],
    },
    MoveDef {
        intent: "Attack 14",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(13, 14), 1)],
    },
];
const WIKI_SKULKING_COLONY_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
    MoveDef {
        intent: "Attack 9",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(9, 11), 1),
            Effect::ApplyPower(Target::Source, 0, Amount::ascended(2, 4)),
        ],
    },
    MoveDef {
        intent: "Attack 7x2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(7, 8), 2)],
    },
];
const WIKI_SLIMED_BERSERKER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Slimed 10",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::AddCard(Pile::Discard, card_id::SLIMED, 10)],
    },
    MoveDef {
        intent: "Attack 4x4",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 5), 4)],
    },
    MoveDef {
        intent: "Weak 3, Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::ApplyPower(Target::Player, power_id::WEAK, Amount::fixed(3, 3)),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(3, 3)),
        ],
    },
    MoveDef {
        intent: "Attack 30",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(30, 33), 1)],
    },
];
const WIKI_SLITHERING_STRANGLER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Constrict 3",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2],
        effects: &[Effect::ApplyPower(
            Target::Player,
            power_id::CONSTRICT,
            Amount::fixed(3, 3),
        )],
    },
    MoveDef {
        intent: "Attack 8, Block 5",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(7, 8), 1),
            Effect::Block(Target::Source, Amount::fixed(5, 5)),
        ],
    },
    MoveDef {
        intent: "Attack 13",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(12, 13), 1)],
    },
];
const WIKI_SLUDGE_SPINNER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 8",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 1)],
    },
    MoveDef {
        intent: "Attack 11",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(11, 12), 1)],
    },
    MoveDef {
        intent: "Attack 6",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(6, 7), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(3, 3)),
        ],
    },
];
const WIKI_SLUMBERING_BEETLE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Sleep",
        weight: 1,
        max_repeats: u8::MAX,
        next: &[0, 1],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 16, Strength 2",
        weight: 1,
        max_repeats: u8::MAX,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(16, 18), 1),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
];
const WIKI_SNAPPING_JAXFRUIT_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Attack 4, Strength 2",
    weight: 1,
    max_repeats: 1,
    next: &[0],
    effects: &[
        Effect::Attack(Target::Player, Amount::ascended(3, 4), 1),
        Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
    ],
}];
const WIKI_SNEAKY_GREMLIN_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 9",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(9, 10), 1)],
    },
];
const WIKI_SOUL_FYSH_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Beckon 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::AddRandom(Pile::Draw, card_id::BECKON, 1),
            Effect::AddCard(Pile::Discard, card_id::BECKON, 1),
        ],
    },
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(16, 17), 1)],
    },
    MoveDef {
        intent: "Attack 7, Beckon",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(7, 8), 1),
            Effect::AddCard(Pile::Discard, card_id::BECKON, 1),
        ],
    },
    MoveDef {
        intent: "Intangible 2",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::INTANGIBLE,
            Amount::fixed(2, 2),
        )],
    },
    MoveDef {
        intent: "Attack 13, Vulnerable 3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(13, 15), 1),
            Effect::ApplyDebuff(Target::Player, power_id::VULNERABLE, Amount::fixed(3, 3)),
        ],
    },
];
const WIKI_SOUL_NEXUS_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 29",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(29, 31), 1)],
    },
    MoveDef {
        intent: "Attack 6x4",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(6, 7), 4)],
    },
    MoveDef {
        intent: "Attack 18",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(18, 19), 1),
            Effect::ApplyDebuff(Target::Player, power_id::VULNERABLE, Amount::fixed(2, 2)),
            Effect::ApplyDebuff(Target::Player, power_id::WEAK, Amount::fixed(2, 2)),
        ],
    },
];
const WIKI_SPECTRAL_KNIGHT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyDebuff(
            Target::Player,
            power_id::HEX,
            Amount::fixed(2, 2),
        )],
    },
    MoveDef {
        intent: "Attack 15",
        weight: 2,
        max_repeats: u8::MAX,
        next: &[1, 2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(15, 17), 1)],
    },
    MoveDef {
        intent: "Attack 3x3",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(3, 4), 3)],
    },
];
const WIKI_SPINY_TOAD_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Buff 5",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(Target::Source, 8, Amount::fixed(5, 5))],
    },
    MoveDef {
        intent: "Attack 23",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(23, 25), 1),
            Effect::ApplyPower(Target::Source, 8, Amount::fixed(-5, -5)),
        ],
    },
    MoveDef {
        intent: "Attack 17",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(17, 19), 1)],
    },
];
const WIKI_STABBOT_MINION_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Attack 11, Frail 1",
    weight: 1,
    max_repeats: 1,
    next: &[0],
    effects: &[
        Effect::Attack(Target::Player, Amount::ascended(11, 12), 1),
        Effect::ApplyPower(Target::Player, power_id::FRAIL, Amount::fixed(1, 1)),
    ],
}];
const WIKI_TEST_SUBJECT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Heal, Buff",
        weight: 1,
        max_repeats: 1,
        next: &[3, 4],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 20",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(20, 22), 1)],
    },
    MoveDef {
        intent: "Attack 14, Vulnerable 1",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(14, 16), 1),
            Effect::ApplyPower(Target::Player, power_id::VULNERABLE, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 10x3",
        weight: 1,
        max_repeats: u8::MAX,
        next: &[3],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 10x3",
        weight: 1,
        max_repeats: 1,
        next: &[5],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(10, 11), 3)],
    },
    MoveDef {
        intent: "Attack 45",
        weight: 1,
        max_repeats: 1,
        next: &[6],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(45, 45), 1)],
    },
    MoveDef {
        intent: "StatusCard, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[
            Effect::AddCard(Pile::Discard, card_id::BURN, 3),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::ascended(2, 3)),
        ],
    },
];
const WIKI_STAGE_1_MOVES: &[MoveDef] = WIKI_TEST_SUBJECT_MOVES;
const WIKI_STAGE_2_MOVES: &[MoveDef] = WIKI_TEST_SUBJECT_MOVES;
const WIKI_STAGE_3_MOVES: &[MoveDef] = WIKI_TEST_SUBJECT_MOVES;
const WIKI_TERROR_EEL_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(16, 18), 1)],
    },
    MoveDef {
        intent: "Attack 3x3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(3, 4), 3),
            Effect::ApplyPower(Target::Source, 83, Amount::fixed(6, 6)),
        ],
    },
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[],
    },
    MoveDef {
        intent: "Debuff",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(Target::Player, 3, Amount::fixed(99, 99))],
    },
];
const WIKI_TEST_SUBJECT_C_COUNT_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Wait",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[],
}];
const WIKI_THE_ADVERSARY_MK_1_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 12",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(12, 12), 1)],
    },
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(15, 15), 1)],
    },
    MoveDef {
        intent: "Attack 8x2, Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::fixed(8, 8), 2),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
        ],
    },
];
const WIKI_THE_ADVERSARY_MK_2_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 13",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(13, 13), 1)],
    },
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(16, 16), 1)],
    },
    MoveDef {
        intent: "Attack 9x2, Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::fixed(9, 9), 2),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(3, 3)),
        ],
    },
];
const WIKI_THE_ADVERSARY_MK_3_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(15, 15), 1)],
    },
    MoveDef {
        intent: "Attack 18",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(18, 18), 1)],
    },
    MoveDef {
        intent: "Attack 10x2, Strength 4",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[
            Effect::Attack(Target::Player, Amount::fixed(10, 10), 2),
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(4, 4)),
        ],
    },
];
const WIKI_THE_ARCHITECT_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Wait",
    weight: 1,
    max_repeats: 1,
    next: &[],
    effects: &[],
}];
const WIKI_THE_FORGOTTEN_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Debuff, Block 8, Buff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::ApplyDebuff(Target::Player, 1, Amount::fixed(-2, -2)),
            Effect::Block(Target::Source, Amount::fixed(8, 8)),
            Effect::ApplyPower(Target::Source, 1, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 13",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(
            Target::Player,
            Amount {
                base: 13,
                upgraded: 15,
                ascension: 9,
                scale: Scale::Power(1),
                multiplier: 1,
                divisor: 1,
            },
            1,
        )],
    },
];
const WIKI_THE_INSATIABLE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Buff, Status 6",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::ApplyPower(Target::Source, power_id::SANDPIT, Amount::fixed(4, 4)),
            Effect::AddRandom(Pile::Draw, 522, 3),
            Effect::AddRandom(Pile::Discard, 522, 3),
        ],
    },
    MoveDef {
        intent: "Attack 28",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(28, 31), 1)],
    },
    MoveDef {
        intent: "Attack 8x2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 2)],
    },
    MoveDef {
        intent: "Attack 8x2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 2)],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::ApplyPower(
            Target::Source,
            0,
            Amount::ascended(2, 3),
        )],
    },
];
const WIKI_THE_LOST_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Debuff, Buff",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::ApplyDebuff(Target::Player, 0, Amount::fixed(-2, -2)),
            Effect::ApplyPower(Target::Source, 0, Amount::fixed(2, 2)),
        ],
    },
    MoveDef {
        intent: "Attack 4x2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 5), 2)],
    },
];
const WIKI_THE_OBSCURA_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Summon",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2, 3],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 10",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2, 3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(10, 11), 1)],
    },
    MoveDef {
        intent: "Strength 3",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2, 3],
        effects: &[Effect::ApplyPower(
            Target::AllEnemies,
            0,
            Amount::fixed(3, 3),
        )],
    },
    MoveDef {
        intent: "Attack 6, Block 6",
        weight: 1,
        max_repeats: 1,
        next: &[1, 2, 3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(6, 7), 1),
            Effect::Block(Target::Source, Amount::ascended(6, 7)),
        ],
    },
];
const WIKI_THIEVING_HOPPER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 19",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(17, 19), 1)],
    },
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 16), 1)],
    },
    MoveDef {
        intent: "Attack 23",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(21, 23), 1)],
    },
    MoveDef {
        intent: "Buff",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::ApplyPower(Target::Source, 195, Amount::fixed(5, 5))],
    },
    MoveDef {
        intent: "Escape",
        weight: 1,
        max_repeats: u8::MAX,
        next: &[4],
        effects: &[],
    },
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[],
    },
];
const WIKI_TOADPOLE_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 3x3",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::ApplyPower(Target::Source, 8, Amount::fixed(-2, -2)),
            Effect::Attack(Target::Player, Amount::ascended(3, 4), 3),
        ],
    },
    MoveDef {
        intent: "Attack 7",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(7, 8), 1)],
    },
    MoveDef {
        intent: "Buff",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(Target::Source, 8, Amount::fixed(2, 2))],
    },
];
const WIKI_TORCH_HEAD_AMALGAM_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 18",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(18, 19), 1)],
    },
    MoveDef {
        intent: "Attack 18",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(18, 19), 1)],
    },
    MoveDef {
        intent: "Attack 8x3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::fixed(8, 8), 3)],
    },
    MoveDef {
        intent: "Attack 14",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 15), 1)],
    },
    MoveDef {
        intent: "Attack 14",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 15), 1)],
    },
];
const WIKI_TOUGH_EGG_MINION_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Summon",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 4",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 5), 1)],
    },
];
const OVICOPTER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Summon",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[],
    },
    MoveDef {
        intent: "Buff",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::STRENGTH,
            Amount::ascended(3, 4),
        )],
    },
    MoveDef {
        intent: "Attack 17",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(16, 17), 1)],
    },
    MoveDef {
        intent: "Attack 8, Vulnerable 2",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1],
        effects: effects![
            Effect::Attack(Target::Player, Amount::ascended(7, 8), 1),
            Effect::ApplyDebuff(Target::Player, power_id::VULNERABLE, Amount::fixed(2, 2)),
        ],
    },
];
const WIKI_TUNNELER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 15",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(13, 15), 1)],
    },
    MoveDef {
        intent: "Buff",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::ApplyPower(Target::Source, 170, Amount::fixed(1, 1)),
            Effect::RawBlock(Target::Source, Amount::tough(32, 37)),
        ],
    },
    MoveDef {
        intent: "Attack 26",
        weight: 1,
        max_repeats: u8::MAX,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(23, 26), 1)],
    },
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[],
    },
];
const WIKI_TURRET_OPERATOR_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 3x5",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(3, 4), 5)],
    },
    MoveDef {
        intent: "Attack 3x5",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(3, 4), 5)],
    },
    MoveDef {
        intent: "Strength 1",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(1, 1))],
    },
];
const WIKI_TWIG_SLIME_M_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 11",
        weight: 2,
        max_repeats: 2,
        next: &[0, 1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(11, 12), 1)],
    },
    MoveDef {
        intent: "StatusCard",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1],
        effects: &[Effect::AddCard(Pile::Discard, 3, 1)],
    },
];
const WIKI_TWIG_SLIME_S_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Attack 4",
    weight: 1,
    max_repeats: 1,
    next: &[0],
    effects: &[Effect::Attack(Target::Player, Amount::ascended(4, 5), 1)],
}];
const WIKI_TWO_TAILED_RAT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 8",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1, 2, 3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(8, 9), 1)],
    },
    MoveDef {
        intent: "Attack 6",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1, 2, 3],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(6, 7), 1)],
    },
    MoveDef {
        intent: "Frail 1",
        weight: 3,
        max_repeats: 1,
        next: &[0, 1, 2, 3],
        effects: &[Effect::ApplyPower(
            Target::Player,
            power_id::FRAIL,
            Amount::fixed(1, 1),
        )],
    },
    MoveDef {
        intent: "Summon",
        weight: 9,
        max_repeats: 1,
        next: &[0, 1, 2, 3],
        effects: &[],
    },
];
const WIKI_VANTOM_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 8",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(7, 8), 1)],
    },
    MoveDef {
        intent: "Attack 7x2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(6, 7), 2)],
    },
    MoveDef {
        intent: "Attack 30, Wound 3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(26, 30), 1),
            Effect::AddCard(Pile::Discard, card_id::WOUND, 3),
        ],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::ApplyPower(Target::Source, 0, Amount::fixed(2, 2))],
    },
];
const WIKI_VINE_SHAMBLER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 9, Tangled 1",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Player, power_id::TANGLED, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 6x2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(6, 7), 2)],
    },
    MoveDef {
        intent: "Attack 16",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(16, 18), 1)],
    },
];
const WIKI_WATERFALL_GIANT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Steam 15",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[Effect::ApplyPower(
            Target::Source,
            power_id::STEAM_ERUPTION,
            Amount::ascended(15, 20),
        )],
    },
    MoveDef {
        intent: "Attack 15, Weak 1, Steam 3",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(15, 16), 1),
            Effect::ApplyDebuff(Target::Player, power_id::WEAK, Amount::fixed(1, 1)),
            Effect::ApplyPower(
                Target::Source,
                power_id::STEAM_ERUPTION,
                Amount::fixed(3, 3),
            ),
        ],
    },
    MoveDef {
        intent: "Attack 10, Steam 3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(10, 11), 1),
            Effect::ApplyPower(
                Target::Source,
                power_id::STEAM_ERUPTION,
                Amount::fixed(3, 3),
            ),
        ],
    },
    MoveDef {
        intent: "Heal 10, Steam 3",
        weight: 1,
        max_repeats: 1,
        next: &[4],
        effects: &[
            Effect::Heal(Target::Source, Amount::tough(10, 15)),
            Effect::ApplyPower(
                Target::Source,
                power_id::STEAM_ERUPTION,
                Amount::fixed(3, 3),
            ),
        ],
    },
    MoveDef {
        intent: "Attack Pressure, Steam 3",
        weight: 1,
        max_repeats: 1,
        next: &[5],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 13, Steam 3",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::Attack(Target::Player, Amount::ascended(13, 14), 1),
            Effect::ApplyPower(
                Target::Source,
                power_id::STEAM_ERUPTION,
                Amount::fixed(3, 3),
            ),
        ],
    },
    MoveDef {
        intent: "DeathBlow",
        weight: 1,
        max_repeats: u8::MAX,
        next: &[6],
        effects: &[],
    },
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[6],
        effects: &[],
    },
];
const WIKI_WRIGGLER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Stun",
        weight: 1,
        max_repeats: 1,
        next: &[],
        effects: &[],
    },
    MoveDef {
        intent: "Attack 7",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: &[Effect::Attack(Target::Player, Amount::ascended(6, 7), 1)],
    },
    MoveDef {
        intent: "Strength 2, Infection",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: &[
            Effect::ApplyPower(Target::Source, power_id::STRENGTH, Amount::fixed(2, 2)),
            Effect::AddCard(Pile::Discard, 519, 1),
        ],
    },
];
const WIKI_ZAPBOT_MINION_MOVES: &[MoveDef] = &[MoveDef {
    intent: "Attack 14",
    weight: 1,
    max_repeats: 1,
    next: &[0],
    effects: &[Effect::Attack(Target::Player, Amount::ascended(14, 15), 1)],
}];
const LEAF_SLIME_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 4",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: effects![Effect::Attack(Target::Player, Amount::ascended(3, 4), 1)],
    },
    MoveDef {
        intent: "Slimed",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: effects![Effect::AddCard(Pile::Discard, 3, 1)],
    },
];
const LEAF_SLIME_M_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Slimed 2",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: effects![Effect::AddCard(Pile::Discard, 3, 2)],
    },
    MoveDef {
        intent: "Attack 9",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: effects![Effect::Attack(Target::Player, Amount::ascended(8, 9), 1)],
    },
];
const KIN_FOLLOWER_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 5",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: effects![Effect::Attack(Target::Player, Amount::fixed(5, 5), 1)],
    },
    MoveDef {
        intent: "Attack 2x2",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: effects![Effect::Attack(Target::Player, Amount::fixed(2, 2), 2)],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: effects![Effect::ApplyPower(
            Target::Source,
            0,
            Amount::ascended(2, 3)
        )],
    },
];
const KIN_PRIEST_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 9, Frail 1",
        weight: 1,
        max_repeats: 1,
        next: &[1],
        effects: effects![
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Player, 4, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 9, Weak 1",
        weight: 1,
        max_repeats: 1,
        next: &[2],
        effects: effects![
            Effect::Attack(Target::Player, Amount::ascended(8, 9), 1),
            Effect::ApplyPower(Target::Player, 2, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Attack 3x3",
        weight: 1,
        max_repeats: 1,
        next: &[3],
        effects: effects![Effect::Attack(Target::Player, Amount::fixed(3, 3), 3)],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 1,
        max_repeats: 1,
        next: &[0],
        effects: effects![Effect::ApplyPower(
            Target::Source,
            0,
            Amount::ascended(2, 3)
        )],
    },
];
const FAKE_MERCHANT_MOVES: &[MoveDef] = &[
    MoveDef {
        intent: "Attack 13",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1, 2, 3],
        effects: effects![Effect::Attack(Target::Player, Amount::ascended(13, 15), 1)],
    },
    MoveDef {
        intent: "Attack 2x8",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1, 2, 3],
        effects: effects![Effect::Attack(Target::Player, Amount::fixed(2, 2), 8)],
    },
    MoveDef {
        intent: "Attack 9, Frail 1",
        weight: 1,
        max_repeats: 1,
        next: &[0, 1, 2],
        effects: effects![
            Effect::Attack(Target::Player, Amount::ascended(9, 10), 1),
            Effect::ApplyPower(Target::Player, 4, Amount::fixed(1, 1)),
        ],
    },
    MoveDef {
        intent: "Strength 2",
        weight: 3,
        max_repeats: 1,
        next: &[0, 1, 2, 3],
        effects: effects![Effect::ApplyPower(Target::Source, 0, Amount::fixed(2, 2),)],
    },
];
const NO_POWERS: &[(Id, i16)] = &[];
const MINION_POWER: &[(Id, i16)] = &[(16, 1)];
const LEAF_SLIME_ENCOUNTER: &[Id] = &[107, 106, 0];
const FOUNDATION_ENCOUNTERS: &[Id] = &[0];
const WIKI_EVENT_0_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::Heal(10)],
    },
];
const WIKI_EVENT_1_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::RemoveCards(2, STRIKE_TAG),
            RunEffect::AddCard("CARD.ULTIMATE_STRIKE", 1),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::RemoveCards(2, DEFEND_TAG),
            RunEffect::AddCard("CARD.ULTIMATE_DEFEND", 1),
        ],
    },
];
const WIKI_EVENT_2_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::TransformCards(None, 1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::UpgradeCards(1)],
    },
];
const WIKI_EVENT_3_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::UpgradeCards(1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
];
const WIKI_EVENT_4_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::LoseHp(5), RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_5_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddCard("CARD.EXTERMINATE", 1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddCard("CARD.SQUASH", 1)],
    },
];
const WIKI_EVENT_6_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::MaxHp(7)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddCard("CARD.BYRDONIS_EGG", 1)],
    },
];
const WIKI_EVENT_7_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(2)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(3)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(4)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(5)],
    },
];
const WIKI_EVENT_8_PAGE_2: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::Gold(135)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::LoseHp(7),
            RunEffect::AddRelic("RELIC.POLLINOUS_CORE"),
        ],
    },
];
const WIKI_EVENT_8_PAGE_1: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::Gold(75)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::LoseHp(6),
            RunEffect::Options(WIKI_EVENT_8_PAGE_2),
        ],
    },
];
const WIKI_EVENT_8_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::Gold(35)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::LoseHp(5),
            RunEffect::Options(WIKI_EVENT_8_PAGE_1),
        ],
    },
];
const WIKI_EVENT_9_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_10_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::LoseHp(8), RunEffect::RandomGold(61, 99)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::LoseHp(11)],
    },
];
const WIKI_EVENT_11_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(2)],
    },
];
const WIKI_EVENT_12_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::UpgradeShuffled(2)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::RemoveCards(1, 0)],
    },
];
const WIKI_EVENT_13_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::PotionRewards("POTION.GLOWWATER_POTION", 1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::MaxHp(-13),
            RunEffect::AddRelic("RELIC.FRESNEL_LENS"),
        ],
    },
];
const WIKI_EVENT_14_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Gold(40),
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_15_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::RemoveCards(2, 0),
            RunEffect::AddCard("CARD.NORMALITY", 1),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EnchantCards(Enchantment::PerfectFit, 1, 1, None)],
    },
];
const WIKI_EVENT_16_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::AddCard("CARD.DECAY", 1),
            RunEffect::EnchantCards(Enchantment::SoulsPower, 1, 1, None),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddRelic("RELIC.FORGOTTEN_SOUL")],
    },
];
const WIKI_EVENT_17_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddRelic("RELIC.BIG_MUSHROOM")],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::LoseHp(15),
            RunEffect::AddRelic("RELIC.FRAGRANT_MUSHROOM"),
        ],
    },
];
const WIKI_EVENT_18_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_19_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_20_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::RemoveCards(2, 0),
            RunEffect::AddCard("CARD.SPORE_MIND", 1),
        ],
    },
    EventOption {
        requirement: Requirement::Gold(100),
        effects: &[RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_21_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::LoseAllGold, RunEffect::TransformCards(None, 2)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::MaxHp(5)],
    },
];
const WIKI_EVENT_22_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::PotionRewards("POTION.FOUL_POTION", 3)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::RandomPotionReward(true)],
    },
];
const WIKI_EVENT_23_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
];
const WIKI_EVENT_24_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(2)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(3)],
    },
];
const WIKI_EVENT_25_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::DowngradeRandom(2), RunEffect::UpgradeRandom(4)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::CloneDeck, RunEffect::AddCard("CARD.BAD_LUCK", 1)],
    },
];
const WIKI_EVENT_26_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(2)],
    },
];
const WIKI_EVENT_27_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::ChooseCommonCards(8, 2)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::LoseHp(14),
            RunEffect::AddRelic("RELIC.CHOSEN_CHEESE"),
        ],
    },
];
const WIKI_EVENT_28_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::Heal(9), RunEffect::UpgradeCards(1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EnchantCards(Enchantment::Sown, 1, 1, None)],
    },
];
const WIKI_EVENT_29_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EnchantCards(
            Enchantment::Sharp,
            2,
            1,
            Some(CardType::Attack),
        )],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EnchantCards(
            Enchantment::Nimble,
            2,
            1,
            Some(CardType::Skill),
        )],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EnchantCards(
            Enchantment::Swift,
            2,
            1,
            Some(CardType::Power),
        )],
    },
];
const WIKI_EVENT_30_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_31_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EnchantCards(Enchantment::Spiral, 1, 1, None)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::HealPercent(33)],
    },
];
const WIKI_EVENT_32_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::Heal(25),
            RunEffect::AddCard("CARD.METAMORPHOSIS", 1),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::UpgradeCards(1), RunEffect::LoseHp(10)],
    },
];
const WIKI_EVENT_33_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::LoseHp(6),
            RunEffect::EnchantCards(Enchantment::Vigorous, 8, 1, Some(CardType::Attack)),
            RunEffect::SkipEventRng(1),
        ],
    },
];
const WIKI_EVENT_34_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::RandomGold(52, 67)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::SkipEventRng(1),
            RunEffect::RandomGold(303, 363),
            RunEffect::AddCard("CARD.GREED", 1),
        ],
    },
];
const WIKI_EVENT_35_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EnchantCards(
            Enchantment::Corrupted,
            1,
            1,
            Some(CardType::Attack),
        )],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::TransformCards(None, 1)],
    },
];
const WIKI_EVENT_36_PAGE_4: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::MaxHpTo(1), RunEffect::UpgradeAll],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
];
const WIKI_EVENT_36_PAGE_3: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::MaxHp(-24),
            RunEffect::UpgradeRandom(1),
            RunEffect::Options(WIKI_EVENT_36_PAGE_4),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
];
const WIKI_EVENT_36_PAGE_2: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::MaxHp(-12),
            RunEffect::UpgradeRandom(1),
            RunEffect::Options(WIKI_EVENT_36_PAGE_3),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
];
const WIKI_EVENT_36_PAGE_1: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::MaxHp(-6),
            RunEffect::UpgradeRandom(1),
            RunEffect::Options(WIKI_EVENT_36_PAGE_2),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
];
const WIKI_EVENT_36_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::MaxHp(-3),
            RunEffect::UpgradeRandom(1),
            RunEffect::Options(WIKI_EVENT_36_PAGE_1),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::Heal(20)],
    },
];
const WIKI_EVENT_37_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Gold(50),
        effects: &[RunEffect::Gold(-50), RunEffect::AddRelic("RELIC.BONE_TEA")],
    },
    EventOption {
        requirement: Requirement::Gold(150),
        effects: &[
            RunEffect::Gold(-150),
            RunEffect::AddRelic("RELIC.EMBER_TEA"),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddRelic("RELIC.TEA_OF_DISCOURTESY")],
    },
];
const WIKI_EVENT_38_OPTIONS: &[EventOption] = &[EventOption {
    requirement: Requirement::Always,
    effects: &[],
}];
const WIKI_EVENT_39_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(2)],
    },
];
const WIKI_EVENT_40_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::Gold(100)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
];
const WIKI_EVENT_41_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddCard("CARD.SPOILS_MAP", 1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_42_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::AddCard("CARD.DECAY", 1),
            RunEffect::AddRelic("RELIC.LOST_WISP"),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::RandomGold(45, 75)],
    },
];
const WIKI_EVENT_43_OPTIONS: &[EventOption] = &[EventOption {
    requirement: Requirement::Always,
    effects: &[],
}];
const WIKI_EVENT_44_FIGHT: &[EventOption] = &[EventOption {
    requirement: Requirement::Always,
    effects: &[RunEffect::LoseHp(11), RunEffect::NextRelics(1)],
}];
const WIKI_EVENT_44_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::AddRelic("RELIC.ROYAL_POISON"),
            RunEffect::FullHeal,
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::Options(WIKI_EVENT_44_FIGHT)],
    },
];
const WIKI_EVENT_45_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::AddRelic("RELIC.SWORD_OF_STONE")],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::RandomGold(101, 121), RunEffect::LoseHp(7)],
    },
];
const WIKI_EVENT_46_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(1)],
    },
];
const WIKI_EVENT_47_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::LoseHp(6), RunEffect::RandomGold(41, 68)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::NextRelics(1),
            RunEffect::AddCard("CARD.CLUMSY", 1),
        ],
    },
];
const WIKI_EVENT_48_OPTIONS: &[EventOption] = &[EventOption {
    requirement: Requirement::Always,
    effects: &[],
}];
const WIKI_EVENT_49_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::LoseHp(8),
            RunEffect::RandomRelic(&[
                "RELIC.DARKSTONE_PERIAPT",
                "RELIC.DREAM_CATCHER",
                "RELIC.HAND_DRILL",
                "RELIC.MAW_BANK",
                "RELIC.THE_BOOT",
            ]),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::Gold(100),
            RunEffect::RandomCard(&[
                "CARD.CALTROPS",
                "CARD.CLASH",
                "CARD.DISTRACTION",
                "CARD.DUAL_WIELD",
                "CARD.ENTRENCH",
                "CARD.HELLO_WORLD",
                "CARD.OUTMANEUVER",
                "CARD.REBOUND",
                "CARD.RIP_AND_TEAR",
                "CARD.STACK",
            ]),
        ],
    },
];
const WIKI_EVENT_50_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::FullHeal,
            RunEffect::AddCard("CARD.POOR_SLEEP", 1),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::MaxHp(-8), RunEffect::NextRelics(1)],
    },
];
const WIKI_EVENT_51_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[],
    },
];
const WIKI_EVENT_52_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::MaxHp(6)],
    },
    EventOption {
        requirement: Requirement::Gold(55),
        effects: &[
            RunEffect::Gold(-55),
            RunEffect::EnchantCards(Enchantment::Steady, 1, 1, None),
        ],
    },
    EventOption {
        requirement: Requirement::Gold(99),
        effects: &[
            RunEffect::Gold(-99),
            RunEffect::EnchantCards(Enchantment::Steady, 1, 2, None),
        ],
    },
];
const WIKI_EVENT_53_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Gold(100),
        effects: &[RunEffect::Gold(-100), RunEffect::RelicOfRarity(0)],
    },
    EventOption {
        requirement: Requirement::Gold(200),
        effects: &[RunEffect::Gold(-200), RunEffect::EventRelic],
    },
    EventOption {
        requirement: Requirement::Gold(300),
        effects: &[
            RunEffect::Gold(-300),
            RunEffect::AddRelic("RELIC.WONGOS_MYSTERY_TICKET"),
        ],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::DowngradeRandom(1)],
    },
];
const WIKI_EVENT_54_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[
            RunEffect::RemoveCards(1, 0),
            RunEffect::AddCard("CARD.GUILTY", 1),
        ],
    },
];
const WIKI_EVENT_55_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EventAction(0)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::TransformCards(None, 1), RunEffect::LoseHp(9)],
    },
];
const WIKI_EVENT_56_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::TransformCards(Some("CARD.PECK"), 1)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::EnchantCards(Enchantment::Slither, 1, 1, None)],
    },
    EventOption {
        requirement: Requirement::Always,
        effects: &[RunEffect::TransformCards(Some("CARD.TORIC_TOUGHNESS"), 1)],
    },
];
const WIKI_EVENT_57_OPTIONS: &[EventOption] = &[
    EventOption {
        requirement: Requirement::Gold(50),
        effects: &[
            RunEffect::Gold(-50),
            RunEffect::AddCard("CARD.ENLIGHTENMENT", 2),
        ],
    },
    EventOption {
        requirement: Requirement::Gold(125),
        effects: &[RunEffect::RemoveCards(1, 0), RunEffect::Gold(-125)],
    },
    EventOption {
        requirement: Requirement::Gold(250),
        effects: &[RunEffect::RemoveCards(2, 0), RunEffect::Gold(-250)],
    },
];
const THE_GLORY_EVENTS: &[Id] = &[
    3, 16, 17, 25, 44, 46, 48, 4, 9, 11, 43, 22, 24, 26, 27, 29, 30, 33, 35, 37, 39, 41, 47, 51, 53,
];
const THE_HIVE_EVENTS: &[Id] = &[
    1, 5, 7, 8, 15, 18, 42, 32, 40, 57, 4, 9, 11, 43, 22, 24, 26, 27, 29, 30, 33, 35, 37, 39, 41,
    47, 51, 53,
];
const THE_OVERGROWTH_EVENTS: &[Id] = &[
    2, 6, 10, 19, 20, 21, 28, 45, 36, 50, 54, 55, 56, 4, 9, 11, 43, 22, 24, 26, 27, 29, 30, 33, 35,
    37, 39, 41, 47, 51, 53,
];
const THE_UNDERDOCKS_EVENTS: &[Id] = &[
    0, 13, 14, 23, 31, 45, 34, 12, 49, 52, 4, 9, 11, 43, 22, 24, 26, 27, 29, 30, 33, 35, 37, 39,
    41, 47, 51, 53,
];
const WIKI_ENCOUNTER_1: &[Id] = &[5, 6, 7];
const BATTLE_DUMMY_1: &[Id] = &[5];
const BATTLE_DUMMY_2: &[Id] = &[6];
const BATTLE_DUMMY_3: &[Id] = &[7];
const WIKI_ENCOUNTER_2: &[Id] = &[122];
const WIKI_ENCOUNTER_3: &[Id] = &[63];
const WIKI_ENCOUNTER_4: &[Id] = &[95];
const WIKI_ENCOUNTER_5: &[Id] = &[119];
const WIKI_ENCOUNTER_6: &[Id] = &[112];
const WIKI_ENCOUNTER_7: &[Id] = &[38];
const WIKI_ENCOUNTER_10: &[Id] = &[23];
const WIKI_ENCOUNTER_11: &[Id] = &[29];
const WIKI_ENCOUNTER_12: &[Id] = &[35];
const WIKI_ENCOUNTER_13: &[Id] = &[31, 84, 51];
const WIKI_ENCOUNTER_14: &[Id] = &[98, 96];
const WIKI_ENCOUNTER_15: &[Id] = &[71, 71, 71, 71];
const WIKI_ENCOUNTER_16: &[Id] = &[53];
const WIKI_ENCOUNTER_17: &[Id] = &[58];
const WIKI_ENCOUNTER_18: &[Id] = &[102, 64];
const WIKI_ENCOUNTER_19: &[Id] = &[71, 71, 71];
const WIKI_ENCOUNTER_20: &[Id] = &[76];
const WIKI_ENCOUNTER_21: &[Id] = &[83];
const WIKI_ENCOUNTER_22: &[Id] = &[121];
const WIKI_ENCOUNTER_23: &[Id] = &[24, 25];
const WIKI_ENCOUNTER_24: &[Id] = &[49, 105];
const WIKI_ENCOUNTER_25: &[Id] = &[17, 17];
const WIKI_ENCOUNTER_26: &[Id] = &[8, 9, 10, 11];
const WIKI_ENCOUNTER_27: &[Id] = &[10, 9];
const WIKI_ENCOUNTER_28: &[Id] = &[26];
const WIKI_ENCOUNTER_29: &[Id] = &[27, 27, 27];
const WIKI_ENCOUNTER_30: &[Id] = &[43];
const WIKI_ENCOUNTER_31: &[Id] = &[44];
const WIKI_ENCOUNTER_32: &[Id] = &[19, 65];
const WIKI_ENCOUNTER_33: &[Id] = &[46];
const WIKI_ENCOUNTER_34: &[Id] = &[50];
const WIKI_ENCOUNTER_35: &[Id] = &[27, 27, 27, 27];
const WIKI_ENCOUNTER_36: &[Id] = &[54, 54];
const WIKI_ENCOUNTER_38: &[Id] = &[10, 11, 79];
const WIKI_ENCOUNTER_39: &[Id] = &[85];
const WIKI_ENCOUNTER_40: &[Id] = &[115, 116, 117];
const WIKI_ENCOUNTER_41: &[Id] = &[97];
const WIKI_ENCOUNTER_42: &[Id] = &[99];
const WIKI_ENCOUNTER_43: &[Id] = &[100];
const WIKI_ENCOUNTER_44: &[Id] = &[104];
const WIKI_ENCOUNTER_45: &[Id] = &[8, 10, 11, 104];
const WIKI_ENCOUNTER_46: &[Id] = &[55];
const WIKI_ENCOUNTER_47: &[Id] = &[55, 55];
const WIKI_ENCOUNTER_48: &[Id] = &[12];
const WIKI_ENCOUNTER_49: &[Id] = &[13];
const WIKI_ENCOUNTER_50: &[Id] = &[16];
const WIKI_ENCOUNTER_51: &[Id] = &[20];
const WIKI_ENCOUNTER_53: &[Id] = &[36];
const WIKI_ENCOUNTER_54: &[Id] = &[107, 106, 0];
const WIKI_ENCOUNTER_55: &[Id] = &[45, 45, 45];
const WIKI_ENCOUNTER_56: &[Id] = &[52];
const WIKI_ENCOUNTER_58: &[Id] = &[80, 32];
const WIKI_ENCOUNTER_59: &[Id] = &[62];
const WIKI_ENCOUNTER_60: &[Id] = &[67, 68, 69, 70];
const WIKI_ENCOUNTER_61: &[Id] = &[74];
const WIKI_ENCOUNTER_63: &[Id] = &[0, 0, 77];
const WIKI_ENCOUNTER_64: &[Id] = &[106, 1, 0, 107];
const WIKI_ENCOUNTER_65: &[Id] = &[2, 2, 3];
const WIKI_ENCOUNTER_66: &[Id] = &[109];
const WIKI_ENCOUNTER_67: &[Id] = &[110];
const WIKI_ENCOUNTER_68: &[Id] = &[18, 18];
const WIKI_ENCOUNTER_69: &[Id] = &[15, 21];
const WIKI_ENCOUNTER_70: &[Id] = &[37, 48];
const WIKI_ENCOUNTER_71: &[Id] = &[34];
const WIKI_ENCOUNTER_72: &[Id] = &[42];
const WIKI_ENCOUNTER_73: &[Id] = &[47];
const WIKI_ENCOUNTER_74: &[Id] = &[18, 18, 18];
const WIKI_ENCOUNTER_75: &[Id] = &[61, 61, 61, 61];
const WIKI_ENCOUNTER_76: &[Id] = &[63];
const WIKI_ENCOUNTER_77: &[Id] = &[72];
const WIKI_ENCOUNTER_78: &[Id] = &[73];
const WIKI_ENCOUNTER_79: &[Id] = &[75];
const WIKI_ENCOUNTER_80: &[Id] = &[78];
const WIKI_ENCOUNTER_81: &[Id] = &[82];
const WIKI_ENCOUNTER_82: &[Id] = &[90];
const WIKI_ENCOUNTER_83: &[Id] = &[101, 101];
const WIKI_ENCOUNTER_85: &[Id] = &[108, 108, 108];
const WIKI_ENCOUNTER_87: &[Id] = &[111];
const LIVING_FOG_ENCOUNTER: &[Id] = &[48];
const AXEBOT_ENCOUNTER: &[Id] = &[4];
const CONSTRUCT_MENAGERIE_ENCOUNTER: &[Id] = &[63, 20, 20];
const FOGMOG_ENCOUNTER: &[Id] = &[33];
const OVERGROWTH_CRAWLERS_ENCOUNTER: &[Id] = &[74, 36];
const SNAPPING_JAXFRUIT_ENCOUNTER: &[Id] = &[80, 32];
const GREMLIN_MERC_ENCOUNTER: &[Id] = &[39];
const SEAPUNK_NORMAL_ENCOUNTER: &[Id] = &[15, 72];
const OVICOPTER_ENCOUNTER: &[Id] = &[120];
const THE_GLORY_ENCOUNTERS: &[Id] = &[11, 12, 13, 14, 15, 10, 20, 18, 22, 23, 17, 27];
const THE_GLORY_ELITES: &[Id] = &[16, 19, 24];
const THE_GLORY_BOSSES: &[Id] = &[91, 21, 25];
const THE_HIVE_ENCOUNTERS: &[Id] = &[29, 30, 28, 38, 32, 33, 37, 39, 40, 41, 42, 45, 46, 47];
const THE_HIVE_ELITES: &[Id] = &[43, 31, 34];
const THE_HIVE_BOSSES: &[Id] = &[35, 36, 44];
const THE_OVERGROWTH_ENCOUNTERS: &[Id] = &[
    54, 61, 55, 56, 58, 59, 50, 49, 60, 63, 64, 67, 57, 66, 65, 70,
];
const THE_OVERGROWTH_ELITES: &[Id] = &[51, 52, 62];
const THE_OVERGROWTH_BOSSES: &[Id] = &[53, 68, 69];
const THE_UNDERDOCKS_ENCOUNTERS: &[Id] = &[77, 71, 72, 74, 87, 75, 103, 79, 89, 80, 81, 83, 86, 88];
const THE_UNDERDOCKS_ELITES: &[Id] = &[78, 82, 85];
const THE_UNDERDOCKS_BOSSES: &[Id] = &[76, 84, 90];
const FOUNDATION_CARDS: &[Id] = &[
    4,
    5,
    6,
    7,
    8,
    9,
    10,
    11,
    14,
    15,
    16,
    17,
    18,
    19,
    20,
    21,
    22,
    24,
    25,
    26,
    27,
    28,
    29,
    30,
    31,
    32,
    33,
    34,
    35,
    36,
    37,
    38,
    39,
    40,
    41,
    42,
    43,
    44,
    45,
    46,
    67,
    68,
    69,
    70,
    71,
    72,
    73,
    74,
    76,
    77,
    78,
    122,
    123,
    124,
    125,
    126,
    127,
    442,
    443,
    444,
    445,
    446,
    447,
    448,
    449,
    450,
    451,
    452,
    453,
    454,
    455,
    456,
    457,
    458,
    459,
    460,
    461,
    462,
    464,
    465,
    466,
    card_id::CORRUPTION,
];
const FOUNDATION_RELICS: &[Id] = &[
    19, 21, 1, 24, 12, 2, 26, 28, 29, 13, 40, 44, 45, 14, 48, 51, 52, 53, 54, 55, 57, 61, 69, 72,
    73, 82, 84, 89, 93, 94, 95, 96, 99, 102, 103, 105, 108, 109, 111, 112, 117, 118, 119, 120, 121,
    3, 123, 124, 128, 129, 130, 135, 137, 140, 142, 143, 144, 15, 147, 148, 149, 151, 154, 158, 5,
    161, 163, 164, 165, 177, 180, 181, 182, 183, 184, 185, 188, 189, 192, 194, 200, 202, 203, 16,
    205, 207, 209, 210, 212, 217, 223, 224, 227, 232, 234, 235, 238, 239, 240, 245, 247, 250, 252,
    255, 258, 259, 261, 263, 4, 264, 266, 268, 271, 272, 274, 275, 276,
];
pub(crate) const COMMON_RELICS: &[Id] = &[
    1, 2, 3, 4, 5, 8, 12, 13, 14, 16, 21, 38, 40, 54, 83, 84, 102, 105, 118, 140, 183, 192, 204,
    205, 230, 238, 239, 266, 271, 272,
];
pub(crate) const UNCOMMON_RELICS: &[Id] = &[
    9, 11, 15, 19, 41, 44, 51, 82, 90, 92, 103, 108, 117, 121, 123, 129, 135, 147, 158, 163, 164,
    177, 179, 180, 181, 182, 184, 185, 188, 206, 207, 210, 221, 232, 235, 249, 250, 259, 260, 264,
];
pub(crate) const RARE_RELICS: &[Id] = &[
    24, 26, 28, 30, 42, 55, 61, 67, 80, 89, 93, 94, 96, 106, 109, 112, 114, 120, 130, 136, 137,
    142, 145, 146, 149, 151, 161, 162, 178, 189, 193, 194, 202, 203, 213, 223, 224, 234, 240, 247,
    254, 255, 258, 261, 263, 268, 274, 275, 279,
];
pub(crate) const SHOP_RELICS: &[Id] = &[
    10, 29, 45, 47, 48, 53, 57, 69, 73, 95, 99, 119, 124, 144, 148, 154, 157, 165, 200, 209, 212,
    217, 227, 245, 252, 262, 269, 276,
];
const FOUNDATION_POTIONS: &[Id] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44,
];
const IRONCLAD_POTIONS: &[Id] = &[45, 46, 47];
const DEFECT_POTIONS: &[Id] = &[48, 49, 50];
const SILENT_POTIONS: &[Id] = &[51, 52, 53];
const REGENT_POTIONS: &[Id] = &[54, 55, 56];
const NECROBINDER_POTIONS: &[Id] = &[57, 58, 59];
pub(crate) const UNCOMMON_POTIONS: &[Id] = &[
    9, 12, 14, 16, 19, 23, 25, 26, 28, 29, 34, 36, 37, 43, 44, 47, 50, 53, 56, 59,
];
pub(crate) const RARE_POTIONS: &[Id] = &[
    11, 13, 17, 18, 20, 21, 24, 27, 30, 31, 32, 33, 38, 39, 41, 46, 49, 52, 55, 58,
];
pub(crate) const ANYTIME_POTIONS: &[Id] = &[20, 24, 45];
pub(crate) const AUTOMATIC_POTIONS: &[Id] = &[21];
pub(crate) const NO_COMBAT_POTIONS: &[Id] = &[21, 24, 37];
const COLORLESS_CARDS: &[&str] = &[
    "CARD.ALCHEMIZE",
    "CARD.ANOINTED",
    "CARD.AUTOMATION",
    "CARD.BEACON_OF_HOPE",
    "CARD.BEAT_DOWN",
    "CARD.BELIEVE_IN_YOU",
    "CARD.BOLAS",
    "CARD.CALAMITY",
    "CARD.CATASTROPHE",
    "CARD.COORDINATE",
    "CARD.DARK_SHACKLES",
    "CARD.DISCOVERY",
    "CARD.DRAMATIC_ENTRANCE",
    "CARD.ENTROPY",
    "CARD.EQUILIBRIUM",
    "CARD.ETERNAL_ARMOR",
    "CARD.FASTEN",
    "CARD.FINESSE",
    "CARD.FISTICUFFS",
    "CARD.FLASH_OF_STEEL",
    "CARD.GANG_UP",
    "CARD.GOLD_AXE",
    "CARD.HAND_OF_GREED",
    "CARD.HIDDEN_GEM",
    "CARD.HUDDLE_UP",
    "CARD.IMPATIENCE",
    "CARD.INTERCEPT",
    "CARD.JACK_OF_ALL_TRADES",
    "CARD.JACKPOT",
    "CARD.KNOCKDOWN",
    "CARD.LIFT",
    "CARD.MASTER_OF_STRATEGY",
    "CARD.MAYHEM",
    "CARD.MIMIC",
    "CARD.MIND_BLAST",
    "CARD.NOSTALGIA",
    "CARD.OMNISLICE",
    "CARD.PANACHE",
    "CARD.PANIC_BUTTON",
    "CARD.PREP_TIME",
    "CARD.PRODUCTION",
    "CARD.PROLONG",
    "CARD.PROWESS",
    "CARD.PURITY",
    "CARD.RALLY",
    "CARD.REND",
    "CARD.RESTLESSNESS",
    "CARD.ROLLING_BOULDER",
    "CARD.SALVO",
    "CARD.SCRAWL",
    "CARD.SECRET_TECHNIQUE",
    "CARD.SECRET_WEAPON",
    "CARD.SEEKER_STRIKE",
    "CARD.SHOCKWAVE",
    "CARD.SPLASH",
    "CARD.STRATAGEM",
    "CARD.TAG_TEAM",
    "CARD.THE_BOMB",
    "CARD.THE_GAMBIT",
    "CARD.THINKING_AHEAD",
    "CARD.THRUMMING_HATCHET",
    "CARD.ULTIMATE_DEFEND",
    "CARD.ULTIMATE_STRIKE",
    "CARD.VOLLEY",
];
const STARTER_RELICS: &[Id] = &[0];
const DEFECT_RELICS: &[Id] = &[6];
const IRONCLAD_RELIC_POOL: &[Id] = &[47, 56, 67, 179, 204, 213, 221];
const DEFECT_RELIC_POOL: &[Id] = &[8, 80, 9, 145, 193, 10, 11];
const SILENT_RELIC_POOL: &[Id] = &[106, 157, 178, 230, 249, 254, 260];
const REGENT_RELIC_POOL: &[Id] = &[83, 92, 136, 146, 162, 206, 269];
const NECROBINDER_RELIC_POOL: &[Id] = &[30, 38, 41, 42, 90, 114, 262];
const SILENT_RELICS: &[Id] = &[7];
const REGENT_RELICS: &[Id] = &[17];
const NECROBINDER_RELICS: &[Id] = &[18];
const NOTHING: &[Id] = &[];
const IRONCLAD_DECK: &[(Id, u8)] = &[
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (1, 0),
    (1, 0),
    (1, 0),
    (1, 0),
    (2, 0),
];
const DEFECT_CARDS: &[Id] = &[
    48, 49, 51, 52, 53, 54, 55, 85, 86, 87, 91, 97, 103, 112, 113, 115, 145, 146, 147, 148, 149,
    150, 151, 152, 153, 154, 155, 156, 157, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174,
    175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193,
    194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 467,
    468, 469, 470, 471,
];
const DEFECT_DECK: &[(Id, u8)] = &[
    (79, 0),
    (79, 0),
    (79, 0),
    (79, 0),
    (80, 0),
    (80, 0),
    (80, 0),
    (80, 0),
    (47, 0),
    (50, 0),
];
const SILENT_CARDS: &[Id] = &[
    92, 93, 94, 95, 102, 104, 116, 119, 120, 132, 133, 134, 135, 136, 138, 139, 140, 141, 142, 143,
    144, 158, 159, 160, 161, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225,
    227, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246,
    247, 248, 249, 250, 251, 252, 253, 254, 255, 256, 257, 258, 259, 260, 261, 262, 263, 264, 265,
    266, 267, 268, 269, 270,
];
const SILENT_DECK: &[(Id, u8)] = &[
    (128, 0),
    (128, 0),
    (128, 0),
    (128, 0),
    (128, 0),
    (129, 0),
    (129, 0),
    (129, 0),
    (129, 0),
    (129, 0),
    (130, 0),
    (131, 0),
];
const REGENT_CARDS: &[Id] = &[
    108, 109, 111, 273, 274, 275, 276, 277, 278, 281, 282, 284, 285, 286, 288, 289, 290, 291, 292,
    293, 294, 295, 296, 297, 298, 299, 300, 301, 302, 303, 304, 305, 307, 308, 309, 310, 311, 312,
    313, 314, 315, 316, 318, 319, 320, 321, 322, 323, 324, 325, 326, 327, 328, 329, 330, 331, 332,
    333, 334, 335, 336, 337, 338, 339, 340, 341, 342, 343, 344, 346, 347, 348, 349, 350, 351, 352,
    353, 354, 355, 356,
];
const REGENT_DECK: &[(Id, u8)] = &[
    (83, 0),
    (83, 0),
    (83, 0),
    (83, 0),
    (84, 0),
    (84, 0),
    (84, 0),
    (84, 0),
    (271, 0),
    (272, 0),
];
const NECROBINDER_DECK: &[(Id, u8)] = &[
    (81, 0),
    (81, 0),
    (81, 0),
    (81, 0),
    (82, 0),
    (82, 0),
    (82, 0),
    (82, 0),
    (357, 0),
    (358, 0),
];
const NECROBINDER_CARDS: &[Id] = &[
    105, 106, 121, 359, 360, 361, 362, 363, 364, 365, 366, 367, 369, 370, 371, 372, 373, 374, 375,
    376, 377, 378, 379, 380, 381, 382, 383, 384, 385, 386, 387, 388, 389, 391, 393, 394, 395, 396,
    397, 398, 400, 401, 402, 403, 404, 405, 406, 407, 408, 409, 410, 412, 413, 414, 415, 416, 417,
    418, 419, 420, 421, 422, 423, 424, 425, 426, 427, 428, 430, 431, 432, 433, 434, 435, 436, 437,
    438, 439, 440, 441,
];

impl CardDef {
    const fn with_rarity(mut self, rarity: CardRarity) -> Self {
        self.rarity = rarity;
        self
    }

    const fn with_hooks(mut self, hooks: &'static [Hook]) -> Self {
        self.hooks = hooks;
        self
    }

    const fn with_tags(mut self, tags: u16) -> Self {
        self.tags = tags;
        self
    }
}

const fn card(
    id: &'static str,
    card_type: CardType,
    cost: [i8; 2],
    target: Target,
    flags: [u16; 2],
    effects: &'static [Effect],
) -> CardDef {
    CardDef {
        id,
        card_type,
        rarity: CardRarity::Common,
        cost,
        star_cost: [-1, -1],
        target,
        flags,
        playable: PlayCondition::Always,
        effects,
        hooks: EMPTY_HOOKS,
        tags: 0,
    }
}

const fn uncommon_card(
    id: &'static str,
    card_type: CardType,
    cost: [i8; 2],
    target: Target,
    flags: [u16; 2],
    effects: &'static [Effect],
) -> CardDef {
    let mut card = card(id, card_type, cost, target, flags, effects);
    card.rarity = CardRarity::Uncommon;
    card
}

const fn rare_card(
    id: &'static str,
    card_type: CardType,
    cost: [i8; 2],
    target: Target,
    flags: [u16; 2],
    effects: &'static [Effect],
) -> CardDef {
    let mut card = card(id, card_type, cost, target, flags, effects);
    card.rarity = CardRarity::Rare;
    card
}

const fn star_card(
    id: &'static str,
    card_type: CardType,
    cost: [i8; 2],
    stars: [i8; 2],
    target: Target,
    effects: &'static [Effect],
) -> CardDef {
    star_card_flags(id, card_type, cost, stars, target, [0, 0], effects)
}

const fn star_card_flags(
    id: &'static str,
    card_type: CardType,
    cost: [i8; 2],
    stars: [i8; 2],
    target: Target,
    flags: [u16; 2],
    effects: &'static [Effect],
) -> CardDef {
    CardDef {
        id,
        card_type,
        rarity: CardRarity::Common,
        cost,
        star_cost: stars,
        target,
        flags,
        playable: PlayCondition::Always,
        effects,
        hooks: EMPTY_HOOKS,
        tags: 0,
    }
}

pub fn foundation_content() -> Content {
    let cards = vec![
        card(
            "CARD.STRIKE_IRONCLAD",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1)],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(STRIKE_TAG),
        card(
            "CARD.DEFEND_IRONCLAD",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(5, 8))],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(DEFEND_TAG),
        card(
            "CARD.BASH",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 10), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(2, 3)),
            ],
        )
        .with_rarity(CardRarity::Basic),
        card(
            "CARD.SLIMED",
            CardType::Status,
            [1, 1],
            Target::Player,
            [EXHAUST | NO_GENERATE, EXHAUST | NO_GENERATE],
            effects![Effect::Draw(1)],
        ),
        card(
            "CARD.IRON_WAVE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(5, 7)),
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 7), 1),
            ],
        ),
        card(
            "CARD.TWIN_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 7), 2)],
        )
        .with_tags(STRIKE_TAG),
        card(
            "CARD.POMMEL_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 10), 1),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(2)],
                    effects![Effect::Draw(1)]
                ),
            ],
        )
        .with_tags(STRIKE_TAG),
        card(
            "CARD.SHRUG_IT_OFF",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(8, 11)),
                Effect::Draw(1),
            ],
        ),
        card(
            "CARD.THUNDERCLAP",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::Attack(Target::AllEnemies, Amount::fixed(4, 7), 1),
                Effect::ApplyPower(Target::AllEnemies, 3, Amount::fixed(1, 1)),
            ],
        ),
        rare_card(
            "CARD.CONFLAGRATION",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::AttackMany(
                Target::AllEnemies,
                Amount::fixed(2, 2),
                Amount::fixed(4, 5),
            )],
        ),
        rare_card(
            "CARD.NOT_YET",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [EXHAUST | NO_GENERATE, EXHAUST | NO_GENERATE],
            effects![Effect::Heal(Target::Player, Amount::fixed(10, 13))],
        ),
        rare_card(
            "CARD.PACTS_END",
            CardType::Attack,
            [0, 0],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::If(
                Condition::ExhaustAtLeast(3),
                &[Effect::Attack(Target::AllEnemies, Amount::fixed(17, 23), 1)],
                &[],
            )],
        ),
        card(
            "CARD.GIANT_ROCK",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(16, 20),
                1,
            )],
        ),
        CardDef {
            id: "CARD.CLASH",
            card_type: CardType::Attack,
            rarity: CardRarity::Common,
            cost: [0, 0],
            star_cost: [-1, -1],
            target: Target::ChosenEnemy,
            flags: [0, 0],
            playable: PlayCondition::AttacksOnly,
            effects: effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(14, 18),
                1,
            )],
            hooks: EMPTY_HOOKS,
            tags: 0,
        },
        card(
            "CARD.ANGER",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 8), 1),
                Effect::CopyCard(Pile::Discard, 1),
            ],
        ),
        uncommon_card(
            "CARD.INFLAME",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 0, Amount::fixed(2, 3))],
        ),
        uncommon_card(
            "CARD.BATTLE_TRANCE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(4)],
                    effects![Effect::Draw(3)]
                ),
                Effect::ApplyPower(Target::Player, 9, Amount::fixed(1, 1)),
            ],
        ),
        uncommon_card(
            "CARD.BURNING_PACT",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Exhaust),
                )][0],
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(3)],
                    effects![Effect::Draw(2)]
                ),
            ],
        ),
        rare_card(
            "CARD.OFFERING",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![
                Effect::LoseHp(Target::Player, Amount::fixed(6, 6)),
                Effect::Energy(2),
                Effect::If(
                    Condition::Upgraded,
                    &[Effect::Draw(5)],
                    effects![Effect::Draw(3)]
                ),
            ],
        ),
        card(
            "CARD.HEADBUTT",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 12), 1),
                Effect::Select(
                    Pile::Discard,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Draw),
                ),
            ],
        ),
        card(
            "CARD.TRUE_GRIT",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(7, 9)),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Select(
                        Pile::Hand,
                        CardFilter::Any,
                        [1, 1],
                        false,
                        false,
                        CardOp::Move(Pile::Exhaust),
                    )],
                    &[Effect::Exhaust(1, true)],
                ),
            ],
        ),
        card(
            "CARD.ARMAMENTS",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(5, 5)),
                Effect::If(
                    Condition::Upgraded,
                    &[Effect::Upgrade(Pile::Hand, u8::MAX, true)],
                    &[Effect::Select(
                        Pile::Hand,
                        CardFilter::Upgradable,
                        [1, 1],
                        false,
                        false,
                        CardOp::Upgrade,
                    )],
                ),
            ],
        ),
        rare_card(
            "CARD.IMPERVIOUS",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::Block(Target::Player, Amount::fixed(30, 40))],
        ),
        uncommon_card(
            "CARD.SHOCKWAVE",
            CardType::Skill,
            [2, 2],
            Target::AllEnemies,
            [EXHAUST, EXHAUST],
            effects![
                Effect::ApplyPower(Target::AllEnemies, 2, Amount::fixed(3, 5)),
                Effect::ApplyPower(Target::AllEnemies, 3, Amount::fixed(3, 5)),
            ],
        ),
        card(
            "CARD.MOLTEN_FIST",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 14), 1),
                Effect::DoublePower(Target::ChosenEnemy, power_id::VULNERABLE, 0),
            ],
        ),
        card(
            "CARD.TREMBLE",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![Effect::ApplyPower(
                Target::ChosenEnemy,
                3,
                Amount::fixed(3, 4),
            )],
        ),
        card(
            "CARD.BODY_SLAM",
            CardType::Attack,
            [1, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::scaled(Scale::Block, 1),
                1,
            )],
        ),
        rare_card(
            "CARD.THRASH",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 4,
                        upgraded: 6,
                        ascension: 0,
                        scale: Scale::CardValue,
                        multiplier: 1,
                        divisor: 1,
                    },
                    2,
                ),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Type(CardType::Attack),
                    [1, 1],
                    true,
                    false,
                    CardOp::Move(Pile::Exhaust),
                ),
            ],
        ),
        card(
            "CARD.BREAK",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(20, 30), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(5, 7)),
            ],
        )
        .with_rarity(CardRarity::Ancient),
        rare_card(
            "CARD.BRAND",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::LoseHp(Target::Player, Amount::fixed(1, 1)),
                effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Exhaust),
                )][0],
                Effect::ApplyPower(Target::Player, 0, Amount::fixed(1, 2)),
            ],
        ),
        uncommon_card(
            "CARD.UPPERCUT",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(13, 13), 1),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(1, 2)),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(1, 2)),
            ],
        ),
        uncommon_card(
            "CARD.BLUDGEON",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(32, 42),
                1,
            )],
        ),
        uncommon_card(
            "CARD.STOMP",
            CardType::Attack,
            [3, 3],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(12, 15), 1)],
        ),
        uncommon_card(
            "CARD.HOWL_FROM_BEYOND",
            CardType::Attack,
            [3, 3],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(16, 21), 1)],
        ),
        uncommon_card(
            "CARD.TAUNT",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(7, 8)),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(1, 2)),
            ],
        ),
        card(
            "CARD.BLOODLETTING",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::LoseHp(Target::Player, Amount::fixed(3, 3)),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Energy(3)],
                    effects![Effect::Energy(2)]
                ),
            ],
        ),
        card(
            "CARD.BLOOD_WALL",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::LoseHp(Target::Player, Amount::fixed(2, 2)),
                Effect::Block(Target::Player, Amount::fixed(16, 20)),
            ],
        ),
        card(
            "CARD.BREAKTHROUGH",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::LoseHp(Target::Player, Amount::fixed(1, 1)),
                Effect::Attack(Target::AllEnemies, Amount::fixed(9, 13), 1),
            ],
        ),
        uncommon_card(
            "CARD.HEMOKINESIS",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::LoseHp(Target::Player, Amount::fixed(2, 2)),
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(15, 20), 1),
            ],
        ),
        card(
            "CARD.CINDER",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(18, 24), 1),
                Effect::RandomCardOp(
                    Pile::Hand,
                    CardFilter::Any,
                    CardOp::Move(Pile::Exhaust),
                    Amount::fixed(1, 1),
                ),
            ],
        ),
        card(
            "CARD.SWORD_BOOMERANG",
            CardType::Attack,
            [1, 1],
            Target::RandomEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::fixed(3, 4),
                &[Effect::Attack(Target::RandomEnemy, Amount::fixed(3, 3), 1)],
            )],
        ),
        uncommon_card(
            "CARD.FEEL_NO_PAIN",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 10, Amount::fixed(3, 4))],
        ),
        rare_card(
            "CARD.DARK_EMBRACE",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 11, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.RUPTURE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 12, Amount::fixed(1, 2))],
        ),
        rare_card(
            "CARD.JUGGERNAUT",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 13, Amount::fixed(6, 8))],
        ),
        rare_card(
            "CARD.DEMON_FORM",
            CardType::Power,
            [3, 3],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 14, Amount::fixed(2, 3))],
        ),
        rare_card(
            "CARD.BARRICADE",
            CardType::Power,
            [3, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 15, Amount::fixed(1, 1))],
        ),
        card(
            "CARD.ZAP",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::Channel(0, 1)],
        )
        .with_rarity(CardRarity::Basic),
        card(
            "CARD.COOLHEADED",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Channel(1, 1),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(2)],
                    effects![Effect::Draw(1)]
                ),
            ],
        ),
        uncommon_card(
            "CARD.DARKNESS",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Channel(2, 1),
                Effect::If(
                    Condition::Upgraded,
                    &[Effect::PassiveLast(2)],
                    &[Effect::PassiveLast(1)],
                ),
            ],
        ),
        card(
            "CARD.DUALCAST",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::Evoke(false), Effect::Evoke(true)],
        )
        .with_rarity(CardRarity::Basic),
        rare_card(
            "CARD.RAINBOW",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [EXHAUST, 0],
            effects![
                Effect::Channel(0, 1),
                Effect::Channel(1, 1),
                Effect::Channel(2, 1),
            ],
        ),
        card(
            "CARD.BALL_LIGHTNING",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(7, 10), 1),
                Effect::Channel(0, 1),
            ],
        ),
        card(
            "CARD.COLD_SNAP",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1),
                Effect::Channel(1, 1),
            ],
        ),
        uncommon_card(
            "CARD.GLACIER",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(6, 9)),
                Effect::Channel(1, 2),
            ],
        ),
        uncommon_card(
            "CARD.FUSION",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::Channel(3, 1)],
        ),
        card(
            "CARD.CLUMSY",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [ETHEREAL | UNPLAYABLE, ETHEREAL | UNPLAYABLE],
            &[],
        ),
        card(
            "CARD.CURSE_OF_THE_BELL",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | ETERNAL, UNPLAYABLE | ETERNAL],
            &[],
        ),
        card(
            "CARD.DAZED",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [ETHEREAL | UNPLAYABLE, ETHEREAL | UNPLAYABLE],
            &[],
        ),
        card(
            "CARD.INJURY",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE, UNPLAYABLE],
            &[],
        ),
        card(
            "CARD.WOUND",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE, UNPLAYABLE],
            &[],
        ),
        card(
            "CARD.WRITHE",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [INNATE | UNPLAYABLE, INNATE | UNPLAYABLE],
            &[],
        ),
        card(
            "CARD.BURN",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE, UNPLAYABLE],
            &[],
        )
        .with_hooks(BURN_HOOKS),
        card(
            "CARD.DOUBT",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE, UNPLAYABLE],
            &[],
        )
        .with_hooks(DOUBT_HOOKS),
        card(
            "CARD.REGRET",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE, UNPLAYABLE],
            &[],
        )
        .with_hooks(REGRET_HOOKS),
        card(
            "CARD.SHAME",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE, UNPLAYABLE],
            &[],
        )
        .with_hooks(SHAME_HOOKS),
        card(
            "CARD.VOID",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [ETHEREAL | UNPLAYABLE, ETHEREAL | UNPLAYABLE],
            &[],
        )
        .with_hooks(VOID_HOOKS),
        uncommon_card(
            "CARD.FORGOTTEN_RITUAL",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST],
            &[Effect::If(
                Condition::Upgraded,
                effects![Effect::If(
                    Condition::ExhaustedThisTurn,
                    &[Effect::Energy(4)],
                    &[],
                )],
                effects![Effect::If(
                    Condition::ExhaustedThisTurn,
                    effects![Effect::Energy(3)],
                    &[]
                )],
            )],
        ),
        rare_card(
            "CARD.TEAR_ASUNDER",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount {
                    base: 1,
                    upgraded: 1,
                    ascension: 0,
                    scale: Scale::HpLossEvents,
                    multiplier: 1,
                    divisor: 1,
                },
                &[Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 7), 1)],
            )],
        ),
        uncommon_card(
            "CARD.ASHEN_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            &[Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 6,
                        upgraded: 6,
                        ascension: 0,
                        scale: Scale::ExhaustSize,
                        multiplier: 4,
                        divisor: 1,
                    },
                    1,
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 6,
                        upgraded: 6,
                        ascension: 0,
                        scale: Scale::ExhaustSize,
                        multiplier: 3,
                        divisor: 1,
                    },
                    1,
                )],
            )],
        )
        .with_tags(STRIKE_TAG),
        uncommon_card(
            "CARD.EVIL_EYE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(8, 11)),
                Effect::If(
                    Condition::ExhaustedThisTurn,
                    &[Effect::Block(Target::Player, Amount::fixed(8, 11))],
                    &[],
                ),
            ],
        ),
        card(
            "CARD.PERFECTED_STRIKE",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            &[Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 6,
                        upgraded: 6,
                        ascension: 0,
                        scale: Scale::Tagged(STRIKE_TAG),
                        multiplier: 3,
                        divisor: 1,
                    },
                    1,
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 6,
                        upgraded: 6,
                        ascension: 0,
                        scale: Scale::Tagged(STRIKE_TAG),
                        multiplier: 2,
                        divisor: 1,
                    },
                    1,
                )],
            )],
        )
        .with_tags(STRIKE_TAG),
        uncommon_card(
            "CARD.DISMANTLE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 10), 1),
                Effect::If(
                    Condition::TargetHasPower(3),
                    &[Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 10), 1)],
                    &[],
                ),
            ],
        ),
        uncommon_card(
            "CARD.BULLY",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            &[Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 4,
                        upgraded: 4,
                        ascension: 0,
                        scale: Scale::TargetPower(3),
                        multiplier: 3,
                        divisor: 1,
                    },
                    1,
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 4,
                        upgraded: 4,
                        ascension: 0,
                        scale: Scale::TargetPower(3),
                        multiplier: 2,
                        divisor: 1,
                    },
                    1,
                )],
            )],
        ),
        rare_card(
            "CARD.FEED",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [EXHAUST | NO_GENERATE, EXHAUST | NO_GENERATE],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 12), 1),
                Effect::If(
                    Condition::TargetDead,
                    &[Effect::MaxHp(Amount::fixed(3, 4))],
                    &[],
                ),
            ],
        ),
        uncommon_card(
            "CARD.DEMONIC_SHIELD",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST | NO_GENERATE, NO_GENERATE],
            effects![
                Effect::LoseHp(Target::Player, Amount::fixed(1, 1)),
                Effect::Block(Target::Player, Amount::scaled(Scale::Block, 1)),
            ],
        ),
        uncommon_card(
            "CARD.RAMPAGE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 9,
                        upgraded: 9,
                        ascension: 0,
                        scale: Scale::CardValue,
                        multiplier: 1,
                        divisor: 1,
                    },
                    1,
                ),
                Effect::GrowCard(Amount::fixed(5, 9)),
            ],
        ),
        uncommon_card(
            "CARD.SPITE",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::HpLostThisTurn,
                &[Effect::Repeat(
                    Amount::fixed(2, 3),
                    effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 5), 1)]
                )],
                effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 5), 1)],
            )],
        ),
        uncommon_card(
            "CARD.PILLAGE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1),
                Effect::DrawUntilNot(CardType::Attack),
            ],
        ),
        card(
            "CARD.STRIKE_DEFECT",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1)],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(STRIKE_TAG),
        card(
            "CARD.DEFEND_DEFECT",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(5, 8))],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(DEFEND_TAG),
        card(
            "CARD.STRIKE_NECROBINDER",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1)],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(STRIKE_TAG),
        card(
            "CARD.DEFEND_NECROBINDER",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(5, 8))],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(DEFEND_TAG),
        card(
            "CARD.STRIKE_REGENT",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1)],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(STRIKE_TAG),
        card(
            "CARD.DEFEND_REGENT",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(5, 8))],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(DEFEND_TAG),
        card(
            "CARD.LEAP",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(9, 12))],
        ),
        card(
            "CARD.BEAM_CELL",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(3, 4), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(1, 2))
            ],
        ),
        card(
            "CARD.SWEEPING_BEAM",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::Attack(Target::AllEnemies, Amount::fixed(6, 9), 1),
                Effect::Draw(1)
            ],
        ),
        uncommon_card(
            "CARD.FINESSE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(4, 7)),
                Effect::Draw(1)
            ],
        ),
        uncommon_card(
            "CARD.DRAMATIC_ENTRANCE",
            CardType::Attack,
            [0, 0],
            Target::AllEnemies,
            [EXHAUST | INNATE, EXHAUST | INNATE],
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(11, 15), 1)],
        ),
        rare_card(
            "CARD.MASTER_OF_STRATEGY",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Draw(4)],
                effects![Effect::Draw(3)]
            )],
        ),
        uncommon_card(
            "CARD.SKIM",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Draw(4)],
                effects![Effect::Draw(3)]
            )],
        ),
        card(
            "CARD.SUCKER_PUNCH",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 10), 1),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(1, 2))
            ],
        ),
        uncommon_card(
            "CARD.LEG_SWEEP",
            CardType::Skill,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(11, 14)),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(2, 3))
            ],
        ),
        uncommon_card(
            "CARD.FOOTWORK",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 1, Amount::fixed(2, 3))],
        ),
        rare_card(
            "CARD.ADRENALINE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Energy(2)],
                    effects![Effect::Energy(1)]
                ),
                Effect::Draw(2)
            ],
        ),
        card(
            "CARD.RIP_AND_TEAR",
            CardType::Attack,
            [1, 1],
            Target::RandomEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::fixed(2, 2),
                effects![Effect::Attack(Target::RandomEnemy, Amount::fixed(7, 9), 1)]
            )],
        ),
        uncommon_card(
            "CARD.BOOT_SEQUENCE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST | INNATE, EXHAUST | INNATE],
            effects![Effect::Block(Target::Player, Amount::fixed(10, 13))],
        ),
        uncommon_card(
            "CARD.ULTIMATE_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(14, 20),
                1
            )],
        )
        .with_tags(STRIKE_TAG),
        card(
            "CARD.PECK",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(2, 2), 3),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(2, 2), 1)],
                    &[]
                )
            ],
        ),
        card(
            "CARD.SQUASH",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 12), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(2, 3))
            ],
        ),
        card(
            "CARD.EXTERMINATE",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(3, 4), 4)],
        ),
        uncommon_card(
            "CARD.SCARE",
            CardType::Skill,
            [0, 0],
            Target::AllEnemies,
            [EXHAUST, 0],
            effects![Effect::ApplyPower(
                Target::AllEnemies,
                2,
                Amount::fixed(1, 1)
            )],
        ),
        rare_card(
            "CARD.SUPERCRITICAL",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Energy(6)],
                effects![Effect::Energy(4)]
            )],
        ),
        uncommon_card(
            "CARD.EXPOSE",
            CardType::Skill,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![Effect::ApplyPower(
                Target::ChosenEnemy,
                3,
                Amount::fixed(2, 3)
            )],
        ),
        card(
            "CARD.WISP",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST | RETAIN],
            effects![Effect::Energy(1)],
        ),
        card(
            "CARD.DEFY",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [ETHEREAL, ETHEREAL],
            effects![
                Effect::Block(Target::Player, Amount::fixed(6, 9)),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(1, 1))
            ],
        ),
        card(
            "CARD.BYRD_SWOOP",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(14, 18),
                1
            )],
        ),
        card(
            "CARD.CELESTIAL_MIGHT",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 6), 3),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 6), 1)],
                    &[]
                )
            ],
        ),
        card(
            "CARD.COSMIC_INDIFFERENCE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(6, 9))],
        ),
        card(
            "CARD.ENTRENCH",
            CardType::Skill,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::RawBlock(
                Target::Player,
                Amount::scaled(Scale::Block, 1)
            )],
        ),
        uncommon_card(
            "CARD.KINGLY_KICK",
            CardType::Attack,
            [4, 4],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(27, 35),
                1
            )],
        ),
        uncommon_card(
            "CARD.ROCKET_PUNCH",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(13, 14), 1),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(2)],
                    effects![Effect::Draw(1)]
                )
            ],
        ),
        uncommon_card(
            "CARD.TESLA_COIL",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(3, 4), 1)],
        ),
        uncommon_card(
            "CARD.THRUMMING_HATCHET",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(11, 14),
                1
            )],
        ),
        card(
            "CARD.UPROAR",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 8), 2)],
        ),
        rare_card(
            "CARD.ASSASSINATE",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST | INNATE, EXHAUST | INNATE],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 13), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(1, 2))
            ],
        ),
        rare_card(
            "CARD.BOLAS",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(3, 4), 1)],
        ),
        card(
            "CARD.CALTROPS",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 8, Amount::fixed(3, 5))],
        ),
        card(
            "CARD.DEADLY_POISON",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::ChosenEnemy,
                7,
                Amount::fixed(5, 7)
            )],
        ),
        card(
            "CARD.POISONED_STAB",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 8), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 7, Amount::fixed(3, 4))
            ],
        ),
        uncommon_card(
            "CARD.PUTREFY",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(2, 3)),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(2, 3))
            ],
        ),
        uncommon_card(
            "CARD.SECOND_WIND",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ExhaustForBlock(Amount::fixed(5, 7))],
        ),
        rare_card(
            "CARD.FIEND_FIRE",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![Effect::ExhaustForAttack(
                Target::ChosenEnemy,
                Amount::fixed(7, 10)
            )],
        ),
        uncommon_card(
            "CARD.FIGHT_ME",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 6), 2),
                Effect::ApplyPower(Target::Player, 0, Amount::fixed(3, 4)),
                Effect::ApplyPower(Target::ChosenEnemy, 0, Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.PRIMAL_FORCE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![Effect::TransformHand(12, [0, 1])],
        ),
        uncommon_card(
            "CARD.WHIRLWIND",
            CardType::Attack,
            [-1, -1],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::X, 1),
                &[Effect::Attack(Target::AllEnemies, Amount::fixed(5, 8), 1)],
            )],
        ),
        uncommon_card(
            "CARD.DRUM_OF_BATTLE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Draw(2)],
        )
        .with_hooks(DRUM_HOOKS),
        card(
            "CARD.STRIKE_SILENT",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1)],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(STRIKE_TAG),
        card(
            "CARD.DEFEND_SILENT",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(5, 8))],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(DEFEND_TAG),
        card(
            "CARD.NEUTRALIZE",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(3, 4), 1),
                Effect::ApplyPower(
                    Target::ChosenEnemyOrDead,
                    power_id::WEAK,
                    Amount::fixed(1, 2)
                )
            ],
        )
        .with_rarity(CardRarity::Basic),
        card(
            "CARD.SURVIVOR",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(8, 11)),
                Effect::Discard(1, false)
            ],
        )
        .with_rarity(CardRarity::Basic),
        card(
            "CARD.BACKFLIP",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(5, 8)),
                Effect::Draw(2)
            ],
        ),
        card(
            "CARD.DAGGER_SPRAY",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(4, 6), 2)],
        ),
        uncommon_card(
            "CARD.DASH",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(10, 13)),
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 13), 1)
            ],
        ),
        card(
            "CARD.DEFLECT",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(4, 7))],
        ),
        uncommon_card(
            "CARD.BACKSTAB",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST | INNATE, EXHAUST | INNATE],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(11, 15),
                1
            )],
        ),
        card(
            "CARD.SHIV",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(4, 6), 1)],
        )
        .with_tags(SHIV_TAG),
        card(
            "CARD.BLADE_DANCE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::AddCard(Pile::Hand, 137, 4)],
                effects![Effect::AddCard(Pile::Hand, 137, 3)]
            )],
        ),
        card(
            "CARD.CLOAK_AND_DAGGER",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(6, 6)),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::AddCard(Pile::Hand, 137, 2)],
                    effects![Effect::AddCard(Pile::Hand, 137, 1)]
                )
            ],
        ),
        uncommon_card(
            "CARD.ACROBATICS",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(4)],
                    effects![Effect::Draw(3)]
                ),
                Effect::Discard(1, false)
            ],
        ),
        card(
            "CARD.DAGGER_THROW",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 12), 1),
                Effect::Draw(1),
                Effect::Discard(1, false)
            ],
        ),
        card(
            "CARD.PREPARED",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Draw(2), Effect::Discard(2, false)],
                effects![Effect::Draw(1), Effect::Discard(1, false)]
            )],
        ),
        card(
            "CARD.SLICE",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1)],
        ),
        uncommon_card(
            "CARD.SKEWER",
            CardType::Attack,
            [-1, -1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::X, 1),
                effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 11), 1)]
            )],
        ),
        uncommon_card(
            "CARD.CAPACITOR",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::OrbSlots(3)],
                effects![Effect::OrbSlots(2)]
            )],
        ),
        rare_card(
            "CARD.DEFRAGMENT",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 17, Amount::fixed(1, 2))],
        ),
        uncommon_card(
            "CARD.CHILL",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::LivingEnemies, 1),
                effects![Effect::Channel(1, 1)]
            )],
        ),
        rare_card(
            "CARD.HYPERBEAM",
            CardType::Attack,
            [2, 2],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::Attack(Target::AllEnemies, Amount::fixed(28, 36), 1),
                Effect::ApplyPower(Target::Player, 17, Amount::fixed(-3, -3))
            ],
        ),
        uncommon_card(
            "CARD.TEMPEST",
            CardType::Skill,
            [-1, -1],
            Target::Player,
            [0, 0],
            effects![Effect::Repeat(
                Amount {
                    base: 0,
                    upgraded: 1,
                    ascension: 0,
                    scale: Scale::X,
                    multiplier: 1,
                    divisor: 1
                },
                effects![Effect::Channel(0, 1)]
            )],
        ),
        uncommon_card(
            "CARD.OVERCLOCK",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(3)],
                    effects![Effect::Draw(2)]
                ),
                Effect::AddCard(Pile::Discard, 62, 1)
            ],
        ),
        card(
            "CARD.TURBO",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Energy(3)],
                    effects![Effect::Energy(2)]
                ),
                Effect::AddCard(Pile::Discard, 66, 1)
            ],
        ),
        uncommon_card(
            "CARD.SUNDER",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(24, 32), 1),
                Effect::If(Condition::TargetDead, effects![Effect::Energy(3)], &[])
            ],
        ),
        card(
            "CARD.BARRAGE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::Orbs, 1),
                effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 7), 1)]
            )],
        ),
        rare_card(
            "CARD.METEOR_STRIKE",
            CardType::Attack,
            [5, 5],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(24, 30), 1),
                Effect::Channel(3, 3)
            ],
        )
        .with_tags(STRIKE_TAG),
        uncommon_card(
            "CARD.DOUBLE_ENERGY",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::DoubleEnergy],
        ),
        card(
            "CARD.COMPILE_DRIVER",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(7, 10), 1),
                Effect::DrawAmount(Amount::scaled(Scale::OrbTypes, 1))
            ],
        ),
        rare_card(
            "CARD.MACHINE_LEARNING",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 18, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.BOUNCING_FLASK",
            CardType::Skill,
            [2, 2],
            Target::RandomEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::fixed(3, 4),
                effects![Effect::ApplyPower(
                    Target::RandomEnemy,
                    7,
                    Amount::fixed(3, 3),
                )]
            )],
        ),
        uncommon_card(
            "CARD.NOXIOUS_FUMES",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 19, Amount::fixed(2, 3))],
        ),
        uncommon_card(
            "CARD.INFINITE_BLADES",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 20, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.ACCURACY",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 21, Amount::fixed(4, 6))],
        ),
        card(
            "CARD.ASCENDERS_BANE",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [
                ETHEREAL | UNPLAYABLE | ETERNAL,
                ETHEREAL | UNPLAYABLE | ETERNAL,
            ],
            &[],
        ),
        card(
            "CARD.NORMALITY",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | LIMIT_THREE, UNPLAYABLE | LIMIT_THREE],
            &[],
        ),
        uncommon_card(
            "CARD.FTL",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 6), 1),
                Effect::If(
                    Condition::PriorCardsBelow([3, 4]),
                    effects![Effect::Draw(1)],
                    &[]
                )
            ],
        ),
        card(
            "CARD.GO_FOR_THE_EYES",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(3, 4), 1),
                Effect::If(
                    Condition::TargetAttacking,
                    effects![Effect::ApplyPower(
                        Target::ChosenEnemy,
                        2,
                        Amount::fixed(1, 2)
                    )],
                    &[]
                )
            ],
        ),
        rare_card(
            "CARD.MULTI_CAST",
            CardType::Skill,
            [-1, -1],
            Target::Player,
            [0, 0],
            effects![Effect::EvokeMany(Amount {
                base: 0,
                upgraded: 1,
                ascension: 0,
                scale: Scale::X,
                multiplier: 1,
                divisor: 1
            })],
        ),
        card(
            "CARD.CHARGE_BATTERY",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(7, 10)),
                Effect::ApplyPower(Target::Player, 22, Amount::fixed(1, 1))
            ],
        ),
        card(
            "CARD.HOLOGRAM",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::Select(
                Pile::Discard,
                CardFilter::Any,
                [1, 1],
                false,
                false,
                CardOp::Move(Pile::Hand)
            )],
        ),
        rare_card(
            "CARD.REBOOT",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::RecycleHand([4, 6])],
        ),
        rare_card(
            "CARD.GENETIC_ALGORITHM",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![
                Effect::Block(
                    Target::Player,
                    Amount {
                        base: 1,
                        upgraded: 1,
                        ascension: 0,
                        scale: Scale::CardValue,
                        multiplier: 1,
                        divisor: 1
                    }
                ),
                Effect::PersistCard(Amount::fixed(3, 4))
            ],
        ),
        uncommon_card(
            "CARD.CHAOS",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::RandomOrb([1, 2])],
        ),
        card(
            "CARD.BOOST_AWAY",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(6, 9)),
                Effect::AddCard(Pile::Discard, 58, 1)
            ],
        ),
        uncommon_card(
            "CARD.FIGHT_THROUGH",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(13, 17)),
                Effect::AddCard(Pile::Discard, 60, 2)
            ],
        ),
        uncommon_card(
            "CARD.SHADOW_SHIELD",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(11, 15)),
                Effect::Channel(2, 1)
            ],
        ),
        uncommon_card(
            "CARD.BULK_UP",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 0, Amount::fixed(2, 3)),
                Effect::ApplyPower(Target::Player, 1, Amount::fixed(2, 3)),
                Effect::OrbSlots(1)
            ],
        ),
        card(
            "CARD.QUADCAST",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::EvokeMany(Amount::fixed(4, 4))],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.GUNK_UP",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(4, 5), 3),
                Effect::AddCard(Pile::Discard, 3, 1)
            ],
        ),
        uncommon_card(
            "CARD.NULL",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 13), 1),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(2, 3)),
                Effect::Channel(2, 1)
            ],
        ),
        uncommon_card(
            "CARD.SCAVENGE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Exhaust)
                ),
                Effect::ApplyPower(Target::Player, 22, Amount::fixed(2, 3))
            ],
        ),
        card(
            "CARD.FOCUSED_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 11), 1),
                Effect::ApplyPower(Target::Player, 17, Amount::fixed(1, 2)),
                Effect::ApplyPower(
                    Target::Player,
                    power_id::FOCUSED_STRIKE,
                    Amount::fixed(1, 2)
                )
            ],
        )
        .with_tags(STRIKE_TAG),
        card(
            "CARD.HOTFIX",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, 0],
            effects![
                Effect::ApplyPower(Target::Player, 17, Amount::fixed(2, 2)),
                Effect::ApplyPower(Target::Player, power_id::HOTFIX, Amount::fixed(2, 2))
            ],
        ),
        uncommon_card(
            "CARD.SYNCHRONIZE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, 0],
            effects![
                Effect::ApplyPower(Target::Player, 17, Amount::scaled(Scale::OrbTypes, 2)),
                Effect::ApplyPower(
                    Target::Player,
                    power_id::SYNCHRONIZE,
                    Amount::scaled(Scale::OrbTypes, 2)
                )
            ],
        ),
        card(
            "CARD.MOMENTUM_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 13), 1),
                Effect::SetCardCost(0)
            ],
        )
        .with_tags(STRIKE_TAG),
        card(
            "CARD.CLAW",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 3,
                        upgraded: 4,
                        ascension: 0,
                        scale: Scale::CardValue,
                        multiplier: 1,
                        divisor: 1
                    },
                    1
                ),
                Effect::GrowAll(184, Amount::fixed(2, 3))
            ],
        ),
        rare_card(
            "CARD.ALL_FOR_ONE",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 14), 1),
                Effect::MoveAll(Pile::Discard, CardFilter::PlayableCost(0), Pile::Hand)
            ],
        ),
        uncommon_card(
            "CARD.SCRAPE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(7, 10), 1),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::DrawFiltered(5, CardFilter::PlayableCost(0))],
                    effects![Effect::DrawFiltered(4, CardFilter::PlayableCost(0))]
                )
            ],
        ),
        uncommon_card(
            "CARD.GLASSWORK",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(5, 8)),
                Effect::Channel(4, 1)
            ],
        ),
        rare_card(
            "CARD.HELIX_DRILL",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::EnergySpent, 1),
                effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(3, 5), 1)]
            )],
        ),
        rare_card(
            "CARD.ICE_LANCE",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(19, 24), 1),
                Effect::Channel(1, 3)
            ],
        ),
        uncommon_card(
            "CARD.REFRACT",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 12), 2),
                Effect::Channel(4, 2)
            ],
        ),
        rare_card(
            "CARD.SHATTER",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::Attack(Target::AllEnemies, Amount::fixed(7, 11), 1),
                Effect::EvokeAll(2)
            ],
        ),
        uncommon_card(
            "CARD.COMPACT",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(6, 7))],
        ),
        card(
            "CARD.BIASED_COGNITION",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 17, Amount::fixed(4, 5)),
                Effect::ApplyPower(Target::Player, 24, Amount::fixed(1, 1))
            ],
        )
        .with_rarity(CardRarity::Ancient),
        rare_card(
            "CARD.COOLANT",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 25, Amount::fixed(2, 3))],
        ),
        uncommon_card(
            "CARD.HAILSTORM",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 26, Amount::fixed(6, 8))],
        ),
        uncommon_card(
            "CARD.LOOP",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 27, Amount::fixed(1, 2))],
        ),
        rare_card(
            "CARD.SPINNER",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![
                    Effect::Channel(4, 1),
                    Effect::OrbSlots(1),
                    Effect::ApplyPower(Target::Player, 28, Amount::fixed(1, 1))
                ],
                effects![Effect::ApplyPower(Target::Player, 28, Amount::fixed(1, 1))]
            )],
        ),
        uncommon_card(
            "CARD.STORM",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 29, Amount::fixed(1, 2))],
        ),
        uncommon_card(
            "CARD.SUBROUTINE",
            CardType::Power,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 30, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.ITERATION",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 31, Amount::fixed(2, 3))],
        ),
        rare_card(
            "CARD.BUFFER",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 32, Amount::fixed(1, 2))],
        ),
        uncommon_card(
            "CARD.FERAL",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 33, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.SMOKESTACK",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 34, Amount::fixed(5, 7))],
        ),
        rare_card(
            "CARD.ADAPTIVE_STRIKE",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(18, 23), 1),
                Effect::CopyCard(Pile::Discard, 1)
            ],
        )
        .with_tags(STRIKE_TAG),
        uncommon_card(
            "CARD.SYNTHESIS",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(14, 20), 1),
                Effect::ApplyPower(Target::Player, 35, Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.CREATIVE_AI",
            CardType::Power,
            [3, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 36, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.WHITE_NOISE",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::RandomCard(Pile::Hand, CardType::Power, 1, true)],
        ),
        rare_card(
            "CARD.TRASH_TO_TREASURE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 37, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.THUNDER",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 38, Amount::fixed(6, 8))],
        ),
        rare_card(
            "CARD.SIGNAL_BOOST",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::ApplyPower(Target::Player, 39, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.ECHO_FORM",
            CardType::Power,
            [3, 3],
            Target::Player,
            [ETHEREAL, 0],
            effects![Effect::ApplyPower(Target::Player, 40, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.FINISHER",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::PriorAttacks, 1),
                effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 8), 1)]
            )],
        ),
        uncommon_card(
            "CARD.FLECHETTES",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::HandType(CardType::Skill), 1),
                effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 7), 1)]
            )],
        ),
        card(
            "CARD.DODGE_AND_ROLL",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::DodgeRoll(Amount::fixed(4, 6))],
        ),
        uncommon_card(
            "CARD.ESCAPE_PLAN",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![Effect::DrawBlockIf(CardType::Skill, Amount::fixed(3, 5))],
        ),
        card(
            "CARD.PIERCING_WAIL",
            CardType::Skill,
            [1, 1],
            Target::AllEnemies,
            [EXHAUST, EXHAUST],
            effects![Effect::TemporaryStrength(
                Target::AllEnemies,
                power_id::PIERCING_WAIL,
                Amount::fixed(6, 8)
            )],
        ),
        card(
            "CARD.LEADING_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(3, 6), 1),
                Effect::AddCard(Pile::Hand, 137, 2)
            ],
        )
        .with_tags(STRIKE_TAG),
        card(
            "CARD.PREDATOR",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(15, 20), 1),
                Effect::ApplyPower(Target::Player, 43, Amount::fixed(2, 2))
            ],
        ),
        uncommon_card(
            "CARD.PRECISE_CUT",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount {
                    base: 13,
                    upgraded: 16,
                    ascension: 0,
                    scale: Scale::HandSize,
                    multiplier: -2,
                    divisor: 1
                },
                1
            )],
        ),
        uncommon_card(
            "CARD.MIRAGE",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::Block(
                Target::Player,
                Amount::scaled(Scale::EnemyPowerTotal(7), 1)
            )],
        ),
        uncommon_card(
            "CARD.POUNCE",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(14, 20), 1),
                Effect::ApplyPower(Target::Player, 44, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.CALCULATED_GAMBLE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST | RETAIN],
            effects![Effect::DiscardHandDraw],
        ),
        uncommon_card(
            "CARD.EXPERTISE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::DrawTo([6, 7])],
        ),
        uncommon_card(
            "CARD.REFLEX",
            CardType::Skill,
            [3, 3],
            Target::Player,
            [SLY, SLY],
            effects![
                Effect::Draw(2),
                Effect::If(Condition::Upgraded, effects![Effect::Draw(1)], &[])
            ],
        ),
        uncommon_card(
            "CARD.TACTICIAN",
            CardType::Skill,
            [3, 3],
            Target::Player,
            [SLY, SLY],
            effects![
                Effect::Energy(1),
                Effect::If(Condition::Upgraded, effects![Effect::Energy(1)], &[])
            ],
        ),
        card(
            "CARD.OUTMANEUVER",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 22, Amount::fixed(2, 3))],
        )
        .with_rarity(CardRarity::Event),
        uncommon_card(
            "CARD.BLUR",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(5, 8)),
                Effect::ApplyPower(Target::Player, 45, Amount::fixed(1, 1))
            ],
        ),
        card(
            "CARD.DISTRACTION",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::RandomCard(Pile::Hand, CardType::Skill, 1, true)],
        ),
        rare_card(
            "CARD.ABRASIVE",
            CardType::Power,
            [3, 3],
            Target::Player,
            [SLY, SLY],
            effects![
                Effect::ApplyPower(Target::Player, 1, Amount::fixed(1, 1)),
                Effect::ApplyPower(Target::Player, 8, Amount::fixed(4, 6))
            ],
        ),
        card(
            "CARD.SNAKEBITE",
            CardType::Skill,
            [2, 2],
            Target::ChosenEnemy,
            [RETAIN, RETAIN],
            effects![Effect::ApplyPower(
                Target::ChosenEnemy,
                7,
                Amount::fixed(7, 10)
            )],
        ),
        card(
            "CARD.UNTOUCHABLE",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [SLY, SLY],
            effects![Effect::Block(Target::Player, Amount::fixed(6, 9))],
        ),
        card(
            "CARD.FLICK_FLACK",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [SLY, SLY],
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(6, 8), 1)],
        ),
        uncommon_card(
            "CARD.HAZE",
            CardType::Skill,
            [3, 3],
            Target::AllEnemies,
            [SLY, SLY],
            effects![Effect::ApplyPower(
                Target::AllEnemies,
                7,
                Amount::fixed(4, 6)
            )],
        ),
        card(
            "CARD.RICOCHET",
            CardType::Attack,
            [2, 2],
            Target::RandomEnemy,
            [SLY, SLY],
            effects![Effect::Repeat(
                Amount::fixed(4, 5),
                effects![Effect::Attack(Target::RandomEnemy, Amount::fixed(3, 3), 1)]
            )],
        ),
        rare_card(
            "CARD.AFTERIMAGE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 46, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.BURST",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 47, Amount::fixed(1, 2))],
        ),
        rare_card(
            "CARD.CORROSIVE_WAVE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 48, Amount::fixed(2, 3))],
        ),
        rare_card(
            "CARD.SHADOWMELD",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 49, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.SHADOW_STEP",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 50, Amount::fixed(1, 1))],
        ),
        card(
            "CARD.WRAITH_FORM",
            CardType::Power,
            [3, 3],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 5, Amount::fixed(2, 3)),
                Effect::ApplyPower(Target::Player, 52, Amount::fixed(1, 1))
            ],
        )
        .with_rarity(CardRarity::Ancient),
        rare_card(
            "CARD.SERPENT_FORM",
            CardType::Power,
            [3, 3],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 53, Amount::fixed(4, 6))],
        ),
        card(
            "CARD.ANTICIPATE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 1, Amount::fixed(2, 3)),
                Effect::ApplyPower(Target::Player, 54, Amount::fixed(2, 3))
            ],
        ),
        rare_card(
            "CARD.MALAISE",
            CardType::Skill,
            [-1, -1],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![
                Effect::ApplyDebuff(
                    Target::ChosenEnemy,
                    0,
                    Amount {
                        base: 0,
                        upgraded: -1,
                        ascension: 0,
                        scale: Scale::X,
                        multiplier: -1,
                        divisor: 1
                    }
                ),
                Effect::ApplyPower(
                    Target::ChosenEnemy,
                    2,
                    Amount {
                        base: 0,
                        upgraded: 1,
                        ascension: 0,
                        scale: Scale::X,
                        multiplier: 1,
                        divisor: 1
                    }
                )
            ],
        ),
        uncommon_card(
            "CARD.MEMENTO_MORI",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 11,
                        upgraded: 11,
                        ascension: 0,
                        scale: Scale::Discarded,
                        multiplier: 5,
                        divisor: 1
                    },
                    1
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 9,
                        upgraded: 9,
                        ascension: 0,
                        scale: Scale::Discarded,
                        multiplier: 4,
                        divisor: 1
                    },
                    1
                )]
            )],
        ),
        rare_card(
            "CARD.STORM_OF_STEEL",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::DiscardHandAdd(137)],
        ),
        uncommon_card(
            "CARD.STRANGLE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 10), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 55, Amount::fixed(2, 3))
            ],
        ),
        card(
            "CARD.SUPPRESS",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [INNATE, INNATE],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(11, 17), 1),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(3, 5))
            ],
        )
        .with_rarity(CardRarity::Ancient),
        rare_card(
            "CARD.THE_HUNT",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [EXHAUST | NO_GENERATE, EXHAUST | NO_GENERATE],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 15), 1),
                Effect::If(
                    Condition::TargetDead,
                    effects![Effect::ApplyPower(Target::Player, 56, Amount::fixed(1, 1))],
                    &[]
                )
            ],
        ),
        rare_card(
            "CARD.ACCELERANT",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 57, Amount::fixed(1, 2))],
        ),
        rare_card(
            "CARD.ENVENOM",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 58, Amount::fixed(1, 2))],
        ),
        rare_card(
            "CARD.FAN_OF_KNIVES",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 59, Amount::fixed(1, 1)),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::AddCard(Pile::Hand, 137, 5)],
                    effects![Effect::AddCard(Pile::Hand, 137, 4)]
                )
            ],
        ),
        rare_card(
            "CARD.TOOLS_OF_THE_TRADE",
            CardType::Power,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 60, Amount::fixed(1, 1))],
        ),
        CardDef {
            id: "CARD.GRAND_FINALE",
            card_type: CardType::Attack,
            rarity: CardRarity::Rare,
            cost: [0, 0],
            star_cost: [-1, -1],
            target: Target::AllEnemies,
            flags: [0, 0],
            playable: PlayCondition::EmptyDrawPile,
            effects: effects![Effect::Attack(Target::AllEnemies, Amount::fixed(60, 75), 1)],
            hooks: EMPTY_HOOKS,
            tags: 0,
        },
        rare_card(
            "CARD.BULLET_TIME",
            CardType::Skill,
            [3, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::FreeHand,
                Effect::ApplyPower(Target::Player, 9, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.BUBBLE_BUBBLE",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::TargetHasPower(7),
                effects![Effect::StackPower(
                    Target::ChosenEnemy,
                    7,
                    Amount::fixed(9, 12)
                )],
                &[]
            )],
        ),
        rare_card(
            "CARD.ECHOING_SLASH",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::Damage(Target::AllEnemies, Amount::fixed(10, 13))],
        ),
        uncommon_card(
            "CARD.UP_MY_SLEEVE",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::AddCard(Pile::Hand, 137, 4)],
                    effects![Effect::AddCard(Pile::Hand, 137, 3)]
                ),
                Effect::ReduceCardCost(1)
            ],
        ),
        uncommon_card(
            "CARD.HAND_TRICK",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(7, 10)),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::TypeWithoutTurnFlag(CardType::Skill, SLY),
                    [1, 1],
                    false,
                    true,
                    CardOp::TurnFlag(SLY)
                )
            ],
        ),
        uncommon_card(
            "CARD.HIDDEN_DAGGERS",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [2, 2],
                    false,
                    false,
                    CardOp::Move(Pile::Discard)
                ),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::AddUpgradedCard(Pile::Hand, 137, 2)],
                    effects![Effect::AddCard(Pile::Hand, 137, 2)]
                )
            ],
        ),
        rare_card(
            "CARD.NIGHTMARE",
            CardType::Skill,
            [3, 2],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::Select(
                Pile::Hand,
                CardFilter::Any,
                [1, 1],
                false,
                true,
                CardOp::CopyNextTurn(3)
            )],
        ),
        rare_card(
            "CARD.BLADE_OF_INK",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::AddFlaggedCard(Pile::Hand, 137, 3, INKY)],
                effects![Effect::AddFlaggedCard(Pile::Hand, 137, 2, INKY)]
            )],
        ),
        rare_card(
            "CARD.MURDER",
            CardType::Attack,
            [3, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount {
                    base: 1,
                    upgraded: 1,
                    ascension: 0,
                    scale: Scale::DrawnCombat,
                    multiplier: 1,
                    divisor: 1
                },
                1
            )],
        ),
        uncommon_card(
            "CARD.PINPOINT",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(15, 19),
                1
            )],
        ),
        rare_card(
            "CARD.MASTER_PLANNER",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 61, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.PHANTOM_BLADES",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 62, Amount::fixed(9, 12))],
        ),
        uncommon_card(
            "CARD.SPEEDSTER",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 63, Amount::fixed(2, 2))],
        ),
        rare_card(
            "CARD.TRACKING",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::SourceHasPower(64),
                effects![Effect::ApplyPower(Target::Player, 64, Amount::fixed(1, 1))],
                effects![Effect::ApplyPower(Target::Player, 64, Amount::fixed(2, 2))]
            )],
        ),
        uncommon_card(
            "CARD.OUTBREAK",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::Player,
                65,
                Amount::fixed(11, 15)
            )],
        ),
        uncommon_card(
            "CARD.WELL_LAID_PLANS",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 66, Amount::fixed(1, 2))],
        ),
        rare_card(
            "CARD.KNIFE_TRAP",
            CardType::Skill,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::PlayExhaustedShivs],
        ),
        star_card(
            "CARD.FALLING_STAR",
            CardType::Attack,
            [0, 0],
            [2, 2],
            Target::ChosenEnemy,
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 12), 1),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(1, 1)),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(1, 1))
            ],
        )
        .with_rarity(CardRarity::Basic),
        card(
            "CARD.VENERATE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Stars(Amount::fixed(2, 3))],
        )
        .with_rarity(CardRarity::Basic),
        star_card(
            "CARD.ALIGNMENT",
            CardType::Skill,
            [0, 0],
            [3, 3],
            Target::Player,
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Energy(3)],
                effects![Effect::Energy(2)]
            )],
        )
        .with_rarity(CardRarity::Uncommon),
        rare_card(
            "CARD.ARSENAL",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 67, Amount::fixed(1, 1))],
        ),
        star_card(
            "CARD.ASTRAL_PULSE",
            CardType::Attack,
            [0, 0],
            [3, 3],
            Target::AllEnemies,
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(6, 8), 2)],
        ),
        rare_card(
            "CARD.BIG_BANG",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST | INNATE],
            effects![
                Effect::Draw(1),
                Effect::Stars(Amount::fixed(1, 1)),
                Effect::Energy(1),
                Effect::Forge(Amount::fixed(5, 5))
            ],
        ),
        uncommon_card(
            "CARD.BLACK_HOLE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 68, Amount::fixed(3, 4))],
        ),
        uncommon_card(
            "CARD.BULWARK",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(12, 15)),
                Effect::Forge(Amount::fixed(10, 13))
            ],
        ),
        card(
            "CARD.SOVEREIGN_BLADE",
            CardType::Attack,
            [2, 1],
            Target::ChosenEnemy,
            [RETAIN, RETAIN],
            effects![
                Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 10,
                        upgraded: 10,
                        ascension: 0,
                        scale: Scale::CardValue,
                        multiplier: 1,
                        divisor: 1
                    },
                    1
                ),
                Effect::Block(Target::Player, Amount::scaled(Scale::Power(82), 1))
            ],
        ),
        card(
            "CARD.MINION_STRIKE",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1),
                Effect::Draw(1)
            ],
        )
        .with_tags(STRIKE_TAG | MINION_TAG),
        card(
            "CARD.BEGONE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Transform(280, 1)
                )],
                effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Transform(280, 0)
                )]
            )],
        ),
        uncommon_card(
            "CARD.CHARGE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Select(
                    Pile::Draw,
                    CardFilter::Any,
                    [2, 2],
                    false,
                    false,
                    CardOp::Transform(283, 1)
                )],
                effects![Effect::Select(
                    Pile::Draw,
                    CardFilter::Any,
                    [2, 2],
                    false,
                    false,
                    CardOp::Transform(283, 0)
                )]
            )],
        ),
        card(
            "CARD.MINION_DIVE_BOMB",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(13, 16),
                1
            )],
        )
        .with_tags(MINION_TAG),
        uncommon_card(
            "CARD.CHILD_OF_THE_STARS",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 69, Amount::fixed(2, 3))],
        ),
        star_card(
            "CARD.CLOAK_OF_STARS",
            CardType::Skill,
            [0, 0],
            [1, 1],
            Target::Player,
            effects![Effect::Block(Target::Player, Amount::fixed(7, 10))],
        ),
        card(
            "CARD.COLLISION_COURSE",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(11, 15), 1),
                Effect::AddCard(Pile::Hand, 287, 1)
            ],
        ),
        card(
            "CARD.DEBRIS",
            CardType::Status,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST],
            &[],
        ),
        star_card(
            "CARD.COMET",
            CardType::Attack,
            [0, 0],
            [5, 5],
            Target::ChosenEnemy,
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(33, 44), 1),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(3, 3)),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(3, 3))
            ],
        )
        .with_rarity(CardRarity::Rare),
        uncommon_card(
            "CARD.CONQUEROR",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Forge(Amount::fixed(3, 5)),
                Effect::ApplyPower(Target::ChosenEnemy, 70, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.CONVERGENCE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 71, Amount::fixed(1, 1)),
                Effect::ApplyPower(Target::Player, 22, Amount::fixed(1, 1)),
                Effect::ApplyPower(Target::Player, 72, Amount::fixed(1, 2))
            ],
        ),
        rare_card(
            "CARD.CRASH_LANDING",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::Attack(Target::AllEnemies, Amount::fixed(21, 26), 1),
                Effect::AddCard(Pile::Hand, 287, 10)
            ],
        ),
        star_card(
            "CARD.CRESCENT_SPEAR",
            CardType::Attack,
            [1, 1],
            [1, 1],
            Target::ChosenEnemy,
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 8,
                        upgraded: 8,
                        ascension: 0,
                        scale: Scale::StarCards,
                        multiplier: 3,
                        divisor: 1
                    },
                    1
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 8,
                        upgraded: 8,
                        ascension: 0,
                        scale: Scale::StarCards,
                        multiplier: 2,
                        divisor: 1
                    },
                    1
                )]
            )],
        ),
        card(
            "CARD.CRUSH_UNDER",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::Attack(Target::AllEnemies, Amount::fixed(7, 8), 1),
                Effect::TemporaryStrength(
                    Target::AllEnemies,
                    power_id::CRUSH_UNDER,
                    Amount::fixed(1, 2)
                )
            ],
        ),
        star_card_flags(
            "CARD.DECISIONS_DECISIONS",
            CardType::Skill,
            [0, 0],
            [6, 6],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![
                Effect::Draw(3),
                Effect::If(Condition::Upgraded, effects![Effect::Draw(2)], &[]),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Type(CardType::Skill),
                    [1, 1],
                    false,
                    false,
                    CardOp::AutoPlay(3)
                )
            ],
        )
        .with_rarity(CardRarity::Rare),
        star_card(
            "CARD.DEVASTATE",
            CardType::Attack,
            [1, 1],
            [4, 4],
            Target::ChosenEnemy,
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(30, 40),
                1
            )],
        )
        .with_rarity(CardRarity::Uncommon),
        star_card_flags(
            "CARD.DYING_STAR",
            CardType::Attack,
            [1, 1],
            [3, 3],
            Target::AllEnemies,
            [ETHEREAL, ETHEREAL],
            effects![
                Effect::Attack(Target::AllEnemies, Amount::fixed(9, 11), 1),
                Effect::TemporaryStrength(
                    Target::AllEnemies,
                    power_id::DYING_STAR,
                    Amount::fixed(9, 11)
                )
            ],
        )
        .with_rarity(CardRarity::Rare),
        rare_card(
            "CARD.FOREGONE_CONCLUSION",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 73, Amount::fixed(2, 3))],
        ),
        uncommon_card(
            "CARD.FURNACE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 74, Amount::fixed(5, 7))],
        ),
        star_card(
            "CARD.GAMMA_BLAST",
            CardType::Attack,
            [0, 0],
            [3, 3],
            Target::ChosenEnemy,
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(13, 18), 1),
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(2, 2)),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(2, 2))
            ],
        )
        .with_rarity(CardRarity::Uncommon),
        card(
            "CARD.GATHER_LIGHT",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(8, 11)),
                Effect::Stars(Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.GENESIS",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 75, Amount::fixed(2, 3))],
        ),
        uncommon_card(
            "CARD.GLIMMER",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Draw(3),
                Effect::If(Condition::Upgraded, effects![Effect::Draw(1)], &[]),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Draw)
                )
            ],
        ),
        card(
            "CARD.GLITTERSTREAM",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(11, 13)),
                Effect::BlockNextTurn(Amount::fixed(5, 7))
            ],
        ),
        card(
            "CARD.GLOW",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Stars(Amount::fixed(1, 2)),
                Effect::Draw(1),
                Effect::ApplyPower(Target::Player, 43, Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.GUARDS",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [u8::MAX, u8::MAX],
                    false,
                    true,
                    CardOp::Transform(306, 1)
                )],
                effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [u8::MAX, u8::MAX],
                    false,
                    true,
                    CardOp::Transform(306, 0)
                )]
            )],
        ),
        card(
            "CARD.MINION_SACRIFICE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::Block(Target::Player, Amount::fixed(8, 11))],
        )
        .with_tags(MINION_TAG),
        star_card(
            "CARD.GUIDING_STAR",
            CardType::Attack,
            [1, 1],
            [2, 2],
            Target::ChosenEnemy,
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(12, 13), 1),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(3)],
                    effects![Effect::Draw(2)]
                )
            ],
        ),
        rare_card(
            "CARD.HEAVENLY_DRILL",
            CardType::Attack,
            [-1, -1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::XAtLeast(4),
                effects![Effect::Repeat(
                    Amount::scaled(Scale::X, 2),
                    effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 10), 1)]
                )],
                effects![Effect::Repeat(
                    Amount::scaled(Scale::X, 1),
                    effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 10), 1)]
                )]
            )],
        ),
        uncommon_card(
            "CARD.HEGEMONY",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(15, 18), 1),
                Effect::ApplyPower(Target::Player, 22, Amount::fixed(2, 3))
            ],
        ),
        card(
            "CARD.HIDDEN_CACHE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Stars(Amount::fixed(1, 1)),
                Effect::ApplyPower(Target::Player, 72, Amount::fixed(3, 4))
            ],
        ),
        rare_card(
            "CARD.I_AM_INVINCIBLE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(10, 13))],
        ),
        uncommon_card(
            "CARD.KINGLY_PUNCH",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount {
                    base: 8,
                    upgraded: 10,
                    ascension: 0,
                    scale: Scale::CardValue,
                    multiplier: 1,
                    divisor: 1
                },
                1
            )],
        )
        .with_hooks(KINGLY_PUNCH_HOOKS),
        uncommon_card(
            "CARD.KNOCKOUT_BLOW",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(30, 38), 1),
                Effect::If(
                    Condition::TargetDead,
                    effects![Effect::Stars(Amount::fixed(5, 5))],
                    &[]
                )
            ],
        ),
        card(
            "CARD.KNOW_THY_PLACE",
            CardType::Skill,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST, 0],
            effects![
                Effect::ApplyPower(Target::ChosenEnemy, power_id::WEAK, Amount::fixed(1, 1)),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.LUNAR_BLAST",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::SkillsPlayed, 1),
                effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(4, 5), 1)]
            )],
        ),
        rare_card(
            "CARD.MAKE_IT_SO",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 9), 1)],
        ),
        star_card(
            "CARD.METEOR_SHOWER",
            CardType::Attack,
            [0, 0],
            [2, 2],
            Target::AllEnemies,
            effects![
                Effect::Attack(Target::AllEnemies, Amount::fixed(14, 21), 1),
                Effect::ApplyPower(Target::AllEnemies, 2, Amount::fixed(2, 2)),
                Effect::ApplyPower(Target::AllEnemies, 3, Amount::fixed(2, 2))
            ],
        )
        .with_rarity(CardRarity::Ancient),
        rare_card(
            "CARD.MONARCHS_GAZE",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 76, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.MONOLOGUE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, RETAIN],
            effects![Effect::ApplyPower(Target::Player, 77, Amount::fixed(1, 1))],
        ),
        star_card(
            "CARD.NEUTRON_AEGIS",
            CardType::Power,
            [1, 1],
            [5, 5],
            Target::Player,
            effects![Effect::ApplyPower(Target::Player, 79, Amount::fixed(8, 11))],
        )
        .with_rarity(CardRarity::Rare),
        uncommon_card(
            "CARD.ORBIT",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 80, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.PALE_BLUE_DOT",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 81, Amount::fixed(1, 2))],
        ),
        uncommon_card(
            "CARD.PARRY",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::Player,
                82,
                Amount::fixed(10, 14)
            )],
        ),
        star_card_flags(
            "CARD.PARTICLE_WALL",
            CardType::Skill,
            [0, 0],
            [2, 2],
            Target::Player,
            [RETURN_TO_HAND, RETURN_TO_HAND],
            effects![Effect::Block(Target::Player, Amount::fixed(9, 12))],
        )
        .with_rarity(CardRarity::Uncommon),
        card(
            "CARD.PATTER",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(8, 10)),
                Effect::ApplyPower(Target::Player, 83, Amount::fixed(2, 3))
            ],
        ),
        card(
            "CARD.PHOTON_CUT",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 13), 1),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Draw(2)],
                    effects![Effect::Draw(1)]
                ),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Draw)
                )
            ],
        ),
        uncommon_card(
            "CARD.PILLAR_OF_CREATION",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 84, Amount::fixed(3, 4))],
        ),
        uncommon_card(
            "CARD.PROPHESIZE",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Draw(9)],
                effects![Effect::Draw(6)]
            )],
        ),
        uncommon_card(
            "CARD.RADIATE",
            CardType::Attack,
            [0, 0],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::StarsGained, 1),
                effects![Effect::Attack(Target::AllEnemies, Amount::fixed(3, 4), 1)]
            )],
        ),
        card(
            "CARD.REFINE_BLADE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Forge(Amount::fixed(9, 13)),
                Effect::ApplyPower(Target::Player, 22, Amount::fixed(1, 1))
            ],
        ),
        star_card(
            "CARD.REFLECT",
            CardType::Skill,
            [1, 1],
            [3, 3],
            Target::Player,
            effects![
                Effect::Block(Target::Player, Amount::fixed(15, 20)),
                Effect::ApplyPower(Target::Player, 85, Amount::fixed(1, 1))
            ],
        )
        .with_rarity(CardRarity::Uncommon),
        star_card(
            "CARD.RESONANCE",
            CardType::Skill,
            [1, 1],
            [3, 3],
            Target::AllEnemies,
            effects![
                Effect::ApplyPower(Target::Player, 0, Amount::fixed(1, 2)),
                Effect::ApplyDebuff(Target::AllEnemies, 0, Amount::fixed(-1, -1))
            ],
        )
        .with_rarity(CardRarity::Uncommon),
        star_card_flags(
            "CARD.ROYAL_GAMBLE",
            CardType::Skill,
            [0, 0],
            [5, 5],
            Target::Player,
            [EXHAUST, EXHAUST | RETAIN],
            effects![Effect::Stars(Amount::fixed(9, 9))],
        )
        .with_rarity(CardRarity::Uncommon),
        rare_card(
            "CARD.ROYALTIES",
            CardType::Power,
            [1, 1],
            Target::Player,
            [NO_GENERATE, NO_GENERATE],
            effects![Effect::ApplyPower(
                Target::Player,
                86,
                Amount::fixed(30, 40)
            )],
        ),
        rare_card(
            "CARD.SEEKING_EDGE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 87, Amount::fixed(1, 1)),
                Effect::Forge(Amount::fixed(7, 11))
            ],
        ),
        star_card(
            "CARD.SEVEN_STARS",
            CardType::Attack,
            [2, 1],
            [7, 7],
            Target::AllEnemies,
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(7, 7), 7)],
        )
        .with_rarity(CardRarity::Rare),
        uncommon_card(
            "CARD.SHINING_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [RETURN_TO_DRAW, RETURN_TO_DRAW],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 11), 1),
                Effect::Stars(Amount::fixed(2, 2))
            ],
        )
        .with_tags(STRIKE_TAG),
        card(
            "CARD.SOLAR_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 10), 1),
                Effect::Stars(Amount::fixed(1, 2))
            ],
        )
        .with_tags(STRIKE_TAG),
        card(
            "CARD.SPOILS_OF_BATTLE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Forge(Amount::fixed(5, 8)), Effect::Draw(2)],
        ),
        star_card(
            "CARD.STARDUST",
            CardType::Attack,
            [0, 0],
            [-2, -2],
            Target::RandomEnemy,
            effects![Effect::Repeat(
                Amount::scaled(Scale::Event, 1),
                effects![Effect::Attack(Target::RandomEnemy, Amount::fixed(5, 7), 1)]
            )],
        )
        .with_rarity(CardRarity::Uncommon),
        uncommon_card(
            "CARD.SUMMON_FORTH",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::MoveAll(Pile::Draw, CardFilter::Id(279), Pile::Hand),
                Effect::MoveAll(Pile::Discard, CardFilter::Id(279), Pile::Hand),
                Effect::MoveAll(Pile::Exhaust, CardFilter::Id(279), Pile::Hand),
                Effect::Forge(Amount::fixed(8, 11))
            ],
        ),
        uncommon_card(
            "CARD.SUPERMASSIVE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 5,
                        upgraded: 5,
                        ascension: 0,
                        scale: Scale::Generated,
                        multiplier: 4,
                        divisor: 1
                    },
                    1
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 5,
                        upgraded: 5,
                        ascension: 0,
                        scale: Scale::Generated,
                        multiplier: 3,
                        divisor: 1
                    },
                    1
                )]
            )],
        ),
        rare_card(
            "CARD.SWORD_SAGE",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 89, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.TERRAFORMING",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 83, Amount::fixed(6, 8))],
        ),
        star_card(
            "CARD.THE_SEALED_THRONE",
            CardType::Power,
            [1, 0],
            [3, 3],
            Target::Player,
            effects![Effect::ApplyPower(Target::Player, 90, Amount::fixed(1, 1))],
        )
        .with_rarity(CardRarity::Ancient),
        star_card(
            "CARD.THE_SMITH",
            CardType::Skill,
            [1, 1],
            [4, 4],
            Target::Player,
            effects![Effect::Forge(Amount::fixed(30, 40))],
        )
        .with_rarity(CardRarity::Rare),
        rare_card(
            "CARD.TYRANNY",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 91, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.VOID_FORM",
            CardType::Power,
            [3, 3],
            Target::Player,
            [ETHEREAL, 0],
            effects![
                Effect::ApplyPower(Target::Player, 92, Amount::fixed(2, 2)),
                Effect::EndTurn
            ],
        ),
        card(
            "CARD.WROUGHT_IN_WAR",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(7, 9), 1),
                Effect::Forge(Amount::fixed(7, 9))
            ],
        ),
        rare_card(
            "CARD.BUNDLE_OF_JOY",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::RandomColorless(
                Pile::Hand,
                Amount::fixed(3, 4),
                false
            )],
        ),
        rare_card(
            "CARD.HEIRLOOM_HAMMER",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(20, 25), 1),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Colorless,
                    [1, 1],
                    false,
                    false,
                    CardOp::CopySelected(1)
                )
            ],
        ),
        uncommon_card(
            "CARD.MANIFEST_AUTHORITY",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(7, 8)),
                Effect::RandomColorless(Pile::Hand, Amount::fixed(1, 1), true)
            ],
        ),
        star_card(
            "CARD.QUASAR",
            CardType::Skill,
            [0, 0],
            [2, 2],
            Target::Player,
            effects![Effect::OfferColorless(3, true, false)],
        )
        .with_rarity(CardRarity::Uncommon),
        uncommon_card(
            "CARD.SPECTRUM_SHIFT",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 93, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.BEAT_INTO_SHAPE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 7), 1),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Forge(Amount {
                        base: 7,
                        upgraded: 7,
                        ascension: 0,
                        scale: Scale::PriorTargetHits,
                        multiplier: 7,
                        divisor: 1
                    })],
                    effects![Effect::Forge(Amount {
                        base: 5,
                        upgraded: 5,
                        ascension: 0,
                        scale: Scale::PriorTargetHits,
                        multiplier: 5,
                        divisor: 1
                    })]
                )
            ],
        ),
        rare_card(
            "CARD.BOMBARDMENT",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(18, 24),
                1
            )],
        ),
        card(
            "CARD.BODYGUARD",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Summon(Amount::fixed(5, 7))],
        )
        .with_rarity(CardRarity::Basic),
        card(
            "CARD.UNLEASH",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::OstyAttack(
                Target::ChosenEnemy,
                Amount {
                    base: 6,
                    upgraded: 9,
                    ascension: 0,
                    scale: Scale::OstyHp,
                    multiplier: 1,
                    divisor: 1
                },
                1
            )],
        )
        .with_rarity(CardRarity::Basic)
        .with_tags(OSTY_ATTACK_TAG),
        card(
            "CARD.AFTERLIFE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::Summon(Amount::fixed(6, 9))],
        ),
        rare_card(
            "CARD.BANSHEES_CRY",
            CardType::Attack,
            [9, 7],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(33, 33), 1)],
        ),
        card(
            "CARD.BLIGHT_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(8, 10), 1),
                Effect::ApplyPower(
                    Target::ChosenEnemy,
                    94,
                    Amount::scaled(Scale::LastDamage, 1)
                )
            ],
        )
        .with_tags(STRIKE_TAG),
        uncommon_card(
            "CARD.BONE_SHARDS",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![Effect::If(
                Condition::OstyAlive,
                effects![
                    Effect::OstyAttack(Target::AllEnemies, Amount::fixed(9, 12), 1),
                    Effect::Block(Target::Player, Amount::fixed(9, 12)),
                    Effect::Kill(Target::Osty)
                ],
                &[]
            )],
        )
        .with_tags(OSTY_ATTACK_TAG),
        uncommon_card(
            "CARD.BORROWED_TIME",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Energy(6)],
                    effects![Effect::Energy(4)]
                ),
                Effect::ApplyPower(Target::Player, 95, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.BURY",
            CardType::Attack,
            [4, 4],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(52, 63),
                1
            )],
        ),
        uncommon_card(
            "CARD.CALCIFY",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 96, Amount::fixed(4, 6))],
        ),
        rare_card(
            "CARD.CALL_OF_THE_VOID",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 97, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.CAPTURE_SPIRIT",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::LoseHp(Target::ChosenEnemy, Amount::fixed(3, 4)),
                Effect::AddRandomCard(368, [3, 4], false)
            ],
        ),
        card(
            "CARD.SOUL",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Draw(3)],
                effects![Effect::Draw(2)]
            )],
        ),
        uncommon_card(
            "CARD.CLEANSE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Summon(Amount::fixed(3, 5)),
                Effect::Select(
                    Pile::Draw,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Exhaust)
                )
            ],
        ),
        uncommon_card(
            "CARD.COUNTDOWN",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 98, Amount::fixed(6, 9))],
        ),
        uncommon_card(
            "CARD.DANSE_MACABRE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 99, Amount::fixed(4, 6))],
        ),
        uncommon_card(
            "CARD.DEATH_MARCH",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 9,
                        upgraded: 9,
                        ascension: 0,
                        scale: Scale::ExtraDrawn,
                        multiplier: 6,
                        divisor: 1
                    },
                    1
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 8,
                        upgraded: 8,
                        ascension: 0,
                        scale: Scale::ExtraDrawn,
                        multiplier: 4,
                        divisor: 1
                    },
                    1
                )]
            )],
        ),
        uncommon_card(
            "CARD.DEATHBRINGER",
            CardType::Skill,
            [2, 2],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::AllEnemies, 94, Amount::fixed(21, 26)),
                Effect::ApplyPower(Target::AllEnemies, 2, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.DEATHS_DOOR",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(6, 7)),
                Effect::If(
                    Condition::DoomApplied,
                    effects![
                        Effect::Block(Target::Player, Amount::fixed(6, 7)),
                        Effect::Block(Target::Player, Amount::fixed(6, 7))
                    ],
                    &[]
                )
            ],
        ),
        uncommon_card(
            "CARD.DEBILITATE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 12), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 100, Amount::fixed(2, 3))
            ],
        ),
        card(
            "CARD.DEFILE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [ETHEREAL, ETHEREAL],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(13, 17),
                1
            )],
        ),
        uncommon_card(
            "CARD.DELAY",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(11, 13)),
                Effect::ApplyPower(Target::Player, 22, Amount::fixed(1, 2))
            ],
        ),
        rare_card(
            "CARD.DEMESNE",
            CardType::Power,
            [3, 2],
            Target::Player,
            [ETHEREAL, ETHEREAL],
            effects![Effect::ApplyPower(Target::Player, 101, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.DEVOUR_LIFE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 102, Amount::fixed(1, 2))],
        ),
        uncommon_card(
            "CARD.DIRGE",
            CardType::Skill,
            [-1, -1],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![
                Effect::Repeat(
                    Amount::scaled(Scale::X, 1),
                    effects![Effect::Summon(Amount::fixed(3, 4))]
                ),
                Effect::Repeat(
                    Amount::scaled(Scale::X, 1),
                    effects![Effect::AddRandomCard(368, [1, 1], true)]
                )
            ],
        ),
        card(
            "CARD.DRAIN_POWER",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 12), 1),
                Effect::If(
                    Condition::Upgraded,
                    effects![Effect::Upgrade(Pile::Discard, 3, true)],
                    effects![Effect::Upgrade(Pile::Discard, 2, true)]
                )
            ],
        ),
        uncommon_card(
            "CARD.DREDGE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST | RETAIN],
            effects![Effect::Select(
                Pile::Discard,
                CardFilter::Any,
                [3, 3],
                false,
                false,
                CardOp::Move(Pile::Hand)
            )],
        ),
        rare_card(
            "CARD.EIDOLON",
            CardType::Skill,
            [2, 1],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::If(
                Condition::HandAtLeast(9),
                effects![
                    Effect::MoveAll(Pile::Hand, CardFilter::Any, Pile::Exhaust),
                    Effect::ApplyPower(Target::Player, 5, Amount::fixed(1, 1))
                ],
                effects![Effect::MoveAll(Pile::Hand, CardFilter::Any, Pile::Exhaust)]
            )],
        ),
        rare_card(
            "CARD.END_OF_DAYS",
            CardType::Skill,
            [3, 3],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::AllEnemies, 94, Amount::fixed(29, 37)),
                Effect::DoomKill
            ],
        ),
        uncommon_card(
            "CARD.ENFEEBLING_TOUCH",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [ETHEREAL, ETHEREAL],
            effects![Effect::ApplyPower(
                Target::ChosenEnemy,
                103,
                Amount::fixed(8, 11)
            )],
        ),
        rare_card(
            "CARD.ERADICATE",
            CardType::Attack,
            [-1, -1],
            Target::ChosenEnemy,
            [RETAIN, RETAIN],
            effects![Effect::Repeat(
                Amount::scaled(Scale::X, 1),
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount::fixed(11, 14),
                    1
                )]
            )],
        ),
        card(
            "CARD.FEAR",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [ETHEREAL, ETHEREAL],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(7, 8), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(1, 2))
            ],
        ),
        uncommon_card(
            "CARD.FETCH",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::OstyAttack(
                Target::ChosenEnemy,
                Amount::fixed(3, 6),
                1
            )],
        )
        .with_tags(OSTY_ATTACK_TAG),
        card(
            "CARD.FLATTEN",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::OstyAttack(
                Target::ChosenEnemy,
                Amount::fixed(12, 16),
                1
            )],
        )
        .with_tags(OSTY_ATTACK_TAG),
        card(
            "CARD.FORBIDDEN_GRIMOIRE",
            CardType::Power,
            [2, 1],
            Target::Player,
            [ETERNAL, ETERNAL],
            effects![Effect::ApplyPower(Target::Player, 104, Amount::fixed(1, 1))],
        )
        .with_rarity(CardRarity::Ancient),
        uncommon_card(
            "CARD.FRIENDSHIP",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyDebuff(Target::Player, 0, Amount::fixed(-2, -1)),
                Effect::ApplyPower(Target::Player, 105, Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.GLIMPSE_BEYOND",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::AddRandomCard(368, [3, 4], false)],
        ),
        card(
            "CARD.GRAVE_WARDEN",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(8, 11)),
                Effect::AddRandomCard(368, [1, 1], false)
            ],
        ),
        card(
            "CARD.GRAVEBLAST",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [EXHAUST, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(4, 6), 1),
                Effect::Select(
                    Pile::Discard,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Hand)
                )
            ],
        ),
        rare_card(
            "CARD.HANG",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 13), 1),
                Effect::DoublePower(Target::ChosenEnemy, 106, 2)
            ],
        ),
        uncommon_card(
            "CARD.HAUNT",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 107, Amount::fixed(6, 8))],
        ),
        CardDef {
            id: "CARD.HIGH_FIVE",
            card_type: CardType::Attack,
            rarity: CardRarity::Common,
            cost: [2, 2],
            star_cost: [-1, -1],
            target: Target::AllEnemies,
            flags: [0, 0],
            playable: PlayCondition::OstyAlive,
            effects: effects![
                Effect::OstyAttack(Target::AllEnemies, Amount::fixed(11, 13), 1),
                Effect::ApplyPower(Target::AllEnemies, 3, Amount::fixed(2, 3))
            ],
            hooks: EMPTY_HOOKS,
            tags: 0,
        }
        .with_rarity(CardRarity::Uncommon)
        .with_tags(OSTY_ATTACK_TAG),
        card(
            "CARD.INVOKE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 108, Amount::fixed(2, 3)),
                Effect::ApplyPower(Target::Player, 22, Amount::fixed(2, 3))
            ],
        ),
        uncommon_card(
            "CARD.LEGION_OF_BONE",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::Summon(Amount::fixed(6, 8))],
        ),
        uncommon_card(
            "CARD.LETHALITY",
            CardType::Power,
            [1, 1],
            Target::Player,
            [ETHEREAL, ETHEREAL],
            effects![Effect::ApplyPower(
                Target::Player,
                109,
                Amount::fixed(50, 75)
            )],
        ),
        uncommon_card(
            "CARD.MELANCHOLY",
            CardType::Skill,
            [3, 3],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(13, 17))],
        ),
        rare_card(
            "CARD.MISERY",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, RETAIN],
            effects![Effect::Misery(Amount::fixed(7, 9))],
        ),
        rare_card(
            "CARD.NECRO_MASTERY",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Summon(Amount::fixed(5, 8)),
                Effect::ApplyPower(Target::Player, 110, Amount::fixed(1, 1))
            ],
        ),
        card(
            "CARD.NEGATIVE_PULSE",
            CardType::Skill,
            [1, 1],
            Target::AllEnemies,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(5, 6)),
                Effect::ApplyPower(Target::AllEnemies, 94, Amount::fixed(7, 11))
            ],
        ),
        rare_card(
            "CARD.NEUROSURGE",
            CardType::Power,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::Energy(3),
                Effect::If(Condition::Upgraded, effects![Effect::Energy(1)], &[]),
                Effect::Draw(2),
                Effect::ApplyPower(Target::Player, 111, Amount::fixed(3, 3))
            ],
        ),
        uncommon_card(
            "CARD.NO_ESCAPE",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::ChosenEnemy,
                94,
                Amount {
                    base: 10,
                    upgraded: 15,
                    ascension: 0,
                    scale: Scale::TargetPowerDiv(94, 10),
                    multiplier: 5,
                    divisor: 1
                }
            )],
        ),
        rare_card(
            "CARD.OBLIVION",
            CardType::Skill,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::ChosenEnemy,
                112,
                Amount::fixed(3, 4)
            )],
        ),
        uncommon_card(
            "CARD.PAGESTORM",
            CardType::Power,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 113, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.PARSE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [ETHEREAL, ETHEREAL],
            effects![Effect::DrawAmount(Amount::fixed(3, 4))],
        ),
        card(
            "CARD.POKE",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::OstyAttack(
                Target::ChosenEnemy,
                Amount::fixed(6, 9),
                1
            )],
        )
        .with_tags(OSTY_ATTACK_TAG),
        card(
            "CARD.PROTECTOR",
            CardType::Attack,
            [1, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::OstyAttack(
                Target::ChosenEnemy,
                Amount {
                    base: 10,
                    upgraded: 15,
                    ascension: 0,
                    scale: Scale::OstyMaxHp,
                    multiplier: 1,
                    divisor: 1
                },
                1
            )],
        )
        .with_rarity(CardRarity::Ancient)
        .with_tags(OSTY_ATTACK_TAG),
        card(
            "CARD.PULL_AGGRO",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Summon(Amount::fixed(4, 5)),
                Effect::Block(Target::Player, Amount::fixed(7, 9))
            ],
        ),
        uncommon_card(
            "CARD.PULL_FROM_BELOW",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::AttackMany(
                Target::ChosenEnemy,
                Amount::fixed(5, 7),
                Amount::scaled(Scale::EtherealPlayed, 1)
            )],
        ),
        uncommon_card(
            "CARD.RATTLE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::OstyAttackMany(
                Target::ChosenEnemy,
                Amount::fixed(7, 9),
                Amount {
                    base: 1,
                    upgraded: 1,
                    ascension: 0,
                    scale: Scale::OstyAttacks,
                    multiplier: 1,
                    divisor: 1
                }
            )],
        )
        .with_tags(OSTY_ATTACK_TAG),
        rare_card(
            "CARD.REANIMATE",
            CardType::Skill,
            [3, 3],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::Summon(Amount::fixed(20, 25))],
        ),
        card(
            "CARD.REAP",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [RETAIN, RETAIN],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::fixed(27, 33),
                1
            )],
        ),
        rare_card(
            "CARD.REAPER_FORM",
            CardType::Power,
            [3, 3],
            Target::Player,
            [0, RETAIN],
            effects![Effect::ApplyPower(Target::Player, 114, Amount::fixed(1, 1))],
        ),
        card(
            "CARD.REAVE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 11), 1),
                Effect::AddRandomCard(368, [1, 1], true)
            ],
        ),
        uncommon_card(
            "CARD.RIGHT_HAND_HAND",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::OstyAttack(
                Target::ChosenEnemy,
                Amount::fixed(4, 6),
                1
            )],
        )
        .with_tags(OSTY_ATTACK_TAG),
        rare_card(
            "CARD.SACRIFICE",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [RETAIN, RETAIN],
            effects![Effect::If(
                Condition::OstyAlive,
                effects![
                    Effect::Kill(Target::Osty),
                    Effect::Block(Target::Player, Amount::scaled(Scale::OstyMaxHp, 2))
                ],
                &[]
            )],
        ),
        card(
            "CARD.SCOURGE",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::ChosenEnemy, 94, Amount::fixed(13, 16)),
                Effect::DrawAmount(Amount::fixed(1, 2))
            ],
        ),
        card(
            "CARD.SCULPTING_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 12), 1),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::WithoutFlag(ETHEREAL),
                    [1, 1],
                    false,
                    false,
                    CardOp::Flag(ETHEREAL)
                )
            ],
        )
        .with_tags(STRIKE_TAG),
        rare_card(
            "CARD.SEANCE",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [ETHEREAL, ETHEREAL],
            effects![Effect::Select(
                Pile::Draw,
                CardFilter::Any,
                [1, 1],
                false,
                false,
                CardOp::Transform(368, 0)
            )],
        ),
        rare_card(
            "CARD.SENTRY_MODE",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 115, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.SEVERANCE",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(13, 18), 1),
                Effect::AddRandomCard(368, [1, 1], false),
                Effect::AddCard(Pile::Discard, 368, 1),
                Effect::AddCard(Pile::Hand, 368, 1)
            ],
        ),
        rare_card(
            "CARD.SHARED_FATE",
            CardType::Skill,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![
                Effect::ApplyDebuff(Target::Player, 0, Amount::fixed(-2, -2)),
                Effect::ApplyDebuff(Target::ChosenEnemy, 0, Amount::fixed(-2, -3))
            ],
        ),
        uncommon_card(
            "CARD.SHROUD",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 116, Amount::fixed(2, 3))],
        ),
        uncommon_card(
            "CARD.SIC_EM",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::OstyAttack(Target::ChosenEnemy, Amount::fixed(5, 6), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 117, Amount::fixed(3, 4))
            ],
        )
        .with_tags(OSTY_ATTACK_TAG),
        card(
            "CARD.SWEEPING_GAZE",
            CardType::Attack,
            [0, 0],
            Target::RandomEnemy,
            [ETHEREAL | EXHAUST, ETHEREAL | EXHAUST],
            effects![Effect::OstyAttack(
                Target::RandomEnemy,
                Amount::fixed(10, 15),
                1
            )],
        )
        .with_tags(OSTY_ATTACK_TAG),
        uncommon_card(
            "CARD.SLEIGHT_OF_FLESH",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::Player,
                118,
                Amount::fixed(9, 13)
            )],
        ),
        card(
            "CARD.SNAP",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::OstyAttack(Target::ChosenEnemy, Amount::fixed(7, 10), 1),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::WithoutFlag(RETAIN),
                    [1, 1],
                    false,
                    false,
                    CardOp::Flag(RETAIN)
                )
            ],
        )
        .with_tags(OSTY_ATTACK_TAG),
        rare_card(
            "CARD.SOUL_STORM",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 9,
                        upgraded: 9,
                        ascension: 0,
                        scale: Scale::ExhaustId(368),
                        multiplier: 3,
                        divisor: 1
                    },
                    1
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 9,
                        upgraded: 9,
                        ascension: 0,
                        scale: Scale::ExhaustId(368),
                        multiplier: 2,
                        divisor: 1
                    },
                    1
                )]
            )],
        ),
        card(
            "CARD.SOW",
            CardType::Attack,
            [1, 1],
            Target::AllEnemies,
            [RETAIN, RETAIN],
            effects![Effect::Attack(Target::AllEnemies, Amount::fixed(8, 11), 1)],
        ),
        rare_card(
            "CARD.SPIRIT_OF_ASH",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 119, Amount::fixed(4, 5))],
        ),
        uncommon_card(
            "CARD.SPUR",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [RETAIN, RETAIN],
            effects![
                Effect::Summon(Amount::fixed(3, 5)),
                Effect::Heal(Target::Osty, Amount::fixed(5, 7))
            ],
        ),
        rare_card(
            "CARD.SQUEEZE",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::OstyAttack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 24,
                        upgraded: 24,
                        ascension: 0,
                        scale: Scale::Tagged(OSTY_ATTACK_TAG),
                        multiplier: 6,
                        divisor: 1
                    },
                    1
                )],
                effects![Effect::OstyAttack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 20,
                        upgraded: 20,
                        ascension: 0,
                        scale: Scale::Tagged(OSTY_ATTACK_TAG),
                        multiplier: 5,
                        divisor: 1
                    },
                    1
                )]
            )],
        )
        .with_tags(OSTY_ATTACK_TAG),
        rare_card(
            "CARD.THE_SCYTHE",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![
                Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 13,
                        upgraded: 13,
                        ascension: 0,
                        scale: Scale::CardValue,
                        multiplier: 1,
                        divisor: 1
                    },
                    1
                ),
                Effect::GrowCard(Amount::fixed(4, 5))
            ],
        ),
        rare_card(
            "CARD.TIMES_UP",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST | RETAIN],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::scaled(Scale::TargetPower(94), 1),
                1
            )],
        ),
        rare_card(
            "CARD.TRANSFIGURE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::Select(
                Pile::Hand,
                CardFilter::Any,
                [1, 1],
                false,
                false,
                CardOp::Transfigure
            )],
        ),
        rare_card(
            "CARD.UNDEATH",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(7, 9)),
                Effect::CopyCard(Pile::Discard, 1)
            ],
        ),
        uncommon_card(
            "CARD.VEILPIERCER",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 13), 1),
                Effect::ApplyPower(Target::Player, 120, Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.AGGRESSION",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 121, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.CASCADE",
            CardType::Skill,
            [-1, -1],
            Target::Player,
            [0, 0],
            effects![Effect::AutoPlayDraw(
                Amount {
                    base: 0,
                    upgraded: 1,
                    ascension: 0,
                    scale: Scale::X,
                    multiplier: 1,
                    divisor: 1
                },
                false
            )],
        ),
        uncommon_card(
            "CARD.COLOSSUS",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(5, 8)),
                Effect::ApplyPower(Target::Player, 122, Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.CRIMSON_MANTLE",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::Player,
                power_id::CRIMSON_MANTLE,
                Amount::fixed(8, 10),
            )],
        ),
        rare_card(
            "CARD.CRUELTY",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::Player,
                125,
                Amount::fixed(25, 50)
            )],
        ),
        uncommon_card(
            "CARD.DOMINATE",
            CardType::Skill,
            [1, 1],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![
                Effect::ApplyPower(Target::ChosenEnemy, 3, Amount::fixed(1, 2)),
                Effect::ApplyPower(Target::Player, 0, Amount::scaled(Scale::TargetPower(3), 1))
            ],
        ),
        uncommon_card(
            "CARD.EXPECT_A_FIGHT",
            CardType::Skill,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Repeat(
                    Amount::scaled(Scale::HandType(CardType::Attack), 1),
                    effects![Effect::Energy(1)]
                ),
                Effect::ApplyPower(Target::Player, 126, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.FLAME_BARRIER",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(12, 16)),
                Effect::ApplyPower(Target::Player, 127, Amount::fixed(4, 6))
            ],
        ),
        card(
            "CARD.HAVOC",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::AutoPlayDraw(Amount::fixed(1, 1), true)],
        ),
        rare_card(
            "CARD.HELLRAISER",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 128, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.INFERNO",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 129, Amount::fixed(6, 9)),
                Effect::ApplyPower(Target::Player, 130, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.JUGGLING",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 131, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.MANGLE",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(15, 20), 1),
                Effect::ApplyPower(Target::ChosenEnemy, 132, Amount::fixed(10, 15))
            ],
        ),
        rare_card(
            "CARD.ONE_TWO_PUNCH",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 133, Amount::fixed(1, 2))],
        ),
        rare_card(
            "CARD.PYRE",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 134, Amount::fixed(1, 2))],
        ),
        uncommon_card(
            "CARD.INFERNAL_BLADE",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::RandomCard(Pile::Hand, CardType::Attack, 1, true)],
        ),
        uncommon_card(
            "CARD.RAGE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 135, Amount::fixed(3, 5))],
        ),
        card(
            "CARD.SETUP_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(7, 9), 1),
                Effect::ApplyPower(Target::Player, 0, Amount::fixed(2, 3)),
                Effect::ApplyPower(Target::Player, 136, Amount::fixed(2, 3))
            ],
        )
        .with_tags(STRIKE_TAG),
        uncommon_card(
            "CARD.STAMPEDE",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 137, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.STOKE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Stoke],
        ),
        uncommon_card(
            "CARD.STONE_ARMOR",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 79, Amount::fixed(4, 6))],
        ),
        rare_card(
            "CARD.TANK",
            CardType::Power,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 141, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.UNMOVABLE",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 138, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.UNRELENTING",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(14, 20), 1),
                Effect::ApplyPower(Target::Player, 139, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.VICIOUS",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 140, Amount::fixed(1, 2))],
        ),
        rare_card(
            "CARD.CONSUMING_SHADOW",
            CardType::Power,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Channel(2, 2),
                Effect::If(Condition::Upgraded, effects![Effect::Channel(2, 1)], &[]),
                Effect::ApplyPower(Target::Player, 142, Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.FLAK_CANNON",
            CardType::Attack,
            [2, 2],
            Target::RandomEnemy,
            [0, 0],
            effects![Effect::FlakCannon(Amount::fixed(8, 11))],
        ),
        card(
            "CARD.LIGHTNING_ROD",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(4, 7)),
                Effect::ApplyPower(Target::Player, 143, Amount::fixed(2, 2))
            ],
        ),
        rare_card(
            "CARD.MODDED",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::OrbSlots(1),
                Effect::DrawAmount(Amount::fixed(1, 2)),
                Effect::ReduceCardCost(-1)
            ],
        ),
        rare_card(
            "CARD.VOLTAIC",
            CardType::Skill,
            [3, 3],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::LightningChanneled, 1),
                effects![Effect::Channel(0, 1)]
            )],
        ),
        rare_card(
            "CARD.ALCHEMIZE",
            CardType::Skill,
            [1, 0],
            Target::Player,
            [EXHAUST | NO_GENERATE, EXHAUST | NO_GENERATE],
            effects![Effect::RandomPotion],
        ),
        rare_card(
            "CARD.ANOINTED",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST | RETAIN],
            effects![Effect::RandomCardOp(
                Pile::Draw,
                CardFilter::Rare,
                CardOp::Move(Pile::Hand),
                Amount::fixed(10, 10)
            )],
        ),
        rare_card(
            "CARD.BEAT_DOWN",
            CardType::Skill,
            [3, 3],
            Target::RandomEnemy,
            [0, 0],
            effects![Effect::AutoPlayRandom(
                Pile::Discard,
                CardFilter::Type(CardType::Attack),
                Amount::fixed(3, 4)
            )],
        ),
        uncommon_card(
            "CARD.CATASTROPHE",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::AutoPlayRandom(
                Pile::Draw,
                CardFilter::PlayableOrAny,
                Amount::fixed(2, 3)
            )],
        ),
        uncommon_card(
            "CARD.DARK_SHACKLES",
            CardType::Skill,
            [0, 0],
            Target::ChosenEnemy,
            [EXHAUST, EXHAUST],
            effects![Effect::TemporaryStrength(
                Target::ChosenEnemy,
                power_id::DARK_SHACKLES,
                Amount::fixed(9, 15)
            )],
        ),
        uncommon_card(
            "CARD.DISCOVERY",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::OfferCharacter(3, true)],
        ),
        uncommon_card(
            "CARD.EQUILIBRIUM",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(13, 16)),
                Effect::ApplyPower(Target::Player, 71, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.FISTICUFFS",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(7, 9), 1),
                Effect::Block(Target::Player, Amount::scaled(Scale::LastDamage, 1))
            ],
        ),
        uncommon_card(
            "CARD.FLASH_OF_STEEL",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(5, 8), 1),
                Effect::Draw(1)
            ],
        ),
        rare_card(
            "CARD.GOLD_AXE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, RETAIN],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount {
                    base: -1,
                    upgraded: -1,
                    ascension: 0,
                    scale: Scale::CardsPlayed,
                    multiplier: 1,
                    divisor: 1
                },
                1
            )],
        ),
        rare_card(
            "CARD.HAND_OF_GREED",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [NO_GENERATE, NO_GENERATE],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(20, 25), 1),
                Effect::If(
                    Condition::TargetDead,
                    effects![Effect::Gold(Amount::fixed(20, 25))],
                    &[]
                )
            ],
        ),
        rare_card(
            "CARD.HIDDEN_GEM",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [NO_GENERATE, NO_GENERATE],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::RandomCardOp(
                    Pile::Draw,
                    CardFilter::NoReplay,
                    CardOp::Replay(3),
                    Amount::fixed(1, 1)
                )],
                effects![Effect::RandomCardOp(
                    Pile::Draw,
                    CardFilter::NoReplay,
                    CardOp::Replay(2),
                    Amount::fixed(1, 1)
                )]
            )],
        ),
        uncommon_card(
            "CARD.IMPATIENCE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::HandWithout(CardType::Attack),
                effects![Effect::DrawAmount(Amount::fixed(2, 3))],
                &[]
            )],
        ),
        uncommon_card(
            "CARD.JACK_OF_ALL_TRADES",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::RandomColorlessOther(
                Pile::Hand,
                Amount::fixed(1, 2)
            )],
        ),
        rare_card(
            "CARD.JACKPOT",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(25, 30), 1),
                Effect::RandomCharacterCost0(Pile::Hand, Amount::fixed(3, 3), true)
            ],
        ),
        uncommon_card(
            "CARD.MIND_BLAST",
            CardType::Attack,
            [1, 0],
            Target::ChosenEnemy,
            [INNATE, INNATE],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount::scaled(Scale::DrawSize, 1),
                1
            )],
        ),
        uncommon_card(
            "CARD.OMNISLICE",
            CardType::Attack,
            [0, 0],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::MoveDamage(
                Target::ChosenEnemy,
                Amount::fixed(8, 11)
            )],
        ),
        uncommon_card(
            "CARD.PANIC_BUTTON",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![
                Effect::Block(Target::Player, Amount::fixed(30, 40)),
                Effect::ApplyPower(Target::Player, 144, Amount::fixed(2, 2))
            ],
        ),
        uncommon_card(
            "CARD.PRODUCTION",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Energy(3)],
                effects![Effect::Energy(2)]
            )],
        ),
        uncommon_card(
            "CARD.PROLONG",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::BlockNextTurn(Amount::scaled(Scale::Block, 1))],
        ),
        uncommon_card(
            "CARD.PURITY",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [RETAIN | EXHAUST, RETAIN | EXHAUST],
            effects![Effect::Select(
                Pile::Hand,
                CardFilter::Any,
                [3, 5],
                false,
                true,
                CardOp::Move(Pile::Exhaust)
            )],
        ),
        rare_card(
            "CARD.REND",
            CardType::Attack,
            [2, 2],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 18,
                        upgraded: 18,
                        ascension: 0,
                        scale: Scale::TargetDebuffs,
                        multiplier: 8,
                        divisor: 1
                    },
                    1
                )],
                effects![Effect::Attack(
                    Target::ChosenEnemy,
                    Amount {
                        base: 15,
                        upgraded: 15,
                        ascension: 0,
                        scale: Scale::TargetDebuffs,
                        multiplier: 5,
                        divisor: 1
                    },
                    1
                )]
            )],
        ),
        uncommon_card(
            "CARD.RESTLESSNESS",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [RETAIN, RETAIN],
            effects![Effect::If(
                Condition::HandEmpty,
                effects![
                    Effect::DrawAmount(Amount::fixed(2, 3)),
                    Effect::If(
                        Condition::Upgraded,
                        effects![Effect::Energy(3)],
                        effects![Effect::Energy(2)]
                    )
                ],
                &[]
            )],
        ),
        rare_card(
            "CARD.SALVO",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(12, 16), 1),
                Effect::ApplyPower(Target::Player, 71, Amount::fixed(1, 1))
            ],
        ),
        rare_card(
            "CARD.SCRAWL",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [EXHAUST, EXHAUST | RETAIN],
            effects![Effect::DrawTo([10, 10])],
        ),
        rare_card(
            "CARD.SECRET_TECHNIQUE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::Select(
                Pile::Draw,
                CardFilter::Type(CardType::Skill),
                [1, 1],
                false,
                false,
                CardOp::Move(Pile::Hand)
            )],
        ),
        rare_card(
            "CARD.SECRET_WEAPON",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, 0],
            effects![Effect::Select(
                Pile::Draw,
                CardFilter::Type(CardType::Attack),
                [1, 1],
                false,
                false,
                CardOp::Move(Pile::Hand)
            )],
        ),
        uncommon_card(
            "CARD.SEEKER_STRIKE",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 12), 1),
                Effect::ChooseRandomDraw(3)
            ],
        )
        .with_tags(STRIKE_TAG),
        uncommon_card(
            "CARD.SPLASH",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::OfferOtherCharacter(CardType::Attack, 3, true)],
        ),
        uncommon_card(
            "CARD.THE_BOMB",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![Effect::Bomb(Amount::fixed(40, 50))],
        ),
        rare_card(
            "CARD.THE_GAMBIT",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(50, 75)),
                Effect::ApplyPower(Target::Player, 145, Amount::fixed(1, 1))
            ],
        ),
        uncommon_card(
            "CARD.THINKING_AHEAD",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, 0],
            effects![
                Effect::Draw(2),
                Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Draw)
                )
            ],
        ),
        uncommon_card(
            "CARD.ULTIMATE_DEFEND",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(Target::Player, Amount::fixed(11, 15))],
        )
        .with_tags(DEFEND_TAG),
        uncommon_card(
            "CARD.VOLLEY",
            CardType::Attack,
            [-1, -1],
            Target::RandomEnemy,
            [0, 0],
            effects![Effect::Repeat(
                Amount::scaled(Scale::X, 1),
                effects![Effect::Attack(
                    Target::RandomEnemy,
                    Amount::fixed(10, 14),
                    1
                )]
            )],
        ),
        uncommon_card(
            "CARD.AUTOMATION",
            CardType::Power,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::Automation(1)],
        ),
        rare_card(
            "CARD.CALAMITY",
            CardType::Power,
            [3, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 147, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.ENTROPY",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 148, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.ETERNAL_ARMOR",
            CardType::Power,
            [3, 3],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 79, Amount::fixed(9, 12))],
        ),
        uncommon_card(
            "CARD.FASTEN",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 149, Amount::fixed(4, 6))],
        ),
        rare_card(
            "CARD.MAYHEM",
            CardType::Power,
            [2, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 150, Amount::fixed(1, 1))],
        ),
        rare_card(
            "CARD.NOSTALGIA",
            CardType::Power,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 151, Amount::fixed(1, 1))],
        ),
        uncommon_card(
            "CARD.PANACHE",
            CardType::Power,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![Effect::Panache(Amount::fixed(10, 14))],
        ),
        uncommon_card(
            "CARD.PREP_TIME",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 153, Amount::fixed(4, 6))],
        ),
        uncommon_card(
            "CARD.PROWESS",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, 0, Amount::fixed(1, 2)),
                Effect::ApplyPower(Target::Player, 1, Amount::fixed(1, 2))
            ],
        ),
        rare_card(
            "CARD.ROLLING_BOULDER",
            CardType::Power,
            [3, 3],
            Target::Player,
            [0, 0],
            effects![Effect::RollingBoulder(Amount::fixed(5, 10))],
        ),
        uncommon_card(
            "CARD.STRATAGEM",
            CardType::Power,
            [1, 0],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(Target::Player, 155, Amount::fixed(1, 1))],
        ),
        card(
            "CARD.TORIC_TOUGHNESS",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [0, 0],
            effects![
                Effect::Block(Target::Player, Amount::fixed(5, 7)),
                Effect::ToricToughness(Amount::fixed(5, 7))
            ],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.INFECTION",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE, UNPLAYABLE],
            &[],
        )
        .with_hooks(INFECTION_HOOKS),
        card(
            "CARD.APPARITION",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [ETHEREAL | EXHAUST, EXHAUST],
            effects![Effect::ApplyPower(Target::Player, 5, Amount::fixed(1, 1))],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.WITHER",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE, UNPLAYABLE],
            &[],
        )
        .with_hooks(WITHER_HOOKS),
        card(
            "CARD.FRANTIC_ESCAPE",
            CardType::Status,
            [1, 1],
            Target::Player,
            [NO_GENERATE, NO_GENERATE],
            effects![Effect::FranticEscape, Effect::ReduceCardCost(-1)],
        ),
        card(
            "CARD.MAD_SCIENCE",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![
                Effect::Block(Target::Player, Amount::fixed(8, 8)),
                Effect::Energy(2)
            ],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.BAD_LUCK",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [ETERNAL | UNPLAYABLE | NO_GENERATE; 2],
            &[],
        )
        .with_hooks(BAD_LUCK_HOOKS),
        card(
            "CARD.DEBT",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE; 2],
            &[],
        )
        .with_hooks(DEBT_HOOKS),
        card(
            "CARD.DECAY",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE; 2],
            &[],
        )
        .with_hooks(DECAY_HOOKS),
        card(
            "CARD.ENTHRALLED",
            CardType::Curse,
            [2, 2],
            Target::Player,
            [ETERNAL | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.FOLLY",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | ETERNAL | INNATE | ETHEREAL | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.GREED",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | ETERNAL | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.GUILTY",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE; 2],
            &[],
        ),
        card(
            "CARD.POOR_SLEEP",
            CardType::Curse,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | RETAIN | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.SPORE_MIND",
            CardType::Curse,
            [1, 1],
            Target::Player,
            [EXHAUST | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.BYRDONIS_EGG",
            CardType::Quest,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE; 2],
            &[],
        )
        .with_rarity(CardRarity::Quest),
        card(
            "CARD.ENLIGHTENMENT",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST; 2],
            effects![Effect::CapHandCosts],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.LANTERN_KEY",
            CardType::Quest,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE; 2],
            &[],
        )
        .with_rarity(CardRarity::Quest),
        card(
            "CARD.METAMORPHOSIS",
            CardType::Skill,
            [2, 2],
            Target::Player,
            [EXHAUST; 2],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::RandomCard(Pile::Draw, CardType::Attack, 5, true)],
                effects![Effect::RandomCard(Pile::Draw, CardType::Attack, 3, true)]
            )],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.DUAL_WIELD",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::If(
                Condition::Upgraded,
                effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::AttackOrPower,
                    [1, 1],
                    false,
                    false,
                    CardOp::CopySelected(2)
                )],
                effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::AttackOrPower,
                    [1, 1],
                    false,
                    false,
                    CardOp::CopySelected(1)
                )]
            )],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.HELLO_WORLD",
            CardType::Power,
            [1, 1],
            Target::Player,
            [0, INNATE],
            effects![Effect::ApplyPower(Target::Player, 207, Amount::fixed(1, 1))],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.REBOUND",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(9, 12), 1),
                Effect::ApplyPower(Target::Player, 233, Amount::fixed(1, 1))
            ],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.STACK",
            CardType::Skill,
            [1, 1],
            Target::Player,
            [0, 0],
            effects![Effect::Block(
                Target::Player,
                Amount {
                    base: 0,
                    upgraded: 3,
                    ascension: 0,
                    scale: Scale::DiscardSize,
                    multiplier: 1,
                    divisor: 1,
                }
            )],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.SPOILS_MAP",
            CardType::Quest,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE; 2],
            &[],
        )
        .with_rarity(CardRarity::Quest),
        card(
            "CARD.TOXIC",
            CardType::Status,
            [1, 1],
            Target::Player,
            [EXHAUST | NO_GENERATE; 2],
            &[],
        )
        .with_hooks(TOXIC_HOOKS),
        card(
            "CARD.DISINTEGRATION",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.MIND_ROT",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.SLOTH",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.WASTE_AWAY",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.APOTHEOSIS",
            CardType::Skill,
            [2, 1],
            Target::Player,
            [EXHAUST | INNATE; 2],
            effects![
                Effect::Upgrade(Pile::Draw, u8::MAX, false),
                Effect::Upgrade(Pile::Hand, u8::MAX, false),
                Effect::Upgrade(Pile::Discard, u8::MAX, false),
                Effect::Upgrade(Pile::Exhaust, u8::MAX, false),
            ],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.BECKON",
            CardType::Status,
            [1, 1],
            Target::Player,
            [NO_GENERATE; 2],
            &[],
        )
        .with_hooks(BECKON_HOOKS),
        card(
            "CARD.BRIGHTEST_FLAME",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::Energy(2),
                Effect::If(Condition::Upgraded, effects![Effect::Energy(1)], &[]),
                Effect::Draw(2),
                Effect::If(Condition::Upgraded, effects![Effect::Draw(1)], &[]),
                Effect::MaxHp(Amount::fixed(-1, -1)),
            ],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.CORRUPTION",
            CardType::Power,
            [3, 2],
            Target::Player,
            [0, 0],
            effects![Effect::ApplyPower(
                Target::Player,
                power_id::CORRUPTION,
                Amount::fixed(1, 1),
            )],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.FEEDING_FRENZY",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [0, 0],
            effects![
                Effect::ApplyPower(Target::Player, power_id::STRENGTH, Amount::fixed(5, 7),),
                Effect::ApplyPower(
                    Target::Player,
                    power_id::FEEDING_FRENZY,
                    Amount::fixed(5, 7),
                ),
            ],
        )
        .with_rarity(CardRarity::Event),
        card(
            "CARD.FUEL",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST | NO_GENERATE; 2],
            effects![
                Effect::Energy(1),
                Effect::Draw(1),
                Effect::If(Condition::Upgraded, effects![Effect::Draw(1)], &[]),
            ],
        ),
        card(
            "CARD.LUMINESCE",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST | RETAIN | NO_GENERATE; 2],
            effects![
                Effect::Energy(2),
                Effect::If(Condition::Upgraded, effects![Effect::Energy(1)], &[]),
            ],
        ),
        card(
            "CARD.MAUL",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [0, 0],
            effects![Effect::Attack(
                Target::ChosenEnemy,
                Amount {
                    base: 5,
                    upgraded: 6,
                    ascension: 0,
                    scale: Scale::CardValue,
                    multiplier: 1,
                    divisor: 1,
                },
                2,
            )],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.NEOWS_FURY",
            CardType::Attack,
            [1, 1],
            Target::ChosenEnemy,
            [EXHAUST | NO_GENERATE; 2],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(10, 14), 1),
                Effect::Select(
                    Pile::Discard,
                    CardFilter::Any,
                    [2, 3],
                    false,
                    true,
                    CardOp::Move(Pile::Hand),
                ),
            ],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.RELAX",
            CardType::Skill,
            [3, 3],
            Target::Player,
            [EXHAUST; 2],
            effects![
                Effect::Block(Target::Player, Amount::fixed(15, 17)),
                Effect::ApplyPower(
                    Target::Player,
                    power_id::DRAW_CARDS_NEXT_TURN,
                    Amount::fixed(2, 3),
                ),
                Effect::ApplyPower(Target::Player, 22, Amount::fixed(2, 3)),
            ],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.SOOT",
            CardType::Status,
            [-1, -1],
            Target::Player,
            [UNPLAYABLE | NO_GENERATE; 2],
            &[],
        ),
        card(
            "CARD.WHISTLE",
            CardType::Attack,
            [3, 3],
            Target::ChosenEnemy,
            [EXHAUST; 2],
            effects![
                Effect::Attack(Target::ChosenEnemy, Amount::fixed(33, 44), 1),
                Effect::Stun(Target::ChosenEnemy),
            ],
        )
        .with_rarity(CardRarity::Ancient),
        card(
            "CARD.WISH",
            CardType::Skill,
            [0, 0],
            Target::Player,
            [EXHAUST, EXHAUST | RETAIN],
            effects![Effect::Select(
                Pile::Draw,
                CardFilter::Any,
                [1, 1],
                false,
                false,
                CardOp::Move(Pile::Hand),
            )],
        )
        .with_rarity(CardRarity::Ancient),
    ];
    let colorless = COLORLESS_CARDS
        .iter()
        .filter_map(|id| {
            cards
                .iter()
                .position(|card| card.id == *id)
                .map(|id| id as Id)
        })
        .collect();
    let content = Content {
        cards,
        powers: vec![
            PowerDef {
                id: "POWER.STRENGTH_POWER",
                kind: PowerKind::Strength,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DEXTERITY_POWER",
                kind: PowerKind::Dexterity,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.WEAK_POWER",
                kind: PowerKind::Weak,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.VULNERABLE_POWER",
                kind: PowerKind::Vulnerable,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FRAIL_POWER",
                kind: PowerKind::Frail,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.INTANGIBLE_POWER",
                kind: PowerKind::Intangible,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ARTIFACT_POWER",
                kind: PowerKind::Artifact,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.POISON_POWER",
                kind: PowerKind::Poison,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.THORNS_POWER",
                kind: PowerKind::Thorns,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.NO_DRAW_POWER",
                kind: PowerKind::NoDraw,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FEEL_NO_PAIN_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: FEEL_NO_PAIN_HOOKS,
            },
            PowerDef {
                id: "POWER.DARK_EMBRACE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: DARK_EMBRACE_HOOKS,
            },
            PowerDef {
                id: "POWER.RUPTURE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: RUPTURE_HOOKS,
            },
            PowerDef {
                id: "POWER.JUGGERNAUT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: JUGGERNAUT_HOOKS,
            },
            PowerDef {
                id: "POWER.DEMON_FORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: DEMON_FORM_HOOKS,
            },
            PowerDef {
                id: "POWER.BARRICADE_POWER",
                kind: PowerKind::BlockRetain,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MINION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FOCUS_POWER",
                kind: PowerKind::Focus,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MACHINE_LEARNING_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: MACHINE_LEARNING_HOOKS,
            },
            PowerDef {
                id: "POWER.NOXIOUS_FUMES_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: NOXIOUS_FUMES_HOOKS,
            },
            PowerDef {
                id: "POWER.INFINITE_BLADES_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: INFINITE_BLADES_HOOKS,
            },
            PowerDef {
                id: "POWER.ACCURACY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ENERGY_NEXT_TURN_POWER",
                kind: PowerKind::OneShot(Trigger::TurnStart),
                debuff: false,
                hooks: ENERGY_NEXT_TURN_HOOKS,
            },
            PowerDef {
                id: "POWER.TEMPORARY_FOCUS_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: TEMPORARY_FOCUS_HOOKS,
            },
            PowerDef {
                id: "POWER.BIASED_COGNITION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: BIASED_COGNITION_HOOKS,
            },
            PowerDef {
                id: "POWER.COOLANT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: COOLANT_HOOKS,
            },
            PowerDef {
                id: "POWER.HAILSTORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: HAILSTORM_HOOKS,
            },
            PowerDef {
                id: "POWER.LOOP_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: LOOP_HOOKS,
            },
            PowerDef {
                id: "POWER.SPINNER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: SPINNER_HOOKS,
            },
            PowerDef {
                id: "POWER.STORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: STORM_HOOKS,
            },
            PowerDef {
                id: "POWER.SUBROUTINE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: SUBROUTINE_HOOKS,
            },
            PowerDef {
                id: "POWER.ITERATION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: ITERATION_HOOKS,
            },
            PowerDef {
                id: "POWER.BUFFER_POWER",
                kind: PowerKind::Buffer,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FERAL_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SMOKESTACK_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: SMOKESTACK_HOOKS,
            },
            PowerDef {
                id: "POWER.FREE_POWER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CREATIVE_AI_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: CREATIVE_AI_HOOKS,
            },
            PowerDef {
                id: "POWER.TRASH_TO_TREASURE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: TRASH_TO_TREASURE_HOOKS,
            },
            PowerDef {
                id: "POWER.THUNDER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SIGNAL_BOOST_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ECHO_FORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.BLOCK_NEXT_TURN_POWER",
                kind: PowerKind::OneShot(Trigger::TurnStart),
                debuff: false,
                hooks: BLOCK_NEXT_TURN_HOOKS,
            },
            PowerDef {
                id: "POWER.RESTORE_STRENGTH_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: RESTORE_STRENGTH_HOOKS,
            },
            PowerDef {
                id: "POWER.DRAW_NEXT_TURN_POWER",
                kind: PowerKind::OneShot(Trigger::TurnStart),
                debuff: false,
                hooks: DRAW_NEXT_TURN_HOOKS,
            },
            PowerDef {
                id: "POWER.FREE_SKILL_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.BLUR_POWER",
                kind: PowerKind::BlockRetainOnce,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.AFTERIMAGE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: AFTERIMAGE_HOOKS,
            },
            PowerDef {
                id: "POWER.BURST_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CORROSIVE_WAVE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: CORROSIVE_WAVE_HOOKS,
            },
            PowerDef {
                id: "POWER.SHADOWMELD_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SHADOW_STEP_POWER",
                kind: PowerKind::OneShot(Trigger::TurnStart),
                debuff: false,
                hooks: SHADOW_STEP_HOOKS,
            },
            PowerDef {
                id: "POWER.DOUBLE_DAMAGE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.WRAITH_FORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: WRAITH_FORM_HOOKS,
            },
            PowerDef {
                id: "POWER.SERPENT_FORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: SERPENT_FORM_HOOKS,
            },
            PowerDef {
                id: "POWER.TEMPORARY_DEXTERITY_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: RESTORE_DEXTERITY_HOOKS,
            },
            PowerDef {
                id: "POWER.STRANGLE_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: STRANGLE_HOOKS,
            },
            PowerDef {
                id: "POWER.THE_HUNT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ACCELERANT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ENVENOM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FAN_OF_KNIVES_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TOOLS_OF_THE_TRADE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MASTER_PLANNER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PHANTOM_BLADES_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SPEEDSTER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: SPEEDSTER_HOOKS,
            },
            PowerDef {
                id: "POWER.TRACKING_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.OUTBREAK_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.WELL_LAID_PLANS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ARSENAL_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: ARSENAL_HOOKS,
            },
            PowerDef {
                id: "POWER.BLACK_HOLE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CHILD_OF_THE_STARS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CONQUEROR_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.RETAIN_HAND_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.STAR_NEXT_TURN_POWER",
                kind: PowerKind::OneShot(Trigger::TurnStart),
                debuff: false,
                hooks: STAR_NEXT_TURN_HOOKS,
            },
            PowerDef {
                id: "POWER.FOREGONE_CONCLUSION_POWER",
                kind: PowerKind::OneShot(Trigger::TurnStart),
                debuff: false,
                hooks: FOREGONE_CONCLUSION_HOOKS,
            },
            PowerDef {
                id: "POWER.FURNACE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: FURNACE_HOOKS,
            },
            PowerDef {
                id: "POWER.GENESIS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: GENESIS_HOOKS,
            },
            PowerDef {
                id: "POWER.MONARCHS_GAZE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MONOLOGUE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: MONOLOGUE_HOOKS,
            },
            PowerDef {
                id: "POWER.MONOLOGUE_RESTORE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: MONOLOGUE_RESTORE_HOOKS,
            },
            PowerDef {
                id: "POWER.PLATING_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: PLATING_HOOKS,
            },
            PowerDef {
                id: "POWER.ORBIT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PALE_BLUE_DOT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PARRY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.VIGOR_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PILLAR_OF_CREATION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: PILLAR_OF_CREATION_HOOKS,
            },
            PowerDef {
                id: "POWER.REFLECT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: REFLECT_HOOKS,
            },
            PowerDef {
                id: "POWER.ROYALTIES_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SEEKING_EDGE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.RESERVED_88",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SWORD_SAGE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.THE_SEALED_THRONE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TYRANNY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.VOID_FORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SPECTRUM_SHIFT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: SPECTRUM_SHIFT_HOOKS,
            },
            PowerDef {
                id: "POWER.DOOM_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.BORROWED_TIME_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CALCIFY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CALL_OF_THE_VOID_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: CALL_OF_THE_VOID_HOOKS,
            },
            PowerDef {
                id: "POWER.COUNTDOWN_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: COUNTDOWN_HOOKS,
            },
            PowerDef {
                id: "POWER.DANSE_MACABRE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DEBILITATE_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DEMESNE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DEVOUR_LIFE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: DEVOUR_LIFE_HOOKS,
            },
            PowerDef {
                id: "POWER.ENFEEBLING_TOUCH_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: true,
                hooks: ENFEEBLING_TOUCH_HOOKS,
            },
            PowerDef {
                id: "POWER.FORBIDDEN_GRIMOIRE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FRIENDSHIP_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HANG_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HAUNT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: HAUNT_HOOKS,
            },
            PowerDef {
                id: "POWER.SUMMON_NEXT_TURN_POWER",
                kind: PowerKind::OneShot(Trigger::TurnStart),
                debuff: false,
                hooks: SUMMON_NEXT_TURN_HOOKS,
            },
            PowerDef {
                id: "POWER.LETHALITY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.NECRO_MASTERY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.NEUROSURGE_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: NEUROSURGE_HOOKS,
            },
            PowerDef {
                id: "POWER.OBLIVION_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: OBLIVION_HOOKS,
            },
            PowerDef {
                id: "POWER.PAGESTORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.REAPER_FORM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SENTRY_MODE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: SENTRY_MODE_HOOKS,
            },
            PowerDef {
                id: "POWER.SHROUD_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SIC_EM_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SLEIGHT_OF_FLESH_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SPIRIT_OF_ASH_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.VEILPIERCER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.AGGRESSION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: AGGRESSION_HOOKS,
            },
            PowerDef {
                id: "POWER.COLOSSUS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CRIMSON_MANTLE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: CRIMSON_MANTLE_HOOKS,
            },
            PowerDef {
                id: "POWER.CRIMSON_MANTLE_DAMAGE",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CRUELTY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.NO_ENERGY_GAIN_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FLAME_BARRIER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HELLRAISER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.INFERNO_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: INFERNO_HOOKS,
            },
            PowerDef {
                id: "POWER.INFERNO_DAMAGE",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.JUGGLING_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MANGLE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: true,
                hooks: MANGLE_HOOKS,
            },
            PowerDef {
                id: "POWER.ONE_TWO_PUNCH_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PYRE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.RAGE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: RAGE_HOOKS,
            },
            PowerDef {
                id: "POWER.SETUP_STRIKE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: SETUP_STRIKE_HOOKS,
            },
            PowerDef {
                id: "POWER.STAMPEDE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.UNMOVABLE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FREE_ATTACK_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.VICIOUS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TANK_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CONSUMING_SHADOW_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: CONSUMING_SHADOW_HOOKS,
            },
            PowerDef {
                id: "POWER.LIGHTNING_ROD_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: LIGHTNING_ROD_HOOKS,
            },
            PowerDef {
                id: "POWER.NO_BLOCK_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: NO_BLOCK_HOOKS,
            },
            PowerDef {
                id: "POWER.THE_GAMBIT_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.AUTOMATION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CALAMITY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: CALAMITY_HOOKS,
            },
            PowerDef {
                id: "POWER.ENTROPY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: ENTROPY_HOOKS,
            },
            PowerDef {
                id: "POWER.FASTEN_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MAYHEM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: MAYHEM_HOOKS,
            },
            PowerDef {
                id: "POWER.NOSTALGIA_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PANACHE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PREP_TIME_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: PREP_TIME_HOOKS,
            },
            PowerDef {
                id: "POWER.ROLLING_BOULDER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.STRATAGEM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: STRATAGEM_HOOKS,
            },
            PowerDef {
                id: "POWER.SHRINK_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CLARITY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DUPLICATION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.GIGANTIFICATION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.RITUAL_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: RITUAL_HOOKS,
            },
            PowerDef {
                id: "POWER.DEMISE_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: DEMISE_HOOKS,
            },
            PowerDef {
                id: "POWER.RADIANCE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: RADIANCE_HOOKS,
            },
            PowerDef {
                id: "POWER.REGEN_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: REGEN_HOOKS,
            },
            PowerDef {
                id: "POWER.ADAPTABLE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ANTICIPATE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ASLEEP_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.BACK_ATTACK_LEFT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.BEACON_OF_HOPE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.BLADE_OF_INK_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.BURROWED_POWER",
                kind: PowerKind::BlockRetain,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CHAINS_OF_BINDING_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CONFUSED_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CONSTRICT_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: CONSTRICT_HOOKS,
            },
            PowerDef {
                id: "POWER.COORDINATE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CORRUPTION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.COVERED_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CRAB_RAGE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CRUSH_UNDER_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: CRUSH_UNDER_HOOKS,
            },
            PowerDef {
                id: "POWER.CURIOUS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.CURL_UP_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DAMPEN_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DARK_SHACKLES_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: DARK_SHACKLES_HOOKS,
            },
            PowerDef {
                id: "POWER.DIAMOND_DIADEM_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DIE_FOR_YOU_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DISINTEGRATION_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DOOR_REVIVAL_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DRAW_CARDS_NEXT_TURN_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DRUM_OF_BATTLE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.DYING_STAR_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: DYING_STAR_HOOKS,
            },
            PowerDef {
                id: "POWER.ENRAGE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ESCAPE_ARTIST_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FEEDING_FRENZY_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: FEEDING_FRENZY_HOOKS,
            },
            PowerDef {
                id: "POWER.FLANKING_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FLEX_POTION_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: FLEX_POTION_HOOKS,
            },
            PowerDef {
                id: "POWER.FLUTTER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.FOCUSED_STRIKE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.GALVANIC_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.GRAPPLE_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.GRAVITY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.GUARDED_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HAMMER_TIME_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HARDENED_SHELL_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HARD_TO_KILL_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HATCH_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HEIST_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HELICAL_DART_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: HELICAL_DART_HOOKS,
            },
            PowerDef {
                id: "POWER.HELLO_WORLD_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HEX_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.HIGH_VOLTAGE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: HIGH_VOLTAGE_HOOKS,
            },
            PowerDef {
                id: "POWER.HOTFIX_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.ILLUSION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.IMBALANCED_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.IMPROVEMENT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.INFESTED_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.INTERCEPT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.KNOCKDOWN_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.LEADERSHIP_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MAGIC_BOMB_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MIND_ROT_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.MONARCHS_GAZE_STRENGTH_DOWN_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.NEMESIS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.NIGHTMARE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PAINFUL_STABS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PAPER_CUTS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PERSONAL_HIVE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.PIERCING_WAIL_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: PIERCING_WAIL_HOOKS,
            },
            PowerDef {
                id: "POWER.PLOW_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.POSSESS_SPEED_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.POSSESS_STRENGTH_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.RAMPART_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: RAMPART_HOOKS,
            },
            PowerDef {
                id: "POWER.RAVENOUS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.REATTACH_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.REBOUND_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.REPTILE_TRINKET_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: REPTILE_TRINKET_HOOKS,
            },
            PowerDef {
                id: "POWER.RINGING_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SANDPIT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: SANDPIT_HOOKS,
            },
            PowerDef {
                id: "POWER.SELF_FORMING_CLAY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SHACKLING_POTION_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: SHACKLING_POTION_HOOKS,
            },
            PowerDef {
                id: "POWER.SHRIEK_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SKITTISH_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SLIPPERY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SLOTH_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SLOW_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: SLOW_HOOKS,
            },
            PowerDef {
                id: "POWER.SLUMBER_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SMOGGY_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SNEAKY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SOAR_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SPEED_POTION_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: SPEED_POTION_HOOKS,
            },
            PowerDef {
                id: "POWER.STEAM_ERUPTION_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.STOCK_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SUCK_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SURPRISE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SURROUNDED_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SWIPE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.SYNCHRONIZE_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TAG_TEAM_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TANGLED_POWER",
                kind: PowerKind::OneShot(Trigger::TurnEnd),
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TEMPORARY_STRENGTH_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TENDER_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TERRITORIAL_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: TERRITORIAL_HOOKS,
            },
            PowerDef {
                id: "POWER.THE_BOMB_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.THIEVERY_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TIME_LIMIT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TORIC_TOUGHNESS_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.VITAL_SPARK_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: VITAL_SPARK_HOOKS,
            },
            PowerDef {
                id: "POWER.WASTE_AWAY_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.TAINTED_POWER",
                kind: PowerKind::Other,
                debuff: true,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.WITHERING_PRESENCE_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
            PowerDef {
                id: "POWER.BACK_ATTACK_RIGHT_POWER",
                kind: PowerKind::Other,
                debuff: false,
                hooks: EMPTY_HOOKS,
            },
        ],
        relics: vec![
            RelicDef {
                id: "RELIC.BURNING_BLOOD",
                hooks: BURNING_BLOOD_HOOKS,
            },
            RelicDef {
                id: "RELIC.ANCHOR",
                hooks: ANCHOR_HOOKS,
            },
            RelicDef {
                id: "RELIC.BAG_OF_PREPARATION",
                hooks: BAG_HOOKS,
            },
            RelicDef {
                id: "RELIC.LANTERN",
                hooks: LANTERN_HOOKS,
            },
            RelicDef {
                id: "RELIC.VAJRA",
                hooks: VAJRA_HOOKS,
            },
            RelicDef {
                id: "RELIC.ODDLY_SMOOTH_STONE",
                hooks: STONE_HOOKS,
            },
            RelicDef {
                id: "RELIC.CRACKED_CORE",
                hooks: CRACKED_CORE_HOOKS,
            },
            RelicDef {
                id: "RELIC.RING_OF_THE_SNAKE",
                hooks: BAG_HOOKS,
            },
            RelicDef {
                id: "RELIC.DATA_DISK",
                hooks: DATA_DISK_HOOKS,
            },
            RelicDef {
                id: "RELIC.GOLD_PLATED_CABLES",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RUNIC_CAPACITOR",
                hooks: RUNIC_CAPACITOR_HOOKS,
            },
            RelicDef {
                id: "RELIC.SYMBIOTIC_VIRUS",
                hooks: SYMBIOTIC_VIRUS_HOOKS,
            },
            RelicDef {
                id: "RELIC.BAG_OF_MARBLES",
                hooks: BAG_OF_MARBLES_HOOKS,
            },
            RelicDef {
                id: "RELIC.BLOOD_VIAL",
                hooks: BLOOD_VIAL_HOOKS,
            },
            RelicDef {
                id: "RELIC.BRONZE_SCALES",
                hooks: BRONZE_SCALES_HOOKS,
            },
            RelicDef {
                id: "RELIC.MERCURY_HOURGLASS",
                hooks: MERCURY_HOURGLASS_HOOKS,
            },
            RelicDef {
                id: "RELIC.RED_MASK",
                hooks: RED_MASK_HOOKS,
            },
            RelicDef {
                id: "RELIC.DIVINE_RIGHT",
                hooks: DIVINE_RIGHT_HOOKS,
            },
            RelicDef {
                id: "RELIC.BOUND_PHYLACTERY",
                hooks: BOUND_PHYLACTERY_HOOKS,
            },
            RelicDef {
                id: "RELIC.AKABEKO",
                hooks: AKABEKO_HOOKS,
            },
            RelicDef {
                id: "RELIC.ALCHEMICAL_COFFER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.AMETHYST_AUBERGINE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ARCANE_SCROLL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ARCHAIC_TOOTH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ART_OF_WAR",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ASTROLABE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BEATING_REMNANT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BEAUTIFUL_BRACELET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BELLOWS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BELT_BUCKLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BIG_HAT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BIG_MUSHROOM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BIIIG_HUG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BING_BONG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BLACK_BLOOD",
                hooks: BLACK_BLOOD_HOOKS,
            },
            RelicDef {
                id: "RELIC.BLACK_STAR",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BLESSED_ANTLER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BLOOD_SOAKED_ROSE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BONE_FLUTE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BONE_TEA",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BOOK_OF_FIVE_RINGS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BOOK_REPAIR_KNIFE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BOOKMARK",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BOOMING_CONCH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BOWLER_HAT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BREAD",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BRILLIANT_SCARF",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BRIMSTONE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BURNING_STICKS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.BYRDPIP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CALLING_BELL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CANDELABRA",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CAPTAINS_WHEEL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CAULDRON",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CENTENNIAL_PUZZLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CHANDELIER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CHARON_S_ASHES",
                hooks: CHARONS_ASHES_HOOKS,
            },
            RelicDef {
                id: "RELIC.CHEMICAL_X",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CHOICES_PARADOX",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CIRCLET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CLAWS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CLOAK_CLASP",
                hooks: CLOAK_CLASP_HOOKS,
            },
            RelicDef {
                id: "RELIC.CROSSBOW",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CURSED_PEARL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DARKSTONE_PERIAPT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DAUGHTER_OF_THE_WIND",
                hooks: DAUGHTER_WIND_HOOKS,
            },
            RelicDef {
                id: "RELIC.DELICATE_FROND",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DEMON_TONGUE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DIAMOND_DIADEM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DINGY_RUG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DISTINGUISHED_CAPE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DIVINE_DESTINY",
                hooks: DIVINE_DESTINY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DOLLYS_MIRROR",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DRAGON_FRUIT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DREAM_CATCHER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DRIFTWOOD",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.DUSTY_TOME",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ECTOPLASM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ELECTRIC_SHRYMP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.EMBER_TEA",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.EMOTION_CHIP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.EMPTY_CAGE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ETERNAL_FEATHER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FENCING_MANUAL",
                hooks: FENCING_MANUAL_HOOKS,
            },
            RelicDef {
                id: "RELIC.FESTIVE_POPPER",
                hooks: FESTIVE_POPPER_HOOKS,
            },
            RelicDef {
                id: "RELIC.FIDDLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FORGOTTEN_SOUL",
                hooks: FORGOTTEN_SOUL_HOOKS,
            },
            RelicDef {
                id: "RELIC.FRAGRANT_MUSHROOM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FRESNEL_LENS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FROZEN_EGG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FUNERARY_MASK",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FUR_COAT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GALACTIC_DUST",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GAMBLING_CHIP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GAME_PIECE",
                hooks: &[Hook {
                    trigger: Trigger::PowerPlayed,
                    effects: &[Effect::Draw(1)],
                }],
            },
            RelicDef {
                id: "RELIC.GHOST_SEED",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GIRYA",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GLASS_EYE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GLITTER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GNARLED_HAMMER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GOLDEN_COMPASS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GOLDEN_PEARL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GORGET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.GREMLIN_HORN",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.HAND_DRILL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.HAPPY_FLOWER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.HELICAL_DART",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.HISTORY_COURSE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.HORN_CLEAT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ICE_CREAM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.INFUSED_CORE",
                hooks: INFUSED_CORE_HOOKS,
            },
            RelicDef {
                id: "RELIC.INK_BOTTLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.INTIMIDATING_HELMET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.IRON_CLUB",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.IVORY_TILE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.JEWELED_MASK",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.JEWELRY_BOX",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.JOSS_PAPER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.JUZU_BRACELET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.KIFUDA",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.KUNAI",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.KUSARIGAMA",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LARGE_CAPSULE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LASTING_CANDY",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LAVA_LAMP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LAVA_ROCK",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LEAD_PAPERWEIGHT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LEAFY_POULTICE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LEES_WAFFLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LETTER_OPENER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LIZARD_TAIL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LOOMING_FRUIT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LORDS_PARASOL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LOST_COFFER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LOST_WISP",
                hooks: LOST_WISP_HOOKS,
            },
            RelicDef {
                id: "RELIC.LUCKY_FYSH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.LUNAR_PASTRY",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MANGO",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MASSIVE_SCROLL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MAW_BANK",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MEAL_TICKET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MEAT_CLEAVER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MEAT_ON_THE_BONE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MEDICAL_KIT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MEMBERSHIP_CARD",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.METRONOME",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MINI_REGENT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MINIATURE_CANNON",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MINIATURE_TENT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MOLTEN_EGG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MR_STRUGGLES",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MUMMIFIED_HAND",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MUSIC_BOX",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MYSTERIOUS_COCOON",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.MYSTIC_LIGHTER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.NEOW_S_TORMENT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.NEW_LEAF",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.NINJA_SCROLL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.NUNCHAKU",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.NUTRITIOUS_OYSTER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.NUTRITIOUS_SOUP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.OLD_COIN",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ORANGE_DOUGH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ORICHALCUM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ORNAMENTAL_FAN",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ORRERY",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_BLOOD",
                hooks: &[Hook {
                    trigger: Trigger::TurnStart,
                    effects: &[Effect::Draw(1)],
                }],
            },
            RelicDef {
                id: "RELIC.PAELS_CLAW",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_EYE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_FLESH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_GROWTH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_HORN",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_LEGION",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_TEARS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_TOOTH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAELS_WING",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PANDORAS_BOX",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PANTOGRAPH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAPER_KRANE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PAPER_PHROG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PARRYING_SHIELD",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PEAR",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PEN_NIB",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PENDULUM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PERMAFROST",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PETRIFIED_TOAD",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PHILOSOPHERS_STONE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PHYLACTERY_UNBOUND",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PLANISPHERE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.POCKETWATCH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.POLLINOUS_CORE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.POMANDER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.POTION_BELT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.POWER_CELL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PRAYER_WHEEL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PRECARIOUS_SHEARS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PRECISE_SCISSORS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PRESERVED_FOG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PRISMATIC_GEM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PUMPKIN_CANDLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PUNCH_DAGGER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RADIANT_PEARL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RAINBOW_RING",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RAZOR_TOOTH",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RED_SKULL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.REGAL_PILLOW",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.REGALITE",
                hooks: REGALITE_HOOKS,
            },
            RelicDef {
                id: "RELIC.REPTILE_TRINKET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RING_OF_THE_DRAKE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RINGING_TRIANGLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RIPPLE_BASIN",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ROYAL_POISON",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.ROYAL_STAMP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RUINED_HELMET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.RUNIC_PYRAMID",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SAI",
                hooks: SAI_HOOKS,
            },
            RelicDef {
                id: "RELIC.SAND_CASTLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SCREAMING_FLAGON",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SCROLL_BOXES",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SEA_GLASS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SEAL_OF_GOLD",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SELF_FORMING_CLAY",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SERE_TALON",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SHOVEL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SHURIKEN",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SIGNET_RING",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SILVER_CRUCIBLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SLING_OF_COURAGE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SMALL_CAPSULE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SNECKO_EYE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SNECKO_SKULL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SOZU",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SPARKLING_ROUGE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SPIKED_GAUNTLETS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.STONE_CALENDAR",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.STONE_CRACKER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.STONE_HUMIDIFIER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.STORYBOOK",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.STRAWBERRY",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.STRIKE_DUMMY",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.STURDY_CLAMP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SWORD_OF_JADE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SWORD_OF_STONE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TANXS_WHISTLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TEA_OF_DISCOURTESY",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.THE_ABACUS",
                hooks: ABACUS_HOOKS,
            },
            RelicDef {
                id: "RELIC.THE_BOOT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.THE_COURIER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.THROWING_AXE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TINGSHA",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TINY_MAILBOX",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TOASTY_MITTENS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TOOLBOX",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TOUCH_OF_OROBAS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TOUGH_BANDAGES",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TOXIC_EGG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TOY_BOX",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TRI_BOOMERANG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TUNGSTEN_ROD",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TUNING_FORK",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.TWISTED_FUNNEL",
                hooks: TWISTED_FUNNEL_HOOKS,
            },
            RelicDef {
                id: "RELIC.UNCEASING_TOP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.UNDYING_SIGIL",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.UNSETTLING_LAMP",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.VAMBRACE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.VELVET_CHOKER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.VENERABLE_TEA_SET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.VERY_HOT_COCOA",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.VEXING_PUZZLEBOX",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.VITRUVIAN_MINION",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WAR_HAMMER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WAR_PAINT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WHETSTONE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WHISPERING_EARRING",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WHITE_BEAST_STATUE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WHITE_STAR",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WING_CHARM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.YUMMY_COOKIE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CHOSEN_CHEESE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.CHARONS_ASHES",
                hooks: CHARONS_ASHES_HOOKS,
            },
            RelicDef {
                id: "RELIC.NEOWS_BONES",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WONGOS_MYSTERY_TICKET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FISHING_ROD",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.HEFTY_TABLET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.KALEIDOSCOPE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.NEOWS_TALISMAN",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.NEOWS_TORMENT",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.PHIAL_HOLSTER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.SILKEN_TRESS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WINGED_BOOTS",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_ANCHOR",
                hooks: FAKE_ANCHOR_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_BLOOD_VIAL",
                hooks: FAKE_BLOOD_VIAL_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_HAPPY_FLOWER",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_LEES_WAFFLE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_MANGO",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_ORICHALCUM",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_SNECKO_EYE",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_STRIKE_DUMMY",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_VENERABLE_TEA_SET",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.FAKE_MERCHANTS_RUG",
                hooks: EMPTY_HOOKS,
            },
            RelicDef {
                id: "RELIC.WONGO_CUSTOMER_APPRECIATION_BADGE",
                hooks: EMPTY_HOOKS,
            },
        ],
        potions: vec![
            PotionDef {
                id: "POTION.STRENGTH_POTION",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 0, Amount::fixed(2, 2))],
            },
            PotionDef {
                id: "POTION.DEXTERITY_POTION",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 1, Amount::fixed(2, 2))],
            },
            PotionDef {
                id: "POTION.BLOCK_POTION",
                target: Target::Player,
                effects: effects![Effect::RawBlock(Target::Player, Amount::fixed(12, 12))],
            },
            PotionDef {
                id: "POTION.ENERGY_POTION",
                target: Target::Player,
                effects: effects![Effect::Energy(2)],
            },
            PotionDef {
                id: "POTION.SWIFT_POTION",
                target: Target::Player,
                effects: effects![Effect::Draw(3)],
            },
            PotionDef {
                id: "POTION.WEAK_POTION",
                target: Target::ChosenEnemy,
                effects: effects![Effect::ApplyPower(
                    Target::ChosenEnemy,
                    2,
                    Amount::fixed(3, 3),
                )],
            },
            PotionDef {
                id: "POTION.VULNERABLE_POTION",
                target: Target::ChosenEnemy,
                effects: effects![Effect::ApplyPower(
                    Target::ChosenEnemy,
                    3,
                    Amount::fixed(3, 3),
                )],
            },
            PotionDef {
                id: "POTION.FIRE_POTION",
                target: Target::ChosenEnemy,
                effects: effects![Effect::Damage(Target::ChosenEnemy, Amount::fixed(20, 20))],
            },
            PotionDef {
                id: "POTION.EXPLOSIVE_AMPOULE",
                target: Target::AllEnemies,
                effects: effects![Effect::Damage(Target::AllEnemies, Amount::fixed(10, 10))],
            },
            PotionDef {
                id: "POTION.POTION_OF_BINDING",
                target: Target::AllEnemies,
                effects: effects![
                    Effect::ApplyPower(Target::AllEnemies, 2, Amount::fixed(1, 1)),
                    Effect::ApplyPower(Target::AllEnemies, 3, Amount::fixed(1, 1)),
                ],
            },
            PotionDef {
                id: "POTION.ATTACK_POTION",
                target: Target::Player,
                effects: effects![Effect::OfferCharacterType(CardType::Attack, 3)],
            },
            PotionDef {
                id: "POTION.BEETLE_JUICE",
                target: Target::ChosenEnemy,
                effects: effects![Effect::ApplyPower(
                    Target::ChosenEnemy,
                    156,
                    Amount::fixed(4, 4),
                )],
            },
            PotionDef {
                id: "POTION.BLESSING_OF_THE_FORGE",
                target: Target::Player,
                effects: effects![Effect::Upgrade(Pile::Hand, u8::MAX, true)],
            },
            PotionDef {
                id: "POTION.BOTTLED_POTENTIAL",
                target: Target::Player,
                effects: effects![Effect::ShuffleHandDraw(5)],
            },
            PotionDef {
                id: "POTION.CLARITY",
                target: Target::Player,
                effects: effects![
                    Effect::Draw(1),
                    Effect::ApplyPower(Target::Player, 157, Amount::fixed(3, 3)),
                ],
            },
            PotionDef {
                id: "POTION.COLORLESS_POTION",
                target: Target::Player,
                effects: effects![Effect::OfferColorless(3, false, true)],
            },
            PotionDef {
                id: "POTION.CURE_ALL",
                target: Target::Player,
                effects: effects![Effect::Energy(1), Effect::Draw(2)],
            },
            PotionDef {
                id: "POTION.DISTILLED_CHAOS",
                target: Target::Player,
                effects: effects![Effect::AutoPlayDraw(Amount::fixed(3, 3), false)],
            },
            PotionDef {
                id: "POTION.DROPLET_OF_PRECOGNITION",
                target: Target::Player,
                effects: effects![Effect::Select(
                    Pile::Draw,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::Move(Pile::Hand),
                )],
            },
            PotionDef {
                id: "POTION.DUPLICATOR",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 158, Amount::fixed(1, 1))],
            },
            PotionDef {
                id: "POTION.ENTROPIC_BREW",
                target: Target::Player,
                effects: effects![Effect::FillPotions],
            },
            PotionDef {
                id: "POTION.FAIRY_IN_A_BOTTLE",
                target: Target::Player,
                effects: &[],
            },
            PotionDef {
                id: "POTION.FLEX_POTION",
                target: Target::Player,
                effects: effects![
                    Effect::ApplyPower(Target::Player, 0, Amount::fixed(5, 5)),
                    Effect::ApplyPower(Target::Player, power_id::FLEX_POTION, Amount::fixed(5, 5),),
                ],
            },
            PotionDef {
                id: "POTION.FORTIFIER",
                target: Target::Player,
                effects: effects![Effect::RawBlock(
                    Target::Player,
                    Amount::scaled(Scale::Block, 2),
                )],
            },
            PotionDef {
                id: "POTION.FRUIT_JUICE",
                target: Target::Player,
                effects: effects![Effect::MaxHp(Amount::fixed(5, 5))],
            },
            PotionDef {
                id: "POTION.FYSH_OIL",
                target: Target::Player,
                effects: effects![
                    Effect::ApplyPower(Target::Player, 0, Amount::fixed(1, 1)),
                    Effect::ApplyPower(Target::Player, 1, Amount::fixed(1, 1)),
                ],
            },
            PotionDef {
                id: "POTION.GAMBLERS_BREW",
                target: Target::Player,
                effects: effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [u8::MAX, u8::MAX],
                    false,
                    true,
                    CardOp::DiscardDraw,
                )],
            },
            PotionDef {
                id: "POTION.GIGANTIFICATION_POTION",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 159, Amount::fixed(1, 1))],
            },
            PotionDef {
                id: "POTION.HEART_OF_IRON",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 79, Amount::fixed(7, 7))],
            },
            PotionDef {
                id: "POTION.LIQUID_BRONZE",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 8, Amount::fixed(3, 3))],
            },
            PotionDef {
                id: "POTION.LIQUID_MEMORIES",
                target: Target::Player,
                effects: effects![Effect::Select(
                    Pile::Discard,
                    CardFilter::Any,
                    [1, 1],
                    false,
                    false,
                    CardOp::MoveFree(Pile::Hand),
                )],
            },
            PotionDef {
                id: "POTION.LUCKY_TONIC",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 32, Amount::fixed(1, 1))],
            },
            PotionDef {
                id: "POTION.MAZALETHS_GIFT",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 160, Amount::fixed(1, 1))],
            },
            PotionDef {
                id: "POTION.OROBIC_ACID",
                target: Target::Player,
                effects: effects![
                    Effect::RandomCard(Pile::Hand, CardType::Attack, 1, true),
                    Effect::RandomCard(Pile::Hand, CardType::Skill, 1, true),
                    Effect::RandomCard(Pile::Hand, CardType::Power, 1, true),
                ],
            },
            PotionDef {
                id: "POTION.POWDERED_DEMISE",
                target: Target::ChosenEnemy,
                effects: effects![Effect::ApplyPower(
                    Target::ChosenEnemy,
                    161,
                    Amount::fixed(9, 9),
                )],
            },
            PotionDef {
                id: "POTION.POWER_POTION",
                target: Target::Player,
                effects: effects![Effect::OfferCharacterType(CardType::Power, 3)],
            },
            PotionDef {
                id: "POTION.RADIANT_TINCTURE",
                target: Target::Player,
                effects: effects![
                    Effect::Energy(1),
                    Effect::ApplyPower(Target::Player, 162, Amount::fixed(3, 3)),
                ],
            },
            PotionDef {
                id: "POTION.REGEN_POTION",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 163, Amount::fixed(5, 5))],
            },
            PotionDef {
                id: "POTION.SHACKLING_POTION",
                target: Target::AllEnemies,
                effects: effects![Effect::TemporaryStrength(
                    Target::AllEnemies,
                    power_id::SHACKLING_POTION,
                    Amount::fixed(7, 7),
                )],
            },
            PotionDef {
                id: "POTION.SHIP_IN_A_BOTTLE",
                target: Target::Player,
                effects: effects![
                    Effect::RawBlock(Target::Player, Amount::fixed(10, 10)),
                    Effect::ApplyPower(Target::Player, 41, Amount::fixed(10, 10)),
                ],
            },
            PotionDef {
                id: "POTION.SKILL_POTION",
                target: Target::Player,
                effects: effects![Effect::OfferCharacterType(CardType::Skill, 3)],
            },
            PotionDef {
                id: "POTION.SNECKO_OIL",
                target: Target::Player,
                effects: effects![Effect::Draw(7), Effect::RandomizeHandCosts],
            },
            PotionDef {
                id: "POTION.SPEED_POTION",
                target: Target::Player,
                effects: effects![
                    Effect::ApplyPower(Target::Player, 1, Amount::fixed(5, 5)),
                    Effect::ApplyPower(Target::Player, power_id::SPEED_POTION, Amount::fixed(5, 5),),
                ],
            },
            PotionDef {
                id: "POTION.STABLE_SERUM",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 71, Amount::fixed(2, 2))],
            },
            PotionDef {
                id: "POTION.TOUCH_OF_INSANITY",
                target: Target::Player,
                effects: effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::CostsResource,
                    [1, 1],
                    false,
                    false,
                    CardOp::FreeCombat,
                )],
            },
            PotionDef {
                id: "POTION.BLOOD_POTION",
                target: Target::Player,
                effects: effects![Effect::HealPercent(Target::Player, Amount::fixed(20, 20))],
            },
            PotionDef {
                id: "POTION.SOLDIERS_STEW",
                target: Target::Player,
                effects: effects![Effect::ReplayTagged(STRIKE_TAG, 1)],
            },
            PotionDef {
                id: "POTION.ASHWATER",
                target: Target::Player,
                effects: effects![Effect::Select(
                    Pile::Hand,
                    CardFilter::Any,
                    [u8::MAX, u8::MAX],
                    false,
                    true,
                    CardOp::Move(Pile::Exhaust),
                )],
            },
            PotionDef {
                id: "POTION.FOCUS_POTION",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 17, Amount::fixed(2, 2))],
            },
            PotionDef {
                id: "POTION.ESSENCE_OF_DARKNESS",
                target: Target::Player,
                effects: effects![Effect::ChannelSlots(2)],
            },
            PotionDef {
                id: "POTION.POTION_OF_CAPACITY",
                target: Target::Player,
                effects: effects![Effect::OrbSlots(2)],
            },
            PotionDef {
                id: "POTION.POISON_POTION",
                target: Target::ChosenEnemy,
                effects: effects![Effect::ApplyPower(
                    Target::ChosenEnemy,
                    7,
                    Amount::fixed(6, 6),
                )],
            },
            PotionDef {
                id: "POTION.GHOST_IN_A_JAR",
                target: Target::Player,
                effects: effects![Effect::ApplyPower(Target::Player, 5, Amount::fixed(1, 1))],
            },
            PotionDef {
                id: "POTION.CUNNING_POTION",
                target: Target::Player,
                effects: effects![Effect::AddUpgradedCard(Pile::Hand, 137, 3)],
            },
            PotionDef {
                id: "POTION.STAR_POTION",
                target: Target::Player,
                effects: effects![Effect::Stars(Amount::fixed(3, 3))],
            },
            PotionDef {
                id: "POTION.COSMIC_CONCOCTION",
                target: Target::Player,
                effects: effects![Effect::DistinctColorless(Pile::Hand, 3, true)],
            },
            PotionDef {
                id: "POTION.KINGS_COURAGE",
                target: Target::Player,
                effects: effects![Effect::Forge(Amount::fixed(15, 15))],
            },
            PotionDef {
                id: "POTION.POTION_OF_DOOM",
                target: Target::ChosenEnemy,
                effects: effects![Effect::ApplyPower(
                    Target::ChosenEnemy,
                    94,
                    Amount::fixed(33, 33),
                )],
            },
            PotionDef {
                id: "POTION.POT_OF_GHOULS",
                target: Target::Player,
                effects: effects![Effect::AddCard(Pile::Hand, 368, 2)],
            },
            PotionDef {
                id: "POTION.BONE_BREW",
                target: Target::Player,
                effects: effects![Effect::Summon(Amount::fixed(15, 15))],
            },
            PotionDef {
                id: "POTION.FOUL_POTION",
                target: Target::ChosenEnemy,
                effects: effects![
                    Effect::LoseHp(Target::Player, Amount::fixed(12, 12)),
                    Effect::Damage(Target::AllEnemies, Amount::fixed(12, 12)),
                ],
            },
            PotionDef {
                id: "POTION.GLOWWATER_POTION",
                target: Target::Player,
                effects: effects![
                    Effect::MoveAll(Pile::Hand, CardFilter::Any, Pile::Exhaust),
                    Effect::Draw(10),
                ],
            },
            PotionDef {
                id: "POTION.POTION_SHAPED_ROCK",
                target: Target::ChosenEnemy,
                effects: effects![Effect::Damage(Target::ChosenEnemy, Amount::fixed(15, 15))],
            },
        ],
        enemies: vec![
            EnemyDef {
                id: "MONSTER.LEAF_SLIME_S",
                hp: 12..=16,
                start: 0,
                moves: LEAF_SLIME_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.LEAF_SLIME_M",
                hp: 33..=36,
                start: 0,
                moves: LEAF_SLIME_M_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.KIN_FOLLOWER",
                hp: 62..=63,
                start: 0,
                moves: KIN_FOLLOWER_MOVES,
                powers: MINION_POWER,
            },
            EnemyDef {
                id: "MONSTER.KIN_PRIEST",
                hp: 199..=199,
                start: 0,
                moves: KIN_PRIEST_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.AXEBOT",
                hp: 46..=46,
                start: 2,
                moves: WIKI_AXEBOT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.BATTLE_FRIEND_V1_0",
                hp: 75..=75,
                start: 0,
                moves: WIKI_BATTLE_FRIEND_V1_0_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.BATTLE_FRIEND_V2_0",
                hp: 150..=150,
                start: 0,
                moves: WIKI_BATTLE_FRIEND_V2_0_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.BATTLE_FRIEND_V3_0",
                hp: 300..=300,
                start: 0,
                moves: WIKI_BATTLE_FRIEND_V3_0_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.BOWLBUG_EGG",
                hp: 24..=24,
                start: 0,
                moves: WIKI_BOWLBUG_EGG_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.BOWLBUG_NECTAR",
                hp: 39..=39,
                start: 0,
                moves: WIKI_BOWLBUG_NECTAR_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.BOWLBUG_ROCK",
                hp: 49..=49,
                start: 0,
                moves: WIKI_BOWLBUG_ROCK_MOVES,
                powers: &[(power_id::IMBALANCED, 1)],
            },
            EnemyDef {
                id: "MONSTER.BOWLBUG_SILK",
                hp: 44..=44,
                start: 1,
                moves: WIKI_BOWLBUG_SILK_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.BYGONE_EFFIGY",
                hp: 132..=132,
                start: 0,
                moves: WIKI_BYGONE_EFFIGY_MOVES,
                powers: &[(power_id::SLOW, 0)],
            },
            EnemyDef {
                id: "MONSTER.BYRDONIS",
                hp: 90..=90,
                start: 0,
                moves: WIKI_BYRDONIS_MOVES,
                powers: &[(power_id::TERRITORIAL, 1)],
            },
            EnemyDef {
                id: "MONSTER.BYRDPIP",
                hp: 9999..=9999,
                start: 0,
                moves: WIKI_BYRDPIP_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.CALCIFIED_CULTIST",
                hp: 42..=42,
                start: 0,
                moves: WIKI_CALCIFIED_CULTIST_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.CEREMONIAL_BEAST",
                hp: 262..=262,
                start: 0,
                moves: WIKI_CEREMONIAL_BEAST_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.CHOMPER",
                hp: 67..=67,
                start: 0,
                moves: WIKI_CHOMPER_MOVES,
                powers: &[(6, 2)],
            },
            EnemyDef {
                id: "MONSTER.CORPSE_SLUG",
                hp: 29..=29,
                start: 0,
                moves: WIKI_CORPSE_SLUG_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.CRUSHER",
                hp: 209..=209,
                start: 0,
                moves: WIKI_CRUSHER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.CUBEX_CONSTRUCT",
                hp: 70..=70,
                start: 0,
                moves: WIKI_CUBEX_CONSTRUCT_MOVES,
                powers: &[(6, 1)],
            },
            EnemyDef {
                id: "MONSTER.DAMP_CULTIST",
                hp: 54..=54,
                start: 0,
                moves: WIKI_DAMP_CULTIST_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.DECIMILLIPEDE_3_SEGMENTS",
                hp: 56..=56,
                start: 0,
                moves: WIKI_DECIMILLIPEDE_3_SEGMENTS_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.DEVOTED_SCULPTOR",
                hp: 172..=172,
                start: 0,
                moves: WIKI_DEVOTED_SCULPTOR_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.DOOR",
                hp: 165..=165,
                start: 0,
                moves: WIKI_DOOR_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.DOORMAKER",
                hp: 512..=512,
                start: 0,
                moves: WIKI_DOORMAKER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.ENTOMANCER",
                hp: 155..=155,
                start: 0,
                moves: WIKI_ENTOMANCER_MOVES,
                powers: &[(power_id::PERSONAL_HIVE, 1)],
            },
            EnemyDef {
                id: "MONSTER.EXOSKELETON",
                hp: 29..=29,
                start: 0,
                moves: WIKI_EXOSKELETON_MOVES,
                powers: &[(power_id::HARD_TO_KILL, 9)],
            },
            EnemyDef {
                id: "MONSTER.EYE_WITH_TEETH",
                hp: 6..=6,
                start: 0,
                moves: WIKI_EYE_WITH_TEETH_MINION_MOVES,
                powers: &[(power_id::ILLUSION, 1), (power_id::MINION, 1)],
            },
            EnemyDef {
                id: "MONSTER.FABRICATOR",
                hp: 155..=155,
                start: 0,
                moves: WIKI_FABRICATOR_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.FAT_GREMLIN",
                hp: 18..=18,
                start: 0,
                moves: WIKI_FAT_GREMLIN_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.FLAIL_KNIGHT",
                hp: 108..=108,
                start: 0,
                moves: WIKI_FLAIL_KNIGHT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.FLYCONID",
                hp: 53..=53,
                start: 0,
                moves: WIKI_FLYCONID_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.FOGMOG",
                hp: 78..=78,
                start: 0,
                moves: WIKI_FOGMOG_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.FOSSIL_STALKER",
                hp: 56..=56,
                start: 1,
                moves: WIKI_FOSSIL_STALKER_MOVES,
                powers: &[(power_id::SUCK, 3)],
            },
            EnemyDef {
                id: "MONSTER.FROG_KNIGHT",
                hp: 199..=199,
                start: 2,
                moves: WIKI_FROG_KNIGHT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.FUZZY_WURM_CRAWLER",
                hp: 58..=59,
                start: 0,
                moves: WIKI_FUZZY_WURM_CRAWLER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.GAS_BOMB",
                hp: 12..=12,
                start: 0,
                moves: WIKI_GAS_BOMB_MINION_MOVES,
                powers: &[(16, 1)],
            },
            EnemyDef {
                id: "MONSTER.GLOBE_HEAD",
                hp: 158..=158,
                start: 0,
                moves: WIKI_GLOBE_HEAD_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.GREMLIN_MERC",
                hp: 53..=53,
                start: 0,
                moves: WIKI_GREMLIN_MERC_MOVES,
                powers: &[(power_id::SURPRISE, 1), (power_id::THIEVERY, 20)],
            },
            EnemyDef {
                id: "MONSTER.GUARDBOT",
                hp: 26..=26,
                start: 0,
                moves: WIKI_GUARDBOT_MINION_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.HATCHLING_MINION",
                hp: 1..=1,
                start: 0,
                moves: WIKI_HATCHLING_MINION_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.HAUNTED_SHIP",
                hp: 67..=67,
                start: 2,
                moves: WIKI_HAUNTED_SHIP_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.HUNTER_KILLER",
                hp: 126..=126,
                start: 0,
                moves: WIKI_HUNTER_KILLER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.INFESTED_PRISM",
                hp: 215..=215,
                start: 0,
                moves: WIKI_INFESTED_PRISM_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.INKLET",
                hp: 18..=18,
                start: 0,
                moves: WIKI_INKLET_MOVES,
                powers: &[(power_id::SLIPPERY, 1)],
            },
            EnemyDef {
                id: "MONSTER.KNOWLEDGE_DEMON",
                hp: 399..=399,
                start: 0,
                moves: WIKI_KNOWLEDGE_DEMON_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.LAGAVULIN_MATRIARCH",
                hp: 233..=233,
                start: 0,
                moves: WIKI_LAGAVULIN_MATRIARCH_MOVES,
                powers: &[(79, 12), (power_id::ASLEEP, 3)],
            },
            EnemyDef {
                id: "MONSTER.LIVING_FOG",
                hp: 82..=82,
                start: 0,
                moves: WIKI_LIVING_FOG_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.LIVING_SHIELD",
                hp: 65..=65,
                start: 0,
                moves: WIKI_LIVING_SHIELD_MOVES,
                powers: &[(power_id::RAMPART, 25)],
            },
            EnemyDef {
                id: "MONSTER.LOUSE_PROGENITOR",
                hp: 141..=141,
                start: 0,
                moves: WIKI_LOUSE_PROGENITOR_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.MAGI_KNIGHT",
                hp: 89..=89,
                start: 0,
                moves: WIKI_MAGI_KNIGHT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.MAWLER",
                hp: 76..=76,
                start: 2,
                moves: WIKI_MAWLER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.MECHA_KNIGHT",
                hp: 320..=320,
                start: 0,
                moves: WIKI_MECHA_KNIGHT_MOVES,
                powers: &[(6, 3)],
            },
            EnemyDef {
                id: "MONSTER.MYTE",
                hp: 69..=69,
                start: 0,
                moves: WIKI_MYTE_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.NIBBIT",
                hp: 44..=48,
                start: 0,
                moves: WIKI_NIBBIT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.NOISEBOT",
                hp: 29..=29,
                start: 0,
                moves: WIKI_NOISEBOT_MINION_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.OSTY",
                hp: 1..=1,
                start: 0,
                moves: WIKI_OSTY_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.OWL_MAGISTRATE",
                hp: 243..=243,
                start: 0,
                moves: WIKI_OWL_MAGISTRATE_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.PAEL_S_LEGION",
                hp: 9999..=9999,
                start: 0,
                moves: WIKI_PAEL_S_LEGION_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.PARAFRIGHT_MINION",
                hp: 21..=21,
                start: 0,
                moves: WIKI_PARAFRIGHT_MINION_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.PHANTASMAL_GARDENER",
                hp: 33..=33,
                start: 0,
                moves: WIKI_PHANTASMAL_GARDENER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.PHROG_PARASITE",
                hp: 66..=68,
                start: 0,
                moves: WIKI_PHROG_PARASITE_MOVES,
                powers: &[(power_id::INFESTED, 4)],
            },
            EnemyDef {
                id: "MONSTER.PUNCH_CONSTRUCT",
                hp: 60..=60,
                start: 0,
                moves: WIKI_PUNCH_CONSTRUCT_MOVES,
                powers: &[(6, 1)],
            },
            EnemyDef {
                id: "MONSTER.QUEEN",
                hp: 419..=419,
                start: 0,
                moves: WIKI_QUEEN_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.ROCKET",
                hp: 199..=199,
                start: 0,
                moves: WIKI_ROCKET_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.ASSASSIN_RUBY_RAIDER",
                hp: 24..=24,
                start: 0,
                moves: WIKI_RUBY_RAIDER_ASSASSIN_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.AXE_RUBY_RAIDER",
                hp: 23..=23,
                start: 0,
                moves: WIKI_RUBY_RAIDER_AXE_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.BRUTE_RUBY_RAIDER",
                hp: 34..=34,
                start: 0,
                moves: WIKI_RUBY_RAIDER_BRUTE_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.CROSSBOW_RUBY_RAIDER",
                hp: 22..=22,
                start: 0,
                moves: WIKI_RUBY_RAIDER_CROSSBOW_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TRACKER_RUBY_RAIDER",
                hp: 26..=26,
                start: 0,
                moves: WIKI_RUBY_RAIDER_TRACKER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SCROLL_OF_BITING",
                hp: 39..=39,
                start: 0,
                moves: WIKI_SCROLL_OF_BITING_MOVES,
                powers: &[(power_id::PAPER_CUTS, 2)],
            },
            EnemyDef {
                id: "MONSTER.SEAPUNK",
                hp: 49..=49,
                start: 0,
                moves: WIKI_SEAPUNK_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SEWER_CLAM",
                hp: 58..=58,
                start: 0,
                moves: WIKI_SEWER_CLAM_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SHRINKER_BEETLE",
                hp: 40..=42,
                start: 0,
                moves: WIKI_SHRINKER_BEETLE_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SKULKING_COLONY",
                hp: 84..=84,
                start: 0,
                moves: WIKI_SKULKING_COLONY_MOVES,
                powers: &[(202, 20)],
            },
            EnemyDef {
                id: "MONSTER.SLIMED_BERSERKER",
                hp: 276..=276,
                start: 0,
                moves: WIKI_SLIMED_BERSERKER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SLITHERING_STRANGLER",
                hp: 54..=56,
                start: 0,
                moves: WIKI_SLITHERING_STRANGLER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SLUDGE_SPINNER",
                hp: 42..=42,
                start: 0,
                moves: WIKI_SLUDGE_SPINNER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SLUMBERING_BEETLE",
                hp: 89..=89,
                start: 0,
                moves: WIKI_SLUMBERING_BEETLE_MOVES,
                powers: &[(power_id::SLUMBER, 3)],
            },
            EnemyDef {
                id: "MONSTER.SNAPPING_JAXFRUIT",
                hp: 36..=36,
                start: 0,
                moves: WIKI_SNAPPING_JAXFRUIT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SNEAKY_GREMLIN",
                hp: 15..=15,
                start: 0,
                moves: WIKI_SNEAKY_GREMLIN_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SOUL_FYSH",
                hp: 221..=221,
                start: 0,
                moves: WIKI_SOUL_FYSH_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SOUL_NEXUS",
                hp: 254..=254,
                start: 0,
                moves: WIKI_SOUL_NEXUS_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SPECTRAL_KNIGHT",
                hp: 97..=97,
                start: 0,
                moves: WIKI_SPECTRAL_KNIGHT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.SPINY_TOAD",
                hp: 124..=124,
                start: 0,
                moves: WIKI_SPINY_TOAD_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.STABBOT",
                hp: 29..=29,
                start: 0,
                moves: WIKI_STABBOT_MINION_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.STAGE_1",
                hp: 1..=1,
                start: 0,
                moves: WIKI_STAGE_1_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.STAGE_2",
                hp: 1..=1,
                start: 0,
                moves: WIKI_STAGE_2_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.STAGE_3",
                hp: 1..=1,
                start: 0,
                moves: WIKI_STAGE_3_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TERROR_EEL",
                hp: 150..=150,
                start: 0,
                moves: WIKI_TERROR_EEL_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TEST_SUBJECT_C_COUNT",
                hp: 1..=1,
                start: 0,
                moves: WIKI_TEST_SUBJECT_C_COUNT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.THE_ADVERSARY_MK_1",
                hp: 100..=100,
                start: 0,
                moves: WIKI_THE_ADVERSARY_MK_1_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.THE_ADVERSARY_MK_2",
                hp: 200..=200,
                start: 0,
                moves: WIKI_THE_ADVERSARY_MK_2_MOVES,
                powers: &[(6, 1)],
            },
            EnemyDef {
                id: "MONSTER.THE_ADVERSARY_MK_3",
                hp: 300..=300,
                start: 0,
                moves: WIKI_THE_ADVERSARY_MK_3_MOVES,
                powers: &[(6, 2)],
            },
            EnemyDef {
                id: "MONSTER.THE_ARCHITECT",
                hp: 9999..=9999,
                start: 0,
                moves: WIKI_THE_ARCHITECT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.THE_FORGOTTEN",
                hp: 111..=111,
                start: 0,
                moves: WIKI_THE_FORGOTTEN_MOVES,
                powers: &[(power_id::POSSESS_SPEED, 1)],
            },
            EnemyDef {
                id: "MONSTER.THE_INSATIABLE",
                hp: 341..=341,
                start: 0,
                moves: WIKI_THE_INSATIABLE_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.THE_LOST",
                hp: 99..=99,
                start: 0,
                moves: WIKI_THE_LOST_MOVES,
                powers: &[(power_id::POSSESS_STRENGTH, 1)],
            },
            EnemyDef {
                id: "MONSTER.THE_OBSCURA",
                hp: 129..=129,
                start: 0,
                moves: WIKI_THE_OBSCURA_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.THIEVING_HOPPER",
                hp: 84..=84,
                start: 0,
                moves: WIKI_THIEVING_HOPPER_MOVES,
                powers: &[(191, 5)],
            },
            EnemyDef {
                id: "MONSTER.TOADPOLE",
                hp: 26..=26,
                start: 0,
                moves: WIKI_TOADPOLE_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TORCH_HEAD_AMALGAM",
                hp: 211..=211,
                start: 0,
                moves: WIKI_TORCH_HEAD_AMALGAM_MOVES,
                powers: MINION_POWER,
            },
            EnemyDef {
                id: "MONSTER.TOUGH_EGG",
                hp: 19..=19,
                start: 0,
                moves: WIKI_TOUGH_EGG_MINION_MOVES,
                powers: &[(power_id::HATCH, 1)],
            },
            EnemyDef {
                id: "MONSTER.TUNNELER",
                hp: 92..=92,
                start: 0,
                moves: WIKI_TUNNELER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TURRET_OPERATOR",
                hp: 51..=51,
                start: 0,
                moves: WIKI_TURRET_OPERATOR_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TWIG_SLIME_M",
                hp: 29..=29,
                start: 0,
                moves: WIKI_TWIG_SLIME_M_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TWIG_SLIME_S",
                hp: 12..=12,
                start: 0,
                moves: WIKI_TWIG_SLIME_S_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TWO_TAILED_RAT",
                hp: 22..=22,
                start: 0,
                moves: WIKI_TWO_TAILED_RAT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.VANTOM",
                hp: 183..=183,
                start: 0,
                moves: WIKI_VANTOM_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.VINE_SHAMBLER",
                hp: 64..=64,
                start: 1,
                moves: WIKI_VINE_SHAMBLER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.WATERFALL_GIANT",
                hp: 260..=260,
                start: 0,
                moves: WIKI_WATERFALL_GIANT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.WRIGGLER",
                hp: 18..=22,
                start: 0,
                moves: WIKI_WRIGGLER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.ZAPBOT",
                hp: 29..=29,
                start: 0,
                moves: WIKI_ZAPBOT_MINION_MOVES,
                powers: &[(power_id::HIGH_VOLTAGE, 2)],
            },
            EnemyDef {
                id: "MONSTER.AEONGLASS",
                hp: 535..=535,
                start: 0,
                moves: AEONGLASS_MOVES,
                powers: &[(power_id::WITHERING_PRESENCE, 6), (6, 3)],
            },
            EnemyDef {
                id: "MONSTER.DECIMILLIPEDE_SEGMENT_FRONT",
                hp: 46..=52,
                start: 0,
                moves: WIKI_DECIMILLIPEDE_3_SEGMENTS_MOVES,
                powers: &[(power_id::REATTACH, 25)],
            },
            EnemyDef {
                id: "MONSTER.DECIMILLIPEDE_SEGMENT_MIDDLE",
                hp: 46..=52,
                start: 0,
                moves: WIKI_DECIMILLIPEDE_3_SEGMENTS_MOVES,
                powers: &[(power_id::REATTACH, 25)],
            },
            EnemyDef {
                id: "MONSTER.DECIMILLIPEDE_SEGMENT_BACK",
                hp: 46..=52,
                start: 0,
                moves: WIKI_DECIMILLIPEDE_3_SEGMENTS_MOVES,
                powers: &[(power_id::REATTACH, 25)],
            },
            EnemyDef {
                id: "MONSTER.PARAFRIGHT",
                hp: 21..=21,
                start: 0,
                moves: WIKI_PARAFRIGHT_MINION_MOVES,
                powers: &[(power_id::ILLUSION, 1), (power_id::MINION, 1)],
            },
            EnemyDef {
                id: "MONSTER.FAKE_MERCHANT_MONSTER",
                hp: 175..=175,
                start: 0,
                moves: FAKE_MERCHANT_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.OVICOPTER",
                hp: 132..=132,
                start: 0,
                moves: OVICOPTER_MOVES,
                powers: NO_POWERS,
            },
            EnemyDef {
                id: "MONSTER.TEST_SUBJECT",
                hp: 111..=111,
                start: 1,
                moves: WIKI_TEST_SUBJECT_MOVES,
                powers: &[(power_id::ADAPTABLE, 1)],
            },
            EnemyDef {
                id: "MONSTER.MYSTERIOUS_KNIGHT",
                hp: 108..=108,
                start: 2,
                moves: WIKI_FLAIL_KNIGHT_MOVES,
                powers: &[(power_id::STRENGTH, 6), (power_id::PLATING, 6)],
            },
        ],
        encounters: vec![
            EncounterDef {
                id: "ENCOUNTER.SLIMES_WEAK",
                enemies: LEAF_SLIME_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.BATTLEWORN_DUMMY_EVENT_ENCOUNTER",
                enemies: WIKI_ENCOUNTER_1,
            },
            EncounterDef {
                id: "ENCOUNTER.BATTLEWORN_DUMMY_1",
                enemies: BATTLE_DUMMY_1,
            },
            EncounterDef {
                id: "ENCOUNTER.BATTLEWORN_DUMMY_2",
                enemies: BATTLE_DUMMY_2,
            },
            EncounterDef {
                id: "ENCOUNTER.BATTLEWORN_DUMMY_3",
                enemies: BATTLE_DUMMY_3,
            },
            EncounterDef {
                id: "ENCOUNTER.MYSTERIOUS_KNIGHT_EVENT_ENCOUNTER",
                enemies: WIKI_ENCOUNTER_2,
            },
            EncounterDef {
                id: "ENCOUNTER.PUNCH_OFF_EVENT_ENCOUNTER",
                enemies: WIKI_ENCOUNTER_3,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_ARCHITECT_EVENT_ENCOUNTER",
                enemies: WIKI_ENCOUNTER_4,
            },
            EncounterDef {
                id: "ENCOUNTER.FAKE_MERCHANT_EVENT_ENCOUNTER",
                enemies: WIKI_ENCOUNTER_5,
            },
            EncounterDef {
                id: "ENCOUNTER.DENSE_VEGETATION_EVENT_ENCOUNTER",
                enemies: WIKI_ENCOUNTER_6,
            },
            EncounterDef {
                id: "ENCOUNTER.GLOBE_HEAD_NORMAL",
                enemies: WIKI_ENCOUNTER_7,
            },
            EncounterDef {
                id: "ENCOUNTER.AXEBOTS_NORMAL",
                enemies: AXEBOT_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.CONSTRUCT_MENAGERIE_NORMAL",
                enemies: CONSTRUCT_MENAGERIE_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.DEVOTED_SCULPTOR_WEAK",
                enemies: WIKI_ENCOUNTER_10,
            },
            EncounterDef {
                id: "ENCOUNTER.FABRICATOR_NORMAL",
                enemies: WIKI_ENCOUNTER_11,
            },
            EncounterDef {
                id: "ENCOUNTER.FROG_KNIGHT_NORMAL",
                enemies: WIKI_ENCOUNTER_12,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_GLORY_ELITE_KNIGHT_GANG",
                enemies: WIKI_ENCOUNTER_13,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_GLORY_NORMAL_LOST_AND_FORGOTTEN",
                enemies: WIKI_ENCOUNTER_14,
            },
            EncounterDef {
                id: "ENCOUNTER.SCROLLS_OF_BITING_NORMAL",
                enemies: WIKI_ENCOUNTER_15,
            },
            EncounterDef {
                id: "ENCOUNTER.MECHA_KNIGHT_ELITE",
                enemies: WIKI_ENCOUNTER_16,
            },
            EncounterDef {
                id: "ENCOUNTER.OWL_MAGISTRATE_NORMAL",
                enemies: WIKI_ENCOUNTER_17,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_GLORY_BOSS_QUEEN",
                enemies: WIKI_ENCOUNTER_18,
            },
            EncounterDef {
                id: "ENCOUNTER.SCROLLS_OF_BITING_WEAK",
                enemies: WIKI_ENCOUNTER_19,
            },
            EncounterDef {
                id: "ENCOUNTER.SLIMED_BERSERKER_NORMAL",
                enemies: WIKI_ENCOUNTER_20,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_GLORY_ELITE_SOUL_NEXUS",
                enemies: WIKI_ENCOUNTER_21,
            },
            EncounterDef {
                id: "ENCOUNTER.TEST_SUBJECT_BOSS",
                enemies: WIKI_ENCOUNTER_22,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_GLORY_BOSS_THE_DOORMAKER",
                enemies: WIKI_ENCOUNTER_23,
            },
            EncounterDef {
                id: "ENCOUNTER.TURRET_OPERATOR_WEAK",
                enemies: WIKI_ENCOUNTER_24,
            },
            EncounterDef {
                id: "ENCOUNTER.CHOMPERS_NORMAL",
                enemies: WIKI_ENCOUNTER_25,
            },
            EncounterDef {
                id: "ENCOUNTER.BOWLBUGS_NORMAL",
                enemies: WIKI_ENCOUNTER_26,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_HIVE_WEAK_BOWLBUGS",
                enemies: WIKI_ENCOUNTER_27,
            },
            EncounterDef {
                id: "ENCOUNTER.ENTOMANCER_ELITE",
                enemies: WIKI_ENCOUNTER_28,
            },
            EncounterDef {
                id: "ENCOUNTER.EXOSKELETONS_WEAK",
                enemies: WIKI_ENCOUNTER_29,
            },
            EncounterDef {
                id: "ENCOUNTER.HUNTER_KILLER_NORMAL",
                enemies: WIKI_ENCOUNTER_30,
            },
            EncounterDef {
                id: "ENCOUNTER.INFESTED_PRISMS_ELITE",
                enemies: WIKI_ENCOUNTER_31,
            },
            EncounterDef {
                id: "ENCOUNTER.KAISER_CRAB_BOSS",
                enemies: WIKI_ENCOUNTER_32,
            },
            EncounterDef {
                id: "ENCOUNTER.KNOWLEDGE_DEMON_BOSS",
                enemies: WIKI_ENCOUNTER_33,
            },
            EncounterDef {
                id: "ENCOUNTER.LOUSE_PROGENITOR_NORMAL",
                enemies: WIKI_ENCOUNTER_34,
            },
            EncounterDef {
                id: "ENCOUNTER.EXOSKELETONS_NORMAL",
                enemies: WIKI_ENCOUNTER_35,
            },
            EncounterDef {
                id: "ENCOUNTER.MYTES_NORMAL",
                enemies: WIKI_ENCOUNTER_36,
            },
            EncounterDef {
                id: "ENCOUNTER.OVICOPTER_NORMAL",
                enemies: OVICOPTER_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.SLUMBERING_BEETLE_NORMAL",
                enemies: WIKI_ENCOUNTER_38,
            },
            EncounterDef {
                id: "ENCOUNTER.SPINY_TOAD_NORMAL",
                enemies: WIKI_ENCOUNTER_39,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_HIVE_ELITE_THE_DECIMILLIPEDE",
                enemies: WIKI_ENCOUNTER_40,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_INSATIABLE_BOSS",
                enemies: WIKI_ENCOUNTER_41,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_OBSCURA_NORMAL",
                enemies: WIKI_ENCOUNTER_42,
            },
            EncounterDef {
                id: "ENCOUNTER.THIEVING_HOPPER_WEAK",
                enemies: WIKI_ENCOUNTER_43,
            },
            EncounterDef {
                id: "ENCOUNTER.TUNNELER_WEAK",
                enemies: WIKI_ENCOUNTER_44,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_HIVE_NORMAL_TUNNELING_TWOSOME",
                enemies: WIKI_ENCOUNTER_45,
            },
            EncounterDef {
                id: "ENCOUNTER.NIBBITS_WEAK",
                enemies: WIKI_ENCOUNTER_46,
            },
            EncounterDef {
                id: "ENCOUNTER.NIBBITS_NORMAL",
                enemies: WIKI_ENCOUNTER_47,
            },
            EncounterDef {
                id: "ENCOUNTER.BYGONE_EFFIGY_ELITE",
                enemies: WIKI_ENCOUNTER_48,
            },
            EncounterDef {
                id: "ENCOUNTER.BYRDONIS_ELITE",
                enemies: WIKI_ENCOUNTER_49,
            },
            EncounterDef {
                id: "ENCOUNTER.CEREMONIAL_BEAST_BOSS",
                enemies: WIKI_ENCOUNTER_50,
            },
            EncounterDef {
                id: "ENCOUNTER.CUBEX_CONSTRUCT_NORMAL",
                enemies: WIKI_ENCOUNTER_51,
            },
            EncounterDef {
                id: "ENCOUNTER.FOGMOG_NORMAL",
                enemies: FOGMOG_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.FUZZY_WURM_CRAWLER_WEAK",
                enemies: WIKI_ENCOUNTER_53,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_OVERGROWTH_WEAK_GROUP_OF_SLIMES",
                enemies: WIKI_ENCOUNTER_54,
            },
            EncounterDef {
                id: "ENCOUNTER.INKLETS_NORMAL",
                enemies: WIKI_ENCOUNTER_55,
            },
            EncounterDef {
                id: "ENCOUNTER.MAWLER_NORMAL",
                enemies: WIKI_ENCOUNTER_56,
            },
            EncounterDef {
                id: "ENCOUNTER.OVERGROWTH_CRAWLERS",
                enemies: OVERGROWTH_CRAWLERS_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.FLYCONID_NORMAL",
                enemies: WIKI_ENCOUNTER_58,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_OVERGROWTH_ELITE_PHROG_PARASITE",
                enemies: WIKI_ENCOUNTER_59,
            },
            EncounterDef {
                id: "ENCOUNTER.RUBY_RAIDERS_NORMAL",
                enemies: WIKI_ENCOUNTER_60,
            },
            EncounterDef {
                id: "ENCOUNTER.SHRINKER_BEETLE_WEAK",
                enemies: WIKI_ENCOUNTER_61,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_OVERGROWTH_NORMAL_SHROOM_AND_SLIME",
                enemies: SNAPPING_JAXFRUIT_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_OVERGROWTH_NORMAL_STRANGLER_AND_FRIEND",
                enemies: WIKI_ENCOUNTER_63,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_OVERGROWTH_NORMAL_SWARM_OF_SLIMES",
                enemies: WIKI_ENCOUNTER_64,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_KIN_BOSS",
                enemies: WIKI_ENCOUNTER_65,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_OVERGROWTH_BOSS_VANTOM",
                enemies: WIKI_ENCOUNTER_66,
            },
            EncounterDef {
                id: "ENCOUNTER.VINE_SHAMBLER_NORMAL",
                enemies: WIKI_ENCOUNTER_67,
            },
            EncounterDef {
                id: "ENCOUNTER.CORPSE_SLUGS_WEAK",
                enemies: WIKI_ENCOUNTER_68,
            },
            EncounterDef {
                id: "ENCOUNTER.CULTISTS_NORMAL",
                enemies: WIKI_ENCOUNTER_69,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_UNDERDOCKS_NORMAL_EVIL_GAS",
                enemies: WIKI_ENCOUNTER_70,
            },
            EncounterDef {
                id: "ENCOUNTER.FOSSIL_STALKER_NORMAL",
                enemies: WIKI_ENCOUNTER_71,
            },
            EncounterDef {
                id: "ENCOUNTER.HAUNTED_SHIP_NORMAL",
                enemies: WIKI_ENCOUNTER_72,
            },
            EncounterDef {
                id: "ENCOUNTER.LAGAVULIN_MATRIARCH_BOSS",
                enemies: WIKI_ENCOUNTER_73,
            },
            EncounterDef {
                id: "ENCOUNTER.CORPSE_SLUGS_NORMAL",
                enemies: WIKI_ENCOUNTER_74,
            },
            EncounterDef {
                id: "ENCOUNTER.PHANTASMAL_GARDENERS_ELITE",
                enemies: WIKI_ENCOUNTER_75,
            },
            EncounterDef {
                id: "ENCOUNTER.PUNCH_CONSTRUCT_NORMAL",
                enemies: WIKI_ENCOUNTER_76,
            },
            EncounterDef {
                id: "ENCOUNTER.SEAPUNK_WEAK",
                enemies: WIKI_ENCOUNTER_77,
            },
            EncounterDef {
                id: "ENCOUNTER.SEWER_CLAM_NORMAL",
                enemies: WIKI_ENCOUNTER_78,
            },
            EncounterDef {
                id: "ENCOUNTER.SKULKING_COLONY_ELITE",
                enemies: WIKI_ENCOUNTER_79,
            },
            EncounterDef {
                id: "ENCOUNTER.SLUDGE_SPINNER_WEAK",
                enemies: WIKI_ENCOUNTER_80,
            },
            EncounterDef {
                id: "ENCOUNTER.SOUL_FYSH_BOSS",
                enemies: WIKI_ENCOUNTER_81,
            },
            EncounterDef {
                id: "ENCOUNTER.TERROR_EEL_ELITE",
                enemies: WIKI_ENCOUNTER_82,
            },
            EncounterDef {
                id: "ENCOUNTER.TOADPOLES_WEAK",
                enemies: WIKI_ENCOUNTER_83,
            },
            EncounterDef {
                id: "ENCOUNTER.GREMLIN_MERC_NORMAL",
                enemies: GREMLIN_MERC_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.TWO_TAILED_RATS_NORMAL",
                enemies: WIKI_ENCOUNTER_85,
            },
            EncounterDef {
                id: "ENCOUNTER.SEAPUNK_NORMAL",
                enemies: SEAPUNK_NORMAL_ENCOUNTER,
            },
            EncounterDef {
                id: "ENCOUNTER.WATERFALL_GIANT_BOSS",
                enemies: WIKI_ENCOUNTER_87,
            },
            EncounterDef {
                id: "ENCOUNTER.AEONGLASS_BOSS",
                enemies: &[114],
            },
            EncounterDef {
                id: "ENCOUNTER.BOWLBUGS_WEAK",
                enemies: WIKI_ENCOUNTER_27,
            },
            EncounterDef {
                id: "ENCOUNTER.DECIMILLIPEDE_ELITE",
                enemies: WIKI_ENCOUNTER_40,
            },
            EncounterDef {
                id: "ENCOUNTER.KNIGHTS_ELITE",
                enemies: WIKI_ENCOUNTER_13,
            },
            EncounterDef {
                id: "ENCOUNTER.PHROG_PARASITE_ELITE",
                enemies: WIKI_ENCOUNTER_59,
            },
            EncounterDef {
                id: "ENCOUNTER.QUEEN_BOSS",
                enemies: WIKI_ENCOUNTER_18,
            },
            EncounterDef {
                id: "ENCOUNTER.SLIMES_NORMAL",
                enemies: WIKI_ENCOUNTER_64,
            },
            EncounterDef {
                id: "ENCOUNTER.SLITHERING_STRANGLER_NORMAL",
                enemies: WIKI_ENCOUNTER_63,
            },
            EncounterDef {
                id: "ENCOUNTER.SNAPPING_JAXFRUIT_NORMAL",
                enemies: WIKI_ENCOUNTER_58,
            },
            EncounterDef {
                id: "ENCOUNTER.SOUL_NEXUS_ELITE",
                enemies: WIKI_ENCOUNTER_21,
            },
            EncounterDef {
                id: "ENCOUNTER.THE_LOST_AND_FORGOTTEN_NORMAL",
                enemies: WIKI_ENCOUNTER_14,
            },
            EncounterDef {
                id: "ENCOUNTER.VANTOM_BOSS",
                enemies: WIKI_ENCOUNTER_66,
            },
            EncounterDef {
                id: "ENCOUNTER.LIVING_FOG_NORMAL",
                enemies: LIVING_FOG_ENCOUNTER,
            },
        ],
        orbs: vec![
            OrbDef {
                id: "ORB.LIGHTNING_ORB",
                timing: OrbTiming::TurnEnd,
                initial_value: 0,
                passive_value: 0,
                passive_decay: 0,
                focus_passive: false,
                passive: effects![Effect::Damage(
                    Target::RandomEnemy,
                    Amount {
                        base: 3,
                        upgraded: 3,
                        ascension: 0,
                        scale: Scale::Power(17),
                        multiplier: 1,
                        divisor: 1,
                    },
                )],
                evoke: effects![Effect::Damage(
                    Target::RandomEnemy,
                    Amount {
                        base: 8,
                        upgraded: 8,
                        ascension: 0,
                        scale: Scale::Power(17),
                        multiplier: 1,
                        divisor: 1,
                    },
                )],
            },
            OrbDef {
                id: "ORB.FROST_ORB",
                timing: OrbTiming::TurnEnd,
                initial_value: 0,
                passive_value: 0,
                passive_decay: 0,
                focus_passive: false,
                passive: effects![Effect::Block(
                    Target::Player,
                    Amount {
                        base: 2,
                        upgraded: 2,
                        ascension: 0,
                        scale: Scale::Power(17),
                        multiplier: 1,
                        divisor: 1,
                    },
                )],
                evoke: effects![Effect::Block(
                    Target::Player,
                    Amount {
                        base: 5,
                        upgraded: 5,
                        ascension: 0,
                        scale: Scale::Power(17),
                        multiplier: 1,
                        divisor: 1,
                    },
                )],
            },
            OrbDef {
                id: "ORB.DARK_ORB",
                timing: OrbTiming::TurnEnd,
                initial_value: 6,
                passive_value: 6,
                passive_decay: 0,
                focus_passive: true,
                passive: &[],
                evoke: effects![Effect::Damage(
                    Target::LowestHpEnemy,
                    Amount::scaled(Scale::Event, 1),
                )],
            },
            OrbDef {
                id: "ORB.PLASMA_ORB",
                timing: OrbTiming::TurnStart,
                initial_value: 0,
                passive_value: 0,
                passive_decay: 0,
                focus_passive: false,
                passive: effects![Effect::Energy(1)],
                evoke: effects![Effect::Energy(2)],
            },
            OrbDef {
                id: "ORB.GLASS_ORB",
                timing: OrbTiming::TurnEnd,
                initial_value: 4,
                passive_value: 0,
                passive_decay: 1,
                focus_passive: false,
                passive: effects![Effect::Damage(
                    Target::AllEnemies,
                    Amount::scaled(Scale::EventPower(17), 1),
                )],
                evoke: effects![Effect::Damage(
                    Target::AllEnemies,
                    Amount {
                        base: 8,
                        upgraded: 8,
                        ascension: 0,
                        scale: Scale::Power(17),
                        multiplier: 1,
                        divisor: 1,
                    },
                )],
            },
        ],
        events: vec![
            EventDef {
                id: "EVENT.ABYSSAL_BATHS",
                options: WIKI_EVENT_0_OPTIONS,
            },
            EventDef {
                id: "EVENT.AMALGAMATOR",
                options: WIKI_EVENT_1_OPTIONS,
            },
            EventDef {
                id: "EVENT.AROMA_OF_CHAOS",
                options: WIKI_EVENT_2_OPTIONS,
            },
            EventDef {
                id: "EVENT.BATTLEWORN_DUMMY",
                options: WIKI_EVENT_3_OPTIONS,
            },
            EventDef {
                id: "EVENT.BRAIN_LEECH",
                options: WIKI_EVENT_4_OPTIONS,
            },
            EventDef {
                id: "EVENT.BUGSLAYER",
                options: WIKI_EVENT_5_OPTIONS,
            },
            EventDef {
                id: "EVENT.BYRDONIS_NEST",
                options: WIKI_EVENT_6_OPTIONS,
            },
            EventDef {
                id: "EVENT.COLORFUL_PHILOSOPHERS",
                options: WIKI_EVENT_7_OPTIONS,
            },
            EventDef {
                id: "EVENT.COLOSSAL_FLOWER",
                options: WIKI_EVENT_8_OPTIONS,
            },
            EventDef {
                id: "EVENT.CRYSTAL_SPHERE",
                options: WIKI_EVENT_9_OPTIONS,
            },
            EventDef {
                id: "EVENT.DENSE_VEGETATION",
                options: WIKI_EVENT_10_OPTIONS,
            },
            EventDef {
                id: "EVENT.DOLL_ROOM",
                options: WIKI_EVENT_11_OPTIONS,
            },
            EventDef {
                id: "EVENT.DOORS_OF_LIGHT_AND_DARK",
                options: WIKI_EVENT_12_OPTIONS,
            },
            EventDef {
                id: "EVENT.DROWNING_BEACON",
                options: WIKI_EVENT_13_OPTIONS,
            },
            EventDef {
                id: "EVENT.ENDLESS_CONVEYOR",
                options: WIKI_EVENT_14_OPTIONS,
            },
            EventDef {
                id: "EVENT.FIELD_OF_MAN_SIZED_HOLES",
                options: WIKI_EVENT_15_OPTIONS,
            },
            EventDef {
                id: "EVENT.GRAVE_OF_THE_FORGOTTEN",
                options: WIKI_EVENT_16_OPTIONS,
            },
            EventDef {
                id: "EVENT.HUNGRY_FOR_MUSHROOMS",
                options: WIKI_EVENT_17_OPTIONS,
            },
            EventDef {
                id: "EVENT.INFESTED_AUTOMATON",
                options: WIKI_EVENT_18_OPTIONS,
            },
            EventDef {
                id: "EVENT.JUNGLE_MAZE_ADVENTURE",
                options: WIKI_EVENT_19_OPTIONS,
            },
            EventDef {
                id: "EVENT.LUMINOUS_CHOIR",
                options: WIKI_EVENT_20_OPTIONS,
            },
            EventDef {
                id: "EVENT.MORPHIC_GROVE",
                options: WIKI_EVENT_21_OPTIONS,
            },
            EventDef {
                id: "EVENT.POTION_COURIER",
                options: WIKI_EVENT_22_OPTIONS,
            },
            EventDef {
                id: "EVENT.PUNCH_OFF",
                options: WIKI_EVENT_23_OPTIONS,
            },
            EventDef {
                id: "EVENT.RANWID_THE_ELDER",
                options: WIKI_EVENT_24_OPTIONS,
            },
            EventDef {
                id: "EVENT.REFLECTIONS",
                options: WIKI_EVENT_25_OPTIONS,
            },
            EventDef {
                id: "EVENT.RELIC_TRADER",
                options: WIKI_EVENT_26_OPTIONS,
            },
            EventDef {
                id: "EVENT.ROOM_FULL_OF_CHEESE",
                options: WIKI_EVENT_27_OPTIONS,
            },
            EventDef {
                id: "EVENT.SAPPHIRE_SEED",
                options: WIKI_EVENT_28_OPTIONS,
            },
            EventDef {
                id: "EVENT.SELF_HELP_BOOK",
                options: WIKI_EVENT_29_OPTIONS,
            },
            EventDef {
                id: "EVENT.SLIPPERY_BRIDGE",
                options: WIKI_EVENT_30_OPTIONS,
            },
            EventDef {
                id: "EVENT.SPIRALING_WHIRLPOOL",
                options: WIKI_EVENT_31_OPTIONS,
            },
            EventDef {
                id: "EVENT.SPIRIT_GRAFTER",
                options: WIKI_EVENT_32_OPTIONS,
            },
            EventDef {
                id: "EVENT.STONE_OF_ALL_TIME",
                options: WIKI_EVENT_33_OPTIONS,
            },
            EventDef {
                id: "EVENT.SUNKEN_TREASURY",
                options: WIKI_EVENT_34_OPTIONS,
            },
            EventDef {
                id: "EVENT.SYMBIOTE",
                options: WIKI_EVENT_35_OPTIONS,
            },
            EventDef {
                id: "EVENT.TABLET_OF_TRUTH",
                options: WIKI_EVENT_36_OPTIONS,
            },
            EventDef {
                id: "EVENT.TEA_MASTER",
                options: WIKI_EVENT_37_OPTIONS,
            },
            EventDef {
                id: "EVENT.THE_ARCHITECT",
                options: WIKI_EVENT_38_OPTIONS,
            },
            EventDef {
                id: "EVENT.THE_FUTURE_OF_POTIONS",
                options: WIKI_EVENT_39_OPTIONS,
            },
            EventDef {
                id: "EVENT.THE_LANTERN_KEY",
                options: WIKI_EVENT_40_OPTIONS,
            },
            EventDef {
                id: "EVENT.THE_LEGENDS_WERE_TRUE",
                options: WIKI_EVENT_41_OPTIONS,
            },
            EventDef {
                id: "EVENT.LOST_WISP",
                options: WIKI_EVENT_42_OPTIONS,
            },
            EventDef {
                id: "EVENT.FAKE_MERCHANT",
                options: WIKI_EVENT_43_OPTIONS,
            },
            EventDef {
                id: "EVENT.ROUND_TEA_PARTY",
                options: WIKI_EVENT_44_OPTIONS,
            },
            EventDef {
                id: "EVENT.SUNKEN_STATUE",
                options: WIKI_EVENT_45_OPTIONS,
            },
            EventDef {
                id: "EVENT.TRIAL",
                options: WIKI_EVENT_46_OPTIONS,
            },
            EventDef {
                id: "EVENT.THIS_OR_THAT",
                options: WIKI_EVENT_47_OPTIONS,
            },
            EventDef {
                id: "EVENT.TINKER_TIME",
                options: WIKI_EVENT_48_OPTIONS,
            },
            EventDef {
                id: "EVENT.TRASH_HEAP",
                options: WIKI_EVENT_49_OPTIONS,
            },
            EventDef {
                id: "EVENT.UNREST_SITE",
                options: WIKI_EVENT_50_OPTIONS,
            },
            EventDef {
                id: "EVENT.WAR_HISTORIAN_REPY",
                options: WIKI_EVENT_51_OPTIONS,
            },
            EventDef {
                id: "EVENT.WATERLOGGED_SCRIPTORIUM",
                options: WIKI_EVENT_52_OPTIONS,
            },
            EventDef {
                id: "EVENT.WELCOME_TO_WONGOS",
                options: WIKI_EVENT_53_OPTIONS,
            },
            EventDef {
                id: "EVENT.WELLSPRING",
                options: WIKI_EVENT_54_OPTIONS,
            },
            EventDef {
                id: "EVENT.WHISPERING_HOLLOW",
                options: WIKI_EVENT_55_OPTIONS,
            },
            EventDef {
                id: "EVENT.WOOD_CARVINGS",
                options: WIKI_EVENT_56_OPTIONS,
            },
            EventDef {
                id: "EVENT.ZEN_WEAVER",
                options: WIKI_EVENT_57_OPTIONS,
            },
            EventDef {
                id: "EVENT.NEOW",
                options: NEOW_OPTIONS,
            },
            EventDef {
                id: "EVENT.DARV",
                options: &NEOW_OPTIONS[..3],
            },
            EventDef {
                id: "EVENT.NONUPEIPE",
                options: &NEOW_OPTIONS[..3],
            },
            EventDef {
                id: "EVENT.OROBAS",
                options: &NEOW_OPTIONS[..3],
            },
            EventDef {
                id: "EVENT.TANX",
                options: &NEOW_OPTIONS[..3],
            },
            EventDef {
                id: "EVENT.TEZCATARA",
                options: &NEOW_OPTIONS[..3],
            },
            EventDef {
                id: "EVENT.PAEL",
                options: &[EventOption {
                    requirement: Requirement::Always,
                    effects: &[],
                }; 3],
            },
            EventDef {
                id: "EVENT.VAKUU",
                options: &[
                    EventOption {
                        requirement: Requirement::Always,
                        effects: &[],
                    },
                    EventOption {
                        requirement: Requirement::Always,
                        effects: &[RunEffect::MaxHp(-9)],
                    },
                    EventOption {
                        requirement: Requirement::Always,
                        effects: &[],
                    },
                ],
            },
        ],
        acts: vec![
            ActDef {
                id: "ACT.FOUNDATION",
                encounters: FOUNDATION_ENCOUNTERS,
                elites: FOUNDATION_ENCOUNTERS,
                bosses: FOUNDATION_ENCOUNTERS,
                events: NOTHING,
                cards: FOUNDATION_CARDS,
                relics: FOUNDATION_RELICS,
                potions: FOUNDATION_POTIONS,
            },
            ActDef {
                id: "ACT.THE_GLORY",
                encounters: THE_GLORY_ENCOUNTERS,
                elites: THE_GLORY_ELITES,
                bosses: THE_GLORY_BOSSES,
                events: THE_GLORY_EVENTS,
                cards: FOUNDATION_CARDS,
                relics: FOUNDATION_RELICS,
                potions: FOUNDATION_POTIONS,
            },
            ActDef {
                id: "ACT.THE_HIVE",
                encounters: THE_HIVE_ENCOUNTERS,
                elites: THE_HIVE_ELITES,
                bosses: THE_HIVE_BOSSES,
                events: THE_HIVE_EVENTS,
                cards: FOUNDATION_CARDS,
                relics: FOUNDATION_RELICS,
                potions: FOUNDATION_POTIONS,
            },
            ActDef {
                id: "ACT.THE_OVERGROWTH",
                encounters: THE_OVERGROWTH_ENCOUNTERS,
                elites: THE_OVERGROWTH_ELITES,
                bosses: THE_OVERGROWTH_BOSSES,
                events: THE_OVERGROWTH_EVENTS,
                cards: FOUNDATION_CARDS,
                relics: FOUNDATION_RELICS,
                potions: FOUNDATION_POTIONS,
            },
            ActDef {
                id: "ACT.THE_UNDERDOCKS",
                encounters: THE_UNDERDOCKS_ENCOUNTERS,
                elites: THE_UNDERDOCKS_ELITES,
                bosses: THE_UNDERDOCKS_BOSSES,
                events: THE_UNDERDOCKS_EVENTS,
                cards: FOUNDATION_CARDS,
                relics: FOUNDATION_RELICS,
                potions: FOUNDATION_POTIONS,
            },
        ],
        characters: vec![
            CharacterDef {
                id: "CHARACTER.IRONCLAD",
                hp: 80,
                gold: 99,
                energy: 3,
                draw: 5,
                orb_slots: 0,
                cards: FOUNDATION_CARDS,
                relic_pool: IRONCLAD_RELIC_POOL,
                potion_pool: IRONCLAD_POTIONS,
                deck: IRONCLAD_DECK,
                relics: STARTER_RELICS,
            },
            CharacterDef {
                id: "CHARACTER.DEFECT",
                hp: 75,
                gold: 99,
                energy: 3,
                draw: 5,
                orb_slots: 3,
                cards: DEFECT_CARDS,
                relic_pool: DEFECT_RELIC_POOL,
                potion_pool: DEFECT_POTIONS,
                deck: DEFECT_DECK,
                relics: DEFECT_RELICS,
            },
            CharacterDef {
                id: "CHARACTER.SILENT",
                hp: 70,
                gold: 99,
                energy: 3,
                draw: 5,
                orb_slots: 0,
                cards: SILENT_CARDS,
                relic_pool: SILENT_RELIC_POOL,
                potion_pool: SILENT_POTIONS,
                deck: SILENT_DECK,
                relics: SILENT_RELICS,
            },
            CharacterDef {
                id: "CHARACTER.REGENT",
                hp: 75,
                gold: 99,
                energy: 3,
                draw: 5,
                orb_slots: 0,
                cards: REGENT_CARDS,
                relic_pool: REGENT_RELIC_POOL,
                potion_pool: REGENT_POTIONS,
                deck: REGENT_DECK,
                relics: REGENT_RELICS,
            },
            CharacterDef {
                id: "CHARACTER.NECROBINDER",
                hp: 66,
                gold: 99,
                energy: 3,
                draw: 5,
                orb_slots: 0,
                cards: NECROBINDER_CARDS,
                relic_pool: NECROBINDER_RELIC_POOL,
                potion_pool: NECROBINDER_POTIONS,
                deck: NECROBINDER_DECK,
                relics: NECROBINDER_RELICS,
            },
        ],
        colorless,
    };
    for (id, name) in [
        (card_id::TOXIC, "CARD.TOXIC"),
        (card_id::DISINTEGRATION, "CARD.DISINTEGRATION"),
        (card_id::MIND_ROT, "CARD.MIND_ROT"),
        (card_id::SLOTH, "CARD.SLOTH"),
        (card_id::WASTE_AWAY, "CARD.WASTE_AWAY"),
        (card_id::BECKON, "CARD.BECKON"),
        (card_id::CORRUPTION, "CARD.CORRUPTION"),
        (card_id::MAUL, "CARD.MAUL"),
    ] {
        assert_eq!(content.cards[id as usize].id, name);
    }
    content.validate().expect("invalid foundation content");
    content
}

pub fn foundation_game(seed: u64) -> (Content, Game) {
    let content = foundation_content();
    let mut game = Game::new_character(&content, seed, 0).unwrap();
    game.begin_run(&content).unwrap();
    (content, game)
}
