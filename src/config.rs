use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeanConfig {
    pub project: ProjectConfig,
    pub runtime: RuntimeConfig,
    pub events: EventConfig,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub commands: CommandConfig,
    #[serde(default)]
    pub workspace: WorkspaceConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    pub default_provider: String,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventConfig {
    #[serde(default = "default_event_format")]
    pub format: EventFormat,
    #[serde(default)]
    pub audit_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ProviderKind,
    pub model: String,
    pub api_key_env: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    #[serde(rename = "openai-compatible")]
    OpenAiCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub worktree_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandConfig {
    #[serde(default)]
    pub allowed: Vec<Vec<String>>,
    #[serde(default = "default_env_allowlist")]
    pub env_allowlist: Vec<String>,
}

impl Default for CommandConfig {
    fn default() -> Self {
        Self {
            allowed: Vec::new(),
            env_allowlist: default_env_allowlist(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventFormat {
    Jsonl,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("config validation failed: {0}")]
    Validation(String),
}

impl LeanConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let display_path = path.display().to_string();
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: display_path.clone(),
            source,
        })?;
        Self::from_str_with_path(&contents, display_path)
    }

    pub fn from_yaml_str(contents: &str) -> Result<Self, ConfigError> {
        Self::from_str_with_path(contents, "<memory>".to_string())
    }

    fn from_str_with_path(contents: &str, path: String) -> Result<Self, ConfigError> {
        let config = serde_yaml::from_str::<Self>(contents)
            .map_err(|source| ConfigError::Parse { path, source })?;
        config.validate()
    }

    fn validate(self) -> Result<Self, ConfigError> {
        if self.project.name.trim().is_empty() {
            return Err(ConfigError::Validation(
                "project.name must not be empty".to_string(),
            ));
        }

        if self.project.root.trim().is_empty() {
            return Err(ConfigError::Validation(
                "project.root must not be empty".to_string(),
            ));
        }

        if self.runtime.default_provider.trim().is_empty() {
            return Err(ConfigError::Validation(
                "runtime.default_provider must not be empty".to_string(),
            ));
        }

        if self.runtime.max_turns == 0 {
            return Err(ConfigError::Validation(
                "runtime.max_turns must be greater than zero".to_string(),
            ));
        }

        let mut provider_names = BTreeSet::new();
        for provider in &self.providers {
            validate_provider_config(provider, &mut provider_names)?;
        }

        if self
            .workspace
            .worktree_root
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(ConfigError::Validation(
                "workspace.worktree_root must not be empty".to_string(),
            ));
        }

        for prefix in &self.commands.allowed {
            if prefix.is_empty() {
                return Err(ConfigError::Validation(
                    "commands.allowed entries must not be empty".to_string(),
                ));
            }

            if prefix.iter().any(|part| part.trim().is_empty()) {
                return Err(ConfigError::Validation(
                    "commands.allowed entries must not contain empty argv parts".to_string(),
                ));
            }
        }

        if self
            .commands
            .env_allowlist
            .iter()
            .any(|name| name.trim().is_empty())
        {
            return Err(ConfigError::Validation(
                "commands.env_allowlist entries must not be empty".to_string(),
            ));
        }

        Ok(self)
    }
}

fn validate_provider_config(
    provider: &ProviderConfig,
    provider_names: &mut BTreeSet<String>,
) -> Result<(), ConfigError> {
    if provider.name.trim().is_empty() {
        return Err(ConfigError::Validation(
            "providers.name must not be empty".to_string(),
        ));
    }

    if provider.name == "mock" {
        return Err(ConfigError::Validation(
            "providers.name must not use reserved provider name mock".to_string(),
        ));
    }

    if !provider_names.insert(provider.name.clone()) {
        return Err(ConfigError::Validation(format!(
            "providers.name must be unique: {}",
            provider.name
        )));
    }

    if provider.model.trim().is_empty() {
        return Err(ConfigError::Validation(
            "providers.model must not be empty".to_string(),
        ));
    }

    if !is_valid_env_name(&provider.api_key_env) {
        return Err(ConfigError::Validation(
            "providers.api_key_env must be a valid environment variable name".to_string(),
        ));
    }

    if let Some(base_url) = &provider.base_url {
        let trimmed = base_url.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::Validation(
                "providers.base_url must not be empty".to_string(),
            ));
        }

        if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
            return Err(ConfigError::Validation(
                "providers.base_url must start with http:// or https://".to_string(),
            ));
        }
    }

    Ok(())
}

fn is_valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn default_max_turns() -> u32 {
    20
}

fn default_event_format() -> EventFormat {
    EventFormat::Jsonl
}

fn default_env_allowlist() -> Vec<String> {
    vec!["PATH".to_string()]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ConfigError, EventFormat, LeanConfig};

    #[test]
    fn parses_valid_config_fixture() {
        let config =
            LeanConfig::from_path(fixture("valid.yaml")).expect("valid fixture should parse");

        assert_eq!(config.project.name, "lean");
        assert_eq!(config.runtime.default_provider, "mock");
        assert_eq!(config.runtime.max_turns, 12);
        assert_eq!(config.events.format, EventFormat::Jsonl);
        assert_eq!(config.events.audit_path, None);
        assert!(config.providers.is_empty());
        assert_eq!(config.commands.allowed, Vec::<Vec<String>>::new());
        assert_eq!(config.commands.env_allowlist, vec!["PATH".to_string()]);
        assert_eq!(config.workspace.worktree_root, None);
    }

    #[test]
    fn parses_optional_audit_path() {
        let config = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: mock
events:
  format: jsonl
  audit_path: target/lean-audit.jsonl
"#,
        )
        .expect("config with audit path should parse");

        assert_eq!(
            config.events.audit_path,
            Some(PathBuf::from("target/lean-audit.jsonl"))
        );
    }

    #[test]
    fn rejects_invalid_config_fixture() {
        let error = LeanConfig::from_path(fixture("invalid.yaml"))
            .expect_err("invalid fixture should fail validation");

        assert!(
            matches!(error, ConfigError::Validation(_)),
            "invalid fixture should fail validation, got {error:?}"
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: mock
events:
  format: jsonl
extra: value
"#,
        )
        .expect_err("unknown fields should fail parsing");

        assert!(
            matches!(error, ConfigError::Parse { .. }),
            "unknown field should be a parse error, got {error:?}"
        );
    }

    #[test]
    fn parses_optional_worktree_root() {
        let config = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: mock
events:
  format: jsonl
workspace:
  worktree_root: ../.lean-worktrees
"#,
        )
        .expect("config with workspace root should parse");

        assert_eq!(
            config.workspace.worktree_root,
            Some(PathBuf::from("../.lean-worktrees"))
        );
    }

    #[test]
    fn rejects_empty_worktree_root() {
        let error = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: mock
events:
  format: jsonl
workspace:
  worktree_root: ""
"#,
        )
        .expect_err("empty workspace root should fail validation");

        assert!(
            matches!(error, ConfigError::Validation(_)),
            "empty workspace root should fail validation, got {error:?}"
        );
    }

    #[test]
    fn parses_command_policy_config() {
        let config = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: mock
events:
  format: jsonl
commands:
  allowed:
    - ["cargo", "test"]
    - ["git", "status"]
  env_allowlist:
    - PATH
    - RUST_LOG
"#,
        )
        .expect("config with command policy should parse");

        assert_eq!(
            config.commands.allowed,
            vec![
                vec!["cargo".to_string(), "test".to_string()],
                vec!["git".to_string(), "status".to_string()],
            ]
        );
        assert_eq!(
            config.commands.env_allowlist,
            vec!["PATH".to_string(), "RUST_LOG".to_string()]
        );
    }

    #[test]
    fn rejects_empty_command_prefixes() {
        let error = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: mock
events:
  format: jsonl
commands:
  allowed:
    - []
"#,
        )
        .expect_err("empty command prefixes should fail validation");

        assert!(
            matches!(error, ConfigError::Validation(_)),
            "empty command prefix should fail validation, got {error:?}"
        );
    }

    #[test]
    fn rejects_empty_env_allowlist_entries() {
        let error = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: mock
events:
  format: jsonl
commands:
  env_allowlist:
    - ""
"#,
        )
        .expect_err("empty env allowlist entries should fail validation");

        assert!(
            matches!(error, ConfigError::Validation(_)),
            "empty env allowlist entry should fail validation, got {error:?}"
        );
    }

    #[test]
    fn parses_real_provider_config() {
        let config = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: minimax
events:
  format: jsonl
providers:
  - name: minimax
    type: openai-compatible
    model: MiniMax-M2.7
    api_key_env: MINIMAX_API_KEY
    base_url: https://api.minimax.io/v1
"#,
        )
        .expect("config with real provider should parse");

        assert_eq!(config.providers.len(), 1);
        let provider = &config.providers[0];
        assert_eq!(provider.name, "minimax");
        assert_eq!(provider.model, "MiniMax-M2.7");
        assert_eq!(provider.api_key_env, "MINIMAX_API_KEY");
        assert_eq!(
            provider.base_url,
            Some("https://api.minimax.io/v1".to_string())
        );
    }

    #[test]
    fn rejects_invalid_provider_config() {
        let error = LeanConfig::from_yaml_str(
            r#"
project:
  name: lean
  root: .
runtime:
  default_provider: bad
events:
  format: jsonl
providers:
  - name: bad
    type: openai-compatible
    model: ""
    api_key_env: "not-valid"
"#,
        )
        .expect_err("invalid provider config should fail validation");

        assert!(
            matches!(error, ConfigError::Validation(_)),
            "invalid provider config should fail validation, got {error:?}"
        );
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/config")
            .join(name)
    }
}
