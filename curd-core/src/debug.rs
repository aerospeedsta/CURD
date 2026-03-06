use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, ChildStdout};
use uuid::Uuid;

use crate::Sandbox;

pub struct DebugEngine {
    pub workspace_root: PathBuf,
    sandbox: Sandbox,
}

impl DebugEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = workspace_root.as_ref().to_path_buf();
        Self {
            workspace_root: std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone()),
            sandbox: Sandbox::new(root),
        }
    }

    pub async fn debug(
        &self,
        language: &str,
        snippet: &str,
        target: Option<&str>,
        target_args: &[String],
    ) -> Result<Value> {
        if let Some(t) = target {
            let graph = crate::GraphEngine::new(&self.workspace_root);
            if graph.is_poisoned(t) {
                anyhow::bail!(
                    "Execution Blocked: Target '{}' is in a poisoned state (compile-time fault detected). Resolve the fault before debugging.",
                    t
                );
            }
        }

        if snippet.trim().is_empty() {
            anyhow::bail!("Snippet must not be empty.");
        }

        let lang = language.to_lowercase();
        let (cmd, args): (&str, Vec<String>) = match lang.as_str() {
            "python" => ("python", vec!["-c".to_string(), snippet.to_string()]),
            "node" | "javascript" => ("node", vec!["-e".to_string(), snippet.to_string()]),
            "ruby" => ("ruby", vec!["-e".to_string(), snippet.to_string()]),
            "php" => ("php", vec!["-r".to_string(), snippet.to_string()]),
            "lua" => ("lua", vec!["-e".to_string(), snippet.to_string()]),
            "julia" => ("julia", vec!["-e".to_string(), snippet.to_string()]),
            "r" => ("Rscript", vec!["-e".to_string(), snippet.to_string()]),
            "elixir" => ("elixir", vec!["-e".to_string(), snippet.to_string()]),
            "haskell" | "ghci" => ("ghci", vec!["-e".to_string(), snippet.to_string()]),
            "java" | "jshell" => {
                let script = self
                    .workspace_root
                    .join(".curd")
                    .join("tmp")
                    .join(format!("curd_debug_{}.jsh", Uuid::new_v4()));
                if let Some(parent) = script.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                tokio::fs::write(&script, format!("{}\n/exit\n", snippet)).await?;

                let mut command = self
                    .sandbox
                    .build_command("jshell", &[script.to_string_lossy().to_string()]);
                let output = command.current_dir(&self.workspace_root).output().await?;

                let _ = tokio::fs::remove_file(&script).await;
                return Ok(json!({
                    "language": language,
                    "target": target,
                    "status": if output.status.success() { "ok" } else { "error" },
                    "exit_code": output.status.code().unwrap_or(-1),
                    "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                }));
            }
            "gdb" => {
                let target_str = target.ok_or_else(|| {
                    anyhow::anyhow!("'target' is required for gdb mode (path to binary)")
                })?;
                let bin = self.validate_debug_target(target_str)?;

                let mut a = vec!["-q".to_string(), "-batch".to_string()];
                for line in snippet.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
                    a.push("-ex".to_string());
                    a.push(line.to_string());
                }
                a.push("--args".to_string());
                a.push(bin);
                a.extend(target_args.iter().cloned());
                ("gdb", a)
            }
            "lldb" => {
                let target_str = target.ok_or_else(|| {
                    anyhow::anyhow!("'target' is required for lldb mode (path to binary)")
                })?;
                let bin = self.validate_debug_target(target_str)?;

                let mut a = vec!["-b".to_string(), "-Q".to_string()];
                for line in snippet.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
                    a.push("-o".to_string());
                    a.push(line.to_string());
                }
                a.push("--".to_string());
                a.push(bin);
                a.extend(target_args.iter().cloned());
                ("lldb", a)
            }
            "jdb" => {
                let target_str = target.ok_or_else(|| {
                    anyhow::anyhow!("'target' is required for jdb mode (class or main target)")
                })?;
                let bin = self.validate_debug_target(target_str)?;

                let mut a = vec!["-sourcepath".to_string(), ".".to_string(), bin];
                a.extend(target_args.iter().cloned());
                ("jdb", a)
            }
            "dlv" => {
                let target_str = target.ok_or_else(|| {
                    anyhow::anyhow!("'target' is required for dlv mode (binary or package)")
                })?;
                let bin = self.validate_debug_target(target_str)?;

                let mut a = vec!["exec".to_string(), bin, "--".to_string()];
                a.extend(target_args.iter().cloned());
                ("dlv", a)
            }
            _ => {
                anyhow::bail!(
                    "Unsupported debug language '{}'. Supported: python, node/javascript, ruby, php, lua, julia, r, elixir, haskell/ghci, java/jshell, gdb, lldb, jdb, dlv",
                    language
                )
            }
        };

        if !command_exists(cmd, &self.workspace_root) {
            anyhow::bail!("Interpreter '{}' not found in PATH.", cmd);
        }

        let mut command = self.sandbox.build_command(cmd, &args);
        let output = command
            .current_dir(&self.workspace_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok(json!({
            "language": language,
            "target": target,
            "status": if output.status.success() { "ok" } else { "error" },
            "exit_code": output.status.code().unwrap_or(-1),
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
        }))
    }

    fn validate_debug_target(&self, target: &str) -> Result<String> {
        if target.contains("..") || target.contains('~') || target.starts_with('/') {
            let resolved = crate::workspace::validate_sandboxed_path(&self.workspace_root, target)?;
            return Ok(resolved.to_string_lossy().to_string());
        }
        if target.contains('/') || target.contains('\\') {
            let resolved = crate::workspace::validate_sandboxed_path(&self.workspace_root, target)?;
            return Ok(resolved.to_string_lossy().to_string());
        }
        Ok(target.to_string())
    }

    pub fn backends(&self) -> Value {
        let specs = vec![
            ("python", "python", "repl", false),
            ("node", "node", "repl", false),
            ("javascript", "node", "repl", false),
            ("ruby", "ruby", "repl", false),
            ("php", "php", "repl", false),
            ("lua", "lua", "repl", false),
            ("julia", "julia", "repl", false),
            ("r", "Rscript", "repl", false),
            ("elixir", "elixir", "repl", false),
            ("haskell", "ghci", "repl", false),
            ("ghci", "ghci", "repl", false),
            ("java", "jshell", "repl", false),
            ("jshell", "jshell", "repl", false),
            ("gdb", "gdb", "debugger", true),
            ("lldb", "lldb", "debugger", true),
            ("jdb", "jdb", "debugger", true),
            ("dlv", "dlv", "debugger", true),
        ];
        let backends: Vec<Value> = specs
            .into_iter()
            .map(|(name, command, engine_type, requires_target)| {
                json!({
                    "name": name,
                    "command": command,
                    "type": engine_type,
                    "available": command_exists(command, &self.workspace_root),
                    "requires_target": requires_target,
                })
            })
            .collect();
        json!({
            "workspace_root": self.workspace_root,
            "backends": backends
        })
    }

    pub async fn start_session(
        &self,
        language: &str,
        target: Option<&str>,
        target_args: &[String],
    ) -> Result<Value> {
        if language.trim().is_empty() {
            anyhow::bail!("language must not be empty");
        }
        let id = next_session_id();
        let stateful_replay = supports_stateful_replay(language);
        let session = DebugSession {
            language: language.to_string(),
            target: target.map(ToString::to_string),
            target_args: target_args.to_vec(),
            history: Vec::new(),
            last_output: None,
            stateful_replay,
        };
        sessions()
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .insert(id, session);
        Ok(json!({
            "session_id": id,
            "status": "started",
            "session_mode": if stateful_replay { "stateful_replay" } else { "stateless_history" },
            "note": if stateful_replay {
                "Session replays prior snippets on each request to preserve interpreter state semantics."
            } else {
                "Session preserves request history, but debugger-style backends run statelessly per request."
            },
            "language": language,
            "target": target,
            "target_args": target_args
        }))
    }

    pub async fn send_session(&self, session_id: u64, snippet: &str) -> Result<Value> {
        let (language, target, target_args) = {
            let guard = sessions()
                .lock()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let s = guard
                .get(&session_id)
                .ok_or_else(|| anyhow::anyhow!("Unknown debug session_id {}", session_id))?;
            (s.language.clone(), s.target.clone(), s.target_args.clone())
        };

        let replay_history = {
            let guard = sessions()
                .lock()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            guard
                .get(&session_id)
                .map(|s| s.stateful_replay)
                .unwrap_or(false)
        };

        let out = if replay_history {
            let full_snippet = {
                let guard = sessions()
                    .lock()
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                let s = guard
                    .get(&session_id)
                    .ok_or_else(|| anyhow::anyhow!("Unknown debug session_id {}", session_id))?;
                if s.history.is_empty() {
                    snippet.to_string()
                } else {
                    format!("{}\n{}", s.history.join("\n"), snippet)
                }
            };
            self.debug(&language, &full_snippet, target.as_deref(), &target_args)
                .await?
        } else {
            self.debug(&language, snippet, target.as_deref(), &target_args)
                .await?
        };

        let mut guard = sessions()
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let s = guard
            .get_mut(&session_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown debug session_id {}", session_id))?;
        s.history.push(snippet.to_string());
        s.last_output = Some(out.clone());

        Ok(json!({
            "session_id": session_id,
            "status": "ok",
            "session_mode": if s.stateful_replay { "stateful_replay" } else { "stateless_history" },
            "history_len": s.history.len(),
            "result": out
        }))
    }

    pub fn recv_session(&self, session_id: u64) -> Result<Value> {
        let guard = sessions()
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let s = guard
            .get(&session_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown debug session_id {}", session_id))?;
        Ok(json!({
            "session_id": session_id,
            "session_mode": if s.stateful_replay { "stateful_replay" } else { "stateless_history" },
            "language": s.language,
            "target": s.target,
            "target_args": s.target_args,
            "history_len": s.history.len(),
            "history": s.history,
            "last_output": s.last_output
        }))
    }

    pub fn stop_session(&self, session_id: u64) -> Result<Value> {
        let mut guard = sessions()
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let removed = guard.remove(&session_id);
        Ok(json!({
            "session_id": session_id,
            "status": if removed.is_some() { "stopped" } else { "not_found" }
        }))
    }
}

#[derive(Debug, Clone)]
struct DebugSession {
    language: String,
    target: Option<String>,
    target_args: Vec<String>,
    history: Vec<String>,
    last_output: Option<Value>,
    stateful_replay: bool,
}

fn sessions() -> &'static Mutex<HashMap<u64, DebugSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<u64, DebugSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_session_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

use crate::shell::command_exists;

fn supports_stateful_replay(language: &str) -> bool {
    matches!(
        language.to_lowercase().as_str(),
        "python"
            | "node"
            | "javascript"
            | "ruby"
            | "php"
            | "lua"
            | "julia"
            | "r"
            | "elixir"
            | "haskell"
            | "ghci"
            | "java"
            | "jshell"
    )
}

// ── Sandboxed DAP Infrastructure ────────────────────────────────────────

pub enum DapMessage {
    Request(Value),
    Response(Value),
    Event(Value),
}

pub struct AsyncRpcFramer<R> {
    reader: BufReader<R>,
}

impl<R: tokio::io::AsyncRead + Unpin> AsyncRpcFramer<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }

    pub async fn next_message(&mut self) -> Result<Value> {
        let mut content_length = 0;
        let mut line = String::new();

        loop {
            line.clear();
            self.reader.read_line(&mut line).await?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(val) = trimmed.to_lowercase().strip_prefix("content-length:") {
                content_length = val
                    .trim()
                    .parse::<usize>()
                    .context("Invalid Content-Length")?;
            }
        }

        if content_length == 0 {
            anyhow::bail!("Missing or zero Content-Length header");
        }

        let mut body = vec![0u8; content_length];
        self.reader.read_exact(&mut body).await?;
        Ok(serde_json::from_slice(&body)?)
    }
}

pub struct SandboxedDapClient {
    child: Child,
    pub framer: AsyncRpcFramer<ChildStdout>,
}

impl SandboxedDapClient {
    pub async fn spawn(
        engine: &DebugEngine,
        cmd: &str,
        args: &[String],
        _target_bin: Option<&str>,
    ) -> Result<Self> {
        let mut command = engine.sandbox.build_command(cmd, args);

        let mut child = command
            .current_dir(&engine.workspace_root)
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn sandboxed debug process")?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;

        Ok(Self {
            child,
            framer: AsyncRpcFramer::new(stdout),
        })
    }

    pub async fn kill(&mut self) -> Result<()> {
        self.child
            .kill()
            .await
            .context("Failed to kill debug process")
    }
}
