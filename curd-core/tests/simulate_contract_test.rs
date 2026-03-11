use curd_core::context::handle_simulate;
use serde_json::json;

#[tokio::test]
async fn simulate_marks_mutating_plan_as_session_required() -> anyhow::Result<()> {
    let res = handle_simulate(&json!({
        "mode": "execute_plan",
        "plan": {
            "id": "00000000-0000-0000-0000-000000000000",
            "nodes": [{
                "id": "00000000-0000-0000-0000-000000000001",
                "op": {"McpCall": {"tool": "manage_file", "args": {"action":"create","path":"tmp.txt","content":"hi"}}},
                "dependencies": [],
                "output_limit": 1024,
                "retry_limit": 0
            }]
        }
    }))
    .await;

    let warnings = res["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|w| {
            w["code"].as_str() == Some("session_required")
                && w["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("active workspace session")
        }),
        "simulate should flag session requirement: {}",
        res
    );
    Ok(())
}
