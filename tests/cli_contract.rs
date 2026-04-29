use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use lean::{
    events::{JsonlEvent, SESSION_RESULT, SessionStatus},
    prompts::default_prompt_bundle,
};
use serde_json::Value;

#[test]
fn binary_help_lists_expected_subcommands() {
    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .arg("--help")
        .output()
        .expect("lean --help should execute");

    assert!(output.status.success(), "lean --help should exit zero");

    let stdout = String::from_utf8(output.stdout).expect("help output should be UTF-8");
    for expected in ["run", "doctor", "list-skills", "list-agents"] {
        assert!(
            stdout.contains(expected),
            "help output should include {expected}"
        );
    }
}

#[test]
fn run_with_mock_provider_emits_parseable_jsonl_result() {
    let home = unique_temp_dir("mock-provider-home");
    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .env("HOME", &home)
        .args(["run", "--provider", "mock", "--json", "--task", "noop"])
        .output()
        .expect("lean run should execute");

    assert!(
        output.status.success(),
        "lean run should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let events = stdout
        .lines()
        .map(|line| serde_json::from_str::<JsonlEvent>(line).expect("line should parse as event"))
        .collect::<Vec<_>>();

    let last_event = events.last().expect("run should emit events");
    assert_eq!(last_event.name(), SESSION_RESULT);
}

#[test]
fn run_with_configured_real_provider_emits_existing_jsonl_contract() {
    let server = FakeHttpServer::spawn(
        200,
        r#"{"choices":[{"message":{"content":"fake real provider completed"}}]}"#,
    );
    let temp_dir = unique_temp_dir("real-provider-contract");
    let home = unique_temp_dir("real-provider-home");
    let config_path = temp_dir.join("lean.yaml");
    let audit_path = temp_dir.join("audit").join("session.jsonl");
    write_real_provider_config(&config_path, server.base_url(), &audit_path);

    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .env("HOME", &home)
        .env("LEAN_TEST_REAL_PROVIDER_KEY", "test-token")
        .args([
            "--config",
            config_path.to_str().expect("config path should be UTF-8"),
            "run",
            "--provider",
            "fake-real",
            "--json",
            "--task",
            "call a fake provider",
        ])
        .output()
        .expect("lean run should execute");

    assert!(
        output.status.success(),
        "lean run should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let events = parse_jsonl_events(&stdout);

    assert_eq!(
        events.first().expect("run should emit a started event"),
        &JsonlEvent::SessionStarted(lean::events::SessionStarted {
            session_id: "session-0001".to_string(),
            task: "call a fake provider".to_string(),
            provider: "fake-real".to_string(),
        })
    );
    assert_eq!(
        events.last().expect("run should emit a final event"),
        &JsonlEvent::SessionResult(lean::events::SessionResult {
            session_id: "session-0001".to_string(),
            status: SessionStatus::Success,
            message: "fake real provider completed".to_string(),
        })
    );

    let request = server.request();
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-token")
    );
    let body = http_body(&request);
    let value = serde_json::from_str::<Value>(body).expect("request body should be JSON");
    assert_eq!(value["model"], "fake-model");
    assert_eq!(value["messages"][0]["role"], "system");
    assert!(message_content_text(&value["messages"][0]).contains("Available Tools"));
    assert_eq!(value["messages"][1]["role"], "user");
    assert_eq!(value["messages"][1]["content"], "call a fake provider");
    assert!(
        home.join(".lean")
            .join("prompts")
            .join("default.json")
            .is_file()
    );

    let audit_contents = fs::read_to_string(audit_path).expect("audit log should be readable");
    let audit_events = parse_jsonl_events(&audit_contents);
    assert_eq!(
        audit_events
            .first()
            .expect("audit should include credential row"),
        &JsonlEvent::CredentialAccessed(lean::events::CredentialAccessed {
            provider: "fake-real".to_string(),
            env_var: "LEAN_TEST_REAL_PROVIDER_KEY".to_string(),
        })
    );
    assert_eq!(
        audit_events
            .last()
            .expect("audit should include final event"),
        events.last().expect("stdout should include final event")
    );
}

#[test]
fn run_with_builtin_minimax_provider_emits_jsonl_and_audits_credential() {
    let server = FakeHttpServer::spawn(
        200,
        r#"{"choices":[{"message":{"content":"builtin minimax completed"}}]}"#,
    );
    let temp_dir = unique_temp_dir("builtin-minimax-contract");
    let home = unique_temp_dir("builtin-minimax-home");
    let config_path = temp_dir.join("lean.yaml");
    let audit_path = temp_dir.join("audit").join("session.jsonl");
    write_builtin_provider_config(&config_path, &audit_path);

    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .env("HOME", &home)
        .env("MINIMAX_API_KEY", "test-token")
        .env("MINIMAX_BASE_URL", server.base_url())
        .args([
            "--config",
            config_path.to_str().expect("config path should be UTF-8"),
            "run",
            "--provider",
            "minimax/MiniMax-M2.7",
            "--json",
            "--task",
            "call builtin minimax",
        ])
        .output()
        .expect("lean run should execute");

    assert!(
        output.status.success(),
        "lean run should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let events = parse_jsonl_events(&stdout);
    assert_eq!(
        events.last().expect("run should emit a final event"),
        &JsonlEvent::SessionResult(lean::events::SessionResult {
            session_id: "session-0001".to_string(),
            status: SessionStatus::Success,
            message: "builtin minimax completed".to_string(),
        })
    );

    let request = server.request();
    let body = http_body(&request);
    let value = serde_json::from_str::<Value>(body).expect("request body should be JSON");
    assert_eq!(value["model"], "MiniMax-M2.7");
    assert_eq!(value["messages"][0]["role"], "system");
    assert_eq!(value["messages"][1]["role"], "user");
    assert_eq!(value["messages"][1]["content"], "call builtin minimax");
    assert_eq!(value["reasoning_split"], Value::Bool(true));

    let audit_contents = fs::read_to_string(audit_path).expect("audit log should be readable");
    let audit_events = parse_jsonl_events(&audit_contents);
    assert_eq!(
        audit_events
            .first()
            .expect("audit should include credential row"),
        &JsonlEvent::CredentialAccessed(lean::events::CredentialAccessed {
            provider: "minimax/MiniMax-M2.7".to_string(),
            env_var: "MINIMAX_API_KEY".to_string(),
        })
    );
}

#[test]
fn run_with_rig_provider_emits_existing_jsonl_contract() {
    let server = FakeHttpServer::spawn(
        200,
        r#"{"id":"chatcmpl-test","object":"chat.completion","created":1,"model":"fake-model","system_fingerprint":null,"choices":[{"index":0,"message":{"role":"assistant","content":"rig provider completed"},"logprobs":null,"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"total_tokens":2}}"#,
    );
    let temp_dir = unique_temp_dir("rig-provider-contract");
    let home = unique_temp_dir("rig-provider-home");
    let config_path = temp_dir.join("lean.yaml");
    let audit_path = temp_dir.join("audit").join("session.jsonl");
    write_rig_provider_config(&config_path, server.base_url(), &audit_path);

    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .env("HOME", &home)
        .env("LEAN_TEST_RIG_PROVIDER_KEY", "test-token")
        .args([
            "--config",
            config_path.to_str().expect("config path should be UTF-8"),
            "run",
            "--provider",
            "rig-openai",
            "--json",
            "--task",
            "call rig provider",
        ])
        .output()
        .expect("lean run should execute");

    assert!(
        output.status.success(),
        "lean run should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let events = parse_jsonl_events(&stdout);

    assert_eq!(
        events.first().expect("run should emit a started event"),
        &JsonlEvent::SessionStarted(lean::events::SessionStarted {
            session_id: "session-0001".to_string(),
            task: "call rig provider".to_string(),
            provider: "rig-openai".to_string(),
        })
    );
    assert_eq!(
        events.last().expect("run should emit a final event"),
        &JsonlEvent::SessionResult(lean::events::SessionResult {
            session_id: "session-0001".to_string(),
            status: SessionStatus::Success,
            message: "rig provider completed".to_string(),
        })
    );

    let request = server.request();
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-token")
    );
    let body = http_body(&request);
    let value = serde_json::from_str::<Value>(body).expect("request body should be JSON");
    assert_eq!(value["model"], "fake-model");
    assert_eq!(value["messages"][0]["role"], "system");
    assert!(message_content_text(&value["messages"][0]).contains("read_file"));
    assert_eq!(value["messages"][1]["role"], "user");
    assert_eq!(
        message_content_text(&value["messages"][1]),
        "call rig provider"
    );

    let audit_contents = fs::read_to_string(audit_path).expect("audit log should be readable");
    let audit_events = parse_jsonl_events(&audit_contents);
    assert_eq!(
        audit_events
            .first()
            .expect("audit should include credential row"),
        &JsonlEvent::CredentialAccessed(lean::events::CredentialAccessed {
            provider: "rig-openai".to_string(),
            env_var: "LEAN_TEST_RIG_PROVIDER_KEY".to_string(),
        })
    );
    assert_eq!(
        audit_events
            .last()
            .expect("audit should include final event"),
        events.last().expect("stdout should include final event")
    );
}

#[test]
fn run_loads_custom_prompt_bundle_from_home_prompt_directory() {
    let server = FakeHttpServer::spawn(
        200,
        r#"{"choices":[{"message":{"content":"custom prompt completed"}}]}"#,
    );
    let temp_dir = unique_temp_dir("custom-prompt-contract");
    let home = unique_temp_dir("custom-prompt-home");
    let prompt_dir = home.join(".lean").join("prompts");
    fs::create_dir_all(&prompt_dir).expect("prompt dir should be writable");
    let mut prompt = default_prompt_bundle();
    prompt.id = "custom".to_string();
    prompt.title = "Custom Prompt".to_string();
    prompt.system = vec!["Use this custom home prompt.".to_string()];
    fs::write(
        prompt_dir.join("custom.json"),
        serde_json::to_string_pretty(&prompt).expect("prompt should serialize"),
    )
    .expect("custom prompt should be writable");

    let config_path = temp_dir.join("lean.yaml");
    let audit_path = temp_dir.join("audit").join("session.jsonl");
    write_real_provider_config(&config_path, server.base_url(), &audit_path);

    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .env("HOME", &home)
        .env("LEAN_TEST_REAL_PROVIDER_KEY", "test-token")
        .args([
            "--config",
            config_path.to_str().expect("config path should be UTF-8"),
            "run",
            "--provider",
            "fake-real",
            "--prompt",
            "custom",
            "--json",
            "--task",
            "use custom prompt",
        ])
        .output()
        .expect("lean run should execute");

    assert!(
        output.status.success(),
        "lean run should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let request = server.request();
    let body = http_body(&request);
    let value = serde_json::from_str::<Value>(body).expect("request body should be JSON");
    assert!(message_content_text(&value["messages"][0]).contains("Use this custom home prompt."));
    assert_eq!(value["messages"][1]["content"], "use custom prompt");
}

#[test]
fn run_with_configured_audit_path_records_complete_jsonl_session() {
    let temp_dir = unique_temp_dir("audit-contract");
    let home = unique_temp_dir("audit-home");
    let config_path = temp_dir.join("lean.yaml");
    let audit_path = temp_dir.join("audit").join("session.jsonl");
    write_config(&config_path, &audit_path);

    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .env("HOME", &home)
        .args([
            "--config",
            config_path.to_str().expect("config path should be UTF-8"),
            "run",
            "--json",
            "--task",
            "noop",
        ])
        .output()
        .expect("lean run should execute");

    assert!(
        output.status.success(),
        "lean run should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let stdout_events = parse_jsonl_events(&stdout);
    let audit_contents = fs::read_to_string(audit_path).expect("audit log should be readable");
    let audit_events = parse_jsonl_events(&audit_contents);

    assert_eq!(stdout_events, audit_events);
    assert_eq!(
        audit_events
            .last()
            .expect("audit should include final event")
            .name(),
        SESSION_RESULT
    );
}

#[test]
fn doctor_valid_config_outputs_structured_success() {
    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .args(["doctor", "--config", "fixtures/config/valid.yaml"])
        .output()
        .expect("lean doctor should execute");

    assert!(
        output.status.success(),
        "lean doctor should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value = parse_stdout_json(output.stdout);
    assert_eq!(value["ok"], Value::Bool(true));
    assert!(value["checks"].is_array());
}

#[test]
fn doctor_invalid_config_outputs_structured_diagnostic() {
    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .args(["doctor", "--config", "fixtures/config/invalid.yaml"])
        .output()
        .expect("lean doctor should execute");

    assert!(
        !output.status.success(),
        "lean doctor should exit non-zero for invalid config"
    );

    let value = parse_stdout_json(output.stdout);
    assert_eq!(value["ok"], Value::Bool(false));
    assert_eq!(
        value["diagnostics"][0]["code"],
        Value::String("config_validation_failed".to_string())
    );
}

#[test]
fn list_skills_json_returns_array() {
    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .args(["list-skills", "--json"])
        .output()
        .expect("lean list-skills should execute");

    assert!(
        output.status.success(),
        "lean list-skills should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(matches!(parse_stdout_json(output.stdout), Value::Array(_)));
}

#[test]
fn list_agents_json_returns_object() {
    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
        .args(["list-agents", "--json"])
        .output()
        .expect("lean list-agents should execute");

    assert!(
        output.status.success(),
        "lean list-agents should exit zero, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value = parse_stdout_json(output.stdout);
    assert!(matches!(value, Value::Object(_)));
    assert_eq!(
        value["agents"][0]["id"],
        Value::String("developer".to_string())
    );
}

fn parse_jsonl_events(contents: &str) -> Vec<JsonlEvent> {
    contents
        .lines()
        .map(|line| serde_json::from_str::<JsonlEvent>(line).expect("line should parse as event"))
        .collect()
}

fn parse_stdout_json(stdout: Vec<u8>) -> Value {
    let stdout = String::from_utf8(stdout).expect("stdout should be UTF-8");
    serde_json::from_str(&stdout).expect("stdout should parse as JSON")
}

fn message_content_text(message: &Value) -> &str {
    message["content"]
        .as_str()
        .or_else(|| message["content"][0]["text"].as_str())
        .expect("message should contain text")
}

fn write_config(config_path: &Path, audit_path: &Path) {
    let contents = format!(
        r#"project:
  name: lean
  root: .
runtime:
  default_provider: mock
  max_turns: 12
events:
  format: jsonl
  audit_path: {}
"#,
        audit_path.display()
    );
    fs::write(config_path, contents).expect("test config should be writable");
}

fn write_real_provider_config(config_path: &Path, base_url: &str, audit_path: &Path) {
    let contents = format!(
        r#"project:
  name: lean
  root: .
runtime:
  default_provider: fake-real
  max_turns: 12
events:
  format: jsonl
  audit_path: {}
providers:
  - name: fake-real
    type: openai-compatible
    model: fake-model
    api_key_env: LEAN_TEST_REAL_PROVIDER_KEY
    base_url: {base_url}
"#,
        audit_path.display()
    );
    fs::write(config_path, contents).expect("test real provider config should be writable");
}

fn write_builtin_provider_config(config_path: &Path, audit_path: &Path) {
    let contents = format!(
        r#"project:
  name: lean
  root: .
runtime:
  default_provider: mock
  max_turns: 12
events:
  format: jsonl
  audit_path: {}
"#,
        audit_path.display()
    );
    fs::write(config_path, contents).expect("test built-in provider config should be writable");
}

fn write_rig_provider_config(config_path: &Path, base_url: &str, audit_path: &Path) {
    let contents = format!(
        r#"project:
  name: lean
  root: .
runtime:
  default_provider: rig-openai
  max_turns: 12
events:
  format: jsonl
  audit_path: {}
providers:
  - name: rig-openai
    type: rig
    family: openai
    model: fake-model
    api_key_env: LEAN_TEST_RIG_PROVIDER_KEY
    base_url: {base_url}
"#,
        audit_path.display()
    );
    fs::write(config_path, contents).expect("test rig provider config should be writable");
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("lean-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("test temp directory should be creatable");
    dir
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
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
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
            .recv_timeout(std::time::Duration::from_secs(2))
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
