# chittr-cartridge

```text
-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

hiii, mocáli here. this is the repo for making cartridges, the little programs behind a chitt. the cli, the sdk, the
docs, the examples. poke around c;
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----
```

everything u need to build a cartridge for Chittr. rust is the main road, and it needs no node: the rust sdk, the abi
docs and schemas, a cli to make and check them, and runnable examples. the repo also vendors an offline xtp generator
that renders bindings and a project scaffold for cpp, csharp, go, python, rust, typescript or zig. that step runs on
node; building what it writes needs that language's own toolchain. the rust starter is still the one that arrives ready
to check and pack c;

## What a Cartridge is

a cartridge is the code and content behind a chitt. it has up to two parts, both compiled to WebAssembly:

- a **Server Guest** that runs on the server. it has **Hooks** that Chittr calls when something happens to its chitt: a
  view, a **Control** being used, a reply, a reaction, a timer firing. it can keep its own key-value store, schedule timers,
  act as the chitt's posting character through **Poster Actions**, and tell viewers that its card has changed with
  **Render Pushes**.
- an optional **Canvas Guest** that runs in the viewer's browser. it draws a canvas and handles its input, and talks to the
  Server Guest only through messages.

alongside the guests, a cartridge has a `manifest.json` (its name, fonts and Controls), a card template written in Liquid
(the `templates/` folder) that places the **Host Parts** the host owns, like the author header and the reply and reaction
bars, any `assets/` the guests load, and optionally a `style.css` for the card, with Tailwind classes that only reach ur
own card.

a Server Guest's `render` returns the data for the card, and the template draws it. `render` gets the **Observer** (the
viewer, or none when logged out), so each viewer can get their own card.

## Install

1. Install Rust with [rustup](https://rustup.rs), then add the WebAssembly target:

   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. Install the `chittr-cartridge` CLI. it creates, checks, packs and watches cartridges. grab a prebuilt zip (below), or
   build it from source:

   ```bash
   cargo install --git https://github.com/hecksadecimal/chittr-cartridge chittr-cartridge --locked
   ```

3. Optional: for bindings or a project scaffold in a language other than rust, use the vendored generator in
   [`tools/xtp/`](https://github.com/hecksadecimal/chittr-cartridge/tree/main/tools/xtp) (it lives in the repository; the
   release zip carries the CLI, docs and examples). it runs offline on Node and renders from any of the schemas, with no
   `xtp` install. the examples already carry their generated rust bindings, so u only need it to change a schema or scaffold
   another language. the rust path above still needs no Node.js; this step does. see
   [Scaffold another language](#scaffold-another-language) for the commands and each backend's toolchain.

no `xtp` install is needed for the rust starter or the examples.

### Install from a release zip

every release has one zip per platform, plus a `SHA256SUMS` file. the platforms are `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`. grab the zip for
yours, check it against the sums, unzip it, and put the binary on your `PATH`:

```bash
ZIP=chittr-cartridge-X.Y.Z-aarch64-apple-darwin.zip
BASE=https://github.com/hecksadecimal/chittr-cartridge/releases/download/vX.Y.Z
curl -LO "$BASE/$ZIP" -LO "$BASE/SHA256SUMS"
grep " $ZIP\$" SHA256SUMS | shasum -a 256 -c -      # on linux, use sha256sum -c -
unzip "$ZIP"
mv "${ZIP%.zip}/chittr-cartridge" ~/.local/bin/       # any folder on your PATH
chittr-cartridge --help
```

the zip unpacks into a folder of the same name. it holds the binary, the README, both licenses, the v1 docs and schemas,
the SDK, and the examples' sources, so the examples build from the unzipped folder as they are. on Windows the binary is `chittr-cartridge.exe`, and `Get-FileHash` checks it against
`SHA256SUMS`.

## Start a Cartridge

`chittr-cartridge new` is the rust starter: it writes the SDK crate, `src/pdk.rs`, `cartridge/manifest.json`, the card
template and `build.sh`, all wired up. pass `--canvas` for a Canvas Guest, or `--abi 2` for the draft v2 bindings.

```bash
chittr-cartridge new my-cartridge            # or: new my-cartridge --canvas, for a Canvas Guest too
cd my-cartridge && ./build.sh && chittr-cartridge check cartridge
```

the new project builds and checks with no edits. [`docs/cartridges/v1/quickstart.md`](docs/cartridges/v1/quickstart.md)
walks u thru it.

if u want a guest in another language, the next section is for u.

## Scaffold another language

the vendored generator in [`tools/xtp/`](https://github.com/hecksadecimal/chittr-cartridge/tree/main/tools/xtp) renders
the xtp bindgen templates from a schema. it is the offline, Node-only runner: the backend bundles and their dependencies
are committed under `tools/xtp/dist/`, so there is no `npm install` and no network at use time. run it from the cartridge
repo root:

```bash
node tools/xtp/bin/xtp-gen.mjs --schema docs/cartridges/v1/server.xtp.yaml --lang rust --out /tmp/server-scaffold --name myplugin
```

swap `--lang` for any of the seven backends:

```bash
node tools/xtp/bin/xtp-gen.mjs --schema docs/cartridges/v1/server.xtp.yaml --lang cpp        --out /tmp/server-cpp --name myplugin
node tools/xtp/bin/xtp-gen.mjs --schema docs/cartridges/v1/server.xtp.yaml --lang csharp     --out /tmp/server-csharp --name myplugin
node tools/xtp/bin/xtp-gen.mjs --schema docs/cartridges/v1/server.xtp.yaml --lang go         --out /tmp/server-go --name myplugin
node tools/xtp/bin/xtp-gen.mjs --schema docs/cartridges/v1/server.xtp.yaml --lang python     --out /tmp/server-python --name myplugin
node tools/xtp/bin/xtp-gen.mjs --schema docs/cartridges/v1/server.xtp.yaml --lang rust       --out /tmp/server-rust --name myplugin
node tools/xtp/bin/xtp-gen.mjs --schema docs/cartridges/v1/server.xtp.yaml --lang typescript --out /tmp/server-typescript --name myplugin
node tools/xtp/bin/xtp-gen.mjs --schema docs/cartridges/v1/server.xtp.yaml --lang zig        --out /tmp/server-zig --name myplugin
```

for a Canvas Guest, point `--schema` at `docs/cartridges/v1/canvas.xtp.yaml` and use a separate `--out`, like
`/tmp/canvas-scaffold`. v2 works the same way with `docs/cartridges/v2/server.xtp.yaml` or
`docs/cartridges/v2/canvas.xtp.yaml`. `--description`, `--app-id` and `--extension-point-id` are optional; they fill the
project fields each backend's templates use.

what u get is a project scaffold and bindings, not a packed Cartridge. `xtp-gen` writes the backend's source templates,
`xtp.toml` and generated binding files; it does not write `manifest.json`, a Liquid template, assets, or a `.cartridge`.
`chittr-cartridge new` is the only starter that writes those pieces. to use a generated scaffold, add the Cartridge files,
build the wasm, and run `chittr-cartridge check` / `pack` yourself.

for a rust Cartridge, the usual loop is `xtp-gen` and then `chittr-cartridge sync /tmp/server-scaffold .`; `sync` copies
only `src/pdk.rs`, pruned to the Hooks ur `src/lib.rs` defines. `scripts/gen-bindings.sh` runs the same render, format and
prune pass for the committed templates.

the catalog is the seven backends above. the repo's SDK, examples, CI and the CLI's `new` / `sync` path are rust; the
other scaffolds are generated from the schema but are not wired into a Chittr example. a Canvas Guest in another language
also needs a port of the command-buffer encoder (see `docs/cartridges/v1/design.md`), not just the host-function bindings.

each backend's template wants its own toolchain. the generator only needs Node; building the generated project needs the
rest:

| `--lang` | what the generated project expects |
| --- | --- |
| `cpp` | `make`, `curl` and a C++23 WASI SDK; the generated `Makefile` downloads a WASI SDK when `WASI_SDK_PATH` is unset, and fetches its C++ headers on first build. |
| `csharp` | .NET SDK 8 with the `wasi-experimental` workload, a WASI SDK in `WASI_SDK_PATH`, and PowerShell for `prepare.ps1`. |
| `go` | Go 1.20–1.24, TinyGo 0.34.0 or newer, and `goimports`; `prepare.sh` runs `go install` for `goimports`. |
| `python` | Python 3.13, `uv` and `extism-py`; `prepare.sh` downloads `extism.pyi`. |
| `rust` | Rust and rustup; `prepare.sh` adds `wasm32-wasip1` and `wasm32-unknown-unknown`. |
| `typescript` | Node.js, npm and `extism-js`; `prepare.sh` runs `npm install`. |
| `zig` | Zig; `zig build` fetches the pinned dependencies from `build.zig.zon`. |

the prepare and build scripts are the backend templates' own, so they may use the network for their dependencies. the
generator itself does not.

## Build the examples

```bash
cargo build                       # the SDK and its macros
cargo test --workspace --all-features
cargo test --manifest-path examples/poll/Cargo.toml   # the poll's Hooks, run natively
cargo test --manifest-path examples/hurtle/Cargo.toml # Hurtle's Hooks and suspension behavior
cargo test --manifest-path examples/breakout/Cargo.toml # Breakout's leaderboard logic
cargo test --manifest-path examples/breakout/canvas/Cargo.toml # Breakout's levels and power-ups
sh scripts/build-examples.sh      # every example's Server Guest and Canvas Guest, as wasm
```

the build script writes `server.wasm` and `canvas.wasm` into each example's `cartridge/` folder. if cargo's build
scripts fail to link on macOS, see [Troubleshooting](#troubleshooting).

the examples:

| Example | What it shows |
| --- | --- |
| `examples/hello` | The smallest Server Guest: `init` and `render`, no Canvas Guest. |
| `examples/doom` | The original Linux DOOM 1.10 engine, with keyboard and pointer-locked mouse input, software rendering, fullscreen, PCM sound effects and OPL music through ABI v2 `audio.pcm` (48 kHz stereo s16le; the host speaker starts muted). Viewers pick a base WAD and mods; Freedoom Phase 1/2 can be bundled as BSD-3 assets, and no proprietary game data ships. Optional WASI SDK build; an asset-free scaffold otherwise. |
| `examples/poll` | An admin-configured poll with a waiting state, transparent Canvas Guest answer buttons, a gate on replies, and a timer that announces the result. The reference for scaffolding, and for testing Hooks with `cargo test`. |
| `examples/snake` | A playable Snake game in a Canvas Guest, with a leaderboard in the Server Guest. |
| `examples/hurtle` | A Wordle-style Canvas game that requests a 15-minute suspension after six failed guesses, through the poster's permission-checked admin action. |
| `examples/paint` | Place: an r/place-style shared pixel board with a cooldown, stamped KV tiles, paced tile loading and fullscreen. |
| `examples/breakout` | A 32×32 square-brick Breakout Canvas Guest with power-ups, particles, fullscreen, a KV-backed level editor (any brick colour) for the poster, and daily and all-time top-five leaderboards. |

`examples/doom` is the one to look at for audio: the Canvas Guest decodes the selected WADs' 8-bit DMX `DS*` lumps,
mixes up to 16 SFX voices, adds the OPL music rendered from the `D_*` MUS lumps, and writes bounded 48 kHz stereo s16le
chunks through `audio_write` (`audio.pcm`). the host's shared speaker starts muted, and the host owns unmute, pause and
visibility. the cartridge requires `audio.pcm` and `input.relative-pointer`; WAD data keeps its own license.

## The CLI

```
chittr-cartridge new <name> [--canvas] [--abi 1|2] [--sdk-path DIR]   create a Rust Cartridge project
chittr-cartridge check <dir|file.cartridge>               validate with the server's rules
chittr-cartridge pack <dir> [out]                         pack and validate a .cartridge
chittr-cartridge watch <dir>                              repack and check on every save (no upload)
chittr-cartridge sync <scaffold-dir> <project>            copy a scaffold's bindings in, pruned to the Hooks you define
```

`check` prints `file: rule` for each rule the archive breaks, and exits 1. `pack` writes `<project>/<project>.cartridge` when
the directory is called `cartridge`, and refuses to write an archive that fails `check`. the validator follows the server's
rules, and the server's parser stays the authority at upload. the [conformance corpus](conformance/README.md) holds the cases
the validator is tested against.

## Test on Chittr

1. Open the composer on Chittr and upload the `.cartridge` file, with **Publish as preview** on. a preview is visible only to
   u and any Testers u add, and its cartridge runs for real.
2. Open the chitt, and use **Replace** in the **Cartridge…** panel to upload a new version. each replacement makes a new
   Cartridge Version, and earlier versions can be rolled back to.

to iterate, rebuild, repack and replace. or run `chittr-cartridge watch cartridge` and let it repack on every save. the
[cookbook](docs/cartridges/v1/cookbook.md#testing-on-chittr-previews-and-testers) has the whole workflow, with the logs to read when
something breaks.

## Troubleshooting

**cargo's build scripts fail to link on macOS.** some macOS setups point the build at the newest SDK, and the linker can't
read it. if you see `tapi error: malformed file`, point `SDKROOT` at an older SDK. list the ones you have, then pick one:

```bash
ls /Library/Developer/CommandLineTools/SDKs
```

the permanent fix is to set it once in `~/.cargo/config.toml`. cargo passes `[env]` to rustc, the build scripts and the
linker, so `cargo build` and `cargo install` work from any shell with no export:

```toml
[env]
SDKROOT = "/Library/Developer/CommandLineTools/SDKs/MacOSX<version>.sdk"
```

if u'd rather set it per shell, export it instead, and the build sees it the same way:

```bash
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX<version>.sdk
cargo build
```

don't use `xcrun --show-sdk-path` for this, cause it prints the default SDK, which can be the broken one. the build scripts
(`scripts/build-examples.sh` and a new project's `build.sh`) never pick an SDK themselves, so the setting has to come from
your environment or from cargo's config, and it reaches them both ways.

if the error shows up with no SDK pinned at all, the toolchain is the likely cause. run `rustup update`, and update the
Command Line Tools (`softwareupdate --list`, then install the one that matches your macOS). a newer toolchain often fixes it
for good, and then the pin can go.

## Releases

a release is cut by pushing a `vX.Y.Z` tag:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

the release workflow (`.github/workflows/release.yml`) builds the CLI in release mode for the five targets above, zips
each one, and creates a GitHub release for the tag with the zips and `SHA256SUMS` attached. the release notes are the
commit subjects since the previous tag. every push to `main` and every pull request runs the CI workflow
(`.github/workflows/ci.yml`): fmt, clippy, the tests (the SDK's, including the `testing` feature, and the CLI's, with the
conformance corpus), every example built to wasm, the poll's Hook tests, and `chittr-cartridge check` on each example.

## Documentation

- [`docs/cartridges/v2/abi.md`](docs/cartridges/v2/abi.md): the v2 contract, draft for review and not yet released: file pickers, scoped binary reads, native framebuffers, capabilities, `config.liquid`, relative pointer input and `audio.pcm`. the CLI can scaffold it with `--abi 2`.
- [`docs/cartridges/v2/design.md`](docs/cartridges/v2/design.md): why v2 is shaped the way it is.
- [`docs/cartridges/v2/proposal.md`](docs/cartridges/v2/proposal.md): the original v2 proposal and the reasoning behind it.
- [`docs/cartridges/v1/quickstart.md`](docs/cartridges/v1/quickstart.md): from an empty folder to a packed Cartridge.
- [`docs/cartridges/v1/guide.md`](docs/cartridges/v1/guide.md): the SDK, piece by piece, and how to test Hooks natively.
- [`docs/cartridges/v1/limits.md`](docs/cartridges/v1/limits.md): every limit an author runs into, with its number.
- [`docs/cartridges/v1/stylesheets.md`](docs/cartridges/v1/stylesheets.md): `style.css`, Tailwind classes, what `url()` can point at, and the v2 badge elements.
- [`docs/cartridges/v1/cookbook.md`](docs/cartridges/v1/cookbook.md): recipes for polls, counters, timers, canvas games and testing on Chittr.
- [`docs/cartridges/v1/abi.md`](docs/cartridges/v1/abi.md): the ABI, with every export, host function, draw op and limit.
- [`docs/cartridges/v1/design.md`](docs/cartridges/v1/design.md): why the ABI is shaped the way it is.
- [`docs/cartridges/v1/server.xtp.yaml`](docs/cartridges/v1/server.xtp.yaml) and
  [`docs/cartridges/v1/canvas.xtp.yaml`](docs/cartridges/v1/canvas.xtp.yaml): the v1 schemas the ABI is defined by. the v2
  schemas are [`docs/cartridges/v2/server.xtp.yaml`](docs/cartridges/v2/server.xtp.yaml) and
  [`docs/cartridges/v2/canvas.xtp.yaml`](docs/cartridges/v2/canvas.xtp.yaml).

## Layout

```
cli/                          the chittr-cartridge CLI, with its starter templates and tests
conformance/                  the conformance corpus the CLI's validator is tested against
sdk/chittr-cartridge          the guest SDK (Server Guests, Canvas Guests with the `canvas` feature, and the `testing` feature)
sdk/chittr-cartridge-macros   the #[hook] attribute
docs/cartridges/v1            the ABI, quickstart, guide, limits, cookbook, design notes and schemas
docs/cartridges/v2            the draft v2 ABI, design notes, proposal and schemas
examples/                     hello, poll, snake, hurtle, paint, breakout and doom
scripts/build-examples.sh     builds every example to wasm
scripts/package-release.sh    packages one release zip of the CLI
tools/xtp/                    vendored xtp backends and the offline Node generator
.github/workflows             the CI and release workflows
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
The DOOM port in `examples/doom` is GPL-2.0; its vendored Chocolate Doom music code is GPL-2.0, and the Nuked OPL3
emulator is LGPL-2.1. see its README and `engine/LICENSE.TXT`. Freedoom assets, when u pack them, are BSD-3-Clause. WAD
game data has separate licensing; no proprietary Doom data is included in the repository.
