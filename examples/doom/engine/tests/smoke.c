/* Native integration smoke. GPL-2.0; host-only test IO is not engine IO. */
#undef printf
#undef fprintf
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include "doom.h"
#include "vendor/doomdef.h"
#include "vendor/doomstat.h"
#include "vendor/g_game.h"
extern boolean gamekeydown[256];

int test_invalid_wad(const char *which);

static void fail(const char *message)
{
    fprintf(stderr,"FAIL: %s; engine: %s\n",message,doom_error());
    exit(1);
}
static uint32_t frame_hash(void)
{
    const unsigned char *pixels = doom_pixels();
    uint32_t hash = 2166136261u;
    int i;
    if (!pixels) fail("no frame");
    for (i=0; i<320*200; ++i) hash = (hash^pixels[i])*16777619u;
    return hash;
}
static void step(int count)
{
    while (count--)
    {
        doom_tick();
        if (doom_error()[0]) fail("tick error");
    }
}
static void press(int key)
{
    doom_key(key,1); step(1);
    doom_key(key,0); step(1);
}
int main(int argc,char **argv)
{
    FILE *f;
    long len;
    unsigned char *wad;
    const unsigned char *wads[2];
    int lengths[2];
    uint32_t initial,level,moving,turning;
    int x,y,i;
    int tick_error = argc == 3 && !strcmp(argv[1],"--tick-error");
    int skill_test = argc == 3 && !strcmp(argv[1],"--skill");
    int music_test = argc == 3 && !strcmp(argv[1],"--music");
    int mouse_test = argc == 3 && !strcmp(argv[1],"--mouse");
    int mod_test = argc == 4 && !strcmp(argv[1],"--mod");
    const char *wadpath = (tick_error || skill_test || music_test || mouse_test || mod_test) ? argv[2] : argc >= 2 ? argv[1] : "";
    if (argc == 3 && !strcmp(argv[1],"--invalid")) return test_invalid_wad(argv[2]);
    if (argc != 2 && !tick_error && !skill_test && !music_test && !mouse_test && !mod_test) { fprintf(stderr,"usage: %s path/to/doom1.wad [mod.wad]\n",argv[0]); return 2; }
    f = fopen(wadpath,"rb");
    if (!f) { perror(argv[1]); return 2; }
    fseek(f,0,SEEK_END); len=ftell(f); rewind(f);
    if (len <= 0 || len > 0x7fffffff) return 2;
    wad=malloc((size_t)len);
    if (!wad || fread(wad,1,(size_t)len,f)!=(size_t)len) return 2;
    fclose(f);
    wads[0] = wad;
    lengths[0] = (int)len;
    if (mod_test)
    {
        unsigned char *modwad;
        FILE *mf = fopen(argv[3],"rb");
        long mlen;
        if (!mf) { perror(argv[3]); return 2; }
        fseek(mf,0,SEEK_END); mlen=ftell(mf); rewind(mf);
        if (mlen <= 0 || mlen > 0x7fffffff) return 2;
        modwad=malloc((size_t)mlen);
        if (!modwad || fread(modwad,1,(size_t)mlen,mf)!=(size_t)mlen) return 2;
        fclose(mf);
        wads[1] = modwad;
        lengths[1] = (int)mlen;
        if (doom_boot(wads,lengths,2,sk_medium)) fail("base + mod boot");
        /* Default gamma maps 1,2,3 to 2,3,4; the important part is that the
           mod's PLAYPAL replaced the base one. */
        if (doom_palette()[0] != 2 || doom_palette()[1] != 3 || doom_palette()[2] != 4)
            fail("mod did not override the base PLAYPAL");
        puts("PASS: base + mod WAD booted, later WAD overrode the base lump");
        return 0;
    }
    if (doom_boot(wads,lengths,1,skill_test ? sk_nightmare : sk_medium)) fail("boot");
    if (skill_test)
    {
        if (gameskill != sk_nightmare || gamestate != GS_LEVEL || !players[0].mo)
            fail("boot did not honour the configured skill");
        puts("PASS: boot honoured the configured skill");
        return 0;
    }
    if (mouse_test)
    {
        angle_t before = players[0].mo->angle;
        fixed_t x = players[0].mo->x, y = players[0].mo->y;
        doom_mouse(300,0,0);
        step(1);
        if (players[0].mo->angle == before) fail("relative mouse did not turn the player");
        before = players[0].mo->angle;
        doom_mouse(0,300,0);
        step(1);
        if (players[0].mo->x != x || players[0].mo->y != y)
            fail("mouse Y moved the player forward/back");
        if (players[0].mo->angle != before) fail("mouse Y turned the player");
        doom_mouse(0,0,8 | 16);
        step(1);
        if (players[0].mo->x != x || players[0].mo->y != y)
            fail("mouse side buttons moved the player");
        puts("PASS: relative mouse turns; Y and side buttons do not move the player");
        return 0;
    }
    if (music_test)
    {
        static short music[48000 * 2];
        int i, nonzero = 0, peak = 0;
        if (!doom_music_active()) fail("level music did not start");
        memset(music,0,sizeof(music));
        doom_music_mix(music,48000);
        for (i = 0; i < (int)(sizeof(music)/sizeof(music[0])); ++i)
        {
            int sample = music[i] < 0 ? -music[i] : music[i];
            if (sample != 0) nonzero = 1;
            if (sample > peak) peak = sample;
        }
        if (!nonzero) fail("level music mixed silence");
        printf("PASS: OPL music produced PCM (peak %d)\n",peak);
        return 0;
    }
    if (tick_error)
    {
        G_DeferedPlayDemo("MISSING"); /* test-only internal fault during G_Ticker */
        doom_tick();
        if (!strstr(doom_error(),"MISSING not found") || doom_pixels())
            fail("tick error did not unwind to host");
        doom_tick();
        puts("PASS: fatal tick error returned safely");
        return 0;
    }
    if (!doom_palette()) fail("no palette");
    if (gamestate != GS_LEVEL || !players[0].mo) fail("boot did not start a level");
    initial = frame_hash();
    /* A short host tap must not disappear even when down+up precede one tick. */
    doom_key(KEY_ESCAPE,1); doom_key(KEY_ESCAPE,0); step(1);
    if (!menuactive) fail("short menu tap lost");
    doom_key(KEY_ESCAPE,1); doom_key(KEY_ESCAPE,0); step(1);
    if (menuactive) fail("short menu close tap lost");
    step(10);
    /* Restart using only public key API, through the original episode menus. */
    press(KEY_ESCAPE); /* open menu */
    press(KEY_ENTER); /* new game */
    if (gamemode != commercial) press(KEY_ENTER); /* episode one */
    press(KEY_ENTER); /* default hurt me plenty */
    step(5);
    if (gamestate != GS_LEVEL || !players[0].mo) fail("menu did not start level");
    level = frame_hash();
    for (i=0; i<800; ++i) doom_key(KEY_UPARROW,1);
    doom_key(KEY_UPARROW,0); step(1);
    if (gamekeydown[KEY_UPARROW]) fail("event queue overflow stuck a key");
    doom_key(-1,1); doom_key(256,1); /* invalid host key values are ignored */
    x = players[0].mo->x; y = players[0].mo->y;
    doom_key(KEY_UPARROW,1); step(25); doom_key(KEY_UPARROW,0); step(1);
    if (players[0].mo->x == x && players[0].mo->y == y) fail("forward input did not move player");
    moving = frame_hash();
    doom_key(KEY_RIGHTARROW,1); step(20); doom_key(KEY_RIGHTARROW,0); step(1);
    turning = frame_hash();
    doom_key(KEY_RCTRL,1); step(70); doom_key(KEY_RCTRL,0); step(1);
    if (initial == 0 || level == moving || moving == turning) fail("frames did not change");
    /* F11 changes gamma; all palette bytes are already corrected by platform. */
    {
        unsigned char old[768];
        memcpy(old,doom_palette(),sizeof(old));
        press(KEY_F11);
        if (!memcmp(old,doom_palette(),sizeof(old))) fail("gamma did not update palette");
    }
    step(350);
    printf("PASS: boot %s, restart menu, movement, turning, firing, gamma, 500+ tics\n",
           gamemode == commercial ? "Doom II" : "Doom I");
    printf("frames initial=%08x level=%08x move=%08x turn=%08x final=%08x\n",
           initial,level,moving,turning,frame_hash());
    /* I_Quit must unwind tick, not terminate/trap the process. */
    press(KEY_F10); /* original quit confirmation shortcut */
    doom_key('y',1);
    doom_tick();
    if (strcmp(doom_error(),"quit")) fail("quit did not return to host");
    doom_tick();
    puts("PASS: quit returned safely");
    /* Deliberately do not free borrowed WAD or global engine zone before exit. */
    return 0;
}
