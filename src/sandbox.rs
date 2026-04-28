use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

use thiserror::Error;

use crate::config::CommandConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPolicy {
    allowed_prefixes: Vec<CommandPrefix>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPrefix {
    parts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxRunner {
    workspace: PathBuf,
    policy: CommandPolicy,
    env_allowlist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxOutput {
    pub status_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CommandPolicyError {
    #[error("command argv must not be empty")]
    EmptyCommand,

    #[error("command prefix must not be empty")]
    EmptyPrefix,

    #[error("command prefix contains an empty argv part")]
    EmptyPrefixPart,

    #[error("command rejected before execution: {program} ({reason})")]
    WrapperRejected { program: String, reason: String },

    #[error("command is not allowed by argv prefix policy: {argv:?}")]
    NotAllowed { argv: Vec<String> },
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("workspace does not exist or is not a directory: {path}")]
    WorkspaceUnavailable { path: String },

    #[error(transparent)]
    Policy(#[from] CommandPolicyError),

    #[error("failed to execute command {argv:?}: {source}")]
    Execute {
        argv: Vec<String>,
        #[source]
        source: std::io::Error,
    },
}

impl CommandPolicy {
    pub fn new(prefixes: impl IntoIterator<Item = CommandPrefix>) -> Self {
        Self {
            allowed_prefixes: prefixes.into_iter().collect(),
        }
    }

    pub fn from_argv_prefixes(
        prefixes: impl IntoIterator<Item = impl IntoIterator<Item = impl Into<String>>>,
    ) -> Result<Self, CommandPolicyError> {
        let prefixes = prefixes
            .into_iter()
            .map(CommandPrefix::from_parts)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::new(prefixes))
    }

    pub fn check(&self, argv: &[String]) -> Result<(), CommandPolicyError> {
        if argv.is_empty() {
            return Err(CommandPolicyError::EmptyCommand);
        }

        reject_wrappers(argv)?;

        if self
            .allowed_prefixes
            .iter()
            .any(|prefix| prefix.matches(argv))
        {
            return Ok(());
        }

        Err(CommandPolicyError::NotAllowed {
            argv: argv.to_vec(),
        })
    }
}

impl TryFrom<&CommandConfig> for CommandPolicy {
    type Error = CommandPolicyError;

    fn try_from(config: &CommandConfig) -> Result<Self, Self::Error> {
        Self::from_argv_prefixes(config.allowed.iter().cloned())
    }
}

impl CommandPrefix {
    pub fn from_parts(
        parts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, CommandPolicyError> {
        let parts = parts.into_iter().map(Into::into).collect::<Vec<_>>();
        if parts.is_empty() {
            return Err(CommandPolicyError::EmptyPrefix);
        }

        if parts.iter().any(|part| part.trim().is_empty()) {
            return Err(CommandPolicyError::EmptyPrefixPart);
        }

        Ok(Self { parts })
    }

    fn matches(&self, argv: &[String]) -> bool {
        argv.len() >= self.parts.len()
            && self
                .parts
                .iter()
                .zip(argv.iter())
                .all(|(prefix, arg)| prefix == arg)
    }
}

impl SandboxRunner {
    pub fn new(workspace: impl Into<PathBuf>, policy: CommandPolicy) -> Self {
        Self {
            workspace: workspace.into(),
            policy,
            env_allowlist: vec!["PATH".to_string()],
        }
    }

    pub fn with_env_allowlist(mut self, env_allowlist: Vec<String>) -> Self {
        self.env_allowlist = env_allowlist;
        self
    }

    pub fn from_config(
        workspace: impl Into<PathBuf>,
        config: &CommandConfig,
    ) -> Result<Self, CommandPolicyError> {
        let policy = CommandPolicy::try_from(config)?;
        Ok(Self::new(workspace, policy).with_env_allowlist(config.env_allowlist.clone()))
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub fn run(&self, argv: &[String]) -> Result<SandboxOutput, SandboxError> {
        if !self.workspace.is_dir() {
            return Err(SandboxError::WorkspaceUnavailable {
                path: self.workspace.display().to_string(),
            });
        }

        self.policy.check(argv)?;
        let (program, args) = argv.split_first().ok_or(CommandPolicyError::EmptyCommand)?;

        let mut command = Command::new(program);
        command.args(args).current_dir(&self.workspace).env_clear();
        for name in &self.env_allowlist {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }

        let output = command.output().map_err(|source| SandboxError::Execute {
            argv: argv.to_vec(),
            source,
        })?;

        Ok(SandboxOutput {
            status_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn reject_wrappers(argv: &[String]) -> Result<(), CommandPolicyError> {
    let program = program_basename(&argv[0]);
    if matches!(
        program,
        "sh" | "bash" | "zsh" | "dash" | "fish" | "cmd" | "powershell" | "pwsh"
    ) {
        return Err(CommandPolicyError::WrapperRejected {
            program: program.to_string(),
            reason: "shell wrappers are not allowed".to_string(),
        });
    }

    if is_interpreter_eval(program, argv.get(1).map(String::as_str)) {
        return Err(CommandPolicyError::WrapperRejected {
            program: program.to_string(),
            reason: "interpreter one-liners are not allowed".to_string(),
        });
    }

    if matches!(
        program,
        "env" | "nohup" | "xargs" | "find" | "eval" | "script" | "setsid"
    ) {
        return Err(CommandPolicyError::WrapperRejected {
            program: program.to_string(),
            reason: "exec-smuggling helpers are not allowed".to_string(),
        });
    }

    Ok(())
}

fn is_interpreter_eval(program: &str, first_arg: Option<&str>) -> bool {
    matches!(
        (program, first_arg),
        ("python" | "python3", Some("-c"))
            | ("perl" | "ruby", Some("-e"))
            | ("node", Some("-e" | "--eval"))
    )
}

fn program_basename(program: &str) -> &str {
    Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(program)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::config::CommandConfig;

    use super::{CommandPolicy, CommandPolicyError, SandboxError, SandboxRunner};

    #[test]
    fn policy_permits_configured_argv_prefixes() {
        let policy =
            CommandPolicy::from_argv_prefixes([vec!["cargo", "test"], vec!["git", "status"]])
                .expect("policy should build");

        assert!(
            policy
                .check(&argv(["cargo", "test", "--", "filter"]))
                .is_ok()
        );
        assert!(policy.check(&argv(["git", "status", "--short"])).is_ok());
    }

    #[test]
    fn policy_rejects_empty_commands_and_empty_prefixes() {
        let policy =
            CommandPolicy::from_argv_prefixes([vec!["cargo"]]).expect("policy should build");
        assert!(matches!(
            policy.check(&[]),
            Err(CommandPolicyError::EmptyCommand)
        ));

        assert!(matches!(
            CommandPolicy::from_argv_prefixes([Vec::<&str>::new()]),
            Err(CommandPolicyError::EmptyPrefix)
        ));
    }

    #[test]
    fn policy_rejects_denied_prefixes() {
        let policy = CommandPolicy::from_argv_prefixes([vec!["cargo", "test"]])
            .expect("policy should build");

        let error = policy
            .check(&argv(["cargo", "run"]))
            .expect_err("unconfigured prefix should fail");

        assert!(
            matches!(error, CommandPolicyError::NotAllowed { .. }),
            "denied prefix should fail with policy error, got {error:?}"
        );
    }

    #[test]
    fn policy_rejects_shell_wrappers_and_interpreter_one_liners() {
        let policy = CommandPolicy::from_argv_prefixes([
            vec!["bash"],
            vec!["python", "-c"],
            vec!["perl", "-e"],
            vec!["ruby", "-e"],
            vec!["node", "-e"],
            vec!["env"],
        ])
        .expect("policy should build");

        for denied in [
            argv(["bash", "-c", "echo no"]),
            argv(["/bin/sh", "-c", "echo no"]),
            argv(["zsh", "-c", "echo no"]),
            argv(["python", "-c", "print(1)"]),
            argv(["python3", "-c", "print(1)"]),
            argv(["perl", "-e", "print 1"]),
            argv(["ruby", "-e", "puts 1"]),
            argv(["node", "--eval", "console.log(1)"]),
            argv(["env", "cargo", "test"]),
        ] {
            assert!(
                matches!(
                    policy.check(&denied),
                    Err(CommandPolicyError::WrapperRejected { .. })
                ),
                "wrapper command should be rejected: {denied:?}"
            );
        }
    }

    #[test]
    fn runner_fails_before_policy_when_workspace_is_missing() {
        let temp = unique_temp_dir("sandbox-missing");
        let missing = temp.join("missing");
        let policy = CommandPolicy::from_argv_prefixes([vec!["rustc", "--version"]])
            .expect("policy should build");
        let runner = SandboxRunner::new(missing, policy);

        let error = runner
            .run(&argv(["rustc", "--version"]))
            .expect_err("missing workspace should fail before execution");

        assert!(
            matches!(error, SandboxError::WorkspaceUnavailable { .. }),
            "missing workspace should fail with startup error, got {error:?}"
        );
    }

    #[test]
    fn runner_executes_allowed_command_in_workspace() {
        let workspace = unique_temp_dir("sandbox-run");
        let policy = CommandPolicy::from_argv_prefixes([vec!["rustc", "--version"]])
            .expect("policy should build");
        let runner = SandboxRunner::new(&workspace, policy);

        let output = runner
            .run(&argv(["rustc", "--version"]))
            .expect("allowed command should execute");

        assert_eq!(output.status_code, Some(0));
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("rustc"),
            "rustc version should be captured on stdout"
        );
    }

    #[test]
    fn runner_builds_from_configured_policy() {
        let workspace = unique_temp_dir("sandbox-config");
        let config = CommandConfig {
            allowed: vec![vec!["rustc".to_string(), "--version".to_string()]],
            env_allowlist: vec!["PATH".to_string()],
        };
        let runner = SandboxRunner::from_config(&workspace, &config).expect("runner should build");

        let output = runner
            .run(&argv(["rustc", "--version"]))
            .expect("configured command should execute");

        assert_eq!(output.status_code, Some(0));
    }

    fn argv<const N: usize>(parts: [&str; N]) -> Vec<String> {
        parts.into_iter().map(ToString::to_string).collect()
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
}
