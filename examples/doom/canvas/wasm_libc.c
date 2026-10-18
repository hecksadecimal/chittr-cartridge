/* Freestanding libc boundary for the DOOM Canvas Guest. No WASI host imports.
 * Formatting/memory/libm come from WASI SDK's static libc, but file descriptors
 * do not exist here. DOOM WAD reads use the engine's in-memory reader.
 * GPL-2.0; see ../engine/LICENSE.TXT. */
#include <wasi/api.h>
#include <string.h>
#include <stdlib.h>

static void *(*host_malloc)(size_t);
static void *(*host_calloc)(size_t, size_t);
static void (*host_free)(void *);
static void *(*host_realloc)(void *, size_t);
void doom_allocator_init(void *(*allocate)(size_t), void *(*zero)(size_t,size_t),
                         void (*release)(void *), void *(*resize)(void *,size_t))
{
    host_malloc = allocate; host_calloc = zero;
    host_free = release; host_realloc = resize;
}
void *malloc(size_t size) { return host_malloc ? host_malloc(size) : NULL; }
void *calloc(size_t n, size_t size) { return host_calloc ? host_calloc(n,size) : NULL; }
void free(void *pointer) { if (host_free) host_free(pointer); }
void *realloc(void *pointer,size_t size) { return host_realloc ? host_realloc(pointer,size) : NULL; }

__wasi_errno_t __wasi_fd_write(__wasi_fd_t fd, const __wasi_ciovec_t *iov,
                              size_t count, __wasi_size_t *written)
{
    size_t n = 0;
    (void)fd;
    for (size_t i = 0; i < count; ++i) n += iov[i].buf_len;
    *written = n; /* Discard upstream diagnostic prints, never native I/O. */
    return 0;
}
__wasi_errno_t __wasi_fd_close(__wasi_fd_t fd)
{ (void)fd; return __WASI_ERRNO_BADF; }
__wasi_errno_t __wasi_fd_seek(__wasi_fd_t fd, __wasi_filedelta_t offset,
                             __wasi_whence_t whence, __wasi_filesize_t *result)
{ (void)fd; (void)offset; (void)whence; *result = 0; return __WASI_ERRNO_BADF; }
__wasi_errno_t __wasi_fd_fdstat_get(__wasi_fd_t fd, __wasi_fdstat_t *stat)
{ (void)fd; memset(stat, 0, sizeof(*stat)); return __WASI_ERRNO_BADF; }
__wasi_errno_t __wasi_clock_time_get(__wasi_clockid_t id, __wasi_timestamp_t precision,
                                    __wasi_timestamp_t *result)
{ (void)id; (void)precision; *result = 0; return __WASI_ERRNO_NOSYS; }
_Noreturn void __wasi_proc_exit(__wasi_exitcode_t code)
{ (void)code; __builtin_trap(); }
