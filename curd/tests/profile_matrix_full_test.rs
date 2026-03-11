use anyhow::anyhow;
use curd::validation::validate_tool_call;
use curd_core::EngineContext;
use serde_json::json;
use tempfile::tempdir;

fn matrix_settings(ceiling: &str) -> String {
    format!(
        r#"[runtime]
ceiling = "{ceiling}"

[profiles.assist]
role = "assist_agent"
capabilities = ["lookup", "read"]
session_required_for_change = true
promotion = "forbidden"

[profiles.supervised]
role = "supervised_agent"
capabilities = ["lookup", "traverse", "read", "change.apply", "session.begin", "session.verify", "exec.task", "plan.execute", "review.run"]
session_required_for_change = true
promotion = "approval_required"

[profiles.autonomous]
role = "autonomous_agent"
capabilities = ["lookup", "traverse", "read", "change.apply", "session.begin", "session.verify", "session.commit", "exec.task", "plan.execute", "plan.parallel", "review.run", "hook.run"]
session_required_for_change = true
promotion = "policy_gated"
"#
    )
}

#[tokio::test]
async fn profile_matrix_full_behaves_as_planned() -> anyhow::Result<()> {
    let full_dir = tempdir()?;
    let full_root = full_dir.path();
    std::fs::create_dir_all(full_root.join("src"))?;
    std::fs::write(full_root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    std::fs::write(full_root.join("settings.toml"), matrix_settings("full"))?;
    let full_ctx = EngineContext::new(full_root.to_str().unwrap());

    validate_tool_call(
        &full_ctx,
        "search",
        &json!({"profile":"assist","query":"alpha","mode":"symbol"}),
        false,
    )
    .map_err(|err| anyhow!("search validation failed: {}", err))?;
    let assist_edit = validate_tool_call(
        &full_ctx,
        "edit",
        &json!({"profile":"assist","session_token":"tok","uri":"src/lib.rs::alpha","code":"pub fn alpha() {}"}),
        false,
    ).expect_err("assist should not gain change capability");
    assert!(
        assist_edit["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("lacks capability")
    );
    validate_tool_call(
        &full_ctx,
        "edit",
        &json!({"profile":"supervised","session_token":"tok","uri":"src/lib.rs::alpha","code":"pub fn alpha() {}"}),
        false,
    )
    .map_err(|err| anyhow!("supervised edit validation failed: {}", err))?;
    validate_tool_call(
        &full_ctx,
        "build",
        &json!({"profile":"autonomous","session_token":"tok","execute":false}),
        false,
    )
    .map_err(|err| anyhow!("autonomous build validation failed: {}", err))?;
    Ok(())
}
