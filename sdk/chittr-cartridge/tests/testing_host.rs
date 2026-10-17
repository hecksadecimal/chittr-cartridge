//! The `testing` feature's fake host: the SDK's own host calls, run against it. The fake must keep the Chittr host's
//! rules, so these tests pin them: TTLs, the write-and-keep-expiry rule, the lost-race answers, quotas, the caps.
#![cfg(feature = "testing")]

use chittr_cartridge::envelope::GateResult;
use chittr_cartridge::kv;
use chittr_cartridge::logging::LogLevel;
use chittr_cartridge::poster::{self, EditArgs};
use chittr_cartridge::push::{self, PushTarget};
use chittr_cartridge::testing::{self, HookClass, TestHost};
use chittr_cartridge::timer;
use serde_json::json;

#[test]
fn set_get_and_delete_round_trip() {
    let host = TestHost::new();
    kv::set("greeting", b"hello", None).unwrap();
    let entry = kv::get("greeting").unwrap().expect("the key is set");
    assert_eq!(entry.value, b"hello");
    assert_eq!(entry.ttl_seconds, None);
    assert_eq!(host.kv_text("greeting").as_deref(), Some("hello"));
    assert!(kv::delete("greeting").unwrap());
    assert!(!kv::delete("greeting").unwrap());
    assert_eq!(kv::get("greeting").unwrap(), None);
}

#[test]
fn ttl_expires_and_a_write_without_one_keeps_the_expiry() {
    let host = TestHost::new();
    kv::set("session", b"a", Some(60)).unwrap();
    host.advance(20);
    assert_eq!(kv::get("session").unwrap().unwrap().ttl_seconds, Some(40));

    kv::set("session", b"b", None).unwrap();
    assert_eq!(kv::get("session").unwrap().unwrap().ttl_seconds, Some(40));

    kv::set("session", b"c", Some(100)).unwrap();
    assert_eq!(kv::get("session").unwrap().unwrap().ttl_seconds, Some(100));

    host.advance(100);
    assert_eq!(kv::get("session").unwrap(), None);
    assert!(host.kv_keys().is_empty());
}

#[test]
fn set_if_absent_loses_to_a_live_value_but_not_an_expired_one() {
    let host = TestHost::new();
    assert!(kv::set_if_absent("lock", b"first", Some(30)).unwrap());
    assert!(!kv::set_if_absent("lock", b"second", None).unwrap());
    assert_eq!(host.kv_text("lock").as_deref(), Some("first"));
    host.advance(30);
    assert!(kv::set_if_absent("lock", b"third", None).unwrap());
    assert_eq!(host.kv_text("lock").as_deref(), Some("third"));
}

#[test]
fn compare_and_swap_checks_the_live_value_and_keeps_the_expiry() {
    let host = TestHost::new();
    kv::set("slot", b"one", Some(50)).unwrap();
    assert!(!kv::compare_and_swap("slot", Some(b"nope"), b"two").unwrap());
    assert!(kv::compare_and_swap("slot", Some(b"one"), b"two").unwrap());
    assert_eq!(host.kv_text("slot").as_deref(), Some("two"));
    assert_eq!(host.kv_ttl("slot"), Some(50));
    // `None` means no live value: it wins on a missing key and loses on a present one.
    assert!(kv::compare_and_swap("fresh", None, b"x").unwrap());
    assert!(!kv::compare_and_swap("fresh", None, b"y").unwrap());
}

#[test]
fn incr_counts_from_zero_and_refuses_text() {
    let host = TestHost::new();
    assert_eq!(kv::incr("views", 1, None).unwrap(), 1);
    assert_eq!(kv::incr("views", 4, None).unwrap(), 5);
    assert_eq!(kv::incr("views", -7, None).unwrap(), -2);
    assert_eq!(host.kv_text("views").as_deref(), Some("-2"));
    kv::set("name", b"pizza", None).unwrap();
    let error = kv::incr("name", 1, None).unwrap_err();
    assert_eq!(error.code, "not_integer");
    let error = kv::incr("views", (1i64 << 53) + 1, None).unwrap_err();
    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn list_and_get_all_are_sorted_and_cap_at_1000() {
    let host = TestHost::new();
    for index in 0..1005 {
        host.put_kv(&format!("row:{index:04}"), b"v", None).unwrap();
    }
    host.put_kv("other", b"v", None).unwrap();
    let listing = kv::list("row:").unwrap();
    assert_eq!(listing.keys.len(), 1000);
    assert!(listing.truncated);
    assert_eq!(listing.keys[0], "row:0000");
    let snapshot = kv::get_all().unwrap();
    assert_eq!(snapshot.entries.len(), 1000);
    assert!(snapshot.truncated);
}

#[test]
fn get_many_lines_up_with_its_keys() {
    let _host = TestHost::new();
    kv::set("a", b"1", None).unwrap();
    kv::set("c", b"3", None).unwrap();
    let values = kv::get_many(&["a", "b", "c"]).unwrap();
    assert_eq!(values, vec![Some(b"1".to_vec()), None, Some(b"3".to_vec())]);
}

#[test]
fn key_and_value_sizes_are_checked() {
    let _host = TestHost::new();
    let long_key = "k".repeat(1025);
    assert_eq!(
        kv::set(&long_key, b"v", None).unwrap_err().code,
        "key_too_large"
    );
    let big = vec![0u8; 1024 * 1024 + 1];
    assert_eq!(
        kv::set("big", &big, None).unwrap_err().code,
        "value_too_large"
    );
    assert_eq!(
        kv::set("ttl", b"v", Some(0)).unwrap_err().code,
        "invalid_argument"
    );
}

#[test]
fn timers_fire_once_and_replace_by_name() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    timer::schedule_timer("close", 3600, json!({ "why": "deadline" })).unwrap();
    timer::schedule_timer("close", 60, json!({ "why": "sooner" })).unwrap();
    assert_eq!(host.timers().len(), 1);
    assert!(host.fire_timer(&chitt, "missing").is_none());

    let input = host
        .fire_timer(&chitt, "close")
        .expect("the timer is pending");
    assert_eq!(input.name, "close");
    assert_eq!(input.payload, json!({ "why": "sooner" }));
    assert_eq!(input.scheduled_for, "2023-11-14T22:14:20Z");
    assert_eq!(host.now(), 1_700_000_060);
    assert!(host.timers().is_empty());
    assert!(!timer::cancel_timer("close").unwrap());
}

#[test]
fn timers_check_delay_and_outstanding_count() {
    let host = TestHost::new();
    assert_eq!(
        timer::schedule_timer("soon", 9, json!(null))
            .unwrap_err()
            .code,
        "invalid_argument"
    );
    assert_eq!(
        timer::schedule_timer("forever", 30 * 24 * 60 * 60 + 1, json!(null))
            .unwrap_err()
            .code,
        "invalid_argument"
    );
    for index in 0..100 {
        timer::schedule_timer(&format!("t{index}"), 10, json!(null)).unwrap();
    }
    assert_eq!(
        timer::schedule_timer("one-too-many", 10, json!(null))
            .unwrap_err()
            .code,
        "quota_exceeded"
    );
    assert_eq!(host.timers().len(), 100);
    assert!(timer::cancel_timer("t0").unwrap());
}

#[test]
fn fire_due_timers_returns_the_ones_that_are_due() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    timer::schedule_timer("early", 10, json!("a")).unwrap();
    timer::schedule_timer("late", 500, json!("b")).unwrap();
    host.advance(10);
    let due = host.fire_due_timers(&chitt);
    assert_eq!(
        due.iter()
            .map(|input| input.name.as_str())
            .collect::<Vec<_>>(),
        ["early"]
    );
    assert_eq!(host.timers().len(), 1);
}

#[test]
fn render_pushes_go_to_everyone_or_one_character() {
    let host = TestHost::new();
    push::push_render(PushTarget::All).unwrap();
    push::push_render(PushTarget::All).unwrap();
    push::push_render(PushTarget::Character("char-2".to_string())).unwrap();
    assert_eq!(
        host.pushes(),
        vec![
            PushTarget::All,
            PushTarget::All,
            PushTarget::Character("char-2".to_string())
        ]
    );
}

#[test]
fn render_pushes_are_refused_in_render_and_capped_per_call() {
    let host = TestHost::new();
    host.begin_hook(HookClass::Render);
    assert_eq!(
        push::push_render(PushTarget::All).unwrap_err().code,
        "not_allowed"
    );

    host.begin_hook(HookClass::Background);
    for index in 0..20 {
        push::push_render(PushTarget::Character(format!("c{index}"))).unwrap();
    }
    assert_eq!(
        push::push_render(PushTarget::Character("c20".to_string()))
            .unwrap_err()
            .code,
        "quota_exceeded"
    );
    // A repeat of a target already queued in this call is free.
    push::push_render(PushTarget::Character("c3".to_string())).unwrap();
    host.begin_hook(HookClass::Background);
    push::push_render(PushTarget::Character("c20".to_string())).unwrap();
}

#[test]
fn host_calls_are_capped_per_hook_call() {
    let host = TestHost::new();
    host.begin_hook(HookClass::Render);
    for _ in 0..100 {
        kv::get("probe").unwrap();
    }
    assert_eq!(kv::get("probe").unwrap_err().code, "rate_limited");
    host.begin_hook(HookClass::Background);
    assert!(kv::get("probe").is_ok());
}

#[test]
fn poster_actions_record_and_return_the_host_replies() {
    let host = TestHost::new();
    host.begin_hook(HookClass::Background);
    let reply = poster::post_reply("chitt-1", "the poll is closed").unwrap();
    assert_eq!(reply.as_deref(), Some("post-1"));
    poster::edit_chitt(EditArgs {
        content: "Closed.".to_string(),
        ..Default::default()
    })
    .unwrap();
    let actions = host.actions();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].action, "post_reply");
    assert_eq!(
        actions[0].args,
        json!({ "postId": "chitt-1", "content": "the poll is closed" })
    );
    assert_eq!(actions[1].action, "edit_chitt");
}

#[test]
fn poster_action_quotas_apply_per_call_and_per_minute() {
    let host = TestHost::new();
    host.begin_hook(HookClass::Render);
    poster::react("chitt-1", poster::Reaction::Like).unwrap();
    assert_eq!(
        poster::react("chitt-1", poster::Reaction::Hate)
            .unwrap_err()
            .code,
        "rate_limited"
    );

    host.begin_hook(HookClass::Background);
    for _ in 0..5 {
        poster::follow("char-1").unwrap();
    }
    assert_eq!(poster::follow("char-1").unwrap_err().code, "rate_limited");

    // Thirty actions in the last minute, across calls (the render's one, the five above, then 24 more), then the
    // window moves on.
    for _ in 0..4 {
        host.begin_hook(HookClass::Background);
        for _ in 0..5 {
            poster::unfollow("char-1").unwrap();
        }
    }
    host.begin_hook(HookClass::Background);
    for _ in 0..4 {
        poster::unfollow("char-1").unwrap();
    }
    host.begin_hook(HookClass::Background);
    assert_eq!(poster::unfollow("char-1").unwrap_err().code, "rate_limited");
    host.advance(61);
    host.begin_hook(HookClass::Background);
    assert!(poster::unfollow("char-1").is_ok());
}

#[test]
fn lifecycle_and_gate_actions_are_deferred() {
    let host = TestHost::new();
    host.begin_hook(HookClass::Lifecycle);
    assert_eq!(poster::post_chitt("hello").unwrap(), None);
    host.begin_hook(HookClass::Gate);
    assert_eq!(poster::post_reply("chitt-1", "later").unwrap(), None);
    assert!(host
        .actions()
        .iter()
        .all(|action| action.reply["deferred"] == json!(true)));
}

#[test]
fn scripted_refusals_come_back_as_errors() {
    let host = TestHost::new();
    host.begin_hook(HookClass::Background);
    host.refuse_action("ban_user", "forbidden", "no permission");
    let error = poster::ban_user("user-9", poster::BanDuration::OneHour, "spam").unwrap_err();
    assert_eq!(error.code, "forbidden");
    assert_eq!(error.message, "no permission");
}

#[test]
fn assets_come_from_the_host_table() {
    let host = TestHost::new();
    host.add_asset("toppings.txt", b"Pineapple\nAnchovies\n");
    let bytes = chittr_cartridge::host::asset("toppings.txt").unwrap();
    assert_eq!(bytes, b"Pineapple\nAnchovies\n");
    assert_eq!(
        chittr_cartridge::host::asset("missing.txt")
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        chittr_cartridge::host::asset("../secret").unwrap_err().code,
        "not_found"
    );
    host.add_asset("big.bin", vec![0u8; 4 * 1024 * 1024 + 1]);
    assert_eq!(
        chittr_cartridge::host::asset("big.bin").unwrap_err().code,
        "too_large"
    );
}

#[test]
fn logs_are_recorded_and_cut_to_512_bytes() {
    let host = TestHost::new();
    chittr_cartridge::log!(LogLevel::Info, "rendering {}", "chitt-1");
    chittr_cartridge::log!(LogLevel::Warn, "{}", "x".repeat(600));
    let logs = host.logs();
    assert_eq!(logs[0].message, "rendering chitt-1");
    assert_eq!(logs[1].message.len(), 512);
}

#[test]
fn envelope_builders_fill_in_the_wire_fields() {
    let chitt = testing::chitt("chitt-1", "poster-1");
    let observer = testing::observer("char-1").poster();
    let vote = testing::control(&chitt, Some(observer.clone()), "vote", "anchovies");
    let wire = serde_json::to_value(&vote).unwrap();
    assert_eq!(wire["v"], json!(1));
    assert_eq!(wire["hook"], json!("on_control"));
    assert_eq!(wire["control"], json!("vote"));
    assert_eq!(wire["value"], json!("anchovies"));
    assert_eq!(wire["observer"]["isPoster"], json!(true));
    assert_eq!(wire["chitt"]["posterId"], json!("poster-1"));

    let gate = testing::reply_gate(&chitt, observer, "hi", "chitt-1");
    let gate_wire = serde_json::to_value(&gate).unwrap();
    assert_eq!(gate_wire["characterId"], json!("char-1"));
    assert_eq!(gate_wire["replyToPostId"], json!("chitt-1"));
    assert_eq!(gate_wire["isOoc"], json!(false));

    let render = testing::render(&chitt, None);
    assert_eq!(
        serde_json::to_value(&render).unwrap()["observer"],
        json!(null)
    );
}

#[test]
fn timer_envelopes_carry_the_fake_clock() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.set_now(0);
    let input = testing::timer(&chitt, "close", json!(null));
    assert_eq!(input.scheduled_for, "1970-01-01T00:00:00Z");
    host.set_now(1_700_000_000);
    let input = testing::timer(&chitt, "close", json!(null));
    assert_eq!(input.scheduled_for, "2023-11-14T22:13:20Z");
}

#[test]
fn gate_results_are_plain_values() {
    assert_eq!(GateResult::deny("no").allow, Some(false));
}
