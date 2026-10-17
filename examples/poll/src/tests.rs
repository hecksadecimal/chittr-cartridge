//! The poll's Hooks, run natively against the SDK's fake host (the `testing` feature). The asset and the manifest
//! are the Cartridge's own files, so these tests also check that the vote enum and the options agree.

use chittr_cartridge::testing::{self, ChittRef, HookClass, TestHost};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use crate::pdk::types::*;
use crate::{
    on_canvas_message, on_control, on_reply, on_timer, render, CLOSED_KEY, CLOSE_CONTROL, CLOSE_TIMER, QUESTION,
    VOTE_CONTROL,
};

const TOPPINGS: &[u8] = include_bytes!("../cartridge/assets/toppings.txt");
const MANIFEST: &str = include_str!("../cartridge/manifest.json");

/// The SDK's envelope, sent over the wire the host would use, as the generated Hook type reads it.
fn wire<T: DeserializeOwned>(input: impl Serialize) -> T {
    let value = serde_json::to_value(input).expect("an envelope serializes");
    serde_json::from_value(value).expect("the SDK's envelope matches the schema's type")
}

/// A fresh fake host with the poll's options, and the Chitt the poll is attached to.
fn setup() -> (TestHost, ChittRef) {
    let host = TestHost::new();
    host.add_asset("toppings.txt", TOPPINGS);
    (host, testing::chitt("chitt-1", "poster-1"))
}

fn vote(host: &TestHost, chitt: &ChittRef, who: &str, option: &str) -> Option<HookError> {
    host.begin_hook(HookClass::Control);
    let input = testing::control(chitt, Some(testing::observer(who)), VOTE_CONTROL, option);
    on_control(wire(input)).expect("the Hook returns").error
}

fn close_as_poster(host: &TestHost, chitt: &ChittRef) -> Option<HookError> {
    host.begin_hook(HookClass::Control);
    let poster = testing::observer("poster-1").poster();
    let input = testing::control(chitt, Some(poster), CLOSE_CONTROL, "close");
    on_control(wire(input)).expect("the Hook returns").error
}

/// Runs `on_timer` for the close timer, as the host does when it fires.
fn close_timer(host: &TestHost, chitt: &ChittRef) -> Option<HookError> {
    host.begin_hook(HookClass::Background);
    let input = testing::timer(chitt, CLOSE_TIMER, Value::Null);
    on_timer(wire(input)).expect("the Hook returns").error
}

fn posted(host: &TestHost) -> Vec<String> {
    host.actions()
        .iter()
        .filter(|action| action.action == "post_reply" && action.reply.get("postId").is_some())
        .map(|action| action.args["content"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn announcements(host: &TestHost) -> usize {
    host.actions().iter().filter(|action| action.action == "edit_chitt").count()
}

#[test]
fn the_manifest_vote_enum_matches_the_options() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("the manifest is JSON");
    let enum_values: Vec<&str> = manifest["controls"]["vote"]["value"]["enum"]
        .as_array()
        .expect("the vote Control has an enum")
        .iter()
        .map(|value| value.as_str().expect("an enum value is a string"))
        .collect();
    let labels: Vec<&str> = std::str::from_utf8(TOPPINGS).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let ids: Vec<String> = labels
        .iter()
        .map(|label| label.to_lowercase().split_whitespace().collect::<Vec<_>>().join("-"))
        .collect();
    assert_eq!(enum_values, ids.iter().map(String::as_str).collect::<Vec<_>>());
}

#[test]
fn init_publishes_the_question_and_schedules_the_close_timer() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Lifecycle);
    let output = crate::init(wire(testing::init(&chitt))).expect("the Hook returns");
    assert_eq!(output.content, QUESTION);
    assert!(output.error.is_none());
    let timers = host.timers();
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].name, CLOSE_TIMER);
    assert_eq!(timers[0].due_at, host.now() + 60 * 60);
}

#[test]
fn votes_move_between_options_and_count_once() {
    let (host, chitt) = setup();
    assert!(vote(&host, &chitt, "char-1", "anchovies").is_none());
    assert!(vote(&host, &chitt, "char-1", "anchovies").is_none());
    assert_eq!(host.kv_text("tally:anchovies").as_deref(), Some("1"));
    assert!(vote(&host, &chitt, "char-1", "pineapple").is_none());
    assert_eq!(host.kv_text("tally:anchovies").as_deref(), Some("0"));
    assert_eq!(host.kv_text("tally:pineapple").as_deref(), Some("1"));
    assert!(host.pushes().contains(&chittr_cartridge::push::PushTarget::All));
}

#[test]
fn votes_are_refused_when_logged_out_or_unknown() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Control);
    let logged_out = testing::control(&chitt, None, VOTE_CONTROL, "anchovies");
    assert_eq!(on_control(wire(logged_out)).unwrap().error.unwrap().code, "logged_out");
    assert_eq!(vote(&host, &chitt, "char-1", "ham").unwrap().code, "unknown_option");
}

#[test]
fn render_shows_the_tallies_and_the_viewers_own_vote() {
    let (host, chitt) = setup();
    vote(&host, &chitt, "char-1", "anchovies");
    vote(&host, &chitt, "char-2", "anchovies");
    vote(&host, &chitt, "char-3", "pineapple");

    host.begin_hook(HookClass::Render);
    let input = testing::render(&chitt, Some(testing::observer("char-1")));
    let output = render(wire(input)).expect("the Hook returns");
    assert!(output.error.is_none());
    let options = output.data["options"].as_array().expect("the card lists options");
    let anchovies = options.iter().find(|row| row["id"] == json!("anchovies")).expect("anchovies is listed");
    assert_eq!(anchovies["votes"], json!(2));
    assert_eq!(anchovies["percent"], json!(66));
    assert_eq!(anchovies["voted"], json!(true));
    assert_eq!(output.data["total"], json!(3));
    assert_eq!(output.data["views"], json!(1));
    assert_eq!(output.data["signedIn"], json!(true));
    assert_eq!(output.data["closed"], json!(false));
}

#[test]
fn render_counts_views_and_works_logged_out() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Render);
    let first = render(wire(testing::render(&chitt, None))).unwrap();
    host.begin_hook(HookClass::Render);
    let second = render(wire(testing::render(&chitt, None))).unwrap();
    assert_eq!(first.data["views"], json!(1));
    assert_eq!(second.data["views"], json!(2));
    assert_eq!(second.data["signedIn"], json!(false));
}

#[test]
fn a_canvas_bar_click_replies_with_that_option_tally() {
    let (host, chitt) = setup();
    vote(&host, &chitt, "char-1", "pineapple");
    host.begin_hook(HookClass::Background);
    let input = testing::canvas_message(&chitt, None, json!({ "bar": 0 }), Some(7));
    let output = on_canvas_message(wire(input)).unwrap();
    let reply = output.reply.expect("a bar click gets a reply");
    assert_eq!(reply["label"], json!("Pineapple"));
    assert_eq!(reply["votes"], json!(1));

    host.begin_hook(HookClass::Background);
    let input = testing::canvas_message(&chitt, None, json!({ "bar": 9 }), None);
    assert_eq!(on_canvas_message(wire(input)).unwrap().error.unwrap().code, "unknown_bar");
}

#[test]
fn the_close_timer_announces_the_leader_once() {
    let (host, chitt) = setup();
    vote(&host, &chitt, "char-1", "anchovies");
    vote(&host, &chitt, "char-2", "anchovies");
    vote(&host, &chitt, "char-3", "pineapple");

    assert!(close_timer(&host, &chitt).is_none());
    assert_eq!(posted(&host), ["The poll is closed. Anchovies leads with 2 votes."]);
    assert_eq!(announcements(&host), 1);
    assert_eq!(host.kv_text(CLOSED_KEY).as_deref(), Some("1"));

    // A timer delivered again, or the poster's Control after it, announces nothing more.
    assert!(close_timer(&host, &chitt).is_none());
    assert!(close_as_poster(&host, &chitt).is_none());
    assert_eq!(posted(&host).len(), 1);
    assert_eq!(announcements(&host), 1);
}

#[test]
fn the_poster_close_then_the_timer_announces_once() {
    let (host, chitt) = setup();
    vote(&host, &chitt, "char-1", "pineapple");
    assert!(close_as_poster(&host, &chitt).is_none());
    assert!(close_timer(&host, &chitt).is_none());
    assert_eq!(posted(&host), ["The poll is closed. Pineapple leads with 1 vote."]);
    assert_eq!(announcements(&host), 1);
}

#[test]
fn a_failed_announcement_is_retried_by_the_timer() {
    let (host, chitt) = setup();
    vote(&host, &chitt, "char-1", "extra-cheese");
    host.refuse_action("post_reply", "unavailable", "the poster's account is busy");

    let failed = close_timer(&host, &chitt).expect("the first delivery fails");
    assert_eq!(failed.code, "unavailable");
    assert!(posted(&host).is_empty());

    host.allow_action("post_reply");
    assert!(close_timer(&host, &chitt).is_none());
    assert_eq!(posted(&host), ["The poll is closed. Extra cheese leads with 1 vote."]);
    assert_eq!(announcements(&host), 1);
}

#[test]
fn a_closed_poll_refuses_votes_and_replies() {
    let (host, chitt) = setup();
    assert!(close_timer(&host, &chitt).is_none());
    assert_eq!(vote(&host, &chitt, "char-1", "anchovies").unwrap().code, "poll_closed");

    host.begin_hook(HookClass::Gate);
    let gate = on_reply(wire(testing::reply_gate(&chitt, testing::observer("char-1"), "hi", "chitt-1"))).unwrap();
    assert_eq!(gate.allow, Some(false));
    assert_eq!(gate.message.as_deref(), Some("The poll is closed, so it takes no more replies."));
}

#[test]
fn an_open_poll_lets_replies_through() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Gate);
    let gate = on_reply(wire(testing::reply_gate(&chitt, testing::observer("char-1"), "hi", "chitt-1"))).unwrap();
    assert_eq!(gate.allow, Some(true));
}

#[test]
fn only_the_poster_can_close_and_only_when_logged_in() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Control);
    let stranger = testing::control(&chitt, Some(testing::observer("char-9")), CLOSE_CONTROL, "close");
    assert_eq!(on_control(wire(stranger)).unwrap().error.unwrap().code, "forbidden");
    host.begin_hook(HookClass::Control);
    let anonymous = testing::control(&chitt, None, CLOSE_CONTROL, "close");
    assert_eq!(on_control(wire(anonymous)).unwrap().error.unwrap().code, "logged_out");
    assert!(host.actions().is_empty());
}
