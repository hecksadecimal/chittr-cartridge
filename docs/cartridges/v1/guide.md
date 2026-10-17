# Cartridge SDK guide (ABI v1)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

the sdk, the way i'd explain it to a friend on the palmhusk. hooks, kv, timers, poster actions, the canvas, and how to test
all of it on ur own machine before u upload anything. sit tight while i tweak the dials
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

this is the guide to the Rust SDK, `chittr-cartridge` (the `chittr_cartridge` crate). it covers what a Server Guest can
do, one piece at a time, and how the SDK's testing feature lets u run the Hooks natively with `cargo test`. the
[ABI](abi.md) is the rulebook, and every number here is in the [limits](limits.md) page too. the [cookbook](cookbook.md)
has recipes that put these pieces together.

the examples write their Hooks against the bindings `xtp plugin init` generates, and the SDK's `#[hook]` attribute does
the same job with less glue. this guide uses `#[hook]`. both end up as the same exports the host calls.

## Setting up

a new project has the SDK already. `chittr-cartridge new my-cartridge` writes this into its `Cargo.toml`:

```toml
[dependencies]
chittr-cartridge = { package = "chittr-cartridge-sdk", git = "https://github.com/hecksadecimal/chittr-cartridge" }

[lib]
name = "plugin"
crate-type = ["cdylib"]
```

the SDK has three features:

| Feature | On by default | What it adds |
| --- | --- | --- |
| `server` | yes | the Hook envelopes, the `#[hook]` attribute, and the KV, timer, Poster Action, Render Push and asset calls |
| `canvas` | no | the Canvas Guest's command-buffer encoder (`Canvas2D`) and the canvas host calls |
| `testing` | no | the in-memory host and the envelope builders, for `cargo test` (see [Testing](#testing)) |

a Server Guest uses `server`, which is the default. a Canvas Guest turns on `canvas` with `default-features = false`.
`testing` goes under `[dev-dependencies]` only, and the wasm build never uses it.

## Hooks

a Hook is a plain function. `#[hook]` takes its name from the function, checks the name against the list of Hooks, and
exports it, with JSON in and out. the function takes one envelope and returns one output:

```rust
use chittr_cartridge::prelude::*;

#[hook]
fn init(_input: InitInput) -> InitOutput {
    InitOutput::new("What should we have for lunch?")
}

#[hook]
fn render(input: RenderInput) -> RenderOutput {
    RenderOutput {
        template: "card".to_string(),
        data: json!({ "signedIn": input.observer.is_some() }),
        canvas: None,
    }
}
```

a Hook that can fail returns `Result<Output, HookError>`. an `Err` is a failure the Hook reports itself: the host gets
`{ "error": { code, message } }`, and the Hook is still a success as far as the host is concerned. a panic or a trap is
not a handled failure, so don't rely on one. the prelude exports `HookError::new(code, message)`.

```rust
#[hook]
fn on_control(input: ControlInput) -> Result<ControlOutput, HookError> {
    match input.observer {
        Some(observer) if observer.is_poster => Ok(ControlOutput::default()),
        Some(_) => Err(HookError::new("forbidden", "only the poster can do that")),
        None => Err(HookError::new("logged_out", "log in to do that")),
    }
}
```

the Hooks, when they run, and their deadlines. a Hook that runs past its deadline is a timeout, and the host discards that
guest instance.

| Hook | Runs when | Deadline | Input | Output |
| --- | --- | --- | --- | --- |
| `init` | the Chitt is published | 10 s | `InitInput` | `InitOutput` (its `content` is required) |
| `migrate` | a Cartridge Version is swapped in or rolled back | 10 s | `MigrateInput` | `MigrateOutput` (an `Err` keeps the old version) |
| `render` | every view of the card | 500 ms | `RenderInput` | `RenderOutput` |
| `on_view` | a real impression, once per Observer per 24 hours | 2 s | `ViewInput` | `ViewOutput` |
| `on_control` | an Observer uses a Control | 1 s | `ControlInput` | `ControlOutput` |
| `on_reply` | a direct reply is about to post (a gate) | 1 s | `ReplyGateInput` | `GateResult` |
| `on_reaction` | a reaction is about to be added (a gate) | 1 s | `ReactionGateInput` | `GateResult` |
| `on_rechitt` | a rechitt is about to be added (a gate) | 1 s | `RechittGateInput` | `GateResult` |
| `on_timer` | a timer the guest scheduled fires | 2 s | `TimerInput` | `TimerOutput` |
| `on_canvas_message` | the Canvas Guest sends a message | 2 s | `CanvasMessageInput` | `CanvasMessageOutput` |

a removal of a reaction or rechitt is a notification, not a gate. it runs in the background with the 2 s deadline, and its
return is ignored. the envelope's `added` field says which one it is.

`init` is the one Hook publishing needs. Chittr calls it once, when the Chitt is published, and takes the Chitt's text from its
`content`. a Cartridge without an `init` still passes `chittr-cartridge check`, with a warning, and Chittr refuses to publish
it. `chittr-cartridge new` writes an `init`, so a fresh project already has one.

`render` gets the Observer, who is the viewer, or `None` when they're logged out. every other envelope that has an Observer
works the same way, so check for `None` in any Hook that needs a login. the envelope fields are camelCase on the wire, and
the Rust field names are snake_case.

`render` runs on every view, so keep it cheap. its output is drawn by the card's Liquid template, and its `canvas` value is
the state a Canvas Guest reads, up to 16 KiB of JSON.

## Key-value store

every Chitt has its own KV store, and a guest can't name another Chitt's keys. keys are UTF-8 strings of at most 1 KiB.
values are bytes of at most 1 MiB. counters are `i64`.

```rust
use chittr_cartridge::kv;

kv::set("greeting", b"hello", None)?;                   // no TTL: keeps the key's expiry, if it has one
kv::set("session", b"open", Some(3600))?;               // a TTL in seconds: sets or replaces the expiry
let entry = kv::get("session")?;                        // Some(Entry { value, ttl_seconds }) or None
let views = kv::incr("views", 1, None)?;                // adds to an integer, 0 when there is none
let claimed = kv::set_if_absent("lock", b"1", Some(30))?;  // false when a live value is already there
let swapped = kv::compare_and_swap("slot", Some(b"a"), b"b")?; // None means "no live value"
let gone = kv::delete("greeting")?;                     // whether it had a live value
```

the rules that matter when you write one:

- a write with a TTL sets or replaces the expiry. a write without one keeps it, so `set` with no TTL doesn't clear a
  timer on a key. `incr` follows the same rule.
- `set_if_absent` and `compare_and_swap` that don't win return `Ok(false)`. that's a lost race, not an error.
- `incr` fails with `not_integer` on a key whose value isn't an integer. `by` must be within 2^53 in magnitude.
- `list(prefix)` and `get_all()` return at most 1,000 entries and say so with `truncated`. `get_many(&keys)` reads up to
  1,000 keys in one host call and returns the values in the same order.
- every host call costs about half a millisecond, and the cap per Hook call is 100 in `render` and 500 in every other Hook.
  past it, a call fails with `rate_limited`. a loop of `get`s is the usual way to hit that. use `get_many` or `get_all`.

the error codes are `unavailable`, `timeout`, `quota_exceeded`, `key_too_large`, `value_too_large`, `not_integer`,
`rate_limited` and `invalid_argument`. a Chitt's store holds 1,000,000 keys and 64 MiB, counting key and value bytes. the
[limits](limits.md) page has the full table.

## Timers

a Hook schedules a named timer, and the host runs the Server Guest's `on_timer` when it fires:

```rust
use chittr_cartridge::timer;

timer::schedule_timer("close", 60 * 60, json!({ "why": "deadline" }))?;
let cancelled = timer::cancel_timer("close")?;   // whether one was outstanding
```

- a delay is from 10 seconds to 30 days.
- scheduling a name that already has a timer replaces it.
- a Chitt can hold 100 timers outstanding. a new name past that fails with `quota_exceeded`.
- the payload is JSON of at most 16 KiB, and it comes back as `payload` in the `TimerInput`.
- a timer is delivered at least once. a delivery that fails is retried, up to 6 failed deliveries, and then it's dropped.
  so `on_timer` has to be safe to run twice. the cookbook's [timers that close things](cookbook.md#timers-that-close-things)
  recipe shows how.
- timers are held while the Chitt is disabled, and each overdue one fires once on re-enable.

```rust
#[hook]
fn on_timer(input: TimerInput) -> Result<TimerOutput, HookError> {
    if input.name == "close" {
        // close the poll, announce it once; see the poll example
    }
    Ok(TimerOutput::default())
}
```

## Poster Actions

a Server Guest can act as its Chitt's posting Character. a Poster Action runs under that Character's current authority,
and the host checks it on every call: the Character exists, it's theirs, it isn't suspended, and the user isn't banned.

```rust
use chittr_cartridge::poster::{self, EditArgs, Reaction};

let reply = poster::post_reply(&input.chitt.id, "the poll is closed.")?;  // Some(new post id), or None when deferred
poster::edit_chitt(EditArgs { content: "closed: pizza won.".to_string(), ..Default::default() })?;
poster::react(&post_id, Reaction::Like)?;
poster::follow(&character_id)?;
```

the catalog has the social actions: `post_reply`, `post_chitt`, `edit_chitt`, `delete_chitt` (the poster's own Chitts,
with the `post_deletion` permission), `react` and `unreact`, `rechitt` and `unrechitt`, `follow` and `unfollow`, and `pin`
and `unpin`. it also has admin actions (`ban_user`, `suspend_character`, `admin_delete_post`, badge management, and so on),
and each one needs its permission on the poster's account, checked on every call. the [ABI](abi.md#poster-actions-server-guest-extismhostuser)
has the table.

- a refused action is an error with one of these codes: `forbidden`, `not_found`, `invalid`, `denied`, `rate_limited`,
  `depth_exceeded` and `unavailable`.
- quotas: 5 actions per Hook call, 1 per `render`, and 30 a minute per Chitt, shared by social and admin actions.
- inside `init` and `migrate`, and in a gate (`on_reply`, `on_reaction`, `on_rechitt`), actions are deferred until the Chitt
  or the swap commits. the call then returns `None`, and the outcome shows only in the Poster Action log. the SDK's return
  types are `Option<String>` for that reason.
- an action that makes another Chitt's Hook run is subject to causation depth. past depth 3 it returns `depth_exceeded`.
- inside a gate, `poster::NEW_POST_REF` (`"$new"`) names the reply the gate is deciding. the host fills it in when the
  reply commits.

## Render Pushes

`push_render` tells the Chitt's Observers that its card has changed, so their clients fetch a fresh render:

```rust
use chittr_cartridge::push::{self, PushTarget};

push::push_render(PushTarget::All)?;                             // every Observer
push::push_render(PushTarget::Character(character_id))?;         // one Observer
```

a push is queued during the Hook call and published only after the call commits. a call that is canceled, traps or times
out publishes none. a Hook call may queue at most 20 distinct targets, and pushing the same target again in one call is
free. `push_render` is refused in `render` with `not_allowed`.

## Assets

a Cartridge's `assets/` folder is readable from the Server Guest with `asset`. the file comes back as bytes, already
decoded from the host's base64:

```rust
use chittr_cartridge::host::asset;

let toppings = asset("toppings.txt")?;      // Vec<u8>
```

the path is relative to `assets/`, with no `..` and no leading `/`. the errors are `not_found` (no such file, or a bad
path), `too_large` (over 4 MiB, the most one reply carries) and `invalid`. a Canvas Guest doesn't call `asset`. it asks for
images and sounds by path with `load_image` and `load_sound`, and the page fetches them.

## Logs

`log!` writes one line to the host's log, which the Chitt keeps for its failure log and its log ring:

```rust
use chittr_cartridge::prelude::*;

log!(LogLevel::Info, "rendering {}", input.chitt.id);
```

a line is cut to 512 bytes on a character boundary. a call keeps up to 50 lines, and a failed call keeps up to 20 of them in the
failure log. log what helps you find a bug in a failed call, and don't log every render.

## Controls

the manifest declares the Controls, and the host checks each use against them before the guest runs. `on_control` gets the
Control's name and its payload: `value` for a value Control, or `fields` for a fields Control. each value has already
been checked against the manifest, so the guest sees only the Controls it declares, with valid payloads.

```json
{
  "controls": {
    "vote": { "value": { "enum": ["pineapple", "anchovies", "extra-cheese"] } },
    "comment": { "fields": { "text": { "type": "string", "maxLength": 280 } } }
  }
}
```

a Control is rate limited before the guest runs: 10 uses per Observer per Chitt in any 10 seconds, and 60 uses per Character
across all Chitts in any 60 seconds. past either, the use gets the "slow down" answer, and no guest runs.

## Canvas Guests

a Canvas Guest runs in the viewer's browser. it draws with a `Canvas2D`-like context over a binary command buffer, and it
talks to the Server Guest only through messages. enable the `canvas` feature and call `Canvas2D::submit` once per frame.
the poll's `canvas/src/lib.rs` and the snake example are the full references. the pieces:

- the Canvas Guest exports `start` (required), `frame` (required) and `event` (optional). `frame` runs only after the guest
  called `request_frame`, and never faster than the effective rate, which is at most 60 frames a second and 15 under
  reduced motion.
- `set_fps` changes the requested rate, and `request_frame` asks for the next frame.
- input comes in as `pointer` and `key` events, and `focus`, `blur`, `resize`, `visibility` and `state` events tell the
  guest what changed.
- `send(message, seq)` sends a message to the Server Guest's `on_canvas_message`. each message gets one answer, a `reply`
  or an `error` event, with the same `seq`. at most 4 KiB, and 4 in flight per canvas.
- the Server Guest answers with `reply` (the JSON the canvas sees) and can ask for a redraw with `rerender: true`. a
  `rerender` draws the sender's card again through `render`, and that delivers fresh `canvas` state.
- images and sounds are loaded by path under `assets/`, with `load_image` and `load_sound`. a loaded image is drawn by its
  handle, and sounds play only on a real click that unmutes the canvas.

```rust
use chittr_cartridge::canvas::prelude::*;

let mut canvas = Canvas2D::new();
canvas.fill_color(rgba(16, 24, 32, 255)).fill_rect(0.0, 0.0, 320.0, 180.0);
canvas.submit()?;
```

the canvas state a `render` returns is sent to the canvas on every render, as the `state` event and the `start` input. keep it
small, and send the big things (a full leaderboard, say) as a reply to a message. the [cookbook](cookbook.md#a-canvas-game-loop)
has a game loop recipe.

## Gates

`on_reply`, `on_reaction` and `on_rechitt` run before the action commits. a gate returns `GateResult`, and the three helpers
say what to do:

```rust
#[hook]
fn on_reply(input: ReplyGateInput) -> GateResult {
    if input.content_raw.len() > 280 {
        GateResult::deny("keep it under 280 characters")
    } else {
        GateResult::allow()
    }
}
```

`GateResult::refuse()` refuses with the generic "This Chitt didn't accept that". `deny` takes the plain-text message the
composer shows beside the user's own text, and the host cuts it at 200 characters. a gate fails open: a trap, a timeout, an
`Err`, or a busy pool commits the action as allowed. so a gate that matters should be simple, and fast. a KV write it makes
stands even when the gate then denies.

## Testing

`cargo test` runs a Server Guest's Hooks on your machine. the `testing` feature replaces the host with a fake that keeps the
Chittr host's rules, and the SDK's envelope builders make the inputs. turn it on as a dev-dependency:

```toml
[dev-dependencies]
chittr-cartridge = { package = "chittr-cartridge-sdk", git = "https://github.com/hecksadecimal/chittr-cartridge", features = ["testing"] }
```

a test starts with `TestHost::new()`, which resets this thread's fake host and gives an empty store at the fixed time
1700000000 (2023-11-14T22:13:20Z). each test thread has its own fake, so tests run in parallel without sharing state.

```rust
use chittr_cartridge::testing::{self, HookClass, TestHost};

#[test]
fn a_vote_counts_once() {
    let host = TestHost::new();
    let chitt = testing::chitt("chitt-1", "poster-1");
    let voter = testing::observer("char-1");

    host.begin_hook(HookClass::Control);
    let input = testing::control(&chitt, Some(voter.clone()), "vote", "anchovies");
    assert!(my_poll::on_control(input).is_ok());
    host.begin_hook(HookClass::Control);
    let input = testing::control(&chitt, Some(voter), "vote", "anchovies");
    assert!(my_poll::on_control(input).is_ok());

    assert_eq!(host.kv_text("tally:anchovies").as_deref(), Some("1"));
}
```

what the handle gives a test:

- the clock: `now`, `set_now`, `advance`. the timers fire as the host would: `fire_timer(&chitt, name)` moves the clock to
  the timer's due time and returns the `on_timer` input, and `fire_due_timers(&chitt)` fires every timer that is due.
- the KV store: `put_kv` seeds a value (quotas apply), and `kv_value`, `kv_text`, `kv_ttl` and `kv_keys` read it back.
- the Poster Action log: `actions()` returns each action with its arguments and the reply the guest saw. `refuse_action`
  makes an action fail with a code and a message, and `allow_action` takes that back.
- Render Pushes (`pushes()`), log lines (`logs()`), and assets (`add_asset`), which `asset()` then reads.
- `begin_hook(HookClass)` sets the kind of Hook the next calls belong to. the host-call cap, the Poster Action quotas and the
  push rules depend on it. call it at the top of each Hook under test. the default is `Background`.

the envelope builders are `init`, `render`, `view`, `timer`, `migrate`, `control`, `control_fields`, `canvas_message`,
`reply_gate`, `reaction_gate` and `rechitt_gate`, plus `chitt` and `observer`. an Observer built with `observer(id)` has
the public profile fields the schema requires, filled with neutral values, and `.poster()` and `.admin()` set its flags.

the fake does what the host does for these: KV sizes, quotas, TTLs and the expiry rules, the 1,000-entry caps, timer
delays and the outstanding count, Render Push targets and the `render` rule, the host-call cap, and the Poster Action quotas
(per call and per minute), deferral in `init`, `migrate` and gates. it doesn't check permissions, read the Chitt's own
state, run the Liquid template, or model timing or concurrency. a Poster Action succeeds unless a test refuses it, and a
reaction always reports `added`. write the permission and state checks as tests of your own logic, and use a preview on
Chittr for the rest.

if your guest uses the generated types from `xtp plugin init` rather than the SDK's envelopes, as the poll example does,
send the SDK's envelope through JSON into the generated type. the poll's `wire` helper does this, and it fails the test if
the SDK's shape and the schema's disagree.

## Where next

- [limits](limits.md): every limit an author hits, with its number and the code that enforces it.
- [cookbook](cookbook.md): the poll, counters, per-Observer state, timers that close things, a canvas game loop, logged-out
  Observers, and the Preview and Tester workflow on Chittr.
- [abi](abi.md): the rulebook. every export, host function and draw op, and the schemas they're generated from.
- [quickstart](quickstart.md): from an empty folder to a packed Cartridge.
