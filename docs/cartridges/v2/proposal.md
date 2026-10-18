# Cartridge ABI v2 proposal

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so this is the wishlist page. v2 isnt real yet, its just me thinking out loud about pickers, framebuffers and a
config panel b4 we freeze anything. design.md is where the actual calls got made c;
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

**Status: draft for discussion — not implemented and not a normative ABI.**

this proposes a versioned successor to the [v1 ABI](../v1/abi.md), motivated by the DOOM port and useful to other
cartridges that process user-selected files or present software-rendered graphics. API names, capability identifiers,
wire layouts, and new quota values below are provisional. the eventual v2 schemas and conformance corpus must define
the contract before it ships; this document does not change v1.

## Summary

the recommended initial v2 core is:

1. **Template-owned file pickers**, with an explicit destination: Server Guest or Canvas Guest.
2. **Scoped file handles and bounded binary reads**, rather than whole-file base64 in Hook or message JSON.
3. **Host-owned framebuffers**, supporting RGBA8 and indexed8 pixels, with efficient host-side scaling.
4. **Capability negotiation and explicit versioning**, while continuing to support v1 unchanged.
5. **An optional `templates/config.liquid` configuration surface** inside the host's “Cartridge…” panel, separate from
   the public Live Chitt card and restricted to eligible owners/admins.

cooperative work-budget info is a useful supporting addition. relative mouse input/pointer lock and PCM audio are
candidates for optional, separately negotiated capabilities; they need not delay the initial core.

## Motivation

### File loading

a viewer should be able to supply their own game data, configuration, image or other input without rebuilding the
cartridge, embedding it in wasm, or handing the guest filesystem access. the server-rendered Liquid template should
place and configure the picker; the host should own the actual browser input and routing.

the current DOOM test WAD is about 14.4 MB. base64 would make it about 19.3 MB. v1 canvas messages and replies are each
limited to 4 KiB, and rendered canvas state to 16 KiB. relaying that file through ordinary messages would need
thousands of chunks, duplicate network traffic and a lot of transient memory. client-directed selections shouldn't
leave the browser at all.

### Framebuffer presentation

DOOM produces a 320×200 indexed framebuffer and a 256-colour palette. v1's RGBA blit ignores transforms, so the current
port expands and scales those pixels inside the guest. a 2048×1280 fullscreen image is roughly 10 MiB per draw.
splitting it into smaller submissions satisfies the per-buffer limit but doesn't reduce total work in the timed call.

a browser profile using the actual Chittr decoder/player reproduced fullscreen frame calls above the 250 ms deadline
under 4× CPU throttling. raising the timeout would just mask the cost. sending native-resolution pixels and letting the
host scale them addresses the cause, while keeping sandbox boundaries intact.

### Configuration separation

cartridge-specific owner/admin actions shouldn't have to occupy the public card, even conditionally. an optional
`templates/config.liquid` should render in the “Cartridge…” menu/panel instead. this creates a distinct place for setup,
imports, difficulty settings, leaderboard resets and other privileged operations without cluttering gameplay or the
feed. the separation must include server-enforced authorization, not just a different place to hide buttons.

## Goals and non-goals

### Goals

- Let a real user select a file through a Host Part in the card template.
- Route a selection explicitly to the server or the card's Canvas Guest.
- Keep large byte payloads separate from ordinary Hook/message JSON.
- Bound file lifetime, byte reads, memory, ingress, and processing work.
- Present software framebuffers efficiently at any allowed canvas backing size.
- Make support discoverable before an unsupported guest starts.
- Preserve the host's control over focus, visibility, fullscreen, sound, and user gestures.
- Separate privileged cartridge configuration from public card rendering, data, tokens, and interactions.

### Non-goals

- WASI, filesystem paths, directory enumeration, or arbitrary local file access.
- Arbitrary fetch, sockets, DOM access, guest-provided JavaScript, or custom GPU shaders.
- Automatically persisting uploaded files or sharing them with other viewers.
- Sending client-selected files to the server implicitly.
- Increasing memory/deadline limits to compensate for inefficient rendering.
- Making the current DOOM resource parsers safe for arbitrary hostile mods merely by adding a picker.

## 1. Template-owned file pickers

illustrative syntax:

```html
<lc-file
  name="wad"
  target="client"
  accept=".wad"
  max-bytes="33554432"
  label="Choose your local WAD">
</lc-file>

<lc-file
  name="import"
  target="server"
  accept=".json,application/json"
  max-bytes="1048576"
  label="Import configuration">
</lc-file>
```

### Host Part rules

- `name` identifies the picker within this rendered card; duplicate names are rejected or removed deterministically.
- `target` is explicitly `client` or `server`. There is no implicit destination and no fallback from client to server.
- `accept` restricts selectable extensions/MIME types. It is a hint/filter, not validation of file contents.
- `max-bytes` may lower, but never raise, the host's file-size cap.
- `label` is plain text used by an accessible, host-owned input.
- The element is a leaf. Template children, arbitrary attributes, script, and input event handlers are not kept.
- The sanitizer and client renderer both validate the attributes. Arbitrary native `<input type="file">` markup remains
  disallowed; this feature does not loosen the general template allowlist.
- One selection delivers one file initially. Multiple-selection/directory semantics are outside the initial scope.

the guest can't open a picker, preselect a file, or reach another file from the same directory. cancelling has no side
effect. re-selecting the same file should work. oversized or disallowed selections should produce an accessible error
without crashing the card or starting guest work.

### Client routing

1. The user selects a file in the host-owned input.
2. The host checks the picker configuration and file size before reading the contents.
3. The page transfers the bytes to the Canvas Guest worker using an ArrayBuffer, not a base64 message to the server.
4. The worker registers a private file handle and sends the guest a small metadata event.
5. The guest reads the bytes in bounded pieces and explicitly releases the handle when finished.

the delivery must be scoped to this rendered card instance and Cartridge Version, not just a Chitt id that could occur
multiple times on a page. a late selection must not reach a replacement guest, a new version, or another viewer's card.
no hidden-tab bulk loading should occur. if the worker is not available, the host must either queue within a strict
bound or show an actionable error; it must not silently drop the selection.

anonymous viewers may use client-local pickers, subject to normal visibility and canvas lifecycle rules. the contents
must not be included in server requests, analytics, crash reports, or logs.

### Server routing

1. A signed-in viewer selects a file in the host-owned input.
2. The browser sends an authenticated multipart upload through a dedicated endpoint, not a canvas `send` message.
3. The endpoint validates authorization and the bounded upload, then supplies a call-scoped handle to a Server Hook.
4. After success, the host can redraw the sender's card, as with a Control.
5. The handle and host-owned bytes are released when the Hook settles, including timeout, cancellation, and failure.

a server picker needs a signed authorization derived from the sanitized render. it should bind the Chitt, active
Cartridge sha, viewer Character, render surface (`card` or `config`), picker name, destination, accepted types, size limit,
and expiration. a client cannot
raise its size cap or invent another server picker by editing the DOM or request fields. the endpoint must also check
current authentication, visibility, blocks/OOC restrictions, active state, and relevant permissions; a valid token is
not a substitute for those checks. stale-version and expired authorizations do not run guest code.

ingress needs byte limits **before multipart parsing**, per-viewer and per-IP rate limits, bounded concurrency, request
timeouts, and cancellation. the guest execution deadline begins when its Hook runs, not while a user is uploading.
server uploads should require a signed-in Character in the initial version; anonymous server uploads need a separate
abuse-control design.

the server must not trust a client-supplied filename, MIME type, or reported size. filenames are display metadata, never
storage paths. MIME types and extensions cannot establish that contents are safe. the host never automatically writes
the file to KV, Cartridge assets, permanent storage, or a public URL.

## 2. File delivery and bounded binary reads

both routes should expose the same metadata shape:

```json
{
  "id": "opaque-scoped-handle",
  "filename": "doom1.wad",
  "mimeType": "application/octet-stream",
  "size": 14445628
}
```

A client event could be `{ "type": "file", "name": "wad", "file": { ... } }`. a server Hook, provisionally `on_file`,
would receive the normal versioned Chitt/Observer envelope plus the picker name and this metadata. the large payload
is not part of either JSON envelope. the server Hook's normal output can remain `{}` or the existing structured error.

### Proposed operations

- **Read:** handle, offset, and requested byte count → a bounded binary result.
- **Close:** handle → release the host-held file early; repeated closes are harmless.

the exact binary success/error representation must be specified in the v2 schemas and byte-level contract. a schema
`bytes` field that is actually base64 JSON must not be mistaken for a zero-base64 binary channel. the SDK should offer
ordinary byte buffers/slices and streaming helpers, without requiring guest authors to handle transport encoding.

Required semantics:

- Handle identifiers are opaque, unguessable, and scoped to one server call or canvas instance.
- Reads validate integer types, nonnegative offsets, maximum chunk size, and arithmetic overflow.
- Reads clip at EOF; offset equal to size returns empty bytes; offset greater than size is invalid.
- Unknown, closed, expired, or foreign handles are refused without exposing any bytes.
- Closing/replacing a worker revokes its handles. Completing a Server Hook revokes its handles.
- Repeated client selections cannot accumulate unbounded host buffers; replacing the same picker's file should revoke
  its previous handle, with documented behavior for an in-progress read.
- A guest that wants longer-lived data must deliberately copy/process it under its normal memory and storage limits.

for DOOM, the guest would assemble or otherwise read a selected WAD over multiple bounded calls, validate it, then retain
its backing allocation for the lifetime of the C engine. file delivery does not make the existing single-instance engine
restartable; replacing a loaded WAD needs a fresh engine instance or an explicit reset implementation.

## 3. Efficient host-owned framebuffers

the host should accept native-resolution frames and own the presentation surface. it's a bounded graphics resource, not
an HTML canvas or GPU context exposed to the guest.

### Formats

- **RGBA8:** four bytes per pixel, with an explicitly specified channel order and alpha convention.
- **Indexed8:** one palette index per pixel plus a 256-entry palette with explicitly specified RGBA entries.

DOOM would send 64,000 indices and about 1 KiB of palette data, instead of a fullscreen-sized RGBA image. the host can
expand to a cached native-resolution surface and use its own drawing API to scale it to a destination rectangle.
damage/pickup flashes and gamma-adjusted palette changes must be reflected, even if the indices do not change.

### Conceptual operations

1. Create a surface with fixed source dimensions and format; receive a scoped handle.
2. Update its pixel/palette data through a bounded binary operation.
3. Draw it into a destination rectangle, defaulting to nearest-neighbour sampling for pixel-art use.
4. Release it explicitly; guest termination releases it automatically.

A one-shot native-frame presentation operation is an alternative if it provides the same bounded-resource and caching
properties. the final API should avoid requiring a new host allocation or image decode on every frame.

### Required guarantees

- Pixel byte counts, dimensions, palette lengths, stride, and overflow are checked before access or allocation.
- Surface handles are scoped to the owning guest and cannot reference another canvas, DOM element, asset, or page region.
- The host clamps/checks destination drawing against the existing canvas/backing-store rules.
- Nearest-neighbour scaling and destination rectangles have specified behavior; RGBA blits that ignore transforms are
  not the only framebuffer path.
- Resource quotas include retained pixels, expanded indexed pixels, staging buffers, and any cached browser resources.
- No data URLs, arbitrary external images, canvas readback, shaders, or browser object references cross the interface.
- Normal frame deadlines remain in force. A faster path is not permission to submit unlimited frames or work.

## 4. Capability negotiation and versioning

### Versioning

v2 must be explicit at upload, validation, and runtime. the manifest ABI field (`v`) is distinct from the author's
Cartridge `version` string. Hook envelope versions and drawing-protocol versions must agree with the selected ABI;
changing only a label in the manifest is insufficient.

keep v1 validation and execution available without reinterpreting its bytes, Hook envelopes, or template rules. v2
imports/opcodes/Host Parts must not be silently accepted as v1. unknown ABI versions are refused clearly.

### Capabilities

the manifest should distinguish **required** and **optional** capabilities. illustrative identifiers are:

- `files.client`, `files.server`
- `framebuffer.rgba8`, `framebuffer.indexed8`, `framebuffer.scaled`
- `clock.monotonic` or an equivalent work-budget facility
- `input.relative-pointer`
- `audio.pcm`

the host validates required support before instantiation, and supplies the available capabilities and effective quotas
at runtime. unsupported optional capabilities have explicit fallback/error behavior. declaring a capability must never
itself grant a browser permission, bypass a user gesture, or permit arbitrary side effects.

capabilities belong to a particular guest/destination: a server upload capability must not authorize client-file reads,
and a Canvas Guest's local-file capability must not authorize a server upload. a manifest declaring support does not
replace template-picker authorization or per-use checks.

## 5. Cooperative work budgets

large-file decoding, map initialization, and game loops need a way to spread work across calls. v2 should clarify a
cooperative continuation pattern:

- Call inputs report a soft work budget, or the host exposes a bounded monotonic/remaining-budget query.
- The guest can retain progress, return, and request a subsequent callback.
- The existing hard deadline still terminates a spinning guest; this is not stack suspension or unrestricted execution.
- Pausing/hiding/replacing a canvas cancels or suspends pending work predictably. Resume must not require catching up an
  unbounded wall-clock interval.

clock precision and query cost need review. do not expose wall-clock secrets or promise that a guest can safely consume
its entire remaining hard deadline: transport and host drawing also take time.

## 6. Optional input and audio capabilities

### Relative pointer input / pointer lock

useful for mouse turning in DOOM. a real host-owned click or key activation enters pointer lock; guest code cannot enter
it on its own. supply relative deltas, coalesced and rate/size bounded, only to the focused canvas. release on Escape,
blur, tab hiding, termination, or user cancellation. send an explicit input-reset notification so held buttons/keys do
not remain stuck. do not add global keyboard hooks, clipboard access, or coordinates outside the canvas interaction.

### PCM audio streaming

useful for the engine's software mixer. support a small, specified set of PCM formats/sample rates and channel counts,
a bounded queue/ring buffer, and explicit backpressure/drop behavior. audio remains behind the existing host speaker
control and real-user unlock; hiding, pausing, muting, or stopping the guest stops output. no guest access to an
AudioContext, microphone, audio devices, or arbitrary worklet code.

local save persistence and user-approved exports are useful future work, but need separate ownership, quota, revocation,
and privacy rules. do not turn file selection into implicit permanent storage.

## 7. Optional owner/admin configuration surface

### Placement and lifecycle

a v2 cartridge may include **`templates/config.liquid`**. when present, the host offers a cartridge-defined configuration
section or tab inside its existing **“Cartridge…” menu/panel** for authorized viewers. it is not another feed card, is
not appended to `card.liquid`, and is never rendered into an ordinary viewer's page merely to be hidden with CSS.

- No `config.liquid`: no cartridge-defined configuration section; existing host management tools are unchanged.
- Render configuration lazily when an eligible viewer opens it, not on every public-card render or feed request.
- Use the active Cartridge Version's template and bindings. Replacing/rolling back a version invalidates stale config
  views and action tokens, just as it does the public card.
- Opening or closing configuration does not restart the public Canvas Guest, reset gameplay, or change stored state.
- A config render failure is contained to that section. The host's close button and authorized native management tools
  remain available; a broken guest template cannot take over the management dialog.
- Opening the panel does not bypass the kill switch, breaker, or normal guest-call limits. Host-owned repair/replacement
  tools can remain available under their existing permissions when guest configuration cannot run.

the host owns the panel shell, navigation, focus behavior, and trusted management controls. the guest can fill only its
sanitized configuration region, not replace “Disable”, “Replace”, logs, or other native management UI.

### Rendering contract

recommended shape: reserve the template name `config` and add an optional, separately authorized **`render_config`**
Server Hook for dynamic data. the exact Hook name/envelope remains to be finalized in the v2 schemas.

- If the file exists without that Hook, render a static configuration template with the host's trusted context and empty
  guest data. The host must not invoke public `render` merely to obtain private configuration data.
- If the Hook exists, its output supplies data for `config.liquid`; the caller cannot choose an arbitrary public/private
  template by sending a template name.
- Supply the acting Observer, Chitt, and host-computed access context. Keep trusted role/context fields separate from
  guest-returned data so the guest cannot overwrite them.
- Apply render-class deadlines, output/tree caps, Liquid restrictions, and sanitizer rules. This is still untrusted
  cartridge content, not arbitrary HTML or JavaScript running in the application.
- The initial config surface supports declared Controls and file pickers, not a second independent Canvas Guest. A
  client-routed picker must bind explicitly to the associated card's existing guest instance; opening the dialog must
  not silently create a hidden worker or route bytes into a different instance.

Illustrative `templates/config.liquid` content (the named Controls must also have configuration-scoped declarations):

```html
<section>
  <h3>Game settings</h3>
  <button data-lc-control="reset_scores" data-lc-value="confirm">Reset leaderboard</button>
  <lc-file
    name="settings_import"
    target="server"
    accept=".json,application/json"
    max-bytes="1048576"
    label="Import game settings">
  </lc-file>
</section>
```

a host-managed confirmation should be available for destructive actions; a guest label saying “confirm” is not itself
an authorization or confirmation guarantee. exact confirmation declaration syntax remains open.

### Owner/admin authorization

proposed default eligibility is **the posting Character's owning account, or a user with the appropriate explicit
administrative configuration permission**. ownership does not mean “uploaded the latest version”, and a viewer using
another Character on the same owning account should not accidentally lose owner access. the exact permission mapping
must be specified by the host rather than inferred from an arbitrary guest-supplied boolean.

configuration actions may be narrowed to owner-only, admin-only, or owner-or-admin audiences through host-validated
Control/file-picker declarations. liquid conditions may customize presentation within that audience, but do not grant
access. cartridge code can restrict an operation further; it cannot expand host-granted rights.

Required boundaries:

- The server authorizes the config render **before running the guest or returning its data**. Hiding a menu entry is
  not the access check. Anonymous and unrelated viewers receive no private configuration tree or data.
- Recheck current ownership/permissions, visibility, active version, and enabled state on every config action/upload,
  not only when the dialog opens. An old token does not preserve revoked permissions.
- Bind action/upload authorizations to **`config` versus `card`**, as well as viewer, Chitt, version, name, and allowed
  payload. A public Control token cannot invoke an identically named private action; a config token cannot be replayed
  as a public interaction or on another Chitt.
- A declaration identifies the allowed surface and audience. Placing a public Control in `config.liquid` must not
  magically make its endpoint privileged; conversely, copying a config-only binding into public markup must not make
  it callable there.
- Hook inputs identify the surface using host-authenticated context, not an untrusted request flag. The existing action
  Hook may be reused with this context, or dedicated config Hooks may be defined; the v2 schema must settle this.
- Owner access to cartridge-defined configuration does **not** automatically grant all existing moderation, replacement,
  failure-log, or administration permissions. Likewise, permission to open the management panel for a native moderation
  task does not automatically authorize every cartridge-defined configuration action.
- Config Hooks do not receive extra Poster Action powers. Those actions still undergo their normal permission checks;
  “admin clicked a button” must not become unrestricted impersonation or a sandbox bypass.

### Data, state, and file routing

config data stays on an authorized, viewer-scoped request path. do not place it in public render responses, public push
payloads, feed caches, snapshots, HTML, or a shared cache without correct authorization partitioning. after an action,
refresh the config region independently; if gameplay/public content changed, request an ordinary public-card render or
invalidation too, without publishing the private config response.

both file-picker destinations retain their ordinary rules within this surface. `target="server"` uploads require
config-scoped authorization and deliver a call-scoped file to the authorized Hook. `target="client"` stays local and
targets the associated card instance only. neither destination gains storage or redistribution permission by appearing
in the management panel. a viewer's personal DOOM WAD picker belongs on the ordinary card if all players should use it;
owner/admin setup imports belong in `config.liquid` when they change shared cartridge configuration.

settings are deliberately stored by guest Hooks under the existing state rules, not by editing the Liquid file or
implicitly serializing form contents. shared-state changes still need input validation, any necessary concurrency
control, and migration handling. the template is versioned presentation, not a separate unversioned settings database.

## 8. Provisional limits

these are starting points for implementation and load testing, **not committed ABI guarantees**:

| Resource | Suggested starting bound |
| --- | --- |
| One selected file | 32 MiB; a template may lower it |
| Live client file handles | 4, with 40 MiB aggregate file bytes per canvas |
| File read | 64 KiB per operation |
| File pickers per card | 8 uniquely named, one file per selection |
| Framebuffer surfaces | 4 handles, with 8 MiB aggregate storage including caches/staging |
| Server upload concurrency | One per card in the UI, plus enforced server-side admission limits |
| Server upload rate | Separate low-frequency per-viewer and per-IP limits; not ordinary canvas-message quotas |

v1's 32 MiB Server Guest and 64 MiB Canvas Guest memory caps, deadline classes, and small ordinary-message/state caps
should remain the starting baseline. new host-held resources need their own aggregate limits; moving bytes out of wasm
must not create an unbounded host-memory escape hatch. network request limits must allow bounded multipart overhead
without accepting arbitrarily large bodies.

## 9. Rollout and acceptance criteria

1. Review this proposal and settle naming, binary result/error framing, effective limits, and version dispatch.
2. Define v2 server/canvas schemas, template rules, and capability declarations before adding host-only interfaces.
3. Extend the host, CLI validator/scaffolder, Rust SDK, native testing helpers, and conformance fixtures together.
4. Keep v1 fixtures and existing example behavior passing unchanged; add an explicit v2 creation/migration path.
5. Add a small v2 example covering both picker destinations, success, refusal, EOF, close, and handle expiry, plus an
   optional `config.liquid` with owner/admin-only Controls and a configuration-scoped upload.
6. Migrate DOOM to client-local WAD loading and native indexed-frame presentation. Its normal cartridge should not
   require embedding or distributing WAD data. Retain source/data license separation.
7. Verify with the actual browser command player, including fullscreen, CPU throttling, resize/DPR, visibility/resume,
   canceled selections, stale versions, and replacement of guest instances. Do not rely only on a host-function stub.

security/conformance tests must cover forged picker authorizations, CSRF, wrong viewer/version/card, visibility and
kill-switch changes, oversized/chunked uploads, malformed ranges and pixel lengths, cross-instance handle guesses,
resource exhaustion, cleanup after failure/timeout, and absence of any server request for a client-routed file.

configuration tests must additionally cover absence of the optional template, lazy rendering, owners without unrelated
moderation powers, admin-only versus owner-only actions, unrelated viewers, revoked rights, cross-surface token replay,
stale open dialogs, private-cache/data isolation, independent refresh, and containment of config rendering failures.

for DOOM, acceptance includes native-resolution-sized frame payloads independent of fullscreen area, preservation of
palette changes and pixel-art scaling, and substantial measured headroom below the existing frame timeout on slower
clients. choosing a local file must not silently distribute it to other viewers.

## Open decisions

- exact capability manifest syntax, discovery payload, and rollout/default ABI selection in the CLI.
- binary read/update wire framing and error representation, including how XTP bindings express it without whole-file
  base64 or mandatory large copies.
- `on_file` Hook shape/class, response/rerender behavior, and whether future server uploads may be asynchronous.
- the exact `render_config` envelope and static-template context, configuration capability declaration, owner/admin
  permission mapping, surface/audience declarations, action-Hook reuse, and destructive-action confirmation semantics.
- whether unavailable client workers queue one bounded selection or require retry; retention while off-screen.
- surface API versus a single frame-presentation operation, partial updates, and exact alpha/stride semantics.
- clock precision and soft-budget defaults; combined resource quotas across files, media, framebuffers, and audio.
- whether relative input and PCM ship with the initial v2 core or are negotiated follow-up additions.
