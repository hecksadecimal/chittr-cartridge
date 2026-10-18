/* In-memory, single-threaded DOOM host platform. GPL-2.0; see LICENSE.TXT.
 * Replaces upstream i_system/i_video/i_net/i_sound and i_main completely. */
#include "portable.h"
#include <stdarg.h>
#ifndef __wasm__
#include <setjmp.h>
#endif
#include "doom.h"
#include "vendor/doomdef.h"
#include "vendor/doomstat.h"
#include "vendor/d_main.h"
#include "vendor/d_net.h"
#include "vendor/i_system.h"
#include "vendor/i_video.h"
#include "vendor/i_sound.h"
#include "vendor/i_net.h"
#include "vendor/m_argv.h"
#include "vendor/v_video.h"
#include "vendor/w_wad.h"

#ifndef __wasm__
static jmp_buf escape;
#define DOOM_GUARD() setjmp(escape)
#else
#define DOOM_GUARD() 0
#endif
static int guarded, attempted, running, ticks;
static char error_text[512];
static byte rgb[768];
/* Preserve event order (including press+release between ticks), then reconcile
 * final physical state if a pathological host floods the bounded queue. */
static byte desired_keys[256], delivered_keys[256];
static struct { byte key, down; } pending_keys[512];
static int pending_count;
void D_DoomTick(void);
void D_ProcessEvents(void);
void W_SetMemory(const byte *data,int len);

int doom_boot(const unsigned char *wad,int len)
{
    static char *argv[] = {"doom-cartridge", NULL};
    if (attempted)
    {
        if (!error_text[0]) strcpy(error_text,"doom_boot may only be called once");
        running = 0;
        return -1;
    }
    attempted = 1;
    guarded = 1;
    if (DOOM_GUARD()) { guarded = 0; running = 0; return -1; }
    W_SetMemory(wad,len);
    myargc = 1; myargv = argv;
    D_DoomMain();
    running = 1;
    D_DoomTick();
    guarded = 0;
    return 0;
}
void doom_tick(void)
{
    if (!running) return;
    guarded = 1;
    if (DOOM_GUARD()) { guarded = 0; running = 0; return; }
    ++ticks;
    D_DoomTick();
    guarded = 0;
}
unsigned char *doom_pixels(void) { return running ? screens[0] : NULL; }
unsigned char *doom_palette(void) { return rgb; }
const char *doom_error(void) { return error_text; }
void doom_key(int key,int down)
{
    if (!running || key < 0 || key > 255) return;
    desired_keys[key] = down != 0;
    if (pending_count < (int)(sizeof(pending_keys)/sizeof(pending_keys[0])))
    {
        pending_keys[pending_count].key = (byte)key;
        pending_keys[pending_count].down = down != 0;
        ++pending_count;
    }
}
void I_Error(char *format,...)
{
    va_list args;
    va_start(args,format);
    vsnprintf(error_text,sizeof(error_text),format,args);
    va_end(args);
    running = 0;
#ifndef __wasm__
    if (guarded) longjmp(escape,1);
#else
    /* WASI libc does not supply setjmp. The host catches the trap; text is
       stored first. Native boot/tick always unwind to the public boundary. */
    __builtin_trap();
#endif
}
void I_Quit(void)
{
    strcpy(error_text,"quit");
    running = 0;
#ifndef __wasm__
    if (guarded) longjmp(escape,1);
#else
    /* No resources are freed. The menu caller can safely return and finish its
       current tic; subsequent doom_tick calls are inert. Host displays "quit". */
    return;
#endif
}
void I_Init(void) {}
byte *I_ZoneBase(int *size)
{
    byte *zone;
    *size = 16*1024*1024;
    zone = malloc((size_t)*size);
    if (!zone) I_Error("Could not allocate DOOM zone");
    return zone;
}
int I_GetTime(void) { return ticks; }
void I_StartFrame(void) {}
static void deliver_key(int key,int down)
{
    event_t event;
    event.type = down ? ev_keydown : ev_keyup;
    event.data1 = key; event.data2 = event.data3 = 0;
    delivered_keys[key] = down;
    D_PostEvent(&event);
    /* Drain immediately, rather than lose events when >63 keys change. */
    D_ProcessEvents();
}
void I_StartTic(void)
{
    int key, i;
    for (i=0; i<pending_count; ++i)
        deliver_key(pending_keys[i].key,pending_keys[i].down);
    pending_count = 0;
    for (key=0; key<256; ++key)
    {
        if (desired_keys[key] != delivered_keys[key])
        {
            deliver_key(key,desired_keys[key]);
        }
    }
}
ticcmd_t *I_BaseTiccmd(void)
{
    static ticcmd_t empty;
    return &empty;
}
byte *I_AllocLow(int length)
{
    byte *mem = calloc(1,(size_t)length);
    if (!mem) I_Error("I_AllocLow: out of memory");
    return mem;
}
void I_Tactile(int on,int off,int total) { (void)on; (void)off; (void)total; }
void I_InitGraphics(void)
{
    I_SetPalette(W_CacheLumpName("PLAYPAL",1));
}
void I_ShutdownGraphics(void) {}
void I_SetPalette(byte *palette)
{
    int i;
    for (i=0; i<768; ++i) rgb[i] = gammatable[usegamma][palette[i]];
}
void I_UpdateNoBlit(void) {}
void I_FinishUpdate(void) {}
void I_WaitVBL(int count) { (void)count; }
void I_ReadScreen(byte *dest) { memcpy(dest,screens[0],320*200); }
void I_BeginRead(void) {}
void I_EndRead(void) {}
void I_InitNetwork(void)
{
    static doomcom_t single;
    doomcom = &single;
    single.id = DOOMCOM_ID;
    single.ticdup = single.numnodes = single.numplayers = 1;
    single.consoleplayer = 0;
    netgame = false;
}
void I_NetCmd(void) { I_Error("Network disabled"); }

/* Silent platform: no device, mixer, sound server, clocks, or host callbacks. */
void I_InitSound(void) {}
void I_UpdateSound(void) {}
void I_SubmitSound(void) {}
void I_ShutdownSound(void) {}
void I_SetChannels(void) {}
int I_GetSfxLumpNum(sfxinfo_t *sfx)
{
    (void)sfx;
    return 0; /* Silent high-level interface never requests an audio lump. */
}
int I_StartSound(int id,int vol,int sep,int pitch,int priority)
{ (void)id; (void)vol; (void)sep; (void)pitch; (void)priority; return 0; }
void I_StopSound(int handle) { (void)handle; }
int I_SoundIsPlaying(int handle) { (void)handle; return 0; }
void I_UpdateSoundParams(int handle,int vol,int sep,int pitch)
{ (void)handle; (void)vol; (void)sep; (void)pitch; }
void I_InitMusic(void) {}
void I_ShutdownMusic(void) {}
void I_SetMusicVolume(int volume) { (void)volume; }
void I_PauseSong(int handle) { (void)handle; }
void I_ResumeSong(int handle) { (void)handle; }
int I_RegisterSong(void *data) { (void)data; return 0; }
void I_PlaySong(int handle,int looping) { (void)handle; (void)looping; }
void I_StopSong(int handle) { (void)handle; }
void I_UnRegisterSong(int handle) { (void)handle; }
