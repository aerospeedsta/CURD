use crate::SearchEngine;
use anyhow::Result;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

/// Safely performs discrete file operations restricted strictly to the workspace boundary
pub struct FileEngine {
    pub workspace_root: PathBuf,
}

impl FileEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
        }
    }

    /// Primary execution router for safe path management
    pub fn manage(
        &self,
        uri: &str,
        action: &str,
        destination: Option<&str>,
        shadow_root: Option<&Path>,
    ) -> Result<Value> {
        let target_root = shadow_root.unwrap_or(&self.workspace_root);
        let clean_uri = crate::workspace::validate_sandboxed_path(target_root, uri)?;

        match action {
            "create" => {
                if clean_uri.exists() {
                    return Err(anyhow::anyhow!(
                        "Cannot create file '{}', it already exists.",
                        uri
                    ));
                }
                if let Some(parent) = clean_uri.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&clean_uri, "")?;
                SearchEngine::new(&self.workspace_root).invalidate_index();
                Ok(json!({"status": "ok", "action": "create", "path": uri}))
            }
            "write" => {
                // Write content to a file (create with content, or overwrite existing)
                let content = destination.unwrap_or("");
                if let Some(parent) = clean_uri.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&clean_uri, content)?;
                SearchEngine::new(&self.workspace_root).invalidate_index();
                Ok(json!({"status": "ok", "action": "write", "path": uri, "bytes": content.len()}))
            }
            "delete" => {
                if !clean_uri.exists() {
                    return Err(anyhow::anyhow!(
                        "Cannot delete file '{}', it does not exist.",
                        uri
                    ));
                }
                if clean_uri.is_dir() {
                    fs::remove_dir_all(&clean_uri)?;
                } else {
                    fs::remove_file(&clean_uri)?;
                }
                SearchEngine::new(&self.workspace_root).invalidate_index();
                Ok(json!({"status": "ok", "action": "delete", "path": uri}))
            }
            "rename" => {
                let dest = destination.ok_or_else(|| {
                    anyhow::anyhow!("'rename' action requires a 'destination' parameter.")
                })?;
                let clean_dest = crate::workspace::validate_sandboxed_path(target_root, dest)?;

                if !clean_uri.exists() {
                    return Err(anyhow::anyhow!(
                        "Cannot rename '{}', it does not exist.",
                        uri
                    ));
                }
                if clean_dest.exists() {
                    return Err(anyhow::anyhow!(
                        "Cannot rename to '{}', destination already exists.",
                        dest
                    ));
                }

                if let Some(parent) = clean_dest.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::rename(&clean_uri, &clean_dest)?;
                SearchEngine::new(&self.workspace_root).invalidate_index();
                Ok(json!({"status": "ok", "action": "rename", "source": uri, "destination": dest}))
            }
            _ => Err(anyhow::anyhow!(
                "Unknown file action '{}'. Valid actions are: create, write, delete, rename.",
                action
            )),
        }
    }
}
