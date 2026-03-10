use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Tool,
    Language,
}

impl PluginKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Tool => "curdt",
            Self::Language => "curdl",
        }
    }

    pub fn directory_name(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Language => "lang",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedPluginKey {
    pub key_id: String,
    pub pubkey_hex: String,
    pub label: Option<String>,
    pub added_at_secs: u64,
    pub fingerprint_sha256: String,
    #[serde(default)]
    pub allowed_kinds: Vec<PluginKind>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustedPluginKeySet {
    #[serde(default)]
    pub keys: Vec<TrustedPluginKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFileManifest {
    pub path: String,
    pub sha256: String,
    pub size: usize,
    #[serde(default)]
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDocParameter {
    pub name: String,
    pub kind: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDocExample {
    pub label: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPluginSpec {
    pub tool_name: String,
    pub executable_path: String,
    #[serde(default)]
    pub default_args: Vec<String>,
    #[serde(default = "default_stdio_protocol")]
    pub protocol: String,
    pub agent_usage: Option<String>,
    pub review_guidance: Option<String>,
    pub downstream_impact: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Vec<ToolDocParameter>,
    #[serde(default)]
    pub examples: Vec<ToolDocExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguagePluginSpec {
    pub language_id: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    pub grammar_library_path: String,
    pub grammar_symbol: String,
    pub query_path: Option<String>,
    pub build_system: Option<String>,
    pub lsp_adapter: Option<String>,
    pub debug_adapter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub package_id: String,
    pub version: String,
    pub kind: PluginKind,
    pub signer_pubkey_hex: String,
    pub description: Option<String>,
    #[serde(default)]
    pub files: Vec<PluginFileManifest>,
    pub tool: Option<ToolPluginSpec>,
    pub language: Option<LanguagePluginSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPayloadFile {
    pub path: String,
    pub content_b64: String,
    #[serde(default)]
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginArchive {
    pub manifest: PluginManifest,
    pub payload_files: Vec<PluginPayloadFile>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginRecord {
    pub package_id: String,
    pub version: String,
    pub kind: PluginKind,
    pub signer_pubkey_hex: String,
    pub install_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub tool: Option<ToolPluginSpec>,
    pub language: Option<LanguagePluginSpec>,
}

#[derive(Debug, Clone)]
pub struct VerifiedPluginArchive {
    pub archive: PluginArchive,
}

fn default_enabled() -> bool { true }
fn default_stdio_protocol() -> String { "json_stdio_v1".to_string() }

pub fn plugin_install_root(workspace_root: &Path, config: &crate::config::PluginConfig) -> PathBuf {
    workspace_root.join(&config.install_root)
}

pub fn trusted_keys_path(workspace_root: &Path, config: &crate::config::PluginConfig) -> PathBuf {
    workspace_root.join(&config.trusted_keys_file)
}

pub fn load_trusted_keys(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
) -> Result<TrustedPluginKeySet> {
    let path = trusted_keys_path(workspace_root, config);
    if !path.exists() {
        return Ok(TrustedPluginKeySet::default());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read trusted plugin keys: {}", path.display()))?;
    let keys: TrustedPluginKeySet =
        serde_json::from_str(&content).context("Failed to parse trusted plugin keys")?;
    validate_trusted_keys(&keys)?;
    Ok(keys)
}

pub fn store_trusted_keys(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
    keys: &TrustedPluginKeySet,
) -> Result<()> {
    validate_trusted_keys(keys)?;
    let path = trusted_keys_path(workspace_root, config);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(keys)?)?;
    Ok(())
}

pub fn create_trusted_plugin_key(
    key_id: &str,
    pubkey_hex: &str,
    label: Option<String>,
    allowed_kinds: Vec<PluginKind>,
    enabled: bool,
) -> Result<TrustedPluginKey> {
    validate_identifier(key_id, "key_id")?;
    crate::auth::IdentityManager::validate_public_key_hex(pubkey_hex)?;
    Ok(TrustedPluginKey {
        key_id: key_id.to_string(),
        pubkey_hex: pubkey_hex.to_string(),
        label,
        added_at_secs: crate::plan::now_secs(),
        fingerprint_sha256: crate::auth::IdentityManager::public_key_fingerprint(pubkey_hex)?,
        allowed_kinds,
        enabled,
    })
}

pub fn validate_trusted_keys(keys: &TrustedPluginKeySet) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for key in &keys.keys {
        validate_identifier(&key.key_id, "key_id")?;
        crate::auth::IdentityManager::validate_public_key_hex(&key.pubkey_hex)?;
        if key.fingerprint_sha256
            != crate::auth::IdentityManager::public_key_fingerprint(&key.pubkey_hex)?
        {
            anyhow::bail!("trusted plugin key '{}' has invalid fingerprint", key.key_id);
        }
        if !seen.insert(key.key_id.clone()) {
            anyhow::bail!("duplicate trusted plugin key id '{}'", key.key_id);
        }
    }
    Ok(())
}

pub fn load_archive_file(path: &Path) -> Result<PluginArchive> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read package {}", path.display()))?;
    serde_json::from_slice(&bytes).context("Failed to parse signed plugin archive")
}

pub fn verify_archive_file(
    path: &Path,
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
) -> Result<VerifiedPluginArchive> {
    let archive = load_archive_file(path)?;
    verify_archive(&archive, path, workspace_root, config)
}

pub fn verify_archive(
    archive: &PluginArchive,
    archive_path: &Path,
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
) -> Result<VerifiedPluginArchive> {
    let ext = archive_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if ext != archive.manifest.kind.extension() {
        anyhow::bail!(
            "Package extension .{} does not match manifest kind {:?}",
            ext,
            archive.manifest.kind
        );
    }
    validate_manifest(&archive.manifest, config)?;
    validate_payload_hashes(archive)?;
    verify_signature_policy(archive, workspace_root, config)?;
    Ok(VerifiedPluginArchive {
        archive: archive.clone(),
    })
}

fn validate_manifest(manifest: &PluginManifest, config: &crate::config::PluginConfig) -> Result<()> {
    if manifest.schema_version == 0 {
        anyhow::bail!("schema_version must be > 0");
    }
    validate_identifier(&manifest.package_id, "package_id")?;
    if manifest.version.trim().is_empty() {
        anyhow::bail!("version must not be empty");
    }
    match manifest.kind {
        PluginKind::Tool => {
            if manifest.tool.is_none() || manifest.language.is_some() {
                anyhow::bail!("tool plugin manifest must include tool spec only");
            }
            let spec = manifest.tool.as_ref().unwrap();
            validate_identifier(&spec.tool_name, "tool_name")?;
            validate_relative_file_path(&spec.executable_path)?;
            if spec.protocol != "json_stdio_v1" {
                anyhow::bail!("unsupported tool plugin protocol: {}", spec.protocol);
            }
            if config.tool_runtime != "sidecar_stdio" {
                anyhow::bail!("tool plugin runtime policy forbids installed tool execution");
            }
        }
        PluginKind::Language => {
            if manifest.language.is_none() || manifest.tool.is_some() {
                anyhow::bail!("language plugin manifest must include language spec only");
            }
            let spec = manifest.language.as_ref().unwrap();
            validate_identifier(&spec.language_id, "language_id")?;
            validate_relative_file_path(&spec.grammar_library_path)?;
            if let Some(path) = &spec.query_path {
                validate_relative_file_path(path)?;
            }
            if !config.allow_native_language_dylibs {
                anyhow::bail!("native language dylibs are disabled by plugin policy");
            }
        }
    }
    for file in &manifest.files {
        validate_relative_file_path(&file.path)?;
        if file.size == 0 {
            anyhow::bail!("manifest file '{}' has zero size", file.path);
        }
    }
    Ok(())
}

fn verify_signature_policy(
    archive: &PluginArchive,
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
) -> Result<()> {
    let trusted = load_trusted_keys(workspace_root, config)?;
    let key = trusted.keys.iter().find(|key| {
        key.enabled
            && key.pubkey_hex == archive.manifest.signer_pubkey_hex
            && (key.allowed_kinds.is_empty() || key.allowed_kinds.contains(&archive.manifest.kind))
    });

    let signing_payload = serde_json::to_vec(&archive.manifest)?;
    if config.require_signatures {
        let signature_hex = archive.signature_hex.as_deref().ok_or_else(|| {
            anyhow::anyhow!("signed plugin archive is missing signature_hex")
        })?;
        if key.is_none() && !config.allow_unsigned_dev_plugins {
            anyhow::bail!("signer is not present in trusted plugin key set");
        }
        if !crate::auth::IdentityManager::verify_signature(
            &archive.manifest.signer_pubkey_hex,
            &signing_payload,
            signature_hex,
        )? {
            anyhow::bail!("plugin archive signature verification failed");
        }
    } else if key.is_none() && !config.allow_unsigned_dev_plugins {
        anyhow::bail!("unsigned or untrusted plugin archive rejected by policy");
    }
    Ok(())
}

fn validate_payload_hashes(archive: &PluginArchive) -> Result<()> {
    let mut by_path: HashMap<&str, &PluginPayloadFile> = HashMap::new();
    for payload in &archive.payload_files {
        validate_relative_file_path(&payload.path)?;
        by_path.insert(payload.path.as_str(), payload);
    }

    for file in &archive.manifest.files {
        let payload = by_path
            .get(file.path.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing payload for manifest file '{}'", file.path))?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload.content_b64.as_bytes())
            .with_context(|| format!("Failed to decode base64 payload for {}", payload.path))?;
        let actual_hash = sha256_hex(&decoded);
        if actual_hash != file.sha256 {
            anyhow::bail!("sha256 mismatch for payload '{}'", payload.path);
        }
        if decoded.len() != file.size {
            anyhow::bail!("size mismatch for payload '{}'", payload.path);
        }
    }
    Ok(())
}

pub fn install_verified_archive(
    verified: &VerifiedPluginArchive,
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
) -> Result<InstalledPluginRecord> {
    let manifest = &verified.archive.manifest;
    let install_dir = plugin_install_root(workspace_root, config)
        .join(manifest.kind.directory_name())
        .join(&manifest.package_id);
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)?;
    }
    fs::create_dir_all(&install_dir)?;

    for payload in &verified.archive.payload_files {
        let out = install_dir.join(&payload.path);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload.content_b64.as_bytes())
            .with_context(|| format!("Failed to decode payload '{}'", payload.path))?;
        fs::write(&out, decoded)?;
        #[cfg(unix)]
        if payload.executable {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&out)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&out, perms)?;
        }
    }

    let manifest_path = install_dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;

    let record = InstalledPluginRecord {
        package_id: manifest.package_id.clone(),
        version: manifest.version.clone(),
        kind: manifest.kind,
        signer_pubkey_hex: manifest.signer_pubkey_hex.clone(),
        install_dir: install_dir.clone(),
        manifest_path,
        tool: manifest.tool.clone(),
        language: manifest.language.clone(),
    };
    fs::write(
        install_dir.join("installed.json"),
        serde_json::to_vec_pretty(&record)?,
    )?;
    Ok(record)
}

pub fn load_installed_plugins(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
    kind: PluginKind,
) -> Result<Vec<InstalledPluginRecord>> {
    let root = plugin_install_root(workspace_root, config).join(kind.directory_name());
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path().join("installed.json");
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let record: InstalledPluginRecord = serde_json::from_str(&content)?;
        validate_installed_plugin_record(workspace_root, config, kind, &record)?;
        out.push(record);
    }
    out.sort_by(|a, b| a.package_id.cmp(&b.package_id));
    Ok(out)
}

pub fn remove_installed_plugin(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
    kind: PluginKind,
    package_id: &str,
) -> Result<bool> {
    validate_identifier(package_id, "package_id")?;
    let path = plugin_install_root(workspace_root, config)
        .join(kind.directory_name())
        .join(package_id);
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(path)?;
    Ok(true)
}

pub fn is_path_within_installed_plugin_root(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
    kind: PluginKind,
    path: &Path,
) -> bool {
    let root = plugin_install_root(workspace_root, config).join(kind.directory_name());
    let Ok(path) = fs::canonicalize(path) else {
        return false;
    };
    let Ok(root) = fs::canonicalize(root) else {
        return false;
    };
    path.starts_with(root)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_installed_plugin_record(
    workspace_root: &Path,
    config: &crate::config::PluginConfig,
    kind: PluginKind,
    record: &InstalledPluginRecord,
) -> Result<()> {
    validate_identifier(&record.package_id, "package_id")?;
    if record.kind != kind {
        anyhow::bail!(
            "installed plugin '{}' kind mismatch: expected {:?}, found {:?}",
            record.package_id,
            kind,
            record.kind
        );
    }
    if !is_path_within_installed_plugin_root(workspace_root, config, kind, &record.install_dir) {
        anyhow::bail!(
            "installed plugin '{}' escaped the verified install root",
            record.package_id
        );
    }
    match kind {
        PluginKind::Tool => {
            let spec = record
                .tool
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("installed tool plugin '{}' missing tool spec", record.package_id))?;
            validate_identifier(&spec.tool_name, "tool_name")?;
            validate_relative_file_path(&spec.executable_path)?;
            if spec.protocol != "json_stdio_v1" {
                anyhow::bail!(
                    "installed tool plugin '{}' uses unsupported protocol '{}'",
                    record.package_id,
                    spec.protocol
                );
            }
        }
        PluginKind::Language => {
            let spec = record.language.as_ref().ok_or_else(|| {
                anyhow::anyhow!("installed language plugin '{}' missing language spec", record.package_id)
            })?;
            validate_identifier(&spec.language_id, "language_id")?;
            validate_relative_file_path(&spec.grammar_library_path)?;
            if let Some(path) = &spec.query_path {
                validate_relative_file_path(path)?;
            }
        }
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        anyhow::bail!("invalid {} '{}'", field, value);
    }
    Ok(())
}

pub fn validate_relative_file_path(path: &str) -> Result<()> {
    if path.is_empty() {
        anyhow::bail!("empty relative file path");
    }
    let p = Path::new(path);
    if p.is_absolute() {
        anyhow::bail!("absolute plugin file paths are not allowed");
    }
    for component in p.components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            anyhow::bail!("unsafe plugin file path: {}", path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    use tempfile::tempdir;

    fn sign_archive(mut archive: PluginArchive, signer: &SigningKey) -> PluginArchive {
        let payload = serde_json::to_vec(&archive.manifest).unwrap();
        let sig = signer.sign(&payload);
        archive.signature_hex = Some(sig.to_bytes().iter().map(|b| format!("{:02x}", b)).collect());
        archive
    }

    fn sample_lang_archive(signer: &SigningKey, pubkey_hex: String) -> PluginArchive {
        let bytes = b"fake-dylib-bytes";
        let file = PluginPayloadFile {
            path: "lib/tree-sitter-demo.dylib".to_string(),
            content_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
            executable: false,
        };
        sign_archive(
            PluginArchive {
                manifest: PluginManifest {
                    schema_version: 1,
                    package_id: "demo-lang".to_string(),
                    version: "0.1.0".to_string(),
                    kind: PluginKind::Language,
                    signer_pubkey_hex: pubkey_hex,
                    description: None,
                    files: vec![PluginFileManifest {
                        path: file.path.clone(),
                        sha256: sha256_hex(bytes),
                        size: bytes.len(),
                        executable: false,
                    }],
                    tool: None,
                    language: Some(LanguagePluginSpec {
                        language_id: "demo".to_string(),
                        extensions: vec!["demo".to_string()],
                        grammar_library_path: file.path.clone(),
                        grammar_symbol: "tree_sitter_demo".to_string(),
                        query_path: None,
                        build_system: None,
                        lsp_adapter: None,
                        debug_adapter: None,
                    }),
                },
                payload_files: vec![file],
                signature_hex: None,
            },
            signer,
        )
    }

    #[test]
    fn verify_and_install_signed_language_archive() {
        let dir = tempdir().unwrap();
        let mut secret = [7u8; 32];
        secret[0] = 11;
        let signer = SigningKey::from_bytes(&secret);
        let pubkey_hex: String = signer
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let cfg = crate::config::PluginConfig::default();
        store_trusted_keys(
            dir.path(),
            &cfg,
            &TrustedPluginKeySet {
                keys: vec![create_trusted_plugin_key(
                    "test",
                    &pubkey_hex,
                    Some("test".to_string()),
                    vec![PluginKind::Language],
                    true,
                ).unwrap()],
            },
        )
        .unwrap();
        let archive = sample_lang_archive(&signer, pubkey_hex);
        let archive_path = dir.path().join("demo.curdl");
        fs::write(&archive_path, serde_json::to_vec(&archive).unwrap()).unwrap();
        let verified = verify_archive_file(&archive_path, dir.path(), &cfg).unwrap();
        let record = install_verified_archive(&verified, dir.path(), &cfg).unwrap();
        assert_eq!(record.kind, PluginKind::Language);
        assert!(record.install_dir.join("lib/tree-sitter-demo.dylib").exists());
    }

    #[test]
    fn trusted_key_constructor_sets_fingerprint_and_validates_pubkey() {
        let mut secret = [3u8; 32];
        secret[0] = 19;
        let signer = SigningKey::from_bytes(&secret);
        let pubkey_hex: String = signer
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let key = create_trusted_plugin_key(
            "acme",
            &pubkey_hex,
            Some("Acme".to_string()),
            vec![PluginKind::Tool],
            true,
        )
        .unwrap();
        assert_eq!(
            key.fingerprint_sha256,
            crate::auth::IdentityManager::public_key_fingerprint(&pubkey_hex).unwrap()
        );
    }

    #[test]
    fn load_installed_plugins_rejects_tampered_tool_protocol() {
        let dir = tempdir().unwrap();
        let cfg = crate::config::PluginConfig::default();
        let install_dir = plugin_install_root(dir.path(), &cfg).join("tool").join("tampered");
        fs::create_dir_all(&install_dir).unwrap();
        fs::write(
            install_dir.join("installed.json"),
            serde_json::to_vec_pretty(&InstalledPluginRecord {
                package_id: "tampered".to_string(),
                version: "0.1.0".to_string(),
                kind: PluginKind::Tool,
                signer_pubkey_hex: "11".repeat(32),
                install_dir: install_dir.clone(),
                manifest_path: install_dir.join("manifest.json"),
                tool: Some(ToolPluginSpec {
                    tool_name: "tampered_tool".to_string(),
                    executable_path: "bin/tool".to_string(),
                    default_args: Vec::new(),
                    protocol: "custom_stdio_v9".to_string(),
                    agent_usage: None,
                    review_guidance: None,
                    downstream_impact: None,
                    description: None,
                    parameters: Vec::new(),
                    examples: Vec::new(),
                }),
                language: None,
            })
            .unwrap(),
        )
        .unwrap();
        let err = load_installed_plugins(dir.path(), &cfg, PluginKind::Tool).unwrap_err();
        assert!(err.to_string().contains("unsupported protocol"));
    }
}
