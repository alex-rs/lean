use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

pub const DEFAULT_PROMPT_NAME: &str = "default";
const PROMPT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromptBundle {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub system: Vec<String>,
    pub tools: Vec<PromptTool>,
    pub tool_use_protocol: ToolUseProtocol,
    pub examples: Vec<PromptExample>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromptTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolUseProtocol {
    pub format: String,
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromptExample {
    pub name: String,
    pub user: String,
    pub assistant: String,
    pub tool_result: String,
    pub assistant_after_tool_result: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptStore {
    prompts_dir: PathBuf,
}

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("HOME is not set; cannot resolve ~/.lean/prompts")]
    HomeUnavailable,

    #[error("prompt name {name} is invalid")]
    InvalidPromptName { name: String },

    #[error("failed to create prompt directory {path}: {source}")]
    CreatePromptDir {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write default prompt {path}: {source}")]
    WriteDefaultPrompt {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read prompt {path}: {source}")]
    ReadPrompt {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse prompt {path}: {source}")]
    ParsePrompt {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("prompt {path} validation failed: {reason}")]
    Validation { path: String, reason: String },
}

impl PromptStore {
    pub fn from_current_user() -> Result<Self, PromptError> {
        let home = env::var_os("HOME")
            .filter(|value| !value.is_empty() && !value.to_string_lossy().trim().is_empty())
            .ok_or(PromptError::HomeUnavailable)?;
        Ok(Self::from_home_dir(home))
    }

    pub fn from_home_dir(home: impl AsRef<Path>) -> Self {
        Self {
            prompts_dir: home.as_ref().join(".lean").join("prompts"),
        }
    }

    pub fn with_prompts_dir(prompts_dir: impl AsRef<Path>) -> Self {
        Self {
            prompts_dir: prompts_dir.as_ref().to_path_buf(),
        }
    }

    pub fn prompts_dir(&self) -> &Path {
        &self.prompts_dir
    }

    pub fn prompt_path(&self, name: &str) -> Result<PathBuf, PromptError> {
        validate_prompt_name(name)?;
        Ok(self.prompts_dir.join(format!("{name}.json")))
    }

    pub fn load_or_create(&self, name: &str) -> Result<PromptBundle, PromptError> {
        let path = self.prompt_path(name)?;
        if !path.exists() && name == DEFAULT_PROMPT_NAME {
            self.write_default_prompt(&path)?;
        }

        self.load_path(&path)
    }

    pub fn load_path(&self, path: impl AsRef<Path>) -> Result<PromptBundle, PromptError> {
        let path = path.as_ref();
        let display_path = path.display().to_string();
        let contents = fs::read_to_string(path).map_err(|source| PromptError::ReadPrompt {
            path: display_path.clone(),
            source,
        })?;
        let bundle = serde_json::from_str::<PromptBundle>(&contents).map_err(|source| {
            PromptError::ParsePrompt {
                path: display_path.clone(),
                source,
            }
        })?;
        bundle.validate(&display_path)?;
        Ok(bundle)
    }

    fn write_default_prompt(&self, path: &Path) -> Result<(), PromptError> {
        fs::create_dir_all(&self.prompts_dir).map_err(|source| PromptError::CreatePromptDir {
            path: self.prompts_dir.display().to_string(),
            source,
        })?;

        let contents =
            serde_json::to_string_pretty(&default_prompt_bundle()).expect("default prompt is JSON");
        fs::write(path, format!("{contents}\n")).map_err(|source| PromptError::WriteDefaultPrompt {
            path: path.display().to_string(),
            source,
        })
    }
}

impl PromptBundle {
    pub fn render_system_prompt(&self) -> String {
        let mut output = String::new();
        output.push_str(&self.title);
        output.push_str("\n\n");

        output.push_str("System Instructions\n");
        for instruction in &self.system {
            output.push_str("- ");
            output.push_str(instruction);
            output.push('\n');
        }

        output.push_str("\nAvailable Tools\n");
        for tool in &self.tools {
            output.push_str("- ");
            output.push_str(&tool.name);
            output.push_str(": ");
            output.push_str(&tool.description);
            output.push('\n');
            output.push_str("  Input schema: ");
            output.push_str(
                &serde_json::to_string(&tool.input_schema)
                    .expect("prompt input schema should serialize"),
            );
            output.push('\n');
            output.push_str("  Result: ");
            output.push_str(&tool.result);
            output.push('\n');
        }

        output.push_str("\nTool Use Protocol\n");
        output.push_str("Format: ");
        output.push_str(&self.tool_use_protocol.format);
        output.push('\n');
        for instruction in &self.tool_use_protocol.instructions {
            output.push_str("- ");
            output.push_str(instruction);
            output.push('\n');
        }

        output.push_str("\nExamples\n");
        for example in &self.examples {
            output.push_str("Example: ");
            output.push_str(&example.name);
            output.push('\n');
            output.push_str("User: ");
            output.push_str(&example.user);
            output.push('\n');
            output.push_str("Assistant: ");
            output.push_str(&example.assistant);
            output.push('\n');
            output.push_str("Tool result: ");
            output.push_str(&example.tool_result);
            output.push('\n');
            output.push_str("Assistant after tool result: ");
            output.push_str(&example.assistant_after_tool_result);
            output.push('\n');
        }

        output
    }

    fn validate(&self, path: &str) -> Result<(), PromptError> {
        if self.schema_version != PROMPT_SCHEMA_VERSION {
            return validation_error(path, "schema_version must be 1");
        }

        validate_nonempty(path, "id", &self.id)?;
        validate_nonempty(path, "title", &self.title)?;
        validate_nonempty_list(path, "system", &self.system)?;
        validate_nonempty_list(
            path,
            "tool_use_protocol.instructions",
            &self.tool_use_protocol.instructions,
        )?;
        validate_nonempty(
            path,
            "tool_use_protocol.format",
            &self.tool_use_protocol.format,
        )?;

        if self.tools.is_empty() {
            return validation_error(path, "tools must not be empty");
        }

        let mut tool_names = BTreeSet::new();
        for tool in &self.tools {
            validate_tool_name(path, &tool.name)?;
            validate_nonempty(path, "tools.description", &tool.description)?;
            validate_nonempty(path, "tools.result", &tool.result)?;
            if !tool.input_schema.is_object() {
                return validation_error(path, "tools.input_schema must be a JSON object");
            }

            if !tool_names.insert(tool.name.clone()) {
                return validation_error(path, "tools.name must be unique");
            }
        }

        if self.examples.is_empty() {
            return validation_error(path, "examples must not be empty");
        }

        for example in &self.examples {
            validate_nonempty(path, "examples.name", &example.name)?;
            validate_nonempty(path, "examples.user", &example.user)?;
            validate_nonempty(path, "examples.assistant", &example.assistant)?;
            validate_nonempty(path, "examples.tool_result", &example.tool_result)?;
            validate_nonempty(
                path,
                "examples.assistant_after_tool_result",
                &example.assistant_after_tool_result,
            )?;
        }

        Ok(())
    }
}

pub fn default_prompt_bundle() -> PromptBundle {
    PromptBundle {
        schema_version: PROMPT_SCHEMA_VERSION,
        id: DEFAULT_PROMPT_NAME.to_string(),
        title: "LEAN Default Coding Agent".to_string(),
        system: vec![
            "You are LEAN, a headless coding-agent runtime operating inside a workspace managed by the host application.".to_string(),
            "Use tools only through the tool-use protocol when file context is required; do not invent file contents, directory listings, or tool results.".to_string(),
            "Treat paths as workspace-relative unless the host provides an absolute workspace path. Never request or reveal secrets, API keys, full request bodies, or raw provider responses.".to_string(),
            "When no tool is needed, answer directly and concisely. When a tool result is returned, use it to decide the next tool request or final answer.".to_string(),
        ],
        tools: vec![
            PromptTool {
                name: "read_file".to_string(),
                description: "Read UTF-8 text from a workspace file, optionally constrained to a 1-based inclusive line range.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["path"],
                    "properties": {
                        "path": { "type": "string", "description": "Workspace-relative file path." },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "end_line": { "type": "integer", "minimum": 1 }
                    }
                }),
                result: "JSON with path, content, start_line, and end_line.".to_string(),
            },
            PromptTool {
                name: "list_directory".to_string(),
                description: "List immediate entries in a workspace directory in deterministic name order.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["path"],
                    "properties": {
                        "path": { "type": "string", "description": "Workspace-relative directory path; use . for the workspace root." }
                    }
                }),
                result: "JSON with path and entries containing name, path, and kind.".to_string(),
            },
        ],
        tool_use_protocol: ToolUseProtocol {
            format: "single JSON object with a top-level tool_use field".to_string(),
            instructions: vec![
                "When requesting a tool, return only JSON and no surrounding prose.".to_string(),
                "Use this shape: {\"tool_use\":{\"name\":\"read_file\",\"arguments\":{\"path\":\"src/main.rs\"}}}.".to_string(),
                "Arguments must match the selected tool input schema exactly.".to_string(),
                "After receiving a tool_result from the host, continue with another tool_use request only if more context is needed; otherwise provide the final answer.".to_string(),
            ],
        },
        examples: vec![
            PromptExample {
                name: "Inspect a file before answering".to_string(),
                user: "Summarize src/main.rs.".to_string(),
                assistant: "{\"tool_use\":{\"name\":\"read_file\",\"arguments\":{\"path\":\"src/main.rs\"}}}".to_string(),
                tool_result: "{\"path\":\"/workspace/src/main.rs\",\"content\":\"fn main() {}\",\"start_line\":null,\"end_line\":null}".to_string(),
                assistant_after_tool_result: "src/main.rs defines an empty main function.".to_string(),
            },
            PromptExample {
                name: "List a directory before choosing a file".to_string(),
                user: "Find the entry point.".to_string(),
                assistant: "{\"tool_use\":{\"name\":\"list_directory\",\"arguments\":{\"path\":\"src\"}}}".to_string(),
                tool_result: "{\"path\":\"/workspace/src\",\"entries\":[{\"name\":\"main.rs\",\"path\":\"/workspace/src/main.rs\",\"kind\":\"file\"}]}".to_string(),
                assistant_after_tool_result: "{\"tool_use\":{\"name\":\"read_file\",\"arguments\":{\"path\":\"src/main.rs\"}}}".to_string(),
            },
        ],
    }
}

fn validate_prompt_name(name: &str) -> Result<(), PromptError> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(PromptError::InvalidPromptName {
            name: name.to_string(),
        });
    }

    Ok(())
}

fn validate_tool_name(path: &str, name: &str) -> Result<(), PromptError> {
    validate_nonempty(path, "tools.name", name)?;
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return validation_error(path, "tools.name contains unsupported characters");
    }

    Ok(())
}

fn validate_nonempty(path: &str, field: &'static str, value: &str) -> Result<(), PromptError> {
    if value.trim().is_empty() {
        return validation_error(path, format!("{field} must not be empty"));
    }

    Ok(())
}

fn validate_nonempty_list(
    path: &str,
    field: &'static str,
    values: &[String],
) -> Result<(), PromptError> {
    if values.is_empty() {
        return validation_error(path, format!("{field} must not be empty"));
    }

    if values.iter().any(|value| value.trim().is_empty()) {
        return validation_error(path, format!("{field} entries must not be empty"));
    }

    Ok(())
}

fn validation_error(path: &str, reason: impl Into<String>) -> Result<(), PromptError> {
    Err(PromptError::Validation {
        path: path.to_string(),
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{DEFAULT_PROMPT_NAME, PromptError, PromptStore, default_prompt_bundle};

    #[test]
    fn creates_and_loads_default_prompt_bundle_when_missing() {
        let home = unique_temp_dir("prompt-home");
        let store = PromptStore::from_home_dir(&home);

        let bundle = store
            .load_or_create(DEFAULT_PROMPT_NAME)
            .expect("default prompt should be created");

        assert_eq!(bundle.id, DEFAULT_PROMPT_NAME);
        assert!(
            home.join(".lean")
                .join("prompts")
                .join("default.json")
                .is_file()
        );
        assert!(bundle.render_system_prompt().contains("Available Tools"));
        assert!(bundle.render_system_prompt().contains("read_file"));
    }

    #[test]
    fn loads_user_edited_prompt_bundle_from_prompt_directory() {
        let home = unique_temp_dir("prompt-custom");
        let store = PromptStore::from_home_dir(&home);
        let custom_path = store
            .prompt_path("custom")
            .expect("prompt name should be valid");
        fs::create_dir_all(custom_path.parent().expect("prompt should have parent"))
            .expect("prompt parent should be created");

        let mut bundle = default_prompt_bundle();
        bundle.id = "custom".to_string();
        bundle.title = "Custom LEAN Prompt".to_string();
        bundle.system = vec!["Use the custom prompt.".to_string()];
        let contents = serde_json::to_string_pretty(&bundle).expect("custom prompt should encode");
        fs::write(&custom_path, contents).expect("custom prompt should be writable");

        let loaded = store
            .load_or_create("custom")
            .expect("custom prompt should load");

        assert_eq!(loaded.title, "Custom LEAN Prompt");
        assert!(
            loaded
                .render_system_prompt()
                .contains("Use the custom prompt.")
        );
    }

    #[test]
    fn rejects_invalid_prompt_name() {
        let store = PromptStore::with_prompts_dir(unique_temp_dir("prompt-invalid-name"));

        let error = store
            .load_or_create("../secret")
            .expect_err("path traversal should fail");

        assert!(matches!(error, PromptError::InvalidPromptName { .. }));

        let error = store
            .load_or_create("team.default")
            .expect_err("dot-separated prompt names should fail");

        assert!(matches!(error, PromptError::InvalidPromptName { .. }));
    }

    #[test]
    fn rejects_malformed_prompt_bundle() {
        let dir = unique_temp_dir("prompt-invalid-json");
        let path = dir.join("bad.json");
        fs::write(&path, r#"{"schema_version":1}"#).expect("bad prompt should be writable");
        let store = PromptStore::with_prompts_dir(&dir);

        let error = store
            .load_path(&path)
            .expect_err("missing fields should fail parsing");

        assert!(matches!(error, PromptError::ParsePrompt { .. }));
    }

    #[test]
    fn rejects_prompt_bundle_with_duplicate_tools() {
        let dir = unique_temp_dir("prompt-invalid-tools");
        let path = dir.join("bad.json");
        let mut bundle = default_prompt_bundle();
        bundle.tools.push(bundle.tools[0].clone());
        fs::write(
            &path,
            serde_json::to_string_pretty(&bundle).expect("bad prompt should encode"),
        )
        .expect("bad prompt should be writable");
        let store = PromptStore::with_prompts_dir(&dir);

        let error = store
            .load_path(&path)
            .expect_err("duplicate tools should fail validation");

        assert!(matches!(error, PromptError::Validation { .. }));
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
