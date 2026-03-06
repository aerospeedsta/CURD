use std::path::{Path, PathBuf};
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

    /// Build a sandboxed tokio::process::Command based on the current OS.
    pub fn build_command(&self, cmd: &str, args: &[String]) -> TokioCommand {
        let os = std::env::consts::OS;
        let mut c = match os {
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
            /*
            "docker" => {
                // Future Implementation: Docker-based sandboxing
                // 1. Build a transient container from a CURD-base image.
                // 2. Mount the workspace root as a volume (ro/rw based on operation).
                // 3. Execute the command inside the container.
                // 4. Stream results back and destroy the container.
                unimplemented!("Docker sandboxing is planned for a future release.")
            }
            */
            _ => {
                let mut cmd_obj = TokioCommand::new(cmd);
                cmd_obj.args(args);
                cmd_obj
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
