use curd_core::{GraphEngine, SearchEngine, context::EngineContext, MutationEngine, context::handle_crawl};
use std::time::Instant;
use std::sync::Arc;
use serde_json::json;

#[tokio::test]
#[ignore] // Run manually with: PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo test --test vscode_stress_test -- --nocapture --ignored
async fn test_vscode_stress_operations() {
    let workspace_root = "/tmp/vscode_bench/vscode";
    
    // Check if the directory exists so we don't fail mysteriously on other machines
    if !std::path::Path::new(workspace_root).exists() {
        println!("Skipping vscode stress test because {} does not exist.", workspace_root);
        return;
    }

    println!("Initializing Search Engine...");
    let se = Arc::new(SearchEngine::new(workspace_root));
    
    println!("Running initial search to trigger indexing...");
    let start_idx = Instant::now();
    let symbols = se.search("registerCommand", None).expect("Failed to search");
    let idx_duration = start_idx.elapsed();
    println!("Initial search/indexing took: {:?}. Found {} 'registerCommand' functions.", idx_duration, symbols.len());

    let ge = Arc::new(GraphEngine::new(workspace_root));
    println!("Building dependency graph...");
    let start_build = Instant::now();
    let _graph = ge.build_dependency_graph().expect("Failed to build graph");
    let build_duration = start_build.elapsed();
    println!("Graph built in: {:?}", build_duration);

    // Get a known symbol from vscode
    let symbol_id = symbols.first().map(|s| s.id.clone()).unwrap_or_else(|| "src/vs/workbench/api/common/extHostCommands.ts::registerCommand".to_string());
    
    println!("Graph Tree Search for {}...", symbol_id);
    let start_tree = Instant::now();
    let tree = ge.graph_tree_with_depths(vec![symbol_id.clone()], 3, 3).expect("Failed graph tree");
    println!("Tree Search (3 up, 3 down) completed in: {:?}", start_tree.elapsed());
    
    if let serde_json::Value::Object(map) = tree {
        let node_count = map.get("nodes").and_then(|n| n.as_array()).map(|a| a.len()).unwrap_or(0);
        let edge_count = map.get("edges").and_then(|n| n.as_array()).map(|a| a.len()).unwrap_or(0);
        println!("  Tree contains {} nodes, {} edges", node_count, edge_count);
    }

    // Set up context to test crawl and mutate
    let (tx, _) = tokio::sync::broadcast::channel(10);
    let ctx = EngineContext {
        workspace_root: std::path::PathBuf::from(workspace_root),
        session_id: uuid::Uuid::new_v4(),
        read_only: false,
        se: se.clone(),
        re: Arc::new(curd_core::ReadEngine::new(workspace_root)),
        ee: Arc::new(curd_core::EditEngine::new(workspace_root)),
        doctore: Arc::new(curd_core::doctor::DoctorEngine::new(workspace_root)),
        ge: ge.clone(),
        we: Arc::new(curd_core::WorkspaceEngine::new(workspace_root)),
        ple: Arc::new(curd_core::PlanEngine::new(workspace_root)),
        mu: Arc::new(MutationEngine::new(workspace_root)),
        fe: Arc::new(curd_core::FindEngine::new(workspace_root)),
        de: Arc::new(curd_core::DiagramEngine::new(workspace_root)),
        fie: Arc::new(curd_core::FileEngine::new(workspace_root)),
        le: Arc::new(curd_core::LspEngine::new(workspace_root)),
        pe: Arc::new(curd_core::ProfileEngine::new(workspace_root)),
        dbe: Arc::new(curd_core::DebugEngine::new(workspace_root)),
        sre: Arc::new(curd_core::SessionReviewEngine::new(workspace_root)),
        doce: Arc::new(curd_core::DocEngine::new()),
        he: Arc::new(curd_core::HistoryEngine::new(workspace_root)),
        tx_events: tx,
        global_state: Arc::new(tokio::sync::Mutex::new(curd_core::ReplState::new())),
        sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        pending_challenges: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        watchdog: Arc::new(curd_core::Watchdog::new(workspace_root.into())),
        she: Arc::new(curd_core::ShellEngine::new(workspace_root)),
    };

    println!("Testing handle_crawl in mutate mode...");
    let crawl_params = json!({
        "mode": "crawl_mutate",
        "roots": [symbol_id],
        "depth": 2,
        "top_k": 5
    });
    
    let start_crawl = Instant::now();
    let crawl_result = handle_crawl(&crawl_params, &ctx).await;
    println!("Crawl completed in: {:?}", start_crawl.elapsed());
    
    let candidates = crawl_result.get("ranked_candidates").and_then(|c| c.as_array());
    println!("  Crawl yielded {} candidates", candidates.map(|c| c.len()).unwrap_or(0));
    
    if let Some(cands) = candidates
        && let Some(top_cand) = cands.first()
            && let Some(uri) = top_cand.get("uri").and_then(|u| u.as_str()) {
                println!("Testing mutation on top candidate: {}", uri);
                let start_mutate = Instant::now();
                // Instead of actually mutating VS Code codebase (which we shouldn't necessarily do unless we use shadow, 
                // but mutate_symbol currently mutates in place), let's just log what we would mutate.
                // Actually wait, mutate_symbol mutates the real file currently and then logs to trace.
                // Let's copy a small file into a tmp dir inside the test and run the mutation engine there,
                // or just accept we're mutating the /tmp/vscode_bench clone which is disposable.
                
                let res = ctx.mu.mutate_symbol(uri);
                println!("  Mutation result: {:?}", res);
                println!("Mutation completed in: {:?}", start_mutate.elapsed());
            }
}
