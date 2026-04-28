use crate::{
    events::{Heartbeat, JsonlEvent, SessionError, SessionResult, SessionStarted, SessionStatus},
    provider::{ModelProvider, ModelRequest},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRun {
    pub task: String,
}

#[derive(Debug)]
pub struct SessionRunner<P> {
    provider: P,
    next_session_number: u64,
}

impl<P> SessionRunner<P>
where
    P: ModelProvider,
{
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            next_session_number: 1,
        }
    }

    pub fn run(&mut self, run: SessionRun) -> Vec<JsonlEvent> {
        let session_id = self.next_session_id();
        let provider = self.provider.name().to_string();
        let mut events = vec![JsonlEvent::SessionStarted(SessionStarted {
            session_id: session_id.clone(),
            task: run.task.clone(),
            provider,
        })];

        events.push(JsonlEvent::Heartbeat(Heartbeat {
            session_id: session_id.clone(),
            sequence: 1,
        }));

        match self.provider.complete(ModelRequest { task: run.task }) {
            Ok(response) => events.push(JsonlEvent::SessionResult(SessionResult {
                session_id,
                status: SessionStatus::Success,
                message: response.final_message,
            })),
            Err(error) => events.push(JsonlEvent::SessionError(SessionError {
                session_id,
                message: error.to_string(),
                recoverable: false,
            })),
        }

        events
    }

    fn next_session_id(&mut self) -> String {
        let session_id = format!("session-{:04}", self.next_session_number);
        self.next_session_number += 1;
        session_id
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        events::{HEARTBEAT, JsonlEvent, SESSION_RESULT, SESSION_STARTED, SessionStatus},
        provider::MockProvider,
    };

    use super::{SessionRun, SessionRunner};

    #[test]
    fn mock_run_loop_emits_stable_event_ordering() {
        let mut runner = SessionRunner::new(MockProvider::new("done"));
        let events = runner.run(SessionRun {
            task: "noop".to_string(),
        });

        let event_names = events.iter().map(JsonlEvent::name).collect::<Vec<_>>();
        assert_eq!(event_names, [SESSION_STARTED, HEARTBEAT, SESSION_RESULT]);

        assert_eq!(
            events,
            vec![
                JsonlEvent::SessionStarted(crate::events::SessionStarted {
                    session_id: "session-0001".to_string(),
                    task: "noop".to_string(),
                    provider: "mock".to_string(),
                }),
                JsonlEvent::Heartbeat(crate::events::Heartbeat {
                    session_id: "session-0001".to_string(),
                    sequence: 1,
                }),
                JsonlEvent::SessionResult(crate::events::SessionResult {
                    session_id: "session-0001".to_string(),
                    status: SessionStatus::Success,
                    message: "done".to_string(),
                }),
            ]
        );
    }
}
