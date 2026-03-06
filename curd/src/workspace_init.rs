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
fn scaffold_curd_dir(root: &Path) -> Result<()> {
    let curd_dir = root.join(".curd");
    fs::create_dir_all(curd_dir.join("builds"))?;
    fs::create_dir_all(curd_dir.join("feedback"))?;
    fs::create_dir_all(curd_dir.join("wiki"))?;
    fs::create_dir_all(curd_dir.join("commits"))?;

    // Create default curd.toml if none exists
    let has_config = root.join("settings.toml").exists()
        || root.join("curd.toml").exists()
        || root.join("CURD.toml").exists();

    if !has_config {
        let default_config = r#"# CURD Workspace Configuration
# See https://github.com/curd-dev/curd for documentation.

[index]
mode = "full"                # full|fast|lazy|scoped

[edit]
churn_limit = 0.30

[doctor]
strict = false
"#;
        fs::write(root.join("curd.toml"), default_config)?;
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
        println!("  .curd/ directory already exists — skipping scaffold.");
    } else {
        scaffold_curd_dir(root)?;
        println!("  ✅ Created .curd/ directory:");
        println!("     ├── builds/      (build capture logs)");
        println!("     ├── feedback/    (annotation ledger)");
        println!("     ├── wiki/        (generated docs)");
        println!("     └── commits/     (provenance tracking)");
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
    println!("    curd build .         Plan a build via CURD's adapters");
    #[cfg(feature = "mcp")]
    println!("    curd mcp .           Start the MCP server");
    #[cfg(feature = "mcp")]
    println!("    curd init-agent      Register an AI agent");
    println!();

    Ok(())
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
        assert!(root.join("curd.toml").exists());
        
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains(".curd/"));
    }
}
