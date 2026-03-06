use anyhow::Result;
use clap::{Parser, Subcommand};
use curd_core::{BuildRequest, DoctorIndexConfig, DoctorProfile, DoctorThresholds};
#[cfg(feature = "mcp")]
use curd_core::McpServer;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "core")]
pub mod doctor;
#[cfg(feature = "mcp")]
pub mod init;
#[cfg(feature = "core")]
pub mod workspace_init;
#[cfg(feature = "core")]
pub mod repl;

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
    Doctor(Box<DoctorArgs>),
    /// Build via CURD control plane (adapter-based dry-run/execute)
    #[cfg(feature = "core")]
    Build {
        /// Path to workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
        /// Build adapter override (cargo|cmake|ninja|make)
        #[arg(long)]
        adapter: Option<String>,
        /// Build profile (debug|release)
        #[arg(long)]
        profile: Option<String>,
        /// Optional build target
        #[arg(long)]
        target: Option<String>,
        /// Execute planned commands (default is dry-run plan only)
        #[arg(long)]
        execute: bool,
        /// Trailing arguments to pass directly to the underlying compiler/adapter
        #[arg(last = true)]
        trailing_args: Vec<String>,
    },
    /// Print a shell hook to implicitly route standard commands (like 'make') through CURD
    #[cfg(feature = "core")]
    Hook {
        /// The target shell (zsh, bash, fish)
        #[arg(value_parser = ["zsh", "bash", "fish"])]
        shell: String,
    },
    /// Compare symbols at AST level
    #[cfg(feature = "core")]
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
    Refactor {
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
        
        #[command(subcommand)]
        command: RefactorCommands,
    },
    /// Manage external workspace contexts (Read-Only or Semantic linking)
    #[cfg(feature = "core")]
    Context {
        /// Path to the primary workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,

        #[command(subcommand)]
        command: ContextCommands,
    },
    /// Initialize CURD workspace: auto-detect build system, create .curd/ directory
    #[cfg(feature = "core")]
    Init {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Print a summary of the current workspace state (index stats, shadow changes, etc.)
    #[cfg(feature = "core")]
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
    Repl {
        /// Path to the workspace root
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Semantic search across the indexed graph
    #[cfg(feature = "core")]
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
    Read {
        /// The URI to read (e.g., 'src/main.rs' or 'src/main.rs::my_function')
        uri: String,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Manually mutate a file or symbol via the AST-aware EditEngine
    #[cfg(feature = "core")]
    Edit {
        /// The URI to edit
        uri: String,
        /// The action to perform ('upsert' or 'delete')
        #[arg(short, long, default_value = "upsert")]
        action: String,
        /// The literal code to insert (ignored if action is 'delete')
        #[arg(short, long, default_value = "")]
        code: String,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Manage manual ShadowStore transaction sessions
    #[cfg(feature = "core")]
    Session {
        #[command(subcommand)]
        command: SessionCommands,
        /// Path to the workspace root
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    #[cfg(feature = "mcp")]
    /// Manage, inspect, and execute saved AI plans
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
}

#[derive(Subcommand)]
pub enum RefactorCommands {
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
pub enum ContextCommands {
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
pub enum SessionCommands {
    /// Open a new shadow transaction
    Begin,
    /// Commit the active shadow transaction to disk
    Commit,
    /// Discard the active shadow transaction
    Rollback,
}

#[cfg(feature = "mcp")]
#[derive(Subcommand)]
pub enum PlanCommands {
    /// List all saved plans in the workspace
    List,
    /// Read a specific plan and format it as a readable execution tree
    Read {
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
    // Initialize tracing
    setup_tracing();

    #[cfg(target_os = "windows")]
    {
        eprintln!("\n{}", "!" .repeat(80));
        eprintln!("WARNING: Native sandboxing is not yet supported on Windows.");
        eprintln!("Interactive agent tools that execute shell commands will run WITHOUT isolation.");
        eprintln!("Please exercise caution when using agentic features on this platform.");
        eprintln!("{}\n", "!" .repeat(80));
    }

    let cli = Cli::parse();
    if let Ok(exe) = std::env::current_exe() {
        // SAFETY: process-wide static initialization at startup.
        unsafe {
            std::env::set_var("CURD_INDEX_WORKER_BIN", exe);
        }
    }

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
        Some(Commands::Doctor(args)) => {
            let resolved = resolve_workspace_root(args.root.clone());
            doctor::run_doctor(
                &resolved,
                args.strict || args.profile.is_some(),
                DoctorThresholds {
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
                args.profile.as_deref().and_then(|s: &str| s.parse::<DoctorProfile>().ok()),
                DoctorIndexConfig {
                    index_mode: args.index_mode.clone(),
                    index_scope: args.index_scope.clone(),
                    index_max_file_size: args.index_max_file_size,
                    index_large_file_policy: args.index_large_file_policy.clone(),
                    index_execution: args.index_execution.clone(),
                    index_chunk_size: args.index_chunk_size,
                    compare_with_full: args.compare_with_full,
                    profile_index: args.profile_index,
                    report_out: args.report_out.clone(),
                },
            )
        }
        #[cfg(feature = "core")]
        Some(Commands::Build {
            root,
            adapter,
            profile,
            target,
            execute,
            trailing_args,
        }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let out = curd_core::run_build(
                &resolved,
                BuildRequest {
                    adapter,
                    profile,
                    target,
                    execute,
                    trailing_args,
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&out)?);
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

            match shell.as_str() {
                "zsh" | "bash" => println!("{}", zsh_bash),
                "fish" => println!("{}", fish),
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
            let out = curd_core::run_diff(&resolved, semantic, symbol)?;
            println!("{}", out);
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Refactor { root, command }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let action = match command {
                RefactorCommands::Rename { symbol, new_name, lsp } => curd_core::RefactorAction::Rename { symbol, new_name, lsp_binary: lsp },
                RefactorCommands::Move { symbol, target_file } => curd_core::RefactorAction::Move { symbol, target_file },
                RefactorCommands::Extract { file_range, new_function_name } => curd_core::RefactorAction::Extract { file_range, new_function_name },
            };
            
            match curd_core::run_refactor(&resolved, action.clone()) {
                Ok(out) => {
                    println!("{}", out);
                }
                Err(e) => {
                        let err_msg = e.to_string();
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
                                            match curd_core::run_refactor(&resolved, sub_action) {
                                                Ok(out) => println!("{}", out),
                                                Err(e) => anyhow::bail!("curd refactor failed on {}: {}", full_id_with_line, e),
                                            }
                                        }
                                    }
                                    return Ok(());
                                }
                            }
                        }
                        anyhow::bail!("curd refactor failed: {}", e);
                    }
                }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Context { root, command }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            let mut registry = curd_core::ContextRegistry::load(&resolved);
            match command {
                ContextCommands::Add { path, alias, index, read } => {
                    let mut ext_path = path;
                    if !ext_path.is_absolute() {
                        ext_path = std::env::current_dir()?.join(ext_path);
                    }
                    ext_path = std::fs::canonicalize(ext_path)?;
                    if !ext_path.exists() {
                        anyhow::bail!("Path does not exist: {}", ext_path.display());
                    }
                    let mode = if index {
                        curd_core::ContextMode::Index
                    } else if read {
                        curd_core::ContextMode::Read
                    } else {
                        curd_core::ContextMode::Write
                    };
                    let alias_name = alias.unwrap_or_else(|| {
                        format!("@{}", ext_path.file_name().and_then(|n| n.to_str()).unwrap_or("ext"))
                    });
                    registry.add(alias_name.clone(), ext_path.clone(), mode.clone());
                    registry.save(&resolved)?;
                    println!("Linked context `{}` -> {} (Mode: {:?})", alias_name, ext_path.display(), mode);
                }
                ContextCommands::Remove { name, force } => {
                    if !force {
                        // For now we just emit a warning if not forced, but we will wire the graph up
                        println!("Checking dependency graph for dangling edges to {}...", name);
                        let graph = curd_core::GraphEngine::new(&resolved);
                        let dep_graph = graph.build_dependency_graph()?;
                        let mut has_dangling = false;
                        for (caller, callees) in dep_graph.outgoing {
                            for callee in callees {
                                if callee.starts_with(&name) {
                                    println!("WARNING: `{}` calls `{}`", caller, callee);
                                    has_dangling = true;
                                }
                            }
                        }
                        if has_dangling {
                            anyhow::bail!("Cannot remove context. Your primary workspace is actively calling functions in this repository. Dependency resolution is a strictly human action—you must manually import or vendor the source code before removing the contextual link. Use --force to override.");
                        }
                    }
                    if registry.remove(&name) {
                        registry.save(&resolved)?;
                        println!("Removed context `{}`", name);
                    } else {
                        println!("Context `{}` not found.", name);
                    }
                }
                ContextCommands::List => {
                    if registry.contexts.is_empty() {
                        println!("No contexts linked. Use `curd context add <dir>` to link an external repository.");
                    } else {
                        println!("Linked Contexts:");
                        for (alias, link) in &registry.contexts {
                            println!("  {} -> {} (Mode: {:?})", alias, link.path.display(), link.mode);
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
        Some(Commands::Status { root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            
            println!("=== CURD Workspace Status ===\n");
            
            // 1. Index Status
            let search = curd_core::SearchEngine::new(&resolved);
            if let Some(stats) = search.last_index_stats() {
                println!("[Index]");
                println!("  Files Scanned: {}", stats.total_files);
                println!("  Cache Hits:    {}", stats.cache_hits);
                println!("  Parse Fail:    {}", stats.parse_fail);
                println!("  No Symbols:    {}", stats.no_symbols);
                println!("  Build Time:    {}ms", stats.total_ms);
            } else {
                println!("[Index]");
                println!("  No index stats available. Run `curd doctor` to seed the index.");
            }
            println!();

            // 2. Shadow Store
            let shadow = curd_core::ShadowStore::new(&resolved);
            if shadow.is_active() {
                let staged = shadow.staged_paths();
                println!("[Shadow Store: ACTIVE]");
                if staged.is_empty() {
                    println!("  No staged changes.");
                } else {
                    println!("  Staged Files:");
                    for path in staged {
                        println!("    - {}", path.display());
                    }
                    println!("\n  Run `curd diff` to view changes or `curd workspace commit` via agent to apply.");
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
            
            let history = curd_core::HistoryEngine::new(&resolved);
            let entries = history.get_history(limit);
            
            if entries.is_empty() {
                println!("No agent history found in `.curd/traces/`.");
            } else {
                for entry in entries {
                    println!("Timestamp: {}", entry.timestamp_unix);
                    println!("Session:   {}", entry.session_id);
                    println!("Operation: {}", entry.operation);
                    if let Ok(pretty) = serde_json::to_string_pretty(&entry.input) {
                        println!("Input:\n{}\n", pretty);
                    }
                    if let Ok(pretty) = serde_json::to_string_pretty(&entry.output) {
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
        Some(Commands::Search { query, kind, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            
            let se = curd_core::SearchEngine::new(&resolved);
            // Quick mapping from string to SymbolKind
            let kind_filter = kind.and_then(|k| serde_json::from_value(serde_json::json!(k)).ok());
            let results = se.search(&query, kind_filter)?;
            println!("{}", serde_json::to_string_pretty(&results)?);
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Graph { uri, depth, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            
            let ge = curd_core::GraphEngine::new(&resolved);
            let results = ge.graph_with_depths(vec![uri.clone()], depth as u8, depth as u8)?;
            println!("{}", serde_json::to_string_pretty(&results)?);
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Read { uri, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            
            let re = curd_core::ReadEngine::new(&resolved);
            let mut current_uri = uri.clone();
            
            loop {
                let results = re.read(vec![current_uri.clone()], 1)?;
                
                if let Some(res) = results.first() {
                    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
                        use std::io::IsTerminal;
                        if err.contains("is overloaded") && std::io::stdout().is_terminal()
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
                                        println!("Resubmitting with targeted overload: {}...", current_uri);
                                        continue;
                                    }
                                }
                            }
                        anyhow::bail!("Read failed: {}", err);
                    }
                    
                    if let Some(source) = res.get("source").and_then(|v: &serde_json::Value| v.as_str()) {
                        // Try to pipe to PAGER
                        let pager = std::env::var("PAGER").unwrap_or_else(|_| "less -R".to_string());
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
                                        .expect("Failed to spawn fallback pager")
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
        Some(Commands::Edit { uri, action, code, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            
            let ee = curd_core::EditEngine::new(&resolved);
            let mut current_uri = uri.clone();
            
            loop {
                match ee.edit(&current_uri, &code, &action, None) {
                    Ok(result) => {
                        println!("Edit applied: {}", serde_json::to_string_pretty(&result)?);
                        break;
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        use std::io::IsTerminal;
                        if err_msg.contains("is overloaded") && std::io::stdout().is_terminal()
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
                        anyhow::bail!("Edit failed: {}", e);
                    }
                }
            }
            Ok(())
        }
        #[cfg(feature = "core")]
        Some(Commands::Session { command, root }) => {
            let resolved = resolve_workspace_root(root);
            enforce_workspace_config(&resolved)?;
            
            let mut shadow = curd_core::ShadowStore::new(&resolved);
            match command {
                SessionCommands::Begin => {
                    shadow.begin()?;
                    println!("Started new shadow transaction.");
                }
                SessionCommands::Commit => {
                    if !shadow.is_active() {
                        anyhow::bail!("No active shadow session to commit.");
                    }
                    shadow.commit()?;
                    println!("Committed shadow changes to disk.");
                }
                SessionCommands::Rollback => {
                    if !shadow.is_active() {
                        anyhow::bail!("No active shadow session to rollback.");
                    }
                    shadow.rollback();
                    println!("Discarded active shadow transaction.");
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
                    let content = fs::read_to_string(&plan_file)?;
                    let plan: curd_core::Plan = serde_json::from_str(&content)?;
                    println!("Plan ID: {}", plan.id);
                    println!("Total Nodes: {}", plan.nodes.len());
                    println!("---");
                    
                    // Display topological sort or basic list
                    for node in &plan.nodes {
                        let op_str = match &node.op {
                            curd_core::plan::ToolOperation::McpCall { tool, args } => format!("Tool: {} \nArgs: {}", tool, serde_json::to_string_pretty(args).unwrap_or_default()),
                            curd_core::plan::ToolOperation::Internal { command, params } => format!("Internal: {} \nParams: {}", command, serde_json::to_string_pretty(params).unwrap_or_default()),
                        };
                        println!("[Node ID: {}]", node.id);
                        if !node.dependencies.is_empty() {
                            println!("Depends On: {:?}", node.dependencies);
                        }
                        println!("{}", op_str);
                        println!("---");
                    }
                }
                PlanCommands::Impl { id, session, dry_run } => {
                    let plan_file = plans_dir.join(format!("{}.json", id));
                    if !plan_file.exists() {
                        anyhow::bail!("Plan not found: {}", id);
                    }
                    let content = fs::read_to_string(&plan_file)?;
                    let plan_json: serde_json::Value = serde_json::from_str(&content)?;
                    
                    if dry_run {
                        println!("Simulating Plan {}...", id);
                        let simulate_params = serde_json::json!({
                            "mode": "execute_plan",
                            "plan": plan_json
                        });
                        let result = curd_core::context::handle_simulate(&simulate_params).await;
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        // Real Execution requires full Context
                        println!("Executing Plan {} under Session {}...", id, session);
                        let plan: curd_core::Plan = serde_json::from_value(plan_json)?;
                        let session_uuid = uuid::Uuid::parse_str(&session).map_err(|_| anyhow::anyhow!("Invalid session UUID"))?;
                        
                        let (tx, _) = tokio::sync::broadcast::channel(100);
                        let ctx = curd_core::context::EngineContext {
                            workspace_root: resolved.clone(),
                            session_id: session_uuid,
                            read_only: false,
                            se: std::sync::Arc::new(curd_core::SearchEngine::new(&resolved)),
                            re: std::sync::Arc::new(curd_core::ReadEngine::new(&resolved)),
                            ee: std::sync::Arc::new(curd_core::EditEngine::new(&resolved)),
                            ge: std::sync::Arc::new(curd_core::GraphEngine::new(&resolved)),
                            we: std::sync::Arc::new(curd_core::WorkspaceEngine::new(&resolved)),
                            ple: std::sync::Arc::new(curd_core::PlanEngine::new(&resolved)),
                            mu: std::sync::Arc::new(curd_core::MutationEngine::new(&resolved)),
                            fe: std::sync::Arc::new(curd_core::FindEngine::new(&resolved)),
                            de: std::sync::Arc::new(curd_core::DiagramEngine::new(&resolved)),
                            fie: std::sync::Arc::new(curd_core::FileEngine::new(&resolved)),
                            le: std::sync::Arc::new(curd_core::LspEngine::new(&resolved)),
                            pe: std::sync::Arc::new(curd_core::ProfileEngine::new(&resolved)),
                            dbe: std::sync::Arc::new(curd_core::DebugEngine::new(&resolved)),
                            doce: std::sync::Arc::new(curd_core::DocEngine::new()),
                            doctore: std::sync::Arc::new(curd_core::doctor::DoctorEngine::new(&resolved)),
                            sre: std::sync::Arc::new(curd_core::SessionReviewEngine::new(&resolved)),
                            he: std::sync::Arc::new(curd_core::HistoryEngine::new(&resolved)),
                            tx_events: tx,
                            global_state: std::sync::Arc::new(tokio::sync::Mutex::new(curd_core::ReplState::new())),
                            sessions: std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
                            pending_challenges: std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
                            watchdog: std::sync::Arc::new(curd_core::Watchdog::new(resolved.clone())),
                            she: std::sync::Arc::new(curd_core::ShellEngine::new(&resolved)),
                        };
                        
                        let mut state = curd_core::ReplState::new();
                        state.active_plan = Some(plan);
                        let result = ctx.ple.execute_active_plan(&ctx, &mut state).await;
                        
                        match result {
                            Ok(res) => println!("Plan execution completed successfully:\n{}", serde_json::to_string_pretty(&res)?),
                            Err(e) => anyhow::bail!("Plan execution failed: {}", e),
                        }
                    }
                }
            }
            Ok(())
        }
        None => {
            // If legacy path provided, use MCP (only when mcp feature enabled)
            #[cfg(feature = "mcp")]
            if let Some(root) = cli.legacy_root {
                let resolved = resolve_workspace_root(root);
                enforce_workspace_config(&resolved)?;
                let server = McpServer::new(&resolved.to_string_lossy());
                return server.run().await;
            }
            // Otherwise, show help instead of silent hang
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
        && out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                let p = PathBuf::from(s);
                return std::fs::canonicalize(&p).unwrap_or(p);
            }
        }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn setup_tracing() {
    // Placeholder for telemetry/logging initialization.
}

fn enforce_workspace_config(root: &std::path::Path) -> Result<()> {
    curd_core::validate_workspace_config(root)
}

#[cfg(test)]
mod tests {
    use super::enforce_workspace_config;
    use tempfile::tempdir;

    #[test]
    fn enforce_workspace_config_accepts_default_workspace() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        enforce_workspace_config(&root).expect("default config should pass");
    }

    #[test]
    fn enforce_workspace_config_rejects_high_severity_findings() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        std::fs::write(
            root.join("settings.toml"),
            r#"
[storage]
sqlite_path = "../outside.sqlite3"
"#,
        )
        .expect("write settings");
        let err = enforce_workspace_config(&root).expect_err("expected config failure");
        assert!(
            err.to_string()
                .contains("config_storage_sqlite_path_invalid")
        );
    }
}
