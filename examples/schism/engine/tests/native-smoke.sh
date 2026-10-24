#!/bin/sh
# Compiles the vendored tracker for the host and runs the browser backend's
# native boundary test. This does not need the WASI SDK or a module file.
#
#   sh engine/tests/native-smoke.sh
#   CC=clang OUT=/tmp/schism-smoke sh engine/tests/native-smoke.sh
#
# macOS: some installed SDKs cannot link the .tbd stubs of another. If the
# default system SDK fails, pass a matching one in SDKROOT.
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
ENGINE=$ROOT/engine
CC=${CC:-cc}
OUT=${OUT:-${TMPDIR:-/tmp}/schism-native-smoke}
mkdir -p "$OUT"

# Pick a working SDK on macOS when the caller did not name one. Try the
# default first, then the installed versions from oldest to newest.
if [ "$(uname -s)" = Darwin ] && [ -z "${SDKROOT:-}" ]; then
  printf 'int main(void) { return 0; }\n' > "$OUT/sdk-test.c"
  for sdk in "" /Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk \
      /Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk; do
    if [ -z "$sdk" ]; then
      if "$CC" "$OUT/sdk-test.c" -o "$OUT/sdk-test" >/dev/null 2>&1; then
        break
      fi
    elif [ -d "$sdk" ] && "$CC" -isysroot "$sdk" "$OUT/sdk-test.c" -o "$OUT/sdk-test" >/dev/null 2>&1; then
      SDKROOT=$sdk
      export SDKROOT
      break
    fi
  done
fi

SOURCES=
while IFS= read -r source; do
  case "$source" in
    ''|'#'*) continue ;;
  esac
  SOURCES="$SOURCES $ENGINE/vendor/$source"
done < "$ENGINE/sources.txt"

# platform/wasm_libc.c is the freestanding WASI allocator/syscall boundary and
# is linked only by build.rs. The native host supplies libc and libm.
# shellcheck disable=SC2086
"$CC" -DHAVE_CONFIG_H -D_GNU_SOURCE -std=gnu11 -O2 -fwrapv -fno-strict-aliasing \
  -Wno-implicit-function-declaration \
  -I"$ENGINE" -I"$ENGINE/vendor" -I"$ENGINE/vendor/include" -I"$ENGINE/compat" \
  "$ENGINE/platform/wasm.c" "$ENGINE/compat/utf8proc.c" \
  "$ENGINE/tests/native_smoke.c" $SOURCES -lm -o "$OUT/schism-native-smoke"

"$OUT/schism-native-smoke"
