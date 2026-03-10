use anyhow::Result;
use std::fs;
use std::path::Path;

/// Information detected about a workspace during `curd init`.
pub struct WorkspaceInfo {
    pub vcs: Option<String>,
    pub languages: Vec<String>,
    pub build_systems: Vec<BuildSystemMatch>,
}

/// A detected build system and its capture strategy.
pub struct BuildSystemMatch {
    pub name: &'static str,
    pub detected_by: &'static str,
    pub capture_hint: &'static str,
}

/// Detect version control system in use.
fn detect_vcs(root: &Path) -> Option<String> {
    if root.join(".git").exists() {
        Some("git".into())
    } else if root.join(".hg").exists() {
        Some("mercurial".into())
    } else if root.join(".svn").exists() {
        Some("svn".into())
    } else {
        None
    }
}

/// Detect languages present by scanning for known config/source files.
fn detect_languages(root: &Path) -> Vec<String> {
    let mut langs = Vec::new();

    let checks: &[(&str, &[&str])] = &[
        ("Rust", &["Cargo.toml"]),
        ("Python", &["pyproject.toml", "setup.py", "setup.cfg"]),
        ("JavaScript", &["package.json"]),
        ("TypeScript", &["tsconfig.json"]),
        ("Go", &["go.mod"]),
        ("Java", &["pom.xml", "build.gradle", "build.gradle.kts"]),
        ("C/C++", &["CMakeLists.txt", "Makefile", "meson.build"]),
        ("Swift", &["Package.swift"]),
        ("Zig", &["build.zig"]),
        ("Elixir", &["mix.exs"]),
        ("C#/.NET", &["*.csproj", "*.sln"]),
    ];

    for (lang, markers) in checks {
        for marker in *markers {
            if let Some(ext) = marker.strip_prefix('*') {
                // Glob check in root dir only
                // e.g. ".csproj"
                if let Ok(entries) = fs::read_dir(root) {
                    for entry in entries.flatten() {
                        if entry.path().to_string_lossy().ends_with(ext) {
                            langs.push(lang.to_string());
                            break;
                        }
                    }
                }
            } else if root.join(marker).exists() {
                langs.push(lang.to_string());
                break;
            }
        }
    }

    langs.dedup();
    langs
}

/// Detect all matching build systems.
fn detect_build_systems(root: &Path) -> Vec<BuildSystemMatch> {
    let mut matches = Vec::new();

    let checks: &[(&str, &str, &str)] = &[
        // (name, detect_file, capture_hint)
        ("Cargo", "Cargo.toml", "--message-format=json"),
        ("CMake", "CMakeLists.txt", "CMAKE_EXPORT_COMPILE_COMMANDS=ON"),
        ("Makefile", "Makefile", "tee .curd/builds/latest.jsonl"),
        ("npm", "package.json", "wrapper script in scripts"),
        ("Go", "go.mod", "-json flag on go build/test"),
        ("Gradle", "build.gradle", "--console=plain capture"),
        ("Gradle (Kotlin)", "build.gradle.kts", "--console=plain capture"),
        ("Gradle", "settings.gradle", "--console=plain capture"),
        ("Gradle (Kotlin)", "settings.gradle.kts", "--console=plain capture"),
        ("Gradle", "gradlew", "--console=plain capture"),
        ("Maven", "pom.xml", "-B batch mode + output redirect"),
        ("Bazel", "BUILD", "--build_event_json_file"),
        ("Bazel", "WORKSPACE", "--build_event_json_file"),
        ("Buck2", ".buckconfig", "--console=simple + BEP output"),
        ("Meson", "meson.build", "compile_commands.json from setup"),
        ("Swift PM", "Package.swift", "swift build structured diagnostics"),
        ("Zig", "build.zig", "--verbose capture"),
        ("Mix", "mix.exs", "--formatter json"),
        ("Poetry/uv", "pyproject.toml", "pip install --report (PEP 668)"),
    ];

    for (name, detect_file, hint) in checks {
        if root.join(detect_file).exists() {
            matches.push(BuildSystemMatch {
                name,
                detected_by: detect_file,
                capture_hint: hint,
            });
        }
    }

    // Glob-based checks
    let glob_checks: &[(&str, &str, &str)] = &[
        ("Xcode", ".xcodeproj", "xcodebuild -resultBundlePath"),
        ("Xcode", ".xcworkspace", "xcodebuild -resultBundlePath"),
        ("MSBuild", ".sln", "-fileLogger -bl (binary log)"),
        ("MSBuild", ".csproj", "-fileLogger -bl (binary log)"),
    ];

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let name_str = entry.file_name().to_string_lossy().to_string();
            for (bs_name, ext, hint) in glob_checks {
                if name_str.ends_with(ext) {
                    // Avoid duplicates
                    if !matches.iter().any(|m| m.name == *bs_name) {
                        matches.push(BuildSystemMatch {
                            name: bs_name,
                            detected_by: ext,
                            capture_hint: hint,
                        });
                    }
                }
            }
        }
    }

    matches
}

/// Scaffold the `.curd/` directory structure.
fn scaffold_curd_dir(root: &Path, info: &WorkspaceInfo) -> Result<()> {
    let curd_dir = root.join(".curd");
    fs::create_dir_all(curd_dir.join("builds"))?;
    fs::create_dir_all(curd_dir.join("feedback"))?;
    fs::create_dir_all(curd_dir.join("wiki"))?;
    fs::create_dir_all(curd_dir.join("commits"))?;
    fs::create_dir_all(curd_dir.join("grammars"))?;

    // Create default curd.toml if none exists
    let has_config = root.join(".curd").join("curd.toml").exists()
        || root.join(".curd").join("settings.toml").exists()
        || root.join("settings.toml").exists()
        || root.join("curd.toml").exists()
        || root.join("CURD.toml").exists();

    if !has_config {
        let mut default_config = r#"# CURD Workspace Configuration
# See https://github.com/curd-dev/curd for documentation.

[index]
mode = "full"                # full|fast|lazy|scoped

[workspace]
require_open_for_all_tools = false

[edit]
churn_limit = 0.30
enforce_transactional = true # Mandatory sessions for AI agents

[doctor]
strict = false

[build.tasks]
"#
        .to_string();

        let mut added_tasks = false;
        for bs in &info.build_systems {
            match bs.name {
                "Cargo" => {
                    default_config.push_str("build = \"cargo build\"\n");
                    default_config.push_str("release = \"cargo build --release\"\n");
                    default_config.push_str("test = \"cargo test\"\n");
                    added_tasks = true;
                }
                "CMake" | "Makefile" => {
                    default_config.push_str("build = \"make\"\n");
                    default_config.push_str("clean = \"make clean\"\n");
                    added_tasks = true;
                }
                "npm" | "yarn" | "pnpm" | "bun" => {
                    let pm = bs.name;
                    default_config.push_str(&format!("dev = \"{} run dev\"\n", pm));
                    default_config.push_str(&format!("build = \"{} run build\"\n", pm));
                    added_tasks = true;
                }
                "Go" => {
                    default_config.push_str("build = \"go build ./...\"\n");
                    default_config.push_str("test = \"go test ./...\"\n");
                    added_tasks = true;
                }
                "Poetry/uv" => {
                    if root.join("uv.lock").exists() {
                        default_config.push_str("install = \"uv sync\"\n");
                        default_config.push_str("test = \"uv run pytest\"\n");
                    } else {
                        default_config.push_str("install = \"poetry install\"\n");
                        default_config.push_str("test = \"poetry run pytest\"\n");
                    }
                    added_tasks = true;
                }
                _ => {}
            }
        }
        
        if !added_tasks {
            default_config.push_str("# build = \"make build\"\n");
            default_config.push_str("# release = \"make release\"\n");
            default_config.push_str("# test = \"make test\"\n");
        }

        fs::write(curd_dir.join("curd.toml"), default_config)?;
    }

    // Add .curd/ to .gitignore if git is in use and not already ignored
    let gitignore_path = root.join(".gitignore");
    if root.join(".git").exists() {
        let gitignore_content = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path).unwrap_or_default()
        } else {
            String::new()
        };

        if !gitignore_content.contains(".curd/") && !gitignore_content.contains(".curd\n") {
            let mut new_content = gitignore_content;
            if !new_content.ends_with('\n') && !new_content.is_empty() {
                new_content.push('\n');
            }
            new_content.push_str("\n# CURD workspace data\n.curd/\n");
            fs::write(&gitignore_path, new_content)?;
        }
    }

    Ok(())
}

/// Full workspace analysis: detect VCS, languages, and build systems.
pub fn analyze_workspace(root: &Path) -> WorkspaceInfo {
    WorkspaceInfo {
        vcs: detect_vcs(root),
        languages: detect_languages(root),
        build_systems: detect_build_systems(root),
    }
}

/// Run the full init flow for a workspace.
pub fn run_init(root: &Path) -> Result<()> {
    println!("\n  🔧 CURD Workspace Setup\n");

    let info = analyze_workspace(root);

    // Display detected environment
    println!("  Detected project:");
    if let Some(ref vcs) = info.vcs {
        println!("    VCS:            {}", vcs);
    }
    if !info.languages.is_empty() {
        println!("    Languages:      {}", info.languages.join(", "));
    }
    if !info.build_systems.is_empty() {
        println!("    Build systems:");
        for bs in &info.build_systems {
            println!("      • {} (found {})", bs.name, bs.detected_by);
        }
    } else {
        println!("    Build system:   (none detected)");
    }

    println!();

    // Scaffold .curd/ directory
    let curd_dir = root.join(".curd");
    if curd_dir.exists() {
        println!("  .curd/ directory already exists — ensuring subdirectories...");
        scaffold_curd_dir(root, &info)?;
    } else {
        scaffold_curd_dir(root, &info)?;
        println!("  ✅ Created .curd/ directory:");
        println!("     ├── builds/      (build capture logs)");
        println!("     ├── feedback/    (annotation ledger)");
        println!("     ├── wiki/        (generated docs)");
        println!("     ├── commits/     (provenance tracking)");
        println!("     └── grammars/    (WASM/Native parser storage)");
    }

    // Bootstrap core grammars
    println!("  📥 Bootstrapping core semantic grammars...");
    if let Ok(mut manager) = curd_core::ParserManager::new(curd_dir.join("grammars")) {
        let _ = manager.bootstrap_core_grammars();
    }

    // Show config status
    if root.join("curd.toml").exists() || root.join("settings.toml").exists() || root.join("CURD.toml").exists() {
        println!("  ✅ Workspace config found.");
    }

    // Gitignore status
    if root.join(".git").exists() {
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
        if gitignore.contains(".curd/") {
            println!("  ✅ .curd/ added to .gitignore.");
        }
    }

    println!("  ✅ Transactional safety barrier enabled by default.");

    // Build capture hint
    if !info.build_systems.is_empty() {
        println!("\n  Build capture hints:");
        for bs in &info.build_systems {
            println!("    {} → {}", bs.name, bs.capture_hint);
        }
    }

    // Next steps
    println!("\n  Next steps:");
    println!("    curd doctor .        Run diagnostics on this workspace");
    println!("    curd build release   Plan and execute a task defined in settings.toml");
    #[cfg(feature = "mcp")]
    println!("    curd mcp .           Start the MCP server");
    #[cfg(feature = "mcp")]
    println!("    curd init-agent      Register an AI agent");
    println!();

    Ok(())
}

/// Surgically cleans up CURD configurations from AI agent directories without deleting their state.
pub fn cleanup_agent_configs(root: &Path) {
    let targets = [
        (root.join(".gemini").join("settings.json"), "mcpServers"),
        (root.join(".copilot").join("mcp-config.json"), "mcpServers"),
        (root.join(".codex").join("config.json"), "mcpServers"),
        (root.join(".cursor").join("mcp.json"), "mcpServers"),
        (root.join(".mcp.json"), "mcpServers"), // claude_desktop / claude_code
        (root.join(".cline").join("cline_mcp_settings.json"), "mcpServers"),
        (root.join(".roo").join("cline_mcp_settings.json"), "mcpServers"),
        (root.join(".windsurf").join("mcp_config.json"), "mcpServers"),
        (root.join(".zed").join("settings.json"), "context_servers"),
    ];

    for (target_path, block_key) in targets {
        if target_path.exists() {
            if let Ok(content) = fs::read_to_string(&target_path) {
                if let Ok(mut config) = serde_json::from_str::<serde_json::Value>(&content) {
                    let mut modified = false;
                    if let Some(obj) = config.as_object_mut() {
                        if let Some(servers) = obj.get_mut(block_key).and_then(|v| v.as_object_mut()) {
                            let keys_to_remove: Vec<String> = servers
                                .keys()
                                .filter(|k| k.starts_with("curd_"))
                                .cloned()
                                .collect();
                            for k in keys_to_remove {
                                servers.remove(&k);
                                modified = true;
                            }
                        }
                    }
                    if modified {
                        let _ = fs::write(&target_path, serde_json::to_string_pretty(&config).unwrap_or_default());
                        println!("Removed CURD blocks from {}", target_path.display());
                    }
                }
            }
        }
    }
    
    // Clean up specific skills silently
    let gemini_skill = root.join(".gemini/skills/propose-plan/SKILL.md");
    if gemini_skill.exists() {
        let _ = fs::remove_file(&gemini_skill);
        if let Some(p) = gemini_skill.parent() { let _ = fs::remove_dir(p); }
    }
    let cursor_rule = root.join(".cursor/rules/propose-plan.mdc");
    if cursor_rule.exists() {
        let _ = fs::remove_file(&cursor_rule);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_vcs() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        
        assert_eq!(detect_vcs(root), None);
        
        fs::create_dir(root.join(".git")).unwrap();
        assert_eq!(detect_vcs(root).unwrap(), "git");
    }

    #[test]
    fn test_detect_languages_and_build_systems() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        
        fs::write(root.join("Cargo.toml"), "").unwrap();
        fs::write(root.join("package.json"), "").unwrap();
        
        let langs = detect_languages(root);
        assert!(langs.contains(&"Rust".to_string()));
        assert!(langs.contains(&"JavaScript".to_string()));
        
        let builds = detect_build_systems(root);
        assert!(builds.iter().any(|b| b.name == "Cargo"));
        assert!(builds.iter().any(|b| b.name == "npm"));
    }

    #[test]
    fn test_scaffold_curd_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
        
        scaffold_curd_dir(root).unwrap();
        
        assert!(root.join(".curd/builds").exists());
        assert!(root.join(".curd/feedback").exists());
        assert!(root.join(".curd/wiki").exists());
        assert!(root.join(".curd/commits").exists());
        assert!(root.join(".curd/curd.toml").exists());
        
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains(".curd/"));
    }
}
