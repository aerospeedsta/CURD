#!/bin/bash
# Wrapper to map 'cargo build' to 'cargo zigbuild build' (or just 'zigbuild')
# for tools like napi-rs that hardcode the 'build' subcommand.

COMMAND=$1
shift

if [ "$COMMAND" = "build" ]; then
    # cargo-zigbuild uses 'zigbuild' as the subcommand for building
    exec cargo-zigbuild zigbuild "$@"
else
    # Fallback to standard cargo for other commands (metadata, etc)
    exec cargo "$COMMAND" "$@"
fi
