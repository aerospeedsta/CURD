use crate::{LspEngine, scan_workspace};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionState {
    id: u64,
    label: Option<String>,
    started_at_secs: u64,
    snapshots: HashMap<String, String>,
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
pub struct SessionReviewEngine {
    pub workspace_root: PathBuf,
    active: Arc<Mutex<Option<SessionState>>>,
}

impl SessionReviewEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = std::fs::canonicalize(workspace_root.as_ref())
            .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf());
        let engine = Self {
            workspace_root: root.clone(),
            active: Arc::new(Mutex::new(None)),
        };
        let _ = fs::create_dir_all(engine.sessions_dir());
        if let Some(id) = engine.head_id()
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

        let id = next_session_id();
        let state = SessionState {
            id,
            label: label.map(ToString::to_string),
            started_at_secs: now_secs(),
            snapshots,
        };
        self.save_session(&state)?;
        self.write_head(id)?;
        if let Ok(mut g) = self.active.lock() {
            *g = Some(state.clone());
        }
        Ok(json!({
            "status": "started",
            "session_id": id,
            "label": state.label,
            "started_at_secs": state.started_at_secs,
            "snapshot_file_count": state.snapshots.len()
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
            anyhow::bail!("No active session. Call session_begin first.");
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
                anyhow::bail!("No active session. Call session_begin first.");
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

        // Missing tests heuristic for core changes
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

        Ok(json!({
            "session_id": session.id,
            "changed_files": changed_files,
            "summary": {
                "high": high,
                "medium": med,
                "low": low,
                "total": findings.len()
            },
            "findings": findings
        }))
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
            "session_id": old.as_ref().map(|s| s.id)
        }))
    }

    fn compute_changes(&self, state: &SessionState) -> Result<Vec<Value>> {
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

    fn compute_raw_changes(&self, state: &SessionState) -> Result<Vec<RawChange>> {
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
        self.workspace_root.join(".curd").join("session_review")
    }
    fn head_path(&self) -> PathBuf {
        self.sessions_dir().join("HEAD")
    }
    fn session_path(&self, id: u64) -> PathBuf {
        self.sessions_dir().join(format!("session_{}.json", id))
    }
    fn write_head(&self, id: u64) -> Result<()> {
        fs::write(self.head_path(), id.to_string())?;
        Ok(())
    }
    fn head_id(&self) -> Option<u64> {
        fs::read_to_string(self.head_path())
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
    }
    fn save_session(&self, state: &SessionState) -> Result<()> {
        let path = self.session_path(state.id);
        fs::write(path, serde_json::to_string_pretty(state)?)?;
        Ok(())
    }
    fn load_session(&self, id: u64) -> Result<SessionState> {
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

    // Prevent OOM for very large files by capping the DP matrix
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

fn next_session_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
