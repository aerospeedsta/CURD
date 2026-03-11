use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::{SearchEngine, Symbol, SymbolKind, storage::Storage, symbols::SymbolRole};
use rayon::prelude::*;

/// Directed graph modeling dependencies (calls / usages) between symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub outgoing: HashMap<String, HashSet<String>>,
    pub incoming: HashMap<String, HashSet<String>>,
    pub edge_kinds: Vec<(String, String, String)>,
    pub edge_metadata: HashMap<String, EdgeMetadata>,
    pub alias_nodes: HashSet<String>,
    pub ffi_diagnostics: Vec<Value>,
    pub fault_states: HashMap<String, FaultState>,
    pub merkle_root: Option<String>,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeMetadata {
    pub tier: String,
    pub confidence: f64,
    pub source: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

const FALLBACK_STUB_LINK_SOURCE: &str = "fallback:stub_link";
const FALLBACK_STUB_NAME_SOURCE: &str = "fallback:stub_name";
const FALLBACK_CALL_SCAN_SOURCE: &str = "fallback:call_scan";
const FALLBACK_IMPORT_SCAN_SOURCE: &str = "fallback:import_scan";
const FALLBACK_CONTAINS_SOURCE: &str = "fallback:contains";
const FALLBACK_DECLARES_SOURCE: &str = "fallback:declares";
const FALLBACK_OWNS_MEMBER_SOURCE: &str = "fallback:owns_member";
const FALLBACK_BRIDGE_SOURCE: &str = "fallback:bridge";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FaultState {
    Clean,
    Poisoned(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphIntegrityReport {
    pub cohesion_ratio: f64,
    pub total_stubs: usize,
    pub resolved_stubs: usize,
    pub symbol_density: f64,
    pub confidence_distribution: HashMap<String, usize>, // "high", "medium", "low" buckets
    pub cycles: Vec<Vec<String>>,
}

impl DependencyGraph {
    pub fn calculate_integrity(&self, symbols: &[Symbol]) -> GraphIntegrityReport {
        let mut total_stubs = 0;
        let mut resolved_stubs = 0;
        let mut high = 0;
        let mut med = 0;
        let mut low = 0;

        for s in symbols {
            if s.role == SymbolRole::Stub {
                total_stubs += 1;
                if self.outgoing.contains_key(&s.id) && !self.outgoing[&s.id].is_empty() {
                    resolved_stubs += 1;
                }
            }
        }

        for meta in self.edge_metadata.values() {
            if meta.confidence >= 0.90 {
                high += 1;
            } else if meta.confidence >= 0.70 {
                med += 1;
            } else {
                low += 1;
            }
        }

        let total_lines: usize = symbols.iter().map(|s| s.end_line - s.start_line + 1).sum();
        let symbol_density = if total_lines > 0 {
            symbols.len() as f64 / (total_lines as f64 / 1000.0)
        } else {
            0.0
        };

        let mut dist = HashMap::new();
        dist.insert("high".to_string(), high);
        dist.insert("medium".to_string(), med);
        dist.insert("low".to_string(), low);

        GraphIntegrityReport {
            cohesion_ratio: if total_stubs > 0 {
                resolved_stubs as f64 / total_stubs as f64
            } else {
                1.0
            },
            total_stubs,
            resolved_stubs,
            symbol_density,
            confidence_distribution: dist,
            cycles: self.find_cycles(),
        }
    }

    pub fn find_cycles(&self) -> Vec<Vec<String>> {
        use petgraph::algo::tarjan_scc;
        use petgraph::graph::DiGraph;

        let mut graph = DiGraph::<usize, ()>::new();
        let mut id_to_node = HashMap::new();
        let mut node_to_id = HashMap::new();

        let all_ids: HashSet<&String> = self.outgoing.keys().chain(self.incoming.keys()).collect();

        for (idx, &id) in all_ids.iter().enumerate() {
            let node = graph.add_node(idx);
            id_to_node.insert(id.clone(), node);
            node_to_id.insert(node, id.clone());
        }

        for (caller, callees) in &self.outgoing {
            if let Some(&u) = id_to_node.get(caller) {
                for callee in callees {
                    if let Some(&v) = id_to_node.get(callee) {
                        graph.add_edge(u, v, ());
                    }
                }
            }
        }

        tarjan_scc(&graph)
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|scc| {
                scc.into_iter()
                    .filter_map(|node| node_to_id.get(&node).cloned())
                    .collect()
            })
            .collect()
    }
    pub fn new() -> Self {
        Self {
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            edge_kinds: Vec::new(),
            edge_metadata: HashMap::new(),
            alias_nodes: HashSet::new(),
            ffi_diagnostics: Vec::new(),
            fault_states: HashMap::new(),
            merkle_root: None,
            origin: "empty".to_string(),
        }
    }

    pub fn compute_merkle_root(&mut self, symbols: &[Symbol]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();

        // 1. Sort symbols by ID to ensure deterministic hashing
        let mut sorted_symbols = symbols.to_vec();
        sorted_symbols.sort_by(|a, b| a.id.cmp(&b.id));

        for s in &sorted_symbols {
            hasher.update(s.id.as_bytes());
            if let Some(hash) = &s.semantic_hash {
                hasher.update(hash.as_bytes());
            }
        }

        // 2. Sort edges to ensure deterministic hashing
        let mut sorted_edges = self.edge_kinds.clone();
        sorted_edges.sort();

        for (from, to, kind) in &sorted_edges {
            hasher.update(from.as_bytes());
            hasher.update(to.as_bytes());
            hasher.update(kind.as_bytes());
        }

        let root = format!("{:x}", hasher.finalize());
        self.merkle_root = Some(root.clone());
        root
    }

    pub fn add_dependency_typed(&mut self, caller_id: &str, callee_id: &str, kind: &str) {
        self.add_dependency_typed_with_metadata(
            caller_id,
            callee_id,
            kind,
            default_edge_metadata(kind),
        );
    }

    pub fn add_dependency_typed_with_metadata(
        &mut self,
        caller_id: &str,
        callee_id: &str,
        kind: &str,
        meta: EdgeMetadata,
    ) {
        if caller_id == callee_id {
            return;
        }
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

        if !self
            .edge_kinds
            .iter()
            .any(|(f, t, k)| f == caller_id && t == callee_id && k == kind)
        {
            self.edge_kinds.push((
                caller_id.to_string(),
                callee_id.to_string(),
                kind.to_string(),
            ));
        }
        self.edge_metadata
            .insert(edge_key(caller_id, callee_id, kind), meta);
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

    fn detailed_edges_in_closure(&self, edges: &[(String, String)]) -> Vec<Value> {
        let mut detailed = Vec::new();
        for (from, to) in edges {
            let kinds: Vec<String> = self
                .edge_kinds
                .iter()
                .filter(|(f, t, _)| f == from && t == to)
                .map(|(_, _, k)| k.clone())
                .collect();
            let kinds = if kinds.is_empty() {
                vec!["calls".to_string()]
            } else {
                kinds
            };
            for kind in kinds {
                let meta = self
                    .edge_metadata
                    .get(&edge_key(from, to, &kind))
                    .cloned()
                    .unwrap_or(EdgeMetadata {
                        tier: default_edge_metadata(&kind).tier,
                        confidence: default_edge_metadata(&kind).confidence,
                        source: default_edge_metadata(&kind).source,
                        evidence: default_edge_metadata(&kind).evidence,
                    });
                detailed.push(json!({
                    "from": from,
                    "to": to,
                    "kind": kind,
                    "tier": meta.tier,
                    "confidence": meta.confidence,
                    "source": meta.source,
                    "evidence": meta.evidence,
                }));
            }
        }
        detailed.sort_by(|a, b| {
            a.get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .cmp(b.get("from").and_then(|v| v.as_str()).unwrap_or(""))
                .then_with(|| {
                    a.get("to")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .cmp(b.get("to").and_then(|v| v.as_str()).unwrap_or(""))
                })
                .then_with(|| {
                    a.get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .cmp(b.get("kind").and_then(|v| v.as_str()).unwrap_or(""))
                })
        });
        detailed
    }

    fn edge_summary(&self, edges: &[Value]) -> Value {
        let mut by_kind: HashMap<String, usize> = HashMap::new();
        let mut by_tier: HashMap<String, usize> = HashMap::new();
        let mut by_source: HashMap<String, usize> = HashMap::new();
        for edge in edges {
            if let Some(kind) = edge.get("kind").and_then(|v| v.as_str()) {
                *by_kind.entry(kind.to_string()).or_default() += 1;
            }
            if let Some(tier) = edge.get("tier").and_then(|v| v.as_str()) {
                *by_tier.entry(tier.to_string()).or_default() += 1;
            }
            let source = edge
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            *by_source.entry(source.to_string()).or_default() += 1;
        }
        json!({
            "by_kind": by_kind,
            "by_tier": by_tier,
            "by_source": by_source,
        })
    }

    fn filter_detailed_edges(
        &self,
        edges: Vec<Value>,
        min_confidence: Option<f64>,
        tiers: Option<&HashSet<String>>,
        sources: Option<&HashSet<String>>,
    ) -> Vec<Value> {
        edges
            .into_iter()
            .filter(|edge| {
                let confidence = edge
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if let Some(min) = min_confidence
                    && confidence < min
                {
                    return false;
                }
                if let Some(tiers) = tiers {
                    let tier = edge.get("tier").and_then(|v| v.as_str()).unwrap_or("");
                    if !tiers.contains(tier) {
                        return false;
                    }
                }
                if let Some(sources) = sources {
                    let source = edge
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    if !sources.contains(source) {
                        return false;
                    }
                }
                true
            })
            .collect()
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

    pub fn storage(&self) -> Result<Storage> {
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
        self.graph_with_filters(uris, up_depth, down_depth, None, None, None)
    }

    pub fn graph_with_depths(
        &self,
        uris: Vec<String>,
        up_depth: u8,
        down_depth: u8,
    ) -> Result<Value> {
        self.graph_with_filters(uris, up_depth, down_depth, None, None, None)
    }

    pub fn graph_with_filters(
        &self,
        uris: Vec<String>,
        up_depth: u8,
        down_depth: u8,
        min_confidence: Option<f64>,
        tiers: Option<HashSet<String>>,
        sources: Option<HashSet<String>>,
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
        let closure_edges = dep_graph.edges_in_closure(&roots, &up_index, &down_index);
        let detailed_edges = dep_graph.filter_detailed_edges(
            dep_graph.detailed_edges_in_closure(&closure_edges),
            min_confidence,
            tiers.as_ref(),
            sources.as_ref(),
        );
        let edges: Vec<(String, String)> = detailed_edges
            .iter()
            .filter_map(|edge| {
                Some((
                    edge.get("from")?.as_str()?.to_string(),
                    edge.get("to")?.as_str()?.to_string(),
                ))
            })
            .collect();
        let typed_edges: Vec<(String, String, String)> = detailed_edges
            .iter()
            .filter_map(|edge| {
                Some((
                    edge.get("from")?.as_str()?.to_string(),
                    edge.get("to")?.as_str()?.to_string(),
                    edge.get("kind")?.as_str()?.to_string(),
                ))
            })
            .collect();
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
            "filters": {
                "min_confidence": min_confidence,
                "tiers": tiers.as_ref().map(|s| {
                    let mut values: Vec<_> = s.iter().cloned().collect();
                    values.sort();
                    values
                }),
                "sources": sources.as_ref().map(|s| {
                    let mut values: Vec<_> = s.iter().cloned().collect();
                    values.sort();
                    values
                })
            },
            "graph_origin": dep_graph.origin,
            "nodes": nodes,
            "edges": edges,
            "typed_edges": typed_edges,
            "detailed_edges": detailed_edges,
            "edge_summary": dep_graph.edge_summary(&detailed_edges),
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
            .filter(|id| {
                id.ends_with(uri)
                    && (id.len() == uri.len()
                        || id.get(id.len() - uri.len() - 2..id.len() - uri.len()) == Some("::"))
            })
            .cloned()
            .collect();
        matches.sort();
        matches.dedup();
        matches.first().cloned()
    }

    pub fn build_dependency_graph(&self) -> Result<DependencyGraph> {
        self.build_dependency_graph_inner(true)
    }

    pub fn build_dependency_graph_fresh(&self) -> Result<DependencyGraph> {
        self.build_dependency_graph_inner(false)
    }

    fn build_dependency_graph_inner(&self, use_cache: bool) -> Result<DependencyGraph> {
        if use_cache && let Some(cached) = self.load_graph_cache() {
            return Ok(cached);
        }

        let search = SearchEngine::new(&self.workspace_root);
        search.ensure_index()?;
        let symbols = search.get_all_symbols()?;

        if use_cache && let Some(cached) = self.load_graph_cache() {
            return Ok(cached);
        }

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
        final_graph.origin = "fallback".to_string();

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
                                final_graph.add_dependency_typed_with_metadata(
                                    &s.id,
                                    &target.id,
                                    "universal_link",
                                    fallback_edge_metadata("universal_link"),
                                );
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
                                final_graph.add_dependency_typed_with_metadata(
                                    &s.id,
                                    &target.id,
                                    "universal_name",
                                    fallback_edge_metadata("universal_name"),
                                );
                            }
                        }
                    }
                }
            }
        }

        for (from, to, confidence, evidence) in resolve_trait_realization_edges(&symbols) {
            let mut meta = fallback_edge_metadata_with_confidence("realizes", confidence);
            meta.evidence = evidence;
            final_graph.add_dependency_typed_with_metadata(&from, &to, "realizes", meta);
        }

        for (from, to, confidence, evidence) in resolve_containment_edges(&symbols) {
            let mut meta = fallback_edge_metadata_with_confidence("contains", confidence);
            meta.evidence = evidence;
            final_graph.add_dependency_typed_with_metadata(&from, &to, "contains", meta);
        }
        for (from, to, confidence, evidence) in resolve_module_declaration_edges(&symbols) {
            let mut meta = fallback_edge_metadata_with_confidence("declares", confidence);
            meta.evidence = evidence;
            final_graph.add_dependency_typed_with_metadata(&from, &to, "declares", meta);
        }
        for (from, to, confidence, evidence) in resolve_member_ownership_edges(&symbols) {
            let mut meta = fallback_edge_metadata_with_confidence("owns_member", confidence);
            meta.evidence = evidence;
            final_graph.add_dependency_typed_with_metadata(&from, &to, "owns_member", meta);
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

        let intra_edges: Vec<(String, String, String, f64, Vec<String>)> = file_to_symbols
            .into_par_iter()
            .map(|(rel_path, syms)| {
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
                let import_targets = resolve_import_edges(&source, &syms, &by_name, &symbol_by_id);
                for s in &syms {
                    if s.role != SymbolRole::Definition {
                        continue;
                    }
                    for (target, confidence, evidence) in &import_targets {
                        if target != &s.id {
                            edges.push((
                                s.id.clone(),
                                target.clone(),
                                "imports".to_string(),
                                *confidence,
                                evidence.clone(),
                            ));
                        }
                    }
                }

                for s in syms {
                    if s.start_byte >= source.len()
                        || s.end_byte > source.len()
                        || s.start_byte >= s.end_byte
                    {
                        continue;
                    }

                    let snippet = &source[s.start_byte..s.end_byte];
                    let caller_parent = s.filepath.parent();
                    let caller_ext = s
                        .filepath
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or_default();

                    for (idx, _) in snippet.match_indices('(') {
                        if idx == 0 {
                            continue;
                        }
                        let name = get_name_before_paren(snippet, idx);
                        if !name.is_empty() && !keywords.contains(name) {
                            let targets = resolve_call_targets(
                                name,
                                &s,
                                caller_parent,
                                caller_ext,
                                &by_name,
                                &symbol_by_id,
                            );
                            for (target, confidence) in targets {
                                // Deduplicate: Don't link to aliases if the primary definition exists
                                if target.contains("::#") {
                                    let primary = target.split("::#").next().unwrap_or(&target);
                                    if symbol_by_id.contains_key(primary) {
                                        edges.push((
                                            s.id.clone(),
                                            primary.to_string(),
                                            "calls".to_string(),
                                            confidence,
                                            fallback_call_edge_evidence(
                                                name,
                                                &s,
                                                caller_parent,
                                                caller_ext,
                                                symbol_by_id.get(primary).unwrap_or(&s),
                                                true,
                                            ),
                                        ));
                                        continue;
                                    }
                                }
                                let evidence = symbol_by_id
                                    .get(&target)
                                    .map(|sym| {
                                        fallback_call_edge_evidence(
                                            name,
                                            &s,
                                            caller_parent,
                                            caller_ext,
                                            sym,
                                            true,
                                        )
                                    })
                                    .unwrap_or_else(|| {
                                        vec!["fallback".to_string(), "call_scan".to_string()]
                                    });
                                edges.push((
                                    s.id.clone(),
                                    target,
                                    "calls".to_string(),
                                    confidence,
                                    evidence,
                                ));
                            }
                        }
                    }
                }
                edges
            })
            .flatten()
            .collect();

        for (from, to, kind, confidence, evidence) in intra_edges {
            let mut meta = fallback_edge_metadata_with_confidence(&kind, confidence);
            meta.evidence = evidence;
            final_graph.add_dependency_typed_with_metadata(&from, &to, &kind, meta);
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

        let mut requested_ids: Vec<String> = ids.iter().cloned().collect();
        requested_ids.sort();
        let storage = self.storage()?;
        let symbol_by_id: HashMap<String, Symbol> = storage
            .get_symbols_by_ids(&requested_ids)?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();

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
                    "fault_state": s.fault_id
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
                    dep_graph.add_dependency_typed_with_metadata(
                        &s.id,
                        target,
                        "bridge",
                        fallback_edge_metadata("bridge"),
                    );
                }
            }
        }
    }

    fn load_graph_cache(&self) -> Option<DependencyGraph> {
        let storage = self.storage().ok()?;
        let mut g = DependencyGraph::new();
        g.origin = "indexed".to_string();

        let mut stmt = storage.conn.prepare("SELECT caller_id, callee_id, kind, tier, confidence, source, COALESCE(evidence, '[]') FROM edges").ok()?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .ok()?;

        for row in rows.flatten() {
            g.add_dependency_typed_with_metadata(
                &row.0,
                &row.1,
                &row.2,
                EdgeMetadata {
                    tier: row.3,
                    confidence: row.4,
                    source: row.5,
                    evidence: serde_json::from_str(&row.6).unwrap_or_default(),
                },
            );
        }

        let mut stmt = storage
            .conn
            .prepare("SELECT id FROM symbols WHERE role = 'stub'")
            .ok()?;
        let ids = stmt.query_map([], |row| row.get::<_, String>(0)).ok()?;
        for id in ids.flatten() {
            g.mark_alias(&id);
        }

        let symbol_count: i64 = storage
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
            .ok()?;
        if symbol_count == 0 {
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
        let mut stmt = match storage.conn.prepare("INSERT INTO edges (caller_id, callee_id, kind, tier, confidence, source, evidence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)") {
            Ok(s) => s,
            Err(_) => return,
        };

        use rusqlite::params;
        for (from, to, kind) in &g.edge_kinds {
            let meta = g
                .edge_metadata
                .get(&edge_key(from, to, kind))
                .cloned()
                .unwrap_or(EdgeMetadata {
                    tier: default_edge_metadata(kind).tier,
                    confidence: default_edge_metadata(kind).confidence,
                    source: Some(g.origin.clone()),
                    evidence: default_edge_evidence(kind, &g.origin),
                });
            let evidence =
                serde_json::to_string(&meta.evidence).unwrap_or_else(|_| "[]".to_string());
            let _ = stmt.execute(params![
                from,
                to,
                kind,
                meta.tier,
                meta.confidence,
                meta.source,
                evidence
            ]);
        }
    }
}

fn edge_key(from: &str, to: &str, kind: &str) -> String {
    format!("{from}\n{to}\n{kind}")
}

fn default_edge_metadata(kind: &str) -> EdgeMetadata {
    EdgeMetadata {
        tier: if kind == "calls" {
            "semantic".to_string()
        } else {
            "structural".to_string()
        },
        confidence: if kind == "calls" { 0.70 } else { 0.90 },
        source: None,
        evidence: Vec::new(),
    }
}

fn fallback_edge_metadata(kind: &str) -> EdgeMetadata {
    fallback_edge_metadata_with_confidence(kind, default_edge_metadata(kind).confidence)
}

fn fallback_edge_metadata_with_confidence(kind: &str, confidence: f64) -> EdgeMetadata {
    let mut meta = default_edge_metadata(kind);
    meta.confidence = confidence;
    let source = match kind {
        "universal_link" => FALLBACK_STUB_LINK_SOURCE,
        "universal_name" => FALLBACK_STUB_NAME_SOURCE,
        "contains" => FALLBACK_CONTAINS_SOURCE,
        "declares" => FALLBACK_DECLARES_SOURCE,
        "owns_member" => FALLBACK_OWNS_MEMBER_SOURCE,
        "bridge" => FALLBACK_BRIDGE_SOURCE,
        "imports" => FALLBACK_IMPORT_SCAN_SOURCE,
        _ => FALLBACK_CALL_SCAN_SOURCE,
    }
    .to_string();
    meta.evidence = default_edge_evidence(kind, &source);
    meta.source = Some(source);
    meta
}

fn default_edge_evidence(kind: &str, source: &str) -> Vec<String> {
    let mut evidence = vec![source.to_string()];
    match kind {
        "universal_link" => evidence.push("stub_link_match".to_string()),
        "universal_name" => evidence.push("stub_name_match".to_string()),
        "contains" => evidence.push("nested_span".to_string()),
        "declares" => evidence.push("module_declaration".to_string()),
        "owns_member" => evidence.push("member_ownership".to_string()),
        "bridge" => evidence.push("cross_language_bridge".to_string()),
        "imports" => evidence.push("import_scan".to_string()),
        "calls" => evidence.push("call_scan".to_string()),
        _ => evidence.push(format!("kind:{kind}")),
    }
    evidence
}

fn resolve_import_edges(
    source: &str,
    file_symbols: &[Symbol],
    by_name: &HashMap<String, Vec<String>>,
    symbol_by_id: &HashMap<String, Symbol>,
) -> Vec<(String, f64, Vec<String>)> {
    let Some(anchor) = file_symbols
        .iter()
        .find(|sym| sym.role == SymbolRole::Definition)
        .or_else(|| file_symbols.first())
    else {
        return Vec::new();
    };
    let caller_parent = anchor.filepath.parent();
    let caller_ext = anchor
        .filepath
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let mut out = Vec::new();
    for import_name in extract_import_names(source) {
        out.extend(resolve_import_targets(
            &import_name,
            anchor,
            caller_parent,
            caller_ext,
            by_name,
            symbol_by_id,
        ));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

fn resolve_trait_realization_edges(symbols: &[Symbol]) -> Vec<(String, String, f64, Vec<String>)> {
    let mut out = Vec::new();
    for s in symbols {
        if s.name.contains(" impl ") || s.name.contains(" for ") {
            // Heuristic for Rust trait impls: "impl Trait for Struct"
            let parts: Vec<&str> = s.name.split(" for ").collect();
            if parts.len() == 2 {
                let trait_name = parts[0].trim_start_matches("impl ").trim();
                let struct_name = parts[1].trim();

                // Find potential trait definition
                for target in symbols {
                    if target.name == trait_name && target.kind == SymbolKind::Interface {
                        out.push((
                            s.id.clone(),
                            target.id.clone(),
                            0.95,
                            vec![format!(
                                "Trait realization: {} realizes {}",
                                s.id, target.id
                            )],
                        ));
                    }
                    if target.name == struct_name
                        && matches!(target.kind, SymbolKind::Struct | SymbolKind::Class)
                    {
                        out.push((
                            s.id.clone(),
                            target.id.clone(),
                            0.95,
                            vec![format!(
                                "Structural implementation: {} implements {}",
                                s.id, target.id
                            )],
                        ));
                    }
                }
            }
        }
    }
    out
}

fn resolve_containment_edges(symbols: &[Symbol]) -> Vec<(String, String, f64, Vec<String>)> {
    let mut edges = Vec::new();
    for child in symbols {
        if child.role != SymbolRole::Definition {
            continue;
        }
        let parent = symbols
            .iter()
            .filter(|parent| {
                parent.id != child.id
                    && parent.role == SymbolRole::Definition
                    && is_container_kind(parent.kind.clone())
                    && parent.filepath == child.filepath
                    && child.start_byte > parent.start_byte
                    && child.end_byte <= parent.end_byte
            })
            .min_by_key(|parent| parent.end_byte.saturating_sub(parent.start_byte));
        let Some(parent) = parent else {
            continue;
        };
        edges.push((
            parent.id.clone(),
            child.id.clone(),
            containment_confidence(parent, child),
            containment_evidence(parent, child),
        ));
    }
    edges.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    edges.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    edges
}

fn is_container_kind(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Module | SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface
    )
}

fn containment_confidence(parent: &Symbol, child: &Symbol) -> f64 {
    let mut confidence = 0.90_f64;
    if parent.kind == SymbolKind::Module {
        confidence += 0.02;
    }
    if child.kind == SymbolKind::Method || child.kind == SymbolKind::Function {
        confidence += 0.03;
    }
    confidence.clamp(0.90, 0.98)
}

fn containment_evidence(parent: &Symbol, child: &Symbol) -> Vec<String> {
    let mut evidence = vec![
        "fallback".to_string(),
        "nested_span".to_string(),
        "same_file".to_string(),
        format!("parent_kind:{:?}", parent.kind).to_lowercase(),
        format!("child_kind:{:?}", child.kind).to_lowercase(),
    ];
    if parent.kind == SymbolKind::Module {
        evidence.push("module_scope".to_string());
    }
    evidence
}

fn resolve_module_declaration_edges(symbols: &[Symbol]) -> Vec<(String, String, f64, Vec<String>)> {
    resolve_containment_edges(symbols)
        .into_iter()
        .filter_map(|(parent, child, confidence, mut evidence)| {
            let parent_sym = symbols.iter().find(|sym| sym.id == parent)?;
            if parent_sym.kind != SymbolKind::Module {
                return None;
            }
            evidence.push("module_declaration".to_string());
            Some((parent, child, confidence.clamp(0.93, 0.99), evidence))
        })
        .collect()
}

fn resolve_member_ownership_edges(symbols: &[Symbol]) -> Vec<(String, String, f64, Vec<String>)> {
    resolve_containment_edges(symbols)
        .into_iter()
        .filter_map(|(parent, child, confidence, mut evidence)| {
            let parent_sym = symbols.iter().find(|sym| sym.id == parent)?;
            let child_sym = symbols.iter().find(|sym| sym.id == child)?;
            if !matches!(
                parent_sym.kind,
                SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface
            ) {
                return None;
            }
            if !matches!(child_sym.kind, SymbolKind::Method | SymbolKind::Function) {
                return None;
            }
            evidence.push("member_ownership".to_string());
            evidence.push(format!("owner_kind:{:?}", parent_sym.kind).to_lowercase());
            Some((parent, child, confidence.clamp(0.94, 0.99), evidence))
        })
        .collect()
}

fn resolve_import_targets(
    imported: &str,
    caller: &Symbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    by_name: &HashMap<String, Vec<String>>,
    symbol_by_id: &HashMap<String, Symbol>,
) -> Vec<(String, f64, Vec<String>)> {
    let leaf = match imported.rfind(['.', ':', '/']) {
        Some(pos) => &imported[pos + 1..],
        None => imported,
    };
    let Some(candidates) = by_name.get(leaf) else {
        return Vec::new();
    };
    let ambiguous = candidates.len() > 1;
    candidates
        .iter()
        .filter_map(|id| {
            let target = symbol_by_id.get(id)?;
            Some((
                id.clone(),
                score_import_confidence(
                    imported,
                    caller,
                    caller_parent,
                    caller_ext,
                    target,
                    !ambiguous,
                ),
                fallback_import_edge_evidence(
                    imported,
                    caller,
                    caller_parent,
                    caller_ext,
                    target,
                    !ambiguous,
                ),
            ))
        })
        .collect()
}

fn score_import_confidence(
    imported: &str,
    _caller: &Symbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    target: &Symbol,
    unique_best: bool,
) -> f64 {
    let mut confidence = 0.60_f64;
    if let Some(cp) = caller_parent
        && target.filepath.parent() == Some(cp)
    {
        confidence += 0.10;
    }
    let target_ext = target
        .filepath
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if target_ext == caller_ext {
        confidence += 0.06;
    }
    if imported.contains("::") || imported.contains('.') || imported.contains('/') {
        confidence += 0.10;
        if target.id.contains(imported) || target.filepath.to_string_lossy().contains(imported) {
            confidence += 0.08;
        }
    }
    if unique_best {
        confidence += 0.08;
    } else {
        confidence -= 0.08;
    }
    confidence.clamp(0.35, 0.94)
}

fn fallback_call_edge_evidence(
    called: &str,
    caller: &Symbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    target: &Symbol,
    unique_best: bool,
) -> Vec<String> {
    let mut evidence = vec!["fallback".to_string(), "call_scan".to_string()];
    if target.filepath == caller.filepath {
        evidence.push("same_file".to_string());
    }
    if let Some(cp) = caller_parent
        && target.filepath.parent() == Some(cp)
    {
        evidence.push("same_parent".to_string());
    }
    let target_ext = target
        .filepath
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if target_ext == caller_ext {
        evidence.push("same_extension".to_string());
    }
    if called.contains("::") || called.contains('.') {
        evidence.push("qualified_call".to_string());
        if target.id.contains(called) || target.filepath.to_string_lossy().contains(called) {
            evidence.push("qualified_match".to_string());
        }
    }
    if unique_best {
        evidence.push("unique_best".to_string());
    } else {
        evidence.push("ambiguous_best".to_string());
    }
    evidence
}

fn fallback_import_edge_evidence(
    imported: &str,
    _caller: &Symbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    target: &Symbol,
    unique_best: bool,
) -> Vec<String> {
    let mut evidence = vec!["fallback".to_string(), "import_scan".to_string()];
    if let Some(cp) = caller_parent
        && target.filepath.parent() == Some(cp)
    {
        evidence.push("same_parent".to_string());
    }
    let target_ext = target
        .filepath
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if target_ext == caller_ext {
        evidence.push("same_extension".to_string());
    }
    if imported.contains("::") || imported.contains('.') || imported.contains('/') {
        evidence.push("qualified_import".to_string());
        if target.id.contains(imported) || target.filepath.to_string_lossy().contains(imported) {
            evidence.push("qualified_match".to_string());
        }
    }
    if unique_best {
        evidence.push("unique_best".to_string());
    } else {
        evidence.push("ambiguous_best".to_string());
    }
    evidence
}

fn extract_import_names(source: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("use ") {
            imports.extend(parse_rust_use_clause(rest));
        } else if let Some(rest) = trimmed.strip_prefix("pub use ") {
            imports.extend(parse_rust_use_clause(rest));
        } else if let Some(rest) = trimmed.strip_prefix("import ") {
            imports.extend(parse_import_clause(rest));
        } else if let Some(rest) = trimmed.strip_prefix("from ")
            && let Some((module, names)) = rest.split_once(" import ")
        {
            imports.extend(parse_from_import_clause(module, names));
        } else if let Some(rest) = trimmed.strip_prefix("#include") {
            let include = rest.trim().trim_matches(&['<', '>', '"'][..]);
            if let Some(name) = include
                .rsplit_once('/')
                .map(|(_, tail)| tail)
                .or(Some(include))
            {
                let leaf = name.split('.').next().unwrap_or(name).trim();
                if !leaf.is_empty() {
                    imports.push(leaf.to_string());
                }
            }
        }
    }
    imports.sort();
    imports.dedup();
    imports
}

fn parse_rust_use_clause(rest: &str) -> Vec<String> {
    let clause = rest.trim().trim_end_matches(';');
    if clause.contains('{') && clause.contains('}') {
        let prefix = clause
            .split('{')
            .next()
            .unwrap_or("")
            .trim_end_matches("::");
        let inside = clause
            .split('{')
            .nth(1)
            .unwrap_or("")
            .split('}')
            .next()
            .unwrap_or("");
        inside
            .split(',')
            .filter_map(|entry| {
                let entry = entry.trim();
                if entry.is_empty() || entry == "self" {
                    return None;
                }
                let leaf = entry.split_whitespace().next().unwrap_or(entry);
                Some(format!("{prefix}::{leaf}"))
            })
            .collect()
    } else {
        vec![
            clause
                .split_whitespace()
                .next()
                .unwrap_or(clause)
                .trim()
                .to_string(),
        ]
    }
}

fn parse_import_clause(rest: &str) -> Vec<String> {
    if let Some((head, _)) = rest.split_once(" from ") {
        return parse_import_bindings(head);
    }
    parse_import_bindings(rest)
}

fn parse_import_bindings(bindings: &str) -> Vec<String> {
    bindings
        .trim()
        .trim_end_matches(';')
        .trim_matches(|c| c == '{' || c == '}')
        .split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            let entry = entry
                .split_whitespace()
                .next()
                .unwrap_or(entry)
                .trim_matches(|c| c == '{' || c == '}');
            let leaf = entry
                .rsplit_once('.')
                .map(|(_, tail)| tail)
                .unwrap_or(entry)
                .trim();
            if leaf.is_empty() || leaf == "*" {
                None
            } else {
                Some(leaf.to_string())
            }
        })
        .collect()
}

fn parse_from_import_clause(module: &str, names: &str) -> Vec<String> {
    let module = module.trim();
    names
        .trim()
        .trim_end_matches(';')
        .trim_matches(|c| c == '(' || c == ')')
        .split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() || entry == "*" {
                return None;
            }
            let leaf = entry.split_whitespace().next().unwrap_or(entry);
            Some(format!("{module}.{leaf}"))
        })
        .collect()
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
) -> Vec<(String, f64)> {
    let leaf = match called.rfind(['.', ':']) {
        Some(pos) => &called[pos + 1..],
        None => called,
    };

    let Some(candidates) = by_name.get(leaf) else {
        return Vec::new();
    };
    if candidates.len() <= 1 {
        return candidates
            .iter()
            .filter_map(|id| {
                let sym = by_id.get(id)?;
                Some((
                    id.clone(),
                    score_call_confidence(called, caller, caller_parent, caller_ext, sym, true),
                ))
            })
            .collect();
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
            && sym.filepath.parent() == Some(cp)
        {
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
    let best_ids: Vec<String> = scored
        .into_iter()
        .filter(|(s, _)| *s == best)
        .map(|(_, id)| id)
        .collect();
    let ambiguous = best_ids.len() > 1;
    best_ids
        .into_iter()
        .filter_map(|id| {
            let sym = by_id.get(&id)?;
            Some((
                id,
                score_call_confidence(called, caller, caller_parent, caller_ext, sym, !ambiguous),
            ))
        })
        .collect()
}

fn score_call_confidence(
    called: &str,
    caller: &Symbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    target: &Symbol,
    unique_best: bool,
) -> f64 {
    let mut confidence = 0.58_f64;
    if target.filepath == caller.filepath {
        confidence += 0.18;
    }
    if let Some(cp) = caller_parent
        && target.filepath.parent() == Some(cp)
    {
        confidence += 0.07;
    }
    let target_ext = target
        .filepath
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if target_ext == caller_ext {
        confidence += 0.04;
    }
    if target.id.starts_with('@') {
        confidence += 0.03;
    }
    if called.contains("::") || called.contains('.') {
        confidence += 0.08;
        if target.id.contains(called) || target.filepath.to_string_lossy().contains(called) {
            confidence += 0.05;
        }
    }
    if unique_best {
        confidence += 0.08;
    } else {
        confidence -= 0.10;
    }
    confidence.clamp(0.35, 0.92)
}

#[cfg(test)]
mod tests {
    use super::GraphEngine;
    use crate::search::IndexWorkerEntry;
    use crate::storage::Storage;
    use crate::{CurdConfig, Symbol, SymbolKind, symbols::SymbolRole};
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn graph_uses_indexed_edge_metadata() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(
            root.join("src/lib.rs"),
            "fn callee() {}\nfn caller() { callee(); }\n",
        )
        .expect("write source");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");
        storage
            .commit_indexing_results(&[IndexWorkerEntry {
                rel: "src/lib.rs".to_string(),
                mtime_ms: 1,
                file_size: 40,
                symbols: vec![
                    Symbol {
                        id: "src/lib.rs::callee".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "callee".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: 13,
                        start_line: 1,
                        end_line: 1,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                    Symbol {
                        id: "src/lib.rs::caller".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "caller".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 14,
                        end_byte: 39,
                        start_line: 2,
                        end_line: 2,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                ],
            }])
            .expect("commit indexing");

        let graph = GraphEngine::new(root);
        let result = graph
            .graph(vec!["src/lib.rs::caller".to_string()], "down", 1)
            .expect("graph result");
        assert_eq!(result["graph_origin"], "indexed");
        let detailed = result["detailed_edges"].as_array().expect("detailed edges");
        assert!(detailed.iter().any(|edge| {
            edge["from"] == "src/lib.rs::caller"
                && edge["to"] == "src/lib.rs::callee"
                && edge["kind"] == "calls"
                && edge["tier"] == "semantic"
                && edge["source"] == "indexed:call_scan"
                && edge["evidence"]
                    .as_array()
                    .map(|evidence| {
                        evidence.iter().any(|entry| entry == "call_scan")
                            && evidence.iter().any(|entry| entry == "same_file")
                    })
                    .unwrap_or(false)
        }));
        assert_eq!(result["edge_summary"]["by_tier"]["semantic"], 1);
        assert_eq!(result["edge_summary"]["by_source"]["indexed:call_scan"], 1);
        let nodes = result["nodes"].as_array().expect("nodes");
        assert!(nodes.iter().any(|node| {
            node["id"] == "src/lib.rs::caller"
                && node["name"] == "caller"
                && node["start_line"] == 2
                && node["end_line"] == 2
        }));
    }

    #[test]
    fn fresh_graph_build_indexes_workspace_without_preexisting_cache() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(
            root.join("src/lib.rs"),
            "fn callee() {}\nfn caller() { callee(); }\n",
        )
        .expect("write source");

        let graph = GraphEngine::new(root)
            .build_dependency_graph_fresh()
            .expect("fresh graph");

        assert!(
            graph
                .outgoing
                .get("src/lib.rs::caller")
                .map(|targets| targets.contains("src/lib.rs::callee"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn graph_uses_indexed_import_edge_metadata() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(root.join("src/helper.rs"), "pub fn callee() {}\n").expect("write helper");
        std::fs::write(
            root.join("src/lib.rs"),
            "use crate::helper::callee;\nfn caller() {}\n",
        )
        .expect("write source");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");
        storage
            .commit_indexing_results(&[
                IndexWorkerEntry {
                    rel: "src/helper.rs".to_string(),
                    mtime_ms: 1,
                    file_size: 19,
                    symbols: vec![Symbol {
                        id: "src/helper.rs::callee".to_string(),
                        filepath: PathBuf::from("src/helper.rs"),
                        name: "callee".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: 18,
                        start_line: 1,
                        end_line: 1,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    }],
                },
                IndexWorkerEntry {
                    rel: "src/lib.rs".to_string(),
                    mtime_ms: 1,
                    file_size: 42,
                    symbols: vec![Symbol {
                        id: "src/lib.rs::caller".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "caller".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 27,
                        end_byte: 40,
                        start_line: 2,
                        end_line: 2,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    }],
                },
            ])
            .expect("commit indexing");

        let graph = GraphEngine::new(root);
        let result = graph
            .graph(vec!["src/lib.rs::caller".to_string()], "down", 1)
            .expect("graph result");
        let detailed = result["detailed_edges"].as_array().expect("detailed edges");
        assert!(
            detailed.iter().any(|edge| {
                edge["from"] == "src/lib.rs::caller"
                    && edge["to"] == "src/helper.rs::callee"
                    && edge["kind"] == "imports"
                    && edge["tier"] == "structural"
                    && edge["source"] == "indexed:import_scan"
                    && edge["evidence"]
                        .as_array()
                        .map(|evidence| {
                            evidence.iter().any(|entry| entry == "import_scan")
                                && evidence.iter().any(|entry| entry == "qualified_import")
                        })
                        .unwrap_or(false)
            }),
            "{result}"
        );
    }

    #[test]
    fn graph_uses_indexed_contains_edge_metadata() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        let source = "mod inner {\n    fn helper() {}\n}\n";
        std::fs::write(root.join("src/lib.rs"), source).expect("write source");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");
        storage
            .commit_indexing_results(&[IndexWorkerEntry {
                rel: "src/lib.rs".to_string(),
                mtime_ms: 1,
                file_size: source.len() as u64,
                symbols: vec![
                    Symbol {
                        id: "src/lib.rs::inner".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "inner".to_string(),
                        kind: SymbolKind::Module,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: source.len(),
                        start_line: 1,
                        end_line: 3,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                    Symbol {
                        id: "src/lib.rs::helper".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "helper".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 16,
                        end_byte: 30,
                        start_line: 2,
                        end_line: 2,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                ],
            }])
            .expect("commit indexing");

        let graph = GraphEngine::new(root);
        let result = graph
            .graph(vec!["src/lib.rs::inner".to_string()], "down", 1)
            .expect("graph result");
        let detailed = result["detailed_edges"].as_array().expect("detailed edges");
        assert!(
            detailed.iter().any(|edge| {
                edge["from"] == "src/lib.rs::inner"
                    && edge["to"] == "src/lib.rs::helper"
                    && edge["kind"] == "contains"
                    && edge["tier"] == "structural"
                    && edge["source"] == "indexed:contains"
                    && edge["evidence"]
                        .as_array()
                        .map(|evidence| {
                            evidence.iter().any(|entry| entry == "nested_span")
                                && evidence.iter().any(|entry| entry == "same_file")
                        })
                        .unwrap_or(false)
            }),
            "{result}"
        );
    }

    #[test]
    fn graph_uses_indexed_declares_edge_metadata() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        let source = "mod inner {\n    fn helper() {}\n}\n";
        std::fs::write(root.join("src/lib.rs"), source).expect("write source");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");
        storage
            .commit_indexing_results(&[IndexWorkerEntry {
                rel: "src/lib.rs".to_string(),
                mtime_ms: 1,
                file_size: source.len() as u64,
                symbols: vec![
                    Symbol {
                        id: "src/lib.rs::inner".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "inner".to_string(),
                        kind: SymbolKind::Module,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: source.len(),
                        start_line: 1,
                        end_line: 3,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                    Symbol {
                        id: "src/lib.rs::helper".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "helper".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 16,
                        end_byte: 30,
                        start_line: 2,
                        end_line: 2,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                ],
            }])
            .expect("commit indexing");

        let graph = GraphEngine::new(root);
        let result = graph
            .graph(vec!["src/lib.rs::inner".to_string()], "down", 1)
            .expect("graph result");
        let detailed = result["detailed_edges"].as_array().expect("detailed edges");
        assert!(
            detailed.iter().any(|edge| {
                edge["from"] == "src/lib.rs::inner"
                    && edge["to"] == "src/lib.rs::helper"
                    && edge["kind"] == "declares"
                    && edge["tier"] == "structural"
                    && edge["source"] == "indexed:declares"
                    && edge["evidence"]
                        .as_array()
                        .map(|evidence| evidence.iter().any(|entry| entry == "module_declaration"))
                        .unwrap_or(false)
            }),
            "{result}"
        );
    }

    #[test]
    fn graph_uses_indexed_member_ownership_edge_metadata() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        let source = "struct Thing {\n    fn helper() {}\n}\n";
        std::fs::write(root.join("src/lib.rs"), source).expect("write source");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");
        storage
            .commit_indexing_results(&[IndexWorkerEntry {
                rel: "src/lib.rs".to_string(),
                mtime_ms: 1,
                file_size: source.len() as u64,
                symbols: vec![
                    Symbol {
                        id: "src/lib.rs::Thing".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "Thing".to_string(),
                        kind: SymbolKind::Struct,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: source.len(),
                        start_line: 1,
                        end_line: 3,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                    Symbol {
                        id: "src/lib.rs::helper".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "helper".to_string(),
                        kind: SymbolKind::Method,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 19,
                        end_byte: 33,
                        start_line: 2,
                        end_line: 2,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                ],
            }])
            .expect("commit indexing");

        let graph = GraphEngine::new(root);
        let result = graph
            .graph(vec!["src/lib.rs::Thing".to_string()], "down", 1)
            .expect("graph result");
        let detailed = result["detailed_edges"].as_array().expect("detailed edges");
        assert!(
            detailed.iter().any(|edge| {
                edge["from"] == "src/lib.rs::Thing"
                    && edge["to"] == "src/lib.rs::helper"
                    && edge["kind"] == "owns_member"
                    && edge["tier"] == "structural"
                    && edge["source"] == "indexed:owns_member"
                    && edge["evidence"]
                        .as_array()
                        .map(|evidence| {
                            evidence.iter().any(|entry| entry == "member_ownership")
                                && evidence.iter().any(|entry| entry == "owner_kind:struct")
                        })
                        .unwrap_or(false)
            }),
            "{result}"
        );
    }

    #[test]
    fn graph_uses_indexed_origin_even_without_edges() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(root.join("src/lib.rs"), "fn lonely() {}\n").expect("write source");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");
        storage
            .commit_indexing_results(&[IndexWorkerEntry {
                rel: "src/lib.rs".to_string(),
                mtime_ms: 1,
                file_size: 14,
                symbols: vec![Symbol {
                    id: "src/lib.rs::lonely".to_string(),
                    filepath: PathBuf::from("src/lib.rs"),
                    name: "lonely".to_string(),
                    kind: SymbolKind::Function,
                    role: SymbolRole::Definition,
                    link_name: None,
                    start_byte: 0,
                    end_byte: 13,
                    start_line: 1,
                    end_line: 1,
                    signature: None,
                    semantic_hash: None,
                    fault_id: None,
                }],
            }])
            .expect("commit indexing");

        let graph = GraphEngine::new(root);
        let result = graph
            .graph(vec!["src/lib.rs::lonely".to_string()], "both", 1)
            .expect("graph result");
        assert_eq!(result["graph_origin"], "indexed");
        assert_eq!(
            result["detailed_edges"].as_array().map(|edges| edges.len()),
            Some(0)
        );
    }

    #[test]
    fn graph_filters_edges_by_min_confidence() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(
            root.join("src/lib.rs"),
            "fn callee() {}\nfn caller() { callee(); }\n",
        )
        .expect("write source");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");
        storage
            .commit_indexing_results(&[IndexWorkerEntry {
                rel: "src/lib.rs".to_string(),
                mtime_ms: 1,
                file_size: 40,
                symbols: vec![
                    Symbol {
                        id: "src/lib.rs::callee".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "callee".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: 13,
                        start_line: 1,
                        end_line: 1,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                    Symbol {
                        id: "src/lib.rs::caller".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "caller".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 14,
                        end_byte: 39,
                        start_line: 2,
                        end_line: 2,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                ],
            }])
            .expect("commit indexing");

        let graph = GraphEngine::new(root);
        let filtered = graph
            .graph_with_filters(
                vec!["src/lib.rs::caller".to_string()],
                0,
                1,
                Some(0.93),
                None,
                None,
            )
            .expect("graph result");
        assert_eq!(
            filtered["detailed_edges"]
                .as_array()
                .map(|edges| edges.len()),
            Some(0)
        );
        assert_eq!(filtered["filters"]["min_confidence"].as_f64(), Some(0.93));
    }

    #[test]
    fn graph_filters_ambiguous_call_edges_at_high_confidence() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(root.join("src/main.rs"), "fn caller() { target(); }\n")
            .expect("write caller");
        std::fs::write(root.join("src/a.rs"), "fn target() {}\n").expect("write target a");
        std::fs::write(root.join("src/b.rs"), "fn target() {}\n").expect("write target b");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");
        storage
            .commit_indexing_results(&[
                IndexWorkerEntry {
                    rel: "src/main.rs".to_string(),
                    mtime_ms: 1,
                    file_size: 24,
                    symbols: vec![Symbol {
                        id: "src/main.rs::caller".to_string(),
                        filepath: PathBuf::from("src/main.rs"),
                        name: "caller".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: 23,
                        start_line: 1,
                        end_line: 1,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    }],
                },
                IndexWorkerEntry {
                    rel: "src/a.rs".to_string(),
                    mtime_ms: 1,
                    file_size: 14,
                    symbols: vec![Symbol {
                        id: "src/a.rs::target".to_string(),
                        filepath: PathBuf::from("src/a.rs"),
                        name: "target".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: 13,
                        start_line: 1,
                        end_line: 1,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    }],
                },
                IndexWorkerEntry {
                    rel: "src/b.rs".to_string(),
                    mtime_ms: 1,
                    file_size: 14,
                    symbols: vec![Symbol {
                        id: "src/b.rs::target".to_string(),
                        filepath: PathBuf::from("src/b.rs"),
                        name: "target".to_string(),
                        kind: SymbolKind::Function,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: 13,
                        start_line: 1,
                        end_line: 1,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    }],
                },
            ])
            .expect("commit indexing");

        let graph = GraphEngine::new(root);
        let low = graph
            .graph_with_filters(
                vec!["src/main.rs::caller".to_string()],
                0,
                1,
                Some(0.55),
                None,
                None,
            )
            .expect("low graph");
        let high = graph
            .graph_with_filters(
                vec!["src/main.rs::caller".to_string()],
                0,
                1,
                Some(0.80),
                None,
                None,
            )
            .expect("high graph");
        assert_eq!(
            low["detailed_edges"].as_array().map(|edges| edges.len()),
            Some(2)
        );
        assert_eq!(
            high["detailed_edges"].as_array().map(|edges| edges.len()),
            Some(0)
        );
    }
}
