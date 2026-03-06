use crate::{CurdConfig, ParserManager, Symbol, SymbolKind, scan_workspace};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::plan::SystemEvent;
use rayon::prelude::*;
use tokio::sync::broadcast;

/// Main search entrypoint for the CURD engine
pub struct SearchEngine {
    pub workspace_root: PathBuf,
    pub config_override: Option<CurdConfig>,
    pub registry: crate::registry::GrammarRegistry,
    tx_events: Option<broadcast::Sender<SystemEvent>>,
    last_stats: Arc<Mutex<Option<IndexBuildStats>>>,
    pub compute_backend: Option<Arc<crate::gpu::ComputeBackend>>,
}

const MAX_FILE_SIZE: u64 = 512 * 1024; // 512 KB

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexWorkerRequest {
    pub workspace_root: String,
    pub files: Vec<String>,
    pub query_hint: Option<String>,
    pub parser_backend: String,
    pub max_file_size: u64,
    pub large_file_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexWorkerEntry {
    pub rel: String,
    pub mtime_ms: u64,
    pub file_size: u64,
    pub symbols: Vec<Symbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexWorkerResponse {
    pub entries: Vec<IndexWorkerEntry>,
    pub unsupported_lang: usize,
    pub skipped_too_large: usize,
    pub large_file_skeleton: usize,
    pub large_file_full: usize,
    pub fast_prefilter_skips: usize,
    pub parse_fail: usize,
    pub parse_fail_samples: Vec<String>,
    pub no_symbols: usize,
    pub native_files: usize,
    pub wasm_files: usize,
    pub native_fallbacks: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FileSymbolCache {
    mtime_ms: u64,
    #[serde(default)]
    file_size: u64,
    symbols: Vec<Symbol>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SymbolIndexCache {
    version: u32,
    files: HashMap<String, FileSymbolCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexBuildStats {
    pub index_mode: String,
    #[serde(default)]
    pub parser_backend: String,
    #[serde(default)]
    pub parser_backend_effective: String,
    #[serde(default)]
    pub compute_backend_effective: String,
    #[serde(default)]
    pub execution_model: String,
    pub max_file_size: u64,
    #[serde(default)]
    pub large_file_policy: String,
    pub chunk_size: usize,
    pub chunk_count: usize,
    pub total_files: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub unsupported_lang: usize,
    pub skipped_too_large: usize,
    #[serde(default)]
    pub large_file_skeleton: usize,
    #[serde(default)]
    pub large_file_full: usize,
    #[serde(default)]
    pub fast_prefilter_skips: usize,
    pub parse_fail: usize,
    #[serde(default)]
    pub parse_fail_samples: Vec<String>,
    #[serde(default)]
    pub native_files: usize,
    #[serde(default)]
    pub wasm_files: usize,
    #[serde(default)]
    pub native_fallbacks: usize,
    pub no_symbols: usize,
    pub scan_ms: u64,
    pub cache_load_ms: u64,
    pub parse_ms: u64,
    pub merge_ms: u64,
    pub serialize_ms: u64,
    pub total_ms: u64,
}

struct ThreadParseContext {
    manager: ParserManager,
    parsers: HashMap<String, Parser>,
    queries: HashMap<String, Query>,
}

fn deserialize_index(data: &[u8]) -> Option<SymbolIndexCache> {
    #[cfg(feature = "index-json")]
    {
        serde_json::from_slice::<SymbolIndexCache>(data).ok()
    }
    #[cfg(not(feature = "index-json"))]
    {
        bincode::deserialize::<SymbolIndexCache>(data).ok()
    }
}

fn serialize_index(cache: &SymbolIndexCache) -> Option<Vec<u8>> {
    #[cfg(feature = "index-json")]
    {
        serde_json::to_vec(cache).ok()
    }
    #[cfg(not(feature = "index-json"))]
    {
        bincode::serialize(cache).ok()
    }
}

impl SearchEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let compute_backend = crate::gpu::ComputeBackend::new().unwrap_or(None).map(Arc::new);
        let root = std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf());
        let registry = crate::registry::GrammarRegistry::load(&root);
        Self {
            workspace_root: root,
            config_override: None,
            registry,
            tx_events: None,
            last_stats: Arc::new(Mutex::new(None)),
            compute_backend,
        }
    }

    pub fn with_config(mut self, cfg: CurdConfig) -> Self {
        self.config_override = Some(cfg);
        self
    }

        pub fn with_events(mut self, tx: broadcast::Sender<SystemEvent>) -> Self {
        self.tx_events = Some(tx);
        self
    }

    /// Recursively scan and parse the workspace, returning matching symbols
    pub fn search(&self, query_str: &str, kind_filter: Option<SymbolKind>) -> Result<Vec<Symbol>> {
        let mut target_alias = None;
        let mut actual_query = query_str;
        
        // Check for explicit context routing e.g. "@backend::process"
        if query_str.starts_with('@')
            && let Some(idx) = query_str.find("::") {
                target_alias = Some(&query_str[..idx]);
                actual_query = &query_str[idx + 2..];
            }

        let mut all_symbols = Vec::new();
        let registry = crate::context_link::ContextRegistry::load(&self.workspace_root);

        // 1. Search Primary Workspace (if target_alias is None or explicitly local, though we don't have a local alias yet)
        if target_alias.is_none() || target_alias == Some("@local") {
            let mut local_syms = self.search_workspace(actual_query, kind_filter.clone(), &self.workspace_root, None)?;
            all_symbols.append(&mut local_syms);
        }

        // 2. Search External Contexts
        for (alias, link) in &registry.contexts {
            if target_alias.is_none() || target_alias == Some(alias) {
                let mut ext_syms = self.search_workspace(actual_query, kind_filter.clone(), &link.path, Some(alias))?;
                all_symbols.append(&mut ext_syms);
            }
        }

        Ok(all_symbols)
    }

    fn search_workspace(&self, query_str: &str, kind_filter: Option<SymbolKind>, root: &Path, prefix: Option<&str>) -> Result<Vec<Symbol>> {
        let cfg = self.config_override.clone().unwrap_or_else(|| CurdConfig::load_from_workspace(root));
        if index_mode(&cfg) == "lazy" {
            let query_lower = query_str.to_lowercase();
            // Consider: search_cached_only for external workspaces? We can just pass the path. 
            // For now, lazy mode in external contexts might skip building.
            if let Some(pfx) = prefix {
                let se = SearchEngine::new(root);
                let mut syms = se.search_cached_only(&query_lower, kind_filter, &cfg);
                for s in &mut syms {
                    s.id = format!("{}::{}", pfx, s.id);
                    if !s.filepath.is_absolute() {
                        s.filepath = root.join(&s.filepath);
                    }
                }
                return Ok(syms);
            }
            return Ok(self.search_cached_only(&query_lower, kind_filter, &cfg));
        }

        let mode = index_mode(&cfg);
        let mut manager = ParserManager::new_with_backend(
            root.join(".curd/grammars"),
            parser_backend_name(&cfg),
        )?;
        let query_lower = query_str.to_lowercase();
        let query_hint = if mode == "fast" {
            Some(query_lower.as_str())
        } else {
            None
        };
        
        let se = if prefix.is_some() { SearchEngine::new(root) } else { self.clone_for_local() };
        let mut syms = se.load_or_build_index(&mut manager, query_hint, &cfg, |s| {
            if !s.name.to_lowercase().contains(&query_lower) {
                return false;
            }
            if let Some(kind) = kind_filter.as_ref() {
                return s.kind == *kind;
            }
            true
        })?;

        if let Some(pfx) = prefix {
            for s in &mut syms {
                s.id = format!("{}::{}", pfx, s.id);
                if !s.filepath.is_absolute() {
                    s.filepath = root.join(&s.filepath);
                }
            }
        }
        Ok(syms)
    }

    fn clone_for_local(&self) -> Self {
        Self {
            workspace_root: self.workspace_root.clone(),
            config_override: self.config_override.clone(),
            registry: self.registry.clone(),
            tx_events: self.tx_events.clone(),
            last_stats: self.last_stats.clone(),
            compute_backend: self.compute_backend.clone(),
        }
    }

    pub fn last_index_stats(&self) -> Option<IndexBuildStats> {
        self.last_stats.lock().ok().and_then(|g| g.clone())
    }

    fn parse_chunk_via_worker(
        &self,
        chunk: &[PathBuf],
        query_hint: Option<&str>,
        parser_backend: &str,
        max_file_size: u64,
        large_file_policy: &str,
    ) -> Option<IndexWorkerResponse> {
        let worker_bin = std::env::current_exe().ok()?;
        let req = IndexWorkerRequest {
            workspace_root: self.workspace_root.to_string_lossy().to_string(),
            files: chunk
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            query_hint: query_hint.map(|s| s.to_string()),
            parser_backend: parser_backend.to_string(),
            max_file_size,
            large_file_policy: large_file_policy.to_string(),
        };
        let req_tmp = tempfile::NamedTempFile::new().ok()?;
        let resp_tmp = tempfile::NamedTempFile::new().ok()?;
        let req_path = req_tmp.path().to_path_buf();
        let resp_path = resp_tmp.path().to_path_buf();
        fs::write(&req_path, serde_json::to_vec(&req).ok()?).ok()?;
        let status = Command::new(worker_bin)
            .arg("index-worker")
            .arg("--request")
            .arg(&req_path)
            .arg("--response")
            .arg(&resp_path)
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
        let bytes = fs::read(resp_path).ok()?;
        serde_json::from_slice::<IndexWorkerResponse>(&bytes).ok()
    }

    fn index_path(&self) -> PathBuf {
        self.workspace_root.join(".curd").join("symbol_index.bin")
    }

    pub fn get_symbols_for_file(&self, rel_path: &str) -> Option<Vec<Symbol>> {
        let index_path = self.index_path();
        if !index_path.exists() {
            return None;
        }
        let data = fs::read(&index_path).ok()?;
        let cache: SymbolIndexCache = deserialize_index(&data)?;
        cache.files.get(rel_path).map(|f| f.symbols.clone())
    }

    fn load_or_build_index<F>(
        &self,
        manager: &mut ParserManager,
        query_hint: Option<&str>,
        cfg: &CurdConfig,
        matches: F,
    ) -> Result<Vec<Symbol>>
    where
        F: Fn(&Symbol) -> bool,
    {
        let t_start = Instant::now();
        let mut files = scan_workspace(&self.workspace_root)?;
        let mode = index_mode(cfg);
        let parser_backend = parser_backend_name(cfg);
        let max_file_size = max_file_size(cfg);
        let large_policy = large_file_policy(cfg);
        let execution = index_execution_model(cfg);
        if mode == "scoped" {
            let scopes = configured_scopes(cfg);
            if !scopes.is_empty() {
                files.retain(|f| {
                    f.strip_prefix(&self.workspace_root)
                        .ok()
                        .map(|rel| {
                            let rels = rel.to_string_lossy();
                            scopes.iter().any(|s| rels.starts_with(s))
                        })
                        .unwrap_or(false)
                });
            }
        }
        let scan_ms = t_start.elapsed().as_millis() as u64;
        let t_cache_load = Instant::now();
        let index_path = self.index_path();
        let mut cache = if index_path.exists() {
            fs::read(&index_path)
                .ok()
                .and_then(|data| deserialize_index(&data))
                .unwrap_or_default()
        } else {
            SymbolIndexCache::default()
        };
        if cache.version == 0 {
            cache.version = 1;
        }
        let cache_load_ms = t_cache_load.elapsed().as_millis() as u64;

        let total_files = files.len();
        let chunk_size = index_chunk_size(cfg);
        let chunk_count = if total_files == 0 {
            0
        } else {
            total_files.div_ceil(chunk_size)
        };
        let processed = Arc::new(AtomicUsize::new(0));
        let cache_hits = AtomicUsize::new(0);
        let cache_misses = AtomicUsize::new(0);
        let unsupported_lang = AtomicUsize::new(0);
        let skipped_too_large = AtomicUsize::new(0);
        let parse_fail = AtomicUsize::new(0);
        let parse_fail_samples: Arc<Mutex<HashMap<String, usize>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let no_symbols = AtomicUsize::new(0);
        let large_file_skeleton = AtomicUsize::new(0);
        let large_file_full = AtomicUsize::new(0);
        let fast_prefilter_skips = AtomicUsize::new(0);
        let native_files = AtomicUsize::new(0);
        let wasm_files = AtomicUsize::new(0);
        let native_fallbacks = AtomicUsize::new(0);
        let parse_time_micros = AtomicU64::new(0);
        let query_hint_lower = query_hint.map(|s| s.to_lowercase());
        let native_requested = parser_backend == "native";
        let stall_threshold_ms = index_stall_threshold_ms(cfg);
        let stall_stop = Arc::new(AtomicBool::new(false));
        let stall_thread = if let Some(tx) = self.tx_events.clone() {
            let processed_watch = Arc::clone(&processed);
            let stop = Arc::clone(&stall_stop);
            Some(std::thread::spawn(move || {
                let mut last = processed_watch.load(Ordering::Relaxed);
                let mut last_change = Instant::now();
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(1000));
                    let cur = processed_watch.load(Ordering::Relaxed);
                    if cur != last {
                        last = cur;
                        last_change = Instant::now();
                        continue;
                    }
                    if cur < total_files
                        && (last_change.elapsed().as_millis() as u64) >= stall_threshold_ms
                    {
                        let _ = tx.send(SystemEvent::NodeCompleted {
                            node_id: uuid::Uuid::nil(),
                            duration_ms: 0,
                            summary: format!(
                                "IndexStall: {}/{} files no_progress_ms={}",
                                cur,
                                total_files,
                                last_change.elapsed().as_millis()
                            ),
                            artifact_path: None,
                        });
                        last_change = Instant::now();
                    }
                }
            }))
        } else {
            None
        };

        let t_merge = Instant::now();
        let mut matched_symbols = Vec::new();
        let mut seen_files = std::collections::HashSet::new();
        for chunk in files.chunks(chunk_size) {
            if execution == "multiprocess" {
                let count = processed.fetch_add(chunk.len(), Ordering::Relaxed);
                let completed = count.saturating_add(chunk.len());
                if let Some(ref tx) = self.tx_events {
                    let _ = tx.send(SystemEvent::NodeCompleted {
                        node_id: uuid::Uuid::nil(),
                        duration_ms: 0,
                        summary: format!("Indexing: {}/{} files", completed, total_files),
                        artifact_path: None,
                    });
                }
                cache_misses.fetch_add(chunk.len(), Ordering::Relaxed);
                let worker_start = Instant::now();
                let worker_out = self.parse_chunk_via_worker(
                    chunk,
                    query_hint_lower.as_deref(),
                    &parser_backend,
                    max_file_size,
                    &large_policy,
                );
                parse_time_micros
                    .fetch_add(worker_start.elapsed().as_micros() as u64, Ordering::Relaxed);
                let out = worker_out.or_else(|| {
                    run_index_worker(IndexWorkerRequest {
                        workspace_root: self.workspace_root.to_string_lossy().to_string(),
                        files: chunk
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect(),
                        query_hint: query_hint_lower.clone(),
                        parser_backend: parser_backend.clone(),
                        max_file_size,
                        large_file_policy: large_policy.clone(),
                    })
                    .ok()
                });
                if let Some(out) = out {
                    unsupported_lang.fetch_add(out.unsupported_lang, Ordering::Relaxed);
                    skipped_too_large.fetch_add(out.skipped_too_large, Ordering::Relaxed);
                    large_file_skeleton.fetch_add(out.large_file_skeleton, Ordering::Relaxed);
                    large_file_full.fetch_add(out.large_file_full, Ordering::Relaxed);
                    fast_prefilter_skips.fetch_add(out.fast_prefilter_skips, Ordering::Relaxed);
                    parse_fail.fetch_add(out.parse_fail, Ordering::Relaxed);
                    no_symbols.fetch_add(out.no_symbols, Ordering::Relaxed);
                    native_files.fetch_add(out.native_files, Ordering::Relaxed);
                    wasm_files.fetch_add(out.wasm_files, Ordering::Relaxed);
                    native_fallbacks.fetch_add(out.native_fallbacks, Ordering::Relaxed);
                    if let Ok(mut samples) = parse_fail_samples.lock() {
                        for kv in out.parse_fail_samples {
                            let (k, v) = kv
                                .rsplit_once('=')
                                .map(|(a, b)| (a.to_string(), b.parse::<usize>().unwrap_or(1)))
                                .unwrap_or((kv, 1));
                            let e = samples.entry(k).or_insert(0);
                            *e = e.saturating_add(v);
                        }
                    }
                    for entry in out.entries {
                        seen_files.insert(entry.rel.clone());
                        for sym in entry.symbols.iter() {
                            if matches(sym) {
                                matched_symbols.push(sym.clone());
                            }
                        }
                        if mode != "fast" || !entry.symbols.is_empty() {
                            cache.files.insert(
                                entry.rel,
                                FileSymbolCache {
                                    mtime_ms: entry.mtime_ms,
                                    file_size: entry.file_size,
                                    symbols: entry.symbols,
                                },
                            );
                        }
                    }
                    continue;
                }
            }

            let process_file = |ctx: &mut ThreadParseContext, file: &PathBuf| {
                let count = processed.fetch_add(1, Ordering::Relaxed);
                let completed = count.saturating_add(1);
                if (completed.is_multiple_of(100) || completed == total_files)
                    && let Some(ref tx) = self.tx_events
                {
                    let _ = tx.send(SystemEvent::NodeCompleted {
                        node_id: uuid::Uuid::nil(),
                        duration_ms: 0,
                        summary: format!("Indexing: {}/{} files", completed, total_files),
                        artifact_path: None,
                    });
                }
                let rel = file
                    .strip_prefix(&self.workspace_root)
                    .ok()?
                    .to_string_lossy()
                    .to_string();
                let (mtime_ms, file_size) = file_meta(file).unwrap_or((0, 0));

                if let Some(entry) = cache.files.get(&rel)
                    && entry.mtime_ms == mtime_ms
                    && entry.file_size == file_size
                    && (mode == "lazy" || !entry.symbols.is_empty())
                {
                    cache_hits.fetch_add(1, Ordering::Relaxed);
                    return Some((rel, mtime_ms, file_size, entry.symbols.clone()));
                }

                cache_misses.fetch_add(1, Ordering::Relaxed);
                let Some(lang_name) = self.lang_for_path(file, cfg) else {
                    unsupported_lang.fetch_add(1, Ordering::Relaxed);
                    return Some((rel, mtime_ms, file_size, Vec::new()));
                };
                if file_size > max_file_size {
                    match large_policy.as_str() {
                        "full" => {
                            large_file_full.fetch_add(1, Ordering::Relaxed);
                        }
                        "skeleton" => {
                            large_file_skeleton.fetch_add(1, Ordering::Relaxed);
                            let symbols =
                                extract_skeleton_symbols(file, &self.workspace_root, &lang_name, self.compute_backend.as_deref());
                            if symbols.is_empty() {
                                no_symbols.fetch_add(1, Ordering::Relaxed);
                            }
                            return Some((rel, mtime_ms, file_size, symbols));
                        }
                        _ => {
                            skipped_too_large.fetch_add(1, Ordering::Relaxed);
                            return Some((rel, mtime_ms, file_size, Vec::new()));
                        }
                    }
                }

                if mode == "fast"
                    && let Some(hint) = query_hint_lower.as_ref()
                    && !hint.is_empty()
                    && !file_contains_case_insensitive(file, hint)
                {
                    fast_prefilter_skips.fetch_add(1, Ordering::Relaxed);
                    return Some((rel, mtime_ms, file_size, Vec::new()));
                }

                let parse_started = Instant::now();
                let parsed = match self.parse_file_with_context(file, &lang_name, ctx) {
                    Ok(v) => {
                        if v.is_empty() {
                            no_symbols.fetch_add(1, Ordering::Relaxed);
                        }
                        v
                    }
                    Err(_) => {
                        parse_fail.fetch_add(1, Ordering::Relaxed);
                        if let Ok(mut samples) = parse_fail_samples.lock() {
                            let key = format!("{}:{}", lang_name, rel);
                            let count = samples.entry(key).or_insert(0);
                            *count = count.saturating_add(1);
                        }
                        Vec::new()
                    }
                };
                parse_time_micros.fetch_add(
                    parse_started.elapsed().as_micros() as u64,
                    Ordering::Relaxed,
                );
                match ctx
                    .manager
                    .resolved_backend_for_language(&lang_name)
                    .as_deref()
                {
                    Some("native") => {
                        native_files.fetch_add(1, Ordering::Relaxed);
                    }
                    Some("wasm") => {
                        wasm_files.fetch_add(1, Ordering::Relaxed);
                        if native_requested {
                            native_fallbacks.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    _ => {}
                }

                Some((rel, mtime_ms, file_size, parsed))
            };

            let chunk_results: Vec<(String, u64, u64, Vec<Symbol>)> =
                if execution == "singlethreaded" {
                    let mut ctx = ThreadParseContext {
                        manager: manager.clone(),
                        parsers: HashMap::new(),
                        queries: HashMap::new(),
                    };
                    chunk
                        .iter()
                        .filter_map(|file| process_file(&mut ctx, file))
                        .collect()
                } else {
                    chunk
                        .par_iter()
                        .map_init(
                            || ThreadParseContext {
                                manager: manager.clone(),
                                parsers: HashMap::new(),
                                queries: HashMap::new(),
                            },
                            process_file,
                        )
                        .filter_map(|entry| entry)
                        .collect()
                };

            for (rel, mtime_ms, file_size, symbols) in chunk_results.into_iter() {
                seen_files.insert(rel.clone());
                for sym in symbols.iter() {
                    if matches(sym) {
                        matched_symbols.push(sym.clone());
                    }
                }
                if mode != "fast" || !symbols.is_empty() {
                    let entry = FileSymbolCache {
                        mtime_ms,
                        file_size,
                        symbols,
                    };
                    cache.files.insert(rel, entry);
                }
            }
        }
        let merge_ms = t_merge.elapsed().as_millis() as u64;

        let t_serialize = Instant::now();
        cache.files.retain(|k, _| seen_files.contains(k));
        if let Some(parent) = index_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Some(serialized) = serialize_index(&cache) {
            let _ = fs::write(index_path, serialized);
        }
        let serialize_ms = t_serialize.elapsed().as_millis() as u64;

        let total_ms = t_start.elapsed().as_millis() as u64;
        stall_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = stall_thread {
            let _ = handle.join();
        }
        let parse_fail_sample_list = {
            let mut rows: Vec<(String, usize)> = parse_fail_samples
                .lock()
                .ok()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
                .unwrap_or_default();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            rows.into_iter()
                .take(5)
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
        };
        let native_files_count = native_files.load(Ordering::Relaxed);
        let wasm_files_count = wasm_files.load(Ordering::Relaxed);
        let native_fallbacks_count = native_fallbacks.load(Ordering::Relaxed);
        let parser_backend_effective = if native_files_count > 0 && wasm_files_count > 0 {
            "mixed"
        } else if native_files_count > 0 {
            "native"
        } else if wasm_files_count > 0 {
            "wasm"
        } else {
            "none"
        };
        let stats = IndexBuildStats {
            index_mode: mode.clone(),
            parser_backend: parser_backend.clone(),
            parser_backend_effective: parser_backend_effective.to_string(),
            compute_backend_effective: self.compute_backend.as_ref().map(|b| b.backend_type().to_string()).unwrap_or_else(|| "cpu_fallback".to_string()),
            execution_model: execution.clone(),
            max_file_size,
            large_file_policy: large_policy.clone(),
            chunk_size,
            chunk_count,
            total_files,
            cache_hits: cache_hits.load(Ordering::Relaxed),
            cache_misses: cache_misses.load(Ordering::Relaxed),
            unsupported_lang: unsupported_lang.load(Ordering::Relaxed),
            skipped_too_large: skipped_too_large.load(Ordering::Relaxed),
            large_file_skeleton: large_file_skeleton.load(Ordering::Relaxed),
            large_file_full: large_file_full.load(Ordering::Relaxed),
            fast_prefilter_skips: fast_prefilter_skips.load(Ordering::Relaxed),
            parse_fail: parse_fail.load(Ordering::Relaxed),
            parse_fail_samples: parse_fail_sample_list,
            native_files: native_files_count,
            wasm_files: wasm_files_count,
            native_fallbacks: native_fallbacks_count,
            no_symbols: no_symbols.load(Ordering::Relaxed),
            scan_ms,
            cache_load_ms,
            parse_ms: parse_time_micros.load(Ordering::Relaxed) / 1000,
            merge_ms,
            serialize_ms,
            total_ms,
        };
        if let Ok(mut guard) = self.last_stats.lock() {
            *guard = Some(stats.clone());
        }
        let _ = crate::record_index_run(&self.workspace_root, cfg, &stats);
        if let Some(ref tx) = self.tx_events {
            let _ = tx.send(SystemEvent::NodeCompleted {
                node_id: uuid::Uuid::nil(),
                duration_ms: total_ms,
                summary: format!(
                    "IndexStats: index_mode={} parser_backend={} parser_backend_effective={} execution_model={} max_file_size={} large_file_policy={} chunk_size={} chunk_count={} total_files={} cache_hits={} cache_misses={} unsupported_lang={} skipped_too_large={} large_file_skeleton={} large_file_full={} fast_prefilter_skips={} parse_fail={} no_symbols={} native_files={} wasm_files={} native_fallbacks={} scan_ms={} cache_load_ms={} parse_ms={} merge_ms={} serialize_ms={} total_ms={}",
                    stats.index_mode,
                    stats.parser_backend,
                    stats.parser_backend_effective,
                    stats.execution_model,
                    stats.max_file_size,
                    stats.large_file_policy,
                    stats.chunk_size,
                    stats.chunk_count,
                    total_files,
                    stats.cache_hits,
                    stats.cache_misses,
                    stats.unsupported_lang,
                    stats.skipped_too_large,
                    stats.large_file_skeleton,
                    stats.large_file_full,
                    stats.fast_prefilter_skips,
                    stats.parse_fail,
                    stats.no_symbols,
                    stats.native_files,
                    stats.wasm_files,
                    stats.native_fallbacks,
                    scan_ms,
                    cache_load_ms,
                    stats.parse_ms,
                    merge_ms,
                    serialize_ms,
                    total_ms
                ),
                artifact_path: None,
            });
        }
        log::info!(
            "index_timing total_ms={} scan_ms={} cache_load_ms={} parse_ms={} merge_ms={} serialize_ms={} total_files={} cache_hits={} cache_misses={} unsupported_lang={} skipped_too_large={} large_file_skeleton={} large_file_full={} fast_prefilter_skips={} parse_fail={} no_symbols={} native_files={} wasm_files={} native_fallbacks={} index_mode={} parser_backend={} parser_backend_effective={} execution_model={} large_file_policy={}",
            total_ms,
            scan_ms,
            cache_load_ms,
            stats.parse_ms,
            merge_ms,
            serialize_ms,
            total_files,
            stats.cache_hits,
            stats.cache_misses,
            stats.unsupported_lang,
            stats.skipped_too_large,
            stats.large_file_skeleton,
            stats.large_file_full,
            stats.fast_prefilter_skips,
            stats.parse_fail,
            stats.no_symbols,
            stats.native_files,
            stats.wasm_files,
            stats.native_fallbacks,
            stats.index_mode,
            stats.parser_backend,
            stats.parser_backend_effective,
            stats.execution_model,
            stats.large_file_policy
        );

        Ok(matched_symbols)
    }

    pub fn invalidate_index(&self) {
        let _ = fs::remove_file(self.index_path());
        let _ = fs::remove_file(self.workspace_root.join(".curd").join("graph_index.json"));
    }

    pub fn parse_file(&self, file_path: &Path, manager: &mut ParserManager) -> Result<Vec<Symbol>> {
        let cfg = self.config_override.clone().unwrap_or_else(|| CurdConfig::load_from_workspace(&self.workspace_root));
        let lang_name = match self.lang_for_path(file_path, &cfg) {
            Some(v) => v,
            None => return Ok(vec![]),
        };
        let (_, file_size) = file_meta(file_path).unwrap_or((0, 0));
        if file_size > max_file_size(&cfg) {
            return Ok(vec![]);
        }
        let mut ctx = ThreadParseContext {
            manager: manager.clone(),
            parsers: HashMap::new(),
            queries: HashMap::new(),
        };
        self.parse_file_with_context(file_path, &lang_name, &mut ctx)
    }

    fn parse_file_with_context(
        &self,
        file_path: &Path,
        lang_name: &str,
        ctx: &mut ThreadParseContext,
    ) -> Result<Vec<Symbol>> {
        let source_bytes = fs::read(file_path)?;
        if source_bytes.is_empty() {
            return Ok(vec![]);
        }
        let source_cow = String::from_utf8_lossy(&source_bytes);
        let source_text: &str = source_cow.as_ref();

        if !ctx.parsers.contains_key(lang_name) {
            let parser = ctx.manager.create_parser(lang_name)?;
            let language = parser
                .language()
                .ok_or_else(|| anyhow::anyhow!("Missing language for parser {}", lang_name))?;
            let query_source = ctx.manager.registry.get_query(lang_name, &self.workspace_root).unwrap_or_default();
            if query_source.is_empty() {
                return Ok(vec![]);
            }
            let query = Query::new(&language, &query_source)?;
            ctx.parsers.insert(lang_name.to_string(), parser);
            ctx.queries.insert(lang_name.to_string(), query);
        }

        let parser = ctx
            .parsers
            .get_mut(lang_name)
            .ok_or_else(|| anyhow::anyhow!("Parser cache miss for {}", lang_name))?;
        let query = ctx
            .queries
            .get(lang_name)
            .ok_or_else(|| anyhow::anyhow!("Query cache miss for {}", lang_name))?;

        let tree = parser
            .parse(source_text, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse {}", file_path.display()))?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), source_text.as_bytes());

        let mut symbols_list = Vec::new();
        struct PendingSymbol {
            id: String,
            name: String,
            kind: SymbolKind,
            start_byte: usize,
            end_byte: usize,
            start_line: usize,
            end_line: usize,
        }
        let mut pending = Vec::new();
        let mut strings_to_hash = Vec::new();

        while let Some(m) = matches.next() {
            let mut kind = SymbolKind::Function;
            let mut name = String::new();
            let mut node = None;

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let text_bytes =
                    &source_text.as_bytes()[capture.node.start_byte()..capture.node.end_byte()];
                let text = String::from_utf8_lossy(text_bytes);

                match capture_name {
                    "func" => {
                        kind = SymbolKind::Function;
                        node = Some(capture.node);
                    }
                    "class" => {
                        kind = SymbolKind::Class;
                        node = Some(capture.node);
                    }
                    "name" => {
                        name = text.into_owned();
                    }
                    _ => {}
                }
            }

            if let Some(target_node) = node
                && !name.is_empty()
            {
                let rel_path = file_path
                    .strip_prefix(&self.workspace_root)
                    .unwrap_or(file_path);
                let id = format!("{}::{}", rel_path.display(), name);

                let symbol_text = &source_text[target_node.start_byte()..target_node.end_byte()];
                strings_to_hash.push(symbol_text);

                pending.push(PendingSymbol {
                    id,
                    name,
                    kind,
                    start_byte: target_node.start_byte(),
                    end_byte: target_node.end_byte(),
                    start_line: target_node.start_position().row + 1,
                    end_line: target_node.end_position().row + 1,
                });
            }
        }

        if !pending.is_empty() {
            let hashes = match self.compute_backend.as_deref() {
                #[cfg(feature = "gpu-embedded")]
                Some(gpu) => pollster::block_on(gpu.hash_batch(&strings_to_hash)).unwrap_or_else(|_| crate::gpu::ComputeBackend::hash_batch_cpu(&strings_to_hash)),
                _ => crate::gpu::ComputeBackend::hash_batch_cpu(&strings_to_hash),
            };

            for (sym, hash_str) in pending.into_iter().zip(hashes.into_iter()) {
                symbols_list.push(Symbol {
                    id: sym.id,
                    filepath: file_path.to_path_buf(),
                    name: sym.name,
                    kind: sym.kind,
                    start_byte: sym.start_byte,
                    end_byte: sym.end_byte,
                    start_line: sym.start_line,
                    end_line: sym.end_line,
                    signature: None,
                    semantic_hash: hash_str,
                });
            }
        }

        Ok(symbols_list)
    }

    fn search_cached_only(
        &self,
        query_lower: &str,
        kind_filter: Option<SymbolKind>,
        cfg: &CurdConfig,
    ) -> Vec<Symbol> {
        let index_path = self.index_path();
        let Some(data) = fs::read(&index_path).ok() else {
            if let Ok(mut guard) = self.last_stats.lock() {
                *guard = Some(IndexBuildStats {
                    index_mode: "lazy".to_string(),
                    parser_backend: parser_backend_name(cfg),
                    parser_backend_effective: "none".to_string(),
                    compute_backend_effective: self.compute_backend.as_ref().map(|b| b.backend_type().to_string()).unwrap_or_else(|| "none".to_string()),
                    execution_model: index_execution_model(cfg),
                    max_file_size: max_file_size(cfg),
                    large_file_policy: large_file_policy(cfg),
                    chunk_size: index_chunk_size(cfg),
                    chunk_count: 0,
                    total_files: 0,
                    cache_hits: 0,
                    cache_misses: 0,
                    unsupported_lang: 0,
                    skipped_too_large: 0,
                    large_file_skeleton: 0,
                    large_file_full: 0,
                    fast_prefilter_skips: 0,
                    parse_fail: 0,
                    parse_fail_samples: Vec::new(),
                    native_files: 0,
                    wasm_files: 0,
                    native_fallbacks: 0,
                    no_symbols: 0,
                    scan_ms: 0,
                    cache_load_ms: 0,
                    parse_ms: 0,
                    merge_ms: 0,
                    serialize_ms: 0,
                    total_ms: 0,
                });
            }
            return Vec::new();
        };
        let Some(cache) = deserialize_index(&data) else {
            if let Ok(mut guard) = self.last_stats.lock() {
                *guard = Some(IndexBuildStats {
                    index_mode: "lazy".to_string(),
                    parser_backend: parser_backend_name(cfg),
                    parser_backend_effective: "none".to_string(),
                    compute_backend_effective: self.compute_backend.as_ref().map(|b| b.backend_type().to_string()).unwrap_or_else(|| "none".to_string()),
                    execution_model: index_execution_model(cfg),
                    max_file_size: max_file_size(cfg),
                    large_file_policy: large_file_policy(cfg),
                    chunk_size: index_chunk_size(cfg),
                    chunk_count: 0,
                    total_files: 0,
                    cache_hits: 0,
                    cache_misses: 0,
                    unsupported_lang: 0,
                    skipped_too_large: 0,
                    large_file_skeleton: 0,
                    large_file_full: 0,
                    fast_prefilter_skips: 0,
                    parse_fail: 0,
                    parse_fail_samples: Vec::new(),
                    native_files: 0,
                    wasm_files: 0,
                    native_fallbacks: 0,
                    no_symbols: 0,
                    scan_ms: 0,
                    cache_load_ms: 0,
                    parse_ms: 0,
                    merge_ms: 0,
                    serialize_ms: 0,
                    total_ms: 0,
                });
            }
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut no_symbols = 0usize;
        for entry in cache.files.values() {
            if entry.symbols.is_empty() {
                no_symbols = no_symbols.saturating_add(1);
            }
            for s in entry.symbols.iter() {
                if !s.name.to_lowercase().contains(query_lower) {
                    continue;
                }
                if let Some(kind) = kind_filter.as_ref()
                    && s.kind != *kind
                {
                    continue;
                }
                out.push(s.clone());
            }
        }
        if let Ok(mut guard) = self.last_stats.lock() {
            *guard = Some(IndexBuildStats {
                index_mode: "lazy".to_string(),
                parser_backend: parser_backend_name(cfg),
                parser_backend_effective: "none".to_string(),
                compute_backend_effective: self.compute_backend.as_ref().map(|b| b.backend_type().to_string()).unwrap_or_else(|| "none".to_string()),
                execution_model: index_execution_model(cfg),
                max_file_size: max_file_size(cfg),
                large_file_policy: large_file_policy(cfg),
                chunk_size: index_chunk_size(cfg),
                chunk_count: if cache.files.is_empty() { 0 } else { 1 },
                total_files: cache.files.len(),
                cache_hits: cache.files.len(),
                cache_misses: 0,
                unsupported_lang: 0,
                skipped_too_large: 0,
                large_file_skeleton: 0,
                large_file_full: 0,
                fast_prefilter_skips: 0,
                parse_fail: 0,
                parse_fail_samples: Vec::new(),
                native_files: 0,
                wasm_files: 0,
                native_fallbacks: 0,
                no_symbols,
                scan_ms: 0,
                cache_load_ms: 0,
                parse_ms: 0,
                merge_ms: 0,
                serialize_ms: 0,
                total_ms: 0,
            });
        }
        out
    }

    fn lang_for_path(&self, path: &Path, cfg: &CurdConfig) -> Option<String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .trim()
            .trim_start_matches('.')
            .to_lowercase();
        if ext.is_empty() {
            return None;
        }
        if let Some(mapped) = cfg.index.extension_language_map.get(&ext) {
            return Some(mapped.trim().to_lowercase());
        }
        if let Some(mapped) = cfg.index.extension_language_map.get(&format!(".{}", ext)) {
            return Some(mapped.trim().to_lowercase());
        }
        if !cfg.index.extension_language_map.is_empty() {
            return None;
        }
        
        self.registry.lang_for_extension(&ext)
    }
}

pub fn run_index_worker(request: IndexWorkerRequest) -> Result<IndexWorkerResponse> {
    let workspace_root = PathBuf::from(request.workspace_root);
    let _cfg = CurdConfig::load_from_workspace(&workspace_root);
    let manager = ParserManager::new_with_backend(
        workspace_root.join(".curd/grammars"),
        if request.parser_backend.is_empty() {
            "wasm".to_string()
        } else {
            request.parser_backend.clone()
        },
    )?;
    let se = SearchEngine::new(&workspace_root);
    let mut ctx = ThreadParseContext {
        manager: manager.clone(),
        parsers: HashMap::new(),
        queries: HashMap::new(),
    };
    let mut entries = Vec::with_capacity(request.files.len());
    let mut unsupported_lang = 0usize;
    let mut skipped_too_large = 0usize;
    let mut large_file_skeleton = 0usize;
    let mut large_file_full = 0usize;
    let mut fast_prefilter_skips = 0usize;
    let mut parse_fail = 0usize;
    let mut parse_fail_samples: HashMap<String, usize> = HashMap::new();
    let mut no_symbols = 0usize;
    let mut native_files = 0usize;
    let mut wasm_files = 0usize;
    let mut native_fallbacks = 0usize;
    let native_requested = request.parser_backend.eq_ignore_ascii_case("native");
    let query_hint = request.query_hint.as_deref().map(str::to_lowercase);

    for file_str in request.files {
        let file = PathBuf::from(file_str);
        let rel = file
            .strip_prefix(&workspace_root)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file.to_string_lossy().to_string());
        let (mtime_ms, file_size) = file_meta(&file).unwrap_or((0, 0));

        let Some(lang_name) = manager.registry.lang_for_extension(&file.extension().and_then(|e| e.to_str()).unwrap_or("").trim().trim_start_matches('.').to_lowercase()) else {
            unsupported_lang = unsupported_lang.saturating_add(1);
            entries.push(IndexWorkerEntry {
                rel,
                mtime_ms,
                file_size,
                symbols: Vec::new(),
            });
            continue;
        };
        if file_size > request.max_file_size {
            match request.large_file_policy.as_str() {
                "full" => {
                    large_file_full = large_file_full.saturating_add(1);
                }
                "skeleton" => {
                    large_file_skeleton = large_file_skeleton.saturating_add(1);
                    let symbols = extract_skeleton_symbols(&file, &workspace_root, &lang_name, None);
                    if symbols.is_empty() {
                        no_symbols = no_symbols.saturating_add(1);
                    }
                    entries.push(IndexWorkerEntry {
                        rel,
                        mtime_ms,
                        file_size,
                        symbols,
                    });
                    continue;
                }
                _ => {
                    skipped_too_large = skipped_too_large.saturating_add(1);
                    entries.push(IndexWorkerEntry {
                        rel,
                        mtime_ms,
                        file_size,
                        symbols: Vec::new(),
                    });
                    continue;
                }
            }
        }
        if let Some(hint) = query_hint.as_deref()
            && !hint.is_empty()
            && !file_contains_case_insensitive(&file, hint)
        {
            fast_prefilter_skips = fast_prefilter_skips.saturating_add(1);
            entries.push(IndexWorkerEntry {
                rel,
                mtime_ms,
                file_size,
                symbols: Vec::new(),
            });
            continue;
        }
        let symbols = match se.parse_file_with_context(&file, &lang_name, &mut ctx) {
            Ok(v) => {
                if v.is_empty() {
                    no_symbols = no_symbols.saturating_add(1);
                }
                v
            }
            Err(_) => {
                parse_fail = parse_fail.saturating_add(1);
                let key = format!("{}:{}", lang_name, rel);
                let e = parse_fail_samples.entry(key).or_insert(0);
                *e = e.saturating_add(1);
                Vec::new()
            }
        };
        match ctx
            .manager
            .resolved_backend_for_language(&lang_name)
            .as_deref()
        {
            Some("native") => {
                native_files = native_files.saturating_add(1);
            }
            Some("wasm") => {
                wasm_files = wasm_files.saturating_add(1);
                if native_requested {
                    native_fallbacks = native_fallbacks.saturating_add(1);
                }
            }
            _ => {}
        }
        entries.push(IndexWorkerEntry {
            rel,
            mtime_ms,
            file_size,
            symbols,
        });
    }

    let mut sample_rows: Vec<(String, usize)> = parse_fail_samples.into_iter().collect();
    sample_rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let parse_fail_samples = sample_rows
        .into_iter()
        .take(5)
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>();

    Ok(IndexWorkerResponse {
        entries,
        unsupported_lang,
        skipped_too_large,
        large_file_skeleton,
        large_file_full,
        fast_prefilter_skips,
        parse_fail,
        parse_fail_samples,
        no_symbols,
        native_files,
        wasm_files,
        native_fallbacks,
    })
}

fn file_meta(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dur = mtime.duration_since(UNIX_EPOCH).ok()?;
    Some((dur.as_millis() as u64, meta.len()))
}

fn max_file_size(cfg: &CurdConfig) -> u64 {
    cfg.index
        .max_file_size
        .or_else(|| {
            std::env::var("CURD_INDEX_MAX_FILE_SIZE")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
        })
        .filter(|v| *v > 0)
        .unwrap_or(MAX_FILE_SIZE)
}

fn index_mode(cfg: &CurdConfig) -> String {
    let mode = cfg
        .index
        .mode
        .clone()
        .or_else(|| std::env::var("CURD_INDEX_MODE").ok())
        .unwrap_or_else(|| "fast".to_string())
        .to_lowercase();
    match mode.as_str() {
        "full" | "fast" | "lazy" | "scoped" => mode,
        _ => "fast".to_string(),
    }
}

fn parser_backend_name(cfg: &CurdConfig) -> String {
    cfg.index
        .parser_backend
        .clone()
        .or_else(|| std::env::var("CURD_PARSER_BACKEND").ok())
        .unwrap_or_else(|| "wasm".to_string())
        .to_lowercase()
}

fn configured_scopes(cfg: &CurdConfig) -> Vec<String> {
    if let Ok(env_scope) = std::env::var("CURD_INDEX_SCOPE") {
        return env_scope
            .split(',')
            .map(|s| s.trim().trim_start_matches("./").to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    cfg.index
        .scope
        .iter()
        .map(|s| s.trim().trim_start_matches("./").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn index_chunk_size(cfg: &CurdConfig) -> usize {
    cfg.index
        .chunk_size
        .or_else(|| {
            std::env::var("CURD_INDEX_CHUNK_SIZE")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
        })
        .filter(|v| *v > 0)
        .unwrap_or(4096)
}

fn index_stall_threshold_ms(cfg: &CurdConfig) -> u64 {
    cfg.index
        .stall_threshold_ms
        .or_else(|| {
            std::env::var("CURD_INDEX_STALL_THRESHOLD_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
        })
        .filter(|v| *v > 0)
        .unwrap_or(15_000)
}

fn large_file_policy(cfg: &CurdConfig) -> String {
    let policy = cfg
        .index
        .large_file_policy
        .clone()
        .or_else(|| std::env::var("CURD_INDEX_LARGE_FILE_POLICY").ok())
        .unwrap_or_else(|| "skip".to_string())
        .to_lowercase();
    match policy.as_str() {
        "skip" | "skeleton" | "full" => policy,
        _ => "skip".to_string(),
    }
}

fn index_execution_model(cfg: &CurdConfig) -> String {
    let mode = cfg
        .index
        .execution
        .clone()
        .or_else(|| std::env::var("CURD_INDEX_EXECUTION").ok())
        .unwrap_or_else(|| "multithreaded".to_string())
        .to_lowercase();
    match mode.as_str() {
        "singlethreaded" => "singlethreaded".to_string(),
        "multiprocess" => "multiprocess".to_string(),
        "multithreaded" => "multithreaded".to_string(),
        _ => "multithreaded".to_string(),
    }
}

fn file_contains_case_insensitive(path: &Path, query_lower: &str) -> bool {
    if query_lower.is_empty() {
        return true;
    }
    fs::read(path)
        .ok()
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .to_lowercase()
                .contains(query_lower)
        })
        .unwrap_or(false)
}

fn extract_skeleton_symbols(
    file_path: &Path,
    workspace_root: &Path,
    lang_name: &str,
    compute_backend: Option<&crate::gpu::ComputeBackend>,
) -> Vec<Symbol> {
    let source = match fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rel = file_path
        .strip_prefix(workspace_root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();
    let mut out = Vec::new();
    let mut byte_offset = 0usize;

    struct PendingSymbol {
        id: String,
        name: String,
        kind: SymbolKind,
        start_byte: usize,
        end_byte: usize,
        start_line: usize,
        end_line: usize,
    }
    let mut pending = Vec::new();
    let mut strings_to_hash = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let mut push = |name: String, kind: SymbolKind| {
            if name.is_empty() {
                return;
            }
            let id = format!("{}::{}", rel, name);
            pending.push(PendingSymbol {
                id,
                name,
                kind,
                start_byte: byte_offset,
                end_byte: byte_offset.saturating_add(line.len()),
                start_line: idx + 1,
                end_line: idx + 1,
            });
            strings_to_hash.push(line);
        };

        if let Some(name) = extract_line_symbol_name(trimmed, lang_name, SymbolKind::Function) {
            push(name, SymbolKind::Function);
        } else if let Some(name) = extract_line_symbol_name(trimmed, lang_name, SymbolKind::Class) {
            push(name, SymbolKind::Class);
        }
        byte_offset = byte_offset.saturating_add(line.len() + 1);
    }

    if pending.is_empty() {
        return out;
    }

    let hashes = match compute_backend {
        #[cfg(feature = "gpu-embedded")]
        Some(gpu) => pollster::block_on(gpu.hash_batch(&strings_to_hash)).unwrap_or_else(|_| crate::gpu::ComputeBackend::hash_batch_cpu(&strings_to_hash)),
        _ => crate::gpu::ComputeBackend::hash_batch_cpu(&strings_to_hash),
    };

    for (sym, hash_str) in pending.into_iter().zip(hashes.into_iter()) {
        out.push(Symbol {
            id: sym.id,
            filepath: file_path.to_path_buf(),
            name: sym.name,
            kind: sym.kind,
            start_byte: sym.start_byte,
            end_byte: sym.end_byte,
            start_line: sym.start_line,
            end_line: sym.end_line,
            signature: None,
            semantic_hash: hash_str,
        });
    }

    out
}

fn extract_line_symbol_name(trimmed: &str, lang_name: &str, kind: SymbolKind) -> Option<String> {
    let is_function = kind == SymbolKind::Function;
    let prefixes: &[&str] = match (lang_name, is_function) {
        ("rust", false) => &["struct ", "enum ", "trait "],
        ("python", false) => &["class "],
        ("go", false) => &["type "],
        ("java", false) => &["class ", "interface ", "enum "],
        ("c", false) | ("cpp", false) => &["struct ", "class "],
        ("javascript", false) | ("typescript", false) => &["class ", "export class "],
        ("rust", true) => &[
            "fn ",
            "pub fn ",
            "pub(crate) fn ",
            "async fn ",
            "pub async fn ",
        ],
        ("python", true) => &["def ", "async def "],
        ("go", true) => &["func "],
        ("java", true) => &["public ", "private ", "protected ", "static ", "final "],
        ("c", true) | ("cpp", true) => &[""],
        ("javascript", true) | ("typescript", true) => {
            &["function ", "export function ", "const ", "let ", "var "]
        }
        _ => &[],
    };

    for p in prefixes {
        if !p.is_empty() && !trimmed.starts_with(p) {
            continue;
        }
        if let Some(name) = parse_identifier(trimmed.trim_start_matches(p)) {
            if is_function && !looks_function_like(trimmed, lang_name) {
                continue;
            }
            return Some(name);
        }
    }
    None
}

fn looks_function_like(line: &str, lang_name: &str) -> bool {
    match lang_name {
        "python" => line.contains('(') && line.contains(')') && line.ends_with(':'),
        "javascript" | "typescript" => {
            line.contains("=>")
                || (line.contains('(') && (line.contains('{') || line.ends_with(';')))
        }
        "rust" | "go" | "java" | "c" | "cpp" => line.contains('(') && line.contains(')'),
        _ => false,
    }
}

fn parse_identifier(s: &str) -> Option<String> {
    let mut started = false;
    let mut out = String::new();
    for ch in s.chars() {
        if !started {
            if ch.is_ascii_alphabetic() || ch == '_' {
                out.push(ch);
                started = true;
            }
            continue;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            break;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}
