# CURD Plugin Extensions

This document covers the two signed plugin package formats CURD accepts:

- `.curdl`: language ecosystem packages
- `.curdt`: native tool packages

Both are packaged as signed JSON archives and installed through CURD's plugin control-plane tools.

For the broader docs set, see:

- [docs/README.md](README.md)
- [GETTING_STARTED.md](GETTING_STARTED.md)
- [TOOLS_AND_PROFILES.md](TOOLS_AND_PROFILES.md)

## `.curdl` language packages

Language packages extend CURD with:

- file-extension to language mapping
- native grammar library loading
- optional query bundle
- optional metadata for:
  - `build_system`
  - `lsp_adapter`
  - `debug_adapter`

The manifest starter lives at:

- [lang_plugin.curdl.template.json](../curd-core/templates/lang_plugin.curdl.template.json)

The native grammar Makefile starter lives at:

- [plugin_makefile.mk](../curd-core/src/bin/templates/plugin_makefile.mk)
- [tree-sitter-curd reference grammar](../examples/tree-sitter-curd/README.md)
- [CURD language extension reference package](../examples/curd-language-extension/README.md)

### What happens on install

When you install a signed `.curdl` archive via `plugin_language add`:

1. CURD verifies the archive extension, manifest kind, payload hashes, and signature.
2. The payload is installed under `.curd/plugins/lang/<package_id>/`.
3. CURD rebuilds `.curd/plugins/lang/languages.toml`.
4. The grammar registry picks up the new language for matching file extensions.

### Build-system metadata

`language.build_system` is now used as adapter metadata during build detection.

If a workspace contains files matching the language plugin's declared extensions, CURD will:

- consider the plugin's `build_system` during `curd build` adapter auto-detection
- surface the plugin-derived build system in `curd init` workspace analysis

For that to execute successfully, the build system must resolve to either:

- a built-in adapter name, or
- a matching `[build.adapters.<name>]` entry in `settings.toml`

Example:

```toml
[build.adapters.acme-build]
detect_files = ["acme.build"]
steps = [["acme-build", "compile"]]
```

If a `.curdl` plugin declares `build_system = "acme-build"` and the workspace contains `*.acme` files, CURD can auto-select that adapter.

## `.curdt` native tool packages

Tool packages extend CURD with signed sandboxed sidecar tools.

They are:

- installed under `.curd/plugins/tool/<package_id>/`
- invoked as sandboxed subprocesses
- required to speak `json_stdio_v1`
- surfaced in docs and MCP/native tool listings with their manifest metadata

The manifest starter lives at:

- [tool_plugin.curdt.template.json](../curd-core/templates/tool_plugin.curdt.template.json)
- [CURD native tool extension reference package](../examples/curd-native-tool-extension/README.md)

### What happens on install

When you install a signed `.curdt` archive via `plugin_tool add`:

1. CURD verifies the archive extension, manifest kind, payload hashes, and signature.
2. The payload is installed under `.curd/plugins/tool/<package_id>/`.
3. CURD records the installed package metadata.
4. Native/MCP tool discovery can expose the installed tool and its manifest-derived docs.

### Runtime contract

The tool executable must:

- read one JSON object from `stdin`
- emit one JSON object on `stdout`
- avoid extra framing or log noise on `stdout`

CURD enforces:

- entrypoint path confinement to the install directory
- subprocess sandboxing
- timeout and output-size limits
- `json_stdio_v1` only

## Packaging

Both plugin kinds are packed with:

- [curd-plugin-pack.rs](../curd-core/src/bin/curd-plugin-pack.rs)

Usage:

```bash
cargo run -q -p curd-core --bin curd-plugin-pack -- \
  --manifest manifest.json \
  --payload-root payload/ \
  --out demo.curdl \
  --private-key-file signing.key
```

Swap the output extension to `.curdt` for tool plugins.

## Install flow

Installation is currently exposed through MCP/native control-plane calls:

- `plugin_language`
- `plugin_tool`
- `plugin_trust`

Trusted signing keys govern which plugin archives may be installed.

Example MCP/native calls:

```json
{"tool":"plugin_trust","args":{"action":"add","key_id":"acme","pubkey_hex":"...","allowed_kinds":["language","tool"]}}
```

```json
{"tool":"plugin_language","args":{"action":"add","archive_path":"/path/to/demo.curdl"}}
```

```json
{"tool":"plugin_tool","args":{"action":"add","archive_path":"/path/to/demo.curdt"}}
```
