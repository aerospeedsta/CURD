use curd_core::{
    CanonicalOperationKind, EngineContext, RuntimeCeiling, active_runtime_ceiling_from_config,
    canonical_op_for_tool, capability_for_tool,
    tool_allowed_by_ceiling as core_tool_allowed_by_ceiling, validate_tool_call_core,
};
use serde_json::{Value, json};

pub fn active_runtime_ceiling(ctx: &EngineContext) -> RuntimeCeiling {
    active_runtime_ceiling_from_config(&ctx.config)
}

pub fn tool_allowed_by_ceiling(ceiling: RuntimeCeiling, tool: &str, args: &Value) -> bool {
    core_tool_allowed_by_ceiling(ceiling, tool, args)
}

pub fn validate_tool_call(
    ctx: &EngineContext,
    tool: &str,
    args: &Value,
    is_human: bool,
) -> Result<(RuntimeCeiling, CanonicalOperationKind, Option<String>), Value> {
    match validate_tool_call_core(ctx, tool, args, is_human) {
        Ok(validated) => Ok(validated),
        Err(err) => Err(json!({
            "error": {
                "code": err.code,
                "message": err.message,
                "details": if err.details.is_null() {
                    json!({
                        "tool": tool,
                        "capability": capability_for_tool(tool).as_str()
                    })
                } else {
                    err.details
                }
            }
        })),
    }
}

pub fn annotate_tool_entry(entry: &mut Value) {
    let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
        return;
    };
    let capability = capability_for_tool(name);
    let op = canonical_op_for_tool(name);
    let workspace_actions = if name == "workspace" {
        json!([
            "status",
            "list",
            "dependencies",
            "begin",
            "commit",
            "rollback",
            "diff"
        ])
    } else {
        Value::Null
    };
    let session_required = matches!(
        capability.as_str(),
        "change.prepare"
            | "change.apply"
            | "change.revert"
            | "exec.task"
            | "exec.command"
            | "plan.execute"
            | "plan.parallel"
    );
    let approval_requirement = match capability.as_str() {
        "session.commit" => json!("user_or_policy"),
        "change.apply" => json!("profile_or_policy"),
        "exec.command" => json!("policy"),
        _ => Value::Null,
    };
    let x_curd = entry.get("x-curd").cloned().unwrap_or_else(|| json!({}));
    let mut obj = x_curd.as_object().cloned().unwrap_or_default();
    obj.insert("capability".to_string(), json!(capability.as_str()));
    obj.insert("operation".to_string(), json!(op));
    obj.insert(
        "lite_available".to_string(),
        json!(tool_allowed_by_ceiling(
            RuntimeCeiling::Lite,
            name,
            &json!({})
        )),
    );
    obj.insert("workspace_actions".to_string(), workspace_actions);
    obj.insert("session_required".to_string(), json!(session_required));
    obj.insert("approval_requirement".to_string(), approval_requirement);
    entry["x-curd"] = Value::Object(obj);
}
