use std::{collections::BTreeMap, fmt, time::Duration};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::{LeanConfig, ProviderConfig, ProviderKind};

pub const MOCK_PROVIDER_NAME: &str = "mock";
pub const DEFAULT_MOCK_FINAL_MESSAGE: &str = "mock provider completed task";
pub const MINIMAX_PROVIDER_NAME: &str = "minimax";
pub const MINIMAX_DEFAULT_MODEL: &str = "MiniMax-M2.7";
pub const MINIMAX_API_KEY_ENV: &str = "MINIMAX_API_KEY";
pub const MINIMAX_BASE_URL: &str = "https://api.minimax.io/v1";
pub const MINIMAX_BASE_URL_ENV: &str = "MINIMAX_BASE_URL";
pub const OPENAI_COMPATIBLE_BASE_URL: &str = "https://api.openai.com/v1";

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
#[error("provider {provider} failed: {kind}")]
pub struct ProviderError {
    pub provider: String,
    pub kind: ProviderErrorKind,
}

impl ProviderError {
    fn new(provider: impl Into<String>, kind: ProviderErrorKind) -> Self {
        Self {
            provider: provider.into(),
            kind,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderErrorKind {
    #[error("HTTP request failed with status {status_code}")]
    HttpStatus { status_code: u16 },

    #[error("HTTP transport failed")]
    HttpTransport,

    #[error("provider returned malformed response: {reason}")]
    MalformedResponse { reason: &'static str },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderRegistryError {
    #[error("unsupported provider: {provider}")]
    UnknownProvider { provider: String },

    #[error("provider {provider} is missing credential environment variable {env_var}")]
    MissingCredential { provider: String, env_var: String },

    #[error("provider {provider} credential environment variable {env_var} must not be empty")]
    EmptyCredential { provider: String, env_var: String },

    #[error("invalid provider {provider}: {reason}")]
    InvalidProviderSpec {
        provider: String,
        reason: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialStore {
    Environment,
    Values(BTreeMap<String, String>),
}

impl CredentialStore {
    fn get(&self, name: &str) -> Option<String> {
        match self {
            Self::Environment => std::env::var(name).ok(),
            Self::Values(values) => values.get(name).cloned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialAccess {
    pub provider: String,
    pub env_var: String,
}

pub struct ResolvedProvider {
    provider: Box<dyn ModelProvider>,
    credential_access: Option<CredentialAccess>,
}

impl ResolvedProvider {
    pub fn credential_access(&self) -> Option<&CredentialAccess> {
        self.credential_access.as_ref()
    }

    pub fn into_provider(self) -> Box<dyn ModelProvider> {
        self.provider
    }
}

#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    providers: Vec<ProviderConfig>,
    credentials: CredentialStore,
}

impl ProviderRegistry {
    pub fn from_config(config: Option<&LeanConfig>) -> Self {
        Self {
            providers: config
                .map(|config| config.providers.clone())
                .unwrap_or_default(),
            credentials: CredentialStore::Environment,
        }
    }

    pub fn with_credentials(
        providers: Vec<ProviderConfig>,
        credentials: BTreeMap<String, String>,
    ) -> Self {
        Self {
            providers,
            credentials: CredentialStore::Values(credentials),
        }
    }

    pub fn resolve(
        &self,
        provider_name: &str,
    ) -> Result<Box<dyn ModelProvider>, ProviderRegistryError> {
        self.resolve_with_audit(provider_name)
            .map(ResolvedProvider::into_provider)
    }

    pub fn resolve_with_audit(
        &self,
        provider_name: &str,
    ) -> Result<ResolvedProvider, ProviderRegistryError> {
        if provider_name == MOCK_PROVIDER_NAME {
            return Ok(ResolvedProvider {
                provider: Box::new(MockProvider::default()),
                credential_access: None,
            });
        }

        if let Some(profile) = self.configured_profile(provider_name) {
            return self.resolve_profile(profile);
        }

        if let Some(profile) = built_in_minimax_profile(provider_name)? {
            return self.resolve_profile(profile);
        }

        Err(ProviderRegistryError::UnknownProvider {
            provider: provider_name.to_string(),
        })
    }

    fn configured_profile(&self, provider_name: &str) -> Option<ResolvedProviderProfile> {
        self.providers
            .iter()
            .find(|provider| provider.name == provider_name)
            .map(ResolvedProviderProfile::from)
    }

    fn resolve_profile(
        &self,
        profile: ResolvedProviderProfile,
    ) -> Result<ResolvedProvider, ProviderRegistryError> {
        let api_key = self.credentials.get(&profile.api_key_env).ok_or_else(|| {
            ProviderRegistryError::MissingCredential {
                provider: profile.name.clone(),
                env_var: profile.api_key_env.clone(),
            }
        })?;

        if api_key.trim().is_empty() {
            return Err(ProviderRegistryError::EmptyCredential {
                provider: profile.name,
                env_var: profile.api_key_env,
            });
        }

        let credential_access = CredentialAccess {
            provider: profile.name.clone(),
            env_var: profile.api_key_env.clone(),
        };

        match profile.kind {
            ProviderKind::OpenAiCompatible => Ok(ResolvedProvider {
                provider: Box::new(OpenAiCompatibleProvider::with_options(
                    profile.name,
                    profile.model,
                    profile.base_url,
                    api_key,
                    profile.reasoning_split,
                )),
                credential_access: Some(credential_access),
            }),
        }
    }
}

impl ModelProvider for Box<dyn ModelProvider> {
    fn name(&self) -> &str {
        self.as_ref().name()
    }

    fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ProviderError> {
        self.as_ref().complete(request)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedProviderProfile {
    name: String,
    kind: ProviderKind,
    model: String,
    api_key_env: String,
    base_url: String,
    reasoning_split: Option<bool>,
}

impl From<&ProviderConfig> for ResolvedProviderProfile {
    fn from(config: &ProviderConfig) -> Self {
        Self {
            name: config.name.clone(),
            kind: config.kind,
            model: config.model.clone(),
            api_key_env: config.api_key_env.clone(),
            base_url: config
                .base_url
                .clone()
                .unwrap_or_else(|| OPENAI_COMPATIBLE_BASE_URL.to_string()),
            reasoning_split: None,
        }
    }
}

fn built_in_minimax_profile(
    provider_name: &str,
) -> Result<Option<ResolvedProviderProfile>, ProviderRegistryError> {
    let model = if provider_name == MINIMAX_PROVIDER_NAME {
        MINIMAX_DEFAULT_MODEL
    } else if let Some(model) = provider_name.strip_prefix("minimax/") {
        let model = model.trim();
        if model.is_empty() {
            return Err(ProviderRegistryError::InvalidProviderSpec {
                provider: provider_name.to_string(),
                reason: "minimax provider model must not be empty",
            });
        }
        model
    } else {
        return Ok(None);
    };

    Ok(Some(ResolvedProviderProfile {
        name: provider_name.to_string(),
        kind: ProviderKind::OpenAiCompatible,
        model: model.to_string(),
        api_key_env: MINIMAX_API_KEY_ENV.to_string(),
        base_url: minimax_base_url(),
        reasoning_split: Some(true),
    }))
}

fn minimax_base_url() -> String {
    std::env::var(MINIMAX_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| MINIMAX_BASE_URL.to_string())
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

pub struct OpenAiCompatibleProvider {
    name: String,
    model: String,
    base_url: String,
    api_key: String,
    reasoning_split: Option<bool>,
    client: reqwest::blocking::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        name: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self::with_options(name, model, base_url, api_key, None)
    }

    fn with_options(
        name: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        reasoning_split: Option<bool>,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("provider HTTP client configuration should be valid");

        Self {
            name: name.into(),
            model: model.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            reasoning_split,
            client,
        }
    }

    fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn map_reqwest_error(&self, error: reqwest::Error) -> ProviderError {
        let kind = if error.is_decode() {
            ProviderErrorKind::MalformedResponse {
                reason: "response JSON was invalid",
            }
        } else {
            ProviderErrorKind::HttpTransport
        };

        ProviderError::new(self.name.clone(), kind)
    }
}

impl fmt::Debug for OpenAiCompatibleProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleProvider")
            .field("name", &self.name)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("reasoning_split", &self.reasoning_split)
            .finish_non_exhaustive()
    }
}

impl ModelProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ProviderError> {
        let body = ChatCompletionRequest {
            model: &self.model,
            messages: vec![ChatCompletionMessage {
                role: "user",
                content: &request.task,
            }],
            stream: false,
            reasoning_split: self.reasoning_split,
        };

        let auth_header = format!("Bearer {}", self.api_key);
        let response = self
            .client
            .post(self.chat_completions_url())
            .header(reqwest::header::AUTHORIZATION, auth_header)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body)
            .send()
            .map_err(|error| self.map_reqwest_error(error))?;

        let status_code = response.status();
        if !status_code.is_success() {
            return Err(ProviderError::new(
                self.name.clone(),
                ProviderErrorKind::HttpStatus {
                    status_code: status_code.as_u16(),
                },
            ));
        }

        let response = response
            .json::<ChatCompletionResponse>()
            .map_err(|error| self.map_reqwest_error(error))?;

        let final_message = response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .ok_or_else(|| {
                ProviderError::new(
                    self.name.clone(),
                    ProviderErrorKind::MalformedResponse {
                        reason: "missing choices[0].message.content",
                    },
                )
            })?;

        Ok(ModelResponse { final_message })
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatCompletionMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_split: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponseMessage {
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Duration,
    };

    use serde_json::Value;

    use crate::config::{ProviderConfig, ProviderKind};

    use super::{
        MINIMAX_API_KEY_ENV, MockProvider, ModelProvider, ModelRequest, OpenAiCompatibleProvider,
        ProviderErrorKind, ProviderRegistry, ProviderRegistryError,
    };

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

    #[test]
    fn registry_resolves_mock_provider() {
        let provider = ProviderRegistry::with_credentials(Vec::new(), BTreeMap::new())
            .resolve("mock")
            .expect("mock provider should resolve");

        assert_eq!(provider.name(), "mock");
    }

    #[test]
    fn registry_rejects_unknown_provider() {
        let error = provider_registry_error(
            ProviderRegistry::with_credentials(Vec::new(), BTreeMap::new()).resolve("missing"),
        );

        assert_eq!(
            error,
            ProviderRegistryError::UnknownProvider {
                provider: "missing".to_string()
            }
        );
    }

    #[test]
    fn registry_rejects_missing_and_empty_credentials() {
        let provider = provider_config();
        let missing = provider_registry_error(
            ProviderRegistry::with_credentials(vec![provider.clone()], BTreeMap::new())
                .resolve("fake"),
        );

        assert_eq!(
            missing,
            ProviderRegistryError::MissingCredential {
                provider: "fake".to_string(),
                env_var: "FAKE_API_KEY".to_string(),
            }
        );

        let empty = provider_registry_error(
            ProviderRegistry::with_credentials(
                vec![provider],
                BTreeMap::from([("FAKE_API_KEY".to_string(), " ".to_string())]),
            )
            .resolve("fake"),
        );

        assert_eq!(
            empty,
            ProviderRegistryError::EmptyCredential {
                provider: "fake".to_string(),
                env_var: "FAKE_API_KEY".to_string(),
            }
        );
    }

    #[test]
    fn registry_resolves_minimax_opencode_style_model_id() {
        let resolved = ProviderRegistry::with_credentials(
            Vec::new(),
            BTreeMap::from([(MINIMAX_API_KEY_ENV.to_string(), "test-token".to_string())]),
        )
        .resolve_with_audit("minimax/MiniMax-M2.7")
        .expect("built-in minimax provider should resolve");

        assert_eq!(resolved.provider.name(), "minimax/MiniMax-M2.7");
        assert_eq!(
            resolved.credential_access(),
            Some(&super::CredentialAccess {
                provider: "minimax/MiniMax-M2.7".to_string(),
                env_var: MINIMAX_API_KEY_ENV.to_string(),
            })
        );
    }

    #[test]
    fn registry_rejects_empty_minimax_model_suffix() {
        let error = provider_registry_error(
            ProviderRegistry::with_credentials(
                Vec::new(),
                BTreeMap::from([(MINIMAX_API_KEY_ENV.to_string(), "test-token".to_string())]),
            )
            .resolve("minimax/ "),
        );

        assert_eq!(
            error,
            ProviderRegistryError::InvalidProviderSpec {
                provider: "minimax/ ".to_string(),
                reason: "minimax provider model must not be empty",
            }
        );
    }

    #[test]
    fn openai_compatible_provider_maps_request_and_response() {
        let server = FakeHttpServer::spawn(
            200,
            r#"{"choices":[{"message":{"content":"real provider completed"}}]}"#,
        );
        let provider =
            OpenAiCompatibleProvider::new("fake", "fake-model", server.base_url(), "test-token");

        let response = provider
            .complete(ModelRequest {
                task: "summarize this".to_string(),
            })
            .expect("fake provider should complete");

        assert_eq!(response.final_message, "real provider completed");

        let request = server.request();
        let request_lower = request.to_ascii_lowercase();
        assert!(request_lower.contains("authorization: bearer test-token"));

        let body = http_body(&request);
        let value = serde_json::from_str::<Value>(body).expect("request body should be JSON");
        assert_eq!(value["model"], "fake-model");
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"], "summarize this");
        assert_eq!(value["stream"], Value::Bool(false));
    }

    #[test]
    fn openai_compatible_provider_returns_sanitized_http_errors() {
        let server = FakeHttpServer::spawn(500, r#"{"error":"raw response body"}"#);
        let provider =
            OpenAiCompatibleProvider::new("fake", "fake-model", server.base_url(), "secret-token");

        let error = provider
            .complete(ModelRequest {
                task: "secret request body".to_string(),
            })
            .expect_err("HTTP status should fail");

        assert_eq!(
            error.kind,
            ProviderErrorKind::HttpStatus { status_code: 500 }
        );
        let message = error.to_string();
        assert!(!message.contains("secret-token"));
        assert!(!message.contains("secret request body"));
        assert!(!message.contains("raw response body"));
    }

    #[test]
    fn openai_compatible_provider_rejects_malformed_responses() {
        let server = FakeHttpServer::spawn(200, r#"{"choices":[{"message":{}}]}"#);
        let provider =
            OpenAiCompatibleProvider::new("fake", "fake-model", server.base_url(), "test-token");

        let error = provider
            .complete(ModelRequest {
                task: "noop".to_string(),
            })
            .expect_err("missing content should fail");

        assert_eq!(
            error.kind,
            ProviderErrorKind::MalformedResponse {
                reason: "missing choices[0].message.content"
            }
        );
    }

    fn provider_config() -> ProviderConfig {
        ProviderConfig {
            name: "fake".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            model: "fake-model".to_string(),
            api_key_env: "FAKE_API_KEY".to_string(),
            base_url: Some("http://127.0.0.1:1/v1".to_string()),
        }
    }

    fn provider_registry_error(
        result: Result<Box<dyn ModelProvider>, ProviderRegistryError>,
    ) -> ProviderRegistryError {
        match result {
            Ok(provider) => panic!("provider {} should not resolve", provider.name()),
            Err(error) => error,
        }
    }

    struct FakeHttpServer {
        url: String,
        requests: mpsc::Receiver<String>,
    }

    impl FakeHttpServer {
        fn spawn(status: u16, body: &'static str) -> Self {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("fake server should bind to localhost");
            let url = format!(
                "http://{}/v1",
                listener
                    .local_addr()
                    .expect("fake server should have address")
            );
            let (requests, request_receiver) = mpsc::channel();

            thread::spawn(move || {
                let (mut stream, _) = listener
                    .accept()
                    .expect("fake server should accept request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("fake server should set read timeout");
                let request = read_http_request(&mut stream);
                requests
                    .send(request)
                    .expect("fake server should send captured request");

                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("fake server should write response");
            });

            Self {
                url,
                requests: request_receiver,
            }
        }

        fn base_url(&self) -> &str {
            &self.url
        }

        fn request(self) -> String {
            self.requests
                .recv_timeout(Duration::from_secs(2))
                .expect("fake server should capture request")
        }
    }

    fn read_http_request(stream: &mut impl Read) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0; 1024];

        loop {
            let count = stream
                .read(&mut buffer)
                .expect("fake server should read request");
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);

            if let Some(expected_len) = expected_http_request_len(&bytes) {
                if bytes.len() >= expected_len {
                    break;
                }
            }
        }

        String::from_utf8(bytes).expect("HTTP request should be UTF-8")
    }

    fn expected_http_request_len(bytes: &[u8]) -> Option<usize> {
        let request = std::str::from_utf8(bytes).ok()?;
        let header_end = request.find("\r\n\r\n")? + 4;
        let content_length = request
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .or_else(|| {
                request
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);

        Some(header_end + content_length)
    }

    fn http_body(request: &str) -> &str {
        request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .expect("request should include body")
    }
}
