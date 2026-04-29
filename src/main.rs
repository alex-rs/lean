use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use lean::{
    audit::AuditWriter,
    catalog::{built_in_agents, built_in_skills},
    cli::{Cli, Commands, RunArgs},
    config::{ConfigError, LeanConfig},
    doctor::run_doctor,
    events::{CredentialAccessed, JsonlEvent},
    prompts::PromptStore,
    provider::{CredentialAccess, ProviderRegistry},
    session::{SessionRun, SessionRunner},
};
use serde::Serialize;

const DEFAULT_CONFIG_PATH: &str = "lean.yaml";
const DEFAULT_CREDENTIAL_AUDIT_PATH: &str = "target/lean-audit.jsonl";

fn main() -> ExitCode {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Run(args) => run_command(&cli, args),
        Commands::Doctor(_) => doctor_command(&cli),
        Commands::ListSkills(_) => list_skills_command(&cli),
        Commands::ListAgents(_) => list_agents_command(&cli),
    }
}

fn run_command(cli: &Cli, args: &RunArgs) -> ExitCode {
    let config = match load_config_for_run(cli, args) {
        Ok(config) => config,
        Err(message) => return exit_with_error(message),
    };
    let provider_name = resolve_provider_name(args, config.as_ref());
    let prompt_bundle = match PromptStore::from_current_user()
        .and_then(|store| store.load_or_create(&args.prompt))
    {
        Ok(bundle) => bundle,
        Err(error) => return exit_with_error(error.to_string()),
    };
    let resolved_provider =
        match ProviderRegistry::from_config(config.as_ref()).resolve_with_audit(&provider_name) {
            Ok(provider) => provider,
            Err(error) => return exit_with_error(error.to_string()),
        };

    if let Some(access) = resolved_provider.credential_access() {
        if let Err(error) = write_credential_audit(access, config.as_ref()) {
            return exit_with_error(error);
        }
    }

    let mut runner = SessionRunner::new(resolved_provider.into_provider());
    let events = runner.run(SessionRun {
        task: args.task.clone(),
        system_prompt: Some(prompt_bundle.render_system_prompt()),
    });

    if let Some(audit_path) = config
        .as_ref()
        .and_then(|config| config.events.audit_path.as_ref())
    {
        if let Err(error) = AuditWriter::new(audit_path).write_events(&events) {
            return exit_with_error(error.to_string());
        }
    }

    if cli.json {
        if let Err(message) = print_jsonl_events(&events) {
            return exit_with_error(message);
        }
    } else if let Some(JsonlEvent::SessionResult(result)) = events.last() {
        println!("{}", result.message);
    }

    ExitCode::SUCCESS
}

fn write_credential_audit(
    access: &CredentialAccess,
    config: Option<&LeanConfig>,
) -> Result<(), String> {
    let audit_path = credential_audit_path(config);
    let event = JsonlEvent::CredentialAccessed(CredentialAccessed {
        provider: access.provider.clone(),
        env_var: access.env_var.clone(),
    });

    AuditWriter::new(audit_path)
        .write_events(&[event])
        .map_err(|error| error.to_string())
}

fn credential_audit_path(config: Option<&LeanConfig>) -> PathBuf {
    config
        .and_then(|config| config.events.audit_path.clone())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CREDENTIAL_AUDIT_PATH))
}

fn load_config_for_run(cli: &Cli, args: &RunArgs) -> Result<Option<LeanConfig>, String> {
    match LeanConfig::from_path(&cli.config) {
        Ok(config) => Ok(Some(config)),
        Err(error) if can_skip_missing_default_config(cli, args, &error) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn can_skip_missing_default_config(cli: &Cli, args: &RunArgs, error: &ConfigError) -> bool {
    args.provider.is_some()
        && cli.config.as_path() == std::path::Path::new(DEFAULT_CONFIG_PATH)
        && matches!(
            error,
            ConfigError::Read { source, .. }
                if source.kind() == std::io::ErrorKind::NotFound
        )
}

fn resolve_provider_name(args: &RunArgs, config: Option<&LeanConfig>) -> String {
    args.provider
        .clone()
        .or_else(|| config.map(|config| config.runtime.default_provider.clone()))
        .expect("config is required when provider is omitted")
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

fn doctor_command(cli: &Cli) -> ExitCode {
    let report = run_doctor(&cli.config);
    if let Err(message) = print_json(&report) {
        return exit_with_error(message);
    }

    if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn list_skills_command(cli: &Cli) -> ExitCode {
    let skills = built_in_skills();
    if cli.json {
        if let Err(message) = print_json(&skills) {
            return exit_with_error(message);
        }
    } else {
        for skill in skills {
            println!("{}", skill.id);
        }
    }

    ExitCode::SUCCESS
}

fn list_agents_command(cli: &Cli) -> ExitCode {
    let roster = built_in_agents();
    if cli.json {
        if let Err(message) = print_json(&roster) {
            return exit_with_error(message);
        }
    } else {
        for agent in roster.agents {
            println!("{}", agent.id);
        }
    }

    ExitCode::SUCCESS
}

fn print_json(value: &impl Serialize) -> Result<(), String> {
    let output = serde_json::to_string(value)
        .map_err(|error| format!("failed to serialize structured output: {error}"))?;
    println!("{output}");
    Ok(())
}

fn exit_with_error(message: String) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(2)
}
