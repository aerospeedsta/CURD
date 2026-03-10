use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolGroupSource {
    ExternalMcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptedToolDescriptor {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGroupRecord {
    pub group_id: String,
    pub description: Option<String>,
    pub source: ToolGroupSource,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub allow_tools: Vec<String>,
    #[serde(default)]
    pub deny_tools: Vec<String>,
    #[serde(default)]
    pub tools: Vec<AdoptedToolDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ToolGroupRegistry {
    #[serde(default)]
    groups: Vec<ToolGroupRecord>,
}

pub struct ToolGroupEngine {
    workspace_root: PathBuf,
    config: crate::config::PluginConfig,
    sessions: Mutex<HashMap<String, ManagedMcpSession>>,
}

struct PersistentMcpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

struct ManagedMcpSession {
    session: PersistentMcpSession,
    restart_count: u32,
    started_at_secs: u64,
    last_restart_at_secs: Option<u64>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGroupSessionStatus {
    pub group_id: String,
    pub healthy: bool,
    pub restart_count: u32,
    pub started_at_secs: Option<u64>,
    pub last_restart_at_secs: Option<u64>,
    pub last_error: Option<String>,
}

impl Drop for ToolGroupEngine {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for (_, managed) in sessions.iter_mut() {
                let _ = managed.session.child.kill();
                let _ = managed.session.child.wait();
            }
        }
    }
}

impl ToolGroupEngine {
    pub fn new(workspace_root: impl AsRef<Path>, config: crate::config::PluginConfig) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
            config,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn list(&self) -> Result<Vec<ToolGroupRecord>> {
        Ok(self.load_registry()?.groups)
    }

    pub fn add_external_mcp_group(
        &self,
        group_id: &str,
        command: &str,
        args: &[String],
        description: Option<String>,
        allow_tools: &[String],
        deny_tools: &[String],
    ) -> Result<ToolGroupRecord> {
        if !self.config.allow_external_mcp_tool_groups {
            anyhow::bail!("external MCP tool groups are disabled by policy");
        }
        validate_group_id(group_id)?;
        if command.trim().is_empty() {
            anyhow::bail!("command must not be empty");
        }
        validate_tool_policy(allow_tools, deny_tools)?;
        let tools = filter_tools_by_policy(
            query_mcp_tools(&self.workspace_root, &self.config, command, args)?,
            allow_tools,
            deny_tools,
        );
        let record = ToolGroupRecord {
            group_id: group_id.to_string(),
            description,
            source: ToolGroupSource::ExternalMcp,
            command: command.to_string(),
            args: args.to_vec(),
            allow_tools: allow_tools.to_vec(),
            deny_tools: deny_tools.to_vec(),
            tools,
        };
        let mut registry = self.load_registry()?;
        registry.groups.retain(|g| g.group_id != group_id);
        registry.groups.push(record.clone());
        registry.groups.sort_by(|a, b| a.group_id.cmp(&b.group_id));
        self.store_registry(&registry)?;
        Ok(record)
    }

    pub fn remove(&self, group_id: &str) -> Result<bool> {
        validate_group_id(group_id)?;
        let mut registry = self.load_registry()?;
        let before = registry.groups.len();
        registry.groups.retain(|g| g.group_id != group_id);
        let removed = before != registry.groups.len();
        self.store_registry(&registry)?;
        Ok(removed)
    }

    pub fn get_doc(&self, tool_name: &str) -> Result<Option<Value>> {
        for group in self.list()? {
            if let Some(tool) = group.tools.iter().find(|tool| tool.name == tool_name) {
                return Ok(Some(json!({
                    "tool": tool.name,
                    "description": tool.description,
                    "parameters_schema": tool.input_schema,
                    "tool_group": {
                        "group_id": group.group_id,
                        "source": "external_mcp",
                        "description": group.description,
                    }
                })));
            }
        }
        Ok(None)
    }

    pub fn session_status(&self, group_id: Option<&str>) -> Result<Vec<ToolGroupSessionStatus>> {
        let groups = self.list()?;
        let mut sessions = self.sessions.lock().unwrap();
        let mut out = Vec::new();
        for group in groups {
            if let Some(requested) = group_id && group.group_id != requested {
                continue;
            }
            let managed = sessions.get_mut(&group.group_id);
            let (healthy, restart_count, started_at_secs, last_restart_at_secs, last_error) =
                if let Some(session) = managed {
                    let healthy = session
                        .session
                        .child
                        .try_wait()
                        .ok()
                        .map(|status| status.is_none())
                        .unwrap_or(false);
                    (
                        healthy,
                        session.restart_count,
                        Some(session.started_at_secs),
                        session.last_restart_at_secs,
                        session.last_error.clone(),
                    )
                } else {
                    (false, 0, None, None, None)
                };
            out.push(ToolGroupSessionStatus {
                group_id: group.group_id,
                healthy,
                restart_count,
                started_at_secs,
                last_restart_at_secs,
                last_error,
            });
        }
        Ok(out)
    }

    pub async fn invoke(&self, tool_name: &str, arguments: &Value) -> Result<Option<Value>> {
        for group in self.list()? {
            if group.tools.iter().any(|tool| tool.name == tool_name) {
                if !tool_allowed_by_policy(tool_name, &group.allow_tools, &group.deny_tools) {
                    anyhow::bail!(
                        "tool '{}' is not permitted by policy for group '{}'",
                        tool_name,
                        group.group_id
                    );
                }
                let response = self.call_group_tool(&group, tool_name, arguments)?;
                return Ok(Some(response));
            }
        }
        Ok(None)
    }

    fn registry_path(&self) -> PathBuf {
        self.workspace_root.join(".curd/tool_groups/registry.json")
    }

    fn load_registry(&self) -> Result<ToolGroupRegistry> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(ToolGroupRegistry::default());
        }
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content).context("Failed to parse tool group registry")
    }

    fn store_registry(&self, registry: &ToolGroupRegistry) -> Result<()> {
        let path = self.registry_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(registry)?)?;
        Ok(())
    }

    fn call_group_tool(
        &self,
        group: &ToolGroupRecord,
        tool_name: &str,
        arguments: &Value,
    ) -> Result<Value> {
        let request = json!({"name": tool_name, "arguments": arguments});
        let first_attempt = {
            let mut sessions = self.sessions.lock().unwrap();
            let session = self.ensure_session_locked(&mut sessions, group)?;
            send_persistent_request(&mut session.session, &self.config, "tools/call", request.clone())
        };
        match first_attempt {
            Ok(response) => {
                if let Ok(mut sessions) = self.sessions.lock()
                    && let Some(session) = sessions.get_mut(&group.group_id)
                {
                    session.last_error = None;
                }
                Ok(response)
            }
            Err(err) => {
                let terminated = {
                    let mut sessions = self.sessions.lock().unwrap();
                    let terminated = sessions
                        .get_mut(&group.group_id)
                        .map(|managed| matches!(managed.session.child.try_wait(), Ok(Some(_))))
                        .unwrap_or(false);
                    if let Some(session) = sessions.get_mut(&group.group_id) {
                        session.last_error = Some(err.to_string());
                    }
                    terminated
                };
                if !terminated {
                    return Err(err);
                }
                let mut sessions = self.sessions.lock().unwrap();
                
                // Track restart count before removing
                let old_restart_count = sessions.get(&group.group_id).map(|s| s.restart_count).unwrap_or(0);
                
                sessions.remove(&group.group_id);
                let session = self.ensure_session_locked(&mut sessions, group)?;
                
                // Increment and carry over restart count
                session.restart_count = old_restart_count + 1;
                session.last_restart_at_secs = Some(now_secs());

                let retry = send_persistent_request(&mut session.session, &self.config, "tools/call", request);
                match retry {
                    Ok(response) => {
                        session.last_error = None;
                        Ok(response)
                    }
                    Err(err) => {
                        session.last_error = Some(err.to_string());
                        Err(err)
                    }
                }
            }
        }
    }

    fn ensure_session_locked<'a>(
        &'a self,
        sessions: &'a mut HashMap<String, ManagedMcpSession>,
        group: &ToolGroupRecord,
    ) -> Result<&'a mut ManagedMcpSession> {
        let needs_restart = sessions
            .get_mut(&group.group_id)
            .map(|managed| matches!(managed.session.child.try_wait(), Ok(Some(_))))
            .unwrap_or(false);
        if needs_restart {
            let mut managed = sessions
                .remove(&group.group_id)
                .ok_or_else(|| anyhow::anyhow!("missing MCP session for group '{}'", group.group_id))?;
            if managed.restart_count >= self.config.external_mcp_max_restarts {
                anyhow::bail!(
                    "external MCP group '{}' exceeded restart budget ({})",
                    group.group_id,
                    self.config.external_mcp_max_restarts
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(
                self.config.external_mcp_restart_backoff_ms,
            ));
            managed.session = spawn_persistent_session(
                &self.workspace_root,
                &self.config,
                &group.command,
                &group.args,
            )?;
            managed.restart_count += 1;
            managed.last_restart_at_secs = Some(crate::plan::now_secs());
            managed.last_error = None;
            sessions.insert(group.group_id.clone(), managed);
        }
        if !sessions.contains_key(&group.group_id) {
            let session = spawn_persistent_session(
                &self.workspace_root,
                &self.config,
                &group.command,
                &group.args,
            )?;
            sessions.insert(
                group.group_id.clone(),
                ManagedMcpSession {
                    session,
                    restart_count: 0,
                    started_at_secs: crate::plan::now_secs(),
                    last_restart_at_secs: None,
                    last_error: None,
                },
            );
        }
        sessions
            .get_mut(&group.group_id)
            .ok_or_else(|| anyhow::anyhow!("failed to create MCP session for group '{}'", group.group_id))
    }
}

fn validate_tool_policy(allow_tools: &[String], deny_tools: &[String]) -> Result<()> {
    let overlap: Vec<&String> = allow_tools
        .iter()
        .filter(|tool| deny_tools.iter().any(|deny| deny == *tool))
        .collect();
    if !overlap.is_empty() {
        anyhow::bail!(
            "tool allow/deny policy overlaps on: {}",
            overlap
                .into_iter()
                .map(|name| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

fn tool_allowed_by_policy(tool_name: &str, allow_tools: &[String], deny_tools: &[String]) -> bool {
    if deny_tools.iter().any(|tool| tool == tool_name) {
        return false;
    }
    allow_tools.is_empty() || allow_tools.iter().any(|tool| tool == tool_name)
}

fn filter_tools_by_policy(
    tools: Vec<AdoptedToolDescriptor>,
    allow_tools: &[String],
    deny_tools: &[String],
) -> Vec<AdoptedToolDescriptor> {
    tools
        .into_iter()
        .filter(|tool| tool_allowed_by_policy(&tool.name, allow_tools, deny_tools))
        .collect()
}

fn query_mcp_tools(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
    command: &str,
    args: &[String],
) -> Result<Vec<AdoptedToolDescriptor>> {
    let response = exchange_mcp(
        workspace_root,
        config,
        command,
        args,
        &[json!({"jsonrpc":"2.0","method":"initialize","params":{},"id":1}), json!({"jsonrpc":"2.0","method":"tools/list","params":{},"id":2})],
    )?;
    let tools = response
        .last()
        .and_then(|value| value.get("result"))
        .and_then(|value| value.get("tools"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("external MCP server did not return tools/list result"))?;
    Ok(tools
        .iter()
        .filter_map(|tool| {
            Some(AdoptedToolDescriptor {
                name: tool.get("name")?.as_str()?.to_string(),
                description: tool
                    .get("description")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string()),
                input_schema: tool.get("inputSchema").cloned().unwrap_or_else(|| json!({})),
            })
        })
        .collect())
}

fn exchange_mcp(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
    command: &str,
    args: &[String],
    messages: &[Value],
) -> Result<Vec<Value>> {
    let sandbox = crate::Sandbox::new(workspace_root);
    let mut child = sandbox
        .build_std_command(command, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn MCP tool group command '{}'", command))?;
    {
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("failed to open MCP stdin"))?;
        for message in messages {
            use std::io::Write;
            writeln!(stdin, "{}", serde_json::to_string(message)?)?;
        }
    }
    let start = std::time::Instant::now();
    loop {
        if let Some(_status) = child.try_wait()? {
            break;
        }
        if start.elapsed().as_secs() >= config.external_mcp_timeout_secs {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "external MCP tool group timed out after {}s",
                config.external_mcp_timeout_secs
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("external MCP tool group failed: {}", stderr.trim());
    }
    if output.stdout.len() > config.external_mcp_max_output_bytes {
        anyhow::bail!(
            "external MCP tool group exceeded max output budget ({} bytes)",
            config.external_mcp_max_output_bytes
        );
    }
    let stdout = String::from_utf8(output.stdout).context("external MCP server emitted non-UTF8 stdout")?;
    let mut out = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("external MCP server emitted invalid JSON line: {}", line))?;
        if value.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
            anyhow::bail!("external MCP response missing jsonrpc=2.0");
        }
        out.push(value);
    }
    if out.is_empty() {
        anyhow::bail!("external MCP server produced no JSON responses");
    }
    Ok(out)
}

fn spawn_persistent_session(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
    command: &str,
    args: &[String],
) -> Result<PersistentMcpSession> {
    let sandbox = crate::Sandbox::new(workspace_root);
    let mut child = sandbox
        .build_std_command(command, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn MCP tool group command '{}'", command))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open MCP stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open MCP stdout"))?;
    let mut session = PersistentMcpSession {
        child,
        stdin,
        stdout: BufReader::new(stdout),
        next_id: 1,
    };
    let _ = send_persistent_request(&mut session, config, "initialize", json!({}))?;
    Ok(session)
}

fn send_persistent_request(
    session: &mut PersistentMcpSession,
    config: &crate::config::PluginConfig,
    method: &str,
    params: Value,
) -> Result<Value> {
    let request_id = session.next_id;
    session.next_id += 1;
    writeln!(
        session.stdin,
        "{}",
        serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": request_id,
        }))?
    )?;
    session.stdin.flush()?;

    let start = std::time::Instant::now();
    let mut line = String::new();
    loop {
        if start.elapsed().as_secs() >= config.external_mcp_timeout_secs {
            anyhow::bail!("external MCP session timed out waiting for response to '{}'", method);
        }
        line.clear();
        let read = session.stdout.read_line(&mut line)?;
        if read == 0 {
            anyhow::bail!("external MCP session terminated before replying to '{}'", method);
        }
        if line.len() > config.external_mcp_max_output_bytes {
            anyhow::bail!("external MCP response line exceeded output budget");
        }
        let value: Value = serde_json::from_str(line.trim())
            .with_context(|| format!("external MCP server emitted invalid JSON line: {}", line.trim()))?;
        if value.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
            anyhow::bail!("external MCP response missing jsonrpc=2.0");
        }
        if value.get("method").and_then(|v| v.as_str()).is_some() {
            continue;
        }
        if value.get("id").and_then(|v| v.as_u64()) == Some(request_id) {
            return Ok(value);
        }
    }
}

fn validate_group_id(group_id: &str) -> Result<()> {
    if group_id.is_empty()
        || !group_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        anyhow::bail!("invalid tool group id '{}'", group_id);
    }
    Ok(())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(unix)]
    #[tokio::test]
    async fn adopts_external_mcp_and_routes_doc_and_call() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let script_path = dir.path().join("fake-mcp.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"jsonrpc":"2.0","method":"notifications/progress","params":{"stage":"boot"}}'
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2026-03-10","serverInfo":{"name":"fake","version":"0.1"}}}'
      ;;
    *'"method":"tools/list"'*)
      echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"foreign_tool","description":"Foreign MCP tool","inputSchema":{"type":"object","properties":{"query":{"type":"string"}}}}]}}'
      ;;
    *'"method":"tools/call"'*)
      echo '{"jsonrpc":"2.0","method":"notifications/log","params":{"msg":"noise"}}'
      echo '{"jsonrpc":"2.0","id":999,"result":{"content":[{"type":"text","text":"wrong"}]}}'
      echo '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"ok"}]}}'
      ;;
  esac
done
"#,
        )
        .unwrap();
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();

        let engine = ToolGroupEngine::new(dir.path(), crate::config::PluginConfig::default());
        let record = engine
            .add_external_mcp_group(
                "foreign",
                script_path.to_str().unwrap(),
                &[],
                Some("Test MCP".to_string()),
                &[],
                &[],
            )
            .unwrap();
        assert_eq!(record.tools.len(), 1);
        let doc = engine.get_doc("foreign_tool").unwrap().unwrap();
        assert_eq!(doc.get("tool").and_then(|v| v.as_str()), Some("foreign_tool"));
        let response = engine.invoke("foreign_tool", &json!({"query":"hello"})).await.unwrap();
        assert!(response.is_some());
        assert_eq!(engine.sessions.lock().unwrap().len(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn applies_allow_and_deny_policy_to_adopted_tools() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let script_path = dir.path().join("fake-mcp.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2026-03-10","serverInfo":{"name":"fake","version":"0.1"}}}'
      ;;
    *'"method":"tools/list"'*)
      echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"kept_tool","inputSchema":{"type":"object"}},{"name":"blocked_tool","inputSchema":{"type":"object"}}]}}'
      ;;
  esac
done
"#,
        )
        .unwrap();
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();

        let engine = ToolGroupEngine::new(dir.path(), crate::config::PluginConfig::default());
        let record = engine
            .add_external_mcp_group(
                "foreign",
                script_path.to_str().unwrap(),
                &[],
                Some("Test MCP".to_string()),
                &["kept_tool".to_string()],
                &["blocked_tool".to_string()],
            )
            .unwrap();
        assert_eq!(record.tools.len(), 1);
        assert_eq!(record.tools[0].name, "kept_tool");
        assert!(engine.get_doc("blocked_tool").unwrap().is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn restarts_persistent_group_within_budget() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let script_path = dir.path().join("restart-mcp.sh");
        let state_file = dir.path().join("restart-mcp.state");
        
        // Initial state
        fs::write(&state_file, "0").unwrap();

        fs::write(
            &script_path,
            format!(r#"#!/bin/sh
state_file="{}"
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":"2026-03-10","serverInfo":{{"name":"fake","version":"0.1"}}}}}}'
      ;;
    *'"method":"tools/list"'*)
      echo '{{"jsonrpc":"2.0","id":2,"result":{{"tools":[{{"name":"foreign_tool","inputSchema":{{"type":"object"}}}}]}}}}'
      ;;
    *'"method":"tools/call"'*)
      count=$(cat "$state_file")
      if [ "$count" = "0" ]; then
        echo "1" > "$state_file"
        echo '{{"jsonrpc":"2.0","id":2,"result":{{"content":[{{"type":"text","text":"dying"}}]}}}}'
        # Exit in background after a slight delay to ensure the parent reads the stdout
        (sleep 0.1 && kill $$) &
      else
        echo '{{"jsonrpc":"2.0","id":2,"result":{{"content":[{{"type":"text","text":"revived"}}]}}}}'
      fi
      ;;
  esac
done
"#, state_file.to_str().unwrap()),
        )
        .unwrap();
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();

        let mut cfg = crate::config::PluginConfig::default();
        cfg.external_mcp_max_restarts = 1;
        cfg.external_mcp_restart_backoff_ms = 1;
        let engine = ToolGroupEngine::new(dir.path(), cfg);
        engine
            .add_external_mcp_group(
                "foreign",
                script_path.to_str().unwrap(),
                &[],
                Some("Test MCP".to_string()),
                &[],
                &[],
            )
            .unwrap();
        
        // First call - should succeed and then the process exits
        let res1 = engine.invoke("foreign_tool", &json!({})).await.unwrap().unwrap();
        assert_eq!(res1["result"]["content"][0]["text"], "dying");
        
        // Second call - should detect termination, restart, and succeed
        let res2 = engine.invoke("foreign_tool", &json!({})).await.unwrap().unwrap();
        assert_eq!(res2["result"]["content"][0]["text"], "revived");

        assert_eq!(engine.sessions.lock().unwrap().len(), 1);
        let status = engine.session_status(Some("foreign")).unwrap();
        assert_eq!(status.len(), 1);
        assert!(status[0].healthy);
        assert_eq!(status[0].restart_count, 1);
    }
}
