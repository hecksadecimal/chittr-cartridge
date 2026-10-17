//! Snake's Server Guest stores each Observer's best score and draws the top-five leaderboard. Written against the
//! bindings `xtp plugin init` generates from `docs/cartridges/v1/server.xtp.yaml` (see the README).

mod pdk;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use extism_pdk::Error;
use pdk::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

const SCORES_KEY: &str = "scores";
const MAX_SCORE: u64 = 1_000_000;
const MAX_CAS_ATTEMPTS: usize = 8;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Score {
    character_id: String,
    trolltag: String,
    score: u64,
}

pub(crate) fn init(_input: InitInput) -> Result<InitOutput, Error> {
    Ok(InitOutput { content: "Snake".to_string(), ..Default::default() })
}

pub(crate) fn render(_input: RenderInput) -> Result<RenderOutput, Error> {
    Ok(match read_scores() {
        Ok(scores) => RenderOutput {
            template: "card".to_string(),
            data: object(json!({ "scores": scores })),
            canvas: Some(Map::new()),
            error: None,
        },
        Err(error) => RenderOutput {
            template: String::new(),
            data: Map::new(),
            canvas: None,
            error: Some(error),
        },
    })
}

pub(crate) fn on_canvas_message(input: CanvasMessageInput) -> Result<CanvasMessageOutput, Error> {
    Ok(match save_message(&input) {
        Ok(changed) => CanvasMessageOutput {
            reply: Some(object(json!({ "saved": changed }))),
            rerender: Some(changed),
            error: None,
        },
        Err(error) => CanvasMessageOutput { reply: None, rerender: None, error: Some(error) },
    })
}

/// Checks a score message and saves it. Returns whether the leaderboard changed.
fn save_message(input: &CanvasMessageInput) -> Result<bool, HookError> {
    let message = &input.message;
    if message.get("type").and_then(Value::as_str) != Some("score") {
        return Err(fail("bad_input", "unknown game message"));
    }
    let score = message
        .get("score")
        .and_then(Value::as_u64)
        .filter(|score| (1..=MAX_SCORE).contains(score))
        .ok_or_else(|| fail("bad_score", "score must be between 1 and 1000000"))?;
    let observer = input
        .observer
        .as_ref()
        .ok_or_else(|| fail("logged_out", "log in to save a score"))?;
    let trolltag = Some(observer.trolltag.as_str())
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| fail("bad_observer", "your Character has no trolltag"))?;
    let entry = Score {
        character_id: observer.character_id.clone(),
        trolltag: trolltag.to_string(),
        score,
    };

    let changed = save_score(entry)?;
    if changed {
        push_render_all()?;
    }
    Ok(changed)
}

fn read_scores() -> Result<Vec<Score>, HookError> {
    let Some(bytes) = kv_read(SCORES_KEY)? else {
        return Ok(Vec::new());
    };
    serde_json::from_slice(&bytes).map_err(|error| fail("invalid_scores", error.to_string()))
}

fn save_score(score: Score) -> Result<bool, HookError> {
    for _ in 0..MAX_CAS_ATTEMPTS {
        let previous = kv_read(SCORES_KEY)?;
        let mut scores: Vec<Score> = match &previous {
            Some(bytes) => serde_json::from_slice(bytes)
                .map_err(|error| fail("invalid_scores", error.to_string()))?,
            None => Vec::new(),
        };
        if let Some(existing) = scores
            .iter_mut()
            .find(|entry| entry.character_id == score.character_id)
        {
            if existing.score >= score.score {
                return Ok(false);
            }
            *existing = score.clone();
        } else {
            scores.push(score.clone());
        }
        scores.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.trolltag.cmp(&b.trolltag))
                .then_with(|| a.character_id.cmp(&b.character_id))
        });
        scores.truncate(5);
        let value = serde_json::to_vec(&scores)
            .map_err(|error| fail("invalid_scores", error.to_string()))?;
        let expected = previous.as_deref();
        if kv_compare_and_swap(SCORES_KEY, expected, &value)? {
            return Ok(scores
                .iter()
                .any(|entry| entry.character_id == score.character_id));
        }
    }
    Err(fail("storage_busy", "could not save the score; try again"))
}

// Host calls. Each returns a failure as a HookError with the code the SDK reports for it, so the Hooks above
// hand it back in their `error` field, never as a trap.

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// A failure the Hook reports itself: a code and a message.
fn fail(code: &str, message: impl Into<String>) -> HookError {
    HookError { code: code.to_string(), message: message.into() }
}

/// The host call itself failed, before it answered.
fn host_failed(error: Error) -> HookError {
    fail("unavailable", error.to_string())
}

/// A host reply that lacks the field its call promises.
fn malformed() -> HookError {
    fail("invalid", "the host's reply is missing a field")
}

/// The code of a generated error enum, as its wire string.
fn code_of<T: Serialize>(code: &T) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn kv_error(error: KvError) -> HookError {
    fail(&code_of(&error.code), error.message)
}

fn kv_read(key: &str) -> Result<Option<Vec<u8>>, HookError> {
    let reply = pdk::kv_get(KvKeyInput { key: key.to_string() }).map_err(host_failed)?;
    if let Some(error) = reply.error {
        return Err(kv_error(error));
    }
    reply.value.map(|value| decode(&value)).transpose()
}

/// Sets `key` to `value` if its live value equals `expected`. `Ok(false)` means the value didn't match: a lost race.
fn kv_compare_and_swap(key: &str, expected: Option<&[u8]>, value: &[u8]) -> Result<bool, HookError> {
    let reply = pdk::kv_compare_and_swap(KvCompareAndSwapInput {
        key: key.to_string(),
        expected: expected.map(|bytes| STANDARD.encode(bytes)),
        value: STANDARD.encode(value),
    })
    .map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(kv_error(error)),
        None => reply.swapped.ok_or_else(malformed),
    }
}

fn decode(text: &str) -> Result<Vec<u8>, HookError> {
    STANDARD.decode(text).map_err(|_| fail("invalid", "a stored value is not base64"))
}

/// Queues a Render Push for every Observer of this Chitt.
fn push_render_all() -> Result<(), HookError> {
    let reply = pdk::push_render(PushRenderInput { character_id: None }).map_err(host_failed)?;
    match reply.error {
        Some(error) => Err(fail(&code_of(&error.code), error.message)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaderboard_orders_scores_and_breaks_ties_by_trolltag() {
        let mut scores = vec![
            Score {
                character_id: "b".into(),
                trolltag: "zed".into(),
                score: 10,
            },
            Score {
                character_id: "a".into(),
                trolltag: "ada".into(),
                score: 10,
            },
            Score {
                character_id: "c".into(),
                trolltag: "cat".into(),
                score: 20,
            },
        ];
        scores.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.trolltag.cmp(&b.trolltag))
                .then_with(|| a.character_id.cmp(&b.character_id))
        });
        assert_eq!(
            scores
                .iter()
                .map(|entry| entry.trolltag.as_str())
                .collect::<Vec<_>>(),
            ["cat", "ada", "zed"]
        );
    }
}
