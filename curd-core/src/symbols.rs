use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents the type of a parsed symbol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Interface,
    Module,
    Variable,
    Method,
    Unknown,
}

pub const PYTHON_QUERY: &str = "(function_definition name: (identifier) @name) @def\n(class_definition name: (identifier) @name) @def";
pub const RUST_QUERY: &str = "(function_item name: (identifier) @name) @def\n(struct_item name: (type_identifier) @name) @def";
pub const JS_QUERY: &str = "(function_declaration name: (identifier) @name) @def\n(class_declaration name: (identifier) @name) @def";
pub const TS_QUERY: &str = "(function_declaration name: (identifier) @name) @def\n(class_declaration name: (type_identifier) @name) @def\n(method_definition name: (property_identifier) @name) @def\n(interface_declaration name: (type_identifier) @name) @def\n(type_alias_declaration name: (type_identifier) @name) @def\n(ambient_declaration (function_declaration name: (identifier) @name)) @stub";
pub const GO_QUERY: &str = "(function_declaration name: (identifier) @name) @def\n(type_declaration (type_spec name: (type_identifier) @name type: (struct_type)) @def)";
pub const C_QUERY: &str = "(function_definition declarator: (function_declarator declarator: (identifier) @name)) @def\n(declaration declarator: (function_declarator declarator: (identifier) @name)) @stub\n(struct_specifier name: (type_identifier) @name) @def";
pub const CPP_QUERY: &str = "(function_definition declarator: (function_declarator declarator: (identifier) @name)) @def\n(declaration declarator: (function_declarator declarator: (identifier) @name)) @stub\n(class_specifier name: (type_identifier) @name) @def";
pub const JAVA_QUERY: &str = "(method_declaration name: (identifier) @name) @def\n(class_declaration name: (identifier) @name) @def";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolRole {
    Definition,
    Stub,
    Reference,
}

/// Core metadata for a single symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: String, // e.g., "src/main.rs::MyStruct", "src/utils.py::helper"
    pub filepath: PathBuf,
    pub name: String,
    pub kind: SymbolKind,
    pub role: SymbolRole,
    pub link_name: Option<String>, // Explicit link identifier (e.g. extern "C" name)
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub semantic_hash: Option<String>,
    pub fault_id: Option<String>,
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
