# Examples

This directory contains reference workflows and extension samples for CURD.

## Workflows

- [session-review-loop](session-review-loop/README.md)
  Safe local mutation flow with an explicit workspace session and review gate.
- [supervised-refactor](supervised-refactor/README.md)
  Human-supervised agent workflow using search, graph, read, edit, verify, and build.
- [parallel-variant-review](parallel-variant-review/README.md)
  Compare isolated variants before promotion.

## Extension Authoring

- [tree-sitter-curd](tree-sitter-curd/README.md)
  Reference Tree-sitter grammar scaffold for `.curd` source files.
- [curd-language-extension](curd-language-extension/README.md)
  Reference `.curdl` packaging layout for the CURD language itself.
- [curd-native-tool-extension](curd-native-tool-extension/README.md)
  Reference `.curdt` packaging layout for custom native tools.

## Notes

- The workflow examples describe what works in the current codebase today.
- The `.curd` language examples show the intended human-facing script direction; the compiled execution artifact remains a plan/DSL IR.
