/* Freestanding libc boundary for the Schism Tracker Canvas Guest.
 *
 * The WASI SDK's libc provides formatting, memory and math routines, but its
 * syscall trampolines are not used: this module supplies host-backed memory
 * allocation and rejects all native file/clock operations.
 *
 * GPL-2.0; see ../LICENSE.
 */
#include <stddef.h>
#include <string.h>
#include <wasi/api.h>

static void *(*host_malloc)(size_t);
static void *(*host_calloc)(size_t, size_t);
static void (*host_free)(void *);
static void *(*host_realloc)(void *, size_t);

void schism_allocator_init(void *(*allocate)(size_t),
	void *(*zero)(size_t, size_t), void (*release)(void *),
	void *(*resize)(void *, size_t))
{
	host_malloc = allocate;
	host_calloc = zero;
	host_free = release;
	host_realloc = resize;
}

void *malloc(size_t size)
{
	return host_malloc ? host_malloc(size) : NULL;
}

void *calloc(size_t count, size_t size)
{
	return host_calloc ? host_calloc(count, size) : NULL;
}

void free(void *pointer)
{
	if (host_free)
		host_free(pointer);
}

void *realloc(void *pointer, size_t size)
{
	return host_realloc ? host_realloc(pointer, size) : NULL;
}

__wasi_errno_t __wasi_fd_write(__wasi_fd_t fd, const __wasi_ciovec_t *iov,
	size_t count, __wasi_size_t *written)
{
	size_t total = 0;
	size_t i;
	(void)fd;
	for (i = 0; i < count; i++)
		total += iov[i].buf_len;
	*written = total; /* Discard engine diagnostics; there is no console. */
	return 0;
}

/* The Canvas Guest has no native filesystem: the host supplies module bytes
 * through the cartridge file picker and song_load_memory(). Every path-based
 * call fails cleanly so failed config/save attempts stay harmless. */
__wasi_errno_t __wasi_fd_close(__wasi_fd_t fd)
{
	(void)fd;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_fd_fdstat_get(__wasi_fd_t fd, __wasi_fdstat_t *stat)
{
	(void)fd;
	memset(stat, 0, sizeof(*stat));
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_fd_fdstat_set_flags(__wasi_fd_t fd,
	__wasi_fdflags_t flags)
{
	(void)fd;
	(void)flags;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_fd_prestat_get(__wasi_fd_t fd,
	__wasi_prestat_t *prestat)
{
	(void)fd;
	memset(prestat, 0, sizeof(*prestat));
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_fd_prestat_dir_name(__wasi_fd_t fd, uint8_t *path,
	__wasi_size_t path_len)
{
	(void)fd;
	(void)path;
	(void)path_len;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_fd_read(__wasi_fd_t fd, const __wasi_iovec_t *iovs,
	size_t iovs_len, __wasi_size_t *read)
{
	(void)fd;
	(void)iovs;
	(void)iovs_len;
	*read = 0;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_fd_readdir(__wasi_fd_t fd, uint8_t *buf,
	__wasi_size_t buf_len, __wasi_dircookie_t cookie, __wasi_size_t *used)
{
	(void)fd;
	(void)buf;
	(void)buf_len;
	(void)cookie;
	*used = 0;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_fd_renumber(__wasi_fd_t fd, __wasi_fd_t to)
{
	(void)fd;
	(void)to;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_fd_seek(__wasi_fd_t fd, __wasi_filedelta_t offset,
	__wasi_whence_t whence, __wasi_filesize_t *result)
{
	(void)fd;
	(void)offset;
	(void)whence;
	*result = 0;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_create_directory(__wasi_fd_t fd, const char *path)
{
	(void)fd;
	(void)path;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_filestat_get(__wasi_fd_t fd,
	__wasi_lookupflags_t flags, const char *path,
	__wasi_filestat_t *stat)
{
	(void)fd;
	(void)flags;
	(void)path;
	memset(stat, 0, sizeof(*stat));
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_filestat_set_times(__wasi_fd_t fd,
	__wasi_lookupflags_t flags, const char *path, __wasi_timestamp_t atim,
	__wasi_timestamp_t mtim, __wasi_fstflags_t fst_flags)
{
	(void)fd;
	(void)flags;
	(void)path;
	(void)atim;
	(void)mtim;
	(void)fst_flags;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_link(__wasi_fd_t old_fd,
	__wasi_lookupflags_t old_flags, const char *old_path,
	__wasi_fd_t new_fd, const char *new_path)
{
	(void)old_fd;
	(void)old_flags;
	(void)old_path;
	(void)new_fd;
	(void)new_path;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_open(__wasi_fd_t fd, __wasi_lookupflags_t dirflags,
	const char *path, __wasi_oflags_t oflags, __wasi_rights_t fs_rights_base,
	__wasi_rights_t fs_rights_inheriting, __wasi_fdflags_t fdflags,
	__wasi_fd_t *opened)
{
	(void)fd;
	(void)dirflags;
	(void)path;
	(void)oflags;
	(void)fs_rights_base;
	(void)fs_rights_inheriting;
	(void)fdflags;
	*opened = 0;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_readlink(__wasi_fd_t fd, const char *path,
	uint8_t *buf, __wasi_size_t buf_len, __wasi_size_t *used)
{
	(void)fd;
	(void)path;
	(void)buf;
	(void)buf_len;
	*used = 0;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_remove_directory(__wasi_fd_t fd,
	const char *path)
{
	(void)fd;
	(void)path;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_rename(__wasi_fd_t fd, const char *old_path,
	__wasi_fd_t new_fd, const char *new_path)
{
	(void)fd;
	(void)old_path;
	(void)new_fd;
	(void)new_path;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_symlink(const char *old_path, __wasi_fd_t fd,
	const char *new_path)
{
	(void)old_path;
	(void)fd;
	(void)new_path;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_path_unlink_file(__wasi_fd_t fd, const char *path)
{
	(void)fd;
	(void)path;
	return __WASI_ERRNO_BADF;
}

__wasi_errno_t __wasi_clock_time_get(__wasi_clockid_t id,
	__wasi_timestamp_t precision, __wasi_timestamp_t *result)
{
	(void)id;
	(void)precision;
	*result = 0;
	return __WASI_ERRNO_NOSYS;
}

__wasi_errno_t __wasi_environ_get(uint8_t **environ, uint8_t *environ_buf)
{
	(void)environ;
	(void)environ_buf;
	return 0;
}

__wasi_errno_t __wasi_environ_sizes_get(__wasi_size_t *count,
	__wasi_size_t *buf_size)
{
	*count = 0;
	*buf_size = 0;
	return 0;
}

_Noreturn void __wasi_proc_exit(__wasi_exitcode_t code)
{
	(void)code;
	__builtin_trap();
}
