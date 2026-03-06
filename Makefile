# CURD Monorepo Makefile
# Orchestrates builds for Rust Core/CLI, Python Bindings, and Node.js Bindings.
#
# Auto-configure PYO3_PYTHON if the local venv exists to avoid version mismatch (e.g. Python 3.14)
LOCAL_PY := $(CURDIR)/curd-python/.venv/bin/python
ifeq ($(PYO3_PYTHON),)
  ifneq ($(wildcard $(LOCAL_PY)),)
    export PYO3_PYTHON := $(LOCAL_PY)
  endif
endif

.PHONY: all build debug release core mcp install uninstall clean test check curd-build curd-build-exec doctor doctor-fast doctor-strict doctor-lazy-compare doctor-fast-compare doctor-multiprocess doctor-profile doctor-ci bench-parser-backends

# Default target
all: release

# Rust Core and CLI
build:
	cargo build --workspace

debug:
	cargo build --workspace

debug-python:
	cd curd-python && uvx maturin build

debug-node:
	cd curd-node && bun install && bun x napi build

release:
	cargo build --workspace --release

release-python:
	cd curd-python && uvx maturin build --release --out dist

release-node:
	cd curd-node && bun install && bun x napi build --release && npm pack

# Tier-specific builds
core:
	cargo build -p curd --release --no-default-features --features core

mcp:
	cargo build -p curd --release --no-default-features --features mcp

full:
	cargo build -p curd --release --features full

# Installation: Deploy the release binary to /usr/local/bin
# Requires sudo if permissions are restricted.
install: release
	@echo "Installing CURD CLI to /usr/local/bin/curd..."
	@cp target/release/curd /usr/local/bin/curd
	@chmod +x /usr/local/bin/curd
	@echo "Installation complete. Run 'curd' to verify."

uninstall:
	@echo "Removing CURD CLI from /usr/local/bin/curd..."
	@rm -f /usr/local/bin/curd
	@echo "Uninstallation complete."

# Comprehensive cleanup
clean:
	cargo clean
	rm -rf .tmp .curd .curd-grammars .git_old
	rm -rf curd-python/target curd-node/node_modules curd-node/target
	rm -f curd-node/*.node curd-node/index.js curd-node/index.d.ts
	find . -name ".DS_Store" -delete
	@echo "Workspace cleaned."

# Tests and Verification
test:
	cargo test

check:
	cargo check

# 4d build control-plane
curd-build:
	cargo run -q -p curd -- build .

curd-build-exec:
	cargo run -q -p curd -- build . --execute

# 4c regression diagnostics
doctor:
	cargo run -q -p curd -- doctor .

doctor-fast:
	cargo run -q -p curd -- doctor . --profile ci-fast

doctor-strict:
	cargo run -q -p curd -- doctor . --profile ci-strict

doctor-lazy-compare:
	cargo run -q -p curd -- doctor . --index-mode lazy --compare-with-full --strict --min-overlap-with-full 0.95

doctor-fast-compare:
	cargo run -q -p curd -- doctor . --index-mode fast --compare-with-full --strict --min-overlap-with-full 0.90

doctor-multiprocess:
	cargo run -q -p curd -- doctor . --index-execution multiprocess --profile ci-fast

doctor-profile:
	cargo run -q -p curd -- doctor . --profile ci-strict --profile-index

doctor-ci:
	./scripts/doctor_ci.sh

bench-parser-backends:
	./scripts/bench_parser_backends.sh
