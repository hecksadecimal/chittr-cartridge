#!/bin/sh
# Builds every example's Server Guest, and its Canvas Guest where it has one, for wasm32-unknown-unknown. Each wasm is
# copied into the example's cartridge/ directory as server.wasm or canvas.wasm, which is the layout a Cartridge is
# packed from. Run it from anywhere; Rust and the wasm target suffice without game data. DOOM and Schism additionally
# run their own build.sh so their playable C engines are selected when WASI SDK 34 is available (see their READMEs).
# If cargo's build scripts fail to link on macOS, set SDKROOT to an SDK your linker knows about; the README's
# troubleshooting note says how.
#
# A guest's wasm is named after its crate's [lib] name, which the examples set to "plugin". The copy takes the
# Cartridge's own file name instead.
set -eu

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
TARGET=wasm32-unknown-unknown

# build <crate-dir> <cartridge-dir> <artifact>: builds the crate and copies its wasm to <cartridge-dir>/<artifact>.wasm.
build() {
  dir=$1
  cartridge=$2
  artifact=$3
  lib=$(awk -F'"' '/^\[lib\]/{f=1; next} /^\[/{f=0} f && /^name/{print $2; exit}' "$dir/Cargo.toml")
  [ -n "$lib" ] || { echo "$dir/Cargo.toml: no [lib] name" >&2; exit 1; }
  (cd "$dir" && cargo build --release --target "$TARGET")
  built="$dir/target/$TARGET/release/$lib.wasm"
  if [ ! -f "$built" ]; then
    echo "$built: build output not found" >&2
    exit 1
  fi
  cp "$built" "$cartridge/$artifact.wasm"
  echo "built $cartridge/$artifact.wasm"
}

for name in poll hello snake hurtle paint breakout doom schism; do
  example="$ROOT/examples/$name"
  if [ "$name" = doom ] || [ "$name" = schism ]; then
    sh "$example/build.sh"
    continue
  fi
  build "$example" "$example/cartridge" server
  if [ -f "$example/canvas/Cargo.toml" ]; then
    build "$example/canvas" "$example/cartridge" canvas
  fi
done
