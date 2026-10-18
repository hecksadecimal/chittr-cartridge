# DOOM engine cartridge

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so we put the whole original linux doom engine inside a cartridge. c; pick ur own base and mod wads (freedoom ships
with it, promise), and the host scales the pixels for u. read the caveats tho, this is an early port and it knows it
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

The original Linux DOOM 1.10 engine, running locally inside a Chittr Canvas Guest. It boots Doom I or Doom II game
data, including Freedoom Phase 1 and Phase 2, with keyboard and pointer-locked mouse controls, software rendering,
fullscreen, the original sound effects, and OPL music. **This is an early port**, not a full-featured source port: no
network play, touch controls, or persistent saves. WAD-lump mods work; DEHACKED and advanced source-port features are
not implemented. Music and SFX come from the selected WADs.

The cartridge carries **no proprietary Doom data**. It bundles Freedoom Phase 1 and Phase 2 (BSD-3) so the card can
offer one-click base WADs, with their license and credits beside them. Viewers can still choose their own base WAD and
up to seven mod PWADs with the card's file pickers. The Canvas Guest reads a selection locally in bounded chunks across
frames (the `file` event only records the handle), validates each file, and hands the set to the engine. Later WADs
override earlier lumps, matching DOOM's `-file` order. Four 64 KiB reads per frame keep every guest call well under the
250 ms deadline and show a loading percentage. The engine then runs at 35 simulation tics/second and its native 320×200 indexed framebuffer is presented
through a single `blitIndexedScaled` (ABI v2 op `0x53`) per frame, so the host scales and expands the palette instead of
the guest. If a host grants only `framebuffer.rgba8`, the guest negotiates that path and sends the same 320×200 frame
through `blitRgbaScaled` (0x52). Music is rendered from the selected WADs' `D_*` MUS lumps through the vendored Chocolate Doom
OPL3 player; sound effects are decoded from `DS*` lumps and mixed into the same `audio.pcm` stream.
Reduced-motion scheduling draws less frequently without changing the simulation rate. Blur or hiding the canvas
releases held keys and pauses; click to resume.

The engine detects the base WAD's game mode: `MAP01` selects Doom II/commercial rules, `E4M1` plus `M_EPI4`
selects retail (Ultimate Doom), and `E1M1` plus `E2M1` or `E3M1` selects registered Doom. It boots directly into the
first map (`E1M1` or `MAP01`) at the difficulty configured by the Chitt's owner (medium by default), only after
choosing a base WAD, clicking the canvas, and pressing Space.
Mods must be added before starting; the original engine boots once per Canvas Guest. The owner/admin **Cartridge…**
panel uses ABI v2's `templates/config.liquid` and `render_config` surface to change the default skill and import a small
settings JSON; the viewer's WADs stay client-local and are never uploaded.

## Build and test

Rust needs the `wasm32-unknown-unknown` target. The engine needs WASI SDK 34's C compiler/static libc; this does
**not** add WASI imports to the finished guest, and it no longer needs a WAD at build time. A plain system Apple clang
cannot compile WebAssembly.

```bash
# From examples/doom:
./setup-wasi.sh                     # optional download: checksum-verified SDK into ignored .tools/, not a global install
./build.sh                          # builds the engine when the WASI SDK is available, else the scaffold
# Move the local test-only WAD aside so it is not packaged with the Freedoom assets.
mv cartridge/assets/doom1.wad /tmp/doom1.wad
chittr-cartridge check cartridge
chittr-cartridge pack cartridge     # local archive with Freedoom: doom.cartridge
mv /tmp/doom1.wad cartridge/assets/doom1.wad
```

`./build.sh` selects the engine whenever a WASI SDK is present (`WASI_SDK_PATH`, or an unpacked SDK under `.tools/`);
`--engine` requires it and `--scaffold` builds the Rust-only status screen. WADs are not a build input. The Freedoom
IWADs are Cartridge assets, not wasm: `canvas.wasm` stays well under the host's 4 MiB limit regardless of which WAD a
viewer uses. `scripts/build-examples.sh` uses this same selection. Upload the packed archive as a Chittr preview for
real-host testing.

The local WAD at `cartridge/assets/doom1.wad` is git-ignored and used only by the native and wasm integration tests
below. Override it with an absolute `DOOM_WAD` path. It is a complete PWAD with DOOM episode data; IWAD and complete
PWAD headers are both accepted. A base WAD needs TITLEPIC, a full 14-palette PLAYPAL and either E1M1 or MAP01; mod
PWADs only need a valid bounded directory. Freedoom Phase 1/2 are 24–29 MiB, so ABI v2's file and asset caps are
raised to 32 MiB and the canvas aggregate to 40 MiB for this build; the Canvas Guest still stays under its 64 MiB
memory cap. `pack cartridge` includes any files you leave under `cartridge/assets/`, so it bundles the Freedoom IWADs
and licenses but the local `doom1.wad` must be moved aside first: **do not commit or redistribute proprietary game
data**. All `.wad` files are git-ignored; the built cartridge carries the Freedoom IWADs. Release-source trees exclude
WADs, built cartridges, and toolchains; the built cartridge includes only the bundled Freedoom IWADs.

### Tests

```bash
cargo test                                                    # Server Guest: card, config, Control/KV and import tests
cargo test --features engine
cargo test --manifest-path canvas/Cargo.toml                  # timing, input, audio, palette, viewport and WAD validation
cargo test --manifest-path canvas/Cargo.toml --features engine # also boots/advances the C engine and decodes audio
ASAN=1 sh engine/tests/native-smoke.sh                         # C boundary, mouse, OPL music, framebuffer checks
```

The native engine test uses the local WAD and a native C compiler, and skips itself when no WAD is present; only wasm
engine builds require the WASI SDK. `sh engine/tests/native-smoke.sh --structural-only` compiles the engine and runs
malformed-container cases without any WAD; CI runs that subset under ASAN. On macOS, a mismatched default system SDK
may require setting `SDKROOT` to a compatible installed SDK, as described in the repository's troubleshooting notes.
Run the wasm integration smoke after `./build.sh` with Node and an existing installation of `@extism/extism`:

```bash
EXTISM_JS_MODULE=/absolute/path/to/node_modules/@extism/extism node tests/wasm-smoke.mjs
EXTISM_JS_MODULE=/absolute/path/to/node_modules/@extism/extism DOOM_WAD=/path/to/freedoom2.wad DOOM_MOD_WAD=/path/to/tiny-mod.wad node tests/wasm-smoke.mjs
```

This uses the same JavaScript Extism interface as the browser host, with `useWasi: false`. It plays the host for the
`<lc-file>` picker and the `file_read`/`file_close` calls, decodes the `0x53` indexed frame, and checks deliberate
startup, incremental WAD loading, optional mod loading/override, real framebuffer output/movement, pointer-lock
movement, OPL music/PCM, frame budgets, blur/resume, bounded fullscreen scaling, allowed imports, and memory below
64 MiB. It needs the local WAD. No Node dependencies are needed by the cartridge or its normal build.
`DOOM_SCREENSHOT=/tmp/doom.ppm` optionally writes the last tested framebuffer. Local timings are not a guarantee for
slower phones; cold startup and level loads still need real-device testing against the 250 ms guest-call deadline.

## Controls

| Key | Action |
| --- | --- |
| Up / Down arrows, W / S | Move forward/back |
| Left / Right arrows | Turn left/right |
| Space (or J) | Fire |
| F (or Ctrl) | Use/open doors |
| Shift | Run |
| A / D | Strafe left/right |
| Q / E | Turn left/right |
| Alt + left/right | Strafe left/right |
| 1–8 | Select weapon |
| Tab | Automap |
| M | Open/close DOOM's menu (Enter selects) |
| P | DOOM's pause key |
| Mouse click | Enter the host-owned pointer lock on the focused canvas |
| Mouse move | Turn while pointer-locked; vertical movement is ignored |
| Left / Right / Middle mouse | Fire / Strafe / Move forward |
| Escape | Release pointer lock and canvas focus (reserved by the Chittr host) |

Mouse deltas are scaled like the original X11 driver. Mouse Y is deliberately ignored in game so moving the mouse
forward/back cannot walk the player; the original menus still use it. The browser's forward/back side buttons are
ignored too. Use DOOM's **Options → Mouse Sensitivity** to tune the default further. Use **F** rather than Ctrl with
W/S: Ctrl+W/Ctrl+S can activate browser shortcuts on some platforms.
Space still starts the game after the initial click; during gameplay it fires.
The host's expand button enters fullscreen. Rendering presents the engine's native 320×200 indexed frame with
nearest-neighbour pixels, scaled to fit the available backing store while preserving the 320×200 aspect ratio and
letterboxing. Fractional scaling is supported; there is no 3× cap. On an indexed8 host a frame is one
`blitIndexedScaled` op carrying 64,000 index bytes plus the current 256-entry palette, so palette changes and flashes are
reflected and a full frame is about 65 KiB. A host that grants only `framebuffer.rgba8` gets the same frame expanded to
RGBA8 through `blitRgbaScaled` (about 256 KiB). Both are far below the 4 MiB command buffer.

Music and SFX use the ABI v2 `audio.pcm` capability: the guest writes bounded interleaved s16le stereo chunks through
the host's shared speaker. The speaker starts muted; the host's own speaker button (offered because the canvas declares
`audio.pcm`) unmutes it on a real click and unlocks browser audio. The host owns mute, pause, visibility and hardware
audio; paused or hidden canvases drop chunks. DOOM's menu volume controls still work. This version **requires**
`audio.pcm`: a host that cannot grant it refuses the cartridge instead of silently running a keyboard-only build.
Likewise, `input.relative-pointer` is required so the pointer-lock controls are not a silent fallback.

Quitting through DOOM's menu stops this instance; reload the card to restart. Engine state is local to each viewer and
not persisted; only the owner-configured default difficulty is stored per Chitt. WADs and mods are chosen before
starting because the original engine boots once per Canvas Guest; reload to change them.

## Configuration

The card has no privileged buttons; the Freedoom buttons are host-rendered client picks, not Controls. The owner or
an administrator opens the host's **Cartridge…** panel, where the Cartridge renders `templates/config.liquid` through
`render_config`. That panel offers:

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
framebuffer caps. It requires `files.client`, `input.relative-pointer` and `audio.pcm`, prefers indexed8, and falls
back to rgba8. If the loaded host somehow still omits audio or pointer lock, the canvas prints which capability is
missing instead of silently pretending they work.

## Implementation

- `engine/vendor/`: source copied from `/Users/hex/Documents/Source/DOOM/linuxdoom-1.10`, revision
  `a77dfb96cb91780ca334d0d4cfd86957558007e0`. The external checkout is untouched and no longer required to build.
  See `engine/README.md` for provenance and the platform/portability changes.
- `engine/vendor/chocolate/`: the music subsystem from Chocolate Doom (GPL-2.0), at the revision recorded in
  `engine/vendor/chocolate/PROVENANCE.md`: MUS conversion, MIDI parsing, memory streams and the Nuked OPL3 emulator.
  `engine/opl_compat.c` replaces Chocolate's SDL timer/audio backend with an offline callback clock and a
  `doom_music_mix` function that adds OPL samples to the Canvas PCM buffer.
- `engine/doom.h`, `platform.c`: one-instance C boundary, an in-memory WAD set (base plus mods), fixed-size indexed
  framebuffer, current palette, the owner-selected `skill_t` on boot, original DOOM key events, relative mouse input,
  and a bounded high-level sound event queue. No infinite `D_DoomLoop`, wall-clock waits, X11, sound server, filesystem
  or sockets.
- `canvas/src/engine.rs`: takes the runtime base and mod WAD bytes and owns them for as long as C retains the pointers.
- `canvas/src/wad.rs`: validates a viewer-supplied base IWAD/PWAD (bounded directory, in-bounds lumps, TITLEPIC, a full
  14-palette PLAYPAL and E1M1 or MAP01) and appended mod PWADs before boot, so a malformed selection is reported
  instead of trapping the guest.
- `canvas/src/lib.rs`: deliberate startup, capability and limit discovery, the owner's difficulty from `state`, capped
  35 Hz catch-up (at most three tics/call), focus/visibility/pointer-lock handling, key and relative-mouse mapping,
  incremental bounded `file_read` loading for the base and each mod, PCM audio, and one fit-to-canvas v2 blit per frame:
  indexed `0x53`, or the RGBA `0x52` fallback.
- `canvas/src/audio.rs`: scans the base and mod WADs for 8-bit DMX `DS*` lumps (later WADs override), mixes up to 16
  SFX voices at 48,000 Hz stereo, adds the OPL music mix, and feeds bounded chunks to `audio_write`.
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
boots once per module; restart/save/resume persistence is follow-up work. The WAD parser checks container bounds, but
arbitrary untrusted mod content is not a supported input to this early port.

## License

The DOOM port is **GPL-2.0**, separate from the repository's general MIT/Apache licensing. `engine/LICENSE.TXT` and
`cartridge/assets/DOOM-LICENSE.txt` carry the GPL text; upstream notices are preserved. The vendored Chocolate Doom music
code is GPL-2.0; the Nuked OPL3 emulator is LGPL-2.1. Their notices and provenance are under
`engine/vendor/chocolate/`. The bundled Freedoom WADs are **BSD-3-Clause**; their copyright, license and contributor
credits are in `cartridge/assets/FREEDOOM-COPYING.adoc` and `FREEDOOM-CREDITS.txt`. The dual-licensed Cartridge SDK is
used under its MIT option. Preserve notices and provide corresponding source/build instructions when distributing
engine binaries. See the WASI SDK distribution for its
static-libc/compiler runtime licenses and sources as well.
**WAD game data is separately licensed**; a source-code license does not grant redistribution rights to commercial
data. This cartridge redistributes only Freedoom under its BSD-3 terms. Do not add proprietary Doom WADs to a build.

This example started with `chittr-cartridge new doom --canvas --abi 2 --sdk-path ../sdk/chittr-cartridge` from `examples/`;
SDK paths were then made repository-relative. See the [ABI](../../docs/cartridges/v2/abi.md) and
[design](../../docs/cartridges/v2/design.md) for the v2 cartridge host contract.
