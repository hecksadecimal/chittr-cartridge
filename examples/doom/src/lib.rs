// The bindings in pdk.rs are generated, and their error enums keep the schema's names.
#[allow(dead_code, non_camel_case_types)]
mod pdk;

use chittr_cartridge::host;
use chittr_cartridge::kv;
use chittr_cartridge::push::{push_render, PushTarget};
use extism_pdk::Error;
use pdk::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

const TITLE: &str = "DOOM";
#[cfg(feature = "engine")]
const STATUS: &str = "First-episode port: choose your own WAD above, click the canvas, then press Space. Keyboard controls; audio and saves are disabled.";
#[cfg(not(feature = "engine"))]
const STATUS: &str =
    "Scaffold build: rebuild with the engine feature to play. The viewer supplies their own WAD.";

const SETTINGS_KEY: &str = "doom:settings";
const SET_SKILL_CONTROL: &str = "set-skill";
const RESET_SETTINGS_CONTROL: &str = "reset-settings";
const IMPORT_SETTINGS_PICKER: &str = "import-settings";
const DEFAULT_SKILL: u8 = 2;
const MAX_SETTINGS_BYTES: i64 = 4096;
const SETTINGS_CHUNK: u32 = 4096;

/// DOOM's five skill levels, in the engine's `skill_t` order. The Control values are stable
/// identifiers; the labels are what the card and configuration panel show.
const SKILLS: [(&str, &str); 5] = [
    ("baby", "I'm too young to die"),
    ("easy", "Hey, not too rough"),
    ("medium", "Hurt me plenty"),
    ("hard", "Ultra-Violence"),
    ("nightmare", "Nightmare!"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Settings {
    #[serde(default = "default_skill")]
    skill: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            skill: DEFAULT_SKILL,
        }
    }
}

fn default_skill() -> u8 {
    DEFAULT_SKILL
}

// Supplies the Chitt's text on publication. Gameplay will run locally in the Canvas Guest.
pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(InitOutput {
        content: format!("{TITLE} — {STATUS}"),
        ..Default::default()
    })
}

pub(crate) fn render(_input: RenderInput) -> Result<RenderOutput, Error> {
    let (settings, custom) = current_settings();
    let mut data = Map::new();
    data.insert("title".to_string(), Value::String(TITLE.to_string()));
    data.insert("status".to_string(), Value::String(STATUS.to_string()));
    data.insert(
        "difficulty".to_string(),
        Value::String(skill_label(settings.skill).to_string()),
    );
    data.insert("difficultyCustom".to_string(), Value::Bool(custom));
    Ok(RenderOutput {
        template: "card".to_string(),
        data,
        // The Canvas Guest reads the owner's default when it boots, so a newly opened card
        // starts at the configured difficulty. Existing games keep their engine instance.
        canvas: json!({ "skill": settings.skill }).as_object().cloned(),
        error: None,
    })
}

/// The owner/admin configuration panel's data. The host authorizes the viewer and sets
/// `surface` and `access`; the guest still checks them before returning private state.
pub(crate) fn render_config(input: RenderConfigInput) -> Result<RenderConfigOutput, Error> {
    if input.surface != "config" {
        return Ok(config_error(fail(
            "wrong_surface",
            "configuration data is only available on the config surface",
        )));
    }
    if !input.access.can_configure || !can_configure(Some(&input.observer)) {
        return Ok(config_error(fail(
            "forbidden",
            "only this Chitt's owner or an administrator can configure DOOM",
        )));
    }
    let (settings, custom) = match current_settings_result() {
        Ok(settings) => settings,
        Err(error) => return Ok(config_error(error)),
    };
    let mut data = Map::new();
    data.insert(
        "selectedSkill".to_string(),
        Value::String(skill_label(settings.skill).to_string()),
    );
    data.insert(
        "defaultSkill".to_string(),
        Value::String(skill_label(DEFAULT_SKILL).to_string()),
    );
    data.insert("custom".to_string(), Value::Bool(custom));
    data.insert(
        "skills".to_string(),
        Value::Array(
            SKILLS
                .iter()
                .map(|(value, label)| {
                    json!({
                        "value": value,
                        "label": label,
                        "selected": parse_skill(value) == Some(settings.skill),
                    })
                })
                .collect(),
        ),
    );
    data.insert(
        "viewerRole".to_string(),
        Value::String(if input.access.is_owner {
            "owner".to_string()
        } else if input.access.is_admin {
            "administrator".to_string()
        } else {
            "configured viewer".to_string()
        }),
    );
    data.insert(
        "viewerIsOwner".to_string(),
        Value::Bool(input.access.is_owner),
    );
    data.insert(
        "viewerIsAdmin".to_string(),
        Value::Bool(input.access.is_admin),
    );
    data.insert("configurationAllowed".to_string(), Value::Bool(true));
    data.insert(
        "requiredAudience".to_string(),
        Value::String(input.access.audience.clone()),
    );
    data.insert("importLimit".to_string(), json!(MAX_SETTINGS_BYTES));
    Ok(RenderConfigOutput {
        data: Some(data),
        error: None,
    })
}

/// The configuration Controls are private to `config.liquid`. The host has already checked
/// the Control token's surface and audience; these checks keep the guest safe on its own.
pub(crate) fn on_control(input: ControlInput) -> Result<ControlOutput, Error> {
    let result = if input.surface != "config" {
        Err(fail(
            "wrong_surface",
            "DOOM's settings Controls only work in the Cartridge configuration panel",
        ))
    } else if !can_configure(input.observer.as_ref()) {
        Err(fail(
            "forbidden",
            "only this Chitt's owner or an administrator can change DOOM's settings",
        ))
    } else if input.control == RESET_SETTINGS_CONTROL
        && !can_configure_owner(input.observer.as_ref())
    {
        Err(fail(
            "forbidden",
            "only this Chitt's owner can reset DOOM's settings",
        ))
    } else {
        match input.control.as_str() {
            SET_SKILL_CONTROL => set_skill(&input),
            RESET_SETTINGS_CONTROL => reset_settings(),
            _ => Err(fail(
                "unknown_control",
                "this Cartridge has no such configuration Control",
            )),
        }
    };
    Ok(ControlOutput {
        error: result.err(),
    })
}

/// A configuration-scoped server upload. Only a small settings document is accepted; the WAD
/// picker stays client-local and never routes through the server.
pub(crate) fn on_file(input: FileInput) -> Result<FileOutput, Error> {
    let result = import_settings(&input);
    Ok(FileOutput {
        rerender: result.is_ok().then_some(true),
        error: result.err(),
    })
}

fn set_skill(input: &ControlInput) -> Result<(), HookError> {
    if input.fields.is_some() {
        return Err(fail("bad_input", "set-skill takes one value, not fields"));
    }
    let value = input
        .value
        .as_deref()
        .ok_or_else(|| fail("bad_input", "set-skill needs a difficulty value"))?;
    let skill = parse_skill(value)
        .ok_or_else(|| fail("bad_input", "choose one of DOOM's five difficulty levels"))?;
    save_settings(Settings { skill })?;
    push_cards();
    Ok(())
}

fn reset_settings() -> Result<(), HookError> {
    kv::delete(SETTINGS_KEY).map_err(|error| fail("storage_error", error.to_string()))?;
    push_cards();
    Ok(())
}

fn import_settings(input: &FileInput) -> Result<(), HookError> {
    if input.surface != "config" {
        return Err(fail(
            "wrong_surface",
            "DOOM's settings import is only available in the configuration panel",
        ));
    }
    if input.name != IMPORT_SETTINGS_PICKER {
        return Err(fail("unknown_picker", "this Cartridge has no such picker"));
    }
    if !can_configure(Some(&input.observer)) {
        return Err(fail(
            "forbidden",
            "only this Chitt's owner or an administrator can import DOOM's settings",
        ));
    }
    if input.file.size <= 0 || input.file.size > MAX_SETTINGS_BYTES {
        return Err(fail(
            "too_large",
            format!("the settings file must be 1 to {MAX_SETTINGS_BYTES} bytes"),
        ));
    }

    let result = (|| -> Result<(), HookError> {
        let mut bytes = Vec::with_capacity(input.file.size as usize);
        let mut offset = 0u64;
        loop {
            let chunk = host::file_read(&input.file.id, offset, SETTINGS_CHUNK)
                .map_err(|error| fail("file_read", error.to_string()))?;
            if chunk.bytes.is_empty() && !chunk.eof {
                return Err(fail("file_read", "the settings read made no progress"));
            }
            offset += chunk.bytes.len() as u64;
            bytes.extend_from_slice(&chunk.bytes);
            if bytes.len() as i64 > MAX_SETTINGS_BYTES {
                return Err(fail(
                    "too_large",
                    format!("the settings file is larger than {MAX_SETTINGS_BYTES} bytes"),
                ));
            }
            if chunk.eof {
                break;
            }
        }

        let text = std::str::from_utf8(&bytes)
            .map_err(|_| fail("bad_settings", "the settings file must be UTF-8 JSON"))?;
        let value: Value = serde_json::from_str(text)
            .map_err(|error| fail("bad_settings", format!("invalid settings JSON: {error}")))?;
        let map = value.as_object().ok_or_else(|| {
            fail(
                "bad_settings",
                "the settings file must contain a JSON object",
            )
        })?;
        if map.len() != 1 || !map.contains_key("skill") {
            return Err(fail(
                "bad_settings",
                r#"the settings file must be exactly {"skill": <level>}"#,
            ));
        }
        let skill = map.get("skill").and_then(parse_skill_json).ok_or_else(|| {
            fail(
                "bad_settings",
                "skill must be baby, easy, medium, hard or nightmare",
            )
        })?;
        save_settings(Settings { skill })?;
        push_cards();
        Ok(())
    })();

    // The handle is call-scoped anyway; closing early releases the host buffer.
    let _ = host::file_close(&input.file.id);
    result
}

fn current_settings() -> (Settings, bool) {
    match load_settings() {
        Ok(Some(settings)) if (settings.skill as usize) < SKILLS.len() => (settings, true),
        _ => (Settings::default(), false),
    }
}

fn current_settings_result() -> Result<(Settings, bool), HookError> {
    match load_settings()? {
        Some(settings) if (settings.skill as usize) < SKILLS.len() => Ok((settings, true)),
        // A corrupt stored value falls back to the default but stays "custom", so the
        // owner still sees the reset control and can repair it.
        Some(_) => Ok((Settings::default(), true)),
        None => Ok((Settings::default(), false)),
    }
}

fn load_settings() -> Result<Option<Settings>, HookError> {
    match kv::get(SETTINGS_KEY) {
        Ok(None) => Ok(None),
        Ok(Some(entry)) => serde_json::from_slice(&entry.value)
            .map(Some)
            .map_err(|error| {
                fail(
                    "invalid_settings",
                    format!("invalid stored settings: {error}"),
                )
            }),
        Err(error) => Err(fail("storage_error", error.to_string())),
    }
}

fn save_settings(settings: Settings) -> Result<(), HookError> {
    let encoded = serde_json::to_vec(&settings)
        .map_err(|error| fail("invalid_settings", error.to_string()))?;
    kv::set(SETTINGS_KEY, encoded, None).map_err(|error| fail("storage_error", error.to_string()))
}

fn push_cards() {
    // A refused push does not undo a saved setting; the next render still sees it.
    let _ = push_render(PushTarget::All);
}

fn can_configure(observer: Option<&Observer>) -> bool {
    observer.is_some_and(|viewer| {
        !viewer.is_suspended && (viewer.is_poster || viewer.is_poster_user || viewer.is_admin)
    })
}

fn can_configure_owner(observer: Option<&Observer>) -> bool {
    observer
        .is_some_and(|viewer| !viewer.is_suspended && (viewer.is_poster || viewer.is_poster_user))
}

fn parse_skill(value: &str) -> Option<u8> {
    let normalized = value.trim().to_ascii_lowercase();
    if let Ok(number) = normalized.parse::<u8>() {
        return ((number as usize) < SKILLS.len()).then_some(number);
    }
    SKILLS
        .iter()
        .position(|(name, _)| *name == normalized)
        .map(|index| index as u8)
}

fn parse_skill_json(value: &Value) -> Option<u8> {
    match value {
        Value::String(value) => parse_skill(value),
        Value::Number(value) => value
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .and_then(|value| ((value as usize) < SKILLS.len()).then_some(value)),
        _ => None,
    }
}

fn skill_label(skill: u8) -> &'static str {
    SKILLS
        .get(skill as usize)
        .map(|(_, label)| *label)
        .unwrap_or(SKILLS[DEFAULT_SKILL as usize].1)
}

fn config_error(error: HookError) -> RenderConfigOutput {
    RenderConfigOutput {
        data: None,
        error: Some(error),
    }
}

fn fail(code: &str, message: impl Into<String>) -> HookError {
    HookError {
        code: code.to_string(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chittr_cartridge::testing::{HookClass, TestHost};

    fn owner() -> Observer {
        Observer {
            is_poster_user: true,
            ..Default::default()
        }
    }

    fn admin() -> Observer {
        Observer {
            is_admin: true,
            ..Default::default()
        }
    }

    fn config_control(control: &str, value: &str) -> ControlInput {
        ControlInput {
            v: 2,
            hook: "on_control".to_string(),
            chitt: ChittRef::default(),
            observer: Some(owner()),
            surface: "config".to_string(),
            control: control.to_string(),
            value: Some(value.to_string()),
            fields: None,
        }
    }

    fn config_input(access: ConfigAccess) -> RenderConfigInput {
        RenderConfigInput {
            v: 2,
            hook: "render_config".to_string(),
            chitt: ChittRef::default(),
            observer: owner(),
            surface: "config".to_string(),
            access,
        }
    }

    fn owner_access() -> ConfigAccess {
        ConfigAccess {
            audience: "owner_or_admin".to_string(),
            is_owner: true,
            is_admin: false,
            can_configure: true,
        }
    }

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
            let host = TestHost::new();
            host.begin_hook(HookClass::Render);
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
    fn anonymous_render_supplies_the_default_difficulty_and_canvas_state() {
        let host = TestHost::new();
        host.begin_hook(HookClass::Render);
        let output = render(RenderInput::default()).unwrap();
        assert!(output.error.is_none());
        assert_eq!(output.template, "card");
        assert_eq!(output.data["title"], TITLE);
        assert_eq!(output.data["status"], STATUS);
        assert_eq!(output.data["difficulty"], skill_label(DEFAULT_SKILL));
        assert_eq!(output.data["difficultyCustom"], false);
        assert_eq!(output.canvas.as_ref().unwrap()["skill"], DEFAULT_SKILL);
    }

    #[test]
    fn owner_control_saves_and_renders_the_configured_difficulty() {
        let host = TestHost::new();
        host.begin_hook(HookClass::Control);
        let output = on_control(config_control(SET_SKILL_CONTROL, "nightmare")).unwrap();
        assert!(output.error.is_none());
        assert!(host.kv_text(SETTINGS_KEY).unwrap().contains("4"));

        host.begin_hook(HookClass::Render);
        let rendered = render(RenderInput::default()).unwrap();
        assert_eq!(rendered.data["difficulty"], "Nightmare!");
        assert_eq!(rendered.data["difficultyCustom"], true);
        assert_eq!(rendered.canvas.as_ref().unwrap()["skill"], 4);
    }

    #[test]
    fn reset_control_returns_to_the_default() {
        let host = TestHost::new();
        host.put_kv(SETTINGS_KEY, r#"{"skill":4}"#, None).unwrap();
        host.begin_hook(HookClass::Control);
        let output = on_control(config_control(RESET_SETTINGS_CONTROL, "reset")).unwrap();
        assert!(output.error.is_none());
        assert!(host.kv_text(SETTINGS_KEY).is_none());

        host.begin_hook(HookClass::Render);
        let rendered = render(RenderInput::default()).unwrap();
        assert_eq!(rendered.data["difficulty"], skill_label(DEFAULT_SKILL));
    }

    #[test]
    fn public_or_unprivileged_control_use_is_refused() {
        let host = TestHost::new();
        host.begin_hook(HookClass::Control);
        let mut input = config_control(SET_SKILL_CONTROL, "hard");
        input.surface = "card".to_string();
        let output = on_control(input).unwrap();
        assert_eq!(output.error.unwrap().code, "wrong_surface");

        host.begin_hook(HookClass::Control);
        let mut input = config_control(SET_SKILL_CONTROL, "hard");
        input.observer = Some(Observer::default());
        let output = on_control(input).unwrap();
        assert_eq!(output.error.unwrap().code, "forbidden");
        assert!(host.kv_text(SETTINGS_KEY).is_none());
    }

    #[test]
    fn an_admin_can_set_difficulty_but_only_the_owner_can_reset() {
        let host = TestHost::new();
        host.put_kv(SETTINGS_KEY, r#"{"skill":1}"#, None).unwrap();

        host.begin_hook(HookClass::Control);
        let mut input = config_control(SET_SKILL_CONTROL, "hard");
        input.observer = Some(admin());
        assert!(on_control(input).unwrap().error.is_none());

        host.begin_hook(HookClass::Control);
        let mut input = config_control(RESET_SETTINGS_CONTROL, "reset");
        input.observer = Some(admin());
        let output = on_control(input).unwrap();
        assert_eq!(output.error.unwrap().code, "forbidden");
        assert!(host.kv_text(SETTINGS_KEY).unwrap().contains("3"));
    }

    #[test]
    fn a_suspended_owner_cannot_configure() {
        let host = TestHost::new();
        host.begin_hook(HookClass::Control);
        let mut input = config_control(SET_SKILL_CONTROL, "hard");
        input.observer = Some(Observer {
            is_poster_user: true,
            is_suspended: true,
            ..Default::default()
        });
        let output = on_control(input).unwrap();
        assert_eq!(output.error.unwrap().code, "forbidden");
    }

    #[test]
    fn invalid_difficulty_values_are_refused() {
        let host = TestHost::new();
        host.begin_hook(HookClass::Control);
        let output = on_control(config_control(SET_SKILL_CONTROL, "ultra-nightmare")).unwrap();
        assert_eq!(output.error.unwrap().code, "bad_input");
        assert!(host.kv_text(SETTINGS_KEY).is_none());
    }

    #[test]
    fn render_config_exposes_private_state_only_with_access() {
        let host = TestHost::new();
        host.put_kv(SETTINGS_KEY, r#"{"skill":3}"#, None).unwrap();
        host.begin_hook(HookClass::Render);
        let output = render_config(config_input(owner_access())).unwrap();
        assert!(output.error.is_none());
        let data = output.data.unwrap();
        assert_eq!(data["selectedSkill"], "Ultra-Violence");
        assert_eq!(data["custom"], true);
        assert_eq!(data["viewerIsOwner"], true);

        host.begin_hook(HookClass::Render);
        let mut access = owner_access();
        access.can_configure = false;
        let output = render_config(config_input(access)).unwrap();
        assert_eq!(output.error.unwrap().code, "forbidden");
    }

    #[test]
    fn a_config_scoped_settings_import_updates_the_default() {
        let host = TestHost::new();
        host.put_file("f_settings", br#"{"skill":"hard"}"#);
        host.begin_hook(HookClass::Background);
        let input = FileInput {
            v: 2,
            hook: "on_file".to_string(),
            chitt: ChittRef::default(),
            observer: owner(),
            surface: "config".to_string(),
            name: IMPORT_SETTINGS_PICKER.to_string(),
            file: FileRef {
                id: "f_settings".to_string(),
                filename: "settings.json".to_string(),
                mime_type: "application/json".to_string(),
                size: 18,
            },
        };
        let output = on_file(input).unwrap();
        assert!(output.error.is_none());
        assert_eq!(output.rerender, Some(true));
        assert!(host.kv_text(SETTINGS_KEY).unwrap().contains("3"));
    }

    #[test]
    fn a_bad_settings_import_is_reported_without_writing() {
        let host = TestHost::new();
        host.put_file("f_bad", br#"{"skill":"impossible"}"#);
        host.begin_hook(HookClass::Background);
        let input = FileInput {
            v: 2,
            hook: "on_file".to_string(),
            chitt: ChittRef::default(),
            observer: owner(),
            surface: "config".to_string(),
            name: IMPORT_SETTINGS_PICKER.to_string(),
            file: FileRef {
                id: "f_bad".to_string(),
                filename: "settings.json".to_string(),
                mime_type: "application/json".to_string(),
                size: 24,
            },
        };
        let output = on_file(input).unwrap();
        assert_eq!(output.error.unwrap().code, "bad_settings");
        assert!(host.kv_text(SETTINGS_KEY).is_none());
    }

    #[test]
    fn skill_names_and_numbers_round_trip() {
        assert_eq!(parse_skill("medium"), Some(2));
        assert_eq!(parse_skill("Nightmare!"), None);
        assert_eq!(parse_skill("nightmare"), Some(4));
        assert_eq!(parse_skill("0"), Some(0));
        assert_eq!(parse_skill("5"), None);
        assert_eq!(parse_skill_json(&json!(1)), Some(1));
        assert_eq!(parse_skill_json(&json!("hard")), Some(3));
        assert_eq!(parse_skill_json(&json!(true)), None);
        assert_eq!(skill_label(4), "Nightmare!");
    }
}
