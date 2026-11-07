#[cfg(feature = "python")]
use super::value_model::python;
use super::*;

#[test]
fn canonical_progress_matches_route_geometry() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 0, 0, 1).unwrap();
    for (act, floor, compass, expected) in [
        (1, 0, None, 0),
        (1, 15, None, 15),
        (2, 0, None, 25),
        (2, 6, None, 31),
        (2, 7, None, 33),
        (2, 10, None, 37),
        (2, 10, Some(2), 35),
        (2, 17, None, 44),
        (3, 0, None, 54),
        (3, 18, None, 72),
    ] {
        game.run.act = act;
        game.run.floor = floor;
        game.golden_compass = compass;
        assert_eq!(canonical_progress(&game), expected);
    }
}

#[test]
fn v44_galactic_dust_block_preview_matches_play() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 65, 3, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    game.run
        .relics
        .push(content.relic_id("RELIC.GALACTIC_DUST").unwrap());
    game.galactic_dust = 9;
    let card = Card {
        id: content.card_id("CARD.FALLING_STAR").unwrap(),
        ..Card::default()
    };
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.enemies.truncate(1);
    combat.hits.truncate(1);
    combat.enemies[0].creature.hp = 999;
    combat.enemies[0].creature.max_hp = 999;
    combat.hand = vec![card];
    combat.stars = 2;
    combat.player.block = 0;
    let action = Action::Play {
        hand: 0,
        target: Some(0),
    };
    let preview = game.expected_action_outcome(&content, &action).unwrap();
    let mut played = game.clone();
    played.step(&content, action).unwrap();
    assert_eq!(preview.block, 10.0);
    assert_eq!(played.combat().unwrap().player.block as f32, preview.block);
    assert_eq!(played.galactic_dust, 1);
}

#[test]
fn lazy_encounters_match_the_constrained_bag_generator() {
    fn generate(content: &Content, groups: &[(&[Id], usize)], mut rng: Rng) -> Vec<Id> {
        let mut result = Vec::new();
        for &(candidates, count) in groups {
            let mut bag = candidates.to_vec();
            bag.sort_unstable();
            for _ in 0..count {
                if bag.is_empty() {
                    bag.extend_from_slice(candidates);
                    bag.sort_unstable();
                }
                let previous = result.last().copied();
                let allowed = |id: Id| {
                    previous != Some(id)
                        && previous.is_none_or(|previous| {
                            encounter_tags(content.encounters[id as usize].id)
                                & encounter_tags(content.encounters[previous as usize].id)
                                == 0
                        })
                };
                let constrained = bag.iter().copied().any(allowed);
                let index = loop {
                    let index = (rng.double() * bag.len() as f64) as usize;
                    if !constrained || allowed(bag[index]) {
                        break index;
                    }
                };
                result.push(bag.remove(index));
            }
        }
        result
    }

    let content = foundation_content();
    let act = content
        .acts
        .iter()
        .position(|act| {
            act.encounters
                .iter()
                .any(|id| content.encounters[*id as usize].id.contains("_WEAK"))
                && act
                    .encounters
                    .iter()
                    .any(|id| !content.encounters[*id as usize].id.contains("_WEAK"))
                && !act.elites.is_empty()
        })
        .unwrap() as Id;
    let mut game = Game::new_character_ascension(&content, 1, 0, 10).unwrap();
    game.begin_act(&content, act).unwrap();
    let weak = content.acts[act as usize]
        .encounters
        .iter()
        .copied()
        .filter(|id| content.encounters[*id as usize].id.contains("_WEAK"))
        .collect::<Vec<_>>();
    let regular = content.acts[act as usize]
        .encounters
        .iter()
        .copied()
        .filter(|id| !content.encounters[*id as usize].id.contains("_WEAK"))
        .collect::<Vec<_>>();
    let counts = (
        game.weak_encounters_left as usize,
        game.regular_encounters_left as usize,
        game.elite_encounters_left as usize,
    );
    let seed = 91;
    let expected = generate(
        &content,
        &[(&weak, counts.0), (&regular, counts.1)],
        Rng::from_seed(seed),
    );
    game.rngs.up_front = Rng::from_seed(seed);
    let actual = (0..expected.len())
        .map(|_| game.next_encounter(&content, false).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);

    let expected = generate(
        &content,
        &[(content.acts[act as usize].elites, counts.2)],
        Rng::from_seed(seed),
    );
    game.rngs.up_front = Rng::from_seed(seed);
    let actual = (0..expected.len())
        .map(|_| game.next_encounter(&content, true).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn v56_distinguishes_remaining_encounter_bags_with_the_same_last_encounter() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let (act, first_id, shared, second_id) = content
        .acts
        .iter()
        .enumerate()
        .find_map(|(act, def)| {
            let pool = def
                .encounters
                .iter()
                .copied()
                .filter(|id| !content.encounters[*id as usize].id.contains("_WEAK"))
                .collect::<Vec<_>>();
            pool.iter().copied().find_map(|shared| {
                let compatible = pool
                    .iter()
                    .copied()
                    .filter(|&id| {
                        id != shared
                            && encounter_tags(content.encounters[id as usize].id)
                                & encounter_tags(content.encounters[shared as usize].id)
                                == 0
                    })
                    .collect::<Vec<_>>();
                (compatible.len() >= 2).then_some((act as Id, compatible[0], shared, compatible[1]))
            })
        })
        .unwrap();
    let mut game = Game::new_character_ascension(&content, 568, 0, 10).unwrap();
    game.begin_act(&content, act).unwrap();
    game.weak_encounters_left = 0;
    game.regular_encounters_left = 3;
    game.encounters = vec![first_id, shared, second_id];
    let reach = |wanted| {
        (0..1024)
            .find_map(|seed| {
                let mut reached = game.clone();
                reached.rngs.up_front = Rng::from_seed(seed);
                (reached.next_encounter(&content, false) == Some(wanted)
                    && reached.next_encounter(&content, false) == Some(shared))
                .then_some(reached)
            })
            .unwrap()
    };
    let mut first = reach(first_id);
    let mut second = reach(second_id);
    assert_eq!(first.last_encounter, second.last_encounter);
    assert_ne!(first.encounters, second.encounters);
    assert_ne!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );
    assert_eq!(first.next_encounter(&content, false), Some(second_id));
    assert_eq!(second.next_encounter(&content, false), Some(first_id));
}

#[test]
fn encounter_bag_enforces_public_repeat_constraints() {
    let content = foundation_content();
    let (act, previous, allowed) = content
        .acts
        .iter()
        .enumerate()
        .find_map(|(act, def)| {
            def.encounters.iter().find_map(|&previous| {
                def.encounters
                    .iter()
                    .copied()
                    .find(|&next| {
                        previous != next
                            && encounter_tags(content.encounters[previous as usize].id)
                                & encounter_tags(content.encounters[next as usize].id)
                                == 0
                    })
                    .map(|next| (act as Id, previous, next))
            })
        })
        .unwrap();
    let mut game = Game::new_character_ascension(&content, 2, 0, 10).unwrap();
    game.begin_act(&content, act).unwrap();
    game.weak_encounters_left = 1;
    game.regular_encounters_left = 0;
    game.last_encounter = Some(previous);
    game.encounters = vec![previous, allowed];
    for seed in 1..=32 {
        let mut sampled = game.clone();
        sampled.rngs.up_front = Rng::from_seed(seed);
        assert_eq!(sampled.next_encounter(&content, false), Some(allowed));
    }
    game.encounters = vec![previous];
    assert_eq!(game.next_encounter(&content, false), Some(previous));
}

#[test]
fn replay_keeps_its_forced_encounter() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 3, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.replaying = true;
    game.encounters = vec![content.acts[0].encounters[0]];
    game.map = Map {
        nodes: vec![MapNode {
            floor: 1,
            lane: 0,
            room: Room::Combat,
            next: vec![],
        }]
        .into(),
        current: None,
    };
    resample_hidden(&mut game, &content, 9);
    game.step(&content, Action::Path(0)).unwrap();
    assert!(game.encounters.is_empty());
}

#[test]
fn direct_event_combat_does_not_consume_encounter_bags() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 3, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let before = (
        game.weak_encounters_left,
        game.regular_encounters_left,
        game.elite_encounters_left,
        game.encounters.clone(),
        game.elites.clone(),
    );
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    assert_eq!(
        before,
        (
            game.weak_encounters_left,
            game.regular_encounters_left,
            game.elite_encounters_left,
            game.encounters,
            game.elites,
        )
    );
}

#[test]
fn run_features_are_zero_at_terminal_states() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 12, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    assert!(
        run_features(&game, &content)
            .iter()
            .any(|&feature| feature != 0.0)
    );
    for phase in [Phase::Won, Phase::Dead] {
        game.phase = phase;
        assert_eq!(run_features(&game, &content), vec![0.0; PUBLIC_GLOBALS]);
    }
}

#[test]
fn run_features_are_floor_hp_resources_and_hp_weighted_resources() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 12, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let uncommon = content
        .cards
        .iter()
        .position(|card| card.rarity == CardRarity::Uncommon)
        .unwrap() as Id;
    let rare = content
        .cards
        .iter()
        .position(|card| card.rarity == CardRarity::Rare)
        .unwrap() as Id;
    game.run.hp = 7;
    game.run.max_hp = 80;
    game.run.gold = 123;
    game.run.deck = vec![
        Card {
            id: uncommon,
            upgrades: 2,
            ..Card::default()
        },
        Card {
            id: rare,
            upgrades: 1,
            ..Card::default()
        },
        Card::default(),
    ];
    game.run.relics = vec![
        crate::foundation::COMMON_RELICS[0],
        crate::foundation::UNCOMMON_RELICS[0],
        crate::foundation::RARE_RELICS[0],
    ];
    game.run.potions = vec![
        Some(crate::foundation::UNCOMMON_POTIONS[0]),
        Some(crate::foundation::RARE_POTIONS[0]),
        None,
    ];
    let features = run_features(&game, &content);
    assert_eq!(features.len(), PUBLIC_GLOBALS);
    assert_eq!(
        features[0],
        canonical_progress(&game) as f32 / (TERMINAL_CATEGORIES - 1) as f32
    );
    assert_eq!(features[1], 7.0);
    assert_eq!(features[2..14].iter().sum::<f32>(), 220.0);
    assert_eq!(features[14..].iter().sum::<f32>(), 7.0 * 220.0);
}

#[test]
fn content_fingerprint_tracks_gameplay_fields() {
    let content = foundation_content();
    let mut changed = foundation_content();
    assert_eq!(content_fingerprint(&content), content_fingerprint(&changed));
    assert_eq!(content.cards[0].id, changed.cards[0].id);
    changed.cards[0].cost[0] += 1;
    assert_ne!(content_fingerprint(&content), content_fingerprint(&changed));
}

#[test]
fn cancel_never_stalls_rewards_or_rest() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 17, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.phase = Phase::Rewards(Rewards {
        gold: 0,
        cards: vec![],
        card_rewards: vec![],
        relics: vec![],
        potions: vec![],
        removals: 0,
    });
    assert!(!game.actions(&content).contains(&Action::Cancel));
    game.phase = Phase::Rest;
    game.step(&content, Action::Cancel).unwrap();
    assert!(matches!(game.phase, Phase::Map));
}

#[test]
fn empty_crystal_clears_resume() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 19, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.crystal = Some(CrystalSphere {
        cells: vec![None; 121],
        clear: vec![false; 121],
        items: vec![],
        revealed: vec![],
        remaining: 1,
        big: false,
    });
    game.resume = Some(Phase::Map);
    game.phase = Phase::Event(0, vec![]);
    game.step(&content, Action::CrystalCell(0, 0)).unwrap();
    assert!(matches!(game.phase, Phase::Map));
    assert!(game.resume.is_none());
}

#[cfg(feature = "python")]
#[test]
fn room_frontier_preserves_pareto_routes_and_engines() {
    let content = foundation_content();
    let mut base = Game::new_character_ascension(&content, 21, 4, 10).unwrap();
    base.begin_act(&content, 0).unwrap();
    base.map.current = Some(0);
    base.run.hp = 50;
    let mut dominated = base.clone();
    dominated.run.hp = 49;
    let mut route = base.clone();
    route.map.current = Some(1);
    route.run.hp = 30;
    let mut engine = base.clone();
    engine.run.hp = 45;
    engine.run.deck.push(Card {
        id: content
            .cards
            .iter()
            .position(|card| card.id == "CARD.COUNTDOWN")
            .unwrap() as Id,
        ..Card::default()
    });
    let candidates = vec![(base, 0), (dominated, 1), (route, 2), (engine, 3)];
    let mut pareto = candidates.clone();
    python::retain_room_frontier(&mut pareto, &content, 4, 4);
    assert_eq!(pareto.len(), 3);
    assert!(!pareto.iter().any(|(_, id)| *id == 1));
    assert!(pareto.iter().any(|(_, id)| *id == 3));
    python::retain_room_frontier(&mut pareto, &content, 1, 2);
    assert!(pareto.iter().any(|(_, id)| *id == 2));
}

#[cfg(feature = "python")]
#[test]
fn search_keeps_only_mandatory_potion_discards() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 22, 3, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.run.potions[0] = Some(0);
    let actions = python::search_actions(&game, &content);
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Path(_)))
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::DiscardPotion(_)))
    );
    game.replacing_potion = true;
    let actions = python::search_actions(&game, &content);
    assert!(!actions.is_empty());
    assert!(
        actions
            .iter()
            .all(|action| matches!(action, Action::DiscardPotion(_)))
    );
}

#[cfg(feature = "python")]
#[test]
fn combat_search_selects_best_clear_in_layer() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 23, 0, 0).unwrap();
    game.begin_act(&content, 0).unwrap();
    let encounter = content
        .encounters
        .iter()
        .position(|encounter| encounter.id == "ENCOUNTER.SLIMES_WEAK")
        .unwrap() as Id;
    game.start_combat(&content, encounter).unwrap();
    let strike = content
        .cards
        .iter()
        .position(|card| card.id == "CARD.STRIKE_IRONCLAD")
        .unwrap() as Id;
    let feed = content
        .cards
        .iter()
        .position(|card| card.id == "CARD.FEED")
        .unwrap() as Id;
    let before = game.run.max_hp;
    let Phase::Combat(combat) = &mut game.phase else {
        panic!("combat did not start");
    };
    combat.hand = vec![
        Card {
            id: strike,
            ..Card::default()
        },
        Card {
            id: feed,
            ..Card::default()
        },
    ];
    combat.draw.clear();
    combat.discard.clear();
    combat.exhaust.clear();
    combat.energy = 3;
    for enemy in &mut combat.enemies {
        enemy.creature.hp = 0;
        enemy.creature.powers.clear();
    }
    combat.enemies[0].creature.hp = 1;
    let (result, plan) = python::combat_search(&game, &content, 8, 1);
    assert!(matches!(plan[0], Action::Play { hand: 1, .. }));
    assert!(result.run.max_hp > before);

    let blood = content
        .potions
        .iter()
        .position(|potion| potion.id == "POTION.BLOOD_POTION")
        .unwrap() as Id;
    game.run.potions[0] = Some(blood);
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!();
    };
    combat.hand.truncate(1);
    combat.player.hp = 40;
    let (result, plan) = python::combat_search(&game, &content, 8, 6);
    assert!(matches!(plan[0], Action::Potion { slot: 0, .. }));
    assert!(result.run.hp > 40);
}

#[test]
fn v62_packed_observation_carries_run_features_and_is_digest_protected() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 600, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let observation = build_observation(&game, &content, layout);
    assert_eq!(observation.globals, run_features(&game, &content));
    assert!(observation.globals.iter().all(|value| value.is_finite()));
    let (character, counts) = (observation.character, observation_counts(&observation));
    let mut globals = observation.globals.clone();
    let mut words = observation_words(&observation);
    let digest = packed_observation_digest(character, &globals, counts, &words);
    assert_eq!(observation_digest(&observation), digest);
    for index in [0, words.len() - 1] {
        words[index] ^= 1;
        assert_ne!(packed_observation_digest(character, &globals, counts, &words), digest);
        words[index] ^= 1;
    }
    globals[0] += 1.0;
    assert_ne!(packed_observation_digest(character, &globals, counts, &words), digest);
}

#[test]
fn v36_orb_previews_match_dualcast_overflow_and_zero_slots() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 71, 2, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    game.run.relics.clear();
    let dualcast = Card {
        id: content.card_id("CARD.DUALCAST").unwrap(),
        ..Card::default()
    };
    let zap = Card {
        id: content.card_id("CARD.ZAP").unwrap(),
        ..Card::default()
    };
    let orb = |name| content.orbs.iter().position(|orb| orb.id == name).unwrap() as Id;
    let (lightning, dark) = (orb("ORB.LIGHTNING_ORB"), orb("ORB.DARK_ORB"));
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.enemies.truncate(1);
    combat.hits.truncate(1);
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;
    combat.enemies[0].creature.block = 0;
    combat.enemies[0].creature.powers.clear();
    combat.player.powers.clear();
    combat.energy = 3;
    combat.hand = vec![dualcast];
    combat.orb_slots = 1;
    combat.orbs = vec![Orb {
        id: lightning,
        value: 0,
    }];
    let action = Action::Play {
        hand: 0,
        target: None,
    };
    let preview = game.expected_action_outcome(&content, &action).unwrap();
    assert_eq!(preview.hp_loss, [16.0]);
    let mut played = game.clone();
    played.step(&content, action.clone()).unwrap();
    assert_eq!(played.combat().unwrap().enemies[0].creature.hp, 84);

    for (slots, expected) in [(1, 11), (0, 0)] {
        let mut game = game.clone();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![zap];
        combat.orb_slots = slots;
        combat.orbs = (slots > 0)
            .then_some(Orb {
                id: dark,
                value: 11,
            })
            .into_iter()
            .collect();
        let preview = game.expected_action_outcome(&content, &action).unwrap();
        assert_eq!(preview.hp_loss, [expected as f32]);
        let mut played = game.clone();
        played.step(&content, action.clone()).unwrap();
        let combat = played.combat().unwrap();
        assert_eq!(combat.enemies[0].creature.hp, 100 - expected);
        assert_eq!((combat.orb_slots, combat.orbs.len()), (1, 1));
    }
}

#[test]
fn v36_fisticuffs_and_misery_previews_match_play() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 72, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    game.run.relics.clear();
    let fisticuffs = Card {
        id: content.card_id("CARD.FISTICUFFS").unwrap(),
        ..Card::default()
    };
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.enemies.truncate(1);
    combat.hits.truncate(1);
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;
    combat.enemies[0].creature.block = 3;
    combat.enemies[0].creature.powers = vec![Power {
        id: power_id::VULNERABLE,
        amount: 1,
        ..Power::default()
    }];
    combat.hand = vec![fisticuffs];
    combat.energy = 3;
    combat.player.block = 0;
    combat.player.powers = [
        (power_id::STRENGTH, 2),
        (power_id::DEXTERITY, 3),
        (power_id::FRAIL, 1),
    ]
    .map(|(id, amount)| Power {
        id,
        amount,
        ..Power::default()
    })
    .to_vec();
    let action = Action::Play {
        hand: 0,
        target: Some(0),
    };
    let preview = game.expected_action_outcome(&content, &action).unwrap();
    assert_eq!((preview.block, preview.hp_loss[0]), (12.0, 10.0));
    let mut played = game.clone();
    played.step(&content, action.clone()).unwrap();
    assert_eq!(played.combat().unwrap().player.block, 12);
    assert_eq!(played.combat().unwrap().enemies[0].creature.hp, 90);

    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.player.powers.push(Power {
        id: power_id::NO_BLOCK,
        amount: 1,
        ..Power::default()
    });
    let preview = game.expected_action_outcome(&content, &action).unwrap();
    assert_eq!(preview.block, 0.0);

    let misery = Card {
        id: content.card_id("CARD.MISERY").unwrap(),
        ..Card::default()
    };
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.hand = vec![misery];
    combat.player.powers.clear();
    combat.enemies[0].creature.block = 0;
    let preview = game.expected_action_outcome(&content, &action).unwrap();
    let mut played = game.clone();
    played.step(&content, action).unwrap();
    let hp = played.combat().unwrap().enemies[0].creature.hp;
    assert_eq!(preview.hp_loss[0], (100 - hp) as f32);
}

#[test]
fn v36_duplicate_hand_exhaust_previews_match_play() {
    let mut content = foundation_content();
    let id = content.card_id("CARD.ZAP").unwrap();
    for (seed, effect) in [
        (
            73,
            Effect::Select(
                Pile::Hand,
                CardFilter::Any,
                [1, 1],
                false,
                false,
                CardOp::Move(Pile::Exhaust),
            ),
        ),
        (74, Effect::ExhaustForBlock(Amount::fixed(5, 5))),
    ] {
        content.cards[id as usize].effects = Box::leak(vec![effect].into_boxed_slice());
        let mut game = Game::new_character_ascension(&content, seed, 2, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        game.run.relics.clear();
        let card = Card {
            id,
            ..Card::default()
        };
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![card, card];
        combat.energy = 3;
        combat.player.block = 0;
        combat.player.powers.clear();
        let action = Action::Play {
            hand: 0,
            target: None,
        };
        let preview = game.expected_action_outcome(&content, &action).unwrap();
        assert_eq!(preview.exhaust, 1.0);
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        assert_eq!(played.combat().unwrap().exhaust.len(), 1);
        assert_eq!(played.combat().unwrap().player.block as f32, preview.block);
    }
}

#[test]
fn v36_play_draw_and_exhaust_hooks_match_play() {
    let mut content = foundation_content();
    let id = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
    content.cards[id as usize].effects = Box::leak(Box::new([
        Effect::Attack(Target::ChosenEnemy, Amount::fixed(6, 6), 1),
        Effect::Draw(1),
    ]));
    content.cards[id as usize].flags = [EXHAUST; 2];
    let ids = [
        "POWER.FEEL_NO_PAIN_POWER",
        "POWER.DARK_EMBRACE_POWER",
        "POWER.JUGGERNAUT_POWER",
        "POWER.AFTERIMAGE_POWER",
        "POWER.SERPENT_FORM_POWER",
        "POWER.SPEEDSTER_POWER",
        "POWER.ITERATION_POWER",
    ]
    .map(|name| {
        content
            .powers
            .iter()
            .position(|power| power.id == name)
            .unwrap() as Id
    });
    let mut game = Game::new_character_ascension(&content, 75, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    game.run.relics = vec![content.relic_id("RELIC.CHARONS_ASHES").unwrap()];
    let card = Card {
        id,
        ..Card::default()
    };
    let filler = Card {
        id: content.card_id("CARD.DEFEND_IRONCLAD").unwrap(),
        ..Card::default()
    };
    let status = Card {
        id: content.card_id("CARD.DAZED").unwrap(),
        ..Card::default()
    };
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.enemies.truncate(1);
    combat.hits.truncate(1);
    combat.enemies[0].creature.hp = 100;
    combat.enemies[0].creature.max_hp = 100;
    combat.enemies[0].creature.block = 0;
    combat.enemies[0].creature.powers.clear();
    combat.hand = vec![card];
    combat.draw = vec![filler, filler, status];
    combat.known_draw_top = 3;
    combat.discard.clear();
    combat.energy = 3;
    combat.player.block = 0;
    combat.player.powers = ids
        .into_iter()
        .zip([3, 1, 5, 2, 3, 2, 1])
        .map(|(id, amount)| Power {
            id,
            amount,
            ..Power::default()
        })
        .collect();
    let action = Action::Play {
        hand: 0,
        target: Some(0),
    };
    let preview = game.expected_action_outcome(&content, &action).unwrap();
    assert_eq!((preview.block, preview.draw, preview.exhaust), (5.0, 3.0, 1.0));
    assert_eq!(preview.hp_loss, [28.0]);
    let drawn = game.combat().unwrap().drawn;
    let mut played = game.clone();
    played.step(&content, action).unwrap();
    let combat = played.combat().unwrap();
    assert_eq!(combat.player.block, 5);
    assert_eq!(combat.enemies[0].creature.hp, 72);
    assert_eq!(combat.drawn - drawn, 3);
    assert_eq!(combat.exhaust.len(), 1);
}

/// The observation's words with candidates sorted: equal for equal model inputs.
fn canonical_words(observation: &Observation) -> Vec<u32> {
    let mut words = vec![observation.character as u32];
    words.extend(observation.globals.iter().map(|value| value.to_bits()));
    for token in &observation.tokens {
        token_words(token, token.parent, &mut words);
    }
    let mut candidates = (observation.candidates.iter())
        .map(|candidate| {
            let mut words = vec![candidate.legal as u32];
            for token in std::iter::once(&candidate.token).chain(&candidate.attached) {
                token_words(token, NONE, &mut words);
            }
            words
        })
        .collect::<Vec<_>>();
    candidates.sort();
    words.extend(candidates.concat());
    words
}

fn has(token: &Token, layout: Layout, namespace: Semantic, value: u32) -> bool {
    token.c.contains(&layout.semantic(namespace, value))
}

fn amount_of(token: &Token, layout: Layout, namespace: Semantic, value: u32) -> Option<f32> {
    let code = layout.semantic(namespace, value);
    (token.c.iter().position(|&slot| slot == code)).map(|slot| token.a[slot])
}

fn placed(observation: &Observation, place: Place) -> impl Iterator<Item = (usize, &Token)> {
    (observation.tokens.iter().enumerate()).filter(move |(_, token)| token.place == place)
}

fn with_ctx<R>(game: &Game, content: &Content, build: impl FnOnce(Ctx) -> R) -> R {
    let masters = master_cards(&game.run.deck);
    build(Ctx { game, content, layout: Layout::new(content), masters: &masters })
}

fn queue_tokens(game: &Game, content: &Content) -> Vec<Token> {
    with_ctx(game, content, |ctx| {
        let mut encoder = Encoder { ctx, tokens: Vec::new() };
        encoder.queue();
        encoder.tokens
    })
}

#[test]
fn v53_observation_is_canonical_under_hidden_resampling() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut first = Game::new_character_ascension(&content, 401, 2, 10).unwrap();
    first.begin_act(&content, 0).unwrap();
    let mut second = first.clone();
    resample_hidden(&mut first, &content, 17);
    resample_hidden(&mut second, &content, 99);
    assert_eq!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );
}

#[test]
fn v56_public_state_mutations_change_observation() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 402, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let base = canonical_words(&build_observation(&game, &content, layout));
    for mutation in 0..4 {
        let mut changed = game.clone();
        match mutation {
            0 => changed.run.gold += 1,
            1 => match &mut changed.phase {
                Phase::Combat(combat) => combat.player.block += 1,
                _ => unreachable!(),
            },
            2 => match &mut changed.phase {
                Phase::Combat(combat) => combat.enemies[0].creature.hp -= 1,
                _ => unreachable!(),
            },
            3 => match &mut changed.phase {
                Phase::Combat(combat) => combat.energy -= 1,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        assert_ne!(
            canonical_words(&build_observation(&changed, &content, layout)),
            base,
            "public mutation {mutation} was not observed"
        );
    }
}

#[test]
fn v53_combat_choice_keeps_typed_filter_and_operation_codes() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 407, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let target = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.choice = Some(Choice {
        pile: Pile::Draw,
        filter: CardFilter::TypeWithoutTurnFlag(CardType::Attack, 0x8123),
        op: CardOp::Transform(target, 7),
        remaining: 2,
        optional: true,
    });
    combat.enemy_turn = true;
    combat.queue.push(Pending {
        effect: Effect::Step(Step::Flush),
        context: Context::player(),
    });
    combat.force_end = true;
    let token = with_ctx(&game, &content, |ctx| ctx.phase(&game.phase, false));
    for (namespace, value) in [
        (Semantic::Pile, Pile::Draw as u32),
        (Semantic::Filter, 3),
        (Semantic::CardType, CardType::Attack as u32),
        (Semantic::CardOp, 7),
        (Semantic::Card, base_card_id(target as u32)),
    ] {
        assert!(has(&token, layout, namespace, value));
    }
    for bit in [0, 1, 5, 8, 15] {
        assert!(has(&token, layout, Semantic::TurnFlagBit, bit));
    }
    for flag in [
        Flag::ChoicePending,
        Flag::Optional,
        Flag::EnemyTurn,
        Flag::TurnEnding,
        Flag::ForcedEnd,
    ] {
        assert!(has(&token, layout, Semantic::Flag, flag as u32));
    }
    assert_eq!(token.f[..3], [0.2, 0.0, 0.0]);
}

#[test]
fn v53_map_is_relabel_invariant_and_edge_sensitive() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut first = Game::new_character_ascension(&content, 404, 0, 10).unwrap();
    first.begin_act(&content, 0).unwrap();
    let mut second = first.clone();
    let len = second.map.nodes.len();
    let old = second.map.nodes.clone();
    let permutation = (0..len).rev().collect::<Vec<_>>();
    let mut inverse = vec![0; len];
    for (new, &old_index) in permutation.iter().enumerate() {
        inverse[old_index] = new;
    }
    second.map.nodes = permutation
        .iter()
        .map(|&old_index| {
            let mut node = old[old_index].clone();
            node.next = node.next.iter().map(|&next| inverse[next]).collect();
            node
        })
        .collect::<Vec<_>>()
        .into();
    second.map.current = first.map.current.map(|current| inverse[current]);
    assert_eq!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );
    let mut changed = first.clone();
    let node = std::sync::Arc::make_mut(&mut changed.map.nodes)
        .iter_mut()
        .find(|node| node.next.len() > 1)
        .unwrap();
    node.next.pop();
    let edges = |game: &Game| {
        let observation = build_observation(game, &content, layout);
        placed(&observation, Place::MapEdge).map(|(_, token)| token.clone()).collect::<Vec<_>>()
    };
    assert_ne!(edges(&first), edges(&changed));
}

#[test]
fn canonical_map_cache_is_exact_and_tracks_dynamic_geometry() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 409, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let mut cache = None;
    let exact = |game: &Game, cache: &mut Option<CanonicalMapCache>| {
        assert_eq!(
            build_observation_cached(game, &content, layout, cache),
            build_observation(game, &content, layout)
        );
    };
    exact(&game, &mut cache);
    let nodes = cache.as_ref().unwrap().map.0.clone();

    game.map.current = Some(0);
    exact(&game, &mut cache);
    assert!(Arc::ptr_eq(&nodes, &cache.as_ref().unwrap().map.0));

    let position = (game.map.nodes[0].lane, game.map.nodes[0].floor);
    game.spoils = Some(position);
    exact(&game, &mut cache);
    assert!(!Arc::ptr_eq(&nodes, &cache.as_ref().unwrap().map.0));

    game.fur_coat_act = Some(game.run.act);
    game.fur_coat.push(position);
    exact(&game, &mut cache);

    let nodes = cache.as_ref().unwrap().map.0.clone();
    let source = Arc::make_mut(&mut game.map.nodes);
    source[0].room = if source[0].room == Room::Combat {
        Room::Shop
    } else {
        Room::Combat
    };
    exact(&game, &mut cache);
    assert!(!Arc::ptr_eq(&nodes, &cache.as_ref().unwrap().map.0));
}

#[test]
fn v53_map_tokens_come_first_and_edges_are_topological() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 408, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let observation = build_observation(&game, &content, layout);
    let nodes = game.map.nodes.len();
    assert!(observation.tokens[..nodes].iter().all(|token| token.place == Place::MapNode));
    let edges = (placed(&observation, Place::MapEdge))
        .map(|(_, token)| (token.parent, token.link))
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), game.map.nodes.iter().map(|node| node.next.len()).sum::<usize>());
    assert!(edges.is_sorted_by_key(|&(source, _)| source));
    assert!(edges.iter().all(|&(source, target)| source < target && (target as usize) < nodes));
}

#[test]
fn v53_path_candidates_reuse_canonical_destination_ids() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 405, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let observation = build_observation(&game, &content, layout);
    let run = |observation: &Observation| placed(observation, Place::Run).next().unwrap().1.link;
    assert_eq!(run(&observation), 0);
    for candidate in &observation.candidates {
        match candidate.action {
            Action::Path(_) => assert!((candidate.token.link as usize) < game.map.nodes.len()),
            _ => assert_eq!(candidate.token.link, NONE),
        }
    }
    let (index, link) = (observation.candidates.iter())
        .find_map(|row| match row.action {
            Action::Path(index) => Some((index, row.token.link)),
            _ => None,
        })
        .unwrap();
    game.map.current = Some(index);
    assert_eq!(run(&build_observation(&game, &content, layout)), link);

    let current = game
        .map
        .nodes
        .iter()
        .enumerate()
        .find(|(_, node)| {
            !node.next.is_empty()
                && game.map.nodes.iter().enumerate().any(|(candidate, next)| {
                    next.floor == node.floor + 1 && !node.next.contains(&candidate)
                })
        })
        .map(|(index, _)| index)
        .unwrap();
    game.map.current = Some(current);
    game.run
        .relics
        .push(content.relic_id("RELIC.WINGED_BOOTS").unwrap());
    game.winged_boots = 0;
    let observation = build_observation(&game, &content, layout);
    let paths = (observation.candidates.iter())
        .filter(|row| matches!(row.action, Action::Path(_)))
        .collect::<Vec<_>>();
    for kind in [1, 2] {
        assert!(paths.iter().any(|row| has(&row.token, layout, Semantic::RouteKind, kind)));
    }
    let links = paths.iter().map(|row| row.token.link).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(links.len(), paths.len());
}

#[test]
fn card_flags_are_dense_numeric_features_independent_of_the_consumer() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 549, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let mut card = Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        flags: u16::MAX,
        turn_flags: u16::MAX,
        ..Card::default()
    };
    card.add_cost(0, ABSOLUTE);
    card.free_this_turn();
    game.run.deck.push(card);
    let observation = build_observation(&game, &content, layout);
    let actual = (placed(&observation, Place::Deck))
        .find(|(_, token)| token.f[4..36].iter().all(|&value| value == 1.0))
        .unwrap()
        .1;
    let [deck, child] = [(Place::Deck, NONE), (Place::Child, 7)]
        .map(|(place, parent)| with_ctx(&game, &content, |ctx| ctx.card(card, place, parent)));
    assert_eq!(actual, &deck);
    assert_eq!((&deck.c, &deck.a, &deck.f), (&child.c, &child.a, &child.f));
    // Absolute, this turn, until played, not reduce-only, star; three modifiers.
    assert_eq!(actual.f[36..], [1.0, 1.0, 1.0, 0.0, 1.0, 0.5]);
}

#[test]
fn v57_empty_potion_is_an_item_and_slots_are_not_semantic() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 549, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let observation = build_observation(&game, &content, layout);
    let empty = layout.semantic(Semantic::Potion, content.potions.len() as u32);
    let potions = placed(&observation, Place::Potion).collect::<Vec<_>>();
    assert_eq!(potions.len(), game.run.potions.len());
    assert!(potions.iter().all(|(_, token)| token.c == [empty]));
}

#[test]
fn v57_map_has_only_real_nodes_and_untyped_edges() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 549, 0, 10).unwrap();
    game.begin_run(&content).unwrap();
    let observation = build_observation(&game, &content, layout);
    let nodes = placed(&observation, Place::MapNode).map(|(_, token)| token).collect::<Vec<_>>();
    assert_eq!(nodes.len(), game.map.nodes.len());
    let ancient = content.events[game.ancient.unwrap() as usize].id;
    let ancient = [
        "EVENT.NEOW",
        "EVENT.OROBAS",
        "EVENT.PAEL",
        "EVENT.TEZCATARA",
        "EVENT.DARV",
        "EVENT.NONUPEIPE",
        "EVENT.TANX",
        "EVENT.VAKUU",
    ]
    .iter()
    .position(|&name| name == ancient)
    .unwrap();
    assert!(has(nodes[0], layout, Semantic::MapNodeKind, 6 + ancient as u32));
    let boss = game.bosses[0].unwrap();
    let boss = 14 + map_bosses(&content).iter().position(|&id| id == boss).unwrap();
    assert!(has(nodes[nodes.len() - 1], layout, Semantic::MapNodeKind, boss as u32));
    assert!(
        placed(&observation, Place::MapEdge)
            .all(|(_, token)| token.c.is_empty() && token.a.is_empty() && token.f.is_empty())
    );
}

#[test]
fn v57_control_and_resolving_play_state_are_continuations() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 549, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    let card = combat.hand[0];
    combat.plays.push(Play {
        card,
        plays: 4,
        done: 0,
        energy: 3,
        stars: 2,
        manual: true,
        context: Context::card(card),
        recorded: vec![],
    });
    combat.unsettling_lamp = Some(card.id);
    combat.extra_turn = true;
    game.run_queue.push(RunEffect::FinishCooking);
    game.reward_continuation = Some(RewardContinuation::AddRandomCurse);
    let tokens = queue_tokens(&game, &content);
    let playing = (tokens.iter())
        .position(|token| has(token, layout, Semantic::PlayKind, 1))
        .unwrap();
    assert_eq!(tokens[playing].place, Place::Continuation);
    assert_eq!(tokens[playing].f, [3.0 / 5.0, 2.0 / 5.0, 4.0 / 5.0]);
    assert!(has(&tokens[playing], layout, Semantic::Flag, Flag::UnsettlingLamp as u32));
    let version = card_version_id(&content, card.id, card.upgrades > 0, card.variant);
    assert!(tokens.iter().any(|token| token.parent == playing as u32
        && token.domain == CARD_DOMAIN as u8
        && has(token, layout, Semantic::Card, version)));
    let cooking = event_effect_index(&RunEffect::FinishCooking) as u32;
    assert!(tokens.iter().any(|token| has(token, layout, Semantic::RunEffect, cooking)));
    for kind in 2..=3 {
        assert!(tokens.iter().any(|token| has(token, layout, Semantic::PlayKind, kind)));
    }
}

#[test]
fn v56_continuation_booleans_and_discard_counts_do_not_alias() {
    let content = foundation_content();
    let game = Game::new_character_ascension(&content, 565, 0, 10).unwrap();
    let effect = |effect| {
        with_ctx(&game, &content, |ctx| {
            let mut items = Items::new(ctx, 0, NONE);
            items.effect(effect, None, false);
            let token = items.out.remove(0);
            (token.c, token.a, token.f)
        })
    };
    for (left, right) in [
        (
            Effect::RandomCard(Pile::Hand, CardType::Attack, 1, false),
            Effect::RandomCard(Pile::Hand, CardType::Attack, 1, true),
        ),
        (
            Effect::OfferColorless(3, false, true),
            Effect::OfferColorless(3, true, true),
        ),
        (
            Effect::OfferCharacter(3, false),
            Effect::OfferCharacter(3, true),
        ),
        (
            Effect::DistinctCharacter(Pile::Hand, 2, false),
            Effect::DistinctCharacter(Pile::Hand, 2, true),
        ),
        (
            Effect::AutoPlayDraw(Amount::fixed(2, 2), false),
            Effect::AutoPlayDraw(Amount::fixed(2, 2), true),
        ),
        (Effect::Discard(1, false), Effect::Discard(1, true)),
        (Effect::Exhaust(1, false), Effect::Exhaust(1, true)),
        (
            Effect::Upgrade(Pile::Hand, 1, false),
            Effect::Upgrade(Pile::Hand, 1, true),
        ),
        (Effect::Evoke(false), Effect::Evoke(true)),
    ] {
        assert_ne!(effect(left), effect(right));
    }
    assert_ne!(
        effect(Effect::Discard(1, false)),
        effect(Effect::Discard(2, false))
    );
    let select = |first, second| {
        effect(Effect::Select(
            Pile::Hand,
            CardFilter::Any,
            [2, 3],
            first,
            second,
            CardOp::Move(Pile::Exhaust),
        ))
    };
    assert_ne!(select(false, false), select(true, false));
    assert_ne!(select(false, false), select(false, true));
    let concepts = |(mut c, ..): (Vec<u32>, Vec<f32>, Vec<f32>)| {
        c.sort();
        c
    };
    assert_ne!(concepts(select(true, false)), concepts(select(false, true)));
    assert_ne!(
        concepts(effect(Effect::OfferColorless(3, true, false))),
        concepts(effect(Effect::OfferColorless(3, false, true)))
    );
    assert_ne!(
        concepts(effect(Effect::PassiveLightning(2))),
        concepts(select(false, false))
    );
    assert_ne!(
        effect(Effect::PassiveLightning(1)).2,
        effect(Effect::PassiveLightning(2)).2
    );
    assert_ne!(
        effect(Effect::CopyCard(Pile::Hand, 1)).2,
        effect(Effect::CopyCard(Pile::Hand, 2)).2
    );
    assert_eq!(
        effect(Effect::CopyCard(Pile::Hand, 1)).2,
        effect(Effect::CopyCard(Pile::Draw, 1)).2
    );

    let run_effect = |effect| {
        with_ctx(&game, &content, |ctx| {
            let mut items = Items::new(ctx, 0, NONE);
            items.run_effect(effect);
            items.out.remove(0).c
        })
    };
    assert_ne!(
        run_effect(RunEffect::RandomPotionReward(false)),
        run_effect(RunEffect::RandomPotionReward(true))
    );
}

#[test]
fn v61_status_cards_are_player_effects_and_enemy_properties_are_actor_features() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 610, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    let card = combat.hand[0];
    combat.nightmares.push((card, 2));
    combat.history_course = Some(card);
    combat.hits[0] = 3;
    combat.enemies[0].value = 12;
    combat.enemies[0].repeats = 2;
    let observation = build_observation(&game, &content, layout);
    let player = placed(&observation, Place::Player).next().unwrap().0 as u32;
    let flagged = |flag: Flag| {
        (placed(&observation, Place::Effect))
            .filter(|(_, token)| {
                token.parent == player
                    && token.domain == CARD_DOMAIN as u8
                    && has(token, layout, Semantic::Flag, flag as u32)
            })
            .map(|(_, token)| token)
            .collect::<Vec<_>>()
    };
    let (nightmares, course) = (flagged(Flag::Nightmare), flagged(Flag::HistoryCourse));
    assert_eq!((nightmares.len(), course.len()), (2, 1));
    assert_ne!(nightmares[0].c, course[0].c);
    let enemy = placed(&observation, Place::Enemy).next().unwrap().1;
    assert_eq!(enemy.f[13..16], [12.0 / 30.0, 3.0 / 10.0, 2.0 / 10.0]);
}

#[test]
fn v55_potion_replacement_menu_is_unique_and_exactly_legal() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 554, 0, 24).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.phase = Phase::Map;
    game.run.potions[0] = Some(content.potion_id("POTION.BLOOD_POTION").unwrap());
    game.run.potions[1] = Some(content.potion_id("POTION.FRUIT_JUICE").unwrap());
    game.pending_potion = Some(content.potion_id("POTION.ENTROPIC_BREW").unwrap());
    game.replacing_potion = true;
    let legal = game.actions(&content);
    let (represented, mask) = candidate_actions(&game, &content);
    assert_eq!(represented, legal);
    assert!(mask.into_iter().all(|legal| legal));
    let observation = build_observation(&game, &content, layout);
    assert_eq!(
        observation
            .candidates
            .iter()
            .map(|candidate| candidate.action.clone())
            .collect::<Vec<_>>(),
        legal
    );
}

#[test]
fn v58_unequal_target_previews_remain_distinct_candidates() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let encounter = content
        .encounters
        .iter()
        .position(|encounter| encounter.enemies.len() >= 2)
        .unwrap() as Id;
    let mut game = Game::new_character_ascension(&content, 567, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, encounter).unwrap();
    let strike = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.enemies.truncate(2);
    combat.hits.resize(2, 0);
    combat.enemies[0].creature.hp = 5;
    combat.enemies[0].creature.block = 0;
    combat.enemies[1].creature.hp = 50;
    combat.enemies[1].creature.block = 20;
    combat.hand = vec![Card {
        id: strike,
        ..Card::default()
    }];
    combat.energy = 99;
    combat.queue.clear();
    combat.choice = None;
    combat.player.hp = combat.player.hp.max(1);
    let observation = build_observation(&game, &content, layout);
    let [first, second] = [0, 1].map(|target| {
        (observation.candidates.iter())
            .find(|row| row.action == Action::Play { hand: 0, target: Some(target) })
            .unwrap()
    });
    assert_ne!(first.token.f, second.token.f);
    assert_ne!(first.token.c, second.token.c);
    assert_eq!(first.attached[0], second.attached[0]);
}

#[test]
fn v55_power_trigger_order_is_semantic_and_actor_ratios_are_finite() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 554, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.player.powers = vec![
        Power {
            id: 0,
            amount: 1,
            ..Power::default()
        },
        Power {
            id: 1,
            amount: 1,
            ..Power::default()
        },
    ];
    let mut ordered = game.clone();
    let first = build_observation(&game, &content, layout);
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.player.powers.reverse();
    let second = build_observation(&game, &content, layout);
    let visible = |observation: &Observation| {
        (observation.tokens.iter())
            .filter(|token| token.domain == POWER_DOMAIN as u8)
            .map(|token| (token.c.clone(), token.a.clone(), token.f.clone()))
            .collect::<Vec<_>>()
    };
    assert_ne!(visible(&first), visible(&second));
    let mut zero = game.clone();
    ordered.step(&content, Action::EndTurn).unwrap();
    game.step(&content, Action::EndTurn).unwrap();
    assert_ne!(
        canonical_words(&build_observation(&ordered, &content, layout)),
        canonical_words(&build_observation(&game, &content, layout))
    );
    let Phase::Combat(combat) = &mut zero.phase else {
        unreachable!()
    };
    (combat.player.hp, combat.player.max_hp) = (0, 0);
    let player = with_ctx(&zero, &content, |ctx| {
        let mut encoder = Encoder { ctx, tokens: Vec::new() };
        encoder.actors();
        encoder.tokens.swap_remove(0)
    });
    assert!(player.f.iter().all(|value| value.is_finite()));
}

#[test]
fn v56_ordered_piles_change_model_inputs_and_successors() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut first = Game::new_character_ascension(&content, 558, 0, 10).unwrap();
    first.begin_act(&content, 0).unwrap();
    first
        .start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let bash = Card {
        id: content.card_id("CARD.BASH").unwrap(),
        ..Card::default()
    };
    let right = content.card_id("CARD.RIGHT_HAND_HAND").unwrap();
    let Phase::Combat(combat) = &mut first.phase else {
        unreachable!()
    };
    combat.hand = vec![bash];
    combat.discard = vec![
        Card {
            id: right,
            value: 1,
            ..Card::default()
        },
        Card {
            id: right,
            value: 2,
            ..Card::default()
        },
    ];
    combat.energy = 10;
    combat.enemies[0].creature.hp = 999;
    combat.enemies[0].creature.max_hp = 999;
    let mut second = first.clone();
    if let Phase::Combat(combat) = &mut second.phase {
        combat.discard.reverse();
    }
    assert_ne!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );
    let action = Action::Play {
        hand: 0,
        target: Some(0),
    };
    first.step(&content, action.clone()).unwrap();
    second.step(&content, action).unwrap();
    assert_ne!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );

    let Phase::Combat(combat) = &mut first.phase else {
        unreachable!()
    };
    combat.hand = vec![
        Card { value: 1, ..bash },
        Card { value: 2, ..bash },
        Card { value: 3, ..bash },
    ];
    combat.choice = Some(Choice {
        pile: Pile::Hand,
        filter: CardFilter::Any,
        op: CardOp::TurnFlag(RETAIN),
        remaining: 1,
        optional: false,
    });
    combat.queue = vec![Pending {
        effect: Effect::ExhaustAttackStep(Target::ChosenEnemy, Amount::fixed(1, 1), 3),
        context: Context {
            target: Some(0),
            ..Context::player()
        },
    }];
    let mut second = first.clone();
    if let Phase::Combat(combat) = &mut second.phase {
        combat.hand.swap(0, 1);
    }
    first.step(&content, Action::Choose(2)).unwrap();
    second.step(&content, Action::Choose(2)).unwrap();
    assert_ne!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );
}

#[test]
fn v56_unknown_draw_middle_remains_permutation_invariant() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut first = Game::new_character_ascension(&content, 559, 0, 10).unwrap();
    first.begin_act(&content, 0).unwrap();
    first
        .start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let cards = first.run.deck.iter().copied().take(5).collect();
    let Phase::Combat(combat) = &mut first.phase else {
        unreachable!()
    };
    combat.draw = cards;
    combat.known_draw_bottom = 1;
    combat.known_draw_top = 1;
    let mut second = first.clone();
    if let Phase::Combat(combat) = &mut second.phase {
        combat.draw[1..4].reverse();
    }
    assert_eq!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );
}

#[test]
fn v56_enemy_instance_renumbering_preserves_wriggler_transitions() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let wriggler = content
        .enemies
        .iter()
        .position(|enemy| enemy.id == "MONSTER.WRIGGLER")
        .unwrap() as Id;
    let mut first = Game::new_character_ascension(&content, 561, 0, 10).unwrap();
    first.begin_act(&content, 0).unwrap();
    first
        .start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let Phase::Combat(combat) = &mut first.phase else {
        unreachable!()
    };
    combat.enemies.truncate(1);
    combat.hits.truncate(1);
    combat.player.hp = 999;
    combat.player.max_hp = 999;
    combat.enemies[0].creature.id = wriggler;
    combat.enemies[0].instance = 1;
    combat.enemies[0].move_index = 0;
    combat.enemies[0].last_move = 0;
    combat.enemies[0].move_history = vec![0];
    let mut second = first.clone();
    let Phase::Combat(combat) = &mut second.phase else {
        unreachable!()
    };
    combat.enemies[0].instance = 2;
    assert_eq!(first.actions(&content), second.actions(&content));
    assert_eq!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );
    first.step(&content, Action::EndTurn).unwrap();
    second.step(&content, Action::EndTurn).unwrap();
    assert_eq!(first.actions(&content), second.actions(&content));
    assert_eq!(
        canonical_words(&build_observation(&first, &content, layout)),
        canonical_words(&build_observation(&second, &content, layout))
    );
}


#[test]
fn v56_dynamic_event_ids_use_typed_semantics() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let event = |name| {
        content
            .events
            .iter()
            .position(|event| event.id == name)
            .unwrap() as Id
    };
    let phase_has = |observation: &Observation, namespace, value| {
        has(placed(observation, Place::Phase).next().unwrap().1, layout, namespace, value)
    };
    let check = |game: &Game, values: &[(usize, Semantic, u32)]| {
        let observation = build_observation(game, &content, layout);
        for &(option, namespace, value) in values {
            assert!(phase_has(&observation, namespace, value));
            let candidate = (observation.candidates.iter())
                .find(|row| row.action == Action::Event(option))
                .unwrap();
            assert!(candidate.attached.iter().any(|token| {
                token.domain == EVENT_DOMAIN as u8 && has(token, layout, namespace, value)
            }));
        }
        observation
    };
    let mut game = Game::new_character_ascension(&content, 564, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();

    let colorful = event("EVENT.COLORFUL_PHILOSOPHERS");
    game.event_data = [1, 2, 3, 0];
    game.phase = Phase::Event(
        colorful,
        content.events[colorful as usize].options[..3].to_vec(),
    );
    check(&game, &[0, 1, 2].map(|value| (value, Semantic::Character, value as u32)));

    let doll = event("EVENT.DOLL_ROOM");
    game.event_data = [0, 1, 2, 0];
    game.phase = Phase::Event(
        doll,
        (10..=12)
            .map(|action| EventOption {
                requirement: Requirement::Always,
                effects: Box::leak(Box::new([RunEffect::EventAction(action)])),
            })
            .collect(),
    );
    check(&game, &[0, 1, 2].map(|value| (value, Semantic::Relic, value as u32)));

    let tinker = event("EVENT.TINKER_TIME");
    let options = vec![
        EventOption {
            requirement: Requirement::Always,
            effects: &[],
        };
        2
    ];
    game.event_data = [1, 3, 0, -1];
    game.phase = Phase::Event(tinker, options.clone());
    check(&game, &[(0, Semantic::CardType, 0), (1, Semantic::CardType, 2)]);
    game.event_data = [4, 6, 0, 2];
    game.phase = Phase::Event(tinker, options);
    let observation = check(&game, &[(0, Semantic::TinkerRider, 3), (1, Semantic::TinkerRider, 5)]);
    assert!(phase_has(&observation, Semantic::CardType, 1));

    let conveyor = event("EVENT.ENDLESS_CONVEYOR");
    game.event_data = [7, 2, 3, 0];
    game.phase = Phase::Event(
        conveyor,
        content.events[conveyor as usize].options[..2].to_vec(),
    );
    let observation = build_observation(&game, &content, layout);
    for dish in [6, 2] {
        assert!(phase_has(&observation, Semantic::ConveyorDish, dish));
    }
}

#[test]
fn relic_counters_are_numeric_inputs_of_any_size() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 555, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    (game.pollinous_core, game.pumpkin_candle) = (25, 300);
    let relics = [("RELIC.POLLINOUS_CORE", 25), ("RELIC.PUMPKIN_CANDLE", 300)]
        .map(|(name, counter)| (content.relic_id(name).unwrap() as u32, counter));
    game.run.relics.extend(relics.map(|(id, _)| id as Id));
    let observation = build_observation(&game, &content, layout);
    for (id, counter) in relics {
        let token = (placed(&observation, Place::Relic))
            .find(|(_, token)| has(token, layout, Semantic::Relic, id))
            .unwrap()
            .1;
        assert_eq!(amount_of(token, layout, Semantic::Relic, id), Some(counter as f32 / 10.0));
        assert_eq!(token.f, [0.0; 3]);
    }
}

#[test]
fn wax_relics_encode_queue_progress_and_melted_state() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 555, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let id = content.relic_id("RELIC.HAPPY_FLOWER").unwrap();
    game.run.relics.push(id);
    let index = game.run.relics.len() - 1;
    game.wax_relics.push(index);
    let wax = |game: &Game| {
        let observation = build_observation(game, &content, layout);
        let token = (placed(&observation, Place::Relic))
            .find(|(_, token)| has(token, layout, Semantic::Relic, id as u32))
            .unwrap()
            .1;
        (token.f[1], token.f[2])
    };
    assert_eq!(wax(&game), (1.0, 0.75));
    game.toy_box_combats = 3;
    game.melted_relics.push(index);
    assert_eq!(wax(&game), (1.0, 1.0));
}

#[test]
fn v55_action_and_phase_discriminants_are_exhaustive() {
    let actions = [
        Action::Play {
            hand: 0,
            target: None,
        },
        Action::Potion {
            slot: 0,
            target: None,
        },
        Action::DiscardPotion(0),
        Action::Choose(0),
        Action::EndTurn,
        Action::Path(0),
        Action::RewardGold,
        Action::RewardCard(0),
        Action::RewardRelic(0),
        Action::RewardPotion(0),
        Action::RewardRemove,
        Action::RerollCards,
        Action::SacrificeCards,
        Action::Buy(0),
        Action::Rest,
        Action::Hatch,
        Action::Lift,
        Action::Cook,
        Action::Kindle,
        Action::Dig,
        Action::Smith(0),
        Action::Event(0),
        Action::CrystalCell(0, 0),
        Action::CrystalTool(false),
        Action::EventRelic(0, 0),
        Action::EventCard(0, Card::default()),
        Action::Enchant(0),
        Action::RemoveCard(0),
        Action::Clone,
        Action::Cancel,
        Action::Done,
        Action::Leave,
    ];
    assert_eq!(
        actions.iter().map(action_kind).collect::<Vec<_>>(),
        (0..labels::ACTIONS.len()).collect::<Vec<_>>()
    );

    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 556, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let combat = game.phase.clone();
    let phases = vec![
        Phase::Map,
        combat,
        Phase::Rewards(Rewards {
            gold: 0,
            cards: vec![],
            card_rewards: vec![],
            relics: vec![],
            potions: vec![],
            removals: 0,
        }),
        Phase::Shop(vec![]),
        Phase::Rest,
        Phase::Event(0, vec![]),
        Phase::RemoveCards(1, 0, false),
        Phase::UpgradeCards(1, false),
        Phase::TransformCards(None, 1, false),
        Phase::EnchantCards(Enchantment::Adroit, 1, 1, None, false),
        Phase::ChooseCards(vec![], 1, false),
        Phase::ChooseBundles(vec![]),
        Phase::Won,
        Phase::Dead,
    ];
    assert_eq!(
        phases.iter().map(phase_index).collect::<Vec<_>>(),
        (0..PHASES)
            .map(Some)
            .chain([None, None])
            .collect::<Vec<_>>()
    );
}


#[test]
fn v56_layout_uses_explicit_token_dimensions_and_positions() {
    let layout = Layout::new(&foundation_content());
    assert_eq!((VERSION, VALUE_MODEL_VERSION), (64, 80));
    assert_eq!(
        (
            DEFAULT_MODEL_WIDTH,
            DEFAULT_MODEL_LAYERS,
            DEFAULT_MODEL_HEADS,
            DEFAULT_MODEL_FEEDFORWARD,
        ),
        (128, 4, 8, 384)
    );
    assert!(valid_model_shape(64, 2, 4, 128));
    assert!(!valid_model_shape(65, 2, 4, 128));
    for (semantic, size) in [
        (Semantic::EnemyPosition, 33),
        (Semantic::PowerPosition, 65),
        (Semantic::OrbPosition, 17),
        (Semantic::DeckOrigin, 257),
        (Semantic::DrawTopPosition, 257),
        (Semantic::DrawBottomPosition, 257),
        (Semantic::RelicPosition, 257),
        (Semantic::ContinuationPosition, 257),
        (Semantic::CrystalRow, 11),
        (Semantic::CrystalColumn, 11),
        (Semantic::CrystalWidth, 4),
        (Semantic::CrystalHeight, 4),
    ] {
        assert_eq!(layout.semantic_sizes[semantic as usize], size);
    }
}

#[test]
fn v56_event_pool_contains_only_eligible_unvisited_events() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 45, 0, 10).unwrap();
    game.begin_act(&content, 1).unwrap();
    let eligible = content.acts[game.act as usize]
        .events
        .iter()
        .copied()
        .filter(|&id| game.event_allowed(&content, id))
        .collect::<Vec<_>>();
    let visited = eligible[0];
    game.visited_events.push(visited);
    let observation = build_observation(&game, &content, layout);
    let actual = (placed(&observation, Place::EventPool))
        .map(|(_, token)| (token.c[0] - layout.semantic(Semantic::Event, 0)) as Id)
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), eligible.len() - 1);
    assert!(!actual.contains(&visited));
    assert!(actual.iter().all(|&id| game.event_allowed(&content, id)));
}

#[test]
fn v57_actions_are_legal_only_and_only_suspended_offers_are_phase_children() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 46, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.run.gold = 0;
    game.run.potions.fill(None);
    let card = Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        ..Card::default()
    };
    let actions = |observation: &Observation| {
        (observation.candidates.iter()).map(|row| row.action.clone()).collect::<Vec<_>>()
    };
    let phase_children = |observation: &Observation, phase: usize| {
        let phase = placed(observation, Place::Phase).nth(phase).unwrap().0 as u32;
        (observation.tokens.iter()).filter(|token| token.parent == phase).cloned().collect::<Vec<_>>()
    };
    game.phase = Phase::Shop(vec![ShopItem::Card(card, 100)]);
    let observation = build_observation(&game, &content, layout);
    assert_eq!(actions(&observation), [Action::Leave]);
    assert!(phase_children(&observation, 0).is_empty());

    game.resume = Some(Phase::Shop(vec![ShopItem::Card(card, 100)]));
    game.phase = Phase::RemoveCards(1, 0, true);
    let observation = build_observation(&game, &content, layout);
    assert_eq!(placed(&observation, Place::Phase).count(), 2);
    let children = phase_children(&observation, 1);
    assert!(children.iter().any(|token| token.domain == OFFER_DOMAIN as u8
        && has(token, layout, Semantic::OfferKind, 4)
        && token.f[0] == 0.1));
    assert!(children.iter().any(|token| token.domain == CARD_DOMAIN as u8
        && has(token, layout, Semantic::Card, base_card_id(card.id as u32))));
    let position = layout.semantic(Semantic::ContinuationPosition, 0);
    let positions = position..position + layout.semantic_sizes[Semantic::ContinuationPosition as usize];
    assert!(children.iter().all(|token| token.place == Place::Child
        && !token.c.iter().any(|code| positions.contains(code))));

    let effects = Box::leak(Box::new([RunEffect::Gold(-999)]));
    game.phase = Phase::Event(
        0,
        vec![
            EventOption {
                requirement: Requirement::Always,
                effects: &[],
            },
            EventOption {
                requirement: Requirement::Gold(999),
                effects,
            },
        ],
    );
    game.resume = None;
    let observation = build_observation(&game, &content, layout);
    assert_eq!(actions(&observation), [Action::Event(0)]);
    assert!(phase_children(&observation, 0).is_empty());
}

#[test]
fn v56_card_versions_origins_and_enemy_targets_are_explicit() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let strike = content.card_id("CARD.STRIKE_IRONCLAD").unwrap();
    assert_ne!(
        card_version_id(&content, strike, false, 0),
        card_version_id(&content, strike, true, 0)
    );
    let mad = content.card_id("CARD.MAD_SCIENCE").unwrap();
    assert_ne!(
        card_version_id(&content, mad, false, 1),
        card_version_id(&content, mad, false, 2)
    );

    let mut game = Game::new_character_ascension(&content, 84, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.hand = vec![Card {
        id: strike,
        instance: game.run.deck[0].instance,
        ..Card::default()
    }];
    combat.energy = 99;
    let enemy = combat.enemies[0].creature.id as u32;
    let observation = build_observation(&game, &content, layout);
    let hand = placed(&observation, Place::Hand).next().unwrap().1;
    assert!(has(hand, layout, Semantic::DeckOrigin, 0));
    let action = (observation.candidates.iter())
        .find(|row| row.action == Action::Play { hand: 0, target: Some(0) })
        .unwrap();
    let actor = placed(&observation, Place::Enemy).next().unwrap().1;
    for (namespace, value) in [(Semantic::Enemy, enemy), (Semantic::EnemyPosition, 0)] {
        assert!(has(&action.token, layout, namespace, value) && has(actor, layout, namespace, value));
    }
}

#[test]
fn v56_continuations_are_flat_execution_items() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 91, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let repeated: &'static [Effect] = Box::leak(Box::new([Effect::Draw(1)]));
    let yes: &'static [Effect] = Box::leak(Box::new([Effect::Block(
        Target::Player,
        Amount::fixed(3, 3),
    )]));
    let no: &'static [Effect] = Box::leak(Box::new([Effect::Damage(
        Target::Player,
        Amount::fixed(99, 99),
    )]));
    let random: &'static [Effect] = Box::leak(Box::new([
        Effect::If(Condition::Always, yes, no),
        Effect::Repeat(Amount::fixed(2, 2), repeated),
    ]));
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.queue = vec![
        Pending {
            effect: Effect::Repeat(Amount::fixed(2, 2), repeated),
            context: Context::player(),
        },
        Pending {
            effect: Effect::If(Condition::Always, yes, no),
            context: Context::player(),
        },
        Pending {
            effect: Effect::Random(1, random),
            context: Context::player(),
        },
    ];
    let tokens = queue_tokens(&game, &content);
    let heads = (tokens.iter()).filter(|token| token.place == Place::Continuation).collect::<Vec<_>>();
    assert_eq!(heads.len(), 4);
    for (order, head) in heads.iter().enumerate() {
        assert!(has(head, layout, Semantic::ContinuationPosition, order as u32));
    }
    assert!(tokens.iter().all(|token| (token.place == Place::Continuation) == (token.parent == NONE)));
    for effect in [random[0], random[1]] {
        let code = layout.semantic(Semantic::Effect, effect_index(&effect) as u32);
        assert!(tokens.iter().all(|token| !token.c.contains(&code)));
    }
}

#[test]
fn v56_continuation_amounts_use_declared_scales_and_current_values() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 92, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let amount = Amount {
        base: 3,
        upgraded: 5,
        ascension: 0,
        scale: Scale::X,
        multiplier: 2,
        divisor: 1,
    };
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.queue.push(Pending {
        effect: Effect::Damage(Target::Player, amount),
        context: Context {
            upgraded: true,
            x: 4,
            ..Context::player()
        },
    });
    let tokens = queue_tokens(&game, &content);
    let damage = effect_index(&Effect::Damage(Target::Player, amount)) as u32;
    let effect = (tokens.iter())
        .find(|token| has(token, layout, Semantic::Effect, damage))
        .unwrap();
    assert_eq!(amount_of(effect, layout, Semantic::Effect, damage), Some(signed_log(13.0 / 30.0)));
    assert_eq!(effect.f[..4], [3.0f32 / 30.0, 5.0 / 30.0, 0.2, 0.1].map(signed_log));
    assert_eq!(effect.f[12], 1.0);
    let context = (tokens.iter()).find(|token| token.domain == CONTEXT_DOMAIN as u8).unwrap();
    assert_eq!(context.f, [0.8, 0.0]);
}

/// Asserts the token graph is well formed and every number bounded; tracks the largest
/// magnitude of each domain's fields and amounts.
fn check_observation(observation: &Observation, layout: Layout, fields: &mut [Vec<f32>], amounts: &mut [f32]) {
    let tokens = &observation.tokens;
    let nodes = tokens.iter().take_while(|token| token.place == Place::MapNode).count() as u32;
    let actor = |index: u32| matches!(tokens[index as usize].place, Place::Player | Place::Ally | Place::Enemy);
    for (index, token) in tokens.iter().enumerate() {
        let earlier = token.parent < index as u32;
        match token.place {
            Place::Child => assert!(earlier && tokens[token.parent as usize].place != Place::Child),
            Place::Effect | Place::Move => assert!(earlier && actor(token.parent)),
            Place::MapEdge => assert!(token.parent < nodes && token.link < nodes),
            _ => assert_eq!(token.parent, NONE, "{:?} token {index} has a parent", token.place),
        }
        match token.place {
            Place::Run => assert!(token.link < nodes),
            Place::MapEdge => {}
            _ => assert_eq!(token.link, NONE),
        }
    }
    for candidate in &observation.candidates {
        let link = candidate.token.link;
        assert_eq!((candidate.token.place, candidate.token.parent), (Place::Candidate, NONE));
        match candidate.action {
            Action::Path(_) => assert!(link < nodes),
            Action::Play { target: Some(_), .. } | Action::Potion { target: Some(_), .. } => {
                assert_eq!(tokens[link as usize].place, Place::Enemy)
            }
            _ => assert_eq!(link, NONE),
        }
        for token in &candidate.attached {
            assert_eq!((token.place, token.parent, token.link), (Place::Child, NONE, NONE));
        }
    }
    let candidates = (observation.candidates.iter())
        .flat_map(|candidate| std::iter::once(&candidate.token).chain(&candidate.attached));
    for token in tokens.iter().chain(candidates) {
        let (name, _, names) = DOMAINS[token.domain as usize];
        assert!(token.c.iter().all(|&code| (code as usize) < layout.concept_vocab()));
        // Linear scales are capped or sized for ±5; logarithmic ones reach ln(1 + 999) ≈ 6.9.
        for (slot, &value) in token.f.iter().enumerate() {
            assert!(value.is_finite() && value.abs() <= 7.0, "{name} {}: {value} {token:?}", names[slot]);
            let max = &mut fields[token.domain as usize][slot];
            *max = max.max(value.abs());
        }
        for &value in &token.a {
            assert!(value.is_finite() && value.abs() <= 7.0, "{name} amount: {value}");
            let max = &mut amounts[token.domain as usize];
            *max = max.max(value.abs());
        }
    }
    #[cfg(feature = "python")]
    {
        let data = python::compact_data(&python::compact_packed_observation(observation)).unwrap();
        assert_eq!(
            (data.character, data.counts, data.digest),
            (observation.character, observation_counts(observation), observation_digest(observation))
        );
        assert_eq!(data.globals, observation.globals);
        assert_eq!(data.words, observation_words(observation));
        let (parsed, legal) = python::parse_row(&data).unwrap();
        let local = |value: u32| if value == NONE { -1 } else { value as i64 };
        let mut expected = (tokens.iter())
            .map(|token| (token.domain, token.place as u8, local(token.parent), local(token.link)))
            .collect::<Vec<_>>();
        for candidate in &observation.candidates {
            let index = expected.len() as i64;
            let token = &candidate.token;
            expected.push((token.domain, token.place as u8, -1, local(token.link)));
            expected.extend(
                (candidate.attached.iter())
                    .map(|token| (token.domain, token.place as u8, index, local(token.link))),
            );
        }
        let parsed = parsed.iter().map(|token| (token.domain, token.place, token.parent, token.link));
        assert_eq!(parsed.collect::<Vec<_>>(), expected);
        assert_eq!(legal, observation.candidates.iter().map(|row| row.legal).collect::<Vec<_>>());
    }
}

#[test]
fn random_walk_observations_are_well_formed() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut fields = DOMAINS.map(|(_, _, names)| vec![0.0f32; names.len()]);
    let mut amounts = [0.0f32; DOMAINS.len()];
    let (mut observations, mut floors) = (0, 0);
    for seed in 0..2 {
        for character in 0..content.characters.len() {
            let mut game = Game::new_character_ascension(&content, 800 + seed, character as Id, 0).unwrap();
            game.begin_run(&content).unwrap();
            let mut rng = Rng::from_seed(seed * 16 + character as u64);
            for _ in 0..3000 {
                if matches!(game.phase, Phase::Won | Phase::Dead) {
                    break;
                }
                // Random play would die in the first act.
                game.run.hp = game.run.max_hp;
                if let Phase::Combat(combat) = &mut game.phase {
                    combat.player.hp = combat.player.max_hp;
                }
                // Removed relics encode the bag only if it never gains a relic outside the starting pool.
                let pool = starting_relic_pool(&game, &content);
                assert!(public_relic_bag(&game, layout).concat().iter().all(|id| pool.contains(id)));
                check_observation(&build_observation(&game, &content, layout), layout, &mut fields, &mut amounts);
                observations += 1;
                let mut actions = game.actions(&content);
                if actions.len() > 1 && rng.next() % 4 != 0 {
                    actions.retain(|action| *action != Action::EndTurn);
                }
                let action = actions[rng.next() as usize % actions.len()].clone();
                game.step(&content, action).unwrap();
            }
            floors += canonical_progress(&game) as usize;
        }
    }
    eprintln!("{observations} observations, {floors} floors");
    for (((name, _, names), fields), amount) in DOMAINS.iter().zip(&fields).zip(amounts) {
        let field = (fields.iter().zip(names.iter()))
            .max_by(|left, right| left.0.total_cmp(right.0))
            .map_or(String::new(), |(value, name)| format!(" max |field| {value:.3} ({name})"));
        eprintln!("{name}: max |amount| {amount:.3}{field}");
    }
}

#[test]
fn traced_state_observations_are_stable() {
    // 200 captured states, 40 per character; skipped without the traces.
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut digest = 0u64;
    let mut states = 0;
    for name in [
        "0001-SILENT",
        "0002-IRONCLAD",
        "0003-REGENT",
        "0004-DEFECT",
        "0005-NECROBINDER",
    ] {
        let path = format!("target/oracle-policy-samples/hunt-r062/{name}.ndjson.zst");
        let Ok(output) = std::process::Command::new("zstd")
            .args(["-dcq", &path])
            .output()
        else {
            return;
        };
        if !output.status.success() {
            return;
        }
        let records = String::from_utf8(output.stdout).unwrap();
        for record in records
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|record| record["kind"] == "decision")
            .step_by(15)
            .take(40)
        {
            let game = crate::replay::game_from_live_snapshot(
                &content,
                &record["before"],
                &record["oracle_before"],
            )
            .unwrap();
            let observation = build_observation(&game, &content, layout);
            digest = digest.rotate_left(5) ^ observation_digest(&observation);
            states += 1;
        }
    }
    assert_eq!(states, 200);
    assert_eq!(digest, 4607526811213350950);
}
