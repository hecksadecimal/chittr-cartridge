#!/bin/sh
# Packages one release zip of the chittr-cartridge CLI: chittr-cartridge-<version>-<target>.zip. It holds the binary, the
# README, both licenses, the v1 docs and schemas, and the examples' sources without their build output. The release
# workflow runs it once per target. It needs a shell and either zip or 7z.
#
# Usage: scripts/package-release.sh <version> <target> <binary> [out-dir]
#   <version>  the release version, without the leading v, e.g. 1.0.0
#   <target>   the Rust target triple the binary was built for, e.g. x86_64-unknown-linux-gnu
#   <binary>   the built chittr-cartridge executable (for a Windows target, the .exe)
#   [out-dir]  where the zip is written; defaults to dist/ at the repository root
set -eu

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo "usage: $0 <version> <target> <binary> [out-dir]" >&2
  exit 2
fi
VERSION=$1
TARGET=$2
BINARY=$3

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
OUT_DIR=${4:-$ROOT/dist}

case "$VERSION" in
  ''|*/*|*' '*) echo "package-release: bad version '$VERSION'" >&2; exit 2 ;;
esac
case "$TARGET" in
  ''|*/*|*' '*) echo "package-release: bad target '$TARGET'" >&2; exit 2 ;;
esac
[ -f "$BINARY" ] || { echo "package-release: $BINARY is not a file" >&2; exit 1; }

case "$TARGET" in
  *windows*) EXE=chittr-cartridge.exe ;;
  *) EXE=chittr-cartridge ;;
esac

NAME=chittr-cartridge-$VERSION-$TARGET
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
DIR=$STAGE/$NAME
mkdir -p "$DIR" "$OUT_DIR"
OUT_DIR=$(CDPATH='' cd -- "$OUT_DIR" && pwd)

cp "$BINARY" "$DIR/$EXE"
chmod 755 "$DIR/$EXE"
cp "$ROOT/README.md" "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$DIR/"
mkdir -p "$DIR/docs/cartridges"
cp -R "$ROOT/docs/cartridges/v1" "$DIR/docs/cartridges/"
# The examples' sources: no target/ directories, and no wasm, which the build script writes into each cartridge/.
(cd "$ROOT" && tar --exclude=target --exclude='*.wasm' --exclude=.DS_Store -cf - examples) |
  (cd "$DIR" && tar -xf -)

rm -f "$OUT_DIR/$NAME.zip"
if command -v zip >/dev/null 2>&1; then
  (cd "$STAGE" && zip -qrX "$OUT_DIR/$NAME.zip" "$NAME")
elif command -v 7z >/dev/null 2>&1; then
  (cd "$STAGE" && 7z a -tzip -bd -bso0 "$OUT_DIR/$NAME.zip" "$NAME" >/dev/null)
else
  echo "package-release: needs zip or 7z on the PATH" >&2
  exit 1
fi
echo "wrote $OUT_DIR/$NAME.zip"
