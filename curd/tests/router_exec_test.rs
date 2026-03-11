mod router_common;

use curd::router::{
    route_execute_dsl, route_execute_plan, route_run_script, route_tool_call,
    route_validated_tool_call,
};
use curd_core::EngineContext;
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;

use router_common::open_test_connection;

#[tokio::test]
async fn route_tool_call_executes_search_via_shared_router() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    let ctx = EngineContext::new(root.to_str().unwrap());

    let res = route_tool_call("search", &json!({"query":"alpha","mode":"symbol"}), &ctx).await;
    assert!(
        res.get("error").is_none(),
        "router search should succeed: {}",
        res
    );
    Ok(())
}

#[tokio::test]
async fn route_tool_call_executes_read_via_shared_router() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    let ctx = EngineContext::new(root.to_str().unwrap());

    let res = route_tool_call(
        "read",
        &json!({"uris":["src/lib.rs::alpha"],"verbosity":1}),
        &ctx,
    )
    .await;
    assert_eq!(res.get("status").and_then(|v| v.as_str()), Some("ok"));
    assert!(res["results"].is_array());
    Ok(())
}

#[tokio::test]
async fn route_execute_dsl_reuses_shared_plan_execution_logic() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    let ctx = EngineContext::new(root.to_str().unwrap());
    let token = open_test_connection(&ctx, root).await?;

    let res = route_execute_dsl(
        &json!({"session_token": token.clone(),"nodes":[{"type":"assign","var":"alpha","value":"42"}]}),
        &ctx,
    )
    .await;
    assert_eq!(res.get("status").and_then(|v| v.as_str()), Some("ok"));

    let connections = ctx.connections.lock().await;
    let entry = connections.get(&token).expect("connection entry");
    assert_eq!(
        entry.state.variables.get("alpha").and_then(|v| v.as_str()),
        Some("42")
    );
    Ok(())
}

#[tokio::test]
async fn route_execute_dsl_requires_active_workspace_session_for_mutation() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    std::fs::write(
        root.join("settings.toml"),
        r#"[profiles.supervised]
role = "supervised_agent"
capabilities = ["lookup", "traverse", "read", "change.apply", "session.begin", "session.verify", "exec.task", "plan.execute", "review.run"]
session_required_for_change = true
"#,
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let token = open_test_connection(&ctx, root).await?;

    let res = route_execute_dsl(
        &json!({"profile":"supervised","session_token":token,"nodes":[{"type":"call","tool":"edit","args":{"uri":"src/lib.rs::alpha","code":"pub fn alpha() {}","adaptation_justification":"router test"}}]}),
        &ctx,
    )
    .await;
    assert!(
        res["error"]
            .as_str()
            .unwrap_or("")
            .contains("requires an active workspace session")
    );
    Ok(())
}

#[tokio::test]
async fn route_run_script_executes_read_only_curd_program() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::write(
        root.join("status.curd"),
        "use profile assist\narg action: string = \"status\"\nworkspace action=$action\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());

    let (res, state) = route_run_script(
        &root.join("status.curd"),
        &serde_json::Map::new(),
        &json!({}),
        &ctx,
        Some({
            let mut state = curd_core::ReplState::new();
            state.is_human_actor = true;
            state
        }),
    )
    .await;
    assert_eq!(res["status"], "ok");
    assert_eq!(state.variables.get("action"), Some(&json!("status")));
    Ok(())
}

#[tokio::test]
async fn route_run_script_requires_session_for_mutating_program() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n")?;
    std::fs::write(
        root.join("mutate.curd"),
        "arg target: string = \"src/lib.rs::alpha\"\nlet patch = \"pub fn alpha() {}\"\natomic {\n  edit uri=$target code=$patch adaptation_justification=\"script test\"\n}\n",
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());

    let (res, _) = route_run_script(
        &root.join("mutate.curd"),
        &serde_json::Map::new(),
        &json!({}),
        &ctx,
        Some({
            let mut state = curd_core::ReplState::new();
            state.is_human_actor = true;
            state
        }),
    )
    .await;
    assert!(
        res["error"]
            .as_str()
            .unwrap_or("")
            .contains("requires an active workspace session")
    );
    Ok(())
}

#[tokio::test]
async fn route_execute_plan_reuses_shared_plan_execution_logic() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    let ctx = EngineContext::new(root.to_str().unwrap());
    let token = open_test_connection(&ctx, root).await?;
    let begin = route_tool_call("workspace", &json!({"action":"begin"}), &ctx).await;
    assert_eq!(begin["status"], "ok");

    let res = route_execute_plan(
        &json!({"session_token": token.clone(),"plan":{"id":Uuid::new_v4(),"nodes":[{"id":Uuid::new_v4(),"op":{"McpCall":{"tool":"manage_file","args":{"action":"create","path":"hello.txt","content":"hi"}}},"dependencies":[],"output_limit":65536,"retry_limit":0}]}}),
        &ctx,
    ).await;
    assert_eq!(res.get("status").and_then(|v| v.as_str()), Some("ok"));
    Ok(())
}

#[tokio::test]
async fn execute_active_plan_requires_active_workspace_session_for_mutation() -> anyhow::Result<()>
{
    let dir = tempdir()?;
    let root = dir.path();
    let ctx = EngineContext::new(root.to_str().unwrap());
    let token = open_test_connection(&ctx, root).await?;
    {
        let mut state = ctx.global_state.lock().await;
        state.active_plan = Some(curd_core::Plan {
            id: Uuid::new_v4(),
            nodes: vec![curd_core::plan::PlanNode {
                id: Uuid::new_v4(),
                op: curd_core::plan::ToolOperation::McpCall {
                    tool: "manage_file".to_string(),
                    args: json!({"action":"create","path":"hello.txt","content":"hi"}),
                },
                dependencies: vec![],
                output_limit: 65536,
                retry_limit: 0,
            }],
        });
    }
    let res = route_validated_tool_call(
        "execute_active_plan",
        &json!({"session_token": token}),
        &ctx,
        true,
    )
    .await;
    assert!(
        res["error"]
            .as_str()
            .unwrap_or("")
            .contains("requires an open workspace session")
    );
    Ok(())
}

#[tokio::test]
async fn route_execute_plan_rejects_unsafe_internal_command() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    let ctx = EngineContext::new(root.to_str().unwrap());
    let token = open_test_connection(&ctx, root).await?;
    let res = route_execute_plan(
        &json!({"session_token":token,"plan":{"id":Uuid::new_v4(),"nodes":[{"id":Uuid::new_v4(),"op":{"Internal":{"command":"noop","params":{}}},"dependencies":[],"output_limit":128,"retry_limit":0}]}}),
        &ctx,
    ).await;
    assert!(
        res["error"]
            .as_str()
            .unwrap_or("")
            .contains("unsupported internal command")
    );
    Ok(())
}
