//! The manifest's `controls`: what a Cartridge declares, and the checks on each declaration. Messages match the
//! server's controls checks, and the caller names the manifest file.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::error::js_len;

const CONTROLS_MAX: usize = 32;
const ENUM_VALUES_MAX: usize = 100;
const ENUM_VALUE_CHARS_MAX: usize = 64;
const FIELDS_MAX: usize = 16;
const STRING_CHARS_MAX: u64 = 2000;

static CONTROL_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_-]{0,31}$").expect("valid regex"));
static FIELD_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_-]{0,31}$").expect("valid regex"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    String,
    Number,
    Boolean,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldSpec {
    pub kind: FieldType,
    pub max_length: Option<u64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// A Control is either a fixed value (an enum of allowed values) or typed fields.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlDeclaration {
    Value(Vec<String>),
    Fields(Vec<(String, FieldSpec)>),
}

/// The declared Controls, in manifest order.
pub type ControlDeclarations = Vec<(String, ControlDeclaration)>;

/// Parses the manifest's `controls`. Absent means none. The error is the message to report under `manifest.json`.
pub fn parse(value: Option<&Value>) -> std::result::Result<ControlDeclarations, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(map) = value.as_object() else {
        return Err(
            r#""controls" must be an object mapping Control names to declarations"#.to_string(),
        );
    };
    if map.len() > CONTROLS_MAX {
        return Err(format!(
            r#""controls" declares {} Controls; the limit is {CONTROLS_MAX}"#,
            map.len()
        ));
    }
    let mut out = Vec::new();
    for (name, declaration) in map {
        if !CONTROL_NAME.is_match(name) {
            return Err(format!(
                r#"Control name "{name}" must start with a lowercase letter and use only lowercase letters, digits, "-" or "_" (at most 32 characters)"#
            ));
        }
        out.push((name.clone(), parse_declaration(name, declaration)?));
    }
    Ok(out)
}

fn parse_declaration(name: &str, value: &Value) -> std::result::Result<ControlDeclaration, String> {
    let Some(map) = value.as_object() else {
        return Err(format!(
            r#"Control "{name}" must be {{ "value": ... }} or {{ "fields": ... }}"#
        ));
    };
    let has_value = map.contains_key("value");
    let has_fields = map.contains_key("fields");
    if map.len() != 1 || (!has_value && !has_fields) {
        return Err(format!(
            r#"Control "{name}" must declare exactly one of "value" or "fields""#
        ));
    }
    if has_value {
        Ok(ControlDeclaration::Value(parse_enum(name, &map["value"])?))
    } else {
        Ok(ControlDeclaration::Fields(parse_fields(
            name,
            &map["fields"],
        )?))
    }
}

fn parse_enum(name: &str, value: &Value) -> std::result::Result<Vec<String>, String> {
    let shape_error = || format!(r#"Control "{name}" value must be {{ "enum": [...] }}"#);
    let map = value.as_object().ok_or_else(shape_error)?;
    if map.keys().any(|key| key != "enum") {
        return Err(shape_error());
    }
    let values = match map.get("enum").and_then(Value::as_array) {
        Some(values) if !values.is_empty() => values,
        _ => {
            return Err(format!(
                r#"Control "{name}" enum must be a non-empty array"#
            ))
        }
    };
    if values.len() > ENUM_VALUES_MAX {
        return Err(format!(
            r#"Control "{name}" enum has {} values; the limit is {ENUM_VALUES_MAX}"#,
            values.len()
        ));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in values {
        let text = match item.as_str() {
            Some(text) if !text.is_empty() && js_len(text) <= ENUM_VALUE_CHARS_MAX => text,
            _ => {
                return Err(format!(
                    r#"Control "{name}" enum values must be strings of 1 to {ENUM_VALUE_CHARS_MAX} characters"#
                ))
            }
        };
        if !seen.insert(text) {
            return Err(format!(r#"Control "{name}" enum repeats "{text}""#));
        }
        out.push(text.to_string());
    }
    Ok(out)
}

fn parse_fields(
    name: &str,
    value: &Value,
) -> std::result::Result<Vec<(String, FieldSpec)>, String> {
    let map = value
        .as_object()
        .ok_or_else(|| format!(r#"Control "{name}" fields must be an object"#))?;
    if map.is_empty() || map.len() > FIELDS_MAX {
        return Err(format!(
            r#"Control "{name}" must declare 1 to {FIELDS_MAX} fields"#
        ));
    }
    let mut out = Vec::new();
    for (field, spec) in map {
        if !FIELD_NAME.is_match(field) || js_len(field) > 32 {
            return Err(format!(
                r#"Control "{name}" field "{field}" must start with a letter and use only letters, digits, "-" or "_""#
            ));
        }
        out.push((field.clone(), parse_field(name, field, spec)?));
    }
    Ok(out)
}

fn parse_field(
    control: &str,
    field: &str,
    value: &Value,
) -> std::result::Result<FieldSpec, String> {
    let location = format!(r#"Control "{control}" field "{field}""#);
    let map = value
        .as_object()
        .ok_or_else(|| format!(r#"{location} must be an object with a "type""#))?;
    const ALLOWED: [&str; 4] = ["type", "maxLength", "min", "max"];
    if let Some(unknown) = map.keys().find(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(format!(r#"{location} has an unknown key "{unknown}""#));
    }
    let kind = match map.get("type").and_then(Value::as_str) {
        Some("string") => FieldType::String,
        Some("number") => FieldType::Number,
        Some("boolean") => FieldType::Boolean,
        _ => {
            return Err(format!(
                r#"{location} type must be "string", "number" or "boolean""#
            ))
        }
    };
    let mut spec = FieldSpec {
        kind,
        max_length: None,
        min: None,
        max: None,
    };
    if map.contains_key("maxLength") {
        if kind != FieldType::String {
            return Err(format!(
                r#"{location} can set maxLength only when its type is "string""#
            ));
        }
        let length = map["maxLength"]
            .as_f64()
            .filter(|n| n.fract() == 0.0 && (1.0..=STRING_CHARS_MAX as f64).contains(n))
            .ok_or_else(|| {
                format!(r#"{location} maxLength must be an integer from 1 to {STRING_CHARS_MAX}"#)
            })?;
        spec.max_length = Some(length as u64);
    }
    if map.contains_key("min") || map.contains_key("max") {
        if kind != FieldType::Number {
            return Err(format!(
                r#"{location} can set min and max only when its type is "number""#
            ));
        }
        for bound in ["min", "max"] {
            if let Some(value) = map.get(bound) {
                let number = value
                    .as_f64()
                    .ok_or_else(|| format!("{location} {bound} must be a finite number"))?;
                if bound == "min" {
                    spec.min = Some(number);
                } else {
                    spec.max = Some(number);
                }
            }
        }
        if let (Some(min), Some(max)) = (spec.min, spec.max) {
            if min > max {
                return Err(format!("{location} min is larger than max"));
            }
        }
    }
    Ok(spec)
}
