# Cartridge ABI v2: design decisions

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so proposal was the brainstorm and this is the "what we picked" page. every call here is locked, recommended or
deferred, so u know what u can build against and what can still move. no surprises pls
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

**Status: decisions for review — not yet normative.**

this page resolves the open decisions in [`proposal.md`](proposal.md) so the v2 schemas and byte-level contract can be
written against something stable. where this page and `proposal.md` disagree, this page is newer and wins; where this
page and the eventual schemas disagree, the schemas win. nothing here changes v1.

the proposal is the motivation. this page is the "what we chose and why". the normative artifacts that follow are
`abi.md`, `server.xtp.yaml` and `canvas.xtp.yaml` in this directory, plus the CLI validator, the Rust SDK and the
conformance corpus.

## How to read the decisions

every decision below is one of three kinds:

- **Locked**: the schemas and host will be written against it.
- **Recommended**: the default to implement, flagged because a reasonable reviewer could choose otherwise.
- **Deferred**: explicitly out of the initial v2 core, reserved so it can be added later without a version break.

the v2 core is: file pickers and bounded reads, efficient framebuffer presentation, capability negotiation, explicit
versioning, and the owner/admin configuration surface. everything else is deferred.

---

## D1. Versioning and dispatch — Locked

**The manifest `v` integer selects the ABI.** `v: 1` is v1, unchanged. `v: 2` is this ABI. Any other value is refused
at upload with a clear message. There is no auto-upgrade and no "latest" fallback.

**The version is carried on the wire too.** Every v2 Hook envelope and every canvas export input carries `"v": 2`. A
guest that returns a v1-shaped envelope, or a host that delivers a v2 Hook to a v1 guest, is a hard failure, not a
best-effort parse. The command-buffer decoder is selected by the manifest ABI, so a v2 opcode in a v1 canvas is
`malformed`, and the v1 opcode set remains byte-identical inside v2.

**Dispatch points, all keyed off the manifest `v`:**

| Layer | Behaviour |
| --- | --- |
| Archive validation | `v` must be 1 or 2; the per-version file, template and control rules are chosen by it |
| Hook list | v2 adds `on_file` and `render_config`; a v1 archive declaring them is refused |
| Host imports | v2 adds `file_read` and `file_close`; a v1 archive importing them is refused |
| Command buffer | v1 opcodes are the v1 table; v2 appends new opcodes and leaves the old bytes alone |
| Templates | v2 adds `<lc-file>` and `templates/config.liquid`; v1 refuses both |
| Capabilities | v1 manifests have none; v2 manifests may declare them |

**CLI rollout.** `chittr-cartridge new` defaults to `--abi 2` once v2 ships, with `--abi 1` available for projects that
must stay on v1. `check` reads `v` and applies the matching rules. `pack` is unchanged. There is no silent migration:
an existing v1 project stays v1 until its author edits the manifest and the parts that need it.

**What v2 does not change.** v1's Hooks, envelopes, host functions, KV store, timers, Poster Actions, Render Pushes,
draw ops 0x01–0x51, limits, deadlines and gates keep their exact bytes and shapes. A v1 cartridge runs on a v2 host
without recompilation. Additions to v2 keep `v: 2` and are listed in its version history; a change to an existing v2
byte or shape needs `v: 3`.

---

## D2. Capability manifest and discovery — Recommended

**Capabilities are scoped to a guest and declared per guest.** A server capability never authorizes a canvas action and
vice versa, so the manifest names which guest each one belongs to rather than leaving the host to guess.

```json
{
  "v": 2,
  "name": "DOOM — first episode",
  "version": "0.3.0",
  "capabilities": {
    "server": {
      "require": ["files.server"],
      "optional": []
    },
    "canvas": {
      "require": ["files.client", "framebuffer.indexed8"],
      "optional": ["input.relative-pointer", "audio.pcm"]
    }
  }
}
```

- A guest key is present only when that guest exists. `server` with no `server.wasm`, or `canvas` with no `canvas.wasm`,
  is refused at upload.
- `require` and `optional` are arrays of capability ids. Both default to empty. Duplicates and unknown ids are refused.
- **Required** capabilities are validated against the host's supported set before the version is instantiated. If the
  host cannot honour one, the upload is refused (or the version is disabled, if the host lost support later). A guest
  never starts believing it has something it does not.
- **Optional** capabilities that the host does not support are simply absent from the effective set. Calling a host
  function that belongs to an unsupported capability answers `not_supported`; it is not a trap and not a silent no-op.
- Declaring a capability grants nothing by itself. It does not bypass a template-picker authorization, a user gesture,
  a permission check or a quota. It only tells the host what the guest may use.

**The initial v2 vocabulary is closed:**

| Capability | Guest | Meaning |
| --- | --- | --- |
| `files.server` | server | The guest may receive a server-routed file through `on_file` and read it |
| `files.client` | canvas | The guest may receive a client-local file event and read it |
| `framebuffer.rgba8` | canvas | The guest may present RGBA8 pixels with a destination rectangle (opcode 0x52) |
| `framebuffer.indexed8` | canvas | The guest may present indexed8 pixels with a palette and a destination rectangle (opcode 0x53) |
| `clock.monotonic` | both | Reserved; see D8 |
| `input.relative-pointer` | canvas | Reserved; see D9 |
| `audio.pcm` | canvas | Reserved; see D9 |

**Discovery.** The effective capability list and effective limits are delivered once per instance:

- Server Guest: the `init` input gains `"capabilities": ["files.server"]` and `"limits": { ... }`.
- Canvas Guest: the `start` input gains `"capabilities": ["files.client", "framebuffer.indexed8"]` and `"limits": { ... }`.

they are stable for the life of that Cartridge Version. the `limits` object carries the effective numbers so a guest
never hard-codes a host's caps. a Canvas Guest receives the full set; a Server Guest receives the same object without the
framebuffer keys (`maxFramebufferPixels`, `maxPaletteEntries`) and without `maxFileBytesPerCanvas`, which are canvas-only.
the schemas name these `CanvasLimits` and `ServerLimits`.

```json
{
  "maxFileBytes": 16777216,
  "maxFileReadBytes": 65536,
  "maxFileHandles": 4,
  "maxFileBytesPerCanvas": 33554432,
  "maxPickers": 8,
  "maxFramebufferPixels": 1048576,
  "maxPaletteEntries": 256
}
```

A guest that wants the effective frame rate, deadline class or memory cap keeps using the v1 mechanisms; `limits` here
covers only the new v2 resources.

*Why not put `capabilities` on every envelope?* it is stable per version and `init`/`start` always run, so repeating it
would be noise. if a host ever needs to revoke a capability mid-life, it disables the version instead.

---

## D3. Template-owned file pickers — Locked

**Syntax.** A new leaf Host Part, `<lc-file>`, with exactly these attributes:

```html
<lc-file
  name="wad"
  target="client"
  accept=".wad"
  max-bytes="16777216"
  label="Choose your local WAD">
</lc-file>
```

- `name` is required, matches the Control-name rule (`^[a-z][a-z0-9_-]{0,31}$`), and is unique within the rendered
  template. A duplicate is refused at upload (both in `card.liquid` and `config.liquid`, counted separately).
- `target` is required and exactly `client` or `server`. There is no default and no fallback.
- `accept` is optional: a comma-separated list of extensions (`.wad`) and MIME types (`application/json`), each at most
  64 characters, at most 16 entries. It is a picker filter, never a content check.
- `max-bytes` is optional: a positive integer. It may lower, never raise, the host cap. An absent value means the host
  cap. A value above the host cap is clamped at render, not refused, so a template stays portable.
- `label` is optional plain text, at most 200 characters, used by the host-owned input's accessible label. Absent means
  the host supplies a default.

**Host Part rules.** the element is a leaf: children, extra attributes, inline script and event handlers are dropped by
the sanitizer. the validator checks the attributes at upload; the client renderer checks them again at render. arbitrary
`<input type="file">` markup stays disallowed; `<lc-file>` does not loosen the general template allowlist. a picker is
per rendered card instance and per Cartridge Version: a late selection never reaches a replacement guest, a new version,
or another viewer's card. one selection delivers one file. directory and multi-select are out of scope.

**Client routing.** the page owns the input, checks size before reading, transfers the bytes to the canvas worker as an
`ArrayBuffer`, and the worker registers a private handle. the guest hears a metadata event; it never sees the bytes in
JSON. if the worker is unavailable, the host shows an actionable error and does not silently drop the selection; it does
not queue across a worker restart. re-selecting the same picker revokes the previous handle (see D4).

Canvas event:

```json
{ "type": "file", "name": "wad", "file": { "id": "f_7Qm…", "filename": "doom1.wad", "mimeType": "application/octet-stream", "size": 14445628 } }
```

**Server routing.** a signed-in viewer selects a file; the browser POSTs a bounded multipart body to a dedicated
endpoint (never a canvas `send`). the endpoint validates the authorization, current auth/visibility/blocks/active state
and the byte limits *before* multipart parsing, then invokes the Server Guest's `on_file` with a call-scoped handle.
the upload is not written to KV, assets, permanent storage or a public URL. anonymous server uploads are out of scope.

**The `on_file` Hook.** background class (2 s), like `on_canvas_message`. input is the standard envelope plus `surface`,
`name` and `file`:

```json
{
  "v": 2,
  "hook": "on_file",
  "chitt": { "id": "…", "posterId": "…", "createdAt": "…" },
  "observer": { "…": "…" },
  "surface": "card",
  "name": "import",
  "file": { "id": "f_7Qm…", "filename": "settings.json", "mimeType": "application/json", "size": 2048 }
}
```

output is a plain object: `{}`, `{ "rerender": true }` to redraw the sender's card through `render`, or
`{ "error": { "code", "message" } }`. there is no `reply`: the file did not come from the canvas worker, so there is no
canvas to answer. `observer` is never null for a server upload; a `target="server"` picker requires a signed-in viewer.

**Authorization binding.** a server picker carries a signed authorization derived from the sanitized render. it binds
the Chitt, the active Cartridge sha, the viewer Character, the render surface (`card` or `config`), the picker name, the
destination, the accepted types, the size limit and an expiry. the client cannot raise its size cap or invent a picker
by editing the DOM or the request. the endpoint rechecks authentication, visibility, blocks/OOC restrictions, active
state and the relevant permission on every upload; a valid token is not a substitute. a stale-version or expired
authorization runs no guest and returns a normal, non-guest error.

---

## D4. File handles and bounded reads — Locked

both routes expose the same metadata and the same two host functions. the metadata shape is identical on the client
event, the `on_file` input and (for the server) the upload's response:

```json
{ "id": "opaque", "filename": "doom1.wad", "mimeType": "application/octet-stream", "size": 14445628 }
```

`id` is opaque, unguessable and scoped to one server call or one canvas instance. `filename` is display metadata only
— never a path, never trusted for type. `mimeType` is the browser's report, a hint only. `size` is the host's measured
byte length, not the client's claim.

**`file_read`** (both guests):

```json
{ "handle": "opaque", "offset": 0, "length": 65536 }
→ { "bytes": "<base64>", "eof": false }
```

- `offset` and `length` are non-negative integers. `length` is 1 to 65536; over the cap is `too_large`, not a clamp.
- Reads clip at EOF. `offset == size` returns empty bytes with `eof: true`. `offset > size` is `invalid`.
- The result is at most 64 KiB of bytes (about 87 KiB of base64 JSON). This is a **bounded chunk channel**, not a
  whole-file channel: the abi.md must say so plainly, because a `format: byte` field in an XTP schema is easy to mistake
  for a zero-base64 binary channel. Whole-file base64 (the 14.4 MiB WAD → 19.3 MiB) is exactly what v2 exists to avoid.
- Unknown, closed, expired or foreign handles answer `not_found` and expose no bytes.

**`file_close`** (both guests): `{ "handle": "opaque" }` → `{}`. Closing an unknown or already-closed handle is
harmless and returns `{}`. A guest may close early; the host releases its buffer.

**Error codes** added to both guests' host-function error vocabulary: `not_found`, `too_large`, `not_supported`
(optional capability absent), and the existing `invalid` and `host`/`unavailable`.

**Lifetime and revocation.**

- A server handle lives only for its `on_file` call. It is revoked when the Hook settles, including timeout,
  cancellation and failure. A guest that needs the bytes later must copy them under its own memory and KV limits.
- A canvas handle lives until the guest closes it, the picker is re-selected, the worker is replaced, or the canvas
  terminates. Replacing the same picker's file revokes its previous handle; an in-flight read of a revoked handle
  answers `not_found` on its next call. Reads are synchronous within a call, so there is no torn read to specify.
- Repeated selections cannot accumulate unbounded host buffers: the live-handle and aggregate-byte caps in D10 are
  enforced at registration time, and a selection that would exceed them is refused before any bytes are read.

*Why base64 JSON rather than a binary host function?* XTP cannot describe a binary host-function result, and v1's one
binary channel is deliberately the command buffer (`submit`). adding a second binary channel would break the rule that
a host function's failure is an `error` field. 64 KiB chunks keep the base64 overhead bounded and small; the drawing
path (D5) gets true binary because it rides the existing command buffer.

---

## D5. Efficient framebuffer presentation — Recommended

**We present native-resolution frames as one-shot command-buffer ops, not as a guest-visible surface API.** the
proposal explicitly allows this ("A one-shot native-frame presentation operation is an alternative"). it meets every
required guarantee with far less lifecycle surface.

Two new draw opcodes:

| Opcode | Name | Arguments |
| --- | --- | --- |
| 0x52 | `blitRgbaScaled` | f32 dx, dy, dw, dh; u32 w, h; u8 smoothing; u32 n; u8 × n (`w × h × 4`) |
| 0x53 | `blitIndexedScaled` | f32 dx, dy, dw, dh; u32 w, h; u8 smoothing; u32 n; u32 × n palette; u8 × (`w × h`) indices |

- `dx, dy, dw, dh` is the destination rectangle in backing-store pixels. Unlike v1's `blitRgba` (0x51), these ops
  honour a destination rectangle, so the guest sends native-resolution pixels and the host scales them.
- `smoothing` is `0` for nearest-neighbour and `1` for linear. It overrides the canvas's `setImageSmoothing` (0x0a) for
  this op only. Pixel-art guests pass `0`.
- The palette is `n` entries of `0xRRGGBBAA` (the same colour packing as v1). `n` is 1 to 256; the host pads the
  remainder to 256 with fully transparent entries. Index `i` selects palette entry `i`; an index past `n` draws
  transparent. Alpha is straight (not premultiplied), and `0xFF` is fully opaque.
- `n` for `blitRgbaScaled` must equal exactly `w × h × 4`, as in v1. For `blitIndexedScaled`, the palette count and the
  index count are separate fields and each is checked exactly.

**Host behaviour and caching.** the host keeps one staging surface per canvas, reused across frames and resized only
when the source dimensions change. it expands indexed8 to RGBA8 into that surface and draws it into the destination
with the host's own 2D context, so no per-frame allocation or decode is needed. the staging surface is a bounded,
host-owned resource; it is never an HTML canvas, GPU context or object reference the guest can reach. retained pixels
are bounded by the source-pixel cap in D10, not by the destination rectangle, so a fullscreen destination does not
inflate the guest's payload.

**Why not a persistent surface handle.** a surface API (create/update/draw/release, four handles, 8 MiB aggregate)
buys partial updates. the motivating workloads redraw the whole native frame each tic, and a 320×200 indexed frame is
64 KiB — cheap to resend and well inside the 4 MiB command buffer. a handle API adds guest-visible lifetime, quota and
cross-instance-guess surface for no measured gain. if partial updates become a real need, `framebuffer.surface` can be
added later as an optional capability, keeping `v: 2`.

**Required guarantees, restated for this shape.** Pixel counts, dimensions, palette lengths and `w × h` overflow are
checked before allocation. A blit whose byte length does not match its dimensions is malformed and crashes the canvas
before anything draws, as in v1. Destination rectangles are checked against the existing backing-store rules. Nothing
crosses the interface except the op bytes: no data URLs, external images, canvas readback, shaders or browser object
references. Frame deadlines are unchanged; a faster path is not permission to submit more frames or more work.

---

## D6. Owner/admin configuration surface — Recommended

**`templates/config.liquid` is optional and separate.** When present, the host offers a cartridge-defined section
inside its existing "Cartridge…" panel for eligible viewers. It is never appended to `card.liquid`, never rendered on
an ordinary viewer's page to be hidden with CSS, and never rendered eagerly on a feed request.

**`render_config` is an optional Server Hook**, render class (500 ms), that supplies the config template's data. It runs
only after the host has authorized the viewer. Its input:

```json
{
  "v": 2,
  "hook": "render_config",
  "chitt": { "id": "…", "posterId": "…", "createdAt": "…" },
  "observer": { "…": "…" },
  "surface": "config",
  "access": { "audience": "owner_or_admin", "isOwner": true, "isAdmin": false, "canConfigure": true }
}
```

- `surface` and `access` are host-computed and host-authenticated. A guest cannot choose its surface by sending a
  template name or a flag; the host always renders `config.liquid` and always supplies the access context itself.
- `access.audience` is the strictest audience the config template's declarations require (see below). `isOwner` and
  `isAdmin` are host facts. `canConfigure` is the host's final decision for this request.
- Output: `{ "data": { … } }`, `{ "data": { … }, "error": { … } }`, or `{ "error": { … } }`. There is no `template`
  field and no `canvas` state; the template name is fixed and there is no second Canvas Guest in the config surface.

**If `config.liquid` exists without `render_config`**, the host renders the static template with `data: {}` and the
host context. The host never invokes public `render` to obtain private data.

**Control declarations gain `audience` and `confirm`.** In the manifest:

```json
"controls": {
  "reset_scores": {
    "value": { "enum": ["confirm"] },
    "audience": "owner_or_admin",
    "confirm": "Reset every score on this Chitt?"
  }
}
```

- `audience` is one of `public` (the default), `owner`, `admin`, `owner_or_admin`. It is validated at upload and
  rechecked on every action.
- `confirm`, when present, is a plain-text prompt at most 200 characters. The host renders the confirmation dialog and
  owns the buttons; a guest label saying "confirm" is not itself a confirmation.
- **Placement must match audience.** A Control bound in `card.liquid` must be `public`; a Control bound in
  `config.liquid` must be non-public. A mismatch is refused at upload. This is what stops a public Control from becoming
  privileged merely by being copied into the config template, or a config-only Control from being callable in public.

**`on_control` gains `surface`.** The host sets `"surface": "card"` or `"surface": "config"` in the v2 `on_control`
envelope, from the host-authenticated render surface, never from the request body. A public Control token cannot invoke
an identically named private action, and a config token cannot be replayed as a public interaction or on another Chitt:
the signed token binds surface, Chitt, version, viewer Character, control name and payload.

**Owner/admin eligibility.** the default audience `owner_or_admin` means the posting Character's owning account, or a
user holding the explicit administrative configuration permission. "Owner" is the account that owns the posting
Character, not "uploaded the latest version", and any Character on that owning account qualifies. Cartridge code may
narrow an audience further; it can never widen it. owner access to cartridge configuration does not grant moderation,
replacement, failure-log or administration permissions, and a moderation permission does not authorize every
cartridge-defined config action. config Hooks get no extra Poster Action powers; those actions keep their normal checks.

**Lifecycle and containment.** render the config lazily when an eligible viewer opens it. use the active Cartridge
Version's template and bindings; replacing or rolling back a version invalidates stale config views and action tokens,
as it does the public card. opening or closing the config does not restart the public Canvas Guest, reset gameplay or
change stored state. a config render failure is contained to the section; the host's close button and native management
tools remain. opening the panel does not bypass the kill switch, breaker or normal guest-call limits.

**File pickers in config.** Both destinations keep their D3 rules. `target="server"` needs a config-scoped
authorization and delivers a call-scoped handle to the authorized `on_file`. `target="client"` stays local and binds to
the associated card's existing canvas instance; opening the config must not create a hidden worker or route bytes into
a different instance.

**Data isolation.** config data stays on the authorized, viewer-scoped request path. it never enters public render
responses, public push payloads, feed caches, snapshots, public HTML or a shared cache. after an action, refresh the
config region independently; if public content changed, request an ordinary public-card render or invalidation too,
without publishing the private config response. settings are stored by guest Hooks under the existing state rules, not
by editing the Liquid file or implicitly serializing form contents.

---

## D7. Limits — Recommended

these are the v2 additions and changes. everything not listed keeps its v1 value.

| Resource | Value | Past it |
| --- | --- | --- |
| One selected file | 16 MiB; a template may lower it | refused before reading (`too_large` in the picker UI) |
| Live file handles per canvas | 4 | a new selection is refused (`limit`) |
| Aggregate file bytes per canvas | 32 MiB | a new selection is refused (`limit`) |
| A server `on_file` call's handle | 1, call-scoped | n/a |
| One `file_read` | 64 KiB | `too_large` |
| File pickers per template | 8, uniquely named | refused at upload |
| `accept` entries | 16, each ≤ 64 chars | refused at upload |
| Framebuffer source pixels per blit | 1,048,576 (`w × h`) | `malformed` (op is out of range) |
| Palette entries | 1 to 256 | `malformed` |
| Staging surfaces per canvas | 1, reused | n/a |
| Server upload body | 16 MiB, checked before multipart parsing | refused, no guest runs |
| Server upload admission | one per card in the UI, plus server-side per-viewer and per-IP limits | refused |
| Config render | render class, 500 ms, v1 output/tree caps | as `render` |

v1's 32 MiB Server Guest and 64 MiB Canvas Guest memory caps, deadline classes and small ordinary-message/state caps
stay as the baseline. The new host-held resources have their own aggregate caps (above), so moving bytes out of wasm
does not become an unbounded host-memory escape hatch. Network request limits must allow bounded multipart overhead
without accepting arbitrarily large bodies.

**Command-buffer budget.** A v2 command buffer stays at 4 MiB. The 1,048,576-pixel blit cap keeps a worst-case indexed
frame (1 MiB of indices plus 1 KiB of palette) and its RGBA staging (4 MiB) bounded, while comfortably covering
320×200, 640×480 and 1024×1024 software framebuffers. A larger source is split by the guest, as DOOM already splits its
output today.

---

## D8. Cooperative work budgets — Deferred

the cooperative continuation pattern is useful but not needed to ship the v2 core, and it needs host measurement before
its numbers are trustworthy. it is deferred and reserved as the optional `clock.monotonic` capability.

When added, the shape is:

- `now` host function (both guests) → `{ "ms": number }`, a monotonic millisecond clock. It is per-instance and reveals
  no wall-clock time. It is not a substitute for `frame`'s `t`.
- Hook and frame inputs may gain a soft `budgetMs`. A guest that can spread work returns early and asks for another
  callback (canvas: `request_frame`; server: a bounded re-entry the host schedules). The hard deadline still terminates
  a spinning guest; this is cooperative, not stack suspension.
- Pausing, hiding or replacing a canvas cancels pending work predictably. Resume does not require catching up an
  unbounded wall-clock interval.

adding these keeps `v: 2`. nothing in D1–D7 depends on them.

---

## D9. Relative pointer and PCM audio — Shipped

both ship as optional capabilities within `v: 2`:

- `input.relative-pointer`: a real host-owned click on the focused canvas enters pointer lock; the guest cannot enter it
  on its own. While locked, `pointer` events carry `relative: true`, `movementX`, `movementY` and `buttons`, with no
  canvas coordinates. Deltas are coalesced to one event per frame and each axis is clamped to the effective
  `maxRelativePointerDelta`. Lock is released on Escape, blur, tab hiding, termination or user cancellation, and the host
  sends `input_reset` so held buttons and keys do not stick. No global keyboard hooks, clipboard access or out-of-canvas
  coordinates.
- `audio.pcm`: interleaved signed 16-bit little-endian PCM at 22050, 44100 or 48000 Hz, mono or stereo, written with
  `audio_write` in chunks of at most 64 KiB. A bounded per-canvas queue (2 s) answers `accepted: false` when full;
  `queuedMs` reports the depth. `audio_stop` clears it. Playback is behind the existing host speaker control and real-user
  unlock; muting, pausing, hiding or terminating the canvas stops it. No AudioContext, microphone, device list or
  arbitrary worklet code is exposed to the guest.

DOOM's "no audio, no mouse aiming" state was the reason these were follow-ups rather than core; the capabilities are now
available for a port to opt into.

---

## D10. Conformance and rollout — Locked

1. **Keep v1 green.** All existing v1 fixtures, examples and conformance cases keep passing unchanged. A v2 host runs
   v1 without reinterpretation.
2. **Add v2 fixtures alongside.** New conformance cases for: unknown `v`, `v`/envelope mismatch, a v2 opcode in a v1
   canvas, `<lc-file>` attribute validation, duplicate picker names, picker placement vs. audience, `render_config`
   without `config.liquid`, config token replay across surfaces, and capability declaration errors.
3. **Add v2 host-function cases.** Read success, EOF, `offset > size`, oversized `length`, unknown/closed/expired
   handles, close idempotence, handle cap, aggregate cap, and revocation on re-selection and worker replacement.
4. **Add v2 framebuffer cases.** `blitIndexedScaled` and `blitRgbaScaled` golden bytes, palette length and index count
   mismatches, pixel-cap overflow, palette changes across frames, and nearest vs. linear.
5. **Extend the CLI, SDK and testing helpers together.** `check` dispatches on `v`; the SDK gains `file_read`/
   `file_close` wrappers, the two blit encoders, and v2 envelope types; the native testing helpers gain fake v2 host
   functions and a v2 envelope fixture.
6. **Ship a small v2 example** covering both picker destinations, success, refusal, EOF, close, handle expiry, and an
   optional `config.liquid` with owner/admin-only Controls and a configuration-scoped upload.
7. **Migrate DOOM** to client-local WAD loading and native indexed-frame presentation (opcode 0x53), keeping source and
   data licensing separate. Its normal cartridge must not embed or distribute WAD data.
8. **Verify with the real browser command player**, including fullscreen, CPU throttling, resize/DPR,
   visibility/resume, canceled selections, stale versions and guest replacement. A host-function stub is not enough.

security tests must cover forged picker authorizations, CSRF, wrong viewer/version/card, visibility and kill-switch
changes, oversized/chunked uploads, malformed ranges and pixel lengths, cross-instance handle guesses, resource
exhaustion, cleanup after failure/timeout, and the absence of any server request for a client-routed file. Config tests
add absence of the template, lazy rendering, owners without unrelated moderation powers, admin-only vs. owner-only
actions, unrelated viewers, revoked rights, cross-surface token replay, stale open dialogs, private-cache/data
isolation, independent refresh, and containment of config render failures.

---

## Open questions for the reviewer

these are the places where i picked a default that a reasonable person could flip. everything else above is either
forced by v1's rules or explicitly allowed by the proposal.

1. **Per-guest capability nesting** (D2) vs. a flat list. Nesting is unambiguous about which guest a capability belongs
   to; a flat list is shorter.
2. **`capabilities`/`limits` only on `init`/`start`** (D2) vs. on every envelope. The former keeps envelopes small; the
   latter is self-describing per call.
3. **One-shot scaled blits** (D5) vs. the proposal's persistent surface handles. I recommend one-shot; the surface API
   stays available as a later optional capability.
4. **Framebuffer pixel cap of 1,048,576** (D7). Raise it if a 1920×1080 native framebuffer is a target; it costs
   staging memory.
5. **Placement must match audience** (D6) vs. allowing a public Control in `config.liquid` that simply runs as public.
   I recommend refusing the mismatch.
6. **`on_file` has no `reply`** (D3). If a server upload should be able to answer the canvas, that needs a different
   routing story than the one in the proposal.
7. **`new` defaults to `--abi 2`** (D1) vs. staying on v1 until the author opts in. I recommend defaulting to 2 once
   v2 is released.
