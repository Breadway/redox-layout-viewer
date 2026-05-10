#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$SCRIPT_DIR/target/release/redox-layout-viewer"

if [[ ! -x "$BIN" ]]; then
  echo "Release binary not found. Building..."
  (cd "$SCRIPT_DIR" && cargo build --release)
fi

exec "$BIN" "$@"
