use serde_json::{Value, json};
use std::fs;
use std::path::Path;

/// Represents a single dependency extracted from a package manifest
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub kind: String, // "runtime", "dev", "build", "optional"
}

/// Represents parsed manifest information for a workspace
#[derive(Debug)]
pub struct ManifestInfo {
    pub manager: String,
    pub manifest_file: String,
    pub dependencies: Vec<Dependency>,
    pub excluded_dirs: Vec<String>,
}

/// Detects and parses package manager manifests at the workspace root
pub fn detect_dependencies(workspace_root: &Path) -> Option<ManifestInfo> {
    // Try each manifest format in priority order
    if let Some(info) = try_cargo(workspace_root) {
        return Some(info);
    }
    if let Some(info) = try_pixi(workspace_root) {
        return Some(info);
    }
    if let Some(info) = try_pyproject(workspace_root) {
        return Some(info);
    }
    if let Some(info) = try_package_json(workspace_root) {
        return Some(info);
    }
    if let Some(info) = try_chromium(workspace_root) {
        return Some(info);
    }
    if let Some(info) = try_generic(workspace_root) {
        return Some(info);
    }
    None
}

/// Convert a ManifestInfo into a JSON value for the agent
pub fn dependencies_to_json(info: &ManifestInfo) -> Value {
    let deps: Vec<Value> = info
        .dependencies
        .iter()
        .map(|d| {
            json!({
                "name": d.name,
                "version": d.version,
                "kind": d.kind,
            })
        })
        .collect();

    json!({
        "status": "ok",
        "manager": info.manager,
        "manifest": info.manifest_file,
        "dependencies": deps,
        "dependency_count": deps.len(),
        "excluded_dirs": info.excluded_dirs,
    })
}

/// Returns the list of directories to exclude based on detected manifests
pub fn get_excluded_dirs(workspace_root: &Path) -> Vec<String> {
    // Always exclude these well-known artifact dirs
    let mut excluded = vec![
        "__pycache__".to_string(),
        ".mypy_cache".to_string(),
        ".tox".to_string(),
        ".curd/grammars".to_string(),
    ];

    if let Some(info) = detect_dependencies(workspace_root) {
        excluded.extend(info.excluded_dirs);
    }

    excluded
}

// ── Cargo (Rust) ───────────────────────────────────────────────────────

fn try_cargo(root: &Path) -> Option<ManifestInfo> {
    let manifest_path = root.join("Cargo.toml");
    if let Ok(meta) = fs::metadata(&manifest_path)
        && meta.len() > 10 * 1024 * 1024
    {
        return None;
    }
    let content = fs::read_to_string(&manifest_path).ok()?;
    let doc: toml::Value = content.parse().ok()?;

    let mut deps = Vec::new();

    // [dependencies]
    if let Some(table) = doc.get("dependencies").and_then(|v| v.as_table()) {
        for (name, val) in table {
            let version = extract_toml_version(val);
            deps.push(Dependency {
                name: name.clone(),
                version,
                kind: "runtime".to_string(),
            });
        }
    }

    // [dev-dependencies]
    if let Some(table) = doc.get("dev-dependencies").and_then(|v| v.as_table()) {
        for (name, val) in table {
            let version = extract_toml_version(val);
            deps.push(Dependency {
                name: name.clone(),
                version,
                kind: "dev".to_string(),
            });
        }
    }

    // [build-dependencies]
    if let Some(table) = doc.get("build-dependencies").and_then(|v| v.as_table()) {
        for (name, val) in table {
            let version = extract_toml_version(val);
            deps.push(Dependency {
                name: name.clone(),
                version,
                kind: "build".to_string(),
            });
        }
    }

    Some(ManifestInfo {
        manager: "cargo".to_string(),
        manifest_file: "Cargo.toml".to_string(),
        dependencies: deps,
        excluded_dirs: vec!["target".to_string()],
    })
}

// ── pixi ────────────────────────────────────────────────────────────────

fn try_pixi(root: &Path) -> Option<ManifestInfo> {
    let manifest_path = root.join("pixi.toml");
    if let Ok(meta) = fs::metadata(&manifest_path)
        && meta.len() > 10 * 1024 * 1024
    {
        return None;
    }
    let content = fs::read_to_string(&manifest_path).ok()?;
    let doc: toml::Value = content.parse().ok()?;

    let mut deps = Vec::new();

    if let Some(table) = doc.get("dependencies").and_then(|v| v.as_table()) {
        for (name, val) in table {
            let version = extract_toml_version(val);
            deps.push(Dependency {
                name: name.clone(),
                version,
                kind: "runtime".to_string(),
            });
        }
    }

    // pixi can have feature-specific deps under [feature.*.dependencies]
    if let Some(features) = doc.get("feature").and_then(|v| v.as_table()) {
        for (_feat_name, feat_val) in features {
            if let Some(table) = feat_val.get("dependencies").and_then(|v| v.as_table()) {
                for (name, val) in table {
                    let version = extract_toml_version(val);
                    deps.push(Dependency {
                        name: name.clone(),
                        version,
                        kind: "runtime".to_string(),
                    });
                }
            }
        }
    }

    Some(ManifestInfo {
        manager: "pixi".to_string(),
        manifest_file: "pixi.toml".to_string(),
        dependencies: deps,
        excluded_dirs: vec![".pixi".to_string()],
    })
}

// ── pyproject.toml (uv / pip / poetry) ──────────────────────────────────

fn try_pyproject(root: &Path) -> Option<ManifestInfo> {
    let manifest_path = root.join("pyproject.toml");
    if let Ok(meta) = fs::metadata(&manifest_path)
        && meta.len() > 10 * 1024 * 1024
    {
        return None;
    }
    let content = fs::read_to_string(&manifest_path).ok()?;
    let doc: toml::Value = content.parse().ok()?;

    let mut deps = Vec::new();
    let mut manager = "pip".to_string();

    // PEP 621 style: [project.dependencies]
    if let Some(arr) = doc
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for item in arr {
            if let Some(spec) = item.as_str() {
                let (name, version) = parse_pep_dep(spec);
                deps.push(Dependency {
                    name,
                    version,
                    kind: "runtime".to_string(),
                });
            }
        }
    }

    // [project.optional-dependencies]
    if let Some(table) = doc
        .get("project")
        .and_then(|p| p.get("optional-dependencies"))
        .and_then(|d| d.as_table())
    {
        for (_group, items) in table {
            if let Some(arr) = items.as_array() {
                for item in arr {
                    if let Some(spec) = item.as_str() {
                        let (name, version) = parse_pep_dep(spec);
                        deps.push(Dependency {
                            name,
                            version,
                            kind: "optional".to_string(),
                        });
                    }
                }
            }
        }
    }

    // uv-specific: [tool.uv.dev-dependencies]
    if let Some(arr) = doc
        .get("tool")
        .and_then(|t| t.get("uv"))
        .and_then(|u| u.get("dev-dependencies"))
        .and_then(|d| d.as_array())
    {
        manager = "uv".to_string();
        for item in arr {
            if let Some(spec) = item.as_str() {
                let (name, version) = parse_pep_dep(spec);
                deps.push(Dependency {
                    name,
                    version,
                    kind: "dev".to_string(),
                });
            }
        }
    }

    // poetry-specific: [tool.poetry.dependencies]
    if let Some(table) = doc
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        manager = "poetry".to_string();
        for (name, val) in table {
            if name == "python" {
                continue;
            }
            let version = extract_toml_version(val);
            deps.push(Dependency {
                name: name.clone(),
                version,
                kind: "runtime".to_string(),
            });
        }
    }

    if deps.is_empty() {
        return None;
    }

    Some(ManifestInfo {
        manager,
        manifest_file: "pyproject.toml".to_string(),
        dependencies: deps,
        excluded_dirs: vec![
            ".venv".to_string(),
            "venv".to_string(),
            "__pycache__".to_string(),
            "dist".to_string(),
            ".tox".to_string(),
        ],
    })
}

// ── package.json (npm / bun / yarn / pnpm) ──────────────────────────────

fn try_package_json(root: &Path) -> Option<ManifestInfo> {
    let manifest_path = root.join("package.json");
    if let Ok(meta) = fs::metadata(&manifest_path)
        && meta.len() > 10 * 1024 * 1024
    {
        return None;
    }
    let content = fs::read_to_string(&manifest_path).ok()?;
    let doc: Value = serde_json::from_str(&content).ok()?;

    let mut deps = Vec::new();

    if let Some(obj) = doc.get("dependencies").and_then(|d| d.as_object()) {
        for (name, val) in obj {
            let version = val.as_str().unwrap_or("*").to_string();
            deps.push(Dependency {
                name: name.clone(),
                version,
                kind: "runtime".to_string(),
            });
        }
    }

    if let Some(obj) = doc.get("devDependencies").and_then(|d| d.as_object()) {
        for (name, val) in obj {
            let version = val.as_str().unwrap_or("*").to_string();
            deps.push(Dependency {
                name: name.clone(),
                version,
                kind: "dev".to_string(),
            });
        }
    }

    if let Some(obj) = doc.get("peerDependencies").and_then(|d| d.as_object()) {
        for (name, val) in obj {
            let version = val.as_str().unwrap_or("*").to_string();
            deps.push(Dependency {
                name: name.clone(),
                version,
                kind: "peer".to_string(),
            });
        }
    }

    // Detect which manager is actually in use
    let manager = if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
        "bun"
    } else if root.join("yarn.lock").exists() {
        "yarn"
    } else if root.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else {
        "npm"
    };

    Some(ManifestInfo {
        manager: manager.to_string(),
        manifest_file: "package.json".to_string(),
        dependencies: deps,
        excluded_dirs: vec![
            "node_modules".to_string(),
            "dist".to_string(),
            ".next".to_string(),
            "build".to_string(),
        ],
    })
}

// ── Chromium (DEPS / GN) ───────────────────────────────────────────────

fn try_chromium(root: &Path) -> Option<ManifestInfo> {
    let deps_path = root.join("DEPS");
    let gn_path = root.join(".gn");

    if !deps_path.exists() && !gn_path.exists() {
        return None;
    }

    let mut dependencies = Vec::new();

    if deps_path.exists()
        && let Ok(content) = fs::read_to_string(&deps_path)
    {
        // Very basic extraction of "external" deps from DEPS
        for line in content.lines() {
            let line = line.trim();
            if (line.starts_with('\'') || line.starts_with('"'))
                && (line.contains("https://") || line.contains("git://"))
            {
                let parts: Vec<&str> = line.split(&['\'', '"'][..]).collect();
                if parts.len() >= 2 {
                    dependencies.push(Dependency {
                        name: parts[1].to_string(),
                        version: "remote".to_string(),
                        kind: "runtime".to_string(),
                    });
                }
            }
        }
    }

    Some(ManifestInfo {
        manager: "chromium".to_string(),
        manifest_file: if deps_path.exists() {
            "DEPS".to_string()
        } else {
            ".gn".to_string()
        },
        dependencies,
        excluded_dirs: vec![
            "out".to_string(),
            "build/linux".to_string(),
            "build/mac".to_string(),
        ],
    })
}

// ── Generic (CMake / Makefile) ────────────────────────────────────────

fn try_generic(root: &Path) -> Option<ManifestInfo> {
    let cmake = root.join("CMakeLists.txt");
    let makefile = root.join("Makefile");
    let go_mod = root.join("go.mod");

    if cmake.exists() {
        return Some(ManifestInfo {
            manager: "cmake".to_string(),
            manifest_file: "CMakeLists.txt".to_string(),
            dependencies: Vec::new(),
            excluded_dirs: vec![
                "build".to_string(),
                "out".to_string(),
                "CMakeFiles".to_string(),
            ],
        });
    }

    if makefile.exists() {
        return Some(ManifestInfo {
            manager: "make".to_string(),
            manifest_file: "Makefile".to_string(),
            dependencies: Vec::new(),
            excluded_dirs: vec!["build".to_string(), "out".to_string(), ".obj".to_string()],
        });
    }

    if go_mod.exists() {
        return Some(ManifestInfo {
            manager: "go".to_string(),
            manifest_file: "go.mod".to_string(),
            dependencies: Vec::new(),
            excluded_dirs: vec!["vendor".to_string(), "bin".to_string()],
        });
    }

    None
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn extract_toml_version(val: &toml::Value) -> String {
    match val {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => t
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("*")
            .to_string(),
        _ => "*".to_string(),
    }
}

/// Parse a PEP 508 dependency specifier like "requests>=2.0" into (name, version)
fn parse_pep_dep(spec: &str) -> (String, String) {
    // Split on first version operator
    let operators = [">=", "<=", "!=", "==", "~=", ">", "<"];
    for op in operators {
        if let Some(idx) = spec.find(op) {
            let name = spec[..idx].trim().to_string();
            let version = spec[idx..].trim().to_string();
            return (name, version);
        }
    }
    // Extras like "requests[security]"
    if let Some(idx) = spec.find('[') {
        return (spec[..idx].trim().to_string(), "*".to_string());
    }
    (spec.trim().to_string(), "*".to_string())
}
