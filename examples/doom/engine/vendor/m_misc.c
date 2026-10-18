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
// Modified 2026-10-18: fixed defaults, no config/save/screenshot files.
// Historical notice above retained; upstream now supplies GPL-2.0 (LICENSE.TXT).
// GPL-2.0 port of m_misc.c,v 1.6 1997/02/03; see ../README.md and LICENSE.TXT.
// Fixed defaults only; config, saves, and screenshots have no file IO.
#include <ctype.h>
#include "doomdef.h"
#include "doomstat.h"
#include "dstrings.h"
#include "z_zone.h"
#include "w_wad.h"
#include "m_misc.h"
#include "m_swap.h"
#include "i_system.h"
#include "v_video.h"
#include "hu_stuff.h"

extern patch_t *hu_font[HU_FONTSIZE];
int M_DrawText(int x,int y,boolean direct,char *string)
{
    int c,w;
    while (*string)
    {
        c = toupper((unsigned char)*string++)-HU_FONTSTART;
        if (c < 0 || c >= HU_FONTSIZE) { x+=4; continue; }
        w = SHORT(hu_font[c]->width);
        if (x+w > SCREENWIDTH) break;
        if (direct) V_DrawPatchDirect(x,y,0,hu_font[c]);
        else V_DrawPatch(x,y,0,hu_font[c]);
        x+=w;
    }
    return x;
}
boolean M_WriteFile(char const *name,void *source,int length)
{
    (void)name; (void)source; (void)length;
    return false;
}
int M_ReadFile(char const *name,byte **buffer)
{
    *buffer = NULL;
    I_Error("File IO disabled: %s",name);
    return 0;
}
int usemouse, usejoystick;
extern int key_right,key_left,key_up,key_down,key_strafeleft,key_straferight;
extern int key_fire,key_use,key_strafe,key_speed;
extern int mousebfire,mousebstrafe,mousebforward;
extern int joybfire,joybstrafe,joybuse,joybspeed;
extern int mouseSensitivity,showMessages,detailLevel,screenblocks,numChannels;
extern char *chat_macros[];
void M_SaveDefaults(void) {}
void M_LoadDefaults(void)
{
    mouseSensitivity = 5;
    snd_SfxVolume = snd_MusicVolume = 0;
    showMessages = 1;
    key_right = KEY_RIGHTARROW;
    key_left = KEY_LEFTARROW;
    key_up = KEY_UPARROW;
    key_down = KEY_DOWNARROW;
    key_strafeleft = ',';
    key_straferight = '.';
    key_fire = KEY_RCTRL;
    key_use = ' ';
    key_strafe = KEY_RALT;
    key_speed = KEY_RSHIFT;
    mousebfire = 0; mousebstrafe = 1; mousebforward = 2;
    joybfire = 0; joybstrafe = 1; joybuse = 3; joybspeed = 2;
    usemouse = usejoystick = 0;
    screenblocks = 10;
    detailLevel = 0;
    numChannels = 3;
    usegamma = 0;
    /* Avoid the original int-cast string defaults on 64-bit native builds. */
    chat_macros[0] = HUSTR_CHATMACRO0;
    chat_macros[1] = HUSTR_CHATMACRO1;
    chat_macros[2] = HUSTR_CHATMACRO2;
    chat_macros[3] = HUSTR_CHATMACRO3;
    chat_macros[4] = HUSTR_CHATMACRO4;
    chat_macros[5] = HUSTR_CHATMACRO5;
    chat_macros[6] = HUSTR_CHATMACRO6;
    chat_macros[7] = HUSTR_CHATMACRO7;
    chat_macros[8] = HUSTR_CHATMACRO8;
    chat_macros[9] = HUSTR_CHATMACRO9;
}
void M_ScreenShot(void)
{
    players[consoleplayer].message = "Screenshots disabled";
}
