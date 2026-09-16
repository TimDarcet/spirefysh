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
    let preview = played_card_preview(&game, &content, card, Some(0));
    let mut played = game.clone();
    played.step(&content, action).unwrap();
    assert_eq!(preview.block, 10);
    assert_eq!(played.combat().unwrap().player.block, preview.block);
    assert_eq!(played.galactic_dust, 1);
}

#[test]
fn v33_hand_moves_are_counted_as_entering_hand() {
    let mut content = foundation_content();
    let all_for_one = content.card_id("CARD.ALL_FOR_ONE").unwrap();
    let hologram = content.card_id("CARD.HOLOGRAM").unwrap();
    let custom = content.card_id("CARD.ZAP").unwrap();
    content.cards[custom as usize].effects = Box::leak(Box::new([Effect::SelectAmount(
        Pile::Discard,
        CardFilter::Any,
        Amount::fixed(2, 2),
        false,
        CardOp::Move(Pile::Hand),
    )]));
    let claw = Card {
        id: content.card_id("CARD.CLAW").unwrap(),
        ..Card::default()
    };
    let strike = Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        ..Card::default()
    };
    for (seed, card, discard, expected) in [
        (61, all_for_one, vec![claw, claw, strike], 2),
        (62, hologram, vec![strike], 1),
        (63, custom, vec![strike, strike], 2),
    ] {
        let mut game = Game::new_character_ascension(&content, seed, 3, 10).unwrap();
        game.begin_act(&content, 0).unwrap();
        game.start_combat(&content, content.acts[0].encounters[0])
            .unwrap();
        let Phase::Combat(combat) = &mut game.phase else {
            unreachable!()
        };
        combat.hand = vec![Card {
            id: card,
            ..Card::default()
        }];
        combat.draw.clear();
        combat.discard = discard;
        combat.energy = 3;
        combat.player.powers.clear();
        combat.enemies[0].creature.hp = 100;
        combat.enemies[0].creature.max_hp = 100;
        combat.enemies[0].creature.powers.clear();
        let played_card = combat.hand[0];
        let target = (card == all_for_one).then_some(0);
        let action = Action::Play { hand: 0, target };
        let preview = played_card_preview(&game, &content, played_card, target);
        assert_eq!(preview.draw, expected);
        let mut played = game.clone();
        played.step(&content, action).unwrap();
        assert_eq!(played.combat().unwrap().hand.len(), expected as usize);
    }
}

#[test]
fn recycling_visible_hand_marks_the_draw_bottom_known() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 10, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let bottom = [
        Card::default(),
        Card {
            id: 1,
            ..Card::default()
        },
    ];
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.hand = vec![
        Card {
            id: content.card_id("CARD.REBOOT").unwrap(),
            ..Card::default()
        },
        bottom[0],
        bottom[1],
    ];
    combat.draw = (2..10)
        .map(|id| Card {
            id,
            ..Card::default()
        })
        .collect();
    combat.known_draw_top = 0;
    combat.known_draw_bottom = 0;
    game.step(
        &content,
        Action::Play {
            hand: 0,
            target: None,
        },
    )
    .unwrap();
    let combat = game.combat().unwrap();
    assert_eq!(combat.known_draw_bottom, 2);
    assert_eq!(&combat.draw[..2], &bottom);
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
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
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
fn potential_has_a_common_terminal_value() {
    let content = foundation_content();
    let mut game = Game::new_character_ascension(&content, 12, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.phase = Phase::Won;
    assert_eq!(potential(&game, &content), Potential::default());
    game.phase = Phase::Dead;
    assert_eq!(potential(&game, &content), Potential::default());
}

#[test]
fn potential_is_floor_plus_hp_weighted_run_resources() {
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
    let value = potential(&game, &content);
    assert_eq!(
        value.floor,
        canonical_progress(&game) as f32 / (TERMINAL_CATEGORIES - 1) as f32
    );
    assert_eq!(value.resources.iter().sum::<f32>(), 1547.0);
    assert_eq!(
        potential_value(&value, &[1.0; POTENTIAL_WEIGHT_COUNT]),
        value.floor + 1547.0
    );
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
fn mixed_active_metadata_has_fixed_width() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let game = Game::new_character_ascension(&content, 40, 0, 10).unwrap();
    let rows = [true, false].map(|active| observation_globals(&game, &content, layout, active));
    assert_eq!(PUBLIC_GLOBALS, 0);
    assert_eq!(globals_len(layout), PUBLIC_GLOBALS);
    assert!(rows.into_iter().all(|row| row.is_empty()));
}

#[test]
fn v62_packed_observation_has_no_direct_globals_and_is_digest_protected() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 600, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let observation = observation_v56(&game, &content, layout, (3, 4));
    let (globals, counts, mut exact, actions, digest) = packed_observation(&observation);
    assert_eq!(observation_digest(&observation), digest);
    assert_eq!(globals, observation.globals);
    assert_eq!(globals.len(), PUBLIC_GLOBALS);
    assert!(globals.iter().all(|value| value.is_finite()));
    assert_eq!(
        packed_observation_digest(observation.character, &globals, &counts, &exact, &actions),
        digest
    );
    exact[0] ^= 1;
    assert_ne!(
        packed_observation_digest(observation.character, &globals, &counts, &exact, &actions),
        digest
    );
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
    let lightning = content
        .orbs
        .iter()
        .position(|orb| orb.id == "ORB.LIGHTNING_ORB")
        .unwrap() as Id;
    let dark = content
        .orbs
        .iter()
        .position(|orb| orb.id == "ORB.DARK_ORB")
        .unwrap() as Id;
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
    let preview = played_card_preview(&game, &content, dualcast, None);
    assert_eq!(
        resolved_hits(&game, &content, 0, None, false, true, &preview.hits).2,
        16
    );
    assert!(preview.orbs.is_empty());
    let mut played = game.clone();
    played
        .step(
            &content,
            Action::Play {
                hand: 0,
                target: None,
            },
        )
        .unwrap();
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
        let preview = played_card_preview(&game, &content, zap, None);
        assert_eq!(
            resolved_hits(&game, &content, 0, None, false, false, &preview.hits).2,
            expected
        );
        assert_eq!((preview.orb_slots, preview.orbs.len()), (1, 1));
        let mut played = game.clone();
        played
            .step(
                &content,
                Action::Play {
                    hand: 0,
                    target: None,
                },
            )
            .unwrap();
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
        skip_next_decay: false,
        value: 0,
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
        skip_next_decay: false,
        value: 0,
    })
    .to_vec();
    let preview = played_card_preview(&game, &content, fisticuffs, Some(0));
    assert_eq!(preview.block, 12);
    assert_eq!(
        resolved_hits(&game, &content, 0, Some(0), false, false, &preview.hits).2,
        10
    );
    let mut played = game.clone();
    played
        .step(
            &content,
            Action::Play {
                hand: 0,
                target: Some(0),
            },
        )
        .unwrap();
    assert_eq!(played.combat().unwrap().player.block, 12);
    assert_eq!(played.combat().unwrap().enemies[0].creature.hp, 90);

    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.player.powers.push(Power {
        id: power_id::NO_BLOCK,
        amount: 1,
        skip_next_decay: false,
        value: 0,
    });
    assert_eq!(
        played_card_preview(&game, &content, fisticuffs, Some(0)).block,
        0
    );

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
    let preview = played_card_preview(&game, &content, misery, Some(0));
    let damage = resolved_hits(&game, &content, 0, Some(0), false, false, &preview.hits).2;
    let mut played = game.clone();
    played
        .step(
            &content,
            Action::Play {
                hand: 0,
                target: Some(0),
            },
        )
        .unwrap();
    assert_eq!(
        100 - played.combat().unwrap().enemies[0].creature.hp,
        damage
    );
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
        let preview = played_card_preview(&game, &content, card, None);
        assert_eq!(preview.exhaust, 1);
        let mut played = game.clone();
        played
            .step(
                &content,
                Action::Play {
                    hand: 0,
                    target: None,
                },
            )
            .unwrap();
        assert_eq!(played.combat().unwrap().exhaust.len(), 1);
        assert_eq!(played.combat().unwrap().player.block, preview.block);
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
            skip_next_decay: false,
            value: 0,
        })
        .collect();
    let preview = played_card_preview(&game, &content, card, Some(0));
    assert_eq!((preview.block, preview.draw, preview.exhaust), (5, 3, 1));
    assert_eq!(
        resolved_hits(&game, &content, 0, Some(0), false, true, &preview.hits).2,
        28
    );
    let drawn = game.combat().unwrap().drawn;
    let mut played = game.clone();
    played
        .step(
            &content,
            Action::Play {
                hand: 0,
                target: Some(0),
            },
        )
        .unwrap();
    let combat = played.combat().unwrap();
    assert_eq!(combat.player.block, 5);
    assert_eq!(combat.enemies[0].creature.hp, 72);
    assert_eq!(combat.drawn - drawn, 3);
    assert_eq!(combat.exhaust.len(), 1);
}

fn model_row_bytes(domain: usize, row: &DomainRow) -> Vec<u8> {
    let structural: &[usize] = match domain {
        RUN_DOMAIN => &[23],
        CARD_DOMAIN => &[0],
        ACTOR_DOMAIN | POWER_DOMAIN | HISTORY_DOMAIN => &[0],
        STATUS_DOMAIN => &[0, 1],
        RELIC_DOMAIN | EVENT_DOMAIN | ENCOUNTER_DOMAIN => &[0],
        POTION_DOMAIN => &[1],
        ORB_DOMAIN => &[0],
        CRYSTAL_DOMAIN => &[7],
        CONTINUATION_DOMAIN => &[2, 3, 5],
        MAP_NODE_DOMAIN => &[0, 8, 9],
        MAP_EDGE_DOMAIN => &[0, 1],
        _ => &[],
    };
    let mut out = Vec::new();
    for &index in structural {
        out.extend(row.u[index].to_le_bytes());
    }
    row.c
        .iter()
        .for_each(|value| out.extend(value.to_le_bytes()));
    row.f
        .iter()
        .for_each(|value| out.extend(value.to_bits().to_le_bytes()));
    out
}

fn v56_candidate_bytes(observation: &ObservationV56, index: usize) -> Vec<u8> {
    let row = &observation.candidates[index];
    let mut out = Vec::new();
    out.extend(row.u[1].to_le_bytes());
    out.extend(row.u[4].to_le_bytes());
    row.c
        .iter()
        .for_each(|value| out.extend(value.to_le_bytes()));
    row.f
        .iter()
        .for_each(|value| out.extend(value.to_bits().to_le_bytes()));
    out.push(row.legal as u8);
    for (domain, rows) in observation.domains.iter().enumerate() {
        for row in rows.iter().filter(|row| row.scope == index as i32) {
            out.push(domain as u8);
            out.extend(model_row_bytes(domain, row));
        }
    }
    out
}

fn v56_bytes(observation: &ObservationV56) -> Vec<u8> {
    let mut out = vec![observation.character];
    observation
        .globals
        .iter()
        .for_each(|value| out.extend(value.to_bits().to_le_bytes()));
    for (domain, rows) in observation.domains.iter().enumerate() {
        let rows = rows
            .iter()
            .filter(|row| row.scope == STATE_SCOPE)
            .collect::<Vec<_>>();
        out.extend((rows.len() as u32).to_le_bytes());
        for row in rows {
            out.extend(model_row_bytes(domain, row));
        }
    }
    let mut candidates = (0..observation.candidates.len())
        .map(|index| v56_candidate_bytes(observation, index))
        .collect::<Vec<_>>();
    candidates.sort();
    out.extend((candidates.len() as u32).to_le_bytes());
    for candidate in candidates {
        out.extend((candidate.len() as u32).to_le_bytes());
        out.extend(candidate);
    }
    out
}

#[test]
fn v53_exact_fields_keep_integer_extremes() {
    let mut row = DomainRow::new(CONTINUATION_DOMAIN, STATE_SCOPE);
    row.u[..6].copy_from_slice(&[0, 255, 256, 65_535, 65_536, u32::MAX]);
    row.s[..4].copy_from_slice(&[0, -1, i32::MIN, i32::MAX]);
    let mut changed = row.clone();
    changed.u[2] = 255;
    assert_ne!(row, changed);
    assert_eq!(row.u[5], u32::MAX);
    assert_eq!(row.s[2], i32::MIN);
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
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
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
    let base = v56_bytes(&observation_v56(&game, &content, layout, (0, 0)));
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
            v56_bytes(&observation_v56(&changed, &content, layout, (0, 0))),
            base,
            "public mutation {mutation} was not observed"
        );
    }
}

#[test]
fn v53_combat_choice_keeps_typed_filter_and_operation_fields() {
    let content = foundation_content();
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
    combat.ending = true;
    combat.force_end = true;
    let row = phase_domain_row(&game, &content);
    assert_eq!(
        &row.u[5..16],
        &[
            Pile::Draw as u32,
            3,
            CardType::Attack as u32,
            7,
            target as u32,
            7,
            2,
            1,
            1,
            1,
            1,
        ]
    );
    assert_eq!(&row.s[..2], &[0x8123, 0]);
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
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
    );
    let mut changed = first.clone();
    let node = std::sync::Arc::make_mut(&mut changed.map.nodes)
        .iter_mut()
        .find(|node| node.next.len() > 1)
        .unwrap();
    node.next.pop();
    assert_ne!(
        observation_v56(&first, &content, layout, (0, 0)).domains[MAP_EDGE_DOMAIN],
        observation_v56(&changed, &content, layout, (0, 0)).domains[MAP_EDGE_DOMAIN]
    );
}

#[test]
fn v53_sparse_map_matches_an_independent_scalar_dag() {
    fn node_value(row: &DomainRow) -> i64 {
        row.u[1..8]
            .iter()
            .enumerate()
            .map(|(index, &value)| (index as i64 + 2) * value as i64)
            .sum()
    }
    fn edge_value(row: &DomainRow) -> i64 {
        row.u[2] as i64 * 37 + row.u[3..].iter().map(|&value| value as i64).sum::<i64>()
    }
    fn recursive(
        id: u32,
        nodes: &std::collections::BTreeMap<u32, &DomainRow>,
        edges: &[&DomainRow],
        memo: &mut std::collections::BTreeMap<u32, i64>,
    ) -> i64 {
        if let Some(&value) = memo.get(&id) {
            return value;
        }
        let value = node_value(nodes[&id])
            + edges
                .iter()
                .filter(|edge| edge.u[0] == id)
                .map(|edge| edge_value(edge) + recursive(edge.u[1], nodes, edges, memo))
                .sum::<i64>();
        memo.insert(id, value);
        value
    }

    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 408, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let observation = observation_v56(&game, &content, layout, (0, 0));
    let nodes = observation.domains[MAP_NODE_DOMAIN]
        .iter()
        .map(|row| (row.u[0], row))
        .collect::<std::collections::BTreeMap<_, _>>();
    let edges = observation.domains[MAP_EDGE_DOMAIN]
        .iter()
        .collect::<Vec<_>>();
    let mut batched = nodes
        .iter()
        .map(|(&id, row)| (id, node_value(row)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut levels = nodes.values().map(|row| row.u[8]).collect::<Vec<_>>();
    levels.sort_unstable();
    levels.dedup();
    for level in levels.into_iter().rev() {
        for row in nodes.values().filter(|row| row.u[8] == level) {
            batched.insert(
                row.u[0],
                node_value(row)
                    + edges
                        .iter()
                        .filter(|edge| edge.u[0] == row.u[0])
                        .map(|edge| edge_value(edge) + batched[&edge.u[1]])
                        .sum::<i64>(),
            );
        }
    }
    let mut reference = std::collections::BTreeMap::new();
    for &id in nodes.keys() {
        recursive(id, &nodes, &edges, &mut reference);
    }
    assert_eq!(batched, reference);
    for candidate in observation
        .candidates
        .iter()
        .filter(|row| matches!(row.action, Action::Path(_)))
    {
        assert_eq!(batched[&candidate.u[4]], reference[&candidate.u[4]]);
    }
    let mut changed = observation.domains[MAP_EDGE_DOMAIN].to_vec();
    changed[0].u[2] += 1;
    let changed = changed.iter().collect::<Vec<_>>();
    assert_ne!(
        recursive(0, &nodes, &edges, &mut std::collections::BTreeMap::new()),
        recursive(0, &nodes, &changed, &mut std::collections::BTreeMap::new())
    );
}

#[test]
fn v53_path_candidates_reuse_canonical_destination_ids() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 405, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let observation = observation_v56(&game, &content, layout, (0, 0));
    assert_eq!(observation.domains[RUN_DOMAIN][0].u[23], 0);
    let nodes = observation.domains[MAP_NODE_DOMAIN]
        .iter()
        .map(|row| row.u[0])
        .collect::<std::collections::BTreeSet<_>>();
    for candidate in observation
        .candidates
        .iter()
        .filter(|row| matches!(row.action, Action::Path(_)))
    {
        assert!(nodes.contains(&candidate.u[4]));
    }
    assert!(
        observation
            .candidates
            .iter()
            .filter(|row| !matches!(row.action, Action::Path(_)))
            .all(|row| row.u[4] == NO_NODE)
    );
    let path = observation
        .candidates
        .iter()
        .find_map(|row| match row.action {
            Action::Path(index) => Some((index, row.u[4])),
            _ => None,
        })
        .unwrap();
    game.map.current = Some(path.0);
    let entered = observation_v56(&game, &content, layout, (0, 0));
    assert_eq!(entered.domains[RUN_DOMAIN][0].u[23], path.1);
    assert_ne!(path.1, 0);

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
    let observation = observation_v56(&game, &content, layout, (0, 0));
    let paths = observation
        .candidates
        .iter()
        .filter(|row| matches!(row.action, Action::Path(_)))
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|row| row.u[5] == 1));
    assert!(paths.iter().any(|row| row.u[5] == 2));
    assert!(paths.iter().all(|row| row.u[14] == row.u[4]));
    assert_eq!(
        paths
            .iter()
            .map(|row| row.u[14])
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        paths.len()
    );
}

#[test]
fn v54_continuation_parent_and_path_associations_do_not_alias() {
    fn tree(swapped: bool) -> Vec<DomainRow> {
        let mut tree = ContinuationBuilder::new(STATE_SCOPE);
        tree.row(9, 0, NO_NODE, 0, 0, 0, 0, 2);
        tree.row(1, 10, 0, 1, 1, 0, 0, 1);
        tree.row(1, 11, 0, 2, 1, 0, 1, 1);
        tree.row(1, 20, if swapped { 2 } else { 1 }, 1, 2, 0, 0, 0);
        tree.row(1, 21, if swapped { 1 } else { 2 }, 2, 2, 0, 0, 0);
        tree.rows
    }
    fn without_structure(mut rows: Vec<DomainRow>) -> Vec<DomainRow> {
        for row in &mut rows {
            for field in [2, 3, 5] {
                row.u[field] = 0;
            }
        }
        rows
    }

    let first = tree(false);
    let second = tree(true);
    assert_eq!(
        without_structure(first.clone()),
        without_structure(second.clone())
    );
    assert_ne!(first, second);
    let mut different_paths = first.clone();
    different_paths[3].u[5] = 3;
    different_paths[4].u[5] = 1;
    assert_eq!(
        without_structure(first.clone()),
        without_structure(different_paths.clone())
    );
    assert_ne!(first, different_paths);
    assert!(
        first
            .iter()
            .all(|row| row.f[16..22].iter().all(|value| value.is_finite()))
    );
}

#[test]
fn v56_categorical_embeddings_sum_without_field_weights() {
    let encoder = TokenEncoderWeights {
        numeric: LinearWeights {
            w: vec![],
            b: vec![0.0; 2],
        },
        norm_w: vec![1.0; 2],
        norm_b: vec![0.0; 2],
    };
    let table = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    assert_eq!(
        encoder.encode(&[1, 2], &[], &table, 2),
        encoder.encode(&[2, 1], &[], &table, 2)
    );
}

#[test]
fn v55_production_populates_flag_heavy_cards_once() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 549, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    let card = Card {
        id: content.card_id("CARD.STRIKE_IRONCLAD").unwrap(),
        flags: u16::MAX,
        turn_flags: u16::MAX,
        ..Card::default()
    };
    game.run.deck.push(card);
    let observation = observation_v56(&game, &content, layout, (0, 0));
    let actual = observation.domains[CARD_DOMAIN]
        .iter()
        .find(|row| row.u[4] == card.id as u32 && row.u[6] == u16::MAX as u32)
        .unwrap();
    let mut encoding = CardEncoding::new(&game, &content);
    let mut expected = card_domain_row(
        &game,
        &content,
        &mut encoding,
        STATE_SCOPE,
        0,
        0,
        0,
        0,
        card,
        0,
    );
    populate_domain_features(&game, &content, layout, CARD_DOMAIN, &mut expected);
    assert_eq!(
        (actual.c.clone(), actual.f.clone()),
        (expected.c, expected.f)
    );
}

#[test]
fn v56_continuation_booleans_and_discard_counts_do_not_alias() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let game = Game::new_character_ascension(&content, 565, 0, 10).unwrap();
    let effect = |effect| {
        let mut tree = ContinuationBuilder::for_game(&game, STATE_SCOPE);
        tree.effect(effect, NO_NODE, 0, 0, 0, 0, None);
        let mut row = tree.rows.remove(0);
        populate_domain_features(&game, &content, layout, CONTINUATION_DOMAIN, &mut row);
        (row.c, row.f)
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

    let run_effect = |effect| {
        let mut tree = ContinuationBuilder::for_game(&game, STATE_SCOPE);
        tree.run_effect(effect, &content, NO_NODE, 0, 0, 0, 0);
        let mut row = tree.rows.remove(0);
        populate_domain_features(&game, &content, layout, CONTINUATION_DOMAIN, &mut row);
        row.c
    };
    assert_ne!(
        run_effect(RunEffect::RandomPotionReward(false)),
        run_effect(RunEffect::RandomPotionReward(true))
    );
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
    let observation = observation_v56(&game, &content, layout, (0, 0));
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
    combat.enemy_power_snapshot.resize(2, Vec::new());
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
    let observation = observation_v56(&game, &content, layout, (0, 0));
    let targets = [0, 1].map(|target| {
            observation
                .candidates
                .iter()
                .position(|row| matches!(row.action, Action::Play { hand: 0, target: Some(value) } if value == target))
                .unwrap()
        });
    assert_ne!(
        observation.candidates[targets[0]].f,
        observation.candidates[targets[1]].f
    );
    assert_ne!(
        observation.candidates[targets[0]].u,
        observation.candidates[targets[1]].u
    );
    assert_eq!(
        observation.candidates[targets[0]].u[14],
        observation.candidates[targets[1]].u[14]
    );
    assert_eq!(
        ValueModel::candidate_object_key(&observation, targets[0]),
        ValueModel::candidate_object_key(&observation, targets[1])
    );
}

#[test]
fn v56_action_auxiliaries_rederive_from_exact_sources() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let mut game = Game::new_character_ascension(&content, 566, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();
    game.start_combat(&content, content.acts[0].encounters[0])
        .unwrap();
    let observation = observation_v56(&game, &content, layout, (0, 0));
    for candidate in &observation.candidates {
        assert_eq!(candidate.f, action_features(&candidate.u, &candidate.s));
    }
    let (globals, counts, mut exact, mut actions, digest) = packed_observation(&observation);
    assert_eq!(
        packed_observation_digest(observation.character, &globals, &counts, &exact, &actions,),
        digest
    );
    exact[0] ^= 1;
    assert_ne!(
        packed_observation_digest(observation.character, &globals, &counts, &exact, &actions,),
        digest
    );
    exact[0] ^= 1;
    let numeric = ACTION_U + ACTION_S + ACTION_C;
    actions[numeric] ^= 1;
    assert_ne!(
        packed_observation_digest(observation.character, &globals, &counts, &exact, &actions,),
        digest
    );
    let candidate = &observation.candidates[0];
    let mut unsigned = candidate.u;
    unsigned[8] ^= 1;
    assert_ne!(candidate.f, action_features(&unsigned, &candidate.s));
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
            skip_next_decay: false,
            value: 0,
        },
        Power {
            id: 1,
            amount: 1,
            skip_next_decay: false,
            value: 0,
        },
    ];
    let mut ordered = game.clone();
    let first = observation_v56(&game, &content, layout, (0, 0));
    let Phase::Combat(combat) = &mut game.phase else {
        unreachable!()
    };
    combat.player.powers.reverse();
    let second = observation_v56(&game, &content, layout, (0, 0));
    let visible = |observation: &ObservationV56| {
        observation.domains[POWER_DOMAIN]
            .iter()
            .map(|row| (row.c.clone(), row.f.clone()))
            .collect::<Vec<_>>()
    };
    assert_ne!(visible(&first), visible(&second));
    ordered.step(&content, Action::EndTurn).unwrap();
    game.step(&content, Action::EndTurn).unwrap();
    assert_ne!(
        v56_bytes(&observation_v56(&ordered, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&game, &content, layout, (0, 0)))
    );
    let mut zero = DomainRow::new(ACTOR_DOMAIN, STATE_SCOPE);
    zero.u[..3].copy_from_slice(&[1, 0, 0]);
    populate_domain_features(&game, &content, layout, ACTOR_DOMAIN, &mut zero);
    assert!(zero.f.iter().all(|value| value.is_finite()));
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
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
    );
    let action = Action::Play {
        hand: 0,
        target: Some(0),
    };
    first.step(&content, action.clone()).unwrap();
    second.step(&content, action).unwrap();
    assert_ne!(
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
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
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
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
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
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
    combat.enemy_power_snapshot.truncate(1);
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
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
    );
    first.step(&content, Action::EndTurn).unwrap();
    second.step(&content, Action::EndTurn).unwrap();
    assert_eq!(first.actions(&content), second.actions(&content));
    assert_eq!(
        v56_bytes(&observation_v56(&first, &content, layout, (0, 0))),
        v56_bytes(&observation_v56(&second, &content, layout, (0, 0)))
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
    let mut game = Game::new_character_ascension(&content, 564, 0, 10).unwrap();
    game.begin_act(&content, 0).unwrap();

    let colorful = event("EVENT.COLORFUL_PHILOSOPHERS");
    game.event_data = [1, 2, 3, 0];
    game.phase = Phase::Event(
        colorful,
        content.events[colorful as usize].options[..3].to_vec(),
    );
    let observation = observation_v56(&game, &content, layout, (0, 0));
    for character in 0..3 {
        assert!(
            observation.domains[PHASE_DOMAIN][0]
                .c
                .contains(&layout.semantic(Semantic::Character, character))
        );
        let scope = observation
            .candidates
            .iter()
            .position(|row| row.action == Action::Event(character as usize))
            .unwrap() as i32;
        assert!(observation.domains[EVENT_DOMAIN].iter().any(|row| {
            row.scope == scope
                && row
                    .c
                    .contains(&layout.semantic(Semantic::Character, character))
        }));
    }

    let doll = event("EVENT.DOLL_ROOM");
    game.event_data = [0, 1, 2, 0];
    game.phase = Phase::Event(
        doll,
        vec![
            EventOption {
                requirement: Requirement::Always,
                effects: &[RunEffect::EventAction(10)],
            },
            EventOption {
                requirement: Requirement::Always,
                effects: &[RunEffect::EventAction(11)],
            },
            EventOption {
                requirement: Requirement::Always,
                effects: &[RunEffect::EventAction(12)],
            },
        ],
    );
    let observation = observation_v56(&game, &content, layout, (0, 0));
    for relic in 0..3 {
        assert!(
            observation.domains[PHASE_DOMAIN][0]
                .c
                .contains(&layout.semantic(Semantic::Relic, relic))
        );
        let scope = observation
            .candidates
            .iter()
            .position(|row| row.action == Action::Event(relic as usize))
            .unwrap() as i32;
        assert!(observation.domains[EVENT_DOMAIN].iter().any(|row| {
            row.scope == scope && row.c.contains(&layout.semantic(Semantic::Relic, relic))
        }));
    }

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
    let observation = observation_v56(&game, &content, layout, (0, 0));
    for (option, card_type) in [0, 2].into_iter().enumerate() {
        assert!(
            observation.domains[PHASE_DOMAIN][0]
                .c
                .contains(&layout.semantic(Semantic::CardType, card_type))
        );
        let scope = observation
            .candidates
            .iter()
            .position(|row| row.action == Action::Event(option))
            .unwrap() as i32;
        assert!(observation.domains[EVENT_DOMAIN].iter().any(|row| {
            row.scope == scope
                && row
                    .c
                    .contains(&layout.semantic(Semantic::CardType, card_type))
        }));
    }
    game.event_data = [4, 6, 0, 2];
    game.phase = Phase::Event(tinker, options);
    let observation = observation_v56(&game, &content, layout, (0, 0));
    assert!(
        observation.domains[PHASE_DOMAIN][0]
            .c
            .contains(&layout.semantic(Semantic::CardType, 1))
    );
    for (option, rider) in [3, 5].into_iter().enumerate() {
        assert!(
            observation.domains[PHASE_DOMAIN][0]
                .c
                .contains(&layout.semantic(Semantic::TinkerRider, rider))
        );
        let scope = observation
            .candidates
            .iter()
            .position(|row| row.action == Action::Event(option))
            .unwrap() as i32;
        assert!(observation.domains[EVENT_DOMAIN].iter().any(|row| {
            row.scope == scope
                && row
                    .c
                    .contains(&layout.semantic(Semantic::TinkerRider, rider))
        }));
    }

    let conveyor = event("EVENT.ENDLESS_CONVEYOR");
    game.event_data = [7, 2, 3, 0];
    game.phase = Phase::Event(
        conveyor,
        content.events[conveyor as usize].options[..2].to_vec(),
    );
    let phase = &observation_v56(&game, &content, layout, (0, 0)).domains[PHASE_DOMAIN][0];
    assert!(
        phase
            .c
            .contains(&layout.semantic(Semantic::ConveyorDish, 6))
    );
    assert!(
        phase
            .c
            .contains(&layout.semantic(Semantic::ConveyorDish, 2))
    );
}

#[test]
fn v55_periodic_relics_encode_phase_and_remaining() {
    let content = foundation_content();
    let layout = Layout::new(&content);
    let game = Game::new_character_ascension(&content, 555, 0, 10).unwrap();
    let id = content.relic_id("RELIC.HAPPY_FLOWER").unwrap();
    let row = |counter| {
        let mut row = DomainRow::new(RELIC_DOMAIN, STATE_SCOPE);
        row.u[..4].copy_from_slice(&[0, id as u32, 1, 0]);
        row.s[0] = counter;
        populate_domain_features(&game, &content, layout, RELIC_DOMAIN, &mut row);
        row
    };
    assert_eq!((row(0).f[10], row(0).f[11]), (0.0, 1.0));
    assert!((row(2).f[10] - 2.0 / 3.0).abs() < 1e-6);
    assert!((row(2).f[11] - 1.0 / 3.0).abs() < 1e-6);
    let id = content.relic_id("RELIC.SILVER_CRUCIBLE").unwrap();
    let capped = |counter| {
        let mut row = DomainRow::new(RELIC_DOMAIN, STATE_SCOPE);
        row.u[..4].copy_from_slice(&[0, id as u32, 1, 0]);
        row.s[0] = counter;
        populate_domain_features(&game, &content, layout, RELIC_DOMAIN, &mut row);
        row
    };
    assert_eq!((capped(2).f[12], capped(2).f[13]), (2.0 / 3.0, 1.0 / 3.0));
    assert_eq!((capped(3).f[14], capped(3).f[15]), (0.0, 1.0));
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
        (0..ACTION_KINDS).collect::<Vec<_>>()
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
        (0..PHASES).collect::<Vec<_>>()
    );
}

#[test]
fn v56_layout_uses_explicit_token_dimensions_and_positions() {
    let layout = Layout::new(&foundation_content());
    assert_eq!((VERSION, VALUE_MODEL_VERSION), (56, 74));
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
        (Semantic::MapFloorPosition, 65),
        (Semantic::DeckOrigin, 257),
        (Semantic::DrawTopPosition, 257),
        (Semantic::DrawBottomPosition, 257),
        (Semantic::RelicPosition, 257),
        (Semantic::WaxPosition, 257),
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
    let observation = observation_v56(&game, &content, layout, (0, 0));
    let actual = observation.domains[EVENT_DOMAIN]
        .iter()
        .filter(|row| row.scope == STATE_SCOPE && row.u[0] == 0)
        .map(|row| row.u[1] as Id)
        .collect::<Vec<_>>();
    assert!(!actual.contains(&visited));
    assert!(
        actual
            .iter()
            .all(|&id| game.event_allowed(&content, id) && !game.visited_events.contains(&id))
    );
}

#[test]
fn v56_actions_are_legal_only_but_all_offers_are_phase_local() {
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
    game.phase = Phase::Shop(vec![ShopItem::Card(card, 100)]);
    let observation = observation_v56(&game, &content, layout, (0, 0));
    assert_eq!(
        observation
            .candidates
            .iter()
            .map(|row| &row.action)
            .collect::<Vec<_>>(),
        [&Action::Leave]
    );
    assert!(observation.domains[CONTINUATION_DOMAIN].iter().any(|row| {
        row.scope == PHASE_SCOPE && row.u[0] == 5 && row.u[11] == card.id as u32 && row.s[5] == 100
    }));
    let position = layout.semantic_offsets[Semantic::ContinuationPosition as usize];
    let position_end = position + layout.semantic_sizes[Semantic::ContinuationPosition as usize];
    assert!(
        observation.domains[CONTINUATION_DOMAIN]
            .iter()
            .filter(|row| row.scope == PHASE_SCOPE)
            .all(|row| row
                .c
                .iter()
                .all(|code| !(*code >= position && *code < position_end)))
    );

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
    let observation = observation_v56(&game, &content, layout, (0, 0));
    assert_eq!(
        observation
            .candidates
            .iter()
            .map(|row| &row.action)
            .collect::<Vec<_>>(),
        [&Action::Event(0)]
    );
    assert_eq!(
        observation.domains[CONTINUATION_DOMAIN]
            .iter()
            .filter(|row| row.scope == PHASE_SCOPE && row.u[0] == 7)
            .count(),
        2
    );
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
    let observation = observation_v56(&game, &content, layout, (0, 0));
    let origin = layout.semantic(Semantic::DeckOrigin, 0);
    assert!(
        observation.domains[CARD_DOMAIN]
            .iter()
            .filter(|row| row.c.contains(&origin))
            .count()
            >= 2
    );
    let action = observation
        .candidates
        .iter()
        .find(|row| {
            matches!(
                row.action,
                Action::Play {
                    hand: 0,
                    target: Some(0)
                }
            )
        })
        .unwrap();
    let actor = observation.domains[ACTOR_DOMAIN]
        .iter()
        .find(|row| row.u[1] == 2)
        .unwrap();
    let enemy = layout.semantic(Semantic::Enemy, actor.u[2]);
    let position = layout.semantic(Semantic::EnemyPosition, 0);
    assert!(action.c.contains(&enemy) && action.c.contains(&position));
    assert!(actor.c.contains(&enemy) && actor.c.contains(&position));
}

#[test]
fn v56_continuations_are_flat_execution_items() {
    let content = foundation_content();
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
    let rows = continuation_domains(&game, &content);
    let items = rows
        .iter()
        .map(|row| row.u[2])
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(items.len(), 4);
    assert!(
        rows.iter()
            .all(|row| row.u[3] == NO_NODE && row.u[4..8] == [0; 4] && row.u[8] > 0)
    );
    assert!(rows.iter().all(|row| !matches!(row.u[1], 92 | 93)));
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
    let mut rows = continuation_domains(&game, &content);
    for row in &mut rows {
        populate_domain_features(&game, &content, layout, CONTINUATION_DOMAIN, row);
    }
    let effect = rows
        .iter()
        .find(|row| row.scope == STATE_SCOPE && row.u[0] == 1)
        .unwrap();
    assert_eq!(effect.f[16..20], [5.0 / 30.0, 0.2, 0.1, 13.0 / 30.0]);
    let context = rows
        .iter()
        .find(|row| row.scope == STATE_SCOPE && row.u[0] == 2)
        .unwrap();
    assert_eq!(context.f[..2], [0.8, 0.0]);
}
