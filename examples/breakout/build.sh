#!/bin/sh
# Builds the Server Guest, and the Canvas Guest when the project has one, for wasm32-unknown-unknown. Each wasm is copied
# into cartridge/. Then `chittr-cartridge check cartridge` validates the result, and `chittr-cartridge pack cartridge`
# packs it into a .cartridge file. If cargo's build scripts fail to link on macOS, set SDKROOT to an older SDK first (see
# the chittr-cartridge README's Troubleshooting section).
set -eu

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
TARGET=wasm32-unknown-unknown

# build <crate-dir> <artifact>: builds the crate and copies its wasm to cartridge/<artifact>.wasm.
build() {
  (cd "$ROOT/$1" && cargo build --release --target "$TARGET")
  cp "$ROOT/$1/target/$TARGET/release/plugin.wasm" "$ROOT/cartridge/$2.wasm"
  echo "built cartridge/$2.wasm"
}

build . server
if [ -f "$ROOT/canvas/Cargo.toml" ]; then
  build canvas canvas
fi
