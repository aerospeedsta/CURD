use curd_core::{
    DebugEngine, DiagramEngine, DocEngine, EditEngine, EngineContext, FileEngine, FindEngine,
    GraphEngine, HistoryEngine, LspEngine, PlanEngine, ProfileEngine, ReadEngine, SearchEngine,
    SessionReviewEngine, ShellEngine, Watchdog, WorkspaceEngine,
    mcp::{McpServerMode, handle_tools_call},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::{Mutex, broadcast};
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
        let text = res["result"]["content"][0]["text"]
            .as_str()
            .expect("tools/call should return JSON text content");
        serde_json::from_str::<Value>(text).expect("tool content should be valid json")
    }};
}

fn mk_ctx(root: PathBuf) -> EngineContext {
    let (tx_events, _) = broadcast::channel(1024);
    let session_id = Uuid::new_v4();
    let watchdog = Arc::new(Watchdog::new(root.clone()));

    EngineContext {
        workspace_root: root.clone(),
        session_id, read_only: false,
        se: Arc::new(SearchEngine::new(&root).with_events(tx_events.clone())),
        re: Arc::new(ReadEngine::new(&root)),
        ee: Arc::new(EditEngine::new(&root).with_watchdog(watchdog.clone())),
        ge: Arc::new(GraphEngine::new(&root)),
            ple: Arc::new(crate::PlanEngine::new(&root)),
            she: Arc::new(crate::ShellEngine::new(&root)),
        we: Arc::new(WorkspaceEngine::new(&root)),
        mu: Arc::new(curd_core::MutationEngine::new(&root)),
        fe: Arc::new(FindEngine::new(&root)),
        de: Arc::new(DiagramEngine::new(&root)),
        fie: Arc::new(FileEngine::new(&root)),
        le: Arc::new(LspEngine::new(&root)),
        pe: Arc::new(ProfileEngine::new(&root)),
        dbe: Arc::new(DebugEngine::new(&root)),
        sre: Arc::new(SessionReviewEngine::new(&root)),
        doce: Arc::new(DocEngine::new()),
        doctore: Arc::new(curd_core::doctor::DoctorEngine::new(&root)),
        he: Arc::new(HistoryEngine::new(&root)),
        tx_events,
        global_state: Arc::new(Mutex::new(curd_core::ReplState::new())),
        sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        pending_challenges: Arc::new(Mutex::new(std::collections::HashMap::new())),
        watchdog,
    }
}

#[tokio::test]
async fn delegation_recovery_and_isolation() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    let ctx = mk_ctx(root.clone());
    let plan_id = Uuid::new_v4().to_string();

    let create = call_tool!(
        "delegate",
        json!({"action":"create","plan_id":plan_id,"nodes":["n1","n2"]}),
        ctx
    );
    assert_eq!(create["status"], "ok");

    let seed = call_tool!(
        "frontier",
        json!({"action":"seed","plan_id":plan_id,"uris":["n1","n2"]}),
        ctx
    );
    assert_eq!(seed["status"], "ok");

    let assign = call_tool!(
        "delegate",
        json!({"action":"auto_assign","plan_id":plan_id,"worker":"worker-a","max_claims":1}),
        ctx
    );
    assert_eq!(assign["status"], "ok");
    assert_eq!(assign["claimed_count"], 1);
    let claimed_node = assign["claimed"][0].as_str().unwrap().to_string();

    let hijack = call_tool!(
        "delegate",
        json!({"action":"heartbeat","plan_id":plan_id,"node_id":claimed_node,"worker":"worker-b"}),
        ctx
    );
    assert!(
        hijack["error"]
            .as_str()
            .unwrap()
            .contains("not claimed by worker")
    );

    std::thread::sleep(Duration::from_secs(1));
    let status = call_tool!(
        "delegate",
        json!({"action":"status","plan_id":plan_id,"stale_timeout_secs":0}),
        ctx
    );
    assert_eq!(status["status"], "ok");
    assert!(status["requeued_stale_claims"].as_u64().unwrap() >= 1);

    let recover = call_tool!(
        "delegate",
        json!({"action":"auto_assign","plan_id":plan_id,"worker":"worker-b","max_claims":1}),
        ctx
    );
    assert_eq!(recover["status"], "ok");
    assert_eq!(recover["claimed_count"], 1);

    Ok(())
}

#[tokio::test]
async fn crawl_modes_are_reproducible_on_fixed_snapshot() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();

    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn a() { b(); }\npub fn b() {}\npub fn shell_write() { b(); }\n",
    )?;

    let ctx = mk_ctx(root.clone());
    let roots = json!(["src/lib.rs::a"]);

    let run1 = call_tool!(
        "crawl",
        json!({"mode":"crawl_heal","roots":roots,"depth":2}),
        ctx
    );
    let run2 = call_tool!(
        "crawl",
        json!({"mode":"crawl_heal","roots":["src/lib.rs::a"],"depth":2}),
        ctx
    );

    assert_eq!(run1["status"], "ok");
    assert_eq!(run1["deterministic_dry_run"], true);
    assert_eq!(run1["ranked_candidates"], run2["ranked_candidates"]);

    let audit = call_tool!(
        "crawl",
        json!({"mode":"crawl_audit","roots":["src/lib.rs::a"],"depth":2}),
        ctx
    );
    let prune = call_tool!(
        "crawl",
        json!({"mode":"crawl_prune","roots":["src/lib.rs::a"],"depth":2}),
        ctx
    );
    assert_eq!(audit["status"], "ok");
    assert_eq!(prune["status"], "ok");

    let with_gists = call_tool!(
        "crawl",
        json!({
            "mode":"crawl_heal",
            "roots":["src/lib.rs::a"],
            "depth":2,
            "include_contract_gists": true,
            "contract_top_k": 2
        }),
        ctx
    );
    assert_eq!(with_gists["status"], "ok");
    assert_eq!(with_gists["contract_gists"]["enabled"], true);
    if let Some(arr) = with_gists["ranked_candidates"].as_array()
        && !arr.is_empty()
    {
        let first = &arr[0];
        assert!(first.get("contract_gist_1line").is_some());
    }

    Ok(())
}

#[tokio::test]
async fn contract_tool_returns_structured_gist() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )?;
    let ctx = mk_ctx(root.clone());

    let out = call_tool!("contract", json!({"uri":"src/lib.rs::add"}), ctx);
    assert_eq!(out["status"], "ok");
    assert_eq!(out["name"], "add");
    assert!(out["contract"]["inputs"].as_array().unwrap().len() >= 2);
    assert!(
        out["contract"]["gist_1line"]
            .as_str()
            .unwrap()
            .contains("add")
    );
    Ok(())
}

#[tokio::test]
async fn proposal_lifecycle_local_only() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn run() {}\n")?;
    let ctx = mk_ctx(root.clone());

    let opened = call_tool!(
        "proposal",
        json!({
            "action":"open",
            "title":"Local-only change proposal",
            "summary":"No git PR required",
            "simulate":{"status":"ok"},
            "crawl":{"status":"ok"}
        }),
        ctx
    );
    assert_eq!(opened["status"], "ok");
    let proposal_id = opened["proposal"]["id"].as_str().unwrap().to_string();
    assert_eq!(opened["proposal"]["status"], "open");

    let status = call_tool!("proposal", json!({"action":"status","id":proposal_id}), ctx);
    assert_eq!(status["status"], "ok");
    assert_eq!(status["proposal"]["title"], "Local-only change proposal");

    let gated = call_tool!(
        "proposal",
        json!({
            "action":"run_gate",
            "id":proposal_id,
            "simulate_args":{"mode":"execute_dsl","nodes":[]},
            "crawl_args":{"mode":"crawl_heal","roots":["src/lib.rs::run"],"depth":1}
        }),
        ctx
    );
    assert_eq!(gated["status"], "ok");

    let approved = call_tool!(
        "proposal",
        json!({"action":"approve","id":proposal_id,"reason":"Checks passed"}),
        ctx
    );
    assert_eq!(approved["status"], "ok");
    assert_eq!(approved["proposal"]["status"], "approved");

    Ok(())
}

#[tokio::test]
async fn search_includes_index_coverage_and_quality_metadata() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() { alpha(); }\n",
    )?;
    let ctx = mk_ctx(root.clone());

    let out = call_tool!(
        "search",
        json!({"mode":"symbol","query":"alpha","limit":10}),
        ctx
    );
    assert_eq!(out["status"], "ok");
    assert!(out.get("index_stats").is_some());
    assert!(out.get("index_coverage").is_some());
    assert!(out.get("index_quality").is_some());

    let cov = out["index_coverage"]
        .as_object()
        .expect("index_coverage object");
    assert!(cov.contains_key("state"));
    assert!(cov.contains_key("coverage_ratio"));

    let qual = out["index_quality"]
        .as_object()
        .expect("index_quality object");
    assert!(qual.contains_key("status"));
    assert!(qual.contains_key("warnings"));

    Ok(())
}

#[test]
fn indexing_is_deterministic_across_chunk_sizes() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn a() { b(); }
pub fn b() {}
pub fn c() { a(); b(); }
"#,
    )?;
    std::fs::write(
        root.join("src/mod.rs"),
        r#"
pub fn x() {}
pub fn y() { x(); }
"#,
    )?;

    use curd_core::CurdConfig;
    let mut cfg = CurdConfig::default();
    cfg.index.mode = Some("full".to_string());
    cfg.index.chunk_size = Some(4096);
    
    let se_default = SearchEngine::new(&root).with_config(cfg.clone());
    let syms_default = se_default.search("", None)?;
    let fp_default: BTreeSet<String> = syms_default
        .iter()
        .map(|s| format!("{}|{}", s.id, s.semantic_hash))
        .collect();

    // Force tiny chunks and rebuild.
    se_default.invalidate_index();
    cfg.index.chunk_size = Some(1);
    
    let se_small = SearchEngine::new(&root).with_config(cfg);
    let syms_small = se_small.search("", None)?;
    let fp_small: BTreeSet<String> = syms_small
        .iter()
        .map(|s| format!("{}|{}", s.id, s.semantic_hash))
        .collect();

    assert_eq!(fp_default, fp_small);
    Ok(())
}

#[test]
fn indexing_is_deterministic_across_execution_models() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn a() { b(); }
pub fn b() {}
pub fn c() { a(); b(); }
"#,
    )?;
    std::fs::write(
        root.join("src/mod.rs"),
        r#"
pub fn x() {}
pub fn y() { x(); }
"#,
    )?;

    use curd_core::CurdConfig;
    let mut cfg = CurdConfig::default();
    cfg.index.mode = Some("full".to_string());
    cfg.index.execution = Some("multithreaded".to_string());
    
    let se_mt = SearchEngine::new(&root).with_config(cfg.clone());
    let syms_mt = se_mt.search("", None)?;
    let fp_mt: BTreeSet<String> = syms_mt
        .iter()
        .map(|s| format!("{}|{}", s.id, s.semantic_hash))
        .collect();

    se_mt.invalidate_index();
    cfg.index.execution = Some("multiprocess".to_string());
    
    let se_mp = SearchEngine::new(&root).with_config(cfg);
    let syms_mp = se_mp.search("", None)?;
    let fp_mp: BTreeSet<String> = syms_mp
        .iter()
        .map(|s| format!("{}|{}", s.id, s.semantic_hash))
        .collect();

    assert_eq!(fp_mt, fp_mp);
    let stats = se_mp.last_index_stats().expect("index stats");
    assert_eq!(stats.execution_model, "multiprocess");
    Ok(())
}

#[test]
fn parser_backend_accounting_is_coherent() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn alpha() {}
pub fn beta() { alpha(); }
"#,
    )?;

    use curd_core::CurdConfig;
    let mut cfg = CurdConfig::default();
    cfg.index.mode = Some("full".to_string());
    cfg.index.execution = Some("multithreaded".to_string());
    cfg.index.parser_backend = Some("native".to_string());
    
    let se = SearchEngine::new(&root).with_config(cfg);
    let _ = se.search("", None)?;
    let stats = se.last_index_stats().expect("index stats");

    assert_eq!(stats.parser_backend, "native");
    assert_eq!(
        stats.native_files + stats.wasm_files,
        stats.cache_misses,
        "effective backend file counts should match parsed miss count"
    );
    let expected_effective = if stats.native_files > 0 && stats.wasm_files > 0 {
        "mixed"
    } else if stats.native_files > 0 {
        "native"
    } else if stats.wasm_files > 0 {
        "wasm"
    } else {
        "none"
    };
    assert_eq!(stats.parser_backend_effective, expected_effective);
    if stats.parser_backend_effective == "wasm" {
        assert_eq!(stats.native_fallbacks, stats.wasm_files);
    }
    Ok(())
}

#[tokio::test]
async fn workspace_commit_requires_approved_proposal_unless_bypassed() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn run() {}\n")?;
    let ctx = mk_ctx(root.clone());

    let blocked_missing = call_tool!("workspace", json!({"action":"commit"}), ctx);
    assert_eq!(blocked_missing["error"]["code"].as_i64().unwrap(), -32012);

    let opened = call_tool!(
        "proposal",
        json!({"action":"open","title":"gate-test"}),
        ctx
    );
    let pid = opened["proposal"]["id"].as_str().unwrap().to_string();

    let blocked_unapproved = call_tool!(
        "workspace",
        json!({"action":"commit","proposal_id":pid}),
        ctx
    );
    assert_eq!(
        blocked_unapproved["error"]["code"].as_i64().unwrap(),
        -32013
    );

    let _ = call_tool!(
        "proposal",
        json!({
            "action":"run_gate",
            "id":pid,
            "simulate_args":{"mode":"execute_dsl","nodes":[]},
            "crawl_args":{"mode":"crawl_heal","roots":["src/lib.rs::run"],"depth":1}
        }),
        ctx
    );
    let _ = call_tool!(
        "proposal",
        json!({"action":"approve","id":pid,"reason":"ready"}),
        ctx
    );

    let _ = call_tool!("workspace", json!({"action":"begin"}), ctx);
    let bypassed = call_tool!(
        "workspace",
        json!({"action":"commit","allow_unapproved":true}),
        ctx
    );
    assert_eq!(bypassed["status"], "ok");
    assert!(bypassed["provenance_path"].as_str().is_some());

    Ok(())
}

#[tokio::test]
async fn proposal_approve_requires_artifact_complete_gate() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn run() {}\n")?;
    let ctx = mk_ctx(root.clone());

    let opened = call_tool!(
        "proposal",
        json!({"action":"open","title":"artifact-gate"}),
        ctx
    );
    let pid = opened["proposal"]["id"].as_str().unwrap().to_string();
    let blocked = call_tool!(
        "proposal",
        json!({"action":"approve","id":pid,"reason":"try approve too early"}),
        ctx
    );
    assert!(
        blocked["error"]
            .as_str()
            .unwrap()
            .contains("simulate.status == ok")
    );

    let gated = call_tool!(
        "proposal",
        json!({
            "action":"run_gate",
            "id":pid,
            "simulate_args":{"mode":"execute_dsl","nodes":[]},
            "crawl_args":{"mode":"crawl_heal","roots":["src/lib.rs::run"],"depth":1}
        }),
        ctx
    );
    assert_eq!(gated["status"], "ok");
    assert_eq!(gated["gate"]["ready_for_approval"], true);

    let approved = call_tool!(
        "proposal",
        json!({"action":"approve","id":pid,"reason":"all artifacts ok"}),
        ctx
    );
    assert_eq!(approved["status"], "ok");
    assert_eq!(approved["proposal"]["status"], "approved");

    Ok(())
}

#[tokio::test]
async fn proposal_approve_blocks_on_snapshot_drift() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn run() {}\n")?;
    let ctx = mk_ctx(root.clone());

    let opened = call_tool!(
        "proposal",
        json!({"action":"open","title":"snapshot-drift"}),
        ctx
    );
    let pid = opened["proposal"]["id"].as_str().unwrap().to_string();
    let _ = call_tool!(
        "proposal",
        json!({
            "action":"run_gate",
            "id":pid,
            "simulate_args":{"mode":"execute_dsl","nodes":[]},
            "crawl_args":{"mode":"crawl_heal","roots":["src/lib.rs::run"],"depth":1}
        }),
        ctx
    );
    std::fs::write(root.join("src/lib.rs"), "pub fn run() { let _x = 1; }\n")?;
    let out = call_tool!(
        "proposal",
        json!({"action":"approve","id":pid,"reason":"should fail stale"}),
        ctx
    );
    assert!(out["error"].as_str().unwrap().contains("snapshot drift"));
    Ok(())
}

#[tokio::test]
async fn proposal_run_gate_requires_non_empty_crawl_roots() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    let ctx = mk_ctx(root.clone());

    let opened = call_tool!(
        "proposal",
        json!({"action":"open","title":"roots-required"}),
        ctx
    );
    let pid = opened["proposal"]["id"].as_str().unwrap().to_string();
    let out = call_tool!(
        "proposal",
        json!({"action":"run_gate","id":pid,"simulate_args":{"mode":"execute_dsl","nodes":[]}}),
        ctx
    );
    assert!(out["error"].as_str().unwrap().contains("crawl_args.roots"));
    Ok(())
}

#[tokio::test]
async fn commit_provenance_paths_are_unique() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    let ctx = mk_ctx(root.clone());

    let _ = call_tool!("workspace", json!({"action":"begin"}), ctx);
    let c1 = call_tool!(
        "workspace",
        json!({"action":"commit","allow_unapproved":true}),
        ctx
    );
    assert_eq!(c1["status"], "ok");
    let p1 = c1["provenance_path"].as_str().unwrap().to_string();

    let _ = call_tool!("workspace", json!({"action":"begin"}), ctx);
    let c2 = call_tool!(
        "workspace",
        json!({"action":"commit","allow_unapproved":true}),
        ctx
    );
    assert_eq!(c2["status"], "ok");
    let p2 = c2["provenance_path"].as_str().unwrap().to_string();

    assert_ne!(p1, p2);
    Ok(())
}

#[tokio::test]
async fn workspace_commit_blocks_stale_approved_proposal() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn run() {}\n")?;
    let ctx = mk_ctx(root.clone());

    let opened = call_tool!(
        "proposal",
        json!({"action":"open","title":"stale-approved"}),
        ctx
    );
    let pid = opened["proposal"]["id"].as_str().unwrap().to_string();
    let _ = call_tool!(
        "proposal",
        json!({
            "action":"run_gate",
            "id":pid,
            "simulate_args":{"mode":"execute_dsl","nodes":[]},
            "crawl_args":{"mode":"crawl_heal","roots":["src/lib.rs::run"],"depth":1}
        }),
        ctx
    );
    let _ = call_tool!(
        "proposal",
        json!({"action":"approve","id":pid,"reason":"approved on current snapshot"}),
        ctx
    );

    std::fs::write(root.join("src/lib.rs"), "pub fn run() { let _x = 2; }\n")?;
    let out = call_tool!(
        "workspace",
        json!({"action":"commit","proposal_id":pid}),
        ctx
    );
    assert_eq!(out["error"]["code"].as_i64().unwrap(), -32015);
    Ok(())
}
