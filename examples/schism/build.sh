#!/bin/sh
# --engine builds the vendored Schism Tracker C engine and requires the WASI SDK.
# --scaffold needs only Rust. auto picks the engine when the WASI SDK is present.
# Modules are not a build input: the viewer picks one at runtime.
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
TARGET=wasm32-unknown-unknown
MODE=${1:-auto}
case "$MODE" in auto|--engine|--scaffold) ;; *) echo 'usage: ./build.sh [--engine|--scaffold]' >&2; exit 2 ;; esac
FEATURES=
if [ "$MODE" != --scaffold ]; then
  if [ -z "${WASI_SDK_PATH:-}" ]; then
    for sdk in "$ROOT"/.tools/wasi-sdk-34.0-*; do
      if [ -x "$sdk/bin/clang" ]; then WASI_SDK_PATH=$sdk; break; fi
    done
  fi
  if [ -n "${WASI_SDK_PATH:-}" ]; then
    export WASI_SDK_PATH
    FEATURES=engine
    echo 'Building the Schism Tracker engine (the viewer supplies modules at runtime).'
  elif [ "$MODE" = --engine ]; then
    echo 'Set WASI_SDK_PATH to an unpacked WASI SDK 34; use --scaffold for a Rust-only build.' >&2
    exit 1
  else
    echo 'Building the asset-free scaffold (no playable engine; WASI SDK not found).'
  fi
else
  echo 'Building the asset-free scaffold (no playable engine).'
fi
build() {
  if [ "$1" = canvas ] && [ -n "$FEATURES" ]; then
    (cd "$ROOT/$1" && cargo build --release --target "$TARGET" --features "$FEATURES")
  else
    (cd "$ROOT/$1" && cargo build --release --target "$TARGET")
  fi
  cp "$ROOT/$1/target/$TARGET/release/plugin.wasm" "$ROOT/cartridge/$2.wasm"
  echo "built cartridge/$2.wasm"
}
build . server
build canvas canvas
if [ -n "$FEATURES" ]; then
  size=$(wc -c < "$ROOT/cartridge/canvas.wasm" | tr -d ' ')
  [ "$size" -le 4194304 ] || { echo 'Canvas Guest exceeds the 4 MiB limit.' >&2; exit 1; }
fi
echo 'Next: chittr-cartridge check cartridge && chittr-cartridge pack cartridge'
