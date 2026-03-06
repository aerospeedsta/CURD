use curd_core::{
    DebugEngine, DiagramEngine, DocEngine, EditEngine, EngineContext, FileEngine, FindEngine,
    GraphEngine, HistoryEngine, LspEngine, PlanEngine, ProfileEngine, ReadEngine, SearchEngine,
    SessionReviewEngine, ShellEngine, Watchdog, WorkspaceEngine,
    mcp::{McpServerMode, handle_tools_call},
};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::{Mutex, broadcast};
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

    let root_path = PathBuf::from(root);
    let (tx_events, _) = broadcast::channel(1024);
    let session_id = Uuid::new_v4();
    let watchdog = Arc::new(Watchdog::new(root_path.clone()));

    let ctx = EngineContext {
        workspace_root: root_path.clone(),
        session_id, read_only: false,
        se: Arc::new(SearchEngine::new(root).with_events(tx_events.clone())),
        re: Arc::new(ReadEngine::new(root)),
        ee: Arc::new(EditEngine::new(root).with_watchdog(watchdog.clone())),
        ge: Arc::new(GraphEngine::new(root)),
            ple: Arc::new(crate::PlanEngine::new(root)),
            she: Arc::new(crate::ShellEngine::new(root)),
        we: Arc::new(WorkspaceEngine::new(root)),
        mu: Arc::new(curd_core::MutationEngine::new(root)),
        fe: Arc::new(FindEngine::new(root)),
        de: Arc::new(DiagramEngine::new(root)),
        fie: Arc::new(FileEngine::new(root)),
        le: Arc::new(LspEngine::new(root)),
        pe: Arc::new(ProfileEngine::new(root)),
        dbe: Arc::new(DebugEngine::new(root)),
        sre: Arc::new(SessionReviewEngine::new(root)),
        doce: Arc::new(DocEngine::new()),
        doctore: Arc::new(curd_core::doctor::DoctorEngine::new(&root_path)),
        he: Arc::new(HistoryEngine::new(root)),
        tx_events,
        global_state: Arc::new(Mutex::new(curd_core::ReplState::new())),
        sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        pending_challenges: Arc::new(Mutex::new(std::collections::HashMap::new())),
        watchdog,
    };

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
    assert_eq!(plan_res["status"], "ok");
    assert!(root.join("42").exists());

    // 6. Verify State Isolation
    let sessions = ctx.sessions.lock().await;
    let entry = sessions.get(session_token).unwrap();
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
    drop(sessions);

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
