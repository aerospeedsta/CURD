# Tool Surface, Profiles, and Runtime Ceilings

CURD exposes one control plane with multiple adapters:

- CLI
- REPL
- MCP
- Node/Python bindings
- future CURD-CODEX and CURD-GPUI style interfaces

Those adapters all sit on the same validation and routing layer.

This is what keeps CURD from becoming a different product for every interface.

- the CLI is not a bypass
- MCP is not a privileged lane
- GPUI should not get special mutation powers
- agent harnesses should not invent parallel permission models

## Runtime ceilings

CURD currently has two runtime ceilings:

- `full`
- `lite`

`full` exposes the full implemented surface.

`lite` is intentionally restrictive. It allows only:

- `workspace`
- `search`
- `read`
- `edit`
- `graph`
- protocol basics

In `lite`, `workspace` is additionally limited to:

- `status`
- `list`
- `dependencies`

## Profiles

Profiles live in `settings.toml` and define actual behavior.

Typical profiles:

- `default`
- `assist`
- `supervised`
- `autonomous`

Example:

```toml
[runtime]
ceiling = "full"

[profiles.assist]
role = "assist_agent"
capabilities = ["lookup", "read"]
session_required_for_change = true
promotion = "forbidden"

[profiles.supervised]
role = "supervised_agent"
capabilities = ["lookup", "traverse", "read", "change.apply", "session.begin", "session.verify", "exec.task", "plan.execute", "review.run"]
session_required_for_change = true
promotion = "approval_required"
```

The effective rule is:

1. ceiling gates what is even possible
2. profile gates what the actor is allowed to do
3. policy can still deny the request

This is the core operating model for serious deployment:

- indie users can stay with a small default profile and just get safer workflows
- teams can run multiple agents from one binary with clearly bounded roles

## Capability atoms

CURD maps tool calls onto capability atoms.

Examples:

- `search` -> `lookup`
- `graph` -> `traverse`
- `read` -> `read`
- `edit` -> `change.apply`
- `build` -> `exec.task` or `exec.command` depending on request
- `plugin_tool` / `plugin_language` -> `plugin.manage`
- `plugin_trust` -> `plugin.trust`

This is what lets native tools and MCP tools be gated the same way.

## `tools/list`

`tools/list` is now richer than a plain name/description list.

It exposes metadata including:

- capability atom
- canonical operation
- whether the tool is available in `lite`
- session requirement hints
- approval requirement hints

Agent clients should use this instead of guessing.

This matters more than it sounds.

If a client reads `tools/list` and respects the metadata, it can adapt safely instead of learning a brittle prompt contract.

## Sessions

Mutation and runtime-affecting flows require a workspace session.

That includes:

- direct mutation tools
- mutating DSL execution
- mutating plan execution

Typical flow:

```bash
curd workspace begin
curd run fix_auth.curd
curd workspace commit
```

## Native plugin tools

Plugin management is intentionally tighter now:

- plugin runtime can be disabled entirely
- `.curdt` tools cannot shadow reserved native tool names
- plugin installation and trust mutation are human-only policy actions

## Build adapters

Built-in adapters now include:

- `cargo`
- `cmake`
- `ninja`
- `make`
- `uv`
- `poetry`
- `pip`
- `conda`
- `mamba`
- `go`
- `gradle`
- `maven`
- `bazel`
- `meson`
- `buck2`
- `npm`
- `yarn`
- `pnpm`
- `bun`
- `pixi`
- `mise`

Language plugins may also contribute build-system metadata through `.curdl` packages.

## Recommended operating profiles

For local human-first use:

- `full` ceiling
- conservative `default` profile
- explicit sessions for mutation

For supervised agents:

- `full` ceiling
- `assist` or `supervised` profile
- no unrestricted raw command execution

For aggressive internal automation:

- `full` ceiling
- explicit `autonomous` profile
- stronger policy and review thresholds

## Why this scales from indie to enterprise

The same control plane can satisfy both if the defaults are light and the boundaries are real.

For indie users:

- the benefit is safer automation, repeatability, and better code understanding

For engineering teams:

- the benefit is one execution model with profiles, sessions, plan artifacts, and clearer approval boundaries

## Related docs

- [CONTROL_PLANE_INTERFACES.md](CONTROL_PLANE_INTERFACES.md)
- [SCRIPTS_AND_PLANS.md](SCRIPTS_AND_PLANS.md)
- [PLUGIN_EXTENSIONS.md](PLUGIN_EXTENSIONS.md)
