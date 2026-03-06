use crate::ReplState;
use anyhow::{Context, Result};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Helper to encode bytes to hex string
fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Helper to decode hex string to bytes
fn decode_hex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        anyhow::bail!("Invalid hex string length");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| anyhow::anyhow!(e)))
        .collect()
}

/// Manages cryptographic identities and handles the freezing/thawing of agent session state.
pub struct IdentityManager {
    global_dir: PathBuf,
}

impl IdentityManager {
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());

        let global_dir = home.join(".curd").join("identities");
        fs::create_dir_all(&global_dir)?;

        Ok(Self { global_dir })
    }

    /// Generates a new Ed25519 keypair and saves the private key to the global keystore.
    /// Returns (Agent_Name, Private_Key_Hex, Public_Key_Hex).
    pub fn generate_keypair(&self, name: &str) -> Result<(String, String, String)> {
        let mut secret_bytes = [0u8; 32];
        getrandom::fill(&mut secret_bytes)
            .map_err(|e| anyhow::anyhow!("getrandom error: {}", e))?;
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();

        let priv_hex = encode_hex(&signing_key.to_bytes());
        let pub_hex = encode_hex(verifying_key.as_bytes());

        // Save private key securely in ~/.curd/identities/
        let key_path = self.global_dir.join(format!("{}.key", name));

        fs::write(&key_path, &priv_hex)?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&key_path, perms)?;
        }

        Ok((name.to_string(), priv_hex, pub_hex))
    }

    /// Verifies that a given signature is valid for the payload using the provided public key hex.
    pub fn verify_signature(pubkey_hex: &str, message: &[u8], signature_hex: &str) -> Result<bool> {
        let pub_bytes = decode_hex(pubkey_hex).context("Invalid public key hex")?;
        let sig_bytes = decode_hex(signature_hex).context("Invalid signature hex")?;

        if pub_bytes.len() != 32 || sig_bytes.len() != 64 {
            return Ok(false);
        }

        let pub_array: [u8; 32] = pub_bytes.as_slice().try_into()
            .map_err(|_| anyhow::anyhow!("Invalid public key length"))?;
        let verifying_key = VerifyingKey::from_bytes(&pub_array)
            .map_err(|e| anyhow::anyhow!("Invalid public key bytes: {}", e))?;

        let sig_array: [u8; 64] = sig_bytes.as_slice().try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature length"))?;
        let signature = Signature::from_bytes(&sig_array);

        Ok(verifying_key.verify(message, &signature).is_ok())
    }

    /// Freezes a ReplState to disk for a specific agent identity (tied to pubkey hash).
    pub fn freeze_session(
        workspace_root: &Path,
        pubkey_hex: &str,
        state: &ReplState,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(pubkey_hex.as_bytes());
        let hash = encode_hex(&hasher.finalize());

        let session_dir = workspace_root.join(".curd").join("sessions").join(&hash);
        fs::create_dir_all(&session_dir)?;

        let state_path = session_dir.join("state.json");
        let serialized = serde_json::to_string_pretty(&state.variables)?;
        fs::write(state_path, serialized)?;
        Ok(())
    }

    /// Thaws a ReplState from disk for a specific agent identity.
    pub fn thaw_session(workspace_root: &Path, pubkey_hex: &str) -> Option<ReplState> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(pubkey_hex.as_bytes());
        let hash = encode_hex(&hasher.finalize());

        let state_path = workspace_root
            .join(".curd")
            .join("sessions")
            .join(&hash)
            .join("state.json");

        if state_path.exists()
            && let Ok(content) = fs::read_to_string(state_path)
                && let Ok(variables) =
                    serde_json::from_str::<HashMap<String, serde_json::Value>>(&content)
                {
                    return Some(ReplState::from_variables(variables));
                }
        None
    }
}
