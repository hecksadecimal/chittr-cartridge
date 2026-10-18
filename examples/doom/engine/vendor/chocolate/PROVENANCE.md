# Vendored Chocolate Doom music subsystem

These files come from the Chocolate Doom repository at commit
`e6f632a4397023b6b17cc312525a16b2aacd64d6` (2026-10-18).
Upstream: <https://github.com/chocolate-doom/chocolate-doom>.

Vendored files:

- `src/i_oplmusic.c` — GENMIDI instrument loading, MIDI channel/voice handling and OPL music playback
- `src/mus2mid.c`, `src/mus2mid.h` — MUS to type-0 MIDI conversion
- `src/midifile.c`, `src/midifile.h` — MIDI parsing
- `src/memio.c`, `src/memio.h` — in-memory streams
- `opl/opl3.c`, `opl/opl3.h`, `opl/wf_rom.h` — Nuked OPL3 emulator
- `opl/opl.h` — OPL interface declarations

`i_swap.h` is a small local compatibility shim for the vendored MIDI reader.

## Local modifications

The cartridge has no filesystem, SDL, audio device or wall-clock timer, so the
files are adapted rather than used as a desktop build:

- `i_oplmusic.c` reads MUS/MIDI data from memory instead of temporary files,
  uses a local `DEH_String` identity macro, and exposes small
  `doom_music_*` entry points instead of Chocolate's `music_module_t` table.
  It checks for a `GENMIDI` lump before initializing.
- `midifile.c` reads tracks from `MEMFILE` streams through
  `MIDI_LoadFileFromMemory`; the file-based loader and its test `main` are not
  part of the cartridge build.
- `i_swap.h` supplies the big-endian swaps the MIDI reader expects.
- `engine/opl_compat.c` implements the Chocolate OPL interface on top of
  Nuked OPL3 with a sample-driven callback clock. It has no SDL dependency.

The upstream copyright and license notices are retained in each file. The
Chocolate Doom code is GPL-2.0. The Nuked OPL3 emulator is LGPL-2.1. The
cartridge as a whole is distributed under GPL-2.0; the Nuked OPL3 source and
notices are included here.
