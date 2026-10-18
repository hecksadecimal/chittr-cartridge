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
// Modified 2026-10-18: checked in-memory WAD directory and lump IO.
// Historical notice above retained; upstream now supplies GPL-2.0 (LICENSE.TXT).
// GPL-2.0 in-memory port of w_wad.c,v 1.5 1997/02/03; see ../README.md.
// No file handles, filesystem access, reloads, or profile output.
#include <ctype.h>
#include "doomtype.h"
#include "i_system.h"
#include "z_zone.h"
#include "w_wad.h"

lumpinfo_t *lumpinfo;
int numlumps;
void **lumpcache;
static const byte *wadbytes;
static size_t wadlength;

static uint32_t le32(const byte *p)
{
    return (uint32_t)p[0] | (uint32_t)p[1]<<8 |
           (uint32_t)p[2]<<16 | (uint32_t)p[3]<<24;
}
/* Validate the entire directory before allocating or exposing engine data.
 * Zero-length marker lumps may legally have offset 0 or EOF. */
void W_SetMemory(const byte *data, int len)
{
    uint32_t count, dir, i;
    if (!data || len < 12) I_Error("WAD: truncated header");
    if (memcmp(data,"IWAD",4) && memcmp(data,"PWAD",4))
        I_Error("WAD: expected IWAD or PWAD");
    count = le32(data+4);
    dir = le32(data+8);
    if (!count || count > INT_MAX / sizeof(lumpinfo_t) ||
        dir > (uint32_t)len || count > ((uint32_t)len-dir)/16)
        I_Error("WAD: directory outside buffer");
    for (i=0; i<count; ++i)
    {
        const byte *entry = data+dir+(size_t)i*16;
        uint32_t pos = le32(entry), size = le32(entry+4);
        if (pos > (uint32_t)len || size > (uint32_t)len-pos)
            I_Error("WAD: lump %u outside buffer", (unsigned)i);
    }
    wadbytes = data;
    wadlength = (size_t)len;
}
void W_InitMultipleFiles(char **filenames)
{
    uint32_t dir;
    int i;
    (void)filenames;
    if (!wadbytes) I_Error("WAD: no in-memory data");
    numlumps = (int)le32(wadbytes+4);
    dir = le32(wadbytes+8);
    lumpinfo = calloc((size_t)numlumps, sizeof(*lumpinfo));
    lumpcache = calloc((size_t)numlumps, sizeof(*lumpcache));
    if (!lumpinfo || !lumpcache) I_Error("WAD: directory allocation failed");
    for (i=0; i<numlumps; ++i)
    {
        const byte *entry = wadbytes+dir+(size_t)i*16;
        lumpinfo[i].position = (int)le32(entry);
        lumpinfo[i].size = (int)le32(entry+4);
        memcpy(lumpinfo[i].name, entry+8, 8);
    }
    if (W_CheckNumForName("PLAYPAL") < 0 ||
        W_CheckNumForName("TITLEPIC") < 0 ||
        W_CheckNumForName("E1M1") < 0)
        I_Error("WAD: incomplete game data (PLAYPAL, TITLEPIC, E1M1)");
    if (W_LumpLength(W_GetNumForName("PLAYPAL")) < 14*768)
        I_Error("WAD: PLAYPAL must contain 14 palettes");
}
void W_Reload(void) {}
int W_CheckNumForName(char *name)
{
    char key[8] = {0};
    int i, j;
    for (i=0; i<8 && name[i]; ++i)
        key[i] = (char)toupper((unsigned char)name[i]);
    for (i=numlumps-1; i>=0; --i)
    {
        for (j=0; j<8; ++j)
            if (toupper((unsigned char)lumpinfo[i].name[j]) != key[j]) break;
        if (j == 8) return i;
    }
    return -1;
}
int W_GetNumForName(char *name)
{
    int i = W_CheckNumForName(name);
    if (i < 0) I_Error("W_GetNumForName: %s not found", name);
    return i;
}
int W_LumpLength(int lump)
{
    if (lump < 0 || lump >= numlumps) I_Error("W_LumpLength: invalid lump %d",lump);
    return lumpinfo[lump].size;
}
void W_ReadLump(int lump, void *dest)
{
    lumpinfo_t *l;
    W_LumpLength(lump);
    l = &lumpinfo[lump];
    if ((size_t)l->position > wadlength ||
        (size_t)l->size > wadlength-(size_t)l->position)
        I_Error("W_ReadLump: invalid bounds");
    memcpy(dest, wadbytes+l->position, (size_t)l->size);
}
void *W_CacheLumpNum(int lump, int tag)
{
    int size = W_LumpLength(lump);
    if (!lumpcache[lump])
    {
        Z_Malloc(size,tag,&lumpcache[lump]);
        W_ReadLump(lump,lumpcache[lump]);
    }
    else Z_ChangeTag(lumpcache[lump],tag);
    return lumpcache[lump];
}
void *W_CacheLumpName(char *name, int tag)
{
    return W_CacheLumpNum(W_GetNumForName(name),tag);
}
