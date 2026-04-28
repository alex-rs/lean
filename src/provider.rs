use thiserror::Error;

pub const MOCK_PROVIDER_NAME: &str = "mock";
pub const DEFAULT_MOCK_FINAL_MESSAGE: &str = "mock provider completed task";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    pub final_message: String,
}

pub trait ModelProvider {
    fn name(&self) -> &str;
    fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ProviderError>;
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("provider {provider} failed: {message}")]
pub struct ProviderError {
    pub provider: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockProvider {
    final_message: String,
}

impl MockProvider {
    pub fn new(final_message: impl Into<String>) -> Self {
        Self {
            final_message: final_message.into(),
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new(DEFAULT_MOCK_FINAL_MESSAGE)
    }
}

impl ModelProvider for MockProvider {
    fn name(&self) -> &str {
        MOCK_PROVIDER_NAME
    }

    fn complete(&self, _request: ModelRequest) -> Result<ModelResponse, ProviderError> {
        Ok(ModelResponse {
            final_message: self.final_message.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{MockProvider, ModelProvider, ModelRequest};

    #[test]
    fn mock_provider_returns_configured_final_message() {
        let provider = MockProvider::new("configured final response");
        let response = provider
            .complete(ModelRequest {
                task: "noop".to_string(),
            })
            .expect("mock provider should not fail");

        assert_eq!(response.final_message, "configured final response");
    }
}
