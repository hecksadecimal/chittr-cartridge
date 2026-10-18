//! Admin-configured first-guess reward. The setting is snapshotted into the winning Game before
//! its final guess is committed, so later configuration changes cannot reward old wins retroactively.

use super::*;

pub(super) const CONTROL: &str = "set-win-badge";
pub(super) const CONFIG_KEY: &str = "first-guess-badge";
pub(super) const MAX_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct BadgeReward {
    pub badge_id: String,
    pub status: BadgeRewardStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BadgeRewardStatus {
    Ready,
    Pending,
    Granted,
    Failed,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(super) fn configured_id() -> Result<Option<String>, HookError> {
    kv::get(CONFIG_KEY)
        .map_err(|error| fail("storage_error", error.to_string()))?
        .map(|entry| {
            String::from_utf8(entry.value)
                .ok()
                .filter(|id| valid_id(id))
                .ok_or_else(|| fail("invalid_state", "the first-guess badge ID is invalid"))
        })
        .transpose()
}

pub(super) fn configure(input: ControlInput) -> Result<(), HookError> {
    if !input
        .observer
        .as_ref()
        .is_some_and(|observer| observer.is_admin)
    {
        return Err(fail(
            "forbidden",
            "only site admins can configure Hurtle's badge",
        ));
    }
    let fields = input.fields.as_ref().filter(|fields| fields.len() == 1);
    let raw = fields
        .and_then(|fields| fields.get("badgeId"))
        .and_then(Value::as_str)
        .filter(|_| input.value.is_none())
        .ok_or_else(|| {
            fail(
                "bad_input",
                "supply a badge ID, or leave it blank to disable awards",
            )
        })?;
    let id = raw.trim();
    if raw.len() > MAX_ID_BYTES || (!id.is_empty() && !valid_id(id)) {
        return Err(fail(
            "bad_input",
            "badge IDs must be at most 128 letters, digits, hyphens or underscores",
        ));
    }
    // Share the reset worker's lease so a save cannot race a cleanup batch. No game initialization
    // or configuration write is permitted while a multi-batch reset is pending.
    with_reset_lock(|| {
        if reset_day()?.is_some() {
            return Err(fail(
                "reset_in_progress",
                "wait for Hurtle's cleanup before setting a badge",
            ));
        }
        if id.is_empty() {
            kv::delete(CONFIG_KEY).map(|_| ())
        } else {
            kv::set(CONFIG_KEY, id.as_bytes(), None)
        }
        .map_err(|error| fail("storage_error", error.to_string()))
    })?;
    push_render(PushTarget::All).map_err(|error| fail("push_error", error.to_string()))
}

pub(super) fn snapshot() -> Result<Option<BadgeReward>, HookError> {
    configured_id().map(|id| {
        id.map(|badge_id| BadgeReward {
            badge_id,
            status: BadgeRewardStatus::Ready,
            error: None,
        })
    })
}

pub(super) fn award(observer: &Observer, game: &mut Game) -> Result<(), HookError> {
    if !game.complete || !game.won || game.guesses.len() != 1 || game.first_guess_badge.is_none() {
        return Ok(());
    }
    let key = game_key(&observer.character_id);
    for _ in 0..4 {
        let (previous, mut stored) = read_game_version(&observer.character_id)?
            .ok_or_else(|| fail("game_updated", "the winning game was reset"))?;
        if stored.day != game.day || !stored.complete || !stored.won || stored.guesses.len() != 1 {
            return Err(fail("game_updated", "the winning game changed elsewhere"));
        }
        let Some(reward) = stored.first_guess_badge.as_mut() else {
            *game = stored;
            return Ok(());
        };
        if matches!(
            reward.status,
            BadgeRewardStatus::Granted | BadgeRewardStatus::Pending
        ) {
            // A success is terminal. Pending is ambiguous (the process may have crashed after the
            // host granted it), so never blindly repeat it.
            *game = stored;
            return Ok(());
        }
        let retrying_refusal = reward.status == BadgeRewardStatus::Failed;
        let badge_id = reward.badge_id.clone();
        reward.status = BadgeRewardStatus::Pending;
        reward.error = None;
        let claimed = serde_json::to_vec(&stored)
            .map_err(|error| fail("invalid_state", error.to_string()))?;
        if !kv::compare_and_swap(&key, Some(&previous), &claimed)
            .map_err(|error| fail("storage_error", error.to_string()))?
        {
            continue;
        }
        // Already owning this badge is success, not a duplicate-grant error. Both the winner ID
        // and held badges come from the trusted Observer, never a canvas message or form field.
        let result = if observer.badges.iter().any(|badge| badge.id == badge_id) {
            Ok(())
        } else {
            poster::grant_badge_to_character(&observer.character_id, &badge_id)
        };
        let reward = stored
            .first_guess_badge
            .as_mut()
            .expect("reward was claimed");
        match result {
            Ok(()) => reward.status = BadgeRewardStatus::Granted,
            Err(error) if error.code == "unavailable" => {
                // A transport failure may mean the host completed the grant but lost its reply.
                // Keep the claim pending so retries can't create an ambiguous duplicate.
                reward.status = BadgeRewardStatus::Pending;
                reward.error = Some(format!(
                    "The badge grant outcome is unknown: {error}. Check the Character's badges before retrying."
                ));
            }
            Err(error) => {
                // The host explicitly refused the action, so a later retry of the completed win is
                // safe after correcting the badge ID, permission or rate limit.
                reward.status = BadgeRewardStatus::Failed;
                let prefix = if retrying_refusal {
                    "The badge grant was refused again"
                } else {
                    "The first-guess badge could not be awarded"
                };
                reward.error = Some(format!("{prefix}: {error}"));
            }
        }
        // Never resurrect a game removed by a reset or overwrite a newly started day's game.
        write_game_if_unchanged(&key, &claimed, &stored)?;
        *game = stored;
        return Ok(());
    }
    Err(fail(
        "game_updated",
        "the badge reward changed elsewhere; refresh the card",
    ))
}
