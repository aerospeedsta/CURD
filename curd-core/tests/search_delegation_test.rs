use curd_core::{CurdConfig, EngineContext, dispatch_tool};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn search_delegation_respects_local_first_provenance() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();

    // Create a local symbol
    fs::write(root.join("src/main.py"), "def local_func():\n    pass\n").unwrap();

    fs::create_dir_all(root.join(".curd")).unwrap();
    let mut config = CurdConfig::default();
    config.reference.enable_delegation = true;
    config.reference.instances.insert(
        "test_ref".to_string(),
        "http://localhost:12345/nonexistent".to_string(),
    );
    config.index.execution = Some("singlethreaded".to_string());

    fs::write(
        root.join(".curd/settings.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let ctx = EngineContext::new(root.to_str().unwrap());

    // 1. Search for existing local symbol -> provenance should be "local"
    let local_req = json!({
        "query": "local_func",
        "mode": "symbol"
    });
    let local_resp = dispatch_tool("search", &local_req, &ctx).await;
    assert_eq!(local_resp["status"], "ok");
    assert_eq!(local_resp["provenance"], "local");
    let syms = local_resp["symbols"].as_array().unwrap();
    println!("SYMBOLS: {:#?}", syms);
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0]["name"], "local_func");

    // 2. Search for missing symbol -> provenance would be "external", but our mock URL is invalid so it falls back to empty local result
    let missing_req = json!({
        "query": "missing_func",
        "mode": "symbol"
    });
    let missing_resp = dispatch_tool("search", &missing_req, &ctx).await;
    assert_eq!(missing_resp["status"], "ok");
    assert_eq!(missing_resp["provenance"], "local"); // Fails to fetch external, returns empty local
    let syms2 = missing_resp["symbols"].as_array().unwrap();
    assert!(syms2.is_empty());
}
