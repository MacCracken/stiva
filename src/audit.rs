//! Structured audit log — append-only JSON-lines log of runtime operations.
//!
//! Each entry records a timestamp, operation type, target (container/image),
//! result, and optional metadata for compliance and forensics.

use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::debug;

/// Runtime operation that was performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AuditOperation {
    Create,
    Start,
    Stop,
    Kill,
    Remove,
    Exec,
    Pull,
    Push,
    Checkpoint,
    Restore,
}

impl std::fmt::Display for AuditOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create => f.write_str("create"),
            Self::Start => f.write_str("start"),
            Self::Stop => f.write_str("stop"),
            Self::Kill => f.write_str("kill"),
            Self::Remove => f.write_str("remove"),
            Self::Exec => f.write_str("exec"),
            Self::Pull => f.write_str("pull"),
            Self::Push => f.write_str("push"),
            Self::Checkpoint => f.write_str("checkpoint"),
            Self::Restore => f.write_str("restore"),
        }
    }
}

/// Outcome of the audited operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AuditResult {
    Success,
    Failed(String),
}

/// A single audit log entry (serialized as one JSON line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub operation: AuditOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_ref: Option<String>,
    pub user: String,
    pub result: AuditResult,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl AuditEntry {
    /// Build an audit entry for a container operation.
    #[must_use]
    pub fn container(operation: AuditOperation, container_id: &str, result: AuditResult) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            operation,
            container_id: Some(container_id.to_string()),
            image_ref: None,
            user: current_user(),
            result,
            metadata: serde_json::Value::Null,
        }
    }

    /// Build an audit entry for an image operation.
    #[must_use]
    pub fn image(operation: AuditOperation, image_ref: &str, result: AuditResult) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            operation,
            container_id: None,
            image_ref: Some(image_ref.to_string()),
            user: current_user(),
            result,
            metadata: serde_json::Value::Null,
        }
    }

    /// Attach metadata to this entry.
    #[must_use]
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Append-only audit logger backed by a file.
pub struct AuditLog {
    path: PathBuf,
    file: Mutex<std::fs::File>,
}

impl AuditLog {
    /// Open or create an append-only audit log at `path`.
    pub fn new(path: &Path) -> Result<Self, StivaError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| StivaError::Audit(format!("failed to open audit log: {e}")))?;

        debug!(path = %path.display(), "audit log opened");
        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
        })
    }

    /// Append an audit entry to the log.
    pub fn log(&self, entry: &AuditEntry) -> Result<(), StivaError> {
        let mut line = serde_json::to_string(entry)
            .map_err(|e| StivaError::Audit(format!("failed to serialize audit entry: {e}")))?;
        line.push('\n');

        let mut file = self
            .file
            .lock()
            .map_err(|e| StivaError::Audit(format!("audit log lock poisoned: {e}")))?;
        file.write_all(line.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// Read the most recent entries (up to `limit`).
    #[must_use = "returns the audit entries"]
    pub fn read_entries(&self, limit: usize) -> Result<Vec<AuditEntry>, StivaError> {
        let content = std::fs::read_to_string(&self.path)?;
        let entries: Vec<AuditEntry> = content
            .lines()
            .rev()
            .take(limit)
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        Ok(entries)
    }

    /// Path to the audit log file.
    #[must_use]
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Get the current effective user name (best-effort).
#[inline]
fn current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| {
            let mut buf = String::new();
            let _ = write!(buf, "uid:{}", rustix::process::getuid().as_raw());
            buf
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_entry_container_serde() {
        let entry = AuditEntry::container(AuditOperation::Create, "abc123", AuditResult::Success);
        let json = serde_json::to_string(&entry).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.operation, AuditOperation::Create);
        assert_eq!(back.container_id.as_deref(), Some("abc123"));
        assert!(back.image_ref.is_none());
    }

    #[test]
    fn audit_entry_image_serde() {
        let entry = AuditEntry::image(
            AuditOperation::Pull,
            "nginx:latest",
            AuditResult::Failed("timeout".into()),
        );
        let json = serde_json::to_string(&entry).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.operation, AuditOperation::Pull);
        assert_eq!(back.image_ref.as_deref(), Some("nginx:latest"));
        assert!(matches!(back.result, AuditResult::Failed(ref s) if s == "timeout"));
    }

    #[test]
    fn audit_entry_with_metadata() {
        let entry = AuditEntry::container(AuditOperation::Kill, "abc123", AuditResult::Success)
            .with_metadata(serde_json::json!({"signal": 15}));
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"signal\":15"));
    }

    #[test]
    fn audit_log_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let log = AuditLog::new(&path).unwrap();

        log.log(&AuditEntry::container(
            AuditOperation::Create,
            "c1",
            AuditResult::Success,
        ))
        .unwrap();
        log.log(&AuditEntry::container(
            AuditOperation::Start,
            "c1",
            AuditResult::Success,
        ))
        .unwrap();
        log.log(&AuditEntry::image(
            AuditOperation::Pull,
            "nginx",
            AuditResult::Success,
        ))
        .unwrap();

        let entries = log.read_entries(10).unwrap();
        assert_eq!(entries.len(), 3);
        // Most recent first.
        assert_eq!(entries[0].operation, AuditOperation::Pull);
        assert_eq!(entries[2].operation, AuditOperation::Create);
    }

    #[test]
    fn audit_log_read_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let log = AuditLog::new(&path).unwrap();

        for _ in 0..10 {
            log.log(&AuditEntry::container(
                AuditOperation::Start,
                "c1",
                AuditResult::Success,
            ))
            .unwrap();
        }

        let entries = log.read_entries(3).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn audit_log_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let log = AuditLog::new(&path).unwrap();
        let entries = log.read_entries(10).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn audit_operation_display() {
        assert_eq!(AuditOperation::Create.to_string(), "create");
        assert_eq!(AuditOperation::Checkpoint.to_string(), "checkpoint");
    }

    #[test]
    fn audit_result_serde() {
        let s = AuditResult::Success;
        let json = serde_json::to_string(&s).unwrap();
        let back: AuditResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AuditResult::Success);

        let f = AuditResult::Failed("oops".into());
        let json = serde_json::to_string(&f).unwrap();
        let back: AuditResult = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, AuditResult::Failed(ref s) if s == "oops"));
    }

    #[test]
    fn concurrent_audit_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let log = std::sync::Arc::new(AuditLog::new(&path).unwrap());

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let log = std::sync::Arc::clone(&log);
                std::thread::spawn(move || {
                    let mut id = String::new();
                    let _ = write!(id, "c{i}");
                    log.log(&AuditEntry::container(
                        AuditOperation::Start,
                        &id,
                        AuditResult::Success,
                    ))
                    .unwrap();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let entries = log.read_entries(100).unwrap();
        assert_eq!(entries.len(), 8);
    }
}
