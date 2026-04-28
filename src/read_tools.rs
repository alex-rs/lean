use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadTools {
    workspace: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReadFileRange {
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadFileOutput {
    pub path: PathBuf,
    pub content: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub path: PathBuf,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: DirectoryEntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Error)]
pub enum ReadToolError {
    #[error("failed to resolve workspace {path}: {source}")]
    ResolveWorkspace {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to resolve path {path}: {source}")]
    ResolvePath {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("path {path} escapes workspace {workspace}")]
    EscapesWorkspace { path: String, workspace: String },

    #[error("path is not a file: {path}")]
    NotFile { path: String },

    #[error("path is not a directory: {path}")]
    NotDirectory { path: String },

    #[error("failed to read file {path}: {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read directory {path}: {source}")]
    ReadDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to inspect directory entry in {path}: {source}")]
    ReadDirectoryEntry {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("line ranges are 1-based and start_line must be less than or equal to end_line")]
    InvalidLineRange,

    #[error("line range {start_line}..{end_line} exceeds file length {line_count}")]
    LineRangeOutOfBounds {
        start_line: usize,
        end_line: usize,
        line_count: usize,
    },
}

impl ReadTools {
    pub fn new(workspace: impl AsRef<Path>) -> Result<Self, ReadToolError> {
        let workspace = workspace.as_ref();
        let workspace =
            workspace
                .canonicalize()
                .map_err(|source| ReadToolError::ResolveWorkspace {
                    path: workspace.display().to_string(),
                    source,
                })?;

        if !workspace.is_dir() {
            return Err(ReadToolError::NotDirectory {
                path: workspace.display().to_string(),
            });
        }

        Ok(Self { workspace })
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub fn read_file(
        &self,
        path: impl AsRef<Path>,
        range: ReadFileRange,
    ) -> Result<ReadFileOutput, ReadToolError> {
        let resolved = self.resolve_workspace_path(path.as_ref())?;
        if !resolved.is_file() {
            return Err(ReadToolError::NotFile {
                path: resolved.display().to_string(),
            });
        }

        let content = fs::read_to_string(&resolved).map_err(|source| ReadToolError::ReadFile {
            path: resolved.display().to_string(),
            source,
        })?;
        let content = apply_line_range(&content, range)?;

        Ok(ReadFileOutput {
            path: resolved,
            content,
            start_line: range.start_line,
            end_line: range.end_line,
        })
    }

    pub fn list_directory(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<DirectoryListing, ReadToolError> {
        let resolved = self.resolve_workspace_path(path.as_ref())?;
        if !resolved.is_dir() {
            return Err(ReadToolError::NotDirectory {
                path: resolved.display().to_string(),
            });
        }

        let entries = fs::read_dir(&resolved)
            .map_err(|source| ReadToolError::ReadDirectory {
                path: resolved.display().to_string(),
                source,
            })?
            .map(|entry| directory_entry(&resolved, entry))
            .collect::<Result<Vec<_>, _>>()?;

        let mut entries = entries;
        entries.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(DirectoryListing {
            path: resolved,
            entries,
        })
    }

    fn resolve_workspace_path(&self, path: &Path) -> Result<PathBuf, ReadToolError> {
        let joined = if path.as_os_str().is_empty() {
            self.workspace.clone()
        } else if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace.join(path)
        };
        let resolved = joined
            .canonicalize()
            .map_err(|source| ReadToolError::ResolvePath {
                path: joined.display().to_string(),
                source,
            })?;

        if !resolved.starts_with(&self.workspace) {
            return Err(ReadToolError::EscapesWorkspace {
                path: resolved.display().to_string(),
                workspace: self.workspace.display().to_string(),
            });
        }

        Ok(resolved)
    }
}

fn directory_entry(
    parent: &Path,
    entry: Result<fs::DirEntry, std::io::Error>,
) -> Result<DirectoryEntry, ReadToolError> {
    let entry = entry.map_err(|source| ReadToolError::ReadDirectoryEntry {
        path: parent.display().to_string(),
        source,
    })?;
    let file_type = entry
        .file_type()
        .map_err(|source| ReadToolError::ReadDirectoryEntry {
            path: parent.display().to_string(),
            source,
        })?;
    let name = entry.file_name().to_string_lossy().to_string();

    let kind = if file_type.is_symlink() {
        DirectoryEntryKind::Symlink
    } else if file_type.is_dir() {
        DirectoryEntryKind::Directory
    } else if file_type.is_file() {
        DirectoryEntryKind::File
    } else {
        DirectoryEntryKind::Other
    };

    Ok(DirectoryEntry {
        path: parent.join(&name),
        name,
        kind,
    })
}

fn apply_line_range(content: &str, range: ReadFileRange) -> Result<String, ReadToolError> {
    match (range.start_line, range.end_line) {
        (None, None) => Ok(content.to_string()),
        (Some(0), _) | (_, Some(0)) => Err(ReadToolError::InvalidLineRange),
        (Some(start), Some(end)) if start > end => Err(ReadToolError::InvalidLineRange),
        _ => {
            let lines = content.lines().collect::<Vec<_>>();
            let start = range.start_line.unwrap_or(1);
            let end = range.end_line.unwrap_or(lines.len());
            if start > lines.len() || end > lines.len() {
                return Err(ReadToolError::LineRangeOutOfBounds {
                    start_line: start,
                    end_line: end,
                    line_count: lines.len(),
                });
            }

            Ok(lines[start - 1..end].join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{DirectoryEntryKind, ReadFileRange, ReadToolError, ReadTools, apply_line_range};

    #[test]
    fn read_file_returns_complete_file_content() {
        let workspace = unique_temp_dir("read-file");
        fs::write(workspace.join("notes.txt"), "one\ntwo\nthree\n")
            .expect("fixture should be writable");
        let tools = ReadTools::new(&workspace).expect("read tools should build");

        let output = tools
            .read_file("notes.txt", ReadFileRange::default())
            .expect("file should be readable");

        assert_eq!(output.content, "one\ntwo\nthree\n");
        assert_eq!(
            output.path,
            workspace.join("notes.txt").canonicalize().unwrap()
        );
    }

    #[test]
    fn read_file_returns_inclusive_line_range() {
        let workspace = unique_temp_dir("read-range");
        fs::write(workspace.join("notes.txt"), "one\ntwo\nthree\n")
            .expect("fixture should be writable");
        let tools = ReadTools::new(&workspace).expect("read tools should build");

        let output = tools
            .read_file(
                "notes.txt",
                ReadFileRange {
                    start_line: Some(2),
                    end_line: Some(3),
                },
            )
            .expect("line range should be readable");

        assert_eq!(output.content, "two\nthree");
        assert_eq!(output.start_line, Some(2));
        assert_eq!(output.end_line, Some(3));
    }

    #[test]
    fn line_ranges_are_validated() {
        assert!(matches!(
            apply_line_range(
                "one\ntwo\n",
                ReadFileRange {
                    start_line: Some(0),
                    end_line: Some(1),
                },
            ),
            Err(ReadToolError::InvalidLineRange)
        ));
        assert!(matches!(
            apply_line_range(
                "one\ntwo\n",
                ReadFileRange {
                    start_line: Some(2),
                    end_line: Some(1),
                },
            ),
            Err(ReadToolError::InvalidLineRange)
        ));
        assert!(matches!(
            apply_line_range(
                "one\ntwo\n",
                ReadFileRange {
                    start_line: Some(3),
                    end_line: Some(3),
                },
            ),
            Err(ReadToolError::LineRangeOutOfBounds { .. })
        ));
    }

    #[test]
    fn list_directory_returns_sorted_entries() {
        let workspace = unique_temp_dir("list-dir");
        fs::write(workspace.join("zeta.txt"), "z").expect("file should be writable");
        fs::create_dir(workspace.join("alpha")).expect("directory should be creatable");
        fs::write(workspace.join("middle.txt"), "m").expect("file should be writable");
        let tools = ReadTools::new(&workspace).expect("read tools should build");

        let listing = tools
            .list_directory("")
            .expect("workspace directory should be listable");

        let names = listing
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["alpha", "middle.txt", "zeta.txt"]);
        assert_eq!(listing.entries[0].kind, DirectoryEntryKind::Directory);
        assert_eq!(listing.entries[1].kind, DirectoryEntryKind::File);
    }

    #[test]
    fn path_traversal_outside_workspace_is_rejected() {
        let temp = unique_temp_dir("traversal");
        let workspace = temp.join("workspace");
        let outside = temp.join("outside.txt");
        fs::create_dir(&workspace).expect("workspace should be creatable");
        fs::write(&outside, "outside").expect("outside fixture should be writable");
        let tools = ReadTools::new(&workspace).expect("read tools should build");

        let error = tools
            .read_file("../outside.txt", ReadFileRange::default())
            .expect_err("outside traversal should be rejected");

        assert!(
            matches!(error, ReadToolError::EscapesWorkspace { .. }),
            "outside traversal should fail with workspace escape, got {error:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let temp = unique_temp_dir("symlink-escape");
        let workspace = temp.join("workspace");
        fs::create_dir(&workspace).expect("workspace should be creatable");
        let outside = temp.join("outside.txt");
        fs::write(&outside, "outside").expect("outside fixture should be writable");
        symlink(&outside, workspace.join("outside-link")).expect("symlink should be creatable");
        let tools = ReadTools::new(&workspace).expect("read tools should build");

        let error = tools
            .read_file("outside-link", ReadFileRange::default())
            .expect_err("symlink escape should be rejected");

        assert!(
            matches!(error, ReadToolError::EscapesWorkspace { .. }),
            "symlink escape should fail with workspace escape, got {error:?}"
        );
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
