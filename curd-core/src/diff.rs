use crate::{ParserManager, SearchEngine, ShadowStore, Symbol};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

pub fn run_diff(root: &Path, semantic: bool, target_symbol: Option<String>) -> Result<String> {
    let mut shadow = ShadowStore::new(root);
    if !semantic {
        return Ok(shadow.diff());
    }

    if !shadow.is_active() || shadow.is_empty() {
        return Ok("No staged changes.".to_string());
    }

    let mut out = String::new();
    let Some(shadow_root) = shadow.shadow_root.clone() else {
        return Ok("No staged changes.".to_string());
    };

    let mut manager = ParserManager::new(root.join(".curd/grammars"))?;
    let search = SearchEngine::new(root);

    let staged_paths = shadow.staged_paths();

    for file_path in staged_paths {
        let rel_path = file_path.strip_prefix(root).unwrap_or(&file_path);
        let shadow_path = shadow_root.join(rel_path);

        let orig_symbols = if file_path.exists() {
            search
                .parse_file(&file_path, &mut manager)
                .unwrap_or_default()
        } else {
            vec![]
        };

        let new_symbols = if shadow_path.exists() {
            search
                .parse_file(&shadow_path, &mut manager)
                .unwrap_or_default()
        } else {
            vec![]
        };

        let mut orig_map: HashMap<String, Symbol> = orig_symbols
            .into_iter()
            .map(|s| {
                let relative_path = file_path
                    .strip_prefix(root)
                    .unwrap_or(&file_path)
                    .to_string_lossy();
                let key = format!("{}::{}", relative_path, s.name);
                (key, s)
            })
            .collect();
        let mut new_map: HashMap<String, Symbol> = new_symbols
            .into_iter()
            .map(|s| {
                let relative_path = shadow_path
                    .strip_prefix(&shadow_root)
                    .unwrap_or(&shadow_path)
                    .to_string_lossy();
                let key = format!("{}::{}", relative_path, s.name);
                (key, s)
            })
            .collect();

        if let Some(ref sym_id) = target_symbol {
            orig_map.retain(|k, _| k == sym_id);
            new_map.retain(|k, _| k == sym_id);
        }

        let mut all_keys: Vec<String> = orig_map.keys().chain(new_map.keys()).cloned().collect();
        all_keys.sort();
        all_keys.dedup();

        for key in all_keys {
            match (orig_map.get(&key), new_map.get(&key)) {
                (Some(o), Some(n)) => {
                    if o.semantic_hash != n.semantic_hash {
                        out.push_str(&format!(
                            "function `{}`: modified (AST differences detected)\n",
                            key
                        ));
                    }
                }
                (Some(_o), None) => {
                    out.push_str(&format!("function `{}`: removed\n", key));
                }
                (None, Some(_n)) => {
                    out.push_str(&format!("function `{}`: added\n", key));
                }
                _ => {}
            }
        }
    }

    if out.is_empty() {
        return Ok("No semantic changes detected.".to_string());
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_run_diff_non_semantic() {
        let dir = tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let file_path = root.join("test.txt");
        fs::write(&file_path, "old line\n").unwrap();

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().unwrap();
        shadow.stage(&file_path, "new line\n").unwrap();

        let diff_out = run_diff(&root, false, None).unwrap();
        assert!(diff_out.contains("-old line"));
        assert!(diff_out.contains("+new line"));
    }

    #[test]
    fn test_run_diff_semantic_no_changes() {
        let dir = tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().unwrap();

        let diff_out = run_diff(&root, true, None).unwrap();
        assert_eq!(diff_out, "No staged changes.");
    }

    #[test]
    fn test_run_diff_semantic_modifications() {
        let dir = tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let curd_dir = root.join(".curd");
        fs::create_dir_all(&curd_dir).unwrap();

        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("main.py");
        fs::write(&file_path, "def foo():\n    return 1\n").unwrap();

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().unwrap();
        shadow
            .stage(&file_path, "def foo():\n    return 2\n")
            .unwrap();

        let diff_out = run_diff(&root, true, None).unwrap();
        if !diff_out.contains("function `src/main.py::foo`: modified") {
            panic!("Unexpected diff_out: {}", diff_out);
        }
    }
}
