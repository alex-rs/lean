use std::{
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

        Ok(self)
    }
}

fn default_max_turns() -> u32 {
    20
}

fn default_event_format() -> EventFormat {
    EventFormat::Jsonl
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

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/config")
            .join(name)
    }
}
