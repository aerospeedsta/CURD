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

[profiles.autonomous]
role = "autonomous_agent"
capabilities = ["lookup", "traverse", "read", "change.apply", "session.begin", "session.verify", "session.commit", "exec.task", "plan.execute", "plan.parallel", "review.run", "hook.run"]
session_required_for_change = true
promotion = "policy_gated"
"#
    )
}

#[tokio::test]
async fn profile_matrix_lite_clips_capabilities() -> anyhow::Result<()> {
    let lite_dir = tempdir()?;
    let lite_root = lite_dir.path();
    std::fs::create_dir_all(lite_root.join("src"))?;
    std::fs::write(lite_root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    std::fs::write(lite_root.join("settings.toml"), matrix_settings("lite"))?;
    let lite_ctx = EngineContext::new(lite_root.to_str().unwrap());

    validate_tool_call(
        &lite_ctx,
        "search",
        &json!({"profile":"assist","query":"alpha","mode":"symbol"}),
        false,
    )
    .map_err(|err| anyhow!("search validation failed: {}", err))?;
    let autonomous_build = validate_tool_call(
        &lite_ctx,
        "build",
        &json!({"profile":"autonomous","session_token":"tok","execute":false}),
        false,
    )
    .expect_err("lite ceiling should clip autonomous exec access");
    assert!(
        autonomous_build["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("runtime ceiling")
    );
    Ok(())
}
