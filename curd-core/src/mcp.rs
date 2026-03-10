use crate::context::{ConnectionEntry, PendingChallenge, handle_benchmark};
use crate::plan::{SystemEvent, SystemEventEnvelope, now_secs};
use crate::{DslNode, EngineContext, Plan, ReplState, dispatch_tool};
use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use std::path::PathBuf;
use std::sync::Arc;

pub const API_VERSION: &str = "0.7.0";

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

        let mode = self.mode;
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Value>(32);
        
        let root_for_ctx = self.root.clone();
        let (ctx_tx, ctx_rx) = tokio::sync::oneshot::channel::<Arc<EngineContext>>();
        let _ctx_handle = tokio::spawn(async move {
            let ctx = EngineContext::new(&root_for_ctx);
            let _ = ctx_tx.send(ctx);
        });
        let mut ctx_rx = Some(ctx_rx);
        let mut shared_ctx: Option<Arc<EngineContext>> = None;

        // Dedicated writer task to ensure synchronized access to stdout
        let writer_handle = tokio::spawn(async move {
            let mut stdout = io::stdout();
            while let Some(msg) = rx.recv().await {
                if let Ok(serialized) = serde_json::to_string(&msg) {
                    let _ = stdout.write_all(format!("{}\n", serialized).as_bytes()).await;
                    let _ = stdout.flush().await;
                }
            }
        });

        let mut handlers = tokio::task::JoinSet::new();
        
        let tx_events_out = tx.clone();
        let mut rx_events = None;

        let mut line = String::new();
        loop {
            if shared_ctx.is_none() && ctx_rx.is_some() {
                // If we HAVE events we want to bridge, we need to wait for ctx
                // But we don't want to block the loop yet.
            }
            line.clear();
            if reader.read_line(&mut line).await? == 0 {
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
                    reader.read_line(&mut line).await?;
                    if line.trim().is_empty() {
                        break;
                    }
                }
                let mut buf = vec![0u8; length];
                reader.read_exact(&mut buf).await?;
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

                // Get or await shared context for tool calls
                if shared_ctx.is_none() {
                    if let Some(rx) = ctx_rx.take() {
                        if method == "initialize" {
                            // Defer joining ctx for initialize to keep boot instant
                            ctx_rx = Some(rx);
                        } else if let Ok(ctx) = rx.await {
                            shared_ctx = Some(ctx.clone());
                            rx_events = Some(ctx.tx_events.subscribe());
                        }
                    }
                }
                let ctx_clone = shared_ctx.clone();

                handlers.spawn(async move {
                    if !mode.allows_method(&method) {
                        let err = json!({"error": {"code": -32601, "message": format!("Method disabled: {}", method)}});
                        let finalized = finalize_response(err, &id);
                        let _ = tx_clone.send(finalized).await;
                        return;
                    }

                    if method == "initialize" {
                        let response = handle_initialize(&req);
                        let finalized = finalize_response(response, &id);
                        let _ = tx_clone.send(finalized).await;
                        return;
                    }

                    // For everything else, we NEED the context
                    let Some(ctx) = ctx_clone else {
                         // Wait a bit more if it's still initializing?
                         let err = json!({"error": {"code": -32000, "message": "Server initializing"}});
                         let finalized = finalize_response(err, &id);
                         let _ = tx_clone.send(finalized).await;
                         return;
                    };

                    ctx.watchdog.heartbeat();

                    let response = match method.as_str() {
                        "tools/list" => handle_tools_list_with_ctx(&ctx, mode),
                        "tools/call" => handle_tools_call(&req, &ctx, mode).await,
                        _ => json!({"error": {"code": -32601, "message": format!("Method not found: {}", method)}}),
                    };

                    let finalized = finalize_response(response, &id);
                    let _ = tx_clone.send(finalized).await;
                });
            }

            // Bridge events if we have a context now
            if let (Some(ref mut rx), Some(ctx)) = (rx_events.as_mut(), shared_ctx.as_ref()) {
                while let Ok(event) = rx.try_recv() {
                    if let Some(msg) = system_event_to_notification(ctx.next_event_envelope(event)) {
                        let _ = tx_events_out.send(msg).await;
                    }
                }
            }
        }

        // Wait for all request handlers to complete
        while handlers.join_next().await.is_some() {}
        // DROP the events handle correctly
        // events_handle.abort(); // Actually I removed it earlier from source

        // Drop the last transmitter so the writer task's receiver can terminate
        drop(tx);

        // Wait for all output to be flushed
        let _ = writer_handle.await;

        // Ensure we HAVE the final context for the freeze hook
        if shared_ctx.is_none() {
            if let Some(rx) = ctx_rx {
                if let Ok(ctx) = rx.await {
                    shared_ctx = Some(ctx);
                }
            }
        }

        // --- FREEZE HOOK ---
        if let Some(ctx) = shared_ctx {
            log::info!("EOF detected. Triggering freeze hook...");
            let connections_guard = ctx.connections.lock().await;
            let mut latest_by_pubkey: std::collections::HashMap<&str, (&ConnectionEntry, &str)> =
                std::collections::HashMap::new();
            for (token, entry) in connections_guard.iter() {
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

fn system_event_to_notification(envelope: SystemEventEnvelope) -> Option<Value> {
    match envelope.event.clone() {
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
                        "event_id": envelope.event_id,
                        "collaboration_id": envelope.collaboration_id,
                        "session_id": envelope.collaboration_id,
                        "ts_secs": envelope.ts_secs,
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
                        "event_id": envelope.event_id,
                        "collaboration_id": envelope.collaboration_id,
                        "session_id": envelope.collaboration_id,
                        "ts_secs": envelope.ts_secs,
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
                stats.insert("event_id".to_string(), json!(envelope.event_id));
                stats.insert("collaboration_id".to_string(), json!(envelope.collaboration_id));
                stats.insert("session_id".to_string(), json!(envelope.collaboration_id));
                stats.insert("ts_secs".to_string(), json!(envelope.ts_secs));
                return Some(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": stats
                }));
            }
            Some(json!({
                "jsonrpc": "2.0",
                "method": "notifications/system_event",
                "params": envelope
            }))
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "method": "notifications/system_event",
            "params": envelope
        })),
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

pub fn handle_tools_list_with_ctx(ctx: &EngineContext, mode: McpServerMode) -> Value {
    let mut tools = get_all_tools();
    tools.extend(dynamic_tool_entries(ctx));
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

fn dynamic_tool_entries(ctx: &EngineContext) -> Vec<Value> {
    let mut out = Vec::new();
    if let Ok(plugins) = ctx.tpe.list() {
        for record in plugins {
            let Some(tool) = record.tool else { continue };
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for param in &tool.parameters {
                properties.insert(
                    param.name.clone(),
                    json!({"type": json_type_for_kind(&param.kind), "description": param.description}),
                );
                if param.required {
                    required.push(Value::String(param.name.clone()));
                }
            }
            out.push(json!({
                "name": tool.tool_name,
                "description": tool.description.unwrap_or_else(|| "Installed signed tool plugin".to_string()),
                "inputSchema": {
                    "type": "object",
                    "properties": Value::Object(properties),
                    "required": required,
                },
                "annotations": {
                    "title": "Signed Tool Plugin",
                    "readOnlyHint": false,
                    "openWorldHint": false,
                },
                "x-curd": {
                    "source": "tool_plugin",
                    "package_id": record.package_id,
                    "agent_usage": tool.agent_usage,
                    "review_guidance": tool.review_guidance,
                    "downstream_impact": tool.downstream_impact,
                    "examples": tool.examples,
                }
            }));
        }
    }
    if let Ok(groups) = ctx.tge.list() {
        for group in groups {
            for tool in group.tools {
                out.push(json!({
                    "name": tool.name,
                    "description": tool.description.unwrap_or_else(|| format!("Adopted MCP tool from group '{}'", group.group_id)),
                    "inputSchema": tool.input_schema,
                    "annotations": {
                        "title": "Adopted MCP Tool",
                        "readOnlyHint": false,
                        "openWorldHint": true,
                    },
                    "x-curd": {
                        "source": "tool_group",
                        "group_id": group.group_id,
                        "group_source": "external_mcp",
                        "allow_tools": group.allow_tools,
                        "deny_tools": group.deny_tools,
                    }
                }));
            }
        }
    }
    out
}

fn json_type_for_kind(kind: &str) -> &'static str {
    let lower = kind.to_ascii_lowercase();
    if lower.contains("array") {
        "array"
    } else if lower.contains("bool") {
        "boolean"
    } else if lower.contains("int") || lower.contains("number") {
        "number"
    } else if lower.contains("object") || lower.contains("map") {
        "object"
    } else {
        "string"
    }
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
    } else if name == "session_open" || name == "connection_open" {
        handle_connection_open(tool_params, ctx).await
    } else if name == "session_verify" || name == "connection_verify" {
        handle_connection_verify(tool_params, ctx).await
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

pub async fn handle_connection_open(params: &Value, ctx: &EngineContext) -> Value {
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
    pending.insert(
        pubkey_hex.to_string(),
        PendingChallenge {
            nonce_hex: nonce_hex.clone(),
            created_at_secs: now_secs(),
        },
    );

    ctx.he.log(
        Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
        ctx.collaboration_id,
        Some(pubkey_hex.to_string()),
        None,
        "connection_open",
        params.clone(),
        json!({"status": "ok"}),
        None,
        None,
        true,
        None,
        None,
    );

    json!({
        "status": "ok",
        "nonce": nonce_hex
    })
}

pub async fn handle_connection_verify(params: &Value, ctx: &EngineContext) -> Value {
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

    let challenge = {
        let mut pending = ctx.pending_challenges.lock().await;
        match pending.remove(pubkey_hex) {
            Some(n) => n,
            None => return json!({"error": "No pending challenge found for this pubkey"}),
        }
    };
    if now_secs().saturating_sub(challenge.created_at_secs)
        > ctx.config.collaboration.session_challenge_ttl_secs
    {
        let out = json!({"error": "Challenge expired"});
        ctx.he.log(
            Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
            ctx.collaboration_id,
            Some(pubkey_hex.to_string()),
            None,
            "connection_verify",
            params.clone(),
            out.clone(),
            None,
            None,
            false,
            Some("Challenge expired".to_string()),
            None,
        );
        return out;
    }

    match crate::auth::IdentityManager::verify_signature(
        pubkey_hex,
        challenge.nonce_hex.as_bytes(),
        signature_hex,
    ) {
        Ok(true) => {
            // AUTHORIZATION CHECK: Ensure this pubkey is actually on the allowlist
            let auth_file = ctx.workspace_root.join(".curd").join("authorized_agents.json");
            let (agent_id, is_authorized) = if auth_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&auth_file)
                    && let Ok(authorized) =
                        serde_json::from_str::<std::collections::HashMap<String, String>>(&content)
                    {
                        let aid = authorized.iter().find(|(_, v)| *v == pubkey_hex).map(|(k, _)| k.clone());
                        match aid {
                            Some(id) => (id, true),
                            None => ("unknown".to_string(), false)
                        }
                    } else { ("unknown".to_string(), false) }
            } else if ctx.config.collaboration.require_authorized_agents_file {
                ("unknown".to_string(), false)
            } else {
                ("anonymous".to_string(), true)
            };

            if !is_authorized {
                let out = json!({"error": "Unauthorized: Agent public key not found in authorized_agents.json"});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    Some(pubkey_hex.to_string()),
                    None,
                    "connection_verify",
                    params.clone(),
                    out.clone(),
                    None,
                    None,
                    false,
                    Some("Unauthorized public key".to_string()),
                    None,
                );
                return out;
            }

            let auth_mgr = match crate::auth::IdentityManager::new() {
                Ok(m) => m,
                Err(e) => return json!({"error": format!("Failed to initialize IdentityManager: {}", e)}),
            };

            let connection_token = match auth_mgr.create_connection_token(&agent_id, pubkey_hex) {
                Ok(t) => t,
                Err(e) => return json!({"error": format!("Failed to create connection token: {}", e)}),
            };
            let (state, restored) = if let Some(frozen_state) =
                crate::auth::IdentityManager::thaw_session(&ctx.workspace_root, pubkey_hex)
            {
                (frozen_state, true)
            } else {
                (ReplState::new(), false)
            };
            let entry = ConnectionEntry {
                agent_id: agent_id.clone(),
                pubkey_hex: pubkey_hex.to_string(),
                state,
                budget: crate::context::ConnectionBudget::default(),
                last_touched_secs: now_secs(),
            };
            let mut connections = ctx.connections.lock().await;
            connections.insert(connection_token.clone(), entry);
            drop(connections);
            let _ = ctx.tx_events.send(SystemEvent::ConnectionAuthenticated {
                agent_id: agent_id.clone(),
                pubkey_hex: pubkey_hex.to_string(),
                restored_state: restored,
            });

            let out = json!({
                "status": "authenticated",
                "connection_token": connection_token,
                "session_token": connection_token,
                "agent_id": agent_id,
                "restored_state": restored,
                "config_hash": ctx.config.compute_hash()
            });            ctx.he.log(
                Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                ctx.collaboration_id,
                Some(pubkey_hex.to_string()),
                None,
                "connection_verify",
                params.clone(),
                out.clone(),
                None,
                None,
                true,
                None,
                None,
            );
            out
        }
        Ok(false) | Err(_) => {
            let out = json!({"error": "Invalid signature. Authentication failed."});
            ctx.he.log(
                Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                ctx.collaboration_id,
                Some(pubkey_hex.to_string()),
                None,
                "connection_verify",
                params.clone(),
                out.clone(),
                None,
                None,
                false,
                Some("Invalid signature".to_string()),
                None,
            );
            out
        }
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

    // Check for authenticated connection
    let connection_token = params
        .get("connection_token")
        .or_else(|| params.get("session_token"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if connection_token.is_empty() {
        return json!({"error": "Unauthorized: connection_token is required for execute_dsl"});
    }

    let mut local_state = {
        let mut connections_guard = ctx.connections.lock().await;
        if let Some(entry) = connections_guard.get_mut(connection_token) {
            entry.state.is_executing_plan = true;
            ReplState::from_variables(entry.state.variables.clone())
        } else {
            return json!({"error": "Unauthorized: Invalid or expired connection_token."});
        }
    };

    let res = ctx.ple.execute_dsl(&nodes, ctx, &mut local_state).await;
    
    let mut connections_guard = ctx.connections.lock().await;
    if let Some(entry) = connections_guard.get_mut(connection_token) {
        entry.state.is_executing_plan = false;
        match res {
            Ok(results) => {
                entry.state.variables.extend(local_state.variables);
                entry.last_touched_secs = now_secs();
                let val = json!({"status": "ok", "results": results});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    None, // agent_id
                    None, // transaction_id
                    "dsl",
                    json!(nodes),
                    val.clone(),
                    None, // base_hash
                    None, // post_hash
                    true,
                    None,
                    None, // verification_result
                );
                val
            }
            Err(e) => {
                let err_msg = e.to_string();
                let val = json!({"error": err_msg.clone()});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    None, // agent_id
                    None, // transaction_id
                    "dsl",
                    json!(nodes),
                    val.clone(),
                    None, // base_hash
                    None, // post_hash
                    false,
                    Some(err_msg),
                    None, // verification_result
                );
                val
            }
        }
    } else {
        json!({"error": "Connection lost during DSL execution"})
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

    let connection_token = params
        .get("connection_token")
        .or_else(|| params.get("session_token"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if connection_token.is_empty() {
        return json!({"error": "Unauthorized: connection_token is required for execute_plan"});
    }

    let mut local_state = {
        let mut connections_guard = ctx.connections.lock().await;
        if let Some(entry) = connections_guard.get_mut(connection_token) {
            entry.state.is_executing_plan = true;
            ReplState::from_variables(entry.state.variables.clone())
        } else {
            return json!({"error": "Unauthorized: Invalid or expired connection_token."});
        }
    };

    let res = ctx.ple.execute_plan(&plan, ctx, &mut local_state).await;
    
    let mut connections_guard = ctx.connections.lock().await;
    if let Some(entry) = connections_guard.get_mut(connection_token) {
        entry.state.is_executing_plan = false;
        match res {
            Ok(ref results) => {
                entry.state.variables.extend(local_state.variables);
                entry.last_touched_secs = now_secs();
                let val = json!({"status": "ok", "results": results});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    None, // agent_id
                    None, // transaction_id
                    "plan",
                    json!(plan),
                    val.clone(),
                    None, // base_hash
                    None, // post_hash
                    true,
                    None,
                    None, // verification_result
                );
                val
            }
            Err(ref e) => {
                let err_msg = e.to_string();
                let val = json!({"error": err_msg.clone()});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    None, // agent_id
                    None, // transaction_id
                    "plan",
                    json!(plan),
                    val.clone(),
                    None, // base_hash
                    None, // post_hash
                    false,
                    Some(err_msg),
                    None, // verification_result
                );
                val
            }
        }
    } else {
        json!({"error": "Connection lost during plan execution"})
    }
}

async fn handle_history(params: &Value, ctx: &EngineContext) -> Value {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let mode = params.get("mode").and_then(|v| v.as_str()).unwrap_or("operations");
    match mode {
        "operations" => {
            let history = ctx.he.get_history(limit);
            json!({"status": "ok", "mode": "operations", "history": history})
        }
        "contributions" => {
            let history = ctx.ce.get_history(limit);
            json!({"status": "ok", "mode": "contributions", "history": history})
        }
        "checkpoints" => {
            let checkpoints = ctx.ce.get_checkpoints(limit);
            json!({"status": "ok", "mode": "checkpoints", "checkpoints": checkpoints})
        }
        "verify_contributions" => {
            let verification = ctx.ce.verify_chain();
            json!({"status": "ok", "mode": "verify_contributions", "verification": verification})
        }
        _ => json!({"error": format!("unsupported history mode: {}", mode)}),
    }
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
            "name": "connection_open",
            "description": "Request a cryptographic challenge to open a new authenticated connection.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pubkey_hex": {"type": "string", "description": "The Ed25519 public key in hex format"}
                },
                "required": ["pubkey_hex"]
            }
        }),
        json!({
            "name": "connection_verify",
            "description": "Verify the cryptographic challenge to receive a connection token.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pubkey_hex": {"type": "string"},
                    "signature_hex": {"type": "string", "description": "Hex signature of the nonce provided by connection_open"}
                },
                "required": ["pubkey_hex", "signature_hex"]
            }
        }),
        json!({
            "name": "session_open",
            "description": "Deprecated alias for connection_open.",
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
            "description": "Deprecated alias for connection_verify.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pubkey_hex": {"type": "string"},
                    "signature_hex": {"type": "string", "description": "Hex signature of the nonce provided by connection_open"}
                },
                "required": ["pubkey_hex", "signature_hex"]
            }
        }),
        json!({
            "name": "search",
            "description": "Instant, SQLite-backed Semantic Graph search. Queries an in-memory mapped database of the entire workspace's AST symbols, returning results in ~10ms even on 200k+ file repos like Chromium. \n\nCRITICAL INSTRUCTION: You MUST use this AST-aware CURD tool instead of standard OS tools like `grep`, `find`, or `rg`. It guarantees zero syntax errors, prevents context loss, operates instantly without spawning shell subprocesses, and is mathematically deterministic.",
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
            "description": "Read files, functions, or classes by URI directly from the Semantic Graph. \n\nCRITICAL INSTRUCTION: You MUST use this AST-aware CURD tool instead of standard OS tools like `cat`, `head`, or `less`. It is aware of exact AST boundaries, ensuring you never truncate a symbol mid-logic. It is significantly faster and safer than arbitrary shell commands.",
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
            "description": "Create, replace, or delete a function or module section via the AST-aware EditEngine. \n\nCRITICAL INSTRUCTION: You MUST use this AST-aware CURD tool instead of standard OS tools like `sed`, `awk`, or `echo`. It operates within a transactional ShadowStore, guaranteeing zero syntax errors, preventing broken builds, and automatically generating architectural changelogs.",
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
            "description": "Query the SQLite-backed Universal Semantic Linker. Provides instant O(1) 'middle-access' to any symbol's blast radius across the entire codebase. Automatically resolves cross-language boundaries (e.g., Rust `extern` to C++ definitions, TS `declare` to JS impls) using a unified role-based model. Vastly superior to running local Language Servers.",
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
            "description": "Execute a shell command safely within the workspace sandbox. \n\nCRITICAL INSTRUCTION: When building or testing, ALWAYS prefer running `curd build <target>` or `curd test` inside this shell instead of raw commands. (e.g. `curd build release`). This uses predefined tasks in [build.tasks] from settings.toml. CURD's wrappers automatically capture semantic backtraces and propagate them back to you directly. Set is_background=true for long-running processes to receive a task_id for later status/termination.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {"type": "string"},
                    "action": {"type": "string", "enum": ["execute", "status", "terminate"], "description": "Default is execute."},
                    "is_background": {"type": "boolean", "description": "If true, returns a task_id immediately."},
                    "task_id": {"type": "string", "description": "Required for action='status' or 'terminate'."}
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "shell_status",
            "description": "Check the status and get buffered output of a background shell task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": {"type": "string", "description": "The unique UUID of the background task."}
                },
                "required": ["task_id"]
            }
        }),
        json!({
            "name": "terminate",
            "description": "Terminate a long-running background shell task using its task_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": {"type": "string", "description": "The unique UUID of the background task."}
                },
                "required": ["task_id"]
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
                    "debug_session_id": {"type": "integer"},
                    "session_id": {"type": "integer", "description": "Deprecated alias for debug_session_id"}
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "session",
            "description": "Deprecated alias for review_cycle. Manage review-cycle baselines.",
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
            "name": "review_cycle",
            "description": "Manage review-cycle baselines and change reviews.",
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
            "name": "plugin_tool",
            "description": "Install, remove, or list signed proprietary tool plugins (.curdt). Installed tool plugins can extend CURD with sandboxed JSON-stdio handlers and ship agent-facing usage/docs metadata.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["add", "remove", "list"]},
                    "archive_path": {"type": "string"},
                    "package_id": {"type": "string"}
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "plugin_language",
            "description": "Install, remove, or list signed language ecosystem plugins (.curdl). These extend language parsing/build/LSP/debug capability through verified native plugin packages.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["add", "remove", "list"]},
                    "archive_path": {"type": "string"},
                    "package_id": {"type": "string"}
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "tool_group",
            "description": "Adopt or remove an external MCP toolset as a CURD-managed ToolGroup. This lets CURD expose tools from a pre-existing MCP server without rewriting that server.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["add_mcp", "remove", "list"]},
                    "group_id": {"type": "string"},
                    "command": {"type": "string"},
                    "args": {"type": "array", "items": {"type": "string"}},
                    "description": {"type": "string"}
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "plugin_trust",
            "description": "Manage trusted signing keys for signed CURD plugin packages. Keys added here govern which .curdt and .curdl archives can be installed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["add", "remove", "list"]},
                    "key_id": {"type": "string"},
                    "pubkey_hex": {"type": "string"},
                    "allowed_kinds": {"type": "array", "items": {"type": "string", "enum": ["tool", "language"]}}
                },
                "required": ["action"]
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
                    "connection_token": {"type": "string", "description": "Token obtained from connection_verify"},
                    "session_token": {"type": "string", "description": "Deprecated alias for connection_token"},
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
                    "connection_token": {"type": "string", "description": "Token obtained from connection_verify"},
                    "session_token": {"type": "string", "description": "Deprecated alias for connection_token"},
                    "plan": {"type": "object"}
                },
                "required": ["plan"]
            }
        }),
        json!({
            "name": "stamina",
            "description": "Check the remaining resource budget (stamina) for the current authenticated connection, including token consumption and time limits.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
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
        if !id.is_null() {
            obj.insert("id".to_string(), id.clone());
        }
        obj.insert("jsonrpc".to_string(), json!("2.0"));
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
        McpServerMode, handle_tools_call, handle_tools_list, handle_tools_list_with_ctx,
        system_event_to_notification,
    };
    use crate::context::EngineContext;
    use crate::plugin_packages::{InstalledPluginRecord, ToolDocExample, ToolDocParameter, ToolPluginSpec};
    use crate::plan::{SystemEvent, SystemEventEnvelope};
    use serde_json::{Value, json};
    use std::path::Path;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn build_test_ctx(root: &Path) -> EngineContext {
        let ctx_arc = EngineContext::new(root.to_str().unwrap());
        ctx_arc.clone_for_repl()
    }

    #[test]
    fn index_stall_event_maps_to_progress_notification() {
        let event = SystemEvent::NodeCompleted {
            node_id: Uuid::nil(),
            duration_ms: 0,
            summary: "IndexStall: 10/100 files no_progress_ms=16000".to_string(),
            artifact_path: None,
        };
        let notif = system_event_to_notification(SystemEventEnvelope {
            event_id: 1,
            collaboration_id: Uuid::nil(),
            ts_secs: 1,
            event,
        })
        .expect("stall notification");
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
        let notif = system_event_to_notification(SystemEventEnvelope {
            event_id: 2,
            collaboration_id: Uuid::nil(),
            ts_secs: 2,
            event,
        })
        .expect("stats notification");
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

    #[tokio::test]
    async fn tools_list_with_ctx_includes_dynamic_plugins_and_groups() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        let ctx = build_test_ctx(root);

        let plugin_dir = root.join(".curd/plugins/tool/demo-tool");
        std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
        std::fs::write(
            plugin_dir.join("installed.json"),
            serde_json::to_vec_pretty(&InstalledPluginRecord {
                package_id: "demo-tool".to_string(),
                version: "0.1.0".to_string(),
                kind: crate::PluginKind::Tool,
                signer_pubkey_hex: "11".repeat(32),
                install_dir: plugin_dir.clone(),
                manifest_path: plugin_dir.join("manifest.json"),
                tool: Some(ToolPluginSpec {
                    tool_name: "demo_tool".to_string(),
                    executable_path: "bin/demo-tool".to_string(),
                    default_args: Vec::new(),
                    protocol: "json_stdio_v1".to_string(),
                    agent_usage: Some("Use for demo requests.".to_string()),
                    review_guidance: Some("Review changed ids.".to_string()),
                    downstream_impact: Some("Affects demo pipeline.".to_string()),
                    description: Some("Demo plugin tool".to_string()),
                    parameters: vec![ToolDocParameter {
                        name: "query".to_string(),
                        kind: "string".to_string(),
                        description: "Query text".to_string(),
                        required: true,
                    }],
                    examples: vec![ToolDocExample {
                        label: "basic".to_string(),
                        arguments: json!({"query": "hello"}),
                    }],
                }),
                language: None,
            })
            .expect("installed.json"),
        )
        .expect("write installed plugin");

        std::fs::create_dir_all(root.join(".curd/tool_groups")).expect("tool groups dir");
        std::fs::write(
            root.join(".curd/tool_groups/registry.json"),
            serde_json::to_vec_pretty(&json!({
                "groups": [{
                    "group_id": "foreign",
                    "source": "external_mcp",
                    "command": "/bin/echo",
                    "args": [],
                    "allow_tools": ["foreign_tool"],
                    "deny_tools": [],
                    "tools": [{
                        "name": "foreign_tool",
                        "description": "Foreign tool",
                        "input_schema": {"type":"object","properties":{"query":{"type":"string"}}}
                    }]
                }]
            }))
            .expect("registry json"),
        )
        .expect("write registry");

        let listed = handle_tools_list_with_ctx(&ctx, McpServerMode::Full);
        let tools = listed["result"]["tools"].as_array().expect("tools array");
        let plugin_tool = tools
            .iter()
            .find(|tool| tool["name"].as_str() == Some("demo_tool"))
            .expect("plugin tool");
        assert_eq!(plugin_tool["x-curd"]["source"], "tool_plugin");
        assert_eq!(plugin_tool["x-curd"]["package_id"], "demo-tool");
        assert!(plugin_tool["x-curd"]["examples"].is_array());

        let group_tool = tools
            .iter()
            .find(|tool| tool["name"].as_str() == Some("foreign_tool"))
            .expect("group tool");
        assert_eq!(group_tool["x-curd"]["source"], "tool_group");
        assert_eq!(group_tool["x-curd"]["group_id"], "foreign");
        assert_eq!(group_tool["x-curd"]["allow_tools"][0], "foreign_tool");
    }
}
