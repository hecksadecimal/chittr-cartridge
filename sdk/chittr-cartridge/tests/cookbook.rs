//! The cookbook's recipes, compiled and run against the fake host, so the docs' code stays true.
#![cfg(feature = "testing")]

mod timers {
    use chittr_cartridge::prelude::*;
    use chittr_cartridge::{kv, poster, timer};

    const CLOSE: &str = "close";

    #[hook]
    pub fn init(_input: InitInput) -> Result<InitOutput, HookError> {
        timer::schedule_timer(CLOSE, 60 * 60, Value::Null)?;
        Ok(InitOutput::new("Which pizza topping is best?"))
    }

    #[hook]
    pub fn on_timer(input: TimerInput) -> Result<TimerOutput, HookError> {
        if input.name == CLOSE {
            close_once(&input.chitt.id)?;
        }
        Ok(TimerOutput::default())
    }

    pub fn close_once(chitt_id: &str) -> Result<(), HookError> {
        if !kv::set_if_absent("announced", b"1", None)? {
            return Ok(());
        }
        if let Err(error) = poster::post_reply(chitt_id, "The poll is closed.") {
            let _ = kv::delete("announced");
            return Err(error.into());
        }
        Ok(())
    }
}

mod state {
    use chittr_cartridge::envelope::Observer;
    use chittr_cartridge::kv::{self, KvError};
    use chittr_cartridge::prelude::Value;

    pub fn state_key(observer: &Observer) -> String {
        format!("state:{}", observer.character_id)
    }

    pub fn read_state(observer: &Observer) -> Result<Option<Value>, KvError> {
        Ok(kv::get(&state_key(observer))?
            .and_then(|entry| serde_json::from_slice(&entry.value).ok()))
    }

    pub fn write_state(observer: &Observer, state: &Value) -> Result<(), KvError> {
        kv::set(
            &state_key(observer),
            serde_json::to_vec(state).unwrap_or_default(),
            None,
        )
    }
}

use chittr_cartridge::prelude::*;
use chittr_cartridge::testing::{self, HookClass, TestHost};
use chittr_cartridge::{kv, timer};

#[test]
fn the_timer_recipe_closes_once_and_retries_after_a_failure() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    host.begin_hook(HookClass::Lifecycle);
    let output = timers::init(testing::init(&chitt)).unwrap();
    assert_eq!(output.content, "Which pizza topping is best?");
    assert_eq!(host.timers()[0].name, "close");

    host.refuse_action("post_reply", "unavailable", "busy");
    host.begin_hook(HookClass::Background);
    let failed = timers::on_timer(testing::timer(&chitt, "close", Value::Null)).unwrap_err();
    assert_eq!(failed.code, "unavailable");
    assert_eq!(host.kv_text("announced"), None);

    host.allow_action("post_reply");
    host.begin_hook(HookClass::Background);
    assert!(timers::on_timer(testing::timer(&chitt, "close", Value::Null)).is_ok());
    host.begin_hook(HookClass::Background);
    assert!(timers::on_timer(testing::timer(&chitt, "close", Value::Null)).is_ok());
    let posts = host
        .actions()
        .iter()
        .filter(|a| a.action == "post_reply" && a.reply.get("postId").is_some())
        .count();
    assert_eq!(posts, 1);
}

#[test]
fn the_daily_counter_recipe_sets_its_expiry_once() {
    let host = TestHost::new();
    let day = "views:2026-10-16";
    for _ in 0..3 {
        kv::set_if_absent(day, b"0", Some(2 * 24 * 60 * 60)).unwrap();
        kv::incr(day, 1, None).unwrap();
    }
    assert_eq!(host.kv_text(day).as_deref(), Some("3"));
    assert_eq!(host.kv_ttl(day), Some(2 * 24 * 60 * 60));
    host.advance(60);
    assert_eq!(host.kv_ttl(day), Some(2 * 24 * 60 * 60 - 60));
}

#[test]
fn the_per_observer_state_recipe_round_trips() {
    let _host = TestHost::new();
    let observer = testing::observer("char-1");
    assert_eq!(state::read_state(&observer).unwrap(), None);
    state::write_state(&observer, &json!({ "picked": "pineapple" })).unwrap();
    assert_eq!(
        state::read_state(&observer).unwrap(),
        Some(json!({ "picked": "pineapple" }))
    );
    let _ = timer::cancel_timer("nothing");
}
