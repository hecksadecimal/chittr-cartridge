/* Hand-maintained configure results for the Chittr Schism Tracker port.
 * The WASI build has no autotools; keep this list minimal and portable. */
#ifndef SCHISM_BUILD_CONFIG_H_
#define SCHISM_BUILD_CONFIG_H_

#define PACKAGE_NAME "Schism Tracker"
#define PACKAGE_VERSION "20251021"
#define PACKAGE_STRING "Schism Tracker 20251021"
#define PACKAGE_TARNAME "schismtracker"
#define PACKAGE_BUGREPORT "https://github.com/schismtracker/schismtracker/issues"

/* Standard headers available in wasi-libc / hosted clang. */
#define HAVE_STDINT_H 1
#define HAVE_INTTYPES_H 1
#define HAVE_LIMITS_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_SYS_PARAM_H 1
#define HAVE_UNISTD_H 1
#define HAVE_ALLOCA_H 1
#define HAVE_ALLOCA 1
#define HAVE_STAT 1
#define HAVE_DIRENT_H 1

/* C99/C11 language features. */
#define HAVE_C99_VLAS 1
#define HAVE_C99_FAMS 1
#define HAVE_ARITHMETIC_RSHIFT 1
#define HAVE_C11_32_ATOMICS 1
#define HAVE_C11_64_ATOMICS 1
#define HAVE_PRAGMA_PACK 1

/* libc replacements that are present where we build. */
#define HAVE_STRCASECMP 1
#define HAVE_STRNCASECMP 1
#define HAVE_STRCASESTR 1
#define HAVE_ASPRINTF 1
#define HAVE_VASPRINTF 1
#define HAVE_SNPRINTF 1
#define HAVE_VSNPRINTF 1
#define HAVE_LOCALTIME_R 1

#define PRIuSZ "zu"
#define PRIxSZ "zx"
#define PRIXSZ "zX"

#define HAVE_FLOAT32 1
#define HAVE_FLOAT64 1

/* The browser backend is registered through the SDL2 backend slots; the
 * actual implementation lives in engine/platform/wasm.c and never includes
 * any SDL headers. */
#define SCHISM_SDL2 1

/* Build the portable browser/WebAssembly platform layer instead of a native
 * windowing toolkit. Set by the canvas engine build. */
#ifndef SCHISM_WASM
#define SCHISM_WASM 1
#endif

#endif

#define OPLSOURCE 2

#define VERSION "20251021"
#define SIZEOF_VOID_P 8
#define SIZEOF_FLOAT 4
#define SIZEOF_DOUBLE 8
#define SIZEOF_LONG_DOUBLE 16
