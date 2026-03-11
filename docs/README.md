# CURD Docs

This directory is the local documentation spine for CURD `v0.7.1-beta`.

CURD can be approached from two angles:

- as a sharp local coding workflow tool
- as a governed backend for serious agentic execution

These docs are organized so both readers can get to the right mental model quickly.

## Choose your path

### I want to use CURD locally and move fast

Start with:

- [GETTING_STARTED.md](GETTING_STARTED.md)
- [examples/README.md](../examples/README.md)

This path is for:

- indie developers
- solo builders
- research workflows
- supervised local agent use

### I need to understand control, policy, and execution boundaries

Start with:

- [TOOLS_AND_PROFILES.md](TOOLS_AND_PROFILES.md)
- [SCRIPTS_AND_PLANS.md](SCRIPTS_AND_PLANS.md)
- [CONTROL_PLANE_INTERFACES.md](CONTROL_PLANE_INTERFACES.md)

This path is for:

- platform engineers
- internal tooling teams
- agent-harness builders
- teams evaluating governance and rollout constraints

## Documentation map

The system is documented in the same order most people discover it:

1. get it running
2. understand the control plane
3. author `.curd` scripts
4. compile and govern plan artifacts
5. extend CURD with plugins

## Start Here

- [GETTING_STARTED.md](GETTING_STARTED.md)
  Local setup, first commands, sessions, and the safest first workflow.
- [SCRIPTS_AND_PLANS.md](SCRIPTS_AND_PLANS.md)
  `.curd` source scripts, `run check`, `run compile`, `plan edit`, and execution artifacts.
- [TOOLS_AND_PROFILES.md](TOOLS_AND_PROFILES.md)
  Tool surface, runtime ceilings, agent profiles, and how policy gates execution.
- [CONTROL_PLANE_INTERFACES.md](CONTROL_PLANE_INTERFACES.md)
  How CURD-CODEX, CURD-GPUI, CLI, REPL, and MCP sit on the same control plane.
- [PLUGIN_EXTENSIONS.md](PLUGIN_EXTENSIONS.md)
  `.curdl` and `.curdt` extension packaging and installation.
- [PACKAGE_MANAGERS.md](PACKAGE_MANAGERS.md)
  Generated release assets for Homebrew, Winget, Scoop, Chocolatey, Pixi, Mise, `uvx`, `pipx`, `bunx`, and `npx`.

## Related References

- [examples/README.md](../examples/README.md)
  Workflow and extension examples.
- [README.md](../README.md)
  Project overview and top-level entry point.

## Documentation Intent

These docs are written with two audiences in mind:

- local developers who want CURD as a serious coding workflow tool
- teams building agentic harnesses on top of CURD's control plane

The local repo docs should stay operational and concrete. The future online docs site can expand them, but it should not contradict them.
