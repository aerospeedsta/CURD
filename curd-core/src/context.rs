use crate::{
    CollaborationStore, CurdConfig, DebugEngine, DiagramEngine, DocEngine, DslNode, EditEngine,
    FileEngine, FindEngine, GraphEngine, HistoryEngine, IndexBuildStats, LspEngine,
    LangPluginEngine, MutationEngine, ParticipantRole, Plan, PlanEngine, ProfileEngine,
    ReadEngine, ReplState, ReviewCycleEngine, SearchEngine, ShellEngine, SymbolKind,
    ToolGroupEngine, ToolPluginEngine, VariantStore,
    VariantWorkspaceBackend, Watchdog, WorkspaceEngine,
    doctor::{DoctorEngine, DoctorIndexConfig, DoctorProfile, DoctorThresholds},
    plan::{SystemEvent, SystemEventEnvelope},
    read_recent_index_runs,
};
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

use std::future::Future;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_risky(&self) -> bool { false }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value>;
}

/// A macro to wrap simple stateless handlers into the Tool trait
macro_rules! define_legacy_tool {
    ($struct_name:ident, $name_str:expr, $handler_fn:ident, $risky:expr) => {
        pub struct $struct_name;
        impl Tool for $struct_name {
            fn name(&self) -> &'static str { $name_str }
            fn is_risky(&self) -> bool { $risky }
            fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
                Box::pin(async move {
                    $handler_fn(params, ctx).await
                })
            }
        }
    };
}

macro_rules! define_stateless_tool {
    ($struct_name:ident, $name_str:expr, $handler_fn:ident, $risky:expr) => {
        pub struct $struct_name;
        impl Tool for $struct_name {
            fn name(&self) -> &'static str { $name_str }
            fn is_risky(&self) -> bool { $risky }
            fn call<'a>(&'a self, params: &'a Value, _ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
                Box::pin(async move {
                    $handler_fn(params).await
                })
            }
        }
    };
}

// Wrap some of our basic stateless tools
define_legacy_tool!(SearchTool, "search", handle_search, false);
define_legacy_tool!(ContractTool, "contract", handle_contract, false);
define_legacy_tool!(WorkspaceTool, "workspace", handle_workspace, false);
define_legacy_tool!(StaminaTool, "stamina", handle_stamina, false);
define_legacy_tool!(CheckpointTool, "checkpoint", handle_checkpoint, false);
define_legacy_tool!(DelegateTool, "delegate", handle_delegate, false);
define_legacy_tool!(FrontierTool, "frontier", handle_frontier, false);
define_legacy_tool!(CrawlTool, "crawl", handle_crawl, false);
define_legacy_tool!(RegisterPlanTool, "register_plan", handle_register_plan, false);
define_legacy_tool!(BindParticipantRoleTool, "bind_participant_role", handle_bind_participant_role, false);
define_legacy_tool!(ListSessionParticipantsTool, "list_session_participants", handle_list_session_participants, false);
define_legacy_tool!(ListCollaborationParticipantsTool, "list_collaboration_participants", handle_list_session_participants, false);
define_legacy_tool!(ClaimHumanOverrideTool, "claim_human_override", handle_claim_human_override, false);
define_legacy_tool!(ReleaseHumanOverrideTool, "release_human_override", handle_release_human_override, false);
define_legacy_tool!(CreatePlanSetTool, "create_plan_set", handle_create_plan_set, false);
define_legacy_tool!(ListPlanSetsTool, "list_plan_sets", handle_list_plan_sets, false);
define_legacy_tool!(CreatePlanVariantTool, "create_plan_variant", handle_create_plan_variant, false);
define_legacy_tool!(ListPlanVariantsTool, "list_plan_variants", handle_list_plan_variants, false);
define_legacy_tool!(SimulatePlanVariantTool, "simulate_plan_variant", handle_simulate_plan_variant, false);
define_legacy_tool!(ComparePlanVariantsTool, "compare_plan_variants", handle_compare_plan_variants, false);
define_legacy_tool!(ReviewPlanVariantTool, "review_plan_variant", handle_review_plan_variant, false);
define_legacy_tool!(PromotePlanVariantTool, "promote_plan_variant", handle_promote_plan_variant, true);
define_legacy_tool!(ToolPluginManageTool, "plugin_tool", handle_tool_plugin, false);
define_legacy_tool!(LanguagePluginManageTool, "plugin_language", handle_language_plugin, false);
define_legacy_tool!(ToolGroupManageTool, "tool_group", handle_tool_group, false);
define_legacy_tool!(PluginTrustManageTool, "plugin_trust", handle_plugin_trust, false);

use crate::mcp::{handle_connection_open, handle_connection_verify};

pub struct SessionOpenTool;
impl Tool for SessionOpenTool {
    fn name(&self) -> &'static str { "session_open" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_connection_open(params, ctx).await
        })
    }
}

pub struct SessionVerifyTool;
impl Tool for SessionVerifyTool {
    fn name(&self) -> &'static str { "session_verify" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_connection_verify(params, ctx).await
        })
    }
}

pub struct ConnectionOpenTool;
impl Tool for ConnectionOpenTool {
    fn name(&self) -> &'static str { "connection_open" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_connection_open(params, ctx).await
        })
    }
}

pub struct ConnectionVerifyTool;
impl Tool for ConnectionVerifyTool {
    fn name(&self) -> &'static str { "connection_verify" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_connection_verify(params, ctx).await
        })
    }
}

pub struct ReadTool;
impl Tool for ReadTool {
    fn name(&self) -> &'static str { "read" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_read(params, Arc::clone(&ctx.re), shadow_root).await
        })
    }
}

pub struct EditTool;
impl Tool for EditTool {
    fn name(&self) -> &'static str { "edit" }
    fn is_risky(&self) -> bool { true }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_edit(params, Arc::clone(&ctx.ee), shadow_root).await
        })
    }
}

pub struct ManageFileTool;
impl Tool for ManageFileTool {
    fn name(&self) -> &'static str { "manage_file" }
    fn is_risky(&self) -> bool { true }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_manage_file(params, Arc::clone(&ctx.fie), shadow_root).await
        })
    }
}

pub struct MutateTool;
impl Tool for MutateTool {
    fn name(&self) -> &'static str { "mutate" }
    fn is_risky(&self) -> bool { true }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_mutate(params, Arc::clone(&ctx.mu), shadow_root).await
        })
    }
}

pub struct ShellTool;
impl Tool for ShellTool {
    fn name(&self) -> &'static str { "shell" }
    fn is_risky(&self) -> bool { true }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_shell(params, &ctx.she, shadow_root.as_deref()).await
        })
    }
}

pub struct ShellStatusTool;
impl Tool for ShellStatusTool {
    fn name(&self) -> &'static str { "shell_status" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_shell(params, &ctx.she, shadow_root.as_deref()).await
        })
    }
}

pub struct TerminateTool;
impl Tool for TerminateTool {
    fn name(&self) -> &'static str { "terminate" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_shell(params, &ctx.she, shadow_root.as_deref()).await
        })
    }
}

pub struct GraphTool;
impl Tool for GraphTool {
    fn name(&self) -> &'static str { "graph" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_graph(params, Arc::clone(&ctx.ge)).await
        })
    }
}

pub struct DiagramTool;
impl Tool for DiagramTool {
    fn name(&self) -> &'static str { "diagram" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_diagram(params, Arc::clone(&ctx.de)).await
        })
    }
}

pub struct LspTool;
impl Tool for LspTool {
    fn name(&self) -> &'static str { "lsp" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_lsp(params, &ctx.le).await
        })
    }
}

pub struct ProfileTool;
impl Tool for ProfileTool {
    fn name(&self) -> &'static str { "profile" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_profile(params, &ctx.pe).await
        })
    }
}

pub struct DebugTool;
impl Tool for DebugTool {
    fn name(&self) -> &'static str { "debug" }
    fn is_risky(&self) -> bool { true }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_debug_dispatcher(params, &ctx.dbe).await
        })
    }
}

pub struct SessionTool;
impl Tool for SessionTool {
    fn name(&self) -> &'static str { "session" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_review_cycle_dispatcher(params, &ctx.rce).await
        })
    }
}

pub struct ReviewCycleTool;
impl Tool for ReviewCycleTool {
    fn name(&self) -> &'static str { "review_cycle" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_review_cycle_dispatcher(params, &ctx.rce).await
        })
    }
}

pub struct DoctorTool;
impl Tool for DoctorTool {
    fn name(&self) -> &'static str { "doctor" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_doctor(params, &ctx.doctore).await
        })
    }
}

pub struct RefactorTool;
impl Tool for RefactorTool {
    fn name(&self) -> &'static str { "refactor" }
    fn is_risky(&self) -> bool { true }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let _ = params;
            let _ = ctx;
            json!({"error": "Refactor tool not implemented."})
        })
    }
}

pub struct TemplateTool;
impl Tool for TemplateTool {
    fn name(&self) -> &'static str { "template" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_template(params, ctx).await
        })
    }
}

pub struct ProposalTool;
impl Tool for ProposalTool {
    fn name(&self) -> &'static str { "proposal" }
    fn is_risky(&self) -> bool { true }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_proposal(params, ctx).await
        })
    }
}

define_stateless_tool!(ResearchTool, "research", handle_research, false);
define_stateless_tool!(SimulateTool, "simulate", handle_simulate, false);

pub struct ProposePlanTool;
impl Tool for ProposePlanTool {
    fn name(&self) -> &'static str { "propose_plan" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_propose_plan(params, ctx).await
        })
    }
}

pub struct ExecuteActivePlanTool;
impl Tool for ExecuteActivePlanTool {
    fn name(&self) -> &'static str { "execute_active_plan" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_execute_active_plan(params, ctx).await
        })
    }
}

pub struct BenchmarkTool;
impl Tool for BenchmarkTool {
    fn name(&self) -> &'static str { "benchmark" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            let allow_bench = cfg!(debug_assertions) || std::env::var("CURD_ALLOW_BENCHMARK").is_ok();
            if allow_bench {
                handle_benchmark(params, ctx).await
            } else {
                json!({"error": "The benchmark tool is disabled in release builds for security and stability. Set CURD_ALLOW_BENCHMARK=1 to override."})
            }
        })
    }
}

pub struct DocTool;
impl Tool for DocTool {
    fn name(&self) -> &'static str { "doc" }
    fn call<'a>(&'a self, params: &'a Value, ctx: &'a EngineContext) -> BoxFuture<'a, Value> {
        Box::pin(async move {
            handle_doc(params, ctx).await
        })
    }
}

pub struct EngineContext {
    pub workspace_root: PathBuf,
    pub collaboration_id: Uuid,
    pub read_only: bool,
    pub config: CurdConfig,
    pub registry: Arc<HashMap<String, Arc<dyn Tool>>>,
    pub se: Arc<SearchEngine>,
    pub re: Arc<ReadEngine>,
    pub ee: Arc<EditEngine>,
    pub ge: Arc<GraphEngine>,
    pub we: Arc<WorkspaceEngine>,
    pub she: Arc<ShellEngine>,
    pub fe: Arc<FindEngine>,
    pub de: Arc<DiagramEngine>,
    pub fie: Arc<FileEngine>,
    pub le: Arc<LspEngine>,
    pub pe: Arc<ProfileEngine>,
    pub dbe: Arc<DebugEngine>,
    pub rce: Arc<ReviewCycleEngine>,
    pub doce: Arc<DocEngine>,
    pub doctore: Arc<DoctorEngine>,
    pub ple: Arc<PlanEngine>,
    pub lpe: Arc<LangPluginEngine>,
    pub tpe: Arc<ToolPluginEngine>,
    pub tge: Arc<ToolGroupEngine>,
    pub he: Arc<HistoryEngine>,
    pub ce: Arc<crate::history::ContributionLedger>,
    pub mu: Arc<MutationEngine>,
    pub tx_events: broadcast::Sender<SystemEvent>,
    pub global_state: Arc<Mutex<ReplState>>,
    pub connections: Arc<Mutex<HashMap<String, ConnectionEntry>>>,
    pub pending_challenges: Arc<Mutex<HashMap<String, PendingChallenge>>>,
    pub watchdog: Arc<Watchdog>,
    pub locks: Arc<Mutex<HashMap<String, LockInfo>>>,
    pub event_seq: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    pub owner_id: String, // "human" or agent_id
    pub expires_at_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionBudget {
    pub tokens_consumed: u64,
    pub hazardous_calls_made: u64,
    pub started_at_secs: u64,
}

impl Default for ConnectionBudget {
    fn default() -> Self {
        Self {
            tokens_consumed: 0,
            hazardous_calls_made: 0,
            started_at_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

pub struct ConnectionEntry {
    pub agent_id: String,
    pub pubkey_hex: String,
    pub state: ReplState,
    pub budget: ConnectionBudget,
    pub last_touched_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingChallenge {
    pub nonce_hex: String,
    pub created_at_secs: u64,
}

impl EngineContext {
    pub fn clone_for_repl(&self) -> Self {
        Self {
            workspace_root: self.workspace_root.clone(),
            collaboration_id: self.collaboration_id,
            read_only: self.read_only,
            config: self.config.clone(),
            registry: Arc::clone(&self.registry),
            se: Arc::clone(&self.se),
            re: Arc::clone(&self.re),
            ee: Arc::clone(&self.ee),
            ge: Arc::clone(&self.ge),
            we: Arc::clone(&self.we),
            she: Arc::clone(&self.she),
            fe: Arc::clone(&self.fe),
            de: Arc::clone(&self.de),
            fie: Arc::clone(&self.fie),
            le: Arc::clone(&self.le),
            pe: Arc::clone(&self.pe),
            dbe: Arc::clone(&self.dbe),
            rce: Arc::clone(&self.rce),
            doce: Arc::clone(&self.doce),
            doctore: Arc::clone(&self.doctore),
            ple: Arc::clone(&self.ple),
            lpe: Arc::clone(&self.lpe),
            tpe: Arc::clone(&self.tpe),
            tge: Arc::clone(&self.tge),
            he: Arc::clone(&self.he),
            ce: Arc::clone(&self.ce),
            mu: Arc::clone(&self.mu),
            tx_events: self.tx_events.clone(),
            global_state: Arc::clone(&self.global_state),
            connections: Arc::clone(&self.connections),
            pending_challenges: Arc::clone(&self.pending_challenges),
            watchdog: Arc::clone(&self.watchdog),
            locks: Arc::clone(&self.locks),
            event_seq: Arc::clone(&self.event_seq),
        }
    }

    pub fn new(root: &str) -> Arc<Self> {
        let root_path = PathBuf::from(root);
        let watchdog = Arc::new(Watchdog::new(root_path.clone()));
        watchdog.start();

        let collaboration_id = Uuid::new_v4();
        let (tx_events, _) = tokio::sync::broadcast::channel(1024);

        // Session Locking
        let curd_dir = crate::workspace::get_curd_dir(&root_path);
        let lock_path = curd_dir.join("SESSION_LOCK");
        let mut read_only = false;
        if let Some(parent) = lock_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if crate::workspace::is_workspace_locked(&root_path) {
            log::warn!("Workspace is locked by another active transaction. Starting in READ-ONLY mode.");
            read_only = true;
        } else {
            let _ = fs::write(&lock_path, std::process::id().to_string());
        }

        let config = CurdConfig::load_from_workspace(&root_path);
        
        let mut registry_map: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(SearchTool),
            Arc::new(ContractTool),
            Arc::new(WorkspaceTool),
            Arc::new(StaminaTool),
            Arc::new(CheckpointTool),
            Arc::new(DelegateTool),
            Arc::new(FrontierTool),
            Arc::new(CrawlTool),
            Arc::new(RegisterPlanTool),
            Arc::new(BindParticipantRoleTool),
            Arc::new(ListSessionParticipantsTool),
            Arc::new(ListCollaborationParticipantsTool),
            Arc::new(ClaimHumanOverrideTool),
            Arc::new(ReleaseHumanOverrideTool),
            Arc::new(CreatePlanSetTool),
            Arc::new(ListPlanSetsTool),
            Arc::new(CreatePlanVariantTool),
            Arc::new(ListPlanVariantsTool),
            Arc::new(SimulatePlanVariantTool),
            Arc::new(ComparePlanVariantsTool),
            Arc::new(ReviewPlanVariantTool),
            Arc::new(PromotePlanVariantTool),
            Arc::new(ToolPluginManageTool),
            Arc::new(LanguagePluginManageTool),
            Arc::new(ToolGroupManageTool),
            Arc::new(PluginTrustManageTool),
            Arc::new(SessionOpenTool),
            Arc::new(SessionVerifyTool),
            Arc::new(ConnectionOpenTool),
            Arc::new(ConnectionVerifyTool),
            Arc::new(ReadTool),
            Arc::new(EditTool),
            Arc::new(ManageFileTool),
            Arc::new(MutateTool),
            Arc::new(ShellTool),
            Arc::new(ShellStatusTool),
            Arc::new(TerminateTool),
            Arc::new(GraphTool),
            Arc::new(DiagramTool),
            Arc::new(LspTool),
            Arc::new(ProfileTool),
            Arc::new(DebugTool),
            Arc::new(SessionTool),
            Arc::new(ReviewCycleTool),
            Arc::new(DoctorTool),
            Arc::new(RefactorTool),
            Arc::new(TemplateTool),
            Arc::new(ProposalTool),
            Arc::new(ResearchTool),
            Arc::new(SimulateTool),
            Arc::new(ProposePlanTool),
            Arc::new(ExecuteActivePlanTool),
            Arc::new(BenchmarkTool),
            Arc::new(DocTool),
        ];

        for tool in tools {
            registry_map.insert(tool.name().to_string(), tool);
        }

        Arc::new(EngineContext {
            workspace_root: root_path.clone(),
            collaboration_id,
            read_only,
            config: config.clone(),
            registry: Arc::new(registry_map),
            se: Arc::new(SearchEngine::new(root).with_events(tx_events.clone())),
            re: Arc::new(ReadEngine::new(root)),
            ee: Arc::new(EditEngine::new(root).with_watchdog(watchdog.clone())),
            ge: Arc::new(GraphEngine::new(root)),
            we: Arc::new(WorkspaceEngine::new(root)),
            she: Arc::new(ShellEngine::new(root)),
            fe: Arc::new(FindEngine::new(root)),
            de: Arc::new(DiagramEngine::new(root)),
            fie: Arc::new(FileEngine::new(root)),
            le: Arc::new(LspEngine::new(root)),
            pe: Arc::new(ProfileEngine::new(root)),
            dbe: Arc::new(DebugEngine::new(root)),
            rce: Arc::new(ReviewCycleEngine::new(root)),
            doce: Arc::new(DocEngine::new()),
            doctore: Arc::new(DoctorEngine::new(root)),
            ple: Arc::new(PlanEngine::new(root)),
            lpe: Arc::new(LangPluginEngine::new(&root_path, config.plugins.clone())),
            tpe: Arc::new(ToolPluginEngine::new(&root_path, config.plugins.clone())),
            tge: Arc::new(ToolGroupEngine::new(&root_path, config.plugins.clone())),
            he: Arc::new(HistoryEngine::new(&root_path)),
            ce: Arc::new(crate::history::ContributionLedger::new(
                &root_path,
                config.provenance.clone(),
            )),
            mu: Arc::new(MutationEngine::new(root)),
            tx_events,
            global_state: Arc::new(Mutex::new(ReplState::new())),
            connections: Arc::new(Mutex::new(HashMap::new())),
            pending_challenges: Arc::new(Mutex::new(HashMap::new())),
            watchdog,
            locks: Arc::new(Mutex::new(HashMap::new())),
            event_seq: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn next_event_envelope(&self, event: SystemEvent) -> SystemEventEnvelope {
        SystemEventEnvelope {
            event_id: self.event_seq.fetch_add(1, Ordering::SeqCst) + 1,
            collaboration_id: self.collaboration_id,
            ts_secs: crate::plan::now_secs(),
            event,
        }
    }
}

impl Drop for EngineContext {
    fn drop(&mut self) {
        if !self.read_only {
            let lock_path = self.workspace_root.join(".curd").join("SESSION_LOCK");
            if lock_path.exists() {
                // Verify it's OUR lock (containing our PID)
                if let Ok(pid_str) = fs::read_to_string(&lock_path)
                    && pid_str.trim() == std::process::id().to_string()
                {
                    let _ = fs::remove_file(lock_path);
                }
            }
        }
    }
}

pub async fn dispatch_tool(name: &str, params: &Value, ctx: &EngineContext) -> Value {
    let workspace_active = ctx.we.shadow.lock().map(|s| s.is_active()).unwrap_or(false);

    // Enforcement of Read-Only mode for hazardous tools
    if ctx.read_only && risky_tool(name) {
        return json!({
            "error": format!("Cannot execute tool '{}': Workspace is locked in READ-ONLY mode by another active transaction.", name)
        });
    }

    if ctx.config.workspace.require_open_for_all_tools && !workspace_optional_tool(name) && !workspace_active {
        return json!({
            "error": format!(
                "WORKSPACE_BARRIER: Cannot execute tool '{}': no active workspace transaction. Run workspace(action: 'begin') first or disable workspace.require_open_for_all_tools in settings.toml.",
                name
            )
        });
    }

    // Enforcement Barrier for hazardous tools
    if ctx.config.edit.enforce_transactional && risky_tool(name) {
        if !workspace_active {
            return json!({
                "error": format!("TRANS_BARRIER: Cannot execute hazardous tool '{}': No active transaction. All writes must be performed within a CURD workspace transaction per AGENTS.md safety rules. Run workspace(action: 'begin') first.", name)
            });
        }
    }

    // 4. Human Override Logic (Lock Table)
    if risky_tool(name) {
        let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
        if !uri.is_empty() {
             let mut locks = ctx.locks.lock().await;
             let now = now_secs();
             // Clean up expired locks
             locks.retain(|_, v| v.expires_at_secs > now);
             
             let agent_id = params.get("agent_id").and_then(|v| v.as_str()).unwrap_or("agent");
             if let Some(lock) = locks.get(uri) {
                 if lock.owner_id == "human" && agent_id != "human" {
                      return json!({
                          "error": "HUMAN_OVERRIDE_CONFLICT: This symbol is currently being edited by a human in the REPL. Agent edits are rejected to prevent state corruption."
                      });
                 }
             }
             
             // Auto-lock for current caller
             locks.insert(uri.to_string(), LockInfo {
                 owner_id: agent_id.to_string(),
                 expires_at_secs: now + 300, // 5 minute lock
             });
        }
    }

    // 3. Resource Budget Enforcement
    let connection_token = params
        .get("connection_token")
        .or_else(|| params.get("session_token"))
        .and_then(|v| v.as_str());
    if let Some(token) = connection_token {
        let mut connections = ctx.connections.lock().await;
        if let Some(entry) = connections.get_mut(token) {
            let now = now_secs();
            let elapsed = now.saturating_sub(entry.budget.started_at_secs);
            
            // Check time budget
            if let Some(max_secs) = ctx.config.budget.max_session_secs {
                if elapsed > max_secs {
                    return json!({"error": format!("BUDGET_EXHAUSTED: Session has exceeded the maximum allowed duration of {}s.", max_secs)});
                }
            }

            // Check hazardous calls budget
            if risky_tool(name) {
                entry.budget.hazardous_calls_made += 1;
                if let Some(max_calls) = ctx.config.budget.max_hazardous_calls {
                    if entry.budget.hazardous_calls_made > max_calls {
                        return json!({"error": format!("BUDGET_EXHAUSTED: Session has exceeded the maximum allowed number of hazardous tool calls ({}).", max_calls)});
                    }
                }
            }

            entry.last_touched_secs = now;
        }
    }

    // Capture response for budget tracking (tokens)
    let res = if let Some(tool) = ctx.registry.get(name) {
        // Apply global 60s timeout to prevent hanging the server
        match tokio::time::timeout(std::time::Duration::from_secs(60), tool.call(params, ctx)).await {
            Ok(r) => r,
            Err(_) => json!({"error": format!("TOOL_TIMEOUT: Execution of '{}' exceeded the maximum allowed duration of 60s.", name)}),
        }
    } else {
        match ctx.tpe.invoke(name, params).await {
            Ok(Some(value)) => value,
            Ok(None) => match ctx.tge.invoke(name, params).await {
                Ok(Some(value)) => value,
                Ok(None) => match name {
                    "find" => json!({"error": "The 'find' tool has been merged into 'search'. Use 'search' with mode='text'."}),
                    _ => json!({"error": format!("Tool not found: {}", name)}),
                },
                Err(err) => json!({"error": format!("Tool group '{}' failed: {}", name, err)}),
            },
            Err(err) => json!({"error": format!("Tool plugin '{}' failed: {}", name, err)}),
        }
    };

    // Update token consumption (heuristic: 1 token per 4 chars of result)
    if let Some(token) = connection_token {
        let mut connections = ctx.connections.lock().await;
        if let Some(entry) = connections.get_mut(token) {
            let res_str = res.to_string();
            let estimated_tokens = (res_str.len() / 4) as u64;
            entry.budget.tokens_consumed += estimated_tokens;

            if let Some(max_tokens) = ctx.config.budget.max_tokens {
                if entry.budget.tokens_consumed > max_tokens {
                    // Note: We return the result but mark subsequent calls as blocked
                    log::warn!("Session {} has exhausted token budget ({} consumed)", token, entry.budget.tokens_consumed);
                }
            }
        }
    }

    let error = res.get("error").and_then(|e| e.as_str()).map(|s| s.to_string());
    let success = error.is_none();

    let transaction_id = ctx.we.shadow.lock().ok().and_then(|s| s.get_transaction_id());

    let base_hash = params.get("base_state_hash").and_then(|v| v.as_str()).map(|s| s.to_string());
    
    // 5. Compute Post-Hash for Traceability
    let post_hash = if success && risky_tool(name) {
        ctx.ge.storage().ok().and_then(|s: crate::storage::Storage| s.compute_state_hash().ok())
    } else {
        None
    };

    let agent_id = if let Some(token) = connection_token {
        ctx.connections.lock().await.get(token).map(|e| e.pubkey_hex.clone())
    } else {
        params.get("agent_id").and_then(|v| v.as_str()).map(|s| s.to_string())
    };

    ctx.he.log(
        Some(ctx.event_seq.load(Ordering::SeqCst)),
        ctx.collaboration_id,
        agent_id,
        transaction_id,
        name,
        params.clone(),
        res.clone(),
        base_hash,
        post_hash,
        success,
        error,
        None, // verification_result
    );

    res
}

fn workspace_optional_tool(name: &str) -> bool {
    matches!(
        name,
        "workspace"
            | "connection_open"
            | "connection_verify"
            | "session_open"
            | "session_verify"
            | "stamina"
            | "plugin_trust"
    )
}


async fn execute_benchmark_target(operation: &str, params: &Value, ctx: &EngineContext) -> Value {
    match operation {
        "search" => handle_search(params, ctx).await,
        "contract" => handle_contract(params, ctx).await,
        "read" => {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_read(params, Arc::clone(&ctx.re), shadow_root).await
        }
        "edit" => {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_edit(params, Arc::clone(&ctx.ee), shadow_root).await
        }
        "graph" => handle_graph(params, Arc::clone(&ctx.ge)).await,
        "workspace" => handle_workspace(params, ctx).await,
        "find" => {
            json!({"error": "The 'find' tool has been merged into 'search'. Use 'search' with mode='text'."})
        }
        "shell" => {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_shell(params, &ctx.she, shadow_root.as_deref()).await
        }
        "shell_status" => {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_shell(params, &ctx.she, shadow_root.as_deref()).await
        }
        "terminate" => {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_shell(params, &ctx.she, shadow_root.as_deref()).await
        }
        "diagram" => handle_diagram(params, Arc::clone(&ctx.de)).await,
        "manage_file" => {
            let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
            handle_manage_file(params, Arc::clone(&ctx.fie), shadow_root).await
        }
        "lsp" => handle_lsp(params, &ctx.le).await,
        "profile" => handle_profile(params, &ctx.pe).await,
        "debug" => handle_debug_dispatcher(params, &ctx.dbe).await,
        "session" => handle_review_cycle_dispatcher(params, &ctx.rce).await,
        "review_cycle" => handle_review_cycle_dispatcher(params, &ctx.rce).await,
        "doc" => handle_doc(params, ctx).await,
        "doctor" => handle_doctor(params, &ctx.doctore).await,
        "simulate" => handle_simulate(params).await,
        "template" => handle_template(params, ctx).await,
        "proposal" => handle_proposal(params, ctx).await,
        "checkpoint" => handle_checkpoint(params, ctx).await,
        "delegate" => handle_delegate(params, ctx).await,
        "frontier" => handle_frontier(params, ctx).await,
        "crawl" => handle_crawl(params, ctx).await,
        "register_plan" => handle_register_plan(params, ctx).await,
        "bind_participant_role" => handle_bind_participant_role(params, ctx).await,
        "list_session_participants" => handle_list_session_participants(params, ctx).await,
        "list_collaboration_participants" => handle_list_session_participants(params, ctx).await,
        "claim_human_override" => handle_claim_human_override(params, ctx).await,
        "release_human_override" => handle_release_human_override(params, ctx).await,
        "create_plan_set" => handle_create_plan_set(params, ctx).await,
        "list_plan_sets" => handle_list_plan_sets(params, ctx).await,
        "create_plan_variant" => handle_create_plan_variant(params, ctx).await,
        "list_plan_variants" => handle_list_plan_variants(params, ctx).await,
        "simulate_plan_variant" => handle_simulate_plan_variant(params, ctx).await,
        "compare_plan_variants" => handle_compare_plan_variants(params, ctx).await,
        "review_plan_variant" => handle_review_plan_variant(params, ctx).await,
        "promote_plan_variant" => handle_promote_plan_variant(params, ctx).await,
        "plugin_tool" => handle_tool_plugin(params, ctx).await,
        "plugin_language" => handle_language_plugin(params, ctx).await,
        "tool_group" => handle_tool_group(params, ctx).await,
        "plugin_trust" => handle_plugin_trust(params, ctx).await,
        "execute_active_plan" => handle_execute_active_plan(params, ctx).await,
        "batch" => json!({"error": "Benchmark does not support operation: batch"}),
        "benchmark" => json!({"error": "Recursive benchmark operation is not allowed"}),
        _ => json!({"error": format!("Tool not found: {}", operation)}),
    }
}

fn is_known_tool_name(name: &str) -> bool {
    matches!(
        name,
        "search"
            | "contract"
            | "read"
            | "edit"
            | "graph"
            | "workspace"
            | "shell"
            | "diagram"
            | "manage_file"
            | "lsp"
            | "profile"
            | "debug"
            | "session"
            | "doc"
            | "doctor"
            | "batch"
            | "benchmark"
            | "simulate"
            | "template"
            | "proposal"
            | "checkpoint"
            | "delegate"
            | "frontier"
            | "crawl"
            | "register_plan"
            | "bind_participant_role"
            | "list_session_participants"
            | "list_collaboration_participants"
            | "claim_human_override"
            | "release_human_override"
            | "create_plan_set"
            | "list_plan_sets"
            | "create_plan_variant"
            | "list_plan_variants"
            | "simulate_plan_variant"
            | "compare_plan_variants"
            | "review_plan_variant"
            | "promote_plan_variant"
            | "plugin_tool"
            | "plugin_language"
            | "tool_group"
            | "plugin_trust"
            | "propose_plan"
            | "execute_active_plan"
            | "execute_dsl"
            | "execute_plan"
            | "history"
            | "session_open"
            | "session_verify"
            | "connection_open"
            | "connection_verify"
            | "research"
    )
}

fn risky_tool(name: &str) -> bool {
    matches!(
        name,
        "edit"
            | "manage_file"
            | "shell"
            | "mutate"
            | "promote_plan_variant"
            | "plugin_tool"
            | "plugin_language"
            | "tool_group"
            | "plugin_trust"
    )
}

fn extract_uri_path(uri: &str) -> &str {
    uri.split("::").next().unwrap_or(uri)
}

fn validate_relative_path(path: &str, root: &std::path::Path) -> bool {
    crate::workspace::validate_sandboxed_path(root, path).is_ok()
}

fn validate_tool_args_for_simulate(
    tool: &str,
    args: &Value,
    root: &std::path::Path,
    findings: &mut Vec<Value>,
    warnings: &mut Vec<Value>,
    scope: &str,
) {
    match tool {
        "read" | "graph" => {
            if let Some(uris) = args.get("uris").and_then(|v| v.as_array()) {
                for uri in uris.iter().filter_map(|v| v.as_str()) {
                    let path = extract_uri_path(uri);
                    if !path.is_empty() && !validate_relative_path(path, root) {
                        findings.push(json!({
                            "severity": "error",
                            "code": "invalid_uri_path",
                            "message": format!("{} has URI outside sandbox: {}", scope, uri)
                        }));
                    }
                }
            } else {
                findings.push(json!({
                    "severity": "error",
                    "code": "invalid_args",
                    "message": format!("{} missing required array: uris", scope)
                }));
            }
        }
        "lsp" => {
            let uri = args.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            if uri.is_empty() {
                findings.push(json!({
                    "severity": "error",
                    "code": "invalid_args",
                    "message": format!("{} missing required field: uri", scope)
                }));
            } else if !validate_relative_path(extract_uri_path(uri), root) {
                findings.push(json!({
                    "severity": "error",
                    "code": "invalid_uri_path",
                    "message": format!("{} has URI outside sandbox: {}", scope, uri)
                }));
            }
        }
        "edit" => {
            let uri = args.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            if uri.is_empty() {
                findings.push(json!({
                    "severity": "error",
                    "code": "invalid_args",
                    "message": format!("{} missing required field: uri", scope)
                }));
            } else if !validate_relative_path(extract_uri_path(uri), root) {
                findings.push(json!({
                    "severity": "error",
                    "code": "invalid_uri_path",
                    "message": format!("{} has URI outside sandbox: {}", scope, uri)
                }));
            }
        }
        "manage_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                findings.push(json!({
                    "severity": "error",
                    "code": "invalid_args",
                    "message": format!("{} missing required field: path", scope)
                }));
            } else if !validate_relative_path(path, root) {
                findings.push(json!({
                    "severity": "error",
                    "code": "invalid_path",
                    "message": format!("{} path outside sandbox: {}", scope, path)
                }));
            }
            if let Some(dest) = args.get("destination").and_then(|v| v.as_str())
                && !validate_relative_path(dest, root)
            {
                findings.push(json!({
                    "severity": "error",
                    "code": "invalid_destination",
                    "message": format!("{} destination outside sandbox: {}", scope, dest)
                }));
            }
        }
        "workspace" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("status");
            if matches!(action, "begin" | "diff" | "commit" | "rollback" | "alerts") {
                warnings.push(json!({
                    "severity": "warning",
                    "code": "stateful_workspace_action",
                    "message": format!("{} includes workspace action with state impact: {}", scope, action)
                }));
            }
        }
        _ => {}
    }
}

pub async fn handle_propose_plan(params: &Value, ctx: &EngineContext) -> Value {
    let idea = params.get("idea").and_then(|v| v.as_str()).unwrap_or("");
    if idea.is_empty() {
        return json!({"error": "idea is required"});
    }

    // In a real scenario, this would involve LLM call or complex heuristics.
    // For now, we generate a draft plan with a single 'research' node as a placeholder.
    let plan_id = Uuid::new_v4();
    let plan = crate::plan::Plan {
        id: plan_id,
        nodes: vec![crate::plan::PlanNode {
            id: Uuid::new_v4(),
            op: crate::plan::ToolOperation::McpCall {
                tool: "research".to_string(),
                args: json!({"query": format!("Investigate how to implement: {}", idea)}),
            },
            dependencies: vec![],
            output_limit: 64 * 1024,
            retry_limit: 1,
        }],
    };

    let mut state = ctx.global_state.lock().await;
    match ctx.ple.register_plan(plan, ctx, &mut state) {
        Ok(res) => res,
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_register_plan(params: &Value, ctx: &EngineContext) -> Value {
    match serde_json::from_value::<crate::Plan>(params.clone()) {
        Ok(plan) => {
            let mut state = ctx.global_state.lock().await;
            match ctx.ple.register_plan(plan, ctx, &mut state) {
                Ok(res) => res,
                Err(e) => json!({"error": e.to_string()}),
            }
        }
        Err(e) => json!({"error": format!("Invalid plan format: {}", e)}),
    }
}

fn split_signature_params(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn derive_contract_from_source(source: &str) -> (Vec<String>, String, Vec<String>) {
    let first = source.lines().next().unwrap_or("").trim();
    let mut inputs = Vec::new();
    let mut output = "unknown".to_string();
    let mut side_effects = Vec::new();

    if let (Some(lp), Some(rp)) = (first.find('('), first.rfind(')'))
        && rp > lp
    {
        let params = &first[(lp + 1)..rp];
        inputs = split_signature_params(params);
        let tail = first[(rp + 1)..].trim();
        if let Some(idx) = tail.find("->") {
            let ret = tail[(idx + 2)..]
                .trim()
                .trim_end_matches('{')
                .trim()
                .to_string();
            if !ret.is_empty() {
                output = ret;
            }
        }
    }

    let lower = source.to_lowercase();
    if lower.contains("std::fs::")
        || lower.contains("read_to_string(")
        || lower.contains("write(")
        || lower.contains("open(")
    {
        side_effects.push("file_io".to_string());
    }
    if lower.contains("http") || lower.contains("reqwest") || lower.contains("fetch(") {
        side_effects.push("network_io".to_string());
    }
    if lower.contains("command::new(") || lower.contains("subprocess") || lower.contains("system(")
    {
        side_effects.push("process_spawn".to_string());
    }
    if side_effects.is_empty() {
        side_effects.push("none_detected".to_string());
    }

    (inputs, output, side_effects)
}

pub async fn handle_contract(params: &Value, ctx: &EngineContext) -> Value {
    let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    if uri.is_empty() {
        return json!({"error":"contract requires: uri"});
    }

    let shadow_root = ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
    let read = handle_read(&json!({"uris":[uri], "verbosity": 1}), Arc::clone(&ctx.re), shadow_root).await;
    if read.get("error").is_some() {
        return read;
    }
    let Some(first) = read
        .get("results")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
    else {
        return json!({"error":"contract could not resolve uri"});
    };
    if let Some(e) = first.get("error").and_then(|v| v.as_str()) {
        return json!({"error": e});
    }
    let source = first.get("source").and_then(|v| v.as_str()).unwrap_or("");
    let name = first.get("name").and_then(|v| v.as_str()).unwrap_or(uri);
    let typ = first
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("function");
    let (inputs, output, side_effects) = derive_contract_from_source(source);
    let gist = format!(
        "{} {} accepts {} input(s) and returns {}.",
        typ,
        name,
        inputs.len(),
        output
    );

    json!({
        "status":"ok",
        "uri": uri,
        "type": typ,
        "name": name,
        "contract": {
            "inputs": inputs,
            "output": output,
            "side_effects": side_effects,
            "errors": [],
            "gist_1line": gist
        }
    })
}

pub async fn handle_execute_active_plan(_params: &Value, ctx: &EngineContext) -> Value {
    let mut state = ctx.global_state.lock().await;
    match ctx.ple.execute_active_plan(ctx, &mut state).await {
        Ok(res) => res,
        Err(e) => json!({"error": e.to_string()}),
    }
}

fn parse_role(raw: &str) -> Option<ParticipantRole> {
    match raw {
        "owner" => Some(ParticipantRole::Owner),
        "editor" => Some(ParticipantRole::Editor),
        "planner" => Some(ParticipantRole::Planner),
        "reviewer" => Some(ParticipantRole::Reviewer),
        "observer" => Some(ParticipantRole::Observer),
        _ => None,
    }
}

fn participant_id_from_params<'a>(params: &'a Value) -> Option<&'a str> {
    params.get("participant_id").and_then(|v| v.as_str())
}

async fn resolve_authenticated_actor(
    params: &Value,
    ctx: &EngineContext,
) -> Result<Option<(String, Option<String>)>, Value> {
    let connection_token = params
        .get("connection_token")
        .or_else(|| params.get("session_token"))
        .and_then(|v| v.as_str());
    if let Some(token) = connection_token {
        let connections = ctx.connections.lock().await;
        let Some(entry) = connections.get(token) else {
            return Err(json!({"error": "invalid or expired connection_token"}));
        };
        return Ok(Some((entry.agent_id.clone(), Some(entry.pubkey_hex.clone()))));
    }
    Ok(None)
}

async fn require_bound_role(
    params: &Value,
    ctx: &EngineContext,
) -> Result<(String, ParticipantRole, bool), Value> {
    let authenticated = resolve_authenticated_actor(params, ctx).await?;
    let participant_id_owned;
    let participant_id = if let Some((agent_id, _pubkey)) = authenticated.as_ref() {
        participant_id_owned = agent_id.clone();
        participant_id_owned.as_str()
    } else {
        participant_id_from_params(params).unwrap_or("")
    };
    if participant_id.is_empty() {
        return Err(json!({"error": "participant_id is required"}));
    }
    let store = CollaborationStore::new(&ctx.workspace_root);
    let mut state = match store.load_or_create(ctx.collaboration_id) {
        Ok(state) => state,
        Err(e) => return Err(json!({"error": e.to_string()})),
    };
    store.prune_expired_locks(&mut state);
    if let Some(binding) = store.find_participant(&state, participant_id) {
        if authenticated.is_none()
            && !binding.is_human
            && ctx.config.collaboration.require_session_token_for_agents
        {
            return Err(json!({"error": "connection_token is required for bound agent participants"}));
        }
        if let Some((_, pubkey)) = authenticated
            && let Some(expected) = binding.pubkey_hex.as_ref()
            && pubkey.as_deref() != Some(expected.as_str())
        {
            return Err(json!({"error": "authenticated pubkey does not match bound participant"}));
        }
        Ok((
            participant_id.to_string(),
            binding.role.clone(),
            binding.is_human,
        ))
    } else if ctx.config.collaboration.require_bound_participants {
        Err(json!({"error": format!("participant is not bound to collaboration: {}", participant_id)}))
    } else {
        let inferred_human = params.get("is_human").and_then(|v| v.as_bool()).unwrap_or(false);
        if !inferred_human && ctx.config.collaboration.require_session_token_for_agents {
            return Err(json!({"error": format!("connection_token is required for agent participants: {}", participant_id)}));
        }
        let role_raw = if inferred_human {
            ctx.config.collaboration.default_human_role.as_str()
        } else {
            ctx.config.collaboration.default_agent_role.as_str()
        };
        let role = parse_role(role_raw).unwrap_or(ParticipantRole::Observer);
        Ok((participant_id.to_string(), role, inferred_human))
    }
}

async fn require_capability(
    params: &Value,
    ctx: &EngineContext,
    cap: crate::CollaborationCapability,
) -> Result<(String, ParticipantRole, bool), Value> {
    let (participant_id, role, is_human) = require_bound_role(params, ctx).await?;
    if crate::role_allows(role.clone(), cap) {
        Ok((participant_id, role, is_human))
    } else {
        Err(json!({"error": format!("participant role is not allowed to perform requested capability: {:?}", cap)}))
    }
}

fn audit_action(
    ctx: &EngineContext,
    operation: &str,
    input: Value,
    output: Value,
    success: bool,
    error: Option<String>,
    actor: Option<String>,
    actor_kind: Option<&str>,
) {
    let transaction_id = ctx.we.shadow.lock().ok().and_then(|s| s.get_transaction_id());
    let sequence_id = ctx.event_seq.load(Ordering::SeqCst);
    let resources = collect_contribution_resources(&input, &output);
    let kind = actor_kind
        .map(str::to_string)
        .unwrap_or_else(|| infer_actor_kind(&actor));
    ctx.he.log(
        Some(sequence_id),
        ctx.collaboration_id,
        actor.clone(),
        transaction_id,
        operation,
        input,
        output,
        None,
        None,
        success,
        error.clone(),
        None,
    );
    ctx.ce.log(
        Some(sequence_id),
        ctx.collaboration_id,
        actor,
        &kind,
        "tool_api",
        transaction_id,
        operation,
        resources,
        success,
        error.clone(),
    );
}

fn infer_actor_kind(actor: &Option<String>) -> String {
    actor.as_deref()
        .map(|value| {
            if value.starts_with("human")
                || value.contains("owner")
                || value.contains("reviewer")
                || value.contains("dev")
            {
                "human"
            } else {
                "agent"
            }
        })
        .unwrap_or("unknown")
        .to_string()
}

fn collect_contribution_resources(input: &Value, output: &Value) -> Vec<String> {
    let mut resources = Vec::new();
    for value in [input, output] {
        push_resource_field(&mut resources, value, "uri", "uri:");
        push_resource_field(&mut resources, value, "path", "path:");
        push_resource_field(&mut resources, value, "resource_key", "lock:");
        push_resource_field(&mut resources, value, "variant_id", "variant:");
        push_resource_field(&mut resources, value, "plan_set_id", "plan_set:");
        push_resource_field(&mut resources, value, "proposal_id", "proposal:");
        push_resource_field(&mut resources, value, "package_id", "plugin:");
        push_resource_field(&mut resources, value, "group_id", "tool_group:");
        push_resource_field(&mut resources, value, "key_id", "plugin_key:");
    }
    resources.sort();
    resources.dedup();
    resources
}

fn push_resource_field(out: &mut Vec<String>, value: &Value, field: &str, prefix: &str) {
    if let Some(raw) = value.get(field).and_then(|v| v.as_str()) {
        out.push(format!("{prefix}{raw}"));
    }
}

fn validated_plan_path(params: &Value, ctx: &EngineContext) -> Result<Option<PathBuf>, Value> {
    let Some(path) = params.get("plan_path").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    match crate::workspace::validate_sandboxed_path(&ctx.workspace_root, path) {
        Ok(p) => Ok(Some(p)),
        Err(e) => Err(json!({"error": format!("unsafe plan_path: {}", e)})),
    }
}

fn emit_system_event(ctx: &EngineContext, event: SystemEvent) {
    let _ = ctx.tx_events.send(event);
}

pub async fn handle_bind_participant_role(params: &Value, ctx: &EngineContext) -> Value {
    if !ctx.config.collaboration.enabled {
        return json!({"error": "collaboration is disabled in settings.toml"});
    }
    let participant_id = params.get("participant_id").and_then(|v| v.as_str()).unwrap_or("");
    if participant_id.is_empty() {
        return json!({"error": "bind_participant_role requires: participant_id"});
    }
    let store = CollaborationStore::new(&ctx.workspace_root);
    let existing_state = store.load_or_create(ctx.collaboration_id);
    let authenticated = match resolve_authenticated_actor(params, ctx).await {
        Ok(actor) => actor,
        Err(err) => return err,
    };
    let binding_origin = if authenticated.is_some() { "remote_authenticated" } else { "local_direct" };
    let bootstrap = existing_state
        .as_ref()
        .map(|s| s.participants.is_empty())
        .unwrap_or(true);
    if let Ok(existing) = &existing_state {
        if existing.participants.is_empty() && ctx.config.collaboration.bootstrap_owner_human_only {
            let is_human = params.get("is_human").and_then(|v| v.as_bool()).unwrap_or(false);
            let role_raw = params.get("role").and_then(|v| v.as_str()).unwrap_or("observer");
            if !(is_human && role_raw == "owner") {
                let out = json!({"error": "first participant bootstrap must be a human owner per settings.toml"});
                audit_action(ctx, "bind_participant_role", params.clone(), out.clone(), false, out.get("error").and_then(|v| v.as_str()).map(str::to_string), Some(participant_id.to_string()), Some("human"));
                return out;
            }
        } else if !existing.participants.is_empty() {
            let Ok((_actor, _role, _is_human)) =
            require_capability(params, ctx, crate::CollaborationCapability::RoleBind).await
            else {
                let out = json!({"error": "bind_participant_role requires a bound owner/editor with RoleBind"});
                audit_action(ctx, "bind_participant_role", params.clone(), out.clone(), false, out.get("error").and_then(|v| v.as_str()).map(str::to_string), Some(participant_id.to_string()), None);
                return out;
            };
        }
    }
    let default_role = if params.get("is_human").and_then(|v| v.as_bool()).unwrap_or(false) {
        ctx.config.collaboration.default_human_role.as_str()
    } else {
        ctx.config.collaboration.default_agent_role.as_str()
    };
    let role_raw = params.get("role").and_then(|v| v.as_str()).unwrap_or(default_role);
    let Some(role) = parse_role(role_raw) else {
        return json!({"error": format!("unsupported role: {role_raw}")});
    };
    let res = match store.bind_participant(
        ctx.collaboration_id,
        participant_id,
        params.get("display_name").and_then(|v| v.as_str()),
        role,
        params.get("is_human").and_then(|v| v.as_bool()).unwrap_or(false),
        params.get("pubkey_hex").and_then(|v| v.as_str()).map(|s| s.to_string()),
        authenticated.as_ref().map(|(id, _)| id.clone()),
        binding_origin,
        bootstrap,
    ) {
        Ok(state) => json!({"status": "ok", "session": state}),
        Err(e) => json!({"error": e.to_string()}),
    };
    audit_action(
        ctx,
        "bind_participant_role",
        params.clone(),
        res.clone(),
        res.get("error").is_none(),
        res.get("error").and_then(|v| v.as_str()).map(str::to_string),
        Some(participant_id.to_string()),
        Some(if params.get("is_human").and_then(|v| v.as_bool()).unwrap_or(false) { "human" } else { "agent" }),
    );
    if res.get("error").is_none() {
        emit_system_event(
            ctx,
            SystemEvent::ParticipantBound {
                participant_id: participant_id.to_string(),
                role: role_raw.to_string(),
                is_human: params.get("is_human").and_then(|v| v.as_bool()).unwrap_or(false),
            },
        );
    }
    res
}

pub async fn handle_list_session_participants(_params: &Value, ctx: &EngineContext) -> Value {
    let store = CollaborationStore::new(&ctx.workspace_root);
    match store.load_or_create(ctx.collaboration_id) {
        Ok(mut state) => {
            store.prune_expired_locks(&mut state);
            json!({"status": "ok", "collaboration_id": ctx.collaboration_id, "session_id": ctx.collaboration_id, "participants": state.participants, "locks": state.human_override_locks})
        }
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_claim_human_override(params: &Value, ctx: &EngineContext) -> Value {
    let Ok((participant_id, _role, is_human)) =
        require_capability(params, ctx, crate::CollaborationCapability::WorkspaceMutate).await
    else {
        return json!({"error": "claim_human_override requires a bound participant with workspace mutation capability"});
    };
    if !is_human {
        return json!({"error": "only human participants can claim human override"});
    }
    let resource_key = params.get("resource_key").and_then(|v| v.as_str()).unwrap_or("");
    if resource_key.is_empty() {
        return json!({"error": "claim_human_override requires: resource_key"});
    }
    if let Err(e) = crate::workspace::validate_sandboxed_path(&ctx.workspace_root, resource_key) {
        return json!({"error": format!("resource_key must be a sandboxed workspace-relative path: {}", e)});
    }
    let ttl_secs = params
        .get("ttl_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(ctx.config.collaboration.human_override_ttl_secs);
    let store = CollaborationStore::new(&ctx.workspace_root);
    let res = match store.claim_lock(ctx.collaboration_id, &participant_id, resource_key, ttl_secs) {
        Ok(state) => {
            ctx.locks.lock().await.insert(
                resource_key.to_string(),
                LockInfo {
                    owner_id: "human".to_string(),
                    expires_at_secs: now_secs() + ttl_secs,
                },
            );
            json!({"status": "ok", "session": state})
        }
        Err(e) => json!({"error": e.to_string()}),
    };
    audit_action(ctx, "claim_human_override", params.clone(), res.clone(), res.get("error").is_none(), res.get("error").and_then(|v| v.as_str()).map(str::to_string), Some(participant_id), Some("human"));
    if res.get("error").is_none() {
        emit_system_event(
            ctx,
            SystemEvent::HumanOverrideClaimed {
                participant_id: params.get("participant_id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                resource_key: resource_key.to_string(),
            },
        );
    }
    res
}

pub async fn handle_release_human_override(params: &Value, ctx: &EngineContext) -> Value {
    let Ok((participant_id, _role, is_human)) =
        require_capability(params, ctx, crate::CollaborationCapability::WorkspaceMutate).await
    else {
        return json!({"error": "release_human_override requires a bound participant with workspace mutation capability"});
    };
    if !is_human {
        return json!({"error": "only human participants can release human override"});
    }
    let resource_key = params.get("resource_key").and_then(|v| v.as_str()).unwrap_or("");
    if resource_key.is_empty() {
        return json!({"error": "release_human_override requires: resource_key"});
    }
    if let Err(e) = crate::workspace::validate_sandboxed_path(&ctx.workspace_root, resource_key) {
        return json!({"error": format!("resource_key must be a sandboxed workspace-relative path: {}", e)});
    }
    let store = CollaborationStore::new(&ctx.workspace_root);
    let res = match store.release_lock(ctx.collaboration_id, &participant_id, resource_key) {
        Ok(state) => {
            ctx.locks.lock().await.remove(resource_key);
            json!({"status": "ok", "session": state})
        }
        Err(e) => json!({"error": e.to_string()}),
    };
    audit_action(ctx, "release_human_override", params.clone(), res.clone(), res.get("error").is_none(), res.get("error").and_then(|v| v.as_str()).map(str::to_string), Some(participant_id), Some("human"));
    if res.get("error").is_none() {
        emit_system_event(
            ctx,
            SystemEvent::HumanOverrideReleased {
                participant_id: params.get("participant_id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                resource_key: resource_key.to_string(),
            },
        );
    }
    res
}

pub async fn handle_create_plan_set(params: &Value, ctx: &EngineContext) -> Value {
    if !ctx.config.variants.enabled {
        return json!({"error": "variants are disabled in settings.toml"});
    }
    let Ok((participant_id, _role, _is_human)) =
        require_capability(params, ctx, crate::CollaborationCapability::PlanCreate).await
    else {
        return json!({"error": "create_plan_set requires a participant with plan creation capability"});
    };
    let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let objective = params.get("objective").and_then(|v| v.as_str()).unwrap_or("");
    if title.is_empty() || objective.is_empty() {
        return json!({"error": "create_plan_set requires: title, objective"});
    }
    let created_by = params.get("created_by").and_then(|v| v.as_str()).unwrap_or(&participant_id);
    let store = VariantStore::new(&ctx.workspace_root);
    let res = match store.create_plan_set(&ctx.config, ctx.collaboration_id, title.to_string(), objective.to_string(), created_by.to_string()) {
        Ok(plan_set) => json!({"status": "ok", "plan_set": plan_set}),
        Err(e) => json!({"error": e.to_string()}),
    };
    audit_action(ctx, "create_plan_set", params.clone(), res.clone(), res.get("error").is_none(), res.get("error").and_then(|v| v.as_str()).map(str::to_string), Some(participant_id), None);
    if let Some(plan_set_id) = res.get("plan_set").and_then(|p| p.get("id")).and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) {
        emit_system_event(
            ctx,
            SystemEvent::PlanSetCreated {
                plan_set_id,
                title: title.to_string(),
            },
        );
        if let Ok(retention) = store.enforce_retention(&ctx.config) {
            if let Some(pruned) = retention.get("pruned_plan_sets").and_then(|v| v.as_array()) {
                for id in pruned.iter().filter_map(|v| v.as_str()).filter_map(|s| Uuid::parse_str(s).ok()) {
                    emit_system_event(ctx, SystemEvent::PlanSetPruned { plan_set_id: id });
                }
            }
        }
    }
    res
}

pub async fn handle_list_plan_sets(_params: &Value, ctx: &EngineContext) -> Value {
    let store = VariantStore::new(&ctx.workspace_root);
    match store.list_plan_sets() {
        Ok(plan_sets) => json!({"status": "ok", "plan_sets": plan_sets}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_create_plan_variant(params: &Value, ctx: &EngineContext) -> Value {
    let Ok((participant_id, _role, _is_human)) =
        require_capability(params, ctx, crate::CollaborationCapability::PlanCreate).await
    else {
        return json!({"error": "create_plan_variant requires a participant with plan creation capability"});
    };
    let Some(plan_set_id) = params.get("plan_set_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "create_plan_variant requires valid plan_set_id"});
    };
    let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let strategy_summary = params.get("strategy_summary").and_then(|v| v.as_str()).unwrap_or("");
    if title.is_empty() {
        return json!({"error": "create_plan_variant requires: title"});
    }
    let created_by = params.get("created_by").and_then(|v| v.as_str()).unwrap_or(&participant_id);
    let plan_value = if let Some(path) = match validated_plan_path(params, ctx) {
        Ok(path) => path,
        Err(err) => return err,
    } {
        let meta = match fs::metadata(&path) {
            Ok(meta) => meta,
            Err(e) => return json!({"error": format!("failed to stat plan_path: {e}")}),
        };
        if meta.len() as usize > ctx.config.variants.max_plan_bytes {
            return json!({"error": format!("plan_path exceeds max_plan_bytes ({})", ctx.config.variants.max_plan_bytes)});
        }
        match fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(value) => value,
                Err(e) => return json!({"error": format!("invalid plan_path JSON: {e}")}),
            },
            Err(e) => return json!({"error": format!("failed to read plan_path: {e}")}),
        }
    } else {
        params.get("plan").cloned().unwrap_or(Value::Null)
    };
    let estimated_plan_bytes = serde_json::to_vec(&plan_value).map(|v| v.len()).unwrap_or(usize::MAX);
    if estimated_plan_bytes > ctx.config.variants.max_plan_bytes {
        return json!({"error": format!("plan payload exceeds max_plan_bytes ({})", ctx.config.variants.max_plan_bytes)});
    }
    let plan: Plan = match serde_json::from_value(plan_value) {
        Ok(plan) => plan,
        Err(e) => return json!({"error": format!("invalid plan payload: {e}")}),
    };
    let backend_raw = params.get("backend").and_then(|v| v.as_str()).unwrap_or(&ctx.config.variants.default_backend);
    if backend_raw == "worktree" && !ctx.config.variants.allow_worktree_backend {
        return json!({"error": "worktree backend is disabled in settings.toml"});
    }
    let backend = match backend_raw {
        "shadow" => VariantWorkspaceBackend::Shadow,
        "worktree" => VariantWorkspaceBackend::Worktree,
        _ => return json!({"error": format!("unsupported backend: {backend_raw}")}),
    };
    let assumptions = params.get("assumptions").and_then(|v| v.as_array()).map(|a| {
        a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>()
    }).unwrap_or_default();
    let risk_tags = params.get("risk_tags").and_then(|v| v.as_array()).map(|a| {
        a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>()
    }).unwrap_or_default();
    let store = VariantStore::new(&ctx.workspace_root);
    let res = match store.create_variant(
        &ctx.config,
        plan_set_id,
        title.to_string(),
        strategy_summary.to_string(),
        created_by.to_string(),
        plan,
        assumptions,
        risk_tags,
        backend,
    ) {
        Ok(variant) => json!({"status": "ok", "variant": variant}),
        Err(e) => json!({"error": e.to_string()}),
    };
    audit_action(ctx, "create_plan_variant", params.clone(), res.clone(), res.get("error").is_none(), res.get("error").and_then(|v| v.as_str()).map(str::to_string), Some(participant_id), None);
    if let Some(variant) = res.get("variant") {
        if let (Some(plan_set_id), Some(variant_id)) = (
            variant.get("plan_set_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()),
            variant.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()),
        ) {
            emit_system_event(
                ctx,
                SystemEvent::PlanVariantCreated {
                    plan_set_id,
                    variant_id,
                    title: title.to_string(),
                },
            );
        }
    }
    res
}

pub async fn handle_list_plan_variants(params: &Value, ctx: &EngineContext) -> Value {
    let Some(plan_set_id) = params.get("plan_set_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "list_plan_variants requires valid plan_set_id"});
    };
    let store = VariantStore::new(&ctx.workspace_root);
    match store.list_variants(plan_set_id) {
        Ok(variants) => json!({"status": "ok", "variants": variants}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_simulate_plan_variant(params: &Value, ctx: &EngineContext) -> Value {
    let Ok((_participant_id, _role, _is_human)) =
        require_capability(params, ctx, crate::CollaborationCapability::PlanSimulate).await
    else {
        return json!({"error": "simulate_plan_variant requires a participant with plan simulation capability"});
    };
    let Some(plan_set_id) = params.get("plan_set_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "simulate_plan_variant requires valid plan_set_id"});
    };
    let Some(variant_id) = params.get("variant_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "simulate_plan_variant requires valid variant_id"});
    };
    let store = VariantStore::new(&ctx.workspace_root);
    match store.load_variant(plan_set_id, variant_id) {
        Ok(mut variant) => match store.simulate_variant(&mut variant, &ctx.config).await {
            Ok(result) => {
                emit_system_event(ctx, SystemEvent::PlanVariantSimulated { plan_set_id, variant_id });
                if let Ok(retention) = store.enforce_retention(&ctx.config)
                    && let Some(pruned) = retention.get("pruned_variant_workspaces").and_then(|v| v.as_array())
                {
                    for item in pruned {
                        if let (Some(ps), Some(vid)) = (
                            item.get("plan_set_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()),
                            item.get("variant_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()),
                        ) {
                            emit_system_event(ctx, SystemEvent::VariantWorkspacePruned { plan_set_id: ps, variant_id: vid });
                        }
                    }
                }
                result
            }
            Err(e) => json!({"error": e.to_string()}),
        },
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_compare_plan_variants(params: &Value, ctx: &EngineContext) -> Value {
    let Ok((_participant_id, _role, _is_human)) =
        require_capability(params, ctx, crate::CollaborationCapability::PlanCompare).await
    else {
        return json!({"error": "compare_plan_variants requires a participant with plan compare capability"});
    };
    let Some(plan_set_id) = params.get("plan_set_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "compare_plan_variants requires valid plan_set_id"});
    };
    let variant_ids = params.get("variant_ids").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str())
            .filter_map(|s| Uuid::parse_str(s).ok())
            .collect::<Vec<_>>()
    }).unwrap_or_default();
    if variant_ids.is_empty() {
        return json!({"error": "compare_plan_variants requires non-empty variant_ids"});
    }
    if variant_ids.len() > 8 {
        return json!({"error": "compare_plan_variants supports at most 8 variants per request"});
    }
    let store = VariantStore::new(&ctx.workspace_root);
    match store.compare_variants(&ctx.config, plan_set_id, &variant_ids) {
        Ok(result) => {
            emit_system_event(ctx, SystemEvent::PlanVariantCompared { plan_set_id, variant_ids });
            result
        }
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_review_plan_variant(params: &Value, ctx: &EngineContext) -> Value {
    let Ok((participant_id, _role, _is_human)) =
        require_capability(params, ctx, crate::CollaborationCapability::PlanReview).await
    else {
        return json!({"error": "review_plan_variant requires a participant with plan review capability"});
    };
    let Some(plan_set_id) = params.get("plan_set_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "review_plan_variant requires valid plan_set_id"});
    };
    let Some(variant_id) = params.get("variant_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "review_plan_variant requires valid variant_id"});
    };
    let decision = params.get("decision").and_then(|v| v.as_str()).unwrap_or("");
    if !matches!(decision, "review" | "approve" | "reject") {
        return json!({"error": "review_plan_variant decision must be one of: review, approve, reject"});
    }
    let store = VariantStore::new(&ctx.workspace_root);
    let res = match store.review_variant(
        plan_set_id,
        variant_id,
        participant_id,
        decision,
        params.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string()),
    ) {
        Ok(variant) => json!({"status": "ok", "variant": variant}),
        Err(e) => json!({"error": e.to_string()}),
    };
    audit_action(ctx, "review_plan_variant", params.clone(), res.clone(), res.get("error").is_none(), res.get("error").and_then(|v| v.as_str()).map(str::to_string), params.get("participant_id").and_then(|v| v.as_str()).map(str::to_string), None);
    if res.get("error").is_none() {
        emit_system_event(
            ctx,
            SystemEvent::PlanVariantReviewed {
                plan_set_id,
                variant_id,
                decision: decision.to_string(),
            },
        );
    }
    res
}

pub async fn handle_promote_plan_variant(params: &Value, ctx: &EngineContext) -> Value {
    let Ok((participant_id, role, _is_human)) =
        require_capability(params, ctx, crate::CollaborationCapability::VariantPromote).await
    else {
        return json!({"error": "promote_plan_variant requires a participant with promotion capability"});
    };
    let Some(plan_set_id) = params.get("plan_set_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "promote_plan_variant requires valid plan_set_id"});
    };
    let Some(variant_id) = params.get("variant_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) else {
        return json!({"error": "promote_plan_variant requires valid variant_id"});
    };
    if !ctx.config.collaboration.editor_can_promote && matches!(role, ParticipantRole::Editor) {
        return json!({"error": "editor promotion is disabled in settings.toml"});
    }
    let store = VariantStore::new(&ctx.workspace_root);
    match store.load_variant(plan_set_id, variant_id) {
        Ok(mut variant) => {
            if ctx.config.variants.require_review_for_promotion
                && !matches!(variant.status, crate::PlanVariantStatus::Reviewed | crate::PlanVariantStatus::Approved)
            {
                return json!({"error": "promotion requires reviewed or approved variant per settings.toml"});
            }
            let collab = CollaborationStore::new(&ctx.workspace_root);
            if let Ok(mut session) = collab.load_or_create(ctx.collaboration_id) {
                collab.prune_expired_locks(&mut session);
                if session.human_override_locks.iter().any(|lock| lock.owner_participant_id != participant_id) {
                    return json!({"error": "promotion blocked by active human override lock"});
                }
            }
            let res = match store.promote_variant(ctx, &mut variant) {
                Ok(result) => result,
                Err(e) => json!({"error": e.to_string()}),
            };
            audit_action(ctx, "promote_plan_variant", params.clone(), res.clone(), res.get("error").is_none(), res.get("error").and_then(|v| v.as_str()).map(str::to_string), Some(participant_id), None);
            if res.get("error").is_none() {
                emit_system_event(ctx, SystemEvent::PlanVariantPromoted { plan_set_id, variant_id });
            }
            res
        }
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_search(params: &Value, ctx: &EngineContext) -> Value {
    let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("symbol");

    match mode {
        "symbol" => {
            let kind = params
                .get("kind")
                .and_then(|v| v.as_str())
                .and_then(|s| match s {
                    "function" => Some(SymbolKind::Function),
                    "class" => Some(SymbolKind::Class),
                    "method" => Some(SymbolKind::Method),
                    _ => None,
                });
            let query_owned = query.to_string();
            let se = Arc::clone(&ctx.se);
            let root = ctx.workspace_root.clone();
            match tokio::task::spawn_blocking(move || {
                let mut res = se.search(&query_owned, kind.clone());
                let stats = se.last_index_stats();
                
                let mut provenance = "local".to_string();
                if let Ok(ref syms) = res
                    && syms.is_empty() {
                        let cfg = CurdConfig::load_from_workspace(&root);
                        if cfg.reference.enable_delegation {
                            for url in cfg.reference.instances.values() {
                                let kind_str = match kind {
                                    Some(SymbolKind::Function) => Some("function"),
                                    Some(SymbolKind::Class) => Some("class"),
                                    Some(SymbolKind::Method) => Some("method"),
                                    _ => None,
                                };
                                let mut params = serde_json::Map::new();
                                params.insert("query".to_string(), json!(query_owned));
                                params.insert("mode".to_string(), json!("symbol"));
                                if let Some(k) = kind_str {
                                    params.insert("kind".to_string(), json!(k));
                                }
                                if !url.starts_with("http://") && !url.starts_with("https://") {
                                    log::warn!("Blocking non-HTTP(S) delegation URL: {}", url);
                                    continue;
                                }

                                // SSRF PROTECTION: Block localhost and private IP ranges
                                // More robust host extraction: find everything between :// and the next / or @
                                let host_part = url.split("://").nth(1).unwrap_or("");
                                let host_and_port = if let Some(at_idx) = host_part.find('@') {
                                    &host_part[at_idx + 1..]
                                } else {
                                    host_part
                                };
                                let host = host_and_port.split('/').next().unwrap_or("").split(':').next().unwrap_or("");
                                
                                let mut is_blocked = false;
                                let host_lower = host.to_lowercase();
                                if host_lower == "localhost" {
                                    is_blocked = true;
                                } else if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(host, 0)) {
                                    for addr in addrs {
                                        let ip = addr.ip();
                                        if ip.is_loopback() || ip.is_unspecified() {
                                            is_blocked = true;
                                            break;
                                        }
                                        if let std::net::IpAddr::V4(ipv4) = ip {
                                            let octets = ipv4.octets();
                                            if octets[0] == 10 || 
                                               (octets[0] == 172 && (16..=31).contains(&octets[1])) || 
                                               (octets[0] == 192 && octets[1] == 168) ||
                                               (octets[0] == 169 && octets[1] == 254) {
                                                is_blocked = true;
                                                break;
                                            }
                                        } else if let std::net::IpAddr::V6(ipv6) = ip {
                                            if (ipv6.segments()[0] & 0xfe00) == 0xfc00 {
                                                is_blocked = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                                
                                if is_blocked {
                                    log::warn!("Blocking restricted delegation URL: {}", url);
                                    continue;
                                }

                                let payload = json!({
                                    "jsonrpc": "2.0",
                                    "id": 1,
                                    "method": "search",
                                    "params": params
                                });
                                let body = serde_json::to_string(&payload).unwrap_or_default();
                                if let Ok(resp) = ureq::post(url).set("Content-Type", "application/json").send_string(&body)
                                    && let Ok(body_str) = resp.into_string()
                                        && let Ok(json) = serde_json::from_str::<Value>(&body_str)
                                            && let Some(arr) = json.get("result").and_then(|r| r.get("symbols")).and_then(|s| s.as_array())
                                                && !arr.is_empty() {
                                                    let ext_syms: Vec<crate::Symbol> = arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect();
                                                    res = Ok(ext_syms);
                                                    provenance = "external".to_string();
                                                    break;
                                                }
                            }
                        }
                    }

                (res, stats, provenance)
            })
            .await
            {
                Ok((Ok(symbols), stats, provenance)) => {
                    let coverage = stats.as_ref().map(build_index_coverage);
                    let quality = stats.as_ref().map(build_index_quality);
                    json!({
                        "status": "ok",
                        "symbols": symbols,
                        "index_stats": stats,
                        "index_coverage": coverage,
                        "index_quality": quality,
                        "provenance": provenance
                    })
                }
                Ok((Err(e), _, _)) => json!({"error": e.to_string()}),
                Err(e) => json!({"error": format!("Task join error in search(symbol): {}", e)}),
            }
        }
        "text" => {
            let query_owned = query.to_string();
            let fe = Arc::clone(&ctx.fe);
            match tokio::task::spawn_blocking(move || fe.find(&query_owned)).await {
                Ok(Ok(res)) => res,
                Ok(Err(e)) => json!({"error": e.to_string()}),
                Err(e) => json!({"error": format!("Task join error in search(text): {}", e)}),
            }
        }
        "tiered" => {
            let query_owned = query.to_string();
            let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
            let fe = Arc::clone(&ctx.fe);
            let se = Arc::clone(&ctx.se);
            let workspace_root = ctx.workspace_root.clone();
            match tokio::task::spawn_blocking(move || {
                let text = fe.find(&query_owned)?;
                let seed_results = text
                    .get("results")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let seed_count = seed_results.len();
                let mut seed_files = std::collections::HashSet::new();
                for row in &seed_results {
                    if let Some(f) = row.get("file").and_then(|v| v.as_str()) {
                        seed_files.insert(f.trim_start_matches("./").to_string());
                    }
                }

                let symbols = se.search(&query_owned, None)?;
                let stats = se.last_index_stats();
                let mut filtered = Vec::new();
                if seed_files.is_empty() {
                    filtered = symbols.clone();
                } else {
                    for s in &symbols {
                        let rel = s
                            .filepath
                            .strip_prefix(&workspace_root)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| s.filepath.to_string_lossy().to_string())
                            .trim_start_matches("./")
                            .to_string();
                        if seed_files.contains(&rel) {
                            filtered.push(s.clone());
                        }
                    }
                    if filtered.is_empty() {
                        filtered = symbols;
                    }
                }
                filtered.truncate(limit);
                let cfg = CurdConfig::load_from_workspace(&workspace_root);
                let (db_rows, tier3_status, tier3_error) =
                    match read_recent_index_runs(&workspace_root, &cfg, 5) {
                        Ok(rows) => (rows, "ok".to_string(), None),
                        Err(e) => (Vec::new(), "degraded".to_string(), Some(e.to_string())),
                    };
                Ok::<_, anyhow::Error>((
                    seed_results,
                    seed_count,
                    filtered,
                    stats,
                    db_rows,
                    tier3_status,
                    tier3_error,
                ))
            })
            .await
            {
                Ok(Ok((
                    seed_results,
                    seed_count,
                    symbols,
                    stats,
                    db_rows,
                    tier3_status,
                    tier3_error,
                ))) => {
                    let coverage = stats.as_ref().map(build_index_coverage);
                    let quality = stats.as_ref().map(build_index_quality);
                    let coverage_state = coverage
                        .as_ref()
                        .and_then(|v| v.get("state"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let unknown_frontier = !matches!(coverage_state, "full" | "scoped_full");
                    json!({
                        "status": "ok",
                        "tiered": {
                            "tier1": {
                                "kind": "text_seed",
                                "count": seed_count,
                                "results": seed_results
                            },
                            "tier2": {
                                "kind": "structured_symbol",
                                "count": symbols.len(),
                                "symbols": symbols
                            },
                            "tier3": {
                                "kind": "local_db_index_runs",
                                "status": tier3_status,
                                "count": db_rows.len(),
                                "error": tier3_error,
                                "rows": db_rows
                            },
                            "frontier": {
                                "unknown_frontier": unknown_frontier,
                                "coverage_state": coverage_state
                            }
                        },
                        "index_stats": stats,
                        "index_coverage": coverage,
                        "index_quality": quality
                    })
                }
                Ok(Err(e)) => json!({"error": e.to_string()}),
                Err(e) => json!({"error": format!("Task join error in search(tiered): {}", e)}),
            }
        }
        _ => json!({"error": format!("Unknown search mode: {}", mode)}),
    }
}

pub fn build_index_coverage(stats: &IndexBuildStats) -> Value {
    let processed = stats.cache_hits.saturating_add(stats.cache_misses);
    let total = stats.total_files;
    let ratio = if total == 0 {
        0.0
    } else {
        (processed as f64 / total as f64).clamp(0.0, 1.0)
    };
    let state = match stats.index_mode.as_str() {
        "full" if ratio >= 0.999 => "full",
        "fast" => "fast_partial",
        "scoped" if ratio >= 0.999 => "scoped_full",
        "scoped" => "scoped_partial",
        "lazy" => "lazy_partial",
        _ if ratio >= 0.999 => "full",
        _ => "partial",
    };
    json!({
        "state": state,
        "mode": stats.index_mode,
        "processed_files": processed,
        "total_files": total,
        "coverage_ratio": ratio
    })
}

pub fn build_index_quality(stats: &IndexBuildStats) -> Value {
    let total = stats.total_files.max(1);
    let no_symbols_ratio = stats.no_symbols as f64 / total as f64;
    let skipped_large_ratio = stats.skipped_too_large as f64 / total as f64;
    let fast_prefilter_ratio = stats.fast_prefilter_skips as f64 / total as f64;

    let mut warnings: Vec<&str> = Vec::new();
    if stats.parse_fail > 0 {
        warnings.push("parse_fail");
    }
    if no_symbols_ratio > 0.90 {
        warnings.push("low_symbol_yield");
    }
    if skipped_large_ratio > 0.20 {
        warnings.push("large_file_skip_pressure");
    }
    if fast_prefilter_ratio > 0.50 {
        warnings.push("fast_prefilter_pressure");
    }

    let status = if stats.parse_fail > 0 {
        "fail"
    } else if warnings.is_empty() {
        "ok"
    } else {
        "warn"
    };

    json!({
        "status": status,
        "warnings": warnings,
        "no_symbols_ratio": no_symbols_ratio,
        "skipped_large_ratio": skipped_large_ratio,
        "fast_prefilter_ratio": fast_prefilter_ratio
    })
}

pub async fn handle_read(params: &Value, engine: Arc<ReadEngine>, shadow_root: Option<PathBuf>) -> Value {
    let uris: Vec<String> = params
        .get("uris")
        .and_then(|u| u.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let verbosity = params
        .get("verbosity")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u8;
    match tokio::task::spawn_blocking(move || engine.read(uris, verbosity, shadow_root.as_deref())).await {
        Ok(Ok(res)) => json!({"status": "ok", "results": res}),
        Ok(Err(e)) => json!({"error": e.to_string()}),
        Err(e) => json!({"error": format!("Task join error in read: {}", e)}),
    }
}

pub async fn handle_edit(params: &Value, engine: Arc<EditEngine>, shadow_root: Option<PathBuf>) -> Value {
    let uri = params
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let code = params
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("upsert")
        .to_string();
    let base_state_hash = params
        .get("base_state_hash")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let justification = params
        .get("adaptation_justification")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if action == "upsert" && justification.trim().is_empty() {
        return json!({"error": "Missing 'adaptation_justification'. You must provide a concise technical reason why this specific adaptation is necessary."});
    }

    match tokio::task::spawn_blocking(move || engine.edit(&uri, &code, &action, base_state_hash.as_deref(), shadow_root.as_deref())).await {
        Ok(Ok(res)) => json!({"status": "ok", "message": res}),
        Ok(Err(e)) => json!({"error": e.to_string()}),
        Err(e) => json!({"error": format!("Task join error in edit: {}", e)}),
    }
}

pub async fn handle_graph(params: &Value, engine: Arc<GraphEngine>) -> Value {
    let uris: Vec<String> = params
        .get("uris")
        .and_then(|u| u.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let direction = params
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("both")
        .to_string();
    let depth = params.get("depth").and_then(|v| v.as_u64()).unwrap_or(1) as u8;
    let min_confidence = params
        .get("min_confidence")
        .and_then(|v| v.as_f64())
        .filter(|v| (0.0..=1.0).contains(v));
    let tiers = params.get("tiers").and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|value| value.as_str().map(|s| s.to_string()))
                .collect::<std::collections::HashSet<_>>()
        })
    });
    let sources = params.get("sources").and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|value| value.as_str().map(|s| s.to_string()))
                .collect::<std::collections::HashSet<_>>()
        })
    });
    match tokio::task::spawn_blocking(move || {
        let up_depth = if direction == "up" || direction == "both" {
            depth
        } else {
            0
        };
        let down_depth = if direction == "down" || direction == "both" {
            depth
        } else {
            0
        };
        engine.graph_with_filters(uris, up_depth, down_depth, min_confidence, tiers, sources)
    })
    .await
    {
        Ok(Ok(res)) => json!({"status": "ok", "graph": res}),
        Ok(Err(e)) => json!({"error": e.to_string()}),
        Err(e) => json!({"error": format!("Task join error in graph: {}", e)}),
    }
}

pub async fn handle_workspace(params: &Value, ctx: &EngineContext) -> Value {
    let engine = Arc::clone(&ctx.we);
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("status");

    if action == "commit" {
        let proposal_id = params
            .get("proposal_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let allow_unapproved = params
            .get("allow_unapproved")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_high = params.get("max_high").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let max_medium = params
            .get("max_medium")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let max_low = params
            .get("max_low")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let allow_high = params
            .get("allow_high")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !allow_unapproved {
            if proposal_id.is_empty() {
                return json!({
                    "error": {
                        "code": -32012,
                        "message": "Commit blocked by proposal gate: missing proposal_id",
                        "details": {
                            "require_approved_proposal": true
                        }
                    }
                });
            }
            let current_snapshot = match compute_workspace_snapshot_id_async(
                ctx.workspace_root.clone(),
            )
            .await
            {
                Ok(s) => s,
                Err(e) => {
                    return json!({
                        "error": {
                            "code": -32016,
                            "message": "Commit blocked by proposal gate: failed to compute workspace snapshot",
                            "details": e
                        }
                    });
                }
            };
            match load_proposal(&ctx.workspace_root, proposal_id) {
                Some(p) if p.status == "approved" => {
                    let approved = p.approved_snapshot_id.as_deref().unwrap_or("");
                    if approved != current_snapshot {
                        return json!({
                            "error": {
                                "code": -32015,
                                "message": "Commit blocked by proposal gate: approved proposal is stale",
                                "details": {
                                    "proposal_id": proposal_id,
                                    "approved_snapshot_id": p.approved_snapshot_id,
                                    "current_snapshot_id": current_snapshot
                                }
                            }
                        });
                    }
                }
                Some(p) => {
                    return json!({
                        "error": {
                            "code": -32013,
                            "message": "Commit blocked by proposal gate: proposal is not approved",
                            "details": {
                                "proposal_id": proposal_id,
                                "proposal_status": p.status
                            }
                        }
                    });
                }
                None => {
                    return json!({
                        "error": {
                            "code": -32014,
                            "message": "Commit blocked by proposal gate: proposal not found",
                            "details": {
                                "proposal_id": proposal_id
                            }
                        }
                    });
                }
            }
        }

        let session_active = ctx
            .rce
            .status()
            .ok()
            .and_then(|v| v.get("active").and_then(|a| a.as_bool()))
            .unwrap_or(false);

        if session_active {
            match ctx.rce.review().await {
                Ok(review) => {
                    let summary = review.get("summary").cloned().unwrap_or_else(|| json!({}));
                    let high = summary.get("high").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let medium =
                        summary.get("medium").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let low = summary.get("low").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                    let high_blocked = !allow_high && high > max_high;
                    let medium_blocked = max_medium.map(|m| medium > m).unwrap_or(false);
                    let low_blocked = max_low.map(|m| low > m).unwrap_or(false);

                    if high_blocked || medium_blocked || low_blocked {
                        return json!({
                            "error": {
                                "code": -32010,
                                "message": "Commit blocked by review-cycle threshold gate",
                                "details": {
                                    "thresholds": {
                                        "max_high": max_high,
                                        "max_medium": max_medium,
                                        "max_low": max_low,
                                        "allow_high": allow_high
                                    },
                                    "summary": summary
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    return json!({
                        "error": {
                            "code": -32011,
                            "message": "Failed to run review-cycle gate before commit",
                            "details": e.to_string()
                        }
                    });
                }
            }
        }
    }

    let action_owned = action.to_string();
    match tokio::task::spawn_blocking(move || engine.execute(&action_owned)).await {
        Ok(Ok(res)) => {
            let mut out = json!({"status": "ok", "result": res});
            if action == "commit" {
                let proposal_id = params
                    .get("proposal_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let allow_unapproved = params
                    .get("allow_unapproved")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let committed = out
                    .get("result")
                    .and_then(|r| r.get("committed"))
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                let stamp = json!({
                    "timestamp_secs": now_secs(),
                    "proposal_id": if proposal_id.is_empty() { json!(null) } else { json!(proposal_id) },
                    "allow_unapproved": allow_unapproved,
                    "committed": committed,
                    "gate": {
                        "max_high": params.get("max_high").cloned().unwrap_or(json!(0)),
                        "max_medium": params.get("max_medium").cloned().unwrap_or(json!(null)),
                        "max_low": params.get("max_low").cloned().unwrap_or(json!(null)),
                        "allow_high": params.get("allow_high").cloned().unwrap_or(json!(false))
                    }
                });
                let cdir = commits_dir(&ctx.workspace_root);
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let path = cdir.join(format!(
                    "commit_{}_{}_{}.json",
                    now_secs(),
                    nanos,
                    Uuid::new_v4()
                ));
                match fs::create_dir_all(&cdir).and_then(|_| {
                    fs::write(
                        &path,
                        serde_json::to_string_pretty(&stamp).unwrap_or_else(|_| "{}".to_string()),
                    )
                }) {
                    Ok(_) => {
                        out["provenance_path"] = json!(
                            path.strip_prefix(&ctx.workspace_root)
                                .ok()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.to_string_lossy().to_string())
                        );
                    }
                    Err(e) => {
                        out["provenance_error"] =
                            json!(format!("Failed to persist commit provenance: {}", e));
                    }
                }
            }
            out
        }
        Ok(Err(e)) => json!({"error": e.to_string()}),
        Err(e) => json!({"error": format!("Task join error in workspace: {}", e)}),
    }
}

pub async fn handle_shell(params: &Value, engine: &ShellEngine, shadow_root: Option<&Path>) -> Value {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("execute");
    
    match action {
        "execute" => {
            let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let is_background = params.get("is_background").and_then(|v| v.as_bool()).unwrap_or(false);
            match engine.shell(command, shadow_root, is_background).await {
                Ok(res) => json!({"status": "ok", "output": res}),
                Err(e) => json!({"error": e.to_string()}),
            }
        }
        "status" => {
            let task_id_str = params.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            if let Ok(task_id) = Uuid::parse_str(task_id_str) {
                match engine.status(task_id).await {
                    Ok(res) => json!({"status": "ok", "result": res}),
                    Err(e) => json!({"error": e.to_string()}),
                }
            } else {
                json!({"error": "Invalid or missing task_id for status action."})
            }
        }
        "terminate" => {
            let task_id_str = params.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            if let Ok(task_id) = Uuid::parse_str(task_id_str) {
                match engine.terminate(task_id).await {
                    Ok(res) => json!({"status": "ok", "result": res}),
                    Err(e) => json!({"error": e.to_string()}),
                }
            } else {
                json!({"error": "Invalid or missing task_id for terminate action."})
            }
        }
        _ => json!({"error": "shell action must be one of: execute, terminate"}),
    }
}

pub async fn handle_diagram(params: &Value, engine: Arc<DiagramEngine>) -> Value {
    let uris: Vec<String> = params
        .get("uris")
        .and_then(|u| u.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let format = params.get("format").and_then(|v| v.as_str()).unwrap_or("mermaid").to_string();
    let up_depth = params.get("up_depth").and_then(|v| v.as_u64()).unwrap_or(1) as u8;
    let down_depth = params.get("down_depth").and_then(|v| v.as_u64()).unwrap_or(1) as u8;

    match tokio::task::spawn_blocking(move || engine.diagram_with_format(uris, &format, up_depth, down_depth)).await {
        Ok(Ok(res)) => json!({"status": "ok", "diagram": res}),
        Ok(Err(e)) => json!({"error": e.to_string()}),
        Err(e) => json!({"error": format!("Task join error in diagram: {}", e)}),
    }
}

pub async fn handle_manage_file(params: &Value, engine: Arc<FileEngine>, shadow_root: Option<PathBuf>) -> Value {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("create")
        .to_string();
    let dest = params
        .get("destination")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    match tokio::task::spawn_blocking(move || engine.manage(&path, &action, dest.as_deref(), shadow_root.as_deref())).await
    {
        Ok(Ok(res)) => json!({"status": "ok", "message": res}),
        Ok(Err(e)) => json!({"error": e.to_string()}),
        Err(e) => json!({"error": format!("Task join error in manage_file: {}", e)}),
    }
}

pub async fn handle_lsp(params: &Value, engine: &LspEngine) -> Value {
    let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("syntax");
    match engine.diagnostics_with_mode(uri, mode).await {
        Ok(res) => json!({"status": "ok", "diagnostics": res}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_profile(params: &Value, engine: &ProfileEngine) -> Value {
    let roots: Vec<String> = params
        .get("roots")
        .and_then(|u| u.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    match engine.profile(None, roots, 2, 3, "ascii").await {
        Ok(res) => json!({"status": "ok", "profile": res}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_debug_dispatcher(params: &Value, engine: &DebugEngine) -> Value {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let result = match action {
        "backends" => Ok(engine.backends()),
        "execute" => {
            let lang = params
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let snippet = params.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            let target = params.get("target").and_then(|v| v.as_str());
            engine.debug(lang, snippet, target, &[]).await
        }
        "start" => {
            let lang = params
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let target = params.get("target").and_then(|v| v.as_str());
            engine.start_session(lang, target, &[]).await
        }
        "send" => {
            let id = params
                .get("debug_session_id")
                .or_else(|| params.get("session_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let snippet = params.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            engine.send_session(id, snippet).await
        }
        "recv" => {
            let id = params
                .get("debug_session_id")
                .or_else(|| params.get("session_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            engine.recv_session(id)
        }
        "stop" => {
            let id = params
                .get("debug_session_id")
                .or_else(|| params.get("session_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            engine.stop_session(id)
        }
        _ => Err(anyhow::anyhow!("Unknown debug action: {}", action)),
    };
    match result {
        Ok(res) => json!({"status": "ok", "result": res}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_review_cycle_dispatcher(params: &Value, engine: &ReviewCycleEngine) -> Value {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let result = match action {
        "begin" => engine.begin(params.get("label").and_then(|v| v.as_str())),
        "status" => engine.status(),
        "changes" => {
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            engine.changes(limit)
        }
        "review" => engine.review().await,
        "end" => engine.end(),
        _ => Err(anyhow::anyhow!("Unknown review_cycle action: {}", action)),
    };
    match result {
        Ok(res) => json!({"status": "ok", "result": res}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_doc(params: &Value, ctx: &EngineContext) -> Value {
    let tool = params
        .get("tool")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let engine = Arc::clone(&ctx.doce);
    let plugin_doc = ctx.tpe.get_doc(&tool).ok().flatten();
    if let Some(doc) = plugin_doc {
        return doc;
    }
    let group_doc = ctx.tge.get_doc(&tool).ok().flatten();
    if let Some(doc) = group_doc {
        return doc;
    }
    match tokio::task::spawn_blocking(move || engine.get_doc(&tool)).await {
        Ok(res) => res,
        Err(e) => json!({"error": format!("Task join error in doc: {}", e)}),
    }
}

pub async fn handle_tool_plugin(params: &Value, ctx: &EngineContext) -> Value {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("list");
    let result = match action {
        "list" => ctx
            .tpe
            .list()
            .map(|plugins| json!({"plugins": plugins})),
        "add" => {
            let archive_path = params.get("archive_path").and_then(|v| v.as_str()).unwrap_or("");
            if archive_path.is_empty() {
                Err(anyhow::anyhow!("Missing required field: archive_path"))
            } else {
                ctx.tpe
                    .add_archive(Path::new(archive_path))
                    .map(|installed| json!({"installed": installed}))
            }
        }
        "remove" => {
            let package_id = params.get("package_id").and_then(|v| v.as_str()).unwrap_or("");
            if package_id.is_empty() {
                Err(anyhow::anyhow!("Missing required field: package_id"))
            } else {
                ctx.tpe
                    .remove(package_id)
                    .map(|removed| json!({"removed": removed, "package_id": package_id}))
            }
        }
        _ => Err(anyhow::anyhow!("Unknown plugin_tool action: {}", action)),
    };
    let out = match result {
        Ok(result) => json!({"status": "ok", "result": result}),
        Err(err) => json!({"error": err.to_string()}),
    };
    audit_action(
        ctx,
        "plugin_tool",
        params.clone(),
        out.clone(),
        out.get("error").is_none(),
        out.get("error").and_then(|v| v.as_str()).map(str::to_string),
        params.get("participant_id").and_then(|v| v.as_str()).map(str::to_string),
        params.get("is_human").and_then(|v| v.as_bool()).map(|v| if v { "human" } else { "agent" }),
    );
    if out.get("error").is_none() {
        match action {
            "add" => {
                if let Some(installed) = out.get("result").and_then(|v| v.get("installed")) {
                    if let (Some(package_id), Some(version), Some(tool_name)) = (
                        installed.get("package_id").and_then(|v| v.as_str()),
                        installed.get("version").and_then(|v| v.as_str()),
                        installed.get("tool").and_then(|v| v.get("tool_name")).and_then(|v| v.as_str()),
                    ) {
                        emit_system_event(ctx, SystemEvent::ToolPluginInstalled {
                            package_id: package_id.to_string(),
                            version: version.to_string(),
                            tool_name: tool_name.to_string(),
                        });
                    }
                }
            }
            "remove" => {
                if let Some(package_id) = params.get("package_id").and_then(|v| v.as_str()) {
                    emit_system_event(ctx, SystemEvent::ToolPluginRemoved { package_id: package_id.to_string() });
                }
            }
            _ => {}
        }
    }
    out
}

pub async fn handle_language_plugin(params: &Value, ctx: &EngineContext) -> Value {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("list");
    let result = match action {
        "list" => ctx
            .lpe
            .list()
            .map(|plugins| json!({"plugins": plugins})),
        "add" => {
            let archive_path = params.get("archive_path").and_then(|v| v.as_str()).unwrap_or("");
            if archive_path.is_empty() {
                Err(anyhow::anyhow!("Missing required field: archive_path"))
            } else {
                ctx.lpe
                    .add_archive(Path::new(archive_path))
                    .map(|installed| json!({"installed": installed}))
            }
        }
        "remove" => {
            let package_id = params.get("package_id").and_then(|v| v.as_str()).unwrap_or("");
            if package_id.is_empty() {
                Err(anyhow::anyhow!("Missing required field: package_id"))
            } else {
                ctx.lpe
                    .remove(package_id)
                    .map(|removed| json!({"removed": removed, "package_id": package_id}))
            }
        }
        _ => Err(anyhow::anyhow!("Unknown plugin_language action: {}", action)),
    };
    let out = match result {
        Ok(result) => json!({"status": "ok", "result": result}),
        Err(err) => json!({"error": err.to_string()}),
    };
    audit_action(
        ctx,
        "plugin_language",
        params.clone(),
        out.clone(),
        out.get("error").is_none(),
        out.get("error").and_then(|v| v.as_str()).map(str::to_string),
        params.get("participant_id").and_then(|v| v.as_str()).map(str::to_string),
        params.get("is_human").and_then(|v| v.as_bool()).map(|v| if v { "human" } else { "agent" }),
    );
    if out.get("error").is_none() {
        match action {
            "add" => {
                if let Some(installed) = out.get("result").and_then(|v| v.get("installed")) {
                    if let (Some(package_id), Some(version), Some(language_id)) = (
                        installed.get("package_id").and_then(|v| v.as_str()),
                        installed.get("version").and_then(|v| v.as_str()),
                        installed.get("language").and_then(|v| v.get("language_id")).and_then(|v| v.as_str()),
                    ) {
                        emit_system_event(ctx, SystemEvent::LanguagePluginInstalled {
                            package_id: package_id.to_string(),
                            version: version.to_string(),
                            language_id: language_id.to_string(),
                        });
                    }
                }
            }
            "remove" => {
                if let Some(package_id) = params.get("package_id").and_then(|v| v.as_str()) {
                    emit_system_event(ctx, SystemEvent::LanguagePluginRemoved { package_id: package_id.to_string() });
                }
            }
            _ => {}
        }
    }
    out
}

pub async fn handle_tool_group(params: &Value, ctx: &EngineContext) -> Value {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("list");
    let result = match action {
        "list" => ctx.tge.list().map(|groups| json!({"groups": groups})),
        "status" => {
            let group_id = params.get("group_id").and_then(|v| v.as_str());
            ctx.tge.session_status(group_id).map(|sessions| json!({"sessions": sessions}))
        }
        "add_mcp" => {
            let group_id = params.get("group_id").and_then(|v| v.as_str()).unwrap_or("");
            let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let args: Vec<String> = params
                .get("args")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            let description = params
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let allow_tools: Vec<String> = params
                .get("allow_tools")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            let deny_tools: Vec<String> = params
                .get("deny_tools")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if group_id.is_empty() || command.is_empty() {
                Err(anyhow::anyhow!("add_mcp requires group_id and command"))
            } else {
                ctx.tge
                    .add_external_mcp_group(
                        group_id,
                        command,
                        &args,
                        description,
                        &allow_tools,
                        &deny_tools,
                    )
                    .map(|group| json!({"group": group}))
            }
        }
        "remove" => {
            let group_id = params.get("group_id").and_then(|v| v.as_str()).unwrap_or("");
            if group_id.is_empty() {
                Err(anyhow::anyhow!("Missing required field: group_id"))
            } else {
                ctx.tge
                    .remove(group_id)
                    .map(|removed| json!({"removed": removed, "group_id": group_id}))
            }
        }
        _ => Err(anyhow::anyhow!("Unknown tool_group action: {}", action)),
    };
    let out = match result {
        Ok(result) => json!({"status": "ok", "result": result}),
        Err(err) => json!({"error": err.to_string()}),
    };
    audit_action(
        ctx,
        "tool_group",
        params.clone(),
        out.clone(),
        out.get("error").is_none(),
        out.get("error").and_then(|v| v.as_str()).map(str::to_string),
        params.get("participant_id").and_then(|v| v.as_str()).map(str::to_string),
        params.get("is_human").and_then(|v| v.as_bool()).map(|v| if v { "human" } else { "agent" }),
    );
    if out.get("error").is_none() {
        match action {
            "add_mcp" => {
                if let Some(group) = out.get("result").and_then(|v| v.get("group")) {
                    if let Some(group_id) = group.get("group_id").and_then(|v| v.as_str()) {
                        let tool_count = group.get("tools").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                        emit_system_event(ctx, SystemEvent::ToolGroupRegistered {
                            group_id: group_id.to_string(),
                            source: "external_mcp".to_string(),
                            tool_count,
                        });
                    }
                }
            }
            "remove" => {
                if let Some(group_id) = params.get("group_id").and_then(|v| v.as_str()) {
                    emit_system_event(ctx, SystemEvent::ToolGroupRemoved { group_id: group_id.to_string() });
                }
            }
            _ => {}
        }
    }
    out
}

pub async fn handle_plugin_trust(params: &Value, ctx: &EngineContext) -> Value {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("list");
    let result = match action {
        "list" => crate::plugin_packages::load_trusted_keys(&ctx.workspace_root, &ctx.config.plugins)
            .map(|keys| json!({"keys": keys.keys})),
        "get" => {
            let key_id = params.get("key_id").and_then(|v| v.as_str()).unwrap_or("");
            if key_id.is_empty() {
                Err(anyhow::anyhow!("get requires key_id"))
            } else {
                crate::plugin_packages::load_trusted_keys(&ctx.workspace_root, &ctx.config.plugins)
                    .and_then(|keys| {
                        keys.keys
                            .into_iter()
                            .find(|key| key.key_id == key_id)
                            .map(|key| json!({"key": key}))
                            .ok_or_else(|| anyhow::anyhow!("trusted key not found: {}", key_id))
                    })
            }
        }
        "add" => {
            let key_id = params.get("key_id").and_then(|v| v.as_str()).unwrap_or("");
            let pubkey_hex = params.get("pubkey_hex").and_then(|v| v.as_str()).unwrap_or("");
            if key_id.is_empty() || pubkey_hex.is_empty() {
                Err(anyhow::anyhow!("add requires key_id and pubkey_hex"))
            } else {
                match crate::plugin_packages::load_trusted_keys(&ctx.workspace_root, &ctx.config.plugins) {
                    Ok(mut set) => {
                        let allowed_kinds: Vec<crate::PluginKind> = params
                            .get("allowed_kinds")
                            .and_then(|v| v.as_array())
                            .into_iter()
                            .flatten()
                            .filter_map(|v| match v.as_str() {
                                Some("tool") => Some(crate::PluginKind::Tool),
                                Some("language") => Some(crate::PluginKind::Language),
                                _ => None,
                            })
                            .collect();
                        set.keys.retain(|key| key.key_id != key_id);
                        match crate::plugin_packages::create_trusted_plugin_key(
                            key_id,
                            pubkey_hex,
                            params.get("label").and_then(|v| v.as_str()).map(str::to_string),
                            allowed_kinds,
                            true,
                        ) {
                            Ok(trusted) => {
                                set.keys.push(trusted);
                                crate::plugin_packages::store_trusted_keys(&ctx.workspace_root, &ctx.config.plugins, &set)
                                    .map(|_| json!({"key_id": key_id, "pubkey_hex": pubkey_hex}))
                            }
                            Err(err) => Err(err),
                        }
                    }
                    Err(err) => Err(err),
                }
            }
        }
        "remove" => {
            let key_id = params.get("key_id").and_then(|v| v.as_str()).unwrap_or("");
            if key_id.is_empty() {
                Err(anyhow::anyhow!("remove requires key_id"))
            } else {
                match crate::plugin_packages::load_trusted_keys(&ctx.workspace_root, &ctx.config.plugins) {
                    Ok(mut set) => {
                        let before = set.keys.len();
                        set.keys.retain(|key| key.key_id != key_id);
                        crate::plugin_packages::store_trusted_keys(&ctx.workspace_root, &ctx.config.plugins, &set)
                            .map(|_| json!({"removed": before != set.keys.len(), "key_id": key_id}))
                    }
                    Err(err) => Err(err),
                }
            }
        }
        "enable" | "disable" => {
            let key_id = params.get("key_id").and_then(|v| v.as_str()).unwrap_or("");
            if key_id.is_empty() {
                Err(anyhow::anyhow!("{} requires key_id", action))
            } else {
                match crate::plugin_packages::load_trusted_keys(&ctx.workspace_root, &ctx.config.plugins) {
                    Ok(mut set) => {
                        let enabled = action == "enable";
                        let mut found = false;
                        for key in &mut set.keys {
                            if key.key_id == key_id {
                                key.enabled = enabled;
                                found = true;
                            }
                        }
                        if !found {
                            Err(anyhow::anyhow!("trusted key not found: {}", key_id))
                        } else {
                            crate::plugin_packages::store_trusted_keys(&ctx.workspace_root, &ctx.config.plugins, &set)
                                .map(|_| json!({"key_id": key_id, "enabled": enabled}))
                        }
                    }
                    Err(err) => Err(err),
                }
            }
        }
        _ => Err(anyhow::anyhow!("Unknown plugin_trust action: {}", action)),
    };
    let out = match result {
        Ok(result) => json!({"status": "ok", "result": result}),
        Err(err) => json!({"error": err.to_string()}),
    };
    audit_action(
        ctx,
        "plugin_trust",
        params.clone(),
        out.clone(),
        out.get("error").is_none(),
        out.get("error").and_then(|v| v.as_str()).map(str::to_string),
        params.get("participant_id").and_then(|v| v.as_str()).map(str::to_string),
        params.get("is_human").and_then(|v| v.as_bool()).map(|v| if v { "human" } else { "agent" }),
    );
    if out.get("error").is_none() {
        match action {
            "add" => {
                if let Some(key_id) = params.get("key_id").and_then(|v| v.as_str()) {
                    emit_system_event(ctx, SystemEvent::PluginTrustedKeyAdded { key_id: key_id.to_string() });
                }
            }
            "remove" => {
                if let Some(key_id) = params.get("key_id").and_then(|v| v.as_str()) {
                    emit_system_event(ctx, SystemEvent::PluginTrustedKeyRemoved { key_id: key_id.to_string() });
                }
            }
            _ => {}
        }
    }
    out
}

pub async fn handle_benchmark(params: &Value, ctx: &EngineContext) -> Value {
    let operation = params
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if operation.is_empty() {
        return json!({"error": "Missing required field: operation"});
    }
    if operation == "benchmark" {
        return json!({"error": "Recursive benchmark operation is not allowed"});
    }

    let args = params.get("params").cloned().unwrap_or_else(|| json!({}));
    let iterations = params
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .clamp(1, 1000) as usize;
    let save_baseline = params
        .get("save_baseline")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut durations_ms = Vec::with_capacity(iterations);
    let mut last_result = json!(null);

    for _ in 0..iterations {
        let start = Instant::now();
        let res = execute_benchmark_target(&operation, &args, ctx).await;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        durations_ms.push(elapsed);
        last_result = res;
    }

    durations_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let total_ms: f64 = durations_ms.iter().sum();
    let avg_ms = total_ms / durations_ms.len() as f64;
    let min_ms = *durations_ms.first().unwrap_or(&0.0);
    let max_ms = *durations_ms.last().unwrap_or(&0.0);
    let p50_ms = percentile(&durations_ms, 0.50);
    let p95_ms = percentile(&durations_ms, 0.95);

    let report = json!({
        "status": "ok",
        "operation": operation,
        "iterations": iterations,
        "timing_ms": {
            "min": min_ms,
            "avg": avg_ms,
            "p50": p50_ms,
            "p95": p95_ms,
            "max": max_ms,
            "total": total_ms
        },
        "last_result": last_result
    });

    if save_baseline {
        let benchmark_dir = ctx.workspace_root.join(".curd").join("benchmarks");
        let _ = fs::create_dir_all(&benchmark_dir);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let file = benchmark_dir.join(format!(
            "{}_{}.json",
            report["operation"].as_str().unwrap_or("operation"),
            ts
        ));
        let _ = fs::write(
            &file,
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        );
    }

    report
}

fn percentile(sorted_values: &[f64], q: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let n = sorted_values.len();
    let rank = ((n - 1) as f64 * q).round() as usize;
    sorted_values[rank.min(n - 1)]
}

fn template_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".curd").join("templates")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChangeProposal {
    id: String,
    title: String,
    status: String,
    created_at_secs: u64,
    updated_at_secs: u64,
    summary: Option<String>,
    simulate: Option<Value>,
    crawl: Option<Value>,
    checkpoints: Option<Value>,
    review: Option<Value>,
    decision_reason: Option<String>,
    #[serde(default)]
    gated_snapshot_id: Option<String>,
    #[serde(default)]
    approved_snapshot_id: Option<String>,
}

fn proposal_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".curd").join("proposals")
}

// Removes non-alphanumeric, non-hyphen, and non-underscore characters.
// This prevents Directory Traversal vectors (e.g. `../../../`) from MCP payload injections hitting `.curd/*` state directories.
fn sanitize_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

fn proposal_path(root: &std::path::Path, id: &str) -> std::path::PathBuf {
    proposal_dir(root).join(format!("{}.json", sanitize_id(id)))
}

fn commits_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".curd").join("commits")
}

fn load_proposal(root: &std::path::Path, id: &str) -> Option<ChangeProposal> {
    fs::read_to_string(proposal_path(root, id))
        .ok()
        .and_then(|s| serde_json::from_str::<ChangeProposal>(&s).ok())
}

fn save_proposal(root: &std::path::Path, proposal: &ChangeProposal) -> Result<(), String> {
    fs::create_dir_all(proposal_dir(root)).map_err(|e| e.to_string())?;
    fs::write(
        proposal_path(root, &proposal.id),
        serde_json::to_string_pretty(proposal).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn artifact_ok(v: &Option<Value>) -> bool {
    v.as_ref()
        .and_then(|x| x.get("status"))
        .and_then(|s| s.as_str())
        == Some("ok")
}

fn compute_workspace_snapshot_id(root: &std::path::Path) -> Result<String, String> {
    let files = crate::scan_workspace(root).map_err(|e| e.to_string())?;
    let mut rows: Vec<(String, u64, u128)> = Vec::new();
    for p in files {
        let rel = p
            .strip_prefix(root)
            .unwrap_or(&p)
            .to_string_lossy()
            .to_string();
        let Ok(meta) = fs::metadata(&p) else {
            continue;
        };
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        rows.push((rel, size, mtime));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (path, size, mtime) in rows {
        path.hash(&mut hasher);
        size.hash(&mut hasher);
        mtime.hash(&mut hasher);
    }
    Ok(format!("{:x}", hasher.finish()))
}

fn can_approve_proposal(p: &ChangeProposal, current_snapshot: &str) -> Result<(), String> {
    if !artifact_ok(&p.simulate) {
        return Err("proposal approve requires simulate.status == ok".to_string());
    }
    if !artifact_ok(&p.crawl) {
        return Err("proposal approve requires crawl.status == ok".to_string());
    }
    let Some(gated) = p.gated_snapshot_id.as_deref() else {
        return Err("proposal approve requires gated_snapshot_id from run_gate".to_string());
    };
    if gated != current_snapshot {
        return Err(format!(
            "proposal approve blocked: snapshot drift detected (gated={}, current={})",
            gated, current_snapshot
        ));
    }
    Ok(())
}

async fn compute_workspace_snapshot_id_async(root: PathBuf) -> Result<String, String> {
    match tokio::task::spawn_blocking(move || compute_workspace_snapshot_id(&root)).await {
        Ok(v) => v,
        Err(e) => Err(format!("snapshot task join error: {}", e)),
    }
}

fn substitute_vars(value: &Value, vars: &serde_json::Map<String, Value>) -> Value {
    match value {
        Value::String(s) => {
            let mut out = s.clone();
            for (k, v) in vars {
                let token = format!("${{{}}}", k);
                let repl = if let Some(ss) = v.as_str() {
                    ss.to_string()
                } else {
                    v.to_string()
                };
                out = out.replace(&token, &repl);
            }
            Value::String(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| substitute_vars(v, vars)).collect()),
        Value::Object(obj) => {
            let mut out = serde_json::Map::new();
            for (k, v) in obj {
                out.insert(k.clone(), substitute_vars(v, vars));
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}

pub async fn handle_template(params: &Value, ctx: &EngineContext) -> Value {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("list");
    let dir = template_dir(&ctx.workspace_root);
    let _ = fs::create_dir_all(&dir);

    match action {
        "register" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let template = params.get("template").cloned().unwrap_or(Value::Null);
            if name.trim().is_empty() || template.is_null() {
                return json!({"error": "template register requires: name, template"});
            }
            let path = dir.join(format!("{}.json", sanitize_id(name)));
            match fs::write(
                path,
                serde_json::to_string_pretty(&template).unwrap_or_else(|_| "{}".to_string()),
            ) {
                Ok(_) => json!({"status": "ok", "action": "register", "name": name}),
                Err(e) => json!({"error": format!("Failed to register template: {}", e)}),
            }
        }
        "list" => {
            let mut names = Vec::new();
            if let Ok(rd) = fs::read_dir(&dir) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("json")
                        && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                    {
                        names.push(stem.to_string());
                    }
                }
            }
            names.sort();
            json!({"status": "ok", "action": "list", "templates": names, "count": names.len()})
        }
        "get" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.trim().is_empty() {
                return json!({"error": "template get requires: name"});
            }
            let path = dir.join(format!("{}.json", sanitize_id(name)));
            match fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            {
                Some(v) => json!({"status": "ok", "action": "get", "name": name, "template": v}),
                None => json!({"error": format!("Template not found: {}", name)}),
            }
        }
        "instantiate" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.trim().is_empty() {
                return json!({"error": "template instantiate requires: name"});
            }
            let vars = params
                .get("vars")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            let path = dir.join(format!("{}.json", sanitize_id(name)));
            let Some(template) = fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            else {
                return json!({"error": format!("Template not found: {}", name)});
            };
            let instantiated = substitute_vars(&template, &vars);
            json!({
                "status": "ok",
                "action": "instantiate",
                "name": name,
                "vars": vars,
                "instantiated": instantiated
            })
        }
        _ => json!({"error": "template action must be one of: register, list, get, instantiate"}),
    }
}

pub async fn handle_proposal(params: &Value, ctx: &EngineContext) -> Value {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("status");
    let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let dir = proposal_dir(&ctx.workspace_root);
    let _ = fs::create_dir_all(&dir);

    match action {
        "open" => {
            let proposal_id = if id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                id.to_string()
            };
            let now = now_secs();
            let title = params
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("untitled-change")
                .to_string();
            let proposal = ChangeProposal {
                id: proposal_id.clone(),
                title,
                status: "open".to_string(),
                created_at_secs: now,
                updated_at_secs: now,
                summary: params
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                simulate: params.get("simulate").cloned(),
                crawl: params.get("crawl").cloned(),
                checkpoints: params.get("checkpoints").cloned(),
                review: params.get("review").cloned(),
                decision_reason: None,
                gated_snapshot_id: None,
                approved_snapshot_id: None,
            };
            match save_proposal(&ctx.workspace_root, &proposal) {
                Ok(_) => json!({"status":"ok","action":"open","proposal":proposal}),
                Err(e) => json!({"error": format!("Failed to save proposal: {}", e)}),
            }
        }
        "status" => {
            if !id.is_empty() {
                match load_proposal(&ctx.workspace_root, id) {
                    Some(p) => json!({"status":"ok","action":"status","proposal":p}),
                    None => json!({"error": format!("Proposal not found: {}", id)}),
                }
            } else {
                let mut ids = Vec::new();
                if let Ok(rd) = fs::read_dir(&dir) {
                    for entry in rd.flatten() {
                        let p = entry.path();
                        if p.extension().and_then(|e| e.to_str()) == Some("json")
                            && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                        {
                            ids.push(stem.to_string());
                        }
                    }
                }
                ids.sort();
                json!({"status":"ok","action":"status","proposals":ids,"count":ids.len()})
            }
        }
        "approve" | "reject" => {
            if id.is_empty() {
                return json!({"error": format!("proposal {} requires: id", action)});
            }
            let Some(mut proposal) = load_proposal(&ctx.workspace_root, id) else {
                return json!({"error": format!("Proposal not found: {}", id)});
            };
            if action == "approve" {
                let current_snapshot =
                    match compute_workspace_snapshot_id_async(ctx.workspace_root.clone()).await {
                        Ok(s) => s,
                        Err(e) => {
                            return json!({"error": format!("Failed to compute snapshot: {}", e)});
                        }
                    };
                if let Err(e) = can_approve_proposal(&proposal, &current_snapshot) {
                    return json!({"error": e});
                }
                proposal.approved_snapshot_id = Some(current_snapshot);
            }
            proposal.status = if action == "approve" {
                "approved".to_string()
            } else {
                "rejected".to_string()
            };
            proposal.updated_at_secs = now_secs();
            proposal.decision_reason = params
                .get("reason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(v) = params.get("review") {
                proposal.review = Some(v.clone());
            }
            match save_proposal(&ctx.workspace_root, &proposal) {
                Ok(_) => json!({"status":"ok","action":action,"proposal":proposal}),
                Err(e) => json!({"error": format!("Failed to update proposal: {}", e)}),
            }
        }
        "run_gate" => {
            if id.is_empty() {
                return json!({"error":"proposal run_gate requires: id"});
            }
            let Some(mut proposal) = load_proposal(&ctx.workspace_root, id) else {
                return json!({"error": format!("Proposal not found: {}", id)});
            };
            let snapshot_id =
                match compute_workspace_snapshot_id_async(ctx.workspace_root.clone()).await {
                    Ok(s) => s,
                    Err(e) => return json!({"error": format!("Failed to compute snapshot: {}", e)}),
                };
            let sim_args = params
                .get("simulate_args")
                .cloned()
                .or_else(|| {
                    proposal
                        .simulate
                        .as_ref()
                        .and_then(|v| v.get("input").cloned())
                })
                .unwrap_or_else(|| json!({"mode":"execute_dsl","nodes":[]}));
            let crawl_args = params
                .get("crawl_args")
                .cloned()
                .or_else(|| {
                    proposal
                        .crawl
                        .as_ref()
                        .and_then(|v| v.get("input").cloned())
                })
                .unwrap_or_else(|| json!({}));
            let roots_ok = crawl_args
                .get("roots")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            if !roots_ok {
                return json!({
                    "error":"proposal run_gate requires non-empty crawl_args.roots",
                    "details":{"expected":"crawl_args.roots: array<string> (non-empty)"}
                });
            }

            let sim = handle_simulate(&sim_args).await;
            let crawl = handle_crawl(&crawl_args, ctx).await;
            proposal.simulate = Some(
                json!({"input": sim_args, "status": sim.get("status").cloned().unwrap_or(json!(null)), "result": sim}),
            );
            proposal.crawl = Some(
                json!({"input": crawl_args, "status": crawl.get("status").cloned().unwrap_or(json!(null)), "result": crawl}),
            );
            proposal.updated_at_secs = now_secs();
            proposal.gated_snapshot_id = Some(snapshot_id.clone());
            proposal.status = if artifact_ok(&proposal.simulate) && artifact_ok(&proposal.crawl) {
                "gated".to_string()
            } else {
                "open".to_string()
            };

            let gate = json!({
                "simulate_ok": artifact_ok(&proposal.simulate),
                "crawl_ok": artifact_ok(&proposal.crawl),
                "snapshot_id": snapshot_id,
                "ready_for_approval": can_approve_proposal(
                    &proposal,
                    proposal.gated_snapshot_id.as_deref().unwrap_or("")
                ).is_ok()
            });
            match save_proposal(&ctx.workspace_root, &proposal) {
                Ok(_) => json!({"status":"ok","action":"run_gate","proposal":proposal,"gate":gate}),
                Err(e) => json!({"error": format!("Failed to update proposal: {}", e)}),
            }
        }
        _ => {
            json!({"error":"proposal action must be one of: open, status, run_gate, approve, reject"})
        }
    }
}

pub async fn handle_stamina(params: &Value, ctx: &EngineContext) -> Value {
    let connection_token = params
        .get("connection_token")
        .or_else(|| params.get("session_token"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if connection_token.is_empty() {
        return json!({"error": "stamina requires: connection_token"});
    }

    let connections = ctx.connections.lock().await;
    if let Some(entry) = connections.get(connection_token) {
        let now = now_secs();
        let elapsed = now.saturating_sub(entry.budget.started_at_secs);
        
        json!({
            "status": "ok",
            "budget": {
                "tokens_consumed": entry.budget.tokens_consumed,
                "max_tokens": ctx.config.budget.max_tokens,
                "hazardous_calls_made": entry.budget.hazardous_calls_made,
                "max_hazardous_calls": ctx.config.budget.max_hazardous_calls,
                "uptime_secs": elapsed,
                "max_session_secs": ctx.config.budget.max_session_secs
            }
        })
    } else {
        json!({"error": "Invalid connection_token"})
    }
}

pub async fn handle_checkpoint(params: &Value, ctx: &EngineContext) -> Value {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("list");
    let plan_id = params.get("plan_id").and_then(|v| v.as_str()).unwrap_or("");
    if plan_id.is_empty() {
        return json!({"error": "checkpoint requires: plan_id"});
    }
    let base = ctx
        .workspace_root
        .join(".curd")
        .join("checkpoints")
        .join(sanitize_id(plan_id));

    match action {
        "list" => {
            let mut files = Vec::new();
            if let Ok(rd) = fs::read_dir(&base) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("json")
                        && let Some(name) = p.file_name().and_then(|n| n.to_str())
                    {
                        files.push(name.to_string());
                    }
                }
            }
            files.sort();
            json!({"status": "ok", "action": "list", "plan_id": plan_id, "checkpoints": files, "count": files.len()})
        }
        "get" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.trim().is_empty() {
                return json!({"error": "checkpoint get requires: name"});
            }
            let path = base.join(format!("{}.json", sanitize_id(name)));
            match fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            {
                Some(v) => {
                    json!({"status": "ok", "action": "get", "plan_id": plan_id, "checkpoint": v})
                }
                None => json!({"error": format!("Checkpoint not found: {}/{}", plan_id, name)}),
            }
        }
        _ => json!({"error": "checkpoint action must be one of: list, get"}),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DelegationNodeState {
    status: String,
    claimed_by: Option<String>,
    completed_by: Option<String>,
    #[serde(default)]
    claimed_at_secs: Option<u64>,
    #[serde(default)]
    heartbeat_at_secs: Option<u64>,
    #[serde(default)]
    requeue_count: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DelegationBoard {
    plan_id: String,
    nodes: std::collections::HashMap<String, DelegationNodeState>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct FrontierQueue {
    plan_id: String,
    queue: Vec<String>,
    visited: std::collections::HashSet<String>,
}

fn delegation_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".curd").join("delegation")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn delegation_path(root: &std::path::Path, plan_id: &str) -> std::path::PathBuf {
    delegation_dir(root).join(format!("{}.json", sanitize_id(plan_id)))
}

fn load_delegation_board(root: &std::path::Path, plan_id: &str) -> Option<DelegationBoard> {
    fs::read_to_string(delegation_path(root, plan_id))
        .ok()
        .and_then(|s| serde_json::from_str::<DelegationBoard>(&s).ok())
}

fn save_delegation_board(root: &std::path::Path, board: &DelegationBoard) -> Result<(), String> {
    fs::create_dir_all(delegation_dir(root)).map_err(|e| e.to_string())?;
    fs::write(
        delegation_path(root, &board.plan_id),
        serde_json::to_string_pretty(board).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub async fn handle_delegate(params: &Value, ctx: &EngineContext) -> Value {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("status");
    let plan_id = params.get("plan_id").and_then(|v| v.as_str()).unwrap_or("");
    if plan_id.is_empty() {
        return json!({"error": "delegate requires: plan_id"});
    }

    match action {
        "create" => {
            let mut nodes = std::collections::HashMap::new();
            if let Some(arr) = params.get("nodes").and_then(|v| v.as_array()) {
                for n in arr {
                    if let Some(id) = n.as_str() {
                        nodes.insert(
                            id.to_string(),
                            DelegationNodeState {
                                status: "pending".to_string(),
                                claimed_by: None,
                                completed_by: None,
                                claimed_at_secs: None,
                                heartbeat_at_secs: None,
                                requeue_count: 0,
                            },
                        );
                    }
                }
            }
            let board = DelegationBoard {
                plan_id: plan_id.to_string(),
                nodes,
            };
            match save_delegation_board(&ctx.workspace_root, &board) {
                Ok(_) => {
                    json!({"status":"ok","action":"create","plan_id":plan_id,"node_count":board.nodes.len()})
                }
                Err(e) => json!({"error": format!("Failed to save delegation board: {}", e)}),
            }
        }
        "claim" => {
            let node_id = params.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
            let worker = params.get("worker").and_then(|v| v.as_str()).unwrap_or("");
            if node_id.is_empty() || worker.is_empty() {
                return json!({"error":"delegate claim requires: node_id, worker"});
            }
            let Some(mut board) = load_delegation_board(&ctx.workspace_root, plan_id) else {
                return json!({"error": format!("Delegation board not found for plan_id {}", plan_id)});
            };
            let Some(node) = board.nodes.get_mut(node_id) else {
                return json!({"error": format!("Node not found in board: {}", node_id)});
            };
            if node.status == "completed" {
                return json!({"error": format!("Node already completed: {}", node_id)});
            }
            node.status = "claimed".to_string();
            node.claimed_by = Some(worker.to_string());
            node.claimed_at_secs = Some(now_secs());
            node.heartbeat_at_secs = Some(now_secs());
            if let Err(e) = save_delegation_board(&ctx.workspace_root, &board) {
                return json!({"error": format!("Failed to save delegation board: {}", e)});
            }
            json!({"status":"ok","action":"claim","plan_id":plan_id,"node_id":node_id,"worker":worker})
        }
        "heartbeat" => {
            let node_id = params.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
            let worker = params.get("worker").and_then(|v| v.as_str()).unwrap_or("");
            if node_id.is_empty() || worker.is_empty() {
                return json!({"error":"delegate heartbeat requires: node_id, worker"});
            }
            let Some(mut board) = load_delegation_board(&ctx.workspace_root, plan_id) else {
                return json!({"error": format!("Delegation board not found for plan_id {}", plan_id)});
            };
            let Some(node) = board.nodes.get_mut(node_id) else {
                return json!({"error": format!("Node not found in board: {}", node_id)});
            };
            if node.status != "claimed" || node.claimed_by.as_deref() != Some(worker) {
                return json!({"error": format!("Node {} is not claimed by worker {}", node_id, worker)});
            }
            node.heartbeat_at_secs = Some(now_secs());
            if let Err(e) = save_delegation_board(&ctx.workspace_root, &board) {
                return json!({"error": format!("Failed to save delegation board: {}", e)});
            }
            json!({"status":"ok","action":"heartbeat","plan_id":plan_id,"node_id":node_id,"worker":worker})
        }
        "complete" => {
            let node_id = params.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
            let worker = params.get("worker").and_then(|v| v.as_str()).unwrap_or("");
            if node_id.is_empty() || worker.is_empty() {
                return json!({"error":"delegate complete requires: node_id, worker"});
            }
            let Some(mut board) = load_delegation_board(&ctx.workspace_root, plan_id) else {
                return json!({"error": format!("Delegation board not found for plan_id {}", plan_id)});
            };
            let Some(node) = board.nodes.get_mut(node_id) else {
                return json!({"error": format!("Node not found in board: {}", node_id)});
            };
            if node.status == "claimed" && node.claimed_by.as_deref() != Some(worker) {
                return json!({"error": format!("Node {} is claimed by another worker", node_id)});
            }
            node.status = "completed".to_string();
            node.completed_by = Some(worker.to_string());
            if node.claimed_by.is_none() {
                node.claimed_by = Some(worker.to_string());
                node.claimed_at_secs = Some(now_secs());
            }
            node.heartbeat_at_secs = Some(now_secs());
            if let Err(e) = save_delegation_board(&ctx.workspace_root, &board) {
                return json!({"error": format!("Failed to save delegation board: {}", e)});
            }
            json!({"status":"ok","action":"complete","plan_id":plan_id,"node_id":node_id,"worker":worker})
        }
        "auto_assign" => {
            let worker = params.get("worker").and_then(|v| v.as_str()).unwrap_or("");
            let max_claims = params
                .get("max_claims")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .clamp(1, 100) as usize;
            if worker.is_empty() {
                return json!({"error":"delegate auto_assign requires: worker"});
            }
            let Some(mut board) = load_delegation_board(&ctx.workspace_root, plan_id) else {
                return json!({"error": format!("Delegation board not found for plan_id {}", plan_id)});
            };
            let mut frontier = load_frontier(&ctx.workspace_root, plan_id);
            let mut claimed = Vec::new();
            let now = now_secs();

            let mut idx = 0usize;
            while idx < frontier.queue.len() && claimed.len() < max_claims {
                let node_id = frontier.queue[idx].clone();
                if let Some(node) = board.nodes.get_mut(&node_id)
                    && node.status == "pending"
                {
                    node.status = "claimed".to_string();
                    node.claimed_by = Some(worker.to_string());
                    node.claimed_at_secs = Some(now);
                    node.heartbeat_at_secs = Some(now);
                    claimed.push(node_id.clone());
                    frontier.visited.insert(node_id.clone());
                    frontier.queue.remove(idx);
                    continue;
                }
                idx += 1;
            }

            if let Err(e) = save_delegation_board(&ctx.workspace_root, &board) {
                return json!({"error": format!("Failed to save delegation board: {}", e)});
            }
            if let Err(e) = save_frontier(&ctx.workspace_root, &frontier) {
                return json!({"error": format!("Failed to save frontier: {}", e)});
            }

            json!({
                "status":"ok",
                "action":"auto_assign",
                "plan_id":plan_id,
                "worker":worker,
                "max_claims":max_claims,
                "claimed":claimed,
                "claimed_count":claimed.len(),
                "frontier_queue_size":frontier.queue.len()
            })
        }
        "status" => match load_delegation_board(&ctx.workspace_root, plan_id) {
            Some(mut board) => {
                let stale_timeout_secs = params
                    .get("stale_timeout_secs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(300);
                let mut requeued = 0usize;
                let now = now_secs();
                for node in board.nodes.values_mut() {
                    if node.status == "claimed" {
                        let hb = node.heartbeat_at_secs.or(node.claimed_at_secs).unwrap_or(0);
                        if now.saturating_sub(hb) > stale_timeout_secs {
                            node.status = "pending".to_string();
                            node.claimed_by = None;
                            node.claimed_at_secs = None;
                            node.heartbeat_at_secs = None;
                            node.requeue_count = node.requeue_count.saturating_add(1);
                            requeued += 1;
                        }
                    }
                }
                if requeued > 0 {
                    let _ = save_delegation_board(&ctx.workspace_root, &board);
                }
                let mut pending = 0usize;
                let mut claimed = 0usize;
                let mut completed = 0usize;
                for n in board.nodes.values() {
                    match n.status.as_str() {
                        "pending" => pending += 1,
                        "claimed" => claimed += 1,
                        "completed" => completed += 1,
                        _ => {}
                    }
                }
                json!({
                    "status":"ok",
                    "action":"status",
                    "plan_id":plan_id,
                    "stale_timeout_secs": stale_timeout_secs,
                    "requeued_stale_claims": requeued,
                    "summary":{"pending":pending,"claimed":claimed,"completed":completed,"total":board.nodes.len()},
                    "board":board
                })
            }
            None => json!({"error": format!("Delegation board not found for plan_id {}", plan_id)}),
        },
        _ => {
            json!({"error":"delegate action must be one of: create, claim, heartbeat, complete, auto_assign, status"})
        }
    }
}

fn frontier_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".curd").join("frontier")
}

fn frontier_path(root: &std::path::Path, plan_id: &str) -> std::path::PathBuf {
    frontier_dir(root).join(format!("{}.json", sanitize_id(plan_id)))
}

fn load_frontier(root: &std::path::Path, plan_id: &str) -> FrontierQueue {
    fs::read_to_string(frontier_path(root, plan_id))
        .ok()
        .and_then(|s| serde_json::from_str::<FrontierQueue>(&s).ok())
        .unwrap_or_else(|| FrontierQueue {
            plan_id: plan_id.to_string(),
            ..FrontierQueue::default()
        })
}

fn save_frontier(root: &std::path::Path, fq: &FrontierQueue) -> Result<(), String> {
    fs::create_dir_all(frontier_dir(root)).map_err(|e| e.to_string())?;
    fs::write(
        frontier_path(root, &fq.plan_id),
        serde_json::to_string_pretty(fq).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub async fn handle_frontier(params: &Value, ctx: &EngineContext) -> Value {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("status");
    let plan_id = params.get("plan_id").and_then(|v| v.as_str()).unwrap_or("");
    if plan_id.is_empty() {
        return json!({"error":"frontier requires: plan_id"});
    }
    let mut fq = load_frontier(&ctx.workspace_root, plan_id);

    match action {
        "seed" => {
            let mut added = 0usize;
            if let Some(uris) = params.get("uris").and_then(|v| v.as_array()) {
                for uri in uris.iter().filter_map(|v| v.as_str()) {
                    if !fq.visited.contains(uri) && !fq.queue.iter().any(|q| q == uri) {
                        fq.queue.push(uri.to_string());
                        added += 1;
                    }
                }
            }
            if let Err(e) = save_frontier(&ctx.workspace_root, &fq) {
                return json!({"error": format!("Failed to save frontier: {}", e)});
            }
            json!({"status":"ok","action":"seed","plan_id":plan_id,"added":added,"queue_size":fq.queue.len()})
        }
        "pop" => {
            let next = if fq.queue.is_empty() {
                None
            } else {
                Some(fq.queue.remove(0))
            };
            if let Some(uri) = &next {
                fq.visited.insert(uri.clone());
            }
            if let Err(e) = save_frontier(&ctx.workspace_root, &fq) {
                return json!({"error": format!("Failed to save frontier: {}", e)});
            }
            json!({"status":"ok","action":"pop","plan_id":plan_id,"next":next,"queue_size":fq.queue.len(),"visited_count":fq.visited.len()})
        }
        "status" => json!({
            "status":"ok",
            "action":"status",
            "plan_id":plan_id,
            "queue_size":fq.queue.len(),
            "visited_count":fq.visited.len(),
            "queue_preview":fq.queue.iter().take(25).cloned().collect::<Vec<_>>()
        }),
        "reset" => {
            fq.queue.clear();
            fq.visited.clear();
            if let Err(e) = save_frontier(&ctx.workspace_root, &fq) {
                return json!({"error": format!("Failed to save frontier: {}", e)});
            }
            json!({"status":"ok","action":"reset","plan_id":plan_id})
        }
        _ => json!({"error":"frontier action must be one of: seed, pop, status, reset"}),
    }
}

pub async fn handle_crawl(params: &Value, ctx: &EngineContext) -> Value {
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("crawl_heal");
    let roots: Vec<String> = params
        .get("roots")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if roots.is_empty() {
        return json!({"error":"crawl requires non-empty roots"});
    }

    let depth = params.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as u8;
    let enqueue = params
        .get("enqueue")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let top_k = params.get("top_k").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let include_contract_gists = params
        .get("include_contract_gists")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let contract_top_k = params
        .get("contract_top_k")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .clamp(1, 50) as usize;
    let plan_id_for_enqueue = params.get("plan_id").and_then(|v| v.as_str()).unwrap_or("");
    let ge = Arc::clone(&ctx.ge);
    let roots_clone = roots.clone();
    let graph =
        match tokio::task::spawn_blocking(move || ge.graph(roots_clone, "both", depth)).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return json!({"error": e.to_string()}),
            Err(e) => return json!({"error": format!("Task join error in crawl graph: {}", e)}),
        };

    let frontier_candidates = graph
        .get("results")
        .and_then(|v| v.as_array())
        .map(|entries| {
            let mut out = Vec::new();
            for e in entries {
                if let Some(fid) = e.get("function_id").and_then(|v| v.as_str()) {
                    out.push(fid.to_string());
                }
                for key in ["callers", "callees"] {
                    if let Some(arr) = e.get(key).and_then(|v| v.as_array()) {
                        for s in arr.iter().filter_map(|v| v.as_str()) {
                            out.push(s.to_string());
                        }
                    }
                }
            }
            out.sort();
            out.dedup();
            out
        })
        .unwrap_or_default();

    let mut ranked_candidates = Vec::new();
    if let Some(entries) = graph.get("results").and_then(|v| v.as_array()) {
        for e in entries {
            let Some(fid) = e.get("function_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let caller_count = e
                .get("callers")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let callee_count = e
                .get("callees")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let count_levels = |side: &str| -> usize {
                e.get(side)
                    .and_then(|v| v.as_array())
                    .map(|levels| {
                        levels
                            .iter()
                            .map(|lvl| lvl.as_array().map(|a| a.len()).unwrap_or(0))
                            .sum()
                    })
                    .unwrap_or(0)
            };
            let up_reach = count_levels("up_levels");
            let down_reach = count_levels("down_levels");
            let score = match mode {
                "crawl_heal" => {
                    (caller_count * 3 + callee_count * 2 + up_reach + down_reach) as i64
                }
                "crawl_audit" => {
                    let s = fid.to_lowercase();
                    let sink_bonus = if s.contains("shell")
                        || s.contains("exec")
                        || s.contains("command")
                        || s.contains("delete")
                        || s.contains("write")
                    {
                        25
                    } else {
                        0
                    };
                    (caller_count + callee_count + up_reach + down_reach) as i64 + sink_bonus
                }
                "crawl_prune" => {
                    let connectivity = caller_count + callee_count + up_reach + down_reach;
                    (10_000usize.saturating_sub(connectivity)) as i64
                }
                "crawl_mutate" => {
                    // Similar to heal but prioritizes nodes that are highly connected to test fault propagation
                    (caller_count * 2 + callee_count * 3 + up_reach * 2 + down_reach) as i64
                }
                _ => {
                    return json!({"error":"crawl mode must be one of: crawl_heal, crawl_audit, crawl_prune, crawl_mutate"});
                }
            };
            ranked_candidates.push(json!({
                "uri": fid,
                "score": score,
                "metrics": {
                    "caller_count": caller_count,
                    "callee_count": callee_count,
                    "up_reach": up_reach,
                    "down_reach": down_reach
                }
            }));
        }
    }

    ranked_candidates.sort_by(|a, b| {
        let sa = a.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let sb = b.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        sb.cmp(&sa).then_with(|| {
            let ua = a.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            let ub = b.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            ua.cmp(ub)
        })
    });

    if include_contract_gists {
        let limit = ranked_candidates.len().min(contract_top_k);
        for cand in ranked_candidates.iter_mut().take(limit) {
            let Some(uri) = cand.get("uri").and_then(|v| v.as_str()) else {
                continue;
            };
            let contract = handle_contract(&json!({"uri": uri}), ctx).await;
            if contract.get("status") == Some(&json!("ok")) {
                let gist = contract
                    .get("contract")
                    .and_then(|v| v.get("gist_1line"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                cand["contract_gist_1line"] = gist;
            }
        }
    }

    let mut enqueued = 0usize;
    if enqueue {
        if plan_id_for_enqueue.is_empty() {
            return json!({"error":"crawl enqueue requires: plan_id"});
        }
        let mut fq = load_frontier(&ctx.workspace_root, plan_id_for_enqueue);
        for cand in ranked_candidates.iter().take(top_k) {
            if let Some(uri) = cand.get("uri").and_then(|v| v.as_str())
                && !fq.visited.contains(uri)
                && !fq.queue.iter().any(|q| q == uri)
            {
                fq.queue.push(uri.to_string());
                enqueued += 1;
            }
        }
        if let Err(e) = save_frontier(&ctx.workspace_root, &fq) {
            return json!({"error": format!("Failed to enqueue crawl candidates: {}", e)});
        }
    }

    let recommendations = match mode {
        "crawl_heal" => vec![
            "Run lsp syntax+semantic on top-ranked frontier candidates".to_string(),
            "Prioritize highest blast-radius nodes for breakage triage".to_string(),
        ],
        "crawl_audit" => vec![
            "Inspect highest-risk sink-like candidates first".to_string(),
            "Trace upstream/downstream neighborhoods for exploitability".to_string(),
        ],
        "crawl_prune" => vec![
            "Review low-connectivity candidates for dead/ghost clusters".to_string(),
            "Confirm no required entrypoint paths reach top-ranked nodes".to_string(),
        ],
        "crawl_mutate" => vec![
            "Apply mutation batch to shadow store on top-ranked candidates".to_string(),
            "Run build/lint via `curd build` in shadow and map failure paths".to_string(),
        ],
        _ => {
            return json!({"error":"crawl mode must be one of: crawl_heal, crawl_audit, crawl_prune, crawl_mutate"});
        }
    };

    json!({
        "status":"ok",
        "mode":mode,
        "deterministic_dry_run": true,
        "roots": roots,
        "depth": depth,
        "frontier_candidates": frontier_candidates,
        "candidate_count": frontier_candidates.len(),
        "ranked_candidates": ranked_candidates,
        "enqueue": {
            "enabled": enqueue,
            "plan_id": if enqueue { json!(plan_id_for_enqueue) } else { json!(null) },
            "top_k": top_k,
            "enqueued": enqueued
        },
        "contract_gists": {
            "enabled": include_contract_gists,
            "top_k": contract_top_k
        },
        "recommendations": recommendations,
        "graph": graph
    })
}

pub async fn handle_simulate(params: &Value) -> Value {
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("execute_plan");

    let mut findings = Vec::new();
    let mut warnings = Vec::new();
    let simulated_nodes;

    match mode {
        "execute_plan" => {
            let Some(plan_val) = params.get("plan") else {
                return json!({"error": "simulate(mode=execute_plan) requires field: plan"});
            };
            let plan: Plan = match serde_json::from_value(plan_val.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return json!({"status": "invalid", "error": format!("Invalid plan schema: {}", e)});
                }
            };
            simulated_nodes = plan.nodes.len();

            let node_ids: std::collections::HashSet<_> = plan.nodes.iter().map(|n| n.id).collect();
            for node in &plan.nodes {
                for dep in &node.dependencies {
                    if let crate::plan::IdOrTag::Id(id) = dep
                        && !node_ids.contains(id)
                    {
                        findings.push(json!({
                            "severity": "error",
                            "code": "missing_dependency",
                            "message": format!("Node {} depends on missing node id {}", node.id, id)
                        }));
                    }
                }

                if let crate::plan::ToolOperation::McpCall { tool, args } = &node.op {
                    if !is_known_tool_name(tool) {
                        findings.push(json!({
                            "severity": "error",
                            "code": "unknown_tool",
                            "message": format!("Unknown tool in plan node {}: {}", node.id, tool)
                        }));
                    } else if risky_tool(tool) {
                        warnings.push(json!({
                            "severity": "warning",
                            "code": "risky_tool",
                            "message": format!("Plan node {} uses mutating/external tool: {}", node.id, tool)
                        }));
                    }
                    validate_tool_args_for_simulate(
                        tool,
                        args,
                        &params
                            .get("workspace_root")
                            .and_then(|v| v.as_str())
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| {
                                std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                            }),
                        &mut findings,
                        &mut warnings,
                        &format!("plan node {}", node.id),
                    );
                }
            }
        }
        "execute_dsl" => {
            let Some(nodes_val) = params.get("nodes") else {
                return json!({"error": "simulate(mode=execute_dsl) requires field: nodes"});
            };
            let nodes: Vec<DslNode> = match serde_json::from_value(nodes_val.clone()) {
                Ok(n) => n,
                Err(e) => {
                    return json!({"status": "invalid", "error": format!("Invalid dsl schema: {}", e)});
                }
            };
            simulated_nodes = nodes.len();

            fn scan_dsl(
                nodes: &[DslNode],
                root: &std::path::Path,
                findings: &mut Vec<Value>,
                warnings: &mut Vec<Value>,
            ) {
                for node in nodes {
                    match node {
                        DslNode::Call { tool, args } => {
                            if !is_known_tool_name(tool) {
                                findings.push(json!({
                                    "severity": "error",
                                    "code": "unknown_tool",
                                    "message": format!("Unknown tool in dsl call: {}", tool)
                                }));
                            } else if risky_tool(tool) {
                                warnings.push(json!({
                                    "severity": "warning",
                                    "code": "risky_tool",
                                    "message": format!("DSL call uses mutating/external tool: {}", tool)
                                }));
                            }
                            validate_tool_args_for_simulate(
                                tool,
                                args,
                                root,
                                findings,
                                warnings,
                                &format!("dsl call {}", tool),
                            );
                        }
                        DslNode::Atomic { nodes } => scan_dsl(nodes, root, findings, warnings),
                        DslNode::Abort { .. } | DslNode::Assign { .. } => {}
                    }
                }
            }

            let root = params
                .get("workspace_root")
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                });
            scan_dsl(&nodes, &root, &mut findings, &mut warnings);
        }
        _ => {
            return json!({"error": "simulate mode must be one of: execute_plan, execute_dsl"});
        }
    }

    let status = if findings.is_empty() { "ok" } else { "invalid" };
    json!({
        "status": status,
        "mode": mode,
        "simulated_nodes": simulated_nodes,
        "summary": {
            "errors": findings.len(),
            "warnings": warnings.len()
        },
        "findings": findings,
        "warnings": warnings,
        "mutated_workspace": false
    })
}

pub async fn handle_doctor(params: &Value, engine: &DoctorEngine) -> Value {
    let strict = params.get("strict").and_then(|v| v.as_bool()).unwrap_or(false);
    let profile = params.get("profile").and_then(|v| v.as_str()).and_then(|s| DoctorProfile::from_str(s).ok());
    let thresholds = if let Some(t) = params.get("thresholds") {
        serde_json::from_value(t.clone()).unwrap_or_default()
    } else {
        DoctorThresholds::default()
    };
    let index_cfg = if let Some(i) = params.get("index_config") {
        serde_json::from_value(i.clone()).unwrap_or_default()
    } else {
        DoctorIndexConfig::default()
    };

    match engine.run(strict, thresholds, profile, index_cfg) {
        Ok(report) => json!(report),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_research(params: &Value) -> Value {
    let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
    if query.is_empty() {
        return json!({"error": "research requires: query"});
    }

    // Barebones mock implementation
    json!({
        "status": "ok",
        "provenance": "external",
        "results": [
            {
                "title": format!("Mock research result for: {}", query),
                "url": "https://example.com/mock-research",
                "snippet": "This is a mock response from the external research delegation."
            }
        ]
    })
}

pub async fn handle_mutate(params: &Value, mu: Arc<MutationEngine>, shadow_root: Option<PathBuf>) -> Value {
    let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    if uri.is_empty() {
        return json!({"error": "mutate requires: uri"});
    }

    match mu.mutate_symbol(uri, shadow_root.as_deref()) {
        Ok(val) => val,
        Err(e) => json!({"error": format!("Mutation failed: {}", e)}),
    }
}

#[cfg(test)]
mod tests {
    use super::handle_plugin_trust;
    use crate::context::EngineContext;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn plugin_trust_add_list_and_remove_round_trips() {
        let dir = tempdir().expect("tempdir");
        let ctx = EngineContext::new(dir.path().to_str().expect("utf8 path"));
        let pubkey_hex = "11".repeat(32);

        let add = handle_plugin_trust(
            &json!({
                "action": "add",
                "key_id": "demo",
                "pubkey_hex": pubkey_hex,
                "label": "demo key",
                "allowed_kinds": ["tool"]
            }),
            &ctx,
        )
        .await;
        assert_eq!(add["status"], "ok");

        let list = handle_plugin_trust(&json!({"action": "list"}), &ctx).await;
        let keys = list["result"]["keys"].as_array().expect("keys array");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0]["key_id"], "demo");
        assert_eq!(keys[0]["allowed_kinds"][0], "tool");
        assert!(keys[0]["fingerprint_sha256"].as_str().is_some());

        let remove = handle_plugin_trust(&json!({"action": "remove", "key_id": "demo"}), &ctx).await;
        assert_eq!(remove["status"], "ok");
        assert_eq!(remove["result"]["removed"], true);
    }

    #[tokio::test]
    async fn plugin_trust_get_enable_and_disable_round_trips() {
        let dir = tempdir().expect("tempdir");
        let ctx = EngineContext::new(dir.path().to_str().expect("utf8 path"));
        let pubkey_hex = "22".repeat(32);

        let add = handle_plugin_trust(
            &json!({
                "action": "add",
                "key_id": "demo2",
                "pubkey_hex": pubkey_hex,
                "label": "demo key 2"
            }),
            &ctx,
        )
        .await;
        assert_eq!(add["status"], "ok");

        let disable = handle_plugin_trust(
            &json!({"action": "disable", "key_id": "demo2"}),
            &ctx,
        )
        .await;
        assert_eq!(disable["status"], "ok");
        assert_eq!(disable["result"]["enabled"], false);

        let get = handle_plugin_trust(
            &json!({"action": "get", "key_id": "demo2"}),
            &ctx,
        )
        .await;
        assert_eq!(get["status"], "ok");
        assert_eq!(get["result"]["key"]["enabled"], false);

        let enable = handle_plugin_trust(
            &json!({"action": "enable", "key_id": "demo2"}),
            &ctx,
        )
        .await;
        assert_eq!(enable["status"], "ok");
        assert_eq!(enable["result"]["enabled"], true);
    }
}
