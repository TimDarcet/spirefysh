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
fn corpse_slug_glomp_attacks_and_applies_weak() {
    let content = foundation_content();
    let slug = content
        .enemies
        .iter()
        .find(|enemy| enemy.id == "MONSTER.CORPSE_SLUG")
        .unwrap();
    assert!(matches!(
        slug.moves[1].effects,
        [
            Effect::Attack(Target::Player, ..),
            Effect::ApplyPower(
                Target::Player,
                power_id::WEAK,
                Amount {
                    base: 1,
                    upgraded: 1,
                    ..
                }
            ),
        ]
    ));
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
        assert_eq!(first.rngs.up_front.1, draws + 1);
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
