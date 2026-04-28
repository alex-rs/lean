use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::events::JsonlEvent;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("failed to create audit directory {path}: {source}")]
    CreateDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to open audit log {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to serialize audit event for {path}: {source}")]
    Serialize {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to write audit log {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditWriter {
    path: PathBuf,
}

impl AuditWriter {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn write_events(&self, events: &[JsonlEvent]) -> Result<(), AuditError> {
        self.ensure_parent_directory()?;

        let path = self.display_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| AuditError::Open {
                path: path.clone(),
                source,
            })?;

        for event in events {
            let line = event
                .to_json_line()
                .map_err(|source| AuditError::Serialize {
                    path: path.clone(),
                    source,
                })?;
            file.write_all(line.as_bytes())
                .map_err(|source| AuditError::Write {
                    path: path.clone(),
                    source,
                })?;
        }

        Ok(())
    }

    fn ensure_parent_directory(&self) -> Result<(), AuditError> {
        let Some(parent) = self.path.parent() else {
            return Ok(());
        };

        if parent.as_os_str().is_empty() || parent == Path::new(".") {
            return Ok(());
        }

        fs::create_dir_all(parent).map_err(|source| AuditError::CreateDirectory {
            path: parent.display().to_string(),
            source,
        })
    }

    fn display_path(&self) -> String {
        self.path.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::events::{Heartbeat, JsonlEvent, SessionStarted};

    use super::AuditWriter;

    #[test]
    fn writes_newline_delimited_events_to_configured_path() {
        let path = std::env::temp_dir()
            .join("lean-audit-tests")
            .join(unique_name())
            .join("audit.jsonl");
        let events = vec![
            JsonlEvent::SessionStarted(SessionStarted {
                session_id: "session-0001".to_string(),
                task: "noop".to_string(),
                provider: "mock".to_string(),
            }),
            JsonlEvent::Heartbeat(Heartbeat {
                session_id: "session-0001".to_string(),
                sequence: 1,
            }),
        ];

        AuditWriter::new(&path)
            .write_events(&events)
            .expect("audit writer should persist events");

        let contents = fs::read_to_string(path).expect("audit file should be readable");
        let parsed = contents
            .lines()
            .map(|line| serde_json::from_str::<JsonlEvent>(line).expect("line should parse"))
            .collect::<Vec<_>>();

        assert_eq!(parsed, events);
    }

    fn unique_name() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        format!("{}-{nanos}", std::process::id())
    }
}
