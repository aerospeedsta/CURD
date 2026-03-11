# CURD Language Extension Reference

This example shows how to package the CURD language itself as a signed `.curdl` extension.

## Purpose

Use this as the reference for third-party language extension authors:

- grammar implementation
- query bundle
- manifest layout
- packaging flow
- installation flow

## Included files

- [manifest.template.json](manifest.template.json)
- [Makefile](Makefile)

## Expected payload layout

```text
payload/
  lib/
    tree-sitter-curd.dylib
  queries/
    curd.scm
```

## Authoring flow

1. Build the grammar shared library from the `tree-sitter-curd` sample.
2. Copy the compiled library into `payload/lib/`.
3. Copy the query file into `payload/queries/`.
4. Fill out the manifest template.
5. Package the archive with `curd-plugin-pack`.
6. Add trust for the signing key.
7. Install the `.curdl` archive through CURD.

## Notes

- `build_system` is optional for the CURD language itself. For `.curd` script files it is typically not needed.
- `lsp_adapter` is shown as illustrative metadata; the current example does not ship a finished LSP implementation.
