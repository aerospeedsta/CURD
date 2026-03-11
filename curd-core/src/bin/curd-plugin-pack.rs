use anyhow::{Context, Result};
use base64::Engine;
use curd_core::auth::IdentityManager;
use curd_core::plugin_packages::{PluginArchive, PluginManifest, PluginPayloadFile};
use std::fs;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 9 {
        eprintln!(
            "Usage: curd-plugin-pack --manifest <manifest.json> --payload-root <dir> --out <archive.curdt|archive.curdl> (--private-key-hex <hex> | --private-key-file <path>)"
        );
        std::process::exit(2);
    }

    let mut manifest_path = None::<PathBuf>;
    let mut payload_root = None::<PathBuf>;
    let mut out_path = None::<PathBuf>;
    let mut private_key_hex = None::<String>;
    let mut private_key_file = None::<PathBuf>;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" => {
                i += 1;
                manifest_path = args.get(i).map(PathBuf::from);
            }
            "--payload-root" => {
                i += 1;
                payload_root = args.get(i).map(PathBuf::from);
            }
            "--out" => {
                i += 1;
                out_path = args.get(i).map(PathBuf::from);
            }
            "--private-key-hex" => {
                i += 1;
                private_key_hex = args.get(i).cloned();
            }
            "--private-key-file" => {
                i += 1;
                private_key_file = args.get(i).map(PathBuf::from);
            }
            other => anyhow::bail!("Unknown argument: {}", other),
        }
        i += 1;
    }

    let manifest_path = manifest_path.context("missing --manifest")?;
    let payload_root = payload_root.context("missing --payload-root")?;
    let out_path = out_path.context("missing --out")?;
    let private_key_hex = match (private_key_hex, private_key_file) {
        (Some(hex), None) => hex,
        (None, Some(path)) => fs::read_to_string(path)?.trim().to_string(),
        _ => anyhow::bail!("provide exactly one of --private-key-hex or --private-key-file"),
    };

    let manifest_content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read manifest {}", manifest_path.display()))?;
    let manifest: PluginManifest =
        serde_json::from_str(&manifest_content).context("Failed to parse plugin manifest JSON")?;

    let mut payload_files = Vec::new();
    for file in &manifest.files {
        let abs = payload_root.join(&file.path);
        let bytes = fs::read(&abs)
            .with_context(|| format!("Failed to read payload file {}", abs.display()))?;
        let actual_hash = curd_core::plugin_packages::sha256_hex(&bytes);
        if actual_hash != file.sha256 {
            anyhow::bail!("sha256 mismatch for payload '{}'", file.path);
        }
        if bytes.len() != file.size {
            anyhow::bail!("size mismatch for payload '{}'", file.path);
        }
        payload_files.push(PluginPayloadFile {
            path: file.path.clone(),
            content_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
            executable: file.executable,
        });
    }

    let manifest_json = serde_json::to_vec(&manifest)?;
    let signature_hex = IdentityManager::sign_message_hex(&private_key_hex, &manifest_json)?;
    let archive = PluginArchive {
        manifest,
        payload_files,
        signature_hex: Some(signature_hex),
    };

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, serde_json::to_vec_pretty(&archive)?)?;

    let ext = out_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    println!(
        "{{\"status\":\"ok\",\"archive\":\"{}\",\"kind_extension\":\"{}\"}}",
        out_path.display(),
        ext
    );
    Ok(())
}
