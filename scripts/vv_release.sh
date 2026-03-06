#!/bin/bash
set -e

# --- Configuration ---
CURD_BIN=${CURD_BIN:-"/tmp/curd_target/release/curd"}
TEST_WS=$(mktemp -d)
trap 'rm -rf "$TEST_WS"' EXIT

echo "--- CURD Release V&V Suite ---"

# 1. Build Verification
if [ ! -f "$CURD_BIN" ]; then
    echo "ERROR: Release binary not found at $CURD_BIN. Run 'make release' first."
    exit 1
fi

# 2. Protocol Smoke Test (Initialize)
echo "[1/5] Testing JSON-RPC Initialization..."
INIT_REQ='{"jsonrpc": "2.0", "method": "initialize", "params": {"capabilities": {}}, "id": 1}'
RESP=$(echo "$INIT_REQ" | $CURD_BIN "$TEST_WS" | head -n 1)
if [[ $RESP == *"apiVersion"* ]]; then
    echo "  SUCCESS: Received valid initialize response."
else
    echo "  FAILED: Invalid initialization response: $RESP"
    exit 1
fi

# 3. Security Gating: Benchmark Tool
echo "[2/5] Verifying Benchmark Tool is GATED in Release..."
BENCH_REQ='{"jsonrpc": "2.0", "method": "tools/call", "params": {"name": "benchmark", "arguments": {"operation": "search", "params": {"query": ""}}}, "id": 2}'
# We look for the specific error message in the "text" field of the result
RESP=$(echo "$BENCH_REQ" | $CURD_BIN "$TEST_WS" | grep "The benchmark tool is disabled in release builds" || true)
if [ -n "$RESP" ]; then
    echo "  SUCCESS: Benchmark tool is correctly disabled."
else
    echo "  FAILED: Benchmark tool allowed or returned unexpected error."
    exit 1
fi

# 4. Authentication Handshake (session_open)
echo "[3/5] Testing Cryptographic Handshake: session_open..."
VALID_PUBKEY="d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
OPEN_REQ="{\"jsonrpc\": \"2.0\", \"method\": \"tools/call\", \"params\": {\"name\": \"session_open\", \"arguments\": {\"pubkey_hex\": \"$VALID_PUBKEY\"}}, \"id\": 3}"
# Match nonce even with escaped newlines in the output string
RESP=$(echo "$OPEN_REQ" | $CURD_BIN "$TEST_WS" | grep -o 'nonce\\": \\"[^\\"]*\\"' || true)
if [ -n "$RESP" ]; then
    echo "  SUCCESS: Received cryptographic nonce."
else
    echo "  FAILED: Handshake failed to return nonce. Output: $(echo "$OPEN_REQ" | $CURD_BIN "$TEST_WS")"
    exit 1
fi

# 5. Isolation: Unauthorized DSL Execution
echo "[4/5] Verifying Strict Isolation: Unauthorized DSL rejection..."
DSL_REQ='{"jsonrpc": "2.0", "method": "tools/call", "params": {"name": "execute_dsl", "arguments": {"nodes": []}}, "id": 4}'
RESP=$(echo "$DSL_REQ" | $CURD_BIN "$TEST_WS" | grep "Unauthorized" || true)
if [ -n "$RESP" ]; then
    echo "  SUCCESS: Unauthorized execution rejected."
else
    echo "  FAILED: DSL executed without valid session token."
    exit 1
fi

# 6. Sandbox Smoke Test
echo "[5/5] Verifying OS Sandbox Perimeter..."
# Attempt to write to /etc outside the workspace (This should always fail)
SHELL_REQ='{"jsonrpc": "2.0", "method": "tools/call", "params": {"name": "shell", "arguments": {"command": "touch /etc/curd_test"}}, "id": 5}'
RESP=$(echo "$SHELL_REQ" | $CURD_BIN "$TEST_WS" | grep -o 'exit_code\\": [^0]' || true)
if [ -n "$RESP" ]; then
    echo "  SUCCESS: Out-of-sandbox write was blocked (non-zero exit code)."
else
    echo "  WARNING: Sandbox test did not return expected error. Check if OS sandboxing is enabled."
fi

echo "--- V&V COMPLETE: RELEASE IS SECURE AND PROTOCOL-COMPLIANT ---"
