use curd_core::validate_workspace_config;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_core_mcp_startup_rejection() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();

    // Create an invalid config: max_file_size = 0 which is high severity
    let bad_config = r#"
[index]
max_file_size = 0
"#;
    fs::write(root.join("curd.toml"), bad_config)?;

    // This should fail
    let result = validate_workspace_config(root);
    assert!(
        result.is_err(),
        "MCP startup/validation should reject invalid config"
    );

    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("high severity"),
        "Error should mention high severity findings: {}",
        err_msg
    );

    Ok(())
}
