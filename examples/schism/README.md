# Schism Tracker cartridge

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so we put the whole of schism tracker in a cartridge. c; load ur own modules, edit patterns, play em, and
dont tell anyone about the tracker u wrote in 1997. no uploads, no saves, no server, just the og 640x400 interface
and a browser speaker. lemme know how it feels
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

this is the original Schism Tracker user interface, pattern/sample/instrument editors, order list, module loaders,
playback engine and mixer, compiled into a Chittr Canvas Guest. it's not a reimplementation: the vendored C engine
owns the screen, keyboard handling, song state and audio. the viewer picks a tracker module with the card's local file
picker, the guest reads it in bounded chunks, and the module loads through Schism's normal format detection. the card
starts on the empty pattern editor, so u can use it before u pick a module.

everything runs locally in the Canvas Guest. the Server Guest only supplies publication text and the card's data; it
runs no tracker code, stores no state, and has no Controls or owner configuration panel. modules are never uploaded,
and edits only live in the guest's memory. reloading the card starts a fresh empty song.

## Build and test

Rust needs the `wasm32-unknown-unknown` target. the engine needs WASI SDK 34's C compiler and static libc. the finished
guest has **no WASI imports**; the SDK is a build tool, not a runtime dependency. a plain system Apple clang can't
compile WebAssembly, ive been told many such things but it really can't.

```bash
# From examples/schism:
./setup-wasi.sh                     # optional download: checksum-verified SDK into ignored .tools/, not a global install
./build.sh                          # builds the engine when the WASI SDK is available, else the scaffold
chittr-cartridge check cartridge
chittr-cartridge pack cartridge     # writes schism.cartridge
```

`./build.sh` picks the engine whenever a WASI SDK is around (`WASI_SDK_PATH`, or an unpacked SDK under `.tools/`).
`--engine` requires it, and `--scaffold` builds the Rust-only status screen (no tracker engine or module loading). the
playable engine makes a `canvas.wasm` of about 1.1 MiB, well under the host's 4 MiB limit. `scripts/build-examples.sh`
runs this same build script, so it also picks the engine when the SDK is there. upload the packed archive as a Chittr
preview to test it on the real host.

### Tests

```bash
cargo test                                                       # Server Guest: publication and card data
cargo test --manifest-path canvas/Cargo.toml                     # input mapping, presentation, capability checks
cargo test --manifest-path canvas/Cargo.toml --features engine   # also boots the C engine and presents frames
sh engine/tests/native-smoke.sh                                  # host C compiler: boot, load a generated MOD, play, check PCM
EXTISM_JS_MODULE=/absolute/path/to/node_modules/@extism/extism node tests/wasm-smoke.mjs
```

the native smoke test compiles the same source list for the host, runs the engine for 120 frames, loads a generated
2,236-byte ProTracker module through the in-memory API, presses F5 and checks for non-silent PCM. it needs a host C
compiler and libm, not the WASI SDK. on macOS it tries the default system SDK and the installed 15.4/26.5 SDKs; set
`SDKROOT` if a different one matches ur linker.

the wasm smoke test uses the same JavaScript Extism interface as the browser host, with `useWasi: false`. it plays the
host for the `<lc-file>` picker and `file_read`/`file_close`, decodes the indexed `0x53` frame, loads and plays the
generated module, checks that F10 opens and closes the menu, and checks audio, blur/resume, resize, memory and the
module's import/export surface. then it repeats the presentation check on the RGBA `0x52` fallback. `@extism/extism`
isn't a cartridge dependency; point `EXTISM_JS_MODULE` at an install u already have, or install it locally for Node.

## Controls

Schism's original keyboard layout applies. the host forwards DOM `code` and `key`, the guest tracks modifier and lock
state, and printable keys use their Unicode keysym. click the canvas first so it gets keys.

| Key | Action |
| --- | --- |
| F5 / F6 / F7 | Play song / pattern / row |
| F8 | Stop playback |
| F2 / F3 / F4 | Pattern editor / sample editor / instrument editor |
| F1 | Help |
| F11 / F12 | Order list and panning / song variables |
| F10 | Main menu; press it again to close or back out (the host reserves Escape) |
| Shift + F5 | Preferences |
| Ctrl + F1 | System configuration |
| Ctrl + F12 | Palette editor |
| Shift + F12 | Font editor |
| Space | Insert the last-used note/instrument/volume/effect |
| 1 | Note cut |
| ` | Note off / panning toggle |
| . | Clear the current field |
| Ctrl + Backspace | Undo history (restore an earlier pattern state) |
| Ctrl + P | Show the song length |
| Arrows | Move the cursor |
| PgUp / PgDn | Move a page at a time |
| Tab / Shift + Tab | Next / previous channel |
| Enter | Open a selected item or confirm |
| Mouse | Move over controls; left click activates (Schism draws its own cursor) |

pattern editing has no separate record toggle: with the pattern editor focused, keys write to the current row
directly, and Space recalls the last-used values. the mask and block commands follow the upstream help page (F1).

the host may reserve browser shortcuts, so a few combos can differ by browser and OS. F5 is also the browser's reload
key; the canvas handles it while focused, but use F8 to stop playback if reloading is a worry. **Escape belongs to the
host and is never delivered to the guest**, so unmodified F10 is translated to Escape: it opens Schism's main menu,
closes dialogs, and backs out of screens. Shift+F10 and Ctrl+F10 are left alone by that translation. the tracker's
internal clipboard still works for copy/paste; there's no system clipboard bridge.

load modules with the card's picker, not F9: the engine's own open dialog needs a filesystem this build doesn't have.
same reason Ctrl+S opens the save dialog but can't write a file, and Shift+F10's export can't write either. F10 is the
menu here, not Schism's Save As shortcut.

## Modules

the picker takes the song formats compiled into this build (up to 16 MiB, further capped by the host's
`maxFileBytes`):

| Extensions | Formats |
| --- | --- |
| `.it` | Impulse Tracker |
| `.s3m` | Scream Tracker 3 |
| `.mod` | ProTracker, 15- and 31-sample variants |
| `.xm` | FastTracker II |
| `.669` | Composer 669 / Unis 669 |
| `.mtm` | MultiTracker |
| `.stm` | Scream Tracker 2 |
| `.far` | Farandole Composer |
| `.okt` | Oktalyzer |
| `.ult` | UltraTracker |
| `.psm` | Epic MegaGames MASI / PSM |
| `.dsm` | DSIK internal format |
| `.imf` | Imago Orpheus |
| `.mdl` | DigiTracker |
| `.mus` | Doom MUS |
| `.mid` | Standard MIDI |
| `.d00` | Delusion Digital Music |
| `.sfx` | Sound FX / MultiMedia Sound |
| `.stx` | Scream Tracker Music Interface Kit |
| `.ams`, `.f2r`, `.liq`, `.med`, `.mf`, `.mt2`, `.ntk` | Recognized only; this build has no loader for these |

the host caps an `lc-file` accept list at 16 entries, so the picker's filter leaves out the three least common loaders
(`.d00`, `.sfx`, `.stx`) even tho the engine can load them. the picker's 16 formats reach every other compiled loader.

picking another file replaces the current song right away. a file is read in up-to-64 KiB chunks, four chunks per
host frame, so even a 16 MiB file takes only about a second at the full read limit and each guest call stays inside
the 250 ms deadline. the card shows the load percentage, and the handle is closed as soon as the read finishes or
fails. a malformed or unsupported file stops playback and swaps the canvas for the loader error; the previous song
stays loaded in memory, and picking another file from the card clears the error and tries again.

modules go as-is thru Schism's original parsers. they're trusted tracker files, not a hardened service for w/e hostile
input someone throws at it. the cartridge bundles no music and redistributes no module data.

## Presentation and audio

the engine renders its native 640×400 indexed framebuffer. if the host grants `framebuffer.indexed8` with 256 palette
entries, each frame is one `blitIndexedScaled` (`0x53`) with the indices plus the current palette, so Schism's palette
changes and fades are exact and a full frame is about 251 KiB. a host that grants only `framebuffer.rgba8` gets the
same frame expanded through `blitRgbaScaled` (`0x52`, about 1 MiB). the host scales and letterboxes it to the canvas;
the guest doesn't resize the engine or smooth the pixels.

audio uses the `audio.pcm` capability at the rate the engine negotiated, normally 48 kHz stereo s16le. one frame
sends at most one bounded chunk (up to `maxPcmChunkBytes`) and stops sending while the host reports more than 800 ms
queued. the speaker starts muted; use the host's own speaker button to unmute and unlock browser audio. blur or hiding
the canvas releases held keys, stops playback, clears queued PCM and pauses the engine; click the canvas to resume.
the host owns mute, pause, visibility and hardware audio, and drops chunks while paused or hidden.

the guest requires `files.client`, `audio.pcm` and either indexed8 or rgba8. a host that can't grant them gets a drawn
explanation instead of a silent keyboard-only build. the manifest declares the required and optional capabilities,
and the guest reads the actual framebuffer, file and PCM limits from `start` instead of guessing.

## What is not implemented

- **No filesystem or saving.** Schism's config load/save, recent-file list, file dialogs and disk export/import go
  through path-based WASI calls that this build rejects. the song lives only in memory; reloading the card loses it.
  there's no "save as" or disk export.
- **No system clipboard**, no drag and drop, and no MIDI input or output. Schism's internal clipboard buffer works.
- **Quit is unavailable.** Ctrl+Q, Ctrl+Shift+Q and the File menu's Quit show a message instead of terminating the
  guest; reload the card to restart.
- **One engine per guest instance.** the vendored C engine keeps process-wide state and can't be reset. load a
  different module whenever u like, but reload the card for a fresh engine.
- **No mouse wheel forwarding.** the backend has a wheel entry point, but the host canvas doesn't send wheel events
  yet.
- **No network, no touch controls, no persistence, and no owner settings.**

fatal C errors trap on wasm and the host reports them. the native tests can unwind to the C boundary instead.

## Implementation

- `engine/vendor/`: a subset of [schismtracker/schismtracker](https://github.com/schismtracker/schismtracker) at
  commit `a982a37ff37518909ea364af648ce15fd710c4d7` (2026-09-26). it has `schism/`, `player/`, `fmt/`, `sys/`, the
  needed headers, fonts, help text, and generated `auto/default-font.c` and `auto/helptext.c`, so the build needs
  neither Perl nor autotools. see `engine/README.md` for provenance and the full patch inventory.
- `engine/platform/wasm.c`: the browser backends (video, events, timer, audio, clipboard, dmoz) and the exported C
  API. it implements the SDL2-named symbols without SDL headers.
- `engine/platform/wasm_libc.c`: the allocator bridge shared with Rust, plus freestanding WASI syscall stubs that
  reject filesystem and clock paths.
- `engine/compat/utf8proc.c`: a minimal ASCII case-folding/pass-through shim for the engine's UTF-8 calls.
- `engine/build-config.h`, `engine/sources.txt`: hand-maintained configure results and the authoritative C source
  list.
- `canvas/build.rs`: compiles the vendored C for wasm32-wasip1 or the host, copies the static libc without WASI
  syscall trampolines, and links with a 64 MiB cap.
- `canvas/src/engine.rs`: owns the one C engine instance, module loading, keys, mouse, focus, frame ticking and audio
  reads.
- `canvas/src/audio.rs`: drains the engine's PCM ring into bounded `audio.pcm` chunks and tracks the host's queue
  depth.
- `canvas/src/allocator.rs`: C and Rust share Rust's allocator through function pointers, with no extra wasm exports.
- `canvas/src/lib.rs`: capability/limit discovery, bounded `file_read` loading, host key and pointer mapping,
  pause/blur handling, indexed or RGBA presentation, and drawn error screens.
- `src/lib.rs`: publication text and card data. it runs no tracker simulation and stores nothing.
- `src/pdk.rs`, `canvas/src/pdk.rs`: small v2 bindings pruned to the exports each guest defines (Server: `init`,
  `render`; Canvas: `start`, `frame`, `event`).
- `tests/wasm-smoke.mjs`, `engine/tests/native_smoke.c`: the browser-level and C-boundary tests.

## License

the Schism Tracker port is **GPL-2.0**, separate from the repository's general MIT/Apache licensing. the upstream
engine is GPL-2.0; `engine/LICENSE` is upstream's unmodified `COPYING`, and `cartridge/assets/SCHISM-LICENSE.txt`
carries the same license inside the packed Cartridge so anyone who gets the wasm binary gets it too. original per-file
copyright, license and warranty notices are kept. the vendored `auto/` sources are generated from upstream's scripts
and carry no content changes.

the dual-licensed Cartridge SDK is used under its MIT option. module files aren't part of this repository, aren't
covered by the engine license, and aren't redistributed by the cartridge. if u distribute a built engine binary, keep
the notices and make the corresponding source and build instructions available; this directory and `engine/README.md`
are that source. see the WASI SDK distribution for its static-libc/compiler runtime licenses and sources too.

this example started from the v2 Cartridge template and follows the structure of the DOOM engine example. see the
[ABI](../../docs/cartridges/v2/abi.md) and [design](../../docs/cartridges/v2/design.md) for the v2 host contract.
