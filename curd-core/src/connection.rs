use crate::ReplState;
use crate::context::{ConnectionBudget, ConnectionEntry, EngineContext, PendingChallenge};
use crate::plan::{SystemEvent, now_secs};
use serde_json::{Value, json};

pub async fn handle_connection_open(params: &Value, ctx: &EngineContext) -> Value {
    let pubkey_hex = params
        .get("pubkey_hex")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if pubkey_hex.is_empty() {
        return json!({"error": "pubkey_hex is required"});
    }

    let mut nonce = [0u8; 32];
    if let Err(e) = getrandom::fill(&mut nonce) {
        return json!({"error": format!("Failed to generate nonce: {}", e)});
    }
    let nonce_hex: String = nonce.iter().map(|b| format!("{:02x}", b)).collect();

    let mut pending = ctx.pending_challenges.lock().await;
    pending.insert(
        pubkey_hex.to_string(),
        PendingChallenge {
            nonce_hex: nonce_hex.clone(),
            created_at_secs: now_secs(),
        },
    );

    ctx.he.log(
        Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
        ctx.collaboration_id,
        Some(pubkey_hex.to_string()),
        None,
        "connection_open",
        params.clone(),
        json!({"status": "ok"}),
        None,
        None,
        true,
        None,
        None,
    );

    json!({
        "status": "ok",
        "nonce": nonce_hex
    })
}

pub async fn handle_connection_verify(params: &Value, ctx: &EngineContext) -> Value {
    let pubkey_hex = params
        .get("pubkey_hex")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let signature_hex = params
        .get("signature_hex")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if pubkey_hex.is_empty() || signature_hex.is_empty() {
        return json!({"error": "pubkey_hex and signature_hex are required"});
    }

    let challenge = {
        let mut pending = ctx.pending_challenges.lock().await;
        match pending.remove(pubkey_hex) {
            Some(n) => n,
            None => return json!({"error": "No pending challenge found for this pubkey"}),
        }
    };
    if now_secs().saturating_sub(challenge.created_at_secs)
        > ctx.config.collaboration.session_challenge_ttl_secs
    {
        let out = json!({"error": "Challenge expired"});
        ctx.he.log(
            Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
            ctx.collaboration_id,
            Some(pubkey_hex.to_string()),
            None,
            "connection_verify",
            params.clone(),
            out.clone(),
            None,
            None,
            false,
            Some("Challenge expired".to_string()),
            None,
        );
        return out;
    }

    match crate::auth::IdentityManager::verify_signature(
        pubkey_hex,
        challenge.nonce_hex.as_bytes(),
        signature_hex,
    ) {
        Ok(true) => {
            let auth_file = ctx
                .workspace_root
                .join(".curd")
                .join("authorized_agents.json");
            let (agent_id, is_authorized) = if auth_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&auth_file)
                    && let Ok(authorized) =
                        serde_json::from_str::<std::collections::HashMap<String, String>>(&content)
                {
                    let aid = authorized
                        .iter()
                        .find(|(_, v)| *v == pubkey_hex)
                        .map(|(k, _)| k.clone());
                    match aid {
                        Some(id) => (id, true),
                        None => ("unknown".to_string(), false),
                    }
                } else {
                    ("unknown".to_string(), false)
                }
            } else if ctx.config.collaboration.require_authorized_agents_file {
                ("unknown".to_string(), false)
            } else {
                ("anonymous".to_string(), true)
            };

            if !is_authorized {
                let out = json!({"error": "Unauthorized: Agent public key not found in authorized_agents.json"});
                ctx.he.log(
                    Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                    ctx.collaboration_id,
                    Some(pubkey_hex.to_string()),
                    None,
                    "connection_verify",
                    params.clone(),
                    out.clone(),
                    None,
                    None,
                    false,
                    Some("Unauthorized public key".to_string()),
                    None,
                );
                return out;
            }

            let auth_mgr = match crate::auth::IdentityManager::new() {
                Ok(m) => m,
                Err(e) => {
                    return json!({"error": format!("Failed to initialize IdentityManager: {}", e)});
                }
            };

            let connection_token = match auth_mgr.create_connection_token(&agent_id, pubkey_hex) {
                Ok(t) => t,
                Err(e) => {
                    return json!({"error": format!("Failed to create connection token: {}", e)});
                }
            };
            let (state, restored) = if let Some(frozen_state) =
                crate::auth::IdentityManager::thaw_session(&ctx.workspace_root, pubkey_hex)
            {
                (frozen_state, true)
            } else {
                (ReplState::new(), false)
            };
            let entry = ConnectionEntry {
                agent_id: agent_id.clone(),
                pubkey_hex: pubkey_hex.to_string(),
                state,
                budget: ConnectionBudget::default(),
                last_touched_secs: now_secs(),
            };
            let mut connections = ctx.connections.lock().await;
            connections.insert(connection_token.clone(), entry);
            drop(connections);
            let _ = ctx.tx_events.send(SystemEvent::ConnectionAuthenticated {
                agent_id: agent_id.clone(),
                pubkey_hex: pubkey_hex.to_string(),
                restored_state: restored,
            });

            let out = json!({
                "status": "authenticated",
                "connection_token": connection_token,
                "session_token": connection_token,
                "agent_id": agent_id,
                "restored_state": restored,
                "config_hash": ctx.config.compute_hash()
            });
            ctx.he.log(
                Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                ctx.collaboration_id,
                Some(pubkey_hex.to_string()),
                None,
                "connection_verify",
                params.clone(),
                out.clone(),
                None,
                None,
                true,
                None,
                None,
            );
            out
        }
        Ok(false) | Err(_) => {
            let out = json!({"error": "Invalid signature. Authentication failed."});
            ctx.he.log(
                Some(ctx.event_seq.load(std::sync::atomic::Ordering::SeqCst)),
                ctx.collaboration_id,
                Some(pubkey_hex.to_string()),
                None,
                "connection_verify",
                params.clone(),
                out.clone(),
                None,
                None,
                false,
                Some("Invalid signature".to_string()),
                None,
            );
            out
        }
    }
}
