use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::Mutex;
use std::io::{BufRead, BufReader, Write};
use crate::Sandbox;

#[derive(Serialize, Debug)]
#[serde(tag = "method", content = "params")]
pub enum PluginRequest {
    #[serde(rename = "load_grammar")]
    LoadGrammar {
        language_id: String,
        plugin_path: String,
        function_name: String,
    },
    #[serde(rename = "parse")]
    Parse {
        language_id: String,
        file_path: String,
        source_code: String,
        query_src: String,
    },
}

#[derive(Deserialize, Debug)]
pub struct PluginResponse {
    pub status: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}

pub struct PluginClient {
    workspace_root: PathBuf,
    inner: Mutex<Option<PluginClientInner>>,
    pub loaded_grammars: Mutex<std::collections::HashSet<String>>,
}

struct PluginClientInner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl PluginClient {
    pub fn new(workspace_root: &Path) -> Result<Self> {
        let client = Self {
            workspace_root: workspace_root.to_path_buf(),
            inner: Mutex::new(None),
            loaded_grammars: Mutex::new(std::collections::HashSet::new()),
        };
        client.ensure_inner()?;
        Ok(client)
    }

    fn ensure_inner(&self) -> Result<()> {
        let mut inner_guard = self.inner.lock().unwrap();
        if inner_guard.is_some() {
            // Check if child is still alive
            if let Some(inner) = inner_guard.as_mut() {
                if let Ok(Some(_status)) = inner.child.try_wait() {
                    // Process died, clear it
                    *inner_guard = None;
                } else {
                    return Ok(());
                }
            }
        }

        let exe = std::env::current_exe().context("Failed to get current executable path")?;
        let host_bin = exe.with_file_name("curd-plugin-host");
        
        if !host_bin.exists() {
            anyhow::bail!("Sidecar binary not found at {}", host_bin.display());
        }

        let sandbox = Sandbox::new(&self.workspace_root);
        let mut cmd = sandbox.build_std_command(host_bin.to_str().unwrap(), &[]);
        cmd.current_dir(&self.workspace_root);

        cmd.stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::inherit());

        let mut child = cmd.spawn().context("Failed to spawn curd-plugin-host sidecar")?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("Failed to open sidecar stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("Failed to open sidecar stdout"))?;

        *inner_guard = Some(PluginClientInner {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        });

        // If we had previously loaded grammars, we need to reload them in the new process
        // (This would happen on the next parse call lazily)
        self.loaded_grammars.lock().unwrap().clear();

        Ok(())
    }

    pub fn send_request(&self, req: &PluginRequest) -> Result<PluginResponse> {
        self.ensure_inner()?;
        let mut inner_guard = self.inner.lock().unwrap();
        let inner = inner_guard.as_mut().unwrap();

        let req_json = serde_json::to_string(req)?;
        
        // Write with timeout-like behavior via non-blocking or just standard io
        // Since we are using std::process::Child, we don't have easy async timeouts here
        // but we can use the reader's behavior.
        writeln!(inner.stdin, "{}", req_json)?;
        inner.stdin.flush()?;

        let mut response_line = String::new();
        // Heuristic: If parsing a single file takes > 30s, something is wrong
        // For now, we'll rely on the fact that sidecar is sandboxed and limited.
        inner.stdout.read_line(&mut response_line)?;

        if response_line.trim().is_empty() {
            anyhow::bail!("Sidecar process terminated unexpectedly.");
        }

        let resp: PluginResponse = serde_json::from_str(&response_line)?;
        Ok(resp)
    }

    pub fn load_grammar(&self, language_id: &str, plugin_path: &str, function_name: &str) -> Result<()> {
        let mut loaded = self.loaded_grammars.lock().unwrap();
        if loaded.contains(language_id) {
            return Ok(());
        }

        let cfg = crate::config::CurdConfig::load_from_workspace(&self.workspace_root);
        let lpe = crate::LangPluginEngine::new(&self.workspace_root, cfg.plugins.clone());
        lpe.validate_plugin_library(Path::new(plugin_path), language_id, function_name)?;

        let req = PluginRequest::LoadGrammar {
            language_id: language_id.to_string(),
            plugin_path: plugin_path.to_string(),
            function_name: function_name.to_string(),
        };
        let resp = self.send_request(&req)?;
        if resp.status == "ok" {
            loaded.insert(language_id.to_string());
            Ok(())
        } else {
            Err(anyhow::anyhow!("Plugin load failed: {}", resp.error.unwrap_or_default()))
        }
    }

    pub fn parse(&self, language_id: &str, file_path: &str, source_code: &str, query_src: &str) -> Result<Vec<crate::Symbol>> {
        let req = PluginRequest::Parse {
            language_id: language_id.to_string(),
            file_path: file_path.to_string(),
            source_code: source_code.to_string(),
            query_src: query_src.to_string(),
        };
        let resp = self.send_request(&req)?;
        if resp.status == "ok" {
            if let Some(res) = resp.result {
                let symbols: Vec<crate::Symbol> = serde_json::from_value(res["symbols"].clone())?;
                Ok(symbols)
            } else {
                Ok(Vec::new())
            }
        } else {
            Err(anyhow::anyhow!("Parse failed: {}", resp.error.unwrap_or_default()))
        }
    }
}

impl Drop for PluginClient {
    fn drop(&mut self) {
        if let Ok(mut inner_guard) = self.inner.lock() {
            if let Some(mut inner) = inner_guard.take() {
                let _ = inner.child.kill();
                let _ = inner.child.wait();
            }
        }
    }
}
