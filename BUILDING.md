# Building CURD

This document provides instructions for compiling CURD and its various bindings from source.

## Prerequisites

To build the entire workspace, you will need the following tools installed:

- **Rust**: Latest stable version (2024 edition).
- **Python**: 3.8 or higher, with `uv` installed.
- **Node.js**: Latest LTS version, with `bun` installed.

## Build Process

The project uses a unified `Makefile` to simplify the build pipeline for all components.

### 1. Standard Build

To build the native Rust CLI and core libraries:
```bash
make build
```

For adapter-aware build planning/execution through CURD itself:
```bash
# dry-run plan
cargo run -q -p curd -- build .

# execute
cargo run -q -p curd -- build . --execute
```

### 2. Debug Build (All Bindings)

To generate non-optimized debug artifacts for the CLI, Python wheels, and Node.js native addons:
```bash
make debug
```
This invokes `uvx maturin build` for `curd-python`, so a global `maturin` install is not required.

### 3. Release Build (All Bindings)

To generate optimized release artifacts for the CLI, Python wheels, and Node.js native addons:
```bash
make release
```
This invokes `uvx maturin build --release` for `curd-python`.

If you are on Python 3.14 and hit a PyO3 compatibility error during Python wheel build, use one of:
- `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 make release`
- `PYO3_PYTHON=$(which python3.13) make release`
- or build just the CLI with `cargo build --release -p curd`.

### 4. Build Tiers

You can build specific tiers using shorthand targets:
```bash
make core    # Core features only
make mcp     # Standard MCP server
make full    # All features + GPU worker
```

### 4. System Installation

To install the `curd` CLI globally to `/usr/local/bin`:
```bash
# May require sudo depending on your permissions
sudo make install
```

### 5. Cleaning the Workspace

To remove all build artifacts, virtual environments, and temporary files:
```bash
make clean
```

## Compilation Details

### macOS Linker Configuration

When building native extensions for Python (PyO3) or Node.js (NAPI) on macOS, the linker must be configured to allow dynamic lookups for runtime symbols. While `.cargo/config.toml` is removed for a cleaner root, `maturin` and `napi-rs` handle most linking requirements. If building manually, ensure `-C link-arg=-undefined -C link-arg=dynamic_lookup` is passed to the Rust compiler.

### Grammar Management

The engine automatically manages Tree-sitter grammars. It implements a lazy-loading mechanism that downloads required `.wasm` files from the official repositories or Unpkg, caching them globally in `~/.curd/grammars/`.
