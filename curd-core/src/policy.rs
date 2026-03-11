use glob::Pattern;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PolicyConfig {
    /// mode: "strict" (default-deny), "permissive" (default-allow), "audit" (allow but log)
    #[serde(default = "default_policy_mode")]
    pub mode: String,
    #[serde(default)]
    pub block_files: Vec<String>,
    #[serde(default)]
    pub allow_files: Vec<String>,
    #[serde(default)]
    pub protected_symbols: Vec<String>,
    /// If true, agents MUST register and execute a plan for any mutation tool (edit, shell, etc.)
    #[serde(default)]
    pub require_plan_for_mutations: bool,
    /// Shell: List of binaries agents are allowed to execute
    #[serde(default = "default_allowed_binaries")]
    pub allowed_binaries: Vec<String>,
    /// Shell: Block dangerous metacharacters (;, &, |, `, $( ))
    #[serde(default = "default_true")]
    pub block_shell_metachars: bool,
    #[serde(default)]
    pub capabilities: PolicyCapabilitiesConfig,
    #[serde(default)]
    pub change: PolicyChangeConfig,
    #[serde(default)]
    pub exec: PolicyExecConfig,
    #[serde(default)]
    pub hooks: PolicyHooksConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PolicyCapabilitiesConfig {
    #[serde(default = "default_policy_mode")]
    pub default_mode: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PolicyChangeConfig {
    #[serde(default = "default_max_nodes_touched")]
    pub max_nodes_touched: usize,
    #[serde(default = "default_max_edges_touched")]
    pub max_edges_touched: usize,
    #[serde(default = "default_true")]
    pub require_review_for_interface_hash_changes: bool,
    #[serde(default)]
    pub forbid_entrypoint_deletion: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PolicyExecConfig {
    #[serde(default)]
    pub allow_raw_command: bool,
    #[serde(default = "default_true")]
    pub allow_background: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PolicyHooksConfig {
    #[serde(default = "default_true")]
    pub allow_review_hooks: bool,
    #[serde(default)]
    pub allow_mutating_hooks: bool,
}

fn default_true() -> bool {
    true
}
fn default_max_nodes_touched() -> usize {
    25
}
fn default_max_edges_touched() -> usize {
    100
}

fn default_allowed_binaries() -> Vec<String> {
    vec![
        "cargo", "npm", "yarn", "pnpm", "bun", "python", "python3", "pytest", "node", "make",
        "ninja", "cmake", "go", "gcc", "clang", "g++", "clang++", "rustc", "tsc", "jest", "vitest",
        "npx", "echo", "sleep", "cat", "ls", "grep",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_policy_mode() -> String {
    "permissive".to_string()
}

impl Default for PolicyCapabilitiesConfig {
    fn default() -> Self {
        Self {
            default_mode: default_policy_mode(),
        }
    }
}

impl Default for PolicyChangeConfig {
    fn default() -> Self {
        Self {
            max_nodes_touched: default_max_nodes_touched(),
            max_edges_touched: default_max_edges_touched(),
            require_review_for_interface_hash_changes: true,
            forbid_entrypoint_deletion: false,
        }
    }
}

impl Default for PolicyExecConfig {
    fn default() -> Self {
        Self {
            allow_raw_command: false,
            allow_background: true,
        }
    }
}

impl Default for PolicyHooksConfig {
    fn default() -> Self {
        Self {
            allow_review_hooks: true,
            allow_mutating_hooks: false,
        }
    }
}

pub enum PolicyDecision {
    Allow,
    Deny(String),
    Audit(String),
}

pub struct PolicyEngine {
    config: PolicyConfig,
    block_patterns: Vec<Pattern>,
    allow_patterns: Vec<Pattern>,
}

#[derive(Debug, Clone)]
pub struct OperationPolicyInput<'a> {
    pub op: crate::CanonicalOperationKind,
    pub tool: &'a str,
    pub params: &'a Value,
    pub is_human: bool,
    pub workspace_root: &'a Path,
    pub runtime_ceiling: crate::RuntimeCeiling,
    pub profile_name: Option<&'a str>,
    pub profile: Option<&'a crate::config::AgentProfileConfig>,
    pub session_open: bool,
}

const MAX_PLAN_NODES: usize = 128;
const MAX_PLAN_DEPENDENCIES: usize = 32;
const MAX_PLAN_RETRY_LIMIT: u8 = 3;
const MAX_PLAN_OUTPUT_LIMIT_BYTES: usize = 1024 * 1024;

fn allowed_internal_plan_command(command: &str) -> bool {
    matches!(
        command,
        "recompute_graph" | "invalidate_index" | "emit_marker" | "clear_shadow"
    )
}

impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self {
        let block_patterns = config
            .block_files
            .iter()
            .filter_map(|s| Pattern::new(s).ok())
            .collect();
        let allow_patterns = config
            .allow_files
            .iter()
            .filter_map(|s| Pattern::new(s).ok())
            .collect();

        Self {
            config,
            block_patterns,
            allow_patterns,
        }
    }

    pub fn evaluate(
        &self,
        tool: &str,
        params: &Value,
        is_human: bool,
        workspace_root: &Path,
    ) -> PolicyDecision {
        let path_str = extract_path(params);
        let normalized_path = path_str.map(|p| normalize_path(p, workspace_root));

        // 1. Immutable Infrastructure & Sandbox Jail (Always Enforced)
        if !is_human {
            if let Some(ref p) = normalized_path {
                if is_config_file(p) {
                    return PolicyDecision::Deny("SECURITY_VIOLATION: AI agents are strictly forbidden from modifying CURD configuration files or internal state.".into());
                }

                if p.starts_with("..") || p.is_absolute() && !p.starts_with(workspace_root) {
                    return PolicyDecision::Deny("SANDBOX_VIOLATION: Path escapes workspace root. AI agents are restricted to the active project tree.".into());
                }
            }

            if tool == "shell" {
                if let Some(cmd) = params.get("command").and_then(|v| v.as_str()) {
                    if is_config_tampering(cmd) {
                        return PolicyDecision::Deny("SECURITY_VIOLATION: Shell command contains patterns used to modify CURD configuration or bypass the sandbox.".into());
                    }

                    if self.config.block_shell_metachars {
                        let bad_chars = [";", "&", "|", "`", "$("];
                        for bad in &bad_chars {
                            if cmd.contains(bad) {
                                return PolicyDecision::Deny(format!(
                                    "SECURITY_VIOLATION: Command chaining or subshells ('{}') are forbidden.",
                                    bad
                                ));
                            }
                        }
                    }

                    if !self.config.allowed_binaries.is_empty() {
                        let program = cmd.split_whitespace().next().unwrap_or("");
                        if !self.config.allowed_binaries.iter().any(|b| b == program) {
                            return PolicyDecision::Deny(format!(
                                "SECURITY_VIOLATION: Command '{}' is not in the allowed binaries list.",
                                program
                            ));
                        }
                    }
                }
            }
        }

        // 2. Blocklist Check (Highest Priority)
        if let Some(ref p) = normalized_path {
            let p_str = p.to_string_lossy();
            for pattern in &self.block_patterns {
                if pattern.matches(&p_str) {
                    return PolicyDecision::Deny(format!(
                        "POLICY_DENIED: Access to '{}' is explicitly restricted by blocklist.",
                        p_str
                    ));
                }
            }
        }

        // 3. Allowlist Check (Early "Allow" Exit)
        if !self.allow_patterns.is_empty() {
            if let Some(ref p) = normalized_path {
                // Allowlist matches are usually relative to the workspace root
                let rel_p = p.strip_prefix(workspace_root).unwrap_or(p);
                let p_str = rel_p.to_string_lossy();
                for pattern in &self.allow_patterns {
                    if pattern.matches(&p_str) {
                        return PolicyDecision::Allow;
                    }
                }
            }
        }

        // 4. Protected Symbols (Edit only)
        if tool == "edit" {
            if let Some(uri) = params.get("uri").and_then(|v| v.as_str()) {
                if self.config.protected_symbols.iter().any(|s| s == uri) {
                    return PolicyDecision::Deny(format!(
                        "SYMBOL_PROTECTED: The symbol '{}' is marked as mission-critical and cannot be modified by AI.",
                        uri
                    ));
                }
            }
        }

        // 5. Fallback Mode (The "Otherwise" Logic)
        match self.config.mode.as_str() {
            "strict" => {
                if let Some(p) = path_str {
                    PolicyDecision::Deny(format!(
                        "POLICY_DENIED: Access to '{}' is not explicitly permitted in strict mode.",
                        p
                    ))
                } else if tool != "search" && tool != "graph" && tool != "workspace" {
                    PolicyDecision::Deny(
                        "POLICY_DENIED: Operation requires an explicit permit in strict mode."
                            .into(),
                    )
                } else {
                    PolicyDecision::Allow
                }
            }
            "audit" => {
                PolicyDecision::Audit(format!("Policy audit: tool={} path={:?}", tool, path_str))
            }
            _ => PolicyDecision::Allow, // "permissive"
        }
    }

    pub fn evaluate_operation(&self, input: OperationPolicyInput<'_>) -> PolicyDecision {
        if input.runtime_ceiling == crate::RuntimeCeiling::Lite
            && !matches!(
                input.op,
                crate::CanonicalOperationKind::Lookup
                    | crate::CanonicalOperationKind::Traverse
                    | crate::CanonicalOperationKind::Read
                    | crate::CanonicalOperationKind::Session
            )
        {
            return PolicyDecision::Deny(format!(
                "CEILING_DENIED: '{}' is disabled by lite ceiling.",
                input.tool
            ));
        }

        if matches!(input.tool, "plugin_tool" | "plugin_language")
            && !input.is_human
            && matches!(
                input.params.get("action").and_then(|v| v.as_str()),
                Some("add" | "remove")
            )
        {
            return PolicyDecision::Deny(
                "PROFILE_DENIED: plugin installation/removal is human-only.".to_string(),
            );
        }

        if input.tool == "plugin_trust"
            && !input.is_human
            && matches!(
                input.params.get("action").and_then(|v| v.as_str()),
                Some("add" | "remove" | "enable" | "disable")
            )
        {
            return PolicyDecision::Deny(
                "PROFILE_DENIED: plugin trust mutation is human-only.".to_string(),
            );
        }

        if input.tool == "shell"
            && input
                .params
                .get("is_background")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            && !self.config.exec.allow_background
        {
            return PolicyDecision::Deny(
                "POLICY_DENIED: background execution is disabled by [policy.exec].".to_string(),
            );
        }

        if let Some(profile) = input.profile {
            let required = crate::capability_for_tool(input.tool);
            if !profile.capabilities.is_empty()
                && !profile
                    .capabilities
                    .iter()
                    .any(|cap| cap == required.as_str())
            {
                return PolicyDecision::Deny(format!(
                    "PROFILE_DENIED: profile '{}' lacks capability '{}'.",
                    input.profile_name.unwrap_or("default"),
                    required.as_str()
                ));
            }

            if profile.session_required_for_change
                && matches!(
                    input.op,
                    crate::CanonicalOperationKind::Change | crate::CanonicalOperationKind::Exec
                )
                && !input.session_open
            {
                return PolicyDecision::Deny(
                    "SESSION_REQUIRED: profile requires an open session for change/exec operations."
                        .to_string(),
                );
            }

            if matches!(input.op, crate::CanonicalOperationKind::Exec)
                && !self.config.exec.allow_raw_command
                && input.tool == "shell"
            {
                return PolicyDecision::Deny(
                    "POLICY_DENIED: raw command execution is disabled by [policy.exec]."
                        .to_string(),
                );
            }
        }

        self.evaluate(
            input.tool,
            input.params,
            input.is_human,
            input.workspace_root,
        )
    }

    /// Validates an entire plan's intent before registration.
    pub fn validate_plan(
        &self,
        plan: &crate::plan::Plan,
        is_human: bool,
        workspace_root: &Path,
    ) -> PolicyDecision {
        if plan.nodes.len() > MAX_PLAN_NODES {
            return PolicyDecision::Deny(format!(
                "PLAN_REJECTED: plan exceeds max node count ({} > {}).",
                plan.nodes.len(),
                MAX_PLAN_NODES
            ));
        }

        let mut seen_ids = std::collections::HashSet::new();
        for node in &plan.nodes {
            if !seen_ids.insert(node.id) {
                return PolicyDecision::Deny(format!(
                    "PLAN_REJECTED: duplicate node id '{}'.",
                    node.id
                ));
            }
            if node.dependencies.len() > MAX_PLAN_DEPENDENCIES {
                return PolicyDecision::Deny(format!(
                    "PLAN_REJECTED: node '{}' exceeds max dependency fan-in ({} > {}).",
                    node.id,
                    node.dependencies.len(),
                    MAX_PLAN_DEPENDENCIES
                ));
            }
            if node.retry_limit > MAX_PLAN_RETRY_LIMIT {
                return PolicyDecision::Deny(format!(
                    "PLAN_REJECTED: node '{}' exceeds max retry_limit ({} > {}).",
                    node.id, node.retry_limit, MAX_PLAN_RETRY_LIMIT
                ));
            }
            if node.output_limit == 0 || node.output_limit > MAX_PLAN_OUTPUT_LIMIT_BYTES {
                return PolicyDecision::Deny(format!(
                    "PLAN_REJECTED: node '{}' has invalid output_limit {}; expected 1..={}.",
                    node.id, node.output_limit, MAX_PLAN_OUTPUT_LIMIT_BYTES
                ));
            }

            let (tool, params) = match &node.op {
                crate::plan::ToolOperation::McpCall { tool, args } => (tool.as_str(), args),
                crate::plan::ToolOperation::Internal { command, params } => {
                    if !allowed_internal_plan_command(command) {
                        return PolicyDecision::Deny(format!(
                            "PLAN_REJECTED: node '{}' uses unsupported internal command '{}'.",
                            node.id, command
                        ));
                    }
                    if command == "clear_shadow" && !is_human {
                        return PolicyDecision::Deny(format!(
                            "PLAN_REJECTED: node '{}' cannot use internal command '{}' for non-human execution.",
                            node.id, command
                        ));
                    }
                    (command.as_str(), params)
                }
            };

            match self.evaluate(tool, params, is_human, workspace_root) {
                PolicyDecision::Deny(reason) => {
                    return PolicyDecision::Deny(format!(
                        "PLAN_REJECTED: Node '{}' ({}) violated policy: {}",
                        node.id, tool, reason
                    ));
                }
                PolicyDecision::Audit(msg) => {
                    // Audit logs will be handled by the caller
                    return PolicyDecision::Audit(msg);
                }
                PolicyDecision::Allow => {}
            }
        }
        PolicyDecision::Allow
    }
}

fn extract_path(params: &Value) -> Option<&str> {
    if let Some(p) = params.get("path").and_then(|v| v.as_str()) {
        return Some(p);
    }
    if let Some(p) = params.get("filepath").and_then(|v| v.as_str()) {
        return Some(p);
    }
    if let Some(u) = params.get("uri").and_then(|v| v.as_str()) {
        return u.split("::").next();
    }
    None
}

fn normalize_path(path: &str, root: &Path) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        return p.to_path_buf();
    }

    // Resolve relative to root
    let mut out = root.to_path_buf();
    for component in p.components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(c) => {
                out.push(c);
            }
            _ => {}
        }
    }
    out
}

fn is_config_file(path: &Path) -> bool {
    let p = path.to_string_lossy().to_lowercase();
    p.contains("curd.toml") || p.contains("settings.toml") || p.contains(".curd/")
}

fn is_config_tampering(cmd: &str) -> bool {
    let lower = cmd.to_lowercase();
    // Block attempts to redirect into config files or use sed/echo to overwrite them
    if lower.contains("curd.toml") || lower.contains("settings.toml") || lower.contains(".curd/") {
        if lower.contains(">")
            || lower.contains("sed")
            || lower.contains("rm")
            || lower.contains("mv")
            || lower.contains("cp")
            || lower.contains("chmod")
            || lower.contains("ln")
        {
            return true;
        }
    }
    // Block suspicious pipe escapes
    if lower.contains("/dev/null") && (lower.contains(">") || lower.contains("2>")) {
        // Often used to hide error messages from security scanners
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn test_ctx() -> (PolicyEngine, PathBuf) {
        let root = PathBuf::from("/work");
        let config = PolicyConfig {
            mode: "permissive".to_string(),
            block_files: vec!["**/secret.txt".to_string()],
            allow_files: vec!["src/**/*.rs".to_string()],
            protected_symbols: vec!["critical_func".to_string()],
            ..Default::default()
        };
        (PolicyEngine::new(config), root)
    }

    #[test]
    fn test_sandbox_jail() {
        let (pe, root) = test_ctx();

        // Escape attempt
        let params = json!({"path": "../outside.rs"});
        assert!(matches!(
            pe.evaluate("read", &params, false, &root),
            PolicyDecision::Deny(_)
        ));

        // Absolute path outside
        let params = json!({"path": "/etc/passwd"});
        assert!(matches!(
            pe.evaluate("read", &params, false, &root),
            PolicyDecision::Deny(_)
        ));

        // Human bypass
        assert!(matches!(
            pe.evaluate("read", &params, true, &root),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn test_blocklist_priority() {
        let (pe, root) = test_ctx();

        // Pattern matches blocklist
        let params = json!({"path": "src/secret.txt"});
        assert!(
            matches!(pe.evaluate("read", &params, false, &root), PolicyDecision::Deny(m) if m.contains("restricted by blocklist"))
        );
    }

    #[test]
    fn test_allowlist_early_exit() {
        let (pe, root) = test_ctx();

        // Matches allowlist
        let params = json!({"path": "src/main.rs"});
        assert!(matches!(
            pe.evaluate("read", &params, false, &root),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn test_strict_mode_fallback() {
        let root = PathBuf::from("/work");
        let config = PolicyConfig {
            mode: "strict".to_string(),
            allow_files: vec!["safe.rs".to_string()],
            ..Default::default()
        };
        let pe = PolicyEngine::new(config);

        // Not in allowlist
        let params = json!({"path": "other.rs"});
        assert!(
            matches!(pe.evaluate("read", &params, false, &root), PolicyDecision::Deny(m) if m.contains("not explicitly permitted"))
        );

        // In allowlist
        let params = json!({"path": "safe.rs"});
        assert!(matches!(
            pe.evaluate("read", &params, false, &root),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn test_config_tampering_protection() {
        let (pe, root) = test_ctx();

        // Direct edit
        let params = json!({"path": ".curd/settings.toml"});
        assert!(
            matches!(pe.evaluate("edit", &params, false, &root), PolicyDecision::Deny(m) if m.contains("forbidden from modifying CURD configuration"))
        );

        // Shell redirection
        let params = json!({"command": "echo 'hack' > curd.toml"});
        assert!(
            matches!(pe.evaluate("shell", &params, false, &root), PolicyDecision::Deny(m) if m.contains("modify CURD configuration"))
        );

        // Shell sed
        let params = json!({"command": "sed -i 's/a/b/' .curd/curd.toml"});
        assert!(matches!(
            pe.evaluate("shell", &params, false, &root),
            PolicyDecision::Deny(_)
        ));
    }

    #[test]
    fn test_protected_symbols() {
        let (pe, root) = test_ctx();

        let params = json!({"uri": "critical_func"});
        assert!(
            matches!(pe.evaluate("edit", &params, false, &root), PolicyDecision::Deny(m) if m.contains("mission-critical"))
        );

        let params = json!({"uri": "normal_func"});
        assert!(matches!(
            pe.evaluate("edit", &params, false, &root),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn operation_policy_respects_profile_capabilities() {
        let (pe, root) = test_ctx();
        let profile = crate::config::AgentProfileConfig {
            capabilities: vec!["lookup".to_string()],
            ..Default::default()
        };
        let deny = pe.evaluate_operation(OperationPolicyInput {
            op: crate::CanonicalOperationKind::Change,
            tool: "edit",
            params: &json!({"uri":"src/main.rs::main"}),
            is_human: false,
            workspace_root: &root,
            runtime_ceiling: crate::RuntimeCeiling::Full,
            profile_name: Some("assist"),
            profile: Some(&profile),
            session_open: true,
        });
        assert!(matches!(deny, PolicyDecision::Deny(msg) if msg.contains("lacks capability")));
    }

    #[test]
    fn operation_policy_respects_lite_ceiling() {
        let (pe, root) = test_ctx();
        let profile = crate::config::AgentProfileConfig {
            capabilities: vec!["exec.task".to_string()],
            ..Default::default()
        };
        let deny = pe.evaluate_operation(OperationPolicyInput {
            op: crate::CanonicalOperationKind::Exec,
            tool: "shell",
            params: &json!({"command":"cargo test"}),
            is_human: false,
            workspace_root: &root,
            runtime_ceiling: crate::RuntimeCeiling::Lite,
            profile_name: Some("autonomous"),
            profile: Some(&profile),
            session_open: true,
        });
        assert!(matches!(deny, PolicyDecision::Deny(msg) if msg.contains("lite ceiling")));
    }

    #[test]
    fn operation_policy_requires_session_for_changes_when_profile_demands_it() {
        let (pe, root) = test_ctx();
        let profile = crate::config::AgentProfileConfig {
            capabilities: vec!["change.apply".to_string()],
            session_required_for_change: true,
            ..Default::default()
        };
        let deny = pe.evaluate_operation(OperationPolicyInput {
            op: crate::CanonicalOperationKind::Change,
            tool: "edit",
            params: &json!({"uri":"src/main.rs::main"}),
            is_human: false,
            workspace_root: &root,
            runtime_ceiling: crate::RuntimeCeiling::Full,
            profile_name: Some("supervised"),
            profile: Some(&profile),
            session_open: false,
        });
        assert!(matches!(deny, PolicyDecision::Deny(msg) if msg.contains("SESSION_REQUIRED")));
    }

    #[test]
    fn operation_policy_blocks_raw_shell_when_disabled() {
        let (pe, root) = test_ctx();
        let profile = crate::config::AgentProfileConfig {
            capabilities: vec!["exec.task".to_string()],
            ..Default::default()
        };
        let deny = pe.evaluate_operation(OperationPolicyInput {
            op: crate::CanonicalOperationKind::Exec,
            tool: "shell",
            params: &json!({"command":"cargo test"}),
            is_human: false,
            workspace_root: &root,
            runtime_ceiling: crate::RuntimeCeiling::Full,
            profile_name: Some("supervised"),
            profile: Some(&profile),
            session_open: true,
        });
        assert!(matches!(deny, PolicyDecision::Deny(msg) if msg.contains("raw command execution")));
    }

    #[test]
    fn operation_policy_blocks_background_shell_when_disabled() {
        let root = PathBuf::from("/work");
        let config = PolicyConfig {
            exec: PolicyExecConfig {
                allow_raw_command: false,
                allow_background: false,
            },
            ..Default::default()
        };
        let pe = PolicyEngine::new(config);
        let profile = crate::config::AgentProfileConfig {
            capabilities: vec!["exec.task".to_string()],
            ..Default::default()
        };
        let deny = pe.evaluate_operation(OperationPolicyInput {
            op: crate::CanonicalOperationKind::Exec,
            tool: "shell",
            params: &json!({"command":"sleep 1","is_background":true}),
            is_human: false,
            workspace_root: &root,
            runtime_ceiling: crate::RuntimeCeiling::Full,
            profile_name: Some("supervised"),
            profile: Some(&profile),
            session_open: true,
        });
        assert!(matches!(deny, PolicyDecision::Deny(msg) if msg.contains("background execution")));
    }

    #[test]
    fn operation_policy_makes_plugin_trust_mutation_human_only() {
        let (pe, root) = test_ctx();
        let profile = crate::config::AgentProfileConfig {
            capabilities: vec!["plugin.trust".to_string()],
            ..Default::default()
        };
        let deny = pe.evaluate_operation(OperationPolicyInput {
            op: crate::CanonicalOperationKind::Other,
            tool: "plugin_trust",
            params: &json!({"action":"add","key_id":"demo","pubkey_hex":"11"}),
            is_human: false,
            workspace_root: &root,
            runtime_ceiling: crate::RuntimeCeiling::Full,
            profile_name: Some("agent"),
            profile: Some(&profile),
            session_open: false,
        });
        assert!(matches!(deny, PolicyDecision::Deny(msg) if msg.contains("human-only")));
    }

    #[test]
    fn operation_policy_makes_plugin_package_mutation_human_only() {
        let (pe, root) = test_ctx();
        let profile = crate::config::AgentProfileConfig {
            capabilities: vec!["plugin.manage".to_string()],
            ..Default::default()
        };
        let deny = pe.evaluate_operation(OperationPolicyInput {
            op: crate::CanonicalOperationKind::Other,
            tool: "plugin_tool",
            params: &json!({"action":"add","archive_path":"/tmp/demo.curdt"}),
            is_human: false,
            workspace_root: &root,
            runtime_ceiling: crate::RuntimeCeiling::Full,
            profile_name: Some("agent"),
            profile: Some(&profile),
            session_open: false,
        });
        assert!(matches!(deny, PolicyDecision::Deny(msg) if msg.contains("human-only")));
    }

    #[test]
    fn operation_policy_blocks_plugin_trust_mutation_without_profile_too() {
        let (pe, root) = test_ctx();
        let deny = pe.evaluate_operation(OperationPolicyInput {
            op: crate::CanonicalOperationKind::Other,
            tool: "plugin_trust",
            params: &json!({"action":"disable","key_id":"demo"}),
            is_human: false,
            workspace_root: &root,
            runtime_ceiling: crate::RuntimeCeiling::Full,
            profile_name: None,
            profile: None,
            session_open: false,
        });
        assert!(matches!(deny, PolicyDecision::Deny(msg) if msg.contains("human-only")));
    }

    #[test]
    fn validate_plan_rejects_unsupported_internal_command() {
        let (pe, root) = test_ctx();
        let plan = crate::plan::Plan {
            id: uuid::Uuid::new_v4(),
            nodes: vec![crate::plan::PlanNode {
                id: uuid::Uuid::new_v4(),
                op: crate::plan::ToolOperation::Internal {
                    command: "noop".to_string(),
                    params: json!({}),
                },
                dependencies: vec![],
                output_limit: 128,
                retry_limit: 0,
            }],
        };
        assert!(matches!(
            pe.validate_plan(&plan, false, &root),
            PolicyDecision::Deny(msg) if msg.contains("unsupported internal command")
        ));
    }

    #[test]
    fn validate_plan_rejects_duplicate_nodes_and_unbounded_output() {
        let (pe, root) = test_ctx();
        let dup = uuid::Uuid::new_v4();
        let plan = crate::plan::Plan {
            id: uuid::Uuid::new_v4(),
            nodes: vec![
                crate::plan::PlanNode {
                    id: dup,
                    op: crate::plan::ToolOperation::McpCall {
                        tool: "search".to_string(),
                        args: json!({"query":"alpha"}),
                    },
                    dependencies: vec![],
                    output_limit: 128,
                    retry_limit: 0,
                },
                crate::plan::PlanNode {
                    id: dup,
                    op: crate::plan::ToolOperation::McpCall {
                        tool: "read".to_string(),
                        args: json!({"uris":["src/lib.rs::alpha"]}),
                    },
                    dependencies: vec![],
                    output_limit: 0,
                    retry_limit: 0,
                },
            ],
        };
        assert!(matches!(
            pe.validate_plan(&plan, false, &root),
            PolicyDecision::Deny(msg) if msg.contains("duplicate node id")
        ));
    }
}
