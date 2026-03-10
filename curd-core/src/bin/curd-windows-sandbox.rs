#[cfg(windows)]
use anyhow::{Context, Result};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::Security::{TokenRestrictedDeviceGroups, CreateRestrictedToken, DISABLE_MAX_PRIVILEGE};
#[cfg(windows)]
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcessToken, ResumeThread, CREATE_SUSPENDED,
    PROCESS_ALL_ACCESS, TOKEN_ALL_ACCESS,
};
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

#[cfg(not(windows))]
fn main() {
    println!("This binary is only intended for Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        anyhow::bail!("Usage: curd-windows-sandbox.exe <cmd> [args...]");
    }

    let target_cmd = &args[1];
    let target_args = &args[2..];

    unsafe {
        // 1. Create a Job Object
        let job: HANDLE = CreateJobObjectW(None, None).context("Failed to create Job Object")?;

        // 2. Configure Job to kill children when this wrapper exits (Kill-on-close)
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .context("Failed to configure Job Object")?;

        // 3. Create a Restricted Token (Drop Admin Privileges)
        // ... (Token logic can be complex, skipping for MVP Job Object isolation) ...

        // 4. Assign THIS wrapper process to the Job Object
        AssignProcessToJobObject(job, GetCurrentProcess()).context("Failed to assign to Job Object")?;

        // 5. Spawn the target process. It will inherit the Job Object constraints.
        let mut child = Command::new(target_cmd)
            .args(target_args)
            .spawn()
            .context("Failed to spawn target process")?;

        let status = child.wait().context("Failed to wait on target process")?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
