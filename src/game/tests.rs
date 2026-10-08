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
        .find(|power| power.id == power_id::INFERNO)
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

    game.trigger(&content, Trigger::TurnStart, Actor::Player, 0);
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
fn omnislice_damages_all_enemies() {
    let content = foundation_content();
    let card = &content.cards[content.card_id("CARD.OMNISLICE").unwrap() as usize];
    assert!(matches!(
        card.effects,
        [Effect::MoveDamage(Target::AllEnemies, _)]
    ));
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
    combat.boulders = vec![5];
    combat.player.add_power(power_id::ROLLING_BOULDER, 5);

    game.tick_boulders();
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
        ..Card::default()
    };

    let (card, exhausted) = game.finish_card_destination(&content, card, false, false);

    assert!(!exhausted);
    assert!(!card.free);
    assert_eq!(card.cost_override, None);
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
        combat.playing = None;
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
            .any(|pending| matches!(pending.effect, Effect::ContinueEndTurn))
    );
    assert_eq!(
        game.actions(&content),
        [Action::Choose(0), Action::Choose(1)]
    );

    game.step(&content, Action::Choose(0)).unwrap();

    let combat = game.combat().unwrap();
    assert!(combat.choice.is_none());
    assert!(combat.queue.is_empty());
    assert!(combat.auto_plays.is_empty());
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

    game.combat_mut().unwrap().history.skills = 1;
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
    combat.playing = None;

    game.finish_combat(&content);

    assert_eq!(game.fishing_rod, 0);
    assert_eq!(game.run.deck[0].upgrades, 1);
    assert!(matches!(game.phase, Phase::Rewards(_)));
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
    combat.enemy_power_snapshot = vec![vec![Power {
        id: power_id::STRENGTH,
        amount: 2,
        skip_next_decay: false,
        value: 0,
    }]];

    let slot = game.spawn_enemy(&content, "MONSTER.TOUGH_EGG");

    let combat = game.combat().unwrap();
    assert_eq!(slot, 0);
    assert_eq!(combat.enemies.len(), 1);
    assert_eq!(combat.hits, [0]);
    assert!(combat.enemy_power_snapshot[0].is_empty());
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
    combat.enemy_power_snapshot = vec![
        vec![Power {
            id: power_id::STRENGTH,
            amount: 1,
            skip_next_decay: false,
            value: 0,
        }];
        3
    ];
    combat.enemies[0].creature.hp = 0;
    combat.enemies[1].creature.id = merc;
    combat.enemies[1].creature.hp = 1;
    combat.enemies[1].creature.powers = vec![
        Power {
            id: power_id::SURPRISE,
            amount: 1,
            skip_next_decay: false,
            value: 0,
        },
        Power {
            id: power_id::THIEVERY,
            amount: 1,
            skip_next_decay: false,
            value: 17,
        },
    ];

    game.kill_actor(&content, Actor::Enemy(1));

    let combat = game.combat().unwrap();
    assert_eq!(combat.enemies.len(), 3);
    assert_eq!(combat.hits[..2], [0, 0]);
    assert!(combat.enemy_power_snapshot[..2].iter().all(Vec::is_empty));
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
fn stock_and_wrigglers_reuse_dead_slots() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 1, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let template = game.combat().unwrap().enemies[0].clone();
    let combat = game.combat_mut().unwrap();
    combat.enemies = vec![template.clone(), template];
    combat.hits = vec![9, 8];
    combat.enemy_power_snapshot = vec![vec![], vec![]];
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
    assert_eq!(combat.enemies.len(), 2);
    assert_eq!(combat.hits[0], 0);
    assert_eq!(
        content.enemies[combat.enemies[0].creature.id as usize].id,
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

    game.start_turn(&content);

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
    assert!(combat.hand.contains(&regret));
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
    combat.player.add_power(power_id::MONARCHS_GAZE, 1);
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

    game.start_turn(&content);

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

    game.start_turn(&content);

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
        combat.enemy_power_snapshot.truncate(1);
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
    game.start_turn(&content);
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
    let combat = game.combat_mut().unwrap();
    combat.hand.clear();
    combat.draw = vec![strike];
    combat.discard.clear();
    combat.draw_per_turn = 1;
    combat.player.powers.clear();
    combat.player.add_power(power_id::FURNACE, 5);

    game.start_turn(&content);

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
            game.start_turn(&content);
        }
    }
}
