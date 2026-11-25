use std::fs::{self, File, Metadata};
use std::io;
use std::path::{Path, PathBuf};

use crate::{GitError, GitProbe};

const NOT_REPOSITORY: &[u8] =
    b"fatal: not a git repository (or any of the parent directories): .git\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalProject {
    Directory { root: PathBuf },
    GitWorktree(WorktreeIdentity),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeIdentity {
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectOpenError {
    #[error("bare Git 仓库没有可打开的工作树")]
    BareRepository,
    #[error("指定目录位于 Git 元数据内")]
    GitMetadataDirectory,
    #[error("Git 元数据损坏或不完整")]
    InvalidRepository {
        #[source]
        source: Option<GitError>,
    },
    #[error(transparent)]
    Git(#[from] GitError),
}

impl From<io::Error> for ProjectOpenError {
    fn from(error: io::Error) -> Self {
        Self::Git(error.into())
    }
}

impl GitProbe {
    /// 读取绝对目录的项目身份。返回的路径均为规范化绝对路径。
    pub async fn open_local_project(
        &self,
        directory: impl AsRef<Path>,
    ) -> Result<LocalProject, ProjectOpenError> {
        let directory = directory.as_ref();
        if !directory.is_absolute() {
            return Err(GitError::RelativePath.into());
        }
        let directory = directory.canonicalize()?;
        if !directory.metadata()?.is_dir() {
            return Err(GitError::NotDirectory.into());
        }
        drop(fs::read_dir(&directory)?);

        let output = self
            .run_command(
                &directory,
                "rev-parse",
                &[
                    "--is-bare-repository",
                    "--is-inside-git-dir",
                    "--is-inside-work-tree",
                ],
            )
            .await?;
        if !output.status.success() {
            let code = output.status.code();
            if code == Some(128)
                && output.stdout.is_empty()
                && output.stderr.as_slice() == NOT_REPOSITORY
            {
                check_skipped_metadata(&directory, None)?;
                return Ok(LocalProject::Directory { root: directory });
            }
            let malformed = [
                b"fatal: invalid gitfile format: ".as_slice(),
                b"fatal: gitfile does not point to a valid repository: ",
                b"fatal: not a git repository: ",
                b"fatal: bad config line ",
            ]
            .iter()
            .any(|prefix| output.stderr.starts_with(prefix));
            let error = GitError::CommandFailed {
                operation: "rev-parse",
                code,
                stderr: output.stderr,
            };
            return Err(if malformed {
                ProjectOpenError::InvalidRepository {
                    source: Some(error),
                }
            } else {
                error.into()
            });
        }

        match output.stdout.as_slice() {
            b"true\ntrue\nfalse\n" => return Err(ProjectOpenError::BareRepository),
            b"false\ntrue\nfalse\n" => return Err(ProjectOpenError::GitMetadataDirectory),
            b"false\nfalse\ntrue\n" => {}
            b"false\nfalse\nfalse\n" => return Err(GitError::NoWorktree.into()),
            _ => return Err(GitError::InvalidMetadata("无法判定 Git 工作树类型").into()),
        }
        let root = self.read_path(&directory, "--show-toplevel").await?;
        if !directory.starts_with(&root) {
            return Err(GitError::InvalidMetadata("输入目录位于工作树根之外").into());
        }
        check_skipped_metadata(&directory, Some(&root))?;
        let git_dir = self.read_path(&root, "--absolute-git-dir").await?;
        let common_dir = self.read_path(&root, "--git-common-dir").await?;
        Ok(LocalProject::GitWorktree(WorktreeIdentity {
            root,
            git_dir,
            common_dir,
        }))
    }
}

fn check_skipped_metadata(directory: &Path, root: Option<&Path>) -> Result<(), ProjectOpenError> {
    for ancestor in directory
        .ancestors()
        .take_while(|ancestor| Some(*ancestor) != root)
    {
        let marker = ancestor.join(".git");
        if metadata_if_present(&marker)?.is_some() {
            match marker.metadata() {
                Ok(metadata) if metadata.is_dir() => {
                    drop(fs::read_dir(&marker)?);
                    check_readable_file(&marker.join("HEAD"))?;
                }
                Ok(metadata) if metadata.is_file() => check_readable_file(&marker)?,
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            return Err(ProjectOpenError::InvalidRepository { source: None });
        }
        if has_repository_layout(ancestor)? {
            check_readable_file(&ancestor.join("HEAD"))?;
            return Err(ProjectOpenError::InvalidRepository { source: None });
        }
    }
    Ok(())
}

fn has_repository_layout(directory: &Path) -> io::Result<bool> {
    let has = |name, is_directory| {
        metadata_if_present(&directory.join(name)).map(|entry| {
            entry.is_some_and(|metadata| {
                metadata.is_symlink()
                    || if is_directory {
                        metadata.is_dir()
                    } else {
                        metadata.is_file()
                    }
            })
        })
    };
    if has("commondir", false)? && has("gitdir", false)? {
        return Ok(true);
    }
    let objects = has("objects", true)?;
    let refs = has("refs", true)?;
    match (objects, refs) {
        (true, true) => Ok(has("HEAD", false)? || has("config", false)?),
        (true, false) | (false, true) => Ok(has("HEAD", false)? && has("config", false)?),
        (false, false) => Ok(false),
    }
}

fn metadata_if_present(path: &Path) -> io::Result<Option<Metadata>> {
    match path.symlink_metadata() {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn check_readable_file(path: &Path) -> io::Result<()> {
    match path.metadata() {
        Ok(metadata) if metadata.is_file() => File::open(path).map(drop),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
