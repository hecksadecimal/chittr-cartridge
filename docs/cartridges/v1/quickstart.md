# Cartridge quickstart (ABI v1)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so u wanna make a cartridge. good. lemme walk u thru it, from an empty folder to a packed one. the cli does most of the
boring stuff for u now
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

a cartridge is the code and content behind a chitt: a Server Guest (`server.wasm`), an optional Canvas Guest
(`canvas.wasm`), a `manifest.json`, the card's `templates/`, and its `assets/`. the ABI itself is in `abi.md`, the two
schemas it's generated from are `server.xtp.yaml` and `canvas.xtp.yaml`, and `design.md` explains why the ABI is shaped
this way.

## Prerequisites

- Rust with the `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`. the host loads wasm32 modules
  with no WASI imports.
- the `chittr-cartridge` CLI:

  ```bash
  cargo install --git https://github.com/hecksadecimal/chittr-cartridge chittr-cartridge
  ```

  the CLI makes projects, checks and packs them, syncs bindings, and watches a project while u edit it. no Node.js and no
  `xtp` needed.
- optional: `xtp`, the XTP CLI, if u want to regenerate the bindings from the schemas yourself (see the last section).

## 1. Create the project

```bash
chittr-cartridge new my-cartridge            # a Server Guest
chittr-cartridge new my-cartridge --canvas   # a Server Guest and a Canvas Guest
cd my-cartridge
```

`new` writes this layout. `src/pdk.rs` is already pruned to the two Hooks the starter defines (`init` and `render`), so the project
builds and checks with no edits:

```
my-cartridge/
  Cargo.toml       the Server Guest crate; chittr-cartridge is the SDK (from GitHub, or --sdk-path DIR)
  build.sh         builds the wasm into cartridge/
  README.md
  src/lib.rs       the Hooks you implement (yours)
  src/pdk.rs       generated bindings, pruned to the Hooks src/lib.rs defines (don't edit)
  canvas/          the Canvas Guest crate, with --canvas
  cartridge/
    manifest.json
    templates/card.liquid
    assets/
```

the starter `render` returns the data the card draws. `render` gets the Observer (the viewer, or none when logged out), so
each viewer can get their own card. it runs on every view, under a 500 ms deadline.

a Server Guest must define `render`, the one Hook the format requires. a Canvas Guest must define `start` and `frame`, and
`event` is optional. implement the other Hooks in `src/lib.rs` as u need them, then run `chittr-cartridge sync` (see below) so
their wrappers show up in `src/pdk.rs`.

the starter also has an `init`, and u'll want to keep it. Chittr calls `init` once, when the chitt is published, and takes the
chitt's text from its `content`. it refuses to publish a Cartridge without one. `check` and `pack` warn about a missing `init`
(`warning: server.wasm: no init Hook — Chittr refuses to publish without one (it supplies the Chitt's text)`), and they still
pass, since the format itself only asks for `render`. `sync` keeps the `init` wrapper in `src/pdk.rs` for as long as
`src/lib.rs` defines `init`, so keep the Hook there.

## 2. Write the Hooks

write the Hooks u implement in `src/lib.rs`. the generated structs are the wire shapes. two things differ from a hand-written
guest:

- a failure is the output's `error` field (`Some(HookError { code, message })`), never an `Err`. an `Err` from a Hook becomes
  a trap.
- an output's optional fields are `Option`s, and `None` is left out of the JSON, so a success return of `{}` is
  `Default::default()` with no `error`.

`Cargo.toml` already has the two edits a Server Guest needs:

```toml
chrono = { version = "0.4", default-features = false, features = ["serde"] }  # the clock pulls in wasm-bindgen exports, which the validator refuses

[profile.release]
opt-level = "s"
lto = true
strip = true
```

drawing on a Canvas Guest goes through the SDK's command-buffer encoder, `Canvas2D`. the starter in `canvas/src/lib.rs`
shows it. call `Canvas2D::submit` to send each frame, and read `examples/poll/canvas/src/lib.rs` for a full example. the host
functions (`request_frame`, `set_fps`, `send`, `load_image`, `load_sound`, `play_sound`) come from the generated bindings in
`src/pdk.rs`.

## 3. The Cartridge's files

`manifest.json` names the Cartridge and declares its fonts and Controls:

```json
{
  "v": 1,
  "name": "My poll",
  "version": "0.1.0",
  "fonts": { "Mono": "assets/geist-mono.woff2" },
  "controls": {
    "vote": { "value": { "enum": ["pineapple", "anchovies"] } }
  }
}
```

`templates/card.liquid` is the card's markup. it draws the `data` that `render` returns, through the Liquid template the host
renders. the Host Parts (`lc-author`, `lc-reply`, `lc-reactions`, ...) are placed by the template, and a Canvas Guest is placed
with `<lc-canvas>`. a Cartridge with `canvas.wasm` has to place it, or the upload gets refused:

```html
<section><h2>{{ question }}</h2><lc-canvas width="320" height="180" label="Results"></lc-canvas></section>
```

`assets/` holds the files the guests and the card use: images and sounds the Canvas Guest loads by path (`load_image`,
`load_sound`), a font named in `fonts`, or data the Server Guest reads with `asset()`. the validator checks the sizes and names
of everything in it.

the poll's `cartridge/` directory is the complete example of these files.

## 4. Check, build and pack

```bash
./build.sh                          # builds server.wasm (and canvas.wasm) into cartridge/
chittr-cartridge check cartridge    # validates with the server's rules; prints "file: rule" on failure
chittr-cartridge pack cartridge     # writes my-cartridge.cartridge, validated
```

`build.sh` runs the release build for `wasm32-unknown-unknown` and copies each wasm into `cartridge/`. `check` takes a
directory or a `.cartridge` file. `pack` writes `my-cartridge.cartridge` into the project, next to `cartridge/`, and refuses
to write an archive that doesn't validate. pass a path as the second argument to put it somewhere else.

if `check` prints a warning, read it before u upload. a warning doesn't stop the upload, but a missing `init` does stop the
publish.

to iterate, run `chittr-cartridge watch cartridge` in a second terminal. it repacks and checks on every save. nothing gets
uploaded: upload the `.cartridge` yourself, through the composer.

## 5. Try it on Chittr

upload the `.cartridge` in the composer with **Publish as preview** on. a preview is visible only to u and any Testers u add,
and its cartridge runs for real. open the chitt and use **Replace** in the **Cartridge…** panel to upload a new version. each
replacement makes a new Cartridge Version, and earlier versions can be rolled back to.

an upload that breaks a rule gets refused with the rule that failed, which is the same text `check` printed.

## 6. Test the Hooks natively

the SDK's `testing` feature runs a Server Guest's Hooks under `cargo test`, against a fake host that keeps the Chittr host's
rules. add it as a dev-dependency, next to the one the starter already has:

```toml
[dev-dependencies]
chittr-cartridge = { package = "chittr-cartridge-sdk", git = "https://github.com/hecksadecimal/chittr-cartridge", features = ["testing"] }
```

then a test hands a Hook an envelope and checks what it did to the host:

```rust
let host = TestHost::new();
let chitt = testing::chitt("chitt-1", "poster-1");
let output = my_cartridge::init(testing::init(&chitt));
assert!(!output.content.is_empty());
assert_eq!(host.timers().len(), 1);
```

the [guide's testing section](guide.md#testing) lists everything the fake host records and fakes, and what it doesn't model.
the poll's `examples/poll/src/tests.rs` is a full suite.

## Regenerating the bindings when the schema changes

when an ABI change adds a Hook or a host function, the schema changes first. then regenerate the bindings:

```bash
SCRATCH=$(mktemp -d)
xtp plugin init --schema-file docs/cartridges/v1/server.xtp.yaml --template Rust --path "$SCRATCH/server" --name my-cartridge -y
chittr-cartridge sync "$SCRATCH/server" .
```

`sync` copies only the scaffold's `src/pdk.rs`, and removes each export wrapper whose Hook your `src/lib.rs` doesn't define. a
wrapper is a block that starts with `#[no_mangle]` and `pub extern "C" fn <name>() -> i32`. the generated `src/pdk.rs` is
marked "do not edit", so change the schema and regenerate rather than editing its types. `sync` never touches `src/lib.rs`,
`Cargo.toml` or `xtp.toml`. for the Canvas Guest, scaffold `canvas.xtp.yaml` the same way and sync it into `canvas/`.

## The SDK path

if u'd rather write against the SDK's helpers, start from `examples/poll` and change its Hooks. the SDK's `#[hook]` attributes
and the `chittr_cartridge::kv`, `timer`, `poster`, `push` and `canvas` modules cover the same host functions, and each example's
README says what it does. use the SDK's error types for failures, and check the output's `error` field the way
`examples/poll/src/lib.rs` does.

the examples are the reference. each one builds to wasm with `sh scripts/build-examples.sh`, so start from a working example and
change it.

for the rest, the [guide](guide.md) covers each SDK piece, the [limits](limits.md) page has every number an author runs into, and
the [cookbook](cookbook.md) has recipes for the common shapes: polls, counters, timers that close things, canvas games, logged-out
Observers, and the Preview and Tester workflow on Chittr.
