use curd_core::{EngineContext, mcp::{McpServerMode, handle_tools_call}};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;

macro_rules! call_tool {
    ($name:expr, $args:expr, $ctx:expr) => {
        {
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
        }
    };
}

#[tokio::test]
async fn test_auth_handshake_and_isolation() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join(".curd/identities"))?;
    std::fs::write(root.join("curd.toml"), "[edit]\nenforce_transactional = false\n")?;

    let ctx = EngineContext::new(root.to_str().unwrap());

    // 1. Generate Keypair (simulating an agent)
    let mut secret_bytes = [0u8; 32];
    getrandom::fill(&mut secret_bytes).unwrap();
    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let verifying_key = signing_key.verifying_key();
    let pub_hex = verifying_key
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    let auth_file = root.join(".curd/authorized_agents.json");
    std::fs::write(&auth_file, format!("{{\"test_agent\": \"{}\"}}", pub_hex)).unwrap();

    // 2. Open Session (Challenge)
    let open_res = call_tool!("session_open", json!({"pubkey_hex": pub_hex}), ctx);
    println!("DEBUG: open_res = {}", open_res);
    assert_eq!(open_res["status"], "ok");
    let nonce_hex = open_res["nonce"].as_str().unwrap();

    // 3. Sign Nonce
    let signature = signing_key.sign(nonce_hex.as_bytes());
    let sig_hex = signature
        .to_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    // 4. Verify Session
    let verify_res = call_tool!(
        "session_verify",
        json!({
            "pubkey_hex": pub_hex,
            "signature_hex": sig_hex
        }),
        ctx
    );
    println!("DEBUG: verify_res = {}", verify_res);
    assert_eq!(verify_res["status"], "authenticated");
    let session_token = verify_res["session_token"].as_str().unwrap();

    // 5. Execute DSL with Token
    let dsl_res = call_tool!(
        "execute_dsl".to_string(),
        json!({
            "session_token": session_token,
            "nodes": [
                {
                    "type": "assign",
                    "var": "secret",
                    "value": "42"
                }
            ]
        }),
        ctx
    );
    println!("DEBUG: dsl_res = {}", dsl_res);
    assert_eq!(dsl_res["status"], "ok");

    // 5.5 Execute plan with session token; args should resolve from session state.
    let plan_id = Uuid::new_v4();
    let node_id = Uuid::new_v4();
    let plan_res = call_tool!(
        "execute_plan".to_string(),
        json!({
            "session_token": session_token,
            "plan": {
                "id": plan_id,
                "nodes": [
                    {
                        "id": node_id,
                        "op": {
                            "McpCall": {
                                "tool": "manage_file",
                                "args": {"action":"create","path":"$secret"}
                            }
                        },
                        "dependencies": [],
                        "output_limit": 65536,
                        "retry_limit": 0
                    }
                ]
            }
        }),
        ctx
    );
    println!("DEBUG: plan_res = {}", plan_res);
    assert_eq!(plan_res["status"], "ok");
    assert!(root.join("42").exists());

    // 6. Verify State Isolation
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
    drop(connections);

    // 7. Execute DSL without Token (Should Fail)
    let fail_res = call_tool!(
        "execute_dsl".to_string(),
        json!({
            "nodes": [
                {
                    "type": "assign",
                    "var": "secret",
                    "value": "99"
                }
            ]
        }),
        ctx
    );

    assert!(fail_res["error"].as_str().unwrap().contains("Unauthorized"));

    Ok(())
}
