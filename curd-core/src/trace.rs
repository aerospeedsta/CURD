use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;

pub fn graph_context_for_text(
    workspace_root: &Path,
    seed_terms: &[String],
    texts: &[&str],
    noise_words: &[&str],
) -> Value {
    let graph = crate::GraphEngine::new(workspace_root);
    let search = crate::SearchEngine::new(workspace_root);
    let mut seeds = Vec::new();
    let mut seen = HashSet::new();
    let mut candidate_summary = TraceCandidateSummary::default();

    for seed in seed_terms {
        if seen.insert(seed.clone()) {
            seeds.push(seed.clone());
        }
    }

    for text in texts {
        let tokens = extract_tokens(text, noise_words);
        for token in &tokens {
            candidate_summary.observe(token);
        }
        for token in tokens {
            if let Ok(matches) = search.search(&token, None) {
                for symbol in matches.into_iter().take(4) {
                    if seen.insert(symbol.id.clone()) {
                        seeds.push(symbol.id);
                    }
                    if seeds.len() >= 6 {
                        break;
                    }
                }
            }
            if seeds.len() >= 6 {
                break;
            }
        }
        if seeds.len() >= 6 {
            break;
        }
    }

    if seeds.is_empty() {
        return json!({
            "seed_nodes": [],
            "candidate_summary": candidate_summary.to_json(),
            "failure_summary": summarize_failure_context(&candidate_summary, None),
        });
    }

    graph
        .graph_with_depths(seeds.clone(), 1, 1)
        .map(|res| {
            let candidate_json = candidate_summary.to_json();
            json!({
                "seed_nodes": seeds,
                "candidate_summary": candidate_json,
                "failure_summary": summarize_failure_context(&candidate_summary, Some(&res)),
                "graph": res,
            })
        })
        .unwrap_or_else(|_| {
            let candidate_json = candidate_summary.to_json();
            json!({
                "seed_nodes": seeds,
                "candidate_summary": candidate_json,
                "failure_summary": summarize_failure_context(&candidate_summary, None),
            })
        })
}

pub fn extract_tokens(text: &str, noise_words: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '.' | '/' | '\\') {
            current.push(ch);
        } else if !current.is_empty() {
            push_token(&mut out, &mut current, noise_words);
        }
    }
    push_token(&mut out, &mut current, noise_words);
    out.sort();
    out.dedup();
    out
}

fn push_token(out: &mut Vec<String>, current: &mut String, noise_words: &[&str]) {
    let token = current.trim_matches(|ch: char| {
        matches!(ch, ':' | '.' | '/' | '\\' | '(' | ')' | ',' | '"' | '\'')
    });
    for candidate in expand_trace_token_candidates(token) {
        if candidate.len() >= 3 && !noise_words.iter().any(|noise| noise == &candidate) {
            out.push(candidate);
        }
    }
    current.clear();
}

fn expand_trace_token_candidates(token: &str) -> Vec<String> {
    if token.is_empty() {
        return Vec::new();
    }
    let normalized = strip_path_position_suffix(token);
    let mut out = vec![normalized.to_string()];
    if (normalized.contains('/') || normalized.contains('\\'))
        && let Some((prefix, ext)) = normalized.rsplit_once('.')
        && !prefix.is_empty()
        && ext.len() <= 4
        && ext.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        out.push(prefix.to_string());
    }
    if let Some(leaf) = normalized.rsplit([':', '.', '/', '\\']).next()
        && leaf != normalized
        && !leaf.is_empty()
    {
        out.push(leaf.to_string());
    }
    if normalized.contains('/') || normalized.contains('\\') {
        if let Some(stem) = normalized
            .rsplit(['/', '\\'])
            .next()
            .and_then(|segment| segment.split('.').next())
            && !stem.is_empty()
        {
            out.push(stem.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn strip_path_position_suffix(token: &str) -> &str {
    let Some((base, tail)) = token.rsplit_once(':') else {
        return token;
    };
    if !tail.chars().all(|ch| ch.is_ascii_digit()) {
        return token;
    }
    let Some((base2, tail2)) = base.rsplit_once(':') else {
        return base;
    };
    if tail2.chars().all(|ch| ch.is_ascii_digit()) {
        return base2;
    }
    base
}

#[derive(Default)]
struct TraceCandidateSummary {
    qualified_symbols: HashSet<String>,
    file_candidates: HashSet<String>,
    leaf_candidates: HashSet<String>,
}

impl TraceCandidateSummary {
    fn observe(&mut self, token: &str) {
        let normalized = strip_path_position_suffix(token);
        if normalized.contains("::") {
            self.qualified_symbols.insert(normalized.to_string());
        }
        if normalized.contains('/') || normalized.contains('\\') {
            self.file_candidates
                .insert(preferred_file_candidate(normalized).to_string());
        }
        if !normalized.contains("::") && !normalized.contains('/') && !normalized.contains('\\') {
            self.leaf_candidates.insert(normalized.to_string());
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "qualified_symbols": sorted_values(&self.qualified_symbols),
            "file_candidates": sorted_values(&self.file_candidates),
            "leaf_candidates": sorted_values(&self.leaf_candidates),
        })
    }
}

fn preferred_file_candidate(token: &str) -> &str {
    if let Some((prefix, ext)) = token.rsplit_once('.')
        && !prefix.is_empty()
        && ext.len() <= 4
        && ext.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        return prefix;
    }
    token
}

fn sorted_values(values: &HashSet<String>) -> Vec<String> {
    let mut out: Vec<String> = values.iter().cloned().collect();
    out.sort();
    out
}

fn summarize_failure_context(summary: &TraceCandidateSummary, graph: Option<&Value>) -> Value {
    let seed_count = graph
        .and_then(|value| value.get("nodes"))
        .and_then(Value::as_array)
        .map(|nodes| nodes.len())
        .unwrap_or(0);
    let edge_count = graph
        .and_then(|value| value.get("detailed_edges"))
        .and_then(Value::as_array)
        .map(|edges| edges.len())
        .unwrap_or(0);
    let likely_primary_kind = if !summary.qualified_symbols.is_empty() {
        "qualified_symbol"
    } else if !summary.file_candidates.is_empty() {
        "file"
    } else if !summary.leaf_candidates.is_empty() {
        "leaf_symbol"
    } else {
        "unknown"
    };
    json!({
        "likely_primary_kind": likely_primary_kind,
        "qualified_symbol_count": summary.qualified_symbols.len(),
        "file_candidate_count": summary.file_candidates.len(),
        "leaf_candidate_count": summary.leaf_candidates.len(),
        "graph_node_count": seed_count,
        "graph_edge_count": edge_count,
        "has_graph_context": graph.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::{extract_tokens, graph_context_for_text};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn extracts_tokens_while_ignoring_noise_words() {
        let tokens = extract_tokens(
            "panic in caller: callee at src/lib.rs:12",
            &["panic", "at", "in"],
        );
        assert!(tokens.iter().any(|t| t == "caller"));
        assert!(tokens.iter().any(|t| t == "callee"));
        assert!(!tokens.iter().any(|t| t == "at"));
        assert!(!tokens.iter().any(|t| t == "panic"));
    }

    #[test]
    fn extracts_qualified_rust_trace_terms() {
        let tokens = extract_tokens(
            "thread 'main' panicked at src/lib.rs:12:5\n  0: mycrate::worker::run_job",
            &["thread", "panicked", "at", "main"],
        );
        assert!(tokens.iter().any(|t| t == "mycrate::worker::run_job"));
        assert!(tokens.iter().any(|t| t == "run_job"));
        assert!(tokens.iter().any(|t| t == "src/lib"));
        assert!(tokens.iter().any(|t| t == "lib"));
    }

    #[test]
    fn extracts_python_traceback_terms() {
        let tokens = extract_tokens(
            "Traceback (most recent call last):\n  File \"pkg/service.py\", line 41, in handle_event",
            &[
                "traceback",
                "most",
                "recent",
                "call",
                "last",
                "file",
                "line",
                "in",
            ],
        );
        assert!(tokens.iter().any(|t| t == "pkg/service"));
        assert!(tokens.iter().any(|t| t == "service"));
        assert!(tokens.iter().any(|t| t == "handle_event"));
    }

    #[test]
    fn graph_context_reports_trace_candidate_summary() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("src dir");
        fs::write(dir.path().join("src/lib.rs"), "fn run_job() {}\n").expect("write source");
        let enriched = graph_context_for_text(
            dir.path(),
            &[],
            &["thread 'main' panicked at src/lib.rs:12:5\n 0: mycrate::worker::run_job"],
            &["thread", "panicked", "at", "main"],
        );
        let summary = &enriched["candidate_summary"];
        assert!(
            summary["qualified_symbols"]
                .as_array()
                .map(|values| values
                    .iter()
                    .any(|value| value == "mycrate::worker::run_job"))
                .unwrap_or(false)
        );
        assert!(
            summary["file_candidates"]
                .as_array()
                .map(|values| values.iter().any(|value| value == "src/lib"))
                .unwrap_or(false)
        );
        assert!(
            summary["leaf_candidates"]
                .as_array()
                .map(|values| values.iter().any(|value| value == "run_job"))
                .unwrap_or(false)
        );
        assert_eq!(
            enriched["failure_summary"]["likely_primary_kind"],
            "qualified_symbol"
        );
        assert_eq!(enriched["failure_summary"]["file_candidate_count"], 1);
    }
}
