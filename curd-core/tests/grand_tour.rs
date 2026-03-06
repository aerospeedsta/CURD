use curd_core::{
    DebugEngine, DiagramEngine, DocEngine, DslNode, EditEngine, EngineContext, FileEngine,
    FindEngine, GraphEngine, HistoryEngine, LspEngine, PlanEngine, ProfileEngine, ReadEngine,
    ReplState, SearchEngine, SessionReviewEngine, ShellEngine, Watchdog, WorkspaceEngine,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

#[tokio::test]
async fn test_library_grand_tour() -> anyhow::Result<()> {
    // 1. Setup a realistic multi-language workspace
    let dir = tempdir()?;
    let root = dir.path();

    // Rust file
    std::fs::write(
        root.join("main.rs"),
        "fn main() { let x = 1; }\nfn helper() { println!(\"helper\"); }",
    )?;
    // Python file
    std::fs::write(
        root.join("utils.py"),
        "def process_data(data):\n    return data.strip()\n\ndef run():\n    process_data('  hello  ')",
    )?;
    // JS file
    std::fs::write(
        root.join("index.js"),
        "function init() { console.log('init'); }",
    )?;

    // 2. Initialize the CURD kernel
    let root_path = PathBuf::from(root);
    let (tx_events, mut rx_events) = broadcast::channel(1024);
    let session_id = Uuid::new_v4();
    let watchdog = Arc::new(Watchdog::new(root_path.clone()));

    let ctx = EngineContext {
        workspace_root: root_path.clone(),
        session_id, read_only: false,
        se: Arc::new(SearchEngine::new(root).with_events(tx_events.clone())),
        re: Arc::new(ReadEngine::new(root)),
        ee: Arc::new(EditEngine::new(root).with_watchdog(watchdog.clone())),
        ge: Arc::new(GraphEngine::new(root)),
            ple: Arc::new(crate::PlanEngine::new(root)),
            she: Arc::new(crate::ShellEngine::new(root)),
        we: Arc::new(WorkspaceEngine::new(root)),
        mu: Arc::new(curd_core::MutationEngine::new(root)),
        fe: Arc::new(FindEngine::new(root)),
        de: Arc::new(DiagramEngine::new(root)),
        fie: Arc::new(FileEngine::new(root)),
        le: Arc::new(LspEngine::new(root)),
        pe: Arc::new(ProfileEngine::new(root)),
        dbe: Arc::new(DebugEngine::new(root)),
        sre: Arc::new(SessionReviewEngine::new(root)),
        doce: Arc::new(DocEngine::new()),
        doctore: Arc::new(curd_core::doctor::DoctorEngine::new(&root_path)),
        he: Arc::new(HistoryEngine::new(root)),
        tx_events,
        global_state: Arc::new(Mutex::new(ReplState::new())),
        sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        pending_challenges: Arc::new(Mutex::new(std::collections::HashMap::new())),
        watchdog,
    };

    let ple = PlanEngine::new(root);
    let mut state = ReplState::new();

    // 2.5 Prime the index
    ctx.se.search("", None)?;

    // 3. EXECUTION: The Grand Tour DSL
    // This plan exercises: Search -> Assign -> Read -> Atomic Edit -> Shell -> Invalidation
    let plan = vec![
        // A. Search for the python function and assign its results to a variable
        DslNode::Assign {
            var: "py_search".to_string(),
            value: json!({"tool": "search", "args": {"query": "process_data", "kind": "function"}}),
        },
        // B. Read that function using complex path resolution
        DslNode::Call {
            tool: "read".to_string(),
            args: json!({"uris": ["$py_search.symbols[0].id"]}),
        },
        // C. Perform a transactional (Atomic) edit
        DslNode::Atomic {
            nodes: vec![
                DslNode::Call {
                    tool: "edit".to_string(),
                    args: json!({
                        "uri": "main.rs::helper",
                        "code": "fn helper() { println!(\"helper v2\"); }",
                        "adaptation_justification": "Upgrade helper logic."
                    }),
                },
                // D. Verify with LSP inside the transaction
                DslNode::Call {
                    tool: "lsp".to_string(),
                    args: json!({"uri": "main.rs", "mode": "syntax"}),
                },
            ],
        },
        // E. Execute a shell command using data from step A
        DslNode::Call {
            tool: "shell".to_string(),
            args: json!({"command": "echo 'Target function was: $py_search.symbols[0].id'"}),
        },
    ];

    // 4. Run the plan
    let results = ple.execute_dsl(&plan, &ctx, &mut state).await?;
    println!(
        "DEBUG: DSL Results: {}",
        serde_json::to_string_pretty(&results)?
    );

    // 5. VALIDATION
    let res_arr = results.as_array().expect("Results should be an array");
    assert_eq!(res_arr.len(), 4, "Should have 4 top-level results");

    // Check Variable Assignment & Search
    let assign_res = &res_arr[0];
    assert!(
        assign_res["value"]["status"] == "ok",
        "Search status should be ok"
    );
    let symbols = assign_res["value"]["symbols"].as_array().unwrap();
    assert!(!symbols.is_empty(), "Should find at least one symbol");
    let found_uri = symbols[0]["id"].as_str().unwrap();
    assert!(found_uri.contains("process_data"));

    // Check Variable Resolution in Read
    let read_res = &res_arr[1];
    assert!(
        read_res["result"]["status"] == "ok",
        "Read status should be ok"
    );
    let actual_uri = read_res["result"]["results"][0]["uri"].as_str().unwrap();
    assert_eq!(actual_uri, found_uri);

    // Check Atomic Success
    let atomic_res = &res_arr[2];
    assert!(atomic_res["atomic"] == "committed");

    // Check Shell Interpolation
    let shell_res = &res_arr[3];
    let stdout = shell_res["result"]["output"]["stdout"].as_str().unwrap();
    let command = shell_res["result"]["output"]["command"]
        .as_str()
        .unwrap_or("");
    // In constrained environments shell execution may be blocked, so interpolation is validated
    // against the expanded command string as well.
    assert!(
        stdout.contains(found_uri) || command.contains(found_uri),
        "Shell interpolation missing. stdout='{}', command='{}'",
        stdout,
        command
    );

    // 6. Verify History
    let history = ctx.he.get_history(10);
    assert!(!history.is_empty(), "History should be logged");

    // 7. Verify Observability (Events were fired)
    let mut event_count = 0;
    while let Ok(_event) = rx_events.try_recv() {
        event_count += 1;
    }
    assert!(event_count > 0, "System events should have been emitted");

    Ok(())
}
