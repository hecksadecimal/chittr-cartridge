use super::*;

#[test]
fn health_and_a_nonfatal_splatter_wait_for_the_wrong_guesses_reveal() {
    let mut game = started_game();
    game.state.health = Some(6);
    assert_eq!(visible_health(&game), Some(6));
    let mut state = game.state.clone();
    state.guesses.push(crane());
    state.health = Some(5);
    state.blood = stains(1);
    update_state(&mut game, state.clone(), true);
    tick(&mut game, 0.0);
    assert_eq!(visible_health(&game), Some(6));
    assert_eq!(game.fx.fresh, 0..1);
    assert_eq!(game.fx.splat, Anim::Running(REVEAL_MS as f64));
    assert_eq!(game.fx.loss, Anim::Idle);
    tick(&mut game, REVEAL_MS as f64);
    assert_eq!(visible_health(&game), Some(5));
    update_state(&mut game, state, false);
    assert_eq!(visible_health(&game), Some(5));
    assert_eq!(
        game.fx.splat,
        Anim::Running(REVEAL_MS as f64),
        "push must not replay the hit"
    );
}

#[test]
fn winning_costs_no_health_and_death_reveals_all_three_splatters() {
    let mut winner = started_game();
    let mut won = winner.state.clone();
    won.guesses.push(Guess {
        word: "blade".into(),
        marks: vec![2; 5],
    });
    won.complete = true;
    won.won = true;
    won.health = Some(6);
    update_state(&mut winner, won, true);
    tick(&mut winner, 0.0);
    assert_eq!(visible_health(&winner), Some(6));
    tick(&mut winner, REVEAL_MS as f64);
    assert_eq!(visible_health(&winner), Some(6));
    assert_eq!(winner.fx.splat, Anim::Idle);

    let mut dying = started_game();
    dying.state.guesses = vec![crane(); 5];
    dying.state.health = Some(1);
    dying.state.blood = stains(5);
    let mut dead = dying.state.clone();
    dead.guesses.push(crane());
    dead.complete = true;
    dead.health = Some(0);
    dead.blood = stains(8);
    update_state(&mut dying, dead, true);
    tick(&mut dying, 0.0);
    assert_eq!(visible_health(&dying), Some(1));
    assert_eq!(dying.fx.fresh, 5..8);
    tick(&mut dying, REVEAL_MS as f64);
    assert_eq!(visible_health(&dying), Some(0));
    assert_eq!(dying.fx.splat.elapsed(dying.clock), 0.0);
}

#[test]
fn reconnect_replays_only_the_deaths_three_stains_even_with_newer_blood() {
    let mut game = Game::default();
    game.state = lost_state(true);
    game.state.health = Some(0);
    game.state.blood = stains(12);
    game.state.own_stains = game.state.blood.stains[5..8]
        .iter()
        .map(|stain| stain.seed)
        .collect();
    game.state.own_stain = game.state.own_stains.first().copied();
    replay_unseen_loss(&mut game);
    assert_eq!(game.fx.fresh, 5..8);
    assert!(game.acknowledge);
    tick(&mut game, 0.0);
    assert_eq!(visible_health(&game), Some(1));
    tick(&mut game, (REVEAL_MS + SPLAT_MS) as f64);
    assert_eq!(visible_health(&game), Some(0));
    assert!(!game.fx.active(game.clock));
}

#[test]
fn health_bar_handles_old_states_reduced_motion_and_new_rounds() {
    let mut game = started_game();
    game.still = true;
    let mut state = game.state.clone();
    state.guesses.push(crane());
    state.blood = stains(1);
    // An old state has no explicit health, so the bar falls back to its saved marks.
    update_state(&mut game, state.clone(), true);
    assert_eq!(visible_health(&game), Some(5));
    assert_eq!(game.fx.splat, Anim::Idle);
    let mut damaged = Canvas2D::new();
    draw_health(&mut damaged, &game);
    state.round = Some(13);
    state.guesses.clear();
    state.health = Some(6);
    update_state(&mut game, state, false);
    assert_eq!(visible_health(&game), Some(6));
    let mut full = Canvas2D::new();
    draw_health(&mut full, &game);
    assert_ne!(damaged.bytes(), full.bytes());
    assert!(full.len() < 4096);
    assert_eq!(visible_health(&Game::default()), None);
}

#[test]
fn unscored_words_and_duplicates_cost_health_without_keyboard_or_tile_feedback() {
    let mut game = started_game();
    game.state.health = Some(6);
    let mut first = game.state.clone();
    first.guesses.push(Guess {
        word: "zzzzz".to_string(),
        marks: vec![],
    });
    first.health = Some(5);
    first.blood = stains(1);
    update_state(&mut game, first.clone(), true);
    tick(&mut game, 0.0);
    assert_eq!(
        visible_health(&game),
        Some(6),
        "damage waits for the row reveal"
    );
    tick(&mut game, REVEAL_MS as f64);
    assert_eq!(visible_health(&game), Some(5));
    assert_eq!(
        letter_palette(&game.state.guesses, None),
        std::collections::HashMap::new()
    );

    let mut valid_wrong = started_game();
    valid_wrong.state.guesses.push(crane());
    let mut unscored_keyboard = Canvas2D::new();
    draw_keyboard(&mut unscored_keyboard, &game);
    let clean = started_game();
    let mut clean_keyboard = Canvas2D::new();
    draw_keyboard(&mut clean_keyboard, &clean);
    assert_eq!(unscored_keyboard.bytes(), clean_keyboard.bytes());
    let mut scored_keyboard = Canvas2D::new();
    draw_keyboard(&mut scored_keyboard, &valid_wrong);
    assert_ne!(scored_keyboard.bytes(), clean_keyboard.bytes());

    let mut duplicate = first;
    duplicate.guesses.push(Guess {
        word: "zzzzz".to_string(),
        marks: vec![],
    });
    duplicate.health = Some(4);
    duplicate.blood = stains(2);
    update_state(&mut game, duplicate, true);
    assert_eq!(visible_health(&game), Some(5));
    tick(&mut game, REVEAL_MS as f64 * 2.0);
    assert_eq!(visible_health(&game), Some(5));
    tick(&mut game, REVEAL_MS as f64 * 3.0);
    assert_eq!(visible_health(&game), Some(4));
    let mut board = Canvas2D::new();
    draw_board(&mut board, &game);
    assert!(board.len() > 0);
    if let Some(dir) = std::env::var_os("HURTLE_PREVIEW_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut scene = Canvas2D::new();
        draw_scene(&mut scene, &game);
        std::fs::write(dir.join("unscored-duplicate.bin"), scene.bytes()).unwrap();
    }
}

#[test]
fn twelve_letter_health_scenes_fit_and_can_be_previewed() {
    for misses in [0, 1, 3, 6] {
        let mut game = started_game();
        game.state.length = Some(12);
        game.state.health = Some(6 - misses);
        let words = [
            "deceptionist",
            "abbreviation",
            "construction",
            "appreciation",
            "championship",
            "constitution",
        ];
        game.state.guesses = words
            .iter()
            .take(misses)
            .map(|word| Guess {
                word: word.to_string(),
                marks: vec![0, 1, 0, 0, 2, 0, 0, 1, 0, 0, 0, 1],
            })
            .collect();
        game.state.complete = misses == 6;
        game.state.penalty_applied = misses == 6;
        game.state.blood = stains(if misses == 6 { 8 } else { misses as u64 });
        let mut canvas = Canvas2D::new();
        draw_scene(&mut canvas, &game);
        assert!(canvas.len() < 600_000);
        if let Some(dir) = std::env::var_os("HURTLE_PREVIEW_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(format!("health-{}.bin", 6 - misses)),
                canvas.bytes(),
            )
            .unwrap();
        }
    }
}

#[test]
fn shared_nonfatal_blood_does_not_damage_or_interrupt_the_viewers_attempt() {
    let mut game = started_game();
    game.state.health = Some(6);
    game.current = "CR".into();
    let mut state = game.state.clone();
    state.blood = stains(1);
    update_state(&mut game, state, false);
    assert_eq!(visible_health(&game), Some(6));
    assert_eq!(game.current, "CR");
    assert_eq!(game.fx.splat, Anim::Queued(0.0));
    assert_eq!(game.fx.loss, Anim::Idle);
}
