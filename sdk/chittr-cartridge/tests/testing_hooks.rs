//! A Cartridge written against the SDK's `#[hook]` attribute, run natively through the `testing` feature. This is the
//! pattern the guide shows: the hooks are plain functions, and the test hands them envelopes and checks the host.
#![cfg(feature = "testing")]

use chittr_cartridge::kv;
use chittr_cartridge::prelude::*;
use chittr_cartridge::testing::{self, HookClass, TestHost};

mod cartridge {
    use chittr_cartridge::prelude::*;
    use chittr_cartridge::{kv, poster, timer};

    #[hook]
    pub fn init(_input: InitInput) -> InitOutput {
        let scheduled = timer::schedule_timer("close", 3600, json!(null));
        match scheduled {
            Ok(()) => InitOutput::new("Which pizza topping?"),
            Err(_) => InitOutput::new(""),
        }
    }

    #[hook]
    pub fn render(input: RenderInput) -> RenderOutput {
        let views = kv::incr("views", 1, None).ok();
        let who = input.observer.map(|observer| observer.name);
        RenderOutput {
            template: "card".to_string(),
            data: json!({ "views": views, "who": who }),
            canvas: None,
        }
    }

    #[hook]
    pub fn on_timer(input: TimerInput) -> Result<TimerOutput, HookError> {
        if input.name != "close" {
            return Ok(TimerOutput::default());
        }
        if !kv::set_if_absent("announced", b"1", None)
            .map_err(|e| HookError::new(e.code, e.message))?
        {
            return Ok(TimerOutput::default());
        }
        // Release the claim when the post fails, so the timer's retry can still announce.
        if let Err(error) = poster::post_reply(&input.chitt.id, "The poll is closed.") {
            let _ = kv::delete("announced");
            return Err(HookError::new(error.code, error.message));
        }
        Ok(TimerOutput::default())
    }

    /// `?` on the SDK's calls: each error converts into the Hook's failure.
    #[hook]
    pub fn on_view(input: ViewInput) -> Result<ViewOutput, HookError> {
        let views = kv::incr(&format!("seen:{}", input.chitt.id), 1, Some(24 * 60 * 60))?;
        if views > 1000 {
            return Err(HookError::new("too_many", "this chitt has enough views"));
        }
        poster::post_reply(&input.chitt.id, "thanks for looking")?;
        Ok(ViewOutput::default())
    }

    #[hook]
    pub fn on_control(input: ControlInput) -> Result<ControlOutput, HookError> {
        match input.observer {
            Some(observer) if observer.is_poster => Ok(ControlOutput::default()),
            Some(_) => Err(HookError::new(
                "forbidden",
                "only the poster can close the poll",
            )),
            None => Err(HookError::new("logged_out", "log in to close the poll")),
        }
    }
}

#[test]
fn init_schedules_a_timer_and_publishes_the_question() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    let output = cartridge::init(testing::init(&chitt));
    assert_eq!(output.content, "Which pizza topping?");
    assert_eq!(host.timers()[0].name, "close");
}

#[test]
fn render_counts_views_per_call_and_sees_logged_out_viewers() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.begin_hook(HookClass::Render);
    let first = cartridge::render(testing::render(&chitt, None));
    host.begin_hook(HookClass::Render);
    let second = cartridge::render(testing::render(&chitt, Some(testing::observer("char-1"))));
    assert_eq!(first.data["views"], json!(1));
    assert_eq!(second.data["views"], json!(2));
    assert_eq!(second.data["who"], json!("char-1"));
    assert_eq!(first.data["who"], json!(null));
}

#[test]
fn on_timer_announces_once_and_a_retry_changes_nothing() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.begin_hook(HookClass::Background);
    assert!(cartridge::on_timer(testing::timer(&chitt, "close", json!(null))).is_ok());
    host.begin_hook(HookClass::Background);
    assert!(cartridge::on_timer(testing::timer(&chitt, "close", json!(null))).is_ok());
    let posts = host
        .actions()
        .iter()
        .filter(|action| action.action == "post_reply")
        .count();
    assert_eq!(posts, 1);
}

#[test]
fn a_refused_post_surfaces_as_the_hook_error() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.refuse_action("post_reply", "denied", "the account can't post");
    host.begin_hook(HookClass::Background);
    let error = cartridge::on_timer(testing::timer(&chitt, "close", json!(null))).unwrap_err();
    assert_eq!(error.code, "denied");
    assert_eq!(
        kv::get("announced").unwrap(),
        None,
        "the claim is released for the retry"
    );
    host.allow_action("post_reply");
    host.begin_hook(HookClass::Background);
    assert!(cartridge::on_timer(testing::timer(&chitt, "close", json!(null))).is_ok());
    assert_eq!(host.kv_text("announced").as_deref(), Some("1"));
}

#[test]
fn question_mark_turns_sdk_errors_into_hook_errors() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.begin_hook(HookClass::Background);
    assert!(cartridge::on_view(testing::view(&chitt, None)).is_ok());
    assert_eq!(host.kv_ttl("seen:chitt-1"), Some(24 * 60 * 60));

    host.refuse_action("post_reply", "rate_limited", "slow down");
    host.begin_hook(HookClass::Background);
    let error = cartridge::on_view(testing::view(&chitt, None)).unwrap_err();
    assert_eq!(error.code, "rate_limited");
}

#[test]
fn on_control_refuses_strangers_and_logged_out_viewers() {
    let _host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    let stranger = testing::control(&chitt, Some(testing::observer("char-9")), "close", "close");
    assert_eq!(
        cartridge::on_control(stranger).unwrap_err().code,
        "forbidden"
    );
    let anonymous = testing::control(&chitt, None, "close", "close");
    assert_eq!(
        cartridge::on_control(anonymous).unwrap_err().code,
        "logged_out"
    );
    let poster_view = testing::control(
        &chitt,
        Some(testing::observer("poster-1").poster()),
        "close",
        "close",
    );
    assert!(cartridge::on_control(poster_view).is_ok());
}
