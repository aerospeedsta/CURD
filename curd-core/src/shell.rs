use anyhow::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Package manager config files and their mutation subcommands
const PKG_MANAGERS: &[(&str, &[&str], &[&str])] = &[
    // (binary, config files that must exist at workspace root, mutation subcommands)
    (
        "mise",
        &["mise.toml", ".mise.toml"],
        &["use", "install", "upgrade", "remove", "uninstall", "trust"],
    ),
    (
        "pixi",
        &["pixi.toml", "pyproject.toml"],
        &["add", "remove", "install", "update"],
    ),
    (
        "uv",
        &["pyproject.toml", "uv.lock"],
        &[
            "add",
            "remove",
            "lock",
            "sync",
            "pip install",
            "pip uninstall",
        ],
    ),
    (
        "pip",
        &["requirements.txt", "pyproject.toml"],
        &["install", "uninstall"],
    ),
    (
        "poetry",
        &["pyproject.toml", "poetry.lock"],
        &["add", "remove", "install", "update"],
    ),
    (
        "bun",
        &["package.json", "bun.lockb"],
        &["add", "remove", "install", "update"],
    ),
    (
        "npm",
        &["package.json", "package-lock.json"],
        &["install", "uninstall", "update", "ci"],
    ),
    (
        "yarn",
        &["package.json", "yarn.lock"],
        &["add", "remove", "install", "upgrade"],
    ),
    (
        "pnpm",
        &["package.json", "pnpm-lock.yaml"],
        &["add", "remove", "install", "update"],
    ),
    (
        "cargo",
        &["Cargo.toml"],
        &["add", "remove", "install", "update"],
    ),
    ("go", &["go.mod"], &["get", "install", "mod tidy"]),
    ("gem", &["Gemfile"], &["install", "update"]),
    ("bundle", &["Gemfile"], &["install", "update", "add"]),
    (
        "composer",
        &["composer.json"],
        &["require", "remove", "install", "update"],
    ),
    (
        "dart",
        &["pubspec.yaml"],
        &["pub add", "pub remove", "pub get", "pub upgrade"],
    ),
    (
        "flutter",
        &["pubspec.yaml"],
        &["pub add", "pub remove", "pub get", "pub upgrade"],
    ),
    (
        "swift",
        &["Package.swift"],
        &["package resolve", "package update"],
    ),
    ("gradle", &["build.gradle", "build.gradle.kts"], &[]), // gradle deps are declarative, no mutation CLI
    ("mvn", &["pom.xml"], &["install", "dependency:resolve"]),
    ("dotnet", &["*.csproj"], &["add", "remove", "restore"]),
    ("mix", &["mix.exs"], &["deps.get", "deps.update"]),
    ("zig", &["build.zig.zon"], &["fetch"]),
    (
        "conan",
        &["conanfile.txt", "conanfile.py"],
        &["install", "create"],
    ),
    ("cmake", &["CMakeLists.txt"], &[]), // cmake deps are declarative
    ("vcpkg", &["vcpkg.json"], &["install", "remove", "update"]),
];

use crate::Sandbox;

/// Securely runs subprocess shell commands for the MCP Agent Sandbox
pub struct ShellEngine {
    pub workspace_root: PathBuf,
    sandbox: Sandbox,
}

impl ShellEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = workspace_root.as_ref().to_path_buf();
        Self {
            workspace_root: std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone()),
            sandbox: Sandbox::new(root),
        }
    }

    /// Execute an arbitrary shell command safely within the workspace directory or an override
    pub async fn shell(&self, command: &str, cwd_override: Option<&Path>) -> Result<Value> {
        let command = command.trim();
        if command.is_empty() {
            return Err(anyhow::anyhow!("Command must not be empty."));
        }

        // Robust command parsing: split by common shell delimiters, respecting quotes
        let command_chains = split_command_chains(command);

        for chain in &command_chains {
            let chain = chain.trim();
            if chain.is_empty() {
                continue;
            }
            self.validate_command(chain)?;
            self.check_package_manager_policy(chain)?;
        }

        let cwd = cwd_override.unwrap_or(&self.workspace_root);
        let output = if cfg!(target_os = "windows") {
            let mut cmd = self.sandbox.build_command("cmd", &["/C".to_string(), command.to_string()]);
            cmd.current_dir(cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| anyhow::anyhow!("Sandbox execution failed: {}", e))?
        } else {
            let mut cmd = self
                .sandbox
                .build_command("sh", &["-c".to_string(), command.to_string()]);
            cmd.current_dir(cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| anyhow::anyhow!("Sandbox execution failed: {}", e))?
        };

        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
        let status_code = output.status.code().unwrap_or(-1);

        Ok(json!({
            "command": command,
            "stdout": stdout_str,
            "stderr": stderr_str,
            "exit_code": status_code,
        }))
    }

    /// Validates a command against the sandbox policy.
    pub fn validate_command(&self, command: &str) -> Result<()> {
        // ── Hard blocklist: binaries that are NEVER allowed ──
        let hard_blocked = [
            "rm", "sudo", "su", "wget", "curl", "chmod", "chown", "chgrp", "mkfs", "dd",
            "shutdown", "reboot",
            "powershell", "powershell.exe", "pwsh", "pwsh.exe", "cmd", "cmd.exe",
            "bash", "sh", "zsh", "fish", "dash", "ksh", "csh", "tcsh",
            "del", "erase", "rmdir", "rd", "format", "diskpart",
            "curd", // Prevent inception / self-modification by the agent
        ];

        let config = crate::config::CurdConfig::load_from_workspace(&self.workspace_root);

        // ── Soft blocklist: binaries the agent should use CURD tools for ──
        let soft_blocked = ["sed", "awk", "cat", "mv", "cp"];

        // ── Path traversal prevention ──
        if command.contains("..") || command.contains('~') {
            return Err(anyhow::anyhow!(
                "Command contains path traversal ('..', '~'). Shell execution is strictly sandboxed to the workspace root."
            ));
        }

        // Detect absolute paths (/...) that are not part of a flag (e.g. --path=/...)
        // We use a simple whitespace-aware check for any segment starting with '/'
        for part in command.split_whitespace() {
            if part.starts_with('/') && !part.contains('=') {
                return Err(anyhow::anyhow!(
                    "Command contains absolute paths ('/'). Shell execution is strictly sandboxed to the workspace root."
                ));
            }
        }

        // Also check if the command itself starts with / (command.split_whitespace() covers this, but being explicit)
        if command.trim().starts_with('/') {
            return Err(anyhow::anyhow!(
                "Command starts with absolute path. Shell execution is strictly sandboxed to the workspace root."
            ));
        }

        let parts = split_shell_like(command)?;
        if parts.is_empty() {
            return Ok(());
        }

        // Robust binary identification: skip leading environment variable assignments (e.g. VAR=val)
        let mut parts_iter = parts.iter();
        let mut binary = parts_iter.next().map(|s| s.as_str()).unwrap_or("");
        while binary.contains('=') && !binary.starts_with('-') {
            binary = parts_iter.next().map(|s| s.as_str()).unwrap_or("");
        }

        if binary.is_empty() {
            return Ok(());
        }

        // Explicit user allowlist overrides both raw paths and soft/hard bans
        if config.shell.allowlist.contains(&binary.to_string()) {
            return Ok(());
        }

        // Block executing files directly via relative paths (e.g. ./abc, .\abc)
        if binary.starts_with("./") || binary.starts_with(".\\") {
            return Err(anyhow::anyhow!(
                "Command '{}' is blocked. Direct execution of local binaries or scripts via relative paths is prohibited in the sandbox.",
                binary
            ));
        }

        // Check hard blocklist
        if hard_blocked.contains(&binary) {
            return Err(anyhow::anyhow!(
                "Command '{}' is blocked by the Sandbox policy. This binary is never allowed inside the sandbox.",
                binary
            ));
        }

        // Check soft blocklist
        if soft_blocked.contains(&binary) {
            return Err(anyhow::anyhow!(
                "Command '{}' is blocked. Use CURD native tools instead: `edit` for modifications, `read` for viewing, `manage_file` for moving/copying.",
                binary
            ));
        }

        Ok(())
    }

    /// Check if a package manager mutation command is allowed.
    ///
    /// Policy:
    /// - Running read-only commands (e.g. `cargo build`, `npm run test`) → always allowed
    /// - Running mutation commands (e.g. `cargo add`, `npm install`) → ONLY if the config
    ///   file exists directly at the workspace root (not inherited from a parent directory)
    pub fn check_package_manager_policy(&self, command: &str) -> Result<()> {
        let parts = split_shell_like(command)?;
        if parts.is_empty() {
            return Ok(());
        }

        // Skip environment variables to find the real package manager binary
        let mut parts_iter = parts.iter();
        let mut binary = parts_iter.next().map(|s| s.as_str()).unwrap_or("");
        while binary.contains('=') && !binary.starts_with('-') {
            binary = parts_iter.next().map(|s| s.as_str()).unwrap_or("");
        }
        let remaining_args: Vec<&str> = parts_iter.map(|s| s.as_str()).collect();

        for (pkg_bin, config_files, mutation_cmds) in PKG_MANAGERS {
            if binary != *pkg_bin {
                continue;
            }

            // Check if any of the remaining args contain a mutation subcommand
            let mut sub_parts = remaining_args.as_slice();
            while !sub_parts.is_empty() && sub_parts[0].starts_with('-') {
                sub_parts = &sub_parts[1..];
            }

            let is_mutation = !sub_parts.is_empty()
                && mutation_cmds.iter().any(|mc| {
                    // Handle multi-word subcommands like "pip install"
                    let mc_parts: Vec<&str> = mc.split_whitespace().collect();
                    sub_parts.len() >= mc_parts.len()
                        && mc_parts.iter().zip(sub_parts.iter()).all(|(a, b)| a == b)
                });

            if !is_mutation {
                return Ok(()); // Read-only command, always fine
            }

            // Mutation detected — check if config file exists at workspace root (NOT a parent)
            let config_at_root = config_files.iter().any(|cf| {
                if cf.contains('*') {
                    if let Ok(entries) = std::fs::read_dir(&self.workspace_root) {
                        return entries
                            .flatten()
                            .any(|e| pattern_matches_file(cf, &e.path()));
                    }
                    false
                } else {
                    self.workspace_root.join(cf).exists()
                }
            });

            if !config_at_root {
                return Err(anyhow::anyhow!(
                    "Cannot run '{}' — no config file ({}) found at workspace root. \
                     This workspace may be a subdirectory of a project managed elsewhere. \
                     Dependency mutations are only allowed when the package manager config exists at the workspace root.",
                    command,
                    config_files.join(" or ")
                ));
            }

            return Ok(());
        }

        Ok(())
    }
}

pub fn split_command_chains(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    let delimiters = [';', '&', '|', '`', '$', '(', ')', '<', '>', '\n', '\r'];

    for ch in command.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => {
                cur.push(ch);
                escaped = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
                cur.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                cur.push(ch);
            }
            c if !in_single && !in_double && delimiters.contains(&c) => {
                if !cur.trim().is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }

    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

pub fn split_shell_like(command: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in command.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => {
                escaped = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }

    if escaped || in_single || in_double {
        anyhow::bail!("Invalid command quoting");
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

pub fn parse_command(command: &str) -> Result<(String, Vec<String>)> {
    let parts = split_shell_like(command)?;
    if parts.is_empty() {
        anyhow::bail!("Command must not be empty");
    }
    let mut iter = parts.into_iter();
    let program = iter.next().unwrap_or_default();
    let args: Vec<String> = iter.collect();
    Ok((program, args))
}

/// Robust, non-shell command existence check.
/// Mitigates shell injection by avoiding `sh -c` and uses absolute path resolution.
pub fn command_exists(command: &str, cwd: &Path) -> bool {
    if command.is_empty() || command.contains(std::path::is_separator) {
        return Path::new(command).exists();
    }

    if cfg!(target_os = "windows") {
        Command::new("where")
            .arg("--")
            .arg(command)
            .current_dir(cwd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s: std::process::ExitStatus| s.success())
            .unwrap_or(false)
    } else {
        // Use 'which' as a standalone binary, avoiding shell builtins and injection.
        Command::new("which")
            .arg("--")
            .arg(command)
            .current_dir(cwd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s: std::process::ExitStatus| s.success())
            .unwrap_or(false)
    }
}

fn pattern_matches_file(pattern: &str, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }
    name == pattern
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_empty_command_rejected() {
        let engine = ShellEngine::new(".");
        let err = engine.shell("   ", None).await.unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn test_pattern_matches_suffix_wildcard() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("demo.csproj");
        std::fs::write(&path, "<Project />").unwrap();
        assert!(pattern_matches_file("*.csproj", &path));
        assert!(!pattern_matches_file("*.toml", &path));
    }

    #[test]
    fn test_split_command_chains_respects_quotes() {
        let chains = split_command_chains("echo \";\" ; rm -rf /");
        assert_eq!(chains.len(), 2);
        assert_eq!(chains[0].trim(), "echo \";\"");
        assert_eq!(chains[1].trim(), "rm -rf /");
    }

    #[test]
    fn test_validate_command_blocks_malicious_chains() {
        let engine = ShellEngine::new(".");
        assert!(engine.validate_command("echo hello ; rm -rf /").is_err());
        assert!(
            engine
                .validate_command("python -c \"import os; os.system('ls')\"")
                .is_ok()
        );
        assert!(
            engine
                .validate_command("python -c \"import os; os.system('rm -rf /')\"")
                .is_err()
        );
        assert!(engine.validate_command("python & rm -rf /").is_err());
    }
}
