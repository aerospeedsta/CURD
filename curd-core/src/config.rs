use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigFinding {
    pub severity: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CurdConfig {
    #[serde(default)]
    pub edit: EditConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub doctor: DoctorConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub reference: ReferenceConfig,
    #[serde(default)]
    pub shell: ShellConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EditConfig {
    #[serde(default = "default_churn_limit")]
    pub churn_limit: f64,
}

fn default_churn_limit() -> f64 {
    0.3
}

impl Default for EditConfig {
    fn default() -> Self {
        Self {
            churn_limit: default_churn_limit(),
        }
    }
}

impl CurdConfig {
    pub fn load_from_workspace(root: &Path) -> Self {
        // Prioritize hidden .curd/settings.toml
        let primary = root.join(".curd").join("settings.toml");
        if primary.exists()
            && let Ok(content) = fs::read_to_string(&primary)
            && let Ok(config) = toml::from_str(&content)
        {
            return config;
        }

        // Fallback to legacy root files
        for file in ["settings.toml", "curd.toml", "CURD.toml"] {
            let config_path = root.join(file);
            if config_path.exists()
                && let Ok(content) = fs::read_to_string(&config_path)
                && let Ok(config) = toml::from_str(&content)
            {
                return config;
            }
        }
        Self {
            edit: EditConfig::default(),
            index: IndexConfig::default(),
            doctor: DoctorConfig::default(),
            build: BuildConfig::default(),
            storage: StorageConfig::default(),
            reference: ReferenceConfig::default(),
            shell: ShellConfig::default(),
        }
    }

    pub fn validate(&self) -> Vec<ConfigFinding> {
        let mut out = Vec::new();
        if let Some(mode) = self.index.mode.as_deref()
            && !matches!(mode, "full" | "fast" | "lazy" | "scoped")
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_index_mode_invalid".to_string(),
                message: format!("Unsupported [index].mode='{}'", mode),
            });
        }
        if let Some(policy) = self.index.large_file_policy.as_deref()
            && !matches!(policy, "skip" | "skeleton" | "full")
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_large_file_policy_invalid".to_string(),
                message: format!("Unsupported [index].large_file_policy='{}'", policy),
            });
        }
        if let Some(exec) = self.index.execution.as_deref()
            && !matches!(exec, "multithreaded" | "multiprocess" | "singlethreaded")
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_index_execution_invalid".to_string(),
                message: format!("Unsupported [index].execution='{}'", exec),
            });
        }
        if let Some(chunk) = self.index.chunk_size
            && chunk == 0
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_index_chunk_size_invalid".to_string(),
                message: "[index].chunk_size must be > 0".to_string(),
            });
        }
        if let Some(max) = self.index.max_file_size
            && max == 0
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_index_max_file_size_invalid".to_string(),
                message: "[index].max_file_size must be > 0".to_string(),
            });
        }
        if let Some(parser_backend) = self.index.parser_backend.as_deref()
            && !matches!(parser_backend, "native" | "wasm")
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_parser_backend_invalid".to_string(),
                message: format!("Unsupported [index].parser_backend='{}'", parser_backend),
            });
        }

        const LANGS: &[&str] = &[
            "python",
            "rust",
            "javascript",
            "typescript",
            "go",
            "c",
            "cpp",
            "java",
        ];
        for (ext, lang) in &self.index.extension_language_map {
            let ext_norm = ext.trim().trim_start_matches('.');
            if ext_norm.is_empty()
                || ext_norm.contains('/')
                || ext_norm.contains('\\')
                || ext_norm.contains("..")
            {
                out.push(ConfigFinding {
                    severity: "high".to_string(),
                    code: "config_extension_key_invalid".to_string(),
                    message: format!(
                        "Invalid extension key '{}' in [index].extension_language_map",
                        ext
                    ),
                });
            }
            let lang_norm = lang.trim().to_lowercase();
            if !LANGS.contains(&lang_norm.as_str()) {
                out.push(ConfigFinding {
                    severity: "high".to_string(),
                    code: "config_extension_lang_invalid".to_string(),
                    message: format!(
                        "Unsupported language '{}' for extension '{}' in [index].extension_language_map",
                        lang, ext
                    ),
                });
            }
        }

        if let Some(profile) = self.build.default_profile.as_deref()
            && !matches!(profile, "debug" | "release")
        {
            out.push(ConfigFinding {
                severity: "medium".to_string(),
                code: "config_build_profile_invalid".to_string(),
                message: format!("Unsupported [build].default_profile='{}'", profile),
            });
        }
        for (name, adapter) in &self.build.adapters {
            if adapter.steps.is_empty() {
                out.push(ConfigFinding {
                    severity: "medium".to_string(),
                    code: "config_build_adapter_empty_steps".to_string(),
                    message: format!("Adapter '{}' has no steps configured", name),
                });
            }
            if let Some(cwd) = adapter.cwd.as_deref()
                && (cwd.starts_with('/') || cwd.starts_with('~') || cwd.contains(".."))
            {
                out.push(ConfigFinding {
                    severity: "high".to_string(),
                    code: "config_build_adapter_cwd_invalid".to_string(),
                    message: format!("Adapter '{}' has unsafe cwd '{}'", name, cwd),
                });
            }
            for f in &adapter.detect_files {
                if f.starts_with('/') || f.starts_with('~') || f.contains("..") {
                    out.push(ConfigFinding {
                        severity: "high".to_string(),
                        code: "config_build_adapter_detect_file_invalid".to_string(),
                        message: format!(
                            "Adapter '{}' has unsafe detect_files entry '{}'",
                            name, f
                        ),
                    });
                }
            }
        }

        if let Some(path) = self.storage.sqlite_path.as_deref()
            && (path.starts_with('/') || path.starts_with('~') || path.contains(".."))
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_storage_sqlite_path_invalid".to_string(),
                message: format!("Unsafe [storage].sqlite_path='{}'", path),
            });
        }
        if let Some(mode) = self.storage.encryption_mode.as_deref()
            && !matches!(mode, "sqlcipher")
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_storage_encryption_mode_invalid".to_string(),
                message: format!("Unsupported [storage].encryption_mode='{}'", mode),
            });
        }
        if let Some(key_env) = self.storage.key_env.as_deref() {
            let valid = !key_env.is_empty()
                && key_env
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !valid {
                out.push(ConfigFinding {
                    severity: "medium".to_string(),
                    code: "config_storage_key_env_invalid".to_string(),
                    message: format!("Invalid [storage].key_env='{}'", key_env),
                });
            }
        }
        out
    }
}

pub fn check_workspace_config(root: &Path) -> Result<(), Vec<ConfigFinding>> {
    let cfg = CurdConfig::load_from_workspace(root);
    let findings = cfg.validate();
    let highs: Vec<_> = findings
        .iter()
        .filter(|f| f.severity.eq_ignore_ascii_case("high"))
        .cloned()
        .collect();
    if highs.is_empty() {
        Ok(())
    } else {
        Err(highs)
    }
}

pub fn validate_workspace_config(root: &Path) -> anyhow::Result<()> {
    match check_workspace_config(root) {
        Ok(_) => Ok(()),
        Err(highs) => {
            let mut lines = Vec::new();
            lines.push("Invalid CURD workspace configuration (high severity):".to_string());
            for f in highs {
                lines.push(format!("- [{}] {}", f.code, f.message));
            }
            lines.push("Run `curd doctor . --strict` for full validation details.".to_string());
            anyhow::bail!(lines.join("\n"));
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct IndexConfig {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub max_file_size: Option<u64>,
    #[serde(default)]
    pub chunk_size: Option<usize>,
    #[serde(default)]
    pub large_file_policy: Option<String>,
    #[serde(default)]
    pub execution: Option<String>,
    #[serde(default)]
    pub stall_threshold_ms: Option<u64>,
    #[serde(default)]
    pub parser_backend: Option<String>,
    #[serde(default)]
    pub extension_language_map: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DoctorConfig {
    #[serde(default)]
    pub strict: Option<bool>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub max_total_ms: Option<u64>,
    #[serde(default)]
    pub max_parse_fail: Option<usize>,
    #[serde(default)]
    pub max_no_symbols_ratio: Option<f64>,
    #[serde(default)]
    pub max_skipped_large_ratio: Option<f64>,
    #[serde(default)]
    pub min_coverage_ratio: Option<f64>,
    #[serde(default)]
    pub require_coverage_state: Option<String>,
    #[serde(default)]
    pub min_symbol_count: Option<usize>,
    #[serde(default)]
    pub min_symbols_per_k_files: Option<f64>,
    #[serde(default)]
    pub min_overlap_with_full: Option<f64>,
    #[serde(default)]
    pub parity_rerun: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BuildConfig {
    #[serde(default)]
    pub preferred_adapter: Option<String>,
    #[serde(default)]
    pub build_dir: Option<String>,
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub default_target: Option<String>,
    #[serde(default)]
    pub adapters: HashMap<String, BuildAdapterConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BuildAdapterConfig {
    #[serde(default)]
    pub detect_files: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub steps: Vec<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageConfig {
    #[serde(default = "default_storage_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub sqlite_path: Option<String>,
    #[serde(default)]
    pub encryption_mode: Option<String>,
    #[serde(default)]
    pub key_env: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ReferenceConfig {
    #[serde(default)]
    pub instances: HashMap<String, String>,
    #[serde(default)]
    pub enable_delegation: bool,
}

fn default_storage_enabled() -> bool {
    true
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            enabled: default_storage_enabled(),
            sqlite_path: None,
            encryption_mode: None,
            key_env: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ShellConfig {
    #[serde(default)]
    pub allowlist: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{CurdConfig, validate_workspace_config};
    use tempfile::tempdir;

    #[test]
    fn loads_settings_toml_first() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(
            root.join("settings.toml"),
            r#"
[edit]
churn_limit = 0.42
[index]
mode = "fast"
chunk_size = 128
[doctor]
strict = true
"#,
        )
        .expect("write settings");

        std::fs::write(
            root.join("CURD.toml"),
            r#"
[edit]
churn_limit = 0.10
"#,
        )
        .expect("write CURD");

        let cfg = CurdConfig::load_from_workspace(root);
        assert!((cfg.edit.churn_limit - 0.42).abs() < 1e-9);
        assert_eq!(cfg.index.mode.as_deref(), Some("fast"));
        assert_eq!(cfg.index.chunk_size, Some(128));
        assert_eq!(cfg.doctor.strict, Some(true));
    }

    #[test]
    fn falls_back_to_curd_toml_then_default() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(
            root.join("curd.toml"),
            r#"
[edit]
churn_limit = 0.25
[index]
mode = "lazy"
"#,
        )
        .expect("write curd.toml");
        let cfg = CurdConfig::load_from_workspace(root);
        assert!((cfg.edit.churn_limit - 0.25).abs() < 1e-9);
        assert_eq!(cfg.index.mode.as_deref(), Some("lazy"));

        std::fs::remove_file(root.join("curd.toml")).expect("remove");
        let cfg_default = CurdConfig::load_from_workspace(root);
        assert!((cfg_default.edit.churn_limit - 0.3).abs() < 1e-9);
        assert!(cfg_default.index.mode.is_none());
    }

    #[test]
    fn validate_flags_invalid_4d_fields() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(
            root.join("settings.toml"),
            r#"
[index]
mode = "invalid"
execution = "forkbomb"
chunk_size = 0
parser_backend = "gpu"
extension_language_map = { "..bad" = "madeuplang" }

[build]
default_profile = "optimized"

[build.adapters.bad]
detect_files = ["../oops"]
cwd = "../escape"
steps = []

[storage]
sqlite_path = "../outside.sqlite3"
encryption_mode = "aes256"
key_env = "BAD-KEY"
"#,
        )
        .expect("write settings");
        let cfg = CurdConfig::load_from_workspace(root);
        let findings = cfg.validate();
        assert!(findings.len() >= 8);
        assert!(
            findings
                .iter()
                .any(|f| f.code == "config_storage_sqlite_path_invalid")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.code == "config_build_adapter_detect_file_invalid")
        );
    }

    #[test]
    fn validate_workspace_config_fails_on_high_findings() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(
            root.join("settings.toml"),
            r#"
[storage]
sqlite_path = "../escape.sqlite3"
"#,
        )
        .expect("write settings");
        let err = validate_workspace_config(root).expect_err("expected validation failure");
        assert!(
            err.to_string()
                .contains("config_storage_sqlite_path_invalid")
        );
    }
}
