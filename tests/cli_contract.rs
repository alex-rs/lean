use std::process::Command;

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
