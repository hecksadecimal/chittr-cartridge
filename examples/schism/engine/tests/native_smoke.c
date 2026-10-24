/*
 * Native boundary smoke test for the browser Schism Tracker backend.
 *
 * Compiles the vendored engine twice: once for wasm32-wasip1 in build.rs and
 * once here with the host C compiler. This exercises boot, one event-loop tick
 * per host frame, the 640x400 framebuffer, module loading from memory, and the
 * PCM ring without a browser or a WASI runtime.
 *
 * GPL-2.0; see ../LICENSE.
 */
#include <stdio.h>
#include <stdint.h>
#include <string.h>

extern int schism_wasm_boot(void);
extern void schism_wasm_frame(void);
extern void schism_wasm_show_pattern_editor(void);
extern void schism_wasm_set_time(uint64_t ms);
extern uint32_t schism_wasm_frame_sequence(void);
extern const uint8_t *schism_wasm_framebuffer(void);
extern int schism_wasm_load_module(const char *name, const unsigned char *data, size_t len);
extern int schism_wasm_song_playing(void);
extern size_t schism_wasm_audio_available(void);
extern size_t schism_wasm_audio_read(void *dst, size_t len);
extern void schism_wasm_key(int down, uint32_t sym, uint32_t scancode,
	unsigned int mod, const char *text);

#define MOD_BYTES (1084 + 64 * 4 * 4 + 128)

static uint8_t mod[MOD_BYTES];

static void put_be16(uint8_t *at, unsigned int value)
{
	at[0] = (uint8_t)(value >> 8);
	at[1] = (uint8_t)value;
}

/* One looped square-wave sample, one pattern, C-2 on the first row. */
static size_t make_mod(void)
{
	uint8_t *sample;
	size_t at = 20;
	int i;

	memset(mod, 0, sizeof(mod));
	memcpy(mod, "Native smoke", 12);

	/* Sample 1: 64 words, volume 48, loop over the whole sample. */
	memcpy(mod + at, "square", 6);
	put_be16(mod + at + 22, 64);
	mod[at + 25] = 48;
	put_be16(mod + at + 28, 64);
	at += 31 * 30;

	mod[at] = 1; /* one order */
	mod[at + 1] = 0; /* restart order */
	at += 2;
	mod[at] = 0; /* order 0 -> pattern 0 */
	at += 128;
	memcpy(mod + at, "M.K.", 4);
	at += 4;

	/* Period 428 (C-2), sample 1, no effect. */
	mod[at] = 1;
	mod[at + 1] = 428 & 0xff;
	mod[at + 2] = 0x10;
	at += 64 * 4 * 4;

	sample = mod + at;
	for (i = 0; i < 128; i++)
		sample[i] = i < 64 ? 96 : (uint8_t)-96;

	return at + 128;
}

int main(void)
{
	uint64_t t;
	uint32_t last;
	uint8_t pcm[16384];
	size_t total = 0;
	size_t mod_len;
	size_t n;
	int i;
	int audible = 0;

	mod_len = make_mod();

	if (schism_wasm_boot() != 0) {
		fprintf(stderr, "boot failed\n");
		return 1;
	}
	schism_wasm_show_pattern_editor();

	last = schism_wasm_frame_sequence();
	for (i = 0, t = 1000; i < 120; i++, t += 16) {
		schism_wasm_set_time(t);
		schism_wasm_frame();
		n = schism_wasm_audio_read(pcm, sizeof(pcm));
		total += n;
	}
	if (schism_wasm_frame_sequence() == last) {
		fprintf(stderr, "the engine did not present a frame\n");
		return 1;
	}
	if (total == 0) {
		fprintf(stderr, "the engine produced no audio\n");
		return 1;
	}

	if (!schism_wasm_load_module("smoke.mod", mod, mod_len)) {
		fprintf(stderr, "module load failed\n");
		return 1;
	}
	/* F5 (scancode 62, SDL keysym SCANCODE_MASK | 0x3e). */
	schism_wasm_key(1, 0x4000003e, 62, 0, NULL);
	schism_wasm_key(0, 0x4000003e, 62, 0, NULL);
	for (i = 0; i < 60; i++, t += 16) {
		schism_wasm_set_time(t);
		schism_wasm_frame();
		n = schism_wasm_audio_read(pcm, sizeof(pcm));
		total += n;
		for (size_t j = 0; j < n; j++) {
			if (pcm[j]) {
				audible = 1;
				break;
			}
		}
	}
	if (!schism_wasm_song_playing()) {
		fprintf(stderr, "F5 did not start playback\n");
		return 1;
	}
	if (!audible) {
		fprintf(stderr, "module playback is silent\n");
		return 1;
	}

	printf("frames=%u audio=%zu first=%u,%u,%u mod=%zu\n",
		schism_wasm_frame_sequence() - last, total,
		schism_wasm_framebuffer()[0], schism_wasm_framebuffer()[1],
		schism_wasm_framebuffer()[640], mod_len);
	return 0;
}
