/* Force-include for the original C89 source; only ISO C headers. GPL-2.0. */
#ifndef CARTRIDGE_DOOM_PORTABLE_H
#define CARTRIDGE_DOOM_PORTABLE_H
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <math.h>
/* Keep engine startup/debug text entirely local: no stdout/stderr descriptor IO.
 * Host-only native test translation units undefine these after this header. */
#define printf(...) ((int)0)
#define fprintf(...) ((int)0)
#endif
