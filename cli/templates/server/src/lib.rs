// The bindings in pdk.rs are generated, and their error enums keep the schema's names.
#![allow(non_camel_case_types)]

mod pdk;

use extism_pdk::Error;
use pdk::types::*;
use serde_json::{Map, Value};

// Runs once, when the Chitt is published. Chittr refuses to publish without it, because it supplies the Chitt's text.
pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(InitOutput { content: "__NAME__ says hi".to_string(), ..Default::default() })
}

// Runs on every view, under a 500 ms deadline. Returns the template to draw (cartridge/templates/card.liquid) and the
// data it draws with. The Observer is the viewer, or none when logged out, so each viewer can get their own card.
pub(crate) fn render(input: RenderInput) -> Result<RenderOutput, Error> {
    let mut data = Map::new();
    data.insert("greeting".to_string(), Value::String("Hello from your Cartridge".to_string()));
    data.insert("observer".to_string(), serde_json::to_value(&input.observer).unwrap_or(Value::Null));
    Ok(RenderOutput { template: "card".to_string(), data, canvas: None, error: None })
}
