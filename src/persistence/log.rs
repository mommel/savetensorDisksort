//! Append-only JSONL execution logger and event parser.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Individual atomic execution event logged to `execution.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEvent {
    pub ts: DateTime<Utc>,
    pub op_id: String,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dst: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execution logger handling thread-safe append writes to `execution.jsonl`.
#[derive(Debug)]
pub struct ExecutionLogger {
    file_path: PathBuf,
}

impl ExecutionLogger {
    pub fn new<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let p = path.as_ref().to_path_buf();
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self { file_path: p })
    }

    fn append_event(&self, event: &LogEvent) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
        {
            if let Ok(json) = serde_json::to_string(event) {
                let _ = writeln!(file, "{}", json);
            }
        }
    }

    pub fn log_copy_start(&self, op_id: &str, src: &str, dst: &str) {
        self.append_event(&LogEvent {
            ts: Utc::now(),
            op_id: op_id.to_string(),
            phase: "copy_start".to_string(),
            src: Some(src.to_string()),
            dst: Some(dst.to_string()),
            bytes: None,
            elapsed_ms: None,
            hash: None,
            path: None,
            new_target: None,
            error: None,
        });
    }

    pub fn log_copy_done(&self, op_id: &str, bytes: u64, elapsed_ms: u64) {
        self.append_event(&LogEvent {
            ts: Utc::now(),
            op_id: op_id.to_string(),
            phase: "copy_done".to_string(),
            src: None,
            dst: None,
            bytes: Some(bytes),
            elapsed_ms: Some(elapsed_ms),
            hash: None,
            path: None,
            new_target: None,
            error: None,
        });
    }

    pub fn log_verify_start(&self, op_id: &str) {
        self.append_event(&LogEvent {
            ts: Utc::now(),
            op_id: op_id.to_string(),
            phase: "verify_start".to_string(),
            src: None,
            dst: None,
            bytes: None,
            elapsed_ms: None,
            hash: None,
            path: None,
            new_target: None,
            error: None,
        });
    }

    pub fn log_verify_ok(&self, op_id: &str, hash: &str) {
        self.append_event(&LogEvent {
            ts: Utc::now(),
            op_id: op_id.to_string(),
            phase: "verify_ok".to_string(),
            src: None,
            dst: None,
            bytes: None,
            elapsed_ms: None,
            hash: Some(hash.to_string()),
            path: None,
            new_target: None,
            error: None,
        });
    }

    pub fn log_delete_original(&self, op_id: &str) {
        self.append_event(&LogEvent {
            ts: Utc::now(),
            op_id: op_id.to_string(),
            phase: "delete_original".to_string(),
            src: None,
            dst: None,
            bytes: None,
            elapsed_ms: None,
            hash: None,
            path: None,
            new_target: None,
            error: None,
        });
    }

    pub fn log_symlink_update(&self, op_id: &str, path: &str, new_target: &str) {
        self.append_event(&LogEvent {
            ts: Utc::now(),
            op_id: op_id.to_string(),
            phase: "symlink_update".to_string(),
            src: None,
            dst: None,
            bytes: None,
            elapsed_ms: None,
            hash: None,
            path: Some(path.to_string()),
            new_target: Some(new_target.to_string()),
            error: None,
        });
    }

    pub fn log_complete(&self, op_id: &str, hash: Option<&str>) {
        self.append_event(&LogEvent {
            ts: Utc::now(),
            op_id: op_id.to_string(),
            phase: "complete".to_string(),
            src: None,
            dst: None,
            bytes: None,
            elapsed_ms: None,
            hash: hash.map(|s| s.to_string()),
            path: None,
            new_target: None,
            error: None,
        });
    }

    pub fn log_error(&self, op_id: &str, error: &str) {
        self.append_event(&LogEvent {
            ts: Utc::now(),
            op_id: op_id.to_string(),
            phase: "error".to_string(),
            src: None,
            dst: None,
            bytes: None,
            elapsed_ms: None,
            hash: None,
            path: None,
            new_target: None,
            error: Some(error.to_string()),
        });
    }
}

/// Read all events from an `execution.jsonl` file.
pub fn read_execution_log<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<LogEvent>> {
    let p = path.as_ref();
    if !p.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(p)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(ev) = serde_json::from_str::<LogEvent>(&line) {
            events.push(ev);
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_logger_and_read() {
        let temp = NamedTempFile::new().unwrap();
        let logger = ExecutionLogger::new(temp.path()).unwrap();

        logger.log_copy_start("op-0001", "/src/a.pt", "/dst/a.pt");
        logger.log_copy_done("op-0001", 1024, 50);
        logger.log_verify_ok("op-0001", "abc123hash");
        logger.log_complete("op-0001", Some("abc123hash"));

        let events = read_execution_log(temp.path()).unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].phase, "copy_start");
        assert_eq!(events[3].phase, "complete");
    }
}
