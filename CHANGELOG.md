# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and the project still uses a beta-style release line.

## [0.7.1-beta] - 2026-03-11

### Added

- BM25 / FTS-backed ranked search as part of the indexed search path.
- `curd test` graph and cohesion audit flow for semantic integrity checks.
- Canonical operation types and shared validation/routing groundwork for native and MCP tool surfaces.
- Profile-based runtime behavior in `settings.toml`, including multi-profile capability definitions.
- `.curd` script authoring support with:
  - `arg`
  - `let`
  - multiline strings
  - `sequence`
  - `atomic`
  - structured explainability comments
- `curd run check <file.curd>` for script preflight and graph-adjacent safeguard reporting.
- `curd run compile <file.curd>` to emit compiled plan artifacts under `.curd/plans/`.
- `curd plan edit <id>` for interactive editing of compiled artifact defaults.
- Compiled script artifacts now carry:
  - source hash
  - source path
  - bound arguments
  - explainability metadata
  - safeguards
  - runtime ceiling context
- Modular plan extension scaffolding in `curd-core`, including:
  - disclosure
  - plan agent modules
  - review modules
  - parallel-plan scaffolding
  - runtime plan scaffolding
- Public and local docs for:
  - control-plane interfaces
  - scripts and plans
  - tools and profiles
  - plugin extensions
  - package managers
- Reference extension assets for:
  - `tree-sitter-curd`
  - a `.curdl` CURD language package
  - a `.curdt` native tool package
- Broader build adapter coverage:
  - `poetry`
  - `pip`
  - `conda`
  - `mamba`
- Package-manager asset generation for:
  - Homebrew
  - Winget
  - Scoop
  - Chocolatey
  - Pixi
  - Mise
  - `uvx`
  - `pipx`
  - `bunx`
  - `npx`

### Changed

- MCP moved out of `curd-core` and into the `curd` control-plane layer.
- CLI, REPL, and MCP paths now share much more of the same validation and routing behavior.
- `curd-core` is closer to a real substrate crate, with transport concerns moved outward.
- `lite` and `full` now act more like runtime ceilings while profiles drive actual behavior.
- Native plugin management gained dedicated CLI commands:
  - `plugin-language`
  - `plugin-tool`
  - `plugin-trust`
- Installed `.curdl` language packages can now influence actual build adapter detection through `build_system`.
- Public docs and README now present CURD as:
  - a local coding workflow tool
  - a governed agentic backend
  instead of only an MCP-centric engine story.
- Public docs site content in `../curd-docs` was updated to match the `0.7.1` architecture and workflow model.
- `curd-self-docs` was updated to mirror the current `CURD-v0.7` tree by default instead of an older repo path.

### Fixed

- Shadow workspace auditing no longer points at the generic `.curd/shadow` parent when a concrete active shadow workspace is required.
- Shadow-root storage paths are isolated more cleanly from live workspace cache state.
- Fresh workspaces no longer report bogus zero-symbol integrity audits in the new `curd test` flow.
- `verify_impact --strict` now behaves more like regression checking instead of a misleading single absolute threshold.
- BM25/FTS index maintenance no longer rebuilds the FTS index on every storage open.
- REPL `test` alias argument splitting no longer collapses flags into one token.
- Native and MCP docs/schemas for `verify_impact`, `execute_dsl`, `execute_plan`, `simulate`, and plugin tools were updated to match current behavior.
- Mutating plan and DSL execution now consistently require active workspace sessions.
- Nested tool calls inside plan and DSL execution now flow through the shared validation path instead of bypassing profile and ceiling checks.
- Batch execution no longer bypasses validation for nested tool calls.
- Plugin systems now obey global plugin disablement more consistently.
- `.curdt` plugins can no longer shadow reserved native CURD tool names.
- Plugin trust and install/remove mutation are now human-only policy actions.
- More runtime and engine paths now return structured errors instead of panicking on lock, path, or pipe assumptions.
- The previously failing `curd` test suite drift that blocked full verification on this machine was fixed, and the minimum verification loop is green again.

### Security and hardening

- Plan execution gained tighter guardrails for:
  - duplicate node rejection
  - retry bounds
  - output limit bounds
  - unsupported internal command rejection
  - non-human `clear_shadow` denial
- Shell execution was hardened with:
  - background-execution policy enforcement
  - foreground timeout enforcement
  - workspace-bound `cwd` validation
  - background task count limits
- Raw custom build commands now respect policy gating instead of bypassing it in direct core execution.
- Additional live `unwrap()`/panic-style paths were removed from core runtime paths, especially around:
  - shell
  - search
  - parser
  - read
  - plugin clients
  - context routing
  - config mutation

## [0.7.0-beta] - 2026-03-10

### Added

- Cascading policy engine with blocklists and allowlists.
- Mandatory plan gating for mutation-capable workflows when configured.
- Task-based build execution via `settings.toml`.
- Configuration hashing for policy integrity.
- CLI and REPL aliases for high-frequency commands.

## [0.6.0-beta] - 2026-03-06

### Added

- Core semantic engine across Rust, Python, TypeScript, JavaScript, C, C++, Java, and Go using Tree-sitter.
- Scalable search and indexing foundation.
- Call-graph and dependency-graph extraction.
- Sandbox-backed symbol mutation and refactoring.
- Context engine and isolated planning support.
- Python and Node bindings.
- Integrated LSP diagnostics support.

### Fixed

- Parser memory-boundary stabilization and safer fallback behavior under contention.
- More aggressive harvesting of zombie processes and stale handles.
- CLI nesting edge cases that previously led to infinite retry loops.

### Changed

- Promoted earlier alpha work into the beta runtime line.
- Relicensed the project under GPLv3.
- Reorganized shell and support scripts into the `scripts/` directory.
