# Original DOOM C engine: in-memory host boundary

First playable milestone: original game simulation/software renderer, episode one,
320×200 indexed video, original keyboard/menu controls, and silent audio. The host
owns pacing, WAD storage, allocation integration, presentation, and input mapping.
This directory does not implement Rust, Canvas, WAD compression, or packaging.

## Provenance and license

Vendored `vendor/*.[ch]` comes from `linuxdoom-1.10` in
[id-Software/DOOM](https://github.com/id-Software/DOOM), revision
`a77dfb96cb91780ca334d0d4cfd86957558007e0` (2024-01-16, “Add GPL information”),
copied from `/Users/hex/Documents/Source/DOOM/linuxdoom-1.10`. The original
checkout is not modified. Changes in this copy are marked with dated comments.

`LICENSE.TXT` is an unmodified copy of upstream's root GNU GPL version 2 license.
`UPSTREAM-README.TXT` is its unmodified root README, including the current
ZeniMax/GPL-2.0 notice. Historical source-file copyright/license/warranty notices
are retained; their references to the original DOOM source license predate the
upstream GPL release. The port is distributed under GPL-2.0, without warranty.
Game WAD data is separate from the engine source license; this directory does
not grant any additional right to distribute game data.

## Public API

See [`doom.h`](doom.h) for exact declarations and key codes:

```c
int doom_boot(const unsigned char *wad, int len); /* 0 success, -1 error */
void doom_tick(void);                            /* one tic + draw */
unsigned char *doom_pixels(void);                 /* 320*200 indices */
unsigned char *doom_palette(void);                /* 768 RGB bytes */
void doom_key(int doom_key, int down);            /* original DOOM codes */
const char *doom_error(void);                     /* empty, diagnostic, or quit */
```

- **Single instance, boot once per process/module.** A failed boot consumes the
  instance too. Original global game state is not resettable or thread-safe.
  `doom_boot` starts E1M1 at medium difficulty and renders the first frame; it
  does not start an infinite loop or automatically play demos.
- Borrowed WAD bytes must remain allocated and unchanged for the instance's
  lifetime. All engine allocation uses ordinary `malloc`/`calloc` calls; a wasm
  embedder can provide shared-allocator shims. There is no shutdown/free API.
- Call `doom_tick` at **35 Hz**. Each call runs exactly one original single-player
  simulation tic and one render. No clock reads, sleeps, `TryRunTics`, network
  synchronization, or blocking transition wipes are involved.
- Frame indices are row-major, 320 bytes per row. Palette entries are RGB triples,
  already corrected by `gammatable[usegamma]`, including damage/pickup palettes.
  Read the current palette along with each frame; F11 changes gamma.
- Pass original DOOM key codes, not browser/SDL codes. Letters are lowercase
  ASCII; arrows are right `0xae`, left `0xac`, up `0xad`, down `0xaf`; Ctrl
  `0x9d` fires, Space uses, Shift `0xb6` runs, Alt `0xb8` strafes, Escape `27`
  opens the menu, Enter is `13`. Menu accessibility when a host consumes Escape
  is the host's responsibility (e.g. map M to original Escape).
- Key events preserve order and repeated keydowns, including down/up taps between
  ticks. A 512-event bound prevents unbounded queues; overflow reconciles final
  physical states, so dropped key-ups cannot leave stuck movement keys. Each
  event is drained without overflowing the original 64-entry event ring.
- After fatal error or quit, `doom_tick`/`doom_key` are inert, and `doom_pixels`
  returns NULL. Inspect `doom_error` before accessing pixels. A second boot is
  rejected. The palette pointer remains allocated but is not a healthy frame.

### Error and quit boundaries

On **native builds**, boot/tick establish `setjmp` boundaries. `I_Error` stores a
bounded diagnostic and `longjmp`s to the caller; `I_Quit` returns the same way
with diagnostic `"quit"`. Neither exits the process. Tests cover both boot errors
and a fatal error triggered during `G_Ticker`, as well as menu quit.

On **wasm**, the available WASI libc has no ordinary setjmp/longjmp. Fatal errors
store their diagnostic and trap; the host must catch the trap and must not
resume that engine instance. Menu quit is nontrapping: it sets stopped state and
`"quit"`, frees no resources, and lets the current tic finish safely. Subsequent
calls are inert. The host can show a stopped/reload screen. Native tests also
exercise this returning-quit branch by defining `__wasm__` for the platform code.

## WAD contract and validation

One complete in-memory **IWAD or PWAD** is accepted, irrespective of its filename.
Header, count, directory extent, every lump position, and every lump length are
validated with overflow-safe little-endian reads before engine initialization.
Zero-size marker lumps are allowed. Lump reads recheck bounds and copy from
memory; there are no file handles. Name lookup is case-insensitive and searches
backward, preserving later-lump precedence.

`PLAYPAL` (at least 14×768 bytes), `TITLEPIC`, and `E1M1` are required immediately;
the renderer/game then requests the remaining original game resources. This
milestone deliberately uses **shareware/episode-one rules even with a larger
WAD**, matching an E1-only packaged asset. Silent `S_*` and `I_*Sound` interfaces
never read audio payloads, so DS*/D_* entries may retain their names with empty
payloads. Original data is not rewritten by the engine.

Directory validation is **not** a complete validator for untrusted DOOM resource
formats: texture, patch, BSP, blockmap, demo, and map internals still mostly use
original engine parsing. Supply a trusted bundled WAD, not arbitrary uploaded
or hostile game data. Semantic corruption can still trap or access invalid
memory. The public API is not a hardened general-purpose WAD service.

## Building

[`sources.txt`](sources.txt) is the **authoritative source list**, with paths
relative to this directory. Compile those files, not `vendor/*.c` indiscriminately.
The original unused `i_main.c`, `i_system.c`, `i_video.c`, `i_sound.c`, `i_net.c`,
`p_saveg.c`, and `f_wipe.c` remain for source provenance but must not be linked.
`platform.c` replaces all original `i_*` implementation files. ISO libc and libm
are sufficient; there are no OS headers/syscalls in the selected implementation.
The original ASCII case-insensitive string helpers are provided by the target
libc (macOS and the WASI SDK both supply them).

Use flags equivalent to:

```sh
-std=gnu99 -O2 -fwrapv -fno-strict-aliasing \
-include path/to/engine/portable.h \
-Ipath/to/engine -Ipath/to/engine/vendor
```

Link libc and libm. Do not define `LINUX`, `NORMALUNIX`, `SNDSERV`, or `USEASM`.
`portable.h` supplies standard declarations for old source units and suppresses
original stdout/stderr logging, so the selected engine does not touch file
descriptors. Native test files explicitly re-enable their own host-only logs.
The compiler's `__wasm__` define selects the error-boundary fallback.

The fixed-point engine intentionally uses wrapping arithmetic and historical
signed-shift idioms; retain `-fwrapv -fno-strict-aliasing`. Some original compiler
warnings remain (enum comparison, a historic bitwise-not precedence expression,
and unsigned-angle `abs`); this port is not a full engine cleanup.

The zone is **16 MiB**, allocated once, plus WAD directory/cache tables and video
buffers. It has been tested against both supplied full PWAD and compact E1 data.
The host must additionally budget its own WAD, allocator, raster/image copies,
and UI buffers. A 16 MiB zone is not a claim that the whole embed uses 16 MiB.

## Native verification

From the repository root:

```sh
examples/doom/engine/tests/native-smoke.sh
ASAN=1 examples/doom/engine/tests/native-smoke.sh /absolute/path/to/episode1.wad
```

`CC` and `OUT` may override compiler and `/tmp/doom-engine-smoke` output path.
The test executable alone accepts a WAD path. All filesystem IO is in test code;
engine paths remain in-memory. Each rejection case runs in a fresh process.
ASAN defaults to `detect_leaks=0` because this intentionally one-shot engine has
no teardown and retains its borrowed WAD/global zone until process/module exit.

On the development Mac, the selected macOS 27 SDK's `.tbd` architecture list was
incompatible with installed Apple clang/ld. Verification used the installed
matching SDK explicitly (this is a local toolchain issue, not an engine need):

```sh
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk \
ASAN=1 examples/doom/engine/tests/native-smoke.sh
```

Verified on arm64 macOS/Apple clang, with the **16 MiB zone**:

- All 12 structural rejection cases: null/negative/short input, bad magic, empty
  or overflowing counts/directories, invalid/wrapping lump extents, missing data.
- Native fatal tick-error unwinding, inert post-error calls, second-boot refusal.
- Real E1M1 boot; same-tick menu taps; restart through original episode/skill
  menus; queue-overflow key-up recovery; movement, turning, firing, F11 gamma;
  more than 500 tics; original F10/Y quit without terminating the host.
- Full supplied PWAD, 14,445,628 bytes / 2,305 lumps, and the parent-generated
  silent E1 WAD, 7,537,560 bytes: both pass ASAN, with identical gameplay hashes.
  Current smoke frame hashes: initial `dcadc15f`, restarted `fdf61095`, moved
  `b3d07a38`, turned `50b42358`, final `6ec8d7e3`.
- All manifest sources compile with WASI SDK 34 clang for wasm32. The resulting
  engine archive has no direct WASI, file, socket, setjmp/longjmp, printf/fprintf,
  or exit dependencies. Final Rust/Canvas linking and browser validation are
  owned by the parent integration task.

## Patch inventory

- `d_main.c`: replace command-line/filesystem startup; return after initialization;
  direct E1M1 start; host-driven singletics; no net updates or blocking wipe loop;
  keep original display ordering and title/menu entry points without auto demos.
- `d_net.c`: original tic-command storage plus explicit single-player setup only.
- `w_wad.c`: validated borrowed-memory directory, lookup, reads, and zone caching;
  remove file reload/profile output paths.
- `m_misc.c`: original text draw with corrected font bound; fixed integer/string
  defaults without pointer truncation; no config, screenshot, or generic file IO.
- `m_menu.c`, `g_game.c`: disable persistent load/save/quick-save and the entire
  unsafe scratch-buffer serialization path, not just its final write. Unused
  original serializer is excluded from the build.
- `s_sound.c`, `doomdef.h`: entirely silent sound and no sound server; no audio
  lump reads. `platform.c` supplies silent low-level sound/music interfaces too.
- `r_data.c`, `r_draw.c`, `p_setup.c`, `z_zone.c`: fix 32-bit pointer assumptions
  for LP64 allocations/alignment/owner comparisons; keep the obsolete on-disk
  texture-directory field 32 bits rather than a native pointer.
- `info.c`, `info.h`: add the missing sprite-name NULL sentinel expected by the
  renderer (caught as a global overread by ASAN).
- `am_map.c`, `m_bbox.h`: explicit integer locals and replace obsolete `values.h`.
- `g_game.c`: clear actual mouse/joystick arrays rather than pointer-sized regions.
