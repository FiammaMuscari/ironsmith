#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PKG_DIR="$ROOT_DIR/pkg"
DEMO_PKG_DIR="$ROOT_DIR/web/wasm_demo/pkg"

THRESHOLD="${IRONSMITH_WASM_SEMANTIC_THRESHOLD:-}"
DIMS="${IRONSMITH_WASM_SEMANTIC_DIMS:-384}"
FEATURES="wasm,generated-registry"

usage() {
  cat <<'USAGE'
Usage: ./rebuild-wasm.sh [--threshold <float>] [--dims <int>] [--features <csv>]

Examples:
  ./rebuild-wasm.sh
  ./rebuild-wasm.sh --threshold 0.90
  IRONSMITH_WASM_SEMANTIC_THRESHOLD=0.85 ./rebuild-wasm.sh

Notes:
  - With --threshold, parser-backed cards are semantically gated at parse time.
  - Default features are "wasm,generated-registry".
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --threshold)
      [[ $# -ge 2 ]] || { echo "missing value for --threshold" >&2; exit 1; }
      THRESHOLD="$2"
      shift 2
      ;;
    --dims)
      [[ $# -ge 2 ]] || { echo "missing value for --dims" >&2; exit 1; }
      DIMS="$2"
      shift 2
      ;;
    --features)
      [[ $# -ge 2 ]] || { echo "missing value for --features" >&2; exit 1; }
      FEATURES="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

cd "$ROOT_DIR"

if [[ -n "$THRESHOLD" ]]; then
  IRONSMITH_PARSER_SEMANTIC_GUARD_DEFAULT=1 \
  IRONSMITH_PARSER_SEMANTIC_THRESHOLD_DEFAULT="$THRESHOLD" \
  IRONSMITH_PARSER_SEMANTIC_DIMS_DEFAULT="$DIMS" \
  wasm-pack build --release --target web --features "$FEATURES"
else
  wasm-pack build --release --target web --features "$FEATURES"
fi

mkdir -p "$DEMO_PKG_DIR"
cp -f \
  "$PKG_DIR/ironsmith.js" \
  "$PKG_DIR/ironsmith_bg.wasm" \
  "$PKG_DIR/ironsmith.d.ts" \
  "$PKG_DIR/ironsmith_bg.wasm.d.ts" \
  "$PKG_DIR/package.json" \
  "$DEMO_PKG_DIR/"
