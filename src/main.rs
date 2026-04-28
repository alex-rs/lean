use std::process::ExitCode;

use clap::Parser;
use lean::cli::{Cli, Commands};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command_name = match &cli.command {
        Commands::Run(_) => "run",
        Commands::Doctor(_) => "doctor",
        Commands::ListSkills(_) => "list-skills",
        Commands::ListAgents(_) => "list-agents",
    };

    eprintln!("lean {command_name} is part of the scaffold and will be wired in a later task");
    ExitCode::from(2)
}
