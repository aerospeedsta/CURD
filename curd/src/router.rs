use crate::validation::validate_tool_call;
use curd_core::context::{ConnectionBudget, ConnectionEntry};
use curd_core::plan::now_secs;
use curd_core::symbols::SymbolRole;
use curd_core::{
    BuildRequest, DoctorIndexConfig, DoctorProfile, DoctorThresholds, DslNode, EngineContext,
    GraphEngine, Plan, RefactorAction, ReplState, SearchEngine, ShadowStore,
    collect_compiled_script_targets, compile_curd_script, compile_curd_script_to_plan,
    compiled_script_requires_shadow_session, dispatch_tool, parse_curd_script,
    recommend_script_safeguards,
};
use serde_json::{Value, json};
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

fn inherit_scope_fields(args: &mut Value, scope: &Value) {
    let Some(scope_obj) = scope.as_object() else {
        return;
    };
    let Some(args_obj) = args.as_object_mut() else {
        return;
    };
    for key in [
        "profile",
        "connection_token",
        "session_token",
        "actor_id",
        "disclosure_level",
    ] {
        if args_obj.get(key).is_none()
            && let Some(value) = scope_obj.get(key)
        {
            args_obj.insert(key.to_string(), value.clone());
        }
    }
}

fn inject_scope_into_dsl_nodes(nodes: &mut [DslNode], scope: &Value) {
    for node in nodes {
        match node {
            DslNode::Call { args, .. } => inherit_scope_fields(args, scope),
            DslNode::Atomic { nodes } => inject_scope_into_dsl_nodes(nodes, scope),
            DslNode::Assign { value, .. } => {
                if let Some(obj) = value.as_object_mut()
                    && obj.contains_key("tool")
                    && obj.contains_key("args")
                    && let Some(args) = obj.get_mut("args")
                {
                    inherit_scope_fields(args, scope);
                }
            }
            DslNode::Abort { .. } => {}
        }
    }
}

fn inject_scope_into_plan(plan: &mut Plan, scope: &Value) {
    for node in &mut plan.nodes {
        if let curd_core::plan::ToolOperation::McpCall { args, .. } = &mut node.op {
            inherit_scope_fields(args, scope);
        }
    }
}

fn tool_requires_shadow_session(tool: &str) -> bool {
    matches!(
        tool,
        "edit"
            | "manage_file"
            | "mutate"
            | "proposal"
            | "refactor"
            | "shell"
            | "build"
            | "execute_plan"
            | "execute_active_plan"
            | "execute_dsl"
    )
}

fn dsl_requires_shadow_session(nodes: &[DslNode]) -> bool {
    compiled_script_requires_shadow_session(nodes)
}

fn plan_requires_shadow_session(plan: &Plan) -> bool {
    plan.nodes.iter().any(|node| match &node.op {
        curd_core::plan::ToolOperation::McpCall { tool, .. } => tool_requires_shadow_session(tool),
        curd_core::plan::ToolOperation::Internal { command, .. } => command == "clear_shadow",
    })
}

pub async fn route_tool_call(name: &str, params: &Value, ctx: &EngineContext) -> Value {
    dispatch_tool(name, params, ctx).await
}

pub async fn route_validated_tool_call(
    name: &str,
    params: &Value,
    ctx: &EngineContext,
    is_human: bool,
) -> Value {
    match validate_tool_call(ctx, name, params, is_human) {
        Ok(_) => route_tool_call(name, params, ctx).await,
        Err(err) => {
            let message = err
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Validation failed")
                .to_string();
            let details = err
                .get("error")
                .and_then(|v| v.get("details"))
                .cloned()
                .unwrap_or(Value::Null);
            json!({
                "error": message,
                "details": details
            })
        }
    }
}

pub async fn route_run_script(
    script_path: &Path,
    arg_overrides: &serde_json::Map<String, Value>,
    params: &Value,
    ctx: &EngineContext,
    initial_state: Option<ReplState>,
) -> (Value, ReplState) {
    let resolved_script_path = if script_path.is_absolute() {
        script_path.to_path_buf()
    } else {
        ctx.workspace_root.join(script_path)
    };
    if resolved_script_path.extension().and_then(|v| v.to_str()) != Some("curd") {
        let mut state = initial_state.unwrap_or_default();
        state.is_human_actor = true;
        return (
            json!({"error": format!("script {} must use the .curd extension", resolved_script_path.display())}),
            state,
        );
    }

    let source = match std::fs::read_to_string(&resolved_script_path) {
        Ok(source) => source,
        Err(e) => {
            let mut state = initial_state.unwrap_or_default();
            state.is_human_actor = true;
            return (
                json!({"error": format!("failed to read script {}: {}", resolved_script_path.display(), e)}),
                state,
            );
        }
    };

    let parsed = match parse_curd_script(&source) {
        Ok(parsed) => parsed,
        Err(e) => {
            let mut state = initial_state.unwrap_or_default();
            state.is_human_actor = true;
            return (
                json!({"error": format!("failed to parse {}: {}", resolved_script_path.display(), e)}),
                state,
            );
        }
    };

    let compiled = match compile_curd_script(&parsed, arg_overrides) {
        Ok(compiled) => compiled,
        Err(e) => {
            let mut state = initial_state.unwrap_or_default();
            state.is_human_actor = true;
            return (
                json!({"error": format!("failed to compile {}: {}", resolved_script_path.display(), e)}),
                state,
            );
        }
    };

    let token = format!("local-script-{}", Uuid::new_v4());
    let mut seeded_state = initial_state.unwrap_or_default();
    seeded_state.is_human_actor = true;
    {
        let mut guard = ctx.connections.lock().await;
        guard.insert(
            token.clone(),
            ConnectionEntry {
                agent_id: "local_human".to_string(),
                pubkey_hex: String::new(),
                state: seeded_state.clone(),
                budget: ConnectionBudget::default(),
                last_touched_secs: now_secs(),
            },
        );
    }

    let mut scope = params.clone();
    if let Some(obj) = scope.as_object_mut() {
        obj.insert("connection_token".to_string(), json!(token.clone()));
        obj.insert("session_token".to_string(), json!(token.clone()));
        let profile_missing = match obj.get("profile") {
            None => true,
            Some(value) => value.is_null(),
        };
        if profile_missing && let Some(profile) = &compiled.metadata.profile {
            obj.insert("profile".to_string(), json!(profile));
        }
    } else {
        scope = json!({
            "connection_token": token.clone(),
            "session_token": token.clone(),
            "profile": compiled.metadata.profile,
        });
    }
    if let Some(obj) = scope.as_object_mut() {
        obj.insert("nodes".to_string(), json!(compiled.nodes));
    }

    let result = route_execute_dsl(&scope, ctx).await;
    let state = {
        let mut guard = ctx.connections.lock().await;
        guard
            .remove(&token)
            .map(|entry| entry.state)
            .unwrap_or_else(|| {
                let mut state = ReplState::new();
                state.is_human_actor = true;
                state
            })
    };
    (result, state)
}

pub async fn route_check_script(
    script_path: &Path,
    arg_overrides: &serde_json::Map<String, Value>,
    ctx: &EngineContext,
) -> Value {
    let resolved_script_path = if script_path.is_absolute() {
        script_path.to_path_buf()
    } else {
        ctx.workspace_root.join(script_path)
    };
    let source = match std::fs::read_to_string(&resolved_script_path) {
        Ok(source) => source,
        Err(e) => {
            return json!({"error": format!("failed to read script {}: {}", resolved_script_path.display(), e)});
        }
    };
    let parsed = match parse_curd_script(&source) {
        Ok(parsed) => parsed,
        Err(e) => {
            return json!({"error": format!("failed to parse {}: {}", resolved_script_path.display(), e)});
        }
    };
    let compiled = match compile_curd_script(&parsed, arg_overrides) {
        Ok(compiled) => compiled,
        Err(e) => {
            return json!({"error": format!("failed to compile {}: {}", resolved_script_path.display(), e)});
        }
    };

    let session_required = dsl_requires_shadow_session(&compiled.nodes);
    let mutation_targets = collect_compiled_script_targets(&compiled.nodes);
    let mut target_reports = Vec::new();
    for target in &mutation_targets {
        match ctx.ge.graph(vec![target.clone()], "both", 1) {
            Ok(graph) => {
                let result = graph
                    .get("results")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .cloned()
                    .unwrap_or(Value::Null);
                target_reports.push(json!({
                    "target": target,
                    "resolved": result.get("function_id").cloned().unwrap_or(Value::Null),
                    "callers": result.get("callers").cloned().unwrap_or_else(|| json!([])),
                    "callees": result.get("callees").cloned().unwrap_or_else(|| json!([])),
                    "edge_summary": graph.get("edge_summary").cloned().unwrap_or(Value::Null)
                }));
            }
            Err(e) => {
                target_reports.push(json!({"target": target, "error": e.to_string()}));
            }
        }
    }

    let mut direct_conflicts = Vec::new();
    for i in 0..target_reports.len() {
        for j in (i + 1)..target_reports.len() {
            let a = &target_reports[i];
            let b = &target_reports[j];
            let a_resolved = a.get("resolved").and_then(|v| v.as_str()).unwrap_or("");
            let b_resolved = b.get("resolved").and_then(|v| v.as_str()).unwrap_or("");
            let a_callers = a
                .get("callers")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let a_callees = a
                .get("callees")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let directly_connected = a_resolved == b_resolved
                || a_callers.iter().any(|v| v.as_str() == Some(b_resolved))
                || a_callees.iter().any(|v| v.as_str() == Some(b_resolved));
            if directly_connected {
                direct_conflicts.push(json!({
                    "left": a.get("target").cloned().unwrap_or(Value::Null),
                    "right": b.get("target").cloned().unwrap_or(Value::Null),
                    "reason": "targets are identical or directly connected in the graph"
                }));
            }
        }
    }

    let mut suggested_safeguards = recommend_script_safeguards(session_required, &mutation_targets);
    if !direct_conflicts.is_empty() {
        suggested_safeguards.push(
            "split directly connected mutations into separate atomic blocks or reviewed plan variants"
                .to_string(),
        );
        suggested_safeguards.push(
            "add an explicit review gate before promoting a compiled plan artifact".to_string(),
        );
    }
    suggested_safeguards.sort();
    suggested_safeguards.dedup();

    json!({
        "status": if direct_conflicts.is_empty() { "ok" } else { "caution" },
        "script": resolved_script_path,
        "profile": parsed.metadata.profile,
        "explainability": parsed.explainability,
        "session_required": session_required,
        "compiled_nodes": compiled.nodes.len(),
        "mutation_targets": mutation_targets,
        "target_reports": target_reports,
        "direct_conflicts": direct_conflicts,
        "suggested_safeguards": suggested_safeguards
    })
}

pub async fn route_compile_script(
    script_path: &Path,
    arg_overrides: &serde_json::Map<String, Value>,
    params: &Value,
    ctx: &EngineContext,
) -> Value {
    let resolved_script_path = if script_path.is_absolute() {
        script_path.to_path_buf()
    } else {
        ctx.workspace_root.join(script_path)
    };
    let source = match std::fs::read_to_string(&resolved_script_path) {
        Ok(source) => source,
        Err(e) => {
            return json!({"error": format!("failed to read script {}: {}", resolved_script_path.display(), e)});
        }
    };
    let parsed = match parse_curd_script(&source) {
        Ok(parsed) => parsed,
        Err(e) => {
            return json!({"error": format!("failed to parse {}: {}", resolved_script_path.display(), e)});
        }
    };
    let mut artifact = match compile_curd_script_to_plan(&parsed, arg_overrides) {
        Ok(artifact) => artifact,
        Err(e) => {
            return json!({"error": format!("failed to compile {}: {}", resolved_script_path.display(), e)});
        }
    };
    if let Some(profile) = params.get("profile").and_then(|v| v.as_str()) {
        artifact.metadata.profile = Some(profile.to_string());
    }
    artifact.source_path = Some(resolved_script_path.to_string_lossy().to_string());
    artifact.runtime_ceiling = Some(
        crate::validation::active_runtime_ceiling(ctx)
            .as_str()
            .to_string(),
    );
    let out_path = params
        .get("out")
        .and_then(|v| v.as_str())
        .map(|p| {
            let path = Path::new(p);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                ctx.workspace_root.join(path)
            }
        })
        .unwrap_or_else(|| {
            ctx.workspace_root
                .join(".curd")
                .join("plans")
                .join(format!("{}.json", artifact.plan.id))
        });
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(
        &out_path,
        serde_json::to_string_pretty(&artifact).unwrap_or_else(|_| "{}".to_string()),
    ) {
        return json!({"error": format!("failed to write plan artifact {}: {}", out_path.display(), e)});
    }
    json!({
        "status": "ok",
        "out": out_path,
        "plan_id": artifact.plan.id,
        "node_count": artifact.plan.nodes.len(),
        "explainability": artifact.explainability,
        "arg_bindings": artifact.arg_bindings,
        "safeguards": artifact.safeguards,
        "runtime_ceiling": artifact.runtime_ceiling
    })
}

pub async fn route_execute_dsl(params: &Value, ctx: &EngineContext) -> Value {
    let mut nodes: Vec<DslNode> = match params
        .get("nodes")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
    {
        Some(n) => n,
        None => return json!({"error": "Invalid or missing 'nodes' for execute_dsl"}),
    };

    let connection_token = params
        .get("connection_token")
        .or_else(|| params.get("session_token"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if connection_token.is_empty() {
        return json!({"error": "Unauthorized: connection_token is required for execute_dsl"});
    }

    inject_scope_into_dsl_nodes(&mut nodes, params);
    if dsl_requires_shadow_session(&nodes)
        && !ctx.we.shadow.lock().map(|s| s.is_active()).unwrap_or(false)
    {
        return json!({
            "error": "SESSION_REQUIRED: execute_dsl payload contains mutating/runtime steps and requires an active workspace session (workspace begin)."
        });
    }

    let mut local_state = {
        let mut connections_guard = ctx.connections.lock().await;
        if let Some(entry) = connections_guard.get_mut(connection_token) {
            entry.state.is_executing_plan = true;
            entry.state.clone()
        } else {
            return json!({"error": "Unauthorized: Invalid or expired connection_token."});
        }
    };

    let res = ctx.ple.execute_dsl(&nodes, ctx, &mut local_state).await;

    let mut connections_guard = ctx.connections.lock().await;
    if let Some(entry) = connections_guard.get_mut(connection_token) {
        entry.state = local_state.clone();
        entry.state.is_executing_plan = false;
        match res {
            Ok(results) => {
                entry.last_touched_secs = now_secs();
                let val = json!({"status": "ok", "results": results});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    None,
                    None,
                    "dsl",
                    json!(nodes),
                    val.clone(),
                    None,
                    None,
                    true,
                    None,
                    None,
                );
                val
            }
            Err(e) => {
                let err_msg = e.to_string();
                let val = json!({"error": err_msg.clone()});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    None,
                    None,
                    "dsl",
                    json!(nodes),
                    val.clone(),
                    None,
                    None,
                    false,
                    Some(err_msg),
                    None,
                );
                val
            }
        }
    } else {
        json!({"error": "Connection lost during DSL execution"})
    }
}

pub async fn route_execute_plan(params: &Value, ctx: &EngineContext) -> Value {
    let mut plan: Plan = match params
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

    inject_scope_into_plan(&mut plan, params);
    if plan_requires_shadow_session(&plan)
        && !ctx.we.shadow.lock().map(|s| s.is_active()).unwrap_or(false)
    {
        return json!({
            "error": "SESSION_REQUIRED: execute_plan payload contains mutating/runtime steps and requires an active workspace session (workspace begin)."
        });
    }

    let mut local_state = {
        let mut connections_guard = ctx.connections.lock().await;
        if let Some(entry) = connections_guard.get_mut(connection_token) {
            entry.state.is_executing_plan = true;
            entry.state.clone()
        } else {
            return json!({"error": "Unauthorized: Invalid or expired connection_token."});
        }
    };

    let res = ctx.ple.execute_plan(&plan, ctx, &mut local_state).await;

    let mut connections_guard = ctx.connections.lock().await;
    if let Some(entry) = connections_guard.get_mut(connection_token) {
        entry.state = local_state.clone();
        entry.state.is_executing_plan = false;
        match res {
            Ok(ref results) => {
                entry.last_touched_secs = now_secs();
                let val = json!({"status": "ok", "results": results});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    None,
                    None,
                    "plan",
                    json!(plan),
                    val.clone(),
                    None,
                    None,
                    true,
                    None,
                    None,
                );
                val
            }
            Err(ref e) => {
                let err_msg = e.to_string();
                let val = json!({"error": err_msg.clone()});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    None,
                    None,
                    "plan",
                    json!(plan),
                    val.clone(),
                    None,
                    None,
                    false,
                    Some(err_msg),
                    None,
                );
                val
            }
        }
    } else {
        json!({"error": "Connection lost during plan execution"})
    }
}

pub async fn route_history(params: &Value, ctx: &EngineContext) -> Value {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("operations");
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

pub async fn route_batch(params: &Value, ctx: &EngineContext) -> Value {
    let tasks = params
        .get("tasks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let inherited_profile = params
        .get("profile")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
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

    let sorted_indices = match petgraph::algo::toposort(&graph, None) {
        Ok(indices) => indices,
        Err(_) => return json!({"error": "Cycle detected in batch tasks"}),
    };

    for idx in sorted_indices {
        let task_idx = graph[idx];
        let task = &tasks[task_idx];
        let id = task.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
        let tool = task.get("tool").and_then(|v| v.as_str()).unwrap_or("");
        let mut task_args = task.get("args").cloned().unwrap_or(default_args.clone());
        if let Some(profile) = &inherited_profile
            && task_args.get("profile").is_none()
            && let Some(obj) = task_args.as_object_mut()
        {
            obj.insert("profile".to_string(), json!(profile));
        }

        let res = route_validated_tool_call(tool, &task_args, ctx, false).await;
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

pub async fn route_workspace_status(ctx: &EngineContext) -> Value {
    let config = curd_core::config::CurdConfig::load_from_workspace(&ctx.workspace_root);
    let recent_index_run =
        curd_core::storage::read_recent_index_runs(&ctx.workspace_root, &config, 1)
            .ok()
            .and_then(|runs| runs.into_iter().next());

    let shadow = ShadowStore::new(&ctx.workspace_root);
    let staged_paths = shadow
        .staged_paths()
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();

    json!({
        "status": "ok",
        "index": recent_index_run,
        "shadow": {
            "active": shadow.is_active(),
            "staged_paths": staged_paths
        }
    })
}

pub async fn route_semantic_audit(params: &Value, ctx: &EngineContext) -> Value {
    let verbose = params
        .get("verbose")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let scope = params
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("all");

    let se = SearchEngine::new(&ctx.workspace_root);
    let ge = GraphEngine::new(&ctx.workspace_root);

    let symbols = match se.get_all_symbols() {
        Ok(symbols) => symbols,
        Err(e) => return json!({"error": format!("Failed to index symbols for audit: {}", e)}),
    };
    let graph = match ge.build_dependency_graph_fresh() {
        Ok(graph) => graph,
        Err(e) => return json!({"error": format!("Failed to build graph for audit: {}", e)}),
    };
    let report = graph.calculate_integrity(&symbols);

    let node_coverage = if scope == "all" || scope == "nodes" {
        let dead_zones = if verbose {
            let indexed_files: std::collections::HashSet<String> = symbols
                .iter()
                .map(|s| s.filepath.to_string_lossy().to_string())
                .collect();
            curd_core::workspace::scan_workspace(&ctx.workspace_root)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|file| {
                    let rel = file
                        .strip_prefix(&ctx.workspace_root)
                        .unwrap_or(&file)
                        .to_string_lossy()
                        .to_string();
                    if !indexed_files.contains(&rel)
                        && !rel.contains(".curd")
                        && !rel.contains("target/")
                    {
                        Some(rel)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        Some(json!({
            "symbols_indexed": symbols.len(),
            "symbol_density": report.symbol_density,
            "dead_zones": dead_zones
        }))
    } else {
        None
    };

    let edge_connectivity = if scope == "all" || scope == "edges" {
        let broken = if verbose && report.total_stubs > report.resolved_stubs {
            symbols
                .iter()
                .filter(|s| {
                    s.role == SymbolRole::Stub
                        && (!graph.outgoing.contains_key(&s.id) || graph.outgoing[&s.id].is_empty())
                })
                .map(|s| s.id.clone())
                .take(10)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        Some(json!({
            "cohesion_ratio": report.cohesion_ratio,
            "broken_links": report.total_stubs.saturating_sub(report.resolved_stubs),
            "resolved_stubs": report.resolved_stubs,
            "total_stubs": report.total_stubs,
            "confidence_distribution": report.confidence_distribution,
            "top_unresolved_linkages": broken
        }))
    } else {
        None
    };

    let architecture_audit = if scope == "all" || scope == "architecture" {
        Some(json!({
            "cycles": report.cycles
        }))
    } else {
        None
    };

    let policy_validation = if scope == "all" || scope == "policy" {
        let cfg = curd_core::config::CurdConfig::load_from_workspace(&ctx.workspace_root);
        let pe = curd_core::PolicyEngine::new(cfg.policy);
        let test_params = json!({"path": ".curd/settings.toml"});
        let decision = pe.evaluate("edit", &test_params, false, &ctx.workspace_root);
        let (pass, reason) = match decision {
            curd_core::policy::PolicyDecision::Deny(reason) => (true, Some(reason)),
            _ => (false, None),
        };
        Some(json!({
            "pass": pass,
            "reason": reason
        }))
    } else {
        None
    };

    json!({
        "status": "ok",
        "scope": scope,
        "node_coverage": node_coverage,
        "edge_connectivity": edge_connectivity,
        "architecture_audit": architecture_audit,
        "policy_validation": policy_validation
    })
}

pub async fn route_session_lifecycle(action: &str, ctx: &EngineContext) -> Value {
    match action {
        "begin" => {
            let workspace =
                route_validated_tool_call("workspace", &json!({"action":"begin"}), ctx, true).await;
            if workspace.get("error").is_some() {
                return json!({"error": workspace.get("error").cloned().unwrap_or(Value::Null)});
            }

            let review =
                route_validated_tool_call("session", &json!({"action":"begin"}), ctx, true).await;
            if review.get("error").is_some() {
                return json!({
                    "error": review.get("error").cloned().unwrap_or(Value::Null),
                    "workspace": workspace
                });
            }

            json!({
                "status": "ok",
                "workspace": workspace,
                "review_cycle": review
            })
        }
        "commit" => {
            let workspace = route_validated_tool_call(
                "workspace",
                &json!({"action":"commit","allow_unapproved":true}),
                ctx,
                true,
            )
            .await;
            if workspace.get("error").is_some() {
                return json!({"error": workspace.get("error").cloned().unwrap_or(Value::Null)});
            }

            let review =
                route_validated_tool_call("session", &json!({"action":"end"}), ctx, true).await;

            json!({
                "status": "ok",
                "workspace": workspace,
                "review_cycle": review
            })
        }
        "rollback" => {
            let workspace =
                route_validated_tool_call("workspace", &json!({"action":"rollback"}), ctx, true)
                    .await;
            if workspace.get("error").is_some() {
                return json!({"error": workspace.get("error").cloned().unwrap_or(Value::Null)});
            }

            let review =
                route_validated_tool_call("session", &json!({"action":"end"}), ctx, true).await;

            json!({
                "status": "ok",
                "workspace": workspace,
                "review_cycle": review
            })
        }
        _ => json!({"error": format!("unsupported session lifecycle action: {}", action)}),
    }
}

pub async fn route_config_command(params: &Value, ctx: &EngineContext) -> Value {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("show");
    let mut cfg = curd_core::CurdConfig::load_from_workspace(&ctx.workspace_root);
    let config_path = cfg.source_path.clone().unwrap_or_else(|| {
        curd_core::workspace::get_curd_dir(&ctx.workspace_root).join("curd.toml")
    });
    cfg.source_path = Some(config_path.clone());

    match action {
        "show" => match toml::to_string_pretty(&cfg) {
            Ok(rendered) => json!({"status": "ok", "config_toml": rendered}),
            Err(e) => json!({"error": format!("Failed to render config: {}", e)}),
        },
        "set" => {
            let Some(key) = params.get("key").and_then(|v| v.as_str()) else {
                return json!({"error": "config set requires key"});
            };
            let Some(value) = params.get("value").and_then(|v| v.as_str()) else {
                return json!({"error": "config set requires value"});
            };

            let mut json_val = match serde_json::to_value(&cfg) {
                Ok(value) => value,
                Err(e) => return json!({"error": format!("Failed to encode config: {}", e)}),
            };
            let parts: Vec<&str> = key.split('.').collect();
            let mut current = &mut json_val;

            for (i, part) in parts.iter().enumerate() {
                if i == parts.len() - 1 {
                    let parsed_val = serde_json::from_str::<serde_json::Value>(value)
                        .unwrap_or(serde_json::Value::String(value.to_string()));
                    current[part] = parsed_val;
                } else {
                    if current.get(part).is_none() {
                        current[part] = json!({});
                    }
                    let Some(next) = current.get_mut(part) else {
                        return json!({"error": format!("Failed to navigate config path at '{}'", part)});
                    };
                    if !next.is_object() {
                        return json!({"error": format!("Config path '{}' is not an object; cannot set nested key '{}'", part, key)});
                    }
                    current = next;
                }
            }

            cfg = match serde_json::from_value(json_val) {
                Ok(cfg) => cfg,
                Err(e) => {
                    return json!({"error": format!("Failed to decode updated config: {}", e)});
                }
            };
            cfg.source_path = Some(config_path.clone());
            match cfg.save_to_workspace() {
                Ok(_) => {
                    json!({"status": "ok", "message": format!("Updated configuration key: {}", key)})
                }
                Err(e) => json!({"error": format!("Failed to save config: {}", e)}),
            }
        }
        "unset" => {
            let Some(key) = params.get("key").and_then(|v| v.as_str()) else {
                return json!({"error": "config unset requires key"});
            };
            let mut json_val = match serde_json::to_value(&cfg) {
                Ok(value) => value,
                Err(e) => return json!({"error": format!("Failed to encode config: {}", e)}),
            };
            let parts: Vec<&str> = key.split('.').collect();
            let mut current = &mut json_val;

            for (i, part) in parts.iter().enumerate() {
                if i == parts.len() - 1 {
                    if let Some(obj) = current.as_object_mut() {
                        obj.remove(*part);
                    }
                } else if let Some(next) = current.get_mut(part) {
                    current = next;
                } else {
                    break;
                }
            }

            cfg = match serde_json::from_value(json_val) {
                Ok(cfg) => cfg,
                Err(e) => {
                    return json!({"error": format!("Failed to decode updated config: {}", e)});
                }
            };
            cfg.source_path = Some(config_path.clone());
            match cfg.save_to_workspace() {
                Ok(_) => {
                    json!({"status": "ok", "message": format!("Removed configuration key: {}", key)})
                }
                Err(e) => json!({"error": format!("Failed to save config: {}", e)}),
            }
        }
        _ => json!({"error": format!("unsupported config action: {}", action)}),
    }
}

pub async fn route_context_command(params: &Value, ctx: &EngineContext) -> Value {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("list");
    let mut registry = curd_core::ContextRegistry::load(&ctx.workspace_root);

    match action {
        "add" => {
            let Some(path) = params.get("path").and_then(|v| v.as_str()) else {
                return json!({"error": "context add requires path"});
            };
            let alias = params.get("alias").and_then(|v| v.as_str());
            let index = params
                .get("index")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let read = params
                .get("read")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let mut ext_path = std::path::PathBuf::from(path);
            if !ext_path.is_absolute() {
                ext_path = std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(ext_path);
            }
            let ext_path = match std::fs::canonicalize(ext_path) {
                Ok(path) => path,
                Err(e) => return json!({"error": format!("Failed to resolve path: {}", e)}),
            };
            if !ext_path.exists() {
                return json!({"error": format!("Path does not exist: {}", ext_path.display())});
            }

            let mode = if index {
                curd_core::ContextMode::Index
            } else if read {
                curd_core::ContextMode::Read
            } else {
                curd_core::ContextMode::Write
            };
            let alias_name = alias.map(ToOwned::to_owned).unwrap_or_else(|| {
                format!(
                    "@{}",
                    ext_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("ext")
                )
            });
            registry.add(alias_name.clone(), ext_path.clone(), mode.clone());
            match registry.save(&ctx.workspace_root) {
                Ok(_) => json!({
                    "status": "ok",
                    "alias": alias_name,
                    "path": ext_path,
                    "mode": mode
                }),
                Err(e) => json!({"error": format!("Failed to save context registry: {}", e)}),
            }
        }
        "remove" => {
            let Some(name) = params.get("name").and_then(|v| v.as_str()) else {
                return json!({"error": "context remove requires name"});
            };
            let force = params
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let mut dangling = Vec::new();
            if !force {
                let graph = curd_core::GraphEngine::new(&ctx.workspace_root);
                let dep_graph = match graph.build_dependency_graph() {
                    Ok(graph) => graph,
                    Err(e) => {
                        return json!({"error": format!("Failed to build dependency graph: {}", e)});
                    }
                };
                for (caller, callees) in dep_graph.outgoing {
                    for callee in callees {
                        if callee.starts_with(name) {
                            dangling.push(json!({
                                "caller": caller,
                                "callee": callee
                            }));
                        }
                    }
                }
                if !dangling.is_empty() {
                    return json!({
                        "error": "Cannot remove context. Your primary workspace is actively calling functions in this repository. Use --force to override.",
                        "dangling_edges": dangling
                    });
                }
            }

            if registry.remove(name) {
                match registry.save(&ctx.workspace_root) {
                    Ok(_) => json!({"status": "ok", "removed": name}),
                    Err(e) => json!({"error": format!("Failed to save context registry: {}", e)}),
                }
            } else {
                json!({"status": "ok", "removed": Value::Null, "message": format!("Context `{}` not found.", name)})
            }
        }
        "list" => {
            let contexts = registry
                .contexts
                .iter()
                .map(|(alias, link)| {
                    json!({
                        "alias": alias,
                        "path": link.path,
                        "mode": link.mode
                    })
                })
                .collect::<Vec<_>>();
            json!({"status": "ok", "contexts": contexts})
        }
        _ => json!({"error": format!("unsupported context action: {}", action)}),
    }
}

pub async fn route_doctor_command(params: &Value, ctx: &EngineContext) -> Value {
    let strict = params
        .get("strict")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let profile = params
        .get("profile")
        .and_then(|v| v.as_str())
        .and_then(|s| DoctorProfile::from_str(s).ok());
    let thresholds = params
        .get("thresholds")
        .and_then(|v| serde_json::from_value::<DoctorThresholds>(v.clone()).ok())
        .unwrap_or_default();
    let index_config = params
        .get("index_config")
        .and_then(|v| serde_json::from_value::<DoctorIndexConfig>(v.clone()).ok())
        .unwrap_or_default();

    let engine = curd_core::DoctorEngine::new(&ctx.workspace_root);
    match engine.run(strict, thresholds, profile, index_config) {
        Ok(report) => json!({"status": "ok", "report": report}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn route_build_command(params: &Value, ctx: &EngineContext) -> Value {
    let req = BuildRequest {
        adapter: params
            .get("adapter")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        profile: params
            .get("profile")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        target: params
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        execute: params
            .get("execute")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        zig: params.get("zig").and_then(|v| v.as_bool()).unwrap_or(false),
        command: params
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        allow_untrusted: params
            .get("allow_untrusted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        trailing_args: params
            .get("trailing_args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };

    match curd_core::run_build(&ctx.workspace_root, req) {
        Ok(out) => json!({"status": out.status, "response": out}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn route_diff_command(params: &Value, ctx: &EngineContext) -> Value {
    let semantic = params
        .get("semantic")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let symbol = params
        .get("symbol")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match curd_core::run_diff(&ctx.workspace_root, semantic, symbol) {
        Ok(output) => json!({"status": "ok", "output": output}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

pub async fn route_refactor_command(action: RefactorAction, ctx: &EngineContext) -> Value {
    match curd_core::run_refactor(&ctx.workspace_root, action) {
        Ok(output) => json!({"status": "ok", "output": output}),
        Err(e) => json!({"error": e.to_string()}),
    }
}
