use crate::{BuildRequest, GraphEngine, SearchEngine, ShadowStore, run_build};
use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub enum RefactorAction {
    Rename {
        symbol: String,
        new_name: String,
        lsp_binary: Option<String>,
    },
    Move {
        symbol: String,
        target_file: PathBuf,
    },
    Extract {
        file_range: String,
        new_function_name: String,
    },
}

pub fn run_refactor(root: &Path, action: RefactorAction) -> Result<String> {
    let mut shadow = ShadowStore::new(root);
    if !shadow.is_active() {
        shadow.begin()?;
    }
    let shadow_root_path = shadow
        .shadow_root
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Failed to initialize shadow root"))?;

    let mut out = String::new();
    let search = SearchEngine::new(root);
    let graph = GraphEngine::new(root);
    let dep_graph = graph.build_dependency_graph()?;

    match action {
        RefactorAction::Rename {
            symbol,
            new_name,
            mut lsp_binary,
        } => {
            if symbol.starts_with('@') {
                return Err(anyhow::anyhow!(
                    "Refactoring symbols inside linked external contexts is prohibited."
                ));
            }

            let chosen_line = 0;
            let chosen_col = 0;
            let chosen_target_file = PathBuf::new();
            let lsp_auto_detected = false;

            // Auto-detect overloads if LSP is not explicitly provided
            if lsp_binary.is_none() {
                let parts: Vec<&str> = symbol.split(':').collect();
                let has_line_hint = parts.len() >= 3
                    && parts[parts.len() - 2].parse::<usize>().is_ok()
                    && parts[parts.len() - 1].parse::<usize>().is_ok();

                let symbol_name = symbol.split("::").last().unwrap_or(&symbol);
                if let Ok(mut symbols) = search.search(symbol_name, None) {
                    symbols.retain(|s| s.id == symbol);
                    if symbols.len() > 1 && !has_line_hint {
                        // Fail fast with options so the agent can re-plan with line numbers
                        let mut options = Vec::new();
                        for target in symbols.iter() {
                            let path = root.join(&target.filepath);
                            if let Ok(src) = std::fs::read_to_string(&path) {
                                let prefix = &src[..target.start_byte];
                                let line = prefix.lines().count(); // 1-based for display
                                let symbol_text =
                                    &src[target.start_byte..target.end_byte.min(src.len())];
                                let signature = symbol_text
                                    .lines()
                                    .next()
                                    .unwrap_or("")
                                    .trim_end_matches(" {")
                                    .trim_end_matches('{')
                                    .trim();
                                options.push(format!(
                                    "{} (line {}): {}",
                                    target.filepath.display(),
                                    line,
                                    signature
                                ));
                            }
                        }
                        return Err(anyhow::anyhow!(
                            "Symbol '{}' is overloaded. Please disambiguate by appending the line number to the URI (e.g., '{}::LINE_NUMBER').\nAvailable options:\n{}",
                            symbol_name,
                            symbol,
                            options.join("\n")
                        ));
                    } else if has_line_hint {
                        // Line hint provided, we will extract it below to start LSP
                        // But we need to set the LSP binary for this extension
                        let file_str = symbol[..symbol.len()
                            - parts[parts.len() - 1].len()
                            - parts[parts.len() - 2].len()
                            - 2]
                            .to_string();
                        let target_file = root.join(file_str);
                        let ext = target_file
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let auto_lsp = match ext.as_str() {
                            "rs" => Some("rust-analyzer"),
                            "py" => Some("pyright-langserver"),
                            "ts" | "tsx" | "js" | "jsx" => Some("typescript-language-server"),
                            "c" | "cpp" | "cc" | "h" | "hpp" => Some("clangd"),
                            "go" => Some("gopls"),
                            "java" => Some("jdtls"),
                            _ => None,
                        };
                        if let Some(cmd) = auto_lsp {
                            out.push_str(&format!(
                                "Auto-starting '{}' for safe renaming of specific overload...\n",
                                cmd
                            ));
                            lsp_binary = Some(cmd.to_string());
                        }
                    } else if symbols.len() == 1 {
                        // Just use greedy AST mode for single definitions
                    }
                }
            }

            if let Some(lsp_cmd) = lsp_binary {
                out.push_str(&format!(
                    "Using LSP ({}) to rename `{}` to `{}`...\n",
                    lsp_cmd, symbol, new_name
                ));

                let target_file;
                let mut line = 0;
                let mut col = 0;

                if lsp_auto_detected && chosen_target_file.exists() {
                    target_file = chosen_target_file;
                    line = chosen_line;
                    col = chosen_col;
                } else {
                    let parts: Vec<&str> = symbol.split(':').collect();
                    if parts.len() >= 3
                        && parts[parts.len() - 2].parse::<usize>().is_ok()
                        && parts[parts.len() - 1].parse::<usize>().is_ok()
                    {
                        col = parts[parts.len() - 1].parse::<usize>().unwrap();
                        line = parts[parts.len() - 2].parse::<usize>().unwrap();
                        let file_str = symbol[..symbol.len()
                            - parts[parts.len() - 1].len()
                            - parts[parts.len() - 2].len()
                            - 2]
                            .to_string();
                        target_file = root.join(file_str);
                    } else {
                        let symbol_name = symbol.split("::").last().unwrap_or(&symbol);
                        let mut symbols = search.search(symbol_name, None)?;
                        symbols.retain(|s| s.id == symbol);
                        if let Some(target) = symbols.first() {
                            target_file = root.join(&target.filepath);
                            if let Ok(src) = std::fs::read_to_string(&target_file) {
                                let prefix = &src[..target.start_byte];
                                line = prefix.lines().count().saturating_sub(1);
                                col = prefix.lines().last().unwrap_or("").len();
                            }
                        } else {
                            return Err(anyhow::anyhow!("Symbol not found: {}", symbol));
                        }
                    }
                }

                let mut client = crate::lsp_client::LspClient::new(&lsp_cmd, &[], root)?;
                client.initialize(root)?;
                let edit_result = client.rename(&target_file, line, col, &new_name)?;

                if let Some(changes) = edit_result.get("changes").and_then(|v| v.as_object()) {
                    for (uri, edits) in changes {
                        let rel_path = uri
                            .replace(&format!("file://{}", root.display()), "")
                            .trim_start_matches('/')
                            .to_string();
                        if crate::workspace::validate_sandboxed_path(root, &rel_path).is_err() {
                            continue;
                        } // Sandbox defense

                        let abs_path = root.join(&rel_path);
                        let shadow_file = shadow_root_path.join(&rel_path);

                        let mut content = std::fs::read_to_string(&shadow_file)
                            .or_else(|_| std::fs::read_to_string(&abs_path))?;

                        // We must apply edits from bottom to top to preserve character offsets (LSP returns line/col, so this is complex)
                        // A true implementation applies line/col mapping. For now, we print that we received it.
                        // Since text edits use Line/Col, we need a helper to apply them.
                        content = apply_lsp_edits(content, edits.as_array().unwrap_or(&vec![]))?;

                        if let Some(parent) = shadow_file.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(&shadow_file, &content)?;
                        shadow.stage(Path::new(&rel_path), &content).ok();
                        out.push_str(&format!("  Updated {}\n", rel_path));
                    }
                } else if let Some(doc_changes) = edit_result
                    .get("documentChanges")
                    .and_then(|v| v.as_array())
                {
                    for change in doc_changes {
                        if let Some(td) = change.get("textDocument") {
                            let uri = td.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                            let rel_path = uri
                                .replace(&format!("file://{}", root.display()), "")
                                .trim_start_matches('/')
                                .to_string();
                            if crate::workspace::validate_sandboxed_path(root, &rel_path).is_err() {
                                continue;
                            } // Sandbox defense

                            let abs_path = root.join(&rel_path);
                            let shadow_file = shadow_root_path.join(&rel_path);

                            let mut content = std::fs::read_to_string(&shadow_file)
                                .or_else(|_| std::fs::read_to_string(&abs_path))?;
                            if let Some(edits) = change.get("edits").and_then(|v| v.as_array()) {
                                content = apply_lsp_edits(content, edits)?;
                            }

                            if let Some(parent) = shadow_file.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            std::fs::write(&shadow_file, &content)?;
                            shadow.stage(Path::new(&rel_path), &content).ok();
                            out.push_str(&format!("  Updated {}\n", rel_path));
                        }
                    }
                } else {
                    out.push_str("LSP returned no changes.\n");
                }
            } else {
                out.push_str(&format!(
                    "Renaming symbol `{}` to `{}` (Greedy AST mode)...\n",
                    symbol, new_name
                ));

                // Collect all targets to modify (definition + callers)
                // Storing (start_byte, end_byte, old_name_to_replace)
                let mut modifications: std::collections::HashMap<
                    PathBuf,
                    Vec<(usize, usize, String)>,
                > = std::collections::HashMap::new();

                let symbol_name = symbol.split("::").last().unwrap_or(&symbol);
                let mut symbols = search.search(symbol_name, None)?;
                symbols.retain(|s| s.id == symbol);

                let old_name = if let Some(target) = symbols.first() {
                    target.name.clone()
                } else {
                    return Err(anyhow::anyhow!("Symbol not found: {}", symbol));
                };

                if let Some(target) = symbols.first() {
                    modifications
                        .entry(target.filepath.clone())
                        .or_default()
                        .push((target.start_byte, target.end_byte, old_name.clone()));
                }

                let callers = dep_graph.get_callers(&symbol);
                out.push_str(&format!("Updating {} callers...\n", callers.len()));
                let old_target_name = old_name.clone();
                let registry = crate::context_link::ContextRegistry::load(root);

                for caller_id in callers {
                    if caller_id.starts_with('@') {
                        let mut can_write = false;
                        let mut resolved_external_path = None;

                        if let Some(idx) = caller_id.find("::") {
                            let alias = &caller_id[..idx];
                            if let Some(link) = registry.contexts.get(alias) {
                                if link.mode == crate::context_link::ContextMode::Write {
                                    can_write = true;
                                    resolved_external_path = Some(link.path.clone());
                                } else {
                                    // Warn the external codebase about the breakage (Read-Only or Index modes)
                                    let alert_path = link.path.join(".curd/alerts.json");
                                    if let Some(parent) = alert_path.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let alert_msg = format!(
                                        "WARNING: Symbol `{}` calls `{}` which was just renamed to `{}` in an upstream workspace.",
                                        caller_id, symbol, new_name
                                    );

                                    let mut alerts: Vec<String> = if alert_path.exists() {
                                        std::fs::read_to_string(&alert_path)
                                            .ok()
                                            .and_then(|c| serde_json::from_str(&c).ok())
                                            .unwrap_or_default()
                                    } else {
                                        Vec::new()
                                    };
                                    alerts.push(alert_msg.clone());
                                    if let Ok(json) = serde_json::to_string_pretty(&alerts) {
                                        let _ = std::fs::write(&alert_path, json);
                                    }
                                    out.push_str(&format!(
                                        "  [EXTERNAL SKIP] {} (Warning appended to {})",
                                        alert_msg,
                                        alert_path.display()
                                    ));
                                }
                            }
                        }

                        if !can_write {
                            continue; // Do not attempt to rewrite callers in external read-only contexts
                        }

                        // We CAN write to it. Resolve its path and append modifications.
                        let caller_name = caller_id.split("::").last().unwrap_or(&caller_id);
                        let mut c_symbols = search.search(caller_name, None)?;
                        c_symbols.retain(|s| s.id == caller_id);
                        if let Some(caller) = c_symbols.first() {
                            let ext_root = resolved_external_path.unwrap();
                            let abs_ext_path = ext_root.join(
                                caller
                                    .filepath
                                    .strip_prefix(&ext_root)
                                    .unwrap_or(&caller.filepath),
                            );
                            modifications.entry(abs_ext_path).or_default().push((
                                caller.start_byte,
                                caller.end_byte,
                                old_target_name.clone(),
                            ));
                        }
                        continue;
                    }

                    let caller_name = caller_id.split("::").last().unwrap_or(&caller_id);
                    let mut c_symbols = search.search(caller_name, None)?;
                    c_symbols.retain(|s| s.id == caller_id);
                    if let Some(caller) = c_symbols.first() {
                        // Append caller modifications
                        modifications
                            .entry(caller.filepath.clone())
                            .or_default()
                            .push((caller.start_byte, caller.end_byte, old_target_name.clone()));
                    }
                }

                // Apply all modifications grouped by file, sorted descending by start_byte to prevent offset shifting
                for (filepath, mut mods) in modifications {
                    mods.sort_by(|a, b| b.0.cmp(&a.0)); // Descending start_byte

                    let abs_path = root.join(&filepath);
                    let shadow_file = shadow_root_path.join(&filepath);
                    let mut content = std::fs::read_to_string(&shadow_file)
                        .or_else(|_| std::fs::read_to_string(&abs_path))?;

                    for (start, end, text_to_replace) in mods {
                        if start >= content.len() || end > content.len() || start >= end {
                            continue; // Bounds might be stale if search index is old, but we fail gracefully
                        }
                        let chunk = &content[start..end];
                        let replaced = chunk.replace(&text_to_replace, &new_name);

                        let mut new_content = content[..start].to_string();
                        new_content.push_str(&replaced);
                        new_content.push_str(&content[end..]);
                        content = new_content;
                    }

                    if let Some(parent) = shadow_file.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::write(&shadow_file, &content)?;
                    shadow.stage(&filepath, &content).ok();
                }
            }

            // 3. Build-before-commit validation via curd build in shadow
            out.push_str("Running shadow build validation...\n");
            let build_resp = run_build(
                &shadow_root_path,
                BuildRequest {
                    adapter: None,
                    profile: None,
                    target: None,
                    execute: true,
                    zig: false,
                    command: None,
                    allow_untrusted: true, // Internal refactor builds are considered trusted as they are triggered by specific user action
                    trailing_args: Vec::new(),
                },
            )?;
            if build_resp.status != "ok" {
                out.push_str("Build failed! Rolling back changes.\n");
                shadow.rollback();
            } else {
                out.push_str("Build passed. Run `curd workspace commit` to apply.\n");
            }
        }
        RefactorAction::Move {
            symbol,
            target_file,
        } => {
            if symbol.starts_with('@') {
                return Err(anyhow::anyhow!(
                    "Refactoring symbols inside linked external contexts is prohibited."
                ));
            }

            out.push_str(&format!(
                "Moving symbol `{}` to `{}`...\n",
                symbol,
                target_file.display()
            ));

            let symbol_name = symbol.split("::").last().unwrap_or(&symbol);
            let mut symbols = search.search(symbol_name, None)?;
            symbols.retain(|s| s.id == symbol);

            if let Some(target) = symbols.first() {
                let source_file_path = root.join(&target.filepath);
                if let Ok(source) = std::fs::read_to_string(&source_file_path) {
                    let symbol_code = &source[target.start_byte..target.end_byte];
                    println!("MOVE: extracted symbol_code: {:?}", symbol_code);

                    // Remove from source file (using shadow)
                    let shadow_source_file = shadow_root_path.join(&target.filepath);
                    let shadow_source =
                        std::fs::read_to_string(&shadow_source_file).unwrap_or(source.clone());
                    let mut final_source_code = shadow_source[..target.start_byte].to_string();
                    final_source_code.push_str(&shadow_source[target.end_byte..]);
                    println!("MOVE: final_source_code: {:?}", final_source_code);
                    std::fs::write(&shadow_source_file, final_source_code.clone())?;
                    let stage_res = shadow.stage(&target.filepath, &final_source_code);
                    println!("MOVE: shadow.stage result: {:?}", stage_res);

                    // Insert into target file (using shadow)
                    let absolute_target = root.join(&target_file);
                    let shadow_target_file = shadow_root_path.join(&target_file);

                    if let Some(parent) = shadow_target_file.parent() {
                        std::fs::create_dir_all(parent)?;
                    }

                    let mut target_content = std::fs::read_to_string(&shadow_target_file)
                        .or_else(|_| std::fs::read_to_string(&absolute_target))
                        .unwrap_or_default();
                    if !target_content.ends_with('\n') && !target_content.is_empty() {
                        target_content.push('\n');
                    }
                    target_content.push_str(symbol_code);
                    target_content.push('\n');
                    std::fs::write(&shadow_target_file, target_content.clone())?;
                    shadow.stage(&target_file, &target_content).ok();
                    out.push_str("Move complete.\n");
                }
            } else {
                return Err(anyhow::anyhow!("Symbol not found: {}", symbol));
            }
        }
        RefactorAction::Extract {
            file_range,
            new_function_name,
        } => {
            if file_range.starts_with('@') {
                return Err(anyhow::anyhow!(
                    "Refactoring symbols inside linked external contexts is prohibited."
                ));
            }

            out.push_str(&format!(
                "Extracting `{}` to `{}`...\n",
                file_range, new_function_name
            ));
            let parts: Vec<&str> = file_range.split(':').collect();
            if parts.len() == 2 {
                let file_path = PathBuf::from(parts[0]);
                let range_parts: Vec<&str> = parts[1].split('-').collect();
                if range_parts.len() == 2 {
                    if let (Ok(start_line), Ok(end_line)) = (
                        range_parts[0].parse::<usize>(),
                        range_parts[1].parse::<usize>(),
                    ) {
                        let absolute_file = root.join(&file_path);
                        let shadow_file = shadow_root_path.join(&file_path);
                        if let Ok(source) = std::fs::read_to_string(&shadow_file)
                            .or_else(|_| std::fs::read_to_string(&absolute_file))
                        {
                            let lines: Vec<&str> = source.lines().collect();
                            if start_line > 0 && end_line <= lines.len() && start_line <= end_line {
                                let extracted_code = lines[start_line - 1..end_line].join("\n");
                                // Very naive extraction - in real life this uses AST
                                let new_func =
                                    format!("\ndef {}():\n{}\n", new_function_name, extracted_code);

                                let mut final_code = lines[..start_line - 1].join("\n");
                                final_code.push('\n');
                                final_code.push_str(&new_func);
                                final_code.push_str(&format!("    {}()\n", new_function_name));
                                final_code.push_str(&lines[end_line..].join("\n"));

                                std::fs::write(&shadow_file, final_code.clone())?;
                                shadow.stage(&file_path, &final_code).ok();
                                out.push_str("Extract complete.\n");
                            } else {
                                out.push_str("Invalid line range.\n");
                            }
                        } else {
                            out.push_str("File not found.\n");
                        }
                    } else {
                        out.push_str("Invalid line numbers.\n");
                    }
                } else {
                    out.push_str("Invalid range format, expected start-end.\n");
                }
            } else {
                out.push_str("Invalid file range format, expected file:start-end.\n");
            }
        }
    }

    Ok(out)
}

fn apply_lsp_edits(content: String, edits: &[serde_json::Value]) -> Result<String> {
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // Convert edits into a mutable, sortable list
    let mut parsed_edits = Vec::new();
    for e in edits {
        let text = e.get("newText").and_then(|v| v.as_str()).unwrap_or("");
        let range = e.get("range").unwrap();
        let start_line = range
            .get("start")
            .unwrap()
            .get("line")
            .unwrap()
            .as_u64()
            .unwrap() as usize;
        let start_col = range
            .get("start")
            .unwrap()
            .get("character")
            .unwrap()
            .as_u64()
            .unwrap() as usize;
        let end_line = range
            .get("end")
            .unwrap()
            .get("line")
            .unwrap()
            .as_u64()
            .unwrap() as usize;
        let end_col = range
            .get("end")
            .unwrap()
            .get("character")
            .unwrap()
            .as_u64()
            .unwrap() as usize;
        parsed_edits.push((start_line, start_col, end_line, end_col, text.to_string()));
    }

    // Sort edits in reverse (bottom-up, right-to-left) so indices stay valid
    parsed_edits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    for (sl, sc, el, ec, new_text) in parsed_edits {
        if sl >= lines.len() {
            continue;
        }

        if sl == el {
            let line = &lines[sl];
            let safe_sc = sc.min(line.len());
            let safe_ec = ec.min(line.len()).max(safe_sc);
            let mut new_line = line[..safe_sc].to_string();
            new_line.push_str(&new_text);
            new_line.push_str(&line[safe_ec..]);
            lines[sl] = new_line;
        } else {
            // Multi-line edit (rare for simple renames, but possible)
            // Left as an exercise or basic best-effort
            let start_line_text = &lines[sl][..sc.min(lines[sl].len())];
            let end_line_text = if el < lines.len() {
                &lines[el][ec.min(lines[el].len())..]
            } else {
                ""
            };

            let mut new_multiline = start_line_text.to_string();
            new_multiline.push_str(&new_text);
            new_multiline.push_str(end_line_text);

            lines[sl] = new_multiline;
            for i in (sl + 1)..=el.min(lines.len() - 1) {
                lines[i] = String::new(); // Tombstone deleted lines
            }
        }
    }

    // Remove tombstones and join
    Ok(lines
        .into_iter()
        .filter(|s| !s.is_empty() || edits.is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn setup_workspace(root: &Path) {
        fs::create_dir_all(root.join(".curd")).unwrap();
        // Create an empty curd config to avoid panics
        fs::write(root.join("curd.toml"), "").unwrap();

        // Copy grammars from the real workspace if running in tests
        if let Ok(cwd) = std::env::current_dir() {
            let real_grammars = cwd.join("../.curd/grammars"); // if running in curd-core, workspace root is ..
            let real_grammars_fallback = cwd.join(".curd/grammars"); // if running from workspace root
            let src = if real_grammars.exists() {
                real_grammars
            } else {
                real_grammars_fallback
            };
            if src.exists() {
                // symlink instead of copying for speed
                #[cfg(unix)]
                std::os::unix::fs::symlink(src, root.join(".curd/grammars")).unwrap();
            }
        }
    }

    #[test]
    fn test_refactor_extract() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        setup_workspace(root);

        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("main.py");
        // We simulate extracting the middle two lines
        fs::write(
            &file_path,
            "def foo():\n    a = 1\n    b = 2\n    return a + b\n",
        )
        .unwrap();

        let mut shadow = ShadowStore::new(root);
        shadow.begin().unwrap();

        let action = RefactorAction::Extract {
            file_range: "src/main.py:2-3".to_string(),
            new_function_name: "extracted_func".to_string(),
        };

        let out = run_refactor(root, action).unwrap();
        assert!(out.contains("Extract complete"));

        let shadow_file = shadow.shadow_root.as_ref().unwrap().join("src/main.py");
        let content = fs::read_to_string(&shadow_file).unwrap();

        if !content.contains("def extracted_func():") {
            panic!("Unexpected content:\n{}", content);
        }

        assert!(content.contains("def extracted_func():"));
        assert!(content.contains("a = 1"));
        assert!(content.contains("b = 2"));
        assert!(content.contains("extracted_func()"));
        // Make sure it preserves the rest
        assert!(content.contains("def foo():"));
        assert!(content.contains("return a + b"));
    }

    #[test]
    fn test_refactor_rename_multiple_callers() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        setup_workspace(root);

        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("main.py");
        let initial_code = "\
def old_func():
    pass

def caller_one():
    old_func()

def caller_two():
    old_func()
";
        fs::write(&file_path, initial_code).unwrap();

        let search = SearchEngine::new(root);
        let all_syms = search.search("", None).unwrap();
        println!(
            "All indexed symbols: {:?}",
            all_syms.iter().map(|s| s.id.clone()).collect::<Vec<_>>()
        );

        let action = RefactorAction::Rename {
            symbol: "src/main.py::old_func".to_string(),
            new_name: "new_func".to_string(),
            lsp_binary: None,
        };

        let _out =
            run_refactor(root, action).unwrap_or_else(|e| panic!("run_refactor failed: {}", e));

        let mut shadow = ShadowStore::new(root);
        shadow.begin().unwrap(); // Just to load state
        let shadow_file = shadow.shadow_root.as_ref().unwrap().join("src/main.py");
        let content = fs::read_to_string(&shadow_file).unwrap();

        assert!(content.contains("def new_func():"));
        assert!(!content.contains("def old_func():"));

        let calls = content.matches("new_func()").count();
        assert_eq!(
            calls, 3,
            "Expected 2 caller instances + 1 def to be updated, got:\n{}",
            content
        );
    }

    #[test]
    fn test_refactor_move() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        setup_workspace(root);

        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("a.py"), "def foo():\n    return 1\n").unwrap();

        let search = SearchEngine::new(root);
        search.search("", None).unwrap(); // index

        let action = RefactorAction::Move {
            symbol: "src/a.py::foo".to_string(),
            target_file: PathBuf::from("src/b.py"),
        };

        let out =
            run_refactor(root, action).unwrap_or_else(|e| panic!("run_refactor failed: {}", e));
        assert!(out.contains("Move complete"));

        let mut shadow = ShadowStore::new(root);
        shadow.begin().unwrap();

        let a_content =
            fs::read_to_string(shadow.shadow_root.as_ref().unwrap().join("src/a.py")).unwrap();
        if a_content.contains("def foo():") {
            panic!("Symbol was not removed! Content is:\n{}", a_content);
        }
        assert!(!a_content.contains("def foo():"));

        let b_content =
            fs::read_to_string(shadow.shadow_root.as_ref().unwrap().join("src/b.py")).unwrap();
        assert!(b_content.contains("def foo():"));
    }
}
