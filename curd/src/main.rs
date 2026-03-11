use anyhow::Result;
use clap::{Args, Parser, Subcommand};
#[cfg(feature = "mcp")]
use curd::McpServer;
use curd_core::{DoctorIndexConfig, DoctorThresholds};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "mcp")]
pub mod init;
#[cfg(feature = "core")]
pub mod repl;
#[cfg(feature = "core")]
pub mod workspace_init;
#[cfg(feature = "core")]
pub mod workspace_lifecycle;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// (Legacy shorthand) Path to the workspace root to start the MCP server
    #[arg(hide = true)]
    legacy_root: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Model Context Protocol (MCP) server over stdin/stdout
    #[cfg(feature = "mcp")]
    Mcp {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Initialize and authorize a new agent keypair, auto-configuring a specified harness
    #[cfg(feature = "mcp")]
    #[command(visible_alias = "ina", visible_alias = "agt")]
    InitAgent {
        /// Optional: Identifier for the agent (e.g., 'alpha', 'claude_coder'). Use commas for multiple names.
        #[arg(short, long)]
        name: Option<String>,

        /// Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted.
        #[arg(short = 'r', long = "harness")]
        harness: Option<String>,

        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Run built-in self diagnostics for indexing and regressions
    #[cfg(feature = "core")]
    #[command(visible_alias = "dct")]
    Doctor(Box<DoctorArgs>),
    /// Build via CURD control plane (adapter-based dry-run/execute)
    #[cfg(feature = "core")]
    #[command(visible_alias = "bld", visible_alias = "b")]
    Build {
        /// Optional build target/task to execute (or directory if no target is specified)
        target_or_dir: Option<String>,
        /// Path to workspace root (if target is provided)
        dir: Option<PathBuf>,
        /// Build adapter override (e.g. cargo, cmake, ninja, make, uv, poetry, pip, conda, mamba, npm, yarn, pnpm, bun)
        #[arg(long)]
        adapter: Option<String>,
        /// Build profile (debug|release)
        #[arg(long)]
        profile: Option<String>,
        /// Execute planned commands (default: true)
        #[arg(long, default_value = "true", action = clap::ArgAction::Set, allow_hyphen_values = true)]
        execute: bool,
        /// Only show the build plan, do not execute.
        #[arg(long)]
        plan: bool,
        /// Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)
        #[arg(short = 'c', long)]
        command: Option<String>,
        /// Allow execution of custom adapters defined in workspace settings.toml without prompt.
        #[arg(long)]
        allow_untrusted: bool,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
        /// Use cargo-zigbuild instead of cargo build
        #[arg(long)]
        zig: bool,
        /// Trailing arguments to pass directly to the underlying compiler/adapter
        #[arg(last = true)]
        trailing_args: Vec<String>,
    },
    /// Print a shell hook to implicitly route standard commands (like 'make') through CURD
    #[cfg(feature = "core")]
    #[command(visible_alias = "hok")]
    Hook {
        /// The target shell (zsh, bash, fish, powershell)
        #[arg(value_parser = ["zsh", "bash", "fish", "powershell"])]
        shell: String,
    },
    /// Compare symbols at AST level
    #[cfg(feature = "core")]
    #[command(visible_alias = "dif")]
    Diff {
        /// Semantic AST-level diff
        #[arg(long)]
        semantic: bool,
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
        /// Optional specific symbol to diff
        #[arg(long)]
        symbol: Option<String>,
    },
    /// Semantic refactoring engine
    #[cfg(feature = "core")]
    #[command(visible_alias = "ref")]
    Refactor {
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,

        #[command(subcommand)]
        command: RefactorCommands,
    },
    /// Manage external workspace contexts (Read-Only or Semantic linking)
    #[cfg(feature = "core")]
    #[command(visible_alias = "ctx")]
    Context {
        /// Path to the primary workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,

        #[command(subcommand)]
        command: ContextCommands,
    },
    /// Initialize CURD workspace: auto-detect build system, create .curd/ directory
    #[cfg(feature = "core")]
    #[command(visible_alias = "ini")]
    Init {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Manage CURD configuration and policies
    #[cfg(feature = "core")]
    #[command(visible_alias = "cfg")]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Install, remove, or list signed language plugins (.curdl)
    #[cfg(feature = "core")]
    #[command(visible_alias = "plang")]
    PluginLanguage {
        #[command(subcommand)]
        command: PluginPackageCommands,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Install, remove, or list signed tool plugins (.curdt)
    #[cfg(feature = "core")]
    #[command(visible_alias = "ptool")]
    PluginTool {
        #[command(subcommand)]
        command: PluginPackageCommands,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Manage trusted signing keys for CURD plugin packages
    #[cfg(feature = "core")]
    #[command(visible_alias = "ptrust")]
    PluginTrust {
        #[command(subcommand)]
        command: PluginTrustCommands,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Soft detach CURD from the current workspace (removes git hooks and scrubs scripts)
    #[cfg(feature = "core")]
    #[command(visible_alias = "det")]
    Detach {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
        /// How to handle an active shadow transaction before detaching
        #[arg(long, value_enum)]
        shadow: Option<workspace_lifecycle::ShadowDisposition>,
    },
    /// Permanently delete CURD from the current workspace by removing the .curd/ directory
    #[cfg(feature = "core")]
    #[command(visible_alias = "del")]
    Delete {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
        /// Force skip confirmation
        #[arg(short, long)]
        yes: bool,
        /// How to handle an active shadow transaction before deleting CURD state
        #[arg(long, value_enum)]
        shadow: Option<workspace_lifecycle::ShadowDisposition>,
    },
    /// Print a summary of the current workspace state (index stats, shadow changes, etc.)
    #[cfg(feature = "core")]
    #[command(visible_alias = "st", visible_alias = "sts")]
    Status {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Tail the agent's mutation and execution history
    #[cfg(feature = "core")]
    Log {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
        /// Number of recent entries to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Start the interactive CURD REPL for semantic exploration
    #[cfg(feature = "core")]
    #[command(visible_alias = "rpl")]
    Repl {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Run a .curd script by compiling it to the current DSL IR
    #[cfg(feature = "core")]
    Run {
        /// Either a script path, or an action like 'check' / 'compile'
        first: String,
        /// Script path when using an explicit action
        second: Option<String>,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Script argument override in key=value form
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Optional profile override
        #[arg(long)]
        profile: Option<String>,
        /// Output path for `curd run compile ...`
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run semantic integrity tests on the symbol graph
    #[cfg(feature = "core")]
    #[command(visible_alias = "tst", name = "test")]
    Test {
        /// Scope of the test (nodes, edges, policy, architecture, all)
        #[arg(default_value = "all", value_parser = ["nodes", "edges", "policy", "architecture", "all"])]
        scope: String,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Show detailed list of broken links and dead zones
        #[arg(short, long)]
        verbose: bool,
    },
    /// Semantic search across the indexed graph
    #[cfg(feature = "core")]
    #[command(visible_alias = "sch", visible_alias = "s")]
    Search {
        /// The query string
        query: String,
        /// Optional kind filter (e.g. 'function', 'class')
        #[arg(long)]
        kind: Option<String>,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Explore the caller/callee dependency graph of a symbol
    #[cfg(feature = "core")]
    #[command(visible_alias = "g")]
    Graph {
        /// The symbol URI to map
        uri: String,
        /// The traversal depth
        #[arg(long, default_value = "1")]
        depth: usize,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Read a file or semantic symbol exactly as the engine parses it
    #[cfg(feature = "core")]
    #[command(visible_alias = "red")]
    Read {
        /// The URI to read (e.g., 'src/main.rs' or 'src/main.rs::my_function')
        uri: String,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Manually mutate a file or symbol via the AST-aware EditEngine
    #[cfg(feature = "core")]
    #[command(visible_alias = "edt", visible_alias = "e")]
    Edit {
        /// The URI to edit
        uri: String,
        /// The action to perform ('upsert' or 'delete')
        #[arg(short, long, default_value = "upsert")]
        action: String,
        /// The literal code to insert (ignored if action is 'delete')
        #[arg(short, long, default_value = "")]
        code: String,
        /// Optional: The expected Merkle Root of the workspace state for optimistic concurrency control
        #[arg(long)]
        base_state_hash: Option<String>,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Generate diagrams from the symbol graph
    #[cfg(feature = "core")]
    #[command(visible_alias = "dia")]
    Diagram {
        /// Symbol URIs to include as roots
        #[arg(short, long)]
        uris: Vec<String>,
        /// Path to the workspace root
        #[arg(short, long, default_value = ".")]
        root: PathBuf,
        /// Output format (ascii, svg, dot, mermaid)
        #[arg(short, long, default_value = "ascii")]
        format: String,
        /// Traversal depth for dependencies
        #[arg(long, default_value = "1")]
        depth: u8,
    },
    /// Manage manual ShadowStore transaction sessions
    #[cfg(feature = "core")]
    #[command(visible_alias = "ses")]
    Session {
        #[command(subcommand)]
        command: SessionCommands,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    #[cfg(feature = "mcp")]
    /// Manage, inspect, and execute saved AI plans
    #[command(visible_alias = "pln", visible_alias = "p")]
    Plan {
        #[command(subcommand)]
        command: PlanCommands,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    #[command(hide = true)]
    IndexWorker {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        response: PathBuf,
    },
    /// Alias for curd doctor indexing
    #[cfg(feature = "core")]
    #[command(visible_alias = "idx")]
    Index(Box<DoctorArgs>),
    #[command(external_subcommand)]
    External(Vec<String>),
    /// Show help for CURD
    #[command(name = "h")]
    H,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current workspace configuration
    Show,
    /// Set a configuration value (e.g. `set index.mode fast`)
    Set {
        /// Key in dotted notation (e.g. build.tasks.test)
        key: String,
        /// Value to set
        value: String,
    },
    /// Remove a configuration value
    Unset {
        /// Key in dotted notation
        key: String,
    },
}

#[derive(Subcommand)]
enum PluginPackageCommands {
    /// List installed plugin packages
    List,
    /// Install a signed plugin archive
    Add {
        /// Path to the signed archive (.curdl or .curdt)
        archive_path: PathBuf,
    },
    /// Remove an installed plugin package
    Remove {
        /// Installed package id
        package_id: String,
    },
}

#[derive(Subcommand)]
enum PluginTrustCommands {
    /// List trusted signing keys
    List,
    /// Get one trusted signing key
    Get {
        /// Trusted key id
        key_id: String,
    },
    /// Add or replace a trusted signing key
    Add(PluginTrustAddArgs),
    /// Remove a trusted signing key
    Remove {
        /// Trusted key id
        key_id: String,
    },
    /// Enable a trusted signing key
    Enable {
        /// Trusted key id
        key_id: String,
    },
    /// Disable a trusted signing key
    Disable {
        /// Trusted key id
        key_id: String,
    },
}

#[derive(Args)]
struct PluginTrustAddArgs {
    /// Trusted key id
    key_id: String,
    /// Ed25519 public key hex
    pubkey_hex: String,
    /// Optional label
    #[arg(long)]
    label: Option<String>,
    /// Allowed plugin kinds for this key
    #[arg(long = "kind", value_parser = ["tool", "language"])]
    kinds: Vec<String>,
}

#[derive(Subcommand)]
enum RefactorCommands {
    /// Rename a function/class/variable
    Rename {
        symbol: String,
        new_name: String,
        /// Optional: Use an external LSP server for type-aware renaming (e.g., 'rust-analyzer')
        #[arg(long)]
        lsp: Option<String>,
    },
    /// Move a function between files
    Move {
        symbol: String,
        target_file: PathBuf,
    },
    /// Extract code into a new function
    Extract {
        file_range: String, // e.g., src/main.rs:10-20
        new_function_name: String,
    },
}

#[derive(Subcommand)]
enum ContextCommands {
    /// Add an external repository context
    Add {
        /// The path to the external repository
        path: PathBuf,
        /// Optional alias for this context (auto-generated if omitted)
        #[arg(long)]
        alias: Option<String>,
        /// Link context in Index mode (Search and Graph only, no file reads)
        #[arg(long)]
        index: bool,
        /// Link context in Read mode (Read-only, no writes)
        #[arg(long)]
        read: bool,
    },
    /// Remove a linked context by alias or path
    Remove {
        /// Alias or path of the context to remove
        name: String,
        /// Force removal even if there are dangling dependencies in the primary workspace
        #[arg(long)]
        force: bool,
    },
    /// List all currently linked contexts
    List,
}

#[derive(Subcommand)]
enum SessionCommands {
    /// Open a new shadow transaction
    Begin,
    /// View changes and architectural impact of the current session
    Review,
    /// View a detailed log of all tool calls and their results
    Log,
    /// Commit the active shadow transaction to disk
    Commit,
    /// Discard the active shadow transaction
    Rollback,
}

#[cfg(feature = "mcp")]
#[derive(Subcommand)]
enum PlanCommands {
    /// List all saved plans in the workspace
    List,
    /// Read a specific plan and format it as a readable execution tree
    Read {
        /// The UUID of the plan
        id: String,
    },
    /// Edit a saved plan or compiled script artifact interactively
    Edit {
        /// The UUID of the plan
        id: String,
    },
    /// Execute a saved plan
    Impl {
        /// The UUID of the plan
        id: String,
        /// The UUID of the active session this plan belongs to
        #[arg(long)]
        session: String,
        /// Perform a dry-run validation (simulate) instead of executing
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(clap::Args)]
pub struct DoctorArgs {
    /// Path to the workspace root
    #[arg(default_value = ".")]
    pub root: PathBuf,
    /// Fail with non-zero status when warnings/findings are present
    #[arg(long)]
    pub strict: bool,
    /// Optional threshold: fail when index total_ms exceeds this value
    #[arg(long)]
    pub max_total_ms: Option<u64>,
    /// Optional threshold: fail when parse_fail count exceeds this value
    #[arg(long)]
    pub max_parse_fail: Option<usize>,
    /// Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)
    #[arg(long)]
    pub max_no_symbols_ratio: Option<f64>,
    /// Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)
    #[arg(long)]
    pub max_skipped_large_ratio: Option<f64>,
    /// Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)
    #[arg(long)]
    pub min_coverage_ratio: Option<f64>,
    /// Optional threshold: fail when coverage state does not match this exactly (e.g. 'full')
    #[arg(long)]
    pub require_coverage_state: Option<String>,
    /// Optional threshold: fail when total symbols found is below this count
    #[arg(long)]
    pub min_symbol_count: Option<usize>,
    /// Optional threshold: fail when symbol density is below this value
    #[arg(long)]
    pub min_symbols_per_k_files: Option<f64>,
    /// Optional threshold: fail when overlap with full index is below this value
    #[arg(long)]
    pub min_overlap_with_full: Option<f64>,
    /// Re-run the index build twice and ensure symbol counts and hashes match exactly
    #[arg(long)]
    pub parity_rerun: bool,
    /// Compare index contents against a 'full' mode run
    #[arg(long)]
    pub compare_with_full: bool,
    /// Run indexing multiple times to generate a performance profile
    #[arg(long)]
    pub profile_index: bool,
    /// Write report as JSON to this path
    #[arg(long)]
    pub report_out: Option<PathBuf>,
    /// The profile of thresholds to apply ('ci_fast' or 'ci_strict'). Overrides config.
    #[arg(long)]
    pub profile: Option<String>,

    /// Optional index mode for this doctor run (e.g. 'lazy', 'full')
    #[arg(long, value_parser = ["lazy", "full"])]
    pub index_mode: Option<String>,
    /// Optional index scope for this doctor run
    #[arg(long, value_parser = ["auto", "forced"])]
    pub index_scope: Option<String>,
    /// Optional max file size in bytes for this doctor run
    #[arg(long)]
    pub index_max_file_size: Option<u64>,
    /// Optional large file policy for this doctor run
    #[arg(long, value_parser = ["skip", "skeleton", "full"])]
    pub index_large_file_policy: Option<String>,
    /// Optional index execution model for this doctor run
    #[arg(long, value_parser = ["multithreaded", "multiprocess", "singlethreaded"])]
    pub index_execution: Option<String>,
    /// Optional chunk size for multiprocess mode
    #[arg(long)]
    pub index_chunk_size: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // SAFETY: process-wide static initialization at startup.
    // Must happen BEFORE Tokio runtime threads take over to avoid UB.
    if let Ok(exe) = std::env::current_exe() {
        unsafe {
            std::env::set_var("CURD_INDEX_WORKER_BIN", exe);
        }
    }

    // Initialize tracing
    setup_tracing();

    #[cfg(target_os = "windows")]
    {
        eprintln!("\n{}", "!".repeat(80));
        eprintln!("WARNING: Native sandboxing is not yet supported on Windows.");
        eprintln!(
            "Interactive agent tools that execute shell commands will run WITHOUT isolation."
        );
        eprintln!("Please exercise caution when using agentic features on this platform.");
        eprintln!("{}\n", "!".repeat(80));
    }

    let cli = Cli::parse();

    match cli.command {
        #[cfg(feature = "mcp")]
        Some(Commands::Mcp { root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let server = McpServer::new(&resolved.to_string_lossy());
            server.run().await
        }
        #[cfg(feature = "mcp")]
        Some(Commands::InitAgent {
            name,
            harness,
            root,
        }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            init::init_agent(name.as_deref(), harness.as_deref(), &resolved)
        }
        #[cfg(feature = "core")]
        Some(Commands::Doctor(args)) | Some(Commands::Index(args)) => {
            let resolved = resolve_workspace_root(args.root.clone());
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let result = curd::router::route_doctor_command(
                &json!({
                    "strict": args.strict || args.profile.is_some(),
                    "profile": args.profile,
                    "thresholds": DoctorThresholds {
                        max_total_ms: args.max_total_ms,
                        max_parse_fail: args.max_parse_fail,
                        max_no_symbols_ratio: args.max_no_symbols_ratio,
                        max_skipped_large_ratio: args.max_skipped_large_ratio,
                        min_coverage_ratio: args.min_coverage_ratio,
                        require_coverage_state: args.require_coverage_state.clone(),
                        min_symbol_count: args.min_symbol_count,
                        min_symbols_per_k_files: args.min_symbols_per_k_files,
                        min_overlap_with_full: args.min_overlap_with_full,
                        parity_rerun: args.parity_rerun,
                    },
                    "index_config": DoctorIndexConfig {
                        index_mode: args.index_mode.clone(),
                        index_scope: args.index_scope.clone(),
                        index_max_file_size: args.index_max_file_size,
                        index_large_file_policy: args.index_large_file_policy.clone(),
                        index_execution: args.index_execution.clone(),
                        index_chunk_size: args.index_chunk_size,
                        compare_with_full: args.compare_with_full,
                        profile_index: args.profile_index,
                        report_out: args.report_out.clone(),
                    }
                }),
                &ctx,
            )
            .await;
            if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
                anyhow::bail!("{}", err);
            }
            let report = result
                .get("report")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            if let Some(summary) = report.get("human_summary").and_then(|v| v.as_str()) {
                println!("{}", summary);
            }
            if let Some(path) = report
                .get("index_config")
                .and_then(|cfg| cfg.get("report_out"))
                .and_then(|v| v.as_str())
            {
                println!("REPORT written to {}", path);
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Build {
            target_or_dir,
            dir,
            adapter,
            profile,
            execute,
            plan,
            command,
            allow_untrusted,
            json,
            zig,
            trailing_args,
        }) => {
            let mut resolved_target = None;
            let mut resolved_root = PathBuf::from(".");

            if let Some(first) = target_or_dir {
                if let Some(second) = dir {
                    resolved_target = Some(first);
                    resolved_root = second;
                } else {
                    let path = PathBuf::from(&first);
                    if path.is_dir() && !std::fs::metadata(&first).is_ok_and(|m| m.is_file()) {
                        resolved_root = path;
                    } else {
                        resolved_target = Some(first);
                    }
                }
            }

            let actual_execute = if plan { false } else { execute };
            let resolved = resolve_workspace_root(resolved_root);
            enforce_workspace_config(&resolved)?;
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let adapter_arg = adapter.clone();
            let profile_arg = profile.clone();
            let target_arg = resolved_target.clone();
            let command_arg = command.clone();
            let trailing_args_arg = trailing_args.clone();
            let build_params = json!({
                "adapter": adapter_arg,
                "profile": profile_arg,
                "target": target_arg,
                "execute": actual_execute,
                "zig": zig,
                "command": command_arg,
                "allow_untrusted": allow_untrusted,
                "trailing_args": trailing_args_arg
            });
            let mut result = curd::router::route_build_command(&build_params, &ctx).await;
            if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
                anyhow::bail!("{}", err);
            }
            let mut out: curd_core::BuildResponse = serde_json::from_value(
                result
                    .get("response")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
            )?;

            if out.untrusted_confirmation_required && !json {
                println!("\n\x1b[1;33m⚠️  UNTRUSTED BUILD ADAPTER DETECTED\x1b[0m");
                println!(
                    "The adapter \x1b[1m'{}'\x1b[0m is defined in the local \x1b[34m.curd/settings.toml\x1b[0m.",
                    out.adapter
                );
                println!("Executing this adapter may run arbitrary commands on your system.");

                let confirmation = dialoguer::Confirm::new()
                    .with_prompt("Do you want to continue with execution?")
                    .default(false)
                    .interact()
                    .unwrap_or(false);

                if confirmation {
                    result = curd::router::route_build_command(
                        &json!({
                            "adapter": adapter,
                            "profile": profile,
                            "target": resolved_target,
                            "execute": actual_execute,
                            "zig": zig,
                            "command": command,
                            "allow_untrusted": true,
                            "trailing_args": trailing_args
                        }),
                        &ctx,
                    )
                    .await;
                    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
                        anyhow::bail!("{}", err);
                    }
                    out = serde_json::from_value(
                        result
                            .get("response")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({})),
                    )?;
                } else {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                use colored::*;
                let title = if actual_execute { "EXECUTION" } else { "PLAN" };
                let header = format!("--- CURD BUILD {} ---", title);
                println!("{}", header.bold().cyan());
                println!("  {}   {}", "Adapter:".dimmed(), out.adapter.bold());
                println!("  {}   {}", "Profile:".dimmed(), out.profile.bold());
                if let Some(ref t) = out.target {
                    println!("  {}   {}", "Target: ".dimmed(), t.bold());
                }
                println!("  {}   {}", "Steps:  ".dimmed(), out.steps.len());

                for (i, step) in out.steps.iter().enumerate() {
                    let cmd_str = step.command.join(" ");
                    println!("    [{}] {}", i + 1, cmd_str.white());
                    if let Some(suc) = step.success {
                        let status_str = if suc {
                            "SUCCESS".green()
                        } else {
                            "FAILED".red()
                        };
                        println!("         {} {}", "Result:".dimmed(), status_str.bold());
                    }
                }

                let footer = if out.status == "ok" {
                    "✔ Build process completed successfully.".green()
                } else {
                    "✘ Build process failed.".red()
                };
                println!("\n{}\n", footer.bold());
            }
            if out.status == "ok" {
                Ok(())
            } else {
                Err(anyhow::anyhow!("curd build failed"))
            }
        }
        #[cfg(feature = "core")]
        Some(Commands::Hook { shell }) => {
            let zsh_bash = r#"_curd_build_hook() {
    local tool="$1"
    shift
    
    if [ ! -d "./.curd" ]; then
        command "$tool" "$@"
        return
    fi

    local intercept=false
    local subcmd="$1"

    case "$tool" in
        cargo)
            case "$subcmd" in
                build|check|test|run) intercept=true ;;
            esac
            ;;
        npm|yarn|pnpm|bun)
            case "$subcmd" in
                build|test|lint) intercept=true ;;
            esac
            ;;
        make|ninja|cmake)
            case "$subcmd" in
                clean|help) intercept=false ;;
                *) intercept=true ;;
            esac
            ;;
    esac

    if [ "$intercept" = true ]; then
        curd build --adapter "$tool" --execute -- "$@"
    else
        command "$tool" "$@"
    fi
}

alias make="_curd_build_hook make"
alias cargo="_curd_build_hook cargo"
alias npm="_curd_build_hook npm"
alias yarn="_curd_build_hook yarn"
alias pnpm="_curd_build_hook pnpm"
alias bun="_curd_build_hook bun"
alias ninja="_curd_build_hook ninja"
alias cmake="_curd_build_hook cmake"
"#;

            let fish = r#"function _curd_build_hook
    set tool $argv[1]
    set -e argv[1]
    
    if not test -d ./.curd
        command $tool $argv
        return
    end

    set intercept false
    set subcmd $argv[1]

    switch $tool
        case cargo
            switch $subcmd
                case build check test run
                    set intercept true
            end
        case npm yarn pnpm bun
            switch $subcmd
                case build test lint
                    set intercept true
            end
        case make ninja cmake
            switch $subcmd
                case clean help
                    set intercept false
                case '*'
                    set intercept true
            end
    end

    if test "$intercept" = true
        curd build --adapter $tool --execute -- $argv
    else
        command $tool $argv
    end
end

alias make="_curd_build_hook make"
alias cargo="_curd_build_hook cargo"
alias npm="_curd_build_hook npm"
alias yarn="_curd_build_hook yarn"
alias pnpm="_curd_build_hook pnpm"
alias bun="_curd_build_hook bun"
alias ninja="_curd_build_hook ninja"
alias cmake="_curd_build_hook cmake"
"#;

            let powershell = r#"function Invoke-CurdBuildHook {
    param($Tool, $Args)
    if (!(Test-Path -Path ".\.curd" -PathType Container)) {
        & $Tool @Args
        return
    }

    $Intercept = $false
    $SubCmd = $Args[0]

    switch ($Tool) {
        "cargo" { if ("build","check","test","run" -contains $SubCmd) { $Intercept = $true } }
        "npm" { if ("build","test","lint" -contains $SubCmd) { $Intercept = $true } }
        "yarn" { if ("build","test","lint" -contains $SubCmd) { $Intercept = $true } }
        "pnpm" { if ("build","test","lint" -contains $SubCmd) { $Intercept = $true } }
        "bun" { if ("build","test","lint" -contains $SubCmd) { $Intercept = $true } }
        "make" { if (!("clean","help" -contains $SubCmd)) { $Intercept = $true } }
        "ninja" { if (!("clean","help" -contains $SubCmd)) { $Intercept = $true } }
        "cmake" { if (!("clean","help" -contains $SubCmd)) { $Intercept = $true } }
    }

    if ($Intercept) {
        curd build --adapter $Tool --execute -- @Args
    } else {
        & $Tool @Args
    }
}

function cargo { Invoke-CurdBuildHook "cargo" $args }
function npm { Invoke-CurdBuildHook "npm" $args }
function yarn { Invoke-CurdBuildHook "yarn" $args }
function pnpm { Invoke-CurdBuildHook "pnpm" $args }
function bun { Invoke-CurdBuildHook "bun" $args }
function make { Invoke-CurdBuildHook "make" $args }
function ninja { Invoke-CurdBuildHook "ninja" $args }
function cmake { Invoke-CurdBuildHook "cmake" $args }
"#;

            match shell.as_str() {
                "zsh" | "bash" => println!("{}", zsh_bash),
                "fish" => println!("{}", fish),
                "powershell" => println!("{}", powershell),
                _ => {}
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Diff {
            semantic,
            root,
            symbol,
        }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let res = curd::router::route_diff_command(
                &json!({
                    "semantic": semantic,
                    "symbol": symbol
                }),
                &ctx,
            )
            .await;
            if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
                anyhow::bail!("{}", err);
            }
            println!(
                "{}",
                res.get("output").and_then(|v| v.as_str()).unwrap_or("")
            );
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Refactor { root, command }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let action = match command {
                RefactorCommands::Rename {
                    symbol,
                    new_name,
                    lsp,
                } => curd_core::RefactorAction::Rename {
                    symbol,
                    new_name,
                    lsp_binary: lsp,
                },
                RefactorCommands::Move {
                    symbol,
                    target_file,
                } => curd_core::RefactorAction::Move {
                    symbol,
                    target_file,
                },
                RefactorCommands::Extract {
                    file_range,
                    new_function_name,
                } => curd_core::RefactorAction::Extract {
                    file_range,
                    new_function_name,
                },
            };

            let result = curd::router::route_refactor_command(action.clone(), &ctx).await;
            if let Some(out) = result.get("output").and_then(|v| v.as_str()) {
                println!("{}", out);
            } else if let Some(err_msg) = result.get("error").and_then(|v| v.as_str()) {
                use std::io::IsTerminal;
                if err_msg.contains("is overloaded") && std::io::stdout().is_terminal() {
                    // Extract options from the error message
                    if let Some(options_idx) = err_msg.find("Available options:\n") {
                        let options_str = &err_msg[options_idx + 19..];
                        let lines: Vec<&str> = options_str.lines().collect();

                        println!("\n{}", &err_msg[..options_idx].trim());
                        if let Ok(selections) = dialoguer::MultiSelect::new()
                                    .with_prompt("Select which overloads to rename (Space to select, Enter to confirm):")
                                    .items(&lines)
                                    .interact()
                                {
                                    if selections.is_empty() {
                                        anyhow::bail!("No overloads selected. Aborting.");
                                    }

                                    for &selection in &selections {
                                        let selected = lines[selection];
                                        if let Some(colon_idx) = selected.find(": ") {
                                            let full_id_with_line = &selected[..colon_idx];
                                            let mut sub_action = action.clone();
                                            if let curd_core::RefactorAction::Rename { symbol: ref mut sym, .. } = sub_action {
                                                *sym = full_id_with_line.to_string();
                                            } else {
                                                anyhow::bail!("Unexpected action state");
                                            }
                                            println!("Resubmitting with targeted overload: {}...", full_id_with_line);
                                            let retried = curd::router::route_refactor_command(sub_action, &ctx).await;
                                            if let Some(out) = retried.get("output").and_then(|v| v.as_str()) {
                                                println!("{}", out);
                                            } else if let Some(err) = retried.get("error").and_then(|v| v.as_str()) {
                                                anyhow::bail!("curd refactor failed on {}: {}", full_id_with_line, err);
                                            }
                                        }
                                    }
                                    return Ok(());
                                }
                    }
                }
                anyhow::bail!("curd refactor failed: {}", err_msg);
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Context { root, command }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            match command {
                ContextCommands::Add {
                    path,
                    alias,
                    index,
                    read,
                } => {
                    let res = curd::router::route_context_command(
                        &json!({
                            "action": "add",
                            "path": path,
                            "alias": alias,
                            "index": index,
                            "read": read
                        }),
                        &ctx,
                    )
                    .await;
                    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
                        anyhow::bail!("{}", err);
                    }
                    println!(
                        "Linked context `{}` -> {} (Mode: {:?})",
                        res.get("alias").and_then(|v| v.as_str()).unwrap_or(""),
                        res.get("path").and_then(|v| v.as_str()).unwrap_or(""),
                        res.get("mode").cloned().unwrap_or(serde_json::Value::Null)
                    );
                }
                ContextCommands::Remove { name, force } => {
                    let res = curd::router::route_context_command(
                        &json!({
                            "action": "remove",
                            "name": name,
                            "force": force
                        }),
                        &ctx,
                    )
                    .await;
                    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
                        if let Some(edges) = res.get("dangling_edges").and_then(|v| v.as_array()) {
                            println!(
                                "Checking dependency graph for dangling edges to {}...",
                                name
                            );
                            for edge in edges {
                                println!(
                                    "WARNING: `{}` calls `{}`",
                                    edge.get("caller").and_then(|v| v.as_str()).unwrap_or(""),
                                    edge.get("callee").and_then(|v| v.as_str()).unwrap_or("")
                                );
                            }
                        }
                        anyhow::bail!("{}", err);
                    }
                    if res.get("removed").is_some_and(|v| !v.is_null()) {
                        println!("Removed context `{}`", name);
                    } else {
                        println!(
                            "{}",
                            res.get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Context not found.")
                        );
                    }
                }
                ContextCommands::List => {
                    let res =
                        curd::router::route_context_command(&json!({"action": "list"}), &ctx).await;
                    let contexts = res
                        .get("contexts")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    if contexts.is_empty() {
                        println!(
                            "No contexts linked. Use `curd context add <dir>` to link an external repository."
                        );
                    } else {
                        println!("Linked Contexts:");
                        for link in contexts {
                            println!(
                                "  {} -> {} (Mode: {:?})",
                                link.get("alias").and_then(|v| v.as_str()).unwrap_or(""),
                                link.get("path").and_then(|v| v.as_str()).unwrap_or(""),
                                link.get("mode").cloned().unwrap_or(serde_json::Value::Null)
                            );
                        }
                    }
                }
            }
            Ok(())
        }
        Some(Commands::IndexWorker { request, response }) => {
            let req_bytes = fs::read(&request)?;
            let req: curd_core::IndexWorkerRequest = serde_json::from_slice(&req_bytes)?;
            let out = curd_core::run_index_worker(req)?;
            fs::write(response, serde_json::to_vec(&out)?)?;
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Init { root }) => {
            let resolved = resolve_workspace_root(root);
            workspace_init::run_init(&resolved)
        }
        #[cfg(feature = "core")]
        Some(Commands::Config { command, root }) => {
            let resolved = resolve_workspace_root(root);
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());

            match command {
                ConfigCommands::Show => {
                    let res =
                        curd::router::route_config_command(&json!({"action":"show"}), &ctx).await;
                    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
                        anyhow::bail!("{}", err);
                    }
                    println!(
                        "{}",
                        res.get("config_toml")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                    );
                }
                ConfigCommands::Set { key, value } => {
                    let res = curd::router::route_config_command(
                        &json!({"action":"set","key":key,"value":value}),
                        &ctx,
                    )
                    .await;
                    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
                        anyhow::bail!("{}", err);
                    }
                    println!(
                        "{}",
                        res.get("message").and_then(|v| v.as_str()).unwrap_or("")
                    );
                }
                ConfigCommands::Unset { key } => {
                    let res = curd::router::route_config_command(
                        &json!({"action":"unset","key":key}),
                        &ctx,
                    )
                    .await;
                    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
                        anyhow::bail!("{}", err);
                    }
                    println!(
                        "{}",
                        res.get("message").and_then(|v| v.as_str()).unwrap_or("")
                    );
                }
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::PluginLanguage { command, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let params = match command {
                PluginPackageCommands::List => json!({"action": "list"}),
                PluginPackageCommands::Add { archive_path } => {
                    json!({"action": "add", "archive_path": archive_path})
                }
                PluginPackageCommands::Remove { package_id } => {
                    json!({"action": "remove", "package_id": package_id})
                }
            };
            let result =
                curd::router::route_validated_tool_call("plugin_language", &params, &ctx, true)
                    .await;
            print_plugin_result(&result)?;
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::PluginTool { command, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let params = match command {
                PluginPackageCommands::List => json!({"action": "list"}),
                PluginPackageCommands::Add { archive_path } => {
                    json!({"action": "add", "archive_path": archive_path})
                }
                PluginPackageCommands::Remove { package_id } => {
                    json!({"action": "remove", "package_id": package_id})
                }
            };
            let result =
                curd::router::route_validated_tool_call("plugin_tool", &params, &ctx, true).await;
            print_plugin_result(&result)?;
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::PluginTrust { command, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let params = match command {
                PluginTrustCommands::List => json!({"action": "list"}),
                PluginTrustCommands::Get { key_id } => json!({"action": "get", "key_id": key_id}),
                PluginTrustCommands::Add(args) => json!({
                    "action": "add",
                    "key_id": args.key_id,
                    "pubkey_hex": args.pubkey_hex,
                    "label": args.label,
                    "allowed_kinds": args.kinds,
                }),
                PluginTrustCommands::Remove { key_id } => {
                    json!({"action": "remove", "key_id": key_id})
                }
                PluginTrustCommands::Enable { key_id } => {
                    json!({"action": "enable", "key_id": key_id})
                }
                PluginTrustCommands::Disable { key_id } => {
                    json!({"action": "disable", "key_id": key_id})
                }
            };
            let result =
                curd::router::route_validated_tool_call("plugin_trust", &params, &ctx, true).await;
            print_plugin_result(&result)?;
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Detach { root, shadow }) => {
            let resolved = resolve_workspace_root(root);
            println!("Performing soft detach...");
            let outcome =
                workspace_lifecycle::resolve_workspace_exit(&resolved, "detach", shadow, false)?;
            println!("{}", outcome.message);
            if !outcome.proceeded {
                return Ok(());
            }

            workspace_lifecycle::cleanup_detach_artifacts(&resolved);
            println!("CURD workspace soft-detached. Local `.curd/` data is preserved.");
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Delete { root, yes, shadow }) => {
            let resolved = resolve_workspace_root(root);
            let curd_dir = resolved.join(".curd");

            if !yes {
                let confirmation = dialoguer::Confirm::new()
                    .with_prompt("WARNING: This will permanently delete your local `.curd/` directory, history, and shadow index. Are you sure?")
                    .default(false)
                    .interact()
                    .unwrap_or(false);
                if !confirmation {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let outcome =
                workspace_lifecycle::resolve_workspace_exit(&resolved, "delete", shadow, yes)?;
            println!("{}", outcome.message);
            if !outcome.proceeded {
                return Ok(());
            }

            workspace_lifecycle::cleanup_detach_artifacts(&resolved);

            if curd_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&curd_dir) {
                    println!("Failed to delete CURD: {}", e);
                } else {
                    println!("Successfully deleted CURD from workspace.");
                }
            } else {
                println!("CURD is not initialized in this workspace.");
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Diagram {
            uris,
            root,
            format,
            depth,
        }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            let res = curd::router::route_validated_tool_call(
                "diagram",
                &json!({
                    "uris": uris,
                    "format": format,
                    "depth": depth
                }),
                &curd_core::EngineContext::new(&resolved.to_string_lossy()),
                true,
            )
            .await;

            if let Some(diag) = res
                .get("diagram")
                .and_then(|d| d.get("diagram"))
                .and_then(|v| v.as_str())
            {
                println!("{}", diag);
            } else {
                println!("{}", serde_json::to_string_pretty(&res)?);
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Test {
            scope,
            root,
            verbose,
        }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            println!("\x1b[1;36m=== CURD Semantic Integrity Audit ===\x1b[0m\n");
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let audit = curd::router::route_semantic_audit(
                &json!({
                    "scope": scope,
                    "verbose": verbose
                }),
                &ctx,
            )
            .await;
            if let Some(err) = audit.get("error").and_then(|v| v.as_str()) {
                anyhow::bail!("{}", err);
            }

            if let Some(node_coverage) = audit.get("node_coverage").filter(|v| !v.is_null()) {
                println!("\x1b[1m[Node Coverage]\x1b[0m");
                println!(
                    "  Symbols indexed: {}",
                    node_coverage
                        .get("symbols_indexed")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!(
                    "  Symbol Density:  {:.2} symbols/KLoC",
                    node_coverage
                        .get("symbol_density")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
                );

                if verbose {
                    let dead_zones = node_coverage
                        .get("dead_zones")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();

                    if !dead_zones.is_empty() {
                        println!("  \x1b[31mDead Zones (0 symbols indexed):\x1b[0m");
                        for dz in dead_zones.iter().take(10) {
                            println!("    • {}", dz.as_str().unwrap_or(""));
                        }
                        if dead_zones.len() > 10 {
                            println!("    ... and {} more", dead_zones.len() - 10);
                        }
                    }
                }

                if node_coverage
                    .get("symbol_density")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    < 5.0
                    && !verbose
                {
                    println!(
                        "  \x1b[33m⚠️  Low density detected. Run with --verbose to see dead zones.\x1b[0m"
                    );
                }
                println!();
            }

            if let Some(edge_connectivity) = audit.get("edge_connectivity").filter(|v| !v.is_null())
            {
                println!("\x1b[1m[Edge Connectivity]\x1b[0m");
                let percentage = (edge_connectivity
                    .get("cohesion_ratio")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    * 100.0) as u32;
                let color = if percentage > 90 {
                    "\x1b[32m"
                } else if percentage > 70 {
                    "\x1b[33m"
                } else {
                    "\x1b[31m"
                };

                println!(
                    "  Cohesion Ratio: {}{}%{}\x1b[0m",
                    color,
                    percentage,
                    if percentage > 90 { " (Optimal)" } else { "" }
                );
                println!(
                    "  Broken Links:   {}",
                    edge_connectivity
                        .get("broken_links")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );

                let broken = edge_connectivity
                    .get("top_unresolved_linkages")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                if verbose && !broken.is_empty() {
                    println!("  \x1b[31mTop Unresolved Linkages:\x1b[0m");
                    for b in broken.iter().take(10) {
                        println!("    • {}", b.as_str().unwrap_or(""));
                    }
                    if broken.len() > 10 {
                        println!("    ... and {} more", broken.len() - 10);
                    }
                }

                println!(
                    "  Resolution:     {}/{} @stubs resolved",
                    edge_connectivity
                        .get("resolved_stubs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    edge_connectivity
                        .get("total_stubs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!();

                println!("\x1b[1m[Confidence Intervals]\x1b[0m");
                let confidence = edge_connectivity
                    .get("confidence_distribution")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                println!(
                    "  \x1b[32mHigh (>=90%):\x1b[0m   {}",
                    confidence.get("high").and_then(|v| v.as_u64()).unwrap_or(0)
                );
                println!(
                    "  \x1b[33mMedium (>=70%):\x1b[0m {}",
                    confidence
                        .get("medium")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                );
                println!(
                    "  \x1b[31mLow (<70%):\x1b[0m    {}",
                    confidence.get("low").and_then(|v| v.as_u64()).unwrap_or(0)
                );
                println!();
            }

            if let Some(architecture) = audit.get("architecture_audit").filter(|v| !v.is_null()) {
                println!("\x1b[1m[Architecture Audit]\x1b[0m");
                let cycles = architecture
                    .get("cycles")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                if cycles.is_empty() {
                    println!("  \x1b[32mNo cyclical dependencies detected.\x1b[0m");
                } else {
                    println!(
                        "  \x1b[31mDetected {} Tangled Clusters (Cycles):\x1b[0m",
                        cycles.len()
                    );
                    for (i, cycle) in cycles.iter().enumerate().take(5) {
                        let cycle_nodes = cycle.as_array().cloned().unwrap_or_default();
                        println!("    Cluster {}: {} nodes", i + 1, cycle_nodes.len());
                        if verbose {
                            for node in cycle_nodes.iter().take(5) {
                                println!("      - {}", node.as_str().unwrap_or(""));
                            }
                            if cycle_nodes.len() > 5 {
                                println!("      ... and {} more", cycle_nodes.len() - 5);
                            }
                        }
                    }
                }
                println!();
            }

            if let Some(policy_validation) = audit.get("policy_validation").filter(|v| !v.is_null())
            {
                println!("\x1b[1m[Policy Validation]\x1b[0m");
                if policy_validation
                    .get("pass")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    println!(
                        "  \x1b[32mPASS:\x1b[0m PolicyEngine correctly blocked illegal access."
                    );
                    if verbose {
                        println!("    Blocked: edit .curd/settings.toml");
                        println!(
                            "    Reason:  {}",
                            policy_validation
                                .get("reason")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                        );
                    }
                } else {
                    println!(
                        "  \x1b[31mFAIL:\x1b[0m PolicyEngine FAILED to block illegal access to configuration."
                    );
                }
                println!();
            }

            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Status { root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            println!("=== CURD Workspace Status ===\n");
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let status = curd::router::route_workspace_status(&ctx).await;
            if let Some(stats) = status.get("index").filter(|v| !v.is_null()) {
                println!("[Index]");
                println!(
                    "  Files Scanned: {}",
                    stats
                        .get("total_files")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                );
                println!(
                    "  Cache Hits:    {}",
                    stats
                        .get("cache_hits")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                );
                println!(
                    "  Parse Fail:    {}",
                    stats
                        .get("parse_fail")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                );
                println!(
                    "  No Symbols:    {}",
                    stats
                        .get("no_symbols")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                );
                println!(
                    "  Build Time:    {}ms",
                    stats.get("total_ms").and_then(|v| v.as_i64()).unwrap_or(0)
                );
            } else {
                println!("[Index]");
                println!("  No index stats available. Run `curd doctor` to seed the index.");
            }
            println!();

            let shadow = status.get("shadow").cloned().unwrap_or_else(|| json!({}));
            if shadow
                .get("active")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                let staged = shadow
                    .get("staged_paths")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                println!("[Shadow Store: ACTIVE]");
                if staged.is_empty() {
                    println!("  No staged changes.");
                } else {
                    println!("  Staged Files:");
                    for path in staged {
                        println!("    - {}", path.as_str().unwrap_or(""));
                    }
                    println!(
                        "\n  Run `curd diff` to view changes or `curd workspace commit` via agent to apply."
                    );
                }
            } else {
                println!("[Shadow Store: INACTIVE]");
                println!("  No active transaction.");
            }

            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Log { root, limit }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let res = curd::router::route_history(
                &json!({
                    "mode": "operations",
                    "limit": limit
                }),
                &ctx,
            )
            .await;
            let entries = res
                .get("history")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            if entries.is_empty() {
                println!("No agent history found in `.curd/traces/`.");
            } else {
                for entry in entries {
                    println!(
                        "Timestamp: {}",
                        entry
                            .get("timestamp_unix")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)
                    );
                    println!(
                        "Collab:    {}",
                        entry
                            .get("collaboration_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                    );
                    println!(
                        "Operation: {}",
                        entry
                            .get("operation")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                    );
                    if let Ok(pretty) = serde_json::to_string_pretty(
                        entry.get("input").unwrap_or(&serde_json::Value::Null),
                    ) {
                        println!("Input:\n{}\n", pretty);
                    }
                    if let Ok(pretty) = serde_json::to_string_pretty(
                        entry.get("output").unwrap_or(&serde_json::Value::Null),
                    ) {
                        println!("Output:\n{}\n", pretty);
                    }
                    println!("--------------------------------------------------");
                }
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Repl { root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            repl::run_repl(&resolved).await
        }
        #[cfg(feature = "core")]
        Some(Commands::Run {
            first,
            second,
            root,
            args,
            profile,
            out,
        }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let arg_overrides = parse_script_arg_overrides(&args)?;
            let (mode, script) = match first.as_str() {
                "check" | "compile" => (
                    first.as_str(),
                    PathBuf::from(second.ok_or_else(|| anyhow::anyhow!("missing script path"))?),
                ),
                _ => ("run", PathBuf::from(first)),
            };
            let res = match mode {
                "check" => curd::router::route_check_script(&script, &arg_overrides, &ctx).await,
                "compile" => {
                    curd::router::route_compile_script(
                        &script,
                        &arg_overrides,
                        &json!({
                            "profile": profile,
                            "out": out
                        }),
                        &ctx,
                    )
                    .await
                }
                _ => {
                    curd::router::route_run_script(
                        &script,
                        &arg_overrides,
                        &json!({
                            "profile": profile
                        }),
                        &ctx,
                        Some({
                            let mut state = curd_core::ReplState::new();
                            state.is_human_actor = true;
                            state
                        }),
                    )
                    .await
                    .0
                }
            };
            println!("{}", serde_json::to_string_pretty(&res)?);
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Search { query, kind, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let res = curd::router::route_validated_tool_call(
                "search",
                &json!({
                    "query": query,
                    "mode": "symbol",
                    "kind": kind,
                    "limit": 20
                }),
                &ctx,
                true,
            )
            .await;
            println!("{}", serde_json::to_string_pretty(&res)?);
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Graph { uri, depth, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let res = curd::router::route_validated_tool_call(
                "graph",
                &json!({
                    "uris": [uri],
                    "depth": depth,
                    "direction": "both"
                }),
                &ctx,
                true,
            )
            .await;
            println!("{}", serde_json::to_string_pretty(&res)?);
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Read { uri, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let mut current_uri = uri.clone();

            loop {
                let routed = curd::router::route_validated_tool_call(
                    "read",
                    &json!({
                        "uris": [current_uri.clone()],
                        "verbosity": 1
                    }),
                    &ctx,
                    true,
                )
                .await;

                if let Some(err) = routed.get("error").and_then(|v| v.as_str()) {
                    anyhow::bail!("Read failed: {}", err);
                }

                let results = routed
                    .get("results")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                if let Some(res) = results.first() {
                    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
                        use std::io::IsTerminal;
                        if err.contains("is overloaded")
                            && std::io::stdout().is_terminal()
                            && let Some(options_idx) = err.find("Available options:\n")
                        {
                            let options_str = &err[options_idx + 19..];
                            let lines: Vec<&str> = options_str.lines().collect();

                            println!("\n{}", &err[..options_idx].trim());
                            if let Ok(selection) = dialoguer::Select::new()
                                .with_prompt("Select which overload to read:")
                                .items(&lines)
                                .default(0)
                                .interact()
                            {
                                let selected = lines[selection];
                                if let Some(colon_idx) = selected.find(": ") {
                                    current_uri = selected[..colon_idx].to_string();
                                    println!(
                                        "Resubmitting with targeted overload: {}...",
                                        current_uri
                                    );
                                    continue;
                                }
                            }
                        }
                        anyhow::bail!("Read failed: {}", err);
                    }

                    if let Some(source) = res
                        .get("source")
                        .and_then(|v: &serde_json::Value| v.as_str())
                    {
                        // Try to pipe to PAGER
                        let pager =
                            std::env::var("PAGER").unwrap_or_else(|_| "less -R".to_string());
                        let mut cmd_iter = pager.split_whitespace();
                        if let Some(program) = cmd_iter.next() {
                            use std::io::Write;
                            let mut child = Command::new(program)
                                .args(cmd_iter)
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                                .unwrap_or_else(|_| {
                                    // Fallback to cat
                                    Command::new("cat")
                                        .stdin(std::process::Stdio::piped())
                                        .spawn()
                                        .unwrap_or_else(|e| {
                                            eprintln!("Failed to spawn fallback pager: {}", e);
                                            std::process::exit(1);
                                        })
                                });

                            if let Some(mut stdin) = child.stdin.take() {
                                let _ = stdin.write_all(source.as_bytes());
                            }
                            child.wait()?;
                        } else {
                            println!("{}", source);
                        }
                    } else {
                        println!("{}", serde_json::to_string_pretty(res)?);
                    }
                }
                break;
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Edit {
            uri,
            action,
            code,
            base_state_hash,
            root,
        }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let mut current_uri = uri.clone();

            loop {
                let routed = curd::router::route_validated_tool_call(
                    "edit",
                    &json!({
                        "uri": current_uri.clone(),
                        "action": action.clone(),
                        "code": code.clone(),
                        "base_state_hash": base_state_hash.clone(),
                        "adaptation_justification": "CLI edit"
                    }),
                    &ctx,
                    true,
                )
                .await;

                if let Some(message) = routed.get("message") {
                    println!("Edit applied: {}", serde_json::to_string_pretty(message)?);
                    break;
                }

                let err_msg = routed
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Edit failed")
                    .to_string();
                {
                    use std::io::IsTerminal;
                    if err_msg.contains("is overloaded")
                        && std::io::stdout().is_terminal()
                        && let Some(options_idx) = err_msg.find("Available options:\n")
                    {
                        let options_str = &err_msg[options_idx + 19..];
                        let lines: Vec<&str> = options_str.lines().collect();

                        println!("\n{}", &err_msg[..options_idx].trim());
                        if let Ok(selection) = dialoguer::Select::new()
                            .with_prompt("Select which overload to edit:")
                            .items(&lines)
                            .default(0)
                            .interact()
                        {
                            let selected = lines[selection];
                            if let Some(colon_idx) = selected.find(": ") {
                                current_uri = selected[..colon_idx].to_string();
                                println!("Resubmitting with targeted overload: {}...", current_uri);
                                continue;
                            }
                        }
                    }
                    anyhow::bail!("Edit failed: {}", err_msg);
                }
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Session { command, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
            let shadow = curd_core::ShadowStore::new(&resolved);
            match command {
                SessionCommands::Begin => {
                    let res = curd::router::route_session_lifecycle("begin", &ctx).await;
                    if let Some(err) = res.get("error") {
                        anyhow::bail!("{}", serde_json::to_string(err)?);
                    }
                    println!("Started new shadow transaction.");
                }
                SessionCommands::Review => {
                    let res = curd::router::route_validated_tool_call(
                        "session",
                        &json!({"action":"review"}),
                        &ctx,
                        true,
                    )
                    .await;
                    println!("{}", serde_json::to_string_pretty(&res)?);
                }
                SessionCommands::Log => {
                    let shadow_id = shadow.get_transaction_id();
                    let items = curd::router::route_history(
                        &json!({
                            "mode": "operations",
                            "limit": 100
                        }),
                        &ctx,
                    )
                    .await
                    .get("history")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                    let filtered: Vec<_> = items
                        .into_iter()
                        .filter(|e| {
                            e.get("transaction_id")
                                .and_then(|v| v.as_str())
                                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                                == shadow_id
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&filtered)?);
                }
                SessionCommands::Commit => {
                    if !shadow.is_active() {
                        anyhow::bail!("No active shadow transaction to commit.");
                    }
                    let res = curd::router::route_session_lifecycle("commit", &ctx).await;
                    if let Some(err) = res.get("error") {
                        anyhow::bail!("{}", serde_json::to_string(err)?);
                    }
                    println!("Committed shadow changes to disk and ended review cycle.");
                }
                SessionCommands::Rollback => {
                    if !shadow.is_active() {
                        anyhow::bail!("No active shadow transaction to rollback.");
                    }
                    let res = curd::router::route_session_lifecycle("rollback", &ctx).await;
                    if let Some(err) = res.get("error") {
                        anyhow::bail!("{}", serde_json::to_string(err)?);
                    }
                    println!("Discarded active shadow transaction and ended review cycle.");
                }
            }
            Ok(())
        }
        #[cfg(feature = "mcp")]
        Some(Commands::Plan { command, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;

            let plans_dir = resolved.join(".curd").join("plans");

            match command {
                PlanCommands::List => {
                    if !plans_dir.exists() {
                        println!("No plans found. Directory `.curd/plans` does not exist.");
                        return Ok(());
                    }
                    println!("Saved Plans:");
                    for entry in fs::read_dir(plans_dir)?.flatten() {
                        if let Some(name) = entry.file_name().to_str()
                            && name.ends_with(".json")
                        {
                            println!("  - {}", name.trim_end_matches(".json"));
                        }
                    }
                }
                PlanCommands::Read { id } => {
                    let plan_file = plans_dir.join(format!("{}.json", id));
                    if !plan_file.exists() {
                        anyhow::bail!("Plan not found: {}", id);
                    }
                    let (plan, artifact_meta) =
                        curd::plan_artifact::load_plan_artifact(&plan_file)?;
                    println!("Plan ID: {}", plan.id);
                    println!("Total Nodes: {}", plan.nodes.len());
                    if let Some(meta) = artifact_meta {
                        println!(
                            "Source Kind: {}",
                            meta["source_kind"].as_str().unwrap_or("")
                        );
                        if let Some(profile) = meta["metadata"]["profile"].as_str() {
                            println!("Profile: {}", profile);
                        }
                        if !meta["explainability"].is_null() {
                            println!(
                                "Explainability: {}",
                                serde_json::to_string_pretty(&meta["explainability"])?
                            );
                        }
                    }
                    println!("---");

                    // Display topological sort or basic list
                    for node in &plan.nodes {
                        let op_str = match &node.op {
                            curd_core::plan::ToolOperation::McpCall { tool, args } => format!(
                                "Tool: {} \nArgs: {}",
                                tool,
                                serde_json::to_string_pretty(args).unwrap_or_default()
                            ),
                            curd_core::plan::ToolOperation::Internal { command, params } => {
                                format!(
                                    "Internal: {} \nParams: {}",
                                    command,
                                    serde_json::to_string_pretty(params).unwrap_or_default()
                                )
                            }
                        };
                        println!("[Node ID: {}]", node.id);
                        if !node.dependencies.is_empty() {
                            println!("Depends On: {:?}", node.dependencies);
                        }
                        println!("{}", op_str);
                        println!("---");
                    }
                }
                PlanCommands::Edit { id } => {
                    let plan_file = plans_dir.join(format!("{}.json", id));
                    if !plan_file.exists() {
                        anyhow::bail!("Plan not found: {}", id);
                    }
                    let (mut plan, mut artifact_meta) =
                        curd::plan_artifact::load_plan_artifact(&plan_file)?;

                    let current_profile = artifact_meta
                        .as_ref()
                        .and_then(|m| m.get("metadata"))
                        .and_then(|m| m.get("profile"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let profile_input: String = dialoguer::Input::new()
                        .with_prompt("Profile override for this plan artifact")
                        .default(current_profile.to_string())
                        .allow_empty(true)
                        .interact_text()?;
                    if let Some(meta) = artifact_meta.as_mut()
                        && let Some(metadata) =
                            meta.get_mut("metadata").and_then(|v| v.as_object_mut())
                    {
                        metadata.insert(
                            "profile".to_string(),
                            if profile_input.trim().is_empty() {
                                serde_json::Value::Null
                            } else {
                                json!(profile_input.trim())
                            },
                        );
                    }

                    let default_output_limit = plan
                        .nodes
                        .first()
                        .map(|n| n.output_limit)
                        .unwrap_or(64 * 1024);
                    let output_limit: usize = dialoguer::Input::new()
                        .with_prompt("Default output_limit for all plan nodes")
                        .default(default_output_limit)
                        .interact_text()?;
                    for node in &mut plan.nodes {
                        node.output_limit = output_limit;
                    }

                    let default_retry_limit =
                        plan.nodes.first().map(|n| n.retry_limit).unwrap_or(0);
                    let retry_limit: u8 = dialoguer::Input::new()
                        .with_prompt("Default retry_limit for all plan nodes")
                        .default(default_retry_limit)
                        .interact_text()?;
                    for node in &mut plan.nodes {
                        node.retry_limit = retry_limit;
                    }

                    let per_node = dialoguer::Confirm::new()
                        .with_prompt("Edit node-specific settings?")
                        .default(false)
                        .interact()?;
                    if per_node {
                        for node in &mut plan.nodes {
                            let label = match &node.op {
                                curd_core::plan::ToolOperation::McpCall { tool, .. } => {
                                    tool.clone()
                                }
                                curd_core::plan::ToolOperation::Internal { command, .. } => {
                                    format!("internal:{}", command)
                                }
                            };
                            let prompt = format!("Adjust node {} ({})?", node.id, label);
                            if !dialoguer::Confirm::new()
                                .with_prompt(prompt)
                                .default(false)
                                .interact()?
                            {
                                continue;
                            }
                            node.output_limit = dialoguer::Input::new()
                                .with_prompt("  output_limit")
                                .default(node.output_limit)
                                .interact_text()?;
                            node.retry_limit = dialoguer::Input::new()
                                .with_prompt("  retry_limit")
                                .default(node.retry_limit)
                                .interact_text()?;
                        }
                    }

                    curd::plan_artifact::save_plan_artifact(&plan_file, &plan, artifact_meta)?;
                    println!("Updated plan artifact {}", id);
                }
                PlanCommands::Impl {
                    id,
                    session,
                    dry_run,
                } => {
                    let plan_file = plans_dir.join(format!("{}.json", id));
                    if !plan_file.exists() {
                        anyhow::bail!("Plan not found: {}", id);
                    }
                    let (plan, _) = curd::plan_artifact::load_plan_artifact(&plan_file)?;
                    let plan_json: serde_json::Value = serde_json::to_value(&plan)?;

                    if dry_run {
                        println!("Simulating Plan {}...", id);
                        let ctx = curd_core::EngineContext::new(&resolved.to_string_lossy());
                        let simulate_params = serde_json::json!({
                            "mode": "execute_plan",
                            "plan": plan_json
                        });
                        let result = curd::router::route_validated_tool_call(
                            "simulate",
                            &simulate_params,
                            &ctx,
                            true,
                        )
                        .await;
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("Executing Plan {} under Session {}...", id, session);
                        let plan: curd_core::Plan = serde_json::from_value(plan_json)?;
                        let _session_uuid = uuid::Uuid::parse_str(&session)
                            .map_err(|_| anyhow::anyhow!("Invalid session UUID format"))?;

                        let ctx = curd_core::context::EngineContext::new(
                            resolved.to_string_lossy().as_ref(),
                        );
                        {
                            let mut state = ctx.global_state.lock().await;
                            state.active_plan = Some(plan);
                        }

                        let result = curd::router::route_validated_tool_call(
                            "execute_active_plan",
                            &serde_json::json!({}),
                            &ctx,
                            true,
                        )
                        .await;

                        if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
                            anyhow::bail!("Plan execution failed: {}", err);
                        }
                        println!(
                            "Plan execution completed successfully:\n{}",
                            serde_json::to_string_pretty(&result)?
                        );
                    }
                }
            }
            Ok(())
        }
        Some(Commands::External(args)) => {
            let _cmd = args.join(" ");
            eprintln!(
                "\x1b[31merror\x1b[0m: unrecognized subroutine '\x1b[33m{}\x1b[0m'",
                _cmd
            );
            eprintln!("\x1b[2mRun `curd help` for a list of available commands.\x1b[0m");
            std::process::exit(1);
        }
        Some(Commands::H) => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            Ok(())
        }
        None => {
            // If legacy path provided, use MCP (only when mcp feature enabled)
            #[cfg(feature = "mcp")]
            if let Some(root) = cli.legacy_root {
                if !root.exists() && root.to_string_lossy() != "." {
                    eprintln!(
                        "\x1b[31merror\x1b[0m: unrecognized subroutine '\x1b[33m{}\x1b[0m'",
                        root.to_string_lossy()
                    );
                    eprintln!("\x1b[2mRun `curd help` for a list of available commands.\x1b[0m");
                    std::process::exit(1);
                }
                let resolved = resolve_workspace_root(root);
                enforce_workspace_config(&resolved)?;
                let server = McpServer::new(&resolved.to_string_lossy());
                return server.run().await;
            }

            use clap::CommandFactory;
            Cli::command().print_help()?;
            #[cfg(not(feature = "mcp"))]
            println!("\n\nNote: Built without MCP support. Use 'curd doctor' or 'curd build'.");
            Ok(())
        }
    }
}

fn resolve_workspace_root(root: PathBuf) -> PathBuf {
    // Explicit env override for predictable automation.
    if let Ok(env_root) = std::env::var("CURD_WORKSPACE_ROOT") {
        let p = PathBuf::from(env_root);
        return std::fs::canonicalize(&p).unwrap_or(p);
    }

    // If caller passed an explicit root that isn't ".", respect it immediately
    // BEFORE falling back to Git top-level logic.
    let root_str = root.to_string_lossy();
    if root_str != "." && !root_str.is_empty() {
        return std::fs::canonicalize(&root).unwrap_or(root);
    }

    // Otherwise, prefer git top-level so running from subdirs still anchors .curd at repo root.
    if let Ok(out) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            let p = PathBuf::from(s);
            return std::fs::canonicalize(&p).unwrap_or(p);
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn parse_script_arg_overrides(
    args: &[String],
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut map = serde_json::Map::new();
    for item in args {
        let Some((key, raw)) = item.split_once('=') else {
            anyhow::bail!("invalid script arg '{}'; expected key=value", item);
        };
        let parsed = serde_json::from_str::<serde_json::Value>(raw)
            .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
        map.insert(key.to_string(), parsed);
    }
    Ok(map)
}

fn print_plugin_result(result: &serde_json::Value) -> Result<()> {
    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        anyhow::bail!("{}", err);
    }
    println!("{}", serde_json::to_string_pretty(result)?);
    Ok(())
}

fn setup_tracing() {
    // Placeholder for telemetry/logging initialization.
}

fn enforce_workspace_config(root: &std::path::Path) -> Result<()> {
    curd_core::validate_workspace_config(root)
}
