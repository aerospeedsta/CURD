use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::{GraphEngine, Sandbox};

pub struct ProfileEngine {
    pub workspace_root: PathBuf,
    sandbox: Sandbox,
}

impl ProfileEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let root = workspace_root.as_ref().to_path_buf();
        Self {
            workspace_root: std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone()),
            sandbox: Sandbox::new(root),
        }
    }

    pub async fn profile(
        &self,
        command: Option<&str>,
        roots: Vec<String>,
        up_depth: u8,
        down_depth: u8,
        format: &str,
    ) -> Result<Value> {
        let sampling_capabilities = self.sampling_capabilities();
        let runtime = if let Some(cmd) = command {
            let shell_engine = crate::ShellEngine::new(&self.workspace_root);
            let shell_res = shell_engine.shell(cmd, None, false).await?;

            Some(json!({
                "command": cmd,
                "exit_code": shell_res.get("exit_code").cloned().unwrap_or(json!(-1)),
                "stdout": shell_res.get("stdout").cloned().unwrap_or(json!("")),
                "stderr": shell_res.get("stderr").cloned().unwrap_or(json!("")),
            }))
        } else {
            None
        };

        // True sampling path: use py-spy when profiling python commands and tool is available.
        if let Some(cmd) = command
            && looks_like_python_command(cmd)
            && command_exists("py-spy", &self.workspace_root)
            && let Ok(sampled) = self.profile_with_py_spy(cmd, format).await {
                return Ok(json!({
                    "format": format,
                    "roots": roots,
                    "up_depth": up_depth,
                    "down_depth": down_depth,
                    "runtime": runtime,
                    "sampling": {
                        "engine": "py-spy",
                        "mode": "sampled",
                        "capabilities": sampling_capabilities
                    },
                    "folded": sampled.get("folded").cloned().unwrap_or_else(|| json!("")),
                    "flamegraph": sampled.get("flamegraph").cloned().unwrap_or_else(|| json!(""))
                }));
            }

        let graph_engine = GraphEngine::new(&self.workspace_root);
        let graph_payload = graph_engine.graph_with_depths(roots.clone(), up_depth, down_depth)?;
        let edges = parse_edges(&graph_payload);
        let folded = build_folded(&roots, &edges);
        let rendered = match format {
            "folded" => json!(folded.clone()),
            "speedscope" => speedscope_from_folded(&folded),
            _ => json!(render_ascii_flamegraph(&folded)),
        };

        Ok(json!({
            "format": format,
            "roots": roots,
            "up_depth": up_depth,
            "down_depth": down_depth,
            "runtime": runtime,
            "sampling": {
                "engine": "graph-approx",
                "mode": "approximate",
                "capabilities": sampling_capabilities
            },
            "folded": folded,
            "flamegraph": rendered
        }))
    }

    pub async fn profile_diff(
        &self,
        baseline_command: &str,
        candidate_command: &str,
        roots: Vec<String>,
        up_depth: u8,
        down_depth: u8,
    ) -> Result<Value> {
        let baseline = self
            .profile(
                Some(baseline_command),
                roots.clone(),
                up_depth,
                down_depth,
                "folded",
            )
            .await?;
        let candidate = self
            .profile(
                Some(candidate_command),
                roots,
                up_depth,
                down_depth,
                "folded",
            )
            .await?;
        let base_folded = baseline
            .get("folded")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cand_folded = candidate
            .get("folded")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let base_weights = folded_frame_weights(base_folded);
        let cand_weights = folded_frame_weights(cand_folded);
        let mut frame_deltas = Vec::new();
        let mut keys: HashSet<String> = base_weights.keys().cloned().collect();
        keys.extend(cand_weights.keys().cloned());
        for k in keys {
            let b = *base_weights.get(&k).unwrap_or(&0);
            let c = *cand_weights.get(&k).unwrap_or(&0);
            frame_deltas.push(json!({
                "frame": k,
                "baseline": b,
                "candidate": c,
                "delta": c - b
            }));
        }
        frame_deltas.sort_by(|a, b| {
            b.get("delta")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .abs()
                .cmp(&a.get("delta").and_then(|v| v.as_i64()).unwrap_or(0).abs())
        });

        Ok(json!({
            "baseline_command": baseline_command,
            "candidate_command": candidate_command,
            "baseline": baseline,
            "candidate": candidate,
            "frame_deltas": frame_deltas
        }))
    }

    fn sampling_capabilities(&self) -> Value {
        json!({
            "py_spy": command_exists("py-spy", &self.workspace_root),
            "perf": command_exists("perf", &self.workspace_root),
            "dtrace": command_exists("dtrace", &self.workspace_root)
        })
    }

    async fn profile_with_py_spy(&self, command: &str, format: &str) -> Result<Value> {
        let shell_engine = crate::ShellEngine::new(&self.workspace_root);
        shell_engine.validate_command(command)?;
        shell_engine.check_package_manager_policy(command)?;

        let (program, args) = crate::shell::parse_command(command)?;

        let mut py_spy_args = vec![
            "record".to_string(),
            "-o".to_string(),
            "-".to_string(),
            "--format".to_string(),
            "raw".to_string(),
            "--".to_string(),
            program,
        ];
        py_spy_args.extend(args);

        let mut command_obj = self.sandbox.build_command("py-spy", &py_spy_args);
        let output = command_obj
            .current_dir(&self.workspace_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let folded = String::from_utf8_lossy(&output.stdout).to_string();
        let flamegraph = match format {
            "folded" => json!(folded.clone()),
            "speedscope" => speedscope_from_folded(&folded),
            _ => json!(render_ascii_flamegraph(&folded)),
        };
        Ok(json!({
            "folded": folded,
            "flamegraph": flamegraph
        }))
    }
}

fn parse_edges(payload: &Value) -> Vec<(String, String)> {
    let mut edges = Vec::new();
    let Some(raw_edges) = payload.get("edges").and_then(|e| e.as_array()) else {
        return edges;
    };
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
    edges
}

fn build_folded(roots: &[String], edges: &[(String, String)]) -> String {
    const MAX_PATHS: usize = 50_000;
    const MAX_DEPTH: usize = 64;
    let mut out = String::new();
    let mut emitted_paths = 0usize;
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to) in edges {
        adj.entry(from.clone()).or_default().push(to.clone());
    }
    for v in adj.values_mut() {
        v.sort();
        v.dedup();
    }
    for root in roots {
        let mut in_stack = HashSet::new();
        let mut stack = vec![root.clone()];
        let mut ctx = DfsContext {
            adj: &adj,
            in_stack: &mut in_stack,
            stack: &mut stack,
            out: &mut out,
            emitted_paths: &mut emitted_paths,
            max_paths: MAX_PATHS,
            max_depth: MAX_DEPTH,
        };
        dfs_folded(root, &mut ctx);
        if emitted_paths >= MAX_PATHS {
            break;
        }
    }
    out
}

struct DfsContext<'a> {
    adj: &'a HashMap<String, Vec<String>>,
    in_stack: &'a mut HashSet<String>,
    stack: &'a mut Vec<String>,
    out: &'a mut String,
    emitted_paths: &'a mut usize,
    max_paths: usize,
    max_depth: usize,
}

fn dfs_folded(node: &str, ctx: &mut DfsContext) {
    if *ctx.emitted_paths >= ctx.max_paths {
        return;
    }
    if !ctx.in_stack.insert(node.to_string()) {
        // Cycle detected in the current path; emit the current stack and stop this branch.
        ctx.out
            .push_str(&format!("{} {}\n", ctx.stack.join(";"), 1));
        *ctx.emitted_paths += 1;
        return;
    }
    let Some(next) = ctx.adj.get(node) else {
        ctx.out
            .push_str(&format!("{} {}\n", ctx.stack.join(";"), 1));
        *ctx.emitted_paths += 1;
        return;
    };
    if next.is_empty() || ctx.stack.len() >= ctx.max_depth {
        ctx.out
            .push_str(&format!("{} {}\n", ctx.stack.join(";"), 1));
        *ctx.emitted_paths += 1;
        return;
    }
    for child in next {
        if *ctx.emitted_paths >= ctx.max_paths {
            break;
        }
        ctx.stack.push(child.clone());
        dfs_folded(child, ctx);
        ctx.stack.pop();
    }
    ctx.in_stack.remove(node);
}

fn render_ascii_flamegraph(folded: &str) -> String {
    let mut weights: HashMap<String, usize> = HashMap::new();
    for line in folded.lines() {
        let mut parts = line.rsplitn(2, ' ');
        let weight = parts
            .next()
            .and_then(|w| w.parse::<usize>().ok())
            .unwrap_or(1);
        let Some(stack) = parts.next() else {
            continue;
        };
        for frame in stack.split(';') {
            *weights.entry(frame.to_string()).or_insert(0) += weight;
        }
    }
    let mut items: Vec<(String, usize)> = weights.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let max = items.first().map(|(_, w)| *w).unwrap_or(1);

    let mut out = String::from("ASCII flamegraph (approx)\n");
    for (name, w) in items {
        let bars = ((w as f64 / max as f64) * 40.0).ceil() as usize;
        out.push_str(&format!("{:<40} | {} ({})\n", name, "#".repeat(bars), w));
    }
    out
}

fn speedscope_from_folded(folded: &str) -> Value {
    let mut frame_map: HashMap<String, usize> = HashMap::new();
    let mut frames: Vec<Value> = Vec::new();
    let mut events: Vec<Value> = Vec::new();
    let mut t: f64 = 0.0;

    for line in folded.lines() {
        let mut parts = line.rsplitn(2, ' ');
        let weight = parts
            .next()
            .and_then(|w| w.parse::<f64>().ok())
            .unwrap_or(1.0);
        let Some(stack) = parts.next() else {
            continue;
        };
        let frame_ids: Vec<usize> = stack
            .split(';')
            .map(|name| {
                if let Some(id) = frame_map.get(name) {
                    *id
                } else {
                    let id = frames.len();
                    frame_map.insert(name.to_string(), id);
                    frames.push(json!({"name": name}));
                    id
                }
            })
            .collect();

        for fid in &frame_ids {
            events.push(json!({"type": "O", "frame": fid, "at": t}));
        }
        t += weight;
        for fid in frame_ids.iter().rev() {
            events.push(json!({"type": "C", "frame": fid, "at": t}));
        }
    }

    json!({
        "$schema": "https://www.speedscope.app/file-format-schema.json",
        "shared": {
            "frames": frames
        },
        "profiles": [{
            "type": "evented",
            "name": "curd-profile",
            "unit": "none",
            "startValue": 0.0,
            "endValue": t,
            "events": events
        }],
        "activeProfileIndex": 0
    })
}

fn folded_frame_weights(folded: &str) -> HashMap<String, i64> {
    let mut out: HashMap<String, i64> = HashMap::new();
    for line in folded.lines() {
        let mut parts = line.rsplitn(2, ' ');
        let weight = parts
            .next()
            .and_then(|w| w.parse::<i64>().ok())
            .unwrap_or(1);
        let Some(stack) = parts.next() else {
            continue;
        };
        for frame in stack.split(';') {
            *out.entry(frame.to_string()).or_insert(0) += weight;
        }
    }
    out
}

fn looks_like_python_command(command: &str) -> bool {
    let c = command.trim_start();
    c.starts_with("python ") || c.starts_with("python3 ") || c.starts_with("py ")
}

use crate::shell::command_exists;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::parse_command;

    #[tokio::test]
    async fn test_parse_command_basic() {
        let (program, args) = parse_command("python -m pytest -q").expect("parse");
        assert_eq!(program, "python");
        assert_eq!(args, vec!["-m", "pytest", "-q"]);
    }

    #[tokio::test]
    async fn test_parse_command_quotes() {
        let (program, args) = parse_command("node -e \"console.log('x')\"").expect("parse");
        assert_eq!(program, "node");
        assert_eq!(args, vec!["-e", "console.log('x')"]);
    }

    #[test]
    fn test_parse_command_rejects_unclosed_quote() {
        assert!(parse_command("python -c \"print(1)").is_err());
    }

    #[test]
    fn test_folded_frame_weights() {
        let weights = folded_frame_weights("a;b 2\na;c 1\n");
        assert_eq!(*weights.get("a").unwrap_or(&0), 3);
        assert_eq!(*weights.get("b").unwrap_or(&0), 2);
        assert_eq!(*weights.get("c").unwrap_or(&0), 1);
    }

    #[test]
    fn test_build_folded_keeps_multiple_dag_paths() {
        let roots = vec!["a".to_string()];
        let edges = vec![
            ("a".to_string(), "b".to_string()),
            ("a".to_string(), "c".to_string()),
            ("b".to_string(), "d".to_string()),
            ("c".to_string(), "d".to_string()),
        ];
        let folded = build_folded(&roots, &edges);
        assert!(folded.contains("a;b;d 1"));
        assert!(folded.contains("a;c;d 1"));
    }
}
