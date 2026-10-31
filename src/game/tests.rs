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
fn corpse_slug_glomp_attacks_and_goop_applies_frail() {
    let content = foundation_content();
    let slug = content
        .enemies
        .iter()
        .find(|enemy| enemy.id == "MONSTER.CORPSE_SLUG")
        .unwrap();
    assert!(matches!(
        slug.moves[1].effects,
        [Effect::Attack(Target::Player, ..)]
    ));
    assert!(matches!(
        slug.moves[2].effects,
        [Effect::ApplyPower(
            Target::Player,
            power_id::FRAIL,
            Amount {
                base: 2,
                upgraded: 2,
                ..
            }
        )]
    ));
}

#[test]
fn piercing_wail_does_not_zero_glomp() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let slug = content
        .enemies
        .iter()
        .position(|enemy| enemy.id == "MONSTER.CORPSE_SLUG")
        .unwrap() as Id;
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 8;
    combat.enemies.truncate(1);
    let mut second = combat.enemies[0].clone();
    second.instance += 1;
    combat.enemies.push(second);
    for (index, enemy) in combat.enemies.iter_mut().enumerate() {
        enemy.creature.id = slug;
        enemy.creature.hp = 20;
        enemy.creature.powers.clear();
        enemy.creature.add_power(power_id::STRENGTH, -6);
        enemy.creature.add_power(power_id::PIERCING_WAIL, 6);
        enemy.move_index = index;
    }
    assert_eq!(
        game.incoming_damage_amount(&content, Actor::Enemy(1), 9, DamageKind::Attack),
        3
    );
    game.step(&content, Action::EndTurn).unwrap();
    assert_eq!(game.combat().unwrap().player.hp, 5);
}

#[test]
fn inferno_tracks_self_damage() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: content.card_id("CARD.INFERNO").unwrap(),
        ..Card::default()
    }];
    combat.energy = 10;
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    let inferno = game
        .combat()
        .unwrap()
        .player
        .powers
        .iter()
        .find(|power| {
            power.id
                == (content
                    .powers
                    .iter()
                    .position(|power| power.id == "POWER.INFERNO_POWER")
                    .unwrap() as Id)
        })
        .unwrap();
    assert_eq!(inferno.amount, 6);
    assert_eq!(
        game.combat()
            .unwrap()
            .player
            .power(power_id::INFERNO_DAMAGE),
        1
    );
    let hp = game.combat().unwrap().player.hp;

    game.start_turn();

    game.resolve(&content);
    assert_eq!(game.combat().unwrap().player.hp, hp - 1);
    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 94);
}

#[test]
fn i_am_invincible_waits_for_end_turn() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let strike = Card {
        id: content.card_id("CARD.STRIKE_REGENT").unwrap(),
        ..Card::default()
    };
    let invincible = Card {
        id: card_id::I_AM_INVINCIBLE,
        ..Card::default()
    };
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![strike];
    combat.draw = vec![invincible];
    combat.energy = 3;
    combat.enemies[0].creature.hp = 100;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().draw, [invincible]);
    assert_eq!(game.combat().unwrap().player.block, 0);
}

#[test]
fn nostalgia_returns_i_am_invincible_to_draw_only_once() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: card_id::I_AM_INVINCIBLE,
        ..Card::default()
    }];
    combat.draw.clear();
    combat.forget_draw_order();
    combat.energy = 3;
    combat.enemies[0].creature.hp = 100;
    combat.player.add_power(power_id::NOSTALGIA, 1);

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    let top = *game.combat().unwrap().draw.last().unwrap();
    assert_eq!(top.id, card_id::I_AM_INVINCIBLE);
    assert_eq!(top.flags & RETURN_TO_DRAW, 0);

    game.step(&content, Action::EndTurn).unwrap();
}

#[test]
fn pillage_with_hellraiser_stops_drawing_once_enemies_are_dead() {
    let content = foundation_content();
    let ironclad = content
        .characters
        .iter()
        .position(|character| character.id == "CHARACTER.IRONCLAD")
        .unwrap() as Id;
    let mut game = Game::new_character_ascension(&content, 1, ironclad, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let card = |name| Card {
        id: content.card_id(name).unwrap(),
        ..Card::default()
    };
    let combat = game.combat_mut().unwrap();
    combat.choice = None;
    combat.hand = vec![card("CARD.PILLAGE")];
    combat.draw.clear();
    combat.discard = vec![card("CARD.STRIKE_IRONCLAD"), card("CARD.STRIKE_IRONCLAD")];
    combat.forget_draw_order();
    combat.energy = 3;
    for enemy in &mut combat.enemies {
        enemy.creature.hp = 40;
        enemy.creature.block = 0;
    }
    combat.player.add_power(power_id::HELLRAISER, 1);
    let pillage = Action::Play {
        hand: 0,
        target: Some(0),
    };

    game.expected_action_outcome(&content, &pillage);
    game.step(&content, pillage).unwrap();

    assert!(
        game.combat()
            .is_none_or(|combat| combat.enemies.iter().all(|enemy| enemy.creature.hp <= 0))
    );
}

#[test]
fn hellraiser_caps_auto_plays_against_a_hardened_shell() {
    let content = foundation_content();
    let ironclad = content
        .characters
        .iter()
        .position(|character| character.id == "CHARACTER.IRONCLAD")
        .unwrap() as Id;
    let colony = content
        .encounters()
        .iter()
        .position(|encounter| encounter.id == "ENCOUNTER.SKULKING_COLONY_ELITE")
        .unwrap() as Id;
    let mut game = Game::new_character_ascension(&content, 1, ironclad, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, colony).unwrap();
    let strike = Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        ..Card::default()
    };
    let combat = game.combat_mut().unwrap();
    combat.choice = None;
    combat.hand = vec![Card {
        id: content.card_id("CARD.PILLAGE").unwrap(),
        ..Card::default()
    }];
    combat.draw.clear();
    combat.discard = vec![strike, strike];
    combat.forget_draw_order();
    combat.energy = 3;
    combat.player.add_power(power_id::HELLRAISER, 1);
    let pillage = Action::Play {
        hand: 0,
        target: Some(0),
    };

    let hp = combat.enemies[0].creature.hp;

    game.expected_action_outcome(&content, &pillage);
    game.step(&content, pillage).unwrap();

    let combat = game.combat().unwrap();
    assert_eq!(combat.enemies[0].creature.hp, hp - 20);
    assert_eq!(combat.player.power(power_id::HELLRAISER), 1);
    assert!(combat.hand.iter().all(|card| card.id == strike.id));
}

#[test]
fn returning_bolas_from_a_known_draw_pile_keeps_known_counts() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let bolas = content.card_id("CARD.BOLAS").unwrap();
    let cosmic = content.card_id("CARD.COSMIC_INDIFFERENCE").unwrap();
    let combat = game.combat_mut().unwrap();
    combat.choice = None;
    combat.hand = [bolas, cosmic, cosmic]
        .map(|id| Card {
            id,
            ..Card::default()
        })
        .to_vec();
    combat.draw.clear();
    combat.discard.clear();
    combat.forget_draw_order();
    combat.energy = 10;
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();
    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    let combat = game.combat().unwrap();
    assert_eq!(
        combat.draw.iter().map(|card| card.id).collect::<Vec<_>>(),
        [bolas, cosmic]
    );
    assert_eq!(combat.known_draw_top, 2);

    game.step(&content, Action::EndTurn).unwrap();
    let combat = game.combat().unwrap();
    assert!(combat.known_draw_bottom + combat.known_draw_top <= combat.draw.len());
    assert!(combat.hand.iter().any(|card| card.id == bolas));
}

#[test]
fn uproar_auto_plays_a_draw_pile_attack() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let uproar = Card {
        id: content.card_id("CARD.UPROAR").unwrap(),
        ..Card::default()
    };
    let beam = Card {
        id: content.card_id("CARD.SWEEPING_BEAM").unwrap(),
        ..Card::default()
    };
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![uproar];
    combat.draw = vec![beam];
    combat.energy = 3;
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 82);
    assert_eq!(game.combat().unwrap().discard, [beam, uproar]);
}

#[test]
fn crash_landing_only_fills_the_hand() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let debris = content.card_id("CARD.DEBRIS").unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = std::iter::once(Card {
        id: content.card_id("CARD.CRASH_LANDING").unwrap(),
        ..Card::default()
    })
    .chain((0..2).map(|_| Card {
        id: content.card_id("CARD.DEFEND_REGENT").unwrap(),
        ..Card::default()
    }))
    .collect();
    combat.energy = 3;
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().hand.len(), 10);
    assert!(
        !game
            .combat()
            .unwrap()
            .discard
            .iter()
            .any(|card| card.id == debris)
    );
}

#[test]
fn hologram_gains_block() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: content.card_id("CARD.HOLOGRAM").unwrap(),
        ..Card::default()
    }];
    combat.discard = vec![Card {
        id: content.card_id("CARD.STRIKE_DEFECT").unwrap(),
        ..Card::default()
    }];
    combat.energy = 3;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().player.block, 3);
}

#[test]
fn cosmic_indifference_returns_a_discard_to_draw() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let venerate = Card {
        id: content.card_id("CARD.VENERATE").unwrap(),
        ..Card::default()
    };
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: content.card_id("CARD.COSMIC_INDIFFERENCE").unwrap(),
        ..Card::default()
    }];
    combat.discard = vec![venerate];
    combat.energy = 3;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().player.block, 6);
    assert_eq!(game.combat().unwrap().draw.last(), Some(&venerate));
}

#[test]
fn the_bomb_shows_its_countdown() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: content.card_id("CARD.THE_BOMB").unwrap(),
        ..Card::default()
    }];
    combat.energy = 3;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    assert_eq!(game.combat().unwrap().player.power(power_id::THE_BOMB), 3);

    game.step(&content, Action::EndTurn).unwrap();
    assert_eq!(game.combat().unwrap().player.power(power_id::THE_BOMB), 2);
}

#[test]
fn panache_instances_count_their_own_plays() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies.truncate(1);
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;
    let card = |id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    };
    combat.hand = vec![card("CARD.PANACHE"), card("CARD.PANACHE")];
    combat.hand.extend([card("CARD.DEFEND_IRONCLAD"); 4]);
    combat.energy = 10;
    let play = |game: &mut Game| {
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        game.step(&content, action).unwrap();
    };
    let lefts = |game: &Game| {
        let powers = game.combat().unwrap().player.powers.iter();
        powers
            .filter(|x| x.id == power_id::PANACHE)
            .map(|x| x.value)
            .collect::<Vec<_>>()
    };
    for _ in 0..5 {
        play(&mut game);
    }
    // Each Panache skips its own play: the first has counted 4 cards, the second 3.
    assert_eq!(lefts(&game), [1, 2]);
    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 100);
    play(&mut game);
    assert_eq!(lefts(&game), [5, 1]);
    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 90);
}

#[test]
fn bolas_returns_before_the_next_draw() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let bolas = content.card_id("CARD.BOLAS").unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: bolas,
        ..Card::default()
    }];
    combat.energy = 3;
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();
    game.step(&content, Action::EndTurn).unwrap();

    assert!(
        game.combat()
            .unwrap()
            .hand
            .iter()
            .any(|card| card.id == bolas)
    );
}

#[test]
fn gambling_chip_sees_ring_of_the_snake_draws() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.obtain_relic(&content, content.relic_id("RELIC.GAMBLING_CHIP").unwrap());
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();

    assert_eq!(game.combat().unwrap().hand.len(), 7);
    assert_eq!(game.combat().unwrap().choice.unwrap().remaining, 7);
}

#[test]
fn choosing_the_whole_draw_pile_preserves_top_order() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let strike = Card {
        id: content.card_id("CARD.STRIKE_REGENT").unwrap(),
        ..Card::default()
    };
    let defend = Card {
        id: content.card_id("CARD.DEFEND_REGENT").unwrap(),
        ..Card::default()
    };
    let combat = game.combat_mut().unwrap();
    combat.choice = None;
    combat.hand.clear();
    combat.draw = vec![strike, defend];
    combat.queue.push(Pending {
        effect: Effect::ChooseDraw(Amount::fixed(2, 2)),
        context: Context::player(),
    });

    game.resolve(&content);

    assert_eq!(game.combat().unwrap().hand, [defend, strike]);
}

#[test]
fn shuffle_sort_matches_dotnet_equal_card_order() {
    let content = foundation_content();
    let mut cards = [
        ("CARD.STRIKE_NECROBINDER", 13, 0),
        ("CARD.UNLEASH", 6, 0),
        ("CARD.SLIMED", 18, 0),
        ("CARD.SLIMED", 19, 0),
        ("CARD.SLIMED", 20, 0),
        ("CARD.SPUR", 9, 0),
        ("CARD.DEFEND_NECROBINDER", 7, 0),
        ("CARD.STRIKE_NECROBINDER", 2, 1),
        ("CARD.SLIMED", 21, 0),
        ("CARD.STRIKE_NECROBINDER", 4, 0),
        ("CARD.STRIKE_NECROBINDER", 10, 0),
        ("CARD.DEFEND_NECROBINDER", 11, 0),
        ("CARD.SLIMED", 15, 0),
        ("CARD.BODYGUARD", 3, 0),
        ("CARD.SLIMED", 16, 0),
        ("CARD.SLIMED", 17, 0),
        ("CARD.SLIMED", 14, 0),
        ("CARD.DEFEND_NECROBINDER", 5, 0),
        ("CARD.SLIMED", 22, 0),
        ("CARD.SLIMED", 23, 0),
    ]
    .map(|(id, instance, upgrades)| Card {
        id: content.card_id(id).unwrap(),
        instance,
        upgrades,
        ..Card::default()
    });

    combat::sort_for_shuffle(&mut cards, &content);
    assert_eq!(
        cards.map(|card| card.instance),
        [
            3, 5, 7, 11, 23, 18, 19, 20, 14, 21, 22, 17, 15, 16, 9, 4, 10, 13, 2, 6
        ]
    );
}

#[test]
fn omnislice_splashes_its_target_damage_unpowered() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 4, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let mut enemy = combat.enemies[0].clone();
    enemy.instance += 1;
    combat.enemies.push(enemy);
    for enemy in &mut combat.enemies {
        enemy.creature.hp = 100;
        enemy.creature.max_hp = 100;
        enemy.creature.block = 0;
        enemy.creature.powers.clear();
    }
    combat.enemies[0]
        .creature
        .add_power(power_id::VULNERABLE, 1);
    combat.hand = vec![Card {
        id: content.card_id("CARD.OMNISLICE").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    let enemies = &game.combat().unwrap().enemies;
    assert_eq!(enemies[0].creature.hp, 88);
    assert_eq!(enemies[1].creature.hp, 88);
}

#[test]
fn tesla_coil_triggers_lightning_on_its_target() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let mut enemy = combat.enemies[0].clone();
    enemy.instance += 1;
    combat.enemies.push(enemy);
    for enemy in &mut combat.enemies {
        enemy.creature.hp = 100;
        enemy.creature.max_hp = 100;
        enemy.creature.block = 0;
        enemy.creature.powers.clear();
    }
    combat.hand = vec![Card {
        id: content.card_id("CARD.TESLA_COIL").unwrap(),
        ..Card::default()
    }];
    combat.orbs = vec![
        Orb {
            id: orb_id::LIGHTNING,
            value: 0,
        };
        2
    ];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(1),
        },
    )
    .unwrap();

    let enemies = &game.combat().unwrap().enemies;
    assert_eq!(enemies[0].creature.hp, 100);
    assert_eq!(enemies[1].creature.hp, 91);
}

#[test]
fn rolling_boulder_ticks_damage_and_display() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;
    combat.player.add_instance(power_id::ROLLING_BOULDER, 5, 0);

    let boulder = game.combat().unwrap().player.powers[0];
    game.tick_boulder(&content, boulder);
    game.resolve(&content);

    let combat = game.combat().unwrap();
    assert_eq!(combat.enemies[0].creature.hp, 95);
    assert_eq!(combat.player.power(power_id::ROLLING_BOULDER), 10);
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
    let mut expected_rng = game.rngs.combat_card_selection;
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
        until_played: true,
        ..Card::default()
    };
    // A combat-long cost (Confused) stays once played.
    let combat_long = Card {
        cost_override: Some(2),
        until_played: false,
        ..card
    };
    game.place_card(&content, combat_long, false, false);
    let discard = &game.combat().unwrap().discard;
    assert_eq!(discard.last().unwrap().cost_override, Some(2));

    let exhausted = game.place_card(&content, card, false, false);

    assert!(!exhausted);
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
        assert_eq!(first.rngs.up_front.1, draws);
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
        combat.plays.clear();
        game.finish_combat(content);
    }

    fn enter_first_boss(game: &mut Game, content: &Content) {
        let boss = game
            .map
            .nodes
            .iter()
            .position(|node| node.room == Room::Boss && (game.run.act < 3 || !node.next.is_empty()))
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
fn cooking_uses_a_cancelable_completion() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.phase = Phase::Rest;
    game.run
        .relics
        .push(content.relic_id("RELIC.MEAT_CLEAVER").unwrap());
    let max_hp = game.run.max_hp;
    game.step(&content, Action::Cook).unwrap();
    assert_eq!(game.run_queue.last(), Some(&RunEffect::FinishCooking));
    game.step(&content, Action::Cancel).unwrap();
    assert!(game.run_queue.is_empty());
    assert_eq!(game.run.max_hp, max_hp);
    game.step(&content, Action::Cook).unwrap();
    for _ in 0..2 {
        let action = game
            .actions(&content)
            .into_iter()
            .find(|action| matches!(action, Action::RemoveCard(_)))
            .unwrap();
        game.step(&content, action).unwrap();
    }
    assert_eq!(game.run.max_hp, max_hp + 9);
}

#[test]
fn neows_bones_curse_is_a_reward_completion() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    let relic = content.relic_id("RELIC.CIRCLET").unwrap();
    game.phase = Phase::Rewards(Rewards {
        gold: 0,
        cards: vec![],
        card_rewards: vec![],
        relics: vec![relic, relic],
        potions: vec![],
        removals: 0,
    });
    game.reward_continuation = Some(RewardContinuation::AddRandomCurse);
    let cards = game.run.deck.len();
    game.step(&content, Action::RewardRelic(0)).unwrap();
    assert!(game.reward_continuation.is_some());
    game.step(&content, Action::RewardRelic(0)).unwrap();
    assert!(game.reward_continuation.is_none());
    assert_eq!(game.run.deck.len(), cards + 1);
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
            .any(|pending| pending.effect == Effect::Step(Step::Flush))
    );
    assert_eq!(
        game.actions(&content),
        [Action::Choose(0), Action::Choose(1)]
    );

    game.step(&content, Action::Choose(0)).unwrap();

    let combat = game.combat().unwrap();
    assert!(combat.choice.is_none());
    assert!(combat.queue.is_empty());
    assert!(combat.plays.is_empty());
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
    game.apply_power(&content, Actor::Player, power_id::SMOGGY, 1);
    assert_eq!(game.combat().unwrap().player.power(power_id::SMOGGY), 1);
    assert!(game.actions(&content).contains(&Action::Play {
        hand: 0,
        target: None
    }));

    // A finished skill play smogs the unafflicted skills.
    game.combat_mut().unwrap().history.skills = 1;
    game.afflict_all(&content, Affliction::Smog, &[CardType::Skill]);
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
    combat.plays.clear();

    game.finish_combat(&content);

    assert_eq!(game.fishing_rod, 0);
    assert_eq!(game.run.deck[0].upgrades, 1);
    assert!(matches!(game.phase, Phase::Rewards(_)));
}

fn snapshot_play(enemy_powers: Vec<Vec<Power>>) -> Play {
    let recorded = enemy_powers.into_iter().enumerate();
    let recorded =
        recorded.flat_map(|(i, powers)| powers.into_iter().map(move |x| (Actor::Enemy(i), x)));
    Play {
        card: Card::default(),
        plays: 1,
        done: 0,
        energy: 0,
        stars: 0,
        manual: true,
        context: Context::player(),
        recorded: recorded.collect(),
    }
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
    combat.plays.push(snapshot_play(vec![vec![Power {
        id: power_id::STRENGTH,
        amount: 2,
        ..Power::default()
    }]]));

    let slot = game.spawn_enemy(&content, "MONSTER.TOUGH_EGG");

    let combat = game.combat().unwrap();
    assert_eq!(slot, 0);
    assert_eq!(combat.enemies.len(), 1);
    assert_eq!(combat.hits, [0]);
    assert!(combat.plays[0].recorded.is_empty());
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
    combat.plays.push(snapshot_play(vec![
        vec![Power {
            id: power_id::STRENGTH,
            amount: 1,
            ..Power::default()
        }];
        3
    ]));
    combat.enemies[0].creature.hp = 0;
    combat.enemies[1].creature.id = merc;
    combat.enemies[1].creature.hp = 1;
    combat.enemies[1].creature.powers = vec![
        Power {
            id: power_id::SURPRISE,
            amount: 1,
            ..Power::default()
        },
        Power {
            id: power_id::THIEVERY,
            amount: 1,
            value: 17,
            ..Power::default()
        },
    ];

    game.kill_actor(&content, Actor::Enemy(1));

    let combat = game.combat().unwrap();
    assert_eq!(combat.enemies.len(), 3);
    assert_eq!(combat.hits[..2], [0, 0]);
    let recorded = combat.plays[0].recorded.iter().map(|(actor, _)| *actor);
    assert!(recorded.eq([Actor::Enemy(2)]));
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
fn stock_reuses_dead_slots_and_wrigglers_append() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let template = game.combat().unwrap().enemies[0].clone();
    let combat = game.combat_mut().unwrap();
    combat.enemies = vec![template.clone(), template];
    combat.hits = vec![9, 8];
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
    assert_eq!(combat.enemies.len(), 3);
    assert_eq!(combat.hits[2], 0);
    assert_eq!(
        content.enemies[combat.enemies[2].creature.id as usize].id,
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

    game.start_turn();

    game.resolve(&content);

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
fn osty_attack_overflow_damages_player() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 4, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 50;
    combat.player.block = 3;
    combat.player.powers.clear();
    combat.osty.hp = 2;
    combat.osty.max_hp = 2;
    combat.osty.powers.clear();
    combat.enemies[0].creature.powers.clear();

    let result = game.damage(
        &content,
        Actor::Enemy(0),
        Actor::Player,
        10,
        DamageKind::Attack,
        None,
    );

    let combat = game.combat().unwrap();
    assert_eq!(combat.osty.hp, 0);
    assert_eq!(combat.player.hp, 45);
    assert_eq!(combat.history.hp_lost, 5);
    assert_eq!(result.lost, 7);
}

#[test]
fn direct_osty_damage_does_not_overflow() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 4, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 50;
    combat.player.block = 2;
    combat.osty.hp = 1;
    combat.osty.max_hp = 1;

    game.damage(
        &content,
        Actor::Enemy(0),
        Actor::Osty,
        2,
        DamageKind::Unpowered,
        None,
    );

    let combat = game.combat().unwrap();
    assert_eq!(combat.player.block, 0);
    assert_eq!(combat.osty.hp, 1);
    assert_eq!(combat.player.hp, 50);

    game.damage(
        &content,
        Actor::Enemy(0),
        Actor::Osty,
        2,
        DamageKind::Unpowered,
        None,
    );

    let combat = game.combat().unwrap();
    assert_eq!(combat.osty.hp, 0);
    assert_eq!(combat.player.hp, 50);
}

#[test]
fn necro_mastery_can_stop_a_multi_hit_attacker() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 4, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 42;
    combat.player.block = 2;
    combat.player.add_power(power_id::NECRO_MASTERY, 1);
    combat.osty.hp = 4;
    combat.osty.max_hp = 4;
    combat.enemies[0].creature.hp = 3;
    combat.enemies[0].creature.powers.clear();
    combat.queue.push(Pending {
        effect: Effect::Attack(Target::Player, Amount::fixed(4, 4), 3),
        context: Context {
            source: Actor::Enemy(0),
            ..Context::player()
        },
    });

    game.resolve(&content);

    assert_eq!(game.combat().unwrap().player.hp, 40);
    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 0);
}

#[test]
fn multi_hit_retargets_after_osty_dies() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 4, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 4;
    combat.player.block = 3;
    combat.osty.hp = 1;
    combat.osty.max_hp = 1;
    combat.enemies[0].creature.powers.clear();
    combat.queue.push(Pending {
        effect: Effect::Attack(Target::Player, Amount::fixed(4, 4), 3),
        context: Context {
            source: Actor::Enemy(0),
            ..Context::player()
        },
    });

    game.resolve(&content);

    assert_eq!(game.combat().unwrap().player.hp, 0);
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
fn glass_orb_evokes_for_twice_its_remaining_value() {
    let content = foundation_content();
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
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.powers.clear();
    combat.player.add_power(power_id::THUNDER, 6);
    combat.orb_slots = 1;
    combat.orbs = vec![Orb {
        id: glass,
        value: 3,
    }];
    combat.queue.push(Pending {
        effect: Effect::Channel(orb_id::LIGHTNING, 1),
        context: Context::player(),
    });

    game.resolve(&content);

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 94);

    game.combat_mut().unwrap().enemies[0].creature.hp = 100;
    game.combat_mut().unwrap().queue.push(Pending {
        effect: Effect::Evoke(true),
        context: Context::player(),
    });
    game.resolve(&content);

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 86);
}

#[test]
fn regret_ignores_block() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 50;
    combat.player.block = 5;
    combat.hand = vec![Card {
        id: content.card_id("CARD.REGRET").unwrap(),
        ..Card::default()
    }];
    combat.enemies[0].move_index = 1;

    game.step(&content, Action::EndTurn).unwrap();

    assert_eq!(game.combat().unwrap().player.hp, 49);
}

#[test]
fn curse_debuffs_survive_their_application_turn() {
    let content = foundation_content();
    for (card, power) in [
        ("CARD.DOUBT", power_id::WEAK),
        ("CARD.SHAME", power_id::FRAIL),
    ] {
        let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let combat = game.combat_mut().unwrap();
        combat.hand = vec![Card {
            id: content.card_id(card).unwrap(),
            ..Card::default()
        }];
        combat.enemies[0].move_index = 1;

        game.step(&content, Action::EndTurn).unwrap();

        assert_eq!(game.combat().unwrap().player.power(power), 1);
    }
}

#[test]
fn debt_loses_gold_without_going_negative() {
    let content = foundation_content();
    for (gold, expected) in [(15, 5), (7, 0)] {
        let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.gold = gold;
        let combat = game.combat_mut().unwrap();
        combat.hand = vec![Card {
            id: content.card_id("CARD.DEBT").unwrap(),
            ..Card::default()
        }];
        combat.enemies[0].move_index = 1;

        game.step(&content, Action::EndTurn).unwrap();

        assert_eq!(game.run.gold, expected);
    }
}

#[test]
fn ethereal_cards_exhaust_when_the_hand_is_retained() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let defend = Card {
        id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
        ..Card::default()
    };
    let bane = Card {
        id: content.card_id("CARD.ASCENDERS_BANE").unwrap(),
        ..Card::default()
    };
    let regret = Card {
        id: content.card_id("CARD.REGRET").unwrap(),
        ..Card::default()
    };
    let slimed = Card {
        id: content.card_id("CARD.SLIMED").unwrap(),
        ..Card::default()
    };
    combat.hand = vec![bane, defend, regret, slimed];
    combat.player.add_power(power_id::RETAIN_HAND, 1);
    combat.enemies[0].move_index = 1;

    game.step(&content, Action::EndTurn).unwrap();

    let combat = game.combat().unwrap();
    assert!(combat.hand.contains(&defend));
    assert!(combat.hand.contains(&slimed));
    assert!(combat.exhaust.contains(&bane));
    assert!(combat.discard.contains(&regret));
}

#[test]
fn enemy_vigor_strengthens_one_attack() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 10).unwrap();
    game.begin_act(&content, 1).unwrap();
    let encounter = content
        .encounters
        .iter()
        .position(|encounter| encounter.id == "ENCOUNTER.TERROR_EEL_ELITE")
        .unwrap() as Id;
    game.start_combat(&content, encounter).unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 68;
    combat.player.block = 10;
    combat.enemies[0].move_index = 0;
    combat.enemies[0].creature.add_power(power_id::VIGOR, 6);

    game.step(&content, Action::EndTurn).unwrap();

    let combat = game.combat().unwrap();
    assert_eq!(combat.player.hp, 54);
    assert_eq!(combat.enemies[0].creature.power(power_id::VIGOR), 0);
}

#[test]
fn lethality_multiplies_strength_before_shrink() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 4, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: content.card_id("CARD.STRIKE_NECROBINDER").unwrap(),
        ..Card::default()
    }];
    combat.energy = 3;
    combat.player.add_power(power_id::STRENGTH, 2);
    combat.player.add_power(power_id::LETHALITY, 75);
    combat.player.add_power(power_id::SHRINK, -1);
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 91);
}

#[test]
fn gold_axe_counts_the_whole_combat() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let strike = Card {
        id: content.card_id("CARD.STRIKE_REGENT").unwrap(),
        ..Card::default()
    };
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![strike, strike];
    combat.energy = 3;
    combat.enemies[0].creature.hp = 1000;
    combat.enemies[0].creature.max_hp = 1000;
    for _ in 0..2 {
        game.step(
            &content,
            Action::Play {
                hand: 0,
                target: Some(0),
            },
        )
        .unwrap();
    }
    game.step(&content, Action::EndTurn).unwrap();
    game.combat_mut().unwrap().hand = vec![Card {
        id: content.card_id("CARD.GOLD_AXE").unwrap(),
        ..Card::default()
    }];
    game.combat_mut().unwrap().energy = 3;
    let hp = game.combat().unwrap().enemies[0].creature.hp;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, hp - 2);
}

#[test]
fn monarchs_gaze_restores_strength_after_enemy_turn() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: content.card_id("CARD.STRIKE_REGENT").unwrap(),
        ..Card::default()
    }];
    combat.energy = 3;
    combat.player.add_power(
        content
            .powers
            .iter()
            .position(|power| power.id == "POWER.MONARCHS_GAZE_POWER")
            .unwrap() as Id,
        1,
    );
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();
    assert_eq!(
        game.combat().unwrap().enemies[0]
            .creature
            .power(power_id::STRENGTH),
        -1
    );
    assert_eq!(
        game.combat().unwrap().enemies[0]
            .creature
            .power(power_id::MONARCHS_GAZE_STRENGTH_DOWN),
        1
    );

    game.step(&content, Action::EndTurn).unwrap();
    assert_eq!(
        game.combat().unwrap().enemies[0]
            .creature
            .power(power_id::STRENGTH),
        0
    );
}

#[test]
fn thorns_stops_a_dead_multi_hit_attacker() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 50;
    combat.player.add_power(
        content
            .powers
            .iter()
            .position(|power| power.id == "POWER.THORNS_POWER")
            .unwrap() as Id,
        3,
    );
    combat.enemies[0].creature.hp = 2;
    combat.enemies[0].creature.powers.clear();
    combat.queue.push(Pending {
        effect: Effect::Attack(Target::Player, Amount::fixed(6, 6), 2),
        context: Context {
            source: Actor::Enemy(0),
            ..Context::player()
        },
    });

    game.resolve(&content);

    let combat = game.combat().unwrap();
    assert_eq!(combat.player.hp, 44);
    assert_eq!(combat.enemies[0].creature.hp, 0);
}

#[test]
fn suck_strength_applies_after_all_hits() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let fossil = content
        .enemies
        .iter()
        .position(|enemy| enemy.id == "MONSTER.FOSSIL_STALKER")
        .unwrap() as Id;
    let combat = game.combat_mut().unwrap();
    combat.player.hp = 16;
    combat.player.block = 5;
    combat.enemies.truncate(1);
    let enemy = &mut combat.enemies[0];
    enemy.creature.id = fossil;
    enemy.creature.hp = 100;
    enemy.creature.powers.clear();
    enemy.creature.add_power(power_id::SUCK, 3);
    enemy.creature.add_power(power_id::STRENGTH, 6);
    enemy.creature.add_power(power_id::WEAK, 1);
    enemy.move_index = 2;

    game.step(&content, Action::EndTurn).unwrap();

    assert_eq!(game.combat().unwrap().player.hp, 7);
    assert_eq!(
        game.combat().unwrap().enemies[0]
            .creature
            .power(power_id::STRENGTH),
        12
    );
}

#[test]
fn strangle_expires_after_the_enemy_turn() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    game.combat_mut().unwrap().enemies[0]
        .creature
        .add_power(power_id::STRANGLE, 2);

    game.step(&content, Action::EndTurn).unwrap();

    assert_eq!(
        game.combat().unwrap().enemies[0]
            .creature
            .power(power_id::STRANGLE),
        0
    );
}

#[test]
fn enemy_rolls_before_turn_start_orb_damage() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let spinner = content
        .enemies
        .iter()
        .position(|enemy| enemy.id == "MONSTER.SLUDGE_SPINNER")
        .unwrap() as Id;
    let loop_power = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.LOOP_POWER")
        .unwrap() as Id;
    let combat = game.combat_mut().unwrap();
    combat.enemies.truncate(1);
    combat.enemies[0].creature.id = spinner;
    combat.enemies[0].creature.hp = 1;
    combat.enemies[0].move_index = 2;
    combat.enemies[0].last_move = 2;
    combat.player.add_power(loop_power, 1);
    combat.orbs = vec![Orb {
        id: orb_id::LIGHTNING,
        value: 0,
    }];
    let rolls = game.rngs.monster_ai.1;

    game.start_turn();

    game.resolve(&content);

    assert_eq!(game.rngs.monster_ai.1, rolls + 1);
    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 0);
}

#[test]
fn cubex_construct_starts_without_block() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let encounter = content
        .encounters
        .iter()
        .position(|encounter| encounter.id == "ENCOUNTER.CUBEX_CONSTRUCT_NORMAL")
        .unwrap() as Id;

    game.start_combat(&content, encounter).unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.block, 0);
}

#[test]
fn echo_form_duplicates_only_the_first_card() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let strike = Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        ..Card::default()
    };
    combat.hand = vec![strike; 2];
    combat.energy = 2;
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.powers.clear();
    combat.player.add_power(power_id::ECHO_FORM, 1);

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();
    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 82);
}

#[test]
fn ritual_starts_gaining_strength_on_the_next_turn() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let cultist = content
        .enemies
        .iter()
        .position(|enemy| enemy.id == "MONSTER.CALCIFIED_CULTIST")
        .unwrap() as Id;
    let enemy = &mut game.combat_mut().unwrap().enemies[0];
    enemy.creature.id = cultist;
    enemy.move_index = 0;
    enemy.last_move = 0;

    game.step(&content, Action::EndTurn).unwrap();

    let enemy = &game.combat().unwrap().enemies[0].creature;
    assert_eq!(enemy.power(power_id::RITUAL), 2);
    assert_eq!(enemy.power(power_id::STRENGTH), 0);
}

#[test]
fn anticipate_restores_dexterity_at_turn_end() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let player = &mut game.combat_mut().unwrap().player;
    player.add_power(power_id::DEXTERITY, 2);
    player.add_power(power_id::ANTICIPATE, 2);

    game.step(&content, Action::EndTurn).unwrap();

    let player = &game.combat().unwrap().player;
    assert_eq!(player.power(power_id::DEXTERITY), 0);
    assert_eq!(player.power(power_id::ANTICIPATE), 0);
}

#[test]
fn infinite_blades_creates_one_shiv_per_stack() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand.clear();
    combat.draw.clear();
    combat.player.add_power(20, 2);

    game.start_turn();

    game.resolve(&content);

    let shiv = content.card_id("CARD.SHIV").unwrap();
    assert_eq!(
        game.combat()
            .unwrap()
            .hand
            .iter()
            .filter(|card| card.id == shiv)
            .count(),
        2
    );
}

#[test]
fn minions_do_not_keep_combat_open() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    let encounter = content
        .encounters
        .iter()
        .position(|encounter| encounter.id == "ENCOUNTER.FOGMOG_NORMAL")
        .unwrap() as Id;
    game.start_combat(&content, encounter).unwrap();
    game.combat_mut().unwrap().enemies[0].creature.hp = 0;
    game.spawn_enemy(&content, "MONSTER.EYE_WITH_TEETH");

    game.finish_combat(&content);

    assert!(!matches!(game.phase, Phase::Combat(_)));
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
fn frail_does_not_reduce_frost_block() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let frost = content
        .orbs
        .iter()
        .position(|orb| orb.id == "ORB.FROST_ORB")
        .unwrap() as Id;
    let combat = game.combat_mut().unwrap();
    combat.player.block = 0;
    combat.player.add_power(power_id::FRAIL, 2);
    combat.orbs = vec![Orb {
        id: frost,
        value: 0,
    }];

    game.passive_orb(&content, Some(0));
    game.resolve(&content);

    assert_eq!(game.combat().unwrap().player.block, 2);
}

#[test]
fn end_turn_orbs_use_temporary_focus() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.orbs = vec![Orb {
        id: orb_id::LIGHTNING,
        value: 0,
    }];
    combat.player.add_power(power_id::FOCUS, 1);
    combat.player.add_power(power_id::FOCUSED_STRIKE, 1);
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;

    game.step(&content, Action::EndTurn).unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 96);
    assert_eq!(game.combat().unwrap().player.power(power_id::FOCUS), 0);
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
    game.start_turn();
    game.resolve(&content);
    assert_eq!(game.combat().unwrap().stars, 6);
}

#[test]
fn furnace_forges_after_hand_draw() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let strike = Card {
        id: content.card_id("CARD.STRIKE_REGENT").unwrap(),
        ..Card::default()
    };
    let furnace = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.FURNACE_POWER")
        .unwrap() as Id;
    let combat = game.combat_mut().unwrap();
    combat.hand.clear();
    combat.draw = vec![strike];
    combat.discard.clear();
    combat.draw_per_turn = 1;
    combat.player.powers.clear();
    combat.player.add_power(furnace, 5);

    game.start_turn();

    game.resolve(&content);

    assert_eq!(
        game.combat()
            .unwrap()
            .hand
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>(),
        [strike.id, card_id::SOVEREIGN_BLADE]
    );
}

#[test]
fn vambrace_only_doubles_the_first_block_card_of_combat() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 3, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.run
        .relics
        .push(content.relic_id("RELIC.VAMBRACE").unwrap());
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let defend = Card {
        id: content.card_id("CARD.DEFEND_REGENT").unwrap(),
        ..Card::default()
    };
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![defend];
    combat.energy = 10;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    assert_eq!(game.combat().unwrap().player.block, 10);

    let combat = game.combat_mut().unwrap();
    combat.history = History::default();
    combat.player.block = 0;
    combat.hand = vec![defend];
    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    assert_eq!(game.combat().unwrap().player.block, 5);
}

#[test]
fn barrage_finishes_all_hits_before_skittish_blocks() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: content.card_id("CARD.BARRAGE").unwrap(),
        ..Card::default()
    }];
    combat.energy = 3;
    combat.orbs = vec![
        Orb {
            id: orb_id::LIGHTNING,
            value: 0,
        },
        Orb {
            id: orb_id::LIGHTNING,
            value: 0,
        },
    ];
    combat.enemies.truncate(1);
    combat.enemies[0].creature.hp = 50;
    combat.enemies[0].creature.max_hp = 50;
    combat.enemies[0].creature.block = 0;
    combat.enemies[0].creature.powers.clear();
    combat.enemies[0].creature.add_power(power_id::SKITTISH, 7);

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 40);
    assert_eq!(game.combat().unwrap().enemies[0].creature.block, 7);
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
fn random_upgrade_shuffles_every_candidate() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 4, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let strike = content.card_id("CARD.STRIKE_NECROBINDER").unwrap();
    let drain = content.card_id("CARD.DRAIN_POWER").unwrap();
    let combat = game.combat_mut().unwrap();
    combat.discard = (0..5)
        .map(|instance| Card {
            id: strike,
            instance,
            ..Card::default()
        })
        .collect();
    combat.hand = vec![Card {
        id: drain,
        ..Card::default()
    }];
    combat.energy = 3;
    let selections = game.rngs.combat_card_selection.1;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.rngs.combat_card_selection.1, selections + 4);
    assert_eq!(
        game.combat()
            .unwrap()
            .discard
            .iter()
            .filter(|card| card.upgrades > 0)
            .count(),
        2
    );
}

#[test]
fn true_grit_uses_one_random_selection() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let mut cards = ["CARD.STRIKE_IRONCLAD", "CARD.DEFEND_IRONCLAD", "CARD.BASH"].map(|id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    });
    for (instance, card) in cards.iter_mut().enumerate() {
        card.instance = instance as u32 + 1;
    }
    let combat = game.combat_mut().unwrap();
    combat.hand = std::iter::once(Card {
        id: content.card_id("CARD.TRUE_GRIT").unwrap(),
        ..Card::default()
    })
    .chain(cards)
    .collect();
    combat.energy = 3;
    let mut rng = game.rngs.combat_card_selection;
    let expected = cards[rng.below(cards.len() as u32) as usize];
    let selections = game.rngs.combat_card_selection.1;

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    assert_eq!(game.rngs.combat_card_selection.1, selections + 1);
    assert!(game.combat().unwrap().exhaust.contains(&expected));
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
            game.start_turn();
            game.resolve(&content);
        }
    }
}

#[test]
fn shrink_ends_when_the_shrinker_beetle_dies() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    let encounter = content
        .encounters
        .iter()
        .position(|encounter| encounter.id == "ENCOUNTER.SHRINKER_BEETLE_WEAK")
        .unwrap() as Id;
    game.start_combat(&content, encounter).unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.add_power(power_id::SHRINK, -1);
    let mut survivor = combat.enemies[0].clone();
    survivor.instance += 1;
    combat.enemies.push(survivor);
    combat.enemies[0].creature.hp = 1;
    combat.enemies[0].creature.block = 0;
    combat.hand = vec![Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().player.power(power_id::SHRINK), 0);
}

#[test]
fn mayhem_plays_the_top_card_after_the_turn_draw() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let mayhem = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.MAYHEM_POWER")
        .unwrap() as Id;
    let combat = game.combat_mut().unwrap();
    let card = |id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    };
    combat.hand.clear();
    combat.draw = vec![card("CARD.DEFEND_IRONCLAD")];
    combat.draw.extend([card("CARD.STRIKE_IRONCLAD"); 5]);
    combat.player.block = 0;
    combat.player.add_power(mayhem, 1);

    game.start_turn();

    game.resolve(&content);

    let combat = game.combat().unwrap();
    assert_eq!(combat.hand.len(), 5);
    assert_eq!(combat.player.block, 5);
}

#[test]
fn retained_hands_still_discard_turn_end_curses() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let regret = content.card_id("CARD.REGRET").unwrap();
    combat.hand = vec![Card {
        id: regret,
        ..Card::default()
    }];
    combat.discard.clear();
    combat.player.add_power(power_id::RETAIN_HAND, 1);

    game.end_turn(&content);

    let combat = game.combat().unwrap();
    assert!(combat.hand.iter().all(|card| card.id != regret));
    assert!(combat.discard.iter().any(|card| card.id == regret));
}

#[test]
fn speedster_ignores_the_turn_draw() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand.clear();
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.block = 0;
    combat.player.add_power(power_id::SPEEDSTER, 2);

    game.start_turn();

    game.resolve(&content);
    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 100);

    let combat = game.combat_mut().unwrap();
    combat.draw.push(combat.hand[0]);
    combat.queue.push(Pending {
        effect: Effect::Draw(1),
        context: Context::player(),
    });
    game.resolve(&content);
    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 98);
}

#[test]
fn cloak_clasp_blocks_once_per_card_in_hand() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.obtain_relic(&content, content.relic_id("RELIC.CLOAK_CLASP").unwrap());
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.block = 0;
    combat.hand.truncate(4);
    for enemy in &mut combat.enemies {
        enemy.stunned = true;
    }
    let barricade = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.BARRICADE_POWER")
        .unwrap() as Id;
    game.combat_mut().unwrap().player.add_power(barricade, 1);

    game.step(&content, Action::EndTurn).unwrap();

    assert_eq!(game.combat().unwrap().player.block, 4);
}

#[test]
fn regen_does_not_heal_after_orbs_win_the_combat() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies.truncate(1);
    combat.enemies[0].creature.hp = 1;
    combat.enemies[0].creature.block = 0;
    combat.player.hp = 20;
    combat.player.add_power(power_id::REGEN, 5);
    combat.orbs = vec![Orb {
        id: orb_id::LIGHTNING,
        value: 0,
    }];

    game.step(&content, Action::EndTurn).unwrap();

    assert!(game.combat().is_none());
    assert_eq!(game.run.hp, 20);
}

#[test]
fn thrash_rolls_even_for_a_single_attack_in_hand() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies[0].creature.hp = 100;
    combat.hand = ["CARD.THRASH", "CARD.STRIKE_IRONCLAD"]
        .map(|id| Card {
            id: content.card_id(id).unwrap(),
            ..Card::default()
        })
        .to_vec();
    let before = format!("{:?}", game.rngs.combat_card_selection);

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_ne!(format!("{:?}", game.rngs.combat_card_selection), before);
    assert!(game.combat().unwrap().hand.is_empty());
}

#[test]
fn hand_selection_is_skipped_once_the_last_enemy_dies() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 4, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies.truncate(1);
    combat.enemies[0].creature.hp = 1;
    combat.enemies[0].creature.block = 0;
    combat.hand = [
        "CARD.SNAP",
        "CARD.STRIKE_NECROBINDER",
        "CARD.DEFEND_NECROBINDER",
    ]
    .map(|id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    })
    .to_vec();

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert!(game.combat().is_none_or(|combat| combat.choice.is_none()));
}

#[test]
fn rupture_ignores_hp_lost_on_the_enemy_turn() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.block = 0;
    combat.player.add_power(power_id::RUPTURE, 1);
    combat.enemy_turn = true;
    combat.queue.push(Pending {
        effect: Effect::LoseHp(Target::Player, Amount::fixed(3, 3)),
        context: Context::player(),
    });
    game.resolve(&content);
    assert_eq!(game.combat().unwrap().player.power(power_id::STRENGTH), 0);

    game.combat_mut().unwrap().enemy_turn = false;
    game.combat_mut().unwrap().queue.push(Pending {
        effect: Effect::LoseHp(Target::Player, Amount::fixed(3, 3)),
        context: Context::player(),
    });
    game.resolve(&content);
    assert_eq!(game.combat().unwrap().player.power(power_id::STRENGTH), 1);
}

#[test]
fn rebound_returns_the_next_card_to_the_top_of_the_draw_pile() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies[0].creature.hp = 100;
    combat.energy = 3;
    let card = |id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    };
    combat.hand = vec![card("CARD.REBOUND"), card("CARD.DEFEND_IRONCLAD")];
    combat.discard.clear();
    let play = |game: &mut Game, target| {
        game.step(&content, Action::Play { hand: 0, target })
            .unwrap()
    };

    play(&mut game, Some(0));
    play(&mut game, None);

    let combat = game.combat().unwrap();
    assert_eq!(combat.discard, vec![card("CARD.REBOUND")]);
    assert_eq!(combat.draw.last(), Some(&card("CARD.DEFEND_IRONCLAD")));
    assert_eq!(combat.player.power(power_id::REBOUND), 0);
}

#[test]
fn stunned_thieving_hopper_does_not_escape() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    let encounter = content
        .encounters
        .iter()
        .position(|encounter| encounter.id == "ENCOUNTER.THIEVING_HOPPER_WEAK")
        .unwrap() as Id;
    game.start_combat(&content, encounter).unwrap();
    let enemy = &mut game.combat_mut().unwrap().enemies[0];
    enemy.move_index = 4;
    enemy.stunned = true;

    game.step(&content, Action::EndTurn).unwrap();

    assert!(game.combat().unwrap().enemies[0].creature.hp > 0);
}

#[test]
fn ring_of_the_snake_draws_its_cards_in_the_first_hand() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    assert_eq!(game.combat().unwrap().hand.len(), 7);
}

#[test]
fn reaper_form_counts_blocked_damage() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.block = 10;
    combat.enemies[0].creature.powers.clear();
    combat.player.add_power(
        content
            .powers
            .iter()
            .position(|power| power.id == "POWER.REAPER_FORM_POWER")
            .unwrap() as Id,
        1,
    );
    combat.hand = vec![Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(
        game.combat().unwrap().enemies[0]
            .creature
            .power(power_id::DOOM),
        6
    );
}

#[test]
fn havoc_on_an_unplayable_card_does_not_play_it() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let card = |id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    };
    combat.hand = vec![card("CARD.HAVOC")];
    combat.draw.push(card("CARD.ASCENDERS_BANE"));
    combat.exhaust.clear();

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    let combat = game.combat().unwrap();
    assert_eq!(combat.exhaust[0].id, card("CARD.ASCENDERS_BANE").id);
    assert_eq!(combat.history.cards, 1);
}

#[test]
fn razor_tooth_upgrades_a_card_between_its_plays() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.obtain_relic(&content, content.relic_id("RELIC.RAZOR_TOOTH").unwrap());
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.block = 0;
    combat.player.add_power(power_id::DUPLICATION, 1);
    combat.hand = vec![Card {
        id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().player.block, 13);
}

#[test]
fn damage_multipliers_round_once_at_the_end() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.block = 0;
    combat.enemies[0].creature.powers.clear();
    combat.player.add_power(power_id::WEAK, 1);
    combat.player.add_power(power_id::SHRINK, -1);
    combat.hand = vec![Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 97);
}

#[test]
fn signal_boost_only_repeats_powers() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.player.block = 0;
    combat.player.add_power(power_id::SIGNAL_BOOST, 1);
    combat.hand = vec![Card {
        id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    let combat = game.combat().unwrap();
    assert_eq!(combat.player.block, 5);
    assert_eq!(combat.player.power(power_id::SIGNAL_BOOST), 1);
}

#[test]
fn echoing_slash_is_powered_and_repeats_per_kill() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies.truncate(1);
    let mut enemy = combat.enemies[0].clone();
    enemy.instance += 1;
    combat.enemies.push(enemy);
    for (enemy, hp) in combat.enemies.iter_mut().zip([5, 30]) {
        enemy.creature.hp = hp;
        enemy.creature.max_hp = 30;
        enemy.creature.block = 0;
        enemy.creature.powers.clear();
    }
    combat.player.add_power(power_id::STRENGTH, 1);
    combat.hand = vec![Card {
        id: content.card_id("CARD.ECHOING_SLASH").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().enemies[1].creature.hp, 8);
}

#[test]
fn skittish_blocks_after_a_whole_multi_hit_attack() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    combat.enemies.truncate(1);
    let enemy = &mut combat.enemies[0].creature;
    enemy.hp = 50;
    enemy.block = 0;
    enemy.powers.clear();
    enemy.add_power(power_id::SKITTISH, 7);
    combat.energy = 3;
    combat.hand = vec![Card {
        id: content.card_id("CARD.RICOCHET").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    let enemy = &game.combat().unwrap().enemies[0].creature;
    assert_eq!((enemy.hp, enemy.block), (38, 7));
}

#[test]
fn iteration_draws_only_for_the_first_status_each_turn() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let card = |id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    };
    let iteration = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.ITERATION_POWER")
        .unwrap() as Id;
    combat.hand.clear();
    combat.discard.clear();
    combat.draw = vec![card("CARD.ZAP"); 4];
    combat
        .draw
        .extend([card("CARD.SLIMED"), card("CARD.SLIMED")]);
    combat.player.add_power(iteration, 1);
    combat.queue.push(Pending {
        effect: Effect::Draw(2),
        context: Context::player(),
    });

    game.resolve(&content);

    assert_eq!(game.combat().unwrap().hand.len(), 3);
}

#[test]
fn compact_turns_statuses_in_hand_into_fuel() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let card = |id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    };
    combat.hand = vec![card("CARD.COMPACT"), card("CARD.DAZED"), card("CARD.ZAP")];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    let hand: Vec<_> = game
        .combat()
        .unwrap()
        .hand
        .iter()
        .map(|card| card.id)
        .collect();
    assert_eq!(hand, vec![card("CARD.FUEL").id, card("CARD.ZAP").id]);
}

#[test]
fn slow_grows_between_repeated_plays() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let enemy = &mut combat.enemies[0].creature;
    enemy.hp = 100;
    enemy.block = 0;
    enemy.powers.clear();
    enemy.add_power(power_id::SLOW, 40);
    combat.player.add_power(power_id::DUPLICATION, 1);
    combat.hand = vec![Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        ..Card::default()
    }];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: Some(0),
        },
    )
    .unwrap();

    assert_eq!(game.combat().unwrap().enemies[0].creature.hp, 100 - 8 - 9);
}

#[test]
fn hand_trick_picks_the_only_skill_without_asking() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 2, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat_mut().unwrap();
    let card = |id| Card {
        id: content.card_id(id).unwrap(),
        ..Card::default()
    };
    combat.hand = vec![
        card("CARD.HAND_TRICK"),
        card("CARD.DEFEND_SILENT"),
        card("CARD.STRIKE_SILENT"),
    ];

    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();

    let combat = game.combat().unwrap();
    assert!(combat.choice.is_none());
    assert_ne!(combat.hand[0].turn_flags & SLY, 0);
}

#[test]
fn bread_raises_max_energy_from_turn_two() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 1, 0).unwrap();
    game.obtain_relic(&content, content.relic_id("RELIC.BREAD").unwrap());
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.combat().unwrap();
    assert_eq!((combat.max_energy, combat.energy), (3, 1));
    game.combat_mut().unwrap().player.hp = 999;
    game.step(&content, Action::EndTurn).unwrap();
    let combat = game.combat().unwrap();
    assert_eq!((combat.max_energy, combat.energy), (4, 4));
}

#[test]
fn metamorphosis_cards_are_free_all_combat() {
    let content = foundation_content();
    let mut game = turn_start_test(&content, 1, &[], &[]);
    game.combat_mut().unwrap().hand = vec![Card {
        id: content.card_id("CARD.METAMORPHOSIS").unwrap(),
        ..Card::default()
    }];
    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    let draw = &game.combat().unwrap().draw;
    assert_eq!(draw.len(), 3);
    assert!(
        draw.iter()
            .all(|card| card.flags & FREE_COMBAT != 0 && !card.free)
    );
}

fn exhaust_with_feel_no_pain_and_forgotten_soul(relic_at_start: bool) -> Game {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    let soul = content.relic_id("RELIC.FORGOTTEN_SOUL").unwrap();
    if relic_at_start {
        game.run.relics.push(soul);
    }
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    if !relic_at_start {
        game.run.relics.push(soul);
    }
    let feel_no_pain = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.FEEL_NO_PAIN_POWER")
        .unwrap() as Id;
    let combat = game.combat_mut().unwrap();
    combat.queue.clear();
    combat.player.add_power(feel_no_pain, 3);
    game.trigger(&content, Trigger::AfterCardExhausted, Actor::Player, 1);
    game
}

#[test]
fn powers_hear_a_pass_before_relics() {
    // CombatState.IterateHookListeners: a creature's powers, then the player's relics, however
    // long each was held. The queue is a stack, so the power listens first.
    for relic_at_start in [false, true] {
        let game = exhaust_with_feel_no_pain_and_forgotten_soul(relic_at_start);
        let effects: Vec<_> = game
            .combat()
            .unwrap()
            .queue
            .iter()
            .map(|x| x.effect)
            .collect();
        assert!(matches!(
            effects[..],
            [
                Effect::Listen(_, Listener::Relic(_)),
                Effect::Listen(_, Listener::Power(..))
            ]
        ));
    }
}

#[test]
fn a_played_card_has_one_pass_in_listener_order() {
    // AfterCardPlayed runs a listener's typed hooks (Rage on an attack) with its any-card ones
    // (Afterimage): the player's powers in power order, the relics, then the enemies.
    let content = foundation_content();
    let shiv = content.card_id("CARD.SHIV").unwrap();
    let slow = content
        .powers
        .iter()
        .position(|x| x.id == "POWER.SLOW_POWER")
        .unwrap() as Id;
    let mut powers = ["POWER.RAGE_POWER", "POWER.AFTERIMAGE_POWER"];
    for _ in 0..2 {
        let mut game = turn_start_test(&content, 0, &powers, &["RELIC.HELICAL_DART"]);
        let combat = game.combat_mut().unwrap();
        combat.queue.clear();
        let player = combat.player.powers.clone();
        let enemy = &mut combat.enemies[0].creature;
        enemy.powers.clear();
        enemy.add_power(slow, 1);
        let card = Card {
            id: shiv,
            ..Card::default()
        };
        let recorded = player.iter().map(|&x| (Actor::Player, x)).collect();
        game.card_played(&content, card, recorded);
        let order: Vec<_> = (game.combat().unwrap().queue.iter().rev())
            .filter_map(|x| match x.effect {
                Effect::Listen(Trigger::AttackPlayed, listener) => Some(listener),
                _ => None,
            })
            .collect();
        assert!(matches!(
            order[..],
            [
                Listener::Power(Actor::Player, first),
                Listener::Power(Actor::Player, second),
                Listener::Relic(0),
                Listener::Power(Actor::Enemy(0), _),
            ] if [first.id, second.id] == [player[0].id, player[1].id]
        ));
        powers.reverse();
    }
}

#[test]
fn guarded_passes_do_not_start_once_the_combat_is_over() {
    // Hook.IterateCombatHookListeners yields nothing once the combat is over or ending.
    let content = foundation_content();
    let mut game = exhaust_with_feel_no_pain_and_forgotten_soul(false);
    let combat = game.combat_mut().unwrap();
    combat.queue.clear();
    for enemy in &mut combat.enemies {
        enemy.creature.hp = 0;
    }
    game.trigger(&content, Trigger::AfterCardExhausted, Actor::Player, 1);
    assert!(game.combat().unwrap().queue.is_empty());
}

fn combat_with_relics(relics: &[&str]) -> Game {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    for relic in relics {
        game.run.relics.push(content.relic_id(relic).unwrap());
    }
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    game
}

#[test]
fn room_entered_strength_follows_relic_order() {
    // Girya and Vajra both apply Strength in AfterRoomEntered, in relic order, so the first one
    // held gets Ruined Helmet's doubling (Girya.cs:50, Vajra's data hook).
    for (relics, strength) in [
        (["RELIC.VAJRA", "RELIC.GIRYA"], 2 * 1 + 2),
        (["RELIC.GIRYA", "RELIC.VAJRA"], 2 * 2 + 1),
    ] {
        let content = foundation_content();
        let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.girya = 2;
        for relic in ["RELIC.RUINED_HELMET"].iter().chain(&relics) {
            game.run.relics.push(content.relic_id(relic).unwrap());
        }
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        assert_eq!(
            game.creature(Actor::Player).power(power_id::STRENGTH),
            strength
        );
    }
}

#[test]
fn combat_start_block_and_stars_count_for_turn_one() {
    // The player's block isn't cleared on turn 1 (Creature.AfterTurnStart), and the combat start
    // is round 1 (CombatState.RoundNumber), so Anchor's block and Divine Right's stars count.
    let content = foundation_content();
    let mut game = combat_with_relics(&["RELIC.ANCHOR", "RELIC.DIVINE_RIGHT"]);
    let combat = game.combat().unwrap();
    assert_eq!(combat.player.block, 10);
    assert_eq!((combat.stars, combat.history.stars_gained), (3, 3));
    game.step(&content, Action::EndTurn).unwrap();
    assert_eq!(game.combat().unwrap().history.stars_gained, 0);
}

#[test]
fn bone_tea_upgrades_the_hand_gambling_chip_redrew() {
    // Gambling Chip redraws in AfterPlayerTurnStart; Bone Tea upgrades in the later
    // AfterSideTurnStart, whichever relic is held first.
    let content = foundation_content();
    for relics in [
        ["RELIC.GAMBLING_CHIP", "RELIC.BONE_TEA"],
        ["RELIC.BONE_TEA", "RELIC.GAMBLING_CHIP"],
    ] {
        let mut game = combat_with_relics(&relics);
        assert!(
            game.combat()
                .unwrap()
                .hand
                .iter()
                .all(|card| card.upgrades == 0)
        );
        game.step(&content, Action::Choose(0)).unwrap();
        game.step(&content, Action::Done).unwrap();
        let combat = game.combat().unwrap();
        assert!(combat.choice.is_none());
        assert_eq!(combat.hand.len(), 5);
        assert!(combat.hand.iter().all(|card| card.upgrades == 1));
    }
}

#[test]
fn turn_listeners_match_the_game() {
    // Every sim listener of a combat-start, turn, card, damage or death pass overrides that hook
    // in the game.
    let Ok(game) = std::fs::read_to_string("target/hunt/hook-listeners.txt") else {
        return;
    };
    let content = foundation_content();
    let title = |word: &str| word[..1].to_owned() + &word[1..].to_lowercase();
    let class = |id: &str| {
        let (kind, name) = id.split_once('.').unwrap();
        let name: String = name.replace("_S_", "S_").split('_').map(title).collect();
        format!("{}s.{name}", title(kind))
    };
    let passes = (combat::hooks::PLAYER_TURN.iter())
        .chain(combat::hooks::END_PLAYER_TURN)
        .chain(combat::hooks::END_ENEMY_TURN)
        .filter_map(|effect| match effect {
            Effect::Fire(trigger) => Some(*trigger),
            _ => None,
        });
    let others = [
        Trigger::AfterRoomEntered,
        Trigger::BeforeCombatStart,
        Trigger::BeforeCombatStartLate,
        Trigger::AfterCardDrawn,
        Trigger::AfterCardExhausted,
        Trigger::AfterCardGeneratedForCombat,
        Trigger::AfterShuffle,
        Trigger::AfterPotionUsed,
        Trigger::BeforeCardPlayed,
        Trigger::AfterCardPlayed,
        Trigger::AttackPlayed,
        Trigger::SkillPlayed,
        Trigger::PowerPlayed,
        Trigger::AfterBlockGained,
        Trigger::AfterDamageGiven,
        Trigger::AfterDamageReceived,
        Trigger::AfterDeath,
        Trigger::AfterCombatVictory,
    ];
    for trigger in others.into_iter().chain(passes) {
        let hook = match trigger {
            Trigger::AttackPlayed | Trigger::SkillPlayed | Trigger::PowerPlayed => {
                "\tAfterCardPlayed:".to_owned()
            }
            Trigger::AutoPrePlay => "\tAfterAutoPrePlayPhaseEntered:".to_owned(),
            Trigger::AutoPrePlayLate => "\tAfterAutoPrePlayPhaseEnteredLate:".to_owned(),
            _ => format!("\t{trigger:?}:"),
        };
        let line = game.lines().find(|line| line.contains(&hook)).unwrap();
        let listeners: Vec<_> = line.split(' ').skip(1).collect();
        let defs = (content.relics.iter().map(|x| (x.id, x.hooks)))
            .chain(content.powers.iter().map(|x| (x.id, x.hooks)))
            .chain(content.cards.iter().map(|x| (x.id, x.hooks)));
        for (id, hooks) in defs {
            // A power the game keeps inside another model has no class; a subclass inherits.
            let class = class(id);
            let (kind, name) = class.split_once('.').unwrap();
            let path = format!("target/hunt/sts2_src/MegaCrit.Sts2.Core.Models.{kind}/{name}.cs");
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            let base = source.split(&format!("class {name} : ")).nth(1);
            let base =
                base.map(|rest| format!("{kind}.{}", rest.split_whitespace().next().unwrap()));
            if combat::hooks::listens(trigger, id, hooks) {
                assert!(
                    listeners.contains(&class.as_str())
                        || base.is_some_and(|base| listeners.contains(&base.as_str())),
                    "{id} in {trigger:?}"
                );
            }
        }
    }
}

#[test]
fn well_laid_plans_pick_holds_the_turn_end() {
    let content = foundation_content();
    let mut game = turn_start_test(&content, 1, &["POWER.WELL_LAID_PLANS_POWER"], &[]);
    let defend = content.card_id("CARD.DEFEND_SILENT").unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = (1..=2)
        .map(|instance| Card {
            id: defend,
            instance,
            ..Card::default()
        })
        .collect();
    combat.enemies[0].creature.hp = 200;

    game.step(&content, Action::EndTurn).unwrap();
    let combat = game.combat().unwrap();
    assert!(combat.choice.is_some());
    assert_eq!((combat.turn, combat.hand.len()), (1, 2));

    game.step(&content, Action::Choose(1)).unwrap();
    let combat = game.combat().unwrap();
    assert_eq!(combat.turn, 2);
    assert!(combat.choice.is_none());
    assert!(combat.hand.iter().any(|card| card.instance == 2));
}

#[test]
fn paels_eye_takes_an_extra_turn_before_the_enemies_act() {
    let content = foundation_content();
    let mut game = turn_start_test(&content, 1, &[], &["RELIC.PAELS_EYE"]);
    let defend = content.card_id("CARD.DEFEND_SILENT").unwrap();
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: defend,
        instance: 1,
        ..Card::default()
    }];
    let hp = combat.player.hp;
    let moves = combat.enemies[0].move_history.len();

    game.step(&content, Action::EndTurn).unwrap();
    let combat = game.combat().unwrap();
    assert_eq!(combat.turn, 2);
    assert!(combat.paels_eye && !combat.extra_turn && !combat.enemy_turn);
    assert_eq!(combat.player.hp, hp);
    assert_eq!(combat.enemies[0].move_history.len(), moves);
    assert!(combat.exhaust.iter().any(|card| card.instance == 1));
}

fn turn_start_test(content: &Content, character: Id, powers: &[&str], relics: &[&str]) -> Game {
    let mut game = Game::new_character_ascension(content, 1, character, 0).unwrap();
    game.begin_act(content, 0).unwrap();
    game.start_combat(content, content.acts[0].encounters[0])
        .unwrap();
    game.run.relics = (relics.iter())
        .map(|relic| content.relic_id(relic).unwrap())
        .collect();
    let combat = game.combat_mut().unwrap();
    combat.hand.clear();
    combat.draw.clear();
    combat.discard.clear();
    combat.orbs.clear();
    combat.orb_slots = 3;
    combat.player.powers.clear();
    for &id in powers {
        let power = content.powers.iter().position(|power| power.id == id);
        combat.player.add_power(power.unwrap() as Id, 1);
    }
    game
}

#[test]
fn before_hand_draw_powers_add_cards_in_power_order() {
    // Nightmare, Hello World and Call of the Void are BeforeHandDraw listeners.
    let content = foundation_content();
    let strike = content.card_id("CARD.STRIKE_DEFECT").unwrap();
    let mut order = [
        ("POWER.NIGHTMARE_POWER", 'N'),
        ("POWER.HELLO_WORLD_POWER", 'H'),
        ("POWER.CALL_OF_THE_VOID_POWER", 'V'),
    ];
    for _ in 0..2 {
        let powers = order.map(|(power, _)| power);
        let mut game = turn_start_test(&content, 1, &powers, &[]);
        let card = Card {
            id: strike,
            ..Card::default()
        };
        game.combat_mut().unwrap().nightmares = vec![(card, 1)];
        game.start_turn();
        game.resolve(&content);
        let kinds: String = (game.combat().unwrap().hand.iter())
            .map(|card| match card {
                card if card.id == strike => 'N',
                card if card.flags & ETHEREAL != 0 => 'V',
                _ => 'H',
            })
            .collect();
        assert_eq!(kinds, String::from_iter(order.map(|(_, kind)| kind)));
        order.reverse();
    }
}

#[test]
fn spinner_and_lightning_rod_channel_in_power_order() {
    let content = foundation_content();
    let mut order = [("POWER.SPINNER_POWER", 4), ("POWER.LIGHTNING_ROD_POWER", 0)];
    for _ in 0..2 {
        let mut game = turn_start_test(&content, 1, &order.map(|(power, _)| power), &[]);
        game.start_turn();
        game.resolve(&content);
        let orbs: Vec<_> = game
            .combat()
            .unwrap()
            .orbs
            .iter()
            .map(|orb| orb.id)
            .collect();
        assert_eq!(orbs, order.map(|(_, orb)| orb));
        order.reverse();
    }
}

#[test]
fn crossbow_and_big_hat_generate_in_relic_order() {
    // Both are AfterSideTurnStart relics sharing the card-generation RNG; Crossbow's attack is
    // free this turn.
    let content = foundation_content();
    let mut order = [("RELIC.CROSSBOW", true), ("RELIC.BIG_HAT", false)];
    for _ in 0..2 {
        let mut game = turn_start_test(&content, 1, &[], &order.map(|(relic, _)| relic));
        game.combat_mut().unwrap().turn = 0;
        game.start_turn();
        game.resolve(&content);
        let hand = &game.combat().unwrap().hand;
        assert_eq!(hand[0].free, order[0].1);
        assert_eq!(hand.last().unwrap().free, order[1].1);
        order.reverse();
    }
}

#[test]
fn seal_of_gold_and_happy_flower_give_energy_in_relic_order() {
    // Both are AfterSideTurnStart relics; Seal of Gold isn't forced ahead of the others.
    let content = foundation_content();
    let mut order = ["RELIC.SEAL_OF_GOLD", "RELIC.HAPPY_FLOWER"];
    for _ in 0..2 {
        let mut game = turn_start_test(&content, 1, &[], &order);
        game.run.gold = 7;
        game.happy_flower = 2;
        game.start_turn();
        game.resolve(&content);
        let combat = game.combat().unwrap();
        assert_eq!(combat.energy, combat.max_energy + 2);
        assert_eq!(game.run.gold, 2);
        order.reverse();
    }
}

#[test]
fn paels_legion_and_letter_opener_each_count_their_turn() {
    // One AfterSideTurnStart arm each: Pael's Legion counts down once, Letter Opener resets.
    let content = foundation_content();
    let mut order = ["RELIC.PAELS_LEGION", "RELIC.LETTER_OPENER"];
    for _ in 0..2 {
        let mut game = turn_start_test(&content, 1, &[], &order);
        let combat = game.combat_mut().unwrap();
        combat.paels_legion = 2;
        combat.letter_opener = 2;
        game.start_turn();
        game.resolve(&content);
        assert_eq!(game.combat().unwrap().paels_legion, 1);
        assert_eq!(game.combat().unwrap().letter_opener, 0);
        order.reverse();
    }
}

#[test]
fn side_turn_powers_tick_on_their_owners_side() {
    // Poison and Slow hear AfterSideTurnStart only when their owner's side starts its turn.
    let content = foundation_content();
    let mut game = turn_start_test(&content, 1, &[], &[]);
    let enemy = &mut game.combat_mut().unwrap().enemies[0].creature;
    enemy.add_power(power_id::SLOW, 2);
    enemy.add_power(power_id::POISON, 3);
    let hp = enemy.hp;
    game.start_turn();
    game.resolve(&content);
    let enemy = &game.combat().unwrap().enemies[0].creature;
    assert_eq!((enemy.hp, enemy.power(power_id::POISON)), (hp, 3));
    assert_eq!(enemy.power(power_id::SLOW), 2);
    game.combat_mut().unwrap().enemy_turn = true;
    let queue = &mut game.combat_mut().unwrap().queue;
    push_effects(queue, super::combat::hooks::ENEMY_TURN, Context::player());
    game.resolve(&content);
    let enemy = &game.combat().unwrap().enemies[0].creature;
    assert_eq!((enemy.hp, enemy.power(power_id::POISON)), (hp - 3, 2));
    assert_eq!(enemy.power(power_id::SLOW), 0);
}

#[test]
fn slumber_and_demise_tick_in_power_order() {
    // Slumber ticking first lets Demise's HP loss end it with a stun; ticking last, the beetle
    // wakes up on its own (WakeUpMove drops Plating) and rolls Roll Out as usual.
    let content = foundation_content();
    let beetle = (content.enemies.iter()).position(|enemy| enemy.id == "MONSTER.SLUMBERING_BEETLE");
    let demise = (content.powers.iter()).position(|power| power.id == "POWER.DEMISE_POWER");
    for slumber_leads in [true, false] {
        let mut game = turn_start_test(&content, 1, &[], &[]);
        let combat = game.combat_mut().unwrap();
        combat.enemy_turn = true;
        let enemy = &mut combat.enemies[0];
        enemy.creature.id = beetle.unwrap() as Id;
        enemy.creature.hp = 50;
        enemy.creature.powers.clear();
        enemy.move_index = 0;
        enemy.last_move = 0;
        let powers = [(power_id::SLUMBER, 2), (demise.unwrap() as Id, 5)];
        let powers = if slumber_leads {
            powers
        } else {
            [powers[1], powers[0]]
        };
        for (id, amount) in powers.into_iter().chain([(power_id::PLATING, 8)]) {
            enemy.creature.add_power(id, amount);
        }
        push_effects(
            &mut combat.queue,
            super::combat::hooks::END_ENEMY_TURN,
            Context::player(),
        );
        game.resolve(&content);
        let enemy = &game.combat().unwrap().enemies[0];
        assert_eq!(enemy.creature.power(power_id::SLUMBER), 0);
        assert_eq!(enemy.creature.hp, 45);
        // Stunned, or Roll Out once the player's turn rolls the moves.
        assert_eq!(enemy.move_index, if slumber_leads { 2 } else { 1 });
        assert_eq!(enemy.creature.power(power_id::PLATING) > 0, slumber_leads);
    }
}

#[test]
fn a_power_pick_blocks_later_turn_start_listeners() {
    // AutoSlay's selector answers picks without pausing, so the pass waits for Foregone
    // Conclusion's pick before Toasty Mittens (a later BeforeHandDraw listener) exhausts.
    let content = foundation_content();
    let powers = ["POWER.FOREGONE_CONCLUSION_POWER"];
    let mut game = turn_start_test(&content, 1, &powers, &["RELIC.TOASTY_MITTENS"]);
    let strike = Card {
        id: content.card_id("CARD.STRIKE_DEFECT").unwrap(),
        ..Card::default()
    };
    game.combat_mut().unwrap().draw = vec![strike; 8];
    game.start_turn();
    game.resolve(&content);
    assert!(game.combat().unwrap().choice.is_some());
    assert!(game.combat().unwrap().exhaust.is_empty());
    assert!(game.combat().unwrap().hand.is_empty());
    game.step(&content, Action::Choose(0)).unwrap();
    let combat = game.combat().unwrap();
    assert_eq!(combat.exhaust.len(), 1);
    assert_eq!(combat.player.power(power_id::STRENGTH), 1);
    assert_eq!(combat.hand.len(), 1 + combat.draw_per_turn as usize);
}

#[test]
fn dark_embrace_draws_for_ethereal_exhausts_in_power_order() {
    // Ethereal exhausts at turn end defer Dark Embrace's draws to its AfterSideTurnEnd, where a
    // No Draw listed before it is already gone and one listed after it still blocks them.
    let content = foundation_content();
    let defend = content.card_id("CARD.DEFEND_SILENT").unwrap();
    let mut order = [("POWER.DARK_EMBRACE_POWER", 5), ("POWER.NO_DRAW_POWER", 6)];
    for _ in 0..2 {
        let mut game = turn_start_test(&content, 1, &order.map(|(power, _)| power), &[]);
        let combat = game.combat_mut().unwrap();
        combat.hand = vec![Card {
            id: defend,
            instance: 1,
            flags: ETHEREAL,
            ..Card::default()
        }];
        combat.draw = (2..12)
            .map(|instance| Card {
                id: defend,
                instance,
                ..Card::default()
            })
            .collect();
        combat.enemies[0].creature.hp = 200;
        game.step(&content, Action::EndTurn).unwrap();
        let combat = game.combat().unwrap();
        assert!(combat.exhaust.iter().any(|card| card.instance == 1));
        assert_eq!((combat.turn, combat.hand.len()), (2, order[0].1));
        order.reverse();
    }
}

#[test]
fn joss_paper_counts_ethereal_exhausts_after_the_flush() {
    let content = foundation_content();
    let defend = content.card_id("CARD.DEFEND_SILENT").unwrap();
    let mut game = turn_start_test(&content, 1, &[], &["RELIC.JOSS_PAPER"]);
    game.joss_paper = 4;
    let combat = game.combat_mut().unwrap();
    combat.hand = vec![Card {
        id: defend,
        instance: 1,
        flags: ETHEREAL,
        ..Card::default()
    }];
    combat.draw = (2..12)
        .map(|instance| Card {
            id: defend,
            instance,
            ..Card::default()
        })
        .collect();
    combat.enemies[0].creature.hp = 200;
    game.step(&content, Action::EndTurn).unwrap();
    assert_eq!((game.joss_paper, game.joss_paper_ethereal), (0, 0));
    assert_eq!(game.combat().unwrap().hand.len(), 6);
}

#[test]
fn exhaust_listeners_resolve_in_listener_order() {
    // AfterCardExhausted: Dark Embrace (a power) draws first, then Joss Paper's draw and Burning
    // Sticks' copy in relic order.
    let content = foundation_content();
    let id = |card| content.card_id(card).unwrap();
    let card = |card| Card {
        id: id(card),
        ..Card::default()
    };
    let mut relics = ["RELIC.JOSS_PAPER", "RELIC.BURNING_STICKS"];
    for _ in 0..2 {
        let mut game = turn_start_test(&content, 0, &["POWER.DARK_EMBRACE_POWER"], &relics);
        game.joss_paper = 4;
        let combat = game.combat_mut().unwrap();
        combat.draw = vec![card("CARD.BASH"), card("CARD.ANGER")];
        combat.hand = vec![card("CARD.DEFEND_IRONCLAD")];
        combat.queue = vec![Pending {
            effect: Effect::RandomCardOp(
                Pile::Hand,
                CardFilter::Any,
                CardOp::Move(Pile::Exhaust),
                Amount::fixed(1, 1),
            ),
            context: Context::player(),
        }];
        game.resolve(&content);
        let hand: Vec<_> = (game.combat().unwrap().hand.iter())
            .map(|card| content.cards[card.id as usize].id)
            .collect();
        let expected = if relics[0] == "RELIC.JOSS_PAPER" {
            ["CARD.ANGER", "CARD.BASH", "CARD.DEFEND_IRONCLAD"]
        } else {
            ["CARD.ANGER", "CARD.DEFEND_IRONCLAD", "CARD.BASH"]
        };
        assert_eq!(hand, expected, "{relics:?}");
        relics.reverse();
    }
}

#[test]
fn potion_relics_resolve_in_relic_order() {
    // AfterPotionUsed: Belt Buckle and Reptile Trinket in relic order, once the potion is done.
    let content = foundation_content();
    let mut relics = ["RELIC.BELT_BUCKLE", "RELIC.REPTILE_TRINKET"];
    for _ in 0..2 {
        let mut game = turn_start_test(&content, 0, &[], &relics);
        game.run.potions = vec![content.potion_id("POTION.BLOCK_POTION"), None, None];
        game.combat_mut().unwrap().queue.clear();
        game.use_potion(&content, 0, None);
        game.resolve(&content);
        let combat = game.combat().unwrap();
        assert!(combat.player.block > 0);
        let powers: Vec<_> = (combat.player.powers.iter())
            .map(|power| power.id)
            .filter(|&id| id == power_id::DEXTERITY || id == power_id::STRENGTH)
            .collect();
        let expected = if relics[0] == "RELIC.BELT_BUCKLE" {
            [power_id::DEXTERITY, power_id::STRENGTH]
        } else {
            [power_id::STRENGTH, power_id::DEXTERITY]
        };
        assert_eq!(powers, expected, "{relics:?}");
        relics.reverse();
    }
}

#[test]
fn belt_buckle_checks_the_potions_once_the_potion_is_done() {
    // BeltBuckle.AfterPotionUsed (BeltBuckle.cs:84): Entropic Brew refilled the slots first.
    let content = foundation_content();
    for (potion, dexterity) in [("POTION.BLOCK_POTION", 2), ("POTION.ENTROPIC_BREW", 0)] {
        let mut game = turn_start_test(&content, 0, &[], &["RELIC.BELT_BUCKLE"]);
        game.run.potions = vec![content.potion_id(potion), None, None];
        game.combat_mut().unwrap().queue.clear();
        game.use_potion(&content, 0, None);
        game.resolve(&content);
        let player = &game.combat().unwrap().player;
        assert_eq!(player.power(power_id::DEXTERITY), dexterity, "{potion}");
    }
}

#[test]
fn gremlin_horn_draws_while_a_dying_enemy_spawns() {
    // Infested stops the combat from ending (ShouldStopCombatFromEnding), so Gremlin Horn, a relic
    // and so an earlier AfterDeath listener than the dying enemy's Infested, still draws.
    let content = foundation_content();
    let mut game = turn_start_test(&content, 0, &[], &["RELIC.GREMLIN_HORN"]);
    let infested = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.INFESTED_POWER");
    let combat = game.combat_mut().unwrap();
    combat.enemies.truncate(1);
    combat.enemies[0].creature.powers = vec![];
    combat.enemies[0]
        .creature
        .add_power(infested.unwrap() as Id, 4);
    combat.draw = vec![Card {
        id: content.card_id("CARD.BASH").unwrap(),
        ..Card::default()
    }];
    game.kill_actor(&content, Actor::Enemy(0));
    let combat = game.combat().unwrap();
    assert_eq!((combat.hand.len(), combat.enemies.len()), (1, 5));
}

#[test]
fn a_killing_hit_skips_after_damage_received() {
    // CreatureCmd.Damage runs AfterDamageReceived only for a target the hit didn't kill, so Demon
    // Tongue heals a survivor but not a player at 0 HP.
    let content = foundation_content();
    for (hp, healed) in [(20, 20), (5, 0)] {
        let mut game = turn_start_test(&content, 0, &[], &["RELIC.DEMON_TONGUE"]);
        let combat = game.combat_mut().unwrap();
        combat.player.hp = hp;
        combat.queue = vec![Pending {
            effect: Effect::LoseHp(Target::Player, Amount::fixed(10, 10)),
            context: Context::player(),
        }];
        game.resolve(&content);
        assert_eq!(game.combat().unwrap().player.hp.max(0), healed);
    }
}

#[test]
fn an_area_hit_lands_on_every_target_before_its_hooks() {
    // One damage command hits every target, then runs each hit's hooks (AfterDamageGiven):
    // Envenom poisons every enemy the area attack hit.
    let content = foundation_content();
    let envenom = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.ENVENOM_POWER");
    let mut game = turn_start_test(&content, 1, &[], &[]);
    let template = game.combat().unwrap().enemies[0].clone();
    let combat = game.combat_mut().unwrap();
    combat.enemies = vec![template.clone(), template];
    combat.hits = vec![0, 0];
    for enemy in &mut combat.enemies {
        enemy.creature.powers.clear();
        enemy.creature.hp = 30;
        enemy.creature.block = 0;
    }
    combat.player.add_power(envenom.unwrap() as Id, 2);
    combat.queue = vec![Pending {
        effect: Effect::Attack(Target::AllEnemies, Amount::fixed(5, 5), 1),
        context: Context::player(),
    }];
    game.resolve(&content);
    for enemy in &game.combat().unwrap().enemies {
        assert_eq!(
            (enemy.creature.hp, enemy.creature.power(power_id::POISON)),
            (25, 2)
        );
    }
}

#[test]
fn auto_plays_and_manual_plays_share_the_per_play_steps() {
    // One OnPlayWrapper: Maul grows after each play of a Duplication series either way.
    let content = foundation_content();
    let maul = content.card_id("CARD.MAUL").unwrap();
    let duplication = content
        .powers
        .iter()
        .position(|power| power.id == "POWER.DUPLICATION_POWER");
    let outcomes = [true, false].map(|manual| {
        let mut game = turn_start_test(&content, 0, &[], &[]);
        let combat = game.combat_mut().unwrap();
        combat.queue.clear();
        combat.energy = 9;
        combat.player.add_power(duplication.unwrap() as Id, 1);
        combat.enemies.truncate(1);
        combat.enemies[0].creature.hp = 999;
        combat.enemies[0].creature.powers.clear();
        let card = Card {
            id: maul,
            instance: 99,
            ..Card::default()
        };
        if manual {
            combat.hand = vec![card];
            game.step(
                &content,
                Action::Play {
                    hand: 0,
                    target: Some(0),
                },
            )
            .unwrap();
        } else {
            combat.queue.push(Pending {
                effect: Effect::AutoPlay(card),
                context: Context {
                    target: Some(0),
                    ..Context::card(card)
                },
            });
            game.resolve(&content);
        }
        let combat = game.combat().unwrap();
        (combat.enemies[0].creature.hp, combat.discard[0].value)
    });
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[0].1, 2);
}

#[test]
fn danse_macabre_reads_the_cost_after_earlier_listeners() {
    // A Veilpiercer listed before Danse Macabre spends itself first, so the Ethereal Bash costs 2
    // again when Danse Macabre reads it; listed after, it still costs 0.
    let content = foundation_content();
    let mut powers = ["POWER.VEILPIERCER_POWER", "POWER.DANSE_MACABRE_POWER"];
    for block in [1, 0] {
        let mut game = turn_start_test(&content, 0, &powers, &[]);
        let combat = game.combat_mut().unwrap();
        combat.queue.clear();
        combat.energy = 3;
        combat.player.block = 0;
        combat.hand = vec![Card {
            id: content.card_id("CARD.BASH").unwrap(),
            flags: ETHEREAL,
            ..Card::default()
        }];
        let target = Some(0);
        game.step(&content, Action::Play { hand: 0, target })
            .unwrap();
        let combat = game.combat().unwrap();
        assert_eq!((combat.player.block, combat.energy), (block, 3));
        powers.reverse();
    }
}

#[test]
fn razor_tooth_music_box_and_juggling_read_the_card_in_listener_order() {
    // Juggling (a power) clones the third attack before Razor Tooth upgrades it; Music Box's copy
    // is upgraded only when Razor Tooth comes first in relic order.
    let content = foundation_content();
    let mut relics = ["RELIC.RAZOR_TOOTH", "RELIC.MUSIC_BOX"];
    for upgrades in [1, 0] {
        let mut game = turn_start_test(&content, 0, &["POWER.JUGGLING_POWER"], &relics);
        let combat = game.combat_mut().unwrap();
        combat.queue.clear();
        combat.energy = 3;
        combat.history.attacks = 2;
        combat.hand = vec![Card {
            id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
            ..Card::default()
        }];
        let target = Some(0);
        game.step(&content, Action::Play { hand: 0, target })
            .unwrap();
        let combat = game.combat().unwrap();
        let hand: Vec<_> = combat
            .hand
            .iter()
            .map(|x| (x.upgrades, x.flags & ETHEREAL))
            .collect();
        assert_eq!(hand, [(0, 0), (upgrades, ETHEREAL)]);
        assert_eq!(combat.discard[0].upgrades, 1);
        relics.reverse();
    }
}

#[test]
fn storm_hears_only_the_plays_it_recorded() {
    // Storm records each power play in BeforeCardPlayed: its own card's play doesn't channel,
    // the next power card's does.
    let content = foundation_content();
    let mut game = turn_start_test(&content, 1, &[], &[]);
    let combat = game.combat_mut().unwrap();
    combat.queue.clear();
    combat.energy = 9;
    combat.hand = ["CARD.STORM", "CARD.DEFRAGMENT"]
        .map(|id| Card {
            id: content.card_id(id).unwrap(),
            ..Card::default()
        })
        .to_vec();
    for orbs in [0, 1] {
        let target = None;
        game.step(&content, Action::Play { hand: 0, target })
            .unwrap();
        assert_eq!(game.combat().unwrap().orbs.len(), orbs);
    }
}
