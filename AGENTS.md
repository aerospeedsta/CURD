# AGENTS.md

Purpose: deterministic editing contract for AI agents working on this repository.

Scope: entire repository.

If any instruction here conflicts with explicit user instructions in a session, follow the user. Otherwise treat this as normative.

---

## 1. Product Intent (Non-Negotiable)

CURD is a code-intelligence control plane, script runtime, and tool-call backend for humans and agents.

Core design goals:

1. Deterministic code surgery via the Shadow Workspace transaction model.
2. Semantic structural understanding over dumb text heuristics (Tree-sitter driven).
3. Explicit governance via `settings.toml` policies, profiles, and plan-gating.
4. Repeatable workflows defined as `.curd` source scripts compiled into plan artifacts.

---

## 2. Runtime Modes and Profiles

As of v0.7, capability is governed by Profiles (`settings.toml`), not just a binary mode switch.

- `CURD_MODE` acts as a runtime ceiling (e.g., `lite` strictly caps capabilities).
- Actual access to tools, execution, and filesystem boundaries is gated by the active profile (e.g., `[profiles.default]`, `[profiles.ci_strict]`).

### 2.1 Tool Execution and Validation

All tool calls (whether originating from MCP, CLI, REPL, or nested within a Plan/DSL execution) route through a shared validation and capability-check layer. Never bypass this layer for direct core execution.

---

## 3. Architecture Intent by Crate

## 3.1 `curd-core`

Authoritative business logic, engines, and domain models live here. Transport concerns (like MCP) do not belong here.

Key engines:

- `SearchEngine`: symbol indexing, BM25/FTS ranked search, and Tree-sitter parsing.
- `ReadEngine`: URI-based reads and contextual mapping.
- `EditEngine`: symbol/module-top edits and refactoring logic.
- `GraphEngine`: dependency/call graph (edges/tree) and topological sorts.
- `WorkspaceEngine`: workspace actions and transaction flow.
- `PlanRuntime` / `PlanAgent`: execution of compiled `.curd` script artifacts.

Support modules:

- `transaction.rs`: physical shadow store (`.curd/shadow`) and commit/rollback/diff.
- `graph_audit.rs`: architectural alerts and semantic integrity post-commit.
- `sandbox.rs`: containerized and restricted shell execution (`bwrap`, macOS sandbox, Docker).
- `policy.rs`: enforcement of blocklists/allowlists.
- `plugin_packages.rs` / `plugin_client.rs`: management and bridging of `.curdl` and `.curdt` extensions.

## 3.2 `curd`

Control plane, routing, and transport layer:

1. CLI argument parsing (`run`, `plan`, `test`, `plugin-*`, etc.).
2. Shared tool routing and validation (`router.rs`, `validation.rs`).
3. Model Context Protocol (MCP) server implementation (`mcp.rs`).
4. Interactive REPL (`repl.rs`).

Heavy handlers run on `spawn_blocking`.

## 3.3 Extensibility (`curd-node`, `curd-python`, Plugins)

- Bindings (`curd-node`, `curd-python`) are thin wrappers around the core library.
- External logic should move towards the Plugin System (`.curdl` for languages, `.curdt` for native tools) rather than bloating the core binary.

---

## 4. API/Envelope Contract

For JSON-RPC / MCP responses:

- Do not silently break schema fields. Additive fields are preferred.
- Errors must be structured. In core runtime paths, return structured errors instead of panicking on lock, path, or pipe assumptions.

---

## 5. Plan and Script Contract

Workflows are authored in `.curd` scripts:

- Supported syntax: `arg`, `let`, multiline strings, `sequence`, `atomic`.
- Scripts are compiled into artifacts (`.curd/plans/<id>.json`) carrying source hashes, bound arguments, and safeguards.
- Mutating plan execution consistently requires an active workspace session.
- Nesting tool calls inside plan execution flows through standard profile/ceiling checks.

---

## 6. Transaction/Shadow Contract

`ShadowStore` invariants:

1. Never write outside workspace root.
2. `begin` creates physical shadow workspace (`.curd/shadow/root`).
3. `diff` reflects staged + implicit shadow changes.
4. `commit` performs conflict-aware writeback.
5. `rollback` cleans shadow state without mutating workspace files.
6. MANDATORY SESSIONS: Agents MUST open a session via `workspace(action: 'begin')` before calling any state-mutating tools (`edit`, `mutate`, `manage_file`, or destructive `shell` commands). Failure to do so will result in a barrier error.

Conflict behavior:

- If base hash mismatch detected, attempt three-way merge via `git merge-file`.

---

## 7. Performance/Safety Guardrails

1. Do not run blocking filesystem/process workloads on Tokio worker threads.
2. Respect `[policy.exec]` (e.g., `allow_background`, timeout enforcement, workspace-bound `cwd` validation).
3. Do not bypass policy gating for raw custom build commands.
4. Keep caches invalidated consistently after source mutations.

---

## 8. Semantic Testing

The `curd test` tool performs semantic integrity and graph cohesion audits. When implementing new features, ensure that the shadow workspace auditing points to the concrete active shadow workspace, not the generic `.curd/shadow` parent.

---

## 9. Editing Rules for Future Agents

When modifying behavior:

1. Update schemas and parameter docs if tool params change.
2. Keep method and tool dispatch behavior consistent across native and MCP surfaces.
3. Add/update tests in relevant crate.
4. Run at minimum:
   - `cargo fmt`
   - `cargo check -q`
   - `cargo test -q -p curd-core`
   - `cargo test -q -p curd`
   - `cargo run -p curd -- test --root .`

Do not:

- introduce hidden behavior changes without schema/update notes.
- weaken transaction safety or sandbox boundaries.
- PERFORM WRITES WITHOUT A SESSION: Always wrap plan executions and individual edits in a `workspace` begin/commit cycle.

End of file.
