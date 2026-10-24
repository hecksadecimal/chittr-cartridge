/* Minimal utf8proc compatibility for the Chittr Schism port.
 * Schism only needs case folding/composition for keyboard text; the full
 * Unicode database is not part of this cartridge. ASCII folding is exact,
 * and everything else passes through unchanged. */
#ifndef SCHISM_COMPAT_UTF8PROC_H_
#define SCHISM_COMPAT_UTF8PROC_H_

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
	UTF8PROC_NULLTERM = (1 << 0),
	UTF8PROC_STABLE = (1 << 1),
	UTF8PROC_COMPAT = (1 << 2),
	UTF8PROC_COMPOSE = (1 << 3),
	UTF8PROC_DECOMPOSE = (1 << 4),
	UTF8PROC_IGNORE = (1 << 5),
	UTF8PROC_CASEFOLD = (1 << 6),
} utf8proc_option_t;

int utf8proc_map(const uint8_t *str, size_t strlen, uint8_t **dstptr,
	utf8proc_option_t options);

#ifdef __cplusplus
}
#endif

#endif
