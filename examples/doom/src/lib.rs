// The bindings in pdk.rs are generated, and their error enums keep the schema's names.
#[allow(dead_code, non_camel_case_types)]
mod pdk;

use extism_pdk::Error;
use pdk::types::*;
use serde_json::{Map, Value};

const TITLE: &str = "DOOM";
#[cfg(feature = "engine")]
const STATUS: &str = "First-episode port: choose your own WAD above, click the canvas, then press Space. Keyboard controls; audio and saves are disabled.";
#[cfg(not(feature = "engine"))]
const STATUS: &str =
    "Scaffold build: rebuild with the engine feature to play. The viewer supplies their own WAD.";

// Supplies the Chitt's text on publication. Gameplay will run locally in the Canvas Guest.
pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(InitOutput {
        content: format!("{TITLE} — {STATUS}"),
        ..Default::default()
    })
}

pub(crate) fn render(_input: RenderInput) -> Result<RenderOutput, Error> {
    let mut data = Map::new();
    data.insert("title".to_string(), Value::String(TITLE.to_string()));
    data.insert("status".to_string(), Value::String(STATUS.to_string()));
    Ok(RenderOutput {
        template: "card".to_string(),
        data,
        canvas: None,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_identifies_the_build() {
        let output = init(InitInput::default()).unwrap();
        assert!(output.error.is_none());
        assert!(output.content.contains(TITLE));
        assert!(output.content.contains(STATUS));
    }

    #[test]
    fn render_accepts_fractional_integer_and_null_observer_ages() {
        for age in [serde_json::json!(9.7), serde_json::json!(9), Value::Null] {
            let mut envelope = serde_json::to_value(RenderInput {
                v: 2,
                hook: "render".to_string(),
                observer: Some(Observer::default()),
                ..Default::default()
            })
            .unwrap();
            envelope["observer"]["ageSweeps"] = age.clone();
            // Exercise the same JSON deserialization the generated render export uses.
            let input: RenderInput = serde_json::from_str(&envelope.to_string()).unwrap();
            let decoded_age = serde_json::to_value(input.observer.as_ref().unwrap()).unwrap();
            assert_eq!(decoded_age["ageSweeps"].as_f64(), age.as_f64());
            assert!(render(input).unwrap().error.is_none());
        }
    }

    #[test]
    fn anonymous_render_supplies_the_card_without_game_data() {
        let output = render(RenderInput::default()).unwrap();
        assert!(output.error.is_none());
        assert_eq!(output.template, "card");
        assert_eq!(output.data["title"], TITLE);
        assert_eq!(output.data["status"], STATUS);
        assert!(output.canvas.is_none());
    }
}
