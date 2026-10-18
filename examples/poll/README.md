# Poll (example Cartridge)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok the poll. admin-set questions and answer choices, transparent canvas buttons, a timer, the works. good one to read after hello
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

an admin-configured poll. a new Chitt says to wait until the poll is ready; a site admin sets its question and 2–10 answer
choices on the card. setup starts the one-hour timer and changes the Chitt's stored text to the question. upgrades refresh
that text through `migrate`. a transparent Canvas Guest draws the choices as buttons; viewers click one to vote. a bar
under each choice animates with its share, and the leading choice is gold. the poster can close the poll, and the result is
announced once, whichever closes it. views count once per signed-in Character; anonymous renders don't count. the Server
Guest is `src/lib.rs`; the Canvas Guest is `canvas/src/lib.rs`.

the Server and Canvas Guest bindings are generated from the XTP schemas in `docs/cartridges/v1/` with `xtp plugin init`
(the stock Rust template), not written by hand. the commands are below.

## Scaffolding

run from the repository root. each `xtp plugin init` writes into a scratch folder, and `chittr-cartridge sync` copies its
`src/pdk.rs` into each guest and prunes it to the Hooks that guest defines. `src/lib.rs` and `canvas/src/lib.rs` are the
author's files, and sync never writes to them.

```bash
SCRATCH=$(mktemp -d)

# Server Guest, from the Server schema
xtp plugin init --schema-file docs/cartridges/v1/server.xtp.yaml --template Rust --path "$SCRATCH/server" --name poll -y
chittr-cartridge sync "$SCRATCH/server" examples/poll

# Canvas Guest, from the Canvas schema
xtp plugin init --schema-file docs/cartridges/v1/canvas.xtp.yaml --template Rust --path "$SCRATCH/canvas" --name poll-canvas -y
chittr-cartridge sync "$SCRATCH/canvas" examples/poll/canvas
```

`xtp plugin init` generates a binding for every export of the schema, each one a `#[no_mangle]` wrapper in `src/pdk.rs`. the
host calls any Hook it finds exported, so `chittr-cartridge sync` removes the wrapper for each Hook the guest's `src/lib.rs`
doesn't define. a wrapper is the block from `#[no_mangle]` to the closing brace of `pub extern "C" fn <name>() -> i32`. the
Server Guest's `src/lib.rs` defines `init`, `migrate`, `render`, `on_control`, `on_reply`, `on_timer` and
`on_canvas_message`, so `on_reaction`, `on_rechitt` and `on_view` get removed. `migrate` refreshes the Chitt's stored body
from its saved poll question when a new Cartridge Version is installed. the Canvas Guest defines `start`, `frame` and
`event`. a Server Guest must define `render`; a Canvas Guest must define `start` and `frame`.

the generated `src/pdk.rs` is marked "do not edit": change the schema and regenerate. the schemas are documented in
`docs/cartridges/v1/` (see `design.md` there).

### What the scaffold doesn't give us

- **`Cargo.toml`** is the template's, with these additions: the SDK, for the Server Guest's host calls (`kv`, `timer`,
  `poster` and `push`), with its `testing` feature under `[dev-dependencies]` for the tests; chrono without its default features (its clock pulls in
  `wasm-bindgen`, whose exports the validator refuses), and a `[profile.release]` that keeps the wasm small. `canvas/Cargo.toml`
  enables the SDK's `canvas` feature for the command-buffer encoder. regenerating doesn't touch these, so keep them when u
  change the dependencies.
- **Canvas rendering** uses `Canvas2D`, the SDK's encoder for the command buffer described in `docs/cartridges/v1/design.md`.
- **Error returns**: the template's output structs for an optional `error` field are only usable as `Option` fields because
  the schemas list `required: []` on those objects. the poll's `src/lib.rs` returns failures in the output's `error` field,
  never as an `Err`, since an `Err` becomes a trap.

## Testing

```bash
cargo test --manifest-path examples/poll/Cargo.toml
```

`src/tests.rs` runs the Server Guest's Hooks against the SDK's fake host (the `testing` feature), so no wasm and no Chittr
are needed. tests cover the waiting state, admin-only one-time setup and validation, the Chitt body edit and migration,
dynamic answer choices, canvas voting, tallies, the close timer announcing once, the retry after a failed announcement, the
reply gate after close, and who may vote and close. CI runs them on every push.

## Building and packaging

```bash
cargo build --manifest-path examples/poll/Cargo.toml --release --target wasm32-unknown-unknown
cp examples/poll/target/wasm32-unknown-unknown/release/plugin.wasm examples/poll/cartridge/server.wasm
cargo build --manifest-path examples/poll/canvas/Cargo.toml --release --target wasm32-unknown-unknown
cp examples/poll/canvas/target/wasm32-unknown-unknown/release/plugin.wasm examples/poll/cartridge/canvas.wasm
chittr-cartridge pack examples/poll/cartridge   # writes examples/poll/poll.cartridge
```

the build writes `examples/poll/cartridge/server.wasm` and `canvas.wasm`. `chittr-cartridge pack` checks the folder and
packs it into `examples/poll/poll.cartridge`. to test the result on Chittr, see the repository README.

## When the schema changes

run the `xtp plugin init` and `chittr-cartridge sync` commands again (use a fresh `SCRATCH`), then rebuild and pack as above.
your `src/lib.rs` is never overwritten, so a Hook u added there is exported as soon as sync keeps its wrapper. a Hook u
didn't implement gets removed by sync.

## Files

- `src/lib.rs`, `canvas/src/lib.rs`: the poll's logic (author's files).
- `src/tests.rs`: the Server Guest's tests (`cargo test`, above).
- `src/pdk.rs`, `canvas/src/pdk.rs`: generated bindings, pruned to the Hooks each guest defines.
- `cartridge/`: the Cartridge's files (`manifest.json`, `templates/card.liquid`, `assets/`), plus `server.wasm` and
  `canvas.wasm`, which the build writes and are not committed.
