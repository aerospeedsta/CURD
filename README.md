# CURD

CURD is a high-performance Model Context Protocol (MCP) server designed for deep codebase analysis and manipulation. It treats code functions and classes as first-class objects, providing a unified interface for symbol indexing, dependency graph analysis, and semantic editing.

## Features

- **Concurrent Symbol Indexing**: Fast extraction of functions, classes, and metadata using Tree-sitter.
- **Dependency Graph**: Full call-graph analysis including callers, callees, and impact analysis.
- **Semantic Editing**: Staged transactions for code modifications with AST-range safety.
- **LSP Integration**: Real-time diagnostics and symbol resolution via language servers.
- **Cross-Language Bindings**: Native support for Python, Node.js, and Rust.
- **Low Latency**: Optimized Rust core with aggressive caching and parallel execution.

## Installation

### Python (via uv)
```bash
cd curd-python
uv pip install .
```

### Node.js (via bun)
```bash
cd curd-node
bun install
```

### Rust (source)
```bash
cargo build --release
```

## Debug vs Release Builds

When building CURD from source, the choice between debug and release builds significantly impacts performance:

- **Debug** (`cargo build`):
    - **Performance**: Slow. Includes extensive runtime checks and no optimizations.
    - **Binary**: Large. Contains full debug symbols.
    - **Use Case**: Development, debugging, and testing.
- **Release** (`cargo build --release`):
    - **Performance**: Fast. Aggressively optimized for production speed.
    - **Binary**: Small. Symbols are stripped for efficiency.
    - **Use Case**: Production usage and MCP server deployment.

## MCP Server Configuration

You can integrate CURD into any MCP client (like Claude Desktop) using the following configurations. **Note:** Ensure you use the **release** binary or installed package for best performance.

### Python (using uvx)
```json
{
  "mcpServers": {
    "curd": {
      "command": "python",
      "args": ["-c", "from curd_python import CurdEngine; CurdEngine('.').run_mcp_server()"]
    }
  }
}
```

### Node.js (using bunx/node)
```json
{
  "mcpServers": {
    "curd": {
      "command": "node",
      "args": ["-e", "const { CurdEngine } = require('curd-node'); new CurdEngine('.').run_mcp_server()"]
    }
  }
}
```

### Rust (direct binary)
```json
{
  "mcpServers": {
    "curd": {
      "command": "/absolute/path/to/curd/target/release/curd",
      "args": []
    }
  }
}
```

## Usage Examples

### Python
```python
from curd_python import CurdEngine

engine = CurdEngine(".")
results = engine.search("my_function")
print(results)
```

### Node.js
```javascript
const { CurdEngine } = require('curd-node');

const engine = new CurdEngine(".");
const results = engine.search("my_function");
console.log(results);
```

### CLI
```bash
# Start MCP server at workspace root
./target/release/curd mcp .

# Run indexing regression diagnostics
./target/release/curd doctor . --profile ci-strict

# Compare lazy-mode symbol overlap against full baseline
./target/release/curd doctor . --index-mode lazy --compare-with-full --min-overlap-with-full 0.95 --strict

# Fast-mode diagnostics with report artifact output
./target/release/curd doctor . --index-mode fast --report-out .curd/benchmarks/doctor_fast.json

# Build planning via CURD control plane (dry-run)
./target/release/curd build .

# Execute planned build commands
./target/release/curd build . --execute
```

## 4c Diagnostics Shortcuts

```bash
make doctor
make doctor-fast
make doctor-strict
make doctor-lazy-compare
make doctor-fast-compare
make doctor-profile
make doctor-ci
make bench-parser-backends
```

Index tuning envs:

```bash
CURD_INDEX_MODE=full|fast|lazy|scoped
CURD_INDEX_SCOPE=src,curd-core/src
CURD_INDEX_MAX_FILE_SIZE=524288
CURD_INDEX_CHUNK_SIZE=4096
CURD_INDEX_LARGE_FILE_POLICY=skip|skeleton|full
CURD_INDEX_EXECUTION=multithreaded|multiprocess|singlethreaded
CURD_INDEX_STALL_THRESHOLD_MS=15000
CURD_PARSER_BACKEND=wasm|native
```

Notes:
- `multiprocess` uses a worker-process path through hidden `curd index-worker` orchestration.
- `native` parser backend is an explicit preference; this build deterministically falls back to WASM if native parser is unavailable.

## Workspace Settings (4d)

CURD now supports workspace config files with precedence:
1. `settings.toml`
2. `curd.toml`
3. `CURD.toml`

Example:

```toml
[edit]
churn_limit = 0.30

[index]
mode = "fast"                    # full|fast|lazy|scoped
scope = ["src", "curd-core/src"] # used for scoped mode
max_file_size = 524288
chunk_size = 4096
large_file_policy = "skip"       # skip|skeleton|full
execution = "multithreaded"      # multithreaded|multiprocess|singlethreaded
stall_threshold_ms = 15000
parser_backend = "native"        # wasm|native
extension_language_map = { cu = "c", inc = "cpp" } # extension -> parser language

[doctor]
strict = false
profile = "ci-fast"              # ci-fast|ci-strict
max_parse_fail = 0
min_overlap_with_full = 0.90

[storage]
enabled = true
sqlite_path = ".curd/curd_state.sqlite3"
encryption_mode = "sqlcipher"    # optional, requires SQLCipher build
key_env = "CURD_DB_KEY"          # env var holding DB key

[build]
preferred_adapter = "cargo"
default_profile = "release"

[build.adapters.mybuild]
detect_files = ["foo.build"]
cwd = "."
steps = [
  ["echo", "building-{profile}"],
  ["echo", "{target}"]
]
```

Precedence model:
- CLI flags override config.
- Environment variables override config.
- Config overrides built-in defaults.

## Components

- **curd-core**: The central Rust library containing all logic.
- **curd**: A binary wrapper for the core engine.
- **curd-python**: Python bindings.
- **curd-node**: Node.js bindings.

## Documentation

- [BUILDING.md](BUILDING.md): Detailed compilation and setup instructions.
- [LICENSE](LICENSE): Released under the MIT License.

## License

This project is licensed under the MIT License.
