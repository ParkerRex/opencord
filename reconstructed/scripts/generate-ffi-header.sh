#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_NAME="discord_ffi"
CONFIG_PATH="$ROOT/crates/$CRATE_NAME/cbindgen.toml"
OUT_DIR="$ROOT/include"
OUT_HEADER="$OUT_DIR/discord_ffi.h"

if ! command -v cbindgen >/dev/null 2>&1; then
  echo "cbindgen is not installed. Install it with: cargo install cbindgen" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

(
  cd "$ROOT"
  cbindgen \
    --config "$CONFIG_PATH" \
    --crate "$CRATE_NAME" \
    --output "$OUT_HEADER"
)

echo "Generated $OUT_HEADER"
