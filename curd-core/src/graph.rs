use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use uuid::Uuid;

use crate::{SearchEngine, Symbol};
use rayon::prelude::*;

/// Directed graph modeling dependencies (calls / usages) between symbols.
/// V: Symbol ID (String)
/// E: Weight/type of the edge (e.g., "calls", "instantiates")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub outgoing: HashMap<String, HashSet<String>>,
    pub incoming: HashMap<String, HashSet<String>>,
    pub edge_kinds: HashMap<(String, String), String>,
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
            edge_kinds: HashMap::new(),
            alias_nodes: HashSet::new(),
            ffi_diagnostics: Vec::new(),
            fault_states: HashMap::new(),
        }
    }

    /// Add a directed dependency curve: `caller_id` -> `callee_id`
    pub fn add_dependency(&mut self, caller_id: &str, callee_id: &str) {
        self.add_dependency_typed(caller_id, callee_id, "calls");
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
        self.edge_kinds
            .entry((caller_id.to_string(), callee_id.to_string()))
            .or_insert_with(|| kind.to_string());
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

    /// Get all symbols that `symbol_id` depends on (Callees)
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

    /// Get all symbols that depend on `symbol_id` (Callers)
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
                .get(&(from.clone(), to.clone()))
                .cloned()
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

/// The MCP-exposed interface for graph queries
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

    pub fn annotate_fault(&self, symbol_id: &str, fault_id: Uuid) -> Result<()> {
        let mut g = self.build_dependency_graph()?;
        g.fault_states
            .insert(symbol_id.to_string(), FaultState::Poisoned(fault_id));
        self.save_graph_cache(&g);
        Ok(())
    }

    pub fn clear_fault(&self, symbol_id: &str) -> Result<()> {
        let mut g = self.build_dependency_graph()?;
        g.fault_states.remove(symbol_id);
        self.save_graph_cache(&g);
        Ok(())
    }

    pub fn is_poisoned(&self, symbol_id: &str) -> bool {
        if let Some(g) = self.load_graph_cache() {
            matches!(g.fault_states.get(symbol_id), Some(FaultState::Poisoned(_)))
        } else {
            false
        }
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
            .filter(|(_, _, k)| k.starts_with("ffi_"))
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

    pub fn graph_tree_with_depths(
        &self,
        uris: Vec<String>,
        up_depth: u8,
        down_depth: u8,
    ) -> Result<Value> {
        let base = self.graph_with_depths(uris, up_depth, down_depth)?;
        let mut tree = Vec::new();
        if let Some(results) = base.get("results").and_then(|v| v.as_array()) {
            for r in results {
                tree.push(json!({
                    "uri": r.get("uri").cloned().unwrap_or(Value::Null),
                    "function_id": r.get("function_id").cloned().unwrap_or(Value::Null),
                    "resolved": r.get("resolved").cloned().unwrap_or(Value::Bool(false)),
                    "parents": r.get("up_levels").cloned().unwrap_or_else(|| json!([])),
                    "children": r.get("down_levels").cloned().unwrap_or_else(|| json!([]))
                }));
            }
        }
        Ok(json!({
            "view": "tree",
            "up_depth": up_depth,
            "down_depth": down_depth,
            "nodes": base.get("nodes").cloned().unwrap_or_else(|| json!([])),
            "tree": tree
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
        let mut symbol_by_id: HashMap<String, Symbol> = HashMap::new();
        for s in &symbols {
            by_name
                .entry(s.name.clone())
                .or_default()
                .push(s.id.clone());
            symbol_by_id.insert(s.id.clone(), s.clone());
        }

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

        let dep_graph_data: (Vec<(String, String)>, HashSet<String>) = file_to_symbols.into_par_iter().fold(
            || (Vec::new(), HashSet::new()),
            |(mut edges, aliases), (rel_path, syms)| {
                let file_path = if rel_path.is_absolute() {
                    rel_path.clone()
                } else {
                    self.workspace_root.join(&rel_path)
                };

                let source = match fs::read_to_string(&file_path) {
                    Ok(src) => src,
                    Err(_) => return (edges, aliases),
                };

                for s in syms {
                    if s.start_byte >= source.len()
                        || s.end_byte > source.len()
                        || s.start_byte >= s.end_byte
                    {
                        continue;
                    }

                    let snippet = &source[s.start_byte..s.end_byte];
                    let caller_parent = s.filepath.parent();
                    let caller_ext = s.filepath.extension().and_then(|e| e.to_str()).unwrap_or_default();

                    for (idx, _) in snippet.match_indices('(') {
                        if idx == 0 {
                            continue;
                        }
                        
                        let name = get_name_before_paren(snippet, idx);

                        if !name.is_empty() && !keywords.contains(name) {
                            let targets = resolve_call_targets(name, &s, caller_parent, caller_ext, &by_name, &symbol_by_id);
                            for target in targets {
                                edges.push((s.id.clone(), target));
                            }
                        }
                    }
                }
                (edges, aliases)
            },
        ).reduce(
            || (Vec::new(), HashSet::new()),
            |(mut e1, mut a1), (e2, a2)| {
                e1.extend(e2);
                a1.extend(a2);
                (e1, a1)
            }
        );

        let mut final_graph = DependencyGraph::new();
        let (edges, aliases) = dep_graph_data;
        for (from, to) in edges {
            final_graph.add_dependency(&from, &to);
        }
        for a in aliases {
            final_graph.mark_alias(&a);
        }
        
        self.add_deterministic_ffi_edges(&mut final_graph, &symbols);
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
                    "file": s.filepath,
                    "start_line": s.start_line,
                    "end_line": s.end_line,
                    "alias": dep_graph.is_alias(&s.id),
                    "fault_state": dep_graph.fault_states.get(&s.id)
                }));
            } else {
                out.push(json!({
                    "id": id,
                    "name": Value::Null,
                    "kind": Value::Null,
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

    fn add_deterministic_ffi_edges(&self, dep_graph: &mut DependencyGraph, symbols: &[Symbol]) {
        let infos = self.collect_ffi_infos(symbols);
        let mut defs_by_name: HashMap<&str, Vec<&FfiInfo>> = HashMap::new();
        for info in &infos {
            if info.is_definition {
                defs_by_name
                    .entry(info.name.as_str())
                    .or_default()
                    .push(info);
            }
        }

        for info in &infos {
            if !info.is_stub {
                continue;
            }
            dep_graph.mark_alias(&info.id);

            if info.abi.as_deref() != Some("C") {
                dep_graph.ffi_diagnostics.push(json!({
                    "severity":"warning",
                    "code":"ffi_unsupported_abi",
                    "message": format!("Unsupported FFI ABI for {}: {:?}", info.id, info.abi),
                    "from": info.id
                }));
                continue;
            }
            if info.has_generics || info.is_variadic {
                dep_graph.ffi_diagnostics.push(json!({
                    "severity":"warning",
                    "code":"ffi_unsupported_signature",
                    "message": format!("FFI edge skipped for {} due to generics/variadic signature", info.id),
                    "from": info.id
                }));
                continue;
            }

            let candidates = defs_by_name
                .get(info.name.as_str())
                .cloned()
                .unwrap_or_default();
            if candidates.is_empty() {
                dep_graph.ffi_diagnostics.push(json!({
                    "severity":"warning",
                    "code":"ffi_missing_definition",
                    "message": format!("No FFI definition found for {}", info.id),
                    "from": info.id
                }));
                continue;
            }

            let matches: Vec<&FfiInfo> = candidates
                .into_iter()
                .filter(|d| d.id != info.id)
                .filter(|d| d.abi.as_deref() == Some("C"))
                .filter(|d| d.param_count == info.param_count)
                .collect();

            if matches.is_empty() {
                dep_graph.ffi_diagnostics.push(json!({
                    "severity":"high",
                    "code":"ffi_mismatch",
                    "message": format!(
                        "FFI signature mismatch for {} (name={}, abi=C, params={})",
                        info.id, info.name, info.param_count
                    ),
                    "from": info.id
                }));
                continue;
            }

            let mut sorted: Vec<&FfiInfo> = matches;
            sorted.sort_by(|a, b| a.id.cmp(&b.id));
            for def in sorted {
                let edge_kind = format!("ffi_static_local:{}->{}", info.language, def.language);
                dep_graph.add_dependency_typed(&info.id, &def.id, &edge_kind);
            }
        }

        dep_graph.ffi_diagnostics.sort_by(|a, b| {
            let ac = a.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let bc = b.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let af = a.get("from").and_then(|v| v.as_str()).unwrap_or("");
            let bf = b.get("from").and_then(|v| v.as_str()).unwrap_or("");
            ac.cmp(bc).then_with(|| af.cmp(bf))
        });
    }

    fn collect_ffi_infos(&self, symbols: &[Symbol]) -> Vec<FfiInfo> {
        let mut infos = Vec::new();
        for s in symbols {
            let file_path = if s.filepath.is_absolute() {
                s.filepath.clone()
            } else {
                self.workspace_root.join(&s.filepath)
            };
            let Ok(source) = fs::read_to_string(&file_path) else {
                continue;
            };
            if s.start_byte >= source.len()
                || s.end_byte > source.len()
                || s.start_byte >= s.end_byte
            {
                continue;
            }
            let snippet = source[s.start_byte..s.end_byte].to_string();
            let language = language_from_path(&s.filepath);
            let info = FfiInfo::from_symbol(s, &snippet, &language);
            if info.is_stub || info.is_definition {
                infos.push(info);
            }
        }
        infos.sort_by(|a, b| a.id.cmp(&b.id));
        infos
    }

    fn add_cross_language_bridge_edges(
        &self,
        dep_graph: &mut DependencyGraph,
        symbols: &[Symbol],
        by_name: &HashMap<String, Vec<String>>,
    ) {
        // Cross-language bridge heuristic:
        // wrapper layer symbols (node/python bindings, cli) -> core symbols by same exported name.
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
}

#[derive(Debug, Clone)]
struct FfiInfo {
    id: String,
    name: String,
    language: String,
    is_stub: bool,
    is_definition: bool,
    abi: Option<String>,
    param_count: usize,
    is_variadic: bool,
    has_generics: bool,
}

impl FfiInfo {
    fn from_symbol(symbol: &Symbol, snippet: &str, language: &str) -> Self {
        let parsed = match language {
            "rust" => parse_rust_ffi(symbol, snippet),
            "go" => parse_go_ffi(symbol, snippet),
            "java" => parse_java_ffi(symbol, snippet),
            "c" | "cpp" => parse_c_family_ffi(symbol, snippet, language),
            _ => parse_generic_foreign(symbol, snippet, language),
        };

        Self {
            id: symbol.id.clone(),
            name: symbol.name.clone(),
            language: language.to_string(),
            is_stub: parsed.is_stub,
            is_definition: parsed.is_definition,
            abi: parsed.abi,
            param_count: parsed.param_count,
            is_variadic: parsed.is_variadic,
            has_generics: parsed.has_generics,
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedFfi {
    is_stub: bool,
    is_definition: bool,
    abi: Option<String>,
    param_count: usize,
    is_variadic: bool,
    has_generics: bool,
}

fn parse_rust_ffi(_symbol: &Symbol, snippet: &str) -> ParsedFfi {
    let (param_count, is_variadic) = parse_param_shape(snippet);
    let is_stub = snippet.contains("extern \"C\"") && snippet.contains("fn ");
    ParsedFfi {
        is_stub,
        is_definition: false,
        abi: if is_stub { Some("C".to_string()) } else { None },
        param_count,
        is_variadic,
        has_generics: snippet.contains('<') && snippet.contains('>'),
    }
}

fn parse_go_ffi(_symbol: &Symbol, snippet: &str) -> ParsedFfi {
    let (param_count, is_variadic) = parse_param_shape(snippet);
    let lower = snippet.to_lowercase();
    let cgo_stub = snippet.contains("C.") && snippet.contains('(');
    let linkname_stub = lower.contains("go:linkname");
    let is_stub = cgo_stub || linkname_stub;
    ParsedFfi {
        is_stub,
        is_definition: false,
        abi: if is_stub { Some("C".to_string()) } else { None },
        param_count,
        is_variadic,
        has_generics: false,
    }
}

fn parse_java_ffi(_symbol: &Symbol, snippet: &str) -> ParsedFfi {
    let (param_count, is_variadic) = parse_param_shape(snippet);
    let lower = snippet.to_lowercase();
    let is_stub = lower.contains(" native ") || lower.contains("native ");
    ParsedFfi {
        is_stub,
        is_definition: false,
        abi: if is_stub { Some("C".to_string()) } else { None },
        param_count,
        is_variadic,
        has_generics: false,
    }
}

fn parse_c_family_ffi(_symbol: &Symbol, snippet: &str, language: &str) -> ParsedFfi {
    let (param_count, is_variadic) = parse_param_shape(snippet);
    let lower = snippet.to_lowercase();
    let is_definition = snippet.contains('(')
        && snippet.contains(')')
        && (!snippet.contains(';') || snippet.contains('{'))
        && (snippet.contains('{') || lower.contains("java_"));
    ParsedFfi {
        is_stub: false,
        is_definition,
        abi: if is_definition {
            Some("C".to_string())
        } else {
            Some(language.to_uppercase())
        },
        param_count,
        is_variadic,
        has_generics: false,
    }
}

fn parse_generic_foreign(_symbol: &Symbol, snippet: &str, _language: &str) -> ParsedFfi {
    let (param_count, is_variadic) = parse_param_shape(snippet);
    let lower = snippet.to_lowercase();
    let is_stub =
        lower.contains("extern ") || lower.contains("foreign ") || lower.contains("native ");
    ParsedFfi {
        is_stub,
        is_definition: false,
        abi: if is_stub {
            Some("unknown".to_string())
        } else {
            None
        },
        param_count,
        is_variadic,
        has_generics: false,
    }
}

fn parse_param_shape(snippet: &str) -> (usize, bool) {
    let Some(lp) = snippet.find('(') else {
        return (0, false);
    };
    let Some(rp) = snippet[lp + 1..].find(')') else {
        return (0, false);
    };
    let body = &snippet[lp + 1..lp + 1 + rp];
    let trimmed = body.trim();
    if trimmed.is_empty() || trimmed == "void" {
        return (0, false);
    }
    let variadic = trimmed.contains("...");
    let count = trimmed
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && *p != "...")
        .count();
    (count, variadic)
}

fn language_from_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "rs" => "rust".to_string(),
        "c" | "h" => "c".to_string(),
        "cc" | "cpp" | "hpp" => "cpp".to_string(),
        "go" => "go".to_string(),
        "java" => "java".to_string(),
        "py" => "python".to_string(),
        "js" | "ts" => "javascript".to_string(),
        _ => "unknown".to_string(),
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GraphCache {
    source_index_mtime_secs: u64,
    nodes: Vec<String>,
    edges: Vec<(String, String)>,
    #[serde(default)]
    typed_edges: Vec<(String, String, String)>,
    #[serde(default)]
    alias_nodes: Vec<String>,
    #[serde(default)]
    ffi_diagnostics: Vec<Value>,
    #[serde(default)]
    fault_states: HashMap<String, FaultState>,
}

impl GraphEngine {
    fn graph_cache_path(&self) -> PathBuf {
        self.workspace_root.join(".curd").join("graph_index.json")
    }

    fn symbol_index_mtime_secs(&self) -> u64 {
        let idx = self.workspace_root.join(".curd").join("symbol_index.json");
        fs::metadata(idx)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn load_graph_cache(&self) -> Option<DependencyGraph> {
        let path = self.graph_cache_path();
        let bytes = fs::read(path).ok()?;
        let cache: GraphCache = bincode::deserialize(&bytes).ok()?;
        if cache.source_index_mtime_secs != self.symbol_index_mtime_secs() {
            return None;
        }
        let mut g = DependencyGraph::new();
        g.fault_states = cache.fault_states;
        g.ffi_diagnostics = cache.ffi_diagnostics;
        for n in cache.alias_nodes {
            g.alias_nodes.insert(n);
        }
        for n in cache.nodes {
            g.ensure_node(&n);
        }
        if !cache.typed_edges.is_empty() {
            for (a, b, kind) in cache.typed_edges {
                g.add_dependency_typed(&a, &b, &kind);
            }
        } else {
            for (a, b) in cache.edges {
                g.add_dependency(&a, &b);
            }
        }
        Some(g)
    }

    fn save_graph_cache(&self, g: &DependencyGraph) {
        let mut nodes: Vec<String> = g
            .outgoing
            .keys()
            .chain(g.incoming.keys())
            .cloned()
            .collect();
        nodes.sort();
        nodes.dedup();
        let mut edges = Vec::new();
        for (from, tos) in &g.outgoing {
            for to in tos {
                edges.push((from.clone(), to.clone()));
            }
        }
        edges.sort();
        let mut typed_edges = Vec::new();
        for ((from, to), kind) in &g.edge_kinds {
            typed_edges.push((from.clone(), to.clone(), kind.clone()));
        }
        typed_edges.sort();
        let mut alias_nodes: Vec<String> = g.alias_nodes.iter().cloned().collect();
        alias_nodes.sort();
        let cache = GraphCache {
            source_index_mtime_secs: self.symbol_index_mtime_secs(),
            nodes,
            edges,
            typed_edges,
            alias_nodes,
            ffi_diagnostics: g.ffi_diagnostics.clone(),
            fault_states: g.fault_states.clone(),
        };
        let path = self.graph_cache_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = bincode::serialize(&cache) {
            let _ = fs::write(path, bytes);
        }
    }
}

fn get_name_before_paren(snippet: &str, idx: usize) -> &str {
    let bytes = snippet.as_bytes();
    let mut end = idx;
    // Skip whitespace backward
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    // Scan backward for identifier characters
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
    // Find the leaf name (e.g. "func" from "mod::func") efficiently
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
        
        // Optimize: skip extension call-string generation
        let sym_ext = sym
            .filepath
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if sym_ext == caller_ext {
            score += 10;
        }
        
        if called.contains("::") || called.contains('.') {
            // Check if 'called' matches parts of the symbol ID or path without allocation
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_bfs_levels_up_down() {
        let mut g = DependencyGraph::new();
        g.add_dependency("a", "b");
        g.add_dependency("b", "c");
        g.add_dependency("x", "a");

        let down = g.bfs_levels("a", "down", 2);
        assert_eq!(down[0], vec!["b".to_string()]);
        assert_eq!(down[1], vec!["c".to_string()]);

        let up = g.bfs_levels("a", "up", 1);
        assert_eq!(up[0], vec!["x".to_string()]);
    }

    #[test]
    fn test_graph_cache_stale_when_symbol_index_mismatch() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".curd")).unwrap();
        std::fs::write(root.join(".curd/symbol_index.json"), "{}").unwrap();

        let cache = GraphCache {
            source_index_mtime_secs: 0,
            nodes: vec!["a".to_string()],
            edges: vec![("a".to_string(), "b".to_string())],
            typed_edges: vec![],
            alias_nodes: vec![],
            ffi_diagnostics: vec![],
            fault_states: HashMap::new(),
        };
        std::fs::write(
            root.join(".curd/graph_index.json"),
            bincode::serialize(&cache).unwrap(),
        )
        .unwrap();

        let engine = GraphEngine::new(root);
        assert!(engine.load_graph_cache().is_none());
    }

    #[test]
    fn test_graph_cache_loads_when_fresh() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".curd")).unwrap();
        std::fs::write(root.join(".curd/symbol_index.json"), "{}").unwrap();
        let engine = GraphEngine::new(root);
        let mtime = engine.symbol_index_mtime_secs();
        let cache = GraphCache {
            source_index_mtime_secs: mtime,
            nodes: vec!["a".to_string(), "b".to_string()],
            edges: vec![("a".to_string(), "b".to_string())],
            typed_edges: vec![],
            alias_nodes: vec![],
            ffi_diagnostics: vec![],
            fault_states: HashMap::new(),
        };
        std::fs::write(
            root.join(".curd/graph_index.json"),
            bincode::serialize(&cache).unwrap(),
        )
        .unwrap();
        let loaded = engine.load_graph_cache();
        assert!(loaded.is_some());
        let g = loaded.unwrap();
        assert_eq!(g.get_callees("a"), vec!["b".to_string()]);
    }

    #[test]
    fn test_resolve_call_targets_prefers_same_file() {
        let caller = Symbol {
            id: "src/a.py::foo".into(),
            filepath: PathBuf::from("src/a.py"),
            name: "foo".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: 1,
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };
        let c1 = Symbol {
            id: "src/a.py::bar".into(),
            filepath: PathBuf::from("src/a.py"),
            name: "bar".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: 1,
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };
        let c2 = Symbol {
            id: "src/other.py::bar".into(),
            filepath: PathBuf::from("src/other.py"),
            name: "bar".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: 1,
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };
        let mut by_name = HashMap::new();
        by_name.insert("bar".to_string(), vec![c1.id.clone(), c2.id.clone()]);
        let mut by_id = HashMap::new();
        by_id.insert(c1.id.clone(), c1);
        by_id.insert(c2.id.clone(), c2);

        let picked = resolve_call_targets("bar", &caller, caller.filepath.parent(), caller.filepath.extension().and_then(|s| s.to_str()).unwrap_or(""), &by_name, &by_id);
        assert_eq!(picked, vec!["src/a.py::bar".to_string()]);
    }

    #[test]
    fn test_cross_language_bridge_edges() {
        let engine = GraphEngine::new(".");
        let wrapper = Symbol {
            id: "curd-python/src/lib.rs::search".into(),
            filepath: PathBuf::from("curd-python/src/lib.rs"),
            name: "search".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: 1,
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };
        let core = Symbol {
            id: "curd-core/src/search.rs::search".into(),
            filepath: PathBuf::from("curd-core/src/search.rs"),
            name: "search".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: 1,
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };
        let symbols = vec![wrapper.clone(), core.clone()];
        let mut by_name = HashMap::new();
        by_name.insert(
            "search".to_string(),
            vec![wrapper.id.clone(), core.id.clone()],
        );
        let mut g = DependencyGraph::new();
        g.ensure_node(&wrapper.id);
        g.ensure_node(&core.id);
        engine.add_cross_language_bridge_edges(&mut g, &symbols, &by_name);
        assert_eq!(g.get_callees(&wrapper.id), vec![core.id]);
    }

    #[test]
    fn test_deterministic_ffi_static_edge_and_alias() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "extern \"C\" { fn foo(x: i32) -> i32; }\n",
        )
        .unwrap();
        std::fs::write(root.join("src/native.c"), "int foo(int x) { return x; }\n").unwrap();

        let stub = Symbol {
            id: "src/lib.rs::foo".into(),
            filepath: PathBuf::from("src/lib.rs"),
            name: "foo".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: "extern \"C\" { fn foo(x: i32) -> i32; }\n".len(),
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };
        let def = Symbol {
            id: "src/native.c::foo".into(),
            filepath: PathBuf::from("src/native.c"),
            name: "foo".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: "int foo(int x) { return x; }\n".len(),
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };

        let engine = GraphEngine::new(root);
        let mut g = DependencyGraph::new();
        g.ensure_node(&stub.id);
        g.ensure_node(&def.id);
        engine.add_deterministic_ffi_edges(&mut g, &[stub.clone(), def.clone()]);
        assert!(g.is_alias(&stub.id));
        let callee = g.get_callees(&stub.id);
        assert_eq!(callee, vec![def.id]);
    }

    #[test]
    fn test_deterministic_ffi_mismatch_diagnostic() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "extern \"C\" { fn bar(x: i32, y: i32) -> i32; }\n",
        )
        .unwrap();
        std::fs::write(root.join("src/native.c"), "int bar(int x) { return x; }\n").unwrap();

        let stub = Symbol {
            id: "src/lib.rs::bar".into(),
            filepath: PathBuf::from("src/lib.rs"),
            name: "bar".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: "extern \"C\" { fn bar(x: i32, y: i32) -> i32; }\n".len(),
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };
        let def = Symbol {
            id: "src/native.c::bar".into(),
            filepath: PathBuf::from("src/native.c"),
            name: "bar".into(),
            kind: crate::SymbolKind::Function,
            start_byte: 0,
            end_byte: "int bar(int x) { return x; }\n".len(),
            start_line: 1,
            end_line: 1,
            signature: None,
            semantic_hash: String::new(),
        };

        let engine = GraphEngine::new(root);
        let mut g = DependencyGraph::new();
        g.ensure_node(&stub.id);
        g.ensure_node(&def.id);
        engine.add_deterministic_ffi_edges(&mut g, &[stub.clone(), def.clone()]);
        assert!(g.get_callees(&stub.id).is_empty());
        assert!(
            g.ffi_diagnostics
                .iter()
                .any(|d| d.get("code").and_then(|v| v.as_str()) == Some("ffi_mismatch"))
        );
    }
}
