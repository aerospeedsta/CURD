use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn load_plan_artifact(path: &Path) -> Result<(curd_core::Plan, Option<serde_json::Value>)> {
    let content = fs::read_to_string(path)?;
    let raw: serde_json::Value = serde_json::from_str(&content)?;
    if let Some(plan_val) = raw.get("plan") {
        let plan: curd_core::Plan = serde_json::from_value(plan_val.clone())?;
        Ok((plan, Some(raw)))
    } else {
        let plan: curd_core::Plan = serde_json::from_value(raw)?;
        Ok((plan, None))
    }
}

pub fn save_plan_artifact(
    path: &Path,
    plan: &curd_core::Plan,
    artifact_meta: Option<serde_json::Value>,
) -> Result<()> {
    let content = if let Some(mut raw) = artifact_meta {
        if let Some(obj) = raw.as_object_mut() {
            obj.insert("plan".to_string(), serde_json::to_value(plan)?);
        }
        serde_json::to_string_pretty(&raw)?
    } else {
        serde_json::to_string_pretty(plan)?
    };
    fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_plan_artifact, save_plan_artifact};
    use curd_core::Plan;
    use curd_core::plan::{PlanNode, ToolOperation};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn load_and_save_plan_artifact_preserves_metadata() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("demo.json");

        let plan = Plan {
            id: Uuid::new_v4(),
            nodes: vec![PlanNode {
                id: Uuid::new_v4(),
                op: ToolOperation::McpCall {
                    tool: "search".to_string(),
                    args: json!({"query":"alpha","mode":"symbol"}),
                },
                dependencies: vec![],
                output_limit: 1024,
                retry_limit: 0,
            }],
        };
        let artifact = json!({
            "format": "curd_plan_artifact/v1",
            "source_kind": "curd_script",
            "metadata": {
                "profile": "supervised",
                "source_path": "demo.curd"
            },
            "explainability": {
                "summary": "demo"
            },
            "plan": plan
        });

        fs::write(&path, serde_json::to_string_pretty(&artifact)?)?;

        let (mut loaded, meta) = load_plan_artifact(&path)?;
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(
            meta.as_ref()
                .and_then(|m| m.get("metadata"))
                .and_then(|m| m.get("profile"))
                .and_then(|v| v.as_str()),
            Some("supervised")
        );

        loaded.nodes[0].output_limit = 2048;
        save_plan_artifact(&path, &loaded, meta)?;

        let persisted: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
        assert_eq!(persisted["metadata"]["profile"], "supervised");
        assert_eq!(persisted["plan"]["nodes"][0]["output_limit"], 2048);
        Ok(())
    }

    #[test]
    fn load_plain_plan_without_artifact_wrapper() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("plain.json");
        let plan = Plan {
            id: Uuid::new_v4(),
            nodes: vec![],
        };
        fs::write(&path, serde_json::to_string_pretty(&plan)?)?;

        let (loaded, meta) = load_plan_artifact(&path)?;
        assert_eq!(loaded.id, plan.id);
        assert!(meta.is_none());
        Ok(())
    }
}
