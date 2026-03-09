use crate::{ParserManager, SearchEngine, Watchdog};
use anyhow::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct EditEngine {
    pub workspace_root: PathBuf,
    pub watchdog: Option<Arc<Watchdog>>,
}

impl EditEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
            watchdog: None,
        }
    }

    pub fn with_watchdog(mut self, watchdog: Arc<Watchdog>) -> Self {
        self.watchdog = Some(watchdog);
        self
    }

    pub fn edit(
        &self,
        uri: &str,
        code: &str,
        action: &str,
        cwd_override: Option<&Path>,
    ) -> Result<Value> {
        if let Some(ref wd) = self.watchdog {
            wd.track_edit(uri);
        }

        let mut actual_uri = uri;
        let mut target_root = cwd_override.unwrap_or(&self.workspace_root).to_path_buf();

        // Safety: If no cwd_override provided, check if we are modifying the real workspace root.
        // If another session has a lock, we must refuse direct edits.
        if cwd_override.is_none() && crate::workspace::is_workspace_locked(&self.workspace_root) {
            anyhow::bail!("Cannot edit '{}': Workspace is locked by another active session. Open a CURD session or close the existing one.", uri);
        }

        if uri.starts_with('@') && let Some(idx) = uri.find("::") {
                let alias = &uri[..idx];
                actual_uri = &uri[idx + 2..];
                
                let registry = crate::context_link::ContextRegistry::load(&self.workspace_root);
                if let Some(link) = registry.contexts.get(alias) {
                    if link.mode != crate::context_link::ContextMode::Write {
                        return Err(anyhow::anyhow!("Context '{}' is linked in {:?} mode. Writes are prohibited.", alias, link.mode));
                    }
                    target_root = link.path.clone();
                } else {
                    return Err(anyhow::anyhow!("Context alias '{}' not found in registry.", alias));
                }
            }

        let parts: Vec<&str> = actual_uri.split("::").collect();
        let file_part = parts.first().copied().unwrap_or("");
        
        // Support nested symbols: Class::method
        let (symbol_name, line_part) = if parts.len() > 3 {
             // file::Class::method::line
             (format!("{}::{}", parts[1], parts[2]), parts.get(3).copied().and_then(|l| l.parse::<usize>().ok()))
        } else if parts.len() == 3 {
             // file::Class::method OR file::symbol::line
             if let Ok(line) = parts[2].parse::<usize>() {
                 (parts[1].to_string(), Some(line))
             } else {
                 (format!("{}::{}", parts[1], parts[2]), None)
             }
        } else {
             (parts.get(1).copied().unwrap_or("").to_string(), None)
        };

        let file_path = crate::workspace::validate_sandboxed_path(&target_root, file_part)?;

        if symbol_name.is_empty() {
            // Module top edit
            if action == "delete" {
                return Err(anyhow::anyhow!(
                    "delete is not supported for module tops; use edit on individual functions"
                ));
            }
            return self.edit_module_top(&file_path, code, uri);
        }

        match action {
            "upsert" => self.upsert_symbol(&file_path, &symbol_name, line_part, code, uri),
            "delete" => self.delete_symbol(&file_path, &symbol_name, line_part, uri),
            _ => Err(anyhow::anyhow!("Unknown action: {}", action)),
        }
    }

    fn edit_module_top(&self, file_path: &Path, new_code: &str, uri: &str) -> Result<Value> {
        if !file_path.exists() {
            // If file doesn't exist, just write the new code directly
            std::fs::write(file_path, new_code)?;
            return Ok(json!({"uri": uri, "action": "created"}));
        }

        let source_code = std::fs::read_to_string(file_path)?;
        let curd_dir = crate::workspace::get_curd_dir(&self.workspace_root);
        let mut manager = ParserManager::new(curd_dir.join("grammars"))?;
        let search = SearchEngine::new(&self.workspace_root);

        let symbols = search.parse_file(file_path, &mut manager)?;

        // Find the absolute earliest start_byte among all symbols
        let mut earliest_byte = source_code.len();
        for s in symbols {
            if s.start_byte < earliest_byte {
                earliest_byte = s.start_byte;
            }
        }

        let mut final_code = new_code.to_string();
        if !final_code.ends_with('\n') {
            final_code.push('\n');
        }

        if earliest_byte < source_code.len() {
            final_code.push_str(&source_code[earliest_byte..]);
        }

        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if let Some(lang) = manager.registry.lang_for_extension(extension) {
            self.validate_churn(&lang, &source_code, 0, earliest_byte)?;
        }

        std::fs::write(file_path, final_code)?;
        SearchEngine::new(&self.workspace_root).invalidate_index();
        Ok(json!({"uri": uri, "action": "replaced"}))
    }

    fn upsert_symbol(
        &self,
        file_path: &Path,
        symbol_name: &str,
        line_part: Option<usize>,
        new_code: &str,
        uri: &str,
    ) -> anyhow::Result<Value> {
        use anyhow::Context;
        println!(
            "DEBUG: upsert_symbol file_path={:?}, exists={}",
            file_path,
            file_path.exists()
        );
        if !file_path.exists() {
            std::fs::write(file_path, new_code).context("write new")?;
            return Ok(json!({"uri": uri, "action": "created"}));
        }

        let source_code = std::fs::read_to_string(file_path).context("read source")?;
        let curd_dir = crate::workspace::get_curd_dir(&self.workspace_root);
        let mut manager = ParserManager::new(curd_dir.join("grammars"))
            .context("ParserManager::new")?;
        let search = SearchEngine::new(&self.workspace_root);

        let symbols = search
            .parse_file(file_path, &mut manager)
            .context("parse_file")?;
            
        let mut matches: Vec<_> = symbols.into_iter().filter(|s| s.name == symbol_name).collect();
        
        // If no direct match, try matching by suffix (e.g., 'to_dict' matches 'Task::to_dict')
        if matches.is_empty() && !symbol_name.contains("::") {
            matches = search
                .parse_file(file_path, &mut manager)?
                .into_iter()
                .filter(|s| s.name.ends_with(&format!("::{}", symbol_name)))
                .collect();
        }

        let target = if matches.len() > 1 {
            if let Some(target_line) = line_part {
                matches.into_iter().find(|s| s.start_line == target_line)
            } else {
                let mut options = Vec::new();
                for m in matches {
                    let text = &source_code[m.start_byte..m.end_byte.min(source_code.len())];
                    let sig = text.lines().next().unwrap_or("").trim_end_matches(" {").trim_end_matches('{').trim().to_string();
                    options.push(format!("{}::{}: {}", m.id, m.start_line, sig));
                }
                return Err(anyhow::anyhow!("Symbol '{}' is overloaded. Please disambiguate by appending the line number to the URI (e.g., '{}::LINE_NUMBER').\nAvailable options:\n{}", symbol_name, uri, options.join("\n")));
            }
        } else {
            matches.into_iter().next()
        };

        let mut final_code = source_code.clone();
        let action_taken;

        if let Some(t) = target {
            // Replace existing
            let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if let Some(lang) = manager.registry.lang_for_extension(extension) {
                self.validate_churn(&lang, &source_code, t.start_byte, t.end_byte)?;
            }

            // Robust range expansion: Tree-sitter nodes often stop exactly at the last token.
            // When replacing, we should take the rest of the line and the newline to avoid
            // leaving ghost code or double newlines.
            let start = t.start_byte.min(source_code.len());
            let mut end = t.end_byte.min(source_code.len()).max(start);
            
            // Expand end to include trailing whitespace and exactly one newline if present
            let bytes = source_code.as_bytes();
            while end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t' || bytes[end] == b'\r') {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'\n' {
                end += 1;
            }

            let mut updated = String::new();
            updated.push_str(&source_code[..start]);
            updated.push_str(new_code);
            // Ensure the new code ends with a newline if the old range did and the new one doesn't
            if !new_code.ends_with('\n') {
                 updated.push('\n');
            }
            updated.push_str(&source_code[end..]);
            final_code = updated;
            action_taken = "replaced";
        } else {
            // Refuse to append to existing files unless it's a new file creation
            anyhow::bail!("SYMBOL_NOT_FOUND: Could not find symbol '{}' in '{}' to replace. Use a different URI or check for typos. CURD v0.6 prevents accidental appending to preserve file integrity.", symbol_name, file_path.display());
        }

        std::fs::write(file_path, final_code)?;
        SearchEngine::new(&self.workspace_root).invalidate_index();
        Ok(json!({"uri": uri, "action": action_taken}))
    }

    fn validate_churn(
        &self,
        language_name: &str,
        source_code: &str,
        target_start: usize,
        target_end: usize,
    ) -> Result<()> {
        if target_start >= target_end || target_start >= source_code.len() {
            return Ok(());
        }

        let mut manager = ParserManager::new(self.workspace_root.join(".curd/grammars"))?;
        let total_nodes = manager.count_nodes(language_name, source_code)?;
        if total_nodes == 0 {
            return Ok(());
        }

        let target_snippet = &source_code[target_start..target_end.min(source_code.len())];
        let target_nodes = manager.count_nodes(language_name, target_snippet)?;

        let config = crate::config::CurdConfig::load_from_workspace(&self.workspace_root);
        
        let limit = if total_nodes <= config.edit.small_file_nodes {
            config.edit.churn_small_limit
        } else if total_nodes >= config.edit.massive_file_nodes {
            config.edit.churn_massive_limit.min(config.edit.churn_limit)
        } else if total_nodes >= config.edit.large_file_nodes {
            config.edit.churn_large_limit.min(config.edit.churn_limit)
        } else {
            config.edit.churn_limit
        };

        let churn = target_nodes as f64 / total_nodes as f64;
        if churn > limit {
            let size_label = if total_nodes <= config.edit.small_file_nodes {
                "Small file"
            } else if total_nodes >= config.edit.massive_file_nodes {
                "Massive file"
            } else if total_nodes >= config.edit.large_file_nodes {
                "Large file"
            } else {
                "Standard file"
            };
            
            anyhow::bail!(
                "AST Churn Limit Exceeded: Modification replaces {:.1}% of the file's AST nodes, which exceeds the current limit of {:.1}% for a {} ({} nodes). Break this into smaller semantic edits.",
                churn * 100.0,
                limit * 100.0,
                size_label,
                total_nodes
            );
        }
        Ok(())
    }

    fn delete_symbol(&self, file_path: &Path, symbol_name: &str, line_part: Option<usize>, uri: &str) -> Result<Value> {
        if !file_path.exists() {
            return Err(anyhow::anyhow!("File not found"));
        }

        let source_code = std::fs::read_to_string(file_path)?;
        let mut manager = ParserManager::new(self.workspace_root.join(".curd/grammars"))?;
        let search = SearchEngine::new(&self.workspace_root);

        let symbols = search.parse_file(file_path, &mut manager)?;
        let matches: Vec<_> = symbols.into_iter().filter(|s| s.name == symbol_name).collect();
        let target = if matches.len() > 1 {
            if let Some(target_line) = line_part {
                matches.into_iter().find(|s| s.start_line == target_line)
            } else {
                let mut options = Vec::new();
                for m in matches {
                    let text = &source_code[m.start_byte..m.end_byte.min(source_code.len())];
                    let sig = text.lines().next().unwrap_or("").trim_end_matches(" {").trim_end_matches('{').trim().to_string();
                    options.push(format!("{}::{}: {}", m.id, m.start_line, sig));
                }
                return Err(anyhow::anyhow!("Symbol '{}' is overloaded. Please disambiguate by appending the line number to the URI (e.g., '{}::LINE_NUMBER').\nAvailable options:\n{}", symbol_name, uri, options.join("\n")));
            }
        } else {
            matches.into_iter().next()
        };

        if let Some(t) = target {
            let mut updated = String::new();
            let start = t.start_byte.min(source_code.len());
            let end = t.end_byte.min(source_code.len()).max(start);
            updated.push_str(&source_code[..start]);
            // Skip the symbol. Might leave an extra blank line depending on formatting, which is fine for now
            updated.push_str(&source_code[end..]);

            std::fs::write(file_path, updated)?;
            SearchEngine::new(&self.workspace_root).invalidate_index();
            Ok(json!({"uri": uri, "action": "deleted"}))
        } else {
            Err(anyhow::anyhow!(
                "Symbol '{}' not found for deletion",
                symbol_name
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_churn_limit_triggers() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // 1. Create a file with multiple functions
        let code = r#"
fn a() { println!("a"); }
fn b() {
    println!("b1");
    println!("b2");
    println!("b3");
    println!("b4");
    println!("b5");
}
"#;
        std::fs::write(root.join("test.rs"), code).unwrap();

        // 2. Create CURD.toml with a strict limit
        std::fs::write(root.join("CURD.toml"), "[edit]\nchurn_limit = 0.1\nsmall_file_nodes = 0\n").unwrap();

        let engine = EditEngine::new(root);

        // 3. Try to replace func 'b' (likely > 10% of nodes)
        let result = engine.edit("test.rs::b", "fn b() { }", "upsert", None);

        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(
            err.contains("AST Churn Limit Exceeded"),
            "Error was: {}",
            err
        );
    }

    #[test]
    fn test_churn_limit_allows_small_change() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        println!("DEBUG: root path is {}", root.display());

        let code = "fn a() {}\n".repeat(20);
        std::fs::write(root.join("test.rs"), code).unwrap();
        std::fs::write(root.join("CURD.toml"), "[edit]\nchurn_limit = 0.5\n").unwrap();

        let engine = EditEngine::new(root);
        // Replace one small function out of 20 (5% churn) - should be allowed by 50% limit
        let result = engine.edit("test.rs::a::1", "fn a() { println!(); }", "upsert", None);
        if let Err(ref e) = result {
            println!(
                "test_churn_limit_allows_small_change FAILED. Error: {:?}",
                e
            );
            let mut current = e.source();
            while let Some(source) = current {
                println!("  Caused by: {:?}", source);
                current = source.source();
            }
        }
        assert!(result.is_ok(), "Small change should be allowed.");
    }

    #[test]
    fn test_delete_symbol() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let code = r#"
fn a() {}
fn b() {}
"#;
        std::fs::write(root.join("test.rs"), code).unwrap();

        let engine = EditEngine::new(root);
        let result = engine.edit("test.rs::b", "", "delete", None);
        assert!(result.is_ok());

        let new_code = std::fs::read_to_string(root.join("test.rs")).unwrap();
        assert!(new_code.contains("fn a() {}"));
        assert!(!new_code.contains("fn b() {}"));
    }

    #[test]
    fn test_edit_module_top() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let code = "fn b() {}\n";
        std::fs::write(root.join("test.rs"), code).unwrap();

        let engine = EditEngine::new(root);
        let result = engine.edit("test.rs", "fn a() {}", "upsert", None);
        assert!(result.is_ok());

        let new_code = std::fs::read_to_string(root.join("test.rs")).unwrap();
        assert!(new_code.contains("fn a() {}"));
        assert!(new_code.contains("fn b() {}"));
        // `a` should be added before `b`
        assert!(new_code.find("fn a()").unwrap() < new_code.find("fn b()").unwrap());
    }
}
