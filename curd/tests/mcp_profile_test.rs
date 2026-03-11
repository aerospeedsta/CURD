use curd::{McpServerMode, handle_tools_call};
use curd_core::EngineContext;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn lite_ceiling_blocks_exec_even_if_profile_would_allow() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::write(
        root.join("settings.toml"),
        r#"
[runtime]
ceiling = "lite"

[profiles.autonomous]
role = "autonomous_agent"
capabilities = ["lookup", "exec.task"]
"#,
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let req = json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "shell",
            "arguments": {
                "profile": "autonomous",
                "command": "cargo test"
            }
        },
        "id": 1
    });
    let res = handle_tools_call(&req, &ctx, McpServerMode::Full).await;
    assert!(res.get("error").is_some());
    assert!(
        res["error"]["message"]
            .as_str()
            .unwrap()
            .contains("runtime ceiling")
    );
    Ok(())
}

#[tokio::test]
async fn profile_denies_tool_when_capability_missing() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::write(
        root.join("settings.toml"),
        r#"
[runtime]
ceiling = "full"

[profiles.assist]
role = "assist_agent"
capabilities = ["lookup", "read"]
"#,
    )?;
    let ctx = EngineContext::new(root.to_str().unwrap());
    let req = json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "edit",
            "arguments": {
                "profile": "assist",
                "uri": "src/main.rs::main",
                "code": "fn main() {}"
            }
        },
        "id": 1
    });
    let res = handle_tools_call(&req, &ctx, McpServerMode::Full).await;
    assert!(res.get("error").is_some());
    assert!(
        res["error"]["message"]
            .as_str()
            .unwrap()
            .contains("lacks capability")
    );
    Ok(())
}
