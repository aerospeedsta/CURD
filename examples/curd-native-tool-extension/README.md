# CURD Native Tool Extension Reference

This example shows how to package a custom native tool as a signed `.curdt` archive.

## Purpose

Use this as the reference for third-party tool extension authors:

- manifest layout
- stdio protocol contract
- payload layout
- packaging flow
- trust + installation flow

## Included files

- [manifest.template.json](manifest.template.json)
- [tool.py](tool.py)
- [Makefile](Makefile)

## Expected payload layout

```text
payload/
  bin/
    curd-demo-tool
```

## Runtime contract

The executable must:

1. read exactly one JSON object from `stdin`
2. emit exactly one JSON object on `stdout`
3. avoid extra stdout logging or framing

CURD treats `.curdt` packages as signed sidecar tools. They are discovered through:

- `curd plugin-tool add ...`
- `curd plugin-tool list`
- `curd plugin-tool doc <tool>`

## Authoring flow

1. Build or write the executable.
2. Place it under `payload/bin/`.
3. Fill out the manifest template.
4. Package the archive with `curd-plugin-pack`.
5. Add trust for the signing key.
6. Install the `.curdt` archive through CURD.

## Notes

- Native plugin tools cannot reuse reserved built-in CURD tool names.
- The executable path must stay inside the package payload.
- Keep the protocol narrow. Input and output should remain single-object JSON.
