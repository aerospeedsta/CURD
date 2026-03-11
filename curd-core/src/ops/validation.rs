use crate::config::{AgentProfileConfig, CurdConfig};
use crate::policy::OperationPolicyInput;
use crate::{
    CanonicalOperationKind, EngineContext, RuntimeCeiling, canonical_op_for_tool,
    capability_for_tool,
};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct OperationValidationError {
    pub code: i64,
    pub message: String,
    pub details: Value,
}

pub fn active_runtime_ceiling_from_config(config: &CurdConfig) -> RuntimeCeiling {
    match std::env::var("CURD_MODE")
        .ok()
        .unwrap_or_else(|| config.runtime.ceiling.clone())
        .to_ascii_lowercase()
        .as_str()
    {
        "lite" => RuntimeCeiling::Lite,
        _ => RuntimeCeiling::Full,
    }
}

pub fn resolve_profile<'a>(
    config: &'a CurdConfig,
    params: &'a Value,
) -> (Option<&'a str>, Option<&'a AgentProfileConfig>) {
    let selected = params
        .get("profile")
        .and_then(|v| v.as_str())
        .or_else(|| {
            params
                .get("arguments")
                .and_then(|v| v.get("profile"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("default");
    let profile = config.profiles.get(selected);
    (Some(selected), profile)
}

pub fn tool_allowed_by_ceiling(ceiling: RuntimeCeiling, tool: &str, args: &Value) -> bool {
    match ceiling {
        RuntimeCeiling::Full => true,
        RuntimeCeiling::Lite => {
            if tool == "workspace" {
                let action = args
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("status");
                return matches!(action, "status" | "list" | "dependencies");
            }
            matches!(tool, "search" | "read" | "edit" | "graph" | "workspace")
        }
    }
}

pub fn validate_tool_call_core(
    ctx: &EngineContext,
    tool: &str,
    args: &Value,
    is_human: bool,
) -> Result<(RuntimeCeiling, CanonicalOperationKind, Option<String>), OperationValidationError> {
    let ceiling = active_runtime_ceiling_from_config(&ctx.config);
    if !tool_allowed_by_ceiling(ceiling, tool, args) {
        return Err(OperationValidationError {
            code: -32601,
            message: format!("Tool disabled by runtime ceiling: {}", tool),
            details: Value::Null,
        });
    }

    let op = canonical_op_for_tool(tool);
    let (profile_name, profile) = resolve_profile(&ctx.config, args);
    let session_open = args
        .get("connection_token")
        .or_else(|| args.get("session_token"))
        .and_then(|v| v.as_str())
        .map(|token| !token.is_empty())
        .unwrap_or(false);

    match ctx.policy_engine.evaluate_operation(OperationPolicyInput {
        op,
        tool,
        params: args,
        is_human,
        workspace_root: &ctx.workspace_root,
        runtime_ceiling: ceiling,
        profile_name,
        profile,
        session_open,
    }) {
        crate::policy::PolicyDecision::Allow => {
            Ok((ceiling, op, profile_name.map(ToOwned::to_owned)))
        }
        crate::policy::PolicyDecision::Audit(msg) => Ok((
            ceiling,
            op,
            profile_name.map(|name| format!("{}|audit:{}", name, msg)),
        )),
        crate::policy::PolicyDecision::Deny(message) => Err(OperationValidationError {
            code: -32001,
            message,
            details: json!({
                "tool": tool,
                "capability": capability_for_tool(tool).as_str(),
                "operation": op,
                "profile": profile_name
            }),
        }),
    }
}
