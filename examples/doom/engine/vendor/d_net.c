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
// Modified 2026-10-18: single-player command storage only; no transport.
// Historical notice above retained; upstream now supplies GPL-2.0 (LICENSE.TXT).
// GPL-2.0 port of d_net.c,v 1.3 1997/02/03; see ../README.md and LICENSE.TXT.
// The original single-player tic command storage, without any network transport.
#include "doomdef.h"
#include "doomstat.h"
#include "d_net.h"
#include "i_net.h"
#include "i_system.h"

doomcom_t *doomcom;
doomdata_t *netbuffer;
ticcmd_t netcmds[MAXPLAYERS][BACKUPTICS];
int maketic, ticdup = 1;
void D_CheckNetGame(void)
{
    I_InitNetwork();
    netbuffer = &doomcom->data;
    consoleplayer = displayplayer = 0;
    memset(playeringame, 0, sizeof(playeringame));
    playeringame[0] = true;
    netgame = false;
    deathmatch = 0;
}
void D_QuitNetGame(void) {}
void NetUpdate(void) {}
void TryRunTics(void) { I_Error("Host must drive single tics"); }
