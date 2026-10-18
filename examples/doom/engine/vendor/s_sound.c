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
// Modified 2026-10-18: silent high-level sound, no audio lump reads.
// Historical notice above retained; upstream now supplies GPL-2.0 (LICENSE.TXT).
// GPL-2.0 replacement for s_sound.c,v 1.6 1997/02/03; see ../README.md.
// Silent game sound interface: importantly, NEVER reads DS*/D_* WAD payloads.
#include "s_sound.h"
int snd_SfxVolume, snd_MusicVolume, numChannels;
void S_Init(int sfx,int music) { (void)sfx; (void)music; }
void S_Start(void) {}
void S_StartSound(void *origin,int sound) { (void)origin; (void)sound; }
void S_StartSoundAtVolume(void *origin,int sound,int volume)
{ (void)origin; (void)sound; (void)volume; }
void S_StopSound(void *origin) { (void)origin; }
void S_StartMusic(int music) { (void)music; }
void S_ChangeMusic(int music,int looping) { (void)music; (void)looping; }
void S_StopMusic(void) {}
void S_PauseSound(void) {}
void S_ResumeSound(void) {}
void S_UpdateSounds(void *listener) { (void)listener; }
void S_SetMusicVolume(int volume) { snd_MusicVolume = volume; }
void S_SetSfxVolume(int volume) { snd_SfxVolume = volume; }
