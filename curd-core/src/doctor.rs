use crate::{
    CurdConfig, IndexBuildStats, SearchEngine, Symbol, build_index_coverage, build_index_quality,
    scan_workspace,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DoctorThresholds {
    pub max_total_ms: Option<u64>,
    pub max_parse_fail: Option<usize>,
    pub max_no_symbols_ratio: Option<f64>,
    pub max_skipped_large_ratio: Option<f64>,
    pub min_coverage_ratio: Option<f64>,
    pub require_coverage_state: Option<String>,
    pub min_symbol_count: Option<usize>,
    pub min_symbols_per_k_files: Option<f64>,
    pub min_overlap_with_full: Option<f64>,
    pub parity_rerun: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DoctorIndexConfig {
    pub index_mode: Option<String>,
    pub index_scope: Option<String>,
    pub index_max_file_size: Option<u64>,
    pub index_large_file_policy: Option<String>,
    pub index_execution: Option<String>,
    pub index_chunk_size: Option<usize>,
    pub compare_with_full: bool,
    pub profile_index: bool,
    pub report_out: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DoctorReport {
    pub status: String,
    pub workspace_root: PathBuf,
    pub scan: ScanStats,
    pub index_probe: IndexProbeStats,
    pub coverage: Value,
    pub quality: Value,
    pub symbol_inventory: SymbolInventoryStats,
    pub mode_comparison: ModeComparisonStats,
    pub parity: ParityStats,
    pub index_config: DoctorIndexConfig,
    pub findings: Vec<DoctorFinding>,
    pub human_summary: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanStats {
    pub files_found: usize,
    pub scan_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IndexProbeStats {
    pub wall_ms: u64,
    pub stats: Option<IndexBuildStats>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SymbolInventoryStats {
    pub computed: bool,
    pub symbol_count: Option<usize>,
    pub symbols_per_k_files: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModeComparisonStats {
    pub enabled: bool,
    pub overlap_with_full: Option<f64>,
    pub current_symbol_count: Option<usize>,
    pub full_symbol_count: Option<usize>,
    pub overlap_symbol_count: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ParityStats {
    pub enabled: bool,
    pub ok: Option<bool>,
    pub symbol_count_run1: Option<usize>,
    pub symbol_count_run2: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DoctorFinding {
    pub severity: String,
    pub code: String,
    pub message: String,
}

pub struct DoctorEngine {
    pub workspace_root: PathBuf,
}

impl DoctorEngine {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: root.as_ref().to_path_buf(),
        }
    }

    pub fn run(
        &self,
        strict: bool,
        mut thresholds: DoctorThresholds,
        profile: Option<DoctorProfile>,
        mut cfg: DoctorIndexConfig,
    ) -> anyhow::Result<DoctorReport> {
        let workspace_root = std::fs::canonicalize(&self.workspace_root)
            .unwrap_or_else(|_| self.workspace_root.clone());
        let workspace_cfg = CurdConfig::load_from_workspace(&workspace_root);

        let effective_profile = profile.or_else(|| {
            workspace_cfg
                .doctor
                .profile
                .as_deref()
                .and_then(|p| DoctorProfile::from_str(p).ok())
        });

        if let Some(p) = effective_profile {
            apply_profile_defaults(&mut thresholds, p);
        }
        let config_findings = workspace_cfg.validate();

        // Merge thresholds from config if not provided
        if thresholds.max_total_ms.is_none() {
            thresholds.max_total_ms = workspace_cfg.doctor.max_total_ms;
        }
        if thresholds.max_parse_fail.is_none() {
            thresholds.max_parse_fail = workspace_cfg.doctor.max_parse_fail;
        }
        if thresholds.max_no_symbols_ratio.is_none() {
            thresholds.max_no_symbols_ratio = workspace_cfg.doctor.max_no_symbols_ratio;
        }
        if thresholds.max_skipped_large_ratio.is_none() {
            thresholds.max_skipped_large_ratio = workspace_cfg.doctor.max_skipped_large_ratio;
        }
        if thresholds.min_coverage_ratio.is_none() {
            thresholds.min_coverage_ratio = workspace_cfg.doctor.min_coverage_ratio;
        }
        if thresholds.require_coverage_state.is_none() {
            thresholds.require_coverage_state = workspace_cfg.doctor.require_coverage_state.clone();
        }
        if thresholds.min_symbol_count.is_none() {
            thresholds.min_symbol_count = workspace_cfg.doctor.min_symbol_count;
        }
        if thresholds.min_symbols_per_k_files.is_none() {
            thresholds.min_symbols_per_k_files = workspace_cfg.doctor.min_symbols_per_k_files;
        }
        if thresholds.min_overlap_with_full.is_none() {
            thresholds.min_overlap_with_full = workspace_cfg.doctor.min_overlap_with_full;
        }
        if !thresholds.parity_rerun {
            thresholds.parity_rerun = workspace_cfg.doctor.parity_rerun.unwrap_or(false);
        }

        // Merge index config from workspace if not provided
        if cfg.index_mode.is_none() {
            cfg.index_mode = workspace_cfg.index.mode.clone();
        }
        if cfg.index_max_file_size.is_none() {
            cfg.index_max_file_size = workspace_cfg.index.max_file_size;
        }
        if cfg.index_large_file_policy.is_none() {
            cfg.index_large_file_policy = workspace_cfg.index.large_file_policy.clone();
        }
        if cfg.index_execution.is_none() {
            cfg.index_execution = workspace_cfg.index.execution.clone();
        }
        if cfg.index_chunk_size.is_none() {
            cfg.index_chunk_size = workspace_cfg.index.chunk_size;
        }

        // Build custom CurdConfig for the SearchEngine
        let mut search_cfg = workspace_cfg.clone();
        if let Some(mode) = cfg.index_mode.as_ref() {
            search_cfg.index.mode = Some(mode.clone());
        }
        if let Some(max_sz) = cfg.index_max_file_size {
            search_cfg.index.max_file_size = Some(max_sz);
        }
        if let Some(large_policy) = cfg.index_large_file_policy.as_ref() {
            search_cfg.index.large_file_policy = Some(large_policy.clone());
        }
        if let Some(execution) = cfg.index_execution.as_ref() {
            search_cfg.index.execution = Some(execution.clone());
        }
        if let Some(chunk_size) = cfg.index_chunk_size {
            search_cfg.index.chunk_size = Some(chunk_size);
        }

        let t_scan = Instant::now();
        let files = scan_workspace(&self.workspace_root)?;
        let scan_ms = t_scan.elapsed().as_millis() as u64;

        let se = SearchEngine::new(&self.workspace_root).with_config(search_cfg.clone());
        let t_index = Instant::now();
        let _probe = se.search("__curd_doctor_probe__", None)?;
        let index_wall_ms = t_index.elapsed().as_millis() as u64;
        let stats = se.last_index_stats();

        let mut findings = Vec::new();
        for cf in config_findings {
            findings.push(DoctorFinding {
                severity: cf.severity,
                code: cf.code,
                message: cf.message,
            });
        }

        let mut inventory = SymbolInventoryStats::default();
        let mut comparison = ModeComparisonStats::default();
        let mut parity = ParityStats::default();
        let mut coverage = json!({"state": "unknown"});
        let mut quality = json!({"status": "unknown"});

        if let Some(ref s) = stats {
            coverage = build_index_coverage(s);
            quality = build_index_quality(s);

            let cov_ratio = coverage
                .get("coverage_ratio")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let cov_state = coverage
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if s.parse_fail > 0 {
                findings.push(DoctorFinding {
                    severity: "high".to_string(),
                    code: "doctor_parse_fail".to_string(),
                    message: format!("parse_fail={} during index build", s.parse_fail),
                });
            }

            if let Some(limit) = thresholds.max_parse_fail
                && s.parse_fail > limit
            {
                findings.push(DoctorFinding {
                    severity: "high".to_string(),
                    code: "doctor_parse_fail_exceeded".to_string(),
                    message: format!("parse_fail={} exceeded limit={}", s.parse_fail, limit),
                });
            }

            if let Some(min_cov) = thresholds.min_coverage_ratio
                && cov_ratio < min_cov
            {
                findings.push(DoctorFinding {
                    severity: "high".to_string(),
                    code: "doctor_coverage_ratio_below_min".to_string(),
                    message: format!("coverage_ratio={:.4} below min={:.4}", cov_ratio, min_cov),
                });
            }

            if let Some(ref req_state) = thresholds.require_coverage_state
                && cov_state != req_state
            {
                findings.push(DoctorFinding {
                    severity: "high".to_string(),
                    code: "doctor_coverage_state_mismatch".to_string(),
                    message: format!("coverage_state='{}' required='{}'", cov_state, req_state),
                });
            }

            let should_compute_inventory = thresholds.min_symbol_count.is_some()
                || thresholds.min_symbols_per_k_files.is_some()
                || thresholds.parity_rerun
                || cfg.compare_with_full;

            if should_compute_inventory {
                let symbols = se.search("", None)?;
                let count = symbols.len();
                let density = if s.total_files == 0 {
                    0.0
                } else {
                    (count as f64 * 1000.0) / s.total_files as f64
                };
                inventory = SymbolInventoryStats {
                    computed: true,
                    symbol_count: Some(count),
                    symbols_per_k_files: Some(density),
                };

                if let Some(min_c) = thresholds.min_symbol_count
                    && count < min_c
                {
                    findings.push(DoctorFinding {
                        severity: "high".to_string(),
                        code: "doctor_symbol_count_below_min".to_string(),
                        message: format!("symbol_count={} below min={}", count, min_c),
                    });
                }

                if thresholds.parity_rerun {
                    let fp1 = symbol_fingerprint(&symbols);
                    let symbols2 = se.search("", None)?;
                    let fp2 = symbol_fingerprint(&symbols2);
                    let ok = fp2 == fp1;
                    parity = ParityStats {
                        enabled: true,
                        ok: Some(ok),
                        symbol_count_run1: Some(count),
                        symbol_count_run2: Some(symbols2.len()),
                    };
                    if !ok {
                        findings.push(DoctorFinding {
                            severity: "high".to_string(),
                            code: "doctor_parity_rerun_mismatch".to_string(),
                            message: "Symbol inventory mismatch across immediate rerun".to_string(),
                        });
                    }
                }

                if cfg.compare_with_full {
                    let mut full_cfg = search_cfg.clone();
                    full_cfg.index.mode = Some("full".to_string());
                    let se_full = SearchEngine::new(&self.workspace_root).with_config(full_cfg);
                    let symbols_full = se_full.search("", None)?;
                    let full_ids: HashSet<_> = symbols_full.iter().map(|s| s.id.as_str()).collect();
                    let cur_ids: HashSet<_> = symbols.iter().map(|s| s.id.as_str()).collect();
                    let overlap_count = cur_ids.intersection(&full_ids).count();
                    let overlap = if full_ids.is_empty() {
                        1.0
                    } else {
                        overlap_count as f64 / full_ids.len() as f64
                    };
                    comparison = ModeComparisonStats {
                        enabled: true,
                        overlap_with_full: Some(overlap),
                        current_symbol_count: Some(count),
                        full_symbol_count: Some(symbols_full.len()),
                        overlap_symbol_count: Some(overlap_count),
                    };
                    if let Some(min_o) = thresholds.min_overlap_with_full
                        && overlap < min_o
                    {
                        findings.push(DoctorFinding {
                            severity: "high".to_string(),
                            code: "doctor_overlap_with_full_below_min".to_string(),
                            message: format!("overlap={:.4} below min={:.4}", overlap, min_o),
                        });
                    }
                }
            }
        }

        let status = if findings.iter().any(|f| f.severity == "high") {
            "fail"
        } else if findings.is_empty() {
            "ok"
        } else {
            "warn"
        };

        let mut report = DoctorReport {
            status: status.to_string(),
            workspace_root: self.workspace_root.clone(),
            scan: ScanStats {
                files_found: files.len(),
                scan_ms,
            },
            index_probe: IndexProbeStats {
                wall_ms: index_wall_ms,
                stats: stats.clone(),
            },
            coverage,
            quality,
            symbol_inventory: inventory,
            mode_comparison: comparison,
            parity,
            index_config: cfg,
            findings,
            human_summary: String::new(),
        };

        report.human_summary = self.generate_summary(&report);

        if strict && report.status == "fail" {
            anyhow::bail!("Doctor strict mode failed:\n{}", report.human_summary);
        }

        Ok(report)
    }

    fn generate_summary(&self, r: &DoctorReport) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "CURD DOCTOR REPORT: {}\n",
            r.status.to_uppercase()
        ));
        out.push_str(&format!("Workspace: {}\n", r.workspace_root.display()));
        out.push_str("--------------------------------------------------\n");
        out.push_str(&format!(
            "Scan:    {} files in {}ms\n",
            r.scan.files_found, r.scan.scan_ms
        ));
        out.push_str(&format!("Index:   {}ms wall time\n", r.index_probe.wall_ms));

        if let Some(ref s) = r.index_probe.stats {
            out.push_str(&format!(
                "  Cache: {} hits, {} misses\n",
                s.cache_hits, s.cache_misses
            ));
            out.push_str(&format!(
                "  Parse: {} success, {} fail\n",
                s.cache_misses.saturating_sub(s.parse_fail),
                s.parse_fail
            ));
        }

        let cov_ratio = r
            .coverage
            .get("coverage_ratio")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let cov_state = r
            .coverage
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        out.push_str(&format!(
            "Coverage: {:.1}% ({})\n",
            cov_ratio * 100.0,
            cov_state
        ));

        let qual_status = r
            .quality
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        out.push_str(&format!("Quality:  {}\n", qual_status));

        if r.symbol_inventory.computed {
            out.push_str(&format!(
                "Symbols:  {} total ({:.2} per 1k files)\n",
                r.symbol_inventory.symbol_count.unwrap_or(0),
                r.symbol_inventory.symbols_per_k_files.unwrap_or(0.0)
            ));
        }

        if !r.findings.is_empty() {
            out.push_str("\nFINDINGS:\n");
            for f in &r.findings {
                out.push_str(&format!(
                    "  [{}] ({}) {}\n",
                    f.severity.to_uppercase(),
                    f.code,
                    f.message
                ));
            }

            // Show detailed parse failure samples if available
            if let Some(ref s) = r.index_probe.stats {
                if !s.parse_fail_samples.is_empty() {
                    out.push_str("\nPARSE FAILURE SAMPLES:\n");
                    for sample in &s.parse_fail_samples {
                        out.push_str(&format!("  • {}\n", sample));
                    }
                }
            }
        } else {
            out.push_str("\nNo findings. Workspace looks healthy.\n");
        }

        out
    }
}

fn symbol_fingerprint(symbols: &[Symbol]) -> Vec<String> {
    let mut lines: Vec<String> = symbols
        .iter()
        .map(|s| {
            format!(
                "{}|{}",
                s.id,
                s.semantic_hash.as_deref().unwrap_or("[no_hash]")
            )
        })
        .collect();
    lines.sort_unstable();
    lines
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum DoctorProfile {
    CiFast,
    CiStrict,
}

impl FromStr for DoctorProfile {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ci_fast" | "cifast" => Ok(DoctorProfile::CiFast),
            "ci_strict" | "cistrict" => Ok(DoctorProfile::CiStrict),
            _ => Err(anyhow::anyhow!("Unknown doctor profile: {}", s)),
        }
    }
}

fn apply_profile_defaults(t: &mut DoctorThresholds, profile: DoctorProfile) {
    match profile {
        DoctorProfile::CiFast => {
            if t.max_parse_fail.is_none() {
                t.max_parse_fail = Some(0);
            }
            if t.min_coverage_ratio.is_none() {
                t.min_coverage_ratio = Some(0.90);
            }
            if t.max_skipped_large_ratio.is_none() {
                t.max_skipped_large_ratio = Some(0.30);
            }
            if t.min_symbols_per_k_files.is_none() {
                t.min_symbols_per_k_files = Some(0.5);
            }
        }
        DoctorProfile::CiStrict => {
            if t.max_parse_fail.is_none() {
                t.max_parse_fail = Some(0);
            }
            if t.max_no_symbols_ratio.is_none() {
                t.max_no_symbols_ratio = Some(0.90);
            }
            if t.max_skipped_large_ratio.is_none() {
                t.max_skipped_large_ratio = Some(0.20);
            }
            if t.min_coverage_ratio.is_none() {
                t.min_coverage_ratio = Some(0.99);
            }
            if t.require_coverage_state.is_none() {
                t.require_coverage_state = Some("full".to_string());
            }
            if t.min_symbols_per_k_files.is_none() {
                t.min_symbols_per_k_files = Some(1.0);
            }
            if t.min_overlap_with_full.is_none() {
                t.min_overlap_with_full = Some(0.95);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_doctor_profile_parsing() {
        assert_eq!(
            DoctorProfile::from_str("ci_fast").unwrap(),
            DoctorProfile::CiFast
        );
        assert_eq!(
            DoctorProfile::from_str("cifast").unwrap(),
            DoctorProfile::CiFast
        );
        assert_eq!(
            DoctorProfile::from_str("ci_strict").unwrap(),
            DoctorProfile::CiStrict
        );
        assert!(DoctorProfile::from_str("unknown").is_err());
    }

    #[test]
    fn test_doctor_apply_profile_defaults() {
        let mut t = DoctorThresholds::default();
        apply_profile_defaults(&mut t, DoctorProfile::CiFast);
        assert_eq!(t.max_parse_fail, Some(0));
        assert_eq!(t.min_coverage_ratio, Some(0.90));

        let mut t2 = DoctorThresholds::default();
        apply_profile_defaults(&mut t2, DoctorProfile::CiStrict);
        assert_eq!(t2.min_coverage_ratio, Some(0.99));
        assert_eq!(t2.require_coverage_state, Some("full".to_string()));
    }

    #[test]
    fn test_doctor_engine_run_empty_workspace() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".curd")).unwrap();
        fs::write(root.join("curd.toml"), "").unwrap();

        let engine = DoctorEngine::new(root);
        let report = engine
            .run(
                false,
                DoctorThresholds::default(),
                None,
                DoctorIndexConfig::default(),
            )
            .unwrap();

        assert_eq!(report.status, "ok"); // an empty workspace shouldn't fail unless thresholds demand it
        assert_eq!(report.scan.files_found, 0); // scan_workspace skips non-source or dot files usually, or empty is 0
    }
}
