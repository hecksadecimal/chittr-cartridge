#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
CC=${CC:-clang}
OUT=${OUT:-/tmp/doom-engine-smoke}
# Original fixed-point code intentionally wraps signed arithmetic.
# ASAN=1 enables address checks; UBSan is separate because upstream relies on
# signed shifts and other historical fixed-point idioms.
SAN=""
if [ "${ASAN:-0}" = 1 ]; then SAN="-fsanitize=address -fno-omit-frame-pointer"; fi
$CC -std=gnu99 -O1 -g -fwrapv -fno-strict-aliasing $SAN \
  -Wno-deprecated-declarations \
  -include portable.h -I. -Ivendor $(tr '\n' ' ' < sources.txt) tests/smoke.c tests/validation.c \
  -lm -o "$OUT"
export ASAN_OPTIONS=${ASAN_OPTIONS:-detect_leaks=0}
for invalid in null negative short magic empty count directory dirwrap position size sizewrap incomplete skill; do
  "$OUT" --invalid "$invalid"
done
# Public-source CI can exercise container validation without any game data.
if [ "${1:-}" = --structural-only ]; then exit 0; fi
"$OUT" --tick-error "${1:-../cartridge/assets/doom1.wad}"
"$OUT" --skill "${1:-../cartridge/assets/doom1.wad}"
"$OUT" "${1:-../cartridge/assets/doom1.wad}"
