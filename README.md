# CURD

CURD is a high-performance, developer friendly engine enclosed in a Model Context Protocol (MCP) server designed for deep codebase analysis and manipulation. It treats code functions and classes as first-class objects, providing a unified interface for symbol indexing, dependency graph analysis, and semantic editing. CURD makes Git reasonable in an agentic world - makes the latter more human, whilst being able to keep track with the speed of agentic commits.

> [!NOTE]
 The codebase is subject to heavy refactoring at this time.
 
## Features

- **Concurrent Symbol Indexing**: Fast extraction of functions, classes, and metadata using Tree-sitter.
- **Dependency Graph**: Full call-graph analysis including callers, callees, and impact analysis.
- **Semantic Editing**: Staged transactions for code modifications with AST-range safety.
- **LSP Integration**: Real-time diagnostics and symbol resolution via language servers.
- **Cross-Language Bindings**: Native support for Python, Node.js, and Rust.
- **Low Latency**: Optimized Rust core with aggressive caching and parallel execution.
- **Cross-Platform Isolation**: Integrated sandboxing using `bwrap` (Linux) and `sandbox-exec` (macOS).

## Build Tiers & Feature Flags

CURD provides three distinct build tiers to balance capability with system footprint:

- **Core** (`--features core`): Minimal build for pure coders. Fast, lightweight, focusing on symbol analysis and local edits. No MCP server.
- **MCP** (`--features mcp`): The standard distribution for agentic use. Includes the Model Context Protocol server for integration with tools like Claude Desktop or Cursor.
- 
> [!TIP]
> Use `make core` or `make mcp` to build the specific tier you need.

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

## iagnostics Shortcuts

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

## Workspace Settings

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
- [LICENSE](LICENSE): Released under the GNU GPLv3 License.

## Platform Support & Sandboxing

CURD implements strict isolation for agentic tool calls:
- **macOS**: Native isolation via `sandbox-exec`.
- **Linux**: Native isolation via `bubblewrap`.
- **Windows**: **Warning**: Native sandboxing is not yet supported. Shell tools run without isolation.

## Project Status

- **CURD Engine:** The high-performance Rust core. It maintains the AST-native shadowstore and handles atomic, conflict-free transactions before they ever touch your Git history.
- **WikiCURD `[In Development]`:** A GPU-accelerated (GPUI) control plane. It transforms raw repository data into a navigable map of reasoning, allowing humans to audit agent traces at sub-millisecond latency.
- **curd-agent `[In Development]`:** A specialized execution harness built on the **Codex** engine. It treats code as a logic-graph to solve repository-level issues (targeting 80% on SWE-bench).

## License
Copyright (C) 2026 Aerospeedsta.

The current prototype of CURD is released under the GNU General Public License v3 (GPLv3).

> [!IMPORTANT]
    Notice of Research Phase & Future Relicensing: I am currently auditing third-party dependencies to ensure proper credit is given to the MIT/Apache projects that make this tool possible. I am also mapping the core architecture for the broader project (WikiCURD and curd-agent). The entire project will transition to a Dual-License (AGPLv3 / Commercial) or Business Source License (BSL) model in a more mature state. Until that point, the repository is in a Source-Available Research Phase; I am not currently accepting external Pull Requests or Issues.
