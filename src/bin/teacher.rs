use std::{env, time::Instant};
use sts2_sim::{
    Action, Card, CardRarity, CardType, Content, Game, Phase, Room, foundation_content,
};

const MAX_STEPS: usize = 1024;
const MAX_DEPTH: usize = 12;

fn card_score(content: &Content, card: Card) -> i32 {
    let def = content.cards()[card.id as usize];
    let rarity = match def.rarity {
        CardRarity::Basic => -12,
        CardRarity::Common => 6,
        CardRarity::Uncommon => 12,
        CardRarity::Rare | CardRarity::Ancient => 18,
        CardRarity::Event | CardRarity::Quest => 0,
    };
    rarity
        + match def.card_type {
            CardType::Curse => -50,
            CardType::Status => -25,
            _ => 0,
        }
        + card.upgrades as i32 * 5
        - def.cost[card.upgrades.min(1) as usize].max(0) as i32
}

fn combat_card_score(content: &Content, card: Card) -> i32 {
    card_score(content, card)
        + match content.cards()[card.id as usize].card_type {
            CardType::Attack => 10,
            CardType::Power => 5,
            _ => 0,
        }
}

fn intent_damage(intent: &str) -> i64 {
    let Some(attack) = intent.split("Attack ").nth(1) else {
        return 0;
    };
    let token = attack.split(',').next().unwrap_or(attack);
    let mut parts = token.split('x');
    let damage = parts
        .next()
        .and_then(|x| x.parse::<i64>().ok())
        .unwrap_or(0);
    damage
        * parts
            .next()
            .and_then(|x| x.parse::<i64>().ok())
            .unwrap_or(1)
}

fn score(game: &Game, content: &Content) -> i64 {
    match game.phase() {
        Phase::Won => return i64::MAX / 4,
        Phase::Dead => return i64::MIN / 4,
        _ => {}
    }
    let run = game.run();
    let hp = game.combat().map_or(run.hp, |combat| combat.player.hp);
    let mut value = ((run.act.saturating_sub(1) as i64 * 17 + run.floor as i64) * 1_000_000)
        + hp as i64 * 10_000
        + run.max_hp as i64 * 500
        + run.relics.len() as i64 * 2_000
        + run.potions.iter().flatten().count() as i64 * 500
        + run.gold as i64
        + run
            .deck
            .iter()
            .map(|&card| card_score(content, card) as i64 * 50)
            .sum::<i64>()
        - run.deck.len() as i64 * run.deck.len() as i64 * 8;
    if let Some(combat) = game.combat() {
        value += combat.player.block as i64 * 500
            + combat.energy as i64 * 20
            + combat.stars as i64 * 2_000
            + combat.osty.hp.max(0) as i64 * 1_000
            + combat
                .draw
                .iter()
                .chain(&combat.hand)
                .chain(&combat.discard)
                .map(|card| combat_card_score(content, *card) as i64 * 200)
                .sum::<i64>()
            - combat
                .exhaust
                .iter()
                .map(|card| combat_card_score(content, *card) as i64 * 100)
                .sum::<i64>();
        value -= combat
            .enemies
            .iter()
            .map(|enemy| {
                let adaptable =
                    enemy.creature.powers.iter().any(|power| {
                        content.powers()[power.id as usize].id == "POWER.ADAPTABLE_POWER"
                    });
                let future_hp = if adaptable
                    && content.enemies()[enemy.creature.id as usize].id == "MONSTER.TEST_SUBJECT"
                {
                    if enemy.creature.max_hp < 200 {
                        525
                    } else {
                        313
                    }
                } else {
                    0
                };
                (enemy.creature.hp.max(0) + future_hp) as i64 * 600
                    + enemy.creature.block as i64 * 50
            })
            .sum::<i64>();
        for power in &combat.player.powers {
            let def = content.powers()[power.id as usize];
            value += power.amount as i64
                * match def.id {
                    "POWER.STRENGTH_POWER" => 5_000,
                    "POWER.DEXTERITY_POWER" => 4_000,
                    "POWER.FOCUS_POWER" => 6_000,
                    "POWER.INTANGIBLE_POWER" => 20_000,
                    _ if def.debuff => -500,
                    _ => 500,
                };
        }
        for orb in &combat.orbs {
            value += match content.orbs()[orb.id as usize].id {
                "ORB.LIGHTNING_ORB" => 3_000,
                "ORB.FROST_ORB" => 2_400,
                "ORB.DARK_ORB" | "ORB.GLASS_ORB" => orb.value as i64 * 900,
                "ORB.PLASMA_ORB" => 4_000,
                _ => 0,
            };
        }
        for enemy in &combat.enemies {
            for power in &enemy.creature.powers {
                let def = content.powers()[power.id as usize];
                value += power.amount as i64
                    * if def.id == "POWER.SANDPIT_POWER" {
                        40_000
                    } else if def.id == "POWER.POISON_POWER" {
                        3_000
                    } else if def.debuff {
                        300
                    } else {
                        -300
                    };
            }
        }
        let incoming = combat
            .enemies
            .iter()
            .filter(|enemy| enemy.creature.hp > 0)
            .map(|enemy| {
                intent_damage(
                    content.enemies()[enemy.creature.id as usize].moves[enemy.move_index].intent,
                )
            })
            .sum::<i64>();
        value -= (incoming - combat.player.block as i64).max(0) * 5_000;
    } else {
        value += 200_000;
    }
    value
}

fn branch_actions(game: &Game, content: &Content) -> Vec<Action> {
    let actions = game.actions(content);
    if matches!(game.phase(), Phase::Rewards(_)) {
        for match_action in [
            |action: &Action| matches!(action, Action::RewardRelic(_)),
            |action: &Action| matches!(action, Action::RewardGold),
            |action: &Action| matches!(action, Action::RewardPotion(_)),
            |action: &Action| matches!(action, Action::RewardRemove),
        ] {
            let selected = actions
                .iter()
                .filter(|action| match_action(action))
                .cloned()
                .collect::<Vec<_>>();
            if !selected.is_empty() {
                return selected;
            }
        }
        let selected = actions
            .iter()
            .filter(|action| matches!(action, Action::RewardCard(_) | Action::Cancel))
            .cloned()
            .collect::<Vec<_>>();
        if !selected.is_empty() {
            return selected;
        }
    }
    actions
}

fn combat_options(
    game: &Game,
    content: &Content,
    width: usize,
    node_budget: usize,
    limit: usize,
) -> Vec<(Game, Vec<Action>)> {
    if game.combat().is_none() {
        return vec![];
    }
    let mut frontier = vec![(game.clone(), Vec::new())];
    let mut finished = Vec::new();
    let mut nodes = 0;
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for (state, plan) in frontier {
            for action in state.actions(content) {
                if nodes >= node_budget || matches!(action, Action::DiscardPotion(_)) {
                    continue;
                }
                nodes += 1;
                if matches!(action, Action::EndTurn) {
                    let mut child_plan = plan.clone();
                    child_plan.push(action);
                    finished.push((state.clone(), child_plan));
                    continue;
                }
                let mut child = state.clone();
                if child.step(content, action.clone()).is_err() {
                    continue;
                }
                let mut child_plan = plan.clone();
                child_plan.push(action);
                if child.combat().is_none() {
                    finished.push((child, child_plan));
                } else {
                    next.push((child, child_plan));
                }
            }
        }
        next.sort_unstable_by_key(|(state, _)| std::cmp::Reverse(score(state, content)));
        next.truncate(width);
        if next.is_empty() || nodes >= node_budget {
            break;
        }
        frontier = next;
    }
    if finished.is_empty()
        && let Some(action) = game
            .actions(content)
            .into_iter()
            .find(|action| matches!(action, Action::EndTurn))
    {
        finished.push((game.clone(), vec![action]));
    }
    finished.sort_unstable_by_key(|(state, _)| std::cmp::Reverse(score(state, content)));
    finished.truncate(limit);
    finished
}

fn combat_plan(game: &Game, content: &Content, width: usize, node_budget: usize) -> Vec<Action> {
    combat_options(game, content, width, node_budget, 1)
        .pop()
        .map(|(_, plan)| plan)
        .unwrap_or_default()
}

fn room(game: &Game, path: usize) -> Room {
    game.map().nodes[path].room
}

fn strategic_action(game: &Game, content: &Content) -> Action {
    let actions = game.actions(content);
    let first = actions[0].clone();
    match game.phase() {
        Phase::Map => {
            let hurt = game.run().hp * 3 < game.run().max_hp * 2;
            actions
                .into_iter()
                .max_by_key(|action| match action {
                    Action::Path(path) => match room(game, *path) {
                        Room::Rest if hurt => 100,
                        Room::Shop if game.run().gold >= 120 => 95,
                        Room::Treasure => 90,
                        Room::Unknown => 80,
                        Room::Combat => 70,
                        Room::Rest => 65,
                        Room::Elite if game.run().hp * 4 >= game.run().max_hp * 3 => 60,
                        Room::Shop => 40,
                        Room::Elite => 10,
                        Room::Boss => 0,
                        _ => 20,
                    },
                    _ => 0,
                })
                .unwrap()
        }
        Phase::Rewards(rewards) => {
            for pick in [
                |a: &Action| matches!(a, Action::RewardRelic(_)),
                |a: &Action| matches!(a, Action::RewardGold),
                |a: &Action| matches!(a, Action::RewardPotion(_)),
                |a: &Action| matches!(a, Action::RewardRemove),
            ] {
                if let Some(action) = actions.iter().find(|action| pick(action)) {
                    return action.clone();
                }
            }
            if !rewards.cards.is_empty() {
                return actions
                    .into_iter()
                    .max_by_key(|action| match action {
                        Action::RewardCard(index) => card_score(content, rewards.cards[*index]),
                        Action::Cancel => 0,
                        _ => -1000,
                    })
                    .unwrap();
            }
            actions
                .into_iter()
                .find(|action| matches!(action, Action::Cancel | Action::Leave))
                .unwrap_or(first)
        }
        Phase::Rest => {
            if game.run().hp * 4 < game.run().max_hp * 3
                && let Some(action) = actions.iter().find(|action| matches!(action, Action::Rest))
            {
                return action.clone();
            }
            actions
                .iter()
                .find(|action| matches!(action, Action::Smith(_)))
                .or_else(|| {
                    actions
                        .iter()
                        .find(|action| !matches!(action, Action::Cancel))
                })
                .cloned()
                .unwrap_or(first)
        }
        Phase::RemoveCards(..) | Phase::TransformCards(..) => actions
            .into_iter()
            .min_by_key(|action| match action {
                Action::RemoveCard(index) => card_score(content, game.run().deck[*index]),
                _ => 1000,
            })
            .unwrap(),
        Phase::UpgradeCards(..) | Phase::EnchantCards(..) => actions
            .into_iter()
            .max_by_key(|action| match action {
                Action::Smith(index) | Action::Enchant(index) => {
                    card_score(content, game.run().deck[*index])
                }
                _ => -1000,
            })
            .unwrap(),
        _ => actions
            .into_iter()
            .filter_map(|action| {
                let mut next = game.clone();
                next.step(content, action.clone()).ok()?;
                Some((score(&next, content), action))
            })
            .max_by_key(|(value, _)| *value)
            .map(|(_, action)| action)
            .unwrap_or(first),
    }
}

fn run(
    content: &Content,
    seed: u64,
    character: u16,
    width: usize,
    budget: usize,
) -> (bool, u8, usize) {
    let mut game = Game::new_character_ascension(content, seed, character, 10).unwrap();
    game.begin_act(content, 0).unwrap();
    let mut plan = Vec::new();
    for step in 1..=MAX_STEPS {
        if matches!(game.phase(), Phase::Won | Phase::Dead) {
            let floor = (game.run().act.saturating_sub(1) * 17 + game.run().floor).min(52);
            return (matches!(game.phase(), Phase::Won), floor, step - 1);
        }
        let actions = game.actions(content);
        if actions.is_empty() {
            break;
        }
        if plan.first().is_none_or(|action| !actions.contains(action)) {
            plan = if game.combat().is_some() {
                combat_plan(&game, content, width, budget)
            } else {
                vec![strategic_action(&game, content)]
            };
        }
        let action = plan
            .first()
            .cloned()
            .unwrap_or_else(|| strategic_action(&game, content));
        if !plan.is_empty() {
            plan.remove(0);
        }
        if game.step(content, action).is_err() {
            break;
        }
    }
    let floor = (game.run().act.saturating_sub(1) * 17 + game.run().floor).min(52);
    (false, floor, MAX_STEPS)
}

fn finish_combat(game: &mut Game, content: &Content, width: usize, budget: usize) -> usize {
    let mut steps = 0;
    while game.combat().is_some() && steps < 256 {
        let plan = combat_plan(game, content, width, budget);
        if plan.is_empty() {
            break;
        }
        for action in plan {
            if game.step(content, action).is_err() {
                return steps;
            }
            steps += 1;
            if game.combat().is_none() || steps >= 256 {
                break;
            }
        }
    }
    steps
}

fn room_beam(
    roots: Vec<Game>,
    content: &Content,
    width: usize,
    combat_width: usize,
    combat_budget: usize,
) -> (Vec<Game>, usize) {
    let mut frontier = roots
        .into_iter()
        .flat_map(|game| {
            game.actions(content).into_iter().filter_map(move |action| {
                let mut next = game.clone();
                next.step(content, action).ok().map(|_| next)
            })
        })
        .collect::<Vec<_>>();
    let mut finished = Vec::new();
    let mut nodes = frontier.len();
    for _ in 0..256 {
        let mut next = Vec::new();
        for mut game in frontier {
            if game.combat().is_some() {
                nodes += finish_combat(&mut game, content, combat_width, combat_budget);
            }
            if matches!(game.phase(), Phase::Map | Phase::Won | Phase::Dead) {
                finished.push(game);
                continue;
            }
            for action in branch_actions(&game, content) {
                if nodes >= width * 2_000 {
                    continue;
                }
                nodes += 1;
                let mut child = game.clone();
                if child.step(content, action).is_ok() {
                    next.push(child);
                }
            }
        }
        next.sort_unstable_by_key(|game| std::cmp::Reverse(score(game, content)));
        next.truncate(width);
        if next.is_empty() || nodes >= width * 2_000 {
            break;
        }
        frontier = next;
    }
    finished.sort_unstable_by_key(|game| std::cmp::Reverse(score(game, content)));
    finished.truncate(width);
    (finished, nodes)
}

fn search_run(
    content: &Content,
    seed: u64,
    character: u16,
    width: usize,
    combat_width: usize,
    combat_budget: usize,
) -> (bool, u8, usize) {
    let mut game = Game::new_character_ascension(content, seed, character, 10).unwrap();
    game.begin_act(content, 0).unwrap();
    let mut beam = vec![game];
    let mut nodes = 0;
    let mut best_floor = 1;
    for _ in 0..60 {
        let result = room_beam(beam, content, width, combat_width, combat_budget);
        beam = result.0;
        nodes += result.1;
        best_floor = best_floor.max(
            beam.iter()
                .map(|game| (game.run().act.saturating_sub(1) * 17 + game.run().floor).min(52))
                .max()
                .unwrap_or(0),
        );
        if beam.iter().any(|game| matches!(game.phase(), Phase::Won)) {
            return (true, 52, nodes);
        }
        beam.retain(|game| !matches!(game.phase(), Phase::Dead));
        if beam.is_empty() {
            break;
        }
    }
    let floor = best_floor.max(
        beam.iter()
            .map(|game| (game.run().act.saturating_sub(1) * 17 + game.run().floor).min(52))
            .max()
            .unwrap_or(0),
    );
    (false, floor, nodes)
}

fn main() {
    let runs = env::args()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .unwrap_or(20);
    let width = env::args()
        .nth(2)
        .and_then(|x| x.parse().ok())
        .unwrap_or(32);
    let budget = env::args()
        .nth(3)
        .and_then(|x| x.parse().ok())
        .unwrap_or(10_000);
    let room_width = env::args().nth(4).and_then(|x| x.parse().ok()).unwrap_or(1);
    let only_character: Option<u16> = env::args().nth(5).and_then(|x| x.parse().ok());
    let content = foundation_content();
    for character in 0..content.characters().len() as u16 {
        if only_character.is_some_and(|only| only != character) {
            continue;
        }
        let started = Instant::now();
        let mut floors = Vec::new();
        let mut wins = 0;
        let mut caps = 0;
        for index in 0..runs {
            let seed = 60_000_000 + character as u64 * 100_000 + index as u64;
            let (won, floor, steps) = if room_width == 1 {
                run(&content, seed, character, width, budget)
            } else {
                search_run(&content, seed, character, room_width, width, budget)
            };
            wins += won as usize;
            caps += (room_width == 1 && steps == MAX_STEPS) as usize;
            floors.push(floor);
        }
        floors.sort_unstable();
        let mean = floors.iter().map(|&x| x as f32).sum::<f32>() / runs as f32;
        println!(
            "{{\"character\":{character},\"runs\":{runs},\"wins\":{wins},\"caps\":{caps},\"mean_floor\":{mean:.2},\"median_floor\":{},\"p90_floor\":{},\"max_floor\":{},\"seconds\":{:.1}}}",
            floors[runs / 2],
            floors[(runs * 9 / 10).min(runs - 1)],
            floors[runs - 1],
            started.elapsed().as_secs_f32(),
        );
    }
}
