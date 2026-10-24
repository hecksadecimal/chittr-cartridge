# Original Schism Tracker engine: in-memory browser boundary

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

this is the engine room. where the C lives, what we patched, and the license stuff. if ur poking at the tracker
itself, start here. if u just wanna make noise, the example's README is the friendlier read <3
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

the real Schism Tracker user interface, pattern/sample/instrument editors,
module loaders, playback engine and mixer, compiled for `wasm32-wasip1`. no
rewritten tracker here: the vendored C engine owns the screen, keyboard
handling and song state. the browser layer supplies a 640×400 indexed
framebuffer, a host-clocked timer, input events, an in-memory module picker and
a PCM ring buffer.

## Provenance and license

`vendor/` is a subset of
[schismtracker/schismtracker](https://github.com/schismtracker/schismtracker) at
commit `a982a37ff37518909ea364af648ce15fd710c4d7` (2026-09-26, “page_waterfall:
zero out remaining incoming bytes after copy”). The upstream checkout used for
this import is unmodified apart from the generated `auto/` sources described
below.

`LICENSE` is an unmodified copy of upstream's root `COPYING` (GNU GPL version
2). The original per-file copyright, license and warranty notices are retained.
This port and the Canvas Guest Rust code are distributed under **GPL-2.0**,
without warranty. `cartridge/assets/SCHISM-LICENSE.txt` carries the same license
inside the packed Cartridge so recipients of the wasm binary receive it too.

Module files are not part of this repository and are not covered by the engine
license. The viewer loads a module chosen by the user; the Cartridge does not
bundle or redistribute tracker music.

The copied subset contains `fmt/`, `player/`, `schism/`, `sys/`, the needed
headers under `include/`, `font/`, `helptext/`, `scripts/`, the generated
`auto/default-font.c` and `auto/helptext.c`, and upstream's `COPYING`,
`README.md`, `AUTHORS` and `AGENTS.md`. The upstream `auto/` directory is
normally produced at build time; it is checked in here so the port builds
without Perl and without autotools.

## Public boundary

`platform/wasm.c` exports the C API used by the Rust Canvas Guest:

```c
int  schism_wasm_boot(void);                 /* starts the UI; 0 on success */
void schism_wasm_frame(void);                /* one host frame / event loop tick */
void schism_wasm_set_time(uint64_t ms);      /* page time, also drives the timer */

const unsigned char *schism_wasm_framebuffer(void); /* 640*400 palette indices */
const unsigned int  *schism_wasm_palette(void);     /* 256 x 0xRRGGBBAA */
unsigned int schism_wasm_frame_sequence(void);
unsigned int schism_wasm_palette_sequence(void);

int  schism_wasm_load_module(const char *name, const void *data, size_t len);
void schism_wasm_show_pattern_editor(void);
int  schism_wasm_song_playing(void);
void schism_wasm_stop_song(void);

void schism_wasm_set_keymod(unsigned int mod);
void schism_wasm_key(int down, uint32_t sym, uint32_t scancode,
                     unsigned int mod, const char *text);
void schism_wasm_mouse_move(int x, int y);
void schism_wasm_mouse_button(int button, int down, int x, int y);
void schism_wasm_mouse_wheel(int delta, int x, int y);
void schism_wasm_focus(int focused);

uint32_t schism_wasm_audio_rate(void);
uint32_t schism_wasm_audio_channels(void);
size_t   schism_wasm_audio_available(void);
size_t   schism_wasm_audio_read(void *dst, size_t len);
void     schism_wasm_audio_clear(void);
```

- **One instance per guest module.** The engine keeps process-wide state and is
  not resettable; the Canvas Guest calls `schism_wasm_boot()` once.
- `schism_wasm_frame()` runs exactly one iteration of upstream's event loop. The
  loop is no longer blocking: `schism/main.c` splits initialization from the
  body. The Rust glue calls one tick per host animation frame.
- Time is the page's absolute `t` in milliseconds. Schism's timer backend reads
  it through `platform/wasm.c`; there are no WASI clock calls. The browser layer
  clamps non-finite or negative values in Rust.
- The framebuffer is row-major, 640 bytes per row. `video_blit11` writes palette
  indices directly, including Schism's software mouse cursor. The palette is
  rebuilt from the engine's 16 base colours through `video_colors_iterate()`
  whenever Schism applies a palette.
- Key events use SDL2-style keysyms and USB HID scancodes, matching Schism's
  existing SDL2 input tables. The Rust guest maps DOM `code`/`key` values and
  tracks latching Caps/Num/Scroll lock state.
- `schism_wasm_load_module` copies the caller's bytes through the normal format
  loaders with `slurp_memstream`; the caller may free them immediately. It
  installs the song and switches to the pattern editor. There is no filesystem.
- Audio is 48 kHz stereo s16le by default. The device vtable pulls a mix sized
  from elapsed page time, then appends it to a 256 KiB ring. The Rust guest
  drains bounded chunks into `audio.pcm`.

### Native restrictions

- All path-based file, stat, directory and clock WASI calls return `BADF` /
  `NOSYS` through `platform/wasm_libc.c`. Config load/save, recent-file lists,
  file dialogs and disk export therefore fail cleanly; the song lives only in
  memory.
- There is no clipboard integration; Schism's internal clipboard buffer works
  for copy/paste inside the tracker.
- Quit paths are intercepted under `SCHISM_WASM`: Ctrl+Q, Ctrl+Shift+Q and the
  File menu's Quit report "Quit is unavailable; reload the card" instead of
  reaching `os_sysexit()`/`exit()`. The unreachable teardown still calls them.
- The module loaders are the original parsers for trusted tracker modules.
  They are not a hardened service for arbitrary hostile uploads.

## Building

`../sources.txt` is the authoritative list of C files, relative to this
directory. `platform/wasm.c` implements the `schism_*_backend_sdl2` symbols
without including SDL headers. `compat/utf8proc.c` is a minimal ASCII
case-folding/pass-through implementation of the API the engine expects.
`platform/wasm_libc.c` supplies the freestanding allocator bridge and rejects
WASI syscalls. Do not link the unused upstream platform files.

Flags used by `canvas/build.rs`:

```sh
-DHAVE_CONFIG_H -D_GNU_SOURCE -std=gnu11 -O2 -fwrapv -fno-strict-aliasing \
-I. -Ivendor -Ivendor/include -Icompat --sysroot=<wasi-sysroot>
```

`build-config.h` defines `SCHISM_SDL2` and `SCHISM_WASM`, and selects only the
loaders and features that need no zlib/bzip2/lzma/zstd/flac/avformat/utf8proc
dependency. `USE_THREADS` is not defined, so the mixer runs from the host frame.

The wasm link uses a local copy of WASI libc with `__wasilibc_real.c.obj`
removed, plus `platform/wasm_libc.c`, Rust's allocator callbacks and
`--max-memory=67108864`. This keeps the module free of WASI host imports.

## Native verification

```sh
sh engine/tests/native-smoke.sh
CC=clang OUT=/tmp/schism-smoke sh engine/tests/native-smoke.sh
```

The script compiles the same source list for the host, runs the engine for 120
frames, loads a generated 2,236-byte ProTracker module through the in-memory
API, presses F5 and checks for non-silent PCM. It needs a host C compiler and
libm, not the WASI SDK. On macOS it tries the default SDK and the installed
15.4/26.5 SDKs; set `SDKROOT` if a different one matches your linker.

`cargo test --features engine` in `canvas/` also boots the C engine and checks
that it presents frames. The browser-level test is
`tests/wasm-smoke.mjs` at the example root; it exercises the full wasm build
through the Extism host interface.

## Patch inventory

- `auto/default-font.c`, `auto/helptext.c`: generated from upstream's
  `scripts/build-font.sh` and `scripts/genhelp.pl`, checked in so the engine
  build does not require Perl. No content changes.
- `fmt/compression.c`: under `SCHISM_WASM`, replace `setjmp`/`longjmp` input
  failure with a `failed` flag because wasi-libc has no ordinary setjmp.
- `include/song.h`, `schism/audio_loadsave.c`: split the format-loader core out
  of `song_create_load()` and add `song_load_memory()` using `slurp_memstream`
  for viewer-supplied bytes. Path loading still uses the original `slurp`.
- `schism/main.c`: move the blocking event loop's locals to file scope, split
  `event_loop_init()` from `event_loop_tick()`, export the tick under
  `SCHISM_WASM`, and make `schism_main()` return after initialization. Signal
  handlers and the native `main()` are compiled out on wasm.
- `schism/page.c`: guard the oscilloscope when the song has no audio buffer or
  the device buffer is shorter than one frame. Found by the native smoke test.
  Under `SCHISM_WASM`, the quit shortcuts and exit prompt report that quitting
  is unavailable instead of terminating the guest.
- `schism/vgamem.c`: guard zero-length sample drawing after the channel
  division; a short audio buffer otherwise underflowed `poshi`. Found by the
  native smoke test.
- `schism/util.c`: define `TMP_MAX` under `SCHISM_WASM`; wasi-libc omits it even
  though the browser port never creates temporary files.
- `platform/wasm.c`: all video, event, audio, timer, clipboard and dmoz backend
  slots; the exported C API above.
- `platform/wasm_libc.c`: shared Rust allocator callbacks and freestanding WASI
  syscall stubs.
- `compat/utf8proc.c`, `compat/utf8proc.h`: minimal UTF-8 API shim for the
  engine's case-folding calls when the real utf8proc library is not linked.
- `build-config.h`: hand-maintained configure results for the wasm and native
  builds.
