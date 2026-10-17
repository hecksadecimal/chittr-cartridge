# Cartridge cookbook (ABI v1)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

recipes. the stuff i'd actually cook for u if u came over. each one is a small thing that works, a bit of code, and the
gotchas i learned the hard way. gimme some yummy yummy data and go nuts
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

each recipe leans on the [guide](guide.md) for the API and the [limits](limits.md) for the numbers, and the examples are
the full versions: `examples/poll`, `examples/snake` and `examples/paint`. the code here is the SDK's, through
`chittr_cartridge`, and it's checked the same way the examples are. run the tests with `cargo test`, and check and pack with
`chittr-cartridge check cartridge` and `chittr-cartridge pack cartridge`.

## The poll

a poll is a Control, a few counters, a timer, and a card. the example has all of it, and it's the one to read first:

- the options come from `assets/toppings.txt`, so the card and the Control's enum are one list. the manifest's `vote` enum
  must match the file, and `tests.rs` checks that it does.
- a vote is a `value` Control. `on_control` looks up the Observer's last vote, moves the tallies (`incr` by -1 on the old
  option, by 1 on the new one), and records the vote under `vote:<characterId>`.
- `render` reads the tallies with one `get_many` call, shows the Observer's own vote, and draws the bars on the Canvas Guest
  as `canvas` state.
- a close timer, scheduled in `init`, closes the poll and announces it. the poster's `close` Control does the same thing
  through the same function. the [timers recipe](#timers-that-close-things) below is the pattern.

the one thing to copy from it: a changed vote is two `incr`s and a `set`, and the `set` comes last. the example undoes the old
tally, adds the new one, and only then records the vote. if a call fails in the middle, the tallies can be off by one, and
the example takes that trade because its KV calls are the only thing that can fail. a poll that can't afford it would move
the tallies with a compare-and-swap, the way the snake example keeps its leaderboard.

## Counters with incr

`incr` adds to an integer and returns the new value, and it starts from 0 when the key has nothing:

```rust
use chittr_cartridge::kv;

let views = kv::incr("views", 1, None)?;     // the new total
kv::incr("tally:anchovies", -1, None)?;      // counters go down too
```

three things to know:

- a TTL on `incr` sets or replaces the key's expiry, so passing one on every call pushes the expiry out each time. that's
  right for "forget this after a day of quiet", and wrong for "a counter for today". for today's counter, set the key with
  a TTL once, and `incr` without one after that:

  ```rust
  let day = "views:2026-10-16";
  kv::set_if_absent(day, b"0", Some(2 * 24 * 60 * 60))?;   // only the first call of the day sets the expiry
  let views = kv::incr(day, 1, None)?;                      // keeps it
  ```

- read several counters with one call. `get_many(&["views", "tally:pineapple"])` is one host call, and a loop of `get`s is
  one call per key, against a cap of 100 in `render` and 500 everywhere else.
- `incr` on a key whose value isn't an integer fails with `not_integer`. keep counters in keys of their own, and don't
  share a key with a JSON blob.

## Per-Observer state

keep one key per Observer, named with the Character's id, and read it with the Observer the Hook was given:

```rust
use chittr_cartridge::envelope::Observer;
use chittr_cartridge::kv;

fn state_key(observer: &Observer) -> String {
    format!("state:{}", observer.character_id)
}

fn read_state(observer: &Observer) -> Result<Option<Value>, KvError> {
    Ok(kv::get(&state_key(observer))?.and_then(|entry| serde_json::from_slice(&entry.value).ok()))
}

fn write_state(observer: &Observer, state: &Value) -> Result<(), KvError> {
    kv::set(&state_key(observer), serde_json::to_vec(state).unwrap_or_default(), None)
}
```

- the key is the Observer's own. a Character can't name another's, and no Hook takes a key from the viewer's input without
  checking it. build the key from the envelope, not from the message.
- a logged-out viewer has no Observer, so there's no state to read or write. render a default card for them, and don't
  write anything.
- `list("state:")` finds every Observer's key, up to 1,000 of them, and says so with `truncated` when there are more. don't
  build a leaderboard that way on a busy Chitt. keep the top few in one key, updated with `compare_and_swap`, as the snake
  example does.

## Timers that close things

a timer is the way to do something later: close a poll, expire an offer, end a round. it's delivered at least once, so the
Hook that runs when it fires has to be safe to run twice.

```rust
use chittr_cartridge::prelude::*;
use chittr_cartridge::{kv, poster, timer};

const CLOSE: &str = "close";

#[hook]
fn init(_input: InitInput) -> Result<InitOutput, HookError> {
    timer::schedule_timer(CLOSE, 60 * 60, Value::Null)?;     // an hour from publish; 10 s to 30 days is the range
    Ok(InitOutput::new("Which pizza topping is best?"))
}

#[hook]
fn on_timer(input: TimerInput) -> Result<TimerOutput, HookError> {
    if input.name == CLOSE {
        close_once(&input.chitt.id)?;
    }
    Ok(TimerOutput::default())
}

/// The same function closes the poll from the poster's Control, so the result is announced once either way.
fn close_once(chitt_id: &str) -> Result<(), HookError> {
    if !kv::set_if_absent("announced", b"1", None)? {
        return Ok(());                                       // already announced
    }
    if let Err(error) = poster::post_reply(chitt_id, "The poll is closed.") {
        let _ = kv::delete("announced");                     // release the claim, so the retry can announce
        return Err(error.into());
    }
    Ok(())
}
```

- the claim is `set_if_absent`, taken before the post. two deliveries, or a timer and a Control, can't both win it.
- when the post fails, the claim comes back out. the host retries the timer, up to 6 failed deliveries before it drops the
  timer, and the retry can still announce. without the release, the retry finds the claim and the result never shows.
- `schedule_timer` with a name that's already scheduled replaces it. to move a deadline, schedule it again. `cancel_timer`
  returns whether one was outstanding, so call it when the thing it guards happens early.
- timers are held while the Chitt is disabled, and each overdue one fires once when it's re-enabled. don't count on the
  timer firing at its exact second.
- a Chitt can hold 100 timers. one timer per deadline is fine, and a timer per Observer is the way to run into that limit.

the poll's test runs this exact path: the timer fires, a second delivery changes nothing, a failed post is retried, and the
Control and the timer announce once between them. see `examples/poll/src/tests.rs`.

## A canvas game loop

a Canvas Guest runs the game in the viewer's browser, and the Server Guest keeps score. the shape:

- `start` sets the frame rate with `set_fps` and asks for the first frame with `request_frame`. the snake example uses 12 fps.
- `frame` steps the game with the frame's `dt`, draws the whole scene, calls `Canvas2D::submit`, and asks for the next
  frame with `request_frame`. nothing is cleared between frames, so draw everything each time.
- `event` turns pointer and key input into state changes. a change that needs a redraw asks for a frame. it doesn't draw
  on its own.
- on game over, the canvas sends the score once with `send(json!({ "type": "score", "score": points }), Some(seq))`. the
  answer comes back as a `reply` or an `error` event with that `seq`.

```rust
use chittr_cartridge::canvas::prelude::*;

// `Game` is your own type: the snake's board, its snake and its food. this is the frame's drawing, and its shape.
fn draw(game: &Game) -> Result<(), CanvasError> {
    let mut canvas = Canvas2D::new();
    canvas.fill_color(rgba(16, 24, 32, 255)).fill_rect(0.0, 0.0, game.width, game.height);
    // ... the snake, the food, the score
    canvas.submit()?;
    request_frame()
}
```

the server side is an `on_canvas_message` that checks the message and saves it. the snake example rejects anything that
isn't `type: score` with a score in range, refuses a logged-out viewer, and saves the score with a compare-and-swap retry
loop, so two saves at once can't lose one. it answers with `saved` and asks for a `rerender` when the board changed, which
draws the card again with fresh state.

gotchas for a game:

- a message gets one answer, and a canvas has 4 in flight at most. send on game over, not every frame. a canvas that sends
  too often gets `slow_down`, and a signed-in Observer is limited to 40 messages in any 2 seconds per Chitt.
- a message is at most 4 KiB of JSON. a replay doesn't fit. send the score and a short summary.
- the canvas can be off-screen or paused. input isn't sent while it is, so don't count on an event to end a round. end it
  from `frame` when the game's own clock says so.
- a `reply` goes only to the canvas that asked. to change the card for everyone, use a Render Push or a `rerender`.

## Handling logged-out Observers

a logged-out viewer is the normal case for most of a Chitt's traffic, so every Hook that takes an Observer has to handle
`None` (or `null` on the canvas side):

```rust
#[hook]
fn render(input: RenderInput) -> RenderOutput {
    let signed_in = input.observer.is_some();
    RenderOutput {
        template: "card".to_string(),
        data: json!({ "signedIn": signed_in, "prompt": (if signed_in { "vote below" } else { "log in to vote" }) }),
        canvas: None,
    }
}
```

- `render` draws the card for whoever is looking. don't assume an Observer, and don't write state for one that's missing.
- a Control's envelope names the Observer as an optional field too. check it in `on_control`, and answer `logged_out` in the
  error, as the poll does.
- a canvas message from a logged-out viewer carries `observer: null`. a canvas message gets the anonymous session's
  limit, which is 40 in any 2 seconds per session per Chitt, plus 60 a second per IP across Chitts. a score from them is
  refused, as the snake example does.
- a gate's envelope carries the acting Character as its Observer, and it's checked the same way. a removal of a reaction or
  rechitt is a notification, and its return is ignored.

## Testing on Chittr: Previews and Testers

the `cargo test` suite and `chittr-cartridge check` catch what's in your code. Chittr catches what only a live Chitt shows:
the real render with a real Observer, the Poster Action authority, the Render Push, the timer actually firing. the workflow:

1. run `cargo test`, then `./build.sh`, then `chittr-cartridge check cartridge`. don't upload what doesn't pass them.
2. pack it with `chittr-cartridge pack cartridge`, or run `chittr-cartridge watch cartridge` to repack on every save.
3. upload the `.cartridge` in the composer with **Publish as preview** on. a preview is visible only to u and the Testers
   u add, and its cartridge runs for real. publishing calls `init`, so the Chitt's text is the one `init` returned.
4. add a Tester, a second account, and check the card in both states: logged in as u, and logged in as the Tester. then
   look at a view with no login at all.
5. try each Control, and each canvas action, twice in a row, to see the rate limit and the "already" answers. a Control is
   limited to 10 uses per Observer per Chitt in any 10 seconds, and the tester is the one who'll notice it first.
6. to iterate, change the code, rebuild, and use **Replace** in the **Cartridge…** panel. each replacement makes a new
   Cartridge Version, and an earlier one can be rolled back to. a Hook's `migrate` runs on the swap, so keep your
   storage compatible with the version before it.

when something goes wrong, look in two places on the Chitt. the **Poster Action log** shows every action the guest made,
with its outcome: the `denied`, `forbidden` and `rate_limited` answers all show up there, and so does a deferred action
from `init` or a gate. the **Failures log** shows each Hook call that failed, with the guest's own log lines for the call.
keep the log lines short, and log the input you'd want to see when the Hook breaks.

when the Cartridge is ready for everyone, publish it the usual way, with the preview box off.
