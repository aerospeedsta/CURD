use crate::{CurdConfig, ParserManager, Symbol, SymbolKind, scan_workspace, storage::Storage, symbols::SymbolRole};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tree_sitter::{Parser, Query, StreamingIterator};
use sha2::Digest;
use rusqlite::params;

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
    pub compute_backend: Arc<std::sync::OnceLock<Option<Arc<crate::gpu::ComputeBackend>>>>,
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

pub struct ThreadParseContext {
    manager: ParserManager,
    parsers: HashMap<String, Parser>,
    queries: HashMap<String, Query>,
}

impl SearchEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root_path = std::path::absolute(workspace_root.as_ref())
            .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf());
        Self {
            workspace_root: root_path.clone(),
            config_override: None,
            registry: crate::registry::GrammarRegistry::load(&root_path),
            tx_events: None,
            last_stats: Arc::new(Mutex::new(None)),
            compute_backend: Arc::new(std::sync::OnceLock::new()),
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

    pub fn invalidate_index(&self) {
        let cfg = self.config_override.clone().unwrap_or_else(|| CurdConfig::load_from_workspace(&self.workspace_root));
        let storage = Storage::open(&self.workspace_root, &cfg).ok();
        if let Some(s) = storage {
            let _ = s.conn.execute("DELETE FROM files", []);
            let _ = s.conn.execute("DELETE FROM symbols", []);
            let _ = s.conn.execute("DELETE FROM edges", []);
        }
    }

    pub fn search(&self, query_str: &str, kind_filter: Option<SymbolKind>) -> Result<Vec<Symbol>> {
        let mut target_alias = None;
        let mut actual_query = query_str;
        
        if query_str.starts_with('@')
            && let Some(idx) = query_str.find("::") {
                target_alias = Some(&query_str[..idx]);
                actual_query = &query_str[idx + 2..];
            }

        let mut all_symbols = Vec::new();
        let registry = crate::context_link::ContextRegistry::load(&self.workspace_root);

        if target_alias.is_none() || target_alias == Some("@local") {
            let mut local_syms = self.search_workspace(actual_query, kind_filter.clone(), &self.workspace_root, None)?;
            all_symbols.append(&mut local_syms);
        }

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
        let mode = index_mode(&cfg);

        if mode == "lazy" {
            let mut syms = self.search_cached_only(&query_str.to_lowercase(), kind_filter.clone(), &cfg);
            if let Some(pfx) = prefix {
                for s in &mut syms {
                    s.id = format!("{}::{}", pfx, s.id);
                    if !s.filepath.is_absolute() {
                        s.filepath = root.join(&s.filepath);
                    }
                }
            }
            if !syms.is_empty() {
                return Ok(syms);
            }
        }
        
        let mut manager = ParserManager::new_with_backend(
            root.join(".curd/grammars"),
            parser_backend_name(&cfg),
        )?;
        
        let query_hint = if mode == "fast" { Some(query_str) } else { None };
        
        let se = if prefix.is_some() { SearchEngine::new(root) } else { self.clone_for_local() };
        // load_or_build_index now only handles ensuring DB is current
        let _ = se.load_or_build_index(&mut manager, query_hint, &cfg, |_| true)?;

        // ALWAYS query from SQLite for search results to be fast
        let mut final_syms = se.search_cached_only(&query_str.to_lowercase(), kind_filter, &cfg);
        
        if let Some(pfx) = prefix {
            for s in &mut final_syms {
                s.id = format!("{}::{}", pfx, s.id);
                if !s.filepath.is_absolute() {
                    s.filepath = root.join(&s.filepath);
                }
            }
        }
        Ok(final_syms)
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

    pub fn get_symbols_for_file(&self, rel_path: &str) -> Option<Vec<Symbol>> {
        let cfg = self.config_override.clone().unwrap_or_else(|| CurdConfig::load_from_workspace(&self.workspace_root));
        let storage = Storage::open(&self.workspace_root, &cfg).ok()?;
        storage.get_symbols_for_file(rel_path).ok()
    }

    pub fn load_or_build_index<F>(
        &self,
        manager: &mut ParserManager,
        query_hint: Option<&str>,
        cfg: &CurdConfig,
        _matches: F,
    ) -> Result<Vec<Symbol>>
    where
        F: Fn(&Symbol) -> bool,
    {
        let t_start = Instant::now();
        let mut all_files = scan_workspace(&self.workspace_root)?;
        let mode = index_mode(cfg);
        let parser_backend = parser_backend_name(cfg);
        let max_file_size = max_file_size(cfg);
        let large_policy = large_file_policy(cfg);
        let execution = index_execution_model(cfg);
        
        if mode == "scoped" {
            let scopes = configured_scopes(cfg);
            if !scopes.is_empty() {
                all_files.retain(|f| {
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
        
        let mut storage = Storage::open(&self.workspace_root, cfg)?;
        let total_files = all_files.len();
        let chunk_size = index_chunk_size(cfg);
        
        let processed = Arc::new(AtomicUsize::new(0));
        let cache_hits = AtomicUsize::new(0);
        let cache_misses = AtomicUsize::new(0);
        let query_hint_lower = query_hint.map(|s| s.to_lowercase());
        
        let mut seen_files = std::collections::HashSet::new();

        // RAPID DELTA DETECTION: Load ALL metadata in one query
        let t_cache_load = Instant::now();
        let all_meta = storage.get_all_file_meta()?;
        let cache_load_ms = t_cache_load.elapsed().as_millis() as u64;

        // PRE-FILTER CACHE HITS using Memory Map
        let files_to_parse: Vec<PathBuf> = all_files.into_par_iter().filter_map(|file| {
            let abs_path = fs::canonicalize(&file).unwrap_or(file);
            let abs_path_str = abs_path.to_string_lossy().to_string();
            let (mtime_ms, file_size) = file_meta(&abs_path).unwrap_or((0, 0));

            if let Some(meta) = all_meta.get(&abs_path_str) {
                if meta.0 == mtime_ms && meta.1 == file_size {
                    cache_hits.fetch_add(1, Ordering::Relaxed);
                    processed.fetch_add(1, Ordering::Relaxed);
                    // Hit! Don't parse.
                    return None;
                }
            }
            Some(abs_path)
        }).collect();

        let files = files_to_parse;
        let chunk_count = if files.is_empty() {
            0
        } else {
            files.len().div_ceil(chunk_size)
        };

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
        let mut all_new_entries = Vec::new();

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
                
                if let Some(out) = worker_out {
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
                    
                    for entry in out.entries {
                        seen_files.insert(entry.rel.clone());
                        all_new_entries.push(entry);
                    }
                }
                continue;
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
                let rel = file.to_string_lossy().to_string();
                let (mtime_ms, file_size) = file_meta(file).unwrap_or((0, 0));

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
                                extract_skeleton_symbols(file, &self.workspace_root, &lang_name, None);
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
                    Err(e) => {
                        parse_fail.fetch_add(1, Ordering::Relaxed);
                        if let Ok(mut samples) = parse_fail_samples.lock() {
                            let key = format!("{}: {}: {}", lang_name, rel, e);
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
                all_new_entries.push(IndexWorkerEntry {
                    rel,
                    mtime_ms,
                    file_size,
                    symbols,
                });
            }
        }
        let merge_ms = t_merge.elapsed().as_millis() as u64;

        // RAPID INITIAL INDEXING: Use a single transaction for all database updates
        let t_serialize = Instant::now();
        if !all_new_entries.is_empty() {
            if let Err(_e) = storage.commit_indexing_results(&all_new_entries) {
            }
        }
        
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
            rows.sort_by(|a, b| b.1.cmp(&a.1));
            rows.into_iter().take(5).map(|(k, v)| format!("{}={}", k, v)).collect()
        };

        let stats = IndexBuildStats {
            index_mode: mode,
            parser_backend: parser_backend.clone(),
            parser_backend_effective: parser_backend,
            compute_backend_effective: self.compute_backend.get().and_then(|b| b.as_ref().map(|b| b.backend_type().to_string())).unwrap_or_else(|| "cpu_fallback".to_string()),
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
            native_files: native_files.load(Ordering::Relaxed),
            wasm_files: wasm_files.load(Ordering::Relaxed),
            native_fallbacks: native_fallbacks.load(Ordering::Relaxed),
            no_symbols: no_symbols.load(Ordering::Relaxed),
            scan_ms,
            cache_load_ms,
            parse_ms: parse_time_micros.load(Ordering::Relaxed) / 1000,
            merge_ms,
            serialize_ms: t_serialize.elapsed().as_millis() as u64,
            total_ms,
        };

        if let Ok(mut guard) = self.last_stats.lock() {
            *guard = Some(stats.clone());
        }

        if let Some(ref tx) = self.tx_events {
            let _ = tx.send(SystemEvent::NodeCompleted {
                node_id: uuid::Uuid::nil(),
                duration_ms: total_ms,
                summary: format!(
                    "IndexStats: total_files={} cache_hits={} cache_misses={} total_ms={}",
                    total_files, stats.cache_hits, stats.cache_misses, stats.total_ms
                ),
                artifact_path: None,
            });
        }

        crate::storage::record_index_run(&self.workspace_root, cfg, &stats).ok();

        Ok(Vec::new()) // symbols are now queried from DB
    }

    fn search_cached_only(
        &self,
        query_lower: &str,
        kind_filter: Option<SymbolKind>,
        cfg: &CurdConfig,
    ) -> Vec<Symbol> {
        let storage = match Storage::open(&self.workspace_root, cfg) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        
        let mut stmt = match storage.conn.prepare(
            "SELECT id, name, kind, role, link_name, filepath, start_byte, end_byte, start_line, end_line, semantic_hash, fault_id 
             FROM symbols WHERE name LIKE ?1"
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        
        let query_param = format!("%{}%", query_lower);
        let rows = stmt.query_map(params![query_param], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let kind_str: String = row.get(2)?;
            let role_str: String = row.get(3)?;
            let link_name: Option<String> = row.get(4)?;
            let filepath_str: String = row.get(5)?;
            let start_byte: i64 = row.get(6)?;
            let end_byte: i64 = row.get(7)?;
            let start_line: i64 = row.get(8)?;
            let end_line: i64 = row.get(9)?;
            let semantic_hash: Option<String> = row.get(10)?;
            let fault_id: Option<String> = row.get(11)?;

            let kind = match kind_str.as_str() {
                "function" => SymbolKind::Function,
                "class" => SymbolKind::Class,
                "struct" => SymbolKind::Struct,
                "interface" => SymbolKind::Interface,
                "module" => SymbolKind::Module,
                "variable" => SymbolKind::Variable,
                "method" => SymbolKind::Method,
                _ => SymbolKind::Unknown,
            };
            let role = match role_str.as_str() {
                "stub" => SymbolRole::Stub,
                "reference" => SymbolRole::Reference,
                _ => SymbolRole::Definition,
            };
            
            Ok(Symbol {
                id,
                name,
                kind,
                role,
                link_name,
                filepath: PathBuf::from(filepath_str),
                start_byte: start_byte as usize,
                end_byte: end_byte as usize,
                start_line: start_line as usize,
                end_line: end_line as usize,
                signature: None,
                semantic_hash,
                fault_id,
            })
        }).ok();

        let mut results = Vec::new();
        if let Some(rows) = rows {
            for row in rows.flatten() {
                if let Some(kind) = kind_filter.as_ref() {
                    if row.kind != *kind { continue; }
                }
                results.push(row);
            }
        }
        results
    }

    pub fn parse_file(&self, path: &Path, manager: &mut ParserManager) -> Result<Vec<Symbol>> {
        let cfg = self.config_override.clone().unwrap_or_else(|| CurdConfig::load_from_workspace(&self.workspace_root));
        let lang = self.lang_for_path(path, &cfg).ok_or_else(|| anyhow::anyhow!("Unsupported language"))?;
        let mut ctx = ThreadParseContext {
            manager: manager.clone(),
            parsers: HashMap::new(),
            queries: HashMap::new(),
        };
        self.parse_file_with_context(path, &lang, &mut ctx)
    }

    fn lang_for_path(&self, path: &Path, _cfg: &CurdConfig) -> Option<String> {
        let ext = path.extension()?.to_str()?.trim_start_matches('.').to_lowercase();
        self.registry.lang_for_extension(&ext)
    }

    pub fn parse_file_with_context(
        &self,
        file_path: &Path,
        lang_name: &str,
        ctx: &mut ThreadParseContext,
    ) -> Result<Vec<Symbol>> {
        let source_code = fs::read_to_string(file_path)?;
        if source_code.is_empty() {
            return Ok(Vec::new());
        }

        let parser = if let Some(p) = ctx.parsers.get_mut(lang_name) {
            p
        } else {
            let p = ctx.manager.create_parser(lang_name)?;
            ctx.parsers.insert(lang_name.to_string(), p);
            ctx.parsers.get_mut(lang_name).unwrap()
        };

        let query = if let Some(q) = ctx.queries.get(lang_name) {
            q
        } else {
            let q_src = ctx.manager.load_query(lang_name)?;
            if q_src.is_empty() {
                return Ok(Vec::new());
            }
            let q = tree_sitter::Query::new(&parser.language().unwrap(), &q_src)?;
            ctx.queries.insert(lang_name.to_string(), q);
            ctx.queries.get(lang_name).unwrap()
        };

        let tree = parser
            .parse(&source_code, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file"))?;

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut captures_iter = cursor.captures(query, tree.root_node(), source_code.as_bytes());

        let mut symbols_list = Vec::new();
        let rel_path = file_path
            .strip_prefix(&self.workspace_root)
            .unwrap_or(file_path)
            .to_path_buf();

        let mut name_counts: HashMap<String, usize> = HashMap::new();

        while let Some((mat, cap_idx)) = captures_iter.next() {
            let cap = mat.captures[*cap_idx];
            let node = cap.node;
            let capture_name = &query.capture_names()[cap.index as usize];
            
            let mut role = SymbolRole::Definition;
            let link_name = None;
            
            match &capture_name[..] {
                "stub" => role = SymbolRole::Stub,
                "def" | "definition" => role = SymbolRole::Definition,
                "ref" | "reference" => role = SymbolRole::Reference,
                "name" => continue, // Skip @name captures as they are auxiliary
                _ => {}
            }

            // Heuristic for kind based on node kind
            let symbol_kind = match node.kind() {
                "function_item" | "function_declaration" | "function_definition" | "method_declaration" | "method_definition" | "function" => SymbolKind::Function,
                "class_declaration" | "class_definition" | "class" => SymbolKind::Class,
                "struct_item" | "struct_specifier" | "struct" => SymbolKind::Struct,
                "interface_declaration" | "interface" | "type_alias_declaration" => SymbolKind::Interface,
                _ => SymbolKind::Unknown,
            };

            // For captures, we often just want the name of the node itself if it's an identifier,
            // or we look for a child named 'name' or 'identifier'
            let mut symbol_name = node.utf8_text(source_code.as_bytes()).unwrap_or("unknown").to_string();
            
            // If the node is a large block, try to find a better name
            if symbol_name.len() > 64 || symbol_name.contains('{') {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if let Ok(name_text) = name_node.utf8_text(source_code.as_bytes()) {
                        symbol_name = name_text.to_string();
                    }
                }
            }

            let range = node.range();
            let symbol_code = &source_code[range.start_byte..range.end_byte.min(source_code.len())];
            let hash_str = format!("{:x}", sha2::Sha256::digest(symbol_code.as_bytes()));

            let count = name_counts.entry(symbol_name.clone()).or_insert(0);
            let id = if *count == 0 {
                format!("{}::{}", rel_path.to_string_lossy(), symbol_name)
            } else {
                format!("{}::{}::#{}", rel_path.to_string_lossy(), symbol_name, count)
            };
            *count += 1;

            symbols_list.push(Symbol {
                id,
                name: symbol_name,
                kind: symbol_kind,
                role,
                link_name,
                filepath: rel_path.clone(),
                start_byte: range.start_byte,
                end_byte: range.end_byte,
                start_line: range.start_point.row + 1,
                end_line: range.end_point.row + 1,
                signature: None,
                semantic_hash: Some(hash_str),
                fault_id: None,
            });
        }

        Ok(symbols_list)
    }
}

pub fn run_index_worker(req: IndexWorkerRequest) -> Result<IndexWorkerResponse> {
    let se = SearchEngine::new(&req.workspace_root);

    let manager = ParserManager::new_with_backend(
        PathBuf::from(&req.workspace_root).join(".curd/grammars"),
        req.parser_backend.clone(),
    )?;
    
    let mut entries = Vec::new();
    let mut unsupported_lang = 0;
    let mut skipped_too_large = 0;
    let mut large_file_skeleton = 0;
    let mut large_file_full = 0;
    let mut fast_prefilter_skips = 0;
    let mut parse_fail = 0;
    let mut parse_fail_samples = Vec::new();
    let mut no_symbols = 0;
    let mut native_files = 0;
    let mut wasm_files = 0;
    let mut native_fallbacks = 0;

    let mut ctx = ThreadParseContext {
        manager: manager.clone(),
        parsers: HashMap::new(),
        queries: HashMap::new(),
    };

    let cfg = CurdConfig::load_from_workspace(Path::new(&req.workspace_root));

    for rel in req.files {
        let full_path = Path::new(&req.workspace_root).join(&rel);
        let (mtime_ms, file_size) = file_meta(&full_path).unwrap_or((0, 0));

        let Some(lang_name) = se.lang_for_path(&full_path, &cfg) else {
            unsupported_lang += 1;
            entries.push(IndexWorkerEntry {
                rel,
                mtime_ms,
                file_size,
                symbols: Vec::new(),
            });
            continue;
        };

        if file_size > req.max_file_size {
            match req.large_file_policy.as_str() {
                "full" => {
                    large_file_full += 1;
                }
                "skeleton" => {
                    large_file_skeleton += 1;
                    let _symbols = extract_skeleton_symbols(&full_path, Path::new(&req.workspace_root), &lang_name, None);
                    entries.push(IndexWorkerEntry {
                        rel,
                        mtime_ms,
                        file_size,
                        symbols: Vec::new(),
                    });
                    continue;
                }
                _ => {
                    skipped_too_large += 1;
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

        if let Some(hint) = req.query_hint.as_ref() {
            if !file_contains_case_insensitive(&full_path, hint) {
                fast_prefilter_skips += 1;
                entries.push(IndexWorkerEntry {
                    rel,
                    mtime_ms,
                    file_size,
                    symbols: Vec::new(),
                });
                continue;
            }
        }

        match se.parse_file_with_context(&full_path, &lang_name, &mut ctx) {
            Ok(symbols) => {
                if symbols.is_empty() {
                    no_symbols += 1;
                }
                match ctx.manager.resolved_backend_for_language(&lang_name).as_deref() {
                    Some("native") => native_files += 1,
                    Some("wasm") => {
                        wasm_files += 1;
                        if req.parser_backend == "native" {
                            native_fallbacks += 1;
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
            Err(_) => {
                parse_fail += 1;
                parse_fail_samples.push(format!("{}:{}", lang_name, rel));
                entries.push(IndexWorkerEntry {
                    rel,
                    mtime_ms,
                    file_size,
                    symbols: Vec::new(),
                });
            }
        }
    }

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

fn large_file_policy(cfg: &CurdConfig) -> String {
    cfg.index
        .large_file_policy
        .clone()
        .or_else(|| std::env::var("CURD_LARGE_FILE_POLICY").ok())
        .unwrap_or_else(|| "skip".to_string())
}

fn index_execution_model(cfg: &CurdConfig) -> String {
    cfg.index
        .execution
        .clone()
        .or_else(|| std::env::var("CURD_INDEX_EXECUTION").ok())
        .unwrap_or_else(|| "multiprocess".to_string())
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

fn file_meta(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dur = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some((dur.as_secs(), meta.len()))
}

fn file_contains_case_insensitive(path: &Path, hint: &str) -> bool {
    if let Ok(content) = fs::read_to_string(path) {
        return content.to_lowercase().contains(hint);
    }
    false
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

fn extract_skeleton_symbols(_path: &Path, _root: &Path, _lang: &str, _compute: Option<&str>) -> Vec<Symbol> {
    Vec::new()
}
