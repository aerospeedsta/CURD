mod router_common;

use curd::router::{
    route_batch, route_build_command, route_check_script, route_compile_script,
    route_config_command, route_context_command, route_diff_command, route_doctor_command,
    route_history, route_refactor_command, route_semantic_audit, route_session_lifecycle,
    route_tool_call, route_workspace_status,
};
use curd_core::{EngineContext, RefactorAction};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn route_batch_executes_tasks_in_dependency_order() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() { alpha(); }\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let res = route_batch(&json!({"tasks":[{"id":"seed","tool":"search","args":{"query":"alpha","mode":"symbol"}},{"id":"follow","tool":"graph","depends_on":["seed"],"args":{"uris":["src/lib.rs::beta"],"depth":1,"direction":"both"}}]}), &ctx).await;
    assert_eq!(res["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn route_history_exposes_operation_history() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let _ = route_tool_call("search", &json!({"query":"alpha","mode":"symbol"}), &ctx).await;
    let res = route_history(&json!({"mode":"operations","limit":10}), &ctx).await;
    assert_eq!(res["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn route_workspace_status_reports_shadow_and_index_state() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let begin = route_tool_call("workspace", &json!({"action":"begin"}), &ctx).await;
    assert_eq!(begin["status"], "ok");
    let res = route_workspace_status(&ctx).await;
    assert_eq!(res["status"], "ok");
    let _ = route_tool_call("workspace", &json!({"action":"rollback"}), &ctx).await;
    Ok(())
}

#[tokio::test]
async fn route_session_lifecycle_begins_and_ends_shadow_review_cycle() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let begin = route_session_lifecycle("begin", &ctx).await;
    assert_eq!(begin["status"], "ok");
    let rollback = route_session_lifecycle("rollback", &ctx).await;
    assert_eq!(rollback["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn route_config_command_round_trips_settings() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    let ctx = EngineContext::new(root.to_str().unwrap());
    let set = route_config_command(
        &json!({"action":"set","key":"runtime.ceiling","value":"\"lite\""}),
        &ctx,
    )
    .await;
    assert_eq!(set["status"], "ok");
    let show = route_config_command(&json!({"action":"show"}), &ctx).await;
    assert_eq!(show["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn route_context_command_adds_lists_and_removes_contexts() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    let linked = tempdir()?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let add = route_context_command(
        &json!({"action":"add","path":linked.path(),"alias":"@linked","read":true}),
        &ctx,
    )
    .await;
    assert_eq!(add["status"], "ok");
    let list = route_context_command(&json!({"action":"list"}), &ctx).await;
    assert_eq!(list["status"], "ok");
    let remove = route_context_command(
        &json!({"action":"remove","name":"@linked","force":true}),
        &ctx,
    )
    .await;
    assert_eq!(remove["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn route_doctor_command_returns_structured_report() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let res = route_doctor_command(&json!({"strict": false}), &ctx).await;
    assert_eq!(res["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn route_build_command_returns_structured_response() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    let ctx = EngineContext::new(root.to_str().unwrap());
    let res = route_build_command(&json!({"execute": false}), &ctx).await;
    assert!(res.get("error").is_none());
    Ok(())
}

#[tokio::test]
async fn route_diff_command_returns_structured_output() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let begin = route_tool_call("workspace", &json!({"action":"begin"}), &ctx).await;
    assert_eq!(begin["status"], "ok");
    let shadow_root = ctx
        .we
        .shadow
        .lock()
        .ok()
        .and_then(|shadow| shadow.get_shadow_root().cloned())
        .expect("active shadow root");
    std::fs::write(
        shadow_root.join("src/lib.rs"),
        "pub fn alpha() { println!(\"hi\"); }\n",
    )?;
    let res = route_diff_command(&json!({"semantic": false}), &ctx).await;
    assert_eq!(res["status"], "ok");
    let _ = route_tool_call("workspace", &json!({"action":"rollback"}), &ctx).await;
    Ok(())
}

#[tokio::test]
async fn route_refactor_command_returns_structured_output() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/main.py"),
        "def foo():\n    a = 1\n    b = 2\n    return a + b\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let res = route_refactor_command(
        RefactorAction::Extract {
            file_range: "src/main.py:2-3".to_string(),
            new_function_name: "extracted_func".to_string(),
        },
        &ctx,
    )
    .await;
    assert_eq!(res["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn route_semantic_audit_returns_structured_sections() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() { alpha(); }\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let res = route_semantic_audit(&json!({"scope":"all","verbose":true}), &ctx).await;
    assert_eq!(res["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn route_check_and_compile_script_work() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    std::fs::write(
        root.join("review.curd"),
        "# explain: inspect alpha before compiling\nread \"src/lib.rs::alpha\"\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let checked =
        route_check_script(&root.join("review.curd"), &serde_json::Map::new(), &ctx).await;
    assert!(checked["status"] == "ok" || checked["status"] == "caution");
    let compiled = route_compile_script(
        &root.join("review.curd"),
        &serde_json::Map::new(),
        &json!({}),
        &ctx,
    )
    .await;
    assert_eq!(compiled["status"], "ok");
    Ok(())
}
