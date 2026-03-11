use crate::config::ProvenanceConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp_unix: u64,
    #[serde(default)]
    pub sequence_id: Option<u64>,
    #[serde(alias = "session_id")]
    pub collaboration_id: Uuid,
    pub agent_id: Option<String>,
    pub transaction_id: Option<Uuid>,
    pub operation: String, // "dsl" or "plan"
    pub input: Value,
    pub output: Value,
    pub base_hash: Option<String>,
    pub post_hash: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub verification_result: Option<Value>,
}

pub struct HistoryEngine {
    pub log_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionEntry {
    pub timestamp_unix: u64,
    pub sequence_id: Option<u64>,
    #[serde(alias = "session_id")]
    pub collaboration_id: Uuid,
    pub actor_id: Option<String>,
    pub actor_kind: String,
    pub actor_surface: String,
    pub transaction_id: Option<Uuid>,
    pub operation: String,
    pub resources: Vec<String>,
    pub success: bool,
    pub error: Option<String>,
    pub prev_hash: Option<String>,
    pub entry_hash: String,
    pub local_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionCheckpoint {
    pub timestamp_unix: u64,
    pub sequence_id: Option<u64>,
    #[serde(alias = "session_id")]
    pub collaboration_id: Uuid,
    pub entries: usize,
    pub tip_hash: String,
    pub local_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionVerification {
    pub valid: bool,
    pub entries_checked: usize,
    pub latest_hash: Option<String>,
    pub error: Option<String>,
}

pub struct ContributionLedger {
    pub log_path: PathBuf,
    pub checkpoint_path: PathBuf,
    pub config: ProvenanceConfig,
}

impl HistoryEngine {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        let curd_dir = crate::workspace::get_curd_dir(workspace_root.as_ref());
        let log_path = curd_dir.join("repl_history.jsonl");
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self { log_path }
    }

    pub fn log(
        &self,
        sequence_id: Option<u64>,
        collaboration_id: Uuid,
        agent_id: Option<String>,
        transaction_id: Option<Uuid>,
        operation: &str,
        input: Value,
        output: Value,
        base_hash: Option<String>,
        post_hash: Option<String>,
        success: bool,
        error: Option<String>,
        verification_result: Option<Value>,
    ) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Redact sensitive fields before logging
        let redacted_input = crate::redact_value(input);
        let redacted_output = crate::redact_value(output);

        let entry = HistoryEntry {
            timestamp_unix: now,
            sequence_id,
            collaboration_id,
            agent_id,
            transaction_id,
            operation: operation.to_string(),
            input: redacted_input,
            output: redacted_output,
            base_hash,
            post_hash,
            success,
            error,
            verification_result,
        };

        if let Ok(serialized) = serde_json::to_string(&entry)
            && let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_path)
        {
            let _ = writeln!(file, "{}", serialized);
        }
    }

    pub fn get_history(&self, limit: usize) -> Vec<HistoryEntry> {
        if let Ok(file) = File::open(&self.log_path) {
            let reader = BufReader::new(file);
            let mut entries: Vec<HistoryEntry> = reader
                .lines()
                .map_while(Result::ok)
                .filter_map(|line| serde_json::from_str(&line).ok())
                .collect();
            entries.reverse();
            entries.truncate(limit);
            entries
        } else {
            Vec::new()
        }
    }
}

impl ContributionLedger {
    pub fn new(workspace_root: impl AsRef<Path>, config: ProvenanceConfig) -> Self {
        let curd_dir = crate::workspace::get_curd_dir(workspace_root.as_ref());
        let history_dir = curd_dir.join("history");
        let log_path = history_dir.join("contributions.jsonl");
        let checkpoint_path = history_dir.join("contribution_checkpoints.jsonl");
        let _ = std::fs::create_dir_all(&history_dir);
        Self {
            log_path,
            checkpoint_path,
            config,
        }
    }

    pub fn log(
        &self,
        sequence_id: Option<u64>,
        collaboration_id: Uuid,
        actor_id: Option<String>,
        actor_kind: &str,
        actor_surface: &str,
        transaction_id: Option<Uuid>,
        operation: &str,
        resources: Vec<String>,
        success: bool,
        error: Option<String>,
    ) {
        if !self.config.enabled {
            return;
        }
        let now = now_secs();
        let prev_hash = if self.config.hash_chain {
            self.read_last_hash()
        } else {
            None
        };
        let mut entry = ContributionEntry {
            timestamp_unix: now,
            sequence_id,
            collaboration_id,
            actor_id,
            actor_kind: actor_kind.to_string(),
            actor_surface: actor_surface.to_string(),
            transaction_id,
            operation: operation.to_string(),
            resources,
            success,
            error,
            prev_hash,
            entry_hash: String::new(),
            local_only: self.config.local_only,
        };
        entry.entry_hash = compute_entry_hash(&entry);

        if let Ok(serialized) = serde_json::to_string(&entry)
            && let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_path)
        {
            let _ = writeln!(file, "{}", serialized);
        }

        if self.config.checkpoint_every > 0 {
            let count = self.entry_count();
            if count > 0 && count % self.config.checkpoint_every == 0 {
                self.write_checkpoint(&entry, count);
            }
        }
    }

    pub fn get_history(&self, limit: usize) -> Vec<ContributionEntry> {
        if let Ok(file) = File::open(&self.log_path) {
            let reader = BufReader::new(file);
            let mut entries: Vec<ContributionEntry> = reader
                .lines()
                .map_while(Result::ok)
                .filter_map(|line| serde_json::from_str(&line).ok())
                .collect();
            entries.reverse();
            entries.truncate(limit);
            entries
        } else {
            Vec::new()
        }
    }

    pub fn get_checkpoints(&self, limit: usize) -> Vec<ContributionCheckpoint> {
        if let Ok(file) = File::open(&self.checkpoint_path) {
            let reader = BufReader::new(file);
            let mut entries: Vec<ContributionCheckpoint> = reader
                .lines()
                .map_while(Result::ok)
                .filter_map(|line| serde_json::from_str(&line).ok())
                .collect();
            entries.reverse();
            entries.truncate(limit);
            entries
        } else {
            Vec::new()
        }
    }

    pub fn verify_chain(&self) -> ContributionVerification {
        let Ok(file) = File::open(&self.log_path) else {
            return ContributionVerification {
                valid: true,
                entries_checked: 0,
                latest_hash: None,
                error: None,
            };
        };
        let reader = BufReader::new(file);
        let mut previous_hash: Option<String> = None;
        let mut count = 0usize;
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            let entry = match serde_json::from_str::<ContributionEntry>(&line) {
                Ok(entry) => entry,
                Err(err) => {
                    return ContributionVerification {
                        valid: false,
                        entries_checked: count,
                        latest_hash: previous_hash,
                        error: Some(format!("invalid contribution entry JSON: {}", err)),
                    };
                }
            };
            let expected_hash = compute_entry_hash(&entry);
            if entry.entry_hash != expected_hash {
                return ContributionVerification {
                    valid: false,
                    entries_checked: count,
                    latest_hash: previous_hash,
                    error: Some(format!(
                        "entry hash mismatch at sequence {:?}",
                        entry.sequence_id
                    )),
                };
            }
            if self.config.hash_chain && entry.prev_hash != previous_hash {
                return ContributionVerification {
                    valid: false,
                    entries_checked: count,
                    latest_hash: previous_hash,
                    error: Some(format!(
                        "hash chain mismatch at sequence {:?}",
                        entry.sequence_id
                    )),
                };
            }
            previous_hash = Some(entry.entry_hash.clone());
            count += 1;
        }
        ContributionVerification {
            valid: true,
            entries_checked: count,
            latest_hash: previous_hash,
            error: None,
        }
    }

    fn read_last_hash(&self) -> Option<String> {
        let file = File::open(&self.log_path).ok()?;
        let reader = BufReader::new(file);
        let mut last = None;
        for line in reader.lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                last = Some(line);
            }
        }
        last.and_then(|line| serde_json::from_str::<ContributionEntry>(&line).ok())
            .map(|entry| entry.entry_hash)
    }

    fn entry_count(&self) -> usize {
        File::open(&self.log_path)
            .ok()
            .map(|file| {
                BufReader::new(file)
                    .lines()
                    .map_while(Result::ok)
                    .filter(|line| !line.trim().is_empty())
                    .count()
            })
            .unwrap_or(0)
    }

    fn write_checkpoint(&self, entry: &ContributionEntry, entries: usize) {
        let checkpoint = ContributionCheckpoint {
            timestamp_unix: entry.timestamp_unix,
            sequence_id: entry.sequence_id,
            collaboration_id: entry.collaboration_id,
            entries,
            tip_hash: entry.entry_hash.clone(),
            local_only: self.config.local_only,
        };
        if let Ok(serialized) = serde_json::to_string(&checkpoint)
            && let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.checkpoint_path)
        {
            let _ = writeln!(file, "{}", serialized);
        }
    }
}

fn compute_entry_hash(entry: &ContributionEntry) -> String {
    #[derive(Serialize)]
    struct CanonicalContribution<'a> {
        timestamp_unix: u64,
        sequence_id: Option<u64>,
        collaboration_id: Uuid,
        actor_id: &'a Option<String>,
        actor_kind: &'a str,
        actor_surface: &'a str,
        transaction_id: Option<Uuid>,
        operation: &'a str,
        resources: &'a [String],
        success: bool,
        error: &'a Option<String>,
        prev_hash: &'a Option<String>,
        local_only: bool,
    }

    let canonical = CanonicalContribution {
        timestamp_unix: entry.timestamp_unix,
        sequence_id: entry.sequence_id,
        collaboration_id: entry.collaboration_id,
        actor_id: &entry.actor_id,
        actor_kind: &entry.actor_kind,
        actor_surface: &entry.actor_surface,
        transaction_id: entry.transaction_id,
        operation: &entry.operation,
        resources: &entry.resources,
        success: entry.success,
        error: &entry.error,
        prev_hash: &entry.prev_hash,
        local_only: entry.local_only,
    };
    let encoded = serde_json::to_vec(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    format!("{:x}", hasher.finalize())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod provenance_tests {
    use super::ContributionLedger;
    use crate::config::ProvenanceConfig;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn contribution_ledger_hash_chains_entries() {
        let dir = tempdir().expect("tempdir");
        let ledger = ContributionLedger::new(
            dir.path(),
            ProvenanceConfig {
                checkpoint_every: 0,
                ..ProvenanceConfig::default()
            },
        );

        ledger.log(
            Some(1),
            Uuid::new_v4(),
            Some("human-dev".to_string()),
            "human",
            "tool_api",
            None,
            "create_plan_set",
            vec!["plan_set:abc".to_string()],
            true,
            None,
        );
        ledger.log(
            Some(2),
            Uuid::new_v4(),
            Some("agent-1".to_string()),
            "agent",
            "tool_api",
            None,
            "create_plan_variant",
            vec!["variant:def".to_string()],
            true,
            None,
        );

        let history = ledger.get_history(10);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].actor_kind, "agent");
        assert_eq!(history[1].actor_kind, "human");
        assert_eq!(
            history[0].prev_hash.as_deref(),
            Some(history[1].entry_hash.as_str())
        );
    }

    #[test]
    fn contribution_ledger_writes_checkpoints() {
        let dir = tempdir().expect("tempdir");
        let ledger = ContributionLedger::new(
            dir.path(),
            ProvenanceConfig {
                checkpoint_every: 2,
                ..ProvenanceConfig::default()
            },
        );
        let session_id = Uuid::new_v4();
        ledger.log(
            Some(1),
            session_id,
            Some("human-dev".to_string()),
            "human",
            "tool_api",
            None,
            "create_plan_set",
            vec!["plan_set:abc".to_string()],
            true,
            None,
        );
        ledger.log(
            Some(2),
            session_id,
            Some("human-dev".to_string()),
            "human",
            "tool_api",
            None,
            "review_plan_variant",
            vec!["variant:def".to_string()],
            true,
            None,
        );

        let checkpoints =
            std::fs::read_to_string(&ledger.checkpoint_path).expect("checkpoint file");
        assert!(checkpoints.contains("\"entries\":2"));
        assert!(checkpoints.contains("\"tip_hash\""));
    }

    #[test]
    fn contribution_ledger_detects_tampering() {
        let dir = tempdir().expect("tempdir");
        let ledger = ContributionLedger::new(dir.path(), ProvenanceConfig::default());
        let session_id = Uuid::new_v4();
        ledger.log(
            Some(1),
            session_id,
            Some("human-dev".to_string()),
            "human",
            "tool_api",
            None,
            "create_plan_set",
            vec!["plan_set:abc".to_string()],
            true,
            None,
        );
        let original = std::fs::read_to_string(&ledger.log_path).expect("ledger");
        let tampered = original.replace("create_plan_set", "create_plan_variant");
        std::fs::write(&ledger.log_path, tampered).expect("rewrite ledger");
        let verification = ledger.verify_chain();
        assert!(!verification.valid);
        assert!(verification.error.unwrap_or_default().contains("mismatch"));
    }
}
