use crate::CurdConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildRequest {
    pub adapter: Option<String>,
    pub profile: Option<String>,
    pub target: Option<String>,
    pub execute: bool,
    pub zig: bool,
    pub command: Option<String>,
    #[serde(default)]
    pub allow_untrusted: bool,
    #[serde(default)]
    pub trailing_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStep {
    pub adapter: String,
    pub cwd: String,
    pub command: Vec<String>,
    pub status: Option<i32>,
    pub success: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResponse {
    pub status: String,
    pub adapter: String,
    pub profile: String,
    pub target: Option<String>,
    pub execute: bool,
    pub steps: Vec<BuildStep>,
    pub untrusted_confirmation_required: bool,
}

pub fn run_build(root: &Path, mut req: BuildRequest) -> Result<BuildResponse> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let cfg = CurdConfig::load_from_workspace(&root);
    if req.profile.is_none() {
        req.profile = cfg.build.default_profile.clone();
    }
    if req.target.is_none() {
        req.target = cfg.build.default_target.clone();
    }
    let profile = req.profile.unwrap_or_else(|| "debug".to_string());

    let mut untrusted_confirmation_required = false;
    
    let (mut steps, adapter) = if let Some(cmd) = req.command {
        if !req.allow_untrusted && req.execute {
            untrusted_confirmation_required = true;
        }

        let shell_bin = if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
        };
        let shell_flag = if cfg!(target_os = "windows") { "/C" } else { "-lc" };
        
        (
            vec![BuildStep {
                adapter: "custom-shell".to_string(),
                cwd: root.to_string_lossy().to_string(),
                command: vec![shell_bin, shell_flag.to_string(), cmd],
                status: None,
                success: None,
            }],
            "custom-shell".to_string(),
        )
    } else {
        let adapter = req
            .adapter
            .or_else(|| cfg.build.preferred_adapter.clone())
            .unwrap_or_else(|| detect_adapter(&root, &cfg));

        // SECURITY: Identify custom adapters from workspace configuration
        let is_custom = !matches!(
            adapter.as_str(),
            "cargo"
                | "cmake"
                | "ninja"
                | "make"
                | "uv"
                | "go"
                | "gradle"
                | "maven"
                | "bazel"
                | "meson"
                | "buck2"
                | "npm"
                | "yarn"
                | "pnpm"
                | "bun"
                | "pixi"
                | "mise"
        );

        if is_custom && !req.allow_untrusted && req.execute {
            untrusted_confirmation_required = true;
        }

        let steps = plan_steps(
            &root,
            &adapter,
            &profile,
            req.target.clone(),
            &cfg.build.build_dir,
            &req.trailing_args,
            req.zig,
            &cfg,
        )?;
        (steps, adapter)
    };

    let mut ok = true;
    if req.execute && !untrusted_confirmation_required {
        let sandbox = crate::Sandbox::new(&root);
        let curd_dir = crate::workspace::get_curd_dir(&root);
        let builds_dir = curd_dir.join("builds");
        let _ = std::fs::create_dir_all(&builds_dir);
        let log_path = builds_dir.join("latest.log");
        let mut log_file = std::fs::File::create(&log_path).ok();
        
        for step in &mut steps {
            if step.command.is_empty() {
                step.status = Some(1);
                step.success = Some(false);
                ok = false;
                continue;
            }
            
            use std::io::Write;
            if let Some(f) = log_file.as_mut() {
                let _ = writeln!(f, "=== Executing: {} ===", step.command.join(" "));
            }
            
            let mut cmd = sandbox.build_std_command(&step.command[0], &step.command[1..]);
            cmd.current_dir(&step.cwd);
            
            cmd.stdout(std::process::Stdio::inherit());
            cmd.stderr(std::process::Stdio::piped());
            
            if let Ok(mut child) = cmd.spawn() {
                let mut captured_stderr = String::new();
                if let Some(stderr) = child.stderr.take() {
                    use std::io::{BufRead, BufReader};
                    let reader = BufReader::new(stderr);
                    for line_res in reader.lines() {
                        if let Ok(line) = line_res {
                            eprintln!("{}", line); // Stream to user
                            if let Some(f) = log_file.as_mut() {
                                let _ = writeln!(f, "{}", line);
                            }
                            if captured_stderr.len() < 1024 * 1024 {
                                captured_stderr.push_str(&line);
                                captured_stderr.push('\n');
                            }
                        }
                    }
                }
                
                if let Ok(status) = child.wait() {
                    step.status = status.code();
                    let success = status.success();
                    step.success = Some(success);
                    
                    if !success {
                        ok = false;
                        parse_and_propagate_faults(&root, &captured_stderr);
                        break;
                    }
                } else {
                    step.status = Some(1);
                    step.success = Some(false);
                    if let Some(f) = log_file.as_mut() {
                        let _ = writeln!(f, "Failed to wait on process.");
                    }
                    ok = false;
                    break;
                }
            } else {
                step.status = Some(1);
                step.success = Some(false);
                if let Some(f) = log_file.as_mut() {
                    let _ = writeln!(f, "Failed to spawn process.");
                }
                ok = false;
                break;
            }
        }
    }

    Ok(BuildResponse {
        status: if ok {
            "ok".to_string()
        } else {
            "error".to_string()
        },
        adapter,
        profile,
        target: req.target,
        execute: req.execute,
        steps,
        untrusted_confirmation_required,
    })
}

fn detect_adapter(root: &Path, cfg: &CurdConfig) -> String {
    let mut custom_names = cfg
        .build
        .adapters
        .keys()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    custom_names.sort();
    for name in custom_names {
        let Some(adapter) = cfg.build.adapters.get(&name) else {
            continue;
        };
        if adapter.detect_files.is_empty() {
            continue;
        }
        let matched = adapter
            .detect_files
            .iter()
            .all(|f| sandbox_rel(root, f).map(|p| p.exists()).unwrap_or(false));
        if matched {
            return name;
        }
    }

    if root.join("Cargo.toml").exists() {
        return "cargo".to_string();
    }
    if root.join("CMakeLists.txt").exists() {
        return "cmake".to_string();
    }
    if root.join("build.ninja").exists() || root.join("build").join("build.ninja").exists() {
        return "ninja".to_string();
    }
    if root.join("Makefile").exists() || root.join("makefile").exists() {
        return "make".to_string();
    }
    if root.join("pyproject.toml").exists() || root.join("uv.lock").exists() {
        return "uv".to_string();
    }
    if root.join("go.mod").exists() {
        return "go".to_string();
    }
    if root.join("package.json").exists() {
        if root.join("pnpm-lock.yaml").exists() {
            return "pnpm".to_string();
        }
        if root.join("yarn.lock").exists() {
            return "yarn".to_string();
        }
        if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
            return "bun".to_string();
        }
        return "npm".to_string();
    }
    if root.join("build.gradle").exists()
        || root.join("build.gradle.kts").exists()
        || root.join("settings.gradle").exists()
        || root.join("settings.gradle.kts").exists()
        || root.join("gradlew").exists()
        || root.join("gradlew.bat").exists()
    {
        return "gradle".to_string();
    }
    if root.join("pixi.toml").exists() {
        return "pixi".to_string();
    }
    if root.join("mise.toml").exists() || root.join(".mise.toml").exists() {
        return "mise".to_string();
    }
    if root.join("pom.xml").exists() {
        return "maven".to_string();
    }
    if root.join("WORKSPACE").exists() || root.join("WORKSPACE.bazel").exists() || root.join("BUILD").exists() || root.join("BUILD.bazel").exists() {
        return "bazel".to_string();
    }
    if root.join("meson.build").exists() {
        return "meson".to_string();
    }
    if root.join(".buckconfig").exists() {
        return "buck2".to_string();
    }
    "make".to_string()
}

fn plan_steps(
    root: &Path,
    adapter: &str,
    profile: &str,
    target: Option<String>,
    build_dir_cfg: &Option<String>,
    trailing_args: &[String],
    zig: bool,
    cfg: &CurdConfig,
) -> Result<Vec<BuildStep>> {
    let mut steps = Vec::new();
    let root_s = root.to_string_lossy().to_string();
    let build_dir = build_dir_cfg.clone().unwrap_or_else(|| "build".to_string());
    let build_dir_path = sandbox_rel(root, &build_dir)?;
    let build_dir_s = build_dir_path.to_string_lossy().to_string();

    let lower_profile = profile.to_lowercase();
    match adapter {
        "cargo" => {
            let mut cmd = if zig {
                vec!["cargo".to_string(), "zigbuild".to_string()]
            } else {
                vec!["cargo".to_string(), "build".to_string()]
            };
            if lower_profile == "release" {
                cmd.push("--release".to_string());
            }
            if let Some(t) = target.clone() {
                cmd.push("--target".to_string());
                cmd.push(t);
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "cmake" => {
            if !build_dir_path.exists() {
                let mut cfg_cmd = vec![
                    "cmake".to_string(),
                    "-S".to_string(),
                    ".".to_string(),
                    "-B".to_string(),
                    build_dir.clone(),
                ];
                if lower_profile == "release" {
                    cfg_cmd.push("-DCMAKE_BUILD_TYPE=Release".to_string());
                } else {
                    cfg_cmd.push("-DCMAKE_BUILD_TYPE=Debug".to_string());
                }
                steps.push(BuildStep {
                    adapter: adapter.to_string(),
                    cwd: root_s.clone(),
                    command: cfg_cmd,
                    status: None,
                    success: None,
                });
            }
            let mut build_cmd = vec!["cmake".to_string(), "--build".to_string(), build_dir];
            if lower_profile == "release" {
                build_cmd.push("--config".to_string());
                build_cmd.push("Release".to_string());
            } else if lower_profile == "debug" {
                build_cmd.push("--config".to_string());
                build_cmd.push("Debug".to_string());
            }
            if let Some(t) = target.clone() {
                build_cmd.push("--target".to_string());
                build_cmd.push(t);
            }
            build_cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: build_cmd,
                status: None,
                success: None,
            });
        }
        "ninja" => {
            let mut cmd = vec!["ninja".to_string()];
            if let Some(t) = target.clone() {
                cmd.push(t);
            }
            cmd.extend(trailing_args.iter().cloned());
            let cwd = if build_dir_path.join("build.ninja").exists() {
                build_dir_s
            } else {
                root_s
            };
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "make" => {
            let mut cmd = vec!["make".to_string()];
            if let Some(t) = target.clone() {
                cmd.push(t);
            } else if lower_profile == "release" {
                cmd.push("release".to_string());
            } else if lower_profile == "debug" {
                cmd.push("debug".to_string());
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "uv" => {
            let mut cmd = vec!["uv".to_string(), "build".to_string()];
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "go" => {
            let mut cmd = vec!["go".to_string(), "build".to_string()];
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "gradle" => {
            let gradlew = root.join("gradlew");
            let mut cmd = if gradlew.exists() {
                vec![gradlew.to_string_lossy().to_string()]
            } else {
                vec!["gradle".to_string()]
            };
            if let Some(t) = target.clone() {
                cmd.push(t);
            } else {
                cmd.push("build".to_string());
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "maven" => {
            let mvnw = root.join("mvnw");
            let mut cmd = if mvnw.exists() {
                vec![mvnw.to_string_lossy().to_string()]
            } else {
                vec!["mvn".to_string()]
            };
            if let Some(t) = target.clone() {
                cmd.push(t);
            } else {
                cmd.push("compile".to_string());
            }
            if lower_profile == "release" {
                cmd.push("-P".to_string());
                cmd.push("release".to_string());
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "bazel" => {
            let mut cmd = vec!["bazel".to_string(), "build".to_string()];
            if lower_profile == "release" {
                cmd.push("-c".to_string());
                cmd.push("opt".to_string());
            } else if lower_profile == "debug" {
                cmd.push("-c".to_string());
                cmd.push("dbg".to_string());
            }
            if let Some(t) = target.clone() {
                cmd.push(t);
            } else {
                cmd.push("//...".to_string());
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "meson" => {
            let mut cmd = vec!["meson".to_string(), "compile".to_string(), "-C".to_string(), build_dir_s];
            if let Some(t) = target.clone() {
                cmd.push(t);
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "buck2" => {
            let mut cmd = vec!["buck2".to_string(), "build".to_string()];
            if let Some(t) = target.clone() {
                cmd.push(t);
            } else {
                cmd.push("//...".to_string());
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "npm" | "yarn" | "pnpm" | "bun" => {
            let mut cmd = vec![adapter.to_string(), "run".to_string(), "build".to_string()];
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "pixi" => {
            let mut cmd = vec!["pixi".to_string(), "run".to_string()];
            if let Some(t) = target.clone() {
                cmd.push(t);
            } else {
                cmd.push("build".to_string());
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        "mise" => {
            let mut cmd = vec!["mise".to_string(), "run".to_string()];
            if let Some(t) = target.clone() {
                cmd.push(t);
            } else {
                cmd.push("build".to_string());
            }
            cmd.extend(trailing_args.iter().cloned());
            steps.push(BuildStep {
                adapter: adapter.to_string(),
                cwd: root_s,
                command: cmd,
                status: None,
                success: None,
            });
        }
        other => {
            if let Some(custom) = cfg.build.adapters.get(other) {
                if custom.steps.is_empty() {
                    anyhow::bail!("Custom adapter '{}' has no configured steps", other);
                }
                for f in &custom.detect_files {
                    let _ = sandbox_rel(root, f)?;
                }
                let cwd = custom
                    .cwd
                    .as_ref()
                    .map(|v| sandbox_rel(root, v))
                    .transpose()?
                    .unwrap_or_else(|| root.to_path_buf())
                    .to_string_lossy()
                    .to_string();
                for raw in &custom.steps {
                    let mut command = Vec::new();
                    for token in raw {
                        command.push(
                            token
                                .replace("{profile}", profile)
                                .replace("{target}", target.as_deref().unwrap_or("")),
                        );
                    }
                    if command.is_empty() {
                        continue;
                    }
                    command.extend(trailing_args.iter().cloned());
                    steps.push(BuildStep {
                        adapter: adapter.to_string(),
                        cwd: cwd.clone(),
                        command,
                        status: None,
                        success: None,
                    });
                }
                if steps.is_empty() {
                    anyhow::bail!(
                        "Custom adapter '{}' resolved to zero executable steps",
                        other
                    );
                }
            } else {
                anyhow::bail!("Unknown build adapter: {}", other);
            }
        }
    }
    Ok(steps)
}

fn sandbox_rel(root: &Path, rel: &str) -> Result<PathBuf> {
    crate::workspace::validate_sandboxed_path(root, rel).map_err(|e| {
        anyhow::anyhow!(
            "Unsafe path '{}' in build configuration (must be workspace-relative): {}",
            rel,
            e
        )
    })
}

fn parse_and_propagate_faults(root: &Path, stderr: &str) {
    let cfg = crate::CurdConfig::load_from_workspace(root);
    let storage = match crate::storage::Storage::open(root, &cfg) {
        Ok(s) => s,
        Err(_) => return,
    };
    let graph_engine = crate::graph::GraphEngine::new(root);

    let lines: Vec<&str> = stderr.lines().collect();
    for line in lines {
        let line = line.trim();
        let mut extracted: Option<(String, usize)> = None;

        // Rust: "--> src/main.rs:10:5"
        if line.starts_with("--> ") {
            let parts: Vec<&str> = line[4..].split(':').collect();
            if parts.len() >= 2 {
                if let Ok(line_num) = parts[1].parse::<usize>() {
                    extracted = Some((parts[0].to_string(), line_num));
                }
            }
        }
        // Node: "at functionName (/path/to/file.js:10:15)" or "at /path/to/file.js:10:15"
        else if line.starts_with("at ") {
            let mut target = line;
            if let Some(start) = line.find('(') {
                if let Some(end) = line.find(')') {
                    target = &line[start + 1..end];
                }
            } else {
                target = &line[3..]; // after "at "
            }
            let parts: Vec<&str> = target.split(':').collect();
            if parts.len() >= 2 {
                if let Ok(line_num) = parts[1].parse::<usize>() {
                    extracted = Some((parts[0].to_string(), line_num));
                }
            }
        }
        // Python: "File \"/path/to/file.py\", line 10, in"
        else if line.starts_with("File \"") && line.contains("\", line ") {
            if let Some(end_quote) = line[6..].find('\"') {
                let file_path = &line[6..6 + end_quote];
                let rest = &line[6 + end_quote + 8..]; // skip ", line "
                if let Some(comma) = rest.find(',') {
                    if let Ok(line_num) = rest[..comma].parse::<usize>() {
                        extracted = Some((file_path.to_string(), line_num));
                    }
                }
            }
        }
        // C/C++: "src/main.cpp:10:5: error: ..."
        else if let Some(err_idx) = line.find(": error:") {
            let prefix = &line[..err_idx];
            let parts: Vec<&str> = prefix.split(':').collect();
            if parts.len() >= 2 {
                let file_path = if parts.len() > 2 {
                    parts[0..parts.len() - 2].join(":")
                } else {
                    parts[0].to_string()
                };
                let line_str = parts[parts.len() - 2];
                if let Ok(line_num) = line_str.parse::<usize>() {
                    extracted = Some((file_path, line_num));
                }
            }
        }

        if let Some((filepath, line_num)) = extracted {
            let clean_path = filepath.trim();
            let abs_path = if Path::new(clean_path).is_absolute() {
                PathBuf::from(clean_path)
            } else {
                root.join(clean_path)
            };
            
            let canon_path = std::fs::canonicalize(&abs_path).unwrap_or(abs_path);
            let abs_path_str = canon_path.to_string_lossy().to_string();

            if let Ok(Some(symbol_id)) = storage.get_symbol_at_line(&abs_path_str, line_num) {
                log::info!("Mapped build error at {}:{} to semantic symbol: {}", clean_path, line_num, symbol_id);
                let _ = graph_engine.annotate_fault(&symbol_id, uuid::Uuid::new_v4());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BuildRequest, run_build};
    use tempfile::tempdir;

    #[test]
    fn build_detects_cargo_and_plans_debug() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\nedition='2021'\n",
        )
        .expect("write cargo");
        let res = run_build(dir.path(), BuildRequest::default()).expect("run_build");
        assert_eq!(res.status, "ok");
        assert_eq!(res.adapter, "cargo");
        assert_eq!(res.profile, "debug");
        assert_eq!(res.steps.len(), 1);
        assert_eq!(res.steps[0].command[0], "cargo");
        assert_eq!(res.steps[0].command[1], "build");
    }

    #[test]
    fn build_config_defaults_are_applied() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("settings.toml"),
            r#"
[build]
preferred_adapter = "make"
default_profile = "release"
"#,
        )
        .expect("write config");
        std::fs::write(dir.path().join("Makefile"), "all:\n\t@echo ok\n").expect("write make");
        let res = run_build(dir.path(), BuildRequest::default()).expect("run_build");
        assert_eq!(res.adapter, "make");
        assert_eq!(res.profile, "release");
        assert_eq!(res.steps[0].command[0], "make");
        assert_eq!(res.steps[0].command[1], "release");
    }

    #[test]
    fn build_custom_adapter_from_settings() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("settings.toml"),
            r#"
[build]
preferred_adapter = "mybuild"

[build.adapters.mybuild]
detect_files = ["foo.build"]
cwd = "."
steps = [["echo", "hello-{profile}"], ["echo", "{target}"]]
"#,
        )
        .expect("write settings");
        std::fs::write(dir.path().join("foo.build"), "x").expect("write marker");
        let res = run_build(
            dir.path(),
            BuildRequest {
                target: Some("all".to_string()),
                ..BuildRequest::default()
            },
        )
        .expect("run_build");
        assert_eq!(res.adapter, "mybuild");
        assert_eq!(res.steps.len(), 2);
        assert_eq!(res.steps[0].command, vec!["echo", "hello-debug"]);
        assert_eq!(res.steps[1].command, vec!["echo", "all"]);
    }

    #[test]
    fn build_custom_adapter_requires_non_empty_steps() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("settings.toml"),
            r#"
[build]
preferred_adapter = "broken"

[build.adapters.broken]
detect_files = ["broken.build"]
steps = []
"#,
        )
        .expect("write settings");
        std::fs::write(dir.path().join("broken.build"), "x").expect("write marker");
        let err = run_build(dir.path(), BuildRequest::default()).expect_err("expected error");
        assert!(err.to_string().contains("no configured steps"));
    }

    #[test]
    fn build_custom_adapter_rejects_unsafe_cwd() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("settings.toml"),
            r#"
[build]
preferred_adapter = "badcwd"

[build.adapters.badcwd]
detect_files = ["ok.marker"]
cwd = "../escape"
steps = [["echo", "x"]]
"#,
        )
        .expect("write settings");
        std::fs::write(dir.path().join("ok.marker"), "x").expect("write marker");
        let err = run_build(dir.path(), BuildRequest::default()).expect_err("expected error");
        assert!(err.to_string().contains("Unsafe path"));
    }

    #[test]
    fn build_rejects_unsafe_build_dir() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("settings.toml"),
            r#"
[build]
preferred_adapter = "cmake"
build_dir = "../outside_build"
"#,
        )
        .expect("write settings");
        std::fs::write(
            dir.path().join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.10)",
        )
        .expect("write cmake");
        let err = run_build(dir.path(), BuildRequest::default()).expect_err("expected error");
        assert!(err.to_string().contains("Unsafe path"));
    }
}
