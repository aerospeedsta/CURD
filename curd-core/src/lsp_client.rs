use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub struct LspClient {
    child: Child,
    request_id: u64,
}

impl LspClient {
    pub fn new(cmd: &str, args: &[&str], workspace_root: &Path) -> Result<Self> {
        let child = Command::new(cmd)
            .args(args)
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start LSP server")?;

        Ok(Self {
            child,
            request_id: 1,
        })
    }

    fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.request_id;
        self.request_id += 1;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.send_payload(req)?;

        self.wait_for_response(id)
    }

    fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.send_payload(req)
    }

    fn send_payload(&mut self, payload: Value) -> Result<()> {
        let json_str = serde_json::to_string(&payload)?;
        let msg = format!("Content-Length: {}\r\n\r\n{}", json_str.len(), json_str);
        if let Some(stdin) = self.child.stdin.as_mut() {
            stdin.write_all(msg.as_bytes())?;
            stdin.flush()?;
        }
        Ok(())
    }

    fn wait_for_response(&mut self, expected_id: u64) -> Result<Value> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("LSP stdout pipe is unavailable"))?;
        let mut reader = BufReader::new(stdout);

        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            if line.is_empty() {
                return Err(anyhow::anyhow!("LSP stream closed unexpectedly"));
            }
            if line.starts_with("Content-Length: ") {
                let len_str = line["Content-Length: ".len()..].trim();
                let len: usize = len_str.parse()?;

                // Read empty line
                reader.read_line(&mut line)?;

                let mut buf = vec![0; len];
                reader.read_exact(&mut buf)?;

                let resp: Value = serde_json::from_slice(&buf)?;
                if let Some(id) = resp.get("id").and_then(|i| i.as_u64())
                    && id == expected_id
                {
                    if let Some(err) = resp.get("error") {
                        return Err(anyhow::anyhow!("LSP Error: {}", err));
                    }
                    return Ok(resp.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        }
    }

    pub fn initialize(&mut self, workspace_root: &Path) -> Result<()> {
        let uri = format!("file://{}", workspace_root.display());
        self.send_request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": uri,
                "capabilities": {
                    "workspace": {
                        "workspaceEdit": {
                            "documentChanges": true
                        }
                    }
                }
            }),
        )?;
        self.send_notification("initialized", json!({}))?;
        Ok(())
    }

    pub fn rename(
        &mut self,
        file_path: &Path,
        line: usize,
        col: usize,
        new_name: &str,
    ) -> Result<Value> {
        let uri = format!("file://{}", file_path.display());
        self.send_request(
            "textDocument/rename",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": col },
                "newName": new_name
            }),
        )
    }
}
