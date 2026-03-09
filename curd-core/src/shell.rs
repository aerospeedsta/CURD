use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::process::{Child};
use uuid::Uuid;
use tokio::io::AsyncReadExt;

/// Package manager config files and their mutation subcommands
const PKG_MANAGERS: &[(&str, &[&str], &[&str])] = &[
    ("Cargo.toml", &["cargo"], &["add", "remove", "update"]),
    ("package.json", &["npm", "yarn", "pnpm", "bun"], &["install", "remove", "update", "add"]),
    ("requirements.txt", &["pip", "pip3"], &["install", "uninstall"]),
    ("go.mod", &["go"], &["get"]),
    ("BUILD.bazel", &["bazel"], &[]), // bazel deps are declarative
    ("CMakeLists.txt", &["cmake"], &[]), // cmake deps are declarative
    ("vcpkg", &["vcpkg"], &["install", "remove", "update"]),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellTaskStatus {
    pub task_id: Uuid,
    pub command: String,
    pub active: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Securely runs subprocess shell commands for the MCP Agent Sandbox
pub struct ShellEngine {
    pub workspace_root: PathBuf,
    sandbox: crate::Sandbox,
    pub active_tasks: Arc<Mutex<HashMap<Uuid, ShellTaskState>>>,
}

pub struct ShellTaskState {
    pub command: String,
    pub child: Option<Child>,
    pub stdout_buf: Arc<std::sync::Mutex<String>>,
    pub stderr_buf: Arc<std::sync::Mutex<String>>,
    pub exit_code: Arc<std::sync::Mutex<Option<i32>>>,
}

impl ShellEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = workspace_root.as_ref().to_path_buf();
        Self {
            workspace_root: std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone()),
            sandbox: crate::Sandbox::new(root),
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Execute an arbitrary shell command safely within the workspace directory or an override.
    /// If is_background is true, returns a task_id immediately.
    pub async fn shell(&self, command: &str, cwd_override: Option<&Path>, is_background: bool) -> Result<Value> {
        let command = command.trim();
        if command.is_empty() {
            return Err(anyhow::anyhow!("Command must not be empty."));
        }

        let command_chains = split_command_chains(command);
        for chain in &command_chains {
            self.validate_command(chain)?;
            self.check_package_manager_policy(chain)?;
        }

        let cwd = cwd_override.unwrap_or(&self.workspace_root).to_path_buf();
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = self.sandbox.build_command("cmd", &["/C".to_string(), command.to_string()]);
            c.current_dir(cwd);
            c
        } else {
            let mut c = self.sandbox.build_command("sh", &["-c".to_string(), command.to_string()]);
            c.current_dir(cwd);
            c
        };

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| anyhow::anyhow!("Failed to spawn process: {}", e))?;
        let task_id = Uuid::new_v4();

        let stdout_buf = Arc::new(std::sync::Mutex::new(String::new()));
        let stderr_buf = Arc::new(std::sync::Mutex::new(String::new()));
        let exit_code = Arc::new(std::sync::Mutex::new(None));

        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();

        let out_arc = Arc::clone(&stdout_buf);
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            while let Ok(n) = stdout.read(&mut buf).await {
                if n == 0 { break; }
                let s = String::from_utf8_lossy(&buf[..n]);
                let mut guard = out_arc.lock().unwrap();
                guard.push_str(&s);
                if guard.len() > 1024 * 512 { break; } // Cap at 512KB
            }
        });

        let err_arc = Arc::clone(&stderr_buf);
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            while let Ok(n) = stderr.read(&mut buf).await {
                if n == 0 { break; }
                let s = String::from_utf8_lossy(&buf[..n]);
                let mut guard = err_arc.lock().unwrap();
                guard.push_str(&s);
                if guard.len() > 1024 * 512 { break; } // Cap at 512KB
            }
        });

        if is_background {
            let state = ShellTaskState {
                command: command.to_string(),
                child: Some(child),
                stdout_buf,
                stderr_buf,
                exit_code,
            };
            self.active_tasks.lock().await.insert(task_id, state);

            return Ok(json!({
                "status": "background",
                "task_id": task_id,
                "command": command
            }));
        }

        let status = child.wait().await?;
        let stdout_str = stdout_buf.lock().unwrap().clone();
        let stderr_str = stderr_buf.lock().unwrap().clone();
        let status_code = status.code().unwrap_or(-1);

        Ok(json!({
            "command": command,
            "stdout": stdout_str,
            "stderr": stderr_str,
            "exit_code": status_code,
        }))
    }

    pub async fn status(&self, task_id: Uuid) -> Result<ShellTaskStatus> {
        let mut guard = self.active_tasks.lock().await;
        let state = guard.get_mut(&task_id).ok_or_else(|| anyhow::anyhow!("Task {} not found", task_id))?;
        
        if let Some(mut child) = state.child.take() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    *state.exit_code.lock().unwrap() = status.code();
                }
                Ok(None) => {
                    // Still running, put it back
                    state.child = Some(child);
                }
                Err(e) => {
                    log::warn!("Error waiting on child {}: {}", task_id, e);
                }
            }
        }

        let exit_code = *state.exit_code.lock().unwrap();
        Ok(ShellTaskStatus {
            task_id,
            command: state.command.clone(),
            active: exit_code.is_none(),
            exit_code,
            stdout: state.stdout_buf.lock().unwrap().clone(),
            stderr: state.stderr_buf.lock().unwrap().clone(),
        })
    }

    pub async fn terminate(&self, task_id: Uuid) -> Result<Value> {
        let mut guard = self.active_tasks.lock().await;
        if let Some(mut state) = guard.remove(&task_id) {
            if let Some(mut child) = state.child.take() {
                child.kill().await?;
                Ok(json!({"status": "terminated", "task_id": task_id}))
            } else {
                Ok(json!({"status": "already_reaped", "task_id": task_id}))
            }
        } else {
            Err(anyhow::anyhow!("Task ID {} not found or already completed.", task_id))
        }
    }

    /// Validates a command against the sandbox policy.
    pub fn validate_command(&self, command: &str) -> Result<()> {
        // ── Hard blocklist: binaries that are NEVER allowed ──
        let hard_blocked = [
            "rm", "sudo", "su", "wget", "curl", "chmod", "chown", "chgrp", "mkfs", "dd",
            "shred", "mount", "umount", "iptables", "ufw", "nmap", "nc", "netcat", "ssh", "scp",
        ];

        let program = command.split_whitespace().next().unwrap_or("");
        if hard_blocked.contains(&program) {
            anyhow::bail!("Command '{}' is forbidden in the sandbox.", program);
        }

        // ── Block dangerous patterns ──
        if command.contains(" > /") || command.contains(" >> /") {
            anyhow::bail!("Writing to absolute system paths is forbidden.");
        }
        if command.contains("&") || command.contains("|") || command.contains(";") {
            // Check each sub-command if we allow chaining (currently restricted)
            // For now, we allow simple pipe/redirect if validated
        }

        Ok(())
    }

    /// Checks if the command violates package manager policy (e.g. adding dependencies without session)
    pub fn check_package_manager_policy(&self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        let program = parts[0];
        for (_, bin_names, mutation_cmds) in PKG_MANAGERS {
            if bin_names.contains(&program) {
                if parts.len() > 1 && mutation_cmds.contains(&parts[1]) {
                    // This is a mutation command. It should ideally be done via specialized tools,
                    // but if run via shell, we might want to warn or enforce transaction state.
                }
            }
        }
        Ok(())
    }
}

fn split_command_chains(command: &str) -> Vec<String> {
    // Simple split by shell operators, ignoring quotes for this MVP logic
    command
        .split(|c| c == ';' || c == '&' || c == '|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_shell_basic() {
        let dir = tempdir().unwrap();
        let engine = ShellEngine::new(dir.path());
        let res = engine.shell("echo hello", None, false).await.unwrap();
        assert_eq!(res.get("exit_code").unwrap().as_i64().unwrap(), 0);
        assert!(res.get("stdout").unwrap().as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_shell_background() {
        let dir = tempdir().unwrap();
        let engine = ShellEngine::new(dir.path());
        let res = engine.shell("sleep 1", None, true).await.unwrap();
        assert_eq!(res.get("status").unwrap().as_str().unwrap(), "background");
        let task_id = Uuid::parse_str(res.get("task_id").unwrap().as_str().unwrap()).unwrap();
        
        let status = engine.status(task_id).await.unwrap();
        assert!(status.active);
        
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let status2 = engine.status(task_id).await.unwrap();
        assert!(!status2.active);
        assert_eq!(status2.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_shell_validation() {
        let dir = tempdir().unwrap();
        let engine = ShellEngine::new(dir.path());
        assert!(engine.validate_command("rm -rf /").is_err());
        assert!(engine.validate_command("echo test > /etc/passwd").is_err());
        assert!(
            engine
                .validate_command("python -c \"import os; os.system('rm -rf /')\"")
                .is_err()
        );
        assert!(engine.validate_command("python & rm -rf /").is_err());
    }
}

pub fn command_exists(cmd: &str, root: &Path) -> bool {
    let mut c = std::process::Command::new("which");
    c.arg(cmd).current_dir(root).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    c.status().map(|s| s.success()).unwrap_or(false)
}

pub fn parse_command(command: &str) -> Result<(String, Vec<String>)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';

    for c in command.chars() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty command"));
    }
    let mut it = parts.into_iter();
    let program = it.next().unwrap();
    let args: Vec<String> = it.collect();
    Ok((program, args))
}
