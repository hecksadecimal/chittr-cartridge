# DOOM — first-episode cartridge port

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so we put the whole original linux doom engine inside a cartridge. c; pick ur own wad (we ship none, promise),
and the host scales the pixels for u. read the caveats tho, this is an early port and it knows it
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

The original Linux DOOM 1.10 engine, running locally inside a Chittr Canvas Guest: first episode, keyboard controls,
software rendering, fullscreen, and silent audio. **This is an early port**, not a full-featured source port: no audio,
network play, mouse aiming, touch controls, persistent saves, other episodes, or DOOM II yet.

The cartridge ships **no game data**. The viewer picks their own WAD with the card's file picker; the Canvas Guest reads
it locally in bounded chunks across frames (the `file` event only records the handle), validates it, and hands it to the
engine. Four 64 KiB reads per frame keep every guest call well under the 250 ms deadline and show a loading
percentage. The engine then runs at 35 simulation tics/second and its native 320×200 indexed framebuffer is presented
through a single `blitIndexedScaled` (ABI v2 op `0x53`) per frame, so the host scales and expands the palette instead of
the guest. If a host grants only
`framebuffer.rgba8`, the guest negotiates that path and sends the same 320×200 frame through `blitRgbaScaled` (0x52).
Reduced-motion scheduling draws less frequently without changing the simulation rate. Blur or hiding the canvas
releases held keys and pauses; click to resume.

The first milestone boots directly into **E1M1** at the difficulty configured by the Chitt's owner (medium by
default), only after choosing a WAD, clicking the canvas, and pressing Space. The owner/admin **Cartridge…** panel uses
ABI v2's `templates/config.liquid` and `render_config` surface to change that default and import a small settings JSON;
the viewer's WAD stays client-local and is never uploaded.

## Build and test

Rust needs the `wasm32-unknown-unknown` target. The engine needs WASI SDK 34's C compiler/static libc; this does
**not** add WASI imports to the finished guest, and it no longer needs a WAD at build time. A plain system Apple clang
cannot compile WebAssembly.

```bash
# From examples/doom:
./setup-wasi.sh                     # optional download: checksum-verified SDK into ignored .tools/, not a global install
./build.sh                          # builds the engine when the WASI SDK is available, else the scaffold
chittr-cartridge check cartridge
chittr-cartridge pack cartridge     # local test archive: doom.cartridge
```

`./build.sh` selects the engine whenever a WASI SDK is present (`WASI_SDK_PATH`, or an unpacked SDK under `.tools/`);
`--engine` requires it and `--scaffold` builds the Rust-only status screen. The WAD is not a build input. Because the
cartridge embeds no data, `canvas.wasm` stays well under the host's 4 MiB limit regardless of which WAD a viewer uses.
`scripts/build-examples.sh` uses this same selection. Upload the packed archive as a Chittr preview for real-host
testing.

The local WAD at `cartridge/assets/doom1.wad` is git-ignored and used only by the native and wasm integration tests
below. Override it with an absolute `DOOM_WAD` path. It is a complete PWAD with DOOM episode data; IWAD and complete
PWAD headers are both accepted, and the guest requires E1M1, TITLEPIC and a full 14-palette PLAYPAL. `pack cartridge`
still includes any files you leave under `cartridge/assets/`, including the local original: **do not commit or
redistribute game data without the appropriate rights**. Release-source packaging excludes WADs, built cartridges, and
toolchains.

### Tests

```bash
cargo test                                                    # Server Guest: card, config, Control/KV and import tests
cargo test --features engine
cargo test --manifest-path canvas/Cargo.toml                  # timing, input, palette, viewport and WAD validation
cargo test --manifest-path canvas/Cargo.toml --features engine # also boots and advances the actual C engine natively
ASAN=1 sh engine/tests/native-smoke.sh                         # C boundary, address sanitizer, framebuffer checks
```

The native engine test uses the local WAD and a native C compiler, and skips itself when no WAD is present; only wasm
engine builds require the WASI SDK. `sh engine/tests/native-smoke.sh --structural-only` compiles the engine and runs
malformed-container cases without any WAD; CI runs that subset under ASAN. On macOS, a mismatched default system SDK
may require setting `SDKROOT` to a compatible installed SDK, as described in the repository's troubleshooting notes.
Run the wasm integration smoke after `./build.sh` with Node and an existing installation of `@extism/extism`:

```bash
EXTISM_JS_MODULE=/absolute/path/to/node_modules/@extism/extism node tests/wasm-smoke.mjs
```

This uses the same JavaScript Extism interface as the browser host, with `useWasi: false`. It plays the host for the
`<lc-file>` picker and the `file_read`/`file_close` calls, decodes the `0x53` indexed frame, and checks deliberate
startup, incremental WAD loading, real framebuffer output/movement, frame budgets, blur/resume, bounded fullscreen
scaling, allowed imports, and memory below 64 MiB. It needs the local WAD. No Node dependencies are needed by the cartridge or its normal build.
`DOOM_SCREENSHOT=/tmp/doom.ppm` optionally writes the last tested framebuffer. Local timings are not a guarantee for
slower phones; cold startup and level loads still need real-device testing against the 250 ms guest-call deadline.

## Controls

| Key | Action |
| --- | --- |
| Arrows / WASD | Move forward/back and turn |
| Space (or J) | Fire |
| F (or Ctrl) | Use/open doors |
| Shift | Run |
| Q / E, or Alt + left/right | Strafe |
| 1–7 | Select weapon |
| Tab | Automap |
| M | Open/close DOOM's menu (Enter selects) |
| P | DOOM's pause key |
| Escape | Release canvas focus (reserved by the Chittr host) |

Use **F** rather than Ctrl with WASD: Ctrl+W/Ctrl+S can activate browser shortcuts on some platforms.
Space still starts the game after the initial click; during gameplay it fires.
The host's expand button enters fullscreen. Rendering presents the engine's native 320×200 indexed frame with
nearest-neighbour pixels, scaled to fit the available backing store while preserving the 320×200 aspect ratio and
letterboxing. Fractional scaling is supported; there is no 3× cap. On an indexed8 host a frame is one
`blitIndexedScaled` op carrying 64,000 index bytes plus the current 256-entry palette, so palette changes and flashes are
reflected and a full frame is about 65 KiB. A host that grants only `framebuffer.rgba8` gets the same frame expanded to
RGBA8 through `blitRgbaScaled` (about 256 KiB). Both are far below the 4 MiB command buffer. Quitting through DOOM's
menu stops this instance; reload the card to restart. Engine state is local to each viewer and not persisted; only the
owner-configured default difficulty is stored per Chitt.

## Configuration

The card has no privileged buttons. The owner or an administrator opens the host's **Cartridge…** panel, where the
Cartridge renders `templates/config.liquid` through `render_config`. That panel offers:

- the five original DOOM difficulties as an owner-or-admin-only `set-skill` Control, with a host-rendered confirmation;
- a `reset-settings` Control, narrowed by the guest to the owner, that restores medium difficulty;
- a `target="server"` settings import for a JSON object such as `{"skill":"nightmare"}`.

An owner or an administrator can set the difficulty; only the owner can reset it. The server stores the default in the
Chitt's KV under `doom:settings` and pushes a fresh render after a change. The Canvas Guest reads the default from
`start`'s `state` and passes it to `doom_boot`, so it applies to new games. An
existing engine instance keeps the difficulty it booted with; reload the card to apply a new default. The settings
import is config-scoped (`files.server`), bounded to 4 KiB, and validated before it is stored. It never accepts or
redistributes a WAD.

The Canvas Guest also reads ABI v2 `start.capabilities` and `start.limits` instead of hard-coding the host's file and
framebuffer caps. It prefers indexed8, falls back to rgba8, and refuses to start with a clear message if neither
framebuffer path or the client-file capability is usable.

## Implementation

- `engine/vendor/`: source copied from `/Users/hex/Documents/Source/DOOM/linuxdoom-1.10`, revision
  `a77dfb96cb91780ca334d0d4cfd86957558007e0`. The external checkout is untouched and no longer required to build.
  See `engine/README.md` for provenance and the platform/portability changes.
- `engine/doom.h`, `platform.c`: one-instance C boundary, in-memory WAD, fixed-size indexed framebuffer, current palette,
  the owner-selected `skill_t` on boot, original DOOM key events and silent single-player platform hooks. No infinite
  `D_DoomLoop`, wall-clock waits, X11, sound server, filesystem or sockets.
- `canvas/src/engine.rs`: takes the runtime WAD bytes and owns them for as long as C retains the pointer.
- `canvas/src/wad.rs`: validates a viewer-supplied IWAD/PWAD (bounded directory, in-bounds lumps, E1M1, TITLEPIC and a
  full 14-palette PLAYPAL) before boot, so a malformed selection is reported instead of trapping the guest.
- `canvas/src/lib.rs`: deliberate startup, capability and limit discovery, the owner's difficulty from `state`, capped
  35 Hz catch-up (at most three tics/call), focus/visibility handling, key mapping, incremental bounded `file_read`
  loading across frames, and one fit-to-canvas v2 blit per frame: indexed `0x53`, or the RGBA `0x52` fallback.
- `canvas/src/allocator.rs`, `canvas/src/wasm_libc.c`: C and Rust share Rust's allocator through function pointers (no extra
  wasm exports). A build-local copy of static libc omits WASI syscall trampolines; diagnostic writes are discarded.
- `canvas/build.rs`: compiles the vendored engine; it does not read, derive or embed a WAD.
- `src/lib.rs`: publication text, public card data, the private `render_config`/Control surface, per-Chitt settings in
  KV, and the config-scoped settings import. It runs no game simulation and sends no WAD messages.
- `cartridge/templates/config.liquid`: the owner/admin panel. Private Controls and the server settings picker are
  declared in `manifest.json` with `audience: "owner_or_admin"`.
- `src/pdk.rs`, `canvas/src/pdk.rs`: generated ABI v2 bindings (Server Guest exports `init`/`render`/`on_control`/
  `on_file`/`render_config`; Canvas Guest exports `start`/`frame`/`event`), pruned to the exports each guest defines.

Fatal C errors currently trap on wasm (reported by the host); native tests can unwind to the C boundary. The engine
boots once per module; restart/save/resume persistence and incremental loading are follow-up work. The WAD parser
checks container bounds, but arbitrary untrusted mod content is not a supported input to this early port.

## License

The DOOM port is **GPL-2.0**, separate from the repository's general MIT/Apache licensing. `engine/LICENSE.TXT` and
`cartridge/assets/DOOM-LICENSE.txt` carry the GPL text; upstream notices are preserved. The dual-licensed Cartridge SDK
is used under its MIT option. Preserve notices and provide corresponding source/build instructions when distributing
engine binaries. See the WASI SDK distribution for its static-libc/compiler runtime licenses and sources as well.
**WAD game data is separately licensed**; a source-code license does not grant redistribution rights to commercial
data, and this cartridge neither embeds nor redistributes any WAD.

This example started with `chittr-cartridge new doom --canvas --abi 2 --sdk-path ../sdk/chittr-cartridge` from `examples/`;
SDK paths were then made repository-relative. See the [ABI](../../docs/cartridges/v2/abi.md) and
[design](../../docs/cartridges/v2/design.md) for the v2 cartridge host contract.
