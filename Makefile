# CURD Monorepo Makefile
# Orchestrates builds for Rust Core/CLI, Python Bindings, and Node.js Bindings.
#
# Auto-configure PYO3_PYTHON if the local venv exists to avoid version mismatch (e.g. Python 3.14)
LOCAL_PY := $(CURDIR)/curd-python/.venv/bin/python
export PYO3_PYTHON := $(if $(wildcard $(LOCAL_PY)),$(LOCAL_PY),python3)

# Global PATH injection for Zig cross-compilation wrappers
export PATH := $(CURDIR)/scripts/bin:$(PATH)

# Cargo command
CARGO := cargo
ZIGBUILD := $(CURDIR)/scripts/cargo-zigbuild-wrapper.sh

.PHONY: all build debug release core mcp install uninstall clean test check curd-build curd-build-exec doctor doctor-fast doctor-strict doctor-lazy-compare doctor-fast-compare doctor-multiprocess doctor-profile doctor-ci bench-parser-backends

# Default target
all: release

# Rust Core and CLI
build:
	$(CARGO) build --workspace

debug:
	$(CARGO) build --workspace

debug-python:
	cd curd-python && uvx maturin build

debug-node:
	cd curd-node && bun install && bun x napi build

release:
	$(CARGO) build --workspace --release

release-python:
	cd curd-python && uvx maturin build --release --out dist

release-node:
	cd curd-node && bun install && bun x napi build --release && npm pack

# Tier-specific builds
core:
	$(CARGO) build -p curd --release --no-default-features --features core

mcp:
	$(CARGO) build -p curd --release --no-default-features --features mcp

full:
	$(CARGO) build -p curd --release --features full

# Installation: Deploy the release binary to /usr/local/bin
# Requires sudo if permissions are restricted.
install: release
	@echo "Installing CURD CLI to /usr/local/bin/curd..."
	@cp target/release/curd /usr/local/bin/curd
	@chmod +x /usr/local/bin/curd
ifeq ($(shell uname),Darwin)
	@echo "Applying macOS security tags and signing binary..."
	@xattr -dr com.apple.quarantine /usr/local/bin/curd 2>/dev/null || true
	@codesign -f -s - /usr/local/bin/curd
endif
	@echo "Installation complete. Run 'curd' to verify."

package:
	@chmod +x scripts/package_macos.sh pkg/scripts/postinstall
	@./scripts/package_macos.sh

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

# --- 4d Cross-Compile Plane (Requires: cargo-zigbuild, zig) ---
# Targets: Linux(GNU), Linux(MUSL), Darwin, Windows, FreeBSD
# Archs: x86_64, aarch64
#
# Note: Using zigbuild provides the sysroots for C dependencies (like wasmtime) 
# automatically for all targets, including FreeBSD.

ZIGBUILD := $(CURDIR)/scripts/cargo-zigbuild-wrapper.sh

# Meta target
release-all: release-linux release-linux-musl release-darwin release-windows release-freebsd

# Linux (glibc)
release-linux: release-linux-x86 release-linux-arm
release-linux-x86:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target x86_64-unknown-linux-gnu
release-linux-arm:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target aarch64-unknown-linux-gnu

# Linux (musl - Static)
release-linux-musl: release-linux-musl-x86 release-linux-musl-arm
release-linux-musl-x86:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target x86_64-unknown-linux-musl
release-linux-musl-arm:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target aarch64-unknown-linux-musl

# macOS (Darwin)
release-darwin: release-darwin-x86 release-darwin-arm
release-darwin-x86:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target x86_64-apple-darwin
release-darwin-arm:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target aarch64-apple-darwin

# Windows (GNU Toolchain)
release-windows: release-windows-x86 release-windows-arm
release-windows-x86:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target x86_64-pc-windows-gnu
release-windows-arm:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target aarch64-pc-windows-gnullvm

# FreeBSD
release-freebsd: release-freebsd-x86
release-freebsd-x86:
	$(ZIGBUILD) build --release -p curd --no-default-features --features "mcp" --target x86_64-unknown-freebsd

# --- OCI / Docker / Podman builds ---
# --- 4d Distribution & Packaging Plane ---
# Output: ./dist/ (Binary artifacts, packages, wheels, and npm packs)

DIST_DIR := $(CURDIR)/dist
ARCHS := x86_64 aarch64

.PHONY: dist dist-cli dist-windows dist-python dist-node dist-prep dist-arch dist-oci dist-deb dist-rpm dist-freebsd dist-nix dist-win-installer map test-dist

dist: dist-prep
	$(MAKE) -j2 dist-cli dist-windows dist-win-installer dist-python dist-node dist-arch dist-oci dist-deb dist-rpm dist-freebsd dist-nix
	@$(MAKE) map

# Windows Interactive Installer (.exe)
dist-win-installer: release-windows-x86
	@echo "Generating Windows NSIS installer..."
	@if command -v makensis >/dev/null; then \
		makensis scripts/installer.nsi; \
		mkdir -p $(DIST_DIR)/cli/windows; \
		mv curd-setup-x64.exe $(DIST_DIR)/cli/windows/; \
	else \
		echo "Skipping Windows installer (makensis not found)"; \
	fi

# Nix Flake generation
dist-nix:
	@echo "Generating Nix Flake..."
	@mkdir -p $(DIST_DIR)/nix
	@echo '{ \
  description = "CURD: Universal Semantic Control Plane"; \
  inputs = { nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable"; }; \
  outputs = { self, nixpkgs }: let \
    system = "x86_64-linux"; \
    pkgs = nixpkgs.legacyPackages.$${system}; \
  in { \
    packages.$${system}.default = pkgs.rustPlatform.buildRustPackage { \
      pname = "curd"; \
      version = "0.6.0-beta"; \
      src = ./.; \
      cargoLock.lockFile = ./Cargo.lock; \
    }; \
  }; \
}' > $(DIST_DIR)/nix/flake.nix

dist-deb: release-linux-x86
	@echo "Generating Debian packages... (Requires cargo-deb)"
	# cargo-deb -p curd --target x86_64-unknown-linux-gnu --output $(DIST_DIR)/cli/linux-glibc/curd_x86_64.deb

# RedHat/Fedora Package builder (.rpm)
dist-rpm: release-linux-x86
	@echo "Generating RPM packages... (Requires cargo-generate-rpm)"
	# cargo-generate-rpm -p curd --target x86_64-unknown-linux-gnu --output $(DIST_DIR)/cli/linux-glibc/curd_x86_64.rpm

# FreeBSD Package builder (.txz)
dist-freebsd: release-freebsd-x86
	@echo "Packaging FreeBSD binaries..."
	@mkdir -p $(DIST_DIR)/cli/freebsd
	@tar -czf $(DIST_DIR)/cli/freebsd/curd-freebsd-x86_64.tar.gz -C target/x86_64-unknown-freebsd/release curd

# Arch Linux Distro Package builder
dist-arch: release-linux-x86
	@echo "Generating Arch Linux PKGBUILDs and building native package..."
	@mkdir -p $(DIST_DIR)/arch/curd-bin $(DIST_DIR)/arch/curd-git
	# Populate templates with current version
	$(eval VERSION=$(shell grep '^version' curd/Cargo.toml | head -1 | cut -d '"' -f 2 | tr '-' '_'))
	@sed "s/\$${VERSION}/$(VERSION)/g" scripts/PKGBUILD-bin.template > $(DIST_DIR)/arch/curd-bin/PKGBUILD
	@sed "s/\$${VERSION}/$(VERSION)/g" scripts/PKGBUILD-git.template > $(DIST_DIR)/arch/curd-git/PKGBUILD
	# Build the -bin package locally via Docker using our fresh binary
	@cp target/x86_64-unknown-linux-gnu/release/curd scripts/curd-x86_64
	@echo "pkgname=curd-bin" > scripts/PKGBUILD
	@echo "pkgver=$(VERSION)" >> scripts/PKGBUILD
	@echo "pkgrel=1" >> scripts/PKGBUILD
	@echo "arch=('x86_64')" >> scripts/PKGBUILD
	@echo "provides=('curd')" >> scripts/PKGBUILD
	@echo "conflicts=('curd')" >> scripts/PKGBUILD
	@echo "source=('curd-x86_64')" >> scripts/PKGBUILD
	@echo "sha256sums=('SKIP')" >> scripts/PKGBUILD
	@echo 'package() { install -Dm755 "$$srcdir/curd-x86_64" "$$pkgdir/usr/bin/curd"; }' >> scripts/PKGBUILD
	@docker run --rm --privileged --platform linux/amd64 --security-opt seccomp=unconfined -v $(CURDIR):/workspace -w /workspace archlinux:latest bash -c "\
		pacman -Syu --noconfirm --disable-sandbox base-devel binutils sudo && \
		useradd -m builduser && \
		chown -R builduser:builduser /workspace && \
		sudo -u builduser bash -c 'cd scripts && makepkg -f --nocheck' && \
		mv scripts/curd-bin-*.pkg.tar.zst dist/arch/curd-bin/"
	@rm -f scripts/PKGBUILD scripts/curd-x86_64

# Automated post-build tests in containers and natively
test-dist: dist-prep
	@echo "Starting exhaustive post-build validation..."
	@chmod +x scripts/test_in_container.sh scripts/vv_release.sh
	
	# 1. macOS Native Tests (Host)
	@echo "Testing macOS Universal Binary (Native ARM64)..."
	@CURD_BIN=$(DIST_DIR)/cli/macos/curd ./scripts/vv_release.sh
	@if [ "$$(uname -m)" = "arm64" ]; then \
		echo "Testing macOS Universal Binary (x86_64 via Rosetta)..."; \
		CURD_BIN=$(DIST_DIR)/cli/macos/curd arch -x86_64 ./scripts/vv_release.sh; \
	fi
	
	# 2. Linux x86_64 (GLIBC/Ubuntu)
	@./scripts/test_in_container.sh $(DIST_DIR)/cli/linux-glibc/curd-x86_64 ubuntu:latest linux/amd64
	
	# 3. Linux x86_64 (MUSL/Alpine)
	@./scripts/test_in_container.sh $(DIST_DIR)/cli/linux-musl/curd-x86_64-static alpine:latest linux/amd64
	
	# 4. Linux aarch64 (GLIBC/Ubuntu - Emulated)
	@./scripts/test_in_container.sh $(DIST_DIR)/cli/linux-glibc/curd-aarch64 arm64v8/ubuntu:latest linux/arm64
	
	# 5. Linux aarch64 (MUSL/Alpine - Emulated)
	@./scripts/test_in_container.sh $(DIST_DIR)/cli/linux-musl/curd-aarch64-static arm64v8/alpine:latest linux/arm64
	
	# 6. Windows x64 (via Wine if available)
	@if command -v wine64 >/dev/null; then \
		echo "Testing Windows x64 binary via Wine..."; \
		wine64 $(DIST_DIR)/cli/windows/curd-x-64.exe --version; \
	else \
		echo "Skipping Windows x64 test (wine64 not found)"; \
	fi

	@echo "All accessible platform tests passed."

# Multi-arch OCI Images (x86_64 and arm64)
dist-oci: dist-cli
	@echo "Building Multi-arch OCI images via Docker Buildx..."
	@mkdir -p $(DIST_DIR)/cli/oci
	# We copy binaries to a temp context to keep Dockerfile simple
	@cp target/x86_64-unknown-linux-musl/release/curd $(DIST_DIR)/cli/linux-musl/curd-x86_64-static
	@cp target/aarch64-unknown-linux-musl/release/curd $(DIST_DIR)/cli/linux-musl/curd-aarch64-static
	# Buildx create if not exists
	@docker buildx create --name curd-builder --use || true
	# Build for both architectures and save as a local tarball
	docker buildx build --platform linux/amd64,linux/arm64 \
		-t curd:latest \
		-f Dockerfile \
		--output type=oci,dest=$(DIST_DIR)/cli/oci/curd-multiarch.oci.tar \
		$(DIST_DIR)/cli/linux-musl/

dist-clean:
	@rm -rf $(DIST_DIR)

dist-prep:
	@mkdir -p $(DIST_DIR)/cli $(DIST_DIR)/python $(DIST_DIR)/node $(DIST_DIR)/arch
	@chmod +x scripts/*.sh 2>/dev/null || true
	@chmod +x pkg/scripts/* 2>/dev/null || true
	@if [ $$(ulimit -n) -lt 4096 ]; then \
		echo "\033[33mWarning: Low file descriptor limit ($$(ulimit -n)). Linker might fail with ProcessFdQuotaExceeded.\033[0m"; \
		echo "\033[33mRun 'ulimit -n 10000' in your shell to fix.\033[0m"; \
	fi

dist-cli: release-darwin-x86 release-darwin-arm release-linux-x86 release-linux-arm release-linux-musl-x86 release-linux-musl-arm
	@echo "Packaging CLI for Darwin and Linux..."
	# macOS Universal Binary & PKG
	@mkdir -p $(DIST_DIR)/cli/macos
	@lipo -create target/x86_64-apple-darwin/release/curd \
	             target/aarch64-apple-darwin/release/curd \
	             -output $(DIST_DIR)/cli/macos/curd
	@scripts/package_macos.sh $(DIST_DIR)/cli/macos/curd
	@mv curd-*.pkg $(DIST_DIR)/cli/macos/
	
	# Linux GLIBC (deb/rpm/tar)
	@mkdir -p $(DIST_DIR)/cli/linux-glibc
	@cp target/x86_64-unknown-linux-gnu/release/curd $(DIST_DIR)/cli/linux-glibc/curd-x86_64
	@cp target/aarch64-unknown-linux-gnu/release/curd $(DIST_DIR)/cli/linux-glibc/curd-aarch64
	@tar -czf $(DIST_DIR)/cli/linux-glibc/curd-linux-x86_64.tar.gz -C $(DIST_DIR)/cli/linux-glibc curd-x86_64
	@tar -czf $(DIST_DIR)/cli/linux-glibc/curd-linux-aarch64.tar.gz -C $(DIST_DIR)/cli/linux-glibc curd-aarch64
	
	# Linux MUSL (static binary)
	@mkdir -p $(DIST_DIR)/cli/linux-musl
	@cp target/x86_64-unknown-linux-musl/release/curd $(DIST_DIR)/cli/linux-musl/curd-x86_64-static
	@cp target/aarch64-unknown-linux-musl/release/curd $(DIST_DIR)/cli/linux-musl/curd-aarch64-static
	@tar -czf $(DIST_DIR)/cli/linux-musl/curd-linux-x86_64-static.tar.gz -C $(DIST_DIR)/cli/linux-musl curd-x86_64-static
	@tar -czf $(DIST_DIR)/cli/linux-musl/curd-linux-aarch64-static.tar.gz -C $(DIST_DIR)/cli/linux-musl curd-aarch64-static

dist-windows: release-windows-x86 release-windows-arm
	@echo "Packaging Windows binaries..."
	@mkdir -p $(DIST_DIR)/cli/windows
	@cp target/x86_64-pc-windows-gnu/release/curd.exe $(DIST_DIR)/cli/windows/curd-x64.exe
	@cp target/aarch64-pc-windows-gnullvm/release/curd.exe $(DIST_DIR)/cli/windows/curd-arm64.exe
	@cp scripts/install_curd.ps1 $(DIST_DIR)/cli/windows/install.ps1
	@zip -j $(DIST_DIR)/cli/windows/curd-win-x64.zip $(DIST_DIR)/cli/windows/curd-x64.exe $(DIST_DIR)/cli/windows/install.ps1
	@zip -j $(DIST_DIR)/cli/windows/curd-win-arm64.zip $(DIST_DIR)/cli/windows/curd-arm64.exe $(DIST_DIR)/cli/windows/install.ps1
	
	# AUR templates are handled by dist-arch

dist-python:
	@echo "Packaging Python wheels for all platforms..."
	# Local host build
	cd curd-python && uvx maturin build --release --out $(DIST_DIR)/python
	# Cross-compile for Linux (x86_64 + aarch64)
	# Note: maturin uses zig to cross-compile if --zig is passed
	cd curd-python && PATH="$(CURDIR)/scripts/bin:$$PATH" uvx maturin build --release --zig --target x86_64-unknown-linux-gnu --out $(DIST_DIR)/python
	cd curd-python && PATH="$(CURDIR)/scripts/bin:$$PATH" uvx maturin build --release --zig --target aarch64-unknown-linux-gnu --out $(DIST_DIR)/python
	# MUSL wheels (Static libc)
	cd curd-python && PATH="$(CURDIR)/scripts/bin:$$PATH" uvx maturin build --release --zig --target x86_64-unknown-linux-musl --out $(DIST_DIR)/python
	cd curd-python && PATH="$(CURDIR)/scripts/bin:$$PATH" uvx maturin build --release --zig --target aarch64-unknown-linux-musl --out $(DIST_DIR)/python

dist-node:
	@echo "Packaging Node artifacts for all platforms..."
	@chmod +x scripts/cargo-zigbuild-wrapper.sh
	# Local platform build
	cd curd-node && bun x @napi-rs/cli build --release
	# Cross builds for Linux (GLIBC) using Zig Linker Injection
	cd curd-node && \
		CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$(CURDIR)/scripts/bin/x86_64-linux-gnu-gcc" \
		bun x @napi-rs/cli build --release --target x86_64-unknown-linux-gnu --platform
	cd curd-node && \
		CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$(CURDIR)/scripts/bin/aarch64-linux-gnu-gcc" \
		bun x @napi-rs/cli build --release --target aarch64-unknown-linux-gnu --platform
	# Cross builds for Linux (MUSL/Static) using Zig Linker Injection
	cd curd-node && \
		CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$(CURDIR)/scripts/bin/x86_64-linux-musl-gcc" \
		bun x @napi-rs/cli build --release --target x86_64-unknown-linux-musl --platform
	cd curd-node && \
		CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="$(CURDIR)/scripts/bin/aarch64-linux-musl-gcc" \
		bun x @napi-rs/cli build --release --target aarch64-unknown-linux-musl --platform
	# Collect all .node files into dist
	@mkdir -p $(DIST_DIR)/node
	@cp curd-node/*.node $(DIST_DIR)/node/
	cd curd-node && npm pack --pack-destination $(DIST_DIR)/node

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
