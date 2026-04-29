use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantTurn {
    FinalAnswer(String),
    ToolUse(ToolUseRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolUseRequest {
    ReadFile(ReadFileToolUse),
    ListDirectory(ListDirectoryToolUse),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFileToolUse {
    pub path: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDirectoryToolUse {
    pub path: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ToolUseParseError {
    #[error("assistant requested multiple tool uses; exactly one is supported")]
    MultipleToolUses,

    #[error("malformed tool-use request: {reason}")]
    MalformedRequest { reason: &'static str },

    #[error("unknown tool requested")]
    UnknownTool,

    #[error("invalid arguments for tool {tool}: {reason}")]
    InvalidArguments {
        tool: &'static str,
        reason: &'static str,
    },
}

impl AssistantTurn {
    pub fn parse(text: impl AsRef<str>) -> Result<Self, ToolUseParseError> {
        parse_assistant_turn(text.as_ref())
    }
}

impl ToolUseRequest {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ReadFile(_) => "read_file",
            Self::ListDirectory(_) => "list_directory",
        }
    }
}

pub fn parse_assistant_turn(text: &str) -> Result<AssistantTurn, ToolUseParseError> {
    let trimmed = text.trim();
    let value = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) if looks_like_malformed_tool_use(trimmed) => {
            return Err(ToolUseParseError::MalformedRequest {
                reason: "tool-use JSON is invalid",
            });
        }
        Err(_) => return Ok(AssistantTurn::FinalAnswer(text.to_string())),
    };

    parse_json_assistant_turn(value).map(|parsed| match parsed {
        Some(tool_use) => AssistantTurn::ToolUse(tool_use),
        None => AssistantTurn::FinalAnswer(text.to_string()),
    })
}

fn parse_json_assistant_turn(value: Value) -> Result<Option<ToolUseRequest>, ToolUseParseError> {
    match value {
        Value::Object(object) => {
            if object.contains_key("tool_uses") {
                return Err(ToolUseParseError::MultipleToolUses);
            }

            let Some(tool_use) = object.get("tool_use") else {
                return Ok(None);
            };

            if matches!(tool_use, Value::Array(_)) {
                return Err(ToolUseParseError::MultipleToolUses);
            }

            if object.len() != 1 {
                return Err(ToolUseParseError::MalformedRequest {
                    reason: "tool-use object must contain only tool_use",
                });
            }

            parse_tool_use(tool_use.clone()).map(Some)
        }
        Value::Array(values) if values.iter().any(value_mentions_tool_use) => {
            Err(ToolUseParseError::MultipleToolUses)
        }
        _ => Ok(None),
    }
}

fn parse_tool_use(value: Value) -> Result<ToolUseRequest, ToolUseParseError> {
    let raw = serde_json::from_value::<RawToolUse>(value).map_err(|_| {
        ToolUseParseError::MalformedRequest {
            reason: "tool_use must contain name and arguments",
        }
    })?;

    match raw.name.as_str() {
        "read_file" => parse_read_file(raw.arguments).map(ToolUseRequest::ReadFile),
        "list_directory" => parse_list_directory(raw.arguments).map(ToolUseRequest::ListDirectory),
        _ => Err(ToolUseParseError::UnknownTool),
    }
}

fn parse_read_file(value: Value) -> Result<ReadFileToolUse, ToolUseParseError> {
    let arguments = serde_json::from_value::<ReadFileArguments>(value).map_err(|_| {
        ToolUseParseError::InvalidArguments {
            tool: "read_file",
            reason: "arguments must match read_file schema",
        }
    })?;

    validate_nonempty_path("read_file", &arguments.path)?;
    validate_line_range(arguments.start_line, arguments.end_line)?;

    Ok(ReadFileToolUse {
        path: arguments.path,
        start_line: arguments.start_line,
        end_line: arguments.end_line,
    })
}

fn parse_list_directory(value: Value) -> Result<ListDirectoryToolUse, ToolUseParseError> {
    let arguments = serde_json::from_value::<ListDirectoryArguments>(value).map_err(|_| {
        ToolUseParseError::InvalidArguments {
            tool: "list_directory",
            reason: "arguments must match list_directory schema",
        }
    })?;

    validate_nonempty_path("list_directory", &arguments.path)?;

    Ok(ListDirectoryToolUse {
        path: arguments.path,
    })
}

fn validate_nonempty_path(tool: &'static str, path: &str) -> Result<(), ToolUseParseError> {
    if path.trim().is_empty() {
        return Err(ToolUseParseError::InvalidArguments {
            tool,
            reason: "path must not be empty",
        });
    }

    Ok(())
}

fn validate_line_range(
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<(), ToolUseParseError> {
    if matches!(start_line, Some(0)) || matches!(end_line, Some(0)) {
        return Err(ToolUseParseError::InvalidArguments {
            tool: "read_file",
            reason: "line ranges are 1-based",
        });
    }

    if let (Some(start), Some(end)) = (start_line, end_line)
        && start > end
    {
        return Err(ToolUseParseError::InvalidArguments {
            tool: "read_file",
            reason: "start_line must be less than or equal to end_line",
        });
    }

    Ok(())
}

fn value_mentions_tool_use(value: &Value) -> bool {
    matches!(
        value,
        Value::Object(object) if object.contains_key("tool_use") || object.contains_key("tool_uses")
    )
}

fn looks_like_malformed_tool_use(trimmed: &str) -> bool {
    (trimmed.starts_with('{') || trimmed.starts_with('[')) && trimmed.contains("tool_use")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawToolUse {
    name: String,
    arguments: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadFileArguments {
    path: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    end_line: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListDirectoryArguments {
    path: String,
}

#[cfg(test)]
mod tests {
    use super::{
        AssistantTurn, ListDirectoryToolUse, ReadFileToolUse, ToolUseParseError, ToolUseRequest,
        parse_assistant_turn,
    };

    #[test]
    fn parses_valid_read_file_tool_use() {
        let parsed = parse_assistant_turn(
            r#"{"tool_use":{"name":"read_file","arguments":{"path":"src/main.rs","start_line":2,"end_line":5}}}"#,
        )
        .expect("read_file tool use should parse");

        assert_eq!(
            parsed,
            AssistantTurn::ToolUse(ToolUseRequest::ReadFile(ReadFileToolUse {
                path: "src/main.rs".to_string(),
                start_line: Some(2),
                end_line: Some(5),
            }))
        );
    }

    #[test]
    fn parses_valid_list_directory_tool_use() {
        let parsed = parse_assistant_turn(
            r#"{"tool_use":{"name":"list_directory","arguments":{"path":"src"}}}"#,
        )
        .expect("list_directory tool use should parse");

        let AssistantTurn::ToolUse(tool_use) = parsed else {
            panic!("assistant turn should be a tool use");
        };

        assert_eq!(tool_use.name(), "list_directory");
        assert_eq!(
            tool_use,
            ToolUseRequest::ListDirectory(ListDirectoryToolUse {
                path: "src".to_string(),
            })
        );
    }

    #[test]
    fn rejects_unknown_tool_without_echoing_raw_name() {
        let error = parse_assistant_turn(
            r#"{"tool_use":{"name":"secret-token-write_file","arguments":{"path":"src/main.rs"}}}"#,
        )
        .expect_err("unknown tool should fail");

        assert_eq!(error, ToolUseParseError::UnknownTool);
        assert!(!error.to_string().contains("secret-token-write_file"));
    }

    #[test]
    fn rejects_invalid_read_file_arguments() {
        let error = parse_assistant_turn(
            r#"{"tool_use":{"name":"read_file","arguments":{"path":"src/main.rs","start_line":0}}}"#,
        )
        .expect_err("zero line should fail");

        assert_eq!(
            error,
            ToolUseParseError::InvalidArguments {
                tool: "read_file",
                reason: "line ranges are 1-based",
            }
        );
    }

    #[test]
    fn rejects_extra_argument_fields() {
        let error = parse_assistant_turn(
            r#"{"tool_use":{"name":"list_directory","arguments":{"path":"src","recursive":true}}}"#,
        )
        .expect_err("unknown argument should fail");

        assert_eq!(
            error,
            ToolUseParseError::InvalidArguments {
                tool: "list_directory",
                reason: "arguments must match list_directory schema",
            }
        );
    }

    #[test]
    fn rejects_multiple_tool_use_requests() {
        let error = parse_assistant_turn(
            r#"{"tool_use":[{"name":"read_file","arguments":{"path":"src/main.rs"}},{"name":"list_directory","arguments":{"path":"src"}}]}"#,
        )
        .expect_err("multiple tool uses should fail");

        assert_eq!(error, ToolUseParseError::MultipleToolUses);
    }

    #[test]
    fn rejects_malformed_tool_use_json_without_echoing_content() {
        let error = parse_assistant_turn(
            r#"{"tool_use":{"name":"read_file","arguments":{"path":"secret request body"}"#,
        )
        .expect_err("malformed tool-use JSON should fail");

        assert_eq!(
            error,
            ToolUseParseError::MalformedRequest {
                reason: "tool-use JSON is invalid",
            }
        );
        assert!(!error.to_string().contains("secret request body"));
    }

    #[test]
    fn leaves_normal_final_answer_text_unchanged() {
        let answer = "\nDone. No tool needed.\n";

        let parsed = parse_assistant_turn(answer).expect("normal answer should parse");

        assert_eq!(parsed, AssistantTurn::FinalAnswer(answer.to_string()));
    }

    #[test]
    fn leaves_non_tool_json_answers_unchanged() {
        let answer = r#"{"answer":"Done without tools."}"#;

        let parsed = AssistantTurn::parse(answer).expect("normal JSON answer should parse");

        assert_eq!(parsed, AssistantTurn::FinalAnswer(answer.to_string()));
    }
}
