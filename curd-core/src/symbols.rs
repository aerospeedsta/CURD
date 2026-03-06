use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents the type of a parsed symbol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Class,
    Method,
}

pub const PYTHON_QUERY: &str = "(function_definition name: (identifier) @name) @func\n(class_definition name: (identifier) @name) @class";
pub const RUST_QUERY: &str = "(function_item name: (identifier) @name) @func\n(struct_item name: (type_identifier) @name) @class";
pub const JS_QUERY: &str = "(function_declaration name: (identifier) @name) @func\n(class_declaration name: (identifier) @name) @class";
pub const TS_QUERY: &str = "(function_declaration name: (identifier) @name) @func\n(class_declaration name: (type_identifier) @name) @class";
pub const GO_QUERY: &str = "(function_declaration name: (identifier) @name) @func\n(type_declaration (type_spec name: (type_identifier) @name type: (struct_type)) @class)";
pub const C_QUERY: &str = "(function_definition declarator: (function_declarator declarator: (identifier) @name)) @func\n(struct_specifier name: (type_identifier) @name) @class";
pub const CPP_QUERY: &str = "(function_definition declarator: (function_declarator declarator: (identifier) @name)) @func\n(class_specifier name: (type_identifier) @name) @class";
pub const JAVA_QUERY: &str = "(method_declaration name: (identifier) @name) @func\n(class_declaration name: (identifier) @name) @class";

/// Core metadata for a single symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: String, // e.g., "src/main.rs::MyStruct", "src/utils.py::helper"
    pub filepath: PathBuf,
    pub name: String,
    pub kind: SymbolKind,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub semantic_hash: String,
}

/// Index for querying and building symbols across the codebase
pub struct SymbolIndex {
    // Currently just holding an in-memory map of file paths to their symbols
    // This will evolve into a concurrent map for rayon processing
    pub symbols: Vec<Symbol>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
        }
    }

    pub fn insert(&mut self, symbol: Symbol) {
        self.symbols.push(symbol);
    }
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}
