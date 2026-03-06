#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TARGET_DIR="${1:-.}"
OUT_DIR="$ROOT_DIR/.curd/benchmarks"
mkdir -p "$OUT_DIR"

echo "[bench_gpu_modes] Target Directory: $TARGET_DIR"

# Ensure detached worker is built
echo "Building curd-gpu-worker..."
cargo build --release -p curd-gpu-worker

echo "[bench_gpu_modes] Mode 1: CPU Fallback"
rm -f "$TARGET_DIR/.curd/symbol_index.bin"
# Build without gpu-embedded feature
cargo run --release -p curd -- doctor "$TARGET_DIR" \
  --report-out "$OUT_DIR/gpu_mode_cpu.json"
echo "Check CPU Hash Report at $OUT_DIR/gpu_mode_cpu.json"

echo "------------------------------------------------"

echo "[bench_gpu_modes] Mode 2: Embedded WGPU"
rm -f "$TARGET_DIR/.curd/symbol_index.bin"
# Build WITH gpu-embedded feature
cargo run --release -p curd --features curd-core/gpu-embedded -- doctor "$TARGET_DIR" \
  --report-out "$OUT_DIR/gpu_mode_embedded.json"
echo "Check Embedded GPU Report at $OUT_DIR/gpu_mode_embedded.json"

echo "------------------------------------------------"

echo "[bench_gpu_modes] Mode 3: External GPU Module"
rm -f "$TARGET_DIR/.curd/symbol_index.bin"
# Build WITH gpu-embedded feature BUT force external GPU
CURD_FORCE_EXTERNAL_GPU=1 cargo run --release -p curd --features curd-core/gpu-embedded -- doctor "$TARGET_DIR" \
  --report-out "$OUT_DIR/gpu_mode_external.json"
echo "Check External GPU Report at $OUT_DIR/gpu_mode_external.json"

echo "------------------------------------------------"
echo "Done."
