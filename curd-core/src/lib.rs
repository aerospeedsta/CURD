pub mod auth;
pub mod build;
pub mod collab;
pub mod config;
pub mod connection;
pub mod context;
pub mod context_link;
pub mod debug;
pub mod deps;
pub mod diagram;
pub mod diff;
pub mod disclosure;
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
pub mod lang_plugin;
pub mod lsp;
pub mod lsp_client;
pub mod mutation;
pub mod ops;
pub mod parser;
pub mod plan;
pub mod plan_agent;
pub mod plan_parallel;
pub mod plan_review;
pub mod plan_runtime;
pub mod plugin_client;
pub mod plugin_packages;
pub mod policy;
pub mod profile;
pub mod read;
pub mod refactor;
pub mod registry;
pub mod review_cycle;
pub mod sandbox;
pub mod script;
pub mod search;
pub mod shell;
pub mod storage;
pub mod symbols;
pub mod tool_group;
pub mod tool_plugin;
pub mod trace;
pub mod transaction;
pub mod variants;
pub mod watchdog;
pub mod workspace;

// Re-export core items for easy access by downstream consumers
pub use auth::IdentityManager;
pub use build::{BuildRequest, BuildResponse, BuildStep, run_build};
pub use collab::{
    CollaborationCapability, CollaborationState, CollaborationStore, HumanOverrideLock,
    ParticipantBinding, ParticipantRole, role_allows,
};
pub use config::{
    AgentProfileConfig, ConfigFinding, CurdConfig, RuntimeConfig, check_workspace_config,
    validate_workspace_config,
};
pub use connection::{handle_connection_open, handle_connection_verify};
pub use context::{
    EngineContext, build_index_coverage, build_index_quality, dispatch_tool, handle_contract,
    handle_debug_dispatcher, handle_diagram, handle_doctor, handle_edit, handle_graph, handle_lsp,
    handle_manage_file, handle_profile, handle_read, handle_research, handle_search, handle_shell,
    handle_simulate, handle_workspace,
};
pub use context_link::{ContextLink, ContextMode, ContextRegistry};
pub use debug::DebugEngine;
pub use deps::{dependencies_to_json, detect_dependencies, get_excluded_dirs};
pub use diagram::DiagramEngine;
pub use diff::run_diff;
pub use disclosure::{
    DisclosureBundle, DisclosureLevel, DisclosureRequest, ExpansionRequest, ScopeSeed,
    build_disclosure_bundle,
};
pub use doc::DocEngine;
pub use doctor as diag;
pub use doctor::{
    DoctorEngine, DoctorFinding, DoctorIndexConfig, DoctorProfile, DoctorReport, DoctorThresholds,
};
pub use edit::EditEngine;
pub use fault::SemanticFault;
pub use file::FileEngine;
pub use find::FindEngine;
pub use graph::{DependencyGraph, GraphEngine};
pub use history::HistoryEngine;
pub use lang_plugin::LangPluginEngine;
pub use lsp::LspEngine;
pub use mutation::MutationEngine;
pub use ops::{
    CanonicalOperationKind, CapabilityAtom, OperationEnvelope, OperationScope,
    OperationValidationError, RuntimeCeiling, active_runtime_ceiling_from_config,
    canonical_op_for_tool, capability_for_tool, resolve_profile, tool_allowed_by_ceiling,
    validate_tool_call_core,
};
pub use parser::ParserManager;
pub use plan::{DslNode, Plan, PlanEngine, ReplState, SystemEvent, SystemEventEnvelope};
pub use plan_agent::PlanAgentOptions;
pub use plan_review::PlanReviewBundle;
pub use plan_runtime::PlanRuntimeTask;
pub use plugin_packages::{
    InstalledPluginRecord, LanguagePluginSpec, PluginArchive, PluginFileManifest, PluginKind,
    ToolDocExample, ToolDocParameter, ToolPluginSpec, TrustedPluginKey, TrustedPluginKeySet,
};
pub use policy::{PolicyConfig, PolicyEngine};
pub use profile::ProfileEngine;
pub use read::ReadEngine;
pub use refactor::{RefactorAction, run_refactor};
pub use registry::{GrammarRegistry, LanguageDef};
pub use review_cycle::ReviewCycleEngine;
pub use sandbox::Sandbox;
pub use script::{
    CompiledCurdPlanArtifact, CompiledCurdPlanNodeArtifact, CompiledCurdSafeguards,
    CompiledCurdScript, CurdScript, ScriptAnnotations, ScriptArgDecl, ScriptMetadata,
    ScriptStatement, collect_compiled_script_targets, compile_curd_script,
    compile_curd_script_to_plan, compiled_script_requires_shadow_session,
    parse_and_compile_curd_script, parse_and_compile_curd_script_to_plan, parse_curd_script,
    recommend_script_safeguards,
};
pub use search::{
    IndexBuildStats, IndexWorkerRequest, IndexWorkerResponse, SearchEngine, run_index_worker,
};
pub use shell::ShellEngine;
pub use storage::{IndexRunRecord, Storage, read_recent_index_runs, record_index_run};
pub use symbols::{Symbol, SymbolIndex, SymbolKind};
pub use tool_group::{AdoptedToolDescriptor, ToolGroupEngine, ToolGroupRecord, ToolGroupSource};
pub use tool_plugin::ToolPluginEngine;
pub use transaction::ShadowStore;
pub use variants::{
    PlanSet, PlanVariant, PlanVariantStatus, VariantStore, VariantWorkspaceBackend,
};
pub use watchdog::Watchdog;
pub use workspace::{WorkspaceEngine, list_workspace, scan_workspace};

/// Recursively redact sensitive fields from a JSON value.
pub fn redact_value(mut val: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match &mut val {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if matches!(
                    k.as_str(),
                    "code"
                        | "snippet"
                        | "signature_hex"
                        | "nonce"
                        | "session_token"
                        | "original_code"
                        | "mutated_code"
                ) {
                    *v = Value::String("[REDACTED]".to_string());
                } else {
                    *v = redact_value(v.clone());
                }
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                *v = redact_value(v.clone());
            }
        }
        _ => {}
    }
    val
}
