# Session Review Loop

This example shows the safest local workflow for a human or a supervised agent.

## Goal

Make a code change, inspect the shadow diff, run the session review, and only then commit back to the workspace.

## Prerequisites

- CURD built locally
- workspace already initialized with `curd init`

## Steps

1. Open a workspace session.

```bash
curd session begin --root .
```

2. Explore the target area.

```bash
curd search "Auth" --root .
curd graph "src/auth.rs::validate" --depth 2 --root .
curd read "src/auth.rs::validate" --root .
```

3. Apply the edit inside the active session.

```bash
curd edit "src/auth.rs::validate" \
  --action upsert \
  --code 'fn validate(token: &str) -> bool { !token.is_empty() }' \
  --root .
```

4. Review the staged session.

```bash
curd session review --root .
curd diff --semantic --root .
curd status .
```

5. Commit or rollback.

```bash
curd session commit --root .
```

Or:

```bash
curd session rollback --root .
```

## Why this matters

This is the baseline safe mutation loop CURD is built around:

- edits happen in shadow state first
- review runs against session deltas
- the live workspace is not mutated until commit
