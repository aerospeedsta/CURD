#!/bin/bash
set -e

# CURD macOS Packaging Script
# Requires: cargo, pkgbuild

VERSION=$(grep '^version' curd/Cargo.toml | head -1 | cut -d '"' -f 2)
IDENTIFIER="com.curd.cli"
INSTALL_LOCATION="/usr/local/bin"
PAYLOAD_DIR="pkg/payload"
SCRIPTS_DIR="pkg/scripts"
PKG_NAME="curd-${VERSION}.pkg"

echo "--- Packaging CURD v${VERSION} ---"

# 1. Binary Source
INPUT_BINARY=$1
if [ -z "$INPUT_BINARY" ]; then
    echo "No binary provided. Building local release binary..."
    cargo build --release --workspace
    INPUT_BINARY="target/release/curd"
fi

# 2. Prepare Payload
echo "Preparing payload from $INPUT_BINARY..."
rm -rf "$PAYLOAD_DIR"
mkdir -p "$PAYLOAD_DIR"
cp "$INPUT_BINARY" "$PAYLOAD_DIR/curd"

# 3. Create Component Package
echo "Building component package..."
if [ -z "$VERSION" ]; then
    echo "ERROR: Could not detect version from curd/Cargo.toml"
    exit 1
fi

COMPONENT_PKG="curd-core.pkg"
pkgbuild --root "$PAYLOAD_DIR" \
         --identifier "$IDENTIFIER" \
         --version "$VERSION" \
         --install-location "$INSTALL_LOCATION" \
         --scripts "$SCRIPTS_DIR" \
         "$COMPONENT_PKG"

# 4. Create Distribution Package (The interactive one)
echo "Synthesizing distribution definition..."
productbuild --synthesize --package "$COMPONENT_PKG" "distribution.xml"

echo "Building final distribution package..."
productbuild --distribution "distribution.xml" \
             --package-path "." \
             "$(pwd)/$PKG_NAME"

rm "$COMPONENT_PKG" "distribution.xml"

echo "--- SUCCESS: Generated $PKG_NAME ---"
echo "You can now distribute this interactive .pkg file."
