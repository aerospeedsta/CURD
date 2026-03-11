use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tree_sitter::wasmtime::Engine;
use tree_sitter::{Parser, Range, WasmStore};

use crate::plugin_client::PluginClient;

/// Manages Tree-sitter parsing and lazy loading of WASM grammars.
#[derive(Clone)]
pub struct ParserManager {
    pub engine: Engine,
    local_grammars_dir: PathBuf,
    global_grammars_dir: PathBuf,
    backend_preference: String,
    loaded_wasm_bytes: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    download_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    resolved_backend_by_lang: Arc<Mutex<HashMap<String, String>>>,
    pub registry: crate::registry::GrammarRegistry,
    pub plugin_client: Option<Arc<PluginClient>>,
}

fn get_home_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    } else {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

impl ParserManager {
    pub fn new(local_grammars_dir: PathBuf) -> Result<Self> {
        Self::new_with_backend(local_grammars_dir, parser_backend())
    }

    pub fn new_with_backend(
        local_grammars_dir: PathBuf,
        backend_preference: String,
    ) -> Result<Self> {
        let engine = Engine::default();

        if !local_grammars_dir.exists() {
            fs::create_dir_all(&local_grammars_dir)
                .context("Failed to create local grammars directory")?;
        }

        let global_grammars_dir = get_home_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(".curd")
            .join("grammars");

        if !global_grammars_dir.exists() {
            let _ = fs::create_dir_all(&global_grammars_dir);
        }

        // Determine workspace root roughly from local_grammars_dir if possible, else fallback to current_dir
        let workspace_root = local_grammars_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let registry = crate::registry::GrammarRegistry::load(&workspace_root);

        let plugin_client = if backend_preference == "native" || backend_preference == "plugin" {
            PluginClient::new(&workspace_root).ok().map(Arc::new)
        } else {
            None
        };

        Ok(Self {
            engine,
            local_grammars_dir,
            global_grammars_dir,
            backend_preference,
            loaded_wasm_bytes: Arc::new(RwLock::new(HashMap::new())),
            download_locks: Arc::new(Mutex::new(HashMap::new())),
            resolved_backend_by_lang: Arc::new(Mutex::new(HashMap::new())),
            registry,
            plugin_client,
        })
    }
    pub fn get_language_bytes(&mut self, language_name: &str) -> Result<Vec<u8>> {
        if language_name.is_empty()
            || !language_name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            anyhow::bail!("Invalid language name: '{}'", language_name);
        }

        {
            let cache = self
                .loaded_wasm_bytes
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
            if let Some(bytes) = cache.get(language_name) {
                return Ok(bytes.clone());
            }
        }

        let lang_lock = {
            let mut locks = self
                .download_locks
                .lock()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
            locks
                .entry(language_name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = lang_lock
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        {
            let cache = self
                .loaded_wasm_bytes
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
            if let Some(bytes) = cache.get(language_name) {
                return Ok(bytes.clone());
            }
        }

        let wasm_filename = format!("tree-sitter-{}.wasm", language_name);
        let local_path = self.local_grammars_dir.join(&wasm_filename);
        let global_path = self.global_grammars_dir.join(&wasm_filename);

        if local_path.exists() {
            let bytes = fs::read(&local_path)?;
            self.validate_checksum(language_name, &bytes, &wasm_filename)?;
            self.loaded_wasm_bytes
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?
                .insert(language_name.to_string(), bytes.clone());
            return Ok(bytes);
        }

        if global_path.exists() {
            let bytes = fs::read(&global_path)?;
            if self
                .validate_checksum(language_name, &bytes, &wasm_filename)
                .is_ok()
            {
                self.loaded_wasm_bytes
                    .write()
                    .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?
                    .insert(language_name.to_string(), bytes.clone());
                return Ok(bytes);
            }
            // If checksum fails, we fall through to download again
        }

        self.download_grammar(language_name)
    }

    pub fn download_grammar(&mut self, language_name: &str) -> Result<Vec<u8>> {
        let wasm_filename = format!("tree-sitter-{}.wasm", language_name);
        let url = format!(
            "https://github.com/tree-sitter/tree-sitter-{}/releases/latest/download/{}",
            language_name, wasm_filename
        );

        let response = ureq::get(&url).call();

        let response = match response {
            Ok(r) => r,
            Err(_) => {
                // Fallback to unpkg for older ABIs or missing releases
                let fallback_url = format!(
                    "https://unpkg.com/tree-sitter-wasms@0.1.13/out/{}",
                    wasm_filename
                );
                ureq::get(&fallback_url).call().with_context(|| {
                    format!(
                        "Failed to download grammar from {} or {}",
                        url, fallback_url
                    )
                })?
            }
        };

        if response.status() == 200 {
            let mut bytes = Vec::new();
            response
                .into_reader()
                .take(10 * 1024 * 1024)
                .read_to_end(&mut bytes)?;

            let global_path = self.global_grammars_dir.join(&wasm_filename);
            let _ = fs::write(&global_path, &bytes);

            let _ = self.validate_checksum(language_name, &bytes, &wasm_filename);
            self.loaded_wasm_bytes
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?
                .insert(language_name.to_string(), bytes.clone());
            Ok(bytes)
        } else {
            anyhow::bail!("Failed to download grammar: HTTP {}", response.status());
        }
    }

    fn validate_checksum(
        &self,
        language_name: &str,
        bytes: &[u8],
        _wasm_filename: &str,
    ) -> Result<()> {
        let expected = get_expected_checksum(language_name);
        if let Some(expected_hex) = expected {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let actual_hex = format!("{:x}", hasher.finalize());
            if actual_hex != expected_hex && ["python", "rust"].contains(&language_name) {
                anyhow::bail!("Checksum mismatch for core language {}", language_name);
            }
        }
        Ok(())
    }

    pub fn create_parser(&mut self, language_name: &str) -> Result<Parser> {
        if (self.backend_preference == "native" || self.backend_preference == "plugin")
            && self.plugin_client.is_some()
        {
            if let Ok(mut m) = self.resolved_backend_by_lang.lock() {
                m.insert(language_name.to_string(), "sidecar".to_string());
            }
            // Return a dummy parser for the main process; the actual parsing happens in SearchEngine::parse_file_with_context
            return Ok(Parser::new());
        }

        if self.backend_preference == "native"
            && let Ok(parser) = self.create_native_parser(language_name)
        {
            if let Ok(mut m) = self.resolved_backend_by_lang.lock() {
                m.insert(language_name.to_string(), "native".to_string());
            }
            return Ok(parser);
        }
        let wasm_bytes = self.get_language_bytes(language_name)?;
        let mut wasm_store = WasmStore::new(&self.engine)?;
        let language = wasm_store.load_language(language_name, &wasm_bytes)?;
        let mut parser = Parser::new();
        parser.set_wasm_store(wasm_store)?;
        parser.set_language(&language)?;
        if let Ok(mut m) = self.resolved_backend_by_lang.lock() {
            m.insert(language_name.to_string(), "wasm".to_string());
        }
        Ok(parser)
    }

    pub fn resolved_backend_for_language(&self, language_name: &str) -> Option<String> {
        self.resolved_backend_by_lang
            .lock()
            .ok()
            .and_then(|m| m.get(language_name).cloned())
    }

    fn create_native_parser(&self, language_name: &str) -> Result<Parser> {
        let mut parser = Parser::new();
        match language_name {
            "rust" => {
                let lang = tree_sitter_rust::LANGUAGE;
                parser.set_language(&lang.into())?;
            }
            "python" => {
                let lang = tree_sitter_python::LANGUAGE;
                parser.set_language(&lang.into())?;
            }
            "javascript" => {
                let lang = tree_sitter_javascript::LANGUAGE;
                parser.set_language(&lang.into())?;
            }
            "typescript" => {
                let lang = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
                parser.set_language(&lang.into())?;
            }
            "go" => {
                let lang = tree_sitter_go::LANGUAGE;
                parser.set_language(&lang.into())?;
            }
            "c" => {
                let lang = tree_sitter_c::LANGUAGE;
                parser.set_language(&lang.into())?;
            }
            "cpp" => {
                let lang = tree_sitter_cpp::LANGUAGE;
                parser.set_language(&lang.into())?;
            }
            "java" => {
                let lang = tree_sitter_java::LANGUAGE;
                parser.set_language(&lang.into())?;
            }
            _ => {
                anyhow::bail!(
                    "Native backend not available for language: {}",
                    language_name
                );
            }
        }
        eprintln!("DEBUG: create_native_parser SUCCESS for {}", language_name);
        Ok(parser)
    }

    pub fn load_query(&self, language_name: &str) -> Result<String> {
        use crate::symbols::*;
        // Priority:
        // 1. Local override in .curd/queries/<lang>.scm
        // 2. Built-in core queries
        let queries_dir = self.local_grammars_dir.parent().map(|p| p.join("queries"));
        if let Some(dir) = queries_dir {
            let local_query = dir.join(format!("{}.scm", language_name));
            if local_query.exists() {
                return Ok(fs::read_to_string(local_query)?);
            }
        }

        match language_name {
            "python" => Ok(PYTHON_QUERY.to_string()),
            "rust" => Ok(RUST_QUERY.to_string()),
            "javascript" => Ok(JS_QUERY.to_string()),
            "typescript" => Ok(TS_QUERY.to_string()),
            "go" => Ok(GO_QUERY.to_string()),
            "c" => Ok(C_QUERY.to_string()),
            "cpp" => Ok(CPP_QUERY.to_string()),
            "java" => Ok(JAVA_QUERY.to_string()),
            _ => Ok(String::new()),
        }
    }

    pub fn bootstrap_core_grammars(&mut self) -> Result<()> {
        let core_langs = [
            "rust",
            "python",
            "javascript",
            "typescript",
            "go",
            "c",
            "cpp",
            "java",
        ];
        for lang in core_langs {
            if let Err(e) = self.get_language_bytes(lang) {
                log::warn!("Failed to bootstrap grammar for {}: {}", lang, e);
            }
        }
        Ok(())
    }

    pub fn count_nodes(&mut self, language_name: &str, source: &str) -> Result<usize> {
        if source.trim().is_empty() {
            return Ok(0);
        }
        let tree = self
            .create_parser(language_name)?
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Parse failed"))?;
        let mut count = 0;
        let mut cursor = tree.walk();
        loop {
            count += 1;
            if cursor.goto_first_child() {
                continue;
            }
            if cursor.goto_next_sibling() {
                continue;
            }
            loop {
                if !cursor.goto_parent() {
                    return Ok(count);
                }
                if cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    pub fn bind_fault_to_ast(
        &mut self,
        language_name: &str,
        source: &str,
        line: usize,
        column: usize,
    ) -> Result<Range> {
        let tree = self
            .create_parser(language_name)?
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Parse failed"))?;
        let node = tree
            .root_node()
            .descendant_for_point_range(
                tree_sitter::Point { row: line, column },
                tree_sitter::Point { row: line, column },
            )
            .ok_or_else(|| anyhow::anyhow!("No node found"))?;
        Ok(node.range())
    }
}

fn parser_backend() -> String {
    std::env::var("CURD_PARSER_BACKEND")
        .unwrap_or_else(|_| "wasm".to_string())
        .to_lowercase()
}

fn get_expected_checksum(lang: &str) -> Option<&'static str> {
    match lang {
        "python" => Some("16108b50df4ee9a30168794252ab55e7c93bfc5765d7fa0aa3e335752c515f47"),
        "rust" => Some("44b8d1a2e307ee8933d7c0bcb6b0f30d56bc999999fd1d2d9608a7dcf6e8ce56"),
        _ => None,
    }
}
