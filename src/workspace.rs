use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWorkspace {
    session_id: String,
    source_root: PathBuf,
    worktree_root: PathBuf,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktreePlan {
    source_root: PathBuf,
    workspace_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommand {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
}

#[derive(Debug, Default)]
pub struct GitWorktreeManager;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("failed to resolve source root {path}: {source}")]
    ResolveSource {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("session id must not be empty")]
    EmptySessionId,

    #[error("session id contains an unsupported path character: {0}")]
    InvalidSessionId(String),

    #[error("worktree root must not be empty")]
    EmptyWorktreeRoot,

    #[error(
        "worktree root {worktree_root} must not be equal to or nested under source root {source_root}"
    )]
    WorktreeRootInsideSource {
        source_root: String,
        worktree_root: String,
    },

    #[error("failed to create worktree parent {path}: {source}")]
    CreateWorktreeRoot {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to run git worktree {action}: {source}")]
    GitIo {
        action: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("git worktree {action} failed with status {status}: {stderr}")]
    GitFailed {
        action: &'static str,
        status: String,
        stderr: String,
    },
}

impl SessionWorkspace {
    pub fn new(
        session_id: impl Into<String>,
        source_root: impl AsRef<Path>,
        configured_worktree_root: Option<&Path>,
    ) -> Result<Self, WorkspaceError> {
        let session_id = session_id.into();
        validate_session_id(&session_id)?;

        let source_root = source_root.as_ref().canonicalize().map_err(|source| {
            WorkspaceError::ResolveSource {
                path: source_root.as_ref().display().to_string(),
                source,
            }
        })?;
        let worktree_root = match configured_worktree_root {
            Some(path) => resolve_configured_worktree_root(&source_root, path)?,
            None => default_worktree_root(&source_root),
        };
        reject_root_inside_source(&source_root, &worktree_root)?;
        let path = worktree_root.join(&session_id);

        Ok(Self {
            session_id,
            source_root,
            worktree_root,
            path,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    pub fn worktree_root(&self) -> &Path {
        &self.worktree_root
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn git_plan(&self) -> GitWorktreePlan {
        GitWorktreePlan {
            source_root: self.source_root.clone(),
            workspace_path: self.path.clone(),
        }
    }
}

impl GitWorktreePlan {
    pub fn add_command(&self) -> GitCommand {
        GitCommand {
            program: OsString::from("git"),
            args: vec![
                OsString::from("worktree"),
                OsString::from("add"),
                OsString::from("--detach"),
                self.workspace_path.as_os_str().to_os_string(),
                OsString::from("HEAD"),
            ],
            cwd: self.source_root.clone(),
        }
    }

    pub fn remove_command(&self) -> GitCommand {
        GitCommand {
            program: OsString::from("git"),
            args: vec![
                OsString::from("worktree"),
                OsString::from("remove"),
                OsString::from("--force"),
                self.workspace_path.as_os_str().to_os_string(),
            ],
            cwd: self.source_root.clone(),
        }
    }
}

impl GitWorktreeManager {
    pub fn create(&self, workspace: &SessionWorkspace) -> Result<(), WorkspaceError> {
        fs::create_dir_all(workspace.worktree_root()).map_err(|source| {
            WorkspaceError::CreateWorktreeRoot {
                path: workspace.worktree_root().display().to_string(),
                source,
            }
        })?;
        run_git_command("add", workspace.git_plan().add_command())
    }

    pub fn remove(&self, workspace: &SessionWorkspace) -> Result<(), WorkspaceError> {
        if !workspace.path().exists() {
            return Ok(());
        }

        run_git_command("remove", workspace.git_plan().remove_command())
    }
}

fn run_git_command(action: &'static str, command: GitCommand) -> Result<(), WorkspaceError> {
    let output = Command::new(&command.program)
        .args(&command.args)
        .current_dir(&command.cwd)
        .output()
        .map_err(|source| WorkspaceError::GitIo { action, source })?;

    if output.status.success() {
        return Ok(());
    }

    Err(WorkspaceError::GitFailed {
        action,
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

fn validate_session_id(session_id: &str) -> Result<(), WorkspaceError> {
    if session_id.is_empty() {
        return Err(WorkspaceError::EmptySessionId);
    }

    if !session_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(WorkspaceError::InvalidSessionId(session_id.to_string()));
    }

    if session_id == "." || session_id == ".." {
        return Err(WorkspaceError::InvalidSessionId(session_id.to_string()));
    }

    Ok(())
}

fn resolve_configured_worktree_root(
    source_root: &Path,
    configured: &Path,
) -> Result<PathBuf, WorkspaceError> {
    if configured.as_os_str().is_empty() {
        return Err(WorkspaceError::EmptyWorktreeRoot);
    }

    let base = if configured.is_absolute() {
        PathBuf::new()
    } else {
        source_root.to_path_buf()
    };

    Ok(normalize_path(base.join(configured)))
}

fn default_worktree_root(source_root: &Path) -> PathBuf {
    let project_name = source_root
        .file_name()
        .unwrap_or_else(|| OsStr::new("project"));
    let parent = source_root.parent().unwrap_or(source_root);
    normalize_path(parent.join(".lean-worktrees").join(project_name))
}

fn reject_root_inside_source(
    source_root: &Path,
    worktree_root: &Path,
) -> Result<(), WorkspaceError> {
    if worktree_root == source_root || worktree_root.starts_with(source_root) {
        return Err(WorkspaceError::WorktreeRootInsideSource {
            source_root: source_root.display().to_string(),
            worktree_root: worktree_root.display().to_string(),
        });
    }

    Ok(())
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{GitWorktreeManager, SessionWorkspace, WorkspaceError};

    #[test]
    fn workspace_uses_deterministic_session_path() {
        let temp = unique_temp_dir("workspace-path");
        let source = temp.join("source");
        fs::create_dir_all(&source).expect("source should be creatable");
        let worktrees = temp.join("worktrees");

        let workspace = SessionWorkspace::new("session-0001", &source, Some(&worktrees))
            .expect("workspace should build");

        assert_eq!(workspace.session_id(), "session-0001");
        assert_eq!(workspace.source_root(), source.canonicalize().unwrap());
        assert_eq!(workspace.worktree_root(), worktrees);
        assert_eq!(workspace.path(), worktrees.join("session-0001"));
    }

    #[test]
    fn default_worktree_root_is_outside_source_root() {
        let temp = unique_temp_dir("workspace-default");
        let source = temp.join("source");
        fs::create_dir_all(&source).expect("source should be creatable");

        let workspace =
            SessionWorkspace::new("session-0001", &source, None).expect("workspace should build");

        assert_eq!(
            workspace.worktree_root(),
            temp.join(".lean-worktrees").join("source")
        );
    }

    #[test]
    fn rejects_worktree_root_inside_source_root() {
        let temp = unique_temp_dir("workspace-inside-source");
        let source = temp.join("source");
        fs::create_dir_all(&source).expect("source should be creatable");

        let error = SessionWorkspace::new("session-0001", &source, Some(&source.join("target")))
            .expect_err("nested worktree roots should be rejected");

        assert!(
            matches!(error, WorkspaceError::WorktreeRootInsideSource { .. }),
            "nested root should fail with containment error, got {error:?}"
        );
    }

    #[test]
    fn rejects_invalid_session_id() {
        let temp = unique_temp_dir("workspace-invalid-id");
        let source = temp.join("source");
        fs::create_dir_all(&source).expect("source should be creatable");

        let error = SessionWorkspace::new("../escape", &source, Some(&temp.join("worktrees")))
            .expect_err("path-like session id should be rejected");

        assert!(
            matches!(error, WorkspaceError::InvalidSessionId(_)),
            "invalid session id should fail, got {error:?}"
        );
    }

    #[test]
    fn git_plan_uses_explicit_argv() {
        let temp = unique_temp_dir("workspace-plan");
        let source = temp.join("source");
        fs::create_dir_all(&source).expect("source should be creatable");
        let workspace =
            SessionWorkspace::new("session-0001", &source, Some(&temp.join("worktrees")))
                .expect("workspace should build");

        let plan = workspace.git_plan();
        let add = plan.add_command();
        assert_eq!(add.program, "git");
        assert_eq!(
            add.args,
            vec![
                "worktree",
                "add",
                "--detach",
                workspace.path().to_str().expect("path should be UTF-8"),
                "HEAD",
            ]
        );
        assert_eq!(add.cwd, workspace.source_root());

        let remove = plan.remove_command();
        assert_eq!(remove.program, "git");
        assert_eq!(
            remove.args,
            vec![
                "worktree",
                "remove",
                "--force",
                workspace.path().to_str().expect("path should be UTF-8"),
            ]
        );
    }

    #[test]
    fn git_worktree_manager_creates_and_removes_detached_worktree() {
        let temp = unique_temp_dir("workspace-manager");
        let source = temp.join("source");
        create_git_repo_with_commit(&source);

        let workspace =
            SessionWorkspace::new("session-0001", &source, Some(&temp.join("worktrees")))
                .expect("workspace should build");
        let manager = GitWorktreeManager;

        manager.create(&workspace).expect("worktree should create");
        assert!(workspace.path().join("README.md").exists());

        manager.remove(&workspace).expect("worktree should remove");
        assert!(!workspace.path().exists());
    }

    fn create_git_repo_with_commit(source: &Path) {
        fs::create_dir_all(source).expect("source should be creatable");
        git(source, ["init"]);
        git(source, ["config", "user.name", "Lean Tests"]);
        git(source, ["config", "user.email", "lean@example.invalid"]);
        fs::write(source.join("README.md"), "fixture\n").expect("fixture should be writable");
        git(source, ["add", "README.md"]);
        git(source, ["commit", "-m", "initial"]);
    }

    fn git<const N: usize>(cwd: &Path, args: [&str; N]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("git should run");

        assert!(
            output.status.success(),
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
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
