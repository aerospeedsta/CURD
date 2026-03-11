# AGENTS Quickstart

Read this first, then read `AGENTS.md` before editing.

1. Core logic belongs in `curd-core`; transport and control plane belongs in `curd` (including MCP and routing).
2. Respect runtime and policy bounds:
   - Behavior is governed by `settings.toml` profiles.
   - `CURD_MODE` (lite/full) acts as a runtime ceiling, but specific permissions are profile-gated.
3. Do not break JSON-RPC or tool schemas:
   - always include `jsonrpc`, `id`, `api_version`.
   - errors must include `details` (nullable).
4. Keep blocking work off Tokio worker threads (use `spawn_blocking` path in CLI).
5. Preserve transaction safety:
   - no writes outside workspace root,
   - keep shadow begin/diff/commit/rollback invariants.
   - mutating tools require an active session and, if configured, an active plan.
6. `.curd` Scripting & Plans:
   - Complex workflows belong in `.curd` source scripts.
   - Execution is plan-gated (compile `.curd` -> `.curd/plans/<id>.json` -> execute).
7. If you change tool behavior, update both:
   - tool schemas and documentation
   - routing logic across native and MCP surfaces
8. Required validation before finishing:
   - `cargo fmt`
   - `cargo clippy --workspace`
   - `cargo test -q -p curd-core`
   - `cargo test -q -p curd`
   - `cargo run -p curd -- test --root .` (for semantic integrity)
9. If uncertain, default to contract stability + safety over feature expansion.
