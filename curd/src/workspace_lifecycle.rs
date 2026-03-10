use anyhow::Result;
use clap::ValueEnum;
use curd_core::ShadowStore;
use dialoguer::Select;
use std::path::Path;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ShadowDisposition {
    Apply,
    Discard,
    Abort,
}

#[derive(Debug, Clone)]
pub struct ActiveTransactionSummary {
    pub transaction_id: Option<Uuid>,
    pub staged_count: usize,
    pub diff_preview: String,
}

#[derive(Debug, Clone)]
pub struct WorkspaceExitOutcome {
    pub proceeded: bool,
    pub summary: Option<ActiveTransactionSummary>,
    pub message: String,
}

pub fn inspect_active_transaction(workspace_root: &Path) -> Result<Option<ActiveTransactionSummary>> {
    let mut shadow = ShadowStore::new(workspace_root);
    if !shadow.is_active() {
        return Ok(None);
    }
    let diff = shadow.diff();
    let preview_lines: Vec<&str> = diff.lines().take(40).collect();
    let diff_preview = preview_lines.join("\n");
    Ok(Some(ActiveTransactionSummary {
        transaction_id: shadow.get_transaction_id(),
        staged_count: shadow.len(),
        diff_preview,
    }))
}

pub fn resolve_workspace_exit(
    workspace_root: &Path,
    destructive_action: &str,
    requested: Option<ShadowDisposition>,
    require_explicit_when_noninteractive: bool,
) -> Result<WorkspaceExitOutcome> {
    let mut shadow = ShadowStore::new(workspace_root);
    if !shadow.is_active() {
        return Ok(WorkspaceExitOutcome {
            proceeded: true,
            summary: None,
            message: format!("No active transaction detected before {destructive_action}."),
        });
    }

    let summary = ActiveTransactionSummary {
        transaction_id: shadow.get_transaction_id(),
        staged_count: {
            let _ = shadow.diff();
            shadow.len()
        },
        diff_preview: shadow.diff().lines().take(40).collect::<Vec<_>>().join("\n"),
    };

    let disposition = match requested {
        Some(choice) => choice,
        None if require_explicit_when_noninteractive => {
            return Ok(WorkspaceExitOutcome {
                proceeded: false,
                summary: Some(summary),
                message: format!(
                    "Active transaction detected. Re-run {destructive_action} with --shadow apply|discard|abort."
                ),
            });
        }
        None => prompt_shadow_disposition(&summary, destructive_action)?,
    };

    match disposition {
        ShadowDisposition::Apply => {
            let written = shadow.commit()?;
            Ok(WorkspaceExitOutcome {
                proceeded: true,
                summary: Some(summary),
                message: format!(
                    "Applied {} shadow change(s) to the workspace before {}.",
                    written.len(),
                    destructive_action
                ),
            })
        }
        ShadowDisposition::Discard => {
            shadow.rollback();
            Ok(WorkspaceExitOutcome {
                proceeded: true,
                summary: Some(summary),
                message: format!(
                    "Discarded active shadow transaction before {}. Workspace files were left unchanged.",
                    destructive_action
                ),
            })
        }
        ShadowDisposition::Abort => Ok(WorkspaceExitOutcome {
            proceeded: false,
            summary: Some(summary),
            message: format!("Aborted {} because an active transaction is still present.", destructive_action),
        }),
    }
}

fn prompt_shadow_disposition(
    summary: &ActiveTransactionSummary,
    destructive_action: &str,
) -> Result<ShadowDisposition> {
    println!(
        "Active transaction detected before {}.\n  transaction_id: {}\n  staged_files: {}",
        destructive_action,
        summary
            .transaction_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        summary.staged_count
    );
    if !summary.diff_preview.trim().is_empty() {
        println!("  diff preview:\n{}\n", indent_preview(&summary.diff_preview));
    }

    let items = [
        "Apply shadow changes into the workspace, then continue",
        "Discard the shadow transaction and keep the current workspace as-is",
        "Abort and do nothing",
    ];
    let selection = Select::new()
        .with_prompt("How should CURD handle the active transaction?")
        .items(&items)
        .default(2)
        .interact()?;

    let choice = match selection {
        0 => ShadowDisposition::Apply,
        1 => ShadowDisposition::Discard,
        _ => ShadowDisposition::Abort,
    };
    Ok(choice)
}

fn indent_preview(preview: &str) -> String {
    preview
        .lines()
        .map(|line| format!("    {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn cleanup_detach_artifacts(workspace_root: &Path) {
    let hook_path = workspace_root.join(".git/hooks/pre-push");
    if hook_path.exists()
        && let Ok(content) = std::fs::read_to_string(&hook_path)
        && content.contains("curd detach")
    {
        let _ = std::fs::remove_file(&hook_path);
        println!("Removed CURD git pre-push hook.");
    }

    crate::workspace_init::cleanup_agent_configs(workspace_root);
}

#[cfg(test)]
mod tests {
    use super::{ShadowDisposition, resolve_workspace_exit};
    use curd_core::ShadowStore;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn apply_shadow_before_exit_writes_to_workspace() {
        let dir = tempdir().expect("tempdir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        fs::write(root.join("a.txt"), "old\n").expect("write");

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().expect("begin");
        shadow.stage(&root.join("a.txt"), "new\n").expect("stage");

        let outcome = resolve_workspace_exit(&root, "detach", Some(ShadowDisposition::Apply), false)
            .expect("resolve");
        assert!(outcome.proceeded);
        assert_eq!(fs::read_to_string(root.join("a.txt")).expect("read"), "new\n");
    }

    #[test]
    fn discard_shadow_before_exit_leaves_workspace_unchanged() {
        let dir = tempdir().expect("tempdir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        fs::write(root.join("a.txt"), "old\n").expect("write");

        let mut shadow = ShadowStore::new(&root);
        shadow.begin().expect("begin");
        shadow.stage(&root.join("a.txt"), "new\n").expect("stage");

        let outcome =
            resolve_workspace_exit(&root, "delete", Some(ShadowDisposition::Discard), false)
                .expect("resolve");
        assert!(outcome.proceeded);
        assert_eq!(fs::read_to_string(root.join("a.txt")).expect("read"), "old\n");
    }
}
