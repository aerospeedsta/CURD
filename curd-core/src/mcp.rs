use crate::context::{SessionEntry, handle_benchmark};
use crate::plan::SystemEvent;
use crate::{DslNode, EngineContext, Plan, ReplState, dispatch_tool};
use anyhow::Result;
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

pub const API_VERSION: &str = "0.3.0";

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpServerMode {
    Lite,
    Full,
}

impl McpServerMode {
    pub fn from_env() -> Self {
        match std::env::var("CURD_MODE")
            .unwrap_or_else(|_| "full".to_string())
            .to_lowercase()
            .as_str()
        {
            "lite" => McpServerMode::Lite,
            _ => McpServerMode::Full,
        }
    }

    pub fn allows_method(self, method: &str) -> bool {
        match self {
            McpServerMode::Full => true,
            McpServerMode::Lite => matches!(method, "initialize" | "tools/list" | "tools/call"),
        }
    }

    pub fn allows_tool(self, tool: &str) -> bool {
        match self {
            McpServerMode::Full => true,
            McpServerMode::Lite => {
                matches!(tool, "search" | "read" | "edit" | "graph" | "workspace")
            }
        }
    }
}

pub struct McpServer {
    root: String,
    mode: McpServerMode,
}

impl McpServer {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
            mode: McpServerMode::from_env(),
        }
    }

    pub async fn run(&self) -> Result<()> {
        let stdin = io::stdin();
        let mut reader = io::BufReader::new(stdin);

        let root_path = PathBuf::from(&self.root);
        crate::validate_workspace_config(&root_path)?;
        
        let ctx = EngineContext::new(&self.root);

        let mode = self.mode;
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Value>(32);
        let mut rx_events = ctx.tx_events.subscribe();

        // Dedicated writer task to ensure synchronized access to stdout
        let writer_handle = tokio::spawn(async move {
            let mut stdout = io::stdout();
            while let Some(msg) = rx.recv().await {
                if let Ok(serialized) = serde_json::to_string(&msg) {
                    let _ = writeln!(stdout, "{}", serialized);
                    let _ = stdout.flush();
                }
            }
        });
        let tx_events_out = tx.clone();
        let events_handle = tokio::spawn(async move {
            loop {
                match rx_events.recv().await {
                    Ok(event) => {
                        if let Some(msg) = system_event_to_notification(event) {
                            let _ = tx_events_out.send(msg).await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let mut handlers = tokio::task::JoinSet::new();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let payload = if trimmed.to_lowercase().starts_with("content-length:") {
                let length = trimmed[15..].trim().parse::<usize>().unwrap_or(0);
                if length > 10 * 1024 * 1024 {
                    anyhow::bail!("Payload too large (max 10MB). Requested: {} bytes", length);
                }
                loop {
                    line.clear();
                    reader.read_line(&mut line)?;
                    if line.trim().is_empty() {
                        break;
                    }
                }
                let mut buf = vec![0u8; length];
                reader.read_exact(&mut buf)?;
                String::from_utf8_lossy(&buf).to_string()
            } else {
                trimmed.to_string()
            };

            if let Ok(request) = serde_json::from_str::<Value>(&payload) {
                let method = request
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if method.starts_with("notifications/") {
                    continue;
                }
                let id = request.get("id").cloned().unwrap_or(Value::Null);

                let req = request.clone();
                let tx_clone = tx.clone();
                let ctx_clone = Arc::clone(&ctx);

                handlers.spawn(async move {
                    if !mode.allows_method(&method) {
                        let err = json!({"error": {"code": -32601, "message": format!("Method disabled: {}", method)}});
                        let finalized = finalize_response(err, &id);
                        let _ = tx_clone.send(finalized).await;
                        return;
                    }

                    ctx_clone.watchdog.heartbeat();

                    let response = match method.as_str() {
                        "initialize" => handle_initialize(&req),
                        "tools/list" => handle_tools_list(mode),
                        "tools/call" => handle_tools_call(&req, &ctx_clone, mode).await,
                        _ => json!({"error": {"code": -32601, "message": format!("Method not found: {}", method)}}),
                    };

                    let finalized = finalize_response(response, &id);
                    let _ = tx_clone.send(finalized).await;
                });
            }
        }

        // Wait for all request handlers to complete
        while handlers.join_next().await.is_some() {}
        events_handle.abort();

        // Drop the last transmitter so the writer task's receiver can terminate
        drop(tx);

        // Wait for all output to be flushed
        let _ = writer_handle.await;

        // --- FREEZE HOOK ---
        log::info!("EOF detected. Triggering freeze hook...");
        let sessions_guard = ctx.sessions.lock().await;
        let mut latest_by_pubkey: std::collections::HashMap<&str, (&SessionEntry, &str)> =
            std::collections::HashMap::new();
        for (token, entry) in sessions_guard.iter() {
            let k = entry.pubkey_hex.as_str();
            match latest_by_pubkey.get(k) {
                Some((existing, _)) if existing.last_touched_secs >= entry.last_touched_secs => {}
                _ => {
                    latest_by_pubkey.insert(k, (entry, token));
                }
            }
        }
        for (entry, _token) in latest_by_pubkey.values().map(|(e, t)| (*e, *t)) {
            if let Err(e) = crate::auth::IdentityManager::freeze_session(
                &ctx.workspace_root,
                &entry.pubkey_hex,
                &entry.state,
            ) {
                log::error!("Failed to freeze session {}: {}", entry.pubkey_hex, e);
            }
        }

        Ok(())
    }
}

// ── MCP Protocol Handlers ──────────────────────────────────────────────

pub fn handle_initialize(_req: &Value) -> Value {
    json!({
        "result": {
            "apiVersion": API_VERSION,
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "curd", "version": "0.1.0" }
        }
    })
}

fn parse_indexing_summary(summary: &str) -> Option<(usize, usize)> {
    let s = summary.strip_prefix("Indexing: ")?;
    let s = s.strip_suffix(" files")?;
    let (processed, total) = s.split_once('/')?;
    let processed_files = processed.trim().parse::<usize>().ok()?;
    let total_files = total.trim().parse::<usize>().ok()?;
    Some((processed_files, total_files))
}

fn parse_index_stats_summary(summary: &str) -> Option<serde_json::Map<String, Value>> {
    let s = summary.strip_prefix("IndexStats: ")?;
    let mut out = serde_json::Map::new();
    for part in s.split_whitespace() {
        let (k, v) = part.split_once('=')?;
        if let Ok(n) = v.parse::<u64>() {
            out.insert(k.to_string(), json!(n));
        } else {
            out.insert(k.to_string(), json!(v));
        }
    }
    Some(out)
}

fn parse_index_stall_summary(summary: &str) -> Option<(usize, usize, u64)> {
    let s = summary.strip_prefix("IndexStall: ")?;
    let (left, right) = s.split_once(" no_progress_ms=")?;
    let prog = left.trim().trim_end_matches(" files");
    let (processed, total) = prog.split_once('/')?;
    let processed_files = processed.trim().parse::<usize>().ok()?;
    let total_files = total.trim().parse::<usize>().ok()?;
    let no_progress_ms = right.trim().parse::<u64>().ok()?;
    Some((processed_files, total_files, no_progress_ms))
}

fn system_event_to_notification(event: SystemEvent) -> Option<Value> {
    match event {
        SystemEvent::NodeCompleted {
            summary,
            duration_ms,
            ..
        } => {
            if let Some((processed_files, total_files)) = parse_indexing_summary(&summary) {
                let percent = if total_files == 0 {
                    0.0
                } else {
                    ((processed_files as f64 / total_files as f64) * 100.0).clamp(0.0, 100.0)
                };
                return Some(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": {
                        "phase": "indexing",
                        "processed_files": processed_files,
                        "total_files": total_files,
                        "percent": percent,
                        "duration_ms": duration_ms,
                        "summary": summary
                    }
                }));
            }
            if let Some((processed_files, total_files, no_progress_ms)) =
                parse_index_stall_summary(&summary)
            {
                return Some(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": {
                        "phase": "stall_detected",
                        "processed_files": processed_files,
                        "total_files": total_files,
                        "no_progress_ms": no_progress_ms,
                        "duration_ms": duration_ms,
                        "summary": summary
                    }
                }));
            }
            if let Some(mut stats) = parse_index_stats_summary(&summary) {
                stats.insert("phase".to_string(), json!("indexing_stats"));
                stats.insert("duration_ms".to_string(), json!(duration_ms));
                stats.insert("summary".to_string(), json!(summary));
                return Some(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": stats
                }));
            }
            None
        }
        _ => None,
    }
}

pub fn handle_tools_list(mode: McpServerMode) -> Value {
    let mut tools = get_all_tools();
    if mode == McpServerMode::Lite {
        tools.retain(|t| {
            let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("");
            name == "search"
                || name == "read"
                || name == "edit"
                || name == "graph"
                || name == "workspace"
        });
    }
    json!({ "result": { "tools": tools } })
}

pub async fn handle_tools_call(req: &Value, ctx: &EngineContext, mode: McpServerMode) -> Value {
    let params = req.get("params").unwrap_or(&Value::Null);
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let default_val = json!({});
    let tool_params = params.get("arguments").unwrap_or(&default_val);

    if !mode.allows_tool(name) {
        return json!({
            "error": {
                "code": -32601,
                "message": format!("Tool disabled in current mode: {}", name),
                "details": null
            }
        });
    }

    if mode == McpServerMode::Lite && name == "workspace" {
        let action = tool_params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("status");
        let allowed = matches!(action, "status" | "list" | "dependencies");
        if !allowed {
            return json!({
                "error": {
                    "code": -32601,
                    "message": format!("Workspace action disabled in lite mode: {}", action),
                    "details": null
                }
            });
        }
    }

    let result = if name == "batch" {
        handle_batch(tool_params, ctx).await
    } else if name == "session_open" {
        handle_session_open(tool_params, ctx).await
    } else if name == "session_verify" {
        handle_session_verify(tool_params, ctx).await
    } else if name == "execute_dsl" {
        handle_execute_dsl(tool_params, ctx).await
    } else if name == "execute_plan" {
        handle_execute_plan(tool_params, ctx).await
    } else if name == "history" {
        handle_history(tool_params, ctx).await
    } else if name == "benchmark" {
        let allow_bench = cfg!(debug_assertions) || std::env::var("CURD_ALLOW_BENCHMARK").is_ok();
        if allow_bench {
            handle_benchmark(tool_params, ctx).await
        } else {
            json!({"error": "The benchmark tool is disabled in release builds for security and stability. Set CURD_ALLOW_BENCHMARK=1 to override."})
        }
    } else {
        dispatch_tool(name, tool_params, ctx).await
    };

    json!({ "result": { "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Error serializing result".to_string()) }] } })
}

async fn handle_session_open(params: &Value, ctx: &EngineContext) -> Value {
    let pubkey_hex = params
        .get("pubkey_hex")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if pubkey_hex.is_empty() {
        return json!({"error": "pubkey_hex is required"});
    }

    let mut nonce = [0u8; 32];
    if let Err(e) = getrandom::fill(&mut nonce) {
        return json!({"error": format!("Failed to generate nonce: {}", e)});
    }
    // Simple hex encoding
    let nonce_hex: String = nonce.iter().map(|b| format!("{:02x}", b)).collect();

    let mut pending = ctx.pending_challenges.lock().await;
    pending.insert(pubkey_hex.to_string(), nonce_hex.clone());

    json!({
        "status": "ok",
        "nonce": nonce_hex
    })
}

async fn handle_session_verify(params: &Value, ctx: &EngineContext) -> Value {
    let pubkey_hex = params
        .get("pubkey_hex")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let signature_hex = params
        .get("signature_hex")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if pubkey_hex.is_empty() || signature_hex.is_empty() {
        return json!({"error": "pubkey_hex and signature_hex are required"});
    }

    let nonce_hex = {
        let mut pending = ctx.pending_challenges.lock().await;
        match pending.remove(pubkey_hex) {
            Some(n) => n,
            None => return json!({"error": "No pending challenge found for this pubkey"}),
        }
    };

    match crate::auth::IdentityManager::verify_signature(
        pubkey_hex,
        nonce_hex.as_bytes(),
        signature_hex,
    ) {
        Ok(true) => {
            let session_token = format!("sess_{}", Uuid::new_v4());
            let (state, restored) = if let Some(frozen_state) =
                crate::auth::IdentityManager::thaw_session(&ctx.workspace_root, pubkey_hex)
            {
                (frozen_state, true)
            } else {
                (ReplState::new(), false)
            };
            let entry = SessionEntry {
                pubkey_hex: pubkey_hex.to_string(),
                state,
                last_touched_secs: now_secs(),
            };
            let mut sessions = ctx.sessions.lock().await;
            sessions.insert(session_token.clone(), entry);

            json!({
                "status": "authenticated",
                "session_token": session_token,
                "restored_state": restored
            })
        }
        Ok(false) | Err(_) => json!({"error": "Invalid signature. Authentication failed."}),
    }
}

async fn handle_execute_dsl(params: &Value, ctx: &EngineContext) -> Value {
    let nodes: Vec<DslNode> = match params
        .get("nodes")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
    {
        Some(n) => n,
        None => return json!({"error": "Invalid or missing 'nodes' for execute_dsl"}),
    };

    // Check for authenticated session
    let session_token = params
        .get("session_token")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if session_token.is_empty() {
        return json!({"error": "Unauthorized: session_token is required for execute_dsl"});
    }

    let mut local_state = {
        let sessions_guard = ctx.sessions.lock().await;
        if let Some(entry) = sessions_guard.get(session_token) {
            ReplState::from_variables(entry.state.variables.clone())
        } else {
            return json!({"error": "Unauthorized: Invalid or expired session_token."});
        }
    };

    
    match ctx.ple.execute_dsl(&nodes, ctx, &mut local_state).await {
        Ok(res) => {
            let mut sessions_guard = ctx.sessions.lock().await;
            if let Some(entry) = sessions_guard.get_mut(session_token) {
                entry.state.variables.extend(local_state.variables);
                entry.last_touched_secs = now_secs();
            }

            let val = json!({"status": "ok", "results": res});
            ctx.he
                .log(ctx.session_id, "dsl", json!(nodes), val.clone(), true, None);
            val
        }
        Err(e) => {
            let err_msg = e.to_string();
            let val = json!({"error": err_msg.clone()});
            ctx.he.log(
                ctx.session_id,
                "dsl",
                json!(nodes),
                val.clone(),
                false,
                Some(err_msg),
            );
            val
        }
    }
}

async fn handle_execute_plan(params: &Value, ctx: &EngineContext) -> Value {
    let plan: Plan = match params
        .get("plan")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
    {
        Some(p) => p,
        None => return json!({"error": "Invalid or missing 'plan' for execute_plan"}),
    };

    let session_token = params
        .get("session_token")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if session_token.is_empty() {
        return json!({"error": "Unauthorized: session_token is required for execute_plan"});
    }

    let mut local_state = {
        let sessions_guard = ctx.sessions.lock().await;
        if let Some(entry) = sessions_guard.get(session_token) {
            ReplState::from_variables(entry.state.variables.clone())
        } else {
            return json!({"error": "Unauthorized: Invalid or expired session_token."});
        }
    };

    
    match ctx.ple.execute_plan(&plan, ctx, &mut local_state).await {
        Ok(res) => {
            let mut sessions_guard = ctx.sessions.lock().await;
            if let Some(entry) = sessions_guard.get_mut(session_token) {
                entry.state.variables.extend(local_state.variables);
                entry.last_touched_secs = now_secs();
            }
            let val = json!({"status": "ok", "results": res});
            ctx.he
                .log(ctx.session_id, "plan", json!(plan), val.clone(), true, None);
            val
        }
        Err(e) => {
            let err_msg = e.to_string();
            let val = json!({"error": err_msg.clone()});
            ctx.he.log(
                ctx.session_id,
                "plan",
                json!(plan),
                val.clone(),
                false,
                Some(err_msg),
            );
            val
        }
    }
}

async fn handle_history(params: &Value, ctx: &EngineContext) -> Value {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let history = ctx.he.get_history(limit);
    json!({"status": "ok", "history": history})
}

async fn handle_batch(params: &Value, ctx: &EngineContext) -> Value {
    let tasks = params
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut results = Vec::new();
    let default_args = json!({});

    use std::collections::HashMap;
    let mut graph = petgraph::graph::DiGraph::<usize, ()>::new();
    let mut node_indices = Vec::new();
    let mut id_to_idx = HashMap::new();

    for (i, task) in tasks.iter().enumerate() {
        let id = task.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let idx = graph.add_node(i);
        node_indices.push(idx);
        if !id.is_empty() {
            id_to_idx.insert(id.to_string(), idx);
        }
    }

    for (i, task) in tasks.iter().enumerate() {
        if let Some(deps) = task.get("depends_on").and_then(|v| v.as_array()) {
            for dep in deps {
                if let Some(dep_id) = dep.as_str()
                    && let Some(&dep_idx) = id_to_idx.get(dep_id)
                {
                    graph.add_edge(dep_idx, node_indices[i], ());
                }
            }
        }
    }

    // Perform topological sort to resolve dependencies
    let sorted_indices = match petgraph::algo::toposort(&graph, None) {
        Ok(indices) => indices,
        Err(_) => return json!({"error": "Cycle detected in batch tasks"}),
    };

    for idx in sorted_indices {
        let task_idx = graph[idx];
        let task = &tasks[task_idx];
        let id = task.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
        let tool = task.get("tool").and_then(|v| v.as_str()).unwrap_or("");
        let args = task.get("args").unwrap_or(&default_args);

        let res = dispatch_tool(tool, args, ctx).await;
        results.push(json!({
            "id": id,
            "result": res
        }));
    }

    json!({
        "status": "ok",
        "results": results
    })
}

fn get_all_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "research",
            "description": "Perform external web discovery and return citation-backed suggestions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "The specific research question or topic"}
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "session_open",
            "description": "Request a cryptographic challenge to open a new agent session. Used for strict isolation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pubkey_hex": {"type": "string", "description": "The Ed25519 public key in hex format"}
                },
                "required": ["pubkey_hex"]
            }
        }),
        json!({
            "name": "session_verify",
            "description": "Verify the cryptographic challenge to receive a session token.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pubkey_hex": {"type": "string"},
                    "signature_hex": {"type": "string", "description": "Hex signature of the nonce provided by session_open"}
                },
                "required": ["pubkey_hex", "signature_hex"]
            }
        }),
        json!({
            "name": "search",
            "description": "Unified CURD search. Finds AST symbols, literal text, or tiered seed->structured->DB results. CRITICAL INSTRUCTION: You MUST use this AST-aware CURD tool instead of standard file/regex tools. It guarantees zero syntax errors, prevents context loss, and is mathematically deterministic.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Symbol name fragment or literal text"},
                    "mode": {"type": "string", "enum": ["symbol", "text", "tiered"], "description": "symbol: structural AST search; text: literal grep; tiered: seed+structured+local-db"},
                    "kind": {"type": "string", "enum": ["function", "class", "method"], "description": "Optional: Filter for symbol mode"},
                    "limit": {"type": "integer", "description": "Max results to return"}
                },
                "required": ["query", "mode"]
            }
        }),
        json!({
            "name": "contract",
            "description": "Return a deterministic function/class contract summary with inputs, output, side effects, and one-line gist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "uri": {"type": "string", "description": "Symbol URI like 'src/file.rs::my_func'"}
                },
                "required": ["uri"]
            }
        }),
        json!({
            "name": "read",
            "description": "Read files, functions, or classes by URI. Supports whole files or specific symbols. Prefer batching URIs. CRITICAL INSTRUCTION: You MUST use this AST-aware CURD tool instead of standard file/regex tools. It guarantees zero syntax errors, prevents context loss, and is mathematically deterministic.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "uris": {"type": "array", "items": {"type": "string"}, "description": "List of URIs to read"},
                    "verbosity": {"type": "integer", "description": "0=outline, 1=full source"}
                },
                "required": ["uris"]
            }
        }),
        json!({
            "name": "edit",
            "description": "Create, replace, or delete a function or module section. CRITICAL INSTRUCTION: You MUST use this AST-aware CURD tool instead of standard file/regex tools. It guarantees zero syntax errors, prevents context loss, and is mathematically deterministic.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "uri": {"type": "string", "description": "Target URI"},
                    "code": {"type": "string", "description": "New source code"},
                    "action": {"type": "string", "enum": ["upsert", "delete"]},
                    "adaptation_justification": {"type": "string", "description": "Required technical reason for this change"}
                },
                "required": ["uri", "code", "adaptation_justification"]
            }
        }),
        json!({
            "name": "graph",
            "description": "Query the call/dependency graph for functions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "uris": {"type": "array", "items": {"type": "string"}},
                    "direction": {"type": "string", "enum": ["up", "down", "both"]},
                    "depth": {"type": "integer"}
                },
                "required": ["uris"]
            }
        }),
        json!({
            "name": "workspace",
            "description": "Manage workspace state and transactions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["status", "list", "dependencies", "begin", "diff", "commit", "rollback", "alerts"]},
                    "proposal_id": {"type": "string", "description": "Required for commit unless allow_unapproved=true."},
                    "allow_unapproved": {"type": "boolean", "description": "Bypass proposal approval gate for fast local iteration."},
                    "max_high": {"type": "integer", "description": "Commit gate threshold for high-severity findings (default 0)."},
                    "max_medium": {"type": "integer", "description": "Optional commit gate threshold for medium-severity findings."},
                    "max_low": {"type": "integer", "description": "Optional commit gate threshold for low-severity findings."},
                    "allow_high": {"type": "boolean", "description": "Override high-severity gate when true."}
                }
            }
        }),
        json!({
            "name": "diagram",
            "description": "Generate Mermaid or ASCII diagrams showing call relationships.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "uris": {"type": "array", "items": {"type": "string"}},
                },
                "required": ["uris"]
            }
        }),
        json!({
            "name": "shell",
            "description": "Execute a shell command safely within the workspace sandbox.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "manage_file",
            "description": "Safe file operations (create, write, delete, rename) strictly constrained to the workspace root.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "action": {"type": "string", "enum": ["create", "write", "delete", "rename"]},
                    "destination": {"type": "string"}
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "lsp",
            "description": "Get syntax and/or semantic diagnostics for a file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "uri": {"type": "string"},
                    "mode": {"type": "string", "enum": ["syntax", "semantic", "both"]}
                },
                "required": ["uri"]
            }
        }),
        json!({
            "name": "profile",
            "description": "In-CURD profiler: generate ASCII flamegraph from dependency call stacks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "roots": {"type": "array", "items": {"type": "string"}},
                },
                "required": ["roots"]
            }
        }),
        json!({
            "name": "debug",
            "description": "Run logical debug sessions or short interpreter snippets for hypothesis testing. Use this sandboxed DAP engine for all execution. It provides semantic, graph-mapped tracebacks instead of raw terminal text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["execute", "backends", "start", "send", "recv", "stop"]},
                    "language": {"type": "string"},
                    "snippet": {"type": "string"},
                    "target": {"type": "string"},
                    "session_id": {"type": "integer"}
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "session",
            "description": "Manage session-scoped review baselines.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["begin", "status", "changes", "review", "end"]},
                    "label": {"type": "string"},
                    "limit": {"type": "integer"}
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "doc",
            "description": "Get verbose documentation and examples for any CURD tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool": {"type": "string"},
                },
                "required": ["tool"]
            }
        }),
        json!({
            "name": "batch",
            "description": "Execute multiple tool calls in one turn as a dependency DAG.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string"},
                                "tool": {"type": "string"},
                                "args": {"type": "object"},
                                "depends_on": {"type": "array", "items": {"type": "string"}}
                            },
                            "required": ["id", "tool", "args"]
                        }
                    }
                },
                "required": ["tasks"]
            }
        }),
        json!({
            "name": "benchmark",
            "description": "Run quick in-process timing for CURD operations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operation": {"type": "string"},
                    "params": {"type": "object"},
                    "iterations": {"type": "integer"},
                    "save_baseline": {"type": "boolean"}
                },
                "required": ["operation"]
            }
        }),
        json!({
            "name": "simulate",
            "description": "Dry-run preflight for execute_plan/execute_dsl payloads. Validates schema and dependencies without mutating workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": {"type": "string", "enum": ["execute_plan", "execute_dsl"]},
                    "plan": {"type": "object"},
                    "nodes": {"type": "array", "items": {"type": "object"}}
                },
                "required": ["mode"]
            }
        }),
        json!({
            "name": "template",
            "description": "Manage reusable workflow templates (register/list/get/instantiate).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["register", "list", "get", "instantiate"]},
                    "name": {"type": "string"},
                    "template": {"type": "object"},
                    "vars": {"type": "object"}
                }
            }
        }),
        json!({
            "name": "proposal",
            "description": "Manage local CURD change proposals (open/status/approve/reject) independent of git workflows.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["open", "status", "run_gate", "approve", "reject"]},
                    "id": {"type": "string", "description": "Proposal id (UUID string recommended)."},
                    "title": {"type": "string"},
                    "summary": {"type": "string"},
                    "simulate": {"type": "object"},
                    "crawl": {"type": "object"},
                    "simulate_args": {"type": "object"},
                    "crawl_args": {"type": "object"},
                    "checkpoints": {"type": "object"},
                    "review": {"type": "object"},
                    "reason": {"type": "string"}
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "checkpoint",
            "description": "Inspect milestone checkpoints for a plan run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["list", "get"]},
                    "plan_id": {"type": "string", "format": "uuid"},
                    "name": {"type": "string"}
                },
                "required": ["action", "plan_id"]
            }
        }),
        json!({
            "name": "delegate",
            "description": "Manager-worker delegation board for plan nodes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["create", "claim", "heartbeat", "complete", "auto_assign", "status"]},
                    "plan_id": {"type": "string", "format": "uuid"},
                    "nodes": {"type": "array", "items": {"type": "string"}},
                    "node_id": {"type": "string", "format": "uuid"},
                    "worker": {"type": "string"},
                    "stale_timeout_secs": {"type": "integer"},
                    "max_claims": {"type": "integer"}
                },
                "required": ["action", "plan_id"]
            }
        }),
        json!({
            "name": "frontier",
            "description": "Frontier queue operations for graph-driven work distribution.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["seed", "pop", "status", "reset"]},
                    "plan_id": {"type": "string", "format": "uuid"},
                    "uris": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["action", "plan_id"]
            }
        }),
        json!({
            "name": "crawl",
            "description": "Deterministic dry-run crawler skeletons for heal/audit/prune modes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": {"type": "string", "enum": ["crawl_heal", "crawl_audit", "crawl_prune", "crawl_mutate"]},
                    "roots": {"type": "array", "items": {"type": "string"}},
                    "depth": {"type": "integer"},
                    "enqueue": {"type": "boolean"},
                    "plan_id": {"type": "string", "format": "uuid"},
                    "top_k": {"type": "integer"},
                    "include_contract_gists": {"type": "boolean"},
                    "contract_top_k": {"type": "integer"}
                },
                "required": ["mode", "roots"]
            }
        }),
        json!({
            "name": "register_plan",
            "description": "Register a high-level multi-step execution plan (DAG) in the engine. This populates the TUI roadmap for observability before execution starts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "nodes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string", "format": "uuid"},
                                "op": {
                                    "type": "object",
                                    "properties": {
                                        "McpCall": {
                                            "type": "object",
                                            "properties": {
                                                "tool": {"type": "string"},
                                                "args": {"type": "object"}
                                            },
                                            "required": ["tool", "args"]
                                        }
                                    }
                                },
                                "dependencies": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "Id": {"type": "string", "format": "uuid"}
                                        }
                                    }
                                }
                            },
                            "required": ["id", "op"]
                        }
                    }
                },
                "required": ["id", "nodes"]
            }
        }),
        json!({
            "name": "execute_active_plan",
            "description": "Execute the previously registered active plan. Transitions nodes from Pending to Executing/Completed in the TUI.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "execute_dsl",
            "description": "Execute a sequence of DSL nodes (Call, Atomic, Abort, Assign). Supports variable interpolation with $var.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_token": {"type": "string", "description": "Token obtained from session_verify"},
                    "nodes": {
                        "type": "array",
                        "items": {"type": "object"}
                    }
                },
                "required": ["nodes"]
            }
        }),
        json!({
            "name": "execute_plan",
            "description": "Execute a dependency-aware DAG Plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "plan": {"type": "object"}
                },
                "required": ["plan"]
            }
        }),
        json!({
            "name": "history",
            "description": "Retrieve the global REPL and Plan execution history.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "description": "Number of entries to return (default 50)"}
                }
            }
        }),
    ]
}

pub fn finalize_response(response: Value, id: &Value) -> Value {
    let mut resp = response;
    normalize_error_shape(&mut resp);
    if let Some(obj) = resp.as_object_mut() {
        obj.insert("id".to_string(), id.clone());
        obj.insert("jsonrpc".to_string(), json!("2.0"));
        obj.insert("api_version".to_string(), json!(API_VERSION));
        if !obj.contains_key("provenance") {
            obj.insert("provenance".to_string(), json!("local"));
        }
    }
    resp
}

fn normalize_error_shape(resp: &mut Value) {
    let Some(obj) = resp.as_object_mut() else {
        return;
    };
    let Some(existing) = obj.remove("error") else {
        return;
    };

    let normalized = match existing {
        Value::Object(mut eobj) => {
            if !eobj.contains_key("code") {
                eobj.insert("code".to_string(), json!(-32000));
            }
            if !eobj.contains_key("message") {
                eobj.insert("message".to_string(), json!("Internal error"));
            }
            if !eobj.contains_key("details") {
                eobj.insert("details".to_string(), Value::Null);
            }
            Value::Object(eobj)
        }
        Value::String(msg) => json!({
            "code": -32000,
            "message": msg,
            "details": null
        }),
        other => json!({
            "code": -32000,
            "message": "Internal error",
            "details": other
        }),
    };

    obj.insert("error".to_string(), normalized);
}

#[cfg(test)]
mod tests {
    use super::{
        McpServerMode, handle_tools_call, handle_tools_list, system_event_to_notification,
    };
    use crate::context::{EngineContext, SessionEntry};
    use crate::plan::{ReplState, SystemEvent};
    use crate::{
        DebugEngine, DiagramEngine, DocEngine, EditEngine, FileEngine, FindEngine, GraphEngine,
        HistoryEngine, LspEngine, ProfileEngine, ReadEngine, SearchEngine,
        SessionReviewEngine, Watchdog, WorkspaceEngine,
    };
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    fn build_test_ctx(root: &Path) -> EngineContext {
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let watchdog = Arc::new(Watchdog::new(root.clone()));
        let (tx_events, _) = tokio::sync::broadcast::channel(32);
        EngineContext {
            workspace_root: root.clone(),
            session_id: Uuid::new_v4(),
            read_only: false,
            se: Arc::new(SearchEngine::new(&root)),
            re: Arc::new(ReadEngine::new(&root)),
            ee: Arc::new(EditEngine::new(&root)),
            doctore: Arc::new(crate::doctor::DoctorEngine::new(&root)),
            ge: Arc::new(GraphEngine::new(&root)),
            ple: Arc::new(crate::PlanEngine::new(&root)),
            she: Arc::new(crate::ShellEngine::new(&root)),
            we: Arc::new(WorkspaceEngine::new(&root)),
            mu: Arc::new(crate::MutationEngine::new(&root)),
            fe: Arc::new(FindEngine::new(&root)),
            de: Arc::new(DiagramEngine::new(&root)),
            fie: Arc::new(FileEngine::new(&root)),
            le: Arc::new(LspEngine::new(&root)),
            pe: Arc::new(ProfileEngine::new(&root)),
            dbe: Arc::new(DebugEngine::new(&root)),
            sre: Arc::new(SessionReviewEngine::new(&root)),
            doce: Arc::new(DocEngine::new()),
            he: Arc::new(HistoryEngine::new(&root)),
            tx_events,
            global_state: Arc::new(Mutex::new(ReplState::new())),
            sessions: Arc::new(Mutex::new(HashMap::<String, SessionEntry>::new())),
            pending_challenges: Arc::new(Mutex::new(HashMap::<String, String>::new())),
            watchdog,
        }
    }

    #[test]
    fn index_stall_event_maps_to_progress_notification() {
        let event = SystemEvent::NodeCompleted {
            node_id: Uuid::nil(),
            duration_ms: 0,
            summary: "IndexStall: 10/100 files no_progress_ms=16000".to_string(),
            artifact_path: None,
        };
        let notif = system_event_to_notification(event).expect("stall notification");
        assert_eq!(notif["method"], "notifications/progress");
        assert_eq!(notif["params"]["phase"], "stall_detected");
        assert_eq!(notif["params"]["processed_files"], 10);
        assert_eq!(notif["params"]["total_files"], 100);
        assert_eq!(notif["params"]["no_progress_ms"], 16000);
    }

    #[test]
    fn index_stats_event_includes_extended_fields() {
        let summary = "IndexStats: index_mode=full parser_backend=native parser_backend_effective=mixed execution_model=multiprocess max_file_size=524288 large_file_policy=skip chunk_size=4096 chunk_count=1 total_files=100 cache_hits=10 cache_misses=90 unsupported_lang=0 skipped_too_large=0 large_file_skeleton=0 large_file_full=0 fast_prefilter_skips=0 parse_fail=0 no_symbols=1 native_files=60 wasm_files=30 native_fallbacks=30 scan_ms=1 cache_load_ms=2 parse_ms=3 merge_ms=4 serialize_ms=5 total_ms=6";
        let event = SystemEvent::NodeCompleted {
            node_id: Uuid::nil(),
            duration_ms: 6,
            summary: summary.to_string(),
            artifact_path: None,
        };
        let notif = system_event_to_notification(event).expect("stats notification");
        assert_eq!(notif["method"], "notifications/progress");
        assert_eq!(notif["params"]["phase"], "indexing_stats");
        assert_eq!(notif["params"]["parser_backend_effective"], "mixed");
        assert_eq!(notif["params"]["native_files"], 60);
        assert_eq!(notif["params"]["wasm_files"], 30);
        assert_eq!(notif["params"]["native_fallbacks"], 30);
        assert_eq!(notif["params"]["execution_model"], "multiprocess");
        assert!(notif["params"].get("summary").is_some());
    }

    #[test]
    fn tools_list_search_mode_includes_tiered() {
        let listed = handle_tools_list(McpServerMode::Full);
        let tools = listed["result"]["tools"]
            .as_array()
            .expect("tools array missing");
        let search = tools
            .iter()
            .find(|v| v.get("name").and_then(|n| n.as_str()) == Some("search"))
            .expect("search tool missing");
        let modes = search["inputSchema"]["properties"]["mode"]["enum"]
            .as_array()
            .expect("mode enum missing");
        assert!(modes.iter().any(|v| v.as_str() == Some("tiered")));
    }

    #[tokio::test]
    async fn tools_call_tiered_returns_tier3_payload() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(
            root.join("main.rs"),
            "fn hello_world() {}\nfn helper_function() {}\n",
        )
        .expect("write test file");
        let ctx = build_test_ctx(root);
        let req = json!({
            "params": {
                "name": "search",
                "arguments": {
                    "mode": "tiered",
                    "query": "hello",
                    "limit": 5
                }
            }
        });

        let resp = handle_tools_call(&req, &ctx, McpServerMode::Full).await;
        let text = resp["result"]["content"][0]["text"]
            .as_str()
            .expect("text result missing");
        let parsed: Value = serde_json::from_str(text).expect("invalid tool payload JSON");
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["tiered"]["tier1"]["kind"], "text_seed");
        assert_eq!(parsed["tiered"]["tier2"]["kind"], "structured_symbol");
        assert_eq!(parsed["tiered"]["tier3"]["kind"], "local_db_index_runs");
        assert!(parsed["tiered"]["tier3"]["status"].is_string());
        assert!(parsed["tiered"]["tier3"]["rows"].is_array());
    }
}
