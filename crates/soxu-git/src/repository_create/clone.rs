//! 显式 Clone（首版仅本地源）：固定参数语义、独立对象复制与只读复核。

use std::path::{Path, PathBuf};

use super::platform::{self, AuthorizedDirectory, DirectoryIdentity};
use super::sandbox::{self, Sandbox};
use super::{RepositoryCreateError, RepositoryCreateResult};

const CLONE_ARGS: [&str; 5] = [
    "clone",
    "--no-local",
    "--no-checkout",
    "--no-recurse-submodules",
    "--template=",
];

/// 已核验的本地来源。
#[derive(Debug, Clone)]
pub(super) struct SourceFacts {
    pub root: PathBuf,
    pub git_dir: PathBuf,
}

/// 核验本地来源：拒绝 bare、关联工作树、UNC、链接、shallow、alternates、promisor 与外部过滤器。
pub(super) async fn verify_source(
    git: &crate::GitProbe,
    source_root: &Path,
) -> Result<SourceFacts, RepositoryCreateError> {
    if !super::target::local_absolute(source_root) {
        return Err(RepositoryCreateError::UnsupportedSource(
            "来源必须是本地绝对路径",
        ));
    }
    let _ = AuthorizedDirectory::open(source_root)?;
    let identity = match git.open_local_project(source_root).await? {
        crate::LocalProject::GitWorktree(identity) => identity,
        crate::LocalProject::Directory { .. } => {
            return Err(RepositoryCreateError::UnsupportedSource(
                "来源不是 Git 仓库",
            ));
        }
    };
    if identity.git_dir != identity.common_dir {
        return Err(RepositoryCreateError::UnsupportedSource("来源是关联工作树"));
    }
    let dot_git = identity.root.join(".git");
    let metadata = std::fs::symlink_metadata(&dot_git).map_err(RepositoryCreateError::Io)?;
    if !platform::plain_directory(&metadata) {
        return Err(RepositoryCreateError::UnsupportedSource(
            "来源 .git 不是实体目录",
        ));
    }
    if identity.git_dir.join("shallow").exists() {
        return Err(RepositoryCreateError::UnsupportedSource("来源是浅克隆"));
    }
    if identity
        .git_dir
        .join("objects")
        .join("info")
        .join("alternates")
        .exists()
    {
        return Err(RepositoryCreateError::UnsupportedSource(
            "来源依赖外部对象库",
        ));
    }
    let config =
        std::fs::read(identity.git_dir.join("config")).map_err(RepositoryCreateError::Io)?;
    super::config_scan::reject_unsafe(&config, "来源仓库配置")?;
    Ok(SourceFacts {
        root: identity.root,
        git_dir: identity.git_dir,
    })
}

/// 运行固定参数语义的 `git clone`。
pub(super) async fn run(
    sandbox: &Sandbox,
    program: &Path,
    source: &Path,
    target: &Path,
) -> Result<RepositoryCreateResult, RepositoryCreateError> {
    let source = platform::native_path(source)?;
    let target = platform::native_path(target)?;
    let mut command = sandbox.command(program, sandbox.root());
    command
        .args(CLONE_ARGS)
        .arg("--config")
        .arg("core.hooksPath=")
        .arg("--")
        .arg(&source)
        .arg(&target);
    super::init::outcome(
        sandbox::run(
            &mut command,
            sandbox::OUTPUT_LIMIT,
            sandbox::COMMAND_TIMEOUT,
        )
        .await,
    )
}

/// 克隆后只读复核与受控发布结果。
#[derive(Debug, Default)]
pub(super) struct CloneObservation {
    pub git_dir_present: bool,
    pub git_dir_identity: Option<DirectoryIdentity>,
    pub head_branch: Option<String>,
    pub head_oid: Option<String>,
    pub published_entries: usize,
    pub notes: Vec<String>,
}

impl CloneObservation {
    /// 远端为空仓库：已克隆但没有可检出提交。
    pub(super) fn empty_remote(&self) -> bool {
        self.head_oid.is_none()
            && self
                .head_branch
                .as_deref()
                .is_some_and(|branch| branch.starts_with("refs/heads/"))
    }
}

/// 复核克隆布局、解析远端 HEAD，并在满足检出前提时受控发布工作树。
pub(super) async fn verify(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
) -> Result<CloneObservation, RepositoryCreateError> {
    let mut observation = CloneObservation::default();
    let git_dir = target.join(".git");
    match std::fs::symlink_metadata(&git_dir) {
        Ok(metadata) => {
            if !platform::plain_directory(&metadata) {
                return Err(RepositoryCreateError::UnsafeContent(
                    "克隆产物 .git 不是受控实体目录",
                ));
            }
            observation.git_dir_present = true;
            observation.git_dir_identity = Some(AuthorizedDirectory::open(&git_dir)?.identity());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            observation.notes.push("目标下没有 .git 目录".to_owned());
            return Ok(observation);
        }
        Err(error) => return Err(RepositoryCreateError::Io(error)),
    }
    if git_dir.join("shallow").exists() {
        return Err(RepositoryCreateError::UnsafeContent("克隆产物是浅克隆"));
    }
    if git_dir
        .join("objects")
        .join("info")
        .join("alternates")
        .exists()
    {
        return Err(RepositoryCreateError::UnsafeContent(
            "克隆产物依赖外部对象库",
        ));
    }
    let config = std::fs::read(git_dir.join("config")).map_err(RepositoryCreateError::Io)?;
    super::config_scan::reject_unsafe(&config, "克隆产物配置")?;

    observation.head_branch = capture(sandbox, program, target, &["symbolic-ref", "-q", "HEAD"])
        .await?
        .map(|stdout| String::from_utf8_lossy(&stdout).trim().to_owned());
    if let Some(branch) = observation.head_branch.as_deref()
        && !branch.starts_with("refs/heads/")
    {
        return Err(RepositoryCreateError::UnsafeContent(
            "远端 HEAD 指向非分支引用",
        ));
    }
    observation.head_oid = capture(
        sandbox,
        program,
        target,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )
    .await?
    .map(|stdout| String::from_utf8_lossy(&stdout).trim().to_owned());

    let Some(oid) = observation.head_oid.clone() else {
        if observation.empty_remote() {
            observation
                .notes
                .push("远端为空仓库，已克隆但没有可检出提交".to_owned());
            return Ok(observation);
        }
        return Err(RepositoryCreateError::UnsafeContent("远端 HEAD 无法解析"));
    };
    let sized = oid.len() == 40 || oid.len() == 64;
    if !sized || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RepositoryCreateError::UnsafeContent(
            "HEAD 不是完整提交 OID",
        ));
    }
    observation.published_entries =
        super::checkout::publish_tree(sandbox, program, target, &oid).await?;
    Ok(observation)
}

async fn capture(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    arguments: &[&str],
) -> Result<Option<Vec<u8>>, RepositoryCreateError> {
    let mut command = sandbox.command(program, sandbox.root());
    command.arg("-C").arg(target).args(arguments);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    Ok(output.status.success().then_some(output.stdout))
}
