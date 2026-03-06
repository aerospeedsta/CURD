#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[doctor_ci] Running strict profile"
cargo run -q -p curd -- doctor . --profile ci-strict --report-out .curd/benchmarks/doctor_ci_strict.json

echo "[doctor_ci] Running lazy/full overlap gate"
cargo run -q -p curd -- doctor . \
  --index-mode lazy \
  --compare-with-full \
  --strict \
  --min-overlap-with-full 0.95 \
  --report-out .curd/benchmarks/doctor_ci_lazy_compare.json

echo "[doctor_ci] Running fast/full overlap gate"
cargo run -q -p curd -- doctor . \
  --index-mode fast \
  --compare-with-full \
  --strict \
  --min-overlap-with-full 0.90 \
  --report-out .curd/benchmarks/doctor_ci_fast_compare.json

echo "[doctor_ci] PASS"
