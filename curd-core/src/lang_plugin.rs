use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::plugin_packages::{
    InstalledPluginRecord, PluginKind, install_verified_archive, load_installed_plugins,
    remove_installed_plugin, verify_archive_file,
};

pub struct LangPluginEngine {
    workspace_root: PathBuf,
    config: crate::config::PluginConfig,
}

impl LangPluginEngine {
    pub fn new(workspace_root: impl AsRef<Path>, config: crate::config::PluginConfig) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
            config,
        }
    }

    pub fn add_archive(&self, archive_path: &Path) -> Result<InstalledPluginRecord> {
        self.ensure_enabled()?;
        let verified = verify_archive_file(archive_path, &self.workspace_root, &self.config)?;
        if verified.archive.manifest.kind != PluginKind::Language {
            anyhow::bail!("expected a .curdl language plugin archive");
        }
        let record = install_verified_archive(&verified, &self.workspace_root, &self.config)?;
        self.rebuild_registry()?;
        Ok(record)
    }

    pub fn remove(&self, package_id: &str) -> Result<bool> {
        self.ensure_enabled()?;
        let removed = remove_installed_plugin(
            &self.workspace_root,
            &self.config,
            PluginKind::Language,
            package_id,
        )?;
        self.rebuild_registry()?;
        Ok(removed)
    }

    pub fn list(&self) -> Result<Vec<InstalledPluginRecord>> {
        self.ensure_enabled()?;
        load_installed_plugins(&self.workspace_root, &self.config, PluginKind::Language)
    }

    pub fn rebuild_registry(&self) -> Result<()> {
        self.ensure_enabled()?;
        let installed = self.list()?;
        let mut languages = HashMap::new();
        for record in installed {
            let Some(spec) = record.language else {
                continue;
            };
            let plugin_path = record.install_dir.join(&spec.grammar_library_path);
            let query_path = spec
                .query_path
                .as_ref()
                .map(|q| record.install_dir.join(q).to_string_lossy().to_string());
            languages.insert(
                spec.language_id.clone(),
                crate::registry::LanguageDef {
                    extensions: spec.extensions,
                    backend: "plugin".to_string(),
                    query_file: query_path,
                    wasm_file: None,
                    plugin_path: Some(plugin_path.to_string_lossy().to_string()),
                    embedded_query: None,
                },
            );
        }
        let root = crate::plugin_packages::plugin_install_root(&self.workspace_root, &self.config)
            .join("lang");
        fs::create_dir_all(&root)?;
        fs::write(
            root.join("languages.toml"),
            toml::to_string_pretty(&crate::registry::GrammarRegistry { languages })
                .context("Failed to serialize language plugin registry")?,
        )?;
        Ok(())
    }

    pub fn validate_plugin_library(
        &self,
        plugin_path: &Path,
        language_id: &str,
        function_name: &str,
    ) -> Result<()> {
        self.ensure_enabled()?;
        let installed = self.list()?;
        let canon = fs::canonicalize(plugin_path)
            .with_context(|| format!("Failed to canonicalize {}", plugin_path.display()))?;
        for record in installed {
            let Some(spec) = record.language else {
                continue;
            };
            if spec.language_id != language_id {
                continue;
            }
            if spec.grammar_symbol != function_name {
                continue;
            }
            let expected = fs::canonicalize(record.install_dir.join(spec.grammar_library_path))
                .context("Failed to canonicalize installed plugin library")?;
            if expected == canon {
                return Ok(());
            }
        }
        anyhow::bail!(
            "language plugin library is not an installed verified package for language '{}'",
            language_id
        )
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
    fn rebuilds_installed_language_registry() {
        let dir = tempdir().unwrap();
        let mut secret = [9u8; 32];
        secret[0] = 13;
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
                        "lang",
                        &pubkey_hex,
                        Some("lang".to_string()),
                        vec![PluginKind::Language],
                        true,
                    )
                    .unwrap(),
                ],
            },
        )
        .unwrap();
        let payload = b"fake";
        let manifest = crate::plugin_packages::PluginManifest {
            schema_version: 1,
            package_id: "demo-lang".to_string(),
            version: "0.1.0".to_string(),
            kind: PluginKind::Language,
            signer_pubkey_hex: pubkey_hex,
            description: None,
            files: vec![crate::plugin_packages::PluginFileManifest {
                path: "lib/demo.dylib".to_string(),
                sha256: crate::plugin_packages::sha256_hex(payload),
                size: payload.len(),
                executable: false,
            }],
            tool: None,
            language: Some(crate::plugin_packages::LanguagePluginSpec {
                language_id: "demo".to_string(),
                extensions: vec!["demo".to_string()],
                grammar_library_path: "lib/demo.dylib".to_string(),
                grammar_symbol: "tree_sitter_demo".to_string(),
                query_path: None,
                build_system: None,
                lsp_adapter: None,
                debug_adapter: None,
            }),
        };
        let payload_files = vec![crate::plugin_packages::PluginPayloadFile {
            path: "lib/demo.dylib".to_string(),
            content_b64: base64::engine::general_purpose::STANDARD.encode(payload),
            executable: false,
        }];
        let signature: String = signer
            .sign(&serde_json::to_vec(&manifest).unwrap())
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let archive = crate::plugin_packages::PluginArchive {
            manifest,
            payload_files,
            signature_hex: Some(signature),
        };
        let archive_path = dir.path().join("demo.curdl");
        fs::write(&archive_path, serde_json::to_vec(&archive).unwrap()).unwrap();

        let engine = LangPluginEngine::new(dir.path(), cfg.clone());
        engine.add_archive(&archive_path).unwrap();
        let registry =
            fs::read_to_string(dir.path().join(".curd/plugins/lang/languages.toml")).unwrap();
        assert!(registry.contains("demo"));
        assert!(registry.contains("plugin"));
    }

    #[test]
    fn disabled_plugin_config_blocks_language_plugin_access() {
        let dir = tempdir().unwrap();
        let mut cfg = crate::config::PluginConfig::default();
        cfg.enabled = false;
        let engine = LangPluginEngine::new(dir.path(), cfg);
        let err = engine
            .list()
            .expect_err("expected disabled plugins to fail");
        assert!(err.to_string().contains("plugins are disabled"));
    }
}
