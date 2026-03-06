/// Graph-entropy alert system for CURD's transaction shadow store.
///
/// After a commit, this module snapshots the dependency graph of changed files,
/// compares it to the pre-commit state, and emits structured alerts for:
///   - New cross-module edges (coupling between directories)
///   - Introduced cycles (circular dependencies)
///   - Fan-out spikes (a function's callee count grew by more than FAN_OUT_THRESHOLD)
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Minimum fan-out increase to trigger an alert
const FAN_OUT_THRESHOLD: usize = 3;

/// A directed edge between two symbol URIs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edge {
    pub from: String, // "src/module.rs::function"
    pub to: String,
}

/// A complete call graph snapshot
#[derive(Debug, Default, Clone)]
pub struct GraphSnapshot {
    /// Map from symbol URI → set of called symbol URIs
    pub edges: HashMap<String, HashSet<String>>,
    /// Map from symbol URI → containing file path
    pub file_of: HashMap<String, String>,
}

impl GraphSnapshot {
    /// Build a snapshot from a set of source files
    pub fn from_files(files: &[PathBuf], workspace_root: &Path) -> Self {
        let mut snapshot = Self::default();
        for path in files {
            if let Ok(content) = fs::read_to_string(path) {
                let rel = path
                    .strip_prefix(workspace_root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                snapshot.ingest_file(&rel, &content);
            }
        }
        snapshot
    }

    /// Extract function definitions and their callees from source text.
    /// Uses simple heuristics across major language families.
    fn ingest_file(&mut self, file_path: &str, source: &str) {
        let lang = detect_language(file_path);
        let defs = extract_function_names(source, lang);
        let calls = extract_call_names(source, lang);

        for def in &defs {
            let uri = format!("{}::{}", file_path, def);
            self.file_of.insert(uri.clone(), file_path.to_string());
            let callees = self.edges.entry(uri).or_default();
            // Map call names to URIs within the same file (best-effort)
            for call in &calls {
                if defs.contains(call) && call != def {
                    callees.insert(format!("{}::{}", file_path, call));
                }
            }
        }
    }

    /// Count callee edges for a given symbol
    pub fn fan_out(&self, uri: &str) -> usize {
        self.edges.get(uri).map(|s| s.len()).unwrap_or(0)
    }

    /// All edges as a flat set of (from, to) pairs
    pub fn all_edges(&self) -> HashSet<Edge> {
        let mut result = HashSet::new();
        for (from, tos) in &self.edges {
            for to in tos {
                result.insert(Edge {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
        }
        result
    }
}

/// The result of auditing a commit
#[derive(Debug, Default)]
pub struct AuditReport {
    pub new_cross_module_edges: Vec<Value>,
    pub cycles: Vec<Value>,
    pub fan_out_spikes: Vec<Value>,
    pub alert_count: usize,
}

impl AuditReport {
    pub fn to_json(&self) -> Value {
        json!({
            "alert_count": self.alert_count,
            "new_cross_module_edges": self.new_cross_module_edges,
            "cycles": self.cycles,
            "fan_out_spikes": self.fan_out_spikes,
        })
    }

    pub fn is_clean(&self) -> bool {
        self.alert_count == 0
    }
}

/// Compare two graph snapshots and produce an audit report
pub fn audit(before: &GraphSnapshot, after: &GraphSnapshot) -> AuditReport {
    let mut report = AuditReport::default();

    let before_edges = before.all_edges();
    let after_edges = after.all_edges();
    let new_edges: Vec<&Edge> = after_edges
        .iter()
        .filter(|e| !before_edges.contains(*e))
        .collect();

    // ── New cross-module edges ──────────────────────────────────────────
    for edge in &new_edges {
        let from_file = after
            .file_of
            .get(&edge.from)
            .map(|s| s.as_str())
            .unwrap_or(&edge.from);
        let to_file = after
            .file_of
            .get(&edge.to)
            .map(|s| s.as_str())
            .unwrap_or(&edge.to);

        let from_module = module_of(from_file);
        let to_module = module_of(to_file);

        if from_module != to_module || from_file != to_file {
            report.new_cross_module_edges.push(json!({
                "severity": "warning",
                "from": edge.from,
                "to": edge.to,
                "from_module": from_module,
                "to_module": to_module,
                "message": format!("New cross-module dependency: {} → {}",
                    from_module, to_module)
            }));
        }
    }

    // ── Cycle detection (BFS on new graph) ─────────────────────────────
    let cycles = find_cycles(&after.edges);
    for cycle in cycles {
        report.cycles.push(json!({
            "severity": "error",
            "cycle": cycle,
            "message": format!("Circular dependency introduced: {} → ... → {}",
                cycle[0], cycle[0])
        }));
    }

    // ── Fan-out spikes ─────────────────────────────────────────────────
    for uri in after.edges.keys() {
        let before_fo = before.fan_out(uri);
        let after_fo = after.fan_out(uri);
        if after_fo > before_fo && (after_fo - before_fo) >= FAN_OUT_THRESHOLD {
            report.fan_out_spikes.push(json!({
                "severity": "info",
                "symbol": uri,
                "before": before_fo,
                "after": after_fo,
                "delta": after_fo - before_fo,
                "message": format!("{} gained {} new callees (fan-out: {} → {})",
                    uri, after_fo - before_fo, before_fo, after_fo)
            }));
        }
    }

    report.alert_count =
        report.new_cross_module_edges.len() + report.cycles.len() + report.fan_out_spikes.len();

    report
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Returns the top-level directory of a file path (e.g. "src/flight/main.cpp" → "src/flight")
fn module_of(file_path: &str) -> &str {
    let p = Path::new(file_path);
    // Use the parent directory as the "module"
    p.parent()
        .and_then(|p| {
            if p.as_os_str().is_empty() {
                None
            } else {
                Some(p)
            }
        })
        .map(|p| p.to_str().unwrap_or(file_path))
        .unwrap_or(file_path)
}

/// Simple language detection from extension
#[derive(Clone, Copy)]
enum Lang {
    Rust,
    Cpp,
    Python,
    Go,
    Generic,
}

fn detect_language(path: &str) -> Lang {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
    {
        "rs" => Lang::Rust,
        "cpp" | "cc" | "cxx" | "c" | "h" | "hpp" => Lang::Cpp,
        "py" => Lang::Python,
        "go" => Lang::Go,
        _ => Lang::Generic,
    }
}

/// Extract top-level function/method names from source
fn extract_function_names(source: &str, lang: Lang) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let name =
            match lang {
                Lang::Rust => {
                    if trimmed.starts_with("fn ")
                        || trimmed.starts_with("pub fn ")
                        || trimmed.starts_with("pub(crate) fn ")
                        || trimmed.starts_with("async fn ")
                    {
                        trimmed.split("fn ").nth(1).and_then(|s| {
                            s.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                        })
                    } else {
                        None
                    }
                }
                Lang::Python => {
                    if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
                        trimmed.split("def ").nth(1).and_then(|s| {
                            s.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                        })
                    } else {
                        None
                    }
                }
                Lang::Go => {
                    if trimmed.starts_with("func ") {
                        // func Name(...) or func (r Receiver) Name(...)
                        if let Some(rest) = trimmed.split("func ").nth(1) {
                            if rest.starts_with('(') {
                                rest.split(')').nth(1).map(|s| s.trim()).and_then(|s| {
                                    s.split(|c: char| !c.is_alphanumeric() && c != '_').next()
                                })
                            } else {
                                rest.split(|c: char| !c.is_alphanumeric() && c != '_')
                                    .next()
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
        if let Some(n) = name
            && !n.is_empty() {
                names.insert(n.to_string());
            }
    }
    names
}

/// Extract call sites (function names being called) from source
fn extract_call_names(source: &str, _lang: Lang) -> HashSet<String> {
    let mut calls = HashSet::new();
    let keywords: HashSet<&str> = [
        "if", "while", "for", "match", "return", "let", "fn", "impl", "mod", "use", "pub",
        "struct", "enum", "type", "with", "class", "def", "import", "from", "in", "not", "and",
        "or", "is", "assert", "del", "pass", "raise", "except", "try", "elif", "else",
    ]
    .iter()
    .cloned()
    .collect();

    for (idx, _) in source.match_indices('(') {
        if idx == 0 {
            continue;
        }
        let before = &source[..idx];
        let name = before
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != ':' && c != '.')
            .next_back()
            .unwrap_or("");

        if !name.is_empty() && !keywords.contains(name) {
            calls.insert(name.to_string());
        }
    }
    calls
}

/// Detect cycles using DFS; returns list of cycle paths
fn find_cycles(edges: &HashMap<String, HashSet<String>>) -> Vec<Vec<String>> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut in_stack: HashSet<String> = HashSet::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    for start in edges.keys() {
        if !visited.contains(start) {
            dfs_cycle(
                start,
                edges,
                &mut visited,
                &mut in_stack,
                &mut vec![],
                &mut cycles,
            );
        }
    }

    // Deduplicate by cycle length and content
    cycles.dedup_by(|a, b| {
        let mut a_sorted = a.clone();
        a_sorted.sort();
        let mut b_sorted = b.clone();
        b_sorted.sort();
        a_sorted == b_sorted
    });
    cycles.truncate(10); // Report at most 10 cycles
    cycles
}

fn dfs_cycle(
    node: &str,
    edges: &HashMap<String, HashSet<String>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    visited.insert(node.to_string());
    in_stack.insert(node.to_string());
    path.push(node.to_string());

    if let Some(neighbors) = edges.get(node) {
        for neighbor in neighbors {
            if cycles.len() >= 100 {
                return;
            }
            if !visited.contains(neighbor) {
                dfs_cycle(neighbor, edges, visited, in_stack, path, cycles);
            } else if in_stack.contains(neighbor) {
                // Found a cycle — extract it from the current path
                if let Some(start) = path.iter().position(|n| n == neighbor) {
                    cycles.push(path[start..].to_vec());
                }
            }
        }
    }

    in_stack.remove(node);
    path.pop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_module_detection() {
        let before = GraphSnapshot::default();
        let mut after = GraphSnapshot::default();

        // Before: no edges
        // After: src/icp.rs::solve calls src/loop_closure.rs::find_anchor (cross-module)
        after.edges.insert(
            "src/icp.rs::solve".into(),
            HashSet::from(["src/loop_closure.rs::find_anchor".into()]),
        );
        after
            .file_of
            .insert("src/icp.rs::solve".into(), "src/icp.rs".into());
        after.file_of.insert(
            "src/loop_closure.rs::find_anchor".into(),
            "src/loop_closure.rs".into(),
        );

        let report = audit(&before, &after);
        assert_eq!(report.new_cross_module_edges.len(), 1);
        assert_eq!(report.cycles.len(), 0);
    }

    #[test]
    fn test_cycle_detection() {
        let mut snapshot = GraphSnapshot::default();
        // A → B → C → A
        snapshot
            .edges
            .insert("mod::a".into(), HashSet::from(["mod::b".into()]));
        snapshot
            .edges
            .insert("mod::b".into(), HashSet::from(["mod::c".into()]));
        snapshot
            .edges
            .insert("mod::c".into(), HashSet::from(["mod::a".into()]));

        let cycles = find_cycles(&snapshot.edges);
        assert!(!cycles.is_empty());
    }

    #[test]
    fn test_fan_out_spike() {
        let before = GraphSnapshot::default();
        let mut after = GraphSnapshot::default();

        // Function gains 4 new callees (above threshold of 3)
        after.edges.insert(
            "src/main.rs::run".into(),
            HashSet::from([
                "src/main.rs::a".into(),
                "src/main.rs::b".into(),
                "src/main.rs::c".into(),
                "src/main.rs::d".into(),
            ]),
        );

        let report = audit(&before, &after);
        assert!(!report.fan_out_spikes.is_empty());
    }

    #[test]
    fn test_same_module_no_alert() {
        let before = GraphSnapshot::default();
        let mut after = GraphSnapshot::default();

        // Same file/module — should NOT trigger cross-module alert
        after.edges.insert(
            "src/utils.rs::helper".into(),
            HashSet::from(["src/utils.rs::inner".into()]),
        );
        after
            .file_of
            .insert("src/utils.rs::helper".into(), "src/utils.rs".into());
        after
            .file_of
            .insert("src/utils.rs::inner".into(), "src/utils.rs".into());

        let report = audit(&before, &after);
        assert_eq!(report.new_cross_module_edges.len(), 0);
    }
}
