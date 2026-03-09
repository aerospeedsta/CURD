use anyhow::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

use crate::{GraphEngine, graph::FaultState};
use std::collections::HashMap;

/// Wraps the GraphEngine to export Mermaid-compatible Markdown flowcharts for agents
pub struct DiagramEngine {
    pub workspace_root: PathBuf,
}

impl DiagramEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
        }
    }

    /// Primary diagram generator based on Caller/Callee AST paths
    pub fn diagram(&self, uris: Vec<String>) -> Result<Value> {
        self.diagram_with_format(uris, "mermaid", 1, 1)
    }

    pub fn diagram_with_format(
        &self,
        uris: Vec<String>,
        format: &str,
        up_depth: u8,
        down_depth: u8,
    ) -> Result<Value> {
        let graph_engine = GraphEngine::new(&self.workspace_root);
        let graph_payload = graph_engine.graph_with_depths(uris.clone(), up_depth, down_depth)?;

        let mut edges: Vec<(String, String)> = Vec::new();
        if let Some(raw_edges) = graph_payload.get("edges").and_then(|e| e.as_array()) {
            for edge in raw_edges {
                let Some(arr) = edge.as_array() else {
                    continue;
                };
                if arr.len() != 2 {
                    continue;
                }
                let Some(from) = arr[0].as_str() else {
                    continue;
                };
                let Some(to) = arr[1].as_str() else {
                    continue;
                };
                edges.push((from.to_string(), to.to_string()));
            }
        }

        let mut fault_states = HashMap::new();
        if let Some(nodes) = graph_payload.get("nodes").and_then(|n| n.as_array()) {
            for node in nodes {
                let id = node.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(fs) = node.get("fault_state")
                    && let Ok(state) = serde_json::from_value::<FaultState>(fs.clone()) {
                        fault_states.insert(id.to_string(), state);
                    }
            }
        }

        let diagram = if format == "ascii" {
            render_ascii(&uris, &edges)
        } else {
            render_mermaid(&uris, &edges, &fault_states)
        };

        Ok(json!({
            "diagram": diagram,
            "format": format,
            "up_depth": up_depth,
            "down_depth": down_depth,
            "uris": uris
        }))
    }
}

fn safe_mermaid_id(uri: &str) -> String {
    uri.replace("::", "_")
        .replace(".", "_")
        .replace("/", "_")
        .replace("-", "_")
        .replace("[", "_")
        .replace("]", "_")
        .replace("\"", "_")
        .replace("\n", "_")
}

fn render_mermaid(
    uris: &[String],
    edges: &[(String, String)],
    fault_states: &HashMap<String, FaultState>,
) -> String {
    let mut out = String::from("```mermaid\ngraph TD;\n");
    out.push_str("  classDef poisoned fill:#ff4444,stroke:#333,stroke-width:2px,color:#fff;\n");

    let mut nodes_in_diagram = HashSet::new();

    if edges.is_empty() {
        for uri in uris {
            let sid = safe_mermaid_id(uri);
            let escaped = escape_mermaid_label(uri);
            out.push_str(&format!("  {}[\"{}\"];\n", sid, escaped));
            nodes_in_diagram.insert(uri.to_string());
        }
    } else {
        for (from, to) in edges {
            let sid_from = safe_mermaid_id(from);
            let sid_to = safe_mermaid_id(to);
            let label_from = escape_mermaid_label(from);
            let label_to = escape_mermaid_label(to);

            out.push_str(&format!(
                "  {}[\"{}\"] --> {}[\"{}\"];\n",
                sid_from, label_from, sid_to, label_to
            ));
            nodes_in_diagram.insert(from.to_string());
            nodes_in_diagram.insert(to.to_string());
        }
    }

    // Apply styles to poisoned nodes
    for (id, state) in fault_states {
        if nodes_in_diagram.contains(id) && matches!(state, FaultState::Poisoned(_)) {
            out.push_str(&format!("  class {} poisoned;\n", safe_mermaid_id(id)));
        }
    }

    out.push_str("```\n");
    out
}

fn escape_mermaid_label(s: &str) -> String {
    s.replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
        .replace('[', "#91;")
        .replace(']', "#93;")
        .replace('`', "#96;")
}

use std::collections::HashSet;

fn render_ascii(uris: &[String], edges: &[(String, String)]) -> String {
    let mut out = String::from("\n\x1b[1;36m┌── Symbol Dependency Matrix\x1b[0m\n");
    if edges.is_empty() {
        for uri in uris {
            out.push_str(&format!("│  \x1b[33m•\x1b[0m {}\n", uri));
        }
        out.push_str("└───────────────────────────\n");
        return out;
    }

    // Build adjacency list
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_nodes = HashSet::new();
    for (from, to) in edges {
        adj.entry(from.clone()).or_default().push(to.clone());
        all_nodes.insert(from.clone());
        all_nodes.insert(to.clone());
    }

    // Find roots (nodes that are in uris and have children)
    for root in uris {
        if !all_nodes.contains(root) {
            out.push_str(&format!("│  \x1b[33m•\x1b[0m {}\n", root));
            continue;
        }
        render_tree_recursive(root, &adj, &mut HashSet::new(), "", true, &mut out);
    }
    out.push_str("\x1b[1;36m└───────────────────────────\x1b[0m\n");
    out
}

fn render_tree_recursive(
    node: &str,
    adj: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    prefix: &str,
    is_last: bool,
    out: &mut String,
) {
    let connector = if prefix.is_empty() {
        "│  "
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    out.push_str(&format!("│  {}{}{}\n", prefix, connector, node));

    if visited.contains(node) {
        return;
    }
    visited.insert(node.to_string());

    if let Some(children) = adj.get(node) {
        let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
        for (i, child) in children.iter().enumerate() {
            render_tree_recursive(
                child,
                adj,
                visited,
                &new_prefix,
                i == children.len() - 1,
                out,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_render_contains_edges() {
        let uris = vec!["a.py::x".to_string()];
        let edges = vec![("a.py::x".to_string(), "b.py::y".to_string())];
        let out = render_ascii(&uris, &edges);
        assert!(out.contains("a.py::x -> b.py::y"));
    }

    #[test]
    fn test_mermaid_render_wraps_fence() {
        let uris = vec!["a.py::x".to_string()];
        let edges: Vec<(String, String)> = Vec::new();
        let fault_states = HashMap::new();
        let out = render_mermaid(&uris, &edges, &fault_states);
        assert!(out.starts_with("```mermaid"));
        assert!(out.contains("a.py::x"));
    }
}
