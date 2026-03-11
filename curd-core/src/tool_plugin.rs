use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::plugin_packages::{
    InstalledPluginRecord, PluginKind, install_verified_archive, load_installed_plugins,
    remove_installed_plugin, verify_archive_file,
};

pub struct ToolPluginEngine {
    workspace_root: PathBuf,
    config: crate::config::PluginConfig,
}

impl ToolPluginEngine {
    pub fn new(workspace_root: impl AsRef<Path>, config: crate::config::PluginConfig) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
            config,
        }
    }

    pub fn add_archive(&self, archive_path: &Path) -> Result<InstalledPluginRecord> {
        self.ensure_enabled()?;
        let verified = verify_archive_file(archive_path, &self.workspace_root, &self.config)?;
        if verified.archive.manifest.kind != PluginKind::Tool {
            anyhow::bail!("expected a .curdt tool plugin archive");
        }
        install_verified_archive(&verified, &self.workspace_root, &self.config)
    }

    pub fn remove(&self, package_id: &str) -> Result<bool> {
        self.ensure_enabled()?;
        remove_installed_plugin(
            &self.workspace_root,
            &self.config,
            PluginKind::Tool,
            package_id,
        )
    }

    pub fn list(&self) -> Result<Vec<InstalledPluginRecord>> {
        self.ensure_enabled()?;
        load_installed_plugins(&self.workspace_root, &self.config, PluginKind::Tool)
    }

    pub fn get_doc(&self, tool_name: &str) -> Result<Option<Value>> {
        self.ensure_enabled()?;
        let installed = self.list()?;
        let record = installed.into_iter().find(|record| {
            record
                .tool
                .as_ref()
                .map(|tool| tool.tool_name == tool_name)
                .unwrap_or(false)
        });
        let Some(record) = record else {
            return Ok(None);
        };
        let spec = record
            .tool
            .context("installed tool plugin missing tool spec")?;
        Ok(Some(json!({
            "tool": spec.tool_name,
            "description": spec.description.unwrap_or_else(|| "Installed signed tool plugin".to_string()),
            "agent_usage": spec.agent_usage,
            "review_guidance": spec.review_guidance,
            "downstream_impact": spec.downstream_impact,
            "parameters": spec.parameters,
            "examples": spec.examples,
            "plugin": {
                "package_id": record.package_id,
                "version": record.version,
                "kind": "tool_plugin",
                "signer_pubkey_hex": record.signer_pubkey_hex,
            }
        })))
    }

    pub async fn invoke(&self, tool_name: &str, params: &Value) -> Result<Option<Value>> {
        self.ensure_enabled()?;
        let installed = self.list()?;
        let record = installed.into_iter().find(|record| {
            record
                .tool
                .as_ref()
                .map(|tool| tool.tool_name == tool_name)
                .unwrap_or(false)
        });
        let Some(record) = record else {
            return Ok(None);
        };
        let spec = record
            .tool
            .clone()
            .context("installed tool plugin missing tool spec")?;
        if spec.protocol != "json_stdio_v1" {
            anyhow::bail!(
                "tool plugin '{}' uses unsupported protocol '{}'",
                tool_name,
                spec.protocol
            );
        }
        if self.config.tool_runtime != "sidecar_stdio" {
            anyhow::bail!("tool plugin runtime policy forbids installed tool execution");
        }
        let install_dir = fs::canonicalize(&record.install_dir).with_context(|| {
            format!(
                "Failed to canonicalize plugin install dir {}",
                record.install_dir.display()
            )
        })?;
        let entry_path = install_dir.join(&spec.executable_path);
        let entry_path = fs::canonicalize(&entry_path).with_context(|| {
            format!(
                "Failed to canonicalize tool plugin entry {}",
                entry_path.display()
            )
        })?;
        if !entry_path.starts_with(&install_dir) {
            anyhow::bail!("tool plugin entry escaped install directory");
        }
        let sandbox = crate::Sandbox::new(&self.workspace_root);
        let mut cmd = sandbox.build_std_command(
            entry_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("tool plugin path is not valid UTF-8"))?,
            &spec.default_args,
        );
        cmd.current_dir(&self.workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .context("Failed to spawn sandboxed tool plugin process")?;
        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| anyhow::anyhow!("Failed to open tool plugin stdin"))?;
            stdin.write_all(serde_json::to_vec(params)?.as_slice())?;
        }
        let start = std::time::Instant::now();
        loop {
            if let Some(_status) = child.try_wait()? {
                break;
            }
            if start.elapsed().as_secs() >= self.config.external_mcp_timeout_secs {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!(
                    "tool plugin '{}' timed out after {}s",
                    tool_name,
                    self.config.external_mcp_timeout_secs
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            anyhow::bail!("tool plugin execution failed: {}", stderr);
        }
        if output.stdout.len() > self.config.external_mcp_max_output_bytes {
            anyhow::bail!(
                "tool plugin '{}' exceeded max output budget ({} bytes)",
                tool_name,
                self.config.external_mcp_max_output_bytes
            );
        }
        let stdout =
            String::from_utf8(output.stdout).context("tool plugin emitted non-UTF8 stdout")?;
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(Some(json!({"status": "ok"})));
        }
        let parsed: Value = serde_json::from_str(trimmed)
            .with_context(|| format!("tool plugin '{}' returned invalid JSON", tool_name))?;
        Ok(Some(parsed))
    }

    fn ensure_enabled(&self) -> Result<()> {
        if !self.config.enabled {
            anyhow::bail!("plugins are disabled by configuration");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    use tempfile::tempdir;

    #[test]
    fn installs_tool_plugin_archive() {
        let dir = tempdir().unwrap();
        let mut secret = [5u8; 32];
        secret[0] = 21;
        let signer = SigningKey::from_bytes(&secret);
        let pubkey_hex: String = signer
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let cfg = crate::config::PluginConfig::default();
        crate::plugin_packages::store_trusted_keys(
            dir.path(),
            &cfg,
            &crate::plugin_packages::TrustedPluginKeySet {
                keys: vec![
                    crate::plugin_packages::create_trusted_plugin_key(
                        "tool",
                        &pubkey_hex,
                        Some("tool".to_string()),
                        vec![PluginKind::Tool],
                        true,
                    )
                    .unwrap(),
                ],
            },
        )
        .unwrap();
        let script = b"#!/bin/sh\nprintf '{\"status\":\"ok\"}'\n";
        let manifest = crate::plugin_packages::PluginManifest {
            schema_version: 1,
            package_id: "demo-tool".to_string(),
            version: "0.1.0".to_string(),
            kind: PluginKind::Tool,
            signer_pubkey_hex: pubkey_hex,
            description: None,
            files: vec![crate::plugin_packages::PluginFileManifest {
                path: "bin/demo-tool".to_string(),
                sha256: crate::plugin_packages::sha256_hex(script),
                size: script.len(),
                executable: true,
            }],
            tool: Some(crate::plugin_packages::ToolPluginSpec {
                tool_name: "demo_tool".to_string(),
                executable_path: "bin/demo-tool".to_string(),
                default_args: Vec::new(),
                protocol: "json_stdio_v1".to_string(),
                agent_usage: Some("Send JSON input and expect JSON output.".to_string()),
                review_guidance: None,
                downstream_impact: None,
                description: Some("Demo proprietary tool.".to_string()),
                parameters: Vec::new(),
                examples: Vec::new(),
            }),
            language: None,
        };
        let signature: String = signer
            .sign(&serde_json::to_vec(&manifest).unwrap())
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let archive = crate::plugin_packages::PluginArchive {
            manifest,
            payload_files: vec![crate::plugin_packages::PluginPayloadFile {
                path: "bin/demo-tool".to_string(),
                content_b64: base64::engine::general_purpose::STANDARD.encode(script),
                executable: true,
            }],
            signature_hex: Some(signature),
        };
        let archive_path = dir.path().join("demo.curdt");
        fs::write(&archive_path, serde_json::to_vec(&archive).unwrap()).unwrap();
        let engine = ToolPluginEngine::new(dir.path(), cfg);
        let record = engine.add_archive(&archive_path).unwrap();
        assert_eq!(record.package_id, "demo-tool");
        assert!(record.install_dir.join("bin/demo-tool").exists());
    }

    #[test]
    fn disabled_plugin_config_blocks_tool_plugin_access() {
        let dir = tempdir().unwrap();
        let mut cfg = crate::config::PluginConfig::default();
        cfg.enabled = false;
        let engine = ToolPluginEngine::new(dir.path(), cfg);
        let err = engine
            .list()
            .expect_err("expected disabled plugins to fail");
        assert!(err.to_string().contains("plugins are disabled"));
    }

    #[tokio::test]
    async fn tool_plugin_invoke_respects_output_budget() {
        let dir = tempdir().unwrap();
        let mut secret = [6u8; 32];
        secret[0] = 22;
        let signer = SigningKey::from_bytes(&secret);
        let pubkey_hex: String = signer
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let mut cfg = crate::config::PluginConfig::default();
        cfg.external_mcp_max_output_bytes = 1;
        crate::plugin_packages::store_trusted_keys(
            dir.path(),
            &cfg,
            &crate::plugin_packages::TrustedPluginKeySet {
                keys: vec![
                    crate::plugin_packages::create_trusted_plugin_key(
                        "tool",
                        &pubkey_hex,
                        Some("tool".to_string()),
                        vec![PluginKind::Tool],
                        true,
                    )
                    .unwrap(),
                ],
            },
        )
        .unwrap();
        let script = b"#!/bin/sh\nprintf '{\"status\":\"ok\"}'\n";
        let manifest = crate::plugin_packages::PluginManifest {
            schema_version: 1,
            package_id: "demo-budget".to_string(),
            version: "0.1.0".to_string(),
            kind: PluginKind::Tool,
            signer_pubkey_hex: pubkey_hex,
            description: None,
            files: vec![crate::plugin_packages::PluginFileManifest {
                path: "bin/demo-budget".to_string(),
                sha256: crate::plugin_packages::sha256_hex(script),
                size: script.len(),
                executable: true,
            }],
            tool: Some(crate::plugin_packages::ToolPluginSpec {
                tool_name: "demo_budget".to_string(),
                executable_path: "bin/demo-budget".to_string(),
                default_args: Vec::new(),
                protocol: "json_stdio_v1".to_string(),
                agent_usage: None,
                review_guidance: None,
                downstream_impact: None,
                description: Some("Budget test tool.".to_string()),
                parameters: Vec::new(),
                examples: Vec::new(),
            }),
            language: None,
        };
        let signature: String = signer
            .sign(&serde_json::to_vec(&manifest).unwrap())
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let archive = crate::plugin_packages::PluginArchive {
            manifest,
            payload_files: vec![crate::plugin_packages::PluginPayloadFile {
                path: "bin/demo-budget".to_string(),
                content_b64: base64::engine::general_purpose::STANDARD.encode(script),
                executable: true,
            }],
            signature_hex: Some(signature),
        };
        let archive_path = dir.path().join("demo-budget.curdt");
        fs::write(&archive_path, serde_json::to_vec(&archive).unwrap()).unwrap();
        let engine = ToolPluginEngine::new(dir.path(), cfg);
        engine.add_archive(&archive_path).unwrap();
        let err = engine.invoke("demo_budget", &json!({})).await.unwrap_err();
        assert!(
            err.to_string().contains("exceeded max output budget"),
            "{err}"
        );
    }
}
