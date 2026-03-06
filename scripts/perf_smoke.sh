#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Determine binary path (handle cross-compilation target dirs)
CURD_BIN="${CURD_BIN:-$(find "$ROOT_DIR/target" -name curd -type f -path "*/release/*" | head -n 1)}"
if [ -z "$CURD_BIN" ] || [ ! -f "$CURD_BIN" ]; then
    CURD_BIN="$ROOT_DIR/target/release/curd"
fi
# Use system python if available, fallback to a guess
PYTHON_BIN="${PYTHON_BIN:-$(command -v python3 || command -v python || echo "python3")}"
TEST_WS="$(mktemp -d)"
trap 'rm -rf "$TEST_WS"' EXIT

cat > "$TEST_WS/main.rs" <<'EOF'
fn main() {
    println!("hello");
}

fn helper() -> i32 { 42 }
EOF

# echo "Building release curd binary..."
# cargo build -q --release -p curd

REQ='{"jsonrpc":"2.0","method":"tools/call","params":{"name":"benchmark","arguments":{"operation":"search","params":{"query":"main","mode":"symbol"},"iterations":5}},"id":1}'
RESP="$(echo "$REQ" | CURD_ALLOW_BENCHMARK=1 "$CURD_BIN" mcp "$TEST_WS")"

P95="$($PYTHON_BIN - "$RESP" <<'PY'
import json,sys
found=None
for line in sys.argv[1].splitlines():
    try:
        obj=json.loads(line)
        if obj.get("id") == 1:
            found=obj
            break
    except: continue
if not found: sys.exit(1)
txt=found["result"]["content"][0]["text"]
payload=json.loads(txt)
print(payload["timing_ms"]["p95"])
PY
)"

echo "search benchmark p95(ms): $P95"

# Coarse fail-safe threshold for severe regressions only.
$PYTHON_BIN - "$P95" <<'PY'
import sys
p95=float(sys.argv[1])
if p95 > 5000.0:
    print(f"Perf smoke failed: p95 too high ({p95} ms)")
    raise SystemExit(1)
print("Perf smoke passed")
PY
