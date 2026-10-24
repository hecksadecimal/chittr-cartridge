# Cartridge ABI v2

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

this is the v2 rulebook. same deal as v1, but with file pickers and native framebuffers bolted on. if the prose and the
schema ever disagree, the schema wins and this page is the one with the bug <3
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

**Status: draft v2 contract for review. v2 is not yet released, and these bytes and shapes are not frozen until it is.**

this is the normative ABI for `v: 2` Cartridges: what the host calls, what a guest may call, and the byte layout of a
frame's draw ops. it's the successor to the [v1 ABI](../v1/abi.md) and incorporates v1's exports, host functions, draw
ops `0x01`–`0x51`, envelopes, limits and deadlines unchanged. v2 adds file pickers and bounded reads, native-resolution
framebuffer presentation, capability negotiation, and the owner/admin configuration surface. the reasoning is in
[`design.md`](design.md); the motivation is in [`proposal.md`](proposal.md).

A change that alters an existing byte or shape is a new `v`. An addition (a new opcode, a new host function, a new
capability) keeps `v: 2` and is listed under "Version history".

Two guests make up a Cartridge. The **Server Guest** (`server.wasm`) runs on the server. Its Hooks and their envelopes
are described in `server.xtp.yaml`, and the sections below cover its host functions. The **Canvas Guest** (`canvas.wasm`)
runs in a Web Worker in the browser, and is described here, in the sections on exports, host functions, the command
buffer and the draw-op table.

**The schemas are the source of truth.** `docs/cartridges/v2/server.xtp.yaml` and `docs/cartridges/v2/canvas.xtp.yaml`
(XTP schema `v1-draft`) name every export and host function, with their shapes and descriptions. The prose below explains
them. Where the two disagree, the schema is the contract and this file is the bug. The Chittr host is checked against the
schemas.

**The rule for changes.** Any change that adds a Hook or a host function updates the schema in the same change. A Hook or
host function that exists in the host but not in the schema is not part of v2.

**v1 is unchanged.** A `v: 1` Cartridge runs on a v2 host without recompilation, under v1's rules. v2 opcodes, Host
Parts, capabilities and Hooks are refused in a v1 archive, and v1's bytes are never reinterpreted as v2. The version is
selected by the manifest's `v` integer alone (section 8).

The Server Guest's `Observer` includes `isSuspended`, a boolean indicating whether the viewing Character currently has an
active suspension. It does not expose the suspension reason or end time.

## 1. Canvas Guest exports

`canvas.wasm` must export `memory` and the functions `start` and `frame`. It may export `event`. Any other export is refused
at upload, and so is a declared memory maximum over 64 MiB (1024 pages). A module that declares no maximum is capped at
64 MiB by the host.

Every export takes one JSON object and returns one JSON object. The return is `{}` on success, or
`{ "error": { "code": string, "message": string } }` when the guest reports a failure itself. An `error` return counts as
a crash. So do a trap, a malformed command buffer, a non-object reply and a call that runs past its limit (section 9).

### `start` (required)

Input:

```json
{
  "width": 640,
  "height": 360,
  "dpr": 2,
  "prefersReducedMotion": false,
  "state": { "bars": [{ "label": "Pepperoni", "votes": 42 }] },
  "fullscreen": false,
  "capabilities": ["files.client", "framebuffer.indexed8"],
  "limits": { "maxFileBytes": 33554432, "maxFileReadBytes": 65536, "maxFileHandles": 4, "maxFileBytesPerCanvas": 41943040, "maxPickers": 8, "maxFramebufferPixels": 1048576, "maxPaletteEntries": 256 }
}
```

`width` and `height` are the backing store's size in device pixels (`css width × dpr`, capped at 2048 per side, aspect ratio
kept). Draw in those pixels. `state` is the render's `canvas` value, or `null`. `capabilities` is the effective set the host
grants this guest (section 8); a capability the host does not support is absent even when the manifest declared it optional.
`limits` reports the effective v2 resource limits, so a guest reads them rather than hard-coding them. Both are stable for
the life of the Cartridge Version.

### `frame` (required)

Input: `{ "t": 16234.5, "dt": 16.7 }`. `t` is the frame time in milliseconds since the page's time origin, `dt` the time since
the previous frame. A frame is delivered only after the guest called `request_frame`, and never faster than the effective
rate (section 3).

### `event` (optional)

Input: `{ "type": string, ...fields }`. The types in v2:

| `type` | Other fields | When |
| --- | --- | --- |
| `state` | `state` (the render's `canvas` value, or `null`) | the render changed, delivered while visible |
| `resize` | `width`, `height`, `fullscreen` | the backing store changed size |
| `visibility` | `visible` (boolean) | the canvas went off-screen or its tab hid (`false`), or came back (`true`) |
| `pointer` | `action`, `x`, `y`, `width`, `height`, `pointerId`, `pointerType`, `buttons`; or `relative`, `movementX`, `movementY`, `buttons` for a pointer-locked mouse | a pointer went down, moved, went up, or was cancelled (section 1.1) |
| `key` | `action`, `key`, `code` | a key went down or up, while the canvas has focus (section 1.1) |
| `focus` / `blur` | none | the canvas gained or lost focus (section 1.1) |
| `input_reset` | none | pointer lock ended or the host released input: clear held keys and buttons (section 1.1) |
| `reply` | `reply` (any JSON), `seq` (when the message had one) | the answer to a `send`, on success (section 1.2) |
| `error` | `code`, `message` (optional), `seq` (when the message had one), or `handle` (for a load) | a `send` failed, its answer was not usable (section 1.2), or an image or sound did not load (section 1.3) |
| `image_loaded` | `handle`, `width`, `height` (device pixels) | an image from `load_image` is decoded and can be drawn (section 1.3) |
| `sound_loaded` | `handle` | a sound from `load_sound` is decoded and can be played (section 1.3) |
| `file` | `name`, `file` (metadata) | the user chose a file in an `<lc-file target="client">` picker (section 6) |

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
- **Relative pointer.** With the `input.relative-pointer` capability, a real host-owned click on the focused canvas asks
  the browser for pointer lock. The guest cannot request lock itself. While locked, `pointer` events have `relative: true`,
  `movementX`, `movementY` and `buttons`, and no canvas coordinates; they are coalesced to one per frame and each axis is
  clamped to the effective `maxRelativePointerDelta`. Lock ends on Escape, blur, a hidden tab, termination or user
  cancellation; the host then sends `input_reset`, so held keys and buttons cannot stick. The host never adds global
  keyboard hooks, clipboard access or coordinates outside the canvas.
- `input_reset` is also sent when the host otherwise releases input. A guest that ignores it still gets `blur`; a game
  should clear its held state on either.
- Wheel and clipboard are not delivered.

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
`busy` (eight sounds already play). The file functions add `not_found` (unknown, closed, expired or foreign handle), `too_large`
(a read over the chunk cap) and `not_supported` (the capability is not in the effective set).

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
| `file_read` | `{ "handle": string, "offset": integer, "length": integer }` | `{ "bytes": base64, "eof": boolean }`, or `{ "error" }` | Reads at most 64 KiB of a file a picker delivered (section 6). Clips at EOF. |
| `file_close` | `{ "handle": string }` | `{}`, or `{ "error" }` | Releases a file handle early. Closing an unknown or already-closed handle is harmless. |
| `audio_write` | `{ "format": "s16le", "sampleRate": integer, "channels": integer, "samples": base64 }` | `{ "accepted": boolean, "queuedMs": integer }`, or `{ "error" }` | Queues one bounded PCM chunk for the host's shared speaker. Only interleaved s16le at 22050, 44100 or 48000 Hz, mono or stereo. At most 64 KiB before encoding. `accepted` is false when the queue is full; `queuedMs` is the depth after the call. Muted or paused canvases drop the chunk. |
| `audio_stop` | `{}` | `{}`, or `{ "error" }` | Stops queued and playing PCM for this canvas. Muting, pausing, hiding or terminating the canvas does the same. |

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

Envelopes (`v: 2`, the Observer is the acting Character): `on_reply` carries the proposed reply `{ characterId, contentRaw,
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

### file_read and file_close (both guests)

A file a picker delivered is read in bounded pieces. The metadata the guest sees is
`{ id, filename, mimeType, size }` (section 6); the bytes never travel in an envelope.

```json
{ "handle": "f_7Qm…", "offset": 0, "length": 65536 }
→ { "bytes": "TWFuIGlzIGRpc3…", "eof": false }
```

- `offset` and `length` are non-negative integers. `length` is 1 to 65536; over the cap is `too_large`, not a clamp.
- Reads clip at EOF. `offset` equal to `size` returns empty bytes with `eof: true`; `offset` greater than `size` is `invalid`.
- `bytes` is a **bounded chunk**, at most 64 KiB before base64 encoding (about 87 KiB of JSON), never the whole file. The
  whole-file base64 that v2 exists to avoid (a 14.4 MiB WAD would be 19.3 MiB of JSON) is not a supported use of this call.
- Unknown, closed, expired and foreign handles answer `not_found` and expose no bytes.
- `file_close` releases a handle early. Closing an unknown or already-closed handle is harmless and returns `{}`.

A server handle lives only for its `on_file` call and is revoked when the Hook settles, including timeout, cancellation and
failure. A canvas handle lives until the guest closes it, the picker is re-selected, the worker is replaced, or the canvas
terminates. Replacing the same picker's file revokes its previous handle; a later read of a revoked handle answers `not_found`.
Reads are synchronous within a call, so there is no torn read. A guest that needs the bytes longer must copy them under its own
memory and KV limits.

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
  "v": 2,
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

### on_file (Server Guest)

A `target="server"` picker delivers its file as the `on_file` Hook (section 6). The browser uploads the bytes to a
dedicated endpoint; the endpoint validates the authorization and the bounded upload, then calls the Hook with a call-scoped
handle. The bytes are not in the envelope.

```json
{
  "v": 2,
  "hook": "on_file",
  "chitt": { "id": "…", "posterId": "…", "createdAt": "…" },
  "observer": { "characterId": "…", "…": "…" },
  "surface": "card",
  "name": "import",
  "file": { "id": "f_7Qm…", "filename": "settings.json", "mimeType": "application/json", "size": 2048 }
}
```

`surface` is `card` or `config`, set by the host from the surface the picker was rendered on. `observer` is the signed-in
Character who selected the file and is never null; a server picker requires a signed-in viewer. The return is a plain object
(see Hook returns in section 2): `{}`, or `{ "rerender": true }` to draw the sender's card again through `render`, or
`{ "error": { code, message } }`. There is no `reply`: the file did not come from the canvas worker, so there is no canvas to
answer. The Hook runs under the background deadline (2 s), and its handle is revoked when it settles, including timeout,
cancellation and failure.

The upload never writes the file to KV, Cartridge assets, permanent storage or a public URL. The server never trusts a
client-supplied filename, MIME type or reported size. The endpoint enforces byte limits before multipart parsing, per-viewer
and per-IP rate limits, bounded concurrency, request timeouts and cancellation. The guest execution deadline begins when the
Hook runs, not while the user uploads.

### render_config (Server Guest)

`templates/config.liquid`, when present, renders in the host's "Cartridge…" panel for an eligible viewer (section 7). Its data
comes from the optional `render_config` Hook, which runs under the render deadline (500 ms) and only after the host has
authorized the viewer. It is never invoked for an ordinary card view, and the host never calls public `render` to obtain
private data.

```json
{
  "v": 2,
  "hook": "render_config",
  "chitt": { "id": "…", "posterId": "…", "createdAt": "…" },
  "observer": { "characterId": "…", "…": "…" },
  "surface": "config",
  "access": { "audience": "owner_or_admin", "isOwner": true, "isAdmin": false, "canConfigure": true }
}
```

`surface` is always `config`, and `access` is host-computed and host-authenticated; a guest cannot choose its surface or widen
its access. The return is `{ "data": { … } }` or `{ "error": { code, message } }`. There is no `template` field: the host
always renders `config.liquid`. If the file exists without this Hook, the host renders it with `data: {}` and the host context.

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
- A buffer is at most 4 MiB. A stream that ends mid-op, has an unknown opcode, an invalid UTF-8 string, a `blitRgba` whose
  length is not `w × h × 4`, a `blitRgbaScaled` whose length is not `w × h × 4`, or a `blitIndexedScaled` whose palette or
  index length does not match its dimensions is malformed.
- A malformed buffer crashes the canvas. The whole buffer is decoded before any of its ops runs, so a malformed buffer draws
  nothing from that submit.
- An op whose number is not finite (NaN or infinity) is skipped, as Canvas2D skips it. A gradient id that was never defined,
  or a `draw_image` handle that was never loaded, is skipped.
- Gradient ids last for one `submit`. Define them in the same buffer that uses them.
- Coordinates are in backing store pixels. The transform, like the other state (fill colour, font, and so on), carries over
  between submits, as it does on a Canvas2D context. Nothing is reset between frames.

## 5. Draw-op table

Opcodes are hexadecimal. The encoder in the Rust SDK (`sdk/chittr-cartridge/src/canvas/encode.rs`) follows this table. Its
golden bytes live in `sdk/chittr-cartridge/tests/fixtures/canvas/command-buffer-v1.bin` for `0x01`–`0x51` and
`sdk/chittr-cartridge/tests/fixtures/canvas/command-buffer-v2.bin` for `0x52`–`0x53`, and any decoder can be checked
against them.

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
| 0x52 | blitRgbaScaled | f32 dx, dy, dw, dh; u32 w, h; u8 smoothing; u32 n; u8 × n (w × h × 4) |
| 0x53 | blitIndexedScaled | f32 dx, dy, dw, dh; u32 w, h; u8 smoothing; u32 n; u32 × n palette; u8 × (w × h) indices |

Enums:

- `setComposite` modes, by index: `source-over`, `source-atop`, `destination-over`, `destination-out`, `lighter`, `multiply`,
  `screen`, `overlay`.
- `font` family: a manifest font's name, or anything else for `sans-serif`. Size is clamped to 1 to 1024 px, weight to 100 to 900.

Limits on variable arguments: at most 64 gradient stops, 64 dash segments, 64 KiB per string, and a blit side of 1 to 2048
pixels.

`blitRgba` draws in v1. `drawImage` draws the image a `load_image` handle names (section 1.3), and is skipped while that image
is not loaded yet, or for a handle that was never loaded.

`blitRgbaScaled` (0x52) and `blitIndexedScaled` (0x53) are v2. They present a native-resolution frame and let the host scale
it to `dx, dy, dw, dh`, a destination rectangle in backing-store pixels. Unlike `blitRgba`, they honour a destination
rectangle, so a guest sends its own pixels instead of expanding them to the destination size first.

- `smoothing` is `0` for nearest-neighbour and `1` for linear. It overrides `setImageSmoothing` (0x0a) for this op only. A
  pixel-art guest passes `0`.
- The palette is `n` entries of `u32` RGBA (`0xRRGGBBAA`, the same packing as a colour). `n` is 1 to 256; the host pads the
  remainder to 256 with fully transparent entries. Index `i` selects palette entry `i`; an index at or past `n` draws
  transparent. Alpha is straight, not premultiplied.
- For `blitRgbaScaled`, `n` must equal exactly `w × h × 4`. For `blitIndexedScaled`, the palette count and the index count are
  separate and each is checked exactly. The host expands indexed8 to RGBA8 in a bounded, host-owned staging surface it reuses
  across frames; the guest never sees that surface and cannot reach it.
- `w × h` is at most 1,048,576 (section 9). The whole op is still bounded by the 4 MiB command buffer.
- A blit whose byte length does not match its dimensions is malformed and crashes the canvas before anything draws, as in v1.
  A destination rectangle is checked against the existing backing-store rules.

## 6. File pickers and delivery

A card or config template places a file picker with the `<lc-file>` Host Part. The host owns the browser input and the
routing; the guest never sees a file dialog, a path or the file's bytes in JSON.

```html
<lc-file
  name="wad"
  target="client"
  accept=".wad"
  max-bytes="33554432"
  label="Choose your local WAD">
</lc-file>
```

- `name` is required and matches the Control-name rule (`^[a-z][a-z0-9_-]{0,31}$`). It is unique within the rendered
template; a duplicate is refused at upload.
- `target` is required and exactly `client` or `server`. There is no default and no fallback from client to server.
- `accept` is optional: a comma-separated list of extensions (`.wad`) and MIME types (`application/json`), each at most 64
  characters, at most 16 entries. It filters the picker and is never a content check.
- `max-bytes` is optional: a positive integer that may lower, never raise, the host cap (section 9). An absent value means
  the host cap. A value above the host cap is clamped at render, so a template stays portable.
- `label` is optional plain text, at most 200 characters, used by the host-owned input's accessible label.
- `asset` is optional: a file path under `assets/` (with or without the `assets/` prefix), requiring `target="client"`.
  The host renders a button instead of a file input and fetches that Cartridge asset, delivering it through the same
  client routing. It is how a card can offer a WAD the Cartridge itself ships, such as Freedoom.

The element is a leaf: template children, arbitrary attributes, script and event handlers are dropped by the sanitizer, and
both the upload validator and the client renderer check the attributes. Arbitrary `<input type="file">` markup stays
disallowed. A picker is scoped to one rendered card instance and one Cartridge Version: a late selection never reaches a
replacement guest, a new version, or another viewer's card. One selection delivers one file; directory and multi-select are
out of scope. Cancelling has no side effect, and re-selecting the same file works. An oversized or disallowed selection
produces an accessible error without crashing the card or starting guest work.

**Metadata.** Every route exposes the same shape, and the bytes never travel with it:

```json
{ "id": "f_7Qm…", "filename": "doom1.wad", "mimeType": "application/octet-stream", "size": 14445628 }
```

`id` is opaque, unguessable and scoped to one server call or one canvas instance. `filename` is display metadata only,
never a path and never trusted for type. `mimeType` is the browser's report, a hint only. `size` is the host's measured
byte length, not the client's claim.

**Client routing.** The page checks the picker configuration and file size before reading, transfers the bytes to the canvas
worker as an `ArrayBuffer`, and the worker registers a private handle and sends the guest a `file` event (section 1). The
bytes never go to the server, and are not included in server requests, analytics, crash reports or logs. If the worker is
unavailable, the host shows an actionable error and does not silently drop the selection; it does not queue across a worker
restart. Anonymous viewers may use client-local pickers, under the normal visibility and canvas lifecycle rules.

**Server routing.** A signed-in viewer selects a file; the browser sends an authenticated multipart upload to a dedicated
endpoint, never a canvas `send`. The endpoint validates the authorization, current authentication, visibility, blocks/OOC
restrictions, active state and the relevant permission, then supplies a call-scoped handle to the `on_file` Hook (section 2).
A server picker needs a signed authorization derived from the sanitized render; it binds the Chitt, the active Cartridge
sha, the viewer Character, the render surface (`card` or `config`), the picker name, the destination, the accepted types,
the size limit and an expiry. A client cannot raise its size cap or invent a picker by editing the DOM or the request.
Stale-version and expired authorizations run no guest.

The server must not trust a client-supplied filename, MIME type or reported size. The host never automatically writes the
file to KV, Cartridge assets, permanent storage or a public URL. Anonymous server uploads are out of scope. The reads are
`file_read` and `file_close` (section 2), bounded per call.

## 7. The configuration surface

A v2 Cartridge may include `templates/config.liquid`. When present, the host offers a cartridge-defined section inside its
existing "Cartridge…" panel for an eligible viewer. It is never appended to `card.liquid`, never rendered on an ordinary
viewer's page to be hidden with CSS, and never rendered eagerly on a feed request. No `config.liquid` means no
cartridge-defined configuration section and no change to the host's existing management tools.

The section's data comes from the optional `render_config` Hook (section 2), which runs under the render deadline and only
after the host has authorized the viewer. If the file exists without the Hook, the host renders it with `data: {}` and the
host context. The host never calls public `render` to obtain private data.

**Controls.** A Control declaration in the manifest gains two optional keys:

```json
"reset_scores": {
  "value": { "enum": ["confirm"] },
  "audience": "owner_or_admin",
  "confirm": "Reset every score on this Chitt?"
}
```

- `audience` is one of `public` (the default), `owner`, `admin` or `owner_or_admin`. It is validated at upload and rechecked
  on every action.
- `confirm`, when present, is a plain-text prompt at most 200 characters. The host renders the confirmation dialog and owns
  its buttons; a guest label saying "confirm" is not itself a confirmation.
- **Placement must match audience.** A Control bound in `card.liquid` must be `public`; a Control bound in `config.liquid`
  must be non-public. A mismatch is refused at upload. This is what stops a public Control from becoming privileged merely by
  being copied into the config template, or a config-only Control from being callable in public.

The `on_control` envelope gains `surface`, set by the host to `card` or `config` from the host-authenticated render surface,
never from the request body. A public Control token cannot invoke an identically named private action, and a config token
cannot be replayed as a public interaction or on another Chitt: the signed token binds surface, Chitt, version, viewer
Character, control name and payload.

**Eligibility.** The default audience `owner_or_admin` means the posting Character's owning account, or a user holding the
explicit administrative configuration permission. "Owner" is the account that owns the posting Character, not "uploaded the
latest version", and any Character on that owning account qualifies. Cartridge code may narrow an audience further; it can
never widen it. Owner access to cartridge configuration does not grant moderation, replacement, failure-log or
administration permissions, and a moderation permission does not authorize every cartridge-defined config action. Config
Hooks get no extra Poster Action powers; those actions keep their normal checks.

**Lifecycle and isolation.** The config renders lazily, uses the active Cartridge Version's template and bindings, and is
invalidated by a version replacement or rollback just like the public card. Opening or closing it does not restart the
public Canvas Guest, reset gameplay or change stored state. A config render failure is contained to the section; the host's
close button and native management tools remain, and opening the panel does not bypass the kill switch, breaker or normal
guest-call limits. Config data stays on the authorized, viewer-scoped request path: it never enters public render responses,
public push payloads, feed caches, snapshots, public HTML or a shared cache. After an action, the host refreshes the config
region independently; a public change also requests an ordinary public-card render or invalidation, without publishing the
private config response. Settings are stored by guest Hooks under the existing state rules, not by editing the Liquid file
or implicitly serializing form contents.

**File pickers in config.** Both destinations keep their section 6 rules. `target="server"` needs a config-scoped
authorization and delivers a call-scoped handle to the authorized `on_file`. `target="client"` stays local and binds to the
associated card's existing canvas instance; opening the config must not create a hidden worker or route bytes into a
different instance. The initial config surface supports declared Controls and file pickers, not a second independent Canvas
Guest.

## 8. Versioning and capabilities

**Version.** The manifest's `v` integer selects the ABI: `1` or `2`. Any other value is refused at upload. The XTP
schema-language version is independent of the Cartridge ABI version: the v2 schemas keep XTP schema `v1-draft`, because the
schema language did not change, while the Cartridge ABI they describe is 2. Every v2 Hook
envelope and canvas export input carries `"v": 2`; a v1-shaped envelope delivered to a v2 guest, or a v2 Hook delivered to
a v1 guest, is a hard failure. The command-buffer decoder is selected by the manifest ABI, so a v2 opcode in a v1 canvas is
malformed and v1's bytes are never reinterpreted. v1 validation and execution stay available unchanged. The version is not
a label a guest may change at runtime.

**Capabilities.** A v2 manifest may declare capabilities, scoped to a guest, and split into required and optional:

```json
"capabilities": {
  "server": { "require": ["files.server"], "optional": [] },
  "canvas": { "require": ["files.client", "framebuffer.indexed8"], "optional": ["input.relative-pointer", "audio.pcm"] }
}
```

- A guest key is present only when that guest exists. Unknown ids, duplicates, and a capability for a missing guest are
  refused at upload.
- Required capabilities are validated against the host's supported set before the version is instantiated. If the host
  cannot honour one, the upload is refused, or the version is disabled if the host lost support later.
- Optional capabilities the host does not support are absent from the effective set. Calling a host function that belongs
  to an unsupported capability answers `not_supported`; it is not a trap and not a silent no-op.
- Declaring a capability grants nothing by itself. It does not bypass a template-picker authorization, a user gesture, a
  permission check or a quota.

The initial vocabulary is closed:

| Capability | Guest | Meaning |
| --- | --- | --- |
| `files.server` | server | Receive a server-routed file through `on_file` and read it |
| `files.client` | canvas | Receive a client-local `file` event and read it |
| `framebuffer.rgba8` | canvas | Present RGBA8 pixels with a destination rectangle (0x52) |
| `framebuffer.indexed8` | canvas | Present indexed8 pixels with a palette and a destination rectangle (0x53) |
| `clock.monotonic` | both | Reserved; not part of v2 yet |
| `input.relative-pointer` | canvas | Host-owned pointer lock with coalesced relative mouse deltas and `input_reset` |
| `audio.pcm` | canvas | Queue interleaved s16le PCM through the host speaker with bounded backpressure |

**Discovery.** The effective capability list and effective limits are delivered once per instance: `init` for a Server Guest
and `start` for a Canvas Guest. They are stable for the life of the Cartridge Version, so a guest reads them there rather
than hard-coding them. A capability's host functions are usable only when the capability is in that list.

## 9. Limits

| Limit | Value | Enforced by |
| --- | --- | --- |
| `canvas.wasm` size | 4 MiB | upload |
| One Cartridge asset under `assets/` | 32 MiB | upload |
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
| One selected file | 32 MiB; a template may lower it | picker, before reading |
| Live client file handles per canvas | 4 | a new selection is refused (`limit`) |
| Aggregate client file bytes per canvas | 40 MiB | a new selection is refused (`limit`) |
| One `file_read` | 64 KiB | `too_large` |
| One `audio_write` | 64 KiB of PCM before base64 | `too_large` |
| PCM queued per canvas | 2 s | `audio_write` answers `accepted: false` |
| Relative-pointer movement | 2,000 px per axis per coalesced frame | clamped by the host |
| File pickers per template | 8, uniquely named | refused at upload |
| `accept` entries | 16, each ≤ 64 characters | refused at upload |
| Framebuffer source pixels per blit (`w × h`) | 1,048,576 | malformed |
| Palette entries | 1 to 256 | malformed |
| Staging surfaces per canvas | 1, reused | host |
| Server upload body | 16 MiB, before multipart parsing | refused; no guest runs |
| Server upload admission | one per card in the UI, plus server-side per-viewer and per-IP limits | refused |
| Config render | render class, 500 ms, the render output cap | as `render` |
| Distinct badges per card (`<lc-badge>`, `<lc-badge-tooltip>`) | 32 | further distinct ids draw nothing |

v1's 32 MiB Server Guest and 64 MiB Canvas Guest memory caps, deadline classes and small ordinary-message/state caps stay as
the baseline. The new host-held resources have their own aggregate caps, so moving bytes out of wasm does not become an
unbounded host-memory escape hatch. The effective v2 numbers are reported to the guest in `init`/`start` (`limits`), so a
guest reads them rather than hard-coding them.

## 10. Version history

- **v1**: the original exports, host functions, command buffer and draw-op table. Unchanged, and still valid on a v2 host.
- **v2**: adds `on_file` and `render_config` Hooks; `file_read` and `file_close`; the `file` canvas event and `<lc-file>`
  Host Part; draw ops 0x52 `blitRgbaScaled` and 0x53 `blitIndexedScaled`; the capability manifest and its discovery on
  `init`/`start`; the `surface` field on `on_control`; the optional `templates/config.liquid` surface; and the
  `<lc-badge>` and `<lc-badge-tooltip>` Host Parts (section 11). The shipped
  capability additions are `input.relative-pointer` (the `relative` pointer event and `input_reset`) and `audio.pcm`
  (`audio_write` and `audio_stop`); both keep `v: 2`. It incorporates
  every v1 export, host function, draw op, envelope, limit and deadline unchanged. An addition within v2 (a new opcode,
  event, host function, Hook or capability) keeps `v: 2`. A change to an existing byte or shape needs a new `v`.

## 11. Badge Host Parts

Two Host Parts draw a Chittr badge by its id. Both are v2 only: a `v: 1` template that contains either is refused with
`templates/{name}.liquid: <lc-badge> and <lc-badge-tooltip> are only allowed when "v" is 2`. The stylesheet (`style.css`)
and the `class` attribute are not v2 only; they are covered, with these elements, in
[Stylesheets and badges](../v1/stylesheets.md).

```html
<button type="button" data-lc-control="cheer" data-lc-value="go"><lc-badge id="founder" width="16" height="16"></lc-badge> Cheer</button>
<p>Earned: <lc-badge-tooltip id="founder"></lc-badge-tooltip></p>
```

| Element | Draws | May sit inside a `<button>` |
| --- | --- | --- |
| `<lc-badge>` | a plain `<img>` of the badge | yes; a click goes to the enclosing element |
| `<lc-badge-tooltip>` | the badge as profiles draw it, with a tooltip showing its name and description | no; the tooltip trigger is interactive |

Both elements:

- require `id`, matching `^[A-Za-z0-9_-]{1,64}$`. An element without a valid id is dropped. The id may come from Liquid
  output, so it is not checked at upload.
- keep `alt`, `width`, `height`, `class`, `title`, `aria-label`, `dir`, `lang` and an inline `style`, cleaned as an
  `<img>`'s are. Other attributes are dropped.
- are leaves: their children are dropped.
- default `alt` and `title` to the badge's name with BBCode stripped, and the size to 24×24.

The host resolves ids on the server after rendering and sends each badge's name, description and image once per card,
beside the tree. Any existing badge may be drawn, held or not. An unknown id draws nothing, and a failed lookup draws no
badges without failing the card. A card holds at most 32 distinct badge ids; further distinct ids are dropped, and
repeats of a kept id are allowed. `<lc-badge-tooltip>` opens on hover and focus.
