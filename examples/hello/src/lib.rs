//! A minimal Server Guest that passes the current Observer to its Liquid card. Written against the bindings
//! `xtp plugin init` generates from `docs/cartridges/v1/server.xtp.yaml` (see the README).

mod pdk;

use extism_pdk::Error;
use pdk::types::*;
use serde_json::{Map, Value};

pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(InitOutput { content: "Hello!".to_string(), ..Default::default() })
}

pub(crate) fn render(input: RenderInput) -> Result<RenderOutput, Error> {
    let mut data = Map::new();
    data.insert("observer".to_string(), serde_json::to_value(&input.observer).unwrap_or(Value::Null));
    Ok(RenderOutput { template: "card".to_string(), data, canvas: None, error: None })
}
