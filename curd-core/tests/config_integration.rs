use curd_core::check_workspace_config;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_config_rejection_smoke() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // 1. Valid config
    fs::write(
        root.join("settings.toml"),
        r#"
[index]
mode = "fast"
[storage]
enabled = true
"#,
    )
    .expect("write settings");
    assert!(
        check_workspace_config(root).is_ok(),
        "Valid config should be accepted"
    );

    // 2. High severity rejection (path escape)
    fs::write(
        root.join("settings.toml"),
        r#"
[storage]
sqlite_path = "../escaped.db"
"#,
    )
    .expect("write settings");
    let res = check_workspace_config(root);
    assert!(res.is_err(), "Unsafe storage path should be rejected");
    let findings = res.err().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f.code == "config_storage_sqlite_path_invalid")
    );
    assert_eq!(findings[0].severity, "high");

    // 3. Multiple high severity rejections
    fs::write(
        root.join("settings.toml"),
        r#"
[index]
mode = "ultra-slow"
[build.adapters.oops]
cwd = "/etc/passwd"
steps = []
"#,
    )
    .expect("write settings");
    let res = check_workspace_config(root);
    assert!(res.is_err());
    let findings = res.err().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f.code == "config_index_mode_invalid")
    );
    assert!(
        findings
            .iter()
            .any(|f| f.code == "config_build_adapter_cwd_invalid")
    );
}

#[test]
fn test_config_precedence_smoke() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // settings.toml (high precedence) is invalid
    fs::write(
        root.join("settings.toml"),
        r#"
[index]
mode = "invalid"
"#,
    )
    .expect("write settings");

    // curd.toml (lower precedence) is valid
    fs::write(
        root.join("curd.toml"),
        r#"
[index]
mode = "fast"
"#,
    )
    .expect("write curd.toml");

    let res = check_workspace_config(root);
    assert!(
        res.is_err(),
        "settings.toml should take precedence even if invalid"
    );
}
