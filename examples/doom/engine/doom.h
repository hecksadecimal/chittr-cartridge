/* DOOM in-memory host boundary. GPL-2.0; see LICENSE.TXT. */
#ifndef CARTRIDGE_DOOM_H
#define CARTRIDGE_DOOM_H
#ifdef __cplusplus
extern "C" {
#endif
/* One engine instance per process/module. Each input buffer must remain alive
 * and unchanged for its lifetime; wads[0] is the base and later entries are
 * mods searched in order, so a mod can override base lumps.
 * Returns 0 on success, -1 on error. Boot once; subsequent boots are rejected
 * (the original engine has global state). A failed boot also consumes the
 * instance. Skill is the original skill_t index (0 baby through 4 nightmare).
 * Starts the first map of a Doom I or Doom II/Freedoom WAD at that skill.
 * No files or sockets.
 * Native errors/quit unwind to this API. Wasm quit stops safely after the
 * current tic; wasm fatal errors store a diagnostic then trap (no setjmp).
 * The wasm host must catch fatal-error traps. */
int doom_boot(const unsigned char *const *wads, const int *lengths, int count, int skill);
/* Exactly one 1/35-second simulation tic and one draw; no wall-clock waits.
 * After an error or quit this is a no-op; inspect doom_error(). */
void doom_tick(void);
/* 320*200 row-major palette indices; NULL before a successful boot. */
unsigned char *doom_pixels(void);
/* 256 RGB triples, already corrected using the engine's current gamma table. */
unsigned char *doom_palette(void);
/* Original DOOM key codes, not host key codes. Lowercase ASCII for letters.
 * Arrows: right=0xae,left=0xac,up=0xad,down=0xaf; Esc=27,Enter=13,
 * Ctrl=0x9d,Shift=0xb6,Alt=0xb8,Backspace=127; space uses, Ctrl fires.
 * Ignored outside a running instance; key values must be in [0,255]. Events
 * queue until the next tick, including taps and repeated keydown events.
 * Queue bound is 512; overflow reconciles final key states to avoid stuck keys. */
void doom_key(int doom_key, int down);
/* Pointer-locked relative mouse movement and buttons (DOM bitmask: 1 left,
 * 2 right, 4 middle; forward/back side buttons are ignored). Mouse Y reaches the
 * original menus but no longer adds in-game forward/back movement. Ignored
 * outside a running instance. */
void doom_mouse(int dx, int dy, int buttons);
/* One queued high-level sound event. The name is an sfxinfo name without the
 * DS lump prefix (at most 7 bytes plus NUL); volume is 0 to 15. */
typedef struct doom_sound_event_s
{
    unsigned char name[8];
    unsigned char volume;
} doom_sound_event_t;
/* Called by the high-level sound interface; the canvas mixer drains it. */
void doom_queue_sound(const char *name, int volume);
/* Returns 1 and fills event when one is queued, 0 when the queue is empty. */
int doom_pop_sound_event(doom_sound_event_t *event);
/* Adds the OPL music mix to an interleaved stereo s16le buffer. */
int doom_music_active(void);
void doom_music_mix(short *samples, int frames);
/* Empty string while healthy; stable diagnostic after fatal error or "quit". */
const char *doom_error(void);
#ifdef __cplusplus
}
#endif
#endif
