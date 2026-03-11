use crate::SearchEngine;
use crate::deps;
use crate::graph_audit::{GraphSnapshot, audit};
use crate::transaction::ShadowStore;
use anyhow::Result;
use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Returns the path to the internal CURD state directory.
/// Priority:
/// 1. CURD_STATE_DIR environment variable
/// 2. workspace_root/.curd
pub fn get_curd_dir(workspace_root: &Path) -> PathBuf {
    if let Ok(env_path) = std::env::var("CURD_STATE_DIR") {
        let p = PathBuf::from(env_path);
        if !p.is_absolute() {
            return workspace_root.join(p);
        }
        return p;
    }

    // Detect if we are in a shadow transaction root
    let root_str = workspace_root.to_string_lossy();
    if let Some(idx) = root_str.find(".curd/shadow/") {
        let real_root = PathBuf::from(&root_str[..idx]);
        return real_root.join(".curd");
    }

    workspace_root.join(".curd")
}

/// Scans a workspace directory for supported source files, correctly ignoring
/// paths specified in .gitignore and avoiding hidden directories like .git.
pub fn scan_workspace(root: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let root = root.as_ref();
    if !root.is_dir() {
        anyhow::bail!(
            "Workspace root is not a valid directory: {}",
            root.display()
        );
    }

    let excluded_dirs = deps::get_excluded_dirs(root);
    let files = Arc::new(Mutex::new(Vec::new()));

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy();
            !excluded_dirs.iter().any(|d| name == *d)
        })
        .threads(threads)
        .build_parallel();

    walker.run(|| {
        let files = Arc::clone(&files);
        Box::new(move |entry| {
            if let Ok(entry) = entry {
                let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
                let path = entry.path();
                if is_file
                    && is_supported_language(path)
                    && let Ok(mut f) = files.lock()
                {
                    f.push(entry.into_path());
                }
            }
            ignore::WalkState::Continue
        })
    });

    let result = Arc::try_unwrap(files)
        .map_err(|_| anyhow::anyhow!("Arc still has multiple owners"))?
        .into_inner()
        .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
    Ok(result)
}

/// Scans a workspace directory and returns ALL files (not just source), respecting .gitignore.
/// Optionally filters by a relative path prefix.
pub fn list_workspace(root: impl AsRef<Path>, prefix: Option<&str>) -> Result<Vec<String>> {
    let root = root.as_ref();
    if !root.is_dir() {
        anyhow::bail!(
            "Workspace root is not a valid directory: {}",
            root.display()
        );
    }

    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let excluded_dirs = deps::get_excluded_dirs(root);
    let entries = Arc::new(Mutex::new(Vec::new()));

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy();
            !excluded_dirs.iter().any(|d| name == *d)
        })
        .threads(threads)
        .build_parallel();

    walker.run(|| {
        let entries = Arc::clone(&entries);
        let canonical_root = canonical_root.clone();
        Box::new(move |entry| {
            if let Ok(entry) = entry
                && entry.file_type().map(|ft| ft.is_file()).unwrap_or(false)
            {
                let path = entry.path();
                if let Ok(abs) = std::fs::canonicalize(path)
                    && let Ok(rel) = abs.strip_prefix(&canonical_root)
                    && let Ok(mut e) = entries.lock()
                {
                    e.push(rel.to_string_lossy().into_owned());
                }
            }
            ignore::WalkState::Continue
        })
    });

    let mut result = Arc::try_unwrap(entries)
        .map_err(|_| anyhow::anyhow!("Arc still has multiple owners"))?
        .into_inner()
        .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
    if let Some(pfx) = prefix {
        result.retain(|e| e.starts_with(pfx));
    }
    result.sort();
    Ok(result)
}

/// Returns the absolute path if it is completely enclosed by the workspace root.
/// Blocks `..` traversal escapes and absolute `/` injections.
pub fn validate_sandboxed_path(workspace_root: &Path, requested_path: &str) -> Result<PathBuf> {
    let workspace_root =
        std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    if requested_path.contains("..")
        || requested_path.starts_with('/')
        || requested_path.starts_with('~')
    {
        return Err(anyhow::anyhow!(
            "Path '{}' contains traversal attempts or absolute roots. All paths must be relative to the workspace root.",
            requested_path
        ));
    }

    let path = Path::new(requested_path);
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
        && (file_name == ".env" || file_name.ends_with(".env"))
    {
        return Err(anyhow::anyhow!(
            "Access to .env files is strictly prohibited."
        ));
    }

    // Specifically protect the internal `.curd` operational directory state
    // against arbitrary modification by agents, regardless of sandbox routing
    for component in path.components() {
        if let std::path::Component::Normal(c) = component {
            if c == ".curd" {
                return Err(anyhow::anyhow!(
                    "Access to the internal '.curd' directory is strictly prohibited to prevent data corruption and security bypasses."
                ));
            }
        }
    }

    let full_path = workspace_root.join(requested_path);

    // Canonicalize it to resolve any symlinks, but gracefully handle if it doesn't exist yet (like on "create")
    // We find the deepest existing ancestor to canonicalize it correctly
    let mut existing_ancestor = &*full_path;
    while !existing_ancestor.exists() {
        let Some(parent) = existing_ancestor.parent() else {
            break;
        };
        existing_ancestor = parent;
    }

    let resolved_path = if let Ok(canon) = fs::canonicalize(existing_ancestor) {
        let rel = full_path
            .strip_prefix(existing_ancestor)
            .unwrap_or_else(|_| Path::new(""));
        if rel.as_os_str().is_empty() {
            canon
        } else {
            canon.join(rel)
        }
    } else {
        full_path
    };

    if !resolved_path.starts_with(&workspace_root) {
        return Err(anyhow::anyhow!(
            "Path '{}' attempts to escape the workspace sandbox. Execution denied.",
            requested_path
        ));
    }

    Ok(resolved_path)
}

/// Returns true if the workspace is currently locked by another active transaction.
pub fn is_workspace_locked(workspace_root: &Path) -> bool {
    let curd_dir = get_curd_dir(workspace_root);
    let lock_path = curd_dir.join("SESSION_LOCK");
    if !lock_path.exists() {
        return false;
    }
    if let Ok(pid_str) = std::fs::read_to_string(&lock_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if pid == std::process::id() {
                return false;
            }

            // Check if the process is actually alive (Unix-specific check)
            #[cfg(unix)]
            {
                let exists = std::process::Command::new("kill")
                    .arg("-0")
                    .arg(pid.to_string())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(true); // Assume alive if check fails

                if !exists {
                    log::info!("Stale lock detected for PID {}. Ignoring.", pid);
                    let _ = std::fs::remove_file(&lock_path);
                    return false;
                }
            }
            return true;
        }
    }
    false
}

/// Very simple initial check to only process source files we care about.
fn is_supported_language(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    matches!(
        ext.as_str(),
        "py" | "rs" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "h" | "hpp" | "go" | "java"
    )
}

/// Manages workspace state and the transaction shadow store for the MCP `workspace` tool.
/// The shadow store is shared via Arc<Mutex> so the same engine instance can be reused
/// across multiple MCP requests in the same process lifetime.
pub struct WorkspaceEngine {
    pub workspace_root: PathBuf,
    pub shadow: Arc<Mutex<ShadowStore>>,
}

impl WorkspaceEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = std::fs::canonicalize(workspace_root.as_ref())
            .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf());
        Self {
            workspace_root: root.clone(),
            shadow: Arc::new(Mutex::new(ShadowStore::new(root))),
        }
    }

    pub fn execute(&self, action: &str) -> anyhow::Result<serde_json::Value> {
        match action {
            "status" => {
                let files = scan_workspace(&self.workspace_root).unwrap_or_default();
                let shadow = self
                    .shadow
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock shadow store: {}", e))?;
                let staged: Vec<String> = shadow
                    .staged_paths()
                    .iter()
                    .filter_map(|p| {
                        p.strip_prefix(&self.workspace_root)
                            .ok()
                            .map(|r| r.to_string_lossy().into_owned())
                    })
                    .collect();
                Ok(serde_json::json!({
                    "transaction_active": shadow.is_active(),
                    "staged_files": staged,
                    "staged_count": staged.len(),
                    "files_found": files.len()
                }))
            }
            "list" => {
                let files = list_workspace(&self.workspace_root, None)?;
                Ok(serde_json::json!({
                    "status": "ok",
                    "files": files,
                    "count": files.len()
                }))
            }
            "dependencies" => match deps::detect_dependencies(&self.workspace_root) {
                Some(info) => Ok(deps::dependencies_to_json(&info)),
                None => Ok(serde_json::json!({
                    "status": "ok",
                    "message": "No package manager manifest detected at workspace root",
                    "dependencies": [],
                    "dependency_count": 0
                })),
            },
            "begin" => {
                self.shadow
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock shadow store: {}", e))?
                    .begin()?;
                Ok(serde_json::json!({
                    "status": "ok",
                    "message": "Transaction started. Workspace cloned to physical shadow."
                }))
            }
            "diff" => {
                let mut shadow = self
                    .shadow
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock shadow store: {}", e))?;
                if !shadow.is_active() {
                    return Ok(serde_json::json!({
                        "status": "ok",
                        "message": "No active transaction. Call begin first.",
                        "diff": ""
                    }));
                }
                let diff = shadow.diff();
                let count = shadow.len();
                Ok(serde_json::json!({
                    "status": "ok",
                    "staged_count": count,
                    "diff": diff
                }))
            }
            "commit" => {
                let mut shadow = self
                    .shadow
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock shadow store: {}", e))?;

                if shadow.has_savepoints() {
                    shadow.commit_savepoint()?;
                    return Ok(serde_json::json!({
                        "status": "ok",
                        "message": "Committed nested savepoint."
                    }));
                }

                let paths_to_commit = shadow.staged_paths();

                // Build pre-commit graph snapshot
                let before_graph =
                    GraphSnapshot::from_files(&paths_to_commit, &self.workspace_root);

                // Perform the atomic commit
                let written = shadow.commit()?;

                // Build post-commit graph snapshot
                let after_graph = GraphSnapshot::from_files(&written, &self.workspace_root);

                // Invalidate symbol index only when files were actually written.
                if !written.is_empty() {
                    SearchEngine::new(&self.workspace_root).invalidate_index();
                }

                // Run architectural audit
                let report = audit(&before_graph, &after_graph);

                // If alerts were generated, write them to disk so the agent can read them
                let curd_dir = get_curd_dir(&self.workspace_root);
                let alerts_file = curd_dir.join("alerts.json");
                if !report.is_clean() {
                    let _ = fs::create_dir_all(&curd_dir);
                    let _ = fs::write(
                        &alerts_file,
                        serde_json::to_string_pretty(&report.to_json())?,
                    );
                } else {
                    let _ = fs::remove_file(&alerts_file); // Clean up old alerts
                }

                let paths: Vec<String> = written
                    .iter()
                    .filter_map(|p| {
                        p.strip_prefix(&self.workspace_root)
                            .ok()
                            .map(|r| r.to_string_lossy().into_owned())
                    })
                    .collect();

                Ok(serde_json::json!({
                    "status": "ok",
                    "committed": paths,
                    "files_written": paths.len(),
                    "architectural_alerts_count": report.alert_count,
                    "alerts_available": !report.is_clean(),
                    "message": "View alerts via `workspace alerts`"
                }))
            }
            "rollback" => {
                let mut shadow = self
                    .shadow
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock shadow store: {}", e))?;
                let is_nested = shadow.has_savepoints();
                let count = shadow.len();
                shadow.rollback();
                let msg = if is_nested {
                    format!(
                        "Rolled back to previous savepoint. Current staged files: {}",
                        shadow.len()
                    )
                } else {
                    format!(
                        "Rolled back {} staged change(s). Disk files untouched.",
                        count
                    )
                };
                Ok(serde_json::json!({
                    "status": "ok",
                    "message": msg
                }))
            }
            "alerts" => {
                let mut report_json = serde_json::json!({
                    "architectural_alerts": serde_json::Value::Null,
                    "watchdog_report": serde_json::Value::Null
                });
                let curd_dir = get_curd_dir(&self.workspace_root);
                let alerts_file = curd_dir.join("alerts.json");
                if let Ok(content) = fs::read_to_string(&alerts_file)
                    && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
                {
                    report_json["architectural_alerts"] = json;
                }

                let watchdog_file = curd_dir.join("watchdog_report.md");
                if let Ok(content) = fs::read_to_string(&watchdog_file) {
                    report_json["watchdog_report"] = serde_json::json!(content);
                }

                Ok(report_json)
            }
            "clear_faults" => {
                let graph = crate::GraphEngine::new(&self.workspace_root);
                let curd_dir = get_curd_dir(&self.workspace_root);
                let _ = std::fs::remove_file(curd_dir.join("watchdog_report.md"));
                // We'll add a clear_all method to GraphEngine
                let mut g = graph.build_dependency_graph()?;
                g.fault_states.clear();
                let _ = std::fs::write(
                    curd_dir.join("graph_index.json"),
                    serde_json::to_string(&g)?,
                );

                Ok(serde_json::json!({
                    "status": "ok",
                    "message": "All semantic faults and watchdog reports cleared."
                }))
            }
            _ => Err(anyhow::anyhow!(
                "Invalid action for workspace: '{}'. Valid: status, list, dependencies, begin, diff, commit, rollback, alerts",
                action
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_scan_workspace() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("test1.py"), "print('hello')").unwrap();
        fs::write(root.join("test2.rs"), "fn main() {}").unwrap();
        fs::write(root.join("ignored.txt"), "some text").unwrap();

        let files = scan_workspace(root).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_list_workspace() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("test1.py"), "print('hello')").unwrap();
        fs::write(root.join("readme.md"), "# hello").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn main() {}").unwrap();

        let all_files = list_workspace(root, None).unwrap();
        assert_eq!(all_files.len(), 3);

        let src_files = list_workspace(root, Some("src/")).unwrap();
        assert_eq!(src_files.len(), 1);
        assert_eq!(src_files[0], "src/lib.rs");
    }
}
