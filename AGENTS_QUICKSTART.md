# AGENTS Quickstart

Read this first, then read `AGENTS.md` before editing.

1. Core logic belongs in `curd-core`; bindings stay thin.
2. Respect runtime split:
- `CURD_MODE=lite`: only `workspace/search/read/edit/graph` (+ protocol basics).
- `full`: all features.
3. Do not break JSON-RPC envelope:
- always include `jsonrpc`, `id`, `api_version`.
- errors must include `details` (nullable).
4. Keep blocking work off Tokio worker threads (use `spawn_blocking` path in CLI).
5. Preserve transaction safety:
- no writes outside workspace root,
- keep shadow begin/diff/commit/rollback invariants.
6. Session review is workspace-instance scoped (not global singleton).
7. If you change tool behavior, update both:
- `tools/list` schema
- route/handler logic
8. If you add core feature, mirror Node/Python bindings unless explicitly CLI-only.
9. Required validation before finishing:
- `cargo fmt`
- `cargo check -q`
- `cargo test -q -p curd-core`
- `cargo test -q -p curd`
10. If uncertain, default to contract stability + safety over feature expansion.
