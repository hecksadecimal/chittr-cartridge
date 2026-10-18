//! Typed Hook envelopes for ABI `v: 1`, as the host sends and expects them.
//!
//! Inputs carry `v: 1` and the Hook name. Outputs are the plain return value of the Hook.
//! Field names are camelCase on the wire, as in the Chittr host.

use serde::{Deserialize, Serialize};

/// The Chitt the Cartridge is attached to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChittRef {
    pub id: String,
    pub poster_id: String,
    pub created_at: String,
}

/// The Character currently viewing or interacting with a Chitt.
///
/// The fields most Hooks branch on are typed. The rest of the public profile
/// (names, avatars, bio, kult and kull scores, badges, ...) stays in `profile` as raw JSON until
/// a ticket needs it typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observer {
    pub character_id: String,
    pub user_id: String,
    pub name: String,
    pub follows_poster: bool,
    pub followed_by_poster: bool,
    pub show_ooc: bool,
    pub do_not_disturb: bool,
    pub mutuals_only_messages: bool,
    pub is_poster: bool,
    pub is_poster_user: bool,
    pub is_admin: bool,
    pub is_suspended: bool,
    #[serde(flatten)]
    pub profile: serde_json::Map<String, serde_json::Value>,
}

/// Input to `init`, which runs once at publish. It has no `observer` key. In ABI v2 it also carries the effective
/// capabilities and resource limits; a v1 host omits both, so they default empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<ServerLimits>,
}

/// The effective v2 resource limits for a Server Guest. Read them rather than hard-coding the defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerLimits {
    pub max_file_bytes: u64,
    pub max_file_read_bytes: u64,
    pub max_file_handles: u32,
    pub max_pickers: u32,
}

/// Output of `init`. `content` becomes the Chitt's raw content; publishing is refused without it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitOutput {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quirks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ooc: Option<bool>,
}

impl InitOutput {
    /// Content only: quirks and OOC keep the host defaults.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            quirks: None,
            ooc: None,
        }
    }
}

/// Input to `render`, which runs on every view. `observer` is `None` when the viewer is logged out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
}

/// Input to `on_view`: a real impression of the Chitt, reported once per Observer per 24 hours.
/// `observer` is `None` when the viewer is logged out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
}

/// Input to `on_timer`: a timer the guest scheduled has fired. It has no `observer` key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    /// The name the timer was scheduled under.
    pub name: String,
    /// The payload it was scheduled with (`null` when none).
    #[serde(default)]
    pub payload: serde_json::Value,
    /// When the timer was due, as an ISO 8601 UTC time.
    pub scheduled_for: String,
}

/// Output of `on_view`: an empty object. The host reads `{ "error": ... }` as a failure instead.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewOutput {}

/// Output of `on_timer`: an empty object. The host reads `{ "error": ... }` as a failure instead.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerOutput {}

/// Output of `render`: the Cartridge template to draw and the data it draws with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderOutput {
    pub template: String,
    pub data: serde_json::Value,
    /// The Canvas Guest's state: JSON of at most 16 KiB, sent to the canvas as `state` on every render.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<serde_json::Value>,
}

/// One Cartridge Version as a `migrate` envelope names it: its author's `version` string and its archive sha256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartridgeVersionRef {
    pub version: String,
    pub sha256: String,
}

/// Input to `migrate`, which the incoming Cartridge Version runs on replacement or rollback. It has no
/// `observer` key. `from` is the version being replaced and `to` is the one that runs after the swap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    pub from: CartridgeVersionRef,
    pub to: CartridgeVersionRef,
}

/// Output of `migrate`: an empty object when the storage moved over. Return `HookError` to keep the old version;
/// the host then restores the storage as it was before the swap.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrateOutput {}

/// A failure the Hook handled itself. Returned as `{ "error": { code, message } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookError {
    pub code: String,
    pub message: String,
}

impl HookError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Input to `on_control`: the Observer used a Control on the card. `control` is the name the
/// manifest declares; `value` is set for a value Control, and `fields` for a fields Control. The host has
/// checked both against the manifest, so the guest sees only declared Controls with valid payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
    /// The render surface the Control was used on, `card` or `config` (ABI v2, host-set). A v1 host omits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    pub control: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Output of `on_control`: an empty object. The host reads `{ "error": ... }` as a failure instead.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlOutput {}

/// Input to `on_canvas_message`: one message from the Canvas Guest's `send`. `message` is the JSON the
/// canvas sent, at most 4 KiB. `seq` is the canvas's sequence number, echoed on its reply event. `observer` is null
/// for a logged-out viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasMessageInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
    pub message: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
}

/// Output of `on_canvas_message`: `{}`, or a `reply` (sent only to the canvas that messaged, as a `reply`
/// event with its `seq`, at most 4 KiB) and `rerender` (draws the sender's card again, which delivers fresh canvas state).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasMessageOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rerender: bool,
}

/// Metadata for a file a picker delivered (ABI v2). `id` is an opaque, scoped handle; the bytes never travel in an
/// envelope. `filename` and `mime_type` are display hints, never validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

/// Input to `on_file`, a server-routed file from an authorized picker (ABI v2). The handle is scoped to this call
/// and revoked when the Hook settles. `surface` is `card` or `config`, host-set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
    pub surface: String,
    pub name: String,
    pub file: FileRef,
}

/// Output of `on_file`: `{}`, or `rerender` to draw the sender's card again through `render`. There is no reply.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOutput {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rerender: bool,
}

/// The host's authorization context for a configuration render or action (ABI v2). Host-computed; a guest cannot set
/// it or widen it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigAccess {
    /// The strictest audience the configuration declarations require: `owner`, `admin` or `owner_or_admin`.
    pub audience: String,
    pub is_owner: bool,
    pub is_admin: bool,
    /// The host's final decision for this request. False means the configuration must not run.
    pub can_configure: bool,
}

/// Input to `render_config`, run only for an authorized owner or admin opening the configuration panel (ABI v2).
/// `surface` is always `config`, and `access` is host-computed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderConfigInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
    pub surface: String,
    pub access: ConfigAccess,
}

/// Output of `render_config`: the data `config.liquid` draws with. The template name is fixed and there is no canvas
/// state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RenderConfigOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Input to `on_reply`, a gate: the reply about to be posted. It has no id yet, because the gate runs
/// before the reply commits. Only direct replies reach it (`reply_to_post_id` is this Chitt). The Observer is
/// the acting Character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyGateInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
    pub character_id: String,
    pub content_raw: String,
    pub content_quirked: String,
    pub is_ooc: bool,
    pub reply_to_post_id: String,
}

/// Input to `on_reaction`: a gate for an added reaction, or a notification when `added` is false (a removal, whose
/// return is ignored). `reaction_type` is one of `like`, `hate`, `pity` or `mediate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionGateInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
    #[serde(rename = "type")]
    pub reaction_type: String,
    pub added: bool,
}

/// Input to `on_rechitt`: a gate for an added rechitt, or a notification when `added` is false (a removal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RechittGateInput {
    pub v: u32,
    pub hook: String,
    pub chitt: ChittRef,
    #[serde(default)]
    pub observer: Option<Observer>,
    pub added: bool,
}

/// A gate's return: `allow` (a missing one means allow) and, to deny, the plain-text `message` the
/// composer shows. A removal's return is ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl GateResult {
    /// Lets the action commit.
    pub fn allow() -> Self {
        Self {
            allow: Some(true),
            message: None,
        }
    }

    /// Refuses the action with the generic "This Chitt didn't accept that".
    pub fn refuse() -> Self {
        Self {
            allow: Some(false),
            message: None,
        }
    }

    /// Refuses the action and shows `message` (plain text; the host caps its length).
    pub fn deny(message: impl Into<String>) -> Self {
        Self {
            allow: Some(false),
            message: Some(message.into()),
        }
    }
}

/// A KV, timer or Render Push failure becomes the Hook's own failure with the host's code and message, so a Hook
/// that returns `Result<_, HookError>` can use `?` on those calls.
impl From<crate::kv::KvError> for HookError {
    fn from(error: crate::kv::KvError) -> Self {
        Self::new(error.code, error.message)
    }
}

/// A refused Poster Action becomes the Hook's own failure, as `From<KvError>` does.
impl From<crate::poster::PosterError> for HookError {
    fn from(error: crate::poster::PosterError) -> Self {
        Self::new(error.code, error.message)
    }
}

/// A failed `asset` call becomes the Hook's own failure, as `From<KvError>` does.
impl From<crate::host::AssetError> for HookError {
    fn from(error: crate::host::AssetError) -> Self {
        Self::new(error.code, error.message)
    }
}

/// A failed `file_read` or `file_close` call becomes the Hook's own failure (ABI v2).
impl From<crate::host::FileError> for HookError {
    fn from(error: crate::host::FileError) -> Self {
        Self::new(error.code, error.message)
    }
}

/// The wire shape of a handled failure.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ErrorEnvelope {
    pub error: HookError,
}
