use serde::{Deserialize, Serialize};

pub const SESSION_STARTED: &str = "session.started";
pub const HEARTBEAT: &str = "heartbeat";
pub const SESSION_RESULT: &str = "session.result";
pub const SESSION_ERROR: &str = "session.error";
pub const CREDENTIAL_ACCESSED: &str = "credential.accessed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum JsonlEvent {
    #[serde(rename = "session.started")]
    SessionStarted(SessionStarted),
    #[serde(rename = "heartbeat")]
    Heartbeat(Heartbeat),
    #[serde(rename = "session.result")]
    SessionResult(SessionResult),
    #[serde(rename = "session.error")]
    SessionError(SessionError),
    #[serde(rename = "credential.accessed")]
    CredentialAccessed(CredentialAccessed),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStarted {
    pub session_id: String,
    pub task: String,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heartbeat {
    pub session_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResult {
    pub session_id: String,
    pub status: SessionStatus,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionError {
    pub session_id: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialAccessed {
    pub provider: String,
    pub env_var: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Success,
    Failed,
}

impl JsonlEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SessionStarted(_) => SESSION_STARTED,
            Self::Heartbeat(_) => HEARTBEAT,
            Self::SessionResult(_) => SESSION_RESULT,
            Self::SessionError(_) => SESSION_ERROR,
            Self::CredentialAccessed(_) => CREDENTIAL_ACCESSED,
        }
    }

    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        let mut line = serde_json::to_string(self)?;
        line.push('\n');
        Ok(line)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{
        CREDENTIAL_ACCESSED, CredentialAccessed, HEARTBEAT, Heartbeat, JsonlEvent, SESSION_ERROR,
        SESSION_RESULT, SESSION_STARTED, SessionError, SessionResult, SessionStarted,
        SessionStatus,
    };

    #[test]
    fn events_serialize_stable_names() {
        let cases = [
            (
                JsonlEvent::SessionStarted(SessionStarted {
                    session_id: "s1".to_string(),
                    task: "noop".to_string(),
                    provider: "mock".to_string(),
                }),
                SESSION_STARTED,
            ),
            (
                JsonlEvent::Heartbeat(Heartbeat {
                    session_id: "s1".to_string(),
                    sequence: 1,
                }),
                HEARTBEAT,
            ),
            (
                JsonlEvent::SessionResult(SessionResult {
                    session_id: "s1".to_string(),
                    status: SessionStatus::Success,
                    message: "done".to_string(),
                }),
                SESSION_RESULT,
            ),
            (
                JsonlEvent::SessionError(SessionError {
                    session_id: "s1".to_string(),
                    message: "failed".to_string(),
                    recoverable: false,
                }),
                SESSION_ERROR,
            ),
            (
                JsonlEvent::CredentialAccessed(CredentialAccessed {
                    provider: "minimax".to_string(),
                    env_var: "MINIMAX_API_KEY".to_string(),
                }),
                CREDENTIAL_ACCESSED,
            ),
        ];

        for (event, expected_name) in cases {
            let value = serde_json::to_value(&event).expect("event should serialize");
            assert_eq!(event.name(), expected_name);
            assert_eq!(value["event"], Value::String(expected_name.to_string()));
        }
    }

    #[test]
    fn json_line_is_newline_terminated_and_round_trips() {
        let event = JsonlEvent::SessionResult(SessionResult {
            session_id: "s1".to_string(),
            status: SessionStatus::Failed,
            message: "validation failed".to_string(),
        });

        let line = event
            .to_json_line()
            .expect("event should serialize to JSONL");

        assert!(line.ends_with('\n'));
        let parsed: JsonlEvent =
            serde_json::from_str(line.trim_end()).expect("event should deserialize");
        assert_eq!(parsed, event);
    }
}
