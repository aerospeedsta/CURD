use serde::{Deserialize, Serialize};
use serde_json::Value;
use glob::Pattern;
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
}

fn default_true() -> bool { true }

fn default_allowed_binaries() -> Vec<String> {
    vec![
        "cargo", "npm", "yarn", "pnpm", "bun", "python", "python3", "pytest",
        "node", "make", "ninja", "cmake", "go", "gcc", "clang", "g++", "clang++",
        "rustc", "tsc", "jest", "vitest", "npx", "echo", "sleep", "cat", "ls", "grep"
    ].into_iter().map(|s| s.to_string()).collect()
}

fn default_policy_mode() -> String {
    "permissive".to_string()
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

impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self {
        let block_patterns = config.block_files.iter()
            .filter_map(|s| Pattern::new(s).ok())
            .collect();
        let allow_patterns = config.allow_files.iter()
            .filter_map(|s| Pattern::new(s).ok())
            .collect();
            
        Self {
            config,
            block_patterns,
            allow_patterns,
        }
    }

    pub fn evaluate(&self, tool: &str, params: &Value, is_human: bool, workspace_root: &Path) -> PolicyDecision {
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
                                return PolicyDecision::Deny(format!("SECURITY_VIOLATION: Command chaining or subshells ('{}') are forbidden.", bad));
                            }
                        }
                    }

                    if !self.config.allowed_binaries.is_empty() {
                        let program = cmd.split_whitespace().next().unwrap_or("");
                        if !self.config.allowed_binaries.iter().any(|b| b == program) {
                            return PolicyDecision::Deny(format!("SECURITY_VIOLATION: Command '{}' is not in the allowed binaries list.", program));
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
                    return PolicyDecision::Deny(format!("POLICY_DENIED: Access to '{}' is explicitly restricted by blocklist.", p_str));
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
                    return PolicyDecision::Deny(format!("SYMBOL_PROTECTED: The symbol '{}' is marked as mission-critical and cannot be modified by AI.", uri));
                }
            }
        }

        // 5. Fallback Mode (The "Otherwise" Logic)
        match self.config.mode.as_str() {
            "strict" => {
                if let Some(p) = path_str {
                    PolicyDecision::Deny(format!("POLICY_DENIED: Access to '{}' is not explicitly permitted in strict mode.", p))
                } else if tool != "search" && tool != "graph" && tool != "workspace" {
                    PolicyDecision::Deny("POLICY_DENIED: Operation requires an explicit permit in strict mode.".into())
                } else {
                    PolicyDecision::Allow
                }
            }
            "audit" => PolicyDecision::Audit(format!("Policy audit: tool={} path={:?}", tool, path_str)),
            _ => PolicyDecision::Allow, // "permissive"
        }
    }

    /// Validates an entire plan's intent before registration.
    pub fn validate_plan(&self, plan: &crate::plan::Plan, is_human: bool, workspace_root: &Path) -> PolicyDecision {
        for node in &plan.nodes {
            let (tool, params) = match &node.op {
                crate::plan::ToolOperation::McpCall { tool, args } => (tool.as_str(), args),
                crate::plan::ToolOperation::Internal { command, params } => (command.as_str(), params),
            };

            match self.evaluate(tool, params, is_human, workspace_root) {
                PolicyDecision::Deny(reason) => {
                    return PolicyDecision::Deny(format!("PLAN_REJECTED: Node '{}' ({}) violated policy: {}", node.id, tool, reason));
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
            Component::CurDir => {},
            Component::ParentDir => { out.pop(); },
            Component::Normal(c) => { out.push(c); },
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
        if lower.contains(">") || lower.contains("sed") || lower.contains("rm") || lower.contains("mv") || lower.contains("cp") || lower.contains("chmod") || lower.contains("ln") {
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
        assert!(matches!(pe.evaluate("read", &params, false, &root), PolicyDecision::Deny(_)));

        // Absolute path outside
        let params = json!({"path": "/etc/passwd"});
        assert!(matches!(pe.evaluate("read", &params, false, &root), PolicyDecision::Deny(_)));
        
        // Human bypass
        assert!(matches!(pe.evaluate("read", &params, true, &root), PolicyDecision::Allow));
    }

    #[test]
    fn test_blocklist_priority() {
        let (pe, root) = test_ctx();
        
        // Pattern matches blocklist
        let params = json!({"path": "src/secret.txt"});
        assert!(matches!(pe.evaluate("read", &params, false, &root), PolicyDecision::Deny(m) if m.contains("restricted by blocklist")));
    }

    #[test]
    fn test_allowlist_early_exit() {
        let (pe, root) = test_ctx();
        
        // Matches allowlist
        let params = json!({"path": "src/main.rs"});
        assert!(matches!(pe.evaluate("read", &params, false, &root), PolicyDecision::Allow));
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
        assert!(matches!(pe.evaluate("read", &params, false, &root), PolicyDecision::Deny(m) if m.contains("not explicitly permitted")));
        
        // In allowlist
        let params = json!({"path": "safe.rs"});
        assert!(matches!(pe.evaluate("read", &params, false, &root), PolicyDecision::Allow));
    }

    #[test]
    fn test_config_tampering_protection() {
        let (pe, root) = test_ctx();
        
        // Direct edit
        let params = json!({"path": ".curd/settings.toml"});
        assert!(matches!(pe.evaluate("edit", &params, false, &root), PolicyDecision::Deny(m) if m.contains("forbidden from modifying CURD configuration")));

        // Shell redirection
        let params = json!({"command": "echo 'hack' > curd.toml"});
        assert!(matches!(pe.evaluate("shell", &params, false, &root), PolicyDecision::Deny(m) if m.contains("modify CURD configuration")));
        
        // Shell sed
        let params = json!({"command": "sed -i 's/a/b/' .curd/curd.toml"});
        assert!(matches!(pe.evaluate("shell", &params, false, &root), PolicyDecision::Deny(_)));
    }

    #[test]
    fn test_protected_symbols() {
        let (pe, root) = test_ctx();
        
        let params = json!({"uri": "critical_func"});
        assert!(matches!(pe.evaluate("edit", &params, false, &root), PolicyDecision::Deny(m) if m.contains("mission-critical")));
        
        let params = json!({"uri": "normal_func"});
        assert!(matches!(pe.evaluate("edit", &params, false, &root), PolicyDecision::Allow));
    }
}

