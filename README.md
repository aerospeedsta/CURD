# CURD (`v0.7.1-beta`)

> [!TIP]
> Start with the docs site: [curd.aerospeedsta.dev](https://curd.aerospeedsta.dev)

CURD is a code-intelligence control plane for humans and agents.

It helps you:

- understand a codebase structurally, not just textually
- mutate code safely inside a shadow workspace
- inspect graph impact before committing changes
- author repeatable workflows in `.curd` source scripts
- compile those workflows into governed plan artifacts

You can use CURD as:

- a serious solo coding tool
- a supervised agent backend
- an internal execution substrate for larger agentic systems

## Product surfaces around CURD

CURD is the backend and control plane.

The broader product direction around it is:

- `CURD`
  the code-intelligence, session, plan, and execution substrate
- `CURD-CODEX(future)`
  an agent-facing coding surface with CURD as the backend
- `WikiCURD(future)`
  a human-first GPUI management surface intended to bring developers and PMs closer to the real execution state of the codebase

That split is intentional:

- CURD owns correctness, routing, sessions, and code intelligence
- agent surfaces use CURD instead of bypassing it
- management surfaces stay human-first instead of turning into another opaque planning tool

## Why CURD exists

Most coding workflows still make you choose between:

- fast but sloppy local automation
- heavy but awkward governed automation

CURD tries to keep both layers:

- the source layer stays readable
- the execution layer stays explicit

That is why CURD has:

- `search`, `graph`, `read`, and `edit`
- workspace shadow sessions
- profiles and runtime ceilings
- `.curd` source scripts
- compiled plan artifacts

## What is new in `0.7.1`

`0.7.1` is the release where CURD starts to feel like a real platform instead of a loose bag of tools.

Highlights:

- BM25/FTS-backed ranked search with safer indexing behavior
- stronger separation between `curd-core` and `curd`
- shared validation across CLI, REPL, and MCP
- profile-gated runtime behavior on top of `lite` / `full` ceilings
- real `.curd` authoring flow:
  - `run check`
  - `run compile`
  - `plan edit`
  - `run`
- plugin hardening for `.curdl` and `.curdt`
- broader package-manager and launcher support
- much clearer local and public docs

See [CHANGELOG.md](CHANGELOG.md) for the full release detail.

## The short mental model

CURD has four important layers:

1. code intelligence
   
   - `search`
   - `graph`
   - `read`

2. safe mutation
   
   - `edit`
   - workspace sessions
   - shadow diffs

3. authoring
   
   - `.curd` scripts

4. governed execution
   
   - compiled plan artifacts
   - profiles
   - policy

If you only want local coding help, you can stay mostly in layers 1 and 2.

If you want agentic workflows, layers 3 and 4 become important.

## Quick start

```bash
curd init
curd doctor .

curd search alpha
curd read src/lib.rs::alpha
curd graph src/lib.rs::alpha --direction both --depth 2
```

### First safe edit

```bash
curd workspace begin
curd edit src/lib.rs::alpha --code "pub fn alpha() {}"
curd workspace diff
curd workspace rollback
```

### First `.curd` flow

```bash
curd run check fix_alpha.curd
curd run compile fix_alpha.curd
curd plan edit <plan-id>
curd workspace begin
curd run fix_alpha.curd
curd workspace commit
```

## `.curd` scripts

`.curd` is the human-facing workflow language.

It supports:

- `use profile ...`
- `use session ...`
- `arg`
- `let`
- multiline strings
- tool-call statements
- `sequence`
- `atomic`
- `abort`

It also supports structured explainability comments:

- `# explain:`
- `# why:`
- `# risk:`
- `# review:`
- `# tag:`

Compiled plan artifacts preserve that metadata for review and execution context.

## Runtime ceilings and profiles

CURD has two runtime ceilings:

- `full`
- `lite`

And profile-based capability gating in `settings.toml`.

That means:

- the runtime ceiling defines the outer boundary
- the active profile defines what the actor can do
- policy can still deny a request

This is what makes CURD usable for both:

- indie/local workflows
- enterprise agent harnesses

## Search and graph

Search is not an afterthought in CURD.

`0.7.1` adds BM25/FTS-backed ranked retrieval to improve top-down entry into a graph-shaped codebase. The graph gives structural context; ranked search gets you to the right place faster.

Use:

- `search` to enter the codebase
- `graph` to understand neighborhood and impact
- `read` to materialize the exact code you care about

## Plugins

CURD supports two signed plugin package formats:

- `.curdl` for language ecosystem packages
- `.curdt` for native tool packages

This release also wires `.curdl` `build_system` metadata into real build detection and adds clearer CLI plugin management flows.

## Installation

CURD is distributed via native OS packages, language-bound wrappers, and container images.

### Native OS Package Managers

| OS | Method | Command |
| :--- | :--- | :--- |
| **macOS / Linux** | **Homebrew** | `brew tap aerospeedsta/curd && brew install curd` |
| **Windows** | **Winget** | `winget install curd` |
| **Windows** | **Scoop** | `scoop bucket add curd https://github.com/aerospeedsta/curd-scoop.git && scoop install curd` |
| **Arch Linux** | **AUR** | `paru -S curd-bin` |
| **Debian / Ubuntu** | **APT** | `sudo dpkg -i curd_amd64.deb` (Download from GitHub Releases) |
| **Fedora / RHEL** | **DNF** | `sudo rpm -i curd_x86_64.rpm` (Download from GitHub Releases) |

### Wrapper Launchers (Instant Run)

Run CURD instantly without system-level installation using our language wrappers:

```bash
# Python (via uvx)
uvx --from curd-python curd --version

# Node.js (via bunx)
bunx --bun curd-node --version
```

### Containerized MCP

Serve the CURD MCP server via Docker:

```bash
docker run -it --rm -v $(pwd):/workspace aerospeedsta/curd:latest
```

### Language Bindings

- **Python**: `pip install curd-python`
- **Node.js**: `npm install curd-node`

For manual pre-compiled binaries and advanced setup, visit [curd.aerospeedsta.dev/setup](https://curd.aerospeedsta.dev/setup).

## Documentation

- [docs/README.md](docs/README.md)
- [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md)
- [docs/SCRIPTS_AND_PLANS.md](docs/SCRIPTS_AND_PLANS.md)
- [docs/TOOLS_AND_PROFILES.md](docs/TOOLS_AND_PROFILES.md)
- [docs/CONTROL_PLANE_INTERFACES.md](docs/CONTROL_PLANE_INTERFACES.md)
- [docs/PLUGIN_EXTENSIONS.md](docs/PLUGIN_EXTENSIONS.md)
- [docs/PACKAGE_MANAGERS.md](docs/PACKAGE_MANAGERS.md)
- [examples/README.md](examples/README.md)

## Components

- `curd-core`
  Authoritative business logic and engines.
- `curd`
  Control plane, CLI, REPL, routing, and MCP surface.
- `curd-python`
  Thin Python binding.
- `curd-node`
  Thin Node.js binding.

## Project status

CURD is now a serious backend for supervised and structured agentic coding workflows.

It is still in beta, but `0.7.1` is the point where the architecture is much more honest:

- one control plane
- safer sessions
- shared validation
- better docs
- real script-to-plan flow

It also now makes more sense as the center of a larger product family:

- CURD as the substrate
- CURD-CODEX as the agent execution surface
- WikiCURD as the GPUI management layer

That is a stronger position than treating each of those as separate disconnected tools.

## License

Copyright (C) 2026 Aerospeedsta.

The current prototype of CURD is released under the GNU General Public License v3 (GPLv3).

## No AI training

This repository and its contents are strictly prohibited from being used for training, fine-tuning, or augmenting large language models or other AI systems without explicit written permission from the author.
