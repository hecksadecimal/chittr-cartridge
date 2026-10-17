# Cartridge ABI v1: design notes

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so this is the why page. why the abi looks like this, what it had to dodge, what we tried first and tossed.
read it b4 u propose a change, pls
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

this page says why the Cartridge ABI is shaped the way it is. `abi.md` says what the ABI is, and the two schemas,
`server.xtp.yaml` and `canvas.xtp.yaml`, say what every name and shape is. come here when u wanna know why a rule exists,
or before u propose a change.

## 1. The ABI is defined by XTP schemas

the Server Guest's and the Canvas Guest's exports and host functions are described by two XTP schema files in this
directory, written in XTP schema `v1-draft`. the schemas are the source of truth for names, shapes and descriptions. a
host that runs Cartridges is checked against them: when the host's Hook list, its host functions or a JSON example stop
matching the schema, the host is out of step with the ABI. the schema descriptions are written for the Cartridge author,
not only for the code generator, so they are the reference for what each field means.

### Two rules that follow from the XTP format

XTP has no unions, so the ABI is built around two rules:

- **Every host function and every Hook takes one JSON object and returns one.** Each has named fields. A failure is an
  `error: { code, message }` field on the output. It is never a trap and never a byte framing. Byte payloads (KV values,
  asset bytes) travel as base64 in a `type: string, format: byte` field. The one exception is the Canvas Guest's
  `submit`, which takes the binary command buffer and returns nothing (section 2).
- **Every Hook return is a plain object.** A gate returns `{ allow?, message? }`, where a missing `allow` means allow.
  Every output type may carry an optional `error`, which is how a Hook reports a failure it handled itself.

earlier drafts of the ABI used a bare `true` or `false` for gates, byte framing for `asset`, and a union for the target of
a Render Push. each of those had to change, cause XTP can't describe it. the Render Push target is now a flat object:
`{ characterId? }`, where an absent `characterId` means every Observer of the Chitt. `asset` is now `{ path }` in and
`{ bytes?, error? }` out.

### What XTP cannot express, and how the ABI handles it

- **Any-JSON values.** a timer payload, a canvas message, a reply, a render's `canvas` state and a Control's `fields` can
  carry any JSON that fits the size limit. XTP has no untyped value and rejects an untyped property, so these are typed
  as `object`. the schema documents the common case, and the host accepts any JSON that fits.
- **Maps.** XTP maps (an `object` with a typed `additionalProperties`) generate Rust that does not compile against the
  validator, and named maps are refused. so the ABI avoids typed maps. for example, `kv_mget` returns
  `{ entries: [{ key, value }] }`, and `kv_get_all` returns `{ entries: [{ key, value }], truncated }`. arrays of entries
  are used rather than objects keyed by name.
- **Empty objects.** the Rust template can't express an empty object. so every output carries its optional `error`, and a
  host function with no input (such as `request_frame`, `kv_get_all` or `unpin`) omits its input. on the wire the call is
  still `{}`.
- **Optional fields in a Rust object.** an object whose fields are all optional says `required: []` in the schema. without
  it, the generator makes every field required, so an empty success would serialize every field, `error` included.
- **Enum values.** the generator requires enum values to be identifiers, so a value such as a moderation duration (`1m`,
  `30d`) can't be an enum. those values are inline strings, and the schema's description lists them. the host checks them.

### Logging is not part of the ABI

guest logs use Extism's own `log_*` imports, not a host function of this ABI, so they need no entry in the schema.

### Consequences for changes

- a new Hook or host function changes the schema in the same change. anything in the host that is not in the schema is not
  part of v1.
- every example and SDK type should round-trip against the same JSON examples. the SDK's tests check that the Rust types
  parse the examples in `sdk/chittr-cartridge/tests/fixtures/cartridge-abi/` and produce them again.
- the generated `src/pdk.rs` of an example is produced from the schema, so a schema change shows up as a regenerated
  binding, and the example's Hooks are pruned to the ones it defines (see the example READMEs).

### Alternatives considered

- **Keep the byte framing for `asset`.** it saves a base64 round trip, but it's the one host function the schema could not
  describe, and it broke the rule that a failure is a field.
- **Keep the Render Push union.** it reads naturally, but XTP can't describe it, so the schema would have to misstate one of
  its two forms.
- **Type every "any JSON" value as untyped.** XTP rejects untyped properties, so that wasn't an option. typing them as
  `object` documents the common case and leaves the host to accept any JSON.
- **Name every map in `components/schemas`.** the validator rejects a named schema with `additionalProperties`, so maps stay
  inline.

## 2. The Canvas draw ops travel as a binary command buffer

a Canvas Guest draws by encoding a frame's draw ops into one compact binary stream and handing it to the host in a single
`submit` call. the stream is little-endian: a `u8` opcode, followed by fixed `f32`, `u32` and length-prefixed UTF-8
arguments. the Rust SDK hides the encoding behind a Canvas2D-like API, so authors never write bytes by hand.

only operations that return a value or have effects outside the canvas stay as their own host functions: `measure_text`,
`load_image`, `load_sound`, `play_sound`, `stop_sound`, `set_fps`, `request_frame` and `send`. drawing stays immediate-mode,
the way it is on Canvas2D. nothing is cleared between frames, and the host presents the canvas when the export returns.

### Why one call per frame

every host call crosses the wasm boundary and marshals its arguments. a 60 frames-per-second scene of a few hundred ops would
make tens of thousands of host calls a second, and the overhead would grow with the scene. one call per frame keeps that
overhead flat. the stream also replays straight onto an `OffscreenCanvasRenderingContext2D`, so the host does no per-op
translation beyond decoding.

### Consequences

- the byte layout is part of the `v: 1` ABI. new opcodes may be added without changing `v`. changing or removing an existing
  one needs a new `v`.
- a malformed stream or an unknown opcode counts as a crash, not a silent skip, so encoder bugs surface early.
- supporting another guest language means porting the encoder as well as the host-function bindings.
- the op table in `abi.md` is the reference. the SDK's encoder is tested against a golden byte file, so a change to the
  encoding shows up as a failing test.

### Alternatives considered

- **One host function per op** (`fill_rect(x, y, w, h)` and so on). the simplest binding, and it matches the Canvas2D API
  most closely, but its per-call overhead grows with the scene's complexity.
- **A JSON list of ops per frame.** it's language-neutral and easy to debug, but parsing dominates on busy frames, and
  `blitRgba` pixel data would bloat the stream.
