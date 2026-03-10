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
pub struct BudgetConfig {
    /// Maximum tokens the agent is allowed to consume in one session (0 = unlimited)
    pub max_tokens: Option<u64>,
    /// Maximum time in seconds a session is allowed to remain active
    pub max_session_secs: Option<u64>,
    /// Maximum number of hazardous tool calls (edit, shell) allowed per session
    pub max_hazardous_calls: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CurdConfig {
    #[serde(default)]
    pub workspace: WorkspacePolicyConfig,
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
    #[serde(default)]
    pub budget: BudgetConfig,
    #[serde(default)]
    pub collaboration: CollaborationConfig,
    #[serde(default)]
    pub variants: VariantsConfig,
    #[serde(default)]
    pub provenance: ProvenanceConfig,
    #[serde(default)]
    pub plugins: PluginConfig,
    #[serde(default)]
    pub policy: crate::policy::PolicyConfig,
    #[serde(skip)]
    pub source_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspacePolicyConfig {
    #[serde(default = "default_workspace_require_open_for_all_tools")]
    pub require_open_for_all_tools: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CollaborationConfig {
    #[serde(default = "default_collaboration_enabled")]
    pub enabled: bool,
    #[serde(default = "default_require_bound_participants")]
    pub require_bound_participants: bool,
    #[serde(default = "default_require_session_token_for_agents")]
    pub require_session_token_for_agents: bool,
    #[serde(default = "default_bootstrap_owner_human_only")]
    pub bootstrap_owner_human_only: bool,
    #[serde(default = "default_human_override_ttl_secs")]
    pub human_override_ttl_secs: u64,
    #[serde(default = "default_default_human_role")]
    pub default_human_role: String,
    #[serde(default = "default_default_agent_role")]
    pub default_agent_role: String,
    #[serde(default = "default_editor_can_promote")]
    pub editor_can_promote: bool,
    #[serde(default = "default_session_challenge_ttl_secs")]
    pub session_challenge_ttl_secs: u64,
    #[serde(default = "default_require_authorized_agents_file")]
    pub require_authorized_agents_file: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VariantsConfig {
    #[serde(default = "default_variants_enabled")]
    pub enabled: bool,
    #[serde(default = "default_variant_backend")]
    pub default_backend: String,
    #[serde(default = "default_allow_worktree_backend")]
    pub allow_worktree_backend: bool,
    #[serde(default = "default_variant_materialization")]
    pub materialization: String,
    #[serde(default = "default_keep_workspaces")]
    pub keep_workspaces: bool,
    #[serde(default = "default_max_compare_files")]
    pub max_compare_files: usize,
    #[serde(default = "default_max_plan_bytes")]
    pub max_plan_bytes: usize,
    #[serde(default = "default_max_materialized_bytes")]
    pub max_materialized_bytes: u64,
    #[serde(default = "default_max_variants_per_plan_set")]
    pub max_variants_per_plan_set: usize,
    #[serde(default = "default_max_plan_sets")]
    pub max_plan_sets: usize,
    #[serde(default = "default_retain_plan_sets")]
    pub retain_plan_sets: usize,
    #[serde(default = "default_retain_variant_workspaces")]
    pub retain_variant_workspaces: usize,
    #[serde(default = "default_require_review_for_promotion")]
    pub require_review_for_promotion: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProvenanceConfig {
    #[serde(default = "default_provenance_enabled")]
    pub enabled: bool,
    #[serde(default = "default_provenance_hash_chain")]
    pub hash_chain: bool,
    #[serde(default = "default_provenance_checkpoint_every")]
    pub checkpoint_every: usize,
    #[serde(default = "default_provenance_local_only")]
    pub local_only: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PluginConfig {
    #[serde(default = "default_plugins_enabled")]
    pub enabled: bool,
    #[serde(default = "default_plugin_install_root")]
    pub install_root: String,
    #[serde(default = "default_plugin_trusted_keys_file")]
    pub trusted_keys_file: String,
    #[serde(default = "default_plugin_require_signatures")]
    pub require_signatures: bool,
    #[serde(default = "default_plugin_allow_unsigned_dev_plugins")]
    pub allow_unsigned_dev_plugins: bool,
    #[serde(default = "default_plugin_allow_native_language_dylibs")]
    pub allow_native_language_dylibs: bool,
    #[serde(default = "default_plugin_tool_runtime")]
    pub tool_runtime: String,
    #[serde(default = "default_plugin_allow_wasm_language_fallback")]
    pub allow_wasm_language_fallback: bool,
    #[serde(default = "default_plugin_allow_external_mcp_tool_groups")]
    pub allow_external_mcp_tool_groups: bool,
    #[serde(default = "default_plugin_external_mcp_timeout_secs")]
    pub external_mcp_timeout_secs: u64,
    #[serde(default = "default_plugin_external_mcp_max_output_bytes")]
    pub external_mcp_max_output_bytes: usize,
    #[serde(default = "default_plugin_external_mcp_max_restarts")]
    pub external_mcp_max_restarts: u32,
    #[serde(default = "default_plugin_external_mcp_restart_backoff_ms")]
    pub external_mcp_restart_backoff_ms: u64,
}

fn default_collaboration_enabled() -> bool { true }
fn default_workspace_require_open_for_all_tools() -> bool { false }
fn default_require_bound_participants() -> bool { true }
fn default_require_session_token_for_agents() -> bool { true }
fn default_bootstrap_owner_human_only() -> bool { true }
fn default_human_override_ttl_secs() -> u64 { 300 }
fn default_default_human_role() -> String { "owner".to_string() }
fn default_default_agent_role() -> String { "planner".to_string() }
fn default_editor_can_promote() -> bool { false }
fn default_session_challenge_ttl_secs() -> u64 { 120 }
fn default_require_authorized_agents_file() -> bool { true }
fn default_variants_enabled() -> bool { true }
fn default_variant_backend() -> String { "shadow".to_string() }
fn default_allow_worktree_backend() -> bool { false }
fn default_variant_materialization() -> String { "workspace_copy".to_string() }
fn default_keep_workspaces() -> bool { true }
fn default_max_compare_files() -> usize { 200 }
fn default_max_plan_bytes() -> usize { 512 * 1024 }
fn default_max_materialized_bytes() -> u64 { 64 * 1024 * 1024 }
fn default_max_variants_per_plan_set() -> usize { 12 }
fn default_max_plan_sets() -> usize { 64 }
fn default_retain_plan_sets() -> usize { 32 }
fn default_retain_variant_workspaces() -> usize { 24 }
fn default_require_review_for_promotion() -> bool { false }
fn default_provenance_enabled() -> bool { true }
fn default_provenance_hash_chain() -> bool { true }
fn default_provenance_checkpoint_every() -> usize { 50 }
fn default_provenance_local_only() -> bool { true }
fn default_plugins_enabled() -> bool { true }
fn default_plugin_install_root() -> String { ".curd/plugins".to_string() }
fn default_plugin_trusted_keys_file() -> String { ".curd/plugins/trusted_keys.json".to_string() }
fn default_plugin_require_signatures() -> bool { true }
fn default_plugin_allow_unsigned_dev_plugins() -> bool { false }
fn default_plugin_allow_native_language_dylibs() -> bool { true }
fn default_plugin_tool_runtime() -> String { "sidecar_stdio".to_string() }
fn default_plugin_allow_wasm_language_fallback() -> bool { false }
fn default_plugin_allow_external_mcp_tool_groups() -> bool { true }
fn default_plugin_external_mcp_timeout_secs() -> u64 { 15 }
fn default_plugin_external_mcp_max_output_bytes() -> usize { 512 * 1024 }
fn default_plugin_external_mcp_max_restarts() -> u32 { 2 }
fn default_plugin_external_mcp_restart_backoff_ms() -> u64 { 250 }

impl Default for CollaborationConfig {
    fn default() -> Self {
        Self {
            enabled: default_collaboration_enabled(),
            require_bound_participants: default_require_bound_participants(),
            require_session_token_for_agents: default_require_session_token_for_agents(),
            bootstrap_owner_human_only: default_bootstrap_owner_human_only(),
            human_override_ttl_secs: default_human_override_ttl_secs(),
            default_human_role: default_default_human_role(),
            default_agent_role: default_default_agent_role(),
            editor_can_promote: default_editor_can_promote(),
            session_challenge_ttl_secs: default_session_challenge_ttl_secs(),
            require_authorized_agents_file: default_require_authorized_agents_file(),
        }
    }
}

impl Default for VariantsConfig {
    fn default() -> Self {
        Self {
            enabled: default_variants_enabled(),
            default_backend: default_variant_backend(),
            allow_worktree_backend: default_allow_worktree_backend(),
            materialization: default_variant_materialization(),
            keep_workspaces: default_keep_workspaces(),
            max_compare_files: default_max_compare_files(),
            max_plan_bytes: default_max_plan_bytes(),
            max_materialized_bytes: default_max_materialized_bytes(),
            max_variants_per_plan_set: default_max_variants_per_plan_set(),
            max_plan_sets: default_max_plan_sets(),
            retain_plan_sets: default_retain_plan_sets(),
            retain_variant_workspaces: default_retain_variant_workspaces(),
            require_review_for_promotion: default_require_review_for_promotion(),
        }
    }
}

impl Default for ProvenanceConfig {
    fn default() -> Self {
        Self {
            enabled: default_provenance_enabled(),
            hash_chain: default_provenance_hash_chain(),
            checkpoint_every: default_provenance_checkpoint_every(),
            local_only: default_provenance_local_only(),
        }
    }
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: default_plugins_enabled(),
            install_root: default_plugin_install_root(),
            trusted_keys_file: default_plugin_trusted_keys_file(),
            require_signatures: default_plugin_require_signatures(),
            allow_unsigned_dev_plugins: default_plugin_allow_unsigned_dev_plugins(),
            allow_native_language_dylibs: default_plugin_allow_native_language_dylibs(),
            tool_runtime: default_plugin_tool_runtime(),
            allow_wasm_language_fallback: default_plugin_allow_wasm_language_fallback(),
            allow_external_mcp_tool_groups: default_plugin_allow_external_mcp_tool_groups(),
            external_mcp_timeout_secs: default_plugin_external_mcp_timeout_secs(),
            external_mcp_max_output_bytes: default_plugin_external_mcp_max_output_bytes(),
            external_mcp_max_restarts: default_plugin_external_mcp_max_restarts(),
            external_mcp_restart_backoff_ms: default_plugin_external_mcp_restart_backoff_ms(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EditConfig {
    #[serde(default = "default_churn_limit")]
    pub churn_limit: f64,
    #[serde(default = "default_churn_small_limit")]
    pub churn_small_limit: f64,
    #[serde(default = "default_churn_large_limit")]
    pub churn_large_limit: f64,
    #[serde(default = "default_churn_massive_limit")]
    pub churn_massive_limit: f64,
    #[serde(default = "default_small_file_nodes")]
    pub small_file_nodes: usize,
    #[serde(default = "default_large_file_nodes")]
    pub large_file_nodes: usize,
    #[serde(default = "default_massive_file_nodes")]
    pub massive_file_nodes: usize,
    #[serde(default = "default_enforce_transactional")]
    pub enforce_transactional: bool,
}

fn default_churn_limit() -> f64 { 0.3 }
fn default_churn_small_limit() -> f64 { 1.0 }
fn default_churn_large_limit() -> f64 { 0.15 }
fn default_churn_massive_limit() -> f64 { 0.05 }
fn default_small_file_nodes() -> usize { 100 }
fn default_large_file_nodes() -> usize { 500 }
fn default_massive_file_nodes() -> usize { 2000 }

fn default_enforce_transactional() -> bool {
    true
}

impl Default for EditConfig {
    fn default() -> Self {
        Self {
            churn_limit: default_churn_limit(),
            churn_small_limit: default_churn_small_limit(),
            churn_large_limit: default_churn_large_limit(),
            churn_massive_limit: default_churn_massive_limit(),
            small_file_nodes: default_small_file_nodes(),
            large_file_nodes: default_large_file_nodes(),
            massive_file_nodes: default_massive_file_nodes(),
            enforce_transactional: default_enforce_transactional(),
        }
    }
}

impl CurdConfig {
    pub fn load_from_workspace(root: &Path) -> Self {
        let curd_dir = crate::workspace::get_curd_dir(root);
        for file in ["curd.toml", "settings.toml"] {
            let primary = curd_dir.join(file);
            if primary.exists()
                && let Ok(content) = fs::read_to_string(&primary)
                && let Ok(mut config) = toml::from_str::<Self>(&content)
            {
                config.source_path = Some(primary);
                return config;
            }
        }

        for file in ["settings.toml", "curd.toml", "CURD.toml"] {
            let config_path = root.join(file);
            if config_path.exists()
                && let Ok(content) = fs::read_to_string(&config_path)
                && let Ok(mut config) = toml::from_str::<Self>(&content)
            {
                config.source_path = Some(config_path);
                return config;
            }
        }
        Self::default()
    }

    pub fn save_to_workspace(&self) -> anyhow::Result<()> {
        let path = self.source_path.as_ref().cloned().unwrap_or_else(|| {
            let root = std::path::PathBuf::from(".");
            crate::workspace::get_curd_dir(&root).join("curd.toml")
        });

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn compute_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        if let Ok(content) = toml::to_string(self) {
            hasher.update(content.as_bytes());
        }
        format!("{:x}", hasher.finalize())
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
        if !matches!(
            self.collaboration.default_human_role.as_str(),
            "owner" | "editor" | "planner" | "reviewer" | "observer"
        ) {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_collaboration_default_human_role_invalid".to_string(),
                message: format!(
                    "Unsupported [collaboration].default_human_role='{}'",
                    self.collaboration.default_human_role
                ),
            });
        }
        if !matches!(
            self.collaboration.default_agent_role.as_str(),
            "owner" | "editor" | "planner" | "reviewer" | "observer"
        ) {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_collaboration_default_agent_role_invalid".to_string(),
                message: format!(
                    "Unsupported [collaboration].default_agent_role='{}'",
                    self.collaboration.default_agent_role
                ),
            });
        }
        if !matches!(
            self.variants.default_backend.as_str(),
            "shadow" | "worktree"
        ) {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_variants_default_backend_invalid".to_string(),
                message: format!(
                    "Unsupported [variants].default_backend='{}'",
                    self.variants.default_backend
                ),
            });
        }
        if !matches!(self.variants.materialization.as_str(), "workspace_copy" | "shadow") {
            out.push(ConfigFinding {
                severity: "medium".to_string(),
                code: "config_variants_materialization_invalid".to_string(),
                message: format!(
                    "Unsupported [variants].materialization='{}'",
                    self.variants.materialization
                ),
            });
        }
        let install_root = self.plugins.install_root.trim();
        if install_root.is_empty()
            || install_root.starts_with('/')
            || install_root.starts_with('~')
            || install_root.contains("..")
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_plugins_install_root_invalid".to_string(),
                message: format!("Unsafe [plugins].install_root='{}'", self.plugins.install_root),
            });
        }
        let trust_file = self.plugins.trusted_keys_file.trim();
        if trust_file.is_empty()
            || trust_file.starts_with('/')
            || trust_file.starts_with('~')
            || trust_file.contains("..")
        {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_plugins_trusted_keys_file_invalid".to_string(),
                message: format!(
                    "Unsafe [plugins].trusted_keys_file='{}'",
                    self.plugins.trusted_keys_file
                ),
            });
        }
        if !matches!(self.plugins.tool_runtime.as_str(), "sidecar_stdio") {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_plugins_tool_runtime_invalid".to_string(),
                message: format!(
                    "Unsupported [plugins].tool_runtime='{}'",
                    self.plugins.tool_runtime
                ),
            });
        }
        if self.plugins.external_mcp_timeout_secs == 0 {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_plugins_external_mcp_timeout_invalid".to_string(),
                message: "[plugins].external_mcp_timeout_secs must be > 0".to_string(),
            });
        }
        if self.plugins.external_mcp_max_output_bytes == 0 {
            out.push(ConfigFinding {
                severity: "high".to_string(),
                code: "config_plugins_external_mcp_output_budget_invalid".to_string(),
                message: "[plugins].external_mcp_max_output_bytes must be > 0".to_string(),
            });
        }
        if self.plugins.external_mcp_restart_backoff_ms > 60_000 {
            out.push(ConfigFinding {
                severity: "medium".to_string(),
                code: "config_plugins_external_mcp_restart_backoff_large".to_string(),
                message: format!(
                    "[plugins].external_mcp_restart_backoff_ms={} is unusually large",
                    self.plugins.external_mcp_restart_backoff_ms
                ),
            });
        }
        if self.plugins.external_mcp_max_restarts > 32 {
            out.push(ConfigFinding {
                severity: "medium".to_string(),
                code: "config_plugins_external_mcp_max_restarts_large".to_string(),
                message: format!(
                    "[plugins].external_mcp_max_restarts={} is unusually large",
                    self.plugins.external_mcp_max_restarts
                ),
            });
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
    #[serde(default)]
    pub tasks: HashMap<String, String>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShellConfig {
    #[serde(default)]
    pub docker_enabled: bool,
    #[serde(default = "default_container_engine")]
    pub container_engine: String,
    #[serde(default = "default_docker_image")]
    pub docker_image: String,
}

fn default_container_engine() -> String {
    "docker".to_string()
}

fn default_docker_image() -> String {
    "ubuntu:latest".to_string()
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            docker_enabled: false,
            container_engine: default_container_engine(),
            docker_image: default_docker_image(),
        }
    }
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
impl Default for WorkspacePolicyConfig {
    fn default() -> Self {
        Self {
            require_open_for_all_tools: default_workspace_require_open_for_all_tools(),
        }
    }
}
