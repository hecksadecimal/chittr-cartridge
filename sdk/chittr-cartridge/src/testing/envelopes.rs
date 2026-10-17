//! Envelope builders: the Hook inputs a test hands to a Hook, typed as the host sends them. Each builder fills in
//! `v` and `hook`, so a test names only what the Hook reads. The `timer` builder uses the fake clock.

use serde_json::{json, Map, Value};

use crate::envelope::{
    CanvasMessageInput, CartridgeVersionRef, ChittRef, ControlInput, InitInput, MigrateInput,
    Observer, ReactionGateInput, RechittGateInput, RenderInput, ReplyGateInput, TimerInput,
    ViewInput,
};
use crate::ABI_VERSION;

/// A Chitt the Cartridge is attached to, with a fixed creation time.
pub fn chitt(id: &str, poster_id: &str) -> ChittRef {
    ChittRef {
        id: id.to_string(),
        poster_id: poster_id.to_string(),
        created_at: "2026-10-01T00:00:00Z".to_string(),
    }
}

/// A signed-in Observer with no flags set. `user_id` is `user-<character id>`. The public profile fields the schema
/// requires (trolltag, discriminator, badges and the rest) are filled with neutral values, so the envelope matches
/// the wire shape. Set the ones a test reads through `profile`.
pub fn observer(character_id: &str) -> Observer {
    let profile = match json!({
        "trolltag": "",
        "discriminator": "0001",
        "avatarUrl": null,
        "bannerUrl": null,
        "bio": null,
        "ageSweeps": null,
        "bloodCaste": "",
        "textColor": "",
        "kultScore": 0,
        "kullScore": 0,
        "badges": [],
        "frond": false,
    }) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    Observer {
        character_id: character_id.to_string(),
        user_id: format!("user-{character_id}"),
        name: character_id.to_string(),
        follows_poster: false,
        followed_by_poster: false,
        show_ooc: false,
        do_not_disturb: false,
        mutuals_only_messages: false,
        is_poster: false,
        is_poster_user: false,
        is_admin: false,
        profile,
    }
}

impl Observer {
    /// Makes this Observer the Chitt's poster: its Character and its account.
    pub fn poster(mut self) -> Self {
        self.is_poster = true;
        self.is_poster_user = true;
        self
    }

    /// Makes this Observer an admin of the site.
    pub fn admin(mut self) -> Self {
        self.is_admin = true;
        self
    }
}

fn envelope_fields(hook: &str, chitt: &ChittRef) -> (u32, String, ChittRef) {
    (ABI_VERSION, hook.to_string(), chitt.clone())
}

/// The input to `init`.
pub fn init(chitt: &ChittRef) -> InitInput {
    let (v, hook, chitt) = envelope_fields("init", chitt);
    InitInput { v, hook, chitt }
}

/// The input to `render`. `observer` is `None` for a logged-out viewer.
pub fn render(chitt: &ChittRef, observer: Option<Observer>) -> RenderInput {
    let (v, hook, chitt) = envelope_fields("render", chitt);
    RenderInput {
        v,
        hook,
        chitt,
        observer,
    }
}

/// The input to `on_view`.
pub fn view(chitt: &ChittRef, observer: Option<Observer>) -> ViewInput {
    let (v, hook, chitt) = envelope_fields("on_view", chitt);
    ViewInput {
        v,
        hook,
        chitt,
        observer,
    }
}

/// The input to `on_timer` for a timer named `name` with `payload`, due now on the fake clock.
pub fn timer(chitt: &ChittRef, name: &str, payload: Value) -> TimerInput {
    timer_due(chitt, name, payload, crate::testing::fake::now())
}

pub(crate) fn timer_due(chitt: &ChittRef, name: &str, payload: Value, due: u64) -> TimerInput {
    let (v, hook, chitt) = envelope_fields("on_timer", chitt);
    TimerInput {
        v,
        hook,
        chitt,
        name: name.to_string(),
        payload,
        scheduled_for: iso8601(due),
    }
}

/// The input to `migrate`, from one Cartridge Version to another.
pub fn migrate(
    chitt: &ChittRef,
    from: CartridgeVersionRef,
    to: CartridgeVersionRef,
) -> MigrateInput {
    let (v, hook, chitt) = envelope_fields("migrate", chitt);
    MigrateInput {
        v,
        hook,
        chitt,
        from,
        to,
    }
}

/// The input to `on_control` for a value Control, such as `vote` with the value `anchovies`.
pub fn control(
    chitt: &ChittRef,
    observer: Option<Observer>,
    name: &str,
    value: &str,
) -> ControlInput {
    ControlInput {
        value: Some(value.to_string()),
        ..control_base(chitt, observer, name)
    }
}

/// The input to `on_control` for a fields Control, with the fields the Observer filled in.
pub fn control_fields(
    chitt: &ChittRef,
    observer: Option<Observer>,
    name: &str,
    fields: Map<String, Value>,
) -> ControlInput {
    ControlInput {
        fields: Some(fields),
        ..control_base(chitt, observer, name)
    }
}

fn control_base(chitt: &ChittRef, observer: Option<Observer>, name: &str) -> ControlInput {
    let (v, hook, chitt) = envelope_fields("on_control", chitt);
    ControlInput {
        v,
        hook,
        chitt,
        observer,
        control: name.to_string(),
        value: None,
        fields: None,
    }
}

/// The input to `on_canvas_message`. `seq` is set when the canvas sent one.
pub fn canvas_message(
    chitt: &ChittRef,
    observer: Option<Observer>,
    message: Value,
    seq: Option<u32>,
) -> CanvasMessageInput {
    let (v, hook, chitt) = envelope_fields("on_canvas_message", chitt);
    CanvasMessageInput {
        v,
        hook,
        chitt,
        observer,
        message,
        seq,
    }
}

/// The input to `on_reply` for a direct reply the Observer is about to post. Its raw and quirked text are both
/// `content`, and it is not OOC.
pub fn reply_gate(
    chitt: &ChittRef,
    observer: Observer,
    content: &str,
    reply_to_post_id: &str,
) -> ReplyGateInput {
    let (v, hook, chitt) = envelope_fields("on_reply", chitt);
    ReplyGateInput {
        v,
        hook,
        chitt,
        character_id: observer.character_id.clone(),
        observer: Some(observer),
        content_raw: content.to_string(),
        content_quirked: content.to_string(),
        is_ooc: false,
        reply_to_post_id: reply_to_post_id.to_string(),
    }
}

/// The input to `on_reaction`: a gate when `added` is true, a notification when false.
pub fn reaction_gate(
    chitt: &ChittRef,
    observer: Observer,
    reaction: &str,
    added: bool,
) -> ReactionGateInput {
    let (v, hook, chitt) = envelope_fields("on_reaction", chitt);
    ReactionGateInput {
        v,
        hook,
        chitt,
        observer: Some(observer),
        reaction_type: reaction.to_string(),
        added,
    }
}

/// The input to `on_rechitt`: a gate when `added` is true, a notification when false.
pub fn rechitt_gate(chitt: &ChittRef, observer: Observer, added: bool) -> RechittGateInput {
    let (v, hook, chitt) = envelope_fields("on_rechitt", chitt);
    RechittGateInput {
        v,
        hook,
        chitt,
        observer: Some(observer),
        added,
    }
}

/// Formats a Unix time as an ISO 8601 UTC instant, such as `2023-11-14T22:13:20Z`.
pub(crate) fn iso8601(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    let year = year_of_era as i64 + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
