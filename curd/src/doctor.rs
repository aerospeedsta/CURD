use anyhow::Result;
use curd_core::{DoctorEngine, DoctorIndexConfig, DoctorProfile, DoctorThresholds};
use std::path::Path;

pub fn run_doctor(
    root: &Path,
    strict: bool,
    thresholds: DoctorThresholds,
    _profile: Option<DoctorProfile>,
    cfg: DoctorIndexConfig,
) -> Result<()> {
    let engine = DoctorEngine::new(root);
    let report = engine.run(strict, thresholds, _profile, cfg)?;

    println!("{}", report.human_summary);

    if let Some(path) = report.index_config.report_out.as_ref() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("REPORT written to {}", path.display());
    }

    Ok(())
}
