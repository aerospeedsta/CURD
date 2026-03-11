use aho_corasick::AhoCorasick;
use anyhow::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

use crate::{ParserManager, SearchEngine, scan_workspace};
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Searches the workspace for text patterns and maps them back to their enclosing structural URIs
pub struct FindEngine {
    pub workspace_root: PathBuf,
}

impl FindEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
        }
    }

    /// Primary semantic grep capability
    pub fn find(&self, query: &str) -> Result<Value> {
        let files = scan_workspace(&self.workspace_root)?;
        let search_engine = SearchEngine::new(&self.workspace_root);
        let manager = ParserManager::new(self.workspace_root.join(".curd/grammars"))?;

        let total_files = files.len();
        let processed = AtomicUsize::new(0);

        let ac = AhoCorasick::new([query])?;

        let all_matches: Vec<Value> = files.par_iter().filter_map(|file| {
            let count = processed.fetch_add(1, Ordering::Relaxed);
            if count.is_multiple_of(1000) || count == total_files - 1 {
                // Progress logging removed for brevity in core tool
            }

            // First, see if the file contains the text at all
            let meta = std::fs::metadata(file).ok()?;
            if meta.len() > 512 * 1024 || meta.len() == 0 {
                return None;
            }
            let source_code = match std::fs::read_to_string(file) {
                Ok(c) => c,
                Err(_) => return None,
            };

            let mut file_matches: Vec<(usize, String)> = Vec::new();
            for mat in ac.find_iter(&source_code) {
                file_matches.push((mat.start(), query.to_string()));
            }

            if file_matches.is_empty() {
                return None;
            }

            let rel_path = file.strip_prefix(&self.workspace_root).unwrap_or(file);
            let rel_path_str = rel_path.to_string_lossy();

            // REUSE the symbol index if available
            let symbols = if let Some(cached) = search_engine.get_symbols_for_file(&rel_path_str) {
                cached
            } else {
                let mut local_manager = manager.clone();
                search_engine
                    .parse_file(file, &mut local_manager)
                    .unwrap_or_default()
            };

            let mut results = Vec::new();
            for (byte_offset, match_text) in file_matches {
                let mut enclosing_symbol = None;
                let mut min_size = usize::MAX;

                for sym in &symbols {
                    if byte_offset >= sym.start_byte && byte_offset <= sym.end_byte {
                        let size = sym.end_byte - sym.start_byte;
                        if size < min_size {
                            min_size = size;
                            enclosing_symbol = Some(sym);
                        }
                    }
                }

                if let Some(sym) = enclosing_symbol {
                    results.push(json!({
                        "file": rel_path_str.to_string(),
                        "uri": sym.id.clone(),
                        "match": match_text,
                        "context_type": if sym.kind == crate::SymbolKind::Function { "function" } else { "class" }
                    }));
                } else {
                    results.push(json!({
                        "file": rel_path_str.to_string(),
                        "uri": rel_path_str.to_string(),
                        "match": match_text,
                        "context_type": "module"
                    }));
                }
            }
            Some(results)
        }).flatten().collect();

        let mut unique_matches = Vec::new();
        let mut seen_uris = std::collections::HashSet::new();

        for m in all_matches {
            if let Some(uri) = m.get("uri").and_then(|v| v.as_str())
                && seen_uris.insert(uri.to_string())
            {
                unique_matches.push(m);
            }
        }

        Ok(json!({
            "results": unique_matches,
            "count": unique_matches.len()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_find_engine() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join(".curd")).unwrap();
        fs::write(root.join("curd.toml"), "").unwrap();

        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(
            src_dir.join("main.py"),
            "def target_func():\n    magic_string = 'foobar'\n    return magic_string\n",
        )
        .unwrap();

        let engine = FindEngine::new(root);

        let res = engine.find("foobar").unwrap();
        let count = res.get("count").and_then(|v| v.as_u64()).unwrap();
        assert_eq!(count, 1);

        let results = res.get("results").and_then(|v| v.as_array()).unwrap();
        let first = results.first().unwrap();

        let match_text = first.get("match").and_then(|v| v.as_str()).unwrap();
        assert_eq!(match_text, "foobar");

        let context_type = first.get("context_type").and_then(|v| v.as_str()).unwrap();
        assert_eq!(context_type, "module");
    }
}
