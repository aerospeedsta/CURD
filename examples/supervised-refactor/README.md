# Supervised Refactor

This example shows a practical supervised agent workflow using the current CLI/control plane.

## Goal

Refactor a function with graph awareness, verify impact, and run a project build before committing.

## Steps

1. Start a session.

```bash
curd session begin --root .
```

2. Map the blast radius.

```bash
curd search "validate" --root .
curd graph "src/auth.rs::validate" --depth 3 --root .
curd read "src/auth.rs::validate" --root .
```

3. Apply the refactor.

```bash
curd refactor --root . rename "src/auth.rs::validate" validate_token
```

4. Run semantic verification and build.

```bash
curd test all --root .
curd build test . --execute
```

5. Inspect the session log and commit if acceptable.

```bash
curd session log --root .
curd session review --root .
curd session commit --root .
```

## Recommended profile

Use a supervised profile that can:

- search
- traverse
- read
- change inside an active session
- run approved build/test tasks

But still requires a human to decide on final promotion/commit when appropriate.
