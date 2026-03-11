use crate::GraphEngine;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct Watchdog {
    pub workspace_root: PathBuf,
    pub last_heartbeat: Arc<Mutex<Instant>>,
    pub last_poisoned_count: Arc<Mutex<usize>>,
    pub edit_history: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

impl Watchdog {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            last_heartbeat: Arc::new(Mutex::new(Instant::now())),
            last_poisoned_count: Arc::new(Mutex::new(0)),
            edit_history: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn heartbeat(&self) {
        let mut hb = self
            .last_heartbeat
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *hb = Instant::now();
    }

    pub fn track_edit(&self, uri: &str) {
        let mut history = self.edit_history.lock().unwrap_or_else(|e| e.into_inner());
        let times = history.entry(uri.to_string()).or_default();
        times.push(Instant::now());

        // Keep only last 5 minutes
        let five_mins_ago = Instant::now() - Duration::from_secs(300);
        times.retain(|&t| t > five_mins_ago);
    }

    pub fn start(&self) {
        let root = self.workspace_root.clone();
        let hb = self.last_heartbeat.clone();
        let last_pc = self.last_poisoned_count.clone();
        let edit_hist = self.edit_history.clone();

        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(60)).await;

                let mut looping_uris = Vec::new();
                {
                    let history = edit_hist.lock().unwrap_or_else(|e| e.into_inner());
                    for (uri, times) in history.iter() {
                        if times.len() >= 5 {
                            looping_uris.push(uri.clone());
                        }
                    }
                }

                let (last_heartbeat, poisoned_count) = {
                    let graph = GraphEngine::new(&root);
                    let g = graph.graph(vec![], "both", 1).unwrap_or_default();
                    let count = if let Some(nodes) = g.get("nodes").and_then(|n| n.as_array()) {
                        nodes
                            .iter()
                            .filter(|n| n.get("fault_state").is_some())
                            .count()
                    } else {
                        0
                    };

                    let hb_guard = hb.lock().unwrap_or_else(|e| e.into_inner());
                    (*hb_guard, count)
                };

                let regression = {
                    let mut last_pc_guard = last_pc.lock().unwrap_or_else(|e| e.into_inner());
                    let reg = poisoned_count > *last_pc_guard;
                    *last_pc_guard = poisoned_count;
                    reg
                };

                if last_heartbeat.elapsed() > Duration::from_secs(1200)
                    || regression
                    || !looping_uris.is_empty()
                {
                    let status = if regression {
                        "Fault Regression Detected".to_string()
                    } else if !looping_uris.is_empty() {
                        "Agent Looping Detected".to_string()
                    } else {
                        "Agent Stagnation Detected".to_string()
                    };

                    let _ = Self::trigger_crisis_intervention(
                        &root,
                        &status,
                        poisoned_count,
                        regression,
                        looping_uris,
                    )
                    .await;

                    // Reset heartbeat if it was a stagnation trigger
                    if !regression {
                        let mut hb_guard = hb.lock().unwrap_or_else(|e| e.into_inner());
                        *hb_guard = Instant::now();
                    }
                }
            }
        });
    }

    async fn trigger_crisis_intervention(
        root: &Path,
        status: &str,
        count: usize,
        regression: bool,
        looping_uris: Vec<String>,
    ) -> Result<()> {
        let graph = GraphEngine::new(root);
        let g = graph.graph(vec![], "both", 1).unwrap_or_default();

        let mut poisoned_uris = Vec::new();
        if let Some(nodes) = g.get("nodes").and_then(|n| n.as_array()) {
            for node in nodes {
                if let Some(id) = node.get("id").and_then(|v| v.as_str())
                    && node.get("fault_state").is_some()
                {
                    poisoned_uris.push(id.to_string());
                }
            }
        }

        let regression_note = if regression {
            "\n### ⚠️ REGRESSION ALERT\n\
            The number of poisoned nodes has INCREASED since the last check. Your latest changes may have introduced cascading failures."
        } else {
            ""
        };

        let loop_note = if !looping_uris.is_empty() {
            let list = looping_uris
                .iter()
                .map(|u| format!("- `{}`", u))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "\n### 🔄 LOOPING ALERT\n\
            The agent is repeatedly editing the following symbols without success:\n\
            {}\n\
            Consider checking if the agent is stuck in an edit-fail cycle.",
                list
            )
        } else {
            "".to_string()
        };

        let u_list = poisoned_uris
            .iter()
            .map(|u| format!("- `{}`", u))
            .collect::<Vec<_>>()
            .join("\n");

        // 2. Generate Report
        let report = format!(
            "# 🚨 CRISIS INTERVENTION REPORT\n\n\
            **Status**: {}\n\
            **Workplace Analysis**:\n\
            - Total Poisoned Nodes: {}\n\
            {}{}\n\n\
            ## Poisoned URIs\n\
            {}\n\n\
            ## Recommended Actions\n\
            1. **Rollback**: If you just introduced a regression, use `workspace action=rollback` immediately.\n\
            2. **Isolate**: Use `diagram uris=[\"ROOT_URI\"]` to see the exact fault propagation path.\n\
            3. **Verify**: Run `lsp uri=... mode=both` on the poisoned files to get detailed AST error messages.\n\
            4. **Simplify**: Break down the current refactor into smaller, validatable steps.\n",
            status, count, regression_note, loop_note, u_list
        );

        let curd_dir = crate::workspace::get_curd_dir(root);
        let report_path = curd_dir.join("watchdog_report.md");
        if let Some(p) = report_path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        tokio::fs::write(report_path, report).await?;

        Ok(())
    }
}
