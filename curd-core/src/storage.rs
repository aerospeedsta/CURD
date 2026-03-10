use crate::{CurdConfig, IndexBuildStats, Symbol, SymbolKind};
use anyhow::Result;
use rusqlite::{Connection, Row, params};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexRunRecord {
    pub ts_unix: i64,
    pub index_mode: String,
    pub parser_backend_requested: String,
    pub parser_backend_effective: String,
    pub execution_model: String,
    pub total_files: i64,
    pub cache_hits: i64,
    pub cache_misses: i64,
    pub native_files: i64,
    pub wasm_files: i64,
    pub native_fallbacks: i64,
    pub parse_fail: i64,
    pub no_symbols: i64,
    pub total_ms: i64,
}

#[derive(Clone)]
struct StoredSymbol {
    id: String,
    name: String,
    kind: String,
    role: String,
    link_name: Option<String>,
    filepath: PathBuf,
    start_byte: usize,
    end_byte: usize,
}

const INDEXED_STUB_LINK_SOURCE: &str = "indexed:stub_link";
const INDEXED_STUB_NAME_SOURCE: &str = "indexed:stub_name";
const INDEXED_CALL_SCAN_SOURCE: &str = "indexed:call_scan";
const INDEXED_IMPORT_SCAN_SOURCE: &str = "indexed:import_scan";
const INDEXED_CONTAINS_SOURCE: &str = "indexed:contains";
const INDEXED_DECLARES_SOURCE: &str = "indexed:declares";
const INDEXED_OWNS_MEMBER_SOURCE: &str = "indexed:owns_member";
const INDEXED_BRIDGE_SOURCE: &str = "indexed:bridge";

/// Initialize the symbol index and graph tables in the SQLite database.
pub fn initialize_symbol_index(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS files (
    filepath TEXT PRIMARY KEY,
    mtime_ms INTEGER NOT NULL,
    file_size INTEGER NOT NULL,
    content_hash TEXT
);

CREATE TABLE IF NOT EXISTS symbols (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    role TEXT NOT NULL,
    link_name TEXT,
    filepath TEXT NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    semantic_hash TEXT,
    fault_id TEXT,
    FOREIGN KEY(filepath) REFERENCES files(filepath) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_filepath ON symbols(filepath);

CREATE TABLE IF NOT EXISTS edges (
    caller_id TEXT NOT NULL,
    callee_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    tier TEXT NOT NULL DEFAULT 'semantic',
    confidence REAL NOT NULL DEFAULT 1.0,
    source TEXT,
    evidence TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY(caller_id, callee_id, kind),
    FOREIGN KEY(caller_id) REFERENCES symbols(id) ON DELETE CASCADE,
    FOREIGN KEY(callee_id) REFERENCES symbols(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_edges_caller ON edges(caller_id);
CREATE INDEX IF NOT EXISTS idx_edges_callee ON edges(callee_id);

CREATE TABLE IF NOT EXISTS index_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts_unix INTEGER NOT NULL,
  index_mode TEXT NOT NULL,
  parser_backend_requested TEXT NOT NULL,
  parser_backend_effective TEXT NOT NULL,
  execution_model TEXT NOT NULL,
  total_files INTEGER NOT NULL,
  cache_hits INTEGER NOT NULL,
  cache_misses INTEGER NOT NULL,
  native_files INTEGER NOT NULL,
  wasm_files INTEGER NOT NULL,
  native_fallbacks INTEGER NOT NULL,
  parse_fail INTEGER NOT NULL,
  no_symbols INTEGER NOT NULL,
  total_ms INTEGER NOT NULL
);

PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
"#,
    )?;
    ensure_edge_columns(conn)?;
    Ok(())
}

fn ensure_edge_columns(conn: &Connection) -> Result<()> {
    let mut existing = HashSet::new();
    let mut stmt = conn.prepare("PRAGMA table_info(edges)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        existing.insert(row?);
    }
    if !existing.contains("tier") {
        conn.execute("ALTER TABLE edges ADD COLUMN tier TEXT NOT NULL DEFAULT 'semantic'", [])?;
    }
    if !existing.contains("confidence") {
        conn.execute("ALTER TABLE edges ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0", [])?;
    }
    if !existing.contains("source") {
        conn.execute("ALTER TABLE edges ADD COLUMN source TEXT", [])?;
    }
    if !existing.contains("evidence") {
        conn.execute("ALTER TABLE edges ADD COLUMN evidence TEXT NOT NULL DEFAULT '[]'", [])?;
    }
    Ok(())
}

pub fn record_index_run(root: &Path, cfg: &CurdConfig, stats: &IndexBuildStats) -> Result<()> {
    if !cfg.storage.enabled {
        return Ok(());
    }
    let db_path = sqlite_path(root, cfg);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = open_storage_conn(db_path, cfg)?;
    initialize_symbol_index(&conn)?;
    
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    conn.execute(
        r#"
INSERT INTO index_runs (
  ts_unix, index_mode, parser_backend_requested, parser_backend_effective, execution_model,
  total_files, cache_hits, cache_misses, native_files, wasm_files, native_fallbacks,
  parse_fail, no_symbols, total_ms
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
"#,
        params![
            ts,
            stats.index_mode,
            stats.parser_backend,
            stats.parser_backend_effective,
            stats.execution_model,
            stats.total_files as i64,
            stats.cache_hits as i64,
            stats.cache_misses as i64,
            stats.native_files as i64,
            stats.wasm_files as i64,
            stats.native_fallbacks as i64,
            stats.parse_fail as i64,
            stats.no_symbols as i64,
            stats.total_ms as i64
        ],
    )?;
    Ok(())
}

/// A connection to the CURD SQLite storage
pub struct Storage {
    pub conn: Connection,
    pub workspace_root: PathBuf,
}

impl Storage {
    pub fn open(root: &Path, cfg: &CurdConfig) -> Result<Self> {
        let db_path = sqlite_path(root, cfg);
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = open_storage_conn(db_path, cfg)?;
        initialize_symbol_index(&conn)?;
        Ok(Self {
            conn,
            workspace_root: root.to_path_buf(),
        })
    }

    pub fn delete_file_data(&self, filepath: &str) -> Result<()> {
        self.conn.execute("DELETE FROM files WHERE filepath = ?1", params![filepath])?;
        Ok(())
    }

    pub fn upsert_file(&self, filepath: &str, mtime_ms: u64, file_size: u64, content_hash: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO files (filepath, mtime_ms, file_size, content_hash) 
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(filepath) DO UPDATE SET 
                mtime_ms = excluded.mtime_ms,
                file_size = excluded.file_size,
                content_hash = excluded.content_hash",
            params![filepath, mtime_ms as i64, file_size as i64, content_hash],
        )?;
        Ok(())
    }

    pub fn insert_symbols(&self, symbols: &[Symbol]) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT OR REPLACE INTO symbols 
             (id, name, kind, role, link_name, filepath, start_byte, end_byte, start_line, end_line, semantic_hash, fault_id) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        for s in symbols {
            let kind_str = match s.kind {
                SymbolKind::Function => "function",
                SymbolKind::Class => "class",
                SymbolKind::Struct => "struct",
                SymbolKind::Interface => "interface",
                SymbolKind::Module => "module",
                SymbolKind::Variable => "variable",
                SymbolKind::Method => "method",
                SymbolKind::Unknown => "unknown",
            };
            let role_str = match s.role {
                crate::symbols::SymbolRole::Definition => "definition",
                crate::symbols::SymbolRole::Stub => "stub",
                crate::symbols::SymbolRole::Reference => "reference",
            };
            
            // SECURITY: Ensure absolute path for storage
            let abs_filepath = if s.filepath.is_absolute() {
                s.filepath.clone()
            } else {
                // If relative, assume it belongs to current workspace being indexed
                // But best if caller provides absolute.
                s.filepath.clone()
            };

            stmt.execute(params![
                s.id,
                s.name,
                kind_str,
                role_str,
                s.link_name,
                abs_filepath.to_string_lossy(),
                s.start_byte as i64,
                s.end_byte as i64,
                s.start_line as i64,
                s.end_line as i64,
                s.semantic_hash,
                s.fault_id
            ])?;
        }
        Ok(())
    }

    pub fn get_file_meta(&self, filepath: &str) -> Result<Option<(u64, u64)>> {
        let mut stmt = self.conn.prepare("SELECT mtime_ms, file_size FROM files WHERE filepath = ?1")?;
        let mut rows = stmt.query_map(params![filepath], |row| {
            Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64))
        })?;
        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_file_meta(&self) -> Result<HashMap<String, (u64, u64)>> {
        let mut stmt = self.conn.prepare("SELECT filepath, mtime_ms, file_size FROM files")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (row.get::<_, i64>(1)? as u64, row.get::<_, i64>(2)? as u64),
            ))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (path, meta) = row?;
            out.insert(path, meta);
        }
        Ok(out)
    }

    pub fn get_symbols_for_file(&self, filepath: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, role, link_name, filepath, start_byte, end_byte, start_line, end_line, semantic_hash, fault_id 
             FROM symbols WHERE filepath = ?1",
        )?;
        let rows = stmt.query_map(params![filepath], decode_symbol_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_symbols_by_ids(&self, ids: &[String]) -> Result<Vec<Symbol>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(200) {
            if chunk.is_empty() {
                continue;
            }
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT id, name, kind, role, link_name, filepath, start_byte, end_byte, start_line, end_line, semantic_hash, fault_id \
                 FROM symbols WHERE id IN ({placeholders})"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(chunk.iter().map(|id| id.as_str())),
                decode_symbol_row,
            )?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn get_symbol_at_line(&self, filepath: &str, line: usize) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM symbols WHERE filepath = ?1 AND start_line <= ?2 AND end_line >= ?2 ORDER BY (end_line - start_line) ASC LIMIT 1"
        )?;
        let mut rows = stmt.query_map(params![filepath, line as i64], |row| row.get::<_, String>(0))?;
        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    pub fn commit_indexing_results(&mut self, entries: &[crate::search::IndexWorkerEntry]) -> Result<()> {
        let conn = &mut self.conn;
        conn.execute("PRAGMA foreign_keys = OFF", [])?;
        conn.execute("PRAGMA synchronous = OFF", [])?;
        
        let tx = conn.transaction()?;
        {
            let mut symbol_stmt = tx.prepare(
                "INSERT OR REPLACE INTO symbols 
                 (id, name, kind, role, link_name, filepath, start_byte, end_byte, start_line, end_line, semantic_hash, fault_id) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            let mut file_stmt = tx.prepare(
                "INSERT OR REPLACE INTO files (filepath, mtime_ms, file_size) VALUES (?1, ?2, ?3)"
            )?;

            for entry in entries {
                tx.execute("DELETE FROM files WHERE filepath = ?1", params![entry.rel])?;
                file_stmt.execute(params![entry.rel, entry.mtime_ms as i64, entry.file_size as i64])?;
                
                for s in &entry.symbols {
                    if s.name.contains("main") {
                    }
                    let kind_str = match s.kind {
                        SymbolKind::Function => "function",
                        SymbolKind::Class => "class",
                        SymbolKind::Struct => "struct",
                        SymbolKind::Interface => "interface",
                        SymbolKind::Module => "module",
                        SymbolKind::Variable => "variable",
                        SymbolKind::Method => "method",
                        SymbolKind::Unknown => "unknown",
                    };
                    let role_str = match s.role {
                        crate::symbols::SymbolRole::Definition => "definition",
                        crate::symbols::SymbolRole::Stub => "stub",
                        crate::symbols::SymbolRole::Reference => "reference",
                    };
                    symbol_stmt.execute(params![
                        s.id,
                        s.name,
                        kind_str,
                        role_str,
                        s.link_name,
                        entry.rel,
                        s.start_byte as i64,
                        s.end_byte as i64,
                        s.start_line as i64,
                        s.end_line as i64,
                        s.semantic_hash,
                        s.fault_id
                    ])?;
                }
            }
        }
        refresh_graph_edges_tx(&tx, &self.workspace_root, entries)?;
        tx.commit()?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        conn.execute("PRAGMA synchronous = NORMAL", [])?;
        Ok(())
    }

    pub fn annotate_symbol_fault(&self, symbol_id: &str, fault_id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE symbols SET fault_id = ?1 WHERE id = ?2",
            params![fault_id.to_string(), symbol_id],
        )?;
        Ok(())
    }

    pub fn clear_symbol_fault(&self, symbol_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE symbols SET fault_id = NULL WHERE id = ?1",
            params![symbol_id],
        )?;
        Ok(())
    }

    pub fn get_symbol_fault(&self, symbol_id: &str) -> Result<Option<Uuid>> {
        let mut stmt = self.conn.prepare("SELECT fault_id FROM symbols WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![symbol_id], |row| {
            let fid: Option<String> = row.get(0)?;
            Ok(fid.and_then(|s| Uuid::parse_str(&s).ok()))
        })?;
        if let Some(row) = rows.next() {
            Ok(row?)
        } else {
            Ok(None)
        }
    }

    pub fn compute_state_hash(&self) -> Result<String> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();

        // 1. Hash all symbols deterministically
        let mut stmt = self.conn.prepare(
            "SELECT id, semantic_hash FROM symbols ORDER BY id ASC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        for row in rows {
            let (id, hash) = row?;
            hasher.update(id.as_bytes());
            if let Some(h) = hash {
                hasher.update(h.as_bytes());
            }
        }

        // 2. Hash all edges deterministically
        let mut stmt = self.conn.prepare(
            "SELECT caller_id, callee_id, kind, tier, confidence, COALESCE(source, ''), COALESCE(evidence, '[]') FROM edges ORDER BY caller_id ASC, callee_id ASC, kind ASC, tier ASC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;

        for row in rows {
            let (from, to, kind, tier, confidence, source, evidence) = row?;
            hasher.update(from.as_bytes());
            hasher.update(to.as_bytes());
            hasher.update(kind.as_bytes());
            hasher.update(tier.as_bytes());
            hasher.update(confidence.to_le_bytes());
            hasher.update(source.as_bytes());
            hasher.update(evidence.as_bytes());
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}

fn decode_symbol_row(row: &Row<'_>) -> rusqlite::Result<Symbol> {
    let kind_str: String = row.get(2)?;
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
    let role_str: String = row.get(3)?;
    let role = match role_str.as_str() {
        "stub" => crate::symbols::SymbolRole::Stub,
        "reference" => crate::symbols::SymbolRole::Reference,
        _ => crate::symbols::SymbolRole::Definition,
    };
    Ok(Symbol {
        id: row.get(0)?,
        name: row.get(1)?,
        kind,
        role,
        link_name: row.get(4)?,
        filepath: PathBuf::from(row.get::<_, String>(5)?),
        start_byte: row.get::<_, i64>(6)? as usize,
        end_byte: row.get::<_, i64>(7)? as usize,
        start_line: row.get::<_, i64>(8)? as usize,
        end_line: row.get::<_, i64>(9)? as usize,
        signature: None,
        semantic_hash: row.get(10)?,
        fault_id: row.get(11)?,
    })
}

fn refresh_graph_edges_tx(
    tx: &rusqlite::Transaction<'_>,
    workspace_root: &Path,
    entries: &[crate::search::IndexWorkerEntry],
) -> Result<()> {
    let mut stmt = tx.prepare(
        "SELECT id, name, kind, role, link_name, filepath, start_byte, end_byte FROM symbols"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(StoredSymbol {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            role: row.get(3)?,
            link_name: row.get(4)?,
            filepath: PathBuf::from(row.get::<_, String>(5)?),
            start_byte: row.get::<_, i64>(6)? as usize,
            end_byte: row.get::<_, i64>(7)? as usize,
        })
    })?;
    let mut symbols = Vec::new();
    for row in rows {
        symbols.push(row?);
    }

    let mut by_name: HashMap<String, Vec<StoredSymbol>> = HashMap::new();
    let mut by_link: HashMap<String, Vec<StoredSymbol>> = HashMap::new();
    let mut by_file: HashMap<PathBuf, Vec<StoredSymbol>> = HashMap::new();
    for sym in &symbols {
        by_name.entry(sym.name.clone()).or_default().push(sym.clone());
        if let Some(link) = &sym.link_name {
            by_link.entry(link.clone()).or_default().push(sym.clone());
        }
        by_file.entry(sym.filepath.clone()).or_default().push(sym.clone());
    }

    let impacted = collect_impacted_symbols(entries, &symbols, &by_name, &by_link);
    if impacted.is_empty() {
        return Ok(());
    }

    let mut delete_stmt =
        tx.prepare("DELETE FROM edges WHERE caller_id = ?1 OR callee_id = ?1")?;
    for symbol_id in impacted.iter().map(|sym| sym.id.as_str()) {
        delete_stmt.execute(params![symbol_id])?;
    }

    let mut edge_stmt = tx.prepare(
        "INSERT OR REPLACE INTO edges (caller_id, callee_id, kind, tier, confidence, source, evidence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    )?;

    for sym in &impacted {
        if sym.role != "stub" {
            continue;
        }
        let mut linked = false;
        if let Some(link) = &sym.link_name
            && let Some(targets) = by_link.get(link)
        {
            for target in targets {
                if target.id != sym.id && target.role == "definition" {
                    edge_stmt.execute(params![
                        sym.id,
                        target.id,
                        "universal_link",
                        "structural",
                        0.98_f64,
                        INDEXED_STUB_LINK_SOURCE,
                        evidence_json(&["indexed", "stub_link_match"]),
                    ])?;
                    linked = true;
                }
            }
        }
        if !linked
            && let Some(targets) = by_name.get(&sym.name)
        {
            for target in targets {
                if target.id != sym.id && target.role == "definition" {
                    edge_stmt.execute(params![
                        sym.id,
                        target.id,
                        "universal_name",
                        "structural",
                        0.85_f64,
                        INDEXED_STUB_NAME_SOURCE,
                        evidence_json(&["indexed", "stub_name_match"]),
                    ])?;
                }
            }
        }
    }

    let keywords: HashSet<&str> = [
        "if", "while", "for", "match", "return", "let", "fn", "impl", "mod", "use", "pub",
        "struct", "enum", "type", "with", "class", "def", "import", "from", "in", "not", "and",
        "or", "is", "assert", "del", "pass", "raise", "except", "try", "elif", "else", "loop",
        "where",
    ]
    .into_iter()
    .collect();

    for (path, file_symbols) in by_file {
        let resolved_path = if path.is_absolute() {
            path.clone()
        } else {
            workspace_root.join(&path)
        };
        let source = match fs::read_to_string(&resolved_path) {
            Ok(src) => src,
            Err(_) => continue,
        };
        let containment_edges = resolve_containment_edges(&file_symbols);
        for (parent, child, confidence, evidence) in containment_edges {
            edge_stmt.execute(params![
                parent,
                child,
                "contains",
                "structural",
                confidence,
                INDEXED_CONTAINS_SOURCE,
                serde_json::to_string(&evidence).unwrap_or_else(|_| "[]".to_string()),
            ])?;
        }
        let declaration_edges = resolve_module_declaration_edges(&file_symbols);
        for (parent, child, confidence, evidence) in declaration_edges {
            edge_stmt.execute(params![
                parent,
                child,
                "declares",
                "structural",
                confidence,
                INDEXED_DECLARES_SOURCE,
                serde_json::to_string(&evidence).unwrap_or_else(|_| "[]".to_string()),
            ])?;
        }
        let member_edges = resolve_member_ownership_edges(&file_symbols);
        for (parent, child, confidence, evidence) in member_edges {
            edge_stmt.execute(params![
                parent,
                child,
                "owns_member",
                "structural",
                confidence,
                INDEXED_OWNS_MEMBER_SOURCE,
                serde_json::to_string(&evidence).unwrap_or_else(|_| "[]".to_string()),
            ])?;
        }
        let import_targets =
            resolve_import_edges(&source, &file_symbols, &by_name);
        for sym in &file_symbols {
            if sym.role != "definition" || !impacted.iter().any(|candidate| candidate.id == sym.id) {
                continue;
            }
            for (target, confidence, evidence) in &import_targets {
                if target != &sym.id {
                    edge_stmt.execute(params![
                        sym.id,
                        target,
                        "imports",
                        "structural",
                        confidence,
                        INDEXED_IMPORT_SCAN_SOURCE,
                        serde_json::to_string(evidence).unwrap_or_else(|_| "[]".to_string()),
                    ])?;
                }
            }
        }
        for sym in file_symbols {
            if !impacted.iter().any(|candidate| candidate.id == sym.id) {
                continue;
            }
            if sym.start_byte >= source.len() || sym.end_byte > source.len() || sym.start_byte >= sym.end_byte {
                continue;
            }
            let snippet = &source[sym.start_byte..sym.end_byte];
            let caller_parent = sym.filepath.parent();
            let caller_ext = sym.filepath.extension().and_then(|e| e.to_str()).unwrap_or_default();
            for (idx, _) in snippet.match_indices('(') {
                if idx == 0 {
                    continue;
                }
                let name = get_name_before_paren(snippet, idx);
                if name.is_empty() || keywords.contains(name) {
                    continue;
                }
                for (target, confidence, evidence) in
                    resolve_call_targets(name, &sym, caller_parent, caller_ext, &by_name)
                {
                    if target != sym.id {
                        edge_stmt.execute(params![
                            sym.id,
                            target,
                            "calls",
                            "semantic",
                            confidence,
                            INDEXED_CALL_SCAN_SOURCE,
                            serde_json::to_string(&evidence).unwrap_or_else(|_| "[]".to_string()),
                        ])?;
                    }
                }
            }
        }
    }

    for sym in &impacted {
        let path = sym.filepath.to_string_lossy();
        let is_wrapper = path.contains("curd-node/")
            || path.contains("curd-python/")
            || path.contains("curd/");
        if !is_wrapper {
            continue;
        }
        if let Some(targets) = by_name.get(&sym.name) {
            for target in targets {
                if target.id != sym.id && target.id.contains("curd-core/") {
                    edge_stmt.execute(params![
                        sym.id,
                        target.id,
                        "bridge",
                        "structural",
                        0.92_f64,
                        INDEXED_BRIDGE_SOURCE,
                        evidence_json(&["indexed", "cross_language_bridge"]),
                    ])?;
                }
            }
        }
    }

    Ok(())
}

fn collect_impacted_symbols(
    entries: &[crate::search::IndexWorkerEntry],
    all_symbols: &[StoredSymbol],
    by_name: &HashMap<String, Vec<StoredSymbol>>,
    by_link: &HashMap<String, Vec<StoredSymbol>>,
) -> Vec<StoredSymbol> {
    let mut impacted_ids = HashSet::new();
    let mut impacted_files = HashSet::new();
    let mut impacted_names = HashSet::new();
    let mut impacted_links = HashSet::new();

    for entry in entries {
        impacted_files.insert(PathBuf::from(&entry.rel));
        for symbol in &entry.symbols {
            impacted_ids.insert(symbol.id.clone());
            impacted_names.insert(symbol.name.clone());
            if let Some(link) = &symbol.link_name {
                impacted_links.insert(link.clone());
            }
        }
    }

    for symbol in all_symbols {
        if impacted_files.contains(&symbol.filepath) {
            impacted_ids.insert(symbol.id.clone());
        }
    }
    for name in impacted_names {
        if let Some(symbols) = by_name.get(&name) {
            for symbol in symbols {
                impacted_ids.insert(symbol.id.clone());
            }
        }
    }
    for link in impacted_links {
        if let Some(symbols) = by_link.get(&link) {
            for symbol in symbols {
                impacted_ids.insert(symbol.id.clone());
            }
        }
    }

    all_symbols
        .iter()
        .filter(|symbol| impacted_ids.contains(&symbol.id))
        .cloned()
        .collect()
}

fn resolve_containment_edges(
    file_symbols: &[StoredSymbol],
) -> Vec<(String, String, f64, Vec<String>)> {
    let mut edges = Vec::new();
    for child in file_symbols {
        if child.role != "definition" {
            continue;
        }
        let parent = file_symbols
            .iter()
            .filter(|parent| {
                parent.id != child.id
                    && parent.role == "definition"
                    && is_container_kind(&parent.kind)
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

fn is_container_kind(kind: &str) -> bool {
    matches!(kind, "module" | "class" | "struct" | "interface")
}

fn containment_confidence(parent: &StoredSymbol, child: &StoredSymbol) -> f64 {
    let mut confidence = 0.90_f64;
    if parent.kind == "module" {
        confidence += 0.02;
    }
    if child.kind == "method" || child.kind == "function" {
        confidence += 0.03;
    }
    confidence.clamp(0.90, 0.98)
}

fn containment_evidence(parent: &StoredSymbol, child: &StoredSymbol) -> Vec<String> {
    let mut evidence = vec![
        "indexed".to_string(),
        "nested_span".to_string(),
        "same_file".to_string(),
        format!("parent_kind:{}", parent.kind),
        format!("child_kind:{}", child.kind),
    ];
    if parent.kind == "module" {
        evidence.push("module_scope".to_string());
    }
    evidence
}

fn resolve_module_declaration_edges(
    file_symbols: &[StoredSymbol],
) -> Vec<(String, String, f64, Vec<String>)> {
    resolve_containment_edges(file_symbols)
        .into_iter()
        .filter_map(|(parent, child, confidence, mut evidence)| {
            let parent_sym = file_symbols.iter().find(|sym| sym.id == parent)?;
            if parent_sym.kind != "module" {
                return None;
            }
            evidence.push("module_declaration".to_string());
            Some((parent, child, confidence.clamp(0.93, 0.99), evidence))
        })
        .collect()
}

fn resolve_member_ownership_edges(
    file_symbols: &[StoredSymbol],
) -> Vec<(String, String, f64, Vec<String>)> {
    resolve_containment_edges(file_symbols)
        .into_iter()
        .filter_map(|(parent, child, confidence, mut evidence)| {
            let parent_sym = file_symbols.iter().find(|sym| sym.id == parent)?;
            let child_sym = file_symbols.iter().find(|sym| sym.id == child)?;
            if !matches!(parent_sym.kind.as_str(), "class" | "struct" | "interface") {
                return None;
            }
            if !matches!(child_sym.kind.as_str(), "method" | "function") {
                return None;
            }
            evidence.push("member_ownership".to_string());
            evidence.push(format!("owner_kind:{}", parent_sym.kind));
            Some((parent, child, confidence.clamp(0.94, 0.99), evidence))
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
    caller: &StoredSymbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    by_name: &HashMap<String, Vec<StoredSymbol>>,
) -> Vec<(String, f64, Vec<String>)> {
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
            .map(|s| {
                (
                    s.id.clone(),
                    score_call_confidence(called, caller, caller_parent, caller_ext, s, true),
                    call_edge_evidence(called, caller, caller_parent, caller_ext, s, true),
                )
            })
            .collect();
    }
    let mut scored: Vec<(i32, String)> = Vec::with_capacity(candidates.len());
    for sym in candidates {
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
        if called.contains("::") || called.contains('.') {
            if sym.id.contains(called) || sym.filepath.to_string_lossy().contains(called) {
                score += 15;
            }
        }
        scored.push((score, sym.id.clone()));
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
            let target = candidates.iter().find(|candidate| candidate.id == id)?;
            Some((
                id,
                score_call_confidence(called, caller, caller_parent, caller_ext, target, !ambiguous),
                call_edge_evidence(called, caller, caller_parent, caller_ext, target, !ambiguous),
            ))
        })
        .collect()
}

fn resolve_import_edges(
    source: &str,
    file_symbols: &[StoredSymbol],
    by_name: &HashMap<String, Vec<StoredSymbol>>,
) -> Vec<(String, f64, Vec<String>)> {
    let Some(anchor) = file_symbols
        .iter()
        .find(|sym| sym.role == "definition")
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
        let targets =
            resolve_import_targets(&import_name, anchor, caller_parent, caller_ext, by_name);
        out.extend(targets);
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

fn evidence_json(values: &[&str]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

fn call_edge_evidence(
    called: &str,
    caller: &StoredSymbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    target: &StoredSymbol,
    unique_best: bool,
) -> Vec<String> {
    let mut evidence = vec!["indexed".to_string(), "call_scan".to_string()];
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

fn resolve_import_targets(
    imported: &str,
    caller: &StoredSymbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    by_name: &HashMap<String, Vec<StoredSymbol>>,
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
        .map(|target| {
            (
                target.id.clone(),
                score_import_confidence(imported, caller, caller_parent, caller_ext, target, !ambiguous),
                import_edge_evidence(imported, caller, caller_parent, caller_ext, target, !ambiguous),
            )
        })
        .collect()
}

fn score_import_confidence(
    imported: &str,
    _caller: &StoredSymbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    target: &StoredSymbol,
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

fn import_edge_evidence(
    imported: &str,
    _caller: &StoredSymbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    target: &StoredSymbol,
    unique_best: bool,
) -> Vec<String> {
    let mut evidence = vec!["indexed".to_string(), "import_scan".to_string()];
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
        let prefix = clause.split('{').next().unwrap_or("").trim_end_matches("::");
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
        vec![clause
            .split_whitespace()
            .next()
            .unwrap_or(clause)
            .trim()
            .to_string()]
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

fn score_call_confidence(
    called: &str,
    caller: &StoredSymbol,
    caller_parent: Option<&Path>,
    caller_ext: &str,
    target: &StoredSymbol,
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

pub fn read_recent_index_runs(
    root: &Path,
    cfg: &CurdConfig,
    limit: usize,
) -> Result<Vec<IndexRunRecord>> {
    if !cfg.storage.enabled {
        return Ok(Vec::new());
    }
    let db_path = sqlite_path(root, cfg);
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = open_storage_conn(db_path, cfg)?;
    let mut stmt = conn.prepare(
        r#"
SELECT
  ts_unix,
  index_mode,
  parser_backend_requested,
  parser_backend_effective,
  execution_model,
  total_files,
  cache_hits,
  cache_misses,
  native_files,
  wasm_files,
  native_fallbacks,
  parse_fail,
  no_symbols,
  total_ms
FROM index_runs
ORDER BY id DESC
LIMIT ?1
"#,
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(IndexRunRecord {
            ts_unix: row.get(0)?,
            index_mode: row.get(1)?,
            parser_backend_requested: row.get(2)?,
            parser_backend_effective: row.get(3)?,
            execution_model: row.get(4)?,
            total_files: row.get(5)?,
            cache_hits: row.get(6)?,
            cache_misses: row.get(7)?,
            native_files: row.get(8)?,
            wasm_files: row.get(9)?,
            native_fallbacks: row.get(10)?,
            parse_fail: row.get(11)?,
            no_symbols: row.get(12)?,
            total_ms: row.get(13)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn sqlite_path(root: &Path, cfg: &CurdConfig) -> PathBuf {
    let curd_dir = crate::workspace::get_curd_dir(root);
    let rel = cfg
        .storage
        .sqlite_path
        .clone()
        .unwrap_or_else(|| {
             curd_dir.strip_prefix(root)
                .unwrap_or(&curd_dir)
                .join("curd_state.sqlite3")
                .to_string_lossy()
                .to_string()
        });
    crate::workspace::validate_sandboxed_path(root, &rel).unwrap_or_else(|_| {
        // Keep deterministic fallback inside workspace if an unsafe path is configured.
        curd_dir.join("curd_state.sqlite3")
    })
}

fn open_storage_conn(db_path: PathBuf, cfg: &CurdConfig) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    if cfg
        .storage
        .encryption_mode
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("sqlcipher"))
    {
        let env_name = cfg
            .storage
            .key_env
            .as_deref()
            .unwrap_or("CURD_DB_KEY")
            .to_string();
        let key = std::env::var(&env_name).map_err(|_| {
            anyhow::anyhow!(
                "SQLCipher mode requested but key env '{}' is not set",
                env_name
            )
        })?;
        if key.is_empty() {
            anyhow::bail!(
                "SQLCipher mode requested but key env '{}' is empty",
                env_name
            );
        }
        // Requires SQLCipher-enabled SQLite build.
        conn.pragma_update(None, "key", key)?;
    }
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::{Storage, read_recent_index_runs, record_index_run};
    use crate::{CurdConfig, IndexBuildStats, Symbol, SymbolKind, symbols::SymbolRole};
    use crate::search::IndexWorkerEntry;
    use rusqlite::params;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn records_index_runs_when_enabled() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        let cfg = CurdConfig::default();
        let stats = IndexBuildStats {
            index_mode: "full".into(),
            parser_backend: "wasm".into(),
            parser_backend_effective: "wasm".into(),
            compute_backend_effective: "none".into(),
            execution_model: "multithreaded".into(),
            max_file_size: 524288,
            large_file_policy: "skip".into(),
            chunk_size: 4096,
            chunk_count: 1,
            total_files: 10,
            cache_hits: 1,
            cache_misses: 9,
            unsupported_lang: 0,
            skipped_too_large: 0,
            large_file_skeleton: 0,
            large_file_full: 0,
            fast_prefilter_skips: 0,
            parse_fail: 0,
            parse_fail_samples: Vec::new(),
            native_files: 0,
            wasm_files: 9,
            native_fallbacks: 0,
            no_symbols: 0,
            scan_ms: 1,
            cache_load_ms: 1,
            parse_ms: 1,
            merge_ms: 1,
            serialize_ms: 1,
            total_ms: 5,
        };
        record_index_run(root, &cfg, &stats).expect("record");
        let db = root.join(".curd").join("curd_state.sqlite3");
        assert!(db.exists());
        let rows = read_recent_index_runs(root, &cfg, 5).expect("read rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].index_mode, "full");
    }

    #[test]
    fn sqlite_path_falls_back_when_configured_path_is_unsafe() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        let mut cfg = CurdConfig::default();
        cfg.storage.sqlite_path = Some("../outside.sqlite3".to_string());
        let stats = IndexBuildStats {
            index_mode: "full".into(),
            parser_backend: "wasm".into(),
            parser_backend_effective: "wasm".into(),
            compute_backend_effective: "none".into(),
            execution_model: "multithreaded".into(),
            max_file_size: 524288,
            large_file_policy: "skip".into(),
            chunk_size: 4096,
            chunk_count: 1,
            total_files: 1,
            cache_hits: 1,
            cache_misses: 0,
            unsupported_lang: 0,
            skipped_too_large: 0,
            large_file_skeleton: 0,
            large_file_full: 0,
            fast_prefilter_skips: 0,
            parse_fail: 0,
            parse_fail_samples: Vec::new(),
            native_files: 0,
            wasm_files: 1,
            native_fallbacks: 0,
            no_symbols: 0,
            scan_ms: 1,
            cache_load_ms: 1,
            parse_ms: 1,
            merge_ms: 1,
            serialize_ms: 1,
            total_ms: 5,
        };
        record_index_run(root, &cfg, &stats).expect("record");
        assert!(root.join(".curd").join("curd_state.sqlite3").exists());
        assert!(!root.join("../outside.sqlite3").exists());
    }

    #[test]
    fn sqlcipher_requires_key_env() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        let mut cfg = CurdConfig::default();
        cfg.storage.encryption_mode = Some("sqlcipher".to_string());
        cfg.storage.key_env = Some("CURD_TEST_SQLCIPHER_KEY".to_string());
        // SAFETY: test-local environment setup.
        unsafe {
            std::env::remove_var("CURD_TEST_SQLCIPHER_KEY");
        }
        let stats = IndexBuildStats {
            index_mode: "full".into(),
            parser_backend: "wasm".into(),
            parser_backend_effective: "wasm".into(),
            compute_backend_effective: "none".into(),
            execution_model: "multithreaded".into(),
            max_file_size: 524288,
            large_file_policy: "skip".into(),
            chunk_size: 4096,
            chunk_count: 1,
            total_files: 1,
            cache_hits: 1,
            cache_misses: 0,
            unsupported_lang: 0,
            skipped_too_large: 0,
            large_file_skeleton: 0,
            large_file_full: 0,
            fast_prefilter_skips: 0,
            parse_fail: 0,
            parse_fail_samples: Vec::new(),
            native_files: 0,
            wasm_files: 1,
            native_fallbacks: 0,
            no_symbols: 0,
            scan_ms: 1,
            cache_load_ms: 1,
            parse_ms: 1,
            merge_ms: 1,
            serialize_ms: 1,
            total_ms: 5,
        };
        let err = record_index_run(root, &cfg, &stats).expect_err("expected missing-key error");
        assert!(err.to_string().contains("SQLCipher mode requested"));
    }

    #[test]
    fn commit_indexing_results_builds_tiered_edges() {
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
        let entry = IndexWorkerEntry {
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
        };

        storage
            .commit_indexing_results(&[entry])
            .expect("commit indexing results");

        let mut stmt = storage
            .conn
            .prepare("SELECT caller_id, callee_id, kind, tier, source FROM edges ORDER BY caller_id, callee_id, kind")
            .expect("prepare query");
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .expect("query rows");
        let all: Vec<_> = rows.map(|r| r.expect("row")).collect();
        assert!(all.iter().any(|(from, to, kind, tier, source)| {
            from == "src/lib.rs::caller"
                && to == "src/lib.rs::callee"
                && kind == "calls"
                && tier == "semantic"
                && source == super::INDEXED_CALL_SCAN_SOURCE
        }));
    }

    #[test]
    fn commit_indexing_results_builds_import_edges() {
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
            .expect("commit indexing results");

        let edge: (String, String, String, String, String) = storage
            .conn
            .query_row(
                "SELECT callee_id, kind, tier, source, evidence FROM edges WHERE caller_id = ?1 AND kind = 'imports'",
                params!["src/lib.rs::caller"],
                |row| Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                )),
            )
            .expect("query import edge");
        assert_eq!(edge.0, "src/helper.rs::callee");
        assert_eq!(edge.1, "imports");
        assert_eq!(edge.2, "structural");
        assert_eq!(edge.3, super::INDEXED_IMPORT_SCAN_SOURCE);
        assert!(edge.4.contains("import_scan"));
        assert!(edge.4.contains("qualified_import"));
    }

    #[test]
    fn commit_indexing_results_builds_contains_edges() {
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
            .expect("commit indexing results");

        let edge: (String, String, String, String, String) = storage
            .conn
            .query_row(
                "SELECT callee_id, kind, tier, source, evidence FROM edges WHERE caller_id = ?1 AND kind = 'contains'",
                params!["src/lib.rs::inner"],
                |row| Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                )),
            )
            .expect("query contains edge");
        assert_eq!(edge.0, "src/lib.rs::helper");
        assert_eq!(edge.1, "contains");
        assert_eq!(edge.2, "structural");
        assert_eq!(edge.3, super::INDEXED_CONTAINS_SOURCE);
        assert!(edge.4.contains("nested_span"));
        assert!(edge.4.contains("parent_kind:module"));
    }

    #[test]
    fn commit_indexing_results_prefers_nearest_container_edge() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        let source = "mod outer {\n    mod inner {\n        fn helper() {}\n    }\n}\n";
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
                        id: "src/lib.rs::outer".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "outer".to_string(),
                        kind: SymbolKind::Module,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 0,
                        end_byte: source.len(),
                        start_line: 1,
                        end_line: 5,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                    Symbol {
                        id: "src/lib.rs::inner".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "inner".to_string(),
                        kind: SymbolKind::Module,
                        role: SymbolRole::Definition,
                        link_name: None,
                        start_byte: 16,
                        end_byte: 56,
                        start_line: 2,
                        end_line: 4,
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
                        start_byte: 36,
                        end_byte: 50,
                        start_line: 3,
                        end_line: 3,
                        signature: None,
                        semantic_hash: None,
                        fault_id: None,
                    },
                ],
            }])
            .expect("commit indexing results");

        let edges: Vec<(String, String)> = storage
            .conn
            .prepare("SELECT caller_id, callee_id FROM edges WHERE kind = 'contains' ORDER BY caller_id, callee_id")
            .expect("prepare query")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query rows")
            .map(|row| row.expect("row"))
            .collect();
        assert!(edges.contains(&(
            "src/lib.rs::outer".to_string(),
            "src/lib.rs::inner".to_string()
        )));
        assert!(edges.contains(&(
            "src/lib.rs::inner".to_string(),
            "src/lib.rs::helper".to_string()
        )));
        assert!(!edges.contains(&(
            "src/lib.rs::outer".to_string(),
            "src/lib.rs::helper".to_string()
        )));
    }

    #[test]
    fn commit_indexing_results_builds_module_declares_edges() {
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
            .expect("commit indexing results");

        let edge: (String, String, String, String, String) = storage
            .conn
            .query_row(
                "SELECT callee_id, kind, tier, source, evidence FROM edges WHERE caller_id = ?1 AND kind = 'declares'",
                params!["src/lib.rs::inner"],
                |row| Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                )),
            )
            .expect("query declares edge");
        assert_eq!(edge.0, "src/lib.rs::helper");
        assert_eq!(edge.1, "declares");
        assert_eq!(edge.2, "structural");
        assert_eq!(edge.3, super::INDEXED_DECLARES_SOURCE);
        assert!(edge.4.contains("module_declaration"));
    }

    #[test]
    fn commit_indexing_results_builds_member_ownership_edges() {
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
            .expect("commit indexing results");

        let edge: (String, String, String, String, String) = storage
            .conn
            .query_row(
                "SELECT callee_id, kind, tier, source, evidence FROM edges WHERE caller_id = ?1 AND kind = 'owns_member'",
                params!["src/lib.rs::Thing"],
                |row| Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                )),
            )
            .expect("query owns_member edge");
        assert_eq!(edge.0, "src/lib.rs::helper");
        assert_eq!(edge.1, "owns_member");
        assert_eq!(edge.2, "structural");
        assert_eq!(edge.3, super::INDEXED_OWNS_MEMBER_SOURCE);
        assert!(edge.4.contains("member_ownership"));
        assert!(edge.4.contains("owner_kind:struct"));
    }

    #[test]
    fn commit_indexing_results_refreshes_impacted_edges_incrementally() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");

        let cfg = CurdConfig::default();
        let mut storage = Storage::open(root, &cfg).expect("open storage");

        std::fs::write(
            root.join("src/lib.rs"),
            "fn callee() {}\nfn caller() { callee(); }\n",
        )
        .expect("write v1 source");
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
            .expect("commit v1");

        std::fs::write(
            root.join("src/lib.rs"),
            "fn helper() {}\nfn caller() { helper(); }\n",
        )
        .expect("write v2 source");
        storage
            .commit_indexing_results(&[IndexWorkerEntry {
                rel: "src/lib.rs".to_string(),
                mtime_ms: 2,
                file_size: 40,
                symbols: vec![
                    Symbol {
                        id: "src/lib.rs::helper".to_string(),
                        filepath: PathBuf::from("src/lib.rs"),
                        name: "helper".to_string(),
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
            .expect("commit v2");

        let mut stmt = storage
            .conn
            .prepare("SELECT caller_id, callee_id, kind FROM edges ORDER BY caller_id, callee_id, kind")
            .expect("prepare query");
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .expect("query rows");
        let all: Vec<_> = rows.map(|r| r.expect("row")).collect();
        assert!(all.iter().any(|(from, to, kind)| {
            from == "src/lib.rs::caller" && to == "src/lib.rs::helper" && kind == "calls"
        }));
        assert!(!all.iter().any(|(from, to, kind)| {
            from == "src/lib.rs::caller" && to == "src/lib.rs::callee" && kind == "calls"
        }));
    }

    #[test]
    fn commit_indexing_results_assigns_higher_confidence_to_local_calls() {
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
            .expect("commit indexing results");

        let confidence: f64 = storage
            .conn
            .query_row(
                "SELECT confidence FROM edges WHERE caller_id = ?1 AND callee_id = ?2 AND kind = 'calls'",
                params!["src/lib.rs::caller", "src/lib.rs::callee"],
                |row| row.get(0),
            )
            .expect("query confidence");
        assert!(confidence > 0.80, "expected strong local call confidence, got {confidence}");
    }
}
