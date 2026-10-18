/* Minimal offline OPL3 backend for the vendored Chocolate Doom music player.
 * GPL-2.0; the Nuked OPL3 emulator in vendor/chocolate/opl3.c is LGPL-2.1.
 * The host has no SDL timer or audio device: the Canvas Guest calls
 * doom_music_mix() with the exact number of 48,000 Hz frames it needs. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "opl.h"
#include "opl3.h"

#define CALLBACK_MAX 64
#define MIX_CHUNK 512

typedef struct
{
    uint64_t due;
    opl_callback_t callback;
    void *data;
    int used;
} opl_callback_entry_t;

static opl3_chip chip;
static unsigned int sample_rate = 48000;
static uint64_t now_us;
static int initialized;
static int paused;
static opl_callback_entry_t callbacks[CALLBACK_MAX];

int snd_samplerate = 48000;
const char *snd_dmxoption = "-opl3";
unsigned int opl_io_port;

opl_init_result_t OPL_Init(unsigned int port_base)
{
    (void) port_base;
    OPL3_Reset(&chip, sample_rate);
    initialized = 1;
    paused = 0;
    now_us = 0;
    return OPL_INIT_OPL3;
}

void OPL_Shutdown(void)
{
    OPL_ClearCallbacks();
    initialized = 0;
    paused = 0;
}

void OPL_SetSampleRate(unsigned int rate)
{
    if (rate < 8000) rate = 8000;
    sample_rate = rate;
    if (initialized)
    {
        OPL3_Reset(&chip, sample_rate);
    }
}

void OPL_WritePort(opl_port_t port, unsigned int value)
{
    (void) port;
    (void) value;
}

unsigned int OPL_ReadPort(opl_port_t port)
{
    (void) port;
    return 0;
}

unsigned int OPL_ReadStatus(void)
{
    return 0;
}

void OPL_WriteRegister(int reg, int value)
{
    if (initialized)
    {
        OPL3_WriteReg(&chip, (uint16_t) reg, (uint8_t) value);
    }
}

opl_init_result_t OPL_Detect(void)
{
    return initialized ? OPL_INIT_OPL3 : OPL_INIT_NONE;
}

void OPL_InitRegisters(int opl3)
{
    int r;

    for (r = OPL_REGS_LEVEL; r <= OPL_REGS_LEVEL + OPL_NUM_OPERATORS; ++r)
        OPL_WriteRegister(r, 0x3f);

    for (r = OPL_REGS_ATTACK; r <= OPL_REGS_WAVEFORM + OPL_NUM_OPERATORS; ++r)
        OPL_WriteRegister(r, 0x00);

    for (r = 1; r < OPL_REGS_LEVEL; ++r)
        OPL_WriteRegister(r, 0x00);

    OPL_WriteRegister(OPL_REG_TIMER_CTRL, 0x60);
    OPL_WriteRegister(OPL_REG_TIMER_CTRL, 0x80);
    OPL_WriteRegister(OPL_REG_WAVEFORM_ENABLE, 0x20);

    if (opl3)
    {
        OPL_WriteRegister(OPL_REG_NEW, 0x01);
        for (r = OPL_REGS_LEVEL; r <= OPL_REGS_LEVEL + OPL_NUM_OPERATORS; ++r)
            OPL_WriteRegister(r | 0x100, 0x3f);
        for (r = OPL_REGS_ATTACK; r <= OPL_REGS_WAVEFORM + OPL_NUM_OPERATORS; ++r)
            OPL_WriteRegister(r | 0x100, 0x00);
        for (r = 1; r < OPL_REGS_LEVEL; ++r)
            OPL_WriteRegister(r | 0x100, 0x00);
    }

    OPL_WriteRegister(OPL_REG_FM_MODE, 0x40);

    if (opl3)
        OPL_WriteRegister(OPL_REG_NEW, 0x01);
}

void OPL_SetCallback(uint64_t us, opl_callback_t callback, void *data)
{
    int i;
    for (i = 0; i < CALLBACK_MAX; ++i)
    {
        if (!callbacks[i].used)
        {
            callbacks[i].used = 1;
            callbacks[i].due = now_us + us;
            callbacks[i].callback = callback;
            callbacks[i].data = data;
            return;
        }
    }
}

void OPL_AdjustCallbacks(float factor)
{
    int i;
    for (i = 0; i < CALLBACK_MAX; ++i)
    {
        if (callbacks[i].used && callbacks[i].due > now_us)
        {
            uint64_t remaining = callbacks[i].due - now_us;
            callbacks[i].due = now_us + (uint64_t) (remaining * factor);
        }
    }
}

void OPL_ClearCallbacks(void)
{
    memset(callbacks, 0, sizeof(callbacks));
}

void OPL_Lock(void) {}
void OPL_Unlock(void) {}
void OPL_Delay(uint64_t us) { (void) us; }

void OPL_SetPaused(int value)
{
    paused = value != 0;
}

static void run_due_callbacks(void)
{
    int i;
    for (i = 0; i < CALLBACK_MAX; ++i)
    {
        if (callbacks[i].used && callbacks[i].due <= now_us)
        {
            opl_callback_t callback = callbacks[i].callback;
            void *data = callbacks[i].data;
            callbacks[i].used = 0;
            if (callback != NULL)
            {
                callback(data);
            }
        }
    }
}

static uint64_t next_callback_time(void)
{
    uint64_t next = UINT64_MAX;
    int i;
    for (i = 0; i < CALLBACK_MAX; ++i)
    {
        if (callbacks[i].used && callbacks[i].due < next)
        {
            next = callbacks[i].due;
        }
    }
    return next;
}

/* Adds music to an existing stereo s16le buffer. */
void doom_music_mix(int16_t *out, int frames)
{
    int done = 0;

    if (!initialized || paused || out == NULL)
    {
        return;
    }

    while (done < frames)
    {
        uint64_t next;
        uint64_t until_us;
        int chunk;
        int16_t music[MIX_CHUNK * 2];
        int i;

        run_due_callbacks();

        next = next_callback_time();
        if (next == UINT64_MAX)
        {
            chunk = frames - done;
        }
        else if (next <= now_us)
        {
            chunk = 1;
        }
        else
        {
            until_us = next - now_us;
            chunk = (int) ((until_us * sample_rate) / 1000000);
            if (chunk < 1) chunk = 1;
            if (chunk > frames - done) chunk = frames - done;
        }
        if (chunk > MIX_CHUNK) chunk = MIX_CHUNK;

        OPL3_GenerateStream(&chip, music, (uint32_t) chunk);
        for (i = 0; i < chunk * 2; ++i)
        {
            int mixed = out[done * 2 + i] + music[i];
            if (mixed > 32767) mixed = 32767;
            if (mixed < -32768) mixed = -32768;
            out[done * 2 + i] = (int16_t) mixed;
        }

        now_us += ((uint64_t) chunk * 1000000) / sample_rate;
        done += chunk;
        run_due_callbacks();
    }
}
