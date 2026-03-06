# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0-beta] - 2026-03-06

### Added
- **Core Semantic Engine:** Full robust support for parsing and indexing of Rust, Python, TypeScript, JavaScript, C, C++, Java, and Go using Tree-sitter.
- **Find Engine:** Substring caching and lazy symbol resolution capable of scaling to massive monorepos.
- **Graph Engine:** Call-graph and dependency-graph extraction capable of performing topological sorts and blast-radius analysis.
- **Edit Engine:** Sandbox-backed, cross-file atomic refactoring and symbol mutation.
- **Context Engine:** Read/write state tracking and isolated agent planning.
- **Bindings:** Zero-overhead `curd-python` and `curd-node` bindings using PyO3 and N-API.
- **Language Server Protocol (LSP):** Integrated `LspEngine` for syntax and semantic diagnostics to validate semantic mutations on the fly.

### Fixed
- Stabilized parser memory boundaries and implemented safe `read_only` fallbacks for lock contention.
- Audited all code paths for zombie processes; implemented proactive file descriptor and process handle harvesting.
- Resolved exhaustive CLI nesting edge-cases leading to infinite retry loops upon incorrect parsing prompts.

### Changed
- Promoted all alpha implementations to a unified beta runtime, clearing extensive internal `TODO` markers indicating complete platform features up through Phase 7.
- **Relicensed to GPLv3:** Migrated the project from MIT to the GNU General Public License v3.
- **Repository Restructuring:** Consolidated all shell scripts into the `scripts/` directory for better organization and removed legacy `.cargo/` configuration.
