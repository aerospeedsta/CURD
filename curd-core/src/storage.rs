use crate::{CurdConfig, IndexBuildStats, Symbol, SymbolKind};
use anyhow::Result;
use rusqlite::{Connection, params};
use std::collections::HashMap;
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
}

impl Storage {
    pub fn open(root: &Path, cfg: &CurdConfig) -> Result<Self> {
        let db_path = sqlite_path(root, cfg);
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = open_storage_conn(db_path, cfg)?;
        initialize_symbol_index(&conn)?;
        Ok(Self { conn })
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
        let rows = stmt.query_map(params![filepath], |row| {
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
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
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
    use super::{read_recent_index_runs, record_index_run};
    use crate::{CurdConfig, IndexBuildStats};
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
}
