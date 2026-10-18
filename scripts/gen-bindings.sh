#!/usr/bin/env bash
# Regenerate the committed Rust scaffold bindings with the offline Node toolkit.
#
# The committed cli/templates/{server,canvas}/src/pdk.rs files are the output of
# `xtp plugin init` (the rust bindgen) followed by `cargo fmt` and the CLI's
# `sync` prune (which drops the `#[no_mangle]` wrappers for Hooks the guest's
# src/lib.rs does not define). This script reproduces that pipeline with
# tools/xtp/bin/xtp-gen.mjs, rustfmt, and tools/xtp/bin/prune-pdk.mjs -- no `xtp`
# CLI, no network.
#
# By default it renders into a temp dir and diffs against the committed files
# (dry run); it only overwrites them with --write.
#
# Usage:
#   scripts/gen-bindings.sh [options]
#
# Options:
#   --version <v1|v2>   Schema directory under docs/cartridges (default: v1)
#   --guest <server|canvas|all>  Guest(s) to regenerate (default: all)
#   --write             Overwrite the committed bindings for the version:
#                       v1 -> cli/templates/<guest>/src/pdk.rs
#                       v2 -> cli/templates/v2/<guest>/pdk.rs
#   --out <dir>         Write the finalized pdk.rs for each guest under <dir>
#                       instead of diffing (no diff, no overwrite)
#   --name <name>       Plugin name passed to the rust templates (default: guest)
#   -h, --help          Show this help
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="v1"
GUEST="all"
WRITE=0
OUT_DIR=""
NAME=""

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:?--version needs a value}"; shift 2 ;;
    --guest) GUEST="${2:?--guest needs a value}"; shift 2 ;;
    --write) WRITE=1; shift ;;
    --out) OUT_DIR="${2:?--out needs a value}"; shift 2 ;;
    --name) NAME="${2:?--name needs a value}"; shift 2 ;;
    -h|--help) sed -n '2,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "gen-bindings: unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ "$VERSION" != "v1" ] && [ "$VERSION" != "v2" ]; then
  echo "gen-bindings: --version must be v1 or v2 (got '$VERSION')" >&2
  exit 2
fi
if [ "$GUEST" != "server" ] && [ "$GUEST" != "canvas" ] && [ "$GUEST" != "all" ]; then
  echo "gen-bindings: --guest must be server, canvas or all (got '$GUEST')" >&2
  exit 2
fi

command -v rustfmt >/dev/null 2>&1 || { echo "gen-bindings: rustfmt not found on PATH" >&2; exit 1; }

XTP_GEN="node tools/xtp/bin/xtp-gen.mjs"
PRUNE="node tools/xtp/bin/prune-pdk.mjs"

guests=()
[ "$GUEST" = "all" ] && guests=(server canvas) || guests=("$GUEST")

TMP="$(mktemp -d "${TMPDIR:-/tmp}/gen-bindings.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

status=0
for guest in "${guests[@]}"; do
  schema="docs/cartridges/$VERSION/$guest.xtp.yaml"
  lib="cli/templates/$guest/src/lib.rs"
  if [ "$VERSION" = "v2" ]; then
    committed="cli/templates/v2/$guest/pdk.rs"
  else
    committed="cli/templates/$guest/src/pdk.rs"
  fi
  plugin_name="${NAME:-$guest}"

  if [ ! -f "$schema" ]; then
    echo "gen-bindings: missing schema $schema" >&2
    status=1
    continue
  fi
  if [ ! -f "$lib" ]; then
    echo "gen-bindings: missing $lib (needed to prune exports)" >&2
    status=1
    continue
  fi

  scaffold="$TMP/$guest"
  # 1. Render the full scaffold (rust backend) from the schema.
  $XTP_GEN --schema "$schema" --lang rust --out "$scaffold" --name "$plugin_name"
  # 2. `cargo fmt` in the scaffold (edition 2021 from the generated Cargo.toml).
  rustfmt --edition 2021 "$scaffold/src/pdk.rs"
  # 3. Prune the wrappers the guest's src/lib.rs does not define.
  finalized="$TMP/$guest.pdk.rs"
  $PRUNE --scaffold "$scaffold" --lib "$lib" --out "$finalized"

  if [ -n "$OUT_DIR" ]; then
    dest="$OUT_DIR/$guest/pdk.rs"
    mkdir -p "$(dirname "$dest")"
    cp "$finalized" "$dest"
    echo "gen-bindings: wrote $dest"
  elif [ "$WRITE" = "1" ]; then
    cp "$finalized" "$committed"
    echo "gen-bindings: overwrote $committed"
  else
    if diff -u "$committed" "$finalized" > "$TMP/$guest.diff"; then
      echo "gen-bindings: $guest ($VERSION): identical to $committed"
    else
      echo "gen-bindings: $guest ($VERSION): DIFFERS from $committed"
      cat "$TMP/$guest.diff"
      status=1
    fi
  fi
done

exit "$status"
