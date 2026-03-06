use crate::{CurdConfig, IndexBuildStats};
use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexRunRecord {
    pub ts_unix: i64,
    pub index_mode: String,
    pub parser_backend_requested: String,
    pub parser_backend_effective: String,
    pub execution_model: String,
    pub total_files: i64,
    pub total_ms: i64,
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
    conn.execute_batch(
        r#"
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
"#,
    )?;
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
            total_ms: row.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn sqlite_path(root: &Path, cfg: &CurdConfig) -> PathBuf {
    let rel = cfg
        .storage
        .sqlite_path
        .clone()
        .unwrap_or_else(|| ".curd/curd_state.sqlite3".to_string());
    crate::workspace::validate_sandboxed_path(root, &rel).unwrap_or_else(|_| {
        // Keep deterministic fallback inside workspace if an unsafe path is configured.
        root.join(".curd").join("curd_state.sqlite3")
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
