use serde_json::{Value, json};
use std::collections::HashMap;

/// Represents detailed documentation for a CURD tool
#[derive(Debug, Clone)]
pub struct ToolDoc {
    pub description: String,
    pub parameters: Vec<ParameterDoc>,
    pub examples: Vec<ExampleDoc>,
}

#[derive(Debug, Clone)]
pub struct ParameterDoc {
    pub name: String,
    pub kind: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct ExampleDoc {
    pub label: String,
    pub arguments: Value,
}

pub struct DocEngine {
    registry: HashMap<String, ToolDoc>,
}

impl DocEngine {
    pub fn new() -> Self {
        let mut registry = HashMap::new();

        // --- search ---
        registry.insert("search".to_string(), ToolDoc {
            description: "Search for functions, classes, or symbols by name pattern. AST-aware and deterministic.".to_string(),
            parameters: vec![
                ParameterDoc { name: "query".into(), kind: "string".into(), description: "Substring match for symbol names.".into(), required: true },
                ParameterDoc { name: "kind".into(), kind: "string (enum)".into(), description: "Optional filter: 'function', 'class', 'method'.".into(), required: false },
                ParameterDoc { name: "limit".into(), kind: "integer".into(), description: "Max results to return.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Search for all classes containing 'User'".into(), arguments: json!({"query": "User", "kind": "class"}) },
            ],
        });

        // --- read ---
        registry.insert("read".to_string(), ToolDoc {
            description: "Read files, functions, or classes by URI. Supports whole files or specific symbols.".to_string(),
            parameters: vec![
                ParameterDoc { name: "uris".into(), kind: "array<string>".into(), description: "List of URIs to read.".into(), required: true },
                ParameterDoc { name: "verbosity".into(), kind: "integer".into(), description: "0=outline, 1=full source.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Read a specific function and a file".into(), arguments: json!({"uris": ["src/main.rs::main", "Cargo.toml"]}) },
            ],
        });
        registry.insert(
            "contract".to_string(),
            ToolDoc {
                description:
                    "Extract a deterministic function/class contract summary for agent traversal."
                        .to_string(),
                parameters: vec![ParameterDoc {
                    name: "uri".into(),
                    kind: "string".into(),
                    description: "Symbol URI (e.g., src/lib.rs::run).".into(),
                    required: true,
                }],
                examples: vec![ExampleDoc {
                    label: "Get a one-line contract gist".into(),
                    arguments: json!({"uri":"src/lib.rs::run"}),
                }],
            },
        );

        // --- edit ---
        registry.insert("edit".to_string(), ToolDoc {
            description: "Create, replace, or delete a function or module section via AST-aware surgical edits.".to_string(),
            parameters: vec![
                ParameterDoc { name: "uri".into(), kind: "string".into(), description: "Target URI (e.g. 'src/lib.rs::my_func') or file path.".into(), required: true },
                ParameterDoc { name: "code".into(), kind: "string".into(), description: "New source code for the section.".into(), required: true },
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "Action: 'upsert' (create/replace) or 'delete'.".into(), required: false },
                ParameterDoc { name: "adaptation_justification".into(), kind: "string".into(), description: "Technical reason for this change.".into(), required: true },
            ],
            examples: vec![
                ExampleDoc { label: "Replace a function with a new implementation".into(), arguments: json!({"uri": "src/utils.py::helper", "code": "def helper():
    print('updated')", "adaptation_justification": "Modernize helper logic."}) },
            ],
        });

        // --- graph ---
        registry.insert("graph".to_string(), ToolDoc {
            description: "Query the call/dependency graph for functions to understand impact.".to_string(),
            parameters: vec![
                ParameterDoc { name: "uris".into(), kind: "array<string>".into(), description: "Roots for the graph traversal.".into(), required: true },
                ParameterDoc { name: "direction".into(), kind: "string (enum)".into(), description: "'up' (callers), 'down' (callees), or 'both'.".into(), required: false },
                ParameterDoc { name: "depth".into(), kind: "integer".into(), description: "Traversal depth.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "See all callers of a function".into(), arguments: json!({"uris": ["src/lib.rs::execute"], "direction": "up", "depth": 2}) },
            ],
        });

        // --- workspace ---
        registry.insert("workspace".to_string(), ToolDoc {
            description: "Manage workspace state, transactions, and diffs.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "Action: 'status', 'list', 'dependencies', 'begin', 'diff', 'commit', 'rollback', 'alerts'.".into(), required: false },
                ParameterDoc { name: "proposal_id".into(), kind: "string".into(), description: "For commit: approved proposal id required unless allow_unapproved=true.".into(), required: false },
                ParameterDoc { name: "allow_unapproved".into(), kind: "boolean".into(), description: "For commit: bypass proposal approval gate for local fast iteration.".into(), required: false },
                ParameterDoc { name: "max_high".into(), kind: "integer".into(), description: "Commit review threshold for high severity findings (default 0).".into(), required: false },
                ParameterDoc { name: "max_medium".into(), kind: "integer".into(), description: "Optional commit review threshold for medium findings.".into(), required: false },
                ParameterDoc { name: "max_low".into(), kind: "integer".into(), description: "Optional commit review threshold for low findings.".into(), required: false },
                ParameterDoc { name: "allow_high".into(), kind: "boolean".into(), description: "Override high-severity review gate when true.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "List detected workspace dependencies".into(), arguments: json!({"action": "dependencies"}) },
                ExampleDoc { label: "Commit with approved proposal".into(), arguments: json!({"action":"commit","proposal_id":"00000000-0000-0000-0000-000000000000"}) },
            ],
        });

        // --- find ---
        registry.insert(
            "find".to_string(),
            ToolDoc {
                description: "Semantic grep: search for text and return enclosing AST URIs."
                    .to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "query".into(),
                        kind: "string".into(),
                        description: "Text or regex to search for.".into(),
                        required: true,
                    },
                    ParameterDoc {
                        name: "is_regex".into(),
                        kind: "boolean".into(),
                        description: "Treat query as a regular expression.".into(),
                        required: false,
                    },
                ],
                examples: vec![ExampleDoc {
                    label: "Find all usages of a secret environment variable".into(),
                    arguments: json!({"query": "API_SECRET", "is_regex": false}),
                }],
            },
        );

        // --- diagram ---
        registry.insert(
            "diagram".to_string(),
            ToolDoc {
                description: "Generate Mermaid or ASCII diagrams showing call relationships."
                    .to_string(),
                parameters: vec![ParameterDoc {
                    name: "uris".into(),
                    kind: "array<string>".into(),
                    description: "Roots for the diagram.".into(),
                    required: true,
                }],
                examples: vec![ExampleDoc {
                    label: "Generate Mermaid diagram for main loop".into(),
                    arguments: json!({"uris": ["src/main.rs::main"]}),
                }],
            },
        );

        // --- shell ---
        registry.insert(
            "shell".to_string(),
            ToolDoc {
                description: "Execute a shell command safely within the workspace sandbox. When building, use `curd build <target>` instead of direct build commands (e.g., `make`) to leverage predefined settings.toml hooks and capture semantic backtraces."
                    .to_string(),
                parameters: vec![ParameterDoc {
                    name: "command".into(),
                    kind: "string".into(),
                    description: "The bash command to run. Use `curd build <target>` for builds.".into(),
                    required: true,
                }],
                examples: vec![ExampleDoc {
                    label: "Run project tests via CURD tasks".into(),
                    arguments: json!({"command": "curd build test"}),
                }],
            },
        );

        // --- manage_file ---
        registry.insert(
            "manage_file".to_string(),
            ToolDoc {
                description: "Safe file operations strictly constrained to the workspace root."
                    .to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "path".into(),
                        kind: "string".into(),
                        description: "Target path relative to workspace root.".into(),
                        required: true,
                    },
                    ParameterDoc {
                        name: "action".into(),
                        kind: "string (enum)".into(),
                        description: "Action: 'create', 'write', 'delete', 'rename'.".into(),
                        required: false,
                    },
                    ParameterDoc {
                        name: "destination".into(),
                        kind: "string".into(),
                        description: "Required for 'rename'.".into(),
                        required: false,
                    },
                ],
                examples: vec![ExampleDoc {
                    label: "Create a new configuration file".into(),
                    arguments: json!({"path": "config/settings.json", "action": "create"}),
                }],
            },
        );

        // --- lsp ---
        registry.insert(
            "lsp".to_string(),
            ToolDoc {
                description: "Get syntax and/or semantic diagnostics for a file.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "uri".into(),
                        kind: "string".into(),
                        description: "File path or URI.".into(),
                        required: true,
                    },
                    ParameterDoc {
                        name: "mode".into(),
                        kind: "string (enum)".into(),
                        description: "'syntax', 'semantic', or 'both'.".into(),
                        required: false,
                    },
                ],
                examples: vec![ExampleDoc {
                    label: "Check for semantic errors in a Rust file".into(),
                    arguments: json!({"uri": "src/lib.rs", "mode": "semantic"}),
                }],
            },
        );

        // --- profile ---
        registry.insert("profile".to_string(), ToolDoc {
            description: "In-CURD profiler: generate flamegraphs from dependency call stacks or real sampling (Python).".to_string(),
            parameters: vec![
                ParameterDoc { name: "roots".into(), kind: "array<string>".into(), description: "Roots for the call stack analysis.".into(), required: true },
                ParameterDoc { name: "command".into(), kind: "string".into(), description: "Optional command to run and sample (Python only via py-spy).".into(), required: false },
                ParameterDoc { name: "format".into(), kind: "string (enum)".into(), description: "'ascii', 'folded', 'speedscope'.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Generate ASCII flamegraph for core module".into(), arguments: json!({"roots": ["curd-core/src/lib.rs"], "format": "ascii"}) },
            ],
        });

        // --- debug ---
        registry.insert("debug".to_string(), ToolDoc {
            description: "Run logical debug sessions or short interpreter snippets for hypothesis testing.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'execute', 'backends', 'start', 'send', 'recv', 'stop'.".into(), required: true },
                ParameterDoc { name: "language".into(), kind: "string".into(), description: "Target language (e.g. 'python', 'javascript') for 'execute' or 'start'.".into(), required: false },
                ParameterDoc { name: "snippet".into(), kind: "string".into(), description: "Code snippet to run or send.".into(), required: false },
                ParameterDoc { name: "debug_session_id".into(), kind: "integer".into(), description: "ID of the interactive debug session.".into(), required: false },
                ParameterDoc { name: "session_id".into(), kind: "integer".into(), description: "Deprecated alias for debug_session_id.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Run a Python snippet for validation".into(), arguments: json!({"action": "execute", "language": "python", "snippet": "print(1 + 1)"}) },
            ],
        });

        // --- review_cycle / legacy session alias ---
        registry.insert(
            "review_cycle".to_string(),
            ToolDoc {
                description: "Manage review-cycle baselines and tracked changes.".to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "action".into(),
                        kind: "string (enum)".into(),
                        description: "'begin', 'status', 'changes', 'review', 'end'.".into(),
                        required: true,
                    },
                    ParameterDoc {
                        name: "label".into(),
                        kind: "string".into(),
                        description: "Optional review-cycle label.".into(),
                        required: false,
                    },
                    ParameterDoc {
                        name: "limit".into(),
                        kind: "integer".into(),
                        description: "Optional max number of changed files to return.".into(),
                        required: false,
                    },
                ],
                examples: vec![
                    ExampleDoc {
                        label: "Start a review cycle".into(),
                        arguments: json!({"action": "begin", "label": "refactor pass"}),
                    },
                    ExampleDoc {
                        label: "Review current changes".into(),
                        arguments: json!({"action": "review"}),
                    },
                ],
            },
        );

        registry.insert(
            "session".to_string(),
            ToolDoc {
                description: "Deprecated alias for review_cycle. Manage review-cycle baselines and tracked changes."
                    .to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "action".into(),
                        kind: "string (enum)".into(),
                        description: "'begin', 'status', 'changes', 'review', 'end'.".into(),
                        required: true,
                    },
                    ParameterDoc {
                        name: "label".into(),
                        kind: "string".into(),
                        description: "Optional name for the session.".into(),
                        required: false,
                    },
                ],
                examples: vec![ExampleDoc {
                    label: "Review all changes in current session".into(),
                    arguments: json!({"action": "review"}),
                }],
            },
        );

        // --- doc ---
        registry.insert(
            "doc".to_string(),
            ToolDoc {
                description: "Get verbose documentation and examples for any CURD tool."
                    .to_string(),
                parameters: vec![ParameterDoc {
                    name: "tool".into(),
                    kind: "string".into(),
                    description: "The name of the tool to document.".into(),
                    required: true,
                }],
                examples: vec![ExampleDoc {
                    label: "Get help for the search tool".into(),
                    arguments: json!({"tool": "search"}),
                }],
            },
        );

        // --- batch ---
        registry.insert("batch".to_string(), ToolDoc {
            description: "Execute multiple tool calls in one turn as a dependency DAG.".to_string(),
            parameters: vec![
                ParameterDoc { name: "tasks".into(), kind: "array<object>".into(), description: "List of tasks with 'id', 'tool', 'args', and 'depends_on'.".into(), required: true },
            ],
            examples: vec![
                ExampleDoc { label: "Run search and then read results".into(), arguments: json!({"tasks": [{"id": "t1", "tool": "search", "args": {"query": "main"}}, {"id": "t2", "tool": "read", "args": {"uris": ["src/main.rs"]}, "depends_on": ["t1"]}]}) },
            ],
        });

        // --- benchmark ---
        registry.insert("benchmark".to_string(), ToolDoc {
            description: "Run quick in-process timing for CURD operations.".to_string(),
            parameters: vec![
                ParameterDoc { name: "operation".into(), kind: "string".into(), description: "The tool name to benchmark.".into(), required: true },
                ParameterDoc { name: "params".into(), kind: "object".into(), description: "Arguments for the tool.".into(), required: true },
                ParameterDoc { name: "iterations".into(), kind: "integer".into(), description: "Number of times to run.".into(), required: false },
                ParameterDoc { name: "save_baseline".into(), kind: "boolean".into(), description: "When true, writes JSON report to .curd/benchmarks.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Benchmark workspace scan performance".into(), arguments: json!({"operation": "workspace", "params": {"action": "list"}, "iterations": 5}) },
            ],
        });

        // --- simulate ---
        registry.insert("simulate".to_string(), ToolDoc {
            description: "Dry-run preflight for execute_plan/execute_dsl payloads. Performs validation without mutating workspace and surfaces session requirements or plan/DSL shape issues before execution.".to_string(),
            parameters: vec![
                ParameterDoc { name: "mode".into(), kind: "string (enum)".into(), description: "'execute_plan' or 'execute_dsl'.".into(), required: true },
                ParameterDoc { name: "plan".into(), kind: "object".into(), description: "Required when mode=execute_plan.".into(), required: false },
                ParameterDoc { name: "nodes".into(), kind: "array<object>".into(), description: "Required when mode=execute_dsl.".into(), required: false },
                ParameterDoc { name: "profile".into(), kind: "string".into(), description: "Optional profile override applied during validation.".into(), required: false },
                ParameterDoc { name: "session_token".into(), kind: "string".into(), description: "Optional agent/session token for scoped validation.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Simulate a plan payload".into(), arguments: json!({"mode": "execute_plan", "plan": {"id": "00000000-0000-0000-0000-000000000000", "nodes": []}}) },
            ],
        });

        registry.insert("plugin_tool".to_string(), ToolDoc {
            description: "Install, remove, or list signed .curdt tool plugins. Plugin installation and removal are human-only operations and obey plugin enablement/policy gates.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'add', 'remove', or 'list'.".into(), required: true },
                ParameterDoc { name: "archive_path".into(), kind: "string".into(), description: "Path to a signed .curdt archive when action=add.".into(), required: false },
                ParameterDoc { name: "package_id".into(), kind: "string".into(), description: "Installed package id when action=remove.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "List installed tool plugins".into(), arguments: json!({"action":"list"}) },
            ],
        });

        registry.insert("plugin_language".to_string(), ToolDoc {
            description: "Install, remove, or list signed .curdl language plugins. Installed language plugins can contribute language definitions and build-system metadata when plugins are enabled.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'add', 'remove', or 'list'.".into(), required: true },
                ParameterDoc { name: "archive_path".into(), kind: "string".into(), description: "Path to a signed .curdl archive when action=add.".into(), required: false },
                ParameterDoc { name: "package_id".into(), kind: "string".into(), description: "Installed package id when action=remove.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "List installed language plugins".into(), arguments: json!({"action":"list"}) },
            ],
        });

        registry.insert("plugin_trust".to_string(), ToolDoc {
            description: "Manage trusted signing keys for CURD plugin packages. Trust mutation is human-only and controls which signed .curdt / .curdl archives may be installed.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'add', 'get', 'remove', 'enable', 'disable', or 'list'.".into(), required: true },
                ParameterDoc { name: "key_id".into(), kind: "string".into(), description: "Trusted key id for get/remove/enable/disable/add.".into(), required: false },
                ParameterDoc { name: "pubkey_hex".into(), kind: "string".into(), description: "Ed25519 public key hex when action=add.".into(), required: false },
                ParameterDoc { name: "allowed_kinds".into(), kind: "array<string>".into(), description: "Allowed plugin kinds for the trusted key when action=add.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "List trusted plugin keys".into(), arguments: json!({"action":"list"}) },
            ],
        });

        // --- template ---
        registry.insert("template".to_string(), ToolDoc {
            description: "Manage reusable workflow templates persisted under .curd/templates.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'register', 'list', 'get', or 'instantiate'.".into(), required: true },
                ParameterDoc { name: "name".into(), kind: "string".into(), description: "Template name.".into(), required: false },
                ParameterDoc { name: "template".into(), kind: "object".into(), description: "Template payload for register.".into(), required: false },
                ParameterDoc { name: "vars".into(), kind: "object".into(), description: "Substitution vars for instantiate, using ${var} tokens.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Register a template".into(), arguments: json!({"action": "register", "name": "refactor_sig", "template": {"mode":"execute_plan","plan":{"id":"${plan_id}","nodes":[]}}}) },
            ],
        });

        // --- proposal ---
        registry.insert("proposal".to_string(), ToolDoc {
            description: "Manage local CURD change proposals (independent of git PRs).".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'open', 'status', 'run_gate', 'approve', or 'reject'.".into(), required: true },
                ParameterDoc { name: "id".into(), kind: "string".into(), description: "Proposal id. Optional for open; optional for status(list all).".into(), required: false },
                ParameterDoc { name: "title".into(), kind: "string".into(), description: "Title for open.".into(), required: false },
                ParameterDoc { name: "summary".into(), kind: "string".into(), description: "One-line summary for open.".into(), required: false },
                ParameterDoc { name: "simulate".into(), kind: "object".into(), description: "Optional simulate artifact payload.".into(), required: false },
                ParameterDoc { name: "crawl".into(), kind: "object".into(), description: "Optional crawl artifact payload.".into(), required: false },
                ParameterDoc { name: "simulate_args".into(), kind: "object".into(), description: "For run_gate: arguments to simulate.".into(), required: false },
                ParameterDoc { name: "crawl_args".into(), kind: "object".into(), description: "For run_gate: arguments to crawl; must include non-empty roots.".into(), required: false },
                ParameterDoc { name: "checkpoints".into(), kind: "object".into(), description: "Optional checkpoint refs.".into(), required: false },
                ParameterDoc { name: "review".into(), kind: "object".into(), description: "Optional review findings payload.".into(), required: false },
                ParameterDoc { name: "reason".into(), kind: "string".into(), description: "Decision reason for approve/reject.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Open a local change proposal".into(), arguments: json!({"action":"open","title":"Refactor parser cache","summary":"Safer invalidation and perf smoke attached"}) },
                ExampleDoc { label: "Run proposal gate checks".into(), arguments: json!({"action":"run_gate","id":"00000000-0000-0000-0000-000000000000","simulate_args":{"mode":"execute_dsl","nodes":[]},"crawl_args":{"mode":"crawl_heal","roots":["src/lib.rs::run"],"depth":2}}) },
                ExampleDoc { label: "Approve proposal".into(), arguments: json!({"action":"approve","id":"00000000-0000-0000-0000-000000000000","reason":"Simulation and crawl passed (same snapshot)"}) },
            ],
        });

        // --- checkpoint ---
        registry.insert("checkpoint".to_string(), ToolDoc {
            description: "Inspect milestone checkpoints persisted during plan execution.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'list' or 'get'.".into(), required: true },
                ParameterDoc { name: "plan_id".into(), kind: "string (uuid)".into(), description: "Plan id to inspect.".into(), required: true },
                ParameterDoc { name: "name".into(), kind: "string".into(), description: "Checkpoint filename for get.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "List checkpoints for a plan".into(), arguments: json!({"action":"list","plan_id":"00000000-0000-0000-0000-000000000000"}) },
            ],
        });

        // --- delegate ---
        registry.insert("delegate".to_string(), ToolDoc {
            description: "Manager-worker delegation board for plan node ownership and completion.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'create', 'claim', 'heartbeat', 'complete', 'auto_assign', 'status'.".into(), required: true },
                ParameterDoc { name: "plan_id".into(), kind: "string (uuid)".into(), description: "Plan id.".into(), required: true },
                ParameterDoc { name: "nodes".into(), kind: "array<string>".into(), description: "Node ids for create action.".into(), required: false },
                ParameterDoc { name: "node_id".into(), kind: "string (uuid)".into(), description: "Node id for claim/heartbeat/complete.".into(), required: false },
                ParameterDoc { name: "worker".into(), kind: "string".into(), description: "Worker identifier for claim/heartbeat/complete/auto_assign.".into(), required: false },
                ParameterDoc { name: "stale_timeout_secs".into(), kind: "integer".into(), description: "Auto-requeue timeout (status action), default 300s.".into(), required: false },
                ParameterDoc { name: "max_claims".into(), kind: "integer".into(), description: "Maximum pending nodes to claim from frontier for auto_assign (default 1, max 100).".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Claim a node".into(), arguments: json!({"action":"claim","plan_id":"00000000-0000-0000-0000-000000000000","node_id":"00000000-0000-0000-0000-000000000001","worker":"agent-alpha"}) },
                ExampleDoc { label: "Auto-assign from frontier".into(), arguments: json!({"action":"auto_assign","plan_id":"00000000-0000-0000-0000-000000000000","worker":"agent-alpha","max_claims":2}) },
            ],
        });

        // --- frontier ---
        registry.insert("frontier".to_string(), ToolDoc {
            description: "Manage graph-driven frontier queues for multi-agent work distribution.".to_string(),
            parameters: vec![
                ParameterDoc { name: "action".into(), kind: "string (enum)".into(), description: "'seed', 'pop', 'status', or 'reset'.".into(), required: true },
                ParameterDoc { name: "plan_id".into(), kind: "string (uuid)".into(), description: "Plan id for queue state.".into(), required: true },
                ParameterDoc { name: "uris".into(), kind: "array<string>".into(), description: "URIs for seed action.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Seed a frontier queue".into(), arguments: json!({"action":"seed","plan_id":"00000000-0000-0000-0000-000000000000","uris":["src/lib.rs::run"]}) },
            ],
        });

        // --- crawl ---
        registry.insert(
            "crawl".to_string(),
            ToolDoc {
                description:
                    "Deterministic dry-run crawler skeletons for heal/audit/prune workflows."
                        .to_string(),
                parameters: vec![
                    ParameterDoc {
                        name: "mode".into(),
                        kind: "string (enum)".into(),
                        description: "'crawl_heal', 'crawl_audit', 'crawl_prune', or 'crawl_mutate'.".into(),
                        required: true,
                    },
                    ParameterDoc {
                        name: "roots".into(),
                        kind: "array<string>".into(),
                        description: "Root URIs/symbols for crawl expansion.".into(),
                        required: true,
                    },
                    ParameterDoc {
                        name: "depth".into(),
                        kind: "integer".into(),
                        description: "Graph expansion depth.".into(),
                        required: false,
                    },
                    ParameterDoc {
                        name: "enqueue".into(),
                        kind: "boolean".into(),
                        description: "When true, enqueue top-ranked candidates into frontier."
                            .into(),
                        required: false,
                    },
                    ParameterDoc {
                        name: "include_contract_gists".into(),
                        kind: "boolean".into(),
                        description: "When true, enrich top candidates with one-line contract gists."
                            .into(),
                        required: false,
                    },
                    ParameterDoc {
                        name: "contract_top_k".into(),
                        kind: "integer".into(),
                        description:
                            "How many ranked candidates receive gist enrichment (default 5)."
                                .into(),
                        required: false,
                    },
                    ParameterDoc {
                        name: "plan_id".into(),
                        kind: "string (uuid)".into(),
                        description: "Required when enqueue=true.".into(),
                        required: false,
                    },
                    ParameterDoc {
                        name: "top_k".into(),
                        kind: "integer".into(),
                        description: "Number of ranked candidates to enqueue (default 20).".into(),
                        required: false,
                    },
                ],
                examples: vec![
                    ExampleDoc {
                        label: "Run crawl_heal dry-run".into(),
                        arguments: json!({"mode":"crawl_heal","roots":["src/lib.rs::run"],"depth":2}),
                    },
                    ExampleDoc {
                        label: "Run crawl with contract gists".into(),
                        arguments: json!({"mode":"crawl_heal","roots":["src/lib.rs::run"],"depth":2,"include_contract_gists":true,"contract_top_k":3}),
                    },
                ],
            },
        );

        // --- execute_dsl ---
        registry.insert("execute_dsl".to_string(), ToolDoc {
            description: "Execute a sequence of DSL nodes (Call, Atomic, Abort, Assign). Supports variable interpolation with $var. If the payload contains mutating or runtime-affecting steps, an active workspace session is required.".to_string(),
            parameters: vec![
                ParameterDoc { name: "nodes".into(), kind: "array<object>".into(), description: "List of DslNodes (Call, Atomic, Abort, Assign).".into(), required: true },
                ParameterDoc { name: "profile".into(), kind: "string".into(), description: "Optional profile override for nested validation.".into(), required: false },
                ParameterDoc { name: "session_token".into(), kind: "string".into(), description: "Required for agent-scoped execution; mutating payloads also require an active workspace session.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Assign search result and then read".into(), arguments: json!({"nodes": [{"type":"assign","var":"res","value":{"tool":"search","args":{"query":"main","mode":"symbol"}}},{"type":"call","tool":"read","args":{"uris":["$res"]}}]}) },
            ],
        });

        // --- execute_plan ---
        registry.insert("execute_plan".to_string(), ToolDoc {
            description: "Execute a dependency-aware DAG Plan. Mutating or runtime-affecting plans require an active workspace session.".to_string(),
            parameters: vec![
                ParameterDoc { name: "plan".into(), kind: "object".into(), description: "The Plan object containing nodes and dependencies.".into(), required: true },
                ParameterDoc { name: "profile".into(), kind: "string".into(), description: "Optional profile override for nested plan validation.".into(), required: false },
                ParameterDoc { name: "session_token".into(), kind: "string".into(), description: "Required for agent-scoped execution; mutating plans also require an active workspace session.".into(), required: false },
            ],
            examples: vec![
                ExampleDoc { label: "Run a simple plan with one search node".into(), arguments: json!({"plan": {"id": "00000000-0000-0000-0000-000000000000", "nodes": [{"id": "00000000-0000-0000-0000-000000000001", "op": {"McpCall": {"tool": "search", "args": {"query": "test","mode":"symbol"}}}, "dependencies": [], "output_limit": 1024, "retry_limit": 0}]}}) },
            ],
        });

        Self { registry }
    }

    pub fn get_doc(&self, tool: &str) -> Value {
        match self.registry.get(tool) {
            Some(doc) => {
                let params: Vec<Value> = doc
                    .parameters
                    .iter()
                    .map(|p| {
                        json!({
                            "name": p.name,
                            "type": p.kind,
                            "description": p.description,
                            "required": p.required
                        })
                    })
                    .collect();

                let examples: Vec<Value> = doc
                    .examples
                    .iter()
                    .map(|e| {
                        json!({
                            "label": e.label,
                            "json_rpc_call": {
                                "jsonrpc": "2.0",
                                "method": "tools/call",
                                "params": {
                                    "name": tool,
                                    "arguments": e.arguments
                                },
                                "id": 1
                            }
                        })
                    })
                    .collect();

                json!({
                    "tool": tool,
                    "description": doc.description,
                    "parameters": params,
                    "examples": examples
                })
            }
            None => json!({"error": format!("No documentation found for tool: {}", tool)}),
        }
    }
}

impl Default for DocEngine {
    fn default() -> Self {
        Self::new()
    }
}
