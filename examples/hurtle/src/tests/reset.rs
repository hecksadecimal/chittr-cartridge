use super::*;
use crate::{RESET_BATCH, RESET_CONTROL, RESET_KEY, RESET_LOCK_KEY, RESET_TIMER};

fn reset_as(
    host: &TestHost,
    chitt: &testing::ChittRef,
    observer: Option<chittr_cartridge::envelope::Observer>,
) -> ControlOutput {
    host.begin_hook(HookClass::Control);
    on_control(wire(testing::control(
        chitt,
        observer,
        RESET_CONTROL,
        "reset",
    )))
    .unwrap()
}

fn reset(host: &TestHost, chitt: &testing::ChittRef) {
    let output = reset_as(host, chitt, Some(testing::observer("admin").admin()));
    assert!(output.error.is_none(), "{:?}", output.error);
}

fn assert_fresh_metadata(host: &TestHost, day: i64) {
    let mut expected = vec![
        crate::DAILY_DAY_KEY,
        crate::DAILY_TIMER_READY_KEY,
        crate::TWELVE_FROM_KEY,
    ];
    expected.sort_unstable();
    assert_eq!(host.kv_keys(), expected);
    for key in expected {
        assert_eq!(host.kv_text(key).as_deref(), Some(day.to_string().as_str()));
    }
    let timers = host.timers();
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].name, DAILY_TIMER);
    assert_eq!(timers[0].due_at, host.now() + DAY_SECONDS);
    assert_eq!(timers[0].payload, json!({ "day": day + 1 }));
}

#[test]
fn anonymous_players_and_non_admin_posters_cannot_reset_even_with_a_forged_control() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.put_kv("untouched", b"keep me", None).unwrap();
    for observer in [
        None,
        Some(player_observer()),
        Some(testing::observer("poster-1").poster()),
    ] {
        let output = reset_as(&host, &chitt, observer);
        assert_eq!(output.error.unwrap().code, "forbidden");
        assert_eq!(host.kv_keys(), ["untouched"]);
        assert_eq!(host.kv_text("untouched").as_deref(), Some("keep me"));
        assert!(host.timers().is_empty());
        assert!(host.pushes().is_empty());
        assert!(host.actions().is_empty());
    }
}

#[test]
fn the_reset_button_and_permission_flag_are_admin_only() {
    let (host, chitt) = setup();
    for (observer, visible) in [
        (None, false),
        (Some(player_observer()), false),
        (Some(testing::observer("poster-1").poster()), false),
        (Some(testing::observer("admin").admin()), true),
    ] {
        host.begin_hook(HookClass::Render);
        let output = render(wire(testing::render(&chitt, observer))).unwrap();
        assert!(output.error.is_none());
        assert_eq!(output.data["isAdmin"], visible);
        assert_eq!(output.data["resetting"], false);
    }
    let template = include_str!("../../cartridge/templates/card.liquid");
    let before_button = template
        .split("data-lc-control=\"reset-day\"")
        .next()
        .unwrap();
    assert!(
        before_button.rfind("{% if isAdmin %}").unwrap()
            > before_button.rfind("{% endif %}").unwrap()
    );
    assert!(template.contains("data-lc-value=\"reset\""));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../cartridge/manifest.json")).unwrap();
    assert_eq!(
        manifest["controls"][RESET_CONTROL]["value"]["enum"],
        json!(["reset"])
    );
}

#[test]
fn an_invalid_admin_control_value_changes_nothing() {
    let (host, chitt) = setup();
    let before = host.kv_keys();
    let timers = serde_json::to_value(&host.timers()[0].payload).unwrap();
    host.begin_hook(HookClass::Control);
    let output = on_control(wire(testing::control(
        &chitt,
        Some(testing::observer("admin").admin()),
        RESET_CONTROL,
        "start",
    )))
    .unwrap();
    assert_eq!(output.error.unwrap().code, "unknown_control");
    assert_eq!(host.kv_keys(), before);
    assert_eq!(
        serde_json::to_value(&host.timers()[0].payload).unwrap(),
        timers
    );
}

#[test]
fn an_admin_reset_removes_all_old_kv_and_opens_the_next_word_for_everyone() {
    let (host, chitt) = setup();
    let day = current_day().unwrap();
    let answer = crate::daily_answer(day).unwrap();
    lose_as(&host, &chitt, &named("loser", "victim", "#a33"));
    win_as(&host, &chitt, &named("winner", "survivor", "#3a3"));
    start_game(&host, &chitt);
    host.put_kv("unknown-old-version-key", b"also gone", None)
        .unwrap();
    host.put_kv("roll:1", b"old roll", None).unwrap();
    let actions = host.actions().len();
    host.advance(3600);

    reset(&host, &chitt);

    assert_fresh_metadata(&host, day + 1);
    assert_ne!(crate::daily_answer(day + 1).unwrap(), answer);
    assert_eq!(
        host.actions().len(),
        actions,
        "no unsuspension or reply-deletion actions"
    );
    assert_eq!(
        host.pushes().last(),
        Some(&chittr_cartridge::push::PushTarget::All)
    );
    for player in ["loser", "winner", "player-1"] {
        assert!(read_game(player).unwrap().is_none());
        host.begin_hook(HookClass::Render);
        let output = render(wire(testing::render(
            &chitt,
            Some(testing::observer(player)),
        )))
        .unwrap();
        assert_eq!(output.data["started"], false);
        assert_eq!(output.data["obituaryCount"], 0);
        assert_eq!(output.data["survivorsCount"], 0);
        assert_eq!(
            output.canvas.unwrap()["blood"],
            json!({ "total": 0, "fallen": 0, "stains": [] })
        );
    }
    start_game(&host, &chitt);
    let game = read_game("player-1").unwrap().unwrap();
    assert_eq!(game.day, day + 1);
    assert!(game.guesses.is_empty());
    // A second intentional reset advances one more day, not back to the creation date.
    reset(&host, &chitt);
    assert_fresh_metadata(&host, day + 2);
}

#[test]
fn resetting_a_legacy_day_switches_to_twelve_letters_and_old_timers_do_not_postpone_it() {
    let (host, chitt) = setup();
    upgrade_live_chitt(&host, &chitt);
    let day = current_day().unwrap();
    assert_eq!(crate::daily_answer(day).unwrap().len(), 5);
    // The host claimed this delivery, but an admin resets before it reaches the guest.
    let stale = serde_json::to_value(host.fire_timer(&chitt, DAILY_TIMER).unwrap()).unwrap();
    reset(&host, &chitt);
    assert_eq!(crate::daily_answer(day + 1).unwrap().len(), 12);
    let deadline = host.timers()[0].due_at;
    host.advance(123);
    for _ in 0..2 {
        host.begin_hook(HookClass::Background);
        assert!(on_timer(wire(stale.clone())).unwrap().error.is_none());
        assert_eq!(current_day().unwrap(), day + 1);
        assert_eq!(host.timers()[0].due_at, deadline);
    }
    // The new timer still advances normally, and its own duplicate is harmless too.
    let next = serde_json::to_value(host.fire_timer(&chitt, DAILY_TIMER).unwrap()).unwrap();
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(next.clone())).unwrap().error.is_none());
    assert_eq!(current_day().unwrap(), day + 2);
    assert_eq!(
        host.kv_text(crate::TWELVE_FROM_KEY),
        Some((day + 1).to_string())
    );
    assert_eq!(host.timers()[0].due_at, host.now() + DAY_SECONDS);
    let deadline = host.timers()[0].due_at;
    host.advance(30);
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(next)).unwrap().error.is_none());
    assert_eq!(host.timers()[0].due_at, deadline);
}

#[test]
fn a_large_reset_clears_past_the_listing_cap_and_blocks_play_until_finished() {
    let (host, chitt) = setup();
    start_game(&host, &chitt);
    let day = current_day().unwrap();
    let old_daily = serde_json::to_value(host.fire_timer(&chitt, DAILY_TIMER).unwrap()).unwrap();
    for index in 0..1300 {
        host.put_kv(&format!("legacy:{index:04}"), b"old", None)
            .unwrap();
    }
    let before = host.kv_keys().len();
    reset(&host, &chitt);
    assert_eq!(host.kv_keys().len(), before - RESET_BATCH + 1);
    assert_eq!(crate::reset_day().unwrap(), Some(day + 1));
    assert_eq!(host.timers()[0].name, RESET_TIMER);
    for observer in [
        None,
        Some(player_observer()),
        Some(testing::observer("admin").admin()),
    ] {
        let is_admin = observer.as_ref().is_some_and(|o| o.is_admin);
        host.begin_hook(HookClass::Render);
        let output = render(wire(testing::render(&chitt, observer))).unwrap();
        assert!(output.error.is_none());
        assert_eq!(output.data["resetting"], true);
        assert_eq!(output.data["isAdmin"], is_admin);
        assert_eq!(output.canvas.unwrap()["started"], false);
    }
    host.begin_hook(HookClass::Control);
    let start = on_control(wire(testing::control(
        &chitt,
        Some(player_observer()),
        "start",
        "start",
    )))
    .unwrap();
    assert_eq!(start.error.unwrap().code, "reset_in_progress");
    assert_eq!(
        guess(&host, &chitt, "abbreviation").error.unwrap().code,
        "reset_in_progress"
    );
    assert_eq!(
        send_seen(&host, &chitt, player_observer())
            .error
            .unwrap()
            .code,
        "reset_in_progress"
    );
    assert_eq!(
        reset_as(&host, &chitt, Some(player_observer()))
            .error
            .unwrap()
            .code,
        "forbidden"
    );
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(old_daily)).unwrap().error.is_none());
    assert!(!host.timers().iter().any(|timer| timer.name == DAILY_TIMER));

    // Clicking during cleanup resumes the same day, rather than advancing again.
    reset(&host, &chitt);
    assert_eq!(crate::reset_day().unwrap(), Some(day + 1));
    let mut last_delivery = None;
    for _ in 0..12 {
        if crate::reset_day().unwrap().is_none() {
            break;
        }
        let timer = serde_json::to_value(host.fire_timer(&chitt, RESET_TIMER).unwrap()).unwrap();
        host.begin_hook(HookClass::Background);
        let output = on_timer(wire(timer.clone())).unwrap();
        assert!(output.error.is_none(), "{:?}", output.error);
        last_delivery = Some(timer);
    }
    assert_fresh_metadata(&host, day + 1);
    start_game(&host, &chitt);
    let game = host.kv_value("game:player-1");
    let deadline = host.timers()[0].due_at;
    // A duplicate cleanup delivery after reopening must not erase a new attempt.
    host.advance(30);
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(last_delivery.unwrap()))
        .unwrap()
        .error
        .is_none());
    assert_eq!(host.kv_value("game:player-1"), game);
    assert_eq!(host.timers()[0].due_at, deadline);
}

#[test]
fn an_interrupted_reset_preserves_its_target_and_can_resume_through_migration() {
    let (host, chitt) = setup();
    let target = current_day().unwrap() + 1;
    // Simulate a crash after partial final initialization, with the durable marker still present.
    host.put_kv(RESET_KEY, target.to_string().as_bytes(), None)
        .unwrap();
    host.put_kv(crate::DAILY_DAY_KEY, target.to_string().as_bytes(), None)
        .unwrap();
    host.put_kv("unknown", b"unfinished", None).unwrap();
    host.put_kv(RESET_LOCK_KEY, b"working", Some(30)).unwrap();
    let before = host.kv_keys();
    let output = reset_as(&host, &chitt, Some(testing::observer("admin").admin()));
    assert_eq!(output.error.unwrap().code, "reset_in_progress");
    assert_eq!(
        host.kv_keys(),
        before,
        "a second worker cannot delete under the first"
    );
    host.advance(31);
    upgrade_live_chitt_again(&host, &chitt);
    assert_fresh_metadata(&host, target);
}

#[test]
fn a_storage_failure_mid_cleanup_recovers_without_advancing_twice() {
    let (host, chitt) = setup();
    let target = current_day().unwrap() + 1;
    for index in 0..40 {
        host.put_kv(&format!("legacy:{index}"), b"old", None).unwrap();
    }
    // Exhaust the fake host's call budget partway through deletion. This also prevents releasing
    // the worker lease, just as a crashed Hook would. The recovery timer and target must survive.
    host.begin_hook(HookClass::Control);
    for _ in 0..480 {
        chittr_cartridge::kv::get("absent").unwrap();
    }
    let output = on_control(wire(testing::control(
        &chitt,
        Some(testing::observer("admin").admin()),
        RESET_CONTROL,
        "reset",
    )))
    .unwrap();
    assert_eq!(output.error.unwrap().code, "storage_error");
    assert_eq!(host.kv_text(RESET_KEY), Some(target.to_string()));
    assert!(host.kv_keys().len() < 45, "some old data was already deleted");
    let recovery = host.fire_timer(&chitt, RESET_TIMER).unwrap();
    host.advance(31);
    host.begin_hook(HookClass::Background);
    assert!(on_timer(wire(recovery)).unwrap().error.is_none());
    assert_fresh_metadata(&host, target);
}

#[test]
fn a_suspended_admin_can_reset_but_still_cannot_play() {
    let (host, chitt) = setup();
    let mut admin = testing::observer("admin").admin();
    admin.is_suspended = true;
    assert!(reset_as(&host, &chitt, Some(admin.clone())).error.is_none());
    host.begin_hook(HookClass::Control);
    let output = on_control(wire(testing::control(
        &chitt,
        Some(admin),
        "start",
        "start",
    )))
    .unwrap();
    assert_eq!(output.error.unwrap().code, "character_suspended");
}
