# `.curd` Scripts and Plan Artifacts

CURD now separates authoring from governed execution.

- `.curd` is the source format
- compiled plan artifacts are the execution contract

## Why both exist

`.curd` is better for humans and agents to author intent.

Compiled plan artifacts are better for:

- bound arguments
- explainability metadata
- safeguards
- profile/runtime context
- repeatable execution

This split is what makes CURD useful to both ends of the spectrum:

- local developers can stay in readable scripts
- teams can still insist on inspectable compiled execution artifacts

## `.curd` today

The current source language supports:

- `use profile ...`
- `use session ...`
- `arg`
- `let`
- multiline strings
- tool-call statements
- `sequence`
- `atomic`
- `abort`

Current intentional limitation:

- `parallel` is parsed but rejected at compile/execute time until the runtime has honest parallel lowering

## Explainability comments

Structured comments are preserved into compiled metadata:

- `# explain: ...`
- `# why: ...`
- `# risk: ...`
- `# review: ...`
- `# tag: ...`

Example:

```curd
# explain: tighten auth validation without changing the public entrypoint
# why: downstream callers depend on the current function name
# risk: auth and session modules are tightly connected

use session required

let patch = """
pub fn validate(token: &str) -> bool {
    !token.is_empty()
}
"""

atomic {
  edit uri="src/auth.rs::validate" action="upsert" code=$patch
  verify_impact strict=true
}
```

## Script lifecycle

### 1. Check

```bash
curd run check fix_auth.curd
```

This does not mutate.

It compiles the script and reports:

- resolved targets
- graph-adjacent impact
- conflict risk
- session requirement
- suggested safeguards

This is the step that turns a script from “idea” into “something you can reason about”.

### 2. Compile

```bash
curd run compile fix_auth.curd
```

This emits a compiled plan artifact under `.curd/plans/`.

The artifact contains:

- compiled DSL/plan payload
- source hash
- source path
- bound argument values
- explainability metadata
- safeguards
- runtime ceiling snapshot when available

### 3. Edit the plan artifact

```bash
curd plan edit <plan-id>
```

Current guided edits include:

- profile
- default `output_limit`
- per-node `output_limit`
- default `retry_limit`
- per-node `retry_limit`

This is the place to refine compiled defaults without hand-editing raw JSON.

In practice, this is where a team can add stronger execution posture after the authoring step:

- choose a stricter profile
- lower output budgets
- reduce retries
- make the plan artifact fit the environment it will run in

### 4. Execute

For mutating scripts:

```bash
curd workspace begin
curd run fix_auth.curd
curd workspace commit
```

If execution should be discarded:

```bash
curd workspace rollback
```

## Script arguments

Arguments let one source script emit multiple concrete plan artifacts.

Example:

```curd
arg target_uri: string
arg strict: bool = true

let patch = """
pub fn alpha() {}
"""

edit uri=$target_uri action="upsert" code=$patch
verify_impact strict=$strict
```

Then:

```bash
curd run check fix.curd --target-uri src/lib.rs::alpha
curd run compile fix.curd --target-uri src/lib.rs::alpha
```

Each bound argument set can produce a different concrete plan artifact.

## Trust model

- `.curd` files are normal source files
- compiled plan artifacts are the stronger execution objects

That means:

- humans and agents author `.curd`
- CURD compiles and enriches the result
- safeguards can be added after impact is visible

This is an intentionally strong separation:

- `.curd` is easy to write
- plan artifacts are easier to govern

## When to stay in `.curd`

Use `.curd` when you want:

- readable reusable workflows
- easier authoring than raw JSON
- iterative graph-aware preflight
- parameterized mutation patterns

## When to care about the plan artifact

Use the compiled artifact when you need:

- stable execution history
- explicit safeguards
- inspectable defaults
- governance-ready execution records

If you are building internal agent infrastructure, this is the part that matters most:

- the plan artifact is the execution object
- the script is the authoring source
- the gap between them is where safeguards, explainability, and policy become concrete

## Related docs

- [GETTING_STARTED.md](GETTING_STARTED.md)
- [TOOLS_AND_PROFILES.md](TOOLS_AND_PROFILES.md)
- [CONTROL_PLANE_INTERFACES.md](CONTROL_PLANE_INTERFACES.md)
