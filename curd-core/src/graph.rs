use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::{SearchEngine, Symbol, storage::Storage, symbols::SymbolRole};
use rayon::prelude::*;

/// Directed graph modeling dependencies (calls / usages) between symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub outgoing: HashMap<String, HashSet<String>>,
    pub incoming: HashMap<String, HashSet<String>>,
    pub edge_kinds: Vec<(String, String, String)>,
    pub alias_nodes: HashSet<String>,
    pub ffi_diagnostics: Vec<Value>,
    pub fault_states: HashMap<String, FaultState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FaultState {
    Clean,
    Poisoned(Uuid),
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            edge_kinds: Vec::new(),
            alias_nodes: HashSet::new(),
            ffi_diagnostics: Vec::new(),
            fault_states: HashMap::new(),
        }
    }

    pub fn add_dependency_typed(&mut self, caller_id: &str, callee_id: &str, kind: &str) {
        self.outgoing
            .entry(caller_id.to_string())
            .or_default()
            .insert(callee_id.to_string());
        self.incoming
            .entry(callee_id.to_string())
            .or_default()
            .insert(caller_id.to_string());
        self.outgoing.entry(callee_id.to_string()).or_default();
        self.incoming.entry(caller_id.to_string()).or_default();
        
        if !self.edge_kinds.iter().any(|(f, t, k)| f == caller_id && t == callee_id && k == kind) {
            self.edge_kinds.push((caller_id.to_string(), callee_id.to_string(), kind.to_string()));
        }
    }

    pub fn add_dependency(&mut self, caller_id: &str, callee_id: &str) {
        self.add_dependency_typed(caller_id, callee_id, "calls");
    }

    pub fn ensure_node(&mut self, symbol_id: &str) {
        self.outgoing.entry(symbol_id.to_string()).or_default();
        self.incoming.entry(symbol_id.to_string()).or_default();
    }

    pub fn mark_alias(&mut self, symbol_id: &str) {
        self.alias_nodes.insert(symbol_id.to_string());
    }

    pub fn is_alias(&self, symbol_id: &str) -> bool {
        self.alias_nodes.contains(symbol_id)
    }

    pub fn get_callees(&self, symbol_id: &str) -> Vec<String> {
        self.outgoing
            .get(symbol_id)
            .map(|s| {
                let mut out: Vec<String> = s.iter().cloned().collect();
                out.sort();
                out
            })
            .unwrap_or_default()
    }

    pub fn get_callers(&self, symbol_id: &str) -> Vec<String> {
        self.incoming
            .get(symbol_id)
            .map(|s| {
                let mut out: Vec<String> = s.iter().cloned().collect();
                out.sort();
                out
            })
            .unwrap_or_default()
    }

    fn bfs_levels(&self, start: &str, direction: &str, depth: u8) -> Vec<Vec<String>> {
        if depth == 0 {
            return Vec::new();
        }
        let mut levels: Vec<Vec<String>> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, u8)> = VecDeque::new();
        queue.push_back((start.to_string(), 0));
        visited.insert(start.to_string());

        while let Some((node, lvl)) = queue.pop_front() {
            if lvl >= depth {
                continue;
            }
            let neighbors = if direction == "up" {
                self.get_callers(&node)
            } else {
                self.get_callees(&node)
            };
            let next_level = lvl + 1;
            if levels.len() < next_level as usize {
                levels.push(Vec::new());
            }
            for n in neighbors {
                if visited.insert(n.clone()) {
                    levels[(next_level - 1) as usize].push(n.clone());
                    queue.push_back((n, next_level));
                }
            }
        }
        for level in &mut levels {
            level.sort();
        }
        levels
    }

    fn edges_in_closure(
        &self,
        roots: &[String],
        up_levels: &HashMap<String, Vec<Vec<String>>>,
        down_levels: &HashMap<String, Vec<Vec<String>>>,
    ) -> Vec<(String, String)> {
        let mut nodes: HashSet<String> = HashSet::new();
        for r in roots {
            nodes.insert(r.clone());
            if let Some(levels) = up_levels.get(r) {
                for l in levels {
                    for n in l {
                        nodes.insert(n.clone());
                    }
                }
            }
            if let Some(levels) = down_levels.get(r) {
                for l in levels {
                    for n in l {
                        nodes.insert(n.clone());
                    }
                }
            }
        }
        let mut edges: Vec<(String, String)> = Vec::new();
        for from in &nodes {
            if let Some(to_set) = self.outgoing.get(from) {
                for to in to_set {
                    if nodes.contains(to) {
                        edges.push((from.clone(), to.clone()));
                    }
                }
            }
        }
        edges.sort();
        edges
    }

    fn typed_edges_in_closure(&self, edges: &[(String, String)]) -> Vec<(String, String, String)> {
        let mut typed = Vec::with_capacity(edges.len());
        for (from, to) in edges {
            let kind = self
                .edge_kinds
                .iter()
                .find(|(f, t, _)| f == from && t == to)
                .map(|(_, _, k)| k.clone())
                .unwrap_or_else(|| "calls".to_string());
            typed.push((from.clone(), to.clone(), kind));
        }
        typed.sort();
        typed
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GraphEngine {
    pub workspace_root: PathBuf,
}

impl GraphEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
        }
    }

    fn storage(&self) -> Result<Storage> {
        let cfg = crate::CurdConfig::load_from_workspace(&self.workspace_root);
        Storage::open(&self.workspace_root, &cfg)
    }

    pub fn annotate_fault(&self, symbol_id: &str, fault_id: Uuid) -> Result<()> {
        self.storage()?.annotate_symbol_fault(symbol_id, fault_id)
    }

    pub fn clear_fault(&self, symbol_id: &str) -> Result<()> {
        self.storage()?.clear_symbol_fault(symbol_id)
    }

    pub fn is_poisoned(&self, symbol_id: &str) -> bool {
        self.storage()
            .and_then(|s| s.get_symbol_fault(symbol_id))
            .ok()
            .flatten()
            .is_some()
    }

    pub fn graph(&self, uris: Vec<String>, direction: &str, depth: u8) -> Result<Value> {
        let up_depth = if direction == "up" || direction == "both" {
            depth
        } else {
            0
        };
        let down_depth = if direction == "down" || direction == "both" {
            depth
        } else {
            0
        };
        self.graph_with_depths(uris, up_depth, down_depth)
    }

    pub fn graph_with_depths(
        &self,
        uris: Vec<String>,
        up_depth: u8,
        down_depth: u8,
    ) -> Result<Value> {
        let dep_graph = self.build_dependency_graph()?;
        let mut results = Vec::new();

        for uri in uris {
            let resolved = self.resolve_uri(&dep_graph, &uri);
            let target = resolved.as_deref().unwrap_or(&uri);
            let mut entry = serde_json::Map::new();
            entry.insert("uri".to_string(), json!(uri));
            entry.insert("function_id".to_string(), json!(target));
            entry.insert("resolved".to_string(), json!(resolved.is_some()));

            let callers = dep_graph.get_callers(target);
            let callees = dep_graph.get_callees(target);
            entry.insert("callers".to_string(), json!(callers));
            entry.insert("callees".to_string(), json!(callees));

            let up_levels = dep_graph.bfs_levels(target, "up", up_depth);
            let down_levels = dep_graph.bfs_levels(target, "down", down_depth);
            entry.insert("up_levels".to_string(), json!(up_levels));
            entry.insert("down_levels".to_string(), json!(down_levels));

            results.push(Value::Object(entry));
        }

        let roots: Vec<String> = results
            .iter()
            .filter_map(|r| r.get("function_id").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        let up_index: HashMap<String, Vec<Vec<String>>> = results
            .iter()
            .filter_map(|r| {
                let id = r.get("function_id")?.as_str()?.to_string();
                let levels =
                    serde_json::from_value::<Vec<Vec<String>>>(r.get("up_levels")?.clone())
                        .unwrap_or_default();
                Some((id, levels))
            })
            .collect();
        let down_index: HashMap<String, Vec<Vec<String>>> = results
            .iter()
            .filter_map(|r| {
                let id = r.get("function_id")?.as_str()?.to_string();
                let levels =
                    serde_json::from_value::<Vec<Vec<String>>>(r.get("down_levels")?.clone())
                        .unwrap_or_default();
                Some((id, levels))
            })
            .collect();
        let edges = dep_graph.edges_in_closure(&roots, &up_index, &down_index);
        let typed_edges = dep_graph.typed_edges_in_closure(&edges);
        let ffi_edges: Vec<(String, String, String)> = typed_edges
            .iter()
            .filter(|(_, _, k)| k.starts_with("ffi_") || k == "bridge" || k == "universal_link")
            .cloned()
            .collect();
        let nodes = self.nodes_with_metadata(&dep_graph, &roots, &up_index, &down_index)?;

        Ok(json!({
            "results": results,
            "count": results.len(),
            "up_depth": up_depth,
            "down_depth": down_depth,
            "nodes": nodes,
            "edges": edges,
            "typed_edges": typed_edges,
            "ffi_edges": ffi_edges,
            "ffi_edge_count": ffi_edges.len(),
            "ffi_diagnostics": dep_graph.ffi_diagnostics
        }))
    }

    fn resolve_uri(&self, dep_graph: &DependencyGraph, uri: &str) -> Option<String> {
        if dep_graph.outgoing.contains_key(uri) || dep_graph.incoming.contains_key(uri) {
            return Some(uri.to_string());
        }
        let mut matches: Vec<String> = dep_graph
            .outgoing
            .keys()
            .chain(dep_graph.incoming.keys())
            .filter(|id| id.ends_with(uri))
            .cloned()
            .collect();
        matches.sort();
        matches.dedup();
        matches.first().cloned()
    }

    pub fn build_dependency_graph(&self) -> Result<DependencyGraph> {
        if let Some(cached) = self.load_graph_cache() {
            return Ok(cached);
        }

        let search = SearchEngine::new(&self.workspace_root);
        let symbols = search.search("", None)?;
        
        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
        let mut by_link: HashMap<String, Vec<String>> = HashMap::new();
        let mut symbol_by_id: HashMap<String, Symbol> = HashMap::new();
        
        for s in &symbols {
            by_name
                .entry(s.name.clone())
                .or_default()
                .push(s.id.clone());
            
            if let Some(link) = &s.link_name {
                by_link.entry(link.clone()).or_default().push(s.id.clone());
            }
            
            symbol_by_id.insert(s.id.clone(), s.clone());
        }

        let mut final_graph = DependencyGraph::new();

        // 1. UNIVERSAL SEMANTIC LINKER (Generic cross-language mapping)
        // Link all "Stubs" to their corresponding "Definitions"
        for s in &symbols {
            if s.role != SymbolRole::Stub {
                continue;
            }
            
            final_graph.mark_alias(&s.id);

            // Priority 1: Match by explicit link_name
            let mut resolved = false;
            if let Some(link) = &s.link_name {
                if let Some(targets) = by_link.get(link) {
                    for target_id in targets {
                        if let Some(target) = symbol_by_id.get(target_id) {
                            if target.role == SymbolRole::Definition && target.id != s.id {
                                final_graph.add_dependency_typed(&s.id, &target.id, "universal_link");
                                resolved = true;
                            }
                        }
                    }
                }
            }

            // Priority 2: Match by name
            if !resolved {
                if let Some(targets) = by_name.get(&s.name) {
                    for target_id in targets {
                        if let Some(target) = symbol_by_id.get(target_id) {
                            if target.role == SymbolRole::Definition && target.id != s.id {
                                final_graph.add_dependency_typed(&s.id, &target.id, "universal_name");
                            }
                        }
                    }
                }
            }
        }

        // 2. Intra-file heuristic mapping (name-based calls)
        let keywords: HashSet<&str> = [
            "if", "while", "for", "match", "return", "let", "fn", "impl", "mod", "use", "pub",
            "struct", "enum", "type", "with", "class", "def", "import", "from", "in", "not", "and",
            "or", "is", "assert", "del", "pass", "raise", "except", "try", "elif", "else", "loop",
            "where",
        ]
        .into_iter()
        .collect();

        let mut file_to_symbols: HashMap<PathBuf, Vec<Symbol>> = HashMap::new();
        for s in &symbols {
            file_to_symbols
                .entry(s.filepath.clone())
                .or_default()
                .push(s.clone());
        }

        let intra_edges: Vec<(String, String)> = file_to_symbols.into_par_iter().map(|(rel_path, syms)| {
            let mut edges = Vec::new();
            let file_path = if rel_path.is_absolute() {
                rel_path.clone()
            } else {
                self.workspace_root.join(&rel_path)
            };

            let source = match fs::read_to_string(&file_path) {
                Ok(src) => src,
                Err(_) => return edges,
            };

            for s in syms {
                if s.start_byte >= source.len() || s.end_byte > source.len() || s.start_byte >= s.end_byte {
                    continue;
                }

                let snippet = &source[s.start_byte..s.end_byte];
                let caller_parent = s.filepath.parent();
                let caller_ext = s.filepath.extension().and_then(|e| e.to_str()).unwrap_or_default();

                for (idx, _) in snippet.match_indices('(') {
                    if idx == 0 { continue; }
                    let name = get_name_before_paren(snippet, idx);
                    if !name.is_empty() && !keywords.contains(name) {
                        let targets = resolve_call_targets(name, &s, caller_parent, caller_ext, &by_name, &symbol_by_id);
                        for target in targets {
                            edges.push((s.id.clone(), target));
                        }
                    }
                }
            }
            edges
        }).flatten().collect();

        for (from, to) in intra_edges {
            final_graph.add_dependency(&from, &to);
        }
        
        // 3. Special Bridge mapping (Wrappers -> Core)
        self.add_cross_language_bridge_edges(&mut final_graph, &symbols, &by_name);
        
        self.save_graph_cache(&final_graph);
        Ok(final_graph)
    }

    fn nodes_with_metadata(
        &self,
        dep_graph: &DependencyGraph,
        roots: &[String],
        up_levels: &HashMap<String, Vec<Vec<String>>>,
        down_levels: &HashMap<String, Vec<Vec<String>>>,
    ) -> Result<Vec<Value>> {
        let search = SearchEngine::new(&self.workspace_root);
        let symbols = search.search("", None)?;
        let symbol_by_id: HashMap<String, Symbol> =
            symbols.into_iter().map(|s| (s.id.clone(), s)).collect();

        let mut ids: HashSet<String> = HashSet::new();
        for r in roots {
            ids.insert(r.clone());
            if let Some(levels) = up_levels.get(r) {
                for l in levels {
                    for n in l {
                        ids.insert(n.clone());
                    }
                }
            }
            if let Some(levels) = down_levels.get(r) {
                for l in levels {
                    for n in l {
                        ids.insert(n.clone());
                    }
                }
            }
        }

        let mut out = Vec::new();
        for id in ids {
            if let Some(s) = symbol_by_id.get(&id) {
                out.push(json!({
                    "id": s.id,
                    "name": s.name,
                    "kind": s.kind,
                    "role": s.role,
                    "file": s.filepath,
                    "start_line": s.start_line,
                    "end_line": s.end_line,
                    "alias": dep_graph.is_alias(&s.id),
                    "fault_state": self.storage()?.get_symbol_fault(&s.id).ok().flatten()
                }));
            } else {
                out.push(json!({
                    "id": id,
                    "name": Value::Null,
                    "kind": Value::Null,
                    "role": Value::Null,
                    "file": Value::Null,
                    "start_line": Value::Null,
                    "end_line": Value::Null,
                    "alias": Value::Bool(false)
                }));
            }
        }
        out.sort_by(|a, b| {
            a.get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .cmp(b.get("id").and_then(|v| v.as_str()).unwrap_or(""))
        });
        Ok(out)
    }

    fn add_cross_language_bridge_edges(
        &self,
        dep_graph: &mut DependencyGraph,
        symbols: &[Symbol],
        by_name: &HashMap<String, Vec<String>>,
    ) {
        for s in symbols {
            let path = s.filepath.to_string_lossy();
            let is_wrapper = path.contains("curd-node/")
                || path.contains("curd-python/")
                || path.contains("curd/");
            if !is_wrapper {
                continue;
            }
            let Some(core_targets) = by_name.get(&s.name) else {
                continue;
            };
            for target in core_targets {
                if target == &s.id {
                    continue;
                }
                if target.contains("curd-core/") {
                    dep_graph.add_dependency_typed(&s.id, target, "bridge");
                }
            }
        }
    }

    fn load_graph_cache(&self) -> Option<DependencyGraph> {
        let storage = self.storage().ok()?;
        let mut g = DependencyGraph::new();
        
        let mut stmt = storage.conn.prepare("SELECT caller_id, callee_id, kind FROM edges").ok()?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        }).ok()?;
        
        for row in rows.flatten() {
            g.add_dependency_typed(&row.0, &row.1, &row.2);
        }
        
        let mut stmt = storage.conn.prepare("SELECT id FROM symbols WHERE role = 'stub'").ok()?;
        let ids = stmt.query_map([], |row| row.get::<_, String>(0)).ok()?;
        for id in ids.flatten() {
            g.mark_alias(&id);
        }

        if g.outgoing.is_empty() && g.incoming.is_empty() {
            return None;
        }
        
        Some(g)
    }

    fn save_graph_cache(&self, g: &DependencyGraph) {
        let storage = match self.storage() {
            Ok(s) => s,
            Err(_) => return,
        };
        
        let _ = storage.conn.execute("DELETE FROM edges", []);
        let mut stmt = match storage.conn.prepare("INSERT INTO edges (caller_id, callee_id, kind) VALUES (?1, ?2, ?3)") {
            Ok(s) => s,
            Err(_) => return,
        };
        
        use rusqlite::params;
        for (from, to, kind) in &g.edge_kinds {
            let _ = stmt.execute(params![from, to, kind]);
        }
    }
}

fn get_name_before_paren(snippet: &str, idx: usize) -> &str {
    let bytes = snippet.as_bytes();
    let mut end = idx;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b':' || b == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    &snippet[start..end]
}

fn resolve_call_targets(
    called: &str,
    caller: &Symbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    by_name: &HashMap<String, Vec<String>>,
    by_id: &HashMap<String, Symbol>,
) -> Vec<String> {
    let leaf = match called.rfind(['.', ':']) {
        Some(pos) => &called[pos + 1..],
        None => called,
    };

    let Some(candidates) = by_name.get(leaf) else {
        return Vec::new();
    };
    if candidates.len() <= 1 {
        return candidates.clone();
    }

    let mut scored: Vec<(i32, String)> = Vec::with_capacity(candidates.len());
    for id in candidates {
        let Some(sym) = by_id.get(id) else {
            continue;
        };
        let mut score = 0;

        if sym.filepath == caller.filepath {
            score += 100;
        }
        if let Some(cp) = caller_parent
            && sym.filepath.parent() == Some(cp) {
                score += 20;
            }
        
        let sym_ext = sym
            .filepath
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if sym_ext == caller_ext {
            score += 10;
        }
        
        if sym.id.starts_with('@') {
            score += 50;
        }
        
        if called.contains("::") || called.contains('.') {
            if sym.id.contains(called) || sym.filepath.to_string_lossy().contains(called) {
                score += 15;
            }
        }
        scored.push((score, id.clone()));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let best = scored.first().map(|v| v.0).unwrap_or(i32::MIN);
    scored
        .into_iter()
        .filter(|(s, _)| *s == best)
        .map(|(_, id)| id)
        .collect()
}
