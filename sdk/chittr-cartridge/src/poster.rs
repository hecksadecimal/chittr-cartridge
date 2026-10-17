//! Poster Actions: a Server Guest acts as its Chitt's posting Character, under that
//! Character's current authority. Each function here is one host call. The wire rule is the KV one: one JSON
//! object in, one out. A refusal comes back as a `PosterError` (codes: `forbidden`, `not_found`, `invalid`,
//! `denied`, `rate_limited`, `depth_exceeded`, `unavailable`), never a trap.
//!
//! Inside `init` and `migrate` the host defers these actions until the Chitt (or swap) commits. Their calls then
//! return `Ok(None)`, the outcome being only in the Poster Action log. Calls that return an id or an action name
//! return `Option<String>` for this reason: `Some` when the action ran, `None` when it was deferred.
//!
//! Quotas: 5 actions per Hook call and 1 per `render`, and 30 per minute per Chitt. Past them a call
//! returns `rate_limited`.

use crate::ffi;
use serde_json::{json, Map, Value};
use std::fmt;

/// The id a deferred action names for the Chitt its gate's own action created. A gate's actions are
/// deferred until the caller commits, so the guest can't know the new id: pass this as `postId`, and the host
/// fills it in when the reply commits. A refused action leaves it unresolved, and the action is `not_found`.
pub const NEW_POST_REF: &str = "$new";

/// A refused Poster Action. `code` is one of the codes in the module docs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct PosterError {
    pub code: String,
    pub message: String,
}

impl PosterError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl fmt::Display for PosterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PosterError {}

pub type PosterResult<T> = Result<T, PosterError>;

/// The reactions a Poster Action can add or remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    Like,
    Hate,
    Pity,
    Mediate,
}

impl Reaction {
    fn as_str(self) -> &'static str {
        match self {
            Reaction::Like => "like",
            Reaction::Hate => "hate",
            Reaction::Pity => "pity",
            Reaction::Mediate => "mediate",
        }
    }
}

/// What a pin points at: a Chitt or a Rechitt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinType {
    Post,
    Rechitt,
}

impl PinType {
    fn as_str(self) -> &'static str {
        match self {
            PinType::Post => "post",
            PinType::Rechitt => "rechitt",
        }
    }
}

/// The arguments of `edit_chitt`. With no `post_id` the action edits the Chitt itself, where `quirks`
/// is applied only when true (as `init` does). Any other Chitt gets the hand-made edit path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditArgs {
    pub post_id: Option<String>,
    pub content: String,
    pub quirks: Option<bool>,
    pub ooc: Option<bool>,
}

/// Posts a reply to any Chitt. Returns the new post's id.
pub fn post_reply(post_id: &str, content: &str) -> PosterResult<Option<String>> {
    let fields = invoke(unsafe {
        ffi::post_reply(json!({ "postId": post_id, "content": content }).to_string())
    })?;
    optional_string(&fields, "postId")
}

/// Posts a top-level Chitt as the poster's Character. Returns the new post's id.
pub fn post_chitt(content: &str) -> PosterResult<Option<String>> {
    let fields = invoke(unsafe { ffi::post_chitt(json!({ "content": content }).to_string()) })?;
    optional_string(&fields, "postId")
}

/// Edits a Chitt the poster's Character wrote, or the Chitt itself when `post_id` is `None`.
pub fn edit_chitt(args: EditArgs) -> PosterResult<()> {
    let mut request = Map::new();
    if let Some(post_id) = args.post_id {
        request.insert("postId".to_string(), json!(post_id));
    }
    request.insert("content".to_string(), json!(args.content));
    if let Some(quirks) = args.quirks {
        request.insert("quirks".to_string(), json!(quirks));
    }
    if let Some(ooc) = args.ooc {
        request.insert("ooc".to_string(), json!(ooc));
    }
    invoke(unsafe { ffi::edit_chitt(Value::Object(request).to_string()) })?;
    Ok(())
}

/// Deletes a Chitt the poster's Character wrote. Needs the post_deletion permission on the poster's account.
pub fn delete_chitt(post_id: &str) -> PosterResult<()> {
    invoke(unsafe { ffi::delete_chitt(json!({ "postId": post_id }).to_string()) })?;
    Ok(())
}

/// Adds a reaction. Returns `"added"`, or `"unchanged"` when the reaction was already there.
pub fn react(post_id: &str, reaction: Reaction) -> PosterResult<Option<String>> {
    let fields = invoke(unsafe {
        ffi::react(json!({ "postId": post_id, "type": reaction.as_str() }).to_string())
    })?;
    optional_string(&fields, "action")
}

/// Removes a reaction. Returns `"removed"`, or `"unchanged"` when there was none.
pub fn unreact(post_id: &str, reaction: Reaction) -> PosterResult<Option<String>> {
    let fields = invoke(unsafe {
        ffi::unreact(json!({ "postId": post_id, "type": reaction.as_str() }).to_string())
    })?;
    optional_string(&fields, "action")
}

/// Rechitts a Chitt. Returns the rechitt's id.
pub fn rechitt(post_id: &str) -> PosterResult<Option<String>> {
    let fields = invoke(unsafe { ffi::rechitt(json!({ "postId": post_id }).to_string()) })?;
    optional_string(&fields, "rechittId")
}

/// Takes back a Rechitt.
pub fn unrechitt(post_id: &str) -> PosterResult<()> {
    invoke(unsafe { ffi::unrechitt(json!({ "postId": post_id }).to_string()) })?;
    Ok(())
}

/// Follows a Character. Returns `"followed"`, or `"unchanged"` when already followed.
pub fn follow(character_id: &str) -> PosterResult<Option<String>> {
    let fields =
        invoke(unsafe { ffi::follow(json!({ "characterId": character_id }).to_string()) })?;
    optional_string(&fields, "action")
}

/// Unfollows a Character. Returns `"unfollowed"`, or `"unchanged"` when not followed.
pub fn unfollow(character_id: &str) -> PosterResult<Option<String>> {
    let fields =
        invoke(unsafe { ffi::unfollow(json!({ "characterId": character_id }).to_string()) })?;
    optional_string(&fields, "action")
}

/// Pins a Chitt or Rechitt to the poster's Character profile.
pub fn pin(post_id: &str, kind: PinType) -> PosterResult<()> {
    invoke(unsafe { ffi::pin(json!({ "postId": post_id, "type": kind.as_str() }).to_string()) })?;
    Ok(())
}

/// Clears the poster's Character's pinned Chitt.
pub fn unpin() -> PosterResult<()> {
    invoke(unsafe { ffi::unpin(json!({}).to_string()) })?;
    Ok(())
}

// ---- Admin Poster Actions. Each needs its admin permission on the poster's account, checked live. ----

/// How long a ban or a suspension runs. `Permanent` never lifts on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanDuration {
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    TwentyFourHours,
    SevenDays,
    ThirtyDays,
    Permanent,
}

impl BanDuration {
    fn as_str(self) -> &'static str {
        match self {
            BanDuration::OneMinute => "1m",
            BanDuration::FiveMinutes => "5m",
            BanDuration::FifteenMinutes => "15m",
            BanDuration::ThirtyMinutes => "30m",
            BanDuration::OneHour => "1h",
            BanDuration::TwentyFourHours => "24h",
            BanDuration::SevenDays => "7d",
            BanDuration::ThirtyDays => "30d",
            BanDuration::Permanent => "permanent",
        }
    }
}

/// Deletes any Chitt, not only the poster's own. Needs `post_deletion`.
pub fn admin_delete_post(post_id: &str) -> PosterResult<()> {
    invoke(unsafe { ffi::admin_delete_post(json!({ "postId": post_id }).to_string()) })?;
    Ok(())
}

/// Edits any Chitt's text. Needs `post_editing`.
pub fn admin_edit_post(post_id: &str, content: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::admin_edit_post(json!({ "postId": post_id, "content": content }).to_string())
    })?;
    Ok(())
}

/// Pins a Chitt site-wide. Needs `pin_chitt`.
pub fn admin_pin_chitt(post_id: &str) -> PosterResult<()> {
    invoke(unsafe { ffi::admin_pin_chitt(json!({ "postId": post_id }).to_string()) })?;
    Ok(())
}

/// Removes the site-wide pin from a Chitt. Needs `pin_chitt`.
pub fn admin_unpin_chitt(post_id: &str) -> PosterResult<()> {
    invoke(unsafe { ffi::admin_unpin_chitt(json!({ "postId": post_id }).to_string()) })?;
    Ok(())
}

/// Bans a user account. Needs `user_banning`.
pub fn ban_user(user_id: &str, duration: BanDuration, reason: &str) -> PosterResult<()> {
    let request = json!({ "userId": user_id, "duration": duration.as_str(), "reason": reason });
    invoke(unsafe { ffi::ban_user(request.to_string()) })?;
    Ok(())
}

/// Lifts a user account's ban. Needs `user_banning`.
pub fn unban_user(user_id: &str) -> PosterResult<()> {
    invoke(unsafe { ffi::unban_user(json!({ "userId": user_id }).to_string()) })?;
    Ok(())
}

/// Suspends a Character. Needs `character_suspension`.
pub fn suspend_character(
    character_id: &str,
    duration: BanDuration,
    reason: &str,
) -> PosterResult<()> {
    let request =
        json!({ "characterId": character_id, "duration": duration.as_str(), "reason": reason });
    invoke(unsafe { ffi::suspend_character(request.to_string()) })?;
    Ok(())
}

/// Lifts a Character's suspension. Needs `character_suspension`.
pub fn unsuspend_character(character_id: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::unsuspend_character(json!({ "characterId": character_id }).to_string())
    })?;
    Ok(())
}

/// Deletes a Character. Needs `character_deletion`.
pub fn admin_delete_character(character_id: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::admin_delete_character(json!({ "characterId": character_id }).to_string())
    })?;
    Ok(())
}

/// Changes a Character's trolltag. Needs `character_suspension`.
pub fn admin_update_trolltag(character_id: &str, trolltag: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::admin_update_trolltag(
            json!({ "characterId": character_id, "trolltag": trolltag }).to_string(),
        )
    })?;
    Ok(())
}

/// Renames a Character. Needs `character_suspension`.
pub fn admin_update_character_name(character_id: &str, name: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::admin_update_character_name(
            json!({ "characterId": character_id, "name": name }).to_string(),
        )
    })?;
    Ok(())
}

/// Changes a Character's bio; an empty bio clears it. Needs `character_suspension`.
pub fn admin_update_character_bio(character_id: &str, bio: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::admin_update_character_bio(
            json!({ "characterId": character_id, "bio": bio }).to_string(),
        )
    })?;
    Ok(())
}

/// Changes a Character's four-digit discriminator. Needs `character_suspension`.
pub fn admin_update_character_discriminator(
    character_id: &str,
    discriminator: &str,
) -> PosterResult<()> {
    let request = json!({ "characterId": character_id, "discriminator": discriminator });
    invoke(unsafe { ffi::admin_update_character_discriminator(request.to_string()) })?;
    Ok(())
}

/// Creates a badge from a `data:image/…` URI. Needs `badge_management`.
pub fn create_badge(name: &str, description: &str, image_data_uri: &str) -> PosterResult<()> {
    let request =
        json!({ "name": name, "description": description, "imageDataUri": image_data_uri });
    invoke(unsafe { ffi::create_badge(request.to_string()) })?;
    Ok(())
}

/// Deletes a badge, with every grant and equip of it. Needs `badge_management`.
pub fn delete_badge(badge_id: &str) -> PosterResult<()> {
    invoke(unsafe { ffi::delete_badge(json!({ "badgeId": badge_id }).to_string()) })?;
    Ok(())
}

/// Grants a badge to a user account. Needs `badge_management`.
pub fn grant_badge_to_user(user_id: &str, badge_id: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::grant_badge_to_user(json!({ "userId": user_id, "badgeId": badge_id }).to_string())
    })?;
    Ok(())
}

/// Revokes a badge from a user account. Needs `badge_management`.
pub fn revoke_badge_from_user(user_id: &str, badge_id: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::revoke_badge_from_user(json!({ "userId": user_id, "badgeId": badge_id }).to_string())
    })?;
    Ok(())
}

/// Grants a badge to a Character. Needs `grant_badge`.
pub fn grant_badge_to_character(character_id: &str, badge_id: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::grant_badge_to_character(
            json!({ "characterId": character_id, "badgeId": badge_id }).to_string(),
        )
    })?;
    Ok(())
}

/// Revokes a badge from a Character. Needs `revoke_badge`.
pub fn revoke_badge_from_character(character_id: &str, badge_id: &str) -> PosterResult<()> {
    invoke(unsafe {
        ffi::revoke_badge_from_character(
            json!({ "characterId": character_id, "badgeId": badge_id }).to_string(),
        )
    })?;
    Ok(())
}

/// Reads a host reply: its fields on success, or the `error` it carries.
fn invoke(reply: Result<String, extism_pdk::Error>) -> PosterResult<Map<String, Value>> {
    let text = reply.map_err(|error| PosterError::new("unavailable", error.to_string()))?;
    let Value::Object(mut fields) = serde_json::from_str::<Value>(&text)
        .map_err(|error| PosterError::new("invalid", error.to_string()))?
    else {
        return Err(PosterError::new(
            "invalid",
            "the host sent a reply that is not an object",
        ));
    };
    if let Some(error) = fields.remove("error") {
        return Err(
            serde_json::from_value::<PosterError>(error).unwrap_or_else(|_| {
                PosterError::new(
                    "invalid",
                    "the host sent an error that is not { code, message }",
                )
            }),
        );
    }
    Ok(fields)
}

/// The reply's string field, or `None` when the host deferred the action. A reply with neither is `invalid`.
fn optional_string(fields: &Map<String, Value>, name: &str) -> PosterResult<Option<String>> {
    if fields.get("deferred") == Some(&Value::Bool(true)) {
        return Ok(None);
    }
    fields
        .get(name)
        .and_then(Value::as_str)
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| PosterError::new("invalid", format!("the host's reply has no {name}")))
}
