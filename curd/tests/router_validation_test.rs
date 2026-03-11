mod router_common;

use curd::router::{route_batch, route_validated_tool_call};
use curd::validation::{active_runtime_ceiling, validate_tool_call};
use curd_core::{EngineContext, RuntimeCeiling};
use serde_json::json;
use tempfile::tempdir;

use router_common::open_test_connection;

#[tokio::test]
async fn route_execute_dsl_enforces_nested_profile_gate() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    std::fs::write(
        root.join("settings.toml"),
        "[profiles.assist]\nrole = \"assist_agent\"\ncapabilities = [\"lookup\", \"read\"]\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let token = open_test_connection(&ctx, root).await?;
    let begin = curd::router::route_tool_call("workspace", &json!({"action":"begin"}), &ctx).await;
    assert_eq!(begin["status"], "ok");
    let res = curd::router::route_execute_dsl(
        &json!({"profile":"assist","session_token":token,"nodes":[{"type":"call","tool":"edit","args":{"uri":"src/lib.rs::alpha","code":"pub fn alpha() {}","adaptation_justification":"router test"}}]}),
        &ctx,
    ).await;
    assert!(
        res["error"]
            .as_str()
            .unwrap_or("")
            .contains("lacks capability")
    );
    Ok(())
}

#[tokio::test]
async fn route_batch_validates_nested_tool_calls() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::write(
        root.join("settings.toml"),
        "[runtime]\nceiling = \"lite\"\n\n[profiles.autonomous]\nrole = \"autonomous_agent\"\ncapabilities = [\"lookup\", \"exec.task\"]\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let res = route_batch(&json!({"profile":"autonomous","tasks":[{"id":"blocked","tool":"shell","args":{"command":"echo hi"}}]}), &ctx).await;
    assert_eq!(res["status"], "ok");
    assert!(
        res["results"][0]["result"]["error"]
            .as_str()
            .unwrap_or("")
            .contains("runtime ceiling")
    );
    Ok(())
}

#[tokio::test]
async fn validate_tool_call_uses_profile_gate() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::write(
        root.join("settings.toml"),
        "[profiles.assist]\nrole = \"assist_agent\"\ncapabilities = [\"lookup\", \"read\"]\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let err = validate_tool_call(
        &ctx,
        "edit",
        &json!({"profile":"assist","uri":"src/lib.rs::alpha","code":"pub fn alpha() {}"}),
        false,
    )
    .expect_err("edit should be denied");
    assert!(
        err["error"]["message"]
            .as_str()
            .unwrap()
            .contains("lacks capability")
    );
    Ok(())
}

#[tokio::test]
async fn route_validated_tool_call_returns_native_error_shape() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::write(
        root.join("settings.toml"),
        "[runtime]\nceiling = \"lite\"\n\n[profiles.autonomous]\nrole = \"autonomous_agent\"\ncapabilities = [\"lookup\", \"exec.task\"]\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let res = route_validated_tool_call(
        "shell",
        &json!({"profile":"autonomous","command":"cargo test"}),
        &ctx,
        true,
    )
    .await;
    assert!(
        res.get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("runtime ceiling")
    );
    assert!(res.get("details").is_some());
    Ok(())
}

#[tokio::test]
async fn runtime_ceiling_falls_back_to_config_when_env_is_absent() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(
        root.join("settings.toml"),
        "[runtime]\nceiling = \"lite\"\n",
    )
    .unwrap();
    unsafe {
        std::env::remove_var("CURD_MODE");
    }
    let ctx = EngineContext::new(root.to_str().unwrap());
    assert_eq!(active_runtime_ceiling(&ctx), RuntimeCeiling::Lite);
}
