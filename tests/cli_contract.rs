use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use lean::events::{JsonlEvent, SESSION_RESULT};
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
    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
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
fn run_with_configured_audit_path_records_complete_jsonl_session() {
    let temp_dir = unique_temp_dir("audit-contract");
    let config_path = temp_dir.join("lean.yaml");
    let audit_path = temp_dir.join("audit").join("session.jsonl");
    write_config(&config_path, &audit_path);

    let output = Command::new(env!("CARGO_BIN_EXE_lean"))
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

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("lean-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("test temp directory should be creatable");
    dir
}
