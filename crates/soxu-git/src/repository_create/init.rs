//! 显式 Init：唯一允许的创建路径是"目标缺失"或"显式授权且持锁复核仍为空"。

use std::path::Path;

use super::platform::DirectoryIdentity;
use super::sandbox::{self, Sandbox};
use super::{RepositoryCreateError, RepositoryCreateResult};

const INIT_ARGS: [&str; 3] = ["--template=", "--initial-branch=main", "--"];

/// 运行固定参数语义的 `git init`。
pub(super) async fn run(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
) -> Result<RepositoryCreateResult, RepositoryCreateError> {
    let target = super::platform::native_path(target)?;
    let mut command = sandbox.command(program, sandbox.root());
    command.arg("init").args(INIT_ARGS).arg(&target);
    outcome(
        sandbox::run(
            &mut command,
            sandbox::OUTPUT_LIMIT,
            sandbox::COMMAND_TIMEOUT,
        )
        .await,
    )
}

pub(super) fn outcome(
    result: Result<sandbox::RunOutput, sandbox::RunFailure>,
) -> Result<RepositoryCreateResult, RepositoryCreateError> {
    Ok(match result {
        Ok(output) if output.status.success() => RepositoryCreateResult::Succeeded,
        Ok(output) => {
            debug_assert!(output.direct_child_reaped, "正常退出必须已回收直接子进程");
            RepositoryCreateResult::Failed {
                exit_code: output.status.code(),
            }
        }
        Err(failure) => match failure.error {
            RepositoryCreateError::Io(error) if error.kind() == std::io::ErrorKind::NotFound => {
                RepositoryCreateResult::SpawnFailed
            }
            RepositoryCreateError::GitTimeout => RepositoryCreateResult::Unknown {
                stop_confirmed: failure.direct_child_reaped,
                reason: "Git 命令超时".to_owned(),
            },
            RepositoryCreateError::OutputLimit => RepositoryCreateResult::Unknown {
                stop_confirmed: failure.direct_child_reaped,
                reason: "Git 输出超过上限".to_owned(),
            },
            RepositoryCreateError::Io(_) => RepositoryCreateResult::Unknown {
                stop_confirmed: failure.direct_child_reaped,
                reason: "Git 进程启动或回收失败".to_owned(),
            },
            other => return Err(other),
        },
    })
}

/// 只读复核：`.git` 为受控实体目录、`HEAD` 指向未出生的 `refs/heads/main`、无外部模板与钩子。
#[derive(Debug, Default)]
pub(super) struct InitObservation {
    pub git_dir_present: bool,
    pub git_dir_identity: Option<DirectoryIdentity>,
    pub head_branch: Option<String>,
    pub head_oid: Option<String>,
    pub notes: Vec<String>,
}

impl InitObservation {
    pub(super) fn unborn_main(&self) -> bool {
        self.head_oid.is_none() && self.head_branch.as_deref() == Some("refs/heads/main")
    }
}

pub(super) async fn verify(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
    holder: Option<&super::platform::AuthorizedDirectory>,
) -> Result<InitObservation, RepositoryCreateError> {
    let mut observation = InitObservation::default();
    let git_dir = target.join(".git");
    match std::fs::symlink_metadata(&git_dir) {
        Ok(metadata) => {
            if !super::platform::plain_directory(&metadata) {
                return Err(RepositoryCreateError::UnsafeContent(
                    "初始化产物 .git 不是受控实体目录",
                ));
            }
            observation.git_dir_present = true;
            observation.git_dir_identity = Some(match holder {
                Some(handle) => handle.child(std::ffi::OsStr::new(".git"))?.identity(),
                None => super::platform::AuthorizedDirectory::open(&git_dir)?.identity(),
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            observation.notes.push("目标下没有 .git 目录".to_owned());
            return Ok(observation);
        }
        Err(error) => return Err(RepositoryCreateError::Io(error)),
    }

    let head = std::fs::read(git_dir.join("HEAD")).map_err(RepositoryCreateError::Io)?;
    let head = std::str::from_utf8(&head)
        .map_err(|_| RepositoryCreateError::UnsafeContent("HEAD 不是合法 UTF-8"))?;
    if head != "ref: refs/heads/main\n" && head != "ref: refs/heads/main" {
        return Err(RepositoryCreateError::UnsafeContent(
            "HEAD 未指向 refs/heads/main",
        ));
    }
    if git_dir.join("refs").join("heads").join("main").exists() {
        return Err(RepositoryCreateError::UnsafeContent(
            "初始化产物已包含 refs/heads/main",
        ));
    }
    let hooks = git_dir.join("hooks");
    if hooks.exists() {
        let mut entries = std::fs::read_dir(&hooks).map_err(RepositoryCreateError::Io)?;
        if entries
            .next()
            .transpose()
            .map_err(RepositoryCreateError::Io)?
            .is_some()
        {
            return Err(RepositoryCreateError::UnsafeContent(
                "初始化产物含钩子样例或钩子",
            ));
        }
    }
    let config = std::fs::read(git_dir.join("config")).map_err(RepositoryCreateError::Io)?;
    super::config_scan::reject_unsafe(&config, "初始化产物配置")?;

    observation.head_branch = read_heads(sandbox, program, target).await?;
    observation.head_oid = None;
    Ok(observation)
}

async fn read_heads(
    sandbox: &Sandbox,
    program: &Path,
    target: &Path,
) -> Result<Option<String>, RepositoryCreateError> {
    let mut command = sandbox.command(program, sandbox.root());
    command
        .arg("-C")
        .arg(target)
        .args(["symbolic-ref", "-q", "HEAD"]);
    let output = sandbox::run(
        &mut command,
        sandbox::OUTPUT_LIMIT,
        sandbox::COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    ))
}
