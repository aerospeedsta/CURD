#!/bin/bash
# CURD Containerized Test Runner
# Usage: ./scripts/test_in_container.sh <binary_path> <container_image> <platform>

BINARY_PATH=$1
IMAGE=$2
PLATFORM=$3

if [ ! -f "$BINARY_PATH" ]; then
    echo "ERROR: Binary $BINARY_PATH not found."
    exit 1
fi

echo "--- Testing $BINARY_PATH in $IMAGE ($PLATFORM) ---"

# Copy binary to a predictable name for the script
BINARY_NAME=$(basename "$BINARY_PATH")
TMP_DIR=$(mktemp -d)
cp "$BINARY_PATH" "$TMP_DIR/curd"
cp "scripts/vv_release.sh" "$TMP_DIR/vv_release.sh"

# Run the test inside the container
# We mount the temporary directory and run the V&V suite
docker run --rm \
    --platform "$PLATFORM" \
    -v "$TMP_DIR:/test" \
    -w /test \
    "$IMAGE" \
    sh -c "chmod +x curd vv_release.sh && CURD_BIN=./curd ./vv_release.sh"

RESULT=$?

rm -rf "$TMP_DIR"

if [ $RESULT -eq 0 ]; then
    echo "✅ SUCCESS: $BINARY_NAME passed V&V in $IMAGE"
else
    echo "❌ FAILED: $BINARY_NAME failed V&V in $IMAGE"
    exit 1
fi
