use anyhow::Result;
use curd::router::route_validated_tool_call;
use curd_core::ReplState;
use serde_json::{Value, json};
use std::path::Path;
use uuid::Uuid;

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

pub async fn run_repl(workspace_root: &Path) -> Result<()> {
    let ctx = curd_core::EngineContext::new(&workspace_root.to_string_lossy());
    let mut state = ReplState::new();
    state.is_human_actor = true;

    let curd_dir = workspace_root.join(".curd");
    if !curd_dir.exists() {
        println!("\x1b[1;31m⚠️  WARNING: CURD is not initialized in this workspace.\x1b[0m");
        println!(
            "\x1b[2mRun 'ini' or 'curd init' to bootstrap the index and safety barriers.\x1b[0m\n"
        );
    }

    println!("\x1b[1;36m=== CURD Interactive REPL ===\x1b[0m");
    if ctx.read_only {
        println!(
            "\x1b[1;33m[READ-ONLY MODE] Another CURD session is active. Mutations are disabled.\x1b[0m"
        );
    }
    println!("Type 'help' for commands, or 'exit' to quit.");
    println!("You can use exact tool schemas, or simple space-separated args.");
    println!("Example: \x1b[33msearch query=\"Auth\" kind=\"class\"\x1b[0m");
    println!("Example: \x1b[33mgraph uris=[\"src/main.rs\"] depth=2\x1b[0m\n");

    let history_path = workspace_root.join(".curd").join("history");
    let mut rl = DefaultEditor::new()?;
    if history_path.exists() {
        let _ = rl.load_history(&history_path);
    }

    loop {
        let readline = rl.readline("\x1b[1;32mcurd>\x1b[0m ");
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(input);

                if input == "exit" || input == "quit" {
                    break;
                }

                if input == "help" {
                    println!("Available commands:");
                    println!(
                        "  search (s, sch), read (red), edit (e, edt), graph (g), diagram (dia)"
                    );
                    println!(
                        "  doctor (dct), shell (shl), workspace (wsp), diff (dif), index (idx)"
                    );
                    println!("  config (cfg), session (ses), plan (p, pln), test (tst)");
                    println!(
                        "  run <file.curd> [key=value ...] (Compile and execute a .curd script)"
                    );
                    println!(
                        "  run check <file.curd> [key=value ...]   (Show graph impact/conflicts)"
                    );
                    println!("  run compile <file.curd> [key=value ...] (Emit a plan artifact)");
                    println!("  detach                   (Soft detach CURD from this workspace)");
                    println!("  delete                   (Hard delete CURD from this workspace)");
                    println!("  let <var> = <tool> ...   (Variable assignment)");
                    println!("  print $<var>             (Inspect a variable)");
                    println!("  plan load <id>           (Load a saved plan into the REPL)");
                    println!(
                        "  plan edit <node_id> <json> (Modify a node in the active REPL plan)"
                    );
                    println!("  plan remove <id>         (Remove a plan node)");
                    println!("  plan graph               (Show the active plan's DAG)");
                    println!("  plan deny                (Delete the active plan from disk)");
                    println!("  plan impl --session <id> (Execute the active plan)");
                    println!("  config show              (Show current workspace configuration)");
                    println!("  config set <key> <val>   (Update a configuration value)");
                    continue;
                }

                // Handle Config command
                if input.starts_with("config") || input.starts_with("cfg") {
                    let parts: Vec<&str> = input.split_whitespace().collect();
                    let action = parts.get(1).unwrap_or(&"");

                    let mut cfg = curd_core::CurdConfig::load_from_workspace(&ctx.workspace_root);

                    match *action {
                        "show" => {
                            println!("{}", toml::to_string_pretty(&cfg)?);
                        }
                        "set" if parts.len() >= 4 => {
                            let key = parts[2];
                            let value = parts[3..].join(" ");

                            let mut json_val = serde_json::to_value(&cfg)?;
                            let key_parts: Vec<&str> = key.split('.').collect();
                            let mut current = &mut json_val;
                            let mut path_error = false;

                            for (i, part) in key_parts.iter().enumerate() {
                                if i == key_parts.len() - 1 {
                                    let parsed_val =
                                        serde_json::from_str::<serde_json::Value>(&value)
                                            .unwrap_or(serde_json::Value::String(value.clone()));
                                    current[part] = parsed_val;
                                } else {
                                    if current.get(part).is_none() {
                                        current[part] = serde_json::json!({});
                                    }
                                    let Some(next) = current.get_mut(part) else {
                                        println!(
                                            "\x1b[31mFailed to update configuration path: {}\x1b[0m",
                                            key
                                        );
                                        path_error = true;
                                        break;
                                    };
                                    current = next;
                                }
                            }

                            if path_error {
                                continue;
                            }

                            match serde_json::from_value(json_val) {
                                Ok(new_cfg) => {
                                    cfg = new_cfg;
                                    if let Err(e) = cfg.save_to_workspace() {
                                        println!("\x1b[31mFailed to save config: {}\x1b[0m", e);
                                    } else {
                                        println!(
                                            "\x1b[32mUpdated configuration key: {}\x1b[0m",
                                            key
                                        );
                                    }
                                }
                                Err(e) => {
                                    println!("\x1b[31mInvalid configuration value: {}\x1b[0m", e)
                                }
                            }
                        }
                        _ => {
                            println!("Usage: config show | config set <key> <value>");
                        }
                    }
                    continue;
                }

                if let Some(stripped) = input.strip_prefix("run ") {
                    let parts = split_repl_args(stripped);
                    let Some(first) = parts.first() else {
                        println!("\x1b[31mUsage: run <file.curd> [key=value ...]\x1b[0m");
                        continue;
                    };
                    let (mode, script, arg_slice) = match first.as_str() {
                        "check" | "compile" => {
                            let Some(script) = parts.get(1) else {
                                println!(
                                    "\x1b[31mUsage: run {} <file.curd> [key=value ...]\x1b[0m",
                                    first
                                );
                                continue;
                            };
                            (first.as_str(), script.as_str(), &parts[2..])
                        }
                        _ => ("run", first.as_str(), &parts[1..]),
                    };
                    match parse_script_arg_overrides(arg_slice) {
                        Ok(arg_overrides) => match mode {
                            "check" => {
                                let res = curd::router::route_check_script(
                                    std::path::Path::new(script),
                                    &arg_overrides,
                                    &ctx,
                                )
                                .await;
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&res).unwrap_or_default()
                                );
                            }
                            "compile" => {
                                let res = curd::router::route_compile_script(
                                    std::path::Path::new(script),
                                    &arg_overrides,
                                    &json!({}),
                                    &ctx,
                                )
                                .await;
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&res).unwrap_or_default()
                                );
                            }
                            _ => {
                                let (res, new_state) = curd::router::route_run_script(
                                    std::path::Path::new(script),
                                    &arg_overrides,
                                    &json!({}),
                                    &ctx,
                                    Some(state.clone()),
                                )
                                .await;
                                state = new_state;
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&res).unwrap_or_default()
                                );
                            }
                        },
                        Err(e) => println!("\x1b[31m{}\x1b[0m", e),
                    }
                    continue;
                }

                // Handle Index command explicitly for the REPL
                if input.starts_with("index") {
                    let parts: Vec<&str> = input.splitn(2, ' ').collect();
                    let arg_str = parts.get(1).unwrap_or(&"");

                    let mut mode = None;
                    for token in arg_str.split_whitespace() {
                        if let Some(eq_idx) = token.find('=') {
                            let key = &token[..eq_idx];
                            let val = token[eq_idx + 1..].trim_matches('"');
                            if key == "mode" {
                                mode = Some(val.to_string());
                            }
                        }
                    }

                    let mut cfg = curd_core::CurdConfig::load_from_workspace(&ctx.workspace_root);
                    if let Some(m) = mode {
                        cfg.index.mode = Some(m);
                    }

                    let se = curd_core::SearchEngine::new(&ctx.workspace_root).with_config(cfg);
                    println!("\x1b[2mStarting indexing operation...\x1b[0m");
                    let start = std::time::Instant::now();
                    match se.search("", None) {
                        Ok(results) => {
                            let duration = start.elapsed();
                            println!("\x1b[32mIndexing complete in {:?}.\x1b[0m", duration);
                            println!("Total Symbols Indexed: {}", results.len());
                            if let Some(stats) = se.last_index_stats() {
                                println!("  Files Scanned: {}", stats.total_files);
                                println!("  Cache Hits:    {}", stats.cache_hits);
                                println!("  Parse Fail:    {}", stats.parse_fail);
                            }
                        }
                        Err(e) => println!("\x1b[31mIndexing failed: {}\x1b[0m", e),
                    }
                    continue;
                }

                if input == "detach" {
                    println!("\x1b[2mPerforming soft detach...\x1b[0m");
                    let outcome = crate::workspace_lifecycle::resolve_workspace_exit(
                        &ctx.workspace_root,
                        "detach",
                        None,
                        false,
                    )?;
                    println!("{}", outcome.message);
                    if !outcome.proceeded {
                        continue;
                    }
                    crate::workspace_lifecycle::cleanup_detach_artifacts(&ctx.workspace_root);
                    println!(
                        "\x1b[32mCURD workspace soft-detached. Local `.curd/` data is preserved.\x1b[0m"
                    );
                    continue;
                }

                if input.starts_with("delete") {
                    let confirmation = if input.contains("-y") {
                        true
                    } else {
                        dialoguer::Confirm::new()
                            .with_prompt("WARNING: This will permanently delete your local `.curd/` directory, history, and shadow index. Are you sure?")
                            .default(false)
                            .interact()
                            .unwrap_or(false)
                    };

                    if confirmation {
                        println!(
                            "\x1b[2mCleaning hooks and forcefully removing `.curd/`...\x1b[0m"
                        );
                        let outcome = crate::workspace_lifecycle::resolve_workspace_exit(
                            &ctx.workspace_root,
                            "delete",
                            None,
                            false,
                        )?;
                        println!("{}", outcome.message);
                        if !outcome.proceeded {
                            continue;
                        }
                        crate::workspace_lifecycle::cleanup_detach_artifacts(&ctx.workspace_root);

                        // Delete data
                        let curd_dir = ctx.workspace_root.join(".curd");
                        if curd_dir.exists() {
                            if let Err(e) = std::fs::remove_dir_all(&curd_dir) {
                                println!("\x1b[31mFailed to delete CURD: {}\x1b[0m", e);
                            } else {
                                println!(
                                    "\x1b[32mSuccessfully deleted CURD from workspace.\x1b[0m"
                                );
                            }
                        } else {
                            println!("CURD is not initialized in this workspace.");
                        }
                        // Break out of the REPL since the directory and config are gone
                        break;
                    } else {
                        println!("Aborted.");
                    }
                    continue;
                }

                // Handle Plan Management for the active in-memory plan.
                if let Some(stripped) = input.strip_prefix("plan ") {
                    let sub_parts: Vec<&str> = stripped.splitn(2, ' ').collect();
                    let plan_cmd = sub_parts[0];
                    let plan_args = sub_parts.get(1).unwrap_or(&"");

                    match plan_cmd {
                        "load" => {
                            let id = plan_args.trim();
                            if let Ok(plan_uuid) = Uuid::parse_str(id) {
                                match ctx.ple.get_plan(plan_uuid) {
                                    Ok(plan) => {
                                        state.active_plan = Some(plan);
                                        println!("\x1b[32mPlan {} loaded into REPL.\x1b[0m", id);
                                    }
                                    Err(e) => println!("\x1b[31mError loading plan: {}\x1b[0m", e),
                                }
                            } else {
                                println!("\x1b[31mInvalid UUID format.\x1b[0m");
                            }
                        }
                        "graph" => {
                            if let Some(plan) = &state.active_plan {
                                println!("Dependency Graph for Plan {}:", plan.id);
                                for node in &plan.nodes {
                                    let label = match &node.op {
                                        curd_core::plan::ToolOperation::McpCall {
                                            tool, ..
                                        } => tool.clone(),
                                        curd_core::plan::ToolOperation::Internal {
                                            command,
                                            ..
                                        } => format!("internal:{}", command),
                                    };
                                    print!("  [{}] {}", node.id, label);
                                    if !node.dependencies.is_empty() {
                                        print!("  <-- depends on: ");
                                        let deps: Vec<String> = node
                                            .dependencies
                                            .iter()
                                            .map(|d| match d {
                                                curd_core::plan::IdOrTag::Id(id) => id.to_string(),
                                                curd_core::plan::IdOrTag::Tag(t) => {
                                                    format!("tag:{}", t)
                                                }
                                            })
                                            .collect();
                                        print!("{}", deps.join(", "));
                                    }
                                    println!();
                                }
                            } else {
                                println!("\x1b[31mNo active plan loaded.\x1b[0m");
                            }
                        }
                        "edit" => {
                            if let Some(plan) = &mut state.active_plan {
                                let edit_parts: Vec<&str> = plan_args.splitn(2, ' ').collect();
                                if edit_parts.len() == 2 {
                                    if let Ok(node_id) = Uuid::parse_str(edit_parts[0]) {
                                        if let Some(node) =
                                            plan.nodes.iter_mut().find(|n| n.id == node_id)
                                        {
                                            if let Ok(new_op) = serde_json::from_str::<
                                                curd_core::plan::ToolOperation,
                                            >(
                                                edit_parts[1]
                                            ) {
                                                node.op = new_op;
                                                ctx.ple.save_plan(plan).ok();
                                                println!(
                                                    "\x1b[32mNode {} updated and plan saved.\x1b[0m",
                                                    node_id
                                                );
                                            } else {
                                                println!(
                                                    "\x1b[31mInvalid ToolOperation JSON.\x1b[0m"
                                                );
                                            }
                                        } else {
                                            println!(
                                                "\x1b[31mNode ID not found in active plan.\x1b[0m"
                                            );
                                        }
                                    } else {
                                        println!("\x1b[31mInvalid Node UUID.\x1b[0m");
                                    }
                                } else {
                                    println!("\x1b[31mUsage: plan edit <node_id> <json>\x1b[0m");
                                }
                            } else {
                                println!("\x1b[31mNo active plan loaded.\x1b[0m");
                            }
                        }
                        "remove" => {
                            if let Some(plan) = &mut state.active_plan {
                                if let Ok(node_id) = Uuid::parse_str(plan_args.trim()) {
                                    plan.nodes.retain(|n| n.id != node_id);
                                    ctx.ple.save_plan(plan).ok();
                                    println!(
                                        "\x1b[32mNode {} removed and plan saved.\x1b[0m",
                                        node_id
                                    );
                                } else {
                                    println!("\x1b[31mInvalid Node UUID.\x1b[0m");
                                }
                            } else {
                                println!("\x1b[31mNo active plan loaded.\x1b[0m");
                            }
                        }
                        "deny" => {
                            if let Some(plan) = state.active_plan.take() {
                                ctx.ple.delete_plan(plan.id).ok();
                                println!("\x1b[32mPlan {} deleted.\x1b[0m", plan.id);
                            } else {
                                println!("\x1b[31mNo active plan loaded to deny.\x1b[0m");
                            }
                        }
                        "impl" => {
                            if let Some(plan) = state.active_plan.clone() {
                                // Very minimal extraction of session ID for REPL implementation
                                if plan_args.contains("--session ") {
                                    let session_id =
                                        plan_args.replace("--session ", "").trim().to_string();
                                    if let Ok(session_uuid) = Uuid::parse_str(&session_id) {
                                        // Update ctx with the session ID
                                        let mut local_ctx = ctx.clone_for_repl();
                                        local_ctx.collaboration_id = session_uuid;
                                        match local_ctx
                                            .ple
                                            .execute_plan(&plan, &local_ctx, &mut state)
                                            .await
                                        {
                                            Ok(res) => println!(
                                                "Plan completed:\n{}",
                                                serde_json::to_string_pretty(&res)
                                                    .unwrap_or_default()
                                            ),
                                            Err(e) => {
                                                println!("\x1b[31mExecution failed: {}\x1b[0m", e)
                                            }
                                        }
                                    } else {
                                        println!("\x1b[31mInvalid session UUID.\x1b[0m");
                                    }
                                } else {
                                    println!("\x1b[31mUsage: plan impl --session <uuid>\x1b[0m");
                                }
                            } else {
                                println!("\x1b[31mNo active plan loaded.\x1b[0m");
                            }
                        }
                        _ => println!("\x1b[31mUnknown plan command: {}\x1b[0m", plan_cmd),
                    }
                    continue;
                }

                // Handle Variable Assignment: `let x = search ...`
                let mut target_var = None;
                let mut cmd_str = input;

                if let Some(stripped) = input.strip_prefix("let ") {
                    if let Some(eq_idx) = stripped.find('=') {
                        let var_name = stripped[..eq_idx].trim().to_string();
                        target_var = Some(var_name);
                        cmd_str = stripped[eq_idx + 1..].trim();
                    }
                } else if let Some(stripped) = input.strip_prefix("print ") {
                    let var_name = stripped.trim();
                    let resolved = state.resolve(&json!(var_name));
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&resolved).unwrap_or_default()
                    );
                    continue;
                }

                // Parse tool and args
                let parts: Vec<&str> = cmd_str.splitn(2, ' ').collect();
                let mut tool = parts[0];
                let arg_str = parts.get(1).unwrap_or(&"");

                // Alias Resolution
                tool = match tool {
                    "s" | "sch" => "search",
                    "red" => "read",
                    "e" | "edt" => "edit",
                    "g" => "graph",
                    "dia" => "diagram",
                    "dct" => "doctor",
                    "shl" => "shell",
                    "wsp" => "workspace",
                    "dif" => "diff",
                    "idx" => "index",
                    "cfg" => "config",
                    "ses" => "session",
                    "p" | "pln" => "plan",
                    "tst" => "test",
                    _ => tool,
                };

                // Handle Test command explicitly
                if tool == "test" {
                    let mut cmd = std::process::Command::new(
                        std::env::current_exe().unwrap_or_else(|_| "curd".into()),
                    );
                    cmd.arg("test");
                    for arg in split_repl_args(arg_str) {
                        cmd.arg(arg);
                    }
                    cmd.arg("--root").arg(&ctx.workspace_root);
                    let _ = cmd.status();
                    continue;
                }

                // Heuristic Parser: Convert 'key=value' and 'key=[a,b]' to JSON
                let mut args_map = serde_json::Map::new();

                // Set default format to ascii for diagrams in the REPL
                if tool == "diagram" {
                    args_map.insert("format".to_string(), json!("ascii"));
                }

                // Very basic tokenizer for REPL convenience
                let mut current_key = String::new();
                let mut in_array = false;
                let mut array_buf = String::new();

                for token in arg_str.split_whitespace() {
                    if in_array {
                        array_buf.push(' ');
                        array_buf.push_str(token);
                        if token.ends_with(']') {
                            in_array = false;
                            if let Ok(arr) = serde_json::from_str::<Value>(&array_buf) {
                                args_map.insert(current_key.clone(), arr);
                            }
                        }
                    } else if let Some(eq_idx) = token.find('=') {
                        let key = &token[..eq_idx];
                        let val = &token[eq_idx + 1..];
                        if val.starts_with('[') && !val.ends_with(']') {
                            current_key = key.to_string();
                            array_buf = val.to_string();
                            in_array = true;
                        } else if let Ok(json_val) = serde_json::from_str::<Value>(val) {
                            args_map.insert(key.to_string(), json_val);
                        } else {
                            args_map.insert(key.to_string(), json!(val.trim_matches('"')));
                        }
                    } else {
                        // Positional mapping fallback based on tool
                        match tool {
                            "search" => {
                                args_map
                                    .insert("query".to_string(), json!(token.trim_matches('"')));
                            }
                            "read" => {
                                args_map
                                    .insert("uris".to_string(), json!([token.trim_matches('"')]));
                            }
                            "graph" => {
                                args_map
                                    .insert("uris".to_string(), json!([token.trim_matches('"')]));
                            }
                            "lsp" => {
                                args_map
                                    .insert("uris".to_string(), json!([token.trim_matches('"')]));
                            }
                            "diagram" => {
                                args_map
                                    .insert("uris".to_string(), json!([token.trim_matches('"')]));
                            }
                            "shell" => {
                                args_map.insert("command".to_string(), json!(arg_str));
                                break;
                            }
                            _ => {}
                        }
                    }
                }

                let mut args_val = Value::Object(args_map);

                // Resolve variables in arguments before calling tool
                args_val = state.resolve(&args_val);

                println!(
                    "\x1b[2mExecuting: {} {}\x1b[0m",
                    tool,
                    serde_json::to_string(&args_val).unwrap_or_default()
                );

                let res = route_validated_tool_call(tool, &args_val, &ctx, true).await;

                if let Some(var) = target_var {
                    state.variables.insert(var.clone(), res.clone());
                    println!("\x1b[32mSaved to variable ${}\x1b[0m", var);
                } else {
                    // High-quality rendering for Diagrams and Graphs
                    if let Some(diag_val) = res
                        .get("diagram")
                        .and_then(|d| d.get("diagram"))
                        .and_then(|v| v.as_str())
                    {
                        println!("{}", diag_val);
                    } else if tool == "graph"
                        && res.get("status").and_then(|v| v.as_str()) == Some("ok")
                    {
                        if let Some(nodes) = res
                            .get("graph")
                            .and_then(|g| g.get("nodes"))
                            .and_then(|v| v.as_array())
                        {
                            if nodes.is_empty() {
                                println!("\x1b[31mResult: Symbol(s) not found in index.\x1b[0m");
                            } else {
                                println!("\n\x1b[1;36m┌── Symbol Graph Summary\x1b[0m");
                                for node in nodes {
                                    let id = node
                                        .get("id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown");
                                    let kind =
                                        node.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                                    if kind == "?" {
                                        println!("│  \x1b[31m•\x1b[0m [Not Found] {}", id);
                                    } else {
                                        println!("│  \x1b[33m•\x1b[0m [{}] {}", kind, id);
                                    }
                                }
                                println!("\x1b[1;36m└───────────────────────────\x1b[0m");
                                println!(
                                    "\x1b[2mHint: Use 'diagram uris=[\"...\"] format=\"ascii\"' for a visual tree.\x1b[0m\n"
                                );
                            }
                        }
                    } else {
                        // Pretty print output with basic coloring for errors
                        if res.get("error").is_some() {
                            println!("\x1b[31mError:\x1b[0m");
                        }
                        println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("\x1b[31mREPL Error: {:?}\x1b[0m", err);
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path);
    Ok(())
}

fn split_repl_args(arg_str: &str) -> Vec<String> {
    arg_str.split_whitespace().map(str::to_string).collect()
}

fn parse_script_arg_overrides(args: &[String]) -> anyhow::Result<serde_json::Map<String, Value>> {
    let mut map = serde_json::Map::new();
    for item in args {
        let Some((key, raw)) = item.split_once('=') else {
            anyhow::bail!("invalid script arg '{}'; expected key=value", item);
        };
        let parsed =
            serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
        map.insert(key.to_string(), parsed);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::{parse_script_arg_overrides, split_repl_args};
    use serde_json::json;

    #[test]
    fn split_repl_args_preserves_flags_as_individual_args() {
        assert_eq!(
            split_repl_args("all --verbose"),
            vec!["all".to_string(), "--verbose".to_string()]
        );
    }

    #[test]
    fn parse_script_arg_overrides_parses_json_literals() {
        let parsed = parse_script_arg_overrides(&[
            "strict=true".to_string(),
            "limit=5".to_string(),
            "target=\"src/lib.rs::alpha\"".to_string(),
        ])
        .expect("script args should parse");
        assert_eq!(parsed.get("strict"), Some(&json!(true)));
        assert_eq!(parsed.get("limit"), Some(&json!(5)));
        assert_eq!(parsed.get("target"), Some(&json!("src/lib.rs::alpha")));
    }
}
