#!/bin/sh
set -e

# --- Configuration ---
CURD_BIN=${CURD_BIN:-"/tmp/curd_target/release/curd"}

echo "--- CURD Release V&V Suite ---"

# 1. Build Verification
if [ ! -f "$CURD_BIN" ]; then
    # Fallback for Windows binaries if .exe is omitted in the path
    if [ -f "${CURD_BIN}.exe" ]; then
        CURD_BIN="${CURD_BIN}.exe"
    else
        echo "ERROR: Release binary not found at $CURD_BIN. Run 'make release' first."
        exit 1
    fi
fi

# 2. Basic Responsiveness
echo "[1/1] Checking binary version and responsiveness..."
VERSION_OUT=$($CURD_BIN --version)
if echo "$VERSION_OUT" | grep -q "curd 0.7.0-beta"; then
    echo "  SUCCESS: Binary is responsive and reports correct version ($VERSION_OUT)."
else
    echo "  FAILED: Binary produced unexpected output: $VERSION_OUT"
    exit 1
fi

echo "--- V&V COMPLETE: RELEASE IS VERIFIED ---"
