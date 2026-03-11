use curd_core::{GraphEngine, SearchEngine};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tempfile::tempdir;

#[test]
#[ignore]
fn local_graph_perf_scaffold_reports_index_query_and_incremental_timings() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    seed_workspace(root, 120, 12);

    let se = SearchEngine::new(root);
    let ge = GraphEngine::new(root);

    let start_index = Instant::now();
    let symbols = se.search("fn_", None).expect("initial search/index");
    let initial_index_ms = start_index.elapsed().as_millis();
    if symbols.is_empty() {
        let report = json!({
            "status": "skipped",
            "reason": "no_symbols_indexed_for_temp_workspace",
            "workspace_files": 120,
            "functions_per_file": 12,
            "initial_index_ms": initial_index_ms,
        });
        emit_report(root, &report);
        if bool_env("CURD_PERF_REQUIRE_SYMBOLS") {
            panic!("{report}");
        }
        return;
    }

    let target = symbols
        .iter()
        .find(|sym| sym.id.ends_with("fn_5"))
        .map(|sym| sym.id.clone())
        .unwrap_or_else(|| symbols[0].id.clone());

    let start_graph = Instant::now();
    let graph = ge
        .graph_with_depths(vec![target.clone()], 2, 2)
        .expect("graph query");
    let graph_query_ms = start_graph.elapsed().as_millis();

    let touched = root.join("src/file_005.rs");
    let original = fs::read_to_string(&touched).expect("read touched file");
    fs::write(&touched, original.replace("fn_5()", "fn_6()")).expect("write touched file");

    let start_incremental = Instant::now();
    let _ = se.search("fn_6", None).expect("incremental reindex search");
    let incremental_ms = start_incremental.elapsed().as_millis();

    let report = json!({
        "status": "ok",
        "workspace_files": 120,
        "functions_per_file": 12,
        "initial_index_ms": initial_index_ms,
        "graph_query_ms": graph_query_ms,
        "incremental_reindex_ms": incremental_ms,
        "query_target": target,
        "graph_node_count": graph["nodes"].as_array().map(|nodes| nodes.len()).unwrap_or(0),
        "graph_edge_count": graph["edges"].as_array().map(|edges| edges.len()).unwrap_or(0),
    });
    emit_report(root, &report);
    assert_perf_budget("CURD_PERF_MAX_INITIAL_INDEX_MS", initial_index_ms);
    assert_perf_budget("CURD_PERF_MAX_GRAPH_QUERY_MS", graph_query_ms);
    assert_perf_budget("CURD_PERF_MAX_INCREMENTAL_REINDEX_MS", incremental_ms);
}

fn seed_workspace(root: &Path, files: usize, functions_per_file: usize) {
    fs::create_dir_all(root.join("src")).expect("src dir");
    for file_idx in 0..files {
        let mut content = String::new();
        for fn_idx in 0..functions_per_file {
            let callee = if fn_idx == 0 { None } else { Some(fn_idx - 1) };
            content.push_str(&format!("pub fn fn_{fn_idx}() {{\n"));
            if let Some(callee) = callee {
                content.push_str(&format!("    fn_{callee}();\n"));
            }
            if file_idx > 0 && fn_idx == 0 {
                content.push_str("    fn_1();\n");
            }
            content.push_str("}\n\n");
        }
        fs::write(root.join(format!("src/file_{file_idx:03}.rs")), content).expect("write file");
    }
}

fn emit_report(root: &Path, report: &serde_json::Value) {
    println!("{report}");
    if let Some(path) = std::env::var_os("CURD_PERF_REPORT_PATH") {
        let path = Path::new(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, report.to_string());
    }
    if bool_env("CURD_PERF_SAVE_WORKSPACE_REPORT") {
        let benchmark_dir = root.join(".curd").join("benchmarks");
        let _ = fs::create_dir_all(&benchmark_dir);
        let _ = fs::write(
            benchmark_dir.join("graph_perf_scaffold.json"),
            report.to_string(),
        );
    }
}

fn bool_env(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

fn assert_perf_budget(name: &str, observed_ms: u128) {
    let Some(limit) = std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u128>().ok())
    else {
        return;
    };
    assert!(
        observed_ms <= limit,
        "{name} exceeded: observed {observed_ms}ms > limit {limit}ms"
    );
}
