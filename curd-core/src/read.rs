use crate::{GraphEngine, ParserManager, SearchEngine, SymbolKind};
use anyhow::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Handles fetching detailed information and source code for URIs
pub struct ReadEngine {
    pub workspace_root: PathBuf,
}

impl ReadEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
        }
    }

    /// Read an array of URIs with a specific verbosity level
    pub fn read(&self, uris: Vec<String>, verbosity: u8) -> Result<Vec<Value>> {
        let search = SearchEngine::new(&self.workspace_root);
        let mut results = Vec::new();

        for uri in uris {
            let mut actual_uri = uri.as_str();
            let mut target_root = self.workspace_root.clone();

            if uri.starts_with('@')
                && let Some(idx) = uri.find("::") {
                    let alias = &uri[..idx];
                    actual_uri = &uri[idx + 2..];

                    let registry = crate::context_link::ContextRegistry::load(&self.workspace_root);
                    if let Some(link) = registry.contexts.get(alias) {
                        if link.mode == crate::context_link::ContextMode::Index {
                            results.push(json!({"uri": uri, "error": format!("Context '{}' is linked in Index mode. File reads are prohibited.", alias)}));
                            continue;
                        }
                        target_root = link.path.clone();
                    } else {
                        results.push(json!({"uri": uri, "error": format!("Context alias '{}' not found in registry.", alias)}));
                        continue;
                    }
                }

            let parts: Vec<&str> = actual_uri.split("::").collect();
            let file_part = parts.first().copied().unwrap_or("");
            let symbol_part = parts.get(1).copied();
            let line_part = parts.get(2).copied().and_then(|l| l.parse::<usize>().ok());

            if let Some(symbol_name) = symbol_part {
                // Function or class read
                match self.read_symbol(&search, &uri, file_part, symbol_name, line_part, verbosity, &target_root) {
                    Ok(val) => results.push(val),
                    Err(e) => results.push(json!({"uri": uri, "error": e.to_string()})),
                }
            } else {
                // File-level read
                match self.read_file(&search, &uri, file_part, verbosity, &target_root) {
                    Ok(val) => results.push(val),
                    Err(e) => results.push(json!({"uri": uri, "error": e.to_string()})),
                }
            }
        }

        Ok(results)
    }

    #[allow(clippy::too_many_arguments)]
    fn read_symbol(
        &self,
        search: &SearchEngine,
        uri: &str,
        file_part: &str,
        symbol_name: &str,
        line_part: Option<usize>,
        verbosity: u8,
        target_root: &Path,
    ) -> Result<Value> {
        let symbols = search.search(symbol_name, None)?;
        let mut matches: Vec<_> = symbols
            .into_iter()
            .filter(|s| s.filepath.to_string_lossy().ends_with(file_part) || file_part.ends_with(&s.filepath.to_string_lossy().to_string()))
            .filter(|s| s.name == symbol_name)
            .collect();

        if matches.is_empty() {
            return Err(anyhow::anyhow!("Symbol '{}' not found in workspace", uri));
        }

        let target = if matches.len() > 1 {
            if let Some(target_line) = line_part {
                // Try to find the exact one
                if let Some(t) = matches.into_iter().find(|s| s.start_line == target_line) {
                    t
                } else {
                    return Err(anyhow::anyhow!("Overloaded symbol '{}' not found at line {}.", symbol_name, target_line));
                }
            } else {
                // Ambiguous! Return the list of signatures and ask the agent to disambiguate.
                let mut options = Vec::new();
                for m in matches {
                    let path = if m.filepath.is_absolute() { m.filepath.clone() } else { target_root.join(&m.filepath) };
                    let sig = if let Ok(src) = std::fs::read_to_string(&path) {
                        let text = &src[m.start_byte..m.end_byte.min(src.len())];
                        text.lines().next().unwrap_or("").trim_end_matches(" {").trim_end_matches('{').trim().to_string()
                    } else {
                        m.name.clone()
                    };
                    options.push(format!("{}::{}: {}", m.id, m.start_line, sig));
                }
                return Err(anyhow::anyhow!("Symbol '{}' is overloaded. Please disambiguate by appending the line number to the URI (e.g., '{}::LINE_NUMBER').\nAvailable options:\n{}", uri, uri, options.join("\n")));
            }
        } else {
            matches.pop().unwrap()
        };

        let file_path = if target.filepath.is_absolute() {
            target.filepath.clone()
        } else {
            target_root.join(&target.filepath)
        };

        let type_str = if target.kind == SymbolKind::Function {
            "function"
        } else {
            "class"
        };

        if verbosity == 0 {
            // Outline only
            return Ok(json!({
                "uri": uri,
                "type": type_str,
                "name": target.name,
                "start_line": target.start_line,
                "end_line": target.end_line,
            }));
        }

        // Full source
        let source_code = std::fs::read_to_string(&file_path)?;

        let start = target.start_byte.min(source_code.len());
        let end = target.end_byte.min(source_code.len()).max(start);
        let snippet = &source_code[start..end];

        let mut result_obj = json!({
            "uri": uri,
            "type": type_str,
            "name": target.name,
            "start_line": target.start_line,
            "end_line": target.end_line,
            "source": snippet,
        });

        if verbosity >= 2 {
            let graph = GraphEngine::new(&self.workspace_root);
            let graph_res = graph.graph_with_depths(vec![target.id.clone()], 1, 1)?;
            if let Some(first) = graph_res
                .get("results")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
            {
                result_obj["callers"] = first.get("callers").cloned().unwrap_or_else(|| json!([]));
                result_obj["callees"] = first.get("callees").cloned().unwrap_or_else(|| json!([]));
            } else {
                result_obj["callers"] = json!([]);
                result_obj["callees"] = json!([]);
            }
        }

        Ok(result_obj)
    }

    fn read_file(
        &self,
        search: &SearchEngine,
        uri: &str,
        file_part: &str,
        verbosity: u8,
        target_root: &Path,
    ) -> Result<Value> {
        let file_path = crate::workspace::validate_sandboxed_path(target_root, file_part)?;
        if !file_path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", file_part));
        }

        let mut manager = ParserManager::new(self.workspace_root.join(".curd/grammars"))?;
        let symbols = search
            .parse_file(&file_path, &mut manager)
            .unwrap_or_default();

        let mut functions = Vec::new();
        let mut classes = Vec::new();

        for s in symbols {
            let outline = json!({
                "name": s.name,
                "start_line": s.start_line,
                "end_line": s.end_line,
                "id": s.id,
            });
            if s.kind == SymbolKind::Function {
                functions.push(outline);
            } else {
                classes.push(outline);
            }
        }

        if verbosity == 0 {
            return Ok(json!({
                "uri": uri,
                "type": "file",
                "functions": functions,
                "classes": classes,
            }));
        }

        let source_code = std::fs::read_to_string(&file_path)?;
        Ok(json!({
            "uri": uri,
            "type": "file",
            "source": source_code,
            "functions": functions,
            "classes": classes,
        }))
    }
}
