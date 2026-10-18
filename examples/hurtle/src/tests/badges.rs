use super::*;
use crate::badges::{BadgeRewardStatus, CONFIG_KEY, CONTROL, MAX_ID_BYTES};

fn configure_as(
    host: &TestHost,
    chitt: &testing::ChittRef,
    observer: Option<chittr_cartridge::envelope::Observer>,
    fields: serde_json::Value,
) -> ControlOutput {
    host.begin_hook(HookClass::Control);
    on_control(wire(testing::control_fields(
        chitt,
        observer,
        CONTROL,
        fields.as_object().unwrap().clone(),
    )))
    .unwrap()
}

fn configure(host: &TestHost, chitt: &testing::ChittRef, id: &str) {
    let output = configure_as(
        host,
        chitt,
        Some(testing::observer("admin").admin()),
        json!({ "badgeId": id }),
    );
    assert!(output.error.is_none(), "{:?}", output.error);
}

fn badge_actions(host: &TestHost) -> Vec<testing::RecordedAction> {
    host.actions()
        .into_iter()
        .filter(|action| action.action == "grant_badge_to_character")
        .collect()
}

fn player_card(
    host: &TestHost,
    chitt: &testing::ChittRef,
    observer: chittr_cartridge::envelope::Observer,
) -> RenderOutput {
    host.begin_hook(HookClass::Render);
    render(wire(testing::render(chitt, Some(observer)))).unwrap()
}

#[test]
fn only_admins_can_set_the_badge_and_only_they_receive_the_setting() {
    let (host, chitt) = setup();
    let before = host.kv_keys();
    for observer in [
        None,
        Some(player_observer()),
        Some(testing::observer("poster-1").poster()),
    ] {
        let output = configure_as(&host, &chitt, observer, json!({ "badgeId": "badge-1" }));
        assert_eq!(output.error.unwrap().code, "forbidden");
        assert_eq!(host.kv_keys(), before);
        assert!(host.pushes().is_empty());
    }
    configure(&host, &chitt, "  badge_1-ABC  ");
    assert_eq!(host.kv_text(CONFIG_KEY).as_deref(), Some("badge_1-ABC"));
    assert!(
        host.actions().is_empty(),
        "configuration never grants a badge"
    );
    let admin = player_card(&host, &chitt, testing::observer("admin").admin());
    assert_eq!(admin.data["firstGuessBadgeId"], "badge_1-ABC");
    assert_eq!(admin.data["firstGuessBadgeEnabled"], true);
    for data in [
        player_card(&host, &chitt, player_observer()).data,
        card(&host, &chitt),
    ] {
        assert!(!data.contains_key("firstGuessBadgeId"));
        assert!(!data.contains_key("firstGuessBadgeEnabled"));
    }
    configure(&host, &chitt, " \t ");
    assert!(host.kv_value(CONFIG_KEY).is_none());
    assert_eq!(
        player_card(&host, &chitt, testing::observer("admin").admin()).data
            ["firstGuessBadgeEnabled"],
        false
    );
}

#[test]
fn badge_configuration_validates_untrusted_fields_and_obeys_the_reset_guard() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "keep-this-badge");
    for fields in [
        json!({}),
        json!({ "badgeId": null }),
        json!({ "badgeId": 5 }),
        json!({ "badgeId": "badge-1", "characterId": "victim" }),
        json!({ "badgeId": "x".repeat(MAX_ID_BYTES + 1) }),
        json!({ "badgeId": "badge with spaces" }),
        json!({ "badgeId": "<script>" }),
    ] {
        let output = configure_as(
            &host,
            &chitt,
            Some(testing::observer("admin").admin()),
            fields,
        );
        assert_eq!(output.error.unwrap().code, "bad_input");
        assert_eq!(host.kv_text(CONFIG_KEY).as_deref(), Some("keep-this-badge"));
    }
    host.put_kv(crate::RESET_KEY, b"99999", None).unwrap();
    let output = configure_as(
        &host,
        &chitt,
        Some(testing::observer("admin").admin()),
        json!({ "badgeId": "new" }),
    );
    assert_eq!(output.error.unwrap().code, "reset_in_progress");
    assert_eq!(host.kv_text(CONFIG_KEY).as_deref(), Some("keep-this-badge"));
}

#[test]
fn the_badge_form_uses_a_named_field_and_does_not_depend_on_stripped_input_values() {
    let template = include_str!("../../cartridge/templates/card.liquid");
    let admin = template.split("{% if isAdmin %}").nth(1).unwrap();
    let form = admin
        .split("<form ")
        .nth(1)
        .unwrap()
        .split("</form>")
        .next()
        .unwrap();
    assert!(form.contains("data-lc-control=\"set-win-badge\""));
    assert!(form.contains("name=\"badgeId\""));
    assert!(form.contains("maxlength=\"128\""));
    assert!(form.contains("{{ firstGuessBadgeId | escape }}"));
    assert!(form.contains("type=\"submit\""));
    let input = form
        .split("<input ")
        .nth(1)
        .unwrap()
        .split('>')
        .next()
        .unwrap();
    assert!(!input.contains("value="));
    assert!(template.contains("{{ badgeRewardError | escape }}"));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../cartridge/manifest.json")).unwrap();
    assert_eq!(
        manifest["controls"][CONTROL]["fields"]["badgeId"],
        json!({ "type": "string", "maxLength": MAX_ID_BYTES })
    );
}

#[test]
fn a_first_valid_guess_win_awards_the_configured_badge_to_the_trusted_winner_once() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "badge-one-shot");
    start_game(&host, &chitt);
    assert_eq!(guess(&host, &chitt, "!!!").error.unwrap().code, "bad_guess");
    let answer = crate::daily_answer(current_day().unwrap()).unwrap();
    host.begin_hook(HookClass::Background);
    let output = on_canvas_message(wire(testing::canvas_message(
        &chitt,
        Some(player_observer()),
        json!({ "type": "guess", "word": answer, "badgeId": "forged", "characterId": "someone-else", "won": true }),
        Some(1),
    )))
    .unwrap();
    assert!(output.error.is_none());
    assert_eq!(output.reply.unwrap()["won"], true);
    let actions = badge_actions(&host);
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].args,
        json!({ "characterId": "player-1", "badgeId": "badge-one-shot" })
    );
    assert_eq!(
        host.actions()
            .iter()
            .filter(|action| action.action == "post_reply")
            .count(),
        1
    );
    let saved = read_game("player-1").unwrap().unwrap();
    assert_eq!(
        saved.first_guess_badge.as_ref().unwrap().status,
        BadgeRewardStatus::Granted
    );
    assert_eq!(
        player_card(&host, &chitt, player_observer()).data["badgeRewardStatus"],
        "granted"
    );
    assert!(
        card(&host, &chitt)["badgeRewardStatus"].is_null(),
        "reward status is private to the winner"
    );

    // Replays, stale in-memory snapshots and a changed setting must not grant any second badge.
    configure(&host, &chitt, "different-badge");
    assert!(guess(&host, &chitt, &answer).error.is_none());
    let mut stale = saved;
    stale.first_guess_badge.as_mut().unwrap().status = BadgeRewardStatus::Ready;
    host.begin_hook(HookClass::Background);
    crate::badges::award(&wire(player_observer()), &mut stale).unwrap();
    assert_eq!(badge_actions(&host).len(), 1);
    assert_eq!(
        read_game("player-1")
            .unwrap()
            .unwrap()
            .first_guess_badge
            .unwrap()
            .badge_id,
        "badge-one-shot"
    );
}

#[test]
fn posted_win_replies_only_mention_a_confirmed_first_guess_reward() {
    let reward_suffix = "Check your badges for your reward!";
    let first_try_prefixes = crate::FIRST_GUESS_WIN_MESSAGES;
    for (enabled, refused, already_held, misses, expected) in [
        (true, false, false, 0, true),
        (true, false, true, 0, true),
        (true, true, false, 0, false),
        (false, false, false, 0, false),
        (true, false, false, 1, false),
    ] {
        let (host, chitt) = setup();
        if enabled {
            configure(&host, &chitt, "badge-1");
        }
        if refused {
            host.refuse_action("grant_badge_to_character", "forbidden", "permission denied");
        }
        let mut observer = player_observer();
        if already_held {
            observer.profile.insert(
                "badges".to_string(),
                json!([{ "id": "badge-1", "name": "One shot" }]),
            );
        }
        start_as(&host, &chitt, &observer);
        for word in six_wrong_guesses().iter().take(misses) {
            assert!(guess_as(&host, &chitt, observer.clone(), word)
                .error
                .is_none());
        }
        let answer = crate::daily_answer(current_day().unwrap()).unwrap();
        assert!(guess_as(&host, &chitt, observer.clone(), &answer)
            .error
            .is_none());
        // Check the actual Poster Action, not just the card's reward-status field.
        let actions = host.actions();
        let reply = actions
            .iter()
            .find(|action| action.action == "post_reply")
            .unwrap();
        let content = reply.args["content"].as_str().unwrap();
        assert_eq!(
            first_try_prefixes
                .iter()
                .any(|phrase| content.contains(phrase)),
            misses == 0,
            "first-try copy must be distinct from ordinary win copy: {content}"
        );
        assert_eq!(content.contains(reward_suffix), expected, "{content}");
        if refused {
            assert!(
                content.contains("The badge could not be assigned:")
                    && content.contains("permission denied"),
                "{content}"
            );
        }
        if !enabled {
            assert!(
                content.contains("No first-guess badge is configured"),
                "{content}"
            );
        }
        assert!(content.contains(&format!("{}/6", misses + 1)));
        assert!(content.ends_with(&["🟩"; crate::WORD_LENGTH].concat()));
        assert!(guess_as(&host, &chitt, observer, &answer).error.is_none());
        assert_eq!(
            host.actions()
                .iter()
                .filter(|action| action.action == "post_reply")
                .count(),
            1
        );
    }
}

#[test]
fn pending_rewards_have_first_try_copy_without_a_successful_badge_claim() {
    let mut game = Game::new(1, "crane".to_string());
    game.complete = true;
    game.won = true;
    game.guesses.push(Guess {
        word: "crane".to_string(),
        marks: vec![2; 5],
    });
    for status in [BadgeRewardStatus::Ready, BadgeRewardStatus::Pending] {
        game.first_guess_badge = Some(crate::badges::BadgeReward {
            badge_id: "badge-1".to_string(),
            status,
            error: None,
        });
        let reply = crate::win_reply(&wire(player_observer()), &game).unwrap();
        assert!(crate::FIRST_GUESS_WIN_MESSAGES
            .iter()
            .any(|phrase| reply.contains(phrase)));
        assert!(!reply.contains("Check your badges for your reward!"));
    }
}

#[test]
fn later_guess_wins_losses_and_unfinished_games_do_not_award_badges() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "badge-1");
    start_game(&host, &chitt);
    assert!(guess(&host, &chitt, &wrong_word()).error.is_none());
    assert!(badge_actions(&host).is_empty());
    let answer = crate::daily_answer(current_day().unwrap()).unwrap();
    assert!(guess(&host, &chitt, &answer).error.is_none());
    assert!(read_game("player-1")
        .unwrap()
        .unwrap()
        .first_guess_badge
        .is_none());
    lose_as(&host, &chitt, &named("loser", "loser", "#a33"));
    assert!(badge_actions(&host).is_empty());
}

#[test]
fn a_refused_badge_does_not_erase_the_win_or_prevent_its_reply() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "badge-1");
    host.refuse_action("grant_badge_to_character", "forbidden", "permission denied");
    win_as(&host, &chitt, &player_observer());
    let game = read_game("player-1").unwrap().unwrap();
    assert!(game.complete && game.won);
    assert_eq!(
        game.first_guess_badge.as_ref().unwrap().status,
        BadgeRewardStatus::Failed
    );
    let output = player_card(&host, &chitt, player_observer());
    assert_eq!(output.data["badgeRewardStatus"], "failed");
    assert!(output.data["badgeRewardError"]
        .as_str()
        .unwrap()
        .contains("permission denied"));
    assert_eq!(output.canvas.unwrap()["won"], true);
    let failed_reply = host
        .actions()
        .into_iter()
        .find(|action| action.action == "post_reply")
        .unwrap();
    assert!(failed_reply.args["content"]
        .as_str()
        .unwrap()
        .contains("The badge could not be assigned"));
    assert!(failed_reply.args["content"]
        .as_str()
        .unwrap()
        .contains("permission denied"));
    host.allow_action("grant_badge_to_character");
    assert!(guess(&host, &chitt, &game.answer).error.is_none());
    assert_eq!(
        badge_actions(&host).len(),
        2,
        "an explicit refusal is retried after a later completed-win submission"
    );
    assert_eq!(
        read_game("player-1")
            .unwrap()
            .unwrap()
            .first_guess_badge
            .unwrap()
            .status,
        BadgeRewardStatus::Granted
    );
    assert_eq!(
        host.actions()
            .iter()
            .filter(|action| action.action == "post_reply")
            .count(),
        2
    );
    assert!(host
        .actions()
        .iter()
        .filter(|action| action.action == "post_reply")
        .any(|action| action.args["content"]
            .as_str()
            .unwrap()
            .contains("badge has now been awarded")));
}

#[test]
fn an_ambiguous_badge_transport_failure_is_reported_and_not_retried_blindly() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "badge-1");
    host.refuse_action(
        "grant_badge_to_character",
        "unavailable",
        "the host lost the action result",
    );
    win_as(&host, &chitt, &player_observer());
    let game = read_game("player-1").unwrap().unwrap();
    let reward = game.first_guess_badge.unwrap();
    assert_eq!(reward.status, BadgeRewardStatus::Pending);
    assert!(reward
        .error
        .as_deref()
        .unwrap()
        .contains("outcome is unknown"));
    let reply = host
        .actions()
        .into_iter()
        .find(|action| action.action == "post_reply")
        .unwrap();
    let content = reply.args["content"].as_str().unwrap();
    assert!(content.contains("reward is pending confirmation"));
    assert!(!content.contains("Check your badges for your reward!"));
    host.allow_action("grant_badge_to_character");
    assert!(guess(&host, &chitt, &game.answer).error.is_none());
    assert_eq!(badge_actions(&host).len(), 1);
    assert!(!host
        .actions()
        .iter()
        .any(|action| action.action == "post_reply"
            && action.args["content"]
                .as_str()
                .unwrap()
                .contains("badge has now been awarded")));
}

#[test]
fn a_refused_win_reply_does_not_prevent_the_badge_award() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "badge-1");
    host.refuse_action("post_reply", "forbidden", "reply refused");
    start_game(&host, &chitt);
    let answer = crate::daily_answer(current_day().unwrap()).unwrap();
    assert!(guess(&host, &chitt, &answer).error.is_some());
    assert_eq!(badge_actions(&host).len(), 1);
    assert_eq!(
        read_game("player-1")
            .unwrap()
            .unwrap()
            .first_guess_badge
            .unwrap()
            .status,
        BadgeRewardStatus::Granted
    );
}

#[test]
fn a_winner_who_already_has_the_badge_is_not_given_a_duplicate_grant() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "badge-1");
    let mut observer = player_observer();
    observer.profile.insert(
        "badges".to_string(),
        json!([{ "id": "badge-1", "name": "One shot" }]),
    );
    win_as(&host, &chitt, &observer);
    assert!(badge_actions(&host).is_empty());
    assert_eq!(
        player_card(&host, &chitt, observer).data["badgeRewardStatus"],
        "granted"
    );
}

#[test]
fn enabling_a_badge_does_not_retroactively_reward_old_or_unconfigured_wins() {
    let (host, chitt) = setup();
    win_as(&host, &chitt, &player_observer());
    let answer = read_game("player-1").unwrap().unwrap().answer;
    configure(&host, &chitt, "badge-1");
    assert!(guess(&host, &chitt, &answer).error.is_none());
    assert!(badge_actions(&host).is_empty());
    // An old-version save without the reward field loads and still does not get a retroactive badge.
    let mut saved = serde_json::to_value(read_game("player-1").unwrap().unwrap()).unwrap();
    saved.as_object_mut().unwrap().remove("first_guess_badge");
    host.put_kv("game:player-1", saved.to_string().as_bytes(), None)
        .unwrap();
    assert!(guess(&host, &chitt, &answer).error.is_none());
    assert!(badge_actions(&host).is_empty());
}

#[test]
fn an_interrupted_claim_is_not_reissued_and_an_unclaimed_win_uses_its_saved_badge() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "original-badge");
    start_game(&host, &chitt);
    let mut game = read_game("player-1").unwrap().unwrap();
    game.won = true;
    game.complete = true;
    game.guesses = vec![Guess {
        word: game.answer.clone(),
        marks: vec![2; game.answer.len()],
    }];
    game.first_guess_badge = crate::badges::snapshot().unwrap();
    host.put_kv("game:player-1", serde_json::to_vec(&game).unwrap(), None)
        .unwrap();
    configure(&host, &chitt, "new-badge");
    assert!(guess(&host, &chitt, &game.answer).error.is_none());
    assert_eq!(badge_actions(&host)[0].args["badgeId"], "original-badge");

    // Simulate a process crash after claiming but before confirming the grant outcome.
    game.first_guess_badge.as_mut().unwrap().status = BadgeRewardStatus::Pending;
    host.put_kv("game:player-1", serde_json::to_vec(&game).unwrap(), None)
        .unwrap();
    assert!(guess(&host, &chitt, &game.answer).error.is_none());
    assert_eq!(badge_actions(&host).len(), 1);
    assert_eq!(
        player_card(&host, &chitt, player_observer()).data["badgeRewardStatus"],
        "pending"
    );
}

#[test]
fn the_setting_survives_rotation_and_upgrade_but_full_reset_clears_it() {
    let (host, chitt) = setup();
    configure(&host, &chitt, "badge-1");
    let timer = host.fire_timer(&chitt, DAILY_TIMER).unwrap();
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(timer)).unwrap().error.is_none());
    upgrade_live_chitt_again(&host, &chitt);
    assert_eq!(host.kv_text(CONFIG_KEY).as_deref(), Some("badge-1"));
    host.begin_hook(HookClass::Control);
    assert!(on_control(wire(testing::control(
        &chitt,
        Some(testing::observer("admin").admin()),
        crate::RESET_CONTROL,
        "reset"
    )))
    .unwrap()
    .error
    .is_none());
    assert!(host.kv_value(CONFIG_KEY).is_none());
    win_as(&host, &chitt, &player_observer());
    assert!(badge_actions(&host).is_empty());
}
