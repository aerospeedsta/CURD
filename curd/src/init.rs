use anyhow::Result;
use curd_core::auth::IdentityManager;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

fn get_home_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    } else {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

const PROPOSE_PLAN_SKILL_MD: &str = r#"---
name: propose-plan
description: Translates a natural language idea into a CURD Plan DAG for human audit.
---

# Propose Plan Skill

## Instructions
1. Use CURD semantic tools (`search`, `graph`, `read`) to gather technical context.
2. Construct a multi-step `Plan` (DAG) following the `curd-core` schema.
3. Write the resulting JSON to `.curd/plans/$(uuid).json`.
4. Respond to the user with the Plan UUID and instructions to review it in the `curd repl`.

## Constraints
- Do NOT execute the plan. Only register it.
- Dependencies must be topologically sound.
- Security Constraint: NEVER attempt to read, write, or access files inside the `.curd` directory via any tool, bash, or python scripts. The `.curd` directory contains sensitive state and keys.
"#;

const PROPOSE_PLAN_RULE_MDC: &str = r#"---
description: Use this rule when the user asks for a complex refactor or architectural change.
globs: ["**/*.rs", "**/*.py", "**/*.ts", "**/*.js", "**/*.cpp", "**/*.go"]
alwaysApply: false
---

# CURD Refactor Planning Rule

When a user proposes a large-scale change:
1. Do NOT execute it immediately.
2. Use `curd search` and `curd graph` to map the blast radius.
3. Generate a `plan.json` in the `.curd/plans/` directory.
4. Notify the user of the Plan UUID so they can audit it in the `curd repl`.

## Security Block
Do NOT under any circumstances attempt to read, write, list or execute operations on the `.curd/` directory using any tool, including native shell or Python interpreters. It contains agentic private keys and transactional system states!
"#;

pub fn init_agent(
    name_override: Option<&str>,
    harness_override: Option<&str>,
    workspace_root: &Path,
) -> Result<()> {
    let mut targets = Vec::new();

    // 1. Determine what to install
    let mut install_mcp = false;
    let mut install_skills = false;

    #[cfg(feature = "mcp")]
    {
        use std::io::IsTerminal;
        if std::io::stdout().is_terminal() {
            let options = vec![
                "Install MCP Server (Direct agent tool access)",
                "Install CURD Skills (Propose-Plan logic)",
            ];
            if let Ok(selections) = dialoguer::MultiSelect::new()
                .with_prompt("Select installation targets (Space to select, Enter to confirm):")
                .items(&options)
                .defaults(&[true, true])
                .interact()
            {
                for idx in selections {
                    if idx == 0 {
                        install_mcp = true;
                    }
                    if idx == 1 {
                        install_skills = true;
                    }
                }
            } else {
                install_mcp = true;
                install_skills = true;
            }
        } else {
            install_mcp = true;
            install_skills = true;
        }
    }
    #[cfg(not(feature = "mcp"))]
    {
        println!("CURD core build detected. Only Skill installation is available.");
        install_skills = true;
    }

    if !install_mcp && !install_skills {
        println!("Nothing selected for installation. Aborting.");
        return Ok(());
    }

    // 2. Resolve Harness Targets
    if let Some(h) = harness_override {
        targets = h.split(',').map(|s| s.trim().to_string()).collect();
    } else {
        let mut detected = Vec::new();
        if let Some(home) = get_home_dir() {
            if home.join(".gemini").exists() || workspace_root.join("GEMINI.md").exists() {
                detected.push("gemini".to_string());
            }

            // Copilot & Codex
            if home.join(".copilot").exists() || workspace_root.join(".copilot").exists() {
                detected.push("copilot".to_string());
            }
            if home.join(".codex").exists() || workspace_root.join(".codex").exists() {
                detected.push("codex".to_string());
            }

            if workspace_root.join(".cursor").exists()
                || home.join("Library/Application Support/Cursor").exists()
                || home.join("AppData/Roaming/Cursor").exists()
            {
                detected.push("cursor".to_string());
            }
            let claude_path = if cfg!(target_os = "macos") {
                home.join("Library/Application Support/Claude/claude_desktop_config.json")
            } else {
                PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
                    .join("Claude/claude_desktop_config.json")
            };
            if claude_path.exists() || workspace_root.join("CLAUDE.md").exists() {
                detected.push("claude_desktop".to_string());
            }
            if workspace_root.join(".mcp.json").exists() {
                detected.push("claude_code".to_string());
            }

            // Cline
            let cline_global = if cfg!(target_os = "macos") {
                home.join("Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json")
            } else if cfg!(target_os = "windows") {
                PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json")
            } else {
                home.join(".config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json")
            };
            if cline_global.exists() || workspace_root.join(".cline").exists() {
                detected.push("cline".to_string());
            }

            // Roo Code
            let roo_global = if cfg!(target_os = "macos") {
                home.join("Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json")
            } else if cfg!(target_os = "windows") {
                PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json")
            } else {
                home.join(".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/cline_mcp_settings.json")
            };
            if roo_global.exists() || workspace_root.join(".roo").exists() {
                detected.push("roo_code".to_string());
            }

            // Windsurf
            let windsurf_global = home.join(".codeium/windsurf/mcp_config.json");
            if windsurf_global.exists() {
                detected.push("windsurf".to_string());
            }

            // Zed
            let zed_global = if cfg!(target_os = "windows") {
                PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
                    .join("Zed/settings.json")
            } else {
                home.join(".config/zed/settings.json")
            };
            if zed_global.exists() || workspace_root.join(".zed").exists() {
                detected.push("zed".to_string());
            }
        }

        if detected.len() > 1 {
            use std::io::IsTerminal;
            if std::io::stdout().is_terminal() {
                println!("Multiple AI harnesses detected.");
                if let Ok(selections) = dialoguer::MultiSelect::new()
                    .with_prompt(
                        "Select which harnesses to configure (Space to select, Enter to confirm):",
                    )
                    .items(&detected)
                    .defaults(&vec![true; detected.len()])
                    .interact()
                {
                    for idx in selections {
                        targets.push(detected[idx].clone());
                    }
                } else {
                    targets = detected;
                }
            } else {
                targets = detected;
            }
        } else {
            targets = detected;
        }
    }

    if targets.is_empty() {
        println!("No AI harnesses detected or specified.");
        return Ok(());
    }

    let auth_manager = IdentityManager::new()?;
    let command_path = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("curd"))
        .to_string_lossy()
        .to_string();

    for harness in targets {
        // 3. Install MCP Server if selected
        if install_mcp {
            println!("Initializing MCP for harness: {}...", harness);
            let agent_name = name_override.unwrap_or(&harness);
            let (_, _priv_hex, pub_hex) = auth_manager.generate_keypair(agent_name)?;

            // Save public key
            let auth_file = workspace_root.join(".curd").join("authorized_agents.json");
            if let Some(parent) = auth_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            use std::collections::HashMap;
            let mut authorized: HashMap<String, String> = if auth_file.exists() {
                serde_json::from_str(&fs::read_to_string(&auth_file)?).unwrap_or_default()
            } else {
                HashMap::new()
            };
            authorized.insert(agent_name.to_string(), pub_hex);
            fs::write(&auth_file, serde_json::to_string_pretty(&authorized)?)?;

            let server_config = json!({
                "command": command_path.clone(),
                "args": ["mcp", workspace_root.to_string_lossy().to_string()],
                "env": {
                    "CURD_AGENT_ID": agent_name
                }
            });

            let (config_path_opt, block_key) = match harness.as_str() {
                "gemini" => (
                    Some(workspace_root.join(".gemini").join("settings.json")),
                    "mcpServers",
                ),
                "copilot" => (
                    Some(workspace_root.join(".copilot").join("mcp-config.json")),
                    "mcpServers",
                ),
                "codex" => (
                    Some(workspace_root.join(".codex").join("config.json")),
                    "mcpServers",
                ),
                "cursor" => (
                    Some(workspace_root.join(".cursor").join("mcp.json")),
                    "mcpServers",
                ),
                "claude_desktop" => (Some(workspace_root.join(".mcp.json")), "mcpServers"),
                "claude_code" => (Some(workspace_root.join(".mcp.json")), "mcpServers"),
                "cline" => (
                    Some(
                        workspace_root
                            .join(".cline")
                            .join("cline_mcp_settings.json"),
                    ),
                    "mcpServers",
                ),
                "roo_code" => (
                    Some(workspace_root.join(".roo").join("cline_mcp_settings.json")),
                    "mcpServers",
                ),
                "windsurf" => (
                    Some(workspace_root.join(".windsurf").join("mcp_config.json")),
                    "mcpServers",
                ),
                "zed" => (
                    Some(workspace_root.join(".zed").join("settings.json")),
                    "context_servers",
                ),
                _ => (None, ""),
            };

            if let Some(config_path) = config_path_opt {
                let server_name = if name_override.is_some() {
                    format!("curd_{}", agent_name)
                } else {
                    "curd".to_string()
                };
                if let Some(parent) = config_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let mut config_json = if config_path.exists() {
                    serde_json::from_str(&fs::read_to_string(&config_path)?).unwrap_or(json!({}))
                } else {
                    json!({})
                };

                if let Some(obj) = config_json.as_object_mut() {
                    if !obj.contains_key(block_key) {
                        obj.insert(block_key.to_string(), json!({}));
                    }
                    if let Some(servers) = obj.get_mut(block_key).and_then(|v| v.as_object_mut()) {
                        servers.insert(server_name, server_config);
                        fs::write(&config_path, serde_json::to_string_pretty(&config_json)?)?;
                        println!(
                            "  - Injected MCP identity '{}' into {}",
                            agent_name,
                            config_path.display()
                        );
                    }
                }
            } else {
                println!(
                    "  ! Warning: Manual configuration required for harness '{}' (MCP)",
                    harness
                );
            }
        }

        // 4. Install Skills if selected
        if install_skills {
            println!("Installing CURD Skills for harness: {}...", harness);
            match harness.as_str() {
                "gemini" => {
                    let skill_dir = workspace_root
                        .join(".gemini")
                        .join("skills")
                        .join("propose-plan");
                    let _ = fs::create_dir_all(&skill_dir);
                    let _ = fs::write(skill_dir.join("SKILL.md"), PROPOSE_PLAN_SKILL_MD);
                    println!("  - Installed Gemini Skill to {}", skill_dir.display());
                }
                "cursor" => {
                    let rule_dir = workspace_root.join(".cursor").join("rules");
                    let _ = fs::create_dir_all(&rule_dir);
                    let _ = fs::write(rule_dir.join("propose-plan.mdc"), PROPOSE_PLAN_RULE_MDC);
                    println!("  - Installed Cursor Rule to {}", rule_dir.display());
                }
                _ => {
                    println!(
                        "  ! Skill logic handles via tools natively for harness '{}'",
                        harness
                    );
                }
            }
        }
    }

    Ok(())
}
