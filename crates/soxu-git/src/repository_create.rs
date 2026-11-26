//! M3 显式受控仓库创建：本地 Init 与本地源 Clone，以及失败留痕与只读诊断。
//!
//! 该入口只做两件 Git 写操作：`git init` 与本地源 `git clone`。
//! 两者都要求一个由宿主绑定的一次性结构化授权：请求 ID、操作、受控父根及其目录身份、
//! 直接目标名和固定参数。打开普通目录、发现项目与刷新清单都不会触发这里的任何写入。
//!
//! 平台边界：Windows 上目录身份只能取 `creation_time + file_attributes`（纯 `std` 无跨重启
//! NTFS FileId），因此仅在同一进程持有目录句柄链期间用于检测替换，不与 Linux 的
//! `dev:ino:ctime` 等价。HTTPS 克隆在传输与凭据策略验收前一律返回 `Unsupported`。

mod checkout;
mod clone;
mod config_scan;
mod init;
mod platform;
mod sandbox;
mod state;
mod target;

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::GitProbe;
use platform::{AuthorizedDirectory, DirectoryIdentity};

pub use target::RepositoryTargetState;

/// Init 的目标接纳模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitMode {
    /// 首版默认：目标缺失。
    MissingTarget,
    /// 显式授权：目标已存在且经持锁复核仍为纯空目录。
    VerifiedEmptyDirectory,
}

impl InitMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MissingTarget => "missing-target",
            Self::VerifiedEmptyDirectory => "verified-empty-directory",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Option<Self>> {
        match text {
            "-" => Some(None),
            "missing-target" => Some(Some(Self::MissingTarget)),
            "verified-empty-directory" => Some(Some(Self::VerifiedEmptyDirectory)),
            _ => None,
        }
    }
}

/// 请求的创建操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryOperation {
    Init,
    CloneLocal,
    CloneUrl,
}

impl RepositoryOperation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::CloneLocal => "clone-local",
            Self::CloneUrl => "clone-url",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "init" => Some(Self::Init),
            "clone-local" => Some(Self::CloneLocal),
            "clone-url" => Some(Self::CloneUrl),
            _ => None,
        }
    }
}

/// 请求的来源。首版只开放本地 Init 与本地源 Clone。
#[derive(Debug, Clone, Copy)]
pub enum RepositorySource<'a> {
    /// 显式 Init。
    Init { mode: InitMode },
    /// 显式 Clone，源为本机绝对路径。
    CloneLocal { source_root: &'a Path },
    /// 受控 HTTPS 端点；首版一律 `Unsupported` 关闭。
    CloneUrl { url: &'a str },
}

/// 一次性结构化授权。
#[derive(Debug, Clone, Copy)]
pub struct RepositoryCreateRequest<'a> {
    pub request_id: &'a str,
    /// 已由宿主打开的受控父目录。
    pub target_root: &'a Path,
    /// 受控父目录内的直接目标名。
    pub target_name: &'a str,
    pub source: RepositorySource<'a>,
}

/// 持久留痕阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryCreateStage {
    /// 已预约目标，尚未启动 Git。
    Prepared,
    /// 已持锁复核并启动 Git。
    Started,
    /// 已退出命令并完成独立只读复核。
    Observed,
}

impl RepositoryCreateStage {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Started => "started",
            Self::Observed => "observed",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "prepared" => Some(Self::Prepared),
            "started" => Some(Self::Started),
            "observed" => Some(Self::Observed),
            _ => None,
        }
    }
}

/// Git 调用结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryCreateResult {
    NotStarted,
    SpawnFailed,
    Succeeded,
    Failed {
        exit_code: Option<i32>,
    },
    Unknown {
        stop_confirmed: bool,
        reason: String,
    },
}

impl RepositoryCreateResult {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "not-started",
            Self::SpawnFailed => "spawn-failed",
            Self::Succeeded => "succeeded",
            Self::Failed { .. } => "failed",
            Self::Unknown { .. } => "unknown",
        }
    }
}

/// 独立复核给出的终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryTerminalState {
    NotStarted,
    Verified,
    PartialObserved,
    Unknown,
}

impl RepositoryTerminalState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not-started",
            Self::Verified => "verified",
            Self::PartialObserved => "partial-observed",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "not-started" => Some(Self::NotStarted),
            "verified" => Some(Self::Verified),
            "partial-observed" => Some(Self::PartialObserved),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// 协调域内的持久留痕。只用于拒绝变参重放与重启只读回查。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryOperationRecord {
    pub request_id: String,
    pub operation: RepositoryOperation,
    pub mode: Option<InitMode>,
    pub target_root: PathBuf,
    pub target: PathBuf,
    pub source: Option<PathBuf>,
    pub stage: RepositoryCreateStage,
    pub result: RepositoryCreateResult,
    pub terminal: Option<RepositoryTerminalState>,
    pub target_absent_before: bool,
    pub target_empty_before: bool,
    pub recorded_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub observed_at: Option<SystemTime>,
    pub notes: Vec<String>,
    pub(crate) target_root_identity: Option<DirectoryIdentity>,
    pub(crate) target_identity: Option<DirectoryIdentity>,
    pub(crate) git_dir_identity: Option<DirectoryIdentity>,
}

/// 独立只读复核给出的观察。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryObservation {
    pub observed_at: SystemTime,
    pub target_state: RepositoryTargetState,
    pub git_dir_present: bool,
    pub head_branch: Option<String>,
    pub head_oid: Option<String>,
    pub published_entries: usize,
    pub notes: Vec<String>,
    pub(crate) target_identity: Option<DirectoryIdentity>,
    pub(crate) git_dir_identity: Option<DirectoryIdentity>,
}

/// 一次显式创建的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryCreateOutcome {
    /// 终态已验证。
    Verified {
        record: RepositoryOperationRecord,
        observation: RepositoryObservation,
    },
    /// 同 ID 同参数重放，只读回查既有留痕。
    ExistingRecord {
        record: RepositoryOperationRecord,
        observation: Option<RepositoryObservation>,
    },
    /// 观察到部分落盘。
    PartialObserved {
        record: RepositoryOperationRecord,
        observation: Option<RepositoryObservation>,
        command_succeeded: bool,
    },
    /// 结果未知；保留预约并禁用后续接入。
    UnknownOutcome {
        record: RepositoryOperationRecord,
        observation: Option<RepositoryObservation>,
        reason: String,
    },
    /// 可以确认命令没有留下目标。
    NotStarted {
        record: RepositoryOperationRecord,
        observation: Option<RepositoryObservation>,
    },
}

/// 由可信宿主绑定的协调域。
#[derive(Debug)]
pub struct RepositoryCreateCoordinator {
    state_root: AuthorizedDirectory,
}

impl RepositoryCreateCoordinator {
    /// 打开宿主私有协调域；目录必须已存在且为普通目录。
    pub fn open(state_root: impl AsRef<Path>) -> Result<Self, RepositoryCreateError> {
        Ok(Self {
            state_root: state::open_root(state_root.as_ref())?,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryCreateError {
    #[error("仓库创建请求字段无效：{0}")]
    InvalidInput(&'static str),
    #[error("路径未通过授权边界检查：{0}")]
    Unauthorized(&'static str),
    #[error("目标已被占用")]
    TargetExists,
    #[error("目标已被请求 {request_id} 预约")]
    TargetReserved { request_id: String },
    #[error("同名请求携带了不同参数")]
    RequestConflict,
    #[error("协调域正被其他创建进程使用")]
    StateBusy,
    #[error("目录身份在检查期间发生变化")]
    IdentityChanged,
    #[error("本轮尚不支持该输入：{0}")]
    Unsupported(&'static str),
    #[error("本地源仓库不满足受控克隆条件：{0}")]
    UnsupportedSource(&'static str),
    #[error("克隆内容不满足受控检出条件：{0}")]
    UnsafeContent(&'static str),
    #[error("文件系统未提供本实现所需的平台原生能力：{0}")]
    UnsupportedFilesystem(&'static str),
    #[error("当前平台不支持受控仓库创建：{0}")]
    UnsupportedPlatform(&'static str),
    #[error("Git 命令超时")]
    GitTimeout,
    #[error("Git 输出超过上限")]
    OutputLimit,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Git(#[from] crate::GitError),
    #[error(transparent)]
    Project(#[from] crate::ProjectOpenError),
}

#[derive(Debug)]
struct Plan {
    request_id: String,
    operation: RepositoryOperation,
    mode: Option<InitMode>,
    target_root: AuthorizedDirectory,
    target: PathBuf,
    source: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Preflight {
    state: RepositoryTargetState,
    identity: Option<DirectoryIdentity>,
    absent: bool,
    empty: bool,
}

impl GitProbe {
    /// 执行一次显式受控仓库创建。
    pub async fn create_repository(
        &self,
        coordinator: &RepositoryCreateCoordinator,
        request: RepositoryCreateRequest<'_>,
    ) -> Result<RepositoryCreateOutcome, RepositoryCreateError> {
        let plan = plan(&request)?;
        let lock = state::acquire(&coordinator.state_root).await?;
        let records = state::read_records(&coordinator.state_root)?;
        if let Some(existing) = records
            .iter()
            .find(|record| record.request_id == plan.request_id)
        {
            if !same_binding(existing, &plan) {
                return Err(RepositoryCreateError::RequestConflict);
            }
            if matches!(existing.result, RepositoryCreateResult::Unknown { .. }) {
                return Ok(RepositoryCreateOutcome::UnknownOutcome {
                    record: existing.clone(),
                    observation: None,
                    reason: "上次执行结果未知，拒绝重放".to_owned(),
                });
            }
            let observation = observe(self, &coordinator.state_root, existing).await;
            return Ok(RepositoryCreateOutcome::ExistingRecord {
                record: existing.clone(),
                observation,
            });
        }
        if let Some(other) = records.iter().find(|record| record.target == plan.target) {
            return Err(RepositoryCreateError::TargetReserved {
                request_id: other.request_id.clone(),
            });
        }

        let before = preflight(&plan)?;
        let mut record = RepositoryOperationRecord {
            request_id: plan.request_id.clone(),
            operation: plan.operation,
            mode: plan.mode,
            target_root: plan.target_root.path().to_owned(),
            target: plan.target.clone(),
            source: plan.source.clone(),
            stage: RepositoryCreateStage::Prepared,
            result: RepositoryCreateResult::NotStarted,
            terminal: None,
            target_absent_before: before.absent,
            target_empty_before: before.empty,
            recorded_at: SystemTime::now(),
            started_at: None,
            observed_at: None,
            notes: vec![platform_note().to_owned()],
            target_root_identity: Some(plan.target_root.identity()),
            target_identity: before.identity,
            git_dir_identity: None,
        };
        state::write_record(&coordinator.state_root, &record)?;

        plan.target_root.refresh()?;
        lock.refresh(&coordinator.state_root)?;
        if preflight(&plan)? != before {
            return Err(RepositoryCreateError::IdentityChanged);
        }
        if let (RepositoryOperation::CloneLocal, Some(source)) =
            (plan.operation, plan.source.as_deref())
        {
            let facts = clone::verify_source(self, source).await?;
            if target::overlaps(&facts.root, &plan.target) {
                return Err(RepositoryCreateError::InvalidInput("来源与目标重叠"));
            }
            record
                .notes
                .push(format!("来源对象库：{}", facts.git_dir.to_string_lossy()));
        }
        record.stage = RepositoryCreateStage::Started;
        record.started_at = Some(SystemTime::now());
        state::rewrite_record(&coordinator.state_root, &record)?;
        lock.refresh(&coordinator.state_root)?;

        let state_root_path = coordinator.state_root.path().to_owned();
        let state_identity = coordinator.state_root.identity();
        let request_id = plan.request_id.clone();
        let owner = tokio::spawn(execute(
            self.clone(),
            state_root_path.clone(),
            state_identity,
            plan,
            lock,
            record,
        ));
        match owner.await {
            Ok(result) => result,
            Err(_) => {
                let root = AuthorizedDirectory::open(&state_root_path)?;
                let record = state::read_record(&root, &request_id)?.ok_or(
                    RepositoryCreateError::InvalidInput("写执行所有者未留下留痕"),
                )?;
                Ok(RepositoryCreateOutcome::UnknownOutcome {
                    record,
                    observation: None,
                    reason: "写执行所有者结果不可用，保留持久预约".to_owned(),
                })
            }
        }
    }

    /// 重启后按请求 ID 只读回查，不重新执行任何 Git 写入。
    pub async fn diagnose_repository_create(
        &self,
        state_root: impl AsRef<Path>,
        request_id: &str,
    ) -> Result<Option<RepositoryOperationRecord>, RepositoryCreateError> {
        crate::worktree_lifecycle::validate_request_id(request_id)
            .map_err(|_| RepositoryCreateError::InvalidInput("请求 ID"))?;
        let root = AuthorizedDirectory::open(state_root.as_ref())?;
        state::read_record(&root, request_id)
    }
}

fn plan(request: &RepositoryCreateRequest<'_>) -> Result<Plan, RepositoryCreateError> {
    crate::worktree_lifecycle::validate_request_id(request.request_id)
        .map_err(|_| RepositoryCreateError::InvalidInput("请求 ID"))?;
    if !target::local_absolute(request.target_root) {
        return Err(RepositoryCreateError::Unauthorized(
            "受控父根必须是本地绝对路径",
        ));
    }
    let target_root = AuthorizedDirectory::open(request.target_root)?;
    let direct = target::child_of(target_root.path(), request.target_name)?;
    let parent = direct
        .parent()
        .ok_or(RepositoryCreateError::InvalidInput("目标缺少父目录"))?;
    if target::repository_at_or_above(parent)?.is_some() {
        return Err(RepositoryCreateError::Unauthorized(
            "目标祖先已属于 Git 仓库",
        ));
    }
    let (operation, mode, source) = match request.source {
        RepositorySource::Init { mode } => (RepositoryOperation::Init, Some(mode), None),
        RepositorySource::CloneLocal { source_root } => {
            if !target::local_absolute(source_root) {
                return Err(RepositoryCreateError::UnsupportedSource(
                    "来源必须是本地绝对路径",
                ));
            }
            let source = platform::native_path(source_root)?;
            if target::overlaps(&source, &direct) {
                return Err(RepositoryCreateError::InvalidInput("来源与目标重叠"));
            }
            (RepositoryOperation::CloneLocal, None, Some(source))
        }
        RepositorySource::CloneUrl { .. } => {
            return Err(RepositoryCreateError::Unsupported(
                "受控 HTTPS 克隆尚未验收，首版只开放本地源",
            ));
        }
    };
    Ok(Plan {
        request_id: request.request_id.to_owned(),
        operation,
        mode,
        target_root,
        target: direct,
        source,
    })
}

fn same_binding(record: &RepositoryOperationRecord, plan: &Plan) -> bool {
    record.operation == plan.operation
        && record.mode == plan.mode
        && record.target_root == plan.target_root.path()
        && record.target == plan.target
        && record.source == plan.source
}

fn preflight(plan: &Plan) -> Result<Preflight, RepositoryCreateError> {
    let state = target::inspect(&plan.target)?;
    let mut preflight = Preflight {
        state,
        identity: None,
        absent: false,
        empty: false,
    };
    match state {
        RepositoryTargetState::LinkOrReparse => {
            return Err(RepositoryCreateError::Unauthorized(
                "目标位置是链接或 reparse point",
            ));
        }
        RepositoryTargetState::Inaccessible => {
            return Err(RepositoryCreateError::Unauthorized("无法读取目标位置状态"));
        }
        RepositoryTargetState::Missing => {
            preflight.absent = true;
        }
        RepositoryTargetState::File => {
            return Err(RepositoryCreateError::TargetExists);
        }
        RepositoryTargetState::Directory => match plan.mode {
            Some(InitMode::VerifiedEmptyDirectory) => {
                let authorized = target::authorized_existing(&plan.target)?
                    .ok_or(RepositoryCreateError::Unauthorized("目标目录身份不可核对"))?;
                if !target::is_empty_directory(&plan.target)? {
                    return Err(RepositoryCreateError::TargetExists);
                }
                preflight.identity = Some(authorized.identity());
                preflight.empty = true;
                authorized.refresh()?;
            }
            _ => return Err(RepositoryCreateError::TargetExists),
        },
    }
    Ok(preflight)
}

async fn execute(
    git: GitProbe,
    state_root_path: PathBuf,
    state_identity: DirectoryIdentity,
    plan: Plan,
    lock: state::StateLock,
    mut record: RepositoryOperationRecord,
) -> Result<RepositoryCreateOutcome, RepositoryCreateError> {
    let state_root = AuthorizedDirectory::open(&state_root_path)?;
    if state_root.identity() != state_identity {
        return Err(RepositoryCreateError::IdentityChanged);
    }
    lock.refresh(&state_root)?;
    let sandbox = sandbox::Sandbox::prepare(state_root.path(), &plan.request_id)?;
    let program = git.program().to_owned();

    let result = match plan.operation {
        RepositoryOperation::Init => init::run(&sandbox, &program, &plan.target).await?,
        RepositoryOperation::CloneLocal => {
            let source = plan
                .source
                .clone()
                .ok_or(RepositoryCreateError::InvalidInput("克隆请求缺少来源"))?;
            clone::run(&sandbox, &program, &source, &plan.target).await?
        }
        RepositoryOperation::CloneUrl => {
            return Err(RepositoryCreateError::Unsupported(
                "受控 HTTPS 克隆尚未验收",
            ));
        }
    };

    record.stage = RepositoryCreateStage::Observed;
    record.result = result.clone();
    record.observed_at = Some(SystemTime::now());
    let observed_state =
        target::inspect(&plan.target).unwrap_or(RepositoryTargetState::Inaccessible);
    let mut observation = None;
    let mut verify_error = None;
    let verified = match plan.operation {
        RepositoryOperation::Init => match init::verify(&sandbox, &program, &plan.target).await {
            Ok(facts) => {
                record.git_dir_identity = facts.git_dir_identity;
                let ok = result == RepositoryCreateResult::Succeeded && facts.unborn_main();
                let entries = facts.notes;
                observation = Some(RepositoryObservation {
                    observed_at: SystemTime::now(),
                    target_state: observed_state,
                    git_dir_present: facts.git_dir_present,
                    head_branch: facts.head_branch,
                    head_oid: facts.head_oid,
                    published_entries: 0,
                    notes: entries,
                    target_identity: record.target_identity,
                    git_dir_identity: record.git_dir_identity,
                });
                ok
            }
            Err(error) => {
                verify_error = Some(error.to_string());
                false
            }
        },
        _ => match clone::verify(&sandbox, &program, &plan.target).await {
            Ok(facts) => {
                record.git_dir_identity = facts.git_dir_identity;
                let ok = result == RepositoryCreateResult::Succeeded
                    && facts.git_dir_present
                    && (facts.empty_remote() || facts.head_oid.is_some());
                observation = Some(RepositoryObservation {
                    observed_at: SystemTime::now(),
                    target_state: observed_state,
                    git_dir_present: facts.git_dir_present,
                    head_branch: facts.head_branch,
                    head_oid: facts.head_oid,
                    published_entries: facts.published_entries,
                    notes: facts.notes,
                    target_identity: record.target_identity,
                    git_dir_identity: record.git_dir_identity,
                });
                ok
            }
            Err(error) => {
                verify_error = Some(error.to_string());
                false
            }
        },
    };

    let git_dir_present = observation
        .as_ref()
        .is_some_and(|observation| observation.git_dir_present);
    let terminal = if let Some(error) = verify_error {
        record.notes.push(format!("只读复核失败：{error}"));
        RepositoryTerminalState::Unknown
    } else if git_dir_present && verified {
        RepositoryTerminalState::Verified
    } else if git_dir_present {
        RepositoryTerminalState::PartialObserved
    } else if matches!(record.result, RepositoryCreateResult::Unknown { .. }) {
        RepositoryTerminalState::Unknown
    } else {
        RepositoryTerminalState::NotStarted
    };
    record.terminal = Some(terminal);
    if let Some(observation) = &observation {
        record.notes.extend(observation.notes.iter().cloned());
    }
    state::rewrite_record(&state_root, &record)?;
    lock.refresh(&state_root)?;
    drop(lock);

    Ok(match terminal {
        RepositoryTerminalState::Verified => RepositoryCreateOutcome::Verified {
            record,
            observation: observation
                .ok_or(RepositoryCreateError::InvalidInput("已验证终态缺少观察"))?,
        },
        RepositoryTerminalState::PartialObserved => RepositoryCreateOutcome::PartialObserved {
            record: record.clone(),
            observation,
            command_succeeded: matches!(record.result, RepositoryCreateResult::Succeeded),
        },
        RepositoryTerminalState::NotStarted => RepositoryCreateOutcome::NotStarted {
            record,
            observation,
        },
        RepositoryTerminalState::Unknown => RepositoryCreateOutcome::UnknownOutcome {
            record,
            observation,
            reason: "无法确认本次执行的最终磁盘状态".to_owned(),
        },
    })
}

async fn observe(
    git: &GitProbe,
    state_root: &AuthorizedDirectory,
    record: &RepositoryOperationRecord,
) -> Option<RepositoryObservation> {
    let sandbox = sandbox::Sandbox::prepare(state_root.path(), &record.request_id).ok()?;
    let program = git.program().to_owned();
    let facts = match record.operation {
        RepositoryOperation::Init => {
            let facts = init::verify(&sandbox, &program, &record.target)
                .await
                .ok()?;
            CloneFact {
                git_dir_present: facts.git_dir_present,
                head_branch: facts.head_branch,
                head_oid: facts.head_oid,
                published_entries: 0,
                notes: facts.notes,
                git_dir_identity: facts.git_dir_identity,
            }
        }
        _ => {
            let facts = clone::verify(&sandbox, &program, &record.target)
                .await
                .ok()?;
            CloneFact {
                git_dir_present: facts.git_dir_present,
                head_branch: facts.head_branch,
                head_oid: facts.head_oid,
                published_entries: facts.published_entries,
                notes: facts.notes,
                git_dir_identity: facts.git_dir_identity,
            }
        }
    };
    Some(RepositoryObservation {
        observed_at: SystemTime::now(),
        target_state: target::inspect(&record.target)
            .unwrap_or(RepositoryTargetState::Inaccessible),
        git_dir_present: facts.git_dir_present,
        head_branch: facts.head_branch,
        head_oid: facts.head_oid,
        published_entries: facts.published_entries,
        notes: facts.notes,
        target_identity: record.target_identity,
        git_dir_identity: facts.git_dir_identity,
    })
}

struct CloneFact {
    git_dir_present: bool,
    head_branch: Option<String>,
    head_oid: Option<String>,
    published_entries: usize,
    notes: Vec<String>,
    git_dir_identity: Option<DirectoryIdentity>,
}

fn platform_note() -> &'static str {
    #[cfg(windows)]
    {
        "Windows 目录身份为 creation_time + file_attributes（纯 std 无跨重启 NTFS FileId），仅同进程句柄链内可核对"
    }
    #[cfg(target_os = "linux")]
    {
        "Linux 目录身份为 dev:ino:ctime"
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        "本平台没有受控目录身份实现"
    }
}
