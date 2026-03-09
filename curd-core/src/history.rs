use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp_unix: u64,
    pub session_id: Uuid,
    pub transaction_id: Option<Uuid>,
    pub operation: String, // "dsl" or "plan"
    pub input: Value,
    pub output: Value,
    pub success: bool,
    pub error: Option<String>,
}

pub struct HistoryEngine {
    pub log_path: PathBuf,
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
        session_id: Uuid,
        transaction_id: Option<Uuid>,
        operation: &str,
        input: Value,
        output: Value,
        success: bool,
        error: Option<String>,
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
            session_id,
            transaction_id,
            operation: operation.to_string(),
            input: redacted_input,
            output: redacted_output,
            success,
            error,
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
