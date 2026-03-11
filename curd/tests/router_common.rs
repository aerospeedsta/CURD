use curd_core::EngineContext;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

pub async fn open_test_connection(
    ctx: &EngineContext,
    root: &std::path::Path,
) -> anyhow::Result<String> {
    std::fs::create_dir_all(root.join(".curd/identities"))?;
    std::fs::write(
        root.join("curd.toml"),
        "[edit]\nenforce_transactional = false\n",
    )?;

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

    let open =
        curd_core::connection::handle_connection_open(&json!({"pubkey_hex": pub_hex}), ctx).await;
    let nonce_hex = open["nonce"].as_str().unwrap();
    let signature = signing_key.sign(nonce_hex.as_bytes());
    let sig_hex = signature
        .to_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    let verify = curd_core::connection::handle_connection_verify(
        &json!({"pubkey_hex": pub_hex, "signature_hex": sig_hex}),
        ctx,
    )
    .await;
    Ok(verify["session_token"]
        .as_str()
        .expect("session token")
        .to_string())
}
