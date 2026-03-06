pub mod auth;
pub mod build;
pub mod config;
pub mod context;
pub mod context_link;
pub mod debug;
pub mod deps;
pub mod diagram;
pub mod diff;
pub mod doc;
pub mod doctor;
pub mod edit;
pub mod fault;
pub mod file;
pub mod find;
pub mod gpu;
pub mod graph;
pub mod graph_audit;
pub mod history;
pub mod lsp;
pub mod lsp_client;
pub mod mcp;
pub mod mutation;
pub mod parser;
pub mod plan;
pub mod profile;
pub mod read;
pub mod refactor;
pub mod registry;
pub mod sandbox;
pub mod search;
pub mod session_review;
pub mod shell;
pub mod storage;
pub mod symbols;
pub mod transaction;
pub mod watchdog;
pub mod workspace;

// Re-export core items for easy access by downstream consumers
pub use auth::IdentityManager;
pub use build::{BuildRequest, BuildResponse, BuildStep, run_build};
pub use config::{ConfigFinding, CurdConfig, check_workspace_config, validate_workspace_config};
pub use context_link::{ContextLink, ContextMode, ContextRegistry};
pub use diff::run_diff;
pub use doctor::{
    DoctorEngine, DoctorFinding, DoctorIndexConfig, DoctorProfile, DoctorReport, DoctorThresholds,
};
pub use doctor as diag;
pub use context::{
    EngineContext, build_index_coverage, build_index_quality, dispatch_tool, handle_contract,
    handle_debug_dispatcher, handle_diagram, handle_doctor, handle_edit, handle_graph, handle_lsp,
    handle_manage_file, handle_profile, handle_read, handle_research, handle_search, handle_shell,
    handle_simulate, handle_workspace,
};
pub use debug::DebugEngine;
pub use deps::{dependencies_to_json, detect_dependencies, get_excluded_dirs};
pub use diagram::DiagramEngine;
pub use doc::DocEngine;
pub use edit::EditEngine;
pub use fault::SemanticFault;
pub use file::FileEngine;
pub use find::FindEngine;
pub use graph::{DependencyGraph, GraphEngine};
pub use history::HistoryEngine;
pub use lsp::LspEngine;
pub use mutation::MutationEngine;
pub use mcp::{API_VERSION, McpServer, McpServerMode};
pub use parser::ParserManager;
pub use plan::{DslNode, Plan, PlanEngine, ReplState};
pub use profile::ProfileEngine;
pub use read::ReadEngine;
pub use refactor::{RefactorAction, run_refactor};
pub use registry::{GrammarRegistry, LanguageDef};
pub use sandbox::Sandbox;
pub use search::{
    IndexBuildStats, IndexWorkerRequest, IndexWorkerResponse, SearchEngine, run_index_worker,
};
pub use session_review::SessionReviewEngine;
pub use shell::ShellEngine;
pub use storage::{IndexRunRecord, read_recent_index_runs, record_index_run};
pub use symbols::{Symbol, SymbolIndex, SymbolKind};
pub use transaction::ShadowStore;
pub use watchdog::Watchdog;
pub use workspace::{WorkspaceEngine, list_workspace, scan_workspace};
