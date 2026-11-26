use std::fs::{self, File, Metadata};
use std::io;
use std::path::{Path, PathBuf};

use crate::{GitError, GitProbe};

const NOT_REPOSITORY: &[u8] =
    b"fatal: not a git repository (or any of the parent directories): .git\n";
// Linux 的 Git 会额外说明已越过文件系统边界，且挂载点路径随运行环境变化。
const NOT_REPOSITORY_MOUNT_POINT_PREFIX: &[u8] =
    b"fatal: not a git repository (or any parent up to mount point ";
const NOT_REPOSITORY_MOUNT_POINT_SUFFIX: &[u8] =
    b")\nStopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set).\n";

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
                && is_not_repository_stderr(&output.stderr)
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

/// 严格判定「当前目录不在任何 Git 仓库内」的 stderr 文本。
///
/// 只承认两种确定格式：Git for Windows 的固定单行文案，以及 Linux 会带上挂载点的两行文案。
/// 其余形式（如 `fatal: not a git repository: …`、畸形或多行错误）一律返回 false，
/// 由调用方继续按 fail-closed 处理，绝不降级为「普通目录」。
fn is_not_repository_stderr(stderr: &[u8]) -> bool {
    if stderr == NOT_REPOSITORY {
        return true;
    }
    let Some(mount_point) = stderr
        .strip_prefix(NOT_REPOSITORY_MOUNT_POINT_PREFIX)
        .and_then(|rest| rest.strip_suffix(NOT_REPOSITORY_MOUNT_POINT_SUFFIX))
    else {
        return false;
    };
    !mount_point.is_empty() && !mount_point.contains(&b'\n')
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

#[cfg(test)]
mod tests {
    use super::{
        NOT_REPOSITORY, NOT_REPOSITORY_MOUNT_POINT_PREFIX, NOT_REPOSITORY_MOUNT_POINT_SUFFIX,
        is_not_repository_stderr,
    };

    fn mount_point_form(mount_point: &[u8]) -> Vec<u8> {
        [
            NOT_REPOSITORY_MOUNT_POINT_PREFIX,
            mount_point,
            NOT_REPOSITORY_MOUNT_POINT_SUFFIX,
        ]
        .concat()
    }

    #[test]
    fn accepts_exact_and_mount_point_not_repository_messages() {
        assert!(is_not_repository_stderr(NOT_REPOSITORY));
        assert!(is_not_repository_stderr(&mount_point_form(b"/")));
        assert!(is_not_repository_stderr(&mount_point_form(
            b"/mnt/f/workspace/repo"
        )));
        assert!(is_not_repository_stderr(&mount_point_form(
            b"C:/workspace/repo with space"
        )));
    }

    #[test]
    fn rejects_ambiguous_or_malformed_not_repository_messages() {
        for stderr in [
            b"fatal: not a git repository: .git\n".as_slice(),
            b"fatal: not a git repository: /mnt/f/repo/.git\n",
            b"fatal: not a git repository\n",
            b"fatal: not a git repository (or any parent up to mount point )\nStopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set).\n",
            b"fatal: not a git repository (or any parent up to mount point /\nStopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set).\n",
            b"fatal: not a git repository (or any parent up to mount point /)\nStopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set).",
            b"fatal: not a git repository (or any parent up to mount point /)\nStopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set).\nextra\n",
            b"fatal: not a git repository (or any parent up to mount point /a\nb)\nStopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set).\n",
            b"fatal: not a git repository (or any of the parent directories): .git",
            b"error: not a git repository (or any of the parent directories): .git\n",
            b"",
        ] {
            assert!(!is_not_repository_stderr(stderr), "{stderr:?}");
        }
    }
}
