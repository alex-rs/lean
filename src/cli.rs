use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "lean",
    version,
    about = "Lightweight Execution Agent Network",
    propagate_version = true
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        default_value = "lean.yaml",
        help = "Path to the LEAN configuration file"
    )]
    pub config: PathBuf,

    #[arg(long, global = true, help = "Emit machine-readable output")]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Commands {
    #[command(about = "Run a task through the configured agent harness")]
    Run(RunArgs),

    #[command(about = "Validate local configuration and environment")]
    Doctor(DoctorArgs),

    #[command(name = "list-skills", about = "List discovered agent skills")]
    ListSkills(ListArgs),

    #[command(name = "list-agents", about = "List available agent profiles")]
    ListAgents(ListArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RunArgs {
    #[arg(long, value_name = "TEXT", help = "Task prompt to execute")]
    pub task: String,

    #[arg(long, value_name = "NAME", help = "Provider adapter to use")]
    pub provider: Option<String>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DoctorArgs {}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ListArgs {}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::{Cli, Commands};

    #[test]
    fn help_lists_expected_subcommands() {
        let mut command = Cli::command();
        let help = command.render_long_help().to_string();

        for expected in ["run", "doctor", "list-skills", "list-agents"] {
            assert!(
                help.contains(expected),
                "top-level help should include {expected}"
            );
        }
    }

    #[test]
    fn parser_accepts_global_options_and_run_args() {
        let cli = Cli::try_parse_from([
            "lean",
            "--config",
            "fixtures/config/valid.yaml",
            "--json",
            "run",
            "--task",
            "noop",
            "--provider",
            "mock",
        ])
        .expect("run command should parse");

        assert_eq!(
            cli.config,
            std::path::PathBuf::from("fixtures/config/valid.yaml")
        );
        assert!(cli.json);
        assert_eq!(
            cli.command,
            Commands::Run(super::RunArgs {
                task: "noop".to_string(),
                provider: Some("mock".to_string()),
            })
        );
    }
}
