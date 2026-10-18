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
// Modified 2026-10-18: high-level sound queues compact events for the Canvas
// Guest's PCM mixer, and music is passed to the vendored Chocolate Doom OPL
// player. It still never reads DS*/D_* WAD payloads itself; the guest decodes
// them from the viewer-supplied WAD.
// Historical notice above retained; upstream now supplies GPL-2.0 (LICENSE.TXT).
#include "doom.h"
#include "doomstat.h"
#include "sounds.h"
#include "s_sound.h"
#include "w_wad.h"
#include "z_zone.h"

int snd_SfxVolume, snd_MusicVolume, numChannels;

static musicinfo_t *mus_playing;
static void *music_handle;

void doom_music_init(void);
void doom_music_volume(int volume);
void doom_music_pause(int paused);
void *doom_music_register(void *data,int len);
void doom_music_unregister(void *handle);
void doom_music_play(void *handle,int looping);
void doom_music_stop(void);

void S_Init(int sfx,int music)
{
    /* The cartridge has no config file; start audible rather than at the
       original default's zero and let DOOM's menu adjust it. */
    snd_SfxVolume = sfx > 0 ? sfx : 15;
    snd_MusicVolume = music > 0 ? music : 15;
    numChannels = 8;
    mus_playing = NULL;
    music_handle = NULL;
    doom_music_init();
    doom_music_volume((snd_MusicVolume * 127) / 15);
}

void S_Start(void)
{
    int mnum;
    static const int spmus[] =
    {
        mus_e3m4, mus_e3m2, mus_e3m3, mus_e1m5, mus_e2m7,
        mus_e2m4, mus_e2m6, mus_e2m5, mus_e1m9
    };

    if (gamemode == commercial)
        mnum = mus_runnin + gamemap - 1;
    else if (gameepisode < 4)
        mnum = mus_e1m1 + (gameepisode - 1) * 9 + gamemap - 1;
    else
        mnum = spmus[gamemap - 1];

    S_ChangeMusic(mnum,1);
}

void S_StartSound(void *origin,int sound)
{
    S_StartSoundAtVolume(origin,sound,snd_SfxVolume);
}

void S_StartSoundAtVolume(void *origin,int sound,int volume)
{
    (void)origin;
    if (sound <= 0 || sound >= NUMSFX) return;
    if (volume <= 0) return;
    doom_queue_sound(S_sfx[sound].name,volume);
}

void S_StopSound(void *origin) { (void)origin; }

void S_StartMusic(int music)
{
    S_ChangeMusic(music,0);
}

void S_ChangeMusic(int musicnum,int looping)
{
    musicinfo_t *music;
    char namebuf[16];
    int lump;

    if (musicnum <= mus_None || musicnum >= NUMMUSIC)
        return;

    music = &S_music[musicnum];
    if (mus_playing == music)
        return;

    S_StopMusic();

    snprintf(namebuf,sizeof(namebuf),"D_%s",music->name);
    lump = W_CheckNumForName(namebuf);
    if (lump < 0)
        return;

    music_handle = doom_music_register(
        W_CacheLumpName(namebuf,PU_STATIC),
        W_LumpLength(lump));
    if (music_handle == NULL)
        return;

    doom_music_play(music_handle,looping);
    mus_playing = music;
}

void S_StopMusic(void)
{
    if (mus_playing == NULL)
        return;
    doom_music_stop();
    if (music_handle != NULL)
        doom_music_unregister(music_handle);
    mus_playing = NULL;
    music_handle = NULL;
}

void S_PauseSound(void) { doom_music_pause(1); }
void S_ResumeSound(void) { doom_music_pause(0); }
void S_UpdateSounds(void *listener) { (void)listener; }

void S_SetMusicVolume(int volume)
{
    snd_MusicVolume = volume;
    doom_music_volume((volume * 127) / 15);
}

void S_SetSfxVolume(int volume) { snd_SfxVolume = volume; }
