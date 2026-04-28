use std::process::ExitCode;

use clap::Parser;
use lean::{
    cli::{Cli, Commands, RunArgs},
    config::LeanConfig,
    events::JsonlEvent,
    provider::{MOCK_PROVIDER_NAME, MockProvider},
    session::{SessionRun, SessionRunner},
};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Run(args) => run_command(&cli, args),
        Commands::Doctor(_) => scaffold_command("doctor"),
        Commands::ListSkills(_) => scaffold_command("list-skills"),
        Commands::ListAgents(_) => scaffold_command("list-agents"),
    }
}

fn run_command(cli: &Cli, args: &RunArgs) -> ExitCode {
    let provider_name = match resolve_provider_name(cli, args) {
        Ok(provider_name) => provider_name,
        Err(message) => return exit_with_error(message),
    };

    if provider_name != MOCK_PROVIDER_NAME {
        return exit_with_error(format!("unsupported provider: {provider_name}"));
    }

    let mut runner = SessionRunner::new(MockProvider::default());
    let events = runner.run(SessionRun {
        task: args.task.clone(),
    });

    if cli.json {
        if let Err(message) = print_jsonl_events(&events) {
            return exit_with_error(message);
        }
    } else if let Some(JsonlEvent::SessionResult(result)) = events.last() {
        println!("{}", result.message);
    }

    ExitCode::SUCCESS
}

fn resolve_provider_name(cli: &Cli, args: &RunArgs) -> Result<String, String> {
    match &args.provider {
        Some(provider) => Ok(provider.clone()),
        None => LeanConfig::from_path(&cli.config)
            .map(|config| config.runtime.default_provider)
            .map_err(|error| error.to_string()),
    }
}

fn print_jsonl_events(events: &[JsonlEvent]) -> Result<(), String> {
    for event in events {
        let line = event
            .to_json_line()
            .map_err(|error| format!("failed to serialize event: {error}"))?;
        print!("{line}");
    }

    Ok(())
}

fn scaffold_command(command_name: &str) -> ExitCode {
    eprintln!("lean {command_name} is part of the scaffold and will be wired in a later task");
    ExitCode::from(2)
}

fn exit_with_error(message: String) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(2)
}
