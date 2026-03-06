#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR=".curd/benchmarks"
mkdir -p "$OUT_DIR"

echo "[bench_parser_backends] wasm cold run"
rm -f .curd/symbol_index.bin
CURD_PARSER_BACKEND=wasm cargo run -q -p curd -- doctor . \
  --report-out "$OUT_DIR/parser_backend_wasm.json"

echo "[bench_parser_backends] native cold run"
rm -f .curd/symbol_index.bin
CURD_PARSER_BACKEND=native cargo run -q -p curd -- doctor . \
  --report-out "$OUT_DIR/parser_backend_native.json"

echo "[bench_parser_backends] done"
echo "  wasm:   $OUT_DIR/parser_backend_wasm.json"
echo "  native: $OUT_DIR/parser_backend_native.json"
