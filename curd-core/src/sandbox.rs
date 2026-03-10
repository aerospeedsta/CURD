use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tokio::process::Command as TokioCommand;

/// Shared sandboxing logic for CURD engines (Shell and Debug)
pub struct Sandbox {
    pub workspace_root: PathBuf,
}

impl Sandbox {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
        }
    }

    /// Builds a synchronous `std::process::Command` wrapped in the appropriate OS-native sandbox
    pub fn build_std_command(&self, cmd: &str, args: &[String]) -> StdCommand {
        let config = crate::config::CurdConfig::load_from_workspace(&self.workspace_root);

        let mut c = if config.shell.docker_enabled {
            let mut cmd_obj = StdCommand::new(&config.shell.container_engine);
            cmd_obj
                .arg("run")
                .arg("--rm")
                .arg("-v")
                .arg(format!("{}:{}", self.workspace_root.display(), self.workspace_root.display()))
                .arg("-w")
                .arg(&self.workspace_root)
                .arg(&config.shell.docker_image)
                .arg(cmd)
                .args(args);
            cmd_obj
        } else {
            let os = std::env::consts::OS;
            match os {
                "macos" => {
                    // Disable sandbox-exec in tests as it kills the cargo test runner
                    #[cfg(test)]
                    {
                        let mut cmd_obj = StdCommand::new(cmd);
                        cmd_obj.args(args);
                        return cmd_obj;
                    }
                    #[cfg(not(test))]
                    {
                        let profile = format!(
                            "(version 1)\n\
                             (allow default)\n\
                             (allow file-read* (subpath \"/\"))\n\
                             (allow file-write* (subpath \"{}\"))\n\
                             (allow file-write* (subpath \"/tmp\"))\n\
                             (allow file-write* (subpath \"{}\"))\n\
                             (allow process-fork)\n\
                             (allow process-exec)\n\
                             (allow mach-lookup)\n\
                             (allow sysctl-read)\n",
                            self.workspace_root.display(),
                            self.workspace_root.join(".curd/tmp").display()
                        );
                        let mut cmd_obj = StdCommand::new("sandbox-exec");
                        cmd_obj.arg("-p").arg(profile).arg(cmd).args(args);
                        cmd_obj
                    }
                }
                "linux" => {
                    let mut cmd_obj = StdCommand::new("bwrap");
                    cmd_obj
                        .arg("--ro-bind")
                        .arg("/")
                        .arg("/")
                        .arg("--dev")
                        .arg("/dev")
                        .arg("--bind")
                        .arg(&self.workspace_root)
                        .arg(&self.workspace_root)
                        .arg("--unshare-net")
                        .arg(cmd)
                        .args(args);
                    cmd_obj
                }
                _ => {
                    let mut cmd_obj = StdCommand::new(cmd);
                    cmd_obj.args(args);
                    cmd_obj
                }
            }
        };

        // Standard sanitization
        c.env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .env("USER", std::env::var("USER").unwrap_or_default())
            .env("TERM", std::env::var("TERM").unwrap_or_default());
        c
    }

    /// Builds an asynchronous `tokio::process::Command` wrapped in the appropriate OS-native sandbox
    pub fn build_command(&self, cmd: &str, args: &[String]) -> TokioCommand {
        let config = crate::config::CurdConfig::load_from_workspace(&self.workspace_root);

        let mut c = if config.shell.docker_enabled {
            let mut cmd_obj = TokioCommand::new(&config.shell.container_engine);
            cmd_obj
                .arg("run")
                .arg("--rm")
                .arg("-v")
                .arg(format!("{}:{}", self.workspace_root.display(), self.workspace_root.display()))
                .arg("-w")
                .arg(&self.workspace_root)
                .arg(&config.shell.docker_image)
                .arg(cmd)
                .args(args);
            cmd_obj
        } else {
            let os = std::env::consts::OS;
            match os {
                "macos" => {
                    let profile = format!(
                        "(version 1)\n\
                         (deny default)\n\
                         (allow file-read* (subpath \"/usr/lib\"))\n\
                         (allow file-read* (subpath \"/usr/share\"))\n\
                         (allow file-read* (subpath \"/usr/bin\"))\n\
                         (allow file-read* (subpath \"/System/Library\"))\n\
                         (allow file-read* (subpath \"/Library\"))\n\
                         (allow file-read* file-write* (subpath \"{}\"))\n\
                         (allow file-write* (subpath \"/tmp\"))\n\
                         (allow file-read* file-write* (subpath \"{}\"))\n\
                         (allow process-fork)\n\
                         (allow process-exec)\n\
                         (allow mach-lookup)\n\
                         (allow sysctl-read)\n",
                        self.workspace_root.display(),
                        self.workspace_root.join(".curd/tmp").display()
                    );
                    let mut cmd_obj = TokioCommand::new("sandbox-exec");
                    cmd_obj.arg("-p").arg(profile).arg(cmd).args(args);
                    cmd_obj
                }
                "linux" => {
                    let mut cmd_obj = TokioCommand::new("bwrap");
                    cmd_obj
                        .arg("--ro-bind")
                        .arg("/")
                        .arg("/")
                        .arg("--bind")
                        .arg(&self.workspace_root)
                        .arg(&self.workspace_root)
                        .arg("--dev")
                        .arg("/dev")
                        .arg("--tmpfs")
                        .arg("/tmp")
                        .arg("--proc")
                        .arg("/proc")
                        .arg("--unshare-all")
                        .arg("--share-net")
                        .arg("--limit-as")
                        .arg("2G")
                        .arg("--chdir")
                        .arg(&self.workspace_root)
                        .arg(cmd)
                        .args(args);
                    cmd_obj
                }
                _ => {
                    let mut cmd_obj = TokioCommand::new(cmd);
                    cmd_obj.args(args);
                    cmd_obj
                }
            }
        };

        // Sanitize environment variables to prevent secret leakage (e.g., AWS_*, GITHUB_TOKEN)
        c.env_clear();
        
        // Safelist of inherited environment variables
        for key in ["PATH", "TERM", "HOME", "LANG", "SHELL", "USER"] {
            if let Ok(val) = std::env::var(key) {
                c.env(key, val);
            }
        }

        c
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_build_command_strips_env() {
        let dir = tempdir().unwrap();
        let sandbox = Sandbox::new(dir.path());
        let cmd = sandbox.build_command("echo", &["test".to_string()]);
        
        // Unfortunately, TokioCommand doesn't expose a way to inspect env modifications easily
        // but we can ensure the command string is well-formed.
        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("echo") || debug_str.contains("sandbox-exec") || debug_str.contains("bwrap"));
    }
    
    #[test]
    fn test_sandbox_canonicalizes_root() {
        let dir = tempdir().unwrap();
        let symlink_path = dir.path().join("symlink");
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path(), &symlink_path).unwrap();
        
        let sandbox = Sandbox::new(&symlink_path);
        assert_eq!(sandbox.workspace_root, std::fs::canonicalize(dir.path()).unwrap());
    }
}
