/*
 * Schism Tracker - a cross-platform Impulse Tracker clone
 *
 * Browser (WebAssembly) platform backend for the Chittr Canvas Guest.
 *
 * This file implements the SDL2-named backend slots without including any SDL
 * headers: video goes to a 640x400 indexed framebuffer, input arrives from the
 * Canvas Guest through exported C functions, audio is pulled by the Rust host
 * glue, and time is supplied by the host frame loop.
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 */

#include "headers.h"

#include "charset.h"
#include "clippy.h"
#include "config.h"
#include "dmoz.h"
#include "events.h"
#include "it.h"
#include "keyboard.h"
#include "log.h"
#include "mem.h"
#include "page.h"
#include "palettes.h"
#include "song.h"
#include "str.h"
#include "timer.h"
#include "video.h"
#include "vgamem.h"

#include "backend/audio.h"
#include "backend/clippy.h"
#include "backend/dmoz.h"
#include "backend/events.h"
#include "backend/timer.h"
#include "backend/video.h"

void event_loop_tick(void);

#define WASM_SCREEN_WIDTH 640
#define WASM_SCREEN_HEIGHT 400
#define WASM_RING_BYTES (256 * 1024)

/* --------------------------------------------------------------------- */
/* shared browser clock */

static uint64_t wasm_now_us = 0;

void schism_wasm_set_time(uint64_t milliseconds)
{
	wasm_now_us = milliseconds * UINT64_C(1000);
}

uint64_t schism_wasm_time_us(void)
{
	return wasm_now_us;
}

/* --------------------------------------------------------------------- */
/* videos */

static uint8_t wasm_framebuffer[WASM_SCREEN_WIDTH * WASM_SCREEN_HEIGHT];
static uint32_t wasm_palette[256];
static uint32_t wasm_identity[256];
static uint32_t wasm_frame_sequence = 0;
static uint32_t wasm_palette_sequence = 0;

const uint8_t *schism_wasm_framebuffer(void)
{
	return wasm_framebuffer;
}

const uint32_t *schism_wasm_palette(void)
{
	return wasm_palette;
}

uint32_t schism_wasm_frame_sequence(void)
{
	return wasm_frame_sequence;
}

uint32_t schism_wasm_palette_sequence(void)
{
	return wasm_palette_sequence;
}

static void wasm_palette_cb(unsigned int index, unsigned char rgb[3])
{
	if (index >= 256)
		return;
	wasm_palette[index] = ((uint32_t)rgb[0] << 24)
		| ((uint32_t)rgb[1] << 16)
		| ((uint32_t)rgb[2] << 8)
		| 0xff;
}

static int wasm_video_init(void)
{
	unsigned int i;

	for (i = 0; i < 256; i++)
		wasm_identity[i] = i;
	memset(wasm_framebuffer, 0, sizeof(wasm_framebuffer));
	memset(wasm_palette, 0, sizeof(wasm_palette));

	return events_init(&schism_events_backend_sdl2);
}

static void wasm_video_quit(void)
{
}

static int wasm_video_is_fullscreen(void)
{
	return 0;
}

static int wasm_video_width(void)
{
	return WASM_SCREEN_WIDTH;
}

static int wasm_video_height(void)
{
	return WASM_SCREEN_HEIGHT;
}

static const char *wasm_video_driver_name(void)
{
	return "browser";
}

static void wasm_video_report(void)
{
	log_appendf(5, " 640x400 indexed, browser canvas");
}

static void wasm_video_set_hardware(int hardware)
{
	(void)hardware;
}

static void wasm_video_shutdown(void)
{
}

static void wasm_video_setup(int interpolation)
{
	(void)interpolation;
}

static int wasm_video_startup(void)
{
	vgamem_clear();
	vgamem_flip();
	return 1;
}

static void wasm_video_fullscreen(int new_fs_flag)
{
	(void)new_fs_flag;
}

static void wasm_video_resize(uint32_t width, uint32_t height)
{
	(void)width;
	(void)height;
}

static void wasm_video_colors(unsigned char palette[16][3])
{
	video_colors_iterate(palette, wasm_palette_cb);
	wasm_palette_sequence++;
}

static int wasm_video_is_focused(void)
{
	return 1;
}

static int wasm_video_is_visible(void)
{
	return 1;
}

static int wasm_video_is_wm_available(void)
{
	return 0;
}

static int wasm_video_is_hardware(void)
{
	return 0;
}

static void wasm_video_show_system_cursor(int show)
{
	(void)show;
}

static int wasm_video_is_screensaver_enabled(void)
{
	return 0;
}

static void wasm_video_toggle_screensaver(int enabled)
{
	(void)enabled;
}

static void wasm_video_translate(uint32_t vx, uint32_t vy, uint32_t *x, uint32_t *y)
{
	video_translate_calculate(vx, vy, 0, 0,
		WASM_SCREEN_WIDTH, WASM_SCREEN_HEIGHT, x, y);
}

static void wasm_video_get_logical_coordinates(int x, int y, int *trans_x, int *trans_y)
{
	*trans_x = x;
	*trans_y = y;
}

static int wasm_video_is_input_grabbed(void)
{
	return 1;
}

static void wasm_video_set_input_grabbed(int enabled)
{
	(void)enabled;
}

static void wasm_video_warp_mouse(uint32_t x, uint32_t y)
{
	(void)x;
	(void)y;
}

static int wasm_video_have_menu(void)
{
	return 0;
}

static void wasm_video_toggle_menu(int on)
{
	(void)on;
}

static void wasm_video_blit(void)
{
	/* bpp 1 with an identity colour table writes the palette index itself,
	 * including Schism's software mouse cursor. */
	video_blit11(1, wasm_framebuffer, WASM_SCREEN_WIDTH, wasm_identity);
	wasm_frame_sequence++;
}

static void wasm_video_mousecursor_changed(void)
{
}

static int wasm_video_get_wm_data(video_wm_data_t *wm_data)
{
	(void)wm_data;
	return 0;
}

static void wasm_video_show_cursor(int enabled)
{
	(void)enabled;
}

static int wasm_video_refresh_rate(float *r)
{
	*r = 60.0f;
	return 0;
}

const schism_video_backend_t schism_video_backend_sdl2 = {
	.init = wasm_video_init,
	.quit = wasm_video_quit,
	.is_fullscreen = wasm_video_is_fullscreen,
	.width = wasm_video_width,
	.height = wasm_video_height,
	.driver_name = wasm_video_driver_name,
	.report = wasm_video_report,
	.set_hardware = wasm_video_set_hardware,
	.shutdown = wasm_video_shutdown,
	.setup = wasm_video_setup,
	.startup = wasm_video_startup,
	.fullscreen = wasm_video_fullscreen,
	.resize = wasm_video_resize,
	.colors = wasm_video_colors,
	.is_focused = wasm_video_is_focused,
	.is_visible = wasm_video_is_visible,
	.is_wm_available = wasm_video_is_wm_available,
	.is_hardware = wasm_video_is_hardware,
	.show_system_cursor = wasm_video_show_system_cursor,
	.is_screensaver_enabled = wasm_video_is_screensaver_enabled,
	.toggle_screensaver = wasm_video_toggle_screensaver,
	.translate = wasm_video_translate,
	.get_logical_coordinates = wasm_video_get_logical_coordinates,
	.is_input_grabbed = wasm_video_is_input_grabbed,
	.set_input_grabbed = wasm_video_set_input_grabbed,
	.warp_mouse = wasm_video_warp_mouse,
	.have_menu = wasm_video_have_menu,
	.toggle_menu = wasm_video_toggle_menu,
	.blit = wasm_video_blit,
	.mousecursor_changed = wasm_video_mousecursor_changed,
	.get_wm_data = wasm_video_get_wm_data,
	.show_cursor = wasm_video_show_cursor,
	.refresh_rate = wasm_video_refresh_rate,
};

/* --------------------------------------------------------------------- */
/* events */

static schism_keymod_t wasm_keymod = 0;

static int wasm_events_init(void)
{
	wasm_keymod = 0;
	return 1;
}

static void wasm_events_quit(void)
{
}

static void wasm_events_pump(void)
{
}

static schism_keymod_t wasm_events_keymod_state(void)
{
	return wasm_keymod;
}

const schism_events_backend_t schism_events_backend_sdl2 = {
	.init = wasm_events_init,
	.quit = wasm_events_quit,
	.flags = 0,
	.pump_events = wasm_events_pump,
	.keymod_state = wasm_events_keymod_state,
};

void schism_wasm_set_keymod(unsigned int mod)
{
	wasm_keymod = (schism_keymod_t)mod;
}

void schism_wasm_key(int down, uint32_t sym, uint32_t scancode,
	unsigned int mod, const char *text)
{
	schism_event_t event;

	memset(&event, 0, sizeof(event));
	wasm_keymod = (schism_keymod_t)mod;
	event.type = down ? SCHISM_KEYDOWN : SCHISM_KEYUP;
	event.key.state = down ? KEY_PRESS : KEY_RELEASE;
	event.key.sym = (schism_keysym_t)sym;
	event.key.scancode = (schism_scancode_t)scancode;
	event.key.mod = wasm_keymod;
	if (down && text) {
		strncpy(event.key.text, text, sizeof(event.key.text) - 1);
		event.key.text[sizeof(event.key.text) - 1] = '\0';
	}
	events_push_event(&event);
}

void schism_wasm_mouse_move(int x, int y)
{
	schism_event_t event;

	memset(&event, 0, sizeof(event));
	event.type = SCHISM_MOUSEMOTION;
	event.motion.x = x;
	event.motion.y = y;
	events_push_event(&event);
}

void schism_wasm_mouse_button(int button, int down, int x, int y)
{
	schism_event_t event;

	memset(&event, 0, sizeof(event));
	event.type = down ? SCHISM_MOUSEBUTTONDOWN : SCHISM_MOUSEBUTTONUP;
	event.button.button = (enum mouse_button)button;
	event.button.state = down != 0;
	event.button.clicks = 1;
	event.button.x = x;
	event.button.y = y;
	events_push_event(&event);
}

void schism_wasm_mouse_wheel(int delta, int x, int y)
{
	schism_event_t event;

	memset(&event, 0, sizeof(event));
	event.type = SCHISM_MOUSEWHEEL;
	event.wheel.y = delta;
	event.wheel.mouse_x = x;
	event.wheel.mouse_y = y;
	events_push_event(&event);
}

void schism_wasm_focus(int focused)
{
	schism_event_t event;

	memset(&event, 0, sizeof(event));
	event.type = focused ? SCHISM_WINDOWEVENT_FOCUS_GAINED
		: SCHISM_WINDOWEVENT_FOCUS_LOST;
	events_push_event(&event);
}

/* --------------------------------------------------------------------- */
/* audio */

struct wasm_audio_device {
	struct schism_audio_device_simple simple;
	schism_audio_spec_t spec;
	uint8_t mix[64 * 1024];
	size_t current_bytes;
	uint64_t last_mix_us;
	uint64_t leftover;
	uint8_t ring[WASM_RING_BYTES];
	size_t head;
	size_t tail;
	size_t length;
};

static struct wasm_audio_device wasm_audio;

static uint32_t wasm_audio_frame_bytes(const struct wasm_audio_device *dev)
{
	return dev->spec.channels * ((dev->spec.bits + 7) / 8);
}

static void *wasm_audio_get_buffer(schism_audio_device_t *device, size_t *buflen)
{
	struct wasm_audio_device *dev = (struct wasm_audio_device *)device;
	uint64_t elapsed;
	uint64_t frames;

	if (!dev || dev->spec.freq == 0 || dev->spec.channels == 0)
		return NULL;

	elapsed = wasm_now_us - dev->last_mix_us;
	if (elapsed > UINT64_C(100000))
		elapsed = UINT64_C(100000);
	dev->last_mix_us = wasm_now_us;

	frames = (elapsed * dev->spec.freq + dev->leftover) / UINT64_C(1000000);
	dev->leftover = (elapsed * dev->spec.freq + dev->leftover) % UINT64_C(1000000);
	if (frames > dev->spec.samples)
		frames = dev->spec.samples;
	if (frames < 1)
		frames = 1;

	dev->current_bytes = (size_t)frames * wasm_audio_frame_bytes(dev);
	if (dev->current_bytes > sizeof(dev->mix))
		dev->current_bytes = sizeof(dev->mix);

	*buflen = dev->current_bytes;
	return dev->mix;
}

static int wasm_audio_play(schism_audio_device_t *device)
{
	struct wasm_audio_device *dev = (struct wasm_audio_device *)device;
	size_t bytes;
	size_t room;

	if (!dev || dev->current_bytes == 0)
		return 0;

	bytes = dev->current_bytes;
	if (bytes > sizeof(dev->ring)) {
		/* Keep the newest audio if a caller ever asks for a huge buffer. */
		memmove(dev->ring, dev->mix + (bytes - sizeof(dev->ring)), sizeof(dev->ring));
		dev->head = 0;
		dev->tail = 0;
		dev->length = sizeof(dev->ring);
		dev->current_bytes = 0;
		return 0;
	}

	/* Make room by dropping the oldest samples if the host cannot keep up. */
	room = sizeof(dev->ring) - dev->length;
	if (bytes > room) {
		size_t drop = bytes - room;
		if (drop > dev->length)
			drop = dev->length;
		dev->head = (dev->head + drop) % sizeof(dev->ring);
		dev->length -= drop;
	}

	{
		size_t first = MIN(bytes, sizeof(dev->ring) - dev->tail);

		memcpy(dev->ring + dev->tail, dev->mix, first);
		if (bytes > first)
			memcpy(dev->ring, dev->mix + first, bytes - first);
		dev->tail = (dev->tail + bytes) % sizeof(dev->ring);
		dev->length += bytes;
	}

	dev->current_bytes = 0;
	return 1;
}

static int64_t wasm_audio_ustowait(schism_audio_device_t *device)
{
	(void)device;
	/* The worker is called once per host frame; get_buffer sizes the mix
	 * from the elapsed frame time, so there is no extra pacing to add. */
	return 0;
}

static const struct schism_audio_device_simple_vtable wasm_audio_vtbl = {
	.get_buffer = wasm_audio_get_buffer,
	.play = wasm_audio_play,
	.wait = NULL,
	.aftercancel = NULL,
	.ustowait = wasm_audio_ustowait,
};

static int wasm_audio_init(void)
{
	return 1;
}

static void wasm_audio_quit(void)
{
}

static int wasm_audio_driver_count(void)
{
	return 1;
}

static const char *wasm_audio_driver_name(int i)
{
	return (i == 0) ? "browser" : NULL;
}

static uint32_t wasm_audio_device_count(uint32_t flags)
{
	return (flags & AUDIO_BACKEND_CAPTURE) ? 0 : 1;
}

static const char *wasm_audio_device_name(uint32_t i)
{
	return (i == 0) ? "browser audio" : NULL;
}

static int wasm_audio_init_driver(const char *driver)
{
	(void)driver;
	return 0;
}

static void wasm_audio_quit_driver(void)
{
}

static schism_audio_device_t *wasm_audio_open_device(uint32_t id,
	const schism_audio_spec_t *desired, schism_audio_spec_t *obtained)
{
	struct wasm_audio_device *dev = &wasm_audio;

	(void)id;

	memset(dev, 0, sizeof(*dev));
	dev->spec = *desired;

	if (dev->spec.freq == 0)
		dev->spec.freq = 48000;
	if (dev->spec.channels == 0)
		dev->spec.channels = 2;
	if (dev->spec.bits == 0)
		dev->spec.bits = 16;
	if (dev->spec.samples == 0)
		dev->spec.samples = 1024;
	if (dev->spec.samples > sizeof(dev->mix) / 4)
		dev->spec.samples = sizeof(dev->mix) / 4;

	*obtained = dev->spec;
	dev->last_mix_us = wasm_now_us;

	if (audio_simple_init((schism_audio_device_t *)dev, &wasm_audio_vtbl,
			desired->callback, AUDIO_SPEC_SILENCE(dev->spec)) < 0)
		return NULL;

	return (schism_audio_device_t *)dev;
}

static void wasm_audio_close_device(schism_audio_device_t *device)
{
	struct wasm_audio_device *dev = (struct wasm_audio_device *)device;

	audio_simple_close(&dev->simple);
	dev->length = 0;
	dev->head = 0;
	dev->tail = 0;
}

static void wasm_audio_lock_device(schism_audio_device_t *device)
{
	audio_simple_device_lock(device);
}

static void wasm_audio_unlock_device(schism_audio_device_t *device)
{
	audio_simple_device_unlock(device);
}

static void wasm_audio_pause_device(schism_audio_device_t *device, int paused)
{
	audio_simple_device_pause(device, paused);
}

static void wasm_audio_control_panel(schism_audio_device_t *device)
{
	(void)device;
}

static void wasm_audio_worker(schism_audio_device_t *device)
{
	if (device)
		audio_simple_worker(&((struct wasm_audio_device *)device)->simple);
}

const schism_audio_backend_t schism_audio_backend_sdl2 = {
	.flags = 0,
	.init = wasm_audio_init,
	.quit = wasm_audio_quit,
	.driver_count = wasm_audio_driver_count,
	.driver_name = wasm_audio_driver_name,
	.device_count = wasm_audio_device_count,
	.device_name = wasm_audio_device_name,
	.init_driver = wasm_audio_init_driver,
	.quit_driver = wasm_audio_quit_driver,
	.open_device = wasm_audio_open_device,
	.close_device = wasm_audio_close_device,
	.lock_device = wasm_audio_lock_device,
	.unlock_device = wasm_audio_unlock_device,
	.pause_device = wasm_audio_pause_device,
	.control_panel = wasm_audio_control_panel,
	.worker = wasm_audio_worker,
};

uint32_t schism_wasm_audio_rate(void)
{
	return wasm_audio.spec.freq ? wasm_audio.spec.freq : 48000;
}

uint32_t schism_wasm_audio_channels(void)
{
	return wasm_audio.spec.channels ? wasm_audio.spec.channels : 2;
}

size_t schism_wasm_audio_available(void)
{
	return wasm_audio.length;
}

size_t schism_wasm_audio_read(void *dst, size_t len)
{
	uint8_t *out = (uint8_t *)dst;
	size_t count;

	if (!dst || len == 0 || wasm_audio.length == 0)
		return 0;

	count = MIN(len, wasm_audio.length);
	{
		size_t first = MIN(count, sizeof(wasm_audio.ring) - wasm_audio.head);

		memcpy(out, wasm_audio.ring + wasm_audio.head, first);
		if (count > first)
			memcpy(out + first, wasm_audio.ring, count - first);
	}
	wasm_audio.head = (wasm_audio.head + count) % sizeof(wasm_audio.ring);
	wasm_audio.length -= count;
	return count;
}

void schism_wasm_audio_clear(void)
{
	wasm_audio.head = wasm_audio.tail = wasm_audio.length = 0;
	wasm_audio.current_bytes = 0;
}

/* --------------------------------------------------------------------- */
/* timer */

static int wasm_timer_init(void)
{
	return 1;
}

static void wasm_timer_quit(void)
{
}

static timer_ticks_t wasm_timer_ticks(void)
{
	return wasm_now_us / UINT64_C(1000);
}

static timer_ticks_t wasm_timer_ticks_us(void)
{
	return wasm_now_us;
}

static void wasm_timer_usleep(uint64_t us)
{
	(void)us;
}

static void wasm_timer_msleep(uint32_t ms)
{
	(void)ms;
}

const schism_timer_backend_t schism_timer_backend_sdl2 = {
	.init = wasm_timer_init,
	.quit = wasm_timer_quit,
	.ticks = wasm_timer_ticks,
	.ticks_us = wasm_timer_ticks_us,
	.usleep = wasm_timer_usleep,
	.msleep = wasm_timer_msleep,
	.oneshot = NULL,
};

/* --------------------------------------------------------------------- */
/* clipboard */

static char *wasm_clipboard = NULL;

static int wasm_clippy_init(void)
{
	return 1;
}

static void wasm_clippy_quit(void)
{
	free(wasm_clipboard);
	wasm_clipboard = NULL;
}

static int wasm_clippy_have(void)
{
	return 1;
}

static void wasm_clippy_set(const char *text)
{
	free(wasm_clipboard);
	wasm_clipboard = text ? str_dup(text) : NULL;
}

static char *wasm_clippy_get(void)
{
	return str_dup(wasm_clipboard ? wasm_clipboard : "");
}

const schism_clippy_backend_t schism_clippy_backend_sdl2 = {
	.init = wasm_clippy_init,
	.quit = wasm_clippy_quit,
	.have_selection = wasm_clippy_have,
	.set_selection = wasm_clippy_set,
	.get_selection = wasm_clippy_get,
	.have_clipboard = wasm_clippy_have,
	.set_clipboard = wasm_clippy_set,
	.get_clipboard = wasm_clippy_get,
};

/* --------------------------------------------------------------------- */
/* dmoz */

static int wasm_dmoz_init(void)
{
	return 1;
}

static void wasm_dmoz_quit(void)
{
}

static char *wasm_dmoz_get_exe_path(void)
{
	return str_dup(".");
}

const schism_dmoz_backend_t schism_dmoz_backend_sdl2 = {
	.init = wasm_dmoz_init,
	.quit = wasm_dmoz_quit,
	.get_exe_path = wasm_dmoz_get_exe_path,
};

/* --------------------------------------------------------------------- */
/* boot and module loading */

int schism_wasm_boot(void)
{
	char *argv[] = { (char *)"schismtracker", NULL };

	return schism_main(1, argv);
}

void schism_wasm_frame(void)
{
	event_loop_tick();
}

int schism_wasm_load_module(const char *name, const unsigned char *data, size_t len)
{
	int loaded;

	if (!data || len == 0)
		return 0;

	loaded = song_load_memory(name && *name ? name : "module", data, len);
	if (loaded) {
		set_page(PAGE_PATTERN_EDITOR);
		status.flags |= NEED_UPDATE;
	}
	return loaded;
}

void schism_wasm_show_pattern_editor(void)
{
	set_page(PAGE_PATTERN_EDITOR);
	status.flags |= NEED_UPDATE;
}

int schism_wasm_song_playing(void)
{
	return song_get_mode() == MODE_PLAYING;
}

void schism_wasm_stop_song(void)
{
	song_stop();
}
