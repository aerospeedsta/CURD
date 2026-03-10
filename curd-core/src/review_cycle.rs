use crate::{LspEngine, scan_workspace};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewCycleState {
    id: Uuid,
    label: Option<String>,
    started_at_secs: u64,
    snapshots: HashMap<String, String>,
    graph_snapshot: Option<crate::graph::DependencyGraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ReviewCycleEngine {
    pub workspace_root: PathBuf,
    active: Arc<Mutex<Option<ReviewCycleState>>>,
}

impl ReviewCycleEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = std::fs::canonicalize(workspace_root.as_ref())
            .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf());
        let engine = Self {
            workspace_root: root.clone(),
            active: Arc::new(Mutex::new(None)),
        };
        let _ = fs::create_dir_all(engine.sessions_dir());
        if let Some(id_str) = engine.head_id_str()
            && let Ok(id) = Uuid::parse_str(&id_str)
            && let Ok(state) = engine.load_session(id)
            && let Ok(mut g) = engine.active.lock()
        {
            *g = Some(state);
        }
        engine
    }

    pub fn begin(&self, label: Option<&str>) -> Result<Value> {
        let mut snapshots = HashMap::new();
        for file in scan_workspace(&self.workspace_root)? {
            let rel = file
                .strip_prefix(&self.workspace_root)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            let content = fs::read_to_string(&file).unwrap_or_default();
            snapshots.insert(rel, content);
        }

        // Capture initial graph state
        let graph_engine = crate::graph::GraphEngine::new(&self.workspace_root);
        let graph_snapshot = graph_engine.build_dependency_graph().ok();

        let id = Uuid::new_v4();
        let state = ReviewCycleState {
            id,
            label: label.map(ToString::to_string),
            started_at_secs: now_secs(),
            snapshots,
            graph_snapshot,
        };
        self.save_session(&state)?;
        self.write_head(id)?;
        if let Ok(mut g) = self.active.lock() {
            *g = Some(state.clone());
        }
        Ok(json!({
            "status": "started",
            "review_cycle_id": id,
            "session_id": id,
            "label": state.label,
            "started_at_secs": state.started_at_secs,
            "snapshot_file_count": state.snapshots.len(),
            "graph_available": state.graph_snapshot.is_some()
        }))
    }

    pub fn status(&self) -> Result<Value> {
        let guard = self
            .active
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if let Some(s) = guard.as_ref() {
            let changed = self.compute_changes(s)?;
            Ok(json!({
                "active": true,
                "review_cycle_id": s.id,
                "session_id": s.id,
                "label": s.label,
                "started_at_secs": s.started_at_secs,
                "changed_files": changed.len()
            }))
        } else {
            Ok(json!({"active": false}))
        }
    }

    pub fn changes(&self, limit: Option<usize>) -> Result<Value> {
        let guard = self
            .active
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let Some(s) = guard.as_ref() else {
            anyhow::bail!("No active review cycle. Call review_cycle(action: 'begin') first.");
        };
        let mut changed = self.compute_changes(s)?;
        changed.sort_by(|a, b| {
            b.get("changed_lines")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .cmp(&a.get("changed_lines").and_then(|v| v.as_u64()).unwrap_or(0))
        });
        if let Some(l) = limit {
            changed.truncate(l);
        }
        Ok(json!({
            "review_cycle_id": s.id,
            "session_id": s.id,
            "count": changed.len(),
            "changes": changed
        }))
    }

    pub async fn review(&self) -> Result<Value> {
        let session = {
            let guard = self
                .active
                .lock()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let Some(s) = guard.as_ref() else {
                anyhow::bail!("No active review cycle. Call review_cycle(action: 'begin') first.");
            };
            s.clone()
        };
        
        let changes = self.compute_raw_changes(&session)?;
        let mut findings: Vec<Finding> = Vec::new();
        let mut changed_files: Vec<String> = Vec::new();

        for ch in &changes {
            changed_files.push(ch.path.clone());
            for (line_no, line) in &ch.added_lines {
                if line.contains("unwrap(") || line.contains("expect(") || line.contains("panic!") {
                    findings.push(Finding {
                        severity: "medium".to_string(),
                        code: "risky_api".to_string(),
                        message: "Introduced unwrap/expect/panic in changed line".to_string(),
                        file: Some(ch.path.clone()),
                        line: Some(*line_no),
                    });
                }
                if line.contains("TODO") || line.contains("FIXME") || line.contains("XXX") {
                    findings.push(Finding {
                        severity: "low".to_string(),
                        code: "todo_marker".to_string(),
                        message: "Introduced TODO/FIXME/XXX marker".to_string(),
                        file: Some(ch.path.clone()),
                        line: Some(*line_no),
                    });
                }
            }
            if ch.changed_lines > 300 {
                findings.push(Finding {
                    severity: "medium".to_string(),
                    code: "large_change".to_string(),
                    message: format!("Large change in one file ({} lines)", ch.changed_lines),
                    file: Some(ch.path.clone()),
                    line: None,
                });
            }
        }

        // Detect Graph Changes
        let mut graph_diff = json!(null);
        if let Some(old_graph) = &session.graph_snapshot {
            let graph_engine = crate::graph::GraphEngine::new(&self.workspace_root);
            if let Ok(new_graph) = graph_engine.build_dependency_graph() {
                graph_diff = self.diff_graphs(old_graph, &new_graph);
            }
        }

        // Syntax diagnostics for changed files
        let lsp = LspEngine::new(&self.workspace_root);
        for path in &changed_files {
            if (path.ends_with(".rs")
                || path.ends_with(".py")
                || path.ends_with(".js")
                || path.ends_with(".ts")
                || path.ends_with(".go")
                || path.ends_with(".c")
                || path.ends_with(".cpp")
                || path.ends_with(".java"))
                && let Ok(diag) = lsp.diagnostics_with_mode(path, "syntax").await
            {
                let count = diag
                    .get("error_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if count > 0 {
                    findings.push(Finding {
                        severity: "high".to_string(),
                        code: "syntax_error".to_string(),
                        message: format!("Syntax diagnostics reported {} issue(s)", count),
                        file: Some(path.clone()),
                        line: None,
                    });
                }
            }
        }

        let touched_core = changed_files.iter().any(|p| is_source_file(p));
        let touched_tests = changed_files.iter().any(|p| is_test_file(p));
        if touched_core && !touched_tests {
            findings.push(Finding {
                severity: "medium".to_string(),
                code: "missing_tests".to_string(),
                message: "Core/source changes detected without corresponding test file changes"
                    .to_string(),
                file: None,
                line: None,
            });
        }

        findings.sort_by(|a, b| severity_rank(&a.severity).cmp(&severity_rank(&b.severity)));
        let high = findings.iter().filter(|f| f.severity == "high").count();
        let med = findings.iter().filter(|f| f.severity == "medium").count();
        let low = findings.iter().filter(|f| f.severity == "low").count();

        // Join with tool call history for provenance
        let history = crate::HistoryEngine::new(&self.workspace_root);
        let provenance: Vec<_> = history.get_history(1000)
            .into_iter()
            .filter(|e| e.transaction_id == Some(session.id))
            .map(|e| {
                json!({
                    "timestamp": e.timestamp_unix,
                    "tool": e.operation,
                    "input": e.input,
                    "success": e.success
                })
            })
            .collect();

        Ok(json!({
            "review_cycle_id": session.id,
            "session_id": session.id,
            "changed_files": changed_files,
            "graph_diff": graph_diff,
            "provenance": provenance,
            "summary": {
                "high": high,
                "medium": med,
                "low": low,
                "total": findings.len()
            },
            "findings": findings
        }))
    }

    fn diff_graphs(&self, old: &crate::graph::DependencyGraph, new: &crate::graph::DependencyGraph) -> Value {
        let mut added_nodes = Vec::new();
        let mut removed_nodes = Vec::new();
        let mut added_edges = Vec::new();
        let mut removed_edges = Vec::new();

        let old_nodes: HashSet<_> = old.outgoing.keys().collect();
        let new_nodes: HashSet<_> = new.outgoing.keys().collect();

        for n in new_nodes.difference(&old_nodes) {
            added_nodes.push(n.to_string());
        }
        for n in old_nodes.difference(&new_nodes) {
            removed_nodes.push(n.to_string());
        }

        // Diff edges
        let mut old_edges = HashSet::new();
        for (from, tos) in &old.outgoing {
            for to in tos {
                old_edges.insert((from.clone(), to.clone()));
            }
        }
        let mut new_edges = HashSet::new();
        for (from, tos) in &new.outgoing {
            for to in tos {
                new_edges.insert((from.clone(), to.clone()));
            }
        }

        for (from, to) in new_edges.difference(&old_edges) {
            added_edges.push(json!({"from": from, "to": to}));
        }
        for (from, to) in old_edges.difference(&new_edges) {
            removed_edges.push(json!({"from": from, "to": to}));
        }

        json!({
            "added_nodes": added_nodes,
            "removed_nodes": removed_nodes,
            "added_edges": added_edges,
            "removed_edges": removed_edges
        })
    }

    pub fn end(&self) -> Result<Value> {
        let mut guard = self
            .active
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let old = guard.take();
        let _ = fs::remove_file(self.head_path());
        Ok(json!({
            "status": if old.is_some() { "ended" } else { "no_active_session" },
            "review_cycle_id": old.as_ref().map(|s| s.id),
            "session_id": old.as_ref().map(|s| s.id)
        }))
    }

    fn compute_changes(&self, state: &ReviewCycleState) -> Result<Vec<Value>> {
        let raw = self.compute_raw_changes(state)?;
        Ok(raw
            .into_iter()
            .map(|c| {
                json!({
                    "path": c.path,
                    "status": c.status,
                    "added_lines": c.added_lines.len(),
                    "removed_lines": c.removed_lines,
                    "changed_lines": c.changed_lines
                })
            })
            .collect())
    }

    fn compute_raw_changes(&self, state: &ReviewCycleState) -> Result<Vec<RawChange>> {
        let mut current_files: HashMap<String, String> = HashMap::new();
        for file in scan_workspace(&self.workspace_root)? {
            let rel = file
                .strip_prefix(&self.workspace_root)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            current_files.insert(rel, fs::read_to_string(file).unwrap_or_default());
        }

        let mut keys: HashSet<String> = state.snapshots.keys().cloned().collect();
        keys.extend(current_files.keys().cloned());
        let mut out = Vec::new();

        for path in keys {
            let old = state.snapshots.get(&path).cloned().unwrap_or_default();
            let new = current_files.get(&path).cloned().unwrap_or_default();
            if old == new {
                continue;
            }
            let status = if !state.snapshots.contains_key(&path) {
                "created"
            } else if !current_files.contains_key(&path) {
                "deleted"
            } else {
                "modified"
            };
            let (added_lines, removed_lines) = diff_added_removed(&old, &new);
            let changed_lines = added_lines.len() + removed_lines;
            out.push(RawChange {
                path,
                status: status.to_string(),
                added_lines,
                removed_lines,
                changed_lines,
            });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn sessions_dir(&self) -> PathBuf {
        self.workspace_root.join(".curd").join("review_cycles")
    }
    fn head_path(&self) -> PathBuf {
        self.sessions_dir().join("HEAD")
    }
    fn session_path(&self, id: Uuid) -> PathBuf {
        self.sessions_dir().join(format!("session_{}.json", id))
    }
    fn write_head(&self, id: Uuid) -> Result<()> {
        fs::write(self.head_path(), id.to_string())?;
        Ok(())
    }
    fn head_id_str(&self) -> Option<String> {
        fs::read_to_string(self.head_path()).ok().map(|s| s.trim().to_string())
    }
    fn save_session(&self, state: &ReviewCycleState) -> Result<()> {
        let path = self.session_path(state.id);
        fs::write(path, serde_json::to_string_pretty(state)?)?;
        Ok(())
    }
    fn load_session(&self, id: Uuid) -> Result<ReviewCycleState> {
        let path = self.session_path(id);
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }
}

#[derive(Debug, Clone)]
struct RawChange {
    path: String,
    status: String,
    added_lines: Vec<(usize, String)>,
    removed_lines: usize,
    changed_lines: usize,
}

fn diff_added_removed(old: &str, new: &str) -> (Vec<(usize, String)>, usize) {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let n = a.len();
    let m = b.len();

    if n.checked_mul(m).is_none_or(|cells| cells > 4_000_000) {
        let added = b
            .iter()
            .enumerate()
            .map(|(i, l)| (i + 1, l.to_string()))
            .collect();
        return (added, n);
    }

    let mut dp = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0usize, 0usize);
    let mut added = Vec::new();
    let mut removed = 0usize;
    while i < n && j < m {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            removed += 1;
            i += 1;
        } else {
            added.push((j + 1, b[j].to_string()));
            j += 1;
        }
    }
    while i < n {
        removed += 1;
        i += 1;
    }
    while j < m {
        added.push((j + 1, b[j].to_string()));
        j += 1;
    }
    (added, removed)
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    }
}

fn is_source_file(path: &str) -> bool {
    const SOURCE_EXTS: &[&str] = &[
        "rs", "py", "js", "ts", "tsx", "go", "c", "cpp", "cc", "cxx", "java", "kt", "swift",
    ];
    let normalized = path.replace('\\', "/");
    if normalized.contains("/tests/") || normalized.contains("/test/") {
        return false;
    }
    normalized
        .rsplit_once('.')
        .map(|(_, ext)| SOURCE_EXTS.contains(&ext))
        .unwrap_or(false)
}

fn is_test_file(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    normalized.contains("/tests/")
        || normalized.contains("/test/")
        || normalized.ends_with("_test.rs")
        || normalized.ends_with("_test.py")
        || normalized.ends_with(".spec.js")
        || normalized.ends_with(".test.js")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".test.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".test.tsx")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
