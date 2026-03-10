use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use tree_sitter::StreamingIterator;
use sha2::Digest;

// --- IPC Protocol ---

#[derive(Deserialize, Debug)]
#[serde(tag = "method", content = "params")]
enum PluginRequest {
    #[serde(rename = "load_grammar")]
    LoadGrammar {
        language_id: String,
        plugin_path: String,
        function_name: String, // e.g., "tree_sitter_python"
    },
    #[serde(rename = "parse")]
    Parse {
        language_id: String,
        file_path: String,
        source_code: String,
        query_src: String,
    },
}

#[derive(Serialize, Debug)]
struct PluginResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn main() -> Result<()> {
    let mut loaded_libraries: HashMap<String, Library> = HashMap::new();
    
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // EOF or error
        };

        if line.trim().is_empty() {
            continue;
        }

        let res = match serde_json::from_str::<PluginRequest>(&line) {
            Ok(req) => handle_request(req, &mut loaded_libraries),
            Err(e) => PluginResponse {
                status: "error".to_string(),
                result: None,
                error: Some(format!("Invalid JSON request: {}", e)),
            },
        };

        let out = serde_json::to_string(&res).unwrap_or_else(|_| r#"{"status":"error","error":"Serialization failed"}"#.to_string());
        writeln!(stdout, "{}", out)?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_request(req: PluginRequest, libraries: &mut HashMap<String, Library>) -> PluginResponse {
    match req {
        PluginRequest::LoadGrammar { language_id, plugin_path, function_name } => {
            unsafe {
                match Library::new(&plugin_path) {
                    Ok(lib) => {
                        let result: Result<Symbol<unsafe extern "C" fn() -> tree_sitter::Language>, _> = lib.get(function_name.as_bytes());
                        match result {
                            Ok(_) => {
                                libraries.insert(language_id.clone(), lib);
                                PluginResponse {
                                    status: "ok".to_string(),
                                    result: Some(json!({"language_id": language_id, "loaded": true})),
                                    error: None,
                                }
                            }
                            Err(e) => {
                                PluginResponse {
                                    status: "error".to_string(),
                                    result: None,
                                    error: Some(format!("Failed to find symbol {}: {}", function_name, e)),
                                }
                            }
                        }
                    }
                    Err(e) => {
                        PluginResponse {
                            status: "error".to_string(),
                            result: None,
                            error: Some(format!("Failed to load plugin from {}: {}", plugin_path, e)),
                        }
                    }
                }
            }
        }
        PluginRequest::Parse { language_id, file_path, source_code, query_src } => {
            let language = if let Some(lib) = libraries.get(&language_id) {
                let func_name = format!("tree_sitter_{}", language_id);
                unsafe {
                    match lib.get::<unsafe extern "C" fn() -> tree_sitter::Language>(func_name.as_bytes()) {
                        Ok(func) => func(),
                        Err(e) => {
                            return PluginResponse {
                                status: "error".to_string(),
                                result: None,
                                error: Some(format!("Failed to load language function from plugin: {}", e)),
                            }
                        }
                    }
                }
            } else {
                // Fallback to built-in languages
                match language_id.as_str() {
                    "rust" => tree_sitter_rust::LANGUAGE.into(),
                    "python" => tree_sitter_python::LANGUAGE.into(),
                    "javascript" => tree_sitter_javascript::LANGUAGE.into(),
                    "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                    "go" => tree_sitter_go::LANGUAGE.into(),
                    "c" => tree_sitter_c::LANGUAGE.into(),
                    "cpp" => tree_sitter_cpp::LANGUAGE.into(),
                    "java" => tree_sitter_java::LANGUAGE.into(),
                    _ => {
                        return PluginResponse {
                            status: "error".to_string(),
                            result: None,
                            error: Some(format!("Language plugin not loaded and not built-in: {}", language_id)),
                        }
                    }
                }
            };

            let mut parser = tree_sitter::Parser::new();
            if let Err(e) = parser.set_language(&language) {
                return PluginResponse {
                    status: "error".to_string(),
                    result: None,
                    error: Some(format!("Failed to set parser language: {}", e)),
                }
            }

            match parser.parse(&source_code, None) {
                Some(tree) => {
                    let query = match tree_sitter::Query::new(&language, &query_src) {
                        Ok(q) => q,
                        Err(e) => {
                            return PluginResponse {
                                status: "error".to_string(),
                                result: None,
                                error: Some(format!("Failed to compile query: {}", e)),
                            }
                        }
                    };

                    let mut cursor = tree_sitter::QueryCursor::new();
                    let mut captures_iter = cursor.captures(&query, tree.root_node(), source_code.as_bytes());

                    let mut symbols_list = Vec::new();
                    let mut name_counts: HashMap<String, usize> = HashMap::new();

                    while let Some((mat, cap_idx)) = captures_iter.next() {
                        let cap = mat.captures[*cap_idx];
                        let node = cap.node;
                        let capture_name = &query.capture_names()[cap.index as usize];
                        
                        let mut role = "definition";
                        
                        match &capture_name[..] {
                            "stub" => role = "stub",
                            "def" | "definition" => role = "definition",
                            "ref" | "reference" => role = "reference",
                            "name" => continue, // We extract names via child fields to avoid duplicates
                            _ => {}
                        }

                        // Heuristic for kind based on node kind
                        let symbol_kind = match node.kind() {
                            "function_item" | "function_declaration" | "function_definition" | "method_declaration" | "method_definition" | "function" => "function",
                            "class_declaration" | "class_definition" | "class" => "class",
                            "struct_item" | "struct_specifier" | "struct" => "struct",
                            "interface_declaration" | "interface" | "type_alias_declaration" => "interface",
                            _ => "unknown",
                        };

                        let name_res: Result<&str, std::str::Utf8Error> = node.utf8_text(source_code.as_bytes());
                        let mut symbol_name = name_res.unwrap_or("unknown").to_string();
                        
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let child_name_res: Result<&str, std::str::Utf8Error> = name_node.utf8_text(source_code.as_bytes());
                            if let Ok(name_text) = child_name_res {
                                symbol_name = name_text.to_string();
                            }
                        }

                        let range = node.range();
                        let symbol_code = &source_code[range.start_byte..range.end_byte.min(source_code.len())];
                        let hash_str = format!("{:x}", sha2::Sha256::digest(symbol_code.as_bytes()));

                        let count = name_counts.entry(symbol_name.clone()).or_insert(0);
                        let id = if *count == 0 {
                            format!("{}::{}", file_path, symbol_name)
                        } else {
                            format!("{}::{}::#{}", file_path, symbol_name, count)
                        };
                        *count += 1;

                        symbols_list.push(json!({
                            "id": id,
                            "name": symbol_name,
                            "kind": symbol_kind,
                            "role": role,
                            "start_byte": range.start_byte,
                            "end_byte": range.end_byte,
                            "start_line": range.start_point.row + 1,
                            "end_line": range.end_point.row + 1,
                            "semantic_hash": hash_str,
                            "link_name": Value::Null,
                            "signature": Value::Null,
                            "fault_id": Value::Null
                        }));
                    }

                    PluginResponse {
                        status: "ok".to_string(),
                        result: Some(json!({"symbols": symbols_list})),
                        error: None,
                    }
                }
                None => {
                    PluginResponse {
                        status: "error".to_string(),
                        result: None,
                        error: Some("Parser returned None (timeout or cancellation)".to_string()),
                    }
                }
            }
        }
    }
}
