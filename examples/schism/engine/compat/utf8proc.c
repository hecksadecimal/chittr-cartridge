/* See utf8proc.h. GPL-2.0; part of the Schism Tracker cartridge port. */
#include <utf8proc.h>
#include <stdlib.h>
#include <string.h>

int utf8proc_map(const uint8_t *str, size_t str_len, uint8_t **dstptr,
	utf8proc_option_t options)
{
	size_t i, length;
	uint8_t *out;

	if (!str || !dstptr)
		return -1;

	if (options & UTF8PROC_NULLTERM)
		length = strlen((const char *)str);
	else
		length = str_len;

	out = malloc(length + 1);
	if (!out)
		return -1;

	for (i = 0; i < length; i++) {
		uint8_t c = str[i];
		if ((options & UTF8PROC_CASEFOLD) && c >= 'A' && c <= 'Z')
			c = (uint8_t)(c + ('a' - 'A'));
		out[i] = c;
	}
	out[length] = 0;

	*dstptr = out;
	return (int)length;
}
