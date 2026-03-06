use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageDef {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default = "default_backend")]
    pub backend: String,
    pub query_file: Option<String>,
    pub wasm_file: Option<String>,
    #[serde(skip)]
    pub embedded_query: Option<&'static str>,
}

fn default_backend() -> String {
    "wasm".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GrammarRegistry {
    #[serde(flatten)]
    pub languages: HashMap<String, LanguageDef>,
}

const DEFAULT_LANGUAGES_TOML: &str = r#"
[rust]
extensions = ["rs"]
backend = "native"

[python]
extensions = ["py", "pyi", "pyc"]
backend = "native"

[javascript]
extensions = ["js", "jsx"]
backend = "native"

[typescript]
extensions = ["ts", "tsx"]
backend = "native"

[go]
extensions = ["go"]
backend = "native"

[c]
extensions = ["c", "h"]
backend = "native"

[cpp]
extensions = ["cpp", "cc", "cxx", "hpp", "hxx"]
backend = "native"

[java]
extensions = ["java"]
backend = "native"
"#;

impl GrammarRegistry {
    pub fn load(workspace_root: &Path) -> Self {
        let mut registry: GrammarRegistry = toml::from_str(DEFAULT_LANGUAGES_TOML).unwrap_or_default();
        
        // Attach default queries
        if let Some(rust) = registry.languages.get_mut("rust") { rust.embedded_query = Some(crate::symbols::RUST_QUERY); }
        if let Some(python) = registry.languages.get_mut("python") { python.embedded_query = Some(crate::symbols::PYTHON_QUERY); }
        if let Some(js) = registry.languages.get_mut("javascript") { js.embedded_query = Some(crate::symbols::JS_QUERY); }
        if let Some(ts) = registry.languages.get_mut("typescript") { ts.embedded_query = Some(crate::symbols::TS_QUERY); }
        if let Some(go) = registry.languages.get_mut("go") { go.embedded_query = Some(crate::symbols::GO_QUERY); }
        if let Some(c) = registry.languages.get_mut("c") { c.embedded_query = Some(crate::symbols::C_QUERY); }
        if let Some(cpp) = registry.languages.get_mut("cpp") { cpp.embedded_query = Some(crate::symbols::CPP_QUERY); }
        if let Some(java) = registry.languages.get_mut("java") { java.embedded_query = Some(crate::symbols::JAVA_QUERY); }

        let custom_toml = workspace_root.join(".curd/grammars/languages.toml");
        if custom_toml.exists()
            && let Ok(content) = fs::read_to_string(&custom_toml)
                && let Ok(custom_reg) = toml::from_str::<GrammarRegistry>(&content) {
                    for (name, def) in custom_reg.languages {
                        registry.languages.insert(name, def);
                    }
                }
        registry
    }

    pub fn lang_for_extension(&self, ext: &str) -> Option<String> {
        for (name, def) in &self.languages {
            if def.extensions.iter().any(|e| e == ext) {
                return Some(name.clone());
            }
        }
        None
    }

    pub fn get_query(&self, lang: &str, workspace_root: &Path) -> Option<String> {
        let def = self.languages.get(lang)?;
        if let Some(qf) = &def.query_file {
            let path = workspace_root.join(".curd/grammars").join(qf);
            if let Ok(content) = fs::read_to_string(path) {
                return Some(content);
            }
        }
        def.embedded_query.map(|s| s.to_string())
    }
}
