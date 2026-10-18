use super::*;

#[test]
fn six_health_loses_one_per_wrong_guess_and_death_leaves_three_splatters() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    assert_eq!(read_game("player-1").unwrap().unwrap().health(), 6);
    let words = six_wrong_guesses();
    for (index, word) in words.iter().enumerate() {
        let reply = guess(&host, &chitt, word).reply.unwrap();
        assert_eq!(reply["health"], 5 - index);
        assert_eq!(reply["maxHealth"], 6);
        assert_eq!(
            reply["blood"]["total"],
            if index == 5 { 8 } else { index + 1 }
        );
        assert_eq!(reply["blood"]["fallen"], usize::from(index == 5));
        let data = card(&host, &chitt);
        assert_eq!(data["failureCount"], usize::from(index == 5));
        assert_eq!(data["obituaryCount"], usize::from(index == 5));
    }
    let lost = read_game("player-1").unwrap().unwrap();
    assert_eq!(lost.wounds.len(), 6);
    let death = &lost.wounds[5].stains;
    assert_eq!(death.len(), 3);
    let seeds: std::collections::BTreeSet<_> = lost
        .wounds
        .iter()
        .flat_map(|w| w.stains.iter().map(|s| s.seed))
        .collect();
    assert_eq!(seeds.len(), 8, "every hit has its own geometry");
    let replay = guess(&host, &chitt, &words[5]).reply.unwrap();
    assert_eq!(replay["health"], 0);
    assert_eq!(replay["blood"]["total"], 8);
    assert_eq!(
        replay["ownStains"],
        json!(death.iter().map(|s| s.seed).collect::<Vec<_>>())
    );
    assert_eq!(
        host.actions()
            .iter()
            .filter(|a| a.action == "suspend_character")
            .count(),
        1
    );
}

#[test]
fn invalid_words_and_duplicates_cost_health_but_receive_no_letter_clues() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    for word in ["!", "short"] {
        assert!(guess(&host, &chitt, word).error.is_some());
    }
    assert_eq!(read_game("player-1").unwrap().unwrap().health(), 6);
    assert_eq!(card(&host, &chitt)["splatterCount"], 0);

    let invalid = "zzzzzzzzzzzz";
    let first = guess(&host, &chitt, invalid).reply.unwrap();
    assert_eq!(first["health"], 5);
    assert_eq!(first["blood"]["total"], 1);
    assert_eq!(first["guesses"][0]["marks"], json!([]));
    assert!(first["answer"].is_null());
    assert_eq!(read_game("player-1").unwrap().unwrap().health(), 5);

    // Repeated words are another unscored hit, not a free retry or a letter-feedback row.
    let duplicate = guess(&host, &chitt, invalid).reply.unwrap();
    assert_eq!(duplicate["health"], 4);
    assert_eq!(duplicate["blood"]["total"], 2);
    assert_eq!(duplicate["guesses"][1]["marks"], json!([]));
    assert_eq!(card(&host, &chitt)["splatterCount"], 2);
    assert_eq!(host.pushes().len(), 2);

    let word = wrong_word();
    let scored = guess(&host, &chitt, &word).reply.unwrap();
    assert_eq!(scored["health"], 3);
    let duplicate_scored = guess(&host, &chitt, &word).reply.unwrap();
    assert_eq!(duplicate_scored["health"], 2);
    assert_eq!(duplicate_scored["guesses"][3]["marks"], json!([]));
    assert_eq!(duplicate_scored["blood"]["total"], 4);
    assert_eq!(host.pushes().len(), 4);
}

#[test]
fn six_unscored_words_can_kill_without_revealing_any_letter_feedback() {
    let (host, chitt) = setup();
    host.refuse_action("suspend_character", "forbidden", "permission denied");
    start_game(&host, &chitt);
    let words: Vec<String> = ('a'..='z')
        .map(|letter| std::iter::repeat_n(letter, 12).collect::<String>())
        .filter(|word| !crate::is_word(word))
        .take(6)
        .collect();
    assert_eq!(words.len(), 6);
    for (index, word) in words.iter().enumerate() {
        let reply = guess(&host, &chitt, word).reply.unwrap();
        assert_eq!(reply["health"], 5 - index);
        assert_eq!(reply["guesses"][index]["marks"], json!([]));
        assert!(reply["answer"].is_null());
    }
    let lost = read_game("player-1").unwrap().unwrap();
    assert!(lost.complete && !lost.won);
    assert_eq!(lost.health(), 0);
    assert_eq!(lost.wounds.last().unwrap().stains.len(), 3);
    assert_eq!(
        crate::read_blood(current_day().unwrap())
            .unwrap()
            .splatter_count(),
        8
    );
    assert_eq!(
        lost.guesses
            .iter()
            .map(|guess| guess.marks.len())
            .sum::<usize>(),
        0
    );
    let loss_reply = host
        .actions()
        .into_iter()
        .find(|action| action.action == "post_reply")
        .unwrap();
    let content = loss_reply.args["content"].as_str().unwrap();
    assert_eq!(content.matches("— no clues —").count(), 6);
    assert!(!content.contains("⬛") && !content.contains("🟨") && !content.contains("🟩"));
}

#[test]
fn survivors_keep_their_remaining_health_and_their_wrong_guesses_mess_up_the_shared_board() {
    for wrong_count in [0, 2, 5] {
        let (host, chitt) = setup();
        start_game(&host, &chitt);
        for word in six_wrong_guesses().iter().take(wrong_count) {
            assert!(guess(&host, &chitt, word).error.is_none());
        }
        let answer = crate::daily_answer(current_day().unwrap()).unwrap();
        let reply = guess(&host, &chitt, &answer).reply.unwrap();
        assert_eq!(reply["won"], true);
        assert_eq!(reply["health"], 6 - wrong_count);
        assert_eq!(reply["blood"]["total"], wrong_count);
        assert_eq!(reply["blood"]["fallen"], 0);
        let data = card(&host, &chitt);
        assert_eq!(data["failureCount"], 0);
        assert_eq!(data["survivorsCount"], 1);
        assert_eq!(data["splatterCount"], wrong_count);
        assert_eq!(
            data["health"],
            serde_json::Value::Null,
            "no viewer means no personal health"
        );
        assert!(!host
            .actions()
            .iter()
            .any(|a| a.action == "suspend_character"));
    }
}

#[test]
fn saved_wounds_recover_their_original_colour_and_are_deduplicated() {
    let (host, chitt) = setup();
    let mut observer = named("player-1", "bleeder", "#0088cc");
    start_as(&host, &chitt, &observer);
    let words = six_wrong_guesses();
    let first = guess_as(&host, &chitt, observer.clone(), &words[0])
        .reply
        .unwrap();
    let game = read_game("player-1").unwrap().unwrap();
    host.begin_hook(HookClass::Background);
    chittr_cartridge::kv::delete(&crate::blood_key(game.day)).unwrap();
    observer
        .profile
        .insert("textColor".to_string(), json!("#aa33cc"));
    // As if the guess committed but its shared-ledger write did not finish. Recovering the saved
    // wound preserves its original colour and is idempotent; a later duplicate is charged anew.
    assert!(crate::record_damage(&wire(observer.clone()), &game).unwrap());
    assert!(!crate::record_damage(&wire(observer.clone()), &game).unwrap());
    assert_eq!(read_game("player-1").unwrap().unwrap().health(), 5);
    assert_eq!(
        crate::shared_canvas_state(None, game.day).unwrap()["blood"],
        first["blood"]
    );
    let next = guess_as(&host, &chitt, observer, &words[1]).reply.unwrap();
    assert_eq!(next["blood"]["stains"][0]["color"], 0x0088ccffu32);
    assert_eq!(next["blood"]["stains"][1]["color"], 0xaa33ccffu32);
    assert_eq!(next["blood"]["total"], 2);
}

#[test]
fn upgrading_keeps_old_blood_and_derives_health_without_retroactive_splatters() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    let words = six_wrong_guesses();
    assert!(guess(&host, &chitt, &words[0]).error.is_none());
    let day = current_day().unwrap();
    let mut old_game = serde_json::to_value(read_game("player-1").unwrap().unwrap()).unwrap();
    old_game.as_object_mut().unwrap().remove("wounds");
    host.put_kv("game:player-1", old_game.to_string().as_bytes(), None)
        .unwrap();
    let old_blood = json!({
        "victims": ["old-victim"],
        "recent": [{ "seed": crate::stain_seed("old-victim", day), "color": 0x0088ccffu32 }]
    });
    host.put_kv(
        &crate::blood_key(day),
        old_blood.to_string().as_bytes(),
        None,
    )
    .unwrap();
    upgrade_live_chitt_again(&host, &chitt);
    assert_eq!(read_game("player-1").unwrap().unwrap().health(), 5);
    assert_eq!(card(&host, &chitt)["splatterCount"], 1);
    let reply = guess(&host, &chitt, &words[1]).reply.unwrap();
    assert_eq!(reply["health"], 4);
    assert_eq!(reply["blood"]["total"], 2);
    assert_eq!(reply["blood"]["fallen"], 1);
    assert_eq!(reply["blood"]["stains"][0], old_blood["recent"][0]);
    assert_eq!(card(&host, &chitt)["obituaryCount"], 1);
}

#[test]
fn personal_health_resets_for_the_new_word_and_is_not_changed_by_other_players() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    assert!(guess(&host, &chitt, &wrong_word()).error.is_none());
    lose_as(&host, &chitt, &named("other-player", "other", "#a33"));
    host.begin_hook(HookClass::Render);
    let own = render(wire(testing::render(&chitt, Some(player_observer())))).unwrap();
    assert_eq!(own.data["health"], 5);
    assert_eq!(own.canvas.unwrap()["health"], 5);
    let timer = host.fire_timer(&chitt, DAILY_TIMER).unwrap();
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(timer)).unwrap().error.is_none());
    start_game(&host, &chitt);
    assert_eq!(read_game("player-1").unwrap().unwrap().health(), 6);
    assert_eq!(card(&host, &chitt)["splatterCount"], 0);
}

#[test]
fn a_full_wall_and_fatal_reply_stay_within_the_canvas_message_budget() {
    let (host, chitt) = setup();
    let day = current_day().unwrap();
    for index in 0..70 {
        host.begin_hook(HookClass::Background);
        let observer: Observer = wire(named(&format!("other-{index}"), "other", "#0088cc"));
        let mut game = Game::new(day, crate::daily_answer(day).unwrap());
        game.wounds.push(crate::wound_for(&observer, day, 1));
        crate::record_damage(&observer, &game).unwrap();
    }
    start_game(&host, &chitt);
    let mut last = None;
    for word in six_wrong_guesses() {
        last = guess(&host, &chitt, &word).reply;
    }
    let reply = last.unwrap();
    assert_eq!(reply["blood"]["total"], 78);
    assert_eq!(reply["blood"]["stains"].as_array().unwrap().len(), 64);
    let encoded = serde_json::to_string(&reply).unwrap();
    assert!(encoded.len() <= 4096, "reply was {} bytes", encoded.len());
    assert!(!encoded.contains("player-1") && !encoded.contains("other-"));
}
