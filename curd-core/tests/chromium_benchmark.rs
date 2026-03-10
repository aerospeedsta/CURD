use curd_core::GraphEngine;
use std::time::Instant;

#[test]
#[ignore] // Run manually with: cargo test --test chromium_benchmark -- --nocapture --ignored
fn test_chromium_5_depth_tree() {
    let workspace_root = "/Users/bharath/workshop/expts/chromium-bench/chromium";
    let symbol_id = "src/ash/webui/boca_ui/boca_ui.h::BocaUI";
    
    let engine = GraphEngine::new(workspace_root);
    
    println!("Building dependency graph for Chromium...");
    let start_build = Instant::now();
    let _graph = engine.build_dependency_graph().expect("Failed to build dependency graph");
    let build_duration = start_build.elapsed();
    println!("Graph built in: {:?}", build_duration);
    
    println!("Searching for symbol: {}", symbol_id);
    let start_search = Instant::now();
    let result = engine.graph_with_depths(vec![symbol_id.to_string()], 5, 5)
        .expect("Failed to search graph");
    let search_duration = start_search.elapsed();
    
    println!("Tree Search result (5 up, 5 down) completed in: {:?}", search_duration);
    
    // Summary of results
    if let serde_json::Value::Object(map) = result {
        if let Some(nodes) = map.get("nodes").and_then(|n| n.as_array()) {
            println!("Nodes found in tree: {}", nodes.len());
        }
        if let Some(edges) = map.get("edges").and_then(|e| e.as_array()) {
            println!("Edges found in tree: {}", edges.len());
        }
    }
}
