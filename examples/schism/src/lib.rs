// The bindings in pdk.rs are generated, and their error enums keep the schema's names.
#![allow(non_camel_case_types)]

mod pdk;

use extism_pdk::Error;
use pdk::types::*;
use serde_json::{Map, Value};

const TITLE: &str = "Schism Tracker";

const STATUS: &str = "The original Schism Tracker, running locally inside the card. Load an IT, S3M, MOD or XM \
module with the picker; the tracker's own pattern editor, sample editor, order list and instrument screens are all \
live. Editing, playback and audio happen in your browser; modules are never uploaded. Sounds start muted: use the \
host's speaker button.";

const FORMATS: [&str; 4] = ["IT", "S3M", "MOD", "XM"];

// Supplies the Chitt's text on publication. The tracker itself runs in the Canvas Guest.
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
    data.insert(
        "formats".to_string(),
        Value::Array(
            FORMATS
                .iter()
                .map(|format| Value::String((*format).to_string()))
                .collect(),
        ),
    );
    data.insert(
        "screen".to_string(),
        Value::String("640 × 400 // Indexed8".to_string()),
    );
    Ok(RenderOutput {
        template: "card".to_string(),
        data,
        // The tracker always starts with an empty song on the pattern editor;
        // there is no owner-configured state to hand the Canvas Guest.
        canvas: None,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_identifies_the_port() {
        let output = init(InitInput::default()).unwrap();
        assert!(output.error.is_none());
        assert!(output.content.contains(TITLE));
        assert!(output.content.contains("S3M"));
        assert!(output.content.contains("browser"));
    }

    #[test]
    fn render_supplies_the_card_data() {
        let output = render(RenderInput::default()).unwrap();
        assert!(output.error.is_none());
        assert_eq!(output.template, "card");
        assert_eq!(output.data["title"], TITLE);
        assert_eq!(output.data["status"], STATUS);
        assert_eq!(output.data["screen"], "640 × 400 // Indexed8");
        let formats = output.data["formats"].as_array().unwrap();
        assert_eq!(formats.len(), FORMATS.len());
        assert_eq!(formats[0], "IT");
        assert!(output.canvas.is_none());
    }
}
