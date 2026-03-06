use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::UNIX_EPOCH;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::{ParserManager, Sandbox, SemanticFault, fault::FaultSeverity, scan_workspace};

/// A lightweight diagnostic engine that reports syntax errors by walking the Tree-sitter AST
pub struct LspEngine {
    pub workspace_root: PathBuf,
    sandbox: Sandbox,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct DiagCacheEntry {
    mtime_ms: u64,
    mode: String,
    diagnostics: Value,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DiagCache {
    files: HashMap<String, DiagCacheEntry>,
}

impl LspEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = workspace_root.as_ref().to_path_buf();
        Self {
            workspace_root: std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone()),
            sandbox: Sandbox::new(root),
        }
    }

    /// Recursively traverse the tree-sitter AST and collect all `(ERROR)` or missing nodes.
    fn collect_errors<'a>(node: Node<'a>, errors: &mut Vec<Value>) {
        if node.is_error() || node.is_missing() {
            let start_point = node.start_position();
            let end_point = node.end_position();

            let message = if node.is_missing() {
                format!("Missing expected syntax: `{}`", node.kind())
            } else {
                "Syntax Error: Unexpected token".to_string()
            };

            errors.push(json!({
                "message": message,
                "severity": "error",
                "code": Value::Null,
                "source": "syntax",
                "tool": "tree-sitter",
                "line": start_point.row + 1,
                "column": start_point.column,
                "end_line": end_point.row + 1,
                "end_column": end_point.column,
            }));
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::collect_errors(child, errors);
        }
    }

    /// Internal fallback for semantic diagnostics when external tools are missing.
    /// Uses tree-sitter queries to identify obvious semantic issues.
    fn internal_semantic_fallback(&self, language: &str, source_code: &str) -> Result<Vec<Value>> {
        let mut manager = ParserManager::new(self.workspace_root.join(".curd/grammars"))?;
        let mut parser = manager.create_parser(language)?;
        let tree = parser
            .parse(source_code, None)
            .ok_or_else(|| anyhow::anyhow!("Parse failed"))?;
        let root = tree.root_node();
        let lang = parser.language().unwrap();

        let mut diags = Vec::new();

        // 1. Duplicate Definitions Check
        let def_query_str = match language {
            "rust" | "python" | "javascript" | "typescript" | "go" => {
                "(function_definition name: (identifier) @name) \
                 (class_definition name: (identifier) @name)"
            }
            _ => "",
        };

        if !def_query_str.is_empty() {
            let query = Query::new(&lang, def_query_str)?;
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source_code.as_bytes());

            // Track definitions per parent scope node ID to avoid global false positives
            let mut seen_in_scope: HashMap<(usize, String), Vec<tree_sitter::Range>> =
                HashMap::new();

            while let Some(m) = matches.next() {
                for cap in m.captures {
                    let name = cap.node.utf8_text(source_code.as_bytes())?.to_string();
                    let parent_id = cap.node.parent().map(|p| p.id()).unwrap_or(0);
                    seen_in_scope
                        .entry((parent_id, name))
                        .or_default()
                        .push(cap.node.range());
                }
            }

            for ((_, name), ranges) in seen_in_scope {
                if ranges.len() > 1 {
                    for r in &ranges[1..] {
                        diags.push(json!({
                            "message": format!("Duplicate definition of '{}' in the same scope", name),
                            "severity": "error",
                            "line": r.start_point.row + 1,
                            "column": r.start_point.column,
                            "source": "semantic-fallback",
                            "tool": "curd-internal"
                        }));
                    }
                }
            }
        }

        // 2. Dead Code (Local) Check: Functions defined but never called in the same file
        let usage_query_str = match language {
            "python" | "javascript" | "typescript" => "(call function: (identifier) @usage)",
            "rust" => "(call_expression function: (identifier) @usage)",
            _ => "",
        };

        if !usage_query_str.is_empty() && !def_query_str.is_empty() {
            let u_query = Query::new(&lang, usage_query_str)?;
            let mut u_cursor = QueryCursor::new();
            let mut u_matches = u_cursor.matches(&u_query, root, source_code.as_bytes());

            let mut used_names = std::collections::HashSet::new();
            while let Some(m) = u_matches.next() {
                for cap in m.captures {
                    used_names.insert(cap.node.utf8_text(source_code.as_bytes())?.to_string());
                }
            }

            // Re-run def query to find unused
            let d_query = Query::new(&lang, def_query_str)?;
            let mut d_cursor = QueryCursor::new();
            let mut d_matches = d_cursor.matches(&d_query, root, source_code.as_bytes());
            while let Some(m) = d_matches.next() {
                for cap in m.captures {
                    let name = cap.node.utf8_text(source_code.as_bytes())?.to_string();
                    if !used_names.contains(&name) && name != "main" {
                        let r = cap.node.range();
                        diags.push(json!({
                            "message": format!("Function '{}' is defined but never used in this file", name),
                            "severity": "warning",
                            "line": r.start_point.row + 1,
                            "column": r.start_point.column,
                            "source": "semantic-fallback",
                            "tool": "curd-internal"
                        }));
                    }
                }
            }
        }

        Ok(diags)
    }

    /// Retrieve diagnostics for a specific file URI.
    pub async fn diagnostics_with_mode(&self, uri: &str, mode: &str) -> Result<Value> {
        let full_path = crate::workspace::validate_sandboxed_path(&self.workspace_root, uri)?;
        if !full_path.exists() {
            return Err(anyhow::anyhow!("File '{}' does not exist.", uri));
        }

        let extension = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let language_name = match extension {
            "rs" => "rust",
            "py" => "python",
            "js" => "javascript",
            "ts" => "typescript",
            "go" => "go",
            "c" | "h" => "c",
            "cpp" | "hpp" | "cc" | "cxx" => "cpp",
            "rb" => "ruby",
            "java" => "java",
            "cs" => "c_sharp",
            "php" => "php",
            "html" => "html",
            "css" => "css",
            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported file extension '{}' for LSP diagnostics",
                    extension
                ));
            }
        };

        let source_code = std::fs::read_to_string(&full_path)?;
        let mut manager = ParserManager::new(self.workspace_root.join(".curd/grammars"))?;
        let mut parser = manager.create_parser(language_name)?;

        let tree = parser
            .parse(&source_code, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file '{}'", uri))?;

        let root_node = tree.root_node();
        let mut syntax_errors = Vec::new();
        let mut semantic_errors = Vec::new();
        let mut semantic_note = None;
        let mut fallback_active = false;

        if mode == "syntax" || mode == "both" {
            Self::collect_errors(root_node, &mut syntax_errors);
        }
        if mode == "semantic" || mode == "both" {
            match self.semantic_diagnostics(&full_path, extension).await {
                Ok(diags) => semantic_errors = diags,
                Err(e) => {
                    semantic_note = Some(format!(
                        "External tool failed: {}. Falling back to internal analysis.",
                        e
                    ));
                    fallback_active = true;
                    if let Ok(internal) =
                        self.internal_semantic_fallback(language_name, &source_code)
                    {
                        semantic_errors = internal;
                    }
                }
            }
        }

        let mut all = Vec::new();
        all.extend(syntax_errors.clone());
        all.extend(semantic_errors.clone());
        all = dedup_diagnostics(all);

        Ok(json!({
            "uri": uri,
            "mode": mode,
            "status": if all.is_empty() { "ok" } else { "error" },
            "error_count": all.len(),
            "diagnostics": all,
            "syntax_diagnostics": syntax_errors,
            "semantic_diagnostics": semantic_errors,
            "semantic_note": semantic_note,
            "fallback_active": fallback_active
        }))
    }

    /// Backward-compatible default: syntax diagnostics only.
    pub fn diagnostics(&self, uri: &str) -> Result<Value> {
        // internal synchronous helper for backward compat (syntax only)
        let full_path = crate::workspace::validate_sandboxed_path(&self.workspace_root, uri)?;
        let source_code = std::fs::read_to_string(&full_path)?;
        let mut manager = ParserManager::new(self.workspace_root.join(".curd/grammars"))?;
        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lang_opt = manager.registry.lang_for_extension(ext);
        let lang = lang_opt.as_deref().unwrap_or("rust");
        let mut parser = manager.create_parser(lang)?;
        let tree = parser
            .parse(&source_code, None)
            .ok_or_else(|| anyhow::anyhow!("Parse failed"))?;
        let mut syntax_errors = Vec::new();
        Self::collect_errors(tree.root_node(), &mut syntax_errors);
        Ok(json!({
            "uri": uri,
            "mode": "syntax",
            "status": "ok",
            "error_count": syntax_errors.len(),
            "diagnostics": syntax_errors
        }))
    }

    /// Workspace-wide diagnostics for all scanned source files.
    pub async fn diagnostics_workspace(&self, mode: &str) -> Result<Value> {
        self.diagnostics_workspace_paginated(mode, None, 0).await
    }

    pub async fn diagnostics_workspace_paginated(
        &self,
        mode: &str,
        limit: Option<usize>,
        offset: usize,
    ) -> Result<Value> {
        let files = scan_workspace(&self.workspace_root)?;
        let mut results = Vec::new();
        let mut total_error_count = 0usize;
        let mut scanned = 0usize;
        let use_cache = mode == "semantic" || mode == "both";
        let mut cache = self.load_diag_cache();

        // Optimization: For Rust semantic checks, run ONE cargo check for the entire workspace
        let mut rust_workspace_diags: HashMap<String, Vec<Value>> = HashMap::new();
        if (mode == "semantic" || mode == "both")
            && self.is_rust_workspace()
            && let Ok(all_diags) = self.run_cargo_check_workspace().await
        {
            rust_workspace_diags = all_diags;
        }

        let start = offset.min(files.len());
        let end = if let Some(l) = limit {
            (start + l).min(files.len())
        } else {
            files.len()
        };

        for file in files.into_iter().skip(start).take(end - start) {
            scanned += 1;
            let rel = file
                .strip_prefix(&self.workspace_root)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            let mtime = file_mtime_ms(&self.workspace_root.join(&rel)).unwrap_or(0);

            let cached = if use_cache {
                cache
                    .files
                    .get(&rel)
                    .cloned()
                    .filter(|e| e.mode == mode && e.mtime_ms == mtime)
                    .map(|e| e.diagnostics)
            } else {
                None
            };

            let diag_result: Result<Value> = if let Some(v) = cached {
                Ok(v)
            } else {
                // Not in cache: compute it.
                if mode == "syntax" {
                    self.diagnostics(&rel)
                } else {
                    let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if extension == "rs" && !rust_workspace_diags.is_empty() {
                        let rust_diags =
                            rust_workspace_diags.get(&rel).cloned().unwrap_or_default();
                        let mut res = json!({
                            "uri": rel,
                            "error_count": rust_diags.len(),
                            "diagnostics": rust_diags
                        });
                        if mode == "both"
                            && let Ok(syntax) = self.diagnostics(&rel)
                        {
                            res = self.merge_diagnostics(syntax, res);
                        }
                        Ok(res)
                    } else {
                        self.diagnostics_with_mode(&rel, mode).await
                    }
                }
            };

            match diag_result {
                Ok(v) => {
                    let count = v
                        .get("error_count")
                        .and_then(|n: &Value| n.as_u64())
                        .unwrap_or(0) as usize;
                    total_error_count += count;
                    if use_cache {
                        cache.files.insert(
                            rel.clone(),
                            DiagCacheEntry {
                                mtime_ms: mtime,
                                mode: mode.to_string(),
                                diagnostics: v.clone(),
                            },
                        );
                    }
                    results.push(v);
                }
                Err(e) => results.push(json!({
                    "uri": rel,
                    "mode": mode,
                    "status": "error",
                    "error_count": 1,
                    "diagnostics": [{
                        "message": format!("Failed diagnostics: {}", e),
                        "severity": "error",
                        "code": Value::Null,
                        "source": "semantic",
                        "tool": "curd"
                    }]
                })),
            }
        }
        if use_cache {
            self.save_diag_cache(&cache);
        }

        Ok(json!({
            "scope": "workspace",
            "mode": mode,
            "file_count": results.len(),
            "scanned_count": scanned,
            "offset": start,
            "limit": limit,
            "error_count": total_error_count,
            "results": results
        }))
    }

    pub async fn get_semantic_faults(&self, uri: &str) -> Result<Vec<SemanticFault>> {
        let diag_result = self.diagnostics_with_mode(uri, "both").await?;
        let items = diag_result
            .get("diagnostics")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Expected array of diagnostics"))?;

        let mut faults = Vec::new();
        let full_path = crate::workspace::validate_sandboxed_path(&self.workspace_root, uri)?;
        let source_code = std::fs::read_to_string(&full_path)?;

        for d in items {
            let message = d
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            let severity_str = d
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("error");
            let line = d.get("line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let column = d.get("column").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

            let ts_line = if line > 0 { line - 1 } else { 0 };

            let severity = match severity_str {
                "error" => FaultSeverity::Error,
                "warning" => FaultSeverity::Warning,
                "information" => FaultSeverity::Information,
                _ => FaultSeverity::Hint,
            };

            let mut fault =
                SemanticFault::new_lsp(message, uri.to_string(), ts_line, column, severity);

            let mut manager = ParserManager::new(self.workspace_root.join(".curd/grammars"))?;
            let extension = Path::new(uri)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if let Some(lang) = manager.registry.lang_for_extension(extension)
                && let Ok(range) = manager.bind_fault_to_ast(&lang, &source_code, ts_line, column) {
                    fault.end_line = Some(range.end_point.row);
                    fault.end_column = Some(range.end_point.column);
                }

            faults.push(fault);
        }
        Ok(faults)
    }

    fn is_rust_workspace(&self) -> bool {
        self.workspace_root.join("Cargo.toml").exists()
    }

    async fn run_cargo_check_workspace(&self) -> Result<HashMap<String, Vec<Value>>> {
        let mut command = self.sandbox.build_command(
            "cargo",
            &["check".to_string(), "--message-format=json".to_string()],
        );
        let output = command
            .current_dir(&self.workspace_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut file_map: HashMap<String, Vec<Value>> = HashMap::new();

        for line in stdout.lines() {
            if let Ok(msg) = serde_json::from_str::<Value>(line)
                && msg.get("reason").and_then(|v| v.as_str()) == Some("compiler-message")
                && let Some(msg_obj) = msg.get("message")
                && let Some(spans) = msg_obj.get("spans").and_then(|v| v.as_array())
            {
                for span in spans {
                    if let Some(file_name) = span.get("file_name").and_then(|v| v.as_str()) {
                        // Skip external crate errors
                        if !file_name.starts_with('/') && !file_name.contains(".cargo") {
                            let diag = json!({
                                "line": span.get("line_start").unwrap_or(&json!(0)),
                                "column": span.get("column_start").unwrap_or(&json!(0)),
                                "message": msg_obj.get("message").unwrap_or(&json!("")),
                                "severity": msg_obj.get("level").unwrap_or(&json!("error")),
                                "source": "semantic"
                            });
                            file_map
                                .entry(file_name.to_string())
                                .or_default()
                                .push(diag);
                        }
                    }
                }
            }
        }
        Ok(file_map)
    }

    fn merge_diagnostics(&self, mut syntax: Value, semantic: Value) -> Value {
        let mut all = syntax
            .get("diagnostics")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if let Some(sem_arr) = semantic.get("diagnostics").and_then(|v| v.as_array()) {
            all.extend(sem_arr.iter().cloned());
        }
        let count = all.len();
        if let Some(obj) = syntax.as_object_mut() {
            obj.insert("diagnostics".to_string(), json!(all));
            obj.insert("error_count".to_string(), json!(count));
        }
        syntax
    }

    async fn semantic_diagnostics(&self, file_path: &Path, extension: &str) -> Result<Vec<Value>> {
        let rel = file_path
            .strip_prefix(&self.workspace_root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();
        let rel_safe = if rel.starts_with('/') {
            rel.clone()
        } else {
            format!("./{}", rel)
        };

        let (cmd, args, parser_kind): (&str, Vec<String>, &str) = match extension {
            "rs" => (
                "cargo",
                vec![
                    "check".to_string(),
                    "--message-format=json-diagnostic-rendered-ansi".to_string(),
                ],
                "cargo_json",
            ),
            "py" => {
                if command_exists("pyright", &self.workspace_root) {
                    let mut args = vec!["--outputjson".to_string()];
                    // If no project config is present, scope to the target file.
                    if !(self.workspace_root.join("pyrightconfig.json").exists()
                        || self.workspace_root.join("pyproject.toml").exists())
                    {
                        args.push(rel_safe.clone());
                    }
                    ("pyright", args, "pyright_json")
                } else if command_exists("python", &self.workspace_root) {
                    (
                        "python",
                        vec!["-m".to_string(), "py_compile".to_string(), rel_safe.clone()],
                        "short",
                    )
                } else {
                    return Err(anyhow::anyhow!("No Python checker found (pyright/python)."));
                }
            }
            "ts" | "js" => {
                let tsconfig = self.workspace_root.join("tsconfig.json");
                if tsconfig.exists() {
                    (
                        "tsc",
                        vec![
                            "-p".to_string(),
                            "tsconfig.json".to_string(),
                            "--pretty".to_string(),
                            "false".to_string(),
                            "--noEmit".to_string(),
                        ],
                        "short",
                    )
                } else {
                    (
                        "tsc",
                        vec![
                            "--noEmit".to_string(),
                            "--pretty".to_string(),
                            "false".to_string(),
                            rel_safe.clone(),
                        ],
                        "short",
                    )
                }
            }
            "go" => ("go", vec!["build".to_string(), rel_safe.clone()], "short"),
            "java" => ("javac", vec!["-Xlint".to_string(), rel_safe.clone()], "short"),
            "c" | "h" => (
                "clang",
                vec!["-fsyntax-only".to_string(), rel_safe.clone()],
                "short",
            ),
            "cpp" | "hpp" | "cc" | "cxx" => (
                "clang++",
                vec!["-fsyntax-only".to_string(), rel_safe.clone()],
                "short",
            ),
            _ => return Ok(Vec::new()),
        };

        if !command_exists(cmd, &self.workspace_root) {
            return Err(anyhow::anyhow!("Checker '{}' is not installed.", cmd));
        }

        let mut command = self.sandbox.build_command(cmd, &args);
        let output = command
            .current_dir(&self.workspace_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = if stdout.is_empty() {
            stderr.clone()
        } else if stderr.is_empty() {
            stdout.clone()
        } else {
            format!("{}\n{}", stdout, stderr)
        };

        if parser_kind == "pyright_json" {
            return Ok(parse_pyright_json(&combined, file_path));
        }
        if parser_kind == "cargo_json" {
            return Ok(parse_cargo_json(&combined, file_path));
        }
        Ok(parse_short_diagnostics(&combined, file_path))
    }

    fn diag_cache_path(&self) -> PathBuf {
        self.workspace_root.join(".curd").join("diag_cache.json")
    }

    fn load_diag_cache(&self) -> DiagCache {
        let p = self.diag_cache_path();
        if !p.exists() {
            return DiagCache::default();
        }
        fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str::<DiagCache>(&s).ok())
            .unwrap_or_default()
    }

    fn save_diag_cache(&self, cache: &DiagCache) {
        let p = self.diag_cache_path();
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(cache) {
            let _ = fs::write(p, text);
        }
    }
}

use crate::shell::command_exists;

fn parse_short_diagnostics(text: &str, file_path: &Path) -> Vec<Value> {
    let file_name = file_path.to_string_lossy().to_string();
    let base = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let mut out = Vec::new();

    for line in text.lines() {
        if !(line.contains(&file_name) || line.contains(base)) {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() < 4 {
            continue;
        }
        let line_no = parts[1].trim().parse::<usize>().unwrap_or(0);
        let col_no = parts[2].trim().parse::<usize>().unwrap_or(0);
        let msg = parts[3].trim().to_string();
        let lower = msg.to_lowercase();
        let severity = if lower.contains(" warning") || lower.starts_with("warning") {
            "warning"
        } else if lower.contains(" note") || lower.starts_with("note") {
            "info"
        } else {
            "error"
        };
        out.push(json!({
            "message": msg,
            "severity": severity,
            "code": Value::Null,
            "line": line_no,
            "column": col_no,
            "end_line": line_no,
            "end_column": col_no,
            "source": "semantic",
            "tool": "checker"
        }));
    }
    out
}

fn parse_pyright_json(text: &str, file_path: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(v): Result<Value, _> = serde_json::from_str(text) else {
        return out;
    };
    let Some(arr) = v.get("generalDiagnostics").and_then(|d| d.as_array()) else {
        return out;
    };
    for d in arr {
        let file_ok = d
            .get("file")
            .and_then(|f| f.as_str())
            .map(|f| {
                let fp = file_path.to_string_lossy();
                f.ends_with(fp.as_ref()) || fp.ends_with(f)
            })
            .unwrap_or(false);
        if !file_ok {
            continue;
        }
        let msg = d
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Semantic diagnostic");
        let sev = d
            .get("severity")
            .and_then(|s| s.as_str())
            .unwrap_or("error");
        let code = d.get("rule").cloned().unwrap_or(Value::Null);
        let line = d
            .get("range")
            .and_then(|r| r.get("start"))
            .and_then(|s| s.get("line"))
            .and_then(|n| n.as_u64())
            .map(|n| n as usize + 1)
            .unwrap_or(0);
        let col = d
            .get("range")
            .and_then(|r| r.get("start"))
            .and_then(|s| s.get("character"))
            .and_then(|n| n.as_u64())
            .map(|n| n as usize + 1)
            .unwrap_or(0);
        let end_line = d
            .get("range")
            .and_then(|r| r.get("end"))
            .and_then(|s| s.get("line"))
            .and_then(|n| n.as_u64())
            .map(|n| n as usize + 1)
            .unwrap_or(line);
        let end_col = d
            .get("range")
            .and_then(|r| r.get("end"))
            .and_then(|s| s.get("character"))
            .and_then(|n| n.as_u64())
            .map(|n| n as usize + 1)
            .unwrap_or(col);
        out.push(json!({
            "message": msg,
            "severity": sev,
            "code": code,
            "line": line,
            "column": col,
            "end_line": end_line,
            "end_column": end_col,
            "source": "semantic",
            "tool": "pyright"
        }));
    }
    out
}

fn parse_cargo_json(text: &str, file_path: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    let file_name = file_path.to_string_lossy();
    for line in text.lines() {
        let Ok(v): Result<Value, _> = serde_json::from_str(line) else {
            continue;
        };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let Some(msg) = v.get("message") else {
            continue;
        };
        let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("error");
        let code = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .cloned()
            .unwrap_or(Value::Null);
        let message = msg
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("rustc diagnostic");
        let spans = msg
            .get("spans")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();
        for sp in spans {
            let Some(file) = sp.get("file_name").and_then(|f| f.as_str()) else {
                continue;
            };
            if !(file.ends_with(file_name.as_ref())
                || file.ends_with(
                    file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default(),
                ))
            {
                continue;
            }
            let line = sp.get("line_start").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
            let col = sp.get("column_start").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
            let end_line = sp
                .get("line_end")
                .and_then(|n| n.as_u64())
                .unwrap_or(line as u64) as usize;
            let end_col = sp
                .get("column_end")
                .and_then(|n| n.as_u64())
                .unwrap_or(col as u64) as usize;
            out.push(json!({
                "message": message,
                "severity": normalize_level(level),
                "code": code.clone(),
                "line": line,
                "column": col,
                "end_line": end_line,
                "end_column": end_col,
                "source": "semantic",
                "tool": "rustc"
            }));
        }
    }
    out
}

fn normalize_level(level: &str) -> &str {
    match level {
        "warning" => "warning",
        "note" | "help" => "info",
        _ => "error",
    }
}

fn file_mtime_ms(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let m = meta.modified().ok()?;
    let d = m.duration_since(UNIX_EPOCH).ok()?;
    Some(d.as_millis() as u64)
}

fn dedup_diagnostics(diags: Vec<Value>) -> Vec<Value> {
    use std::collections::HashMap;
    let mut best: HashMap<String, Value> = HashMap::new();
    for d in diags {
        let msg = d.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let line = d.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
        let col = d.get("column").and_then(|v| v.as_u64()).unwrap_or(0);
        let sev = d
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("error");
        let key = format!("{}|{}|{}|{}", msg, line, col, sev);

        let score = diagnostic_score(&d);
        let replace = best
            .get(&key)
            .map(|b| score > diagnostic_score(b))
            .unwrap_or(true);
        if replace {
            best.insert(key, d);
        }
    }
    let mut out: Vec<Value> = best.into_values().collect();
    out.sort_by(|a, b| {
        let la = a.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
        let lb = b.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
        let ca = a.get("column").and_then(|v| v.as_u64()).unwrap_or(0);
        let cb = b.get("column").and_then(|v| v.as_u64()).unwrap_or(0);
        la.cmp(&lb).then_with(|| ca.cmp(&cb))
    });
    out
}

fn diagnostic_score(d: &Value) -> i32 {
    // Prefer semantic/tool diagnostics when duplicates occur.
    match d.get("source").and_then(|v| v.as_str()).unwrap_or("") {
        "semantic" => 2,
        "syntax" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_workspace_pagination_bounds() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.py"), "print('a')\n").unwrap();
        std::fs::write(root.join("b.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("c.ts"), "function x(){}\n").unwrap();

        let engine = LspEngine::new(root);
        let page = engine
            .diagnostics_workspace_paginated("syntax", Some(2), 1)
            .await
            .unwrap();
        assert_eq!(
            page.get("offset").and_then(|v: &Value| v.as_u64()).unwrap(),
            1
        );
        assert_eq!(
            page.get("file_count")
                .and_then(|v: &Value| v.as_u64())
                .unwrap(),
            2
        );
        assert!(
            page.get("results")
                .and_then(|v: &Value| v.as_array())
                .is_some()
        );
    }

    #[test]
    fn test_dedup_prefers_semantic() {
        let syntax = json!({
            "message": "x",
            "severity": "error",
            "line": 1,
            "column": 1,
            "source": "syntax"
        });
        let sem = json!({
            "message": "x",
            "severity": "error",
            "line": 1,
            "column": 1,
            "source": "semantic"
        });
        let out = dedup_diagnostics(vec![syntax, sem]);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].get("source").and_then(|v| v.as_str()),
            Some("semantic")
        );
    }
}
