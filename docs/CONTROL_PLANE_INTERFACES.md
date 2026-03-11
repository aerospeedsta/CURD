# CURD Control Plane Interfaces

This document describes how application-facing agents should use CURD's control plane, especially for:

- CURD-CODEX style agent interfaces
- CURD-GPUI / WikiCURD style operator interfaces
- app-local agents that need a stable, bounded way to drive CURD

The goal is simple:

- one control plane
- multiple interfaces
- no bypass around safety, profiles, sessions, or policy

For the broader local docs spine, start at:

- [docs/README.md](README.md)
- [GETTING_STARTED.md](GETTING_STARTED.md)
- [SCRIPTS_AND_PLANS.md](SCRIPTS_AND_PLANS.md)
- [TOOLS_AND_PROFILES.md](TOOLS_AND_PROFILES.md)

## Core rule

All interfaces must route through the same control plane.

That means:

- CLI
- REPL
- MCP
- CURD-CODEX
- CURD-GPUI
- app-specific agent shells

should all be treated as adapters over the same validated operation path.

They must not:

- call `curd-core` business logic directly to skip validation
- bypass runtime ceiling checks
- bypass profile/capability checks
- mutate the workspace outside a session
- invent their own permission model

## Architecture

The intended flow is:

1. Interface adapter
2. Validation and normalization
3. Canonical operation routing
4. `curd-core` execution
5. Structured result / telemetry / history

In repository terms:

- `curd-core`
  owns business logic and core execution
- `curd`
  owns control-plane routing, validation, MCP, and native adapter behavior

## Interface roles

### CURD-CODEX

CURD-CODEX should be treated as an agent-facing control-plane client.

It should:

- submit tool/operation requests
- select a profile
- provide session or connection tokens
- consume structured JSON results
- use `tools/list` metadata to understand capability and approval requirements
- treat `.curd` source scripts as authoring inputs and compiled plan artifacts as governed execution artifacts

It should not:

- assume direct shell ownership
- assume it can execute plans without a workspace session
- assume tool availability outside the active runtime ceiling

Recommended behavior:

- query `tools/list`
- choose a profile
- open a connection/session
- begin a workspace session before mutating work
- use `simulate`, `search`, `graph`, `read`, `edit`, `execute_dsl`, `execute_plan`
- use `.curd` source flows as `write -> check -> compile -> edit plan -> execute`
- inspect structured errors and adapt

### CURD-GPUI / WikiCURD

CURD-GPUI should be treated as the operator-facing control-plane client.

It should:

- render state, plans, traces, diffs, review findings, graph views
- use the same routed operations as other clients
- prefer read/traverse/history/status/doctor/context/session review flows
- display profile and approval boundaries clearly

It can surface:

- active runtime ceiling
- current profile
- session state
- pending plan state
- plan checkpoints
- operation history
- review findings
- shadow diff and staged paths

It should not gain special mutation privileges just because it is visual.

## Required request context

For app agents, requests should carry the right scope explicitly when applicable:

- `profile`
- `session_token` or `connection_token`
- `actor_id` if your app tracks actor identity
- `disclosure_level` if using progressive disclosure surfaces later

If a profile is omitted, CURD falls back to `profiles.default`.

## Sessions and mutation

Mutation-capable flows must respect the session model.

Required rule:

- if a tool, plan, or DSL payload contains mutating/runtime steps, the client must open a workspace session first

Current control-plane enforcement:

- direct mutating tools are policy-checked
- `execute_dsl` and `execute_plan` now reject mutating payloads unless a shadow workspace session is active
- nested tool calls inside DSL/plan execution are validated against runtime ceiling and profile capability

App agents should assume:

- `workspace(action="begin")` before edits, file management, refactors, or runtime-affecting plan execution
- `workspace(action="commit")` or `workspace(action="rollback")` to close the change set

## Profiles and ceilings

Two layers matter:

1. runtime ceiling
   - `full`
   - `lite`

2. agent profile from `settings.toml`
   - capability atoms
   - session requirements
   - promotion mode

An interface must not treat these as advisory.

Examples:

- a profile can still be clipped by `lite`
- `full` does not mean raw shell is automatically allowed
- `exec.task` does not imply unrestricted raw command execution

## Plans and DSLs

Plans and DSL execution are not privileged bypasses.

Current hardening rules:

- plan nodes are bounded in count
- dependency fan-in is bounded
- retries are bounded
- output compaction limits are bounded
- duplicate node ids are rejected
- unsupported internal commands are rejected
- non-human plan execution cannot use `clear_shadow`
- mutating plan/DSL payloads require an active workspace session

For app agents, the safe model is:

1. simulate first when possible
2. open a workspace session
3. execute the plan or DSL
4. inspect results and history
5. commit or rollback explicitly

For `.curd` workflows, the recommended progression is:

1. author a `.curd` script
2. run a graph/safeguard preflight (`run check`)
3. compile to a plan artifact (`run compile`)
4. adjust plan defaults (`plan edit`)
5. execute through the normal session-gated control plane

## Errors and adaptation

Interfaces should treat CURD errors as structured control signals.

Important categories:

- ceiling denied
- profile denied
- session required
- policy denied
- unsupported internal command
- validation failed

Do not flatten these into generic text in the app layer.
Preserve the structured fields so the agent or UI can adapt correctly.

## Recommended integration pattern for app agents

1. Start by reading `tools/list`.
2. Select the intended profile.
3. Open a connection/session if acting as an agent.
4. Open a workspace session before any mutating or runtime plan flow.
5. Use routed operations only.
6. Persist CURD's structured outputs in the app's own state model.
7. Render approval, session, and profile boundaries explicitly in the UI.

## What app agents must never do

- call `curd-core` directly to perform business logic
- run plan payloads as a way to bypass normal tool restrictions
- assume GPUI/CODEX are privileged surfaces
- write directly into workspace files outside CURD's session flow
- convert structured policy errors into silent retries without changing scope

## Practical summary

CURD-CODEX and CURD-GPUI are not separate execution authorities.

They are interface adapters over the same control plane.

That control plane is responsible for:

- validation
- normalization
- profile gating
- runtime ceiling enforcement
- session enforcement
- policy decisions
- routing into `curd-core`

If your app agent follows that contract, it stays aligned with CURD's safety and determinism model instead of drifting into a parallel runtime.
