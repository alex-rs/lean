use std::process::Command;

use lean::events::{JsonlEvent, SESSION_RESULT};

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
