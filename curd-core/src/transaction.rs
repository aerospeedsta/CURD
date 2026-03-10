use crate::deps;
use anyhow::Result;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

/// Shadow store for the CURD transaction system.
/// When a transaction is active, the workspace is cloned to a temporary shadow root.
/// Edits and shell commands execute against the shadow root.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionState {
    pub active_uuid: String,
    pub staged_files: Vec<PathBuf>,
    pub base_hashes: HashMap<PathBuf, String>,
    #[serde(default)]
    pub shadow_meta: HashMap<PathBuf, ShadowFileMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ShadowFileMeta {
    pub size: u64,
    pub mtime_secs: u64,
}

#[derive(Debug)]
pub struct ShadowStore {
    pub workspace_root: PathBuf,
    pub shadow_root: Option<PathBuf>,
    active: bool,
    staged_files: HashSet<PathBuf>, // Tracks relative paths modified via `stage`
    base_hashes: HashMap<PathBuf, String>, // Tracks SHA256 hashes of files when begin() was called
    shadow_meta: HashMap<PathBuf, ShadowFileMeta>, // Shadow metadata snapshot at begin()
    savepoints: Vec<Savepoint>,     // Stack for nested transactions
}

#[derive(Debug)]
struct Savepoint {
    staged_files: HashSet<PathBuf>,
    base_hashes: HashMap<PathBuf, String>,
}

impl ShadowStore {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = std::fs::canonicalize(workspace_root.as_ref())
            .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf());
        let mut store = Self {
            workspace_root: root,
            shadow_root: None,
            active: false,
            staged_files: HashSet::new(),
            base_hashes: HashMap::new(),
            shadow_meta: HashMap::new(),
            savepoints: Vec::new(),
        };
        store.load_head();
        store
    }

    /// Attempts to reload an interrupted transaction from .curd/shadow/HEAD
    fn load_head(&mut self) {
        let head = self.head_path();
        if head.exists() {
            if let Ok(json) = fs::read_to_string(&head)
                && let Ok(state) = serde_json::from_str::<TransactionState>(&json)
            {
                let shadow_dir = self
                    .workspace_root
                    .join(".curd")
                    .join("shadow")
                    .join(&state.active_uuid);
                if shadow_dir.exists() {
                    self.shadow_root = Some(shadow_dir);
                    self.active = true;
                    self.staged_files = state.staged_files.into_iter().collect();
                    self.base_hashes = state.base_hashes;
                    self.shadow_meta = state.shadow_meta;
                    log::info!(
                        "Recovered active shadow workspace transaction: {}",
                        state.active_uuid
                    );
                    return;
                }
            }
            // If we failed to parse or shadow_dir missing, clean up the orphaned head
            let _ = fs::remove_file(head);
        }
    }

    /// Path to the HEAD manifest tracking active transaction state
    fn head_path(&self) -> PathBuf {
        self.workspace_root
            .join(".curd")
            .join("shadow")
            .join("HEAD")
    }

    fn write_head(&self) -> Result<()> {
        if let Some(ref root) = self.shadow_root {
            let uuid = root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let state = TransactionState {
                active_uuid: uuid,
                staged_files: self.staged_files.iter().cloned().collect(),
                base_hashes: self.base_hashes.clone(),
                shadow_meta: self.shadow_meta.clone(),
            };
            let json = serde_json::to_string_pretty(&state)?;

            let head_path = self.head_path();
            if let Some(parent) = head_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(head_path, json)?;
        }
        Ok(())
    }

    /// Start a new transaction or a nested savepoint.
    pub fn begin(&mut self) -> Result<()> {
        if !self.active && crate::workspace::is_workspace_locked(&self.workspace_root) {
            anyhow::bail!(
                "Cannot start transaction: Workspace is locked by another active transaction."
            );
        }

        if self.active {
            // Push current state to savepoints stack
            self.savepoints.push(Savepoint {
                staged_files: self.staged_files.clone(),
                base_hashes: self.base_hashes.clone(),
            });
            log::info!(
                "Started nested savepoint (depth: {})",
                self.savepoints.len()
            );
            return Ok(());
        }

        // Generate a new temporary shadow directory inside the workspace's .curd directory
        // This avoids macOS /var/folders permissions errors
        let shadow_dir = self
            .workspace_root
            .join(".curd")
            .join("shadow")
            .join(format!("curd_shadow_{}", Uuid::new_v4()));

        fs::create_dir_all(&shadow_dir)?;

        // Clone the workspace into the shadow directory utilizing CoW and capturing base hashes
        let (base_hashes, shadow_meta) = self.clone_workspace_to_shadow(&shadow_dir)?;
        self.base_hashes = base_hashes;
        self.shadow_meta = shadow_meta;

        self.shadow_root = Some(shadow_dir);
        self.active = true;
        self.staged_files.clear();

        self.write_head()?;
        Ok(())
    }

    /// Recursively copies the workspace to the shadow root using `WalkBuilder` to ignore `.git`/`target`.
    /// Captures SHA-256 for conflict detection. Returns the base hashes map.
    fn clone_workspace_to_shadow(
        &self,
        shadow_dir: &Path,
    ) -> Result<(HashMap<PathBuf, String>, HashMap<PathBuf, ShadowFileMeta>)> {
        let mut base_hashes = HashMap::new();
        let mut shadow_meta = HashMap::new();
        let mut builder = WalkBuilder::new(&self.workspace_root);
        builder
            .hidden(false)
            .parents(false)
            .ignore(true)
            .git_ignore(true);
        let excludes = deps::get_excluded_dirs(&self.workspace_root);
        builder.filter_entry(move |entry| {
            let path = entry.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && (name == ".git" || name == ".curd" || excludes.contains(&name.to_string()))
            {
                return false;
            }
            true
        });

        for result in builder.build() {
            let entry = match result {
                Ok(e) => e,
                Err(e) => {
                    log::debug!("Skipping entry due to walker error: {}", e);
                    continue;
                }
            };

            let source_path = entry.path();
            if source_path.is_file() {
                let rel_path = match source_path.strip_prefix(&self.workspace_root) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let dest_path = shadow_dir.join(rel_path);
                if let Some(parent) = dest_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                // CoW copy
                if fs::copy(source_path, &dest_path).is_ok() {
                    if let Ok(meta) = fs::metadata(&dest_path) {
                        let mtime_secs = meta
                            .modified()
                            .ok()
                            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        shadow_meta.insert(
                            rel_path.to_path_buf(),
                            ShadowFileMeta {
                                size: meta.len(),
                                mtime_secs,
                            },
                        );
                    }
                    // Try to hash it for the conflict detection map
                    if let Ok(bytes) = fs::read(source_path) {
                        let mut hasher = Sha256::new();
                        hasher.update(&bytes);
                        let result = hasher.finalize();
                        base_hashes.insert(rel_path.to_path_buf(), format!("{:x}", result));
                    }
                }
            }
        }
        Ok((base_hashes, shadow_meta))
    }

    fn metadata_signature(path: &Path) -> Option<ShadowFileMeta> {
        let meta = fs::metadata(path).ok()?;
        let mtime_secs = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Some(ShadowFileMeta {
            size: meta.len(),
            mtime_secs,
        })
    }

    /// Returns true if a transaction is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn has_savepoints(&self) -> bool {
        !self.savepoints.is_empty()
    }

    /// Stage a write. Updates the file in the physical shadow workspace.
    pub fn stage(&mut self, path: &Path, new_content: &str) -> Result<()> {
        if !self.active {
            anyhow::bail!("No active transaction. Call workspace(action: 'begin') first.");
        }

        let shadow_root = self
            .shadow_root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No active shadow root for transaction stage"))?;
        // Convert to absolute path, then to relative, then map to shadow path
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        };
        let rel_path = abs_path.strip_prefix(&self.workspace_root).map_err(|_| {
            anyhow::anyhow!(
                "Refusing to stage path outside workspace root: {}",
                abs_path.display()
            )
        })?;

        let dest_path = shadow_root.join(rel_path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&dest_path, new_content)?;
        self.staged_files.insert(rel_path.to_path_buf());
        self.write_head()?;

        Ok(())
    }

    /// Returns the active shadow root, if any.
    pub fn get_shadow_root(&self) -> Option<&PathBuf> {
        self.shadow_root.as_ref()
    }

    /// Returns the UUID of the active transaction, if any.
    pub fn get_transaction_id(&self) -> Option<Uuid> {
        self.shadow_root.as_ref().and_then(|root| {
            let name = root.file_name()?.to_string_lossy();
            if name.starts_with("curd_shadow_") {
                Uuid::parse_str(&name[12..]).ok()
            } else {
                None
            }
        })
    }

    /// Mark a file as staged without modifying content in shadow.
    pub fn mark_staged(&mut self, path: &Path) -> Result<()> {
        if !self.active {
            anyhow::bail!("No active transaction. Call workspace(action: 'begin') first.");
        }
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        };
        let rel_path = abs_path.strip_prefix(&self.workspace_root).map_err(|_| {
            anyhow::anyhow!(
                "Refusing to stage path outside workspace root: {}",
                abs_path.display()
            )
        })?;
        self.staged_files.insert(rel_path.to_path_buf());
        self.write_head()?;
        Ok(())
    }

    /// Produce a unified diff of all staged or implicitly modified changes.
    pub fn diff(&mut self) -> String {
        self.discover_implicit_changes();

        if !self.active || self.staged_files.is_empty() {
            return "No staged changes.".to_string();
        }

        let mut out = String::new();
        let Some(shadow_root) = self.shadow_root.as_ref() else {
            return "No active shadow root.".to_string();
        };
        let mut paths: Vec<_> = self.staged_files.iter().collect();
        paths.sort();

        for rel_path in paths {
            let orig_path = self.workspace_root.join(rel_path);
            let shadow_path = shadow_root.join(rel_path);

            let original = fs::read_to_string(&orig_path).unwrap_or_default();
            let staged = fs::read_to_string(&shadow_path).unwrap_or_default();

            let display = rel_path.display();
            out.push_str(&format!("--- a/{display}\n+++ b/{display}\n"));
            out.push_str(&unified_diff(&original, &staged));
            out.push('\n');
        }

        out
    }

    /// Returns list of staged file paths (absolute paths in the real workspace).
    pub fn staged_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self
            .staged_files
            .iter()
            .map(|p| self.workspace_root.join(p))
            .collect();
        paths.sort();
        paths
    }

    /// Commit current changes. If in a savepoint, simply pops the stack.
    pub fn commit_savepoint(&mut self) -> Result<()> {
        if !self.active {
            anyhow::bail!("No active transaction to commit.");
        }
        if self.savepoints.pop().is_some() {
            log::info!(
                "Committed savepoint (remaining depth: {})",
                self.savepoints.len()
            );
            return Ok(());
        }
        Err(anyhow::anyhow!(
            "No active savepoint to commit. Use commit() for final transaction."
        ))
    }

    /// Commit all staged and implicitly modified changes to disk atomically.
    pub fn commit(&mut self) -> Result<Vec<PathBuf>> {
        if !self.active {
            anyhow::bail!("No active transaction to commit.");
        }

        if !self.savepoints.is_empty() {
            anyhow::bail!(
                "Cannot perform final commit while nested savepoints are active. Depth: {}",
                self.savepoints.len()
            );
        }

        self.discover_implicit_changes();

        let shadow_root = self
            .shadow_root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No active shadow root for transaction commit"))?;
        let plan_parent = self.workspace_root.join(".curd").join("tmp");
        fs::create_dir_all(&plan_parent)?;
        let plan_dir = tempfile::Builder::new()
            .prefix("curd_commit_plan_")
            .tempdir_in(plan_parent)?;
        let mut plan_files: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut written = Vec::new();

        for rel_path in &self.staged_files {
            let orig_path = self.workspace_root.join(rel_path);
            let shadow_path = shadow_root.join(rel_path);

            if shadow_path.exists() {
                if let Some(parent) = orig_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                // Conflict detection via base hash
                let mut conflict = false;
                if let Some(base_hash) = self.base_hashes.get(rel_path) {
                    // Metadata prefilter: if unchanged since begin, skip expensive hashing.
                    let base_meta = self.shadow_meta.get(rel_path);
                    let current_meta = Self::metadata_signature(&orig_path);
                    if base_meta.is_some()
                        && current_meta.is_some()
                        && base_meta == current_meta.as_ref()
                    {
                        conflict = false;
                    } else if let Ok(current_bytes) = fs::read(&orig_path) {
                        let mut hasher = Sha256::new();
                        hasher.update(&current_bytes);
                        let current_hash = format!("{:x}", hasher.finalize());
                        if current_hash != *base_hash {
                            conflict = true;
                        }
                    } else {
                        // File disappeared after begin(); treat as out-of-band change.
                        conflict = true;
                    }
                } else if orig_path.exists() {
                    // File was created in both shadow and real workspace out-of-band
                    conflict = true;
                }

                let planned_path = plan_dir.path().join(rel_path);
                if let Some(parent) = planned_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                if conflict {
                    log::warn!(
                        "Conflict detected in {}. Attempting three-way merge...",
                        rel_path.display()
                    );

                    // Reconstruct the base file by asking git for the HEAD version of this file.
                    // If unavailable (e.g. untracked file), fallback to empty base.
                    let mut base_tmp = tempfile::Builder::new()
                        .prefix("curd_base_")
                        .tempfile_in(shadow_root)?;
                    let git_show = Command::new("git")
                        .args(["show", &format!("HEAD:{}", rel_path.display())])
                        .current_dir(&self.workspace_root)
                        .output();

                    {
                        use std::io::Write;
                        match git_show {
                            Ok(output) if output.status.success() => {
                                base_tmp.write_all(&output.stdout)?;
                            }
                            _ => {
                                // Untracked/new file or no git metadata available for this path.
                                base_tmp.write_all(b"")?;
                            }
                        }
                    }

                    // Build merge result in the plan dir. Do not mutate real workspace files unless all files prepare cleanly.
                    if orig_path.exists() {
                        fs::copy(&orig_path, &planned_path)?;
                    } else {
                        fs::write(&planned_path, b"")?;
                    }

                    // Execute `git merge-file` (which natively modifies the first file argument in-place)
                    // Usage: git merge-file <current/ours> <base> <other/theirs>
                    // Here: current = planned copy of real file (human), base = git HEAD/empty fallback, other = shadow file (AI)
                    let planned_path_str = planned_path.to_string_lossy().to_string();
                    let base_path_str = base_tmp.path().to_string_lossy().to_string();
                    let shadow_path_str = shadow_path.to_string_lossy().to_string();

                    let merge_status = Command::new("git")
                        .args([
                            "merge-file",
                            "-L",
                            "Real Workspace (Human)",
                            "-L",
                            "Base (Pre-Transaction)",
                            "-L",
                            "Shadow Workspace (AI)",
                            "--",
                            &planned_path_str,
                            &base_path_str,
                            &shadow_path_str,
                        ])
                        .current_dir(&self.workspace_root)
                        .status();

                    if let Ok(status) = merge_status {
                        if !status.success() {
                            anyhow::bail!(
                                "Merge conflict detected in {}. Commit aborted before writing to real workspace.",
                                rel_path.display()
                            );
                        } else {
                            log::info!(
                                "Prepared auto-merged conflicting out-of-band changes in {}",
                                rel_path.display()
                            );
                            plan_files.push((rel_path.clone(), planned_path.clone()));
                        }
                    } else {
                        anyhow::bail!(
                            "Failed to execute git merge-file for {}. Aborting.",
                            rel_path.display()
                        );
                    }
                } else {
                    fs::copy(&shadow_path, &planned_path)?;
                    plan_files.push((rel_path.clone(), planned_path.clone()));
                }
            }
        }

        // Phase 2: apply all prepared writes only after successful planning.
        for (rel_path, planned_path) in plan_files {
            let orig_path = self.workspace_root.join(&rel_path);
            if let Some(parent) = orig_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(planned_path, &orig_path)?;
            written.push(orig_path);
        }

        self.cleanup_shadow();
        self.active = false;
        self.staged_files.clear();
        self.shadow_meta.clear();
        self.savepoints.clear();
        let _ = fs::remove_file(self.head_path());
        written.sort();
        Ok(written)
    }

    /// Scans the physical shadow workspace for files that were modified by shell tools
    /// but never explicitly `stage`d via EditEngine.
    fn discover_implicit_changes(&mut self) {
        if !self.active {
            return;
        }
        if let Some(shadow_root) = self.shadow_root.as_ref() {
            let mut builder = WalkBuilder::new(shadow_root);
            builder
                .hidden(false)
                .parents(false)
                .ignore(true)
                .git_ignore(true);
            let excludes = deps::get_excluded_dirs(&self.workspace_root);
            builder.filter_entry(move |entry| {
                let path = entry.path();
                if path.is_dir()
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && (name == ".git" || excludes.contains(&name.to_string()))
                {
                    return false;
                }
                true
            });

            for entry in builder.build().flatten() {
                let shadow_path = entry.path();
                if shadow_path.is_file()
                    && let Ok(rel_path) = shadow_path.strip_prefix(shadow_root)
                {
                    if self.staged_files.contains(rel_path) {
                        continue;
                    }
                    if let (Some(base_meta), Ok(meta)) =
                        (self.shadow_meta.get(rel_path), fs::metadata(shadow_path))
                    {
                        let curr_mtime = meta
                            .modified()
                            .ok()
                            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        if meta.len() == base_meta.size && curr_mtime == base_meta.mtime_secs {
                            continue;
                        }
                    }
                    // Check if implicitly modified
                    if let Ok(bytes) = fs::read(shadow_path) {
                        let mut hasher = Sha256::new();
                        hasher.update(&bytes);
                        let current_hash = format!("{:x}", hasher.finalize());

                        // Was it changed since we cloned it? Or is it entirely new?
                        let mut changed = true;
                        if let Some(base) = self.base_hashes.get(rel_path) {
                            log::debug!(
                                "Found base hash for {}: {} vs current: {}",
                                rel_path.display(),
                                base,
                                current_hash
                            );
                            if *base == current_hash {
                                changed = false;
                            }
                        } else {
                            // Sometimes relative paths might have different components (e.g., leading ./)
                            // depending on the Walker API. Fallback manual search:
                            let mut found = false;
                            for (base_path, base_hash) in &self.base_hashes {
                                if base_path.file_name() == rel_path.file_name() {
                                    found = true;
                                    log::debug!(
                                        "Fallback manual search found {}: {} vs current: {}",
                                        rel_path.display(),
                                        base_hash,
                                        current_hash
                                    );
                                    if *base_hash == current_hash {
                                        changed = false;
                                        break;
                                    }
                                }
                            }
                            if !found {
                                log::debug!(
                                    "No base hash found for {}, assuming implicitly changed new file.",
                                    rel_path.display()
                                );
                            }
                        }

                        if changed {
                            log::info!("Implicitly tracked changed file: {}", rel_path.display());
                            self.staged_files.insert(rel_path.to_path_buf());
                        }
                    }
                }
            }
            let _ = self.write_head();
        }
    }

    /// Discard current changes. If in a savepoint, rolls back to previous state.
    pub fn rollback(&mut self) {
        if !self.active {
            return;
        }

        if let Some(sp) = self.savepoints.pop() {
            log::info!(
                "Rolling back to savepoint (remaining depth: {})",
                self.savepoints.len()
            );
            self.staged_files = sp.staged_files;
            self.base_hashes = sp.base_hashes;
            let _ = self.write_head();
            return;
        }

        self.cleanup_shadow();
        self.active = false;
        self.staged_files.clear();
        self.shadow_meta.clear();
        self.savepoints.clear();
        let _ = fs::remove_file(self.head_path());
    }

    fn cleanup_shadow(&mut self) {
        if let Some(root) = self.shadow_root.take()
            && let Err(e) = fs::remove_dir_all(&root)
        {
            log::error!(
                "Failed to clean up shadow directory at {}: {}",
                root.display(),
                e
            );
        }
    }

    /// Number of files currently staged.
    pub fn len(&self) -> usize {
        self.staged_files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.staged_files.is_empty()
    }
}

// ── Minimal unified diff (no external crate) ────────────────────────────

fn unified_diff(original: &str, staged: &str) -> String {
    let orig_lines: Vec<&str> = original.lines().collect();
    let new_lines: Vec<&str> = staged.lines().collect();
    let mut out = String::new();
    let n = orig_lines.len();
    let m = new_lines.len();
    const MAX_DP_CELLS: usize = 4_000_000;
    if n.checked_mul(m).is_none_or(|cells| cells > MAX_DP_CELLS) {
        return simple_line_diff(&orig_lines, &new_lines);
    }

    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if orig_lines[i] == new_lines[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if orig_lines[i] == new_lines[j] {
            out.push_str(&format!(" {}\n", orig_lines[i]));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push_str(&format!("-{}\n", orig_lines[i]));
            i += 1;
        } else {
            out.push_str(&format!("+{}\n", new_lines[j]));
            j += 1;
        }
    }
    while i < n {
        out.push_str(&format!("-{}\n", orig_lines[i]));
        i += 1;
    }
    while j < m {
        out.push_str(&format!("+{}\n", new_lines[j]));
        j += 1;
    }

    out
}

fn simple_line_diff(orig_lines: &[&str], new_lines: &[&str]) -> String {
    const LOOKAHEAD: usize = 24;
    let mut out = String::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < orig_lines.len() && j < new_lines.len() {
        if orig_lines[i] == new_lines[j] {
            out.push_str(&format!(" {}\n", orig_lines[i]));
            i += 1;
            j += 1;
            continue;
        }

        // Lightweight realignment: look ahead in a small window to avoid full-file churn
        // when one side has local insertions/deletions.
        let max_i = (i + LOOKAHEAD).min(orig_lines.len());
        let max_j = (j + LOOKAHEAD).min(new_lines.len());
        let mut realigned = false;
        for ii in i + 1..max_i {
            if orig_lines[ii] == new_lines[j] {
                for line in orig_lines.iter().take(ii).skip(i) {
                    out.push_str(&format!("-{}\n", line));
                }
                i = ii;
                realigned = true;
                break;
            }
        }
        if realigned {
            continue;
        }
        for jj in j + 1..max_j {
            if new_lines[jj] == orig_lines[i] {
                for line in new_lines.iter().take(jj).skip(j) {
                    out.push_str(&format!("+{}\n", line));
                }
                j = jj;
                realigned = true;
                break;
            }
        }
        if realigned {
            continue;
        } else {
            out.push_str(&format!("-{}\n", orig_lines[i]));
            out.push_str(&format!("+{}\n", new_lines[j]));
            i += 1;
            j += 1;
        }
    }
    while i < orig_lines.len() {
        out.push_str(&format!("-{}\n", orig_lines[i]));
        i += 1;
    }
    while j < new_lines.len() {
        out.push_str(&format!("+{}\n", new_lines[j]));
        j += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_begin_stage_commit() {
        let dir = tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let file = root.join("test.py");
        fs::write(&file, "def foo():\n    return 1\n").unwrap();

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().unwrap();
        assert!(shadow.is_active());

        shadow.stage(&file, "def foo():\n    return 42\n").unwrap();
        assert_eq!(shadow.len(), 1);

        let written = shadow.commit().unwrap();
        assert_eq!(written.len(), 1);
        assert!(!shadow.is_active());

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("42"));
    }

    #[test]
    fn test_rollback_leaves_disk_unchanged() {
        let dir = tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let file = root.join("test.py");
        fs::write(&file, "original\n").unwrap();

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().unwrap();
        shadow.stage(&file, "modified\n").unwrap();
        shadow.rollback();

        assert!(!shadow.is_active());
        assert_eq!(shadow.len(), 0);
        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "original\n");
    }

    #[test]
    fn test_diff_output() {
        let dir = tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let file = root.join("test.py");
        fs::write(&file, "line1\nline2\nline3\n").unwrap();

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().unwrap();
        shadow.stage(&file, "line1\nmodified\nline3\n").unwrap();

        let diff = shadow.diff();
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn test_unified_diff_handles_prefix_insertion_cleanly() {
        let diff = super::unified_diff("a\nb\nc\n", "x\na\nb\nc\n");
        assert!(diff.contains("+x"));
        assert!(diff.contains(" a"));
        assert!(diff.contains(" b"));
        assert!(diff.contains(" c"));
    }

    #[test]
    fn test_stage_without_transaction_fails() {
        let mut shadow = ShadowStore::new(PathBuf::from("."));
        let result = shadow.stage(Path::new("foo.py"), "content");
        assert!(result.is_err());
    }

    #[test]
    fn test_stage_rejects_outside_workspace() {
        let dir = tempdir().unwrap();
        let mut shadow = ShadowStore::new(dir.path());
        shadow.begin().unwrap();

        let outside = PathBuf::from("/tmp/curd_outside_test.py");
        let result = shadow.stage(&outside, "x");
        assert!(result.is_err());
    }

    #[test]
    fn test_rollback_clears_head_file() {
        let dir = tempdir().unwrap();
        let mut shadow = ShadowStore::new(dir.path());
        shadow.begin().unwrap();
        let head = shadow.head_path();
        assert!(head.exists());

        shadow.rollback();
        assert!(!head.exists());
    }

    #[test]
    fn test_mark_staged_does_not_write_file_content() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.py");
        fs::write(&file, "original\n").unwrap();

        let mut shadow = ShadowStore::new(dir.path());
        shadow.begin().unwrap();
        shadow.mark_staged(Path::new("test.py")).unwrap();

        let shadow_file = shadow.get_shadow_root().unwrap().join("test.py");
        let _content = fs::read_to_string(shadow_file).unwrap();
        assert_eq!(_content, "original\n");
    }

    #[test]
    fn test_nested_savepoints() {
        let dir = tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let file1 = root.join("file1.txt");
        fs::write(&file1, "base").unwrap();

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().unwrap();
        shadow.stage(&file1, "state1").unwrap();
        assert_eq!(shadow.len(), 1);

        // Nested begin
        shadow.begin().unwrap();
        shadow.stage(&file1, "state2").unwrap();
        assert_eq!(shadow.len(), 1);

        // Rollback nested
        shadow.rollback();
        assert_eq!(shadow.len(), 1);
        let content =
            fs::read_to_string(shadow.get_shadow_root().unwrap().join("file1.txt")).unwrap();
        // Physical shadow file remains at the latest staged write in this simple savepoint model.
        assert_eq!(content, "state2");
    }
}
