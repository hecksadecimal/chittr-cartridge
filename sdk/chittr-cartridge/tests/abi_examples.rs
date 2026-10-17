//! The Cartridge ABI examples (tests/fixtures/cartridge-abi/*.json, written against the schemas in
//! docs/cartridges/v1/) round-trip through the SDK's serde types.
//!
//! An example with an `error` member is a failure: the member must parse as a HookError, and the rest of the example
//! must round-trip through the output type. Numbers are compared as f64, because the SDK's canvas types use f32.
//!
//! Run with: cargo test --features canvas --test abi_examples

#![cfg(all(feature = "server", feature = "canvas"))]

use chittr_cartridge::canvas::envelope as canvas;
use chittr_cartridge::envelope::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};

fn examples(file: &str, name: &str) -> Vec<Value> {
    let path = format!(
        "{}/tests/fixtures/cartridge-abi/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    let all: Map<String, Value> =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
    all.get(name)
        .unwrap_or_else(|| panic!("{file} has no examples for {name}"))
        .as_array()
        .unwrap_or_else(|| panic!("{name} examples must be an array"))
        .clone()
}

/// Every number as f64, so an integer example and an f32 field compare equal.
fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(number) => Value::from(number.as_f64().expect("a finite number")),
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), normalize(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn round_trip<T: Serialize + DeserializeOwned>(file: &str, name: &str) {
    for example in examples(file, name) {
        let mut body = example.clone();
        if let Some(error) = body
            .as_object_mut()
            .and_then(|object| object.remove("error"))
        {
            let parsed: HookError = serde_json::from_value(error.clone())
                .unwrap_or_else(|e| panic!("{name} error {error}: {e}"));
            assert_eq!(
                normalize(&serde_json::to_value(parsed).unwrap()),
                normalize(&error),
                "{name}: error round-trips"
            );
        }
        let typed: T =
            serde_json::from_value(body.clone()).unwrap_or_else(|e| panic!("{name} {body}: {e}"));
        let back = serde_json::to_value(&typed).unwrap();
        assert_eq!(normalize(&back), normalize(&body), "{name} round-trips");
    }
}

#[test]
fn server_hook_envelopes_round_trip() {
    round_trip::<HookError>("server-examples.json", "HookError");
    round_trip::<ChittRef>("server-examples.json", "ChittRef");
    round_trip::<Observer>("server-examples.json", "Observer");
    round_trip::<CartridgeVersionRef>("server-examples.json", "CartridgeVersionRef");
    round_trip::<InitInput>("server-examples.json", "InitInput");
    round_trip::<InitOutput>("server-examples.json", "InitOutput");
    round_trip::<RenderInput>("server-examples.json", "RenderInput");
    round_trip::<RenderOutput>("server-examples.json", "RenderOutput");
    round_trip::<ViewInput>("server-examples.json", "ViewInput");
    round_trip::<ViewOutput>("server-examples.json", "ViewOutput");
    round_trip::<TimerInput>("server-examples.json", "TimerInput");
    round_trip::<TimerOutput>("server-examples.json", "TimerOutput");
    round_trip::<ControlInput>("server-examples.json", "ControlInput");
    round_trip::<ControlOutput>("server-examples.json", "ControlOutput");
    round_trip::<MigrateInput>("server-examples.json", "MigrateInput");
    round_trip::<MigrateOutput>("server-examples.json", "MigrateOutput");
    round_trip::<CanvasMessageInput>("server-examples.json", "CanvasMessageInput");
    round_trip::<CanvasMessageOutput>("server-examples.json", "CanvasMessageOutput");
    round_trip::<ReplyGateInput>("server-examples.json", "ReplyGateInput");
    round_trip::<ReactionGateInput>("server-examples.json", "ReactionGateInput");
    round_trip::<RechittGateInput>("server-examples.json", "RechittGateInput");
    round_trip::<GateResult>("server-examples.json", "GateOutput");
}

#[test]
fn canvas_envelopes_round_trip() {
    round_trip::<canvas::CanvasError>("canvas-examples.json", "CanvasError");
    round_trip::<canvas::Reply>("canvas-examples.json", "CanvasReply");
    round_trip::<canvas::Reply>("canvas-examples.json", "CanvasHostReply");
    round_trip::<canvas::StartInput>("canvas-examples.json", "CanvasStartInput");
    round_trip::<canvas::FrameInput>("canvas-examples.json", "CanvasFrameInput");
    round_trip::<canvas::EventInput>("canvas-examples.json", "CanvasEventInput");
}
