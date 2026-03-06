use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    Index, // Search and graph only
    Read,  // Search, graph, and full file read
    Write, // Full access (except shell)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextLink {
    pub path: PathBuf,
    pub mode: ContextMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextRegistry {
    pub contexts: HashMap<String, ContextLink>,
}

impl ContextRegistry {
    pub fn load(workspace_root: &Path) -> Self {
        let path = workspace_root.join(".curd/contexts.json");
        if path.exists()
            && let Ok(content) = fs::read_to_string(&path)
                && let Ok(registry) = serde_json::from_str::<ContextRegistry>(&content) {
                    return registry;
                }
        Self::default()
    }

    pub fn save(&self, workspace_root: &Path) -> Result<()> {
        let curd_dir = workspace_root.join(".curd");
        fs::create_dir_all(&curd_dir)?;
        let path = curd_dir.join("contexts.json");
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn add(&mut self, alias: String, path: PathBuf, mode: ContextMode) {
        self.contexts.insert(alias, ContextLink { path, mode });
    }

    pub fn remove(&mut self, alias_or_path: &str) -> bool {
        if self.contexts.remove(alias_or_path).is_some() {
            return true;
        }
        // Try removing by path
        let mut to_remove = None;
        for (alias, link) in &self.contexts {
            if link.path.to_string_lossy() == alias_or_path {
                to_remove = Some(alias.clone());
                break;
            }
        }
        if let Some(alias) = to_remove {
            self.contexts.remove(&alias);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_context_registry_lifecycle() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let mut registry = ContextRegistry::default();
        registry.add("@test_api".to_string(), PathBuf::from("/tmp/test_api"), ContextMode::Read);
        registry.add("@test_lib".to_string(), PathBuf::from("/tmp/test_lib"), ContextMode::Index);

        assert_eq!(registry.contexts.len(), 2);
        
        // Save to disk
        registry.save(root).unwrap();

        // Load from disk
        let loaded = ContextRegistry::load(root);
        assert_eq!(loaded.contexts.len(), 2);
        assert_eq!(loaded.contexts.get("@test_api").unwrap().mode, ContextMode::Read);

        // Remove by alias
        assert!(registry.remove("@test_api"));
        assert_eq!(registry.contexts.len(), 1);

        // Remove by path
        assert!(registry.remove("/tmp/test_lib"));
        assert!(registry.contexts.is_empty());
    }
}
