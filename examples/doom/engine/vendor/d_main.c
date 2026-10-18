// Copyright (C) 1993-1996 by id Software, Inc.
//
// This source is available for distribution and/or modification
// only under the terms of the DOOM Source Code License as
// published by id Software. All rights reserved.
//
// The source is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// FITNESS FOR A PARTICULAR PURPOSE. See the DOOM Source Code License
// for more details.
//
// Modified 2026-10-18: bounded startup, single-tic loop, immediate transitions.
// Historical notice above retained; upstream now supplies GPL-2.0 (LICENSE.TXT).
// This GPL-2.0 port replaces startup/loop/OS paths; see ../README.md and LICENSE.TXT.
// Derived from d_main.c,v 1.8 1997/02/03 22:45:09 b1 Exp.
#include "doomdef.h"
#include "doomstat.h"
#include "sounds.h"
#include "z_zone.h"
#include "w_wad.h"
#include "s_sound.h"
#include "v_video.h"
#include "m_misc.h"
#include "m_menu.h"
#include "i_system.h"
#include "i_sound.h"
#include "i_video.h"
#include "g_game.h"
#include "f_finale.h"
#include "hu_stuff.h"
#include "wi_stuff.h"
#include "st_stuff.h"
#include "am_map.h"
#include "p_setup.h"
#include "r_local.h"
#include "d_main.h"
#include "d_net.h"

char *wadfiles[MAXWADFILES];
boolean devparm, nomonsters, respawnparm, fastparm, drone;
boolean singletics = true;
skill_t startskill = sk_medium;
int startepisode = 1, startmap = 1;
/* Set by doom_boot before D_DoomMain; the host chooses the configured default. */
extern int cartridge_start_skill;
boolean autostart;
FILE *debugfile;
boolean advancedemo;
char wadfile[1024], mapdir[1024], basedefault[1024];
char title[128];
extern boolean inhelpscreens;
void D_CheckNetGame(void);
void D_DoAdvanceDemo(void);
void R_ExecuteSetViewSize(void);
void G_BuildTiccmd(ticcmd_t *cmd);
extern boolean setsizeneeded;

event_t events[MAXEVENTS];
int eventhead, eventtail;
void D_PostEvent(event_t *ev)
{
    int next = (eventhead + 1) & (MAXEVENTS - 1);
    /* Never confuse a full ring with an empty one. Host boundary normally
       drains at most one event/key per tick, below MAXEVENTS. */
    if (next == eventtail)
        eventtail = (eventtail + 1) & (MAXEVENTS - 1);
    events[eventhead] = *ev;
    eventhead = next;
}
void D_ProcessEvents(void)
{
    for (; eventtail != eventhead; eventtail = (eventtail+1)&(MAXEVENTS-1))
    {
        event_t *ev = &events[eventtail];
        if (!M_Responder(ev)) G_Responder(ev);
    }
}

gamestate_t wipegamestate = GS_DEMOSCREEN;
/* Original display ordering, with transitions drawn immediately instead of
   entering the wall-clock-driven, blocking melt-wipe loop. */
void D_Display(void)
{
    static boolean viewactivestate, menuactivestate, inhelpscreensstate, fullscreen;
    static gamestate_t oldgamestate = -1;
    static int borderdrawcount;
    boolean changed, redrawsbar = false;
    int y;
    if (nodrawers) return;
    if (setsizeneeded)
    {
        R_ExecuteSetViewSize();
        oldgamestate = -1;
        borderdrawcount = 3;
    }
    changed = gamestate != wipegamestate;
    if (gamestate == GS_LEVEL && gametic) HU_Erase();
    switch (gamestate)
    {
      case GS_LEVEL:
        if (!gametic) break;
        if (automapactive) AM_Drawer();
        if (changed || (viewheight != 200 && fullscreen)) redrawsbar = true;
        if (inhelpscreensstate && !inhelpscreens) redrawsbar = true;
        ST_Drawer(viewheight == 200, redrawsbar);
        fullscreen = viewheight == 200;
        break;
      case GS_INTERMISSION: WI_Drawer(); break;
      case GS_FINALE: F_Drawer(); break;
      case GS_DEMOSCREEN: D_PageDrawer(); break;
    }
    I_UpdateNoBlit();
    if (gamestate == GS_LEVEL && !automapactive && gametic)
        R_RenderPlayerView(&players[displayplayer]);
    if (gamestate == GS_LEVEL && gametic) HU_Drawer();
    if (gamestate != oldgamestate && gamestate != GS_LEVEL)
        I_SetPalette(W_CacheLumpName("PLAYPAL", PU_CACHE));
    if (gamestate == GS_LEVEL && oldgamestate != GS_LEVEL)
    {
        viewactivestate = false;
        R_FillBackScreen();
    }
    if (gamestate == GS_LEVEL && !automapactive && scaledviewwidth != 320)
    {
        if (menuactive || menuactivestate || !viewactivestate) borderdrawcount = 3;
        if (borderdrawcount) { R_DrawViewBorder(); borderdrawcount--; }
    }
    menuactivestate = menuactive;
    viewactivestate = viewactive;
    inhelpscreensstate = inhelpscreens;
    oldgamestate = wipegamestate = gamestate;
    if (paused)
    {
        y = automapactive ? 4 : viewwindowy+4;
        V_DrawPatchDirect(viewwindowx+(scaledviewwidth-68)/2,y,0,
                         W_CacheLumpName("M_PAUSE",PU_CACHE));
    }
    M_Drawer();
    I_FinishUpdate();
}

/* The host owns pacing. There is deliberately no D_DoomLoop. */
void D_DoomTick(void)
{
    I_StartFrame();
    I_StartTic();
    D_ProcessEvents();
    G_BuildTiccmd(&netcmds[consoleplayer][maketic % BACKUPTICS]);
    if (advancedemo) D_DoAdvanceDemo();
    M_Ticker();
    G_Ticker();
    gametic++;
    maketic++;
    D_Display();
}

int demosequence, pagetic;
char *pagename;
void D_PageTicker(void) { if (--pagetic < 0) D_AdvanceDemo(); }
void D_PageDrawer(void)
{
    V_DrawPatch(0,0,0,W_CacheLumpName(pagename,PU_CACHE));
}
void D_AdvanceDemo(void) { advancedemo = true; }
void D_DoAdvanceDemo(void)
{
    players[consoleplayer].playerstate = PST_LIVE;
    advancedemo = false;
    usergame = false;
    paused = false;
    gameaction = ga_nothing;
    /* Keep title attract mode in-memory, without unbounded demo parsing.
       Real games begin through the original menus and G_InitNew. */
    demosequence = 0;
    pagetic = 35 * 11;
    gamestate = GS_DEMOSCREEN;
    pagename = "TITLEPIC";
}
void D_StartTitle(void)
{
    gameaction = ga_nothing;
    demosequence = -1;
    D_AdvanceDemo();
}
void D_AddFile(char *file) { (void)file; }

void D_DoomMain(void)
{
    /* WAD directory is already bounds-checked by doom_boot. This first
       cartridge milestone deliberately exposes only DOOM episode one. */
    V_Init();
    M_LoadDefaults();
    Z_Init();
    W_InitMultipleFiles(wadfiles);
    gamemode = shareware;
    M_Init();
    R_Init();
    P_Init();
    I_Init();
    D_CheckNetGame();
    S_Init(snd_SfxVolume, snd_MusicVolume);
    HU_Init();
    ST_Init();
    I_InitGraphics();
    G_InitNew((skill_t)cartridge_start_skill,1,1);
}
