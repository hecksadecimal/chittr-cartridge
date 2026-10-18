//! The poll's Hooks, run natively against the SDK's fake host (the `testing` feature). These tests cover admin setup,
//! the waiting state, dynamic answer buttons in the Canvas Guest, voting, and the close timer.

use chittr_cartridge::testing::{self, ChittRef, HookClass, TestHost};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::pdk::types::*;
use crate::{
    on_canvas_message, on_control, on_reply, on_timer, render, CLOSED_KEY, CLOSE_CONTROL,
    CLOSE_TIMER, CONFIGURE_CONTROL, CONFIG_KEY, WAITING_MESSAGE,
};

const QUESTION: &str = "Which pizza topping is best?";
const READY_CONFIG: &str = r#"{"question":"Which pizza topping is best?","options":[{"id":"pineapple","label":"Pineapple"},{"id":"anchovies","label":"Anchovies"},{"id":"extra-cheese","label":"Extra cheese"}]}"#;
const MANIFEST: &str = include_str!("../cartridge/manifest.json");

/// The SDK's envelope, sent over the wire the host would use, as the generated Hook type reads it.
fn wire<T: DeserializeOwned>(input: impl serde::Serialize) -> T {
    let value = serde_json::to_value(input).expect("an envelope serializes");
    serde_json::from_value(value).expect("the SDK's envelope matches the schema's type")
}

fn bare_setup() -> (TestHost, ChittRef) {
    (TestHost::new(), testing::chitt("chitt-1", "poster-1"))
}

/// A ready poll, without setup-side effects.
fn setup() -> (TestHost, ChittRef) {
    let (host, chitt) = bare_setup();
    host.put_kv(CONFIG_KEY, READY_CONFIG, None).unwrap();
    (host, chitt)
}

fn configure_as(
    host: &TestHost,
    chitt: &ChittRef,
    who: Option<chittr_cartridge::envelope::Observer>,
    fields: Value,
) -> Option<HookError> {
    host.begin_hook(HookClass::Control);
    let input = testing::control_fields(
        chitt,
        who,
        CONFIGURE_CONTROL,
        fields.as_object().expect("fields are an object").clone(),
    );
    on_control(wire(input)).expect("the Hook returns").error
}

fn configure(host: &TestHost, chitt: &ChittRef) -> Option<HookError> {
    configure_as(
        host,
        chitt,
        Some(testing::observer("admin").admin()),
        json!({ "question": QUESTION, "options": "Pineapple\nAnchovies\nExtra cheese" }),
    )
}

fn vote(host: &TestHost, chitt: &ChittRef, who: &str, option: &str) -> Option<HookError> {
    host.begin_hook(HookClass::Background);
    let input = testing::canvas_message(
        chitt,
        Some(testing::observer(who)),
        json!({ "optionId": option }),
        Some(1),
    );
    on_canvas_message(wire(input))
        .expect("the Hook returns")
        .error
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

fn card(
    host: &TestHost,
    chitt: &ChittRef,
    observer: Option<chittr_cartridge::envelope::Observer>,
) -> RenderOutput {
    host.begin_hook(HookClass::Render);
    render(wire(testing::render(chitt, observer))).expect("the Hook returns")
}

fn posted(host: &TestHost) -> Vec<String> {
    host.actions()
        .iter()
        .filter(|action| action.action == "post_reply" && action.reply.get("postId").is_some())
        .map(|action| {
            action.args["content"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

fn announcements(host: &TestHost) -> usize {
    host.actions()
        .iter()
        .filter(|action| action.action == "edit_chitt")
        .count()
}

#[test]
fn init_posts_the_waiting_message_without_starting_the_timer() {
    let (host, chitt) = bare_setup();
    host.begin_hook(HookClass::Lifecycle);
    let output = crate::init(wire(testing::init(&chitt))).expect("the Hook returns");
    assert_eq!(output.content, WAITING_MESSAGE);
    assert!(output.error.is_none());
    assert!(host.timers().is_empty());
}

#[test]
fn an_unconfigured_poll_shows_the_waiting_message_and_admin_setup_form_state() {
    let (host, chitt) = bare_setup();
    let visitor = card(&host, &chitt, Some(testing::observer("visitor")));
    assert_eq!(visitor.data["ready"], json!(false));
    assert_eq!(visitor.data["waitMessage"], json!(WAITING_MESSAGE));
    assert_eq!(visitor.canvas, None);
    assert!(host.kv_value("views").is_none());

    let admin = card(&host, &chitt, Some(testing::observer("admin").admin()));
    assert_eq!(admin.data["isAdmin"], json!(true));
    assert_eq!(admin.data["ready"], json!(false));
}

#[test]
fn a_close_timer_delivered_before_setup_does_nothing() {
    let (host, chitt) = bare_setup();
    assert!(close_timer(&host, &chitt).is_none());
    assert!(host.kv_value(CLOSED_KEY).is_none());
    assert!(host.actions().is_empty());
}

#[test]
fn only_a_site_admin_can_configure_the_question_and_setup_starts_its_timer() {
    let (host, chitt) = bare_setup();
    let before = host.kv_keys();
    for observer in [
        None,
        Some(testing::observer("visitor")),
        Some(testing::observer("poster-1").poster()),
    ] {
        let error = configure_as(&host, &chitt, observer, json!({ "question": QUESTION }))
            .expect("non-admin configuration is refused");
        assert_eq!(error.code, "forbidden");
        assert_eq!(host.kv_keys(), before);
        assert!(host.timers().is_empty());
        assert!(host.pushes().is_empty());
    }

    assert!(configure(&host, &chitt).is_none());
    let saved: Value = serde_json::from_str(&host.kv_text(CONFIG_KEY).unwrap()).unwrap();
    assert_eq!(saved["question"], json!(QUESTION));
    assert_eq!(saved["options"].as_array().unwrap().len(), 3);
    let edit = host
        .actions()
        .into_iter()
        .find(|action| action.action == "edit_chitt")
        .expect("setting the poll updates the Chitt text");
    assert_eq!(edit.args["content"], json!(QUESTION));
    let timers = host.timers();
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].name, CLOSE_TIMER);
    assert_eq!(timers[0].due_at, host.now() + 60 * 60);
    assert!(host
        .pushes()
        .contains(&chittr_cartridge::push::PushTarget::All));

    let ready = card(&host, &chitt, Some(testing::observer("visitor")));
    assert_eq!(ready.data["ready"], json!(true));
    assert_eq!(ready.data["question"], json!(QUESTION));
}

#[test]
fn a_failed_chitt_edit_rolls_back_the_new_poll_and_timer() {
    let (host, chitt) = bare_setup();
    host.refuse_action("edit_chitt", "unavailable", "the Chitt could not be edited");
    let error = configure(&host, &chitt).expect("a refused edit fails setup");
    assert_eq!(error.code, "unavailable");
    assert!(host.kv_value(CONFIG_KEY).is_none());
    assert!(host.timers().is_empty());
}

#[test]
fn migration_refreshes_the_chitt_body_from_saved_poll_details() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Lifecycle);
    let output = crate::migrate(wire(testing::migrate(
        &chitt,
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "0.1.0".to_string(),
            sha256: "old".to_string(),
        },
        chittr_cartridge::envelope::CartridgeVersionRef {
            version: "1.0.0".to_string(),
            sha256: "new".to_string(),
        },
    )))
    .unwrap();
    assert!(output.error.is_none());
    let edit = host
        .actions()
        .pop()
        .expect("migration queues the body edit");
    assert_eq!(edit.action, "edit_chitt");
    assert_eq!(edit.args["content"], json!(QUESTION));
    assert_eq!(edit.reply["deferred"], json!(true));
}

#[test]
fn admin_choices_render_on_the_canvas_and_are_votable_by_id() {
    let (host, chitt) = bare_setup();
    let error = configure_as(
        &host,
        &chitt,
        Some(testing::observer("admin").admin()),
        json!({ "question": "Choose a color", "options": "Blue\nGreen\nGold" }),
    );
    assert!(error.is_none());

    let card = card(&host, &chitt, Some(testing::observer("voter")));
    let canvas = card.canvas.expect("the poll draws its choices in a canvas");
    assert_eq!(canvas["options"][0]["label"], json!("Blue"));
    assert_eq!(canvas["options"][2]["id"], json!("gold"));
    assert_eq!(card.data["canvasHeight"], json!(3 * 64 + 32));
    assert!(vote(&host, &chitt, "voter", "green").is_none());
    assert_eq!(host.kv_text("tally:green").as_deref(), Some("1"));
}

#[test]
fn question_configuration_validates_and_is_one_time() {
    let (host, chitt) = bare_setup();
    for fields in [
        json!({}),
        json!({ "question": " " }),
        json!({ "question": "x".repeat(201), "options": "A\nB" }),
        json!({ "question": QUESTION, "options": "One\nOne" }),
        json!({ "question": QUESTION, "options": "Only one" }),
        json!({ "question": QUESTION, "options": "A\nB", "extra": "field" }),
    ] {
        let error = configure_as(
            &host,
            &chitt,
            Some(testing::observer("admin").admin()),
            fields,
        )
        .unwrap();
        assert_eq!(error.code, "bad_input");
        assert!(host.kv_value(CONFIG_KEY).is_none());
        assert!(host.timers().is_empty());
    }

    assert!(configure(&host, &chitt).is_none());
    let error = configure_as(
        &host,
        &chitt,
        Some(testing::observer("admin").admin()),
        json!({ "question": "Another question?", "options": "A\nB" }),
    )
    .unwrap();
    assert_eq!(error.code, "poll_already_ready");
    assert_eq!(host.timers().len(), 1);
    assert_eq!(card(&host, &chitt, None).data["question"], json!(QUESTION));
}

#[test]
fn the_canvas_renders_dynamic_answers_without_html_dropdowns() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("the manifest is JSON");
    assert_eq!(
        manifest["controls"]["configure"]["fields"]["question"]["maxLength"],
        200
    );
    assert_eq!(
        manifest["controls"]["configure"]["fields"]["options"]["maxLength"],
        1000
    );
    assert!(manifest["controls"].get("vote").is_none());
    let template = include_str!("../cartridge/templates/card.liquid");
    let canvas = include_str!("../canvas/src/lib.rs");
    assert!(template.contains("<lc-canvas"));
    assert!(!template.contains("<select"));
    assert!(template.contains("name=\"options\""));
    assert!(canvas.contains("clear_rect(0.0, 0.0, width, height)"));
    assert!(!canvas.contains("fill_rect(0.0, 0.0, width, height)"));
    assert!(canvas.contains("BAR_ANIMATION_MS"));
    assert!(canvas.contains("WINNER_COLOR"));
    assert!(canvas.contains("optionId"));
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
    assert!(host
        .pushes()
        .contains(&chittr_cartridge::push::PushTarget::All));
}

#[test]
fn votes_are_refused_when_not_ready_logged_out_or_unknown() {
    let (host, chitt) = bare_setup();
    assert_eq!(
        vote(&host, &chitt, "char-1", "anchovies").unwrap().code,
        "poll_not_ready"
    );
    assert!(configure(&host, &chitt).is_none());

    host.begin_hook(HookClass::Background);
    let logged_out =
        testing::canvas_message(&chitt, None, json!({ "optionId": "anchovies" }), None);
    assert_eq!(
        on_canvas_message(wire(logged_out))
            .unwrap()
            .error
            .unwrap()
            .code,
        "logged_out"
    );
    assert_eq!(
        vote(&host, &chitt, "char-1", "not-an-option").unwrap().code,
        "unknown_option"
    );
}

#[test]
fn render_shows_tallies_and_the_viewers_own_vote() {
    let (host, chitt) = setup();
    vote(&host, &chitt, "char-1", "anchovies");
    vote(&host, &chitt, "char-2", "anchovies");
    vote(&host, &chitt, "char-3", "pineapple");

    let output = card(&host, &chitt, Some(testing::observer("char-1")));
    assert!(output.error.is_none());
    let options = output.data["options"]
        .as_array()
        .expect("the card lists options");
    let anchovies = options
        .iter()
        .find(|row| row["id"] == json!("anchovies"))
        .expect("anchovies is listed");
    assert_eq!(anchovies["votes"], json!(2));
    assert_eq!(anchovies["percent"], json!(66));
    assert_eq!(anchovies["voted"], json!(true));
    assert_eq!(output.data["total"], json!(3));
    assert_eq!(output.data["views"], json!(1));
    assert_eq!(output.data["signedIn"], json!(true));
    assert_eq!(output.data["closed"], json!(false));
}

#[test]
fn render_counts_views_once_per_character_and_ignores_anonymous_visits() {
    let (host, chitt) = setup();
    let anonymous = card(&host, &chitt, None);
    assert_eq!(anonymous.data["views"], Value::Null);
    assert!(host.kv_value(crate::VIEWS_KEY).is_none());

    let first = card(&host, &chitt, Some(testing::observer("char-1")));
    let repeat = card(&host, &chitt, Some(testing::observer("char-1")));
    assert_eq!(first.data["views"], json!(1));
    assert_eq!(repeat.data["views"], json!(1));

    let another = card(&host, &chitt, Some(testing::observer("char-2")));
    assert_eq!(another.data["views"], json!(2));
    assert_eq!(card(&host, &chitt, None).data["views"], json!(2));
}

#[test]
fn the_close_timer_announces_the_leader_once() {
    let (host, chitt) = setup();
    vote(&host, &chitt, "char-1", "anchovies");
    vote(&host, &chitt, "char-2", "anchovies");
    vote(&host, &chitt, "char-3", "pineapple");

    assert!(close_timer(&host, &chitt).is_none());
    assert_eq!(
        posted(&host),
        ["The poll is closed. Anchovies leads with 2 votes."]
    );
    assert_eq!(announcements(&host), 1);
    assert_eq!(host.kv_text(CLOSED_KEY).as_deref(), Some("1"));

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
    assert_eq!(
        posted(&host),
        ["The poll is closed. Pineapple leads with 1 vote."]
    );
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
    assert_eq!(
        posted(&host),
        ["The poll is closed. Extra cheese leads with 1 vote."]
    );
    assert_eq!(announcements(&host), 1);
}

#[test]
fn a_closed_poll_refuses_votes_and_replies() {
    let (host, chitt) = setup();
    assert!(close_timer(&host, &chitt).is_none());
    assert_eq!(
        vote(&host, &chitt, "char-1", "anchovies").unwrap().code,
        "poll_closed"
    );

    host.begin_hook(HookClass::Gate);
    let gate = on_reply(wire(testing::reply_gate(
        &chitt,
        testing::observer("char-1"),
        "hi",
        "chitt-1",
    )))
    .unwrap();
    assert_eq!(gate.allow, Some(false));
    assert_eq!(
        gate.message.as_deref(),
        Some("The poll is closed, so it takes no more replies.")
    );
}

#[test]
fn an_open_poll_lets_replies_through() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Gate);
    let gate = on_reply(wire(testing::reply_gate(
        &chitt,
        testing::observer("char-1"),
        "hi",
        "chitt-1",
    )))
    .unwrap();
    assert_eq!(gate.allow, Some(true));
}

#[test]
fn only_the_poster_can_close_and_only_when_logged_in() {
    let (host, chitt) = setup();
    host.begin_hook(HookClass::Control);
    let stranger = testing::control(
        &chitt,
        Some(testing::observer("char-9")),
        CLOSE_CONTROL,
        "close",
    );
    assert_eq!(
        on_control(wire(stranger)).unwrap().error.unwrap().code,
        "forbidden"
    );
    host.begin_hook(HookClass::Control);
    let anonymous = testing::control(&chitt, None, CLOSE_CONTROL, "close");
    assert_eq!(
        on_control(wire(anonymous)).unwrap().error.unwrap().code,
        "logged_out"
    );
    assert!(host.actions().is_empty());
}
