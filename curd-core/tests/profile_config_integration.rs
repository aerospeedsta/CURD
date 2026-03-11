use curd_core::check_workspace_config;
use std::fs;
use tempfile::tempdir;

#[test]
fn settings_toml_accepts_multiple_profiles() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(
        root.join("settings.toml"),
        r#"
[runtime]
ceiling = "full"

[profiles.default]
role = "human_core"
capabilities = ["lookup", "traverse", "read", "change.apply", "session.begin", "session.verify", "session.commit"]
promotion = "user_only"

[profiles.assist]
role = "assist_agent"
capabilities = ["lookup", "traverse", "read", "change.prepare", "session.begin", "session.verify", "review.run"]
promotion = "forbidden"
"#,
    )
    .expect("write settings");

    assert!(check_workspace_config(root).is_ok());
}

#[test]
fn invalid_profile_capability_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(
        root.join("settings.toml"),
        r#"
[profiles.default]
role = "human_core"
capabilities = ["lookup", "delete_the_world"]
"#,
    )
    .expect("write settings");

    let err = check_workspace_config(root).expect_err("invalid capability should fail");
    assert!(
        err.iter()
            .any(|finding| finding.code == "config_profile_capability_invalid")
    );
}

#[test]
fn invalid_profile_promotion_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(
        root.join("settings.toml"),
        r#"
[profiles.default]
role = "human_core"
capabilities = ["lookup"]
promotion = "surprise_me"
"#,
    )
    .expect("write settings");

    let err = check_workspace_config(root).expect_err("invalid promotion should fail");
    assert!(
        err.iter()
            .any(|finding| finding.code == "config_profile_promotion_invalid")
    );
}

#[test]
fn invalid_runtime_ceiling_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(
        root.join("settings.toml"),
        r#"
[runtime]
ceiling = "godmode"
"#,
    )
    .expect("write settings");

    let err = check_workspace_config(root).expect_err("invalid ceiling should fail");
    assert!(
        err.iter()
            .any(|finding| finding.code == "config_runtime_ceiling_invalid")
    );
}
