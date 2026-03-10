use curd_core::{CurdConfig, EngineContext, Plan, dispatch_tool};
use curd_core::plan::{PlanNode, ToolOperation};
use serde_json::json;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn configurable_collaboration_and_variant_flow_works() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn value() -> i32 { 1 }\n").unwrap();
    fs::create_dir_all(root.join(".curd")).unwrap();

    let mut config = CurdConfig::default();
    config.collaboration.human_override_ttl_secs = 120;
    config.variants.default_backend = "shadow".to_string();
    fs::write(
        root.join(".curd/settings.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let ctx = EngineContext::new(root.to_str().unwrap());

    let bind = dispatch_tool(
        "bind_participant_role",
        &json!({
            "participant_id": "dev1",
            "display_name": "Developer 1",
            "role": "owner",
            "is_human": true
        }),
        &ctx,
    )
    .await;
    assert_eq!(bind["status"], "ok");

    let claim = dispatch_tool(
        "claim_human_override",
        &json!({
            "participant_id": "dev1",
            "resource_key": "src/lib.rs",
        }),
        &ctx,
    )
    .await;
    assert_eq!(claim["status"], "ok", "{claim}");

    let create_set = dispatch_tool(
        "create_plan_set",
        &json!({
            "participant_id": "dev1",
            "title": "compare approaches",
            "objective": "Evaluate multiple implementation variants",
            "created_by": "dev1"
        }),
        &ctx,
    )
    .await;
    assert_eq!(create_set["status"], "ok");
    let plan_set_id = create_set["plan_set"]["id"].as_str().unwrap().to_string();

    let plan = Plan {
        id: Uuid::new_v4(),
        nodes: vec![],
    };
    let create_variant = dispatch_tool(
        "create_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": plan_set_id,
            "title": "baseline-preserving variant",
            "strategy_summary": "Keep behavior unchanged and compare structure.",
            "created_by": "dev1",
            "plan": serde_json::to_value(plan).unwrap()
        }),
        &ctx,
    )
    .await;
    assert_eq!(create_variant["status"], "ok");
    let variant_id = create_variant["variant"]["id"].as_str().unwrap().to_string();

    let simulate = dispatch_tool(
        "simulate_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": plan_set_id,
            "variant_id": variant_id
        }),
        &ctx,
    )
    .await;
    assert_eq!(simulate["status"], "ok", "{simulate}");
    let workspace_root = simulate["workspace_root"].as_str().unwrap();
    assert!(
        root.join(".curd")
            .join("variants")
            .join(&plan_set_id)
            .join(&variant_id)
            .join("graph.json")
            .exists()
    );
    fs::write(
        format!("{workspace_root}/src/lib.rs"),
        "pub fn value() -> i32 { 1 }\npub fn extra() -> i32 { value() }\n",
    )
    .unwrap();

    let compare = dispatch_tool(
        "compare_plan_variants",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": plan_set_id,
            "variant_ids": [variant_id]
        }),
        &ctx,
    )
    .await;
    assert_eq!(compare["status"], "ok");
    assert_eq!(compare["variants"][0]["summary"]["files_changed"], 1);
    assert!(
        compare["variants"][0]["summary"]["graph_delta"]["added_nodes"]
            .as_array()
            .map(|nodes| !nodes.is_empty())
            .unwrap_or(false),
        "{compare}"
    );
    assert!(
        compare["variants"][0]["summary"]["graph_delta"]["added_edges"]
            .as_array()
            .map(|edges| !edges.is_empty())
            .unwrap_or(false),
        "{compare}"
    );

    let review = dispatch_tool(
        "review_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": plan_set_id,
            "variant_id": variant_id,
            "decision": "approve",
            "summary": "Baseline-safe variant approved."
        }),
        &ctx,
    )
    .await;
    assert_eq!(review["status"], "ok");
    assert_eq!(review["variant"]["status"], "approved");

    let release = dispatch_tool(
        "release_human_override",
        &json!({
            "participant_id": "dev1",
            "resource_key": "src/lib.rs",
        }),
        &ctx,
    )
    .await;
    assert_eq!(release["status"], "ok");
}

#[tokio::test]
async fn variant_plan_payload_respects_size_budget() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".curd")).unwrap();

    let mut config = CurdConfig::default();
    config.variants.max_plan_bytes = 64;
    fs::write(
        root.join(".curd/settings.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let ctx = EngineContext::new(root.to_str().unwrap());
    let bind = dispatch_tool(
        "bind_participant_role",
        &json!({
            "participant_id": "dev1",
            "role": "owner",
            "is_human": true
        }),
        &ctx,
    )
    .await;
    assert_eq!(bind["status"], "ok");

    let create_set = dispatch_tool(
        "create_plan_set",
        &json!({
            "participant_id": "dev1",
            "title": "budgeted",
            "objective": "Enforce plan import budget"
        }),
        &ctx,
    )
    .await;
    assert_eq!(create_set["status"], "ok");
    let plan_set_id = create_set["plan_set"]["id"].as_str().unwrap().to_string();

    let oversized = dispatch_tool(
        "create_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": plan_set_id,
            "title": "too-large",
            "plan": serde_json::to_value(Plan {
                id: Uuid::new_v4(),
                nodes: vec![PlanNode {
                    id: Uuid::new_v4(),
                    op: ToolOperation::McpCall {
                        tool: "search".to_string(),
                        args: json!({ "query": "x".repeat(256) }),
                    },
                    dependencies: vec![],
                    output_limit: 1024,
                    retry_limit: 0,
                }],
            }).unwrap()
        }),
        &ctx,
    )
    .await;
    assert!(oversized.get("error").is_some(), "{oversized}");
}

#[tokio::test]
async fn bootstrap_requires_human_owner_when_configured() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".curd")).unwrap();

    let mut config = CurdConfig::default();
    config.collaboration.bootstrap_owner_human_only = true;
    fs::write(
        root.join(".curd/settings.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let ctx = EngineContext::new(root.to_str().unwrap());
    let denied = dispatch_tool(
        "bind_participant_role",
        &json!({
            "participant_id": "agent1",
            "role": "owner",
            "is_human": false
        }),
        &ctx,
    )
    .await;
    assert!(denied.get("error").is_some(), "{denied}");

    let allowed = dispatch_tool(
        "bind_participant_role",
        &json!({
            "participant_id": "dev1",
            "role": "owner",
            "is_human": true
        }),
        &ctx,
    )
    .await;
    assert_eq!(allowed["status"], "ok");
    assert_eq!(allowed["session"]["bootstrap_participant_id"], "dev1");
    let participants = allowed["session"]["participants"].as_array().unwrap();
    assert_eq!(participants[0]["binding_origin"], "local_direct");
    assert_eq!(participants[0]["bootstrap"], true);
}

#[tokio::test]
async fn max_variants_per_plan_set_is_enforced() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".curd")).unwrap();

    let mut config = CurdConfig::default();
    config.variants.max_variants_per_plan_set = 1;
    fs::write(
        root.join(".curd/settings.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let ctx = EngineContext::new(root.to_str().unwrap());
    let bind = dispatch_tool(
        "bind_participant_role",
        &json!({
            "participant_id": "dev1",
            "role": "owner",
            "is_human": true
        }),
        &ctx,
    )
    .await;
    assert_eq!(bind["status"], "ok");

    let create_set = dispatch_tool(
        "create_plan_set",
        &json!({
            "participant_id": "dev1",
            "title": "quota",
            "objective": "Enforce variant count"
        }),
        &ctx,
    )
    .await;
    assert_eq!(create_set["status"], "ok");
    let plan_set_id = create_set["plan_set"]["id"].as_str().unwrap().to_string();

    let mk_plan = || Plan {
        id: Uuid::new_v4(),
        nodes: vec![],
    };

    let first = dispatch_tool(
        "create_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": plan_set_id,
            "title": "v1",
            "plan": serde_json::to_value(mk_plan()).unwrap()
        }),
        &ctx,
    )
    .await;
    assert_eq!(first["status"], "ok");

    let second = dispatch_tool(
        "create_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": plan_set_id,
            "title": "v2",
            "plan": serde_json::to_value(mk_plan()).unwrap()
        }),
        &ctx,
    )
    .await;
    assert!(second.get("error").is_some(), "{second}");
}

#[tokio::test]
async fn retention_prunes_old_plan_sets_and_variant_workspaces() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn value() -> i32 { 1 }\n").unwrap();
    fs::create_dir_all(root.join(".curd")).unwrap();

    let mut config = CurdConfig::default();
    config.variants.retain_plan_sets = 1;
    config.variants.retain_variant_workspaces = 1;
    fs::write(root.join(".curd/settings.toml"), toml::to_string(&config).unwrap()).unwrap();

    let ctx = EngineContext::new(root.to_str().unwrap());
    let bind = dispatch_tool(
        "bind_participant_role",
        &json!({
            "participant_id": "dev1",
            "role": "owner",
            "is_human": true
        }),
        &ctx,
    )
    .await;
    assert_eq!(bind["status"], "ok");

    let first_set = dispatch_tool(
        "create_plan_set",
        &json!({
            "participant_id": "dev1",
            "title": "first",
            "objective": "retention"
        }),
        &ctx,
    )
    .await;
    assert_eq!(first_set["status"], "ok");
    let first_set_id = first_set["plan_set"]["id"].as_str().unwrap().to_string();
    std::thread::sleep(Duration::from_secs(1));

    let second_set = dispatch_tool(
        "create_plan_set",
        &json!({
            "participant_id": "dev1",
            "title": "second",
            "objective": "retention"
        }),
        &ctx,
    )
    .await;
    assert_eq!(second_set["status"], "ok");
    let second_set_id = second_set["plan_set"]["id"].as_str().unwrap().to_string();
    assert_ne!(first_set_id, second_set_id);
    assert!(!root.join(".curd/plansets").join(format!("{first_set_id}.json")).exists());
    assert!(root.join(".curd/plansets").join(format!("{second_set_id}.json")).exists());

    let v1 = dispatch_tool(
        "create_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": second_set_id,
            "title": "v1",
            "plan": serde_json::to_value(Plan { id: Uuid::new_v4(), nodes: vec![] }).unwrap()
        }),
        &ctx,
    )
    .await;
    let v1_id = v1["variant"]["id"].as_str().unwrap().to_string();
    let s1 = dispatch_tool(
        "simulate_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": second_set_id,
            "variant_id": v1_id
        }),
        &ctx,
    )
    .await;
    assert_eq!(s1["status"], "ok", "{s1}");
    std::thread::sleep(Duration::from_secs(1));

    let v2 = dispatch_tool(
        "create_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": second_set_id,
            "title": "v2",
            "plan": serde_json::to_value(Plan { id: Uuid::new_v4(), nodes: vec![] }).unwrap()
        }),
        &ctx,
    )
    .await;
    let v2_id = v2["variant"]["id"].as_str().unwrap().to_string();
    let s2 = dispatch_tool(
        "simulate_plan_variant",
        &json!({
            "participant_id": "dev1",
            "plan_set_id": second_set_id,
            "variant_id": v2_id
        }),
        &ctx,
    )
    .await;
    assert_eq!(s2["status"], "ok", "{s2}");

    assert!(!root.join(".curd/variants").join(&second_set_id).join(&v1_id).join("workspace").exists());
    assert!(root.join(".curd/variants").join(&second_set_id).join(&v2_id).join("workspace").exists());
}
