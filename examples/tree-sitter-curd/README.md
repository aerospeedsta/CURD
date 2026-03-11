# tree-sitter-curd

This is a reference grammar scaffold for the future `.curd` source language.

It is intentionally small and meant to show extension authors how to structure:

- a Tree-sitter grammar repository
- CURD symbol queries
- a matching `.curdl` package layout

## What this sample covers

- `arg`
- `let`
- multiline string values
- plain tool calls
- `sequence`
- `parallel`
- `atomic`
- `abort`

## Files

- [grammar.js](grammar.js)
- [package.json](package.json)
- [corpus/basic.txt](corpus/basic.txt)
- [queries/curd.scm](queries/curd.scm)
- [queries/highlights.scm](queries/highlights.scm)

## Generate the parser

```bash
cd examples/tree-sitter-curd
npm install
npm run generate
npm test
```

## Notes

- This grammar is a reference scaffold, not a finished language definition.
- It is meant to be packaged into a `.curdl` language extension using the companion sample under `examples/curd-language-extension/`.
- Multiline strings are included because `.curd` scripts need ergonomic patch/data literals, but richer script features should still be expanded from here rather than treated as solved.
