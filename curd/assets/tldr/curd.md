# curd

> Semantic code intelligence control plane for humans and agents.
> More information: <https://curd.aerospeedsta.dev>

- Initialize a CURD workspace:
  curd init

- Search for a symbol by name:
  curd search {{symbol_name}}

- Explore the caller/callee graph of a symbol:
  curd graph {{symbol_uri}}

- Read a specific function or class:
  curd read {{symbol_uri}}

- Start a shadow workspace session for safe edits:
  curd workspace begin

- Commit shadow changes to disk:
  curd workspace commit

- Rollback shadow changes:
  curd workspace rollback
