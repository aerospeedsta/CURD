use curd::{McpServerMode, handle_tools_call, handle_tools_list};
use curd_core::EngineContext;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;

macro_rules! call_tool {
    ($name:expr, $args:expr, $ctx:expr) => {{
        let req = json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": $name,
                "arguments": $args
            },
            "id": 1
        });
        let res = handle_tools_call(&req, &$ctx, McpServerMode::Full).await;
        let text = res["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str::<serde_json::Value>(text).unwrap()
    }};
}

#[tokio::test]
async fn auth_handshake_and_isolation_still_work_via_curd_mcp() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join(".curd/identities"))?;
    std::fs::write(
        root.join("curd.toml"),
        "[edit]\nenforce_transactional = false\n",
    )?;

    let ctx = EngineContext::new(root.to_str().unwrap());

    let mut secret_bytes = [0u8; 32];
    getrandom::fill(&mut secret_bytes).unwrap();
    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let verifying_key = signing_key.verifying_key();
    let pub_hex = verifying_key
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    std::fs::write(
        root.join(".curd/authorized_agents.json"),
        format!("{{\"test_agent\": \"{}\"}}", pub_hex),
    )?;

    let open_res = call_tool!("session_open", json!({"pubkey_hex": pub_hex}), ctx);
    assert_eq!(open_res["status"], "ok");
    let nonce_hex = open_res["nonce"].as_str().unwrap();

    let signature = signing_key.sign(nonce_hex.as_bytes());
    let sig_hex = signature
        .to_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    let verify_res = call_tool!(
        "session_verify",
        json!({
            "pubkey_hex": pub_hex,
            "signature_hex": sig_hex
        }),
        ctx
    );
    assert_eq!(verify_res["status"], "authenticated");
    let session_token = verify_res["session_token"].as_str().unwrap();

    let dsl_res = call_tool!(
        "execute_dsl".to_string(),
        json!({
            "session_token": session_token,
            "nodes": [{"type": "assign", "var": "secret", "value": "42"}]
        }),
        ctx
    );
    assert_eq!(dsl_res["status"], "ok");

    let begin_res = call_tool!(
        "workspace".to_string(),
        json!({
            "session_token": session_token,
            "action": "begin"
        }),
        ctx
    );
    assert_eq!(begin_res["status"], "ok");

    let plan_res = call_tool!(
        "execute_plan".to_string(),
        json!({
            "session_token": session_token,
            "plan": {
                "id": Uuid::new_v4(),
                "nodes": [{
                    "id": Uuid::new_v4(),
                    "op": {"McpCall": {"tool": "manage_file", "args": {"action":"create","path":"$secret"}}},
                    "dependencies": [],
                    "output_limit": 65536,
                    "retry_limit": 0
                }]
            }
        }),
        ctx
    );
    assert_eq!(plan_res["status"], "ok");
    let shadow_root = ctx
        .we
        .shadow
        .lock()
        .ok()
        .and_then(|shadow| shadow.get_shadow_root().cloned())
        .expect("active shadow root");
    assert!(shadow_root.join("42").exists());

    let rollback_res = call_tool!(
        "workspace".to_string(),
        json!({
            "session_token": session_token,
            "action": "rollback"
        }),
        ctx
    );
    assert_eq!(rollback_res["status"], "ok");

    let connections = ctx.connections.lock().await;
    let entry = connections.get(session_token).unwrap();
    assert_eq!(
        entry
            .state
            .variables
            .get("secret")
            .unwrap()
            .as_str()
            .unwrap(),
        "42"
    );

    Ok(())
}

#[test]
fn tools_list_includes_capability_annotations() {
    let listed = handle_tools_list(McpServerMode::Full);
    let tools = listed["result"]["tools"].as_array().unwrap();
    let search = tools
        .iter()
        .find(|tool| tool["name"] == "search")
        .expect("search tool");
    assert_eq!(search["x-curd"]["capability"], "lookup");
    assert_eq!(search["x-curd"]["operation"], "lookup");
    let edit = tools
        .iter()
        .find(|tool| tool["name"] == "edit")
        .expect("edit tool");
    assert_eq!(edit["x-curd"]["session_required"], true);
    assert_eq!(edit["x-curd"]["approval_requirement"], "profile_or_policy");
}
