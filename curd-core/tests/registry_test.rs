use anyhow::Result;
use curd_core::context::EngineContext;
use serde_json::json;
use tempfile::tempdir;
use std::fs;

#[tokio::test]
async fn test_tool_registry_dispatch() -> Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    
    // 1. Authenticate to get a token
    use ed25519_dalek::{SigningKey, Signer};
    let mut priv_bytes = [0u8; 32];
    priv_bytes[0] = 42; // deterministic for test
    let signing_key = SigningKey::from_bytes(&priv_bytes);
    let verifying_key = signing_key.verifying_key();
    let pubkey_bytes = verifying_key.as_bytes();
    let pubkey_hex: String = pubkey_bytes.iter().map(|b| format!("{:02x}", b)).collect();

    // Create authorized_agents.json to satisfy auth check
    fs::create_dir_all(root.join(".curd"))?;
    let authorized = json!({
        "test_agent": pubkey_hex
    });
    fs::write(root.join(".curd/authorized_agents.json"), authorized.to_string())?;

    let ctx = EngineContext::new(root.to_str().unwrap());

    let open_res = curd_core::context::dispatch_tool("session_open", &json!({"pubkey_hex": pubkey_hex}), &ctx).await;
    let nonce = open_res.get("nonce").expect(&format!("nonce missing: {:?}", open_res)).as_str().unwrap();
    
    let sig = signing_key.sign(nonce.as_bytes());
    let sig_hex: String = sig.to_bytes().iter().map(|b| format!("{:02x}", b)).collect();

    let verify_res = curd_core::context::dispatch_tool("session_verify", &json!({
        "pubkey_hex": pubkey_hex,
        "signature_hex": sig_hex
    }), &ctx).await;
    
    let connection_token = verify_res.get("connection_token").expect(&format!("token missing: {:?}", verify_res)).as_str().unwrap();

    // 2. Verify a simple stateless tool (stamina)
    let res = curd_core::context::dispatch_tool("stamina", &json!({"connection_token": connection_token}), &ctx).await;
    assert!(res.get("budget").is_some(), "Stamina tool should return a budget object. Result: {:?}", res);

    // 3. Verify a stateful tool (read)
    let res = curd_core::context::dispatch_tool("read", &json!({"uris": ["nonexistent.rs"], "connection_token": connection_token}), &ctx).await;
    let results = res.get("results").unwrap().as_array().unwrap();
    assert!(results.is_empty() || results[0].get("error").is_some(), "Reading nonexistent file should return empty or error");

    // 4. Verify tool not found
    let res = curd_core::context::dispatch_tool("invalid_tool_name", &json!({}), &ctx).await;
    assert!(res.get("error").unwrap().as_str().unwrap().contains("Tool not found"));

    Ok(())
}

#[tokio::test]
async fn test_tool_execution_timeout() -> Result<()> {
    let dir = tempdir()?;
    let _ctx = EngineContext::new(dir.path().to_str().unwrap());
    Ok(())
}
