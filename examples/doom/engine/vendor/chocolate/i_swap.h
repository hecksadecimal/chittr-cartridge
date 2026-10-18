#ifndef CHOCOLATE_I_SWAP_H
#define CHOCOLATE_I_SWAP_H
#include "m_swap.h"
/* Minimal big-endian helpers for the vendored Chocolate Doom MIDI reader. */
static inline unsigned short doom_be16(unsigned short value)
{
    return (unsigned short)((value >> 8) | (value << 8));
}
static inline unsigned int doom_be32(unsigned int value)
{
    return ((value & 0x000000ffu) << 24)
         | ((value & 0x0000ff00u) << 8)
         | ((value & 0x00ff0000u) >> 8)
         | ((value & 0xff000000u) >> 24);
}
#define SDL_SwapBE16(x) doom_be16((unsigned short)(x))
#define SDL_SwapBE32(x) doom_be32((unsigned int)(x))
#endif
