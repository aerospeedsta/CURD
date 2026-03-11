# Getting Started with CURD

This guide is the shortest path from checkout to a real CURD workflow.

If you are evaluating CURD quickly, this is the most important mental model:

- search and graph are your understanding layer
- workspace sessions are your safety layer
- `.curd` is your authoring layer
- compiled plan artifacts are your governed execution layer

## What CURD is

CURD is a code-intelligence control plane with:

- symbol-aware search and reads
- graph-aware traversal and impact analysis
- workspace-shadow sessions for safe mutation
- `.curd` source scripts for authoring intent
- compiled plan artifacts for governed execution

## Install and build

### Rust

```bash
~/.cargo/bin/cargo build --release
```

### Python binding

```bash
cd curd-python
uv pip install .
```

### Node binding

```bash
cd curd-node
bun install
```

## Initialize a repo

From the root of the project you want CURD to manage:

```bash
curd init
curd doctor .
```

`curd init` creates the local `.curd/` workspace state and detects:

- source layout
- build adapters
- dependency ecosystem
- plugin-derived language/build metadata when enabled

## Two good first workflows

### Local developer workflow

Use this if you want CURD as a serious coding assistant without a lot of ceremony.

```bash
curd search alpha
curd read src/lib.rs::alpha
curd graph src/lib.rs::alpha --direction both --depth 2

curd workspace begin
curd edit src/lib.rs::alpha --code "pub fn alpha() {}"
curd workspace diff
curd workspace rollback
```

### Agent or team workflow

Use this if you want a reviewable authoring-to-execution flow.

```bash
curd run check fix_alpha.curd
curd run compile fix_alpha.curd
curd plan edit <plan-id>
curd workspace begin
curd run fix_alpha.curd
curd workspace commit
```

## First read-only workflow

Use CURD without mutation first.

```bash
curd search alpha
curd read src/lib.rs::alpha
curd graph src/lib.rs::alpha --direction both --depth 2
```

This gives you:

- a symbol hit
- a direct source read
- a quick impact neighborhood

## First safe mutation workflow

CURD requires a workspace session before mutation.

```bash
curd workspace begin
curd edit src/lib.rs::alpha --code "pub fn alpha() {}"
curd workspace diff
curd workspace rollback
```

When you want to keep the change:

```bash
curd workspace begin
curd edit src/lib.rs::alpha --code "pub fn alpha() {}"
curd workspace commit
```

## First `.curd` workflow

Author a script:

```curd
# explain: tighten alpha implementation safely
use session required

let patch = """
pub fn alpha() {
    println!("updated");
}
"""

atomic {
  edit uri="src/lib.rs::alpha" action="upsert" code=$patch
  verify_impact strict=true
}
```

Then run the authoring loop:

```bash
curd run check fix_alpha.curd
curd run compile fix_alpha.curd
curd plan edit <plan-id>
curd workspace begin
curd run fix_alpha.curd
curd workspace commit
```

## Modes and profiles

Two layers control behavior:

1. runtime ceiling
   - `full`
   - `lite`

2. profile
   - capability atoms selected in `settings.toml`

`lite` is a hard ceiling. Profiles cannot escape it.

## What makes CURD different

Most coding tools make you choose between:

- fast but sloppy local automation
- governed but heavy enterprise automation

CURD aims to keep the source layer light while making the execution layer explicit.

That means:

- local developers get repeatable safe workflows instead of prompt-only automation
- teams get sessions, profiles, plans, and reviewable execution boundaries without inventing a second platform

## Next steps

- Learn the script and plan flow in [SCRIPTS_AND_PLANS.md](SCRIPTS_AND_PLANS.md)
- Learn profiles and tool metadata in [TOOLS_AND_PROFILES.md](TOOLS_AND_PROFILES.md)
- Learn plugin authoring in [PLUGIN_EXTENSIONS.md](PLUGIN_EXTENSIONS.md)
