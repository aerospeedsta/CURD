use crate::{
    CurdConfig, DebugEngine, DiagramEngine, DocEngine, DslNode, EditEngine, FileEngine, FindEngine,
    GraphEngine, HistoryEngine, IndexBuildStats, LspEngine, MutationEngine, Plan, PlanEngine, ProfileEngine,
    ReadEngine, ReplState, SearchEngine, SessionReviewEngine, ShellEngine, SymbolKind, Watchdog,
    WorkspaceEngine,
    doctor::{DoctorEngine, DoctorIndexConfig, DoctorProfile, DoctorThresholds},
    plan::SystemEvent,
    read_recent_index_runs,
};
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

pub struct EngineContext {
    pub workspace_root: PathBuf,
    pub session_id: Uuid,
    pub read_only: bool,
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
    pub sre: Arc<SessionReviewEngine>,
    pub doce: Arc<DocEngine>,
    pub doctore: Arc<DoctorEngine>,
    pub ple: Arc<PlanEngine>,
    pub he: Arc<HistoryEngine>,
    pub mu: Arc<MutationEngine>,
    pub tx_events: broadcast::Sender<SystemEvent>,
    pub global_state: Arc<Mutex<ReplState>>,
    pub sessions: Arc<Mutex<HashMap<String, SessionEntry>>>,
    pub pending_challenges: Arc<Mutex<HashMap<String, String>>>,
    pub watchdog: Arc<Watchdog>,
}

pub struct SessionEntry {
    pub pubkey_hex: String,
    pub state: ReplState,
    pub last_touched_secs: u64,
}

impl EngineContext {
    pub fn clone_for_repl(&self) -> Self {
        Self {
            workspace_root: self.workspace_root.clone(),
            session_id: self.session_id,
            read_only: self.read_only,
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
            sre: Arc::clone(&self.sre),
            doce: Arc::clone(&self.doce),
            doctore: Arc::clone(&self.doctore),
            ple: Arc::clone(&self.ple),
            he: Arc::clone(&self.he),
            mu: Arc::clone(&self.mu),
            tx_events: self.tx_events.clone(),
            global_state: Arc::clone(&self.global_state),
            sessions: Arc::clone(&self.sessions),
            pending_challenges: Arc::clone(&self.pending_challenges),
            watchdog: Arc::clone(&self.watchdog),
        }
    }

    pub fn new(root: &str) -> Arc<Self> {
        let root_path = PathBuf::from(root);
        let watchdog = Arc::new(Watchdog::new(root_path.clone()));
        watchdog.start();

        let session_id = Uuid::new_v4();
        let (tx_events, _) = tokio::sync::broadcast::channel(1024);

        // Session Locking
        let lock_path = root_path.join(".curd").join("SESSION_LOCK");
        let mut read_only = false;
        if let Some(parent) = lock_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if crate::workspace::is_workspace_locked(&root_path) {
            log::warn!("Workspace is locked by another session. Starting in READ-ONLY mode.");
            read_only = true;
        } else {
            let _ = fs::write(&lock_path, std::process::id().to_string());
        }

        Arc::new(EngineContext {
            workspace_root: root_path.clone(),
            session_id,
            read_only,
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
            sre: Arc::new(SessionReviewEngine::new(root)),
            doce: Arc::new(DocEngine::new()),
            doctore: Arc::new(DoctorEngine::new(root)),
            ple: Arc::new(PlanEngine::new(root)),
            he: Arc::new(HistoryEngine::new(&root_path)),
            mu: Arc::new(MutationEngine::new(root)),
            tx_events,
            global_state: Arc::new(Mutex::new(ReplState::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            pending_challenges: Arc::new(Mutex::new(HashMap::new())),
            watchdog,
        })
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
    // Enforcement of Read-Only mode for hazardous tools
    if ctx.read_only && risky_tool(name) {
        return json!({
            "error": format!("Cannot execute tool '{}': Workspace is locked in READ-ONLY mode by another active session.", name)
        });
    }

    let res = match name {
        "search" => handle_search(params, ctx).await,
        "contract" => handle_contract(params, ctx).await,
        "read" => handle_read(params, Arc::clone(&ctx.re)).await,
        "edit" => handle_edit(params, Arc::clone(&ctx.ee)).await,
        "graph" => handle_graph(params, Arc::clone(&ctx.ge)).await,
        "workspace" => handle_workspace(params, ctx).await,
        "find" => {
            json!({"error": "The 'find' tool has been merged into 'search'. Use 'search' with mode='text'."})
        }
        "shell" => handle_shell(params, &ctx.she).await,
        "diagram" => handle_diagram(params, Arc::clone(&ctx.de)).await,
        "manage_file" => handle_manage_file(params, Arc::clone(&ctx.fie)).await,
        "lsp" => handle_lsp(params, &ctx.le).await,
        "profile" => handle_profile(params, &ctx.pe).await,
        "debug" => handle_debug_dispatcher(params, &ctx.dbe).await,
        "session" => handle_session_dispatcher(params, &ctx.sre).await,
        "doc" => handle_doc(params, Arc::clone(&ctx.doce)).await,
        "doctor" => handle_doctor(params, &ctx.doctore).await,
        "benchmark" => handle_benchmark(params, ctx).await,
        "simulate" => handle_simulate(params).await,
        "template" => handle_template(params, ctx).await,
        "proposal" => handle_proposal(params, ctx).await,
        "checkpoint" => handle_checkpoint(params, ctx).await,
        "delegate" => handle_delegate(params, ctx).await,
        "frontier" => handle_frontier(params, ctx).await,
        "crawl" => handle_crawl(params, ctx).await,
        "register_plan" => handle_register_plan(params, ctx).await,
        "propose_plan" => handle_propose_plan(params, ctx).await,
        "execute_active_plan" => handle_execute_active_plan(params, ctx).await,
        "research" => handle_research(params).await,
        "mutate" => handle_mutate(params, Arc::clone(&ctx.mu)).await,
        _ => json!({"error": format!("Tool not found: {}", name)}),
    };

    let error = res.get("error").and_then(|e| e.as_str()).map(|s| s.to_string());
    let success = error.is_none();

    ctx.he.log(
        ctx.session_id,
        name,
        params.clone(),
        res.clone(),
        success,
        error,
    );

    res
}


async fn execute_benchmark_target(operation: &str, params: &Value, ctx: &EngineContext) -> Value {
    match operation {
        "search" => handle_search(params, ctx).await,
        "contract" => handle_contract(params, ctx).await,
        "read" => handle_read(params, Arc::clone(&ctx.re)).await,
        "edit" => handle_edit(params, Arc::clone(&ctx.ee)).await,
        "graph" => handle_graph(params, Arc::clone(&ctx.ge)).await,
        "workspace" => handle_workspace(params, ctx).await,
        "find" => {
            json!({"error": "The 'find' tool has been merged into 'search'. Use 'search' with mode='text'."})
        }
        "shell" => handle_shell(params, &ctx.she).await,
        "diagram" => handle_diagram(params, Arc::clone(&ctx.de)).await,
        "manage_file" => handle_manage_file(params, Arc::clone(&ctx.fie)).await,
        "lsp" => handle_lsp(params, &ctx.le).await,
        "profile" => handle_profile(params, &ctx.pe).await,
        "debug" => handle_debug_dispatcher(params, &ctx.dbe).await,
        "session" => handle_session_dispatcher(params, &ctx.sre).await,
        "doc" => handle_doc(params, Arc::clone(&ctx.doce)).await,
        "doctor" => handle_doctor(params, &ctx.doctore).await,
        "simulate" => handle_simulate(params).await,
        "template" => handle_template(params, ctx).await,
        "proposal" => handle_proposal(params, ctx).await,
        "checkpoint" => handle_checkpoint(params, ctx).await,
        "delegate" => handle_delegate(params, ctx).await,
        "frontier" => handle_frontier(params, ctx).await,
        "crawl" => handle_crawl(params, ctx).await,
        "register_plan" => handle_register_plan(params, ctx).await,
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
            | "propose_plan"
            | "execute_active_plan"
            | "execute_dsl"
            | "execute_plan"
            | "history"
            | "session_open"
            | "session_verify"
            | "research"
    )
}

fn risky_tool(name: &str) -> bool {
    matches!(name, "edit" | "manage_file" | "shell")
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

    let read = handle_read(&json!({"uris":[uri], "verbosity": 1}), Arc::clone(&ctx.re)).await;
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

pub async fn handle_read(params: &Value, engine: Arc<ReadEngine>) -> Value {
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
    match tokio::task::spawn_blocking(move || engine.read(uris, verbosity)).await {
        Ok(Ok(res)) => json!({"status": "ok", "results": res}),
        Ok(Err(e)) => json!({"error": e.to_string()}),
        Err(e) => json!({"error": format!("Task join error in read: {}", e)}),
    }
}

pub async fn handle_edit(params: &Value, engine: Arc<EditEngine>) -> Value {
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
    let justification = params
        .get("adaptation_justification")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if action == "upsert" && justification.trim().is_empty() {
        return json!({"error": "Missing 'adaptation_justification'. You must provide a concise technical reason why this specific adaptation is necessary."});
    }

    match tokio::task::spawn_blocking(move || engine.edit(&uri, &code, &action, None)).await {
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
    match tokio::task::spawn_blocking(move || engine.graph(uris, &direction, depth)).await {
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
            .sre
            .status()
            .ok()
            .and_then(|v| v.get("active").and_then(|a| a.as_bool()))
            .unwrap_or(false);

        if session_active {
            match ctx.sre.review().await {
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
                                "message": "Commit blocked by session review threshold gate",
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
                            "message": "Failed to run session review gate before commit",
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

pub async fn handle_shell(params: &Value, engine: &ShellEngine) -> Value {
    let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
    match engine.shell(command, None).await {
        Ok(res) => json!({"status": "ok", "output": res}),
        Err(e) => json!({"error": e.to_string()}),
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
    match tokio::task::spawn_blocking(move || engine.diagram(uris)).await {
        Ok(Ok(res)) => json!({"status": "ok", "diagram": res}),
        Ok(Err(e)) => json!({"error": e.to_string()}),
        Err(e) => json!({"error": format!("Task join error in diagram: {}", e)}),
    }
}

pub async fn handle_manage_file(params: &Value, engine: Arc<FileEngine>) -> Value {
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
    match tokio::task::spawn_blocking(move || engine.manage(&path, &action, dest.as_deref())).await
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
                .get("session_id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let snippet = params.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            engine.send_session(id, snippet).await
        }
        "recv" => {
            let id = params
                .get("session_id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            engine.recv_session(id)
        }
        "stop" => {
            let id = params
                .get("session_id")
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

pub async fn handle_session_dispatcher(params: &Value, engine: &SessionReviewEngine) -> Value {
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
        _ => Err(anyhow::anyhow!("Unknown session action: {}", action)),
    };
    match result {
        Ok(res) => json!({"status": "ok", "result": res}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn handle_doc(params: &Value, engine: Arc<DocEngine>) -> Value {
    let tool = params
        .get("tool")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    match tokio::task::spawn_blocking(move || engine.get_doc(&tool)).await {
        Ok(res) => res,
        Err(e) => json!({"error": format!("Task join error in doc: {}", e)}),
    }
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
            let path = dir.join(format!("{}.json", name));
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
            let path = dir.join(format!("{}.json", name));
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
            let path = dir.join(format!("{}.json", name));
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
        .join(plan_id);

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
            let path = base.join(name);
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

pub async fn handle_mutate(params: &Value, mu: Arc<MutationEngine>) -> Value {
    let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    if uri.is_empty() {
        return json!({"error": "mutate requires: uri"});
    }

    match mu.mutate_symbol(uri) {
        Ok(val) => val,
        Err(e) => json!({"error": format!("Mutation failed: {}", e)}),
    }
}

