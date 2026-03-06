use anyhow::Result;
use curd_core::{ReplState, dispatch_tool};
use serde_json::{Value, json};
use std::io::{self, Write};
use std::path::Path;
use uuid::Uuid;

pub async fn run_repl(workspace_root: &Path) -> Result<()> {
    let ctx = curd_core::EngineContext::new(&workspace_root.to_string_lossy());
    let mut state = ReplState::new();

    println!("\x1b[1;36m=== CURD Interactive REPL ===\x1b[0m");
    if ctx.read_only {
        println!("\x1b[1;33m[READ-ONLY MODE] Another CURD session is active. Mutations are disabled.\x1b[0m");
    }
    println!("Type 'help' for commands, or 'exit' to quit.");
    println!("You can use exact tool schemas, or simple space-separated args.");
    println!("Example: \x1b[33msearch query=\"Auth\" kind=\"class\"\x1b[0m");
    println!("Example: \x1b[33mgraph uris=[\"src/main.rs\"] depth=2\x1b[0m\n");

    loop {
        print!("\x1b[1;32mcurd>\x1b[0m ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input == "exit" || input == "quit" {
            break;
        }

        if input == "help" {
            println!("Available commands:");
            println!("  search, read, edit, graph, diagram, lsp, doctor, shell, workspace, diff, index");
            println!("  let <var> = <tool> ...   (Variable assignment)");
            println!("  print $<var>             (Inspect a variable)");
            println!("  plan load <id>           (Load a plan into the REPL)");
            println!("  plan edit <id> <json>    (Modify a plan node)");
            println!("  plan remove <id>         (Remove a plan node)");
            println!("  plan graph               (Show the active plan's DAG)");
            println!("  plan deny                (Delete the active plan from disk)");
            println!("  plan impl --session <id> (Execute the active plan)");
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

        // Handle Plan Management: `plan load <id>`, `plan edit <id> <json>`, etc.
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
                                curd_core::plan::ToolOperation::McpCall { tool, .. } => tool.clone(),
                                curd_core::plan::ToolOperation::Internal { command, .. } => format!("internal:{}", command),
                            };
                            print!("  [{}] {}", node.id, label);
                            if !node.dependencies.is_empty() {
                                print!("  <-- depends on: ");
                                let deps: Vec<String> = node.dependencies.iter().map(|d| match d {
                                    curd_core::plan::IdOrTag::Id(id) => id.to_string(),
                                    curd_core::plan::IdOrTag::Tag(t) => format!("tag:{}", t),
                                }).collect();
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
                                if let Some(node) = plan.nodes.iter_mut().find(|n| n.id == node_id) {
                                    if let Ok(new_op) = serde_json::from_str::<curd_core::plan::ToolOperation>(edit_parts[1]) {
                                        node.op = new_op;
                                        ctx.ple.save_plan(plan).ok();
                                        println!("\x1b[32mNode {} updated and plan saved.\x1b[0m", node_id);
                                    } else {
                                        println!("\x1b[31mInvalid ToolOperation JSON.\x1b[0m");
                                    }
                                } else {
                                    println!("\x1b[31mNode ID not found in active plan.\x1b[0m");
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
                            println!("\x1b[32mNode {} removed and plan saved.\x1b[0m", node_id);
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
                            let session_id = plan_args.replace("--session ", "").trim().to_string();
                            if let Ok(session_uuid) = Uuid::parse_str(&session_id) {
                                // Update ctx with the session ID
                                let mut local_ctx = ctx.clone_for_repl();
                                local_ctx.session_id = session_uuid;
                                match local_ctx.ple.execute_plan(&plan, &local_ctx, &mut state).await {
                                    Ok(res) => println!("Plan completed:\n{}", serde_json::to_string_pretty(&res).unwrap_or_default()),
                                    Err(e) => println!("\x1b[31mExecution failed: {}\x1b[0m", e),
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
            println!("{}", serde_json::to_string_pretty(&resolved).unwrap_or_default());
            continue;
        }

        // Parse tool and args
        let parts: Vec<&str> = cmd_str.splitn(2, ' ').collect();
        let tool = parts[0];
        let arg_str = parts.get(1).unwrap_or(&"");

        // Heuristic Parser: Convert 'key=value' and 'key=[a,b]' to JSON
        let mut args_map = serde_json::Map::new();
        
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
                    "search" => { args_map.insert("query".to_string(), json!(token.trim_matches('"'))); },
                    "read" => { args_map.insert("uris".to_string(), json!([token.trim_matches('"')])); },
                    "graph" => { args_map.insert("uris".to_string(), json!([token.trim_matches('"')])); },
                    "lsp" => { args_map.insert("uris".to_string(), json!([token.trim_matches('"')])); },
                    "diagram" => { args_map.insert("uris".to_string(), json!([token.trim_matches('"')])); },
                    "shell" => { args_map.insert("command".to_string(), json!(arg_str)); break; },
                    _ => {}
                }
            }
        }

        let mut args_val = Value::Object(args_map);
        
        // Resolve variables in arguments before calling tool
        args_val = state.resolve(&args_val);

        println!("\x1b[2mExecuting: {} {}\x1b[0m", tool, serde_json::to_string(&args_val).unwrap_or_default());
        
        let res = dispatch_tool(tool, &args_val, &ctx).await;

        if let Some(var) = target_var {
            state.variables.insert(var.clone(), res.clone());
            println!("\x1b[32mSaved to variable ${}\x1b[0m", var);
        } else {
            // Pretty print output with basic coloring for errors
            if res.get("error").is_some() {
                println!("\x1b[31mError:\x1b[0m");
            }
            println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
        }
    }

    Ok(())
}