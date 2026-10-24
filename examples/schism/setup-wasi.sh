#!/bin/sh
# Optional local C toolchain, pinned and checksum verified; never installed globally.
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) platform=arm64-macos; sha=9c59398106b417f8f14913380fdf0097a8cc0ff4af9eb3ce0065a859e88d49e9 ;;
  Darwin/x86_64) platform=x86_64-macos; sha=87d27fa8adc68dee59bfbf2e22a6d34ef717c34d6bf1d8af2a56fc929d9ce0eb ;;
  Linux/aarch64|Linux/arm64) platform=arm64-linux; sha=f7e243dff54d60bcc576e94d6166b69f410f2500ae4a9ceef34315be10e77971 ;;
  Linux/x86_64) platform=x86_64-linux; sha=b761e3a0721dbae9c09a0059e5fdb2bf917d1b4a8a7b430fb3b5aafb0984b2c4 ;;
  *) echo 'Unsupported host: download WASI SDK 34 and set WASI_SDK_PATH manually.' >&2; exit 1 ;;
esac
NAME=wasi-sdk-34.0-$platform
mkdir -p "$ROOT/.tools"
if [ ! -x "$ROOT/.tools/$NAME/bin/clang" ]; then
  ARCHIVE=$(mktemp)
  trap 'rm -f "$ARCHIVE"' EXIT
  curl -fL "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/$NAME.tar.gz" -o "$ARCHIVE"
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$ARCHIVE" | awk '{print $1}')
  else
    actual=$(shasum -a 256 "$ARCHIVE" | awk '{print $1}')
  fi
  [ "$actual" = "$sha" ] || { echo 'WASI SDK checksum mismatch' >&2; exit 1; }
  tar -xzf "$ARCHIVE" -C "$ROOT/.tools"
fi
printf 'WASI SDK ready: %s\n' "$ROOT/.tools/$NAME"
printf 'Run ./build.sh; it finds this local toolchain automatically.\n'
