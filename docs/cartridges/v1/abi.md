# Cartridge ABI v1

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

this is the rulebook. every export, every host function, every byte of the draw ops. the stuff in here is what the host
actually does, so if the prose and the schema ever disagree, the schema wins and this page is the one with the bug
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

This is the normative ABI for `v: 1` Cartridges: what the host calls, what a guest may call, and the byte layout of a
frame's draw ops. The Rust SDK in this repository (`sdk/chittr-cartridge`) and the Chittr host both follow it. A change
that alters an existing byte or shape is a new `v`. An addition (a new opcode, a new host function) keeps `v: 1` and is
listed under "Version history".

Two guests make up a Cartridge. The **Server Guest** (`server.wasm`) runs on the server. Its Hooks and their envelopes
are described in `server.xtp.yaml`, and the sections below cover its host functions. The **Canvas Guest** (`canvas.wasm`)
runs in a Web Worker in the browser, and is described here, in the sections on exports, host functions, the command
buffer and the draw-op table.

**The schemas are the source of truth.** `docs/cartridges/v1/server.xtp.yaml` and `docs/cartridges/v1/canvas.xtp.yaml`
(XTP schema `v1-draft`) name every export and host function, with their shapes and descriptions. The prose below explains
them. Where the two disagree, the schema is the contract and this file is the bug. The Chittr host is checked against the
schemas.

**The rule for changes.** Any change that adds a Hook or a host function updates the schema in the same change. A Hook or
host function that exists in the host but not in the schema is not part of v1.

The Server Guest's `Observer` includes `isSuspended`, a boolean indicating whether the viewing Character currently has an
active suspension. It does not expose the suspension reason or end time.

## 1. Canvas Guest exports

`canvas.wasm` must export `memory` and the functions `start` and `frame`. It may export `event`. Any other export is refused
at upload, and so is a declared memory maximum over 64 MiB (1024 pages). A module that declares no maximum is capped at
64 MiB by the host.

Every export takes one JSON object and returns one JSON object. The return is `{}` on success, or
`{ "error": { "code": string, "message": string } }` when the guest reports a failure itself. An `error` return counts as
a crash. So do a trap, a malformed command buffer, a non-object reply and a call that runs past its limit (section 6).

### `start` (required)

Input:

```json
{
  "width": 640,
  "height": 360,
  "dpr": 2,
  "prefersReducedMotion": false,
  "state": { "bars": [{ "label": "Pepperoni", "votes": 42 }] },
  "fullscreen": false
}
```

`width` and `height` are the backing store's size in device pixels (`css width × dpr`, capped at 2048 per side, aspect ratio
kept). Draw in those pixels. `state` is the render's `canvas` value, or `null`.

### `frame` (required)

Input: `{ "t": 16234.5, "dt": 16.7 }`. `t` is the frame time in milliseconds since the page's time origin, `dt` the time since
the previous frame. A frame is delivered only after the guest called `request_frame`, and never faster than the effective
rate (section 3).

### `event` (optional)

Input: `{ "type": string, ...fields }`. The types in v1:

| `type` | Other fields | When |
| --- | --- | --- |
| `state` | `state` (the render's `canvas` value, or `null`) | the render changed, delivered while visible |
| `resize` | `width`, `height`, `fullscreen` | the backing store changed size |
| `visibility` | `visible` (boolean) | the canvas went off-screen or its tab hid (`false`), or came back (`true`) |
| `pointer` | `action`, `x`, `y`, `width`, `height`, `pointerId`, `pointerType`, `buttons` | a pointer went down, moved, went up, or was cancelled (section 1.1) |
| `key` | `action`, `key`, `code` | a key went down or up, while the canvas has focus (section 1.1) |
| `focus` / `blur` | none | the canvas gained or lost focus (section 1.1) |
| `reply` | `reply` (any JSON), `seq` (when the message had one) | the answer to a `send`, on success (section 1.2) |
| `error` | `code`, `message` (optional), `seq` (when the message had one), or `handle` (for a load) | a `send` failed, its answer was not usable (section 1.2), or an image or sound did not load (section 1.3) |
| `image_loaded` | `handle`, `width`, `height` (device pixels) | an image from `load_image` is decoded and can be drawn (section 1.3) |
| `sound_loaded` | `handle` | a sound from `load_sound` is decoded and can be played (section 1.3) |

A guest ignores types it does not know. While the canvas is paused, `frame` and every other event are held, and the latest
`state` is delivered on resume. Input is not sent while the canvas is paused or off-screen.

`start`'s `fullscreen` and each `resize`'s `fullscreen` say whether the card is fullscreen at that moment. A guest cannot
enter fullscreen itself (section 1.4).

### 1.1 Input

- `pointer`: `action` is `down`, `move`, `up` or `cancel`. `x` and `y` are in CSS pixels, relative to the canvas element's
  box, and `width` and `height` are that box's CSS size, so a guest scales by `backing width / width`. `pointerType` is
  `mouse`, `touch` or `pen`; `buttons` is the DOM bitmask. Moves are coalesced to one per frame. Touch and pen arrive as
  pointers; `touch-action` is `pan-y` by default, so the feed still scrolls, and `<lc-canvas touch="none">` opts out.
- `key`: `action` is `down` or `up`, with the DOM's `key` and `code` (each at most 32 characters). Keys reach the guest only
  while the canvas has focus. The canvas is focusable (`tabindex`), a click focuses it, and Escape gives the focus back to
  the page; Escape is not delivered to the guest.
- `focus` and `blur` are sent when focus moves in or out.
- Wheel, clipboard and pointer lock are not delivered.

Example, a pointer release:

```json
{ "type": "pointer", "action": "up", "x": 120.5, "y": 64, "width": 320, "height": 180, "pointerId": 1, "pointerType": "mouse", "buttons": 0 }
```

### 1.2 Messages

`send` (section 2) starts a message to the Server Guest's `on_canvas_message`. Each message gets exactly one answer, as an
event: a `reply` on success, or an `error`. Both carry the `seq` the message was sent with, when it had one. A message with
no `seq` gets its answer without one.

```json
{ "type": "reply", "seq": 3, "reply": { "label": "Pineapple", "votes": 42, "percent": 30 } }
{ "type": "error", "seq": 3, "code": "slow_down" }
```

`error` codes: `slow_down` (rate limited), `busy` (the Server Guest pool's queue was full, or this canvas already had 4
messages in flight), `disabled` (the Chitt is switched off), `stale` (the card was out of date and has been drawn again),
`bad_token`, `not_found`, `too_large` (over 4 KiB), `invalid`, `timeout` (no answer within 5 s, or the Server Guest ran past
its 2 s deadline), `network`, `trap`, `invalid_return` and `guest_error` (the Server Guest failed; a guest's own code is not
passed on).

### 1.3 Images and sounds

The worker cannot fetch, so the page does the loading. A guest names an asset by its path under `assets/` (for example
`pizza.webp`). The call returns a handle at once, and the page then fetches and decodes the asset, and the guest hears back
with an event. Loading the same path twice returns the same handle and loads nothing new.

- An image's decoded side is at most 4096 px. A larger one, or one that fails to decode or fetch, is an `error` event with
  the handle: `{ "type": "error", "handle": 1, "code": "load_failed" | "too_large", "message": "..." }`.
- A sound is decoded into the page's one shared `AudioContext`. Its formats are whatever the browser decodes.
- Images are drawn with `drawImage` (0x50, section 5). Sounds are played only by `play_sound` (section 2).
- A `drawImage` whose image is not loaded yet is skipped, as an unknown handle is.

### 1.4 Fullscreen and sound controls

These belong to the host, not to the guest. The guest can't press them and can't tell a press from a script.

- **Fullscreen:** `<lc-canvas fullscreen>` makes the host draw an expand button on the card. Only a real click (or a key
  activation of that button) enters fullscreen. Esc, a window blur, the tab hiding, the browser leaving fullscreen, or the
  button again leave it. The canvas is letterboxed to keep its aspect ratio, and the guest is told through `resize` with
  `fullscreen` true or false.
- **Sound:** a canvas is muted until its speaker is turned on. The host draws the speaker toggle once the guest has called
  `load_sound`. A real click on it unmutes the canvas, and that click also unlocks the browser's audio. While muted,
  `play_sound` is dropped: it still returns an id, but nothing plays and no slot is taken. Muting stops every sound on the
  canvas.
- Nothing plays while the canvas is paused, off-screen or in a hidden tab, and pausing stops every sound, loops included. A
  sound that was playing does not resume by itself.

## 2. Host functions (`extism:host/user`)

Every host function takes one JSON object and returns one JSON object, with the exception of `submit`. A host function never
traps on bad input: it returns `{ "error": { "code", "message" } }` instead. Codes are `invalid` (a field of the wrong type),
`not_running` (the canvas has stopped) and `host` (the runtime failed); `send` adds `busy` and `too_large`. The media functions
add `limit` (too many loads), `not_loaded` (the sound is still decoding), `load_failed` (the sound or image did not load) and
`busy` (eight sounds already play).

| Function | Input | Output | Notes |
| --- | --- | --- | --- |
| `submit` | binary: the command buffer (section 4) | nothing | The only path to the canvas. Malformed bytes crash the canvas. |
| `request_frame` | `{}` | `{}` | Asks for one more `frame`. A guest that wants animation calls it again each frame. |
| `set_fps` | `{ "fps": number }` | `{}` | Any time. Clamped to 1 to 60. Reduced motion caps it at 15. |
| `measure_text` | `{ "family": string, "size": number, "weight": number, "text": string }` | `{ "width": number, "ascent": number, "descent": number }` | Pixels, measured in the same font `font` would select. |
| `send` | `{ "message": any JSON, "seq"?: integer }` | `{}`, or `{ "error": { "code": "busy" \| "too_large" \| "invalid", ... } }` | Sends one message to the Server Guest's `on_canvas_message` (section 1.2). At most 4 KiB of JSON, and 4 in flight per canvas. The answer comes later as a `reply` or `error` event. |
| `load_image` | `{ "path": string }` | `{ "handle": integer }`, or `{ "error" }` | Asks the page to fetch and decode `assets/{path}` (section 1.3). The image arrives later as `image_loaded` or `error`. A path is relative to `assets/`, with no `..` and no leading `/`. Loading a path again returns its handle. At most 32 images and 32 sounds per canvas. |
| `load_sound` | `{ "path": string }` | `{ "handle": integer }`, or `{ "error" }` | As `load_image`, for a sound. It arrives later as `sound_loaded` or `error`. |
| `play_sound` | `{ "handle": integer, "volume"?: number, "loop"?: boolean }` | `{ "id": integer }`, or `{ "error" }` | Plays a loaded sound. `volume` is clamped to 0 to 1 (default 1); `loop` defaults to false. While muted or paused the call is dropped: it returns an id and plays nothing. Past 8 sounds playing on one canvas it answers `busy`. A sound that is not loaded yet answers `not_loaded`. |
| `stop_sound` | `{ "id": integer }` | `{}`, or `{ "error" }` | Stops one playback. An id that is not playing is a no-op. |

Example, `measure_text`:

```json
{ "family": "Mono", "size": 12, "weight": 700, "text": "Pepperoni" }
→ { "width": 71.2, "ascent": 9.1, "descent": 2.3 }
```

`request_frame` and `set_fps` are the only host functions that pace drawing. Nothing is cleared between frames: each frame draws
what it needs (immediate mode).

### Hook returns

Every Hook returns one JSON object, and the host refuses any other shape as `invalid_return`. A Hook's normal return is `{}`,
where it has no fields to return. A failure the Hook handled itself is `{ "error": { "code", "message" } }`, and that is the
only other shape a Hook may return. XTP has no unions, so no Hook returns a bare `true`, `false`, string or array.

### Gates (`on_reply`, `on_reaction`, `on_rechitt`)

An addition is a gate: the host asks the Server Guest before the action commits. Its return is a plain object,
`{ "allow"?: boolean, "message"?: string }`, where a missing `allow` means allow. To deny, return
`{ "allow": false, "message": "..." }`. The message is plain text, cut to 200 characters, and the composer shows it beside
the user's own text. A denial without a message reads "This Chitt didn't accept that".

A gate has a 1 s deadline and runs on the reserved gate slot. It fails open: a trap, a timeout, an `{ "error" }` return, a
busy pool or a disabled Chitt commits the action as allowed. KV writes made during a gate stand. Poster Actions a gate requests
are deferred until it decides: they run after the commit, and `"$new"` in their `postId` names the new reply (`on_reply`
only). They run whether the gate allowed or denied.

A removal (`added: false`) is a notification. It runs in the background after the commit, and its return is ignored, so it
can't refuse anything.

Envelopes (`v: 1`, the Observer is the acting Character): `on_reply` carries the proposed reply `{ characterId, contentRaw,
contentQuirked, isOoc, replyToPostId }` for direct replies only; `on_reaction` carries `{ type, added }`; `on_rechitt` carries
`{ added }`.

A Poster Action on another Chitt runs that Chitt's gate inline, under the gate deadline capped by the caller's remaining time.
A refusal comes back to the caller as `{ "error": { "code": "denied", "message" } }`. A Live Chitt's own actions skip its own
gates. Causation depth is carried, and an action past depth 3 returns `depth_exceeded`.

### asset (Server Guest, `extism:host/user`)

`asset` reads a file the Cartridge carries under `assets/`. It is read-only and every Server Guest has it. It takes
`{ "path": string }`, such as `"img/logo.png"`, and returns `{ "bytes": string }`, the file base64 encoded, or
`{ "error": { "code", "message" } }`. Codes: `not_found` (no such file, or a bad path), `too_large` (over 4 MiB) and `invalid`
(a bad request). A call never traps.

### Poster Actions (Server Guest, `extism:host/user`)

A Server Guest acts as its Chitt's posting Character. Each function takes one JSON object and returns
`{ "ok": true, ...result }`, `{ "ok": true, "deferred": true }` (inside `init`, and gates later), or
`{ "error": { "code", "message" } }`. Codes: `forbidden`, `not_found`, `invalid`, `denied`, `rate_limited`, `depth_exceeded`,
`unavailable`. A call never traps.

| Function | Input | Output |
| --- | --- | --- |
| `post_reply` | `{ postId, content, ooc? }` | `{ postId }` |
| `post_chitt` | `{ content, ooc? }` | `{ postId }` |
| `edit_chitt` | `{ postId?, content, quirks?, ooc? }` | `{}`; `postId` defaults to the Chitt itself, where quirks apply only when `quirks` is true |
| `delete_chitt` | `{ postId }` | `{}`; the poster's own Chitts only, and the poster needs `post_deletion` |
| `react` / `unreact` | `{ postId, type }` | `{ action, likeCount, hateCount, pityCount, mediateCount }` |
| `rechitt` / `unrechitt` | `{ postId }` | `{ rechittId }` / `{}` |
| `follow` / `unfollow` | `{ characterId }` | `{ action }` |
| `pin` / `unpin` | `{ postId, type?: "post" \| "rechitt" }` / `{}` | `{}` |

Authority is checked on every call: the poster's Character exists and is theirs, it is not suspended, the user is not banned,
and (for an admin action) the user still holds the action's permission. Quotas: 5 actions per Hook call, 1 per `render`, and
30 a minute per Chitt, shared by social and admin actions. Past them, `rate_limited`. Every action, successful or not, is
written to the Poster Action log. A Chitt that is disabled answers `unavailable`. The catalog has no tier management, DMs,
account actions (password, email, Patreon), IP bans or private-data reads.

Admin Poster Actions. Each takes the input shown and returns `{}` on success. Each needs the permission in the last column on
the poster's account, checked on every call and answering `forbidden` without it.

| Function | Input | Permission |
| --- | --- | --- |
| `admin_delete_post` | `{ postId }` | `post_deletion` |
| `admin_edit_post` | `{ postId, content }` | `post_editing` |
| `admin_pin_chitt` / `admin_unpin_chitt` | `{ postId }` | `pin_chitt` |
| `ban_user` | `{ userId, duration, reason? }` | `user_banning` |
| `unban_user` | `{ userId }` | `user_banning` |
| `suspend_character` | `{ characterId, duration, reason? }` | `character_suspension` |
| `unsuspend_character` | `{ characterId }` | `character_suspension` |
| `admin_delete_character` | `{ characterId }` | `character_deletion` |
| `admin_update_trolltag` | `{ characterId, trolltag }` | `character_suspension` |
| `admin_update_character_name` | `{ characterId, name }` | `character_suspension` |
| `admin_update_character_bio` | `{ characterId, bio }` | `character_suspension` |
| `admin_update_character_discriminator` | `{ characterId, discriminator }` | `character_suspension` |
| `create_badge` | `{ name, description, imageDataUri }` | `badge_management` |
| `delete_badge` | `{ badgeId }` | `badge_management` |
| `grant_badge_to_user` / `revoke_badge_from_user` | `{ userId, badgeId }` | `badge_management` |
| `grant_badge_to_character` | `{ characterId, badgeId }` | `grant_badge` |
| `revoke_badge_from_character` | `{ characterId, badgeId }` | `revoke_badge` |

`duration` is one of `1m`, `5m`, `15m`, `30m`, `1h`, `24h`, `7d`, `30d`, `permanent`.

### Render Pushes (Server Guest, `extism:host/user`)

`push_render` tells the Chitt's Observers that its card has changed, so their clients fetch a fresh render. It takes
`{ "characterId"?: string }`: with no `characterId` it pushes to every Observer, and with one it pushes to that Character
alone. It returns `{}` or `{ "error": { "code", "message" } }`. The push is published only after the Hook call returns ok; a
trapped, timed-out or canceled call publishes none. Codes: `not_allowed` (any call to it from `render`), `quota_exceeded` (a
call may queue at most 20 distinct targets, and the refusal goes to the Failures log), `invalid_argument`, `rate_limited`,
`timeout`. A push carries no render. Broadcast pushes to the same Chitt coalesce, with a spread that grows with its audience.

### on_canvas_message (Server Guest)

A Canvas Guest's `send` reaches its Chitt's Server Guest as the `on_canvas_message` Hook. It takes the standard envelope,
plus the message:

```json
{
  "v": 1,
  "hook": "on_canvas_message",
  "chitt": { "id": "…", "posterId": "…", "createdAt": "…" },
  "observer": null,
  "message": { "bar": 1 },
  "seq": 3
}
```

`observer` is null for a logged-out viewer. `seq` is present only when the canvas sent one. The return is a plain object (see
Hook returns in section 2): `{}`, or `{ "reply"?: any JSON, "rerender"?: boolean }`, or `{ "error": { code, message } }`.

```json
{ "reply": { "label": "Pineapple", "votes": 42, "percent": 30 } }
{ "rerender": true }
```

`reply` (at most 4 KiB of JSON) goes back only to the canvas that sent the message, as a `reply` event with its `seq`.
`rerender: true` draws the sender's card again through `render`, which delivers fresh `canvas` state. A message gets one
answer, a `reply` or an `error` event, even when the reply is absent (then `reply` is null). The Hook runs under the background
deadline (2 s), in the background class of the pool. A full queue answers `busy`.

Each canvas message carries a canvas token, which the render returns as `canvas.token`. It binds the Chitt, the Cartridge
Version, the Observer (or the anonymous session of a logged-out viewer, which the `lc_anon` cookie carries) and the time of
issue, and expires after 24 hours. A stale token (another version, another Observer, or expired) runs no guest: the answer is
`stale`, and the card is drawn again. Rate limits, checked before any guest runs: a signed-in Observer gets 20 messages a second
with bursts of 40, per Chitt; a logged-out viewer gets the same per anonymous session per Chitt, plus 60 a second per IP across
all Chitts.

## 3. Timing

- The effective rate is the lower of the guest's `set_fps` request, 60, the display's frame rate, and 15 under reduced motion.
- Safari runs worker frames at about 30 fps, so `set_fps(60)` may get 30 there.
- A canvas that has not reported ready within 5 s of starting crashes. So does any call that runs past 250 ms.

## 4. The command buffer

A frame's draw ops are one byte stream, passed to `submit` in a single call. Each op is a `u8` opcode followed by its fixed
arguments, in the order of the table in section 5.

- All numbers are little-endian. `f32` is an IEEE 754 single.
- A colour is a `u32` RGBA, `0xRRGGBBAA`.
- A string is a `u32` byte length, then that many bytes of UTF-8.
- An array is a `u32` count, then its elements.
- A boolean is a `u8`: `0` or `1`.
- An enum is a `u8` index into the list named in the table. An index past the list is malformed.
- A buffer is at most 4 MiB. A stream that ends mid-op, has an unknown opcode, an invalid UTF-8 string or a blit whose length
  is not `w × h × 4` is malformed.
- A malformed buffer crashes the canvas. The whole buffer is decoded before any of its ops runs, so a malformed buffer draws
  nothing from that submit.
- An op whose number is not finite (NaN or infinity) is skipped, as Canvas2D skips it. A gradient id that was never defined,
  or a `draw_image` handle that was never loaded, is skipped.
- Gradient ids last for one `submit`. Define them in the same buffer that uses them.
- Coordinates are in backing store pixels. The transform, like the other state (fill colour, font, and so on), carries over
  between submits, as it does on a Canvas2D context. Nothing is reset between frames.

## 5. Draw-op table

Opcodes are hexadecimal. The encoder in the Rust SDK (`sdk/chittr-cartridge/src/canvas/encode.rs`) follows this table. Its
golden bytes live in `sdk/chittr-cartridge/tests/fixtures/canvas/command-buffer-v1.bin`, and any decoder can be checked against
them.

| Opcode | Name | Arguments |
| --- | --- | --- |
| 0x01 | save | |
| 0x02 | restore | |
| 0x03 | setTransform | f32 a, b, c, d, e, f |
| 0x04 | transform | f32 a, b, c, d, e, f |
| 0x05 | translate | f32 x, y |
| 0x06 | rotate | f32 radians |
| 0x07 | scale | f32 x, y |
| 0x08 | setAlpha | f32 alpha |
| 0x09 | setComposite | u8 mode (index into COMPOSITE_OPS) |
| 0x0a | setImageSmoothing | u8 bool |
| 0x10 | beginPath | |
| 0x11 | moveTo | f32 x, y |
| 0x12 | lineTo | f32 x, y |
| 0x13 | quadTo | f32 cpx, cpy, x, y |
| 0x14 | bezierTo | f32 c1x, c1y, c2x, c2y, x, y |
| 0x15 | arc | f32 x, y, r, start, end; u8 ccw |
| 0x16 | arcTo | f32 x1, y1, x2, y2, r |
| 0x17 | ellipse | f32 x, y, rx, ry, rotation, start, end; u8 ccw |
| 0x18 | rect | f32 x, y, w, h |
| 0x19 | roundRect | f32 x, y, w, h, r |
| 0x1a | closePath | |
| 0x1b | fill | u8 rule (0 nonzero, 1 evenodd) |
| 0x1c | stroke | |
| 0x1d | clip | u8 rule (0 nonzero, 1 evenodd) |
| 0x20 | fillRect | f32 x, y, w, h |
| 0x21 | strokeRect | f32 x, y, w, h |
| 0x22 | clearRect | f32 x, y, w, h |
| 0x30 | fillColor | u32 rgba |
| 0x31 | strokeColor | u32 rgba |
| 0x32 | lineWidth | f32 width |
| 0x33 | lineCap | u8 cap (0 butt, 1 round, 2 square) |
| 0x34 | lineJoin | u8 join (0 miter, 1 round, 2 bevel) |
| 0x35 | miterLimit | f32 limit |
| 0x36 | lineDash | u32 n; f32 segment × n |
| 0x37 | fillGradient | u32 id |
| 0x38 | strokeGradient | u32 id |
| 0x39 | defLinear | u32 id; f32 x0, y0, x1, y1; u32 n; (f32 offset, u32 rgba) × n |
| 0x3a | defRadial | u32 id; f32 x0, y0, r0, x1, y1, r1; u32 n; (f32 offset, u32 rgba) × n |
| 0x40 | font | string family; f32 size; f32 weight |
| 0x41 | textAlign | u8 align (0 start, 1 end, 2 left, 3 right, 4 center) |
| 0x42 | textBaseline | u8 baseline (0 top, 1 hanging, 2 middle, 3 alphabetic, 4 ideographic, 5 bottom) |
| 0x43 | fillText | f32 x, y; string text |
| 0x44 | strokeText | f32 x, y; string text |
| 0x50 | drawImage | u32 handle; f32 sx, sy, sw, sh, dx, dy, dw, dh |
| 0x51 | blitRgba | f32 x, y; u32 w, h; u32 n; u8 × n (w × h × 4) |

Enums:

- `setComposite` modes, by index: `source-over`, `source-atop`, `destination-over`, `destination-out`, `lighter`, `multiply`,
  `screen`, `overlay`.
- `font` family: a manifest font's name, or anything else for `sans-serif`. Size is clamped to 1 to 1024 px, weight to 100 to 900.

Limits on variable arguments: at most 64 gradient stops, 64 dash segments, 64 KiB per string, and a blit side of 1 to 2048
pixels.

`blitRgba` draws in v1. `drawImage` draws the image a `load_image` handle names (section 1.3), and is skipped while that image
is not loaded yet, or for a handle that was never loaded.

## 6. Limits

| Limit | Value | Enforced by |
| --- | --- | --- |
| `canvas.wasm` size | 4 MiB | upload |
| Declared memory maximum | 64 MiB (1024 pages) | upload; the host caps it at instantiation |
| Ready after start | 5 s | host watchdog; crash `startup` |
| Each `start`, `frame`, `event` call, or snapshot | 250 ms | host watchdog; crash `timeout` |
| One command buffer | 4 MiB | `submit` |
| Render `canvas` state | 16 KiB of JSON | render (a larger state fails the render) |
| Backing store | 2048 px per side | host |
| Off-screen before termination | 30 s | host |
| Live canvases per page | 4 | host |
| Crashes per canvas per page session | 3 | host; the canvas then stays stopped |
| `send` message | 4 KiB of JSON | `send`, and the route |
| Messages in flight per canvas | 4 | `send` (answers `busy`) |
| Answer wait for a message | 5 s | page; then the guest gets `error { code: "timeout" }` |
| Crash reports | half of the crashes, 5 s apart, 20 per page session; 30 a minute per IP on the server | page, route |
| Images and sounds per canvas | 32 of each | worker (`load_image`, `load_sound`) |
| Decoded image side | 4096 px | page (`too_large` error) |
| Sounds playing at once | 8 | worker (`play_sound` answers `busy`) |

## 7. Version history

- **v1**: the exports, host functions, command buffer and draw-op table above. An addition within v1 (a new opcode, event,
  host function or Hook) keeps `v: 1`. A change to an existing byte or shape needs a new `v`.
- **v1 addition**: an optional `style.css` at the archive root and the `class` attribute in templates (see
  [Stylesheets and badges](stylesheets.md)). `class` used to be stripped, so a class already in an older template now
  picks up the site's CSS inside its card. Nothing else about an archive without `style.css` changes.
