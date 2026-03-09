# AGENTS.md

Purpose: deterministic editing contract for AI agents working on this repository.

Scope: entire repository.

If any instruction here conflicts with explicit user instructions in a session, follow the user. Otherwise treat this as normative.

---

## 1. Product Intent (Non-Negotiable)

CURD is a code-intelligence and tool-call runtime with two operating surfaces:

1. Lite mode: minimal functional coding layer.
2. Full mode: extended analysis/debug/profile/session-review platform.

Core design goals:

1. Function-centric operations over file-centric heuristics.
2. Stable JSON-RPC/tool contracts.
3. Safe edit workflows via transaction/shadow model.
4. Explicit capability introspection and deterministic behavior.

---

## 2. Runtime Modes

Mode is selected by env var `CURD_MODE`:

- `full` (default)
- `lite`

### 2.1 Lite mode contract

Allowed methods/tools only:

- `workspace`
- `search`
- `read`
- `edit`
- `graph`
- protocol basics (`initialize`, `tools/list`, `tools/call`)

Additionally for `workspace` in Lite:

- allowed actions: `status`, `list`, `dependencies`

Everything else MUST be blocked with a structured error.

### 2.2 Full mode contract

All implemented methods/tools are available.

---

## 3. Architecture Intent by Crate

## 3.1 `curd-core`

Authoritative business logic lives here.

Key engines:

- `SearchEngine`: symbol indexing and parsing.
- `ReadEngine`: URI-based reads.
- `EditEngine`: symbol/module-top edits.
- `GraphEngine`: dependency/call graph (edges/tree).
- `DiagramEngine`: mermaid/ascii rendering.
- `ProfileEngine`: folded/ascii/speedscope + compare.
- `DebugEngine`: one-shot debug + session APIs.
- `LspEngine`: syntax/semantic diagnostics.
- `WorkspaceEngine`: workspace actions and transaction flow.
- `SessionReviewEngine`: non-git session baseline/change/review.

Support modules:

- `transaction.rs`: physical shadow store and commit/rollback/diff.
- `graph_audit.rs`: architectural alerts post-commit.
- `deps.rs`: dependency detection.
- `shell.rs`, `file.rs`, `find.rs`, `parser.rs`, `symbols.rs`.

## 3.2 `curd`

Control plane only:

1. parse JSON-RPC input
2. route to handlers
3. call core engines
4. normalize response envelope

Heavy handlers run on `spawn_blocking`.

## 3.3 `curd-node` and `curd-python`

Thin bindings only.

Rule: no independent business logic drift from `curd-core`.

---

## 4. API/Envelope Contract

Every response MUST include:

- `jsonrpc: "2.0"`
- `id`
- `api_version`

Error object MUST include:

- `code`
- `message`
- `details` (nullable)

Do not silently break schema fields. Additive fields are preferred.

---

## 5. Session Review Contract

`SessionReviewEngine` is workspace-instance scoped, not global singleton.

Required behavior:

- `session_begin`: snapshot current source files.
- `session_status`: active state + changed-file count.
- `session_changes`: per-file delta stats.
- `session_review`: findings only for session deltas.
- `session_end`: terminate active session.

Review findings are structured with:

- `severity`
- `code`
- `message`
- `file` (optional)
- `line` (optional)

Commit review gate (`workspace commit`) may block by thresholds:

- `max_high` (default 0)
- `max_medium` (optional)
- `max_low` (optional)
- `allow_high` override

---

## 6. Transaction/Shadow Contract

`ShadowStore` invariants:

1. Never write outside workspace root.
2. `begin` creates physical shadow workspace.
3. `diff` reflects staged + implicit shadow changes.
4. `commit` performs conflict-aware writeback.
5. `rollback` cleans shadow state without mutating workspace files.
6. MANDATORY SESSIONS: As of v0.5.0, agents MUST open a session via `workspace(action: 'begin')` before calling any state-mutating tools (`edit`, `mutate`, `manage_file`, or destructive `shell` commands). Failure to do so will result in a barrier error.

Conflict behavior:

- If base hash mismatch detected, attempt three-way merge via `git merge-file`.
- If `git show HEAD:<path>` unavailable (untracked/new file), fallback to empty base.

---

## 7. Debug/Profile Semantics

## 7.1 Debug

- One-shot `debug` executes a fresh process.
- Session endpoints are currently `stateless_history` unless explicitly upgraded.
- Do not claim persistent interpreter state unless implemented with persistent child process pipes.

## 7.2 Profile

- Approx path may use graph-derived folded stacks.
- Sampling capability flags must be explicit.
- Folded generation must avoid DAG path truncation (cycle detection must be stack-local).

---

## 8. Performance/Safety Guardrails

1. Do not run blocking filesystem/process workloads on Tokio worker threads.
2. Prefer metadata prefilter before hashing large trees when possible.
3. Keep caches invalidated consistently after source mutations.
4. Do not remove safety checks for workspace-bound path validation.

---

## 9. Editing Rules for Future Agents

When modifying behavior:

1. Update `tools/list` schema if tool params changed.
2. Keep method and tool dispatch behavior consistent.
3. Add/update tests in relevant crate.
4. Run at minimum:
   - `cargo fmt`
   - `cargo check -q`
   - `cargo test -q -p curd-core`
   - `cargo test -q -p curd`
5. If adding core feature, mirror in Node/Python bindings unless intentionally Full-only CLI control-plane feature.

Do not:

- introduce hidden behavior changes without schema/update notes.
- move business logic from core into bindings.
- weaken transaction safety boundaries.
- PERFORM WRITES WITHOUT A SESSION: Always wrap plan executions and individual edits in a `workspace` begin/commit cycle.

---

## 10. Composable Feature Blocks (Canonical)

Implement features as one or more of these blocks:

1. `Contract Block`
- schema, response fields, error semantics.

2. `Core Logic Block`
- implementation in `curd-core` engine.

3. `Routing Block`
- CLI method/tool routing and mode gating.

4. `Binding Block`
- Node/Python wrappers.

5. `Safety Block`
- path checks, concurrency model, conflict handling.

6. `Verification Block`
- tests + smoke command examples.

A feature is considered complete only when all relevant blocks are addressed.

---

## 11. Known Deliberate Simplifications

These are intentional unless changed explicitly:

1. Debug sessions are logical history wrappers, not persistent REPL state.
2. Some analysis/profiling outputs are approximate by design.
3. Lite mode is intentionally restrictive for simpler/safer deployment.

---

## 12. Preferred Evolution Path

When in doubt, prioritize:

1. contract stability
2. safety/reversibility
3. deterministic outputs
4. performance
5. feature breadth

End of file.
