use crate::{ParserManager, SearchEngine};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use rand::{Rng};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationTrace {
    pub schema_version: String,
    pub timestamp_secs: u64,
    pub uri: String,
    pub original_code: String,
    pub mutated_code: String,
    pub mutation_type: String,
    pub agent_repair: Option<String>,
    pub build_status: Option<String>,
}

pub struct MutationEngine {
    pub workspace_root: PathBuf,
}

impl MutationEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
        }
    }

    pub fn record_trace(&self, trace: &MutationTrace) -> Result<()> {
        let curd_dir = crate::workspace::get_curd_dir(&self.workspace_root);
        let dir = curd_dir.join("traces");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("repl_history.jsonl");
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        let json_line = serde_json::to_string(trace)?;
        use std::io::Write;
        writeln!(file, "{}", json_line)?;
        Ok(())
    }

    pub fn mutate_symbol(&self, uri: &str, shadow_root: Option<&Path>) -> Result<Value> {
        let mut parts = uri.splitn(2, "::");
        let file_part = parts.next().unwrap_or("");
        let symbol_name = parts.next().ok_or_else(|| anyhow::anyhow!("Invalid URI: missing symbol part"))?;

        let file_path = crate::workspace::validate_sandboxed_path(&self.workspace_root, file_part)?;
        if !file_path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", file_part));
        }
        
        // If we have a shadow_root from an active session, use it.
        // Otherwise, fail if enforcement is on (checked at dispatch level).
        let shadow_file = if let Some(root) = shadow_root {
            root.join(file_part)
        } else {
            // Fallback to real path if no shadow root (though dispatch_tool might have blocked this)
            file_path.clone()
        };

        if let Some(parent) = shadow_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let source_code = std::fs::read_to_string(&shadow_file).or_else(|_| std::fs::read_to_string(&file_path))?;
        
        let curd_dir = crate::workspace::get_curd_dir(&self.workspace_root);
        let mut manager = ParserManager::new(curd_dir.join("grammars"))?;
        let search = SearchEngine::new(&self.workspace_root);
        
        // Parse the shadow file or real file depending on what we loaded
        let parse_target = if shadow_file.exists() { &shadow_file } else { &file_path };
        let symbols = search.parse_file(parse_target, &mut manager)?;
        
        let symbol = symbols.into_iter().find(|s| s.name == symbol_name)
            .ok_or_else(|| anyhow::anyhow!("Symbol '{}' not found in file '{}'", symbol_name, file_part))?;

        let symbol_code = &source_code[symbol.start_byte..symbol.end_byte];
        let (mutated_code, mutation_type) = self.apply_random_mutation(symbol_code)?;

        let mut final_code = source_code[..symbol.start_byte].to_string();
        final_code.push_str(&mutated_code);
        final_code.push_str(&source_code[symbol.end_byte..]);

        std::fs::write(&shadow_file, &final_code)?;
        
        // If we are in a shadow root, we should also notify the shadow store manifest
        // if we have access to it. For now, just writing to the shadow file is 
        // enough as `discover_implicit_changes` will find it during diff/commit.
        
        // Invalidate index since we changed the file
        SearchEngine::new(&self.workspace_root).invalidate_index();

        // Record trace
        let trace = MutationTrace {
            schema_version: "1.0".to_string(),
            timestamp_secs: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            uri: uri.to_string(),
            original_code: if crate::redact_value(json!({"code": symbol_code})).get("code").is_some_and(|v| v == "[REDACTED]") { "[REDACTED]".into() } else { symbol_code.to_string() },
            mutated_code: if crate::redact_value(json!({"code": mutated_code})).get("code").is_some_and(|v| v == "[REDACTED]") { "[REDACTED]".into() } else { mutated_code.clone() },
            mutation_type: mutation_type.clone(),
            agent_repair: None,
            build_status: None,
        };
        let _ = self.record_trace(&trace);

        Ok(json!({
            "uri": uri,
            "status": "mutated",
            "mutation_type": mutation_type
        }))
    }

    fn apply_random_mutation(&self, code: &str) -> Result<(String, String)> {
        let mut lines: Vec<String> = code.lines().map(|s| s.to_string()).collect();
        if lines.is_empty() {
             return Ok((format!("// mutated\n{}", code), "comment_injection".to_string()));
        }

        let mut rng = rand::thread_rng();
        let mutation_choice = rng.gen_range(0..3);

        let mutation_type;

        match mutation_choice {
            0 => {
                // Delete a random line if more than 1 line
                if lines.len() > 1 {
                    let idx = rng.gen_range(0..lines.len());
                    lines.remove(idx);
                    mutation_type = "line_deletion".to_string();
                } else {
                    lines.push("// CURD_POISON".to_string());
                    mutation_type = "comment_injection".to_string();
                }
            }
            1 => {
                // Swap two random lines if more than 1
                if lines.len() >= 2 {
                    let idx1 = rng.gen_range(0..lines.len());
                    let idx2 = rng.gen_range(0..lines.len());
                    lines.swap(idx1, idx2);
                    mutation_type = "line_swap".to_string();
                } else {
                    lines.insert(0, "// CURD_POISON".to_string());
                    mutation_type = "comment_injection".to_string();
                }
            }
            2 => {
                 // Duplicate a random line
                 let idx = rng.gen_range(0..lines.len());
                 let line = lines[idx].clone();
                 lines.insert(idx, line);
                 mutation_type = "line_duplication".to_string();
            }
            _ => {
                mutation_type = "none".to_string();
            }
        }

        Ok((lines.join("\n"), mutation_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_apply_random_mutation() {
        let engine = MutationEngine::new(".");
        
        let code = "def foo():\n    a = 1\n    return a\n";
        let (mutated, mtype) = engine.apply_random_mutation(code).unwrap();
        
        assert_ne!(mutated, code);
        assert!(!mtype.is_empty());
        
        let code_single = "def bar(): pass";
        let (mutated_single, mtype_single) = engine.apply_random_mutation(code_single).unwrap();
        assert!(mutated_single.contains("CURD_POISON") || mtype_single == "line_duplication");
    }

    #[test]
    fn test_mutate_symbol_missing_file() {
        let dir = tempdir().unwrap();
        let engine = MutationEngine::new(dir.path());
        let res = engine.mutate_symbol("src/missing.py::foo", None);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("not found"));
    }
}
