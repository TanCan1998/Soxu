use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

mod execution;
mod identity;
mod state;

use identity::{AuthorizedDirectory, DirectoryIdentity, checked_directory};

use crate::probe::{COMMAND_TIMEOUT, OUTPUT_LIMIT};
use crate::{GitError, GitProbe, ProjectOpenError, WorktreeIdentity};

const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(100);
const MAX_REQUEST_BYTES: usize = 128;
const MAX_BRANCH_REF_BYTES: usize = 256;
const MAX_INTENT_BYTES: usize = 64 * 1024;
const INTENT_HEADER: &[u8] = b"SOXUWT3\0unix-dev-ino-birth\0";
const AUTHORIZED_IDENTITY_BYTES: usize = 5 * DirectoryIdentity::ENCODED_LEN;
const ALL_IDENTITY_BYTES: usize = 7 * DirectoryIdentity::ENCODED_LEN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeRegistrationRequest<'a> {
    pub request_id: &'a str,
    pub source: &'a WorktreeIdentity,
    pub workspace_root: &'a Path,
    pub target_name: &'a str,
    pub branch_ref: &'a str,
    pub base_oid: &'a str,
}

#[derive(Debug)]
pub struct WorktreeRegistrationCoordinator {
    state_root: AuthorizedDirectory,
}

impl WorktreeRegistrationCoordinator {
    /// 由可信宿主绑定所有注册进程和重启实例共用的私有持久目录。
    ///
    /// 宿主须先停止旧写者，并列出该域覆盖范围内的全部旧状态根。
    /// 其它根中的留痕保留原地。有留痕时拒绝启用写入。
    pub fn open(
        state_root: impl AsRef<Path>,
        legacy_state_roots: &[&Path],
    ) -> Result<Self, WorktreeLifecycleError> {
        let state_root = checked_directory(state_root.as_ref())?;
        state::require_private_root(&state_root)?;
        for legacy_root in legacy_state_roots {
            let legacy_root = checked_directory(legacy_root)?;
            if legacy_root.identity() != state_root.identity()
                && let Some(intents) = state::read_intents_directory(&legacy_root)?
                && fs::read_dir(intents.path())?.next().transpose()?.is_some()
            {
                return Err(WorktreeLifecycleError::LegacyStateRequiresReview);
            }
        }
        Ok(Self { state_root })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentIntent {
    pub request_id: String,
    pub source: WorktreeIdentity,
    pub workspace_root: PathBuf,
    pub target: PathBuf,
    pub branch_ref: String,
    pub base_oid: String,
    pub recorded_at: SystemTime,
    pub execution: ExecutionRecord,
    authorized_directories: Option<IntentDirectories>,
    registered_directories: Option<[DirectoryIdentity; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IntentDirectories {
    state: DirectoryIdentity,
    workspace: DirectoryIdentity,
    source: [DirectoryIdentity; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStage {
    Prepared,
    Started,
    Completed,
    Observed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRecord {
    pub stage: ExecutionStage,
    pub result: ExecutionResult,
    pub target_absent_before: bool,
    pub ref_absent_before: bool,
    pub started_at: Option<SystemTime>,
    pub observed_at: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationObservation {
    pub observed_at: SystemTime,
    pub ref_present: bool,
    pub ref_oid: Option<String>,
    pub target_registered: bool,
    pub target_matches: bool,
    pub target_identity: Option<WorktreeIdentity>,
    pub inventory: crate::WorktreeInventory,
    pub inventory_entry_count: usize,
    pub target_state: TargetState,
    pub locked: Option<String>,
    pub prunable: Option<String>,
    registered_directories: Option<[DirectoryIdentity; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetState {
    NotExamined,
    Absent,
    Directory,
    File,
    LinkOrReparse,
    Inaccessible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRegistration {
    pub intent: PersistentIntent,
    pub observation: RegistrationObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationOutcome {
    Verified(VerifiedRegistration),
    ExistingIntent(VerifiedRegistration),
    PartialFootprint {
        intent: PersistentIntent,
        observation: RegistrationObservation,
        command_succeeded: bool,
    },
    UnknownOutcome {
        intent: PersistentIntent,
        observation: Option<RegistrationObservation>,
        reason: &'static str,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum WorktreeLifecycleError {
    #[error("注册请求字段无效：{0}")]
    InvalidInput(&'static str),
    #[error("路径未通过授权边界检查：{0}")]
    Unauthorized(&'static str),
    #[error("目标根已被文件系统占用")]
    TargetExists,
    #[error("新分支引用已存在")]
    RefExists,
    #[error("目标根已被请求 {request_id} 预约")]
    TargetReserved { request_id: String },
    #[error("分支引用已被请求 {request_id} 预约")]
    RefReserved { request_id: String },
    #[error("同名请求携带了不同参数")]
    RequestConflict,
    #[error("持久状态目录正被其他注册进程使用")]
    StateBusy,
    #[error("Git 身份或授权目录在检查期间发生变化")]
    IdentityChanged,
    #[error("无法确认原注册进程已停止")]
    ExecutionStillUnknown,
    #[error("旧状态留痕无法绑定到当前协调域，需要复核")]
    LegacyStateRequiresReview,
    #[error("仓库配置包含当前注册底座不支持的来源：{0}")]
    UnsupportedConfiguration(&'static str),
    #[error("文件系统未提供本实现所需的平台原生能力：{0}")]
    UnsupportedFilesystem(&'static str),
    #[error("仓库对象来源不能在本地受控边界内证明：{0}")]
    UnsupportedObjectSource(&'static str),
    #[error("本轮尚不支持的基准输入：{0}")]
    UnsupportedBase(&'static str),
    #[error("工作树清单包含无法安全比较的路径")]
    AmbiguousRegisteredPath,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Project(#[from] ProjectOpenError),
    #[error(transparent)]
    WorktreeInventory(#[from] crate::WorktreeInventoryError),
}

impl GitProbe {
    /// 以持久意图为边界执行一次新分支 linked worktree 注册，不检出文件。
    pub async fn register_new_worktree(
        &self,
        coordinator: &WorktreeRegistrationCoordinator,
        request: WorktreeRegistrationRequest<'_>,
    ) -> Result<RegistrationOutcome, WorktreeLifecycleError> {
        let mut plan = validate_request(coordinator, &request)?;
        let lock = state::acquire_state_lock(&plan.state_root).await?;
        let intents = read_intents(&plan.state_root)?;
        if let Some(existing) = intents
            .iter()
            .find(|intent| intent.request_id == request.request_id)
        {
            if !same_binding(existing, &plan.intent) {
                return Err(WorktreeLifecycleError::RequestConflict);
            }
            if existing.authorized_directories.is_none() {
                return Ok(RegistrationOutcome::UnknownOutcome {
                    intent: existing.clone(),
                    observation: None,
                    reason: "旧版记录缺少目录身份，保留预约",
                });
            }
            if existing.authorized_directories != plan.intent.authorized_directories {
                return Err(WorktreeLifecycleError::IdentityChanged);
            }
            return observe_registration(
                self,
                &plan.state_root,
                &plan.workspace_root,
                existing.clone(),
                None,
            )
            .await;
        }
        check_reservations(&plan, &intents)?;

        let before = preflight(self, &plan).await?;
        lock.refresh(&plan.state_root)?;
        let mut persisted = plan.intent.clone();
        persisted.execution.target_absent_before = before.target_absent;
        persisted.execution.ref_absent_before = before.ref_absent;
        write_intent(&plan.state_root, &persisted)?;
        plan.intent.execution = persisted.execution.clone();
        let execution_task = tokio::spawn(execute_new_worktree(self.clone(), plan.clone(), lock));
        match execution_task.await {
            Ok(result) => result,
            Err(_) => {
                plan.state_root.refresh()?;
                let path = state::intent_path(&plan.state_root, request.request_id);
                let intent = decode_intent(&state::read_regular_file(&path)?)?;
                if !same_binding(&intent, &plan.intent)
                    || intent.authorized_directories != plan.intent.authorized_directories
                {
                    return Err(WorktreeLifecycleError::RequestConflict);
                }
                Ok(RegistrationOutcome::UnknownOutcome {
                    intent,
                    observation: None,
                    reason: "写执行所有者结果不可用，保留持久预约",
                })
            }
        }
    }

    /// 重启后按请求 ID 做只读复核，不重新执行 Git 写入。
    pub async fn diagnose_registration(
        &self,
        state_root: impl AsRef<Path>,
        request_id: &str,
        source_root: impl AsRef<Path>,
    ) -> Result<Option<RegistrationObservation>, WorktreeLifecycleError> {
        self.diagnose_registration_inner(state_root, request_id, source_root, None)
            .await
    }

    /// 显式授权单个目标后，做只读目标核验。
    pub async fn diagnose_registration_with_target(
        &self,
        state_root: impl AsRef<Path>,
        request_id: &str,
        source_root: impl AsRef<Path>,
        target_authorization: impl AsRef<Path>,
    ) -> Result<Option<RegistrationObservation>, WorktreeLifecycleError> {
        self.diagnose_registration_inner(
            state_root,
            request_id,
            source_root,
            Some(target_authorization.as_ref()),
        )
        .await
    }

    async fn diagnose_registration_inner(
        &self,
        state_root: impl AsRef<Path>,
        request_id: &str,
        source_root: impl AsRef<Path>,
        target_authorization: Option<&Path>,
    ) -> Result<Option<RegistrationObservation>, WorktreeLifecycleError> {
        let state_root = checked_directory(state_root.as_ref())?;
        let source_root = checked_directory(source_root.as_ref())?;
        validate_request_id(request_id)?;
        let intent = read_intents(&state_root)?
            .into_iter()
            .find(|intent| intent.request_id == request_id);
        let Some(intent) = intent else {
            return Ok(None);
        };
        if !path_components_equivalent(&intent.source.root, source_root.path()) {
            return Err(WorktreeLifecycleError::Unauthorized("请求不属于该源根"));
        }
        let directories = intent
            .authorized_directories
            .ok_or(WorktreeLifecycleError::LegacyStateRequiresReview)?;
        if directories.state != state_root.identity() {
            return Err(WorktreeLifecycleError::IdentityChanged);
        }
        if stop_unconfirmed(&intent) {
            return Err(WorktreeLifecycleError::ExecutionStillUnknown);
        }
        let target_workspace = match target_authorization {
            None => None,
            Some(authorized) => {
                let workspace_root = checked_directory(&intent.workspace_root)?;
                if !path_components_equivalent(authorized, &intent.target)
                    || !authorized.parent().is_some_and(|parent| {
                        path_components_equivalent(parent, workspace_root.path())
                    })
                {
                    return Err(WorktreeLifecycleError::Unauthorized("目标授权不匹配"));
                }
                Some(workspace_root)
            }
        };
        verify_intent(self, &intent, target_workspace.as_ref())
            .await
            .map(Some)
    }
}

fn check_reservations(
    plan: &RegistrationPlan,
    intents: &[PersistentIntent],
) -> Result<(), WorktreeLifecycleError> {
    for intent in intents {
        let directories = intent
            .authorized_directories
            .filter(|dirs| dirs.state == plan.state_root.identity())
            .ok_or(WorktreeLifecycleError::LegacyStateRequiresReview)?;
        let matching_name = intent
            .target
            .file_name()
            .zip(plan.intent.target.file_name())
            .is_some_and(|(old, new)| {
                old.as_encoded_bytes()
                    .eq_ignore_ascii_case(new.as_encoded_bytes())
            });
        let same_target = directories.workspace == plan.workspace_root.identity() && matching_name;
        if same_target || paths_equivalent(&intent.target, &plan.intent.target) {
            return Err(WorktreeLifecycleError::TargetReserved {
                request_id: intent.request_id.clone(),
            });
        }
        let same_repository = directories.source[2] == plan.source.common_dir.identity();
        if same_repository && branch_refs_conflict(&intent.branch_ref, &plan.intent.branch_ref) {
            return Err(WorktreeLifecycleError::RefReserved {
                request_id: intent.request_id.clone(),
            });
        }
        if stop_unconfirmed(intent)
            && (same_repository
                || paths_equivalent(&intent.source.common_dir, &plan.intent.source.common_dir))
        {
            return Err(WorktreeLifecycleError::ExecutionStillUnknown);
        }
    }
    Ok(())
}

async fn execute_new_worktree(
    git: GitProbe,
    plan: RegistrationPlan,
    lock: state::StateLock,
) -> Result<RegistrationOutcome, WorktreeLifecycleError> {
    let branch_short = plan.intent.branch_ref.strip_prefix("refs/heads/").ok_or(
        WorktreeLifecycleError::InvalidInput("分支必须位于 refs/heads 下"),
    )?;
    preflight(&git, &plan).await?;

    let target_arg = native_command_path(&plan.intent.target)?;
    let mut command = git.command(plan.source.root.path());
    command
        .arg("--no-replace-objects")
        .arg("-c")
        .arg("core.hooksPath=")
        .arg("worktree")
        .arg("add")
        .args(["--no-checkout", "--no-track", "-b", branch_short, "--"])
        .arg(&target_arg)
        .arg(&plan.intent.base_oid);
    let mut started = plan.intent.clone();
    started.execution.stage = ExecutionStage::Started;
    started.execution.started_at = Some(SystemTime::now());
    started.execution.result = ExecutionResult::Unknown {
        stop_confirmed: false,
        reason: "running".to_owned(),
    };
    lock.refresh(&plan.state_root)?;
    rewrite_intent(&plan.state_root, &started)?;

    lock.refresh(&plan.state_root)?;
    if let Err(error) = refresh_registration_boundaries(&plan)
        .and_then(|()| target_absent(&plan.intent.target).map(|_| ()))
    {
        rewrite_intent(&plan.state_root, &plan.intent)?;
        return Err(error);
    }
    let mut executed = started;
    executed.execution.stage = ExecutionStage::Completed;
    executed.execution.result = execution::run(&mut command, OUTPUT_LIMIT, COMMAND_TIMEOUT).await;
    if executed.execution.result == ExecutionResult::SpawnFailed {
        executed.execution.started_at = None;
    }
    lock.refresh(&plan.state_root)?;
    rewrite_intent(&plan.state_root, &executed)?;
    observe_registration(
        &git,
        &plan.state_root,
        &plan.workspace_root,
        executed,
        Some(&lock),
    )
    .await
}

fn stop_unconfirmed(intent: &PersistentIntent) -> bool {
    intent.execution.stage == ExecutionStage::Started
        || matches!(
            intent.execution.result,
            ExecutionResult::Unknown {
                stop_confirmed: false,
                ..
            }
        )
}

async fn observe_registration(
    git: &GitProbe,
    state_root: &AuthorizedDirectory,
    workspace_root: &AuthorizedDirectory,
    mut intent: PersistentIntent,
    lock: Option<&state::StateLock>,
) -> Result<RegistrationOutcome, WorktreeLifecycleError> {
    let newly_executed = lock.is_some();
    if stop_unconfirmed(&intent) {
        return Ok(RegistrationOutcome::UnknownOutcome {
            intent,
            observation: None,
            reason: "无法确认注册进程已停止，保留预约",
        });
    }
    if newly_executed && intent.execution.result == ExecutionResult::SpawnFailed {
        return Ok(RegistrationOutcome::UnknownOutcome {
            intent,
            observation: None,
            reason: "注册进程启动失败",
        });
    }
    let command_succeeded = intent.execution.result == ExecutionResult::Succeeded;
    let observation = match verify_intent(git, &intent, Some(workspace_root)).await {
        Ok(observation) => observation,
        Err(error) => {
            return Ok(RegistrationOutcome::UnknownOutcome {
                intent,
                observation: None,
                reason: execution_error_reason(&error),
            });
        }
    };
    let can_verify =
        command_succeeded && (newly_executed || intent.registered_directories.is_some());
    let class = classification(&intent, &observation, can_verify);
    if let Some(lock) = lock {
        intent.execution.stage = ExecutionStage::Observed;
        intent.execution.observed_at = Some(observation.observed_at);
        if matches!(class, OutcomeClass::Verified) {
            intent.registered_directories = observation.registered_directories;
        }
        lock.refresh(state_root)?;
        rewrite_intent(state_root, &intent)?;
    }
    Ok(match class {
        OutcomeClass::Verified if newly_executed => {
            RegistrationOutcome::Verified(VerifiedRegistration {
                intent,
                observation,
            })
        }
        OutcomeClass::Verified => RegistrationOutcome::ExistingIntent(VerifiedRegistration {
            intent,
            observation,
        }),
        OutcomeClass::Partial => RegistrationOutcome::PartialFootprint {
            intent,
            observation,
            command_succeeded,
        },
        OutcomeClass::Unknown(reason) => RegistrationOutcome::UnknownOutcome {
            intent,
            observation: Some(observation),
            reason,
        },
    })
}

#[derive(Clone)]
struct RegistrationPlan {
    state_root: AuthorizedDirectory,
    source: SourceIdentity,
    workspace_root: AuthorizedDirectory,
    intent: PersistentIntent,
}

enum OutcomeClass {
    Verified,
    Partial,
    Unknown(&'static str),
}

#[derive(Clone)]
struct SourceIdentity {
    root: AuthorizedDirectory,
    git_dir: AuthorizedDirectory,
    common_dir: AuthorizedDirectory,
}

impl SourceIdentity {
    fn identities(&self) -> [DirectoryIdentity; 3] {
        [
            self.root.identity(),
            self.git_dir.identity(),
            self.common_dir.identity(),
        ]
    }

    fn refresh(&self) -> Result<(), WorktreeLifecycleError> {
        self.root.refresh()?;
        self.git_dir.refresh()?;
        self.common_dir.refresh()
    }
}

fn validate_request(
    coordinator: &WorktreeRegistrationCoordinator,
    request: &WorktreeRegistrationRequest<'_>,
) -> Result<RegistrationPlan, WorktreeLifecycleError> {
    validate_request_id(request.request_id)?;
    validate_target_name(request.target_name)?;
    validate_branch_ref(request.branch_ref)?;
    let state_root = coordinator.state_root.clone();
    let workspace_root = checked_directory(request.workspace_root)?;
    let source = canonical_source_identity(request.source)?;
    let workspace_path = workspace_root.path().to_path_buf();
    let source_root = source.root.path();
    let source_common = source.common_dir.path();
    if path_related(&workspace_path, source_root)
        || path_related(&workspace_path, source_common)
        || path_related(source_root, &workspace_path)
        || path_related(source_common, &workspace_path)
    {
        return Err(WorktreeLifecycleError::Unauthorized(
            "工作区根必须与源工作树隔离",
        ));
    }
    let state_path = state_root.path();
    for directory in [
        &source.root,
        &source.git_dir,
        &source.common_dir,
        &workspace_root,
    ] {
        if state_root.identity() == directory.identity()
            || path_related(state_path, directory.path())
            || path_related(directory.path(), state_path)
        {
            return Err(WorktreeLifecycleError::Unauthorized(
                "状态目录必须与仓库和工作区隔离",
            ));
        }
    }
    let target = workspace_path.join(request.target_name);
    if !is_full_oid(request.base_oid.as_bytes()) {
        return Err(WorktreeLifecycleError::InvalidInput(
            "基准必须是完整提交 OID",
        ));
    }
    let authorized_directories = Some(IntentDirectories {
        state: state_root.identity(),
        workspace: workspace_root.identity(),
        source: source.identities(),
    });
    Ok(RegistrationPlan {
        state_root,
        source,
        workspace_root,
        intent: PersistentIntent {
            request_id: request.request_id.to_owned(),
            source: request.source.clone(),
            workspace_root: workspace_path.to_path_buf(),
            target,
            branch_ref: request.branch_ref.to_owned(),
            base_oid: request.base_oid.to_ascii_lowercase(),
            recorded_at: SystemTime::now(),
            authorized_directories,
            registered_directories: None,
            execution: ExecutionRecord {
                stage: ExecutionStage::Prepared,
                result: ExecutionResult::NotStarted,
                target_absent_before: false,
                ref_absent_before: false,
                started_at: None,
                observed_at: None,
            },
        },
    })
}

fn canonical_source_identity(
    source: &WorktreeIdentity,
) -> Result<SourceIdentity, WorktreeLifecycleError> {
    let root = checked_directory(&source.root)?;
    let git_dir = checked_directory(&source.git_dir)?;
    let common_dir = checked_directory(&source.common_dir)?;
    if !path_components_equivalent(root.path(), &source.root)
        || !path_components_equivalent(git_dir.path(), &source.git_dir)
        || !path_components_equivalent(common_dir.path(), &source.common_dir)
    {
        return Err(WorktreeLifecycleError::Unauthorized(
            "Git 身份路径必须是规范路径",
        ));
    }
    if !path_related(git_dir.path(), common_dir.path()) {
        return Err(WorktreeLifecycleError::Unauthorized(
            "工作树 Git 目录不在共享仓库内",
        ));
    }
    Ok(SourceIdentity {
        root,
        git_dir,
        common_dir,
    })
}

fn path_related(left: &Path, right: &Path) -> bool {
    let left = left.components().collect::<Vec<_>>();
    let right = right.components().collect::<Vec<_>>();
    path_component_groups_equivalent(&left, &right)
        || (1..=left.len().min(right.len())).any(|length| {
            path_component_groups_equivalent(&left[..length], &right)
                || path_component_groups_equivalent(&left, &right[..length])
        })
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    path_components_equivalent(left, right)
}

fn path_components_equivalent(left: &Path, right: &Path) -> bool {
    path_component_groups_equivalent(
        &left.components().collect::<Vec<_>>(),
        &right.components().collect::<Vec<_>>(),
    )
}

fn path_component_groups_equivalent(left: &[Component<'_>], right: &[Component<'_>]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left_component, right_component)| {
                match (
                    path_component_bytes(left_component),
                    path_component_bytes(right_component),
                ) {
                    (Some(left_bytes), Some(right_bytes)) => {
                        if left_bytes.is_ascii() && right_bytes.is_ascii() {
                            left_bytes.eq_ignore_ascii_case(right_bytes)
                        } else {
                            left_bytes == right_bytes
                        }
                    }
                    (None, None) => left_component == right_component,
                    _ => false,
                }
            })
}

fn path_component_bytes<'a>(component: &'a Component<'a>) -> Option<&'a [u8]> {
    match component {
        Component::Normal(path) => Some(path.as_encoded_bytes()),
        _ => None,
    }
}

fn branch_refs_equivalent(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right) && left.is_ascii() && right.is_ascii()
}

fn branch_refs_conflict(left: &str, right: &str) -> bool {
    branch_refs_equivalent(left, right)
        || left
            .strip_prefix(right)
            .is_some_and(|remainder| remainder.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn same_binding(left: &PersistentIntent, right: &PersistentIntent) -> bool {
    left.request_id == right.request_id
        && paths_equivalent(&left.source.root, &right.source.root)
        && paths_equivalent(&left.source.git_dir, &right.source.git_dir)
        && paths_equivalent(&left.source.common_dir, &right.source.common_dir)
        && paths_equivalent(&left.workspace_root, &right.workspace_root)
        && paths_equivalent(&left.target, &right.target)
        && branch_refs_equivalent(&left.branch_ref, &right.branch_ref)
        && left.base_oid == right.base_oid
}

fn validate_request_id(request_id: &str) -> Result<(), WorktreeLifecycleError> {
    if request_id.is_empty()
        || request_id.len() > MAX_REQUEST_BYTES
        || !request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        || request_id.starts_with('.')
    {
        return Err(WorktreeLifecycleError::InvalidInput("请求 ID"));
    }
    Ok(())
}

fn validate_target_name(target_name: &str) -> Result<(), WorktreeLifecycleError> {
    let stem = target_name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    let valid = !target_name.is_empty()
        && target_name.len() <= 64
        && target_name != "."
        && target_name != ".."
        && !target_name.contains("..")
        && target_name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
        && target_name
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && !target_name.ends_with('-')
        && !reserved;
    if valid {
        Ok(())
    } else {
        Err(WorktreeLifecycleError::InvalidInput("目标目录名"))
    }
}

fn validate_branch_ref(branch_ref: &str) -> Result<(), WorktreeLifecycleError> {
    let Some(short_name) = branch_ref.strip_prefix("refs/heads/soxu/") else {
        return Err(WorktreeLifecycleError::InvalidInput(
            "分支必须位于 refs/heads/soxu 下",
        ));
    };
    let valid = !short_name.is_empty()
        && branch_ref.len() <= MAX_BRANCH_REF_BYTES
        && branch_ref.is_ascii()
        && short_name.split('/').all(|segment| {
            !segment.is_empty()
                && segment.len() <= 64
                && !segment.ends_with('.')
                && !segment.ends_with(".lock")
                && !segment.starts_with('-')
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                })
        });
    if valid {
        Ok(())
    } else {
        Err(WorktreeLifecycleError::InvalidInput("分支引用"))
    }
}

fn is_full_oid(bytes: &[u8]) -> bool {
    matches!(bytes.len(), 40 | 64)
        && bytes.iter().all(u8::is_ascii_hexdigit)
        && !bytes.iter().all(|byte| *byte == b'0')
}

fn native_command_path(path: &Path) -> Result<PathBuf, WorktreeLifecycleError> {
    let native = native_git_path(path);
    if native.as_os_str().to_str().is_none() {
        return Err(WorktreeLifecycleError::Unauthorized(
            "目标路径必须可无损传给 Git",
        ));
    }
    Ok(native)
}

fn native_git_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let stripped = path.to_str().and_then(|text| text.strip_prefix(r"\\?\"));
        if let Some(drive_path) = stripped.filter(|drive_path| {
            drive_path
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic())
                && drive_path.as_bytes().get(1) == Some(&b':')
        }) {
            return PathBuf::from(drive_path);
        }
        path.to_owned()
    }
    #[cfg(not(windows))]
    {
        path.to_owned()
    }
}

fn write_intent(
    state_root: &AuthorizedDirectory,
    intent: &PersistentIntent,
) -> Result<(), WorktreeLifecycleError> {
    let bytes = encode_intent(intent)?;
    state::write_intent(state_root, &bytes, &intent.request_id)
}

fn rewrite_intent(
    state_root: &AuthorizedDirectory,
    intent: &PersistentIntent,
) -> Result<(), WorktreeLifecycleError> {
    let bytes = encode_intent(intent)?;
    state::rewrite_intent(state_root, &bytes, &intent.request_id)
}

fn read_intents(
    state_root: &AuthorizedDirectory,
) -> Result<Vec<PersistentIntent>, WorktreeLifecycleError> {
    let Some(intents) = state::read_intents_directory(state_root)? else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    let entries = fs::read_dir(intents.path())?;
    let mut total_bytes = 0usize;
    for (index, entry) in entries.enumerate() {
        if index >= state::MAX_INTENT_FILES {
            return Err(WorktreeLifecycleError::InvalidInput("意图文件总量超限"));
        }
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "intent")
        {
            let bytes = state::read_regular_file(&path)?;
            total_bytes = total_bytes.saturating_add(bytes.len());
            if total_bytes > state::MAX_TOTAL_INTENT_BYTES {
                return Err(WorktreeLifecycleError::InvalidInput("意图字节总量超限"));
            }
            let intent = decode_intent(&bytes)?;
            validate_request_id(&intent.request_id)?;
            if path.file_stem().and_then(|name| name.to_str()) != Some(intent.request_id.as_str()) {
                return Err(WorktreeLifecycleError::InvalidInput(
                    "意图文件名与请求 ID 不一致",
                ));
            }
            result.push(intent);
        }
    }
    result.sort_by(|left, right| left.request_id.cmp(&right.request_id));
    Ok(result)
}

fn encode_intent(intent: &PersistentIntent) -> Result<Vec<u8>, WorktreeLifecycleError> {
    let stage = match intent.execution.stage {
        ExecutionStage::Prepared => "prepared",
        ExecutionStage::Started => "started",
        ExecutionStage::Completed => "completed",
        ExecutionStage::Observed => "observed",
    };
    let result = match &intent.execution.result {
        ExecutionResult::NotStarted => "not_started".to_owned(),
        ExecutionResult::SpawnFailed => "spawn_failed".to_owned(),
        ExecutionResult::Succeeded => "succeeded".to_owned(),
        ExecutionResult::Failed { exit_code } => match exit_code {
            Some(code) => format!("failed:{code}"),
            None => "failed:none".to_owned(),
        },
        ExecutionResult::Unknown {
            stop_confirmed,
            reason,
        } => format!("unknown:{}:{reason}", if *stop_confirmed { 1 } else { 0 }),
    };
    let time_text = |time: Option<SystemTime>| -> Result<String, WorktreeLifecycleError> {
        match time {
            None => Ok(String::new()),
            Some(time) => Ok(time
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|_| WorktreeLifecycleError::InvalidInput("系统时间"))?
                .as_nanos()
                .to_string()),
        }
    };
    let started_text = time_text(intent.execution.started_at)?;
    let observed_text = time_text(intent.execution.observed_at)?;
    let recorded_text = time_text(Some(intent.recorded_at))?;
    let strings = [
        intent.request_id.as_bytes(),
        path_bytes(&intent.source.root)?,
        path_bytes(&intent.source.git_dir)?,
        path_bytes(&intent.source.common_dir)?,
        path_bytes(&intent.workspace_root)?,
        path_bytes(&intent.target)?,
        intent.branch_ref.as_bytes(),
        intent.base_oid.as_bytes(),
        stage.as_bytes(),
        result.as_bytes(),
        if intent.execution.target_absent_before {
            b"1"
        } else {
            b"0"
        },
        if intent.execution.ref_absent_before {
            b"1"
        } else {
            b"0"
        },
        started_text.as_bytes(),
        observed_text.as_bytes(),
        recorded_text.as_bytes(),
    ];
    let dirs = intent
        .authorized_directories
        .ok_or(WorktreeLifecycleError::IdentityChanged)?;
    let mut output = INTENT_HEADER.to_vec();
    for field in strings {
        output.extend((field.len() as u64).to_le_bytes());
        output.extend_from_slice(field);
    }
    for identity in [dirs.state, dirs.workspace].into_iter().chain(dirs.source) {
        output.extend(identity.to_bytes());
    }
    if let Some(pair) = intent.registered_directories {
        for identity in pair {
            output.extend(identity.to_bytes());
        }
    }
    if output.len() > MAX_INTENT_BYTES {
        return Err(WorktreeLifecycleError::InvalidInput("意图记录超过大小限制"));
    }
    Ok(output)
}

fn path_bytes(path: &Path) -> Result<&[u8], WorktreeLifecycleError> {
    std::str::from_utf8(path.as_os_str().as_encoded_bytes())
        .map(str::as_bytes)
        .map_err(|_| WorktreeLifecycleError::Unauthorized("持久路径必须为 UTF-8"))
}

fn decode_intent(bytes: &[u8]) -> Result<PersistentIntent, WorktreeLifecycleError> {
    let (mut cursor, has_identity) = if let Some(body) = bytes.strip_prefix(INTENT_HEADER) {
        (body, true)
    } else if let Some(body) = bytes.strip_prefix(b"SOXUWT2\0") {
        (body, false)
    } else {
        return Err(WorktreeLifecycleError::InvalidInput("意图记录头"));
    };
    let mut fields = Vec::new();
    for _ in 0..15 {
        let Some((length_bytes, rest)) = cursor.split_first_chunk::<8>() else {
            return Err(WorktreeLifecycleError::InvalidInput("意图字段长度"));
        };
        let length = usize::try_from(u64::from_le_bytes(*length_bytes))
            .map_err(|_| WorktreeLifecycleError::InvalidInput("意图字段长度"))?;
        let Some((field, rest)) = rest.split_at_checked(length) else {
            return Err(WorktreeLifecycleError::InvalidInput("意图字段截断"));
        };
        fields.push(field);
        cursor = rest;
    }
    let (authorized_directories, registered_directories) = match (has_identity, cursor.len()) {
        (false, 0) => (None, None),
        (true, AUTHORIZED_IDENTITY_BYTES | ALL_IDENTITY_BYTES) => {
            let (chunks, _) = cursor.as_chunks::<{ DirectoryIdentity::ENCODED_LEN }>();
            let mut identities = chunks.iter().map(|field| {
                DirectoryIdentity::from_bytes(field)
                    .ok_or(WorktreeLifecycleError::InvalidInput("意图目录身份"))
            });
            let mut next = || {
                identities
                    .next()
                    .ok_or(WorktreeLifecycleError::InvalidInput("缺少意图目录身份"))?
            };
            let dirs = IntentDirectories {
                state: next()?,
                workspace: next()?,
                source: [next()?, next()?, next()?],
            };
            let target = if cursor.len() == ALL_IDENTITY_BYTES {
                Some([next()?, next()?])
            } else {
                None
            };
            (Some(dirs), target)
        }
        _ => return Err(WorktreeLifecycleError::InvalidInput("意图记录尾随数据")),
    };
    let path = |field: &[u8]| -> Result<PathBuf, WorktreeLifecycleError> {
        let text = std::str::from_utf8(field)
            .map_err(|_| WorktreeLifecycleError::InvalidInput("意图路径编码"))?;
        Ok(PathBuf::from(text))
    };
    let text = |field: &[u8]| -> Result<String, WorktreeLifecycleError> {
        String::from_utf8(field.to_vec())
            .map_err(|_| WorktreeLifecycleError::InvalidInput("意图文本编码"))
    };
    let time = |field: &[u8]| -> Result<Option<SystemTime>, WorktreeLifecycleError> {
        if field.is_empty() {
            return Ok(None);
        }
        let text = std::str::from_utf8(field)
            .map_err(|_| WorktreeLifecycleError::InvalidInput("意图时间"))?;
        let nanos = text
            .parse::<u128>()
            .map_err(|_| WorktreeLifecycleError::InvalidInput("意图时间"))?;
        let time = SystemTime::UNIX_EPOCH.checked_add(Duration::from_nanos(
            u64::try_from(nanos).map_err(|_| WorktreeLifecycleError::InvalidInput("意图时间"))?,
        ));
        Ok(time)
    };
    let execution = ExecutionRecord {
        stage: match text(fields[8])?.as_str() {
            "prepared" => ExecutionStage::Prepared,
            "started" => ExecutionStage::Started,
            "completed" => ExecutionStage::Completed,
            "observed" => ExecutionStage::Observed,
            _ => return Err(WorktreeLifecycleError::InvalidInput("执行阶段")),
        },
        result: {
            let result = text(fields[9])?;
            match result.as_str() {
                "not_started" => ExecutionResult::NotStarted,
                "spawn_failed" => ExecutionResult::SpawnFailed,
                "succeeded" => ExecutionResult::Succeeded,
                "failed:none" => ExecutionResult::Failed { exit_code: None },
                failed
                    if failed
                        .strip_prefix("failed:")
                        .is_some_and(|code| code.parse::<i32>().is_ok()) =>
                {
                    ExecutionResult::Failed {
                        exit_code: failed
                            .strip_prefix("failed:")
                            .and_then(|code| code.parse().ok()),
                    }
                }
                unknown if unknown.starts_with("unknown:") => {
                    let value = unknown.strip_prefix("unknown:").unwrap_or_default();
                    let Some((stop_confirmed, reason)) = value.split_once(':') else {
                        return Err(WorktreeLifecycleError::InvalidInput("执行结果"));
                    };
                    if reason.is_empty() || reason.contains(':') {
                        return Err(WorktreeLifecycleError::InvalidInput("执行结果"));
                    }
                    ExecutionResult::Unknown {
                        stop_confirmed: match stop_confirmed {
                            "0" => false,
                            "1" => true,
                            _ => return Err(WorktreeLifecycleError::InvalidInput("执行结果")),
                        },
                        reason: reason.to_owned(),
                    }
                }
                _ => return Err(WorktreeLifecycleError::InvalidInput("执行结果")),
            }
        },
        target_absent_before: fields[10] == b"1",
        ref_absent_before: fields[11] == b"1",
        started_at: time(fields[12])?,
        observed_at: time(fields[13])?,
    };
    if (fields[10].is_empty() || fields[11].is_empty())
        || (fields[10] != b"0" && fields[10] != b"1")
        || (fields[11] != b"0" && fields[11] != b"1")
    {
        return Err(WorktreeLifecycleError::InvalidInput("执行前观测"));
    }
    Ok(PersistentIntent {
        request_id: text(fields[0])?,
        source: WorktreeIdentity {
            root: path(fields[1])?,
            git_dir: path(fields[2])?,
            common_dir: path(fields[3])?,
        },
        workspace_root: path(fields[4])?,
        target: path(fields[5])?,
        branch_ref: text(fields[6])?,
        base_oid: text(fields[7])?,
        recorded_at: time(fields[14])?.ok_or(WorktreeLifecycleError::InvalidInput("意图时间"))?,
        execution,
        authorized_directories,
        registered_directories,
    })
}

struct PreflightObservation {
    target_absent: bool,
    ref_absent: bool,
}

async fn preflight(
    git: &GitProbe,
    plan: &RegistrationPlan,
) -> Result<PreflightObservation, WorktreeLifecycleError> {
    refresh_registration_boundaries(plan)?;
    verify_direct_metadata(git, &plan.source).await?;
    let source_root = plan.source.root.path();
    let reopened = git.open_local_project(source_root).await?;
    let crate::LocalProject::GitWorktree(identity) = reopened else {
        return Err(WorktreeLifecycleError::IdentityChanged);
    };
    if !path_components_equivalent(&identity.root, source_root)
        || !path_components_equivalent(&identity.git_dir, plan.source.git_dir.path())
        || !path_components_equivalent(&identity.common_dir, plan.source.common_dir.path())
    {
        return Err(WorktreeLifecycleError::IdentityChanged);
    }
    drop(fs::read_dir(plan.workspace_root.path())?);
    let inventory = git.list_worktrees(source_root).await?;
    for entry in &inventory.entries {
        let path = entry
            .path
            .to_path_buf()
            .map_err(|_| WorktreeLifecycleError::AmbiguousRegisteredPath)?;
        let path = native_git_path(&path);
        let workspace = native_git_path(plan.workspace_root.path());
        let target = native_git_path(&plan.intent.target);
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(WorktreeLifecycleError::AmbiguousRegisteredPath);
        }
        let sibling = matches!(entry.kind, crate::WorktreeKind::Linked)
            && path
                .parent()
                .is_some_and(|parent| paths_equivalent(parent, &workspace))
            && !paths_equivalent(&path, &target);
        if path_related(&target, &path) || (path_related(&workspace, &path) && !sibling) {
            return Err(WorktreeLifecycleError::Unauthorized(
                "工作区根与其他 Git 注册路径重叠",
            ));
        }
    }
    let check_ref = git
        .run_command(
            source_root,
            "check-ref-format",
            &[plan.intent.branch_ref.as_str()],
        )
        .await?;
    if check_ref.status.code() != Some(0) {
        return Err(WorktreeLifecycleError::InvalidInput(
            "分支引用未通过 Git 检查",
        ));
    }
    let replace_refs = read_success(
        git,
        source_root,
        "for-each-ref",
        &["--format=%(refname)", "refs/replace/"],
    )
    .await?;
    if !replace_refs.is_empty() {
        return Err(WorktreeLifecycleError::UnsupportedObjectSource("替换对象"));
    }
    let object_format =
        read_success(git, source_root, "rev-parse", &["--show-object-format"]).await?;
    let expected_width = match object_format.as_slice() {
        b"sha1\n" => 40,
        b"sha256\n" => 64,
        _ => return Err(WorktreeLifecycleError::UnsupportedConfiguration("对象格式")),
    };
    if plan.intent.base_oid.len() != expected_width {
        return Err(WorktreeLifecycleError::InvalidInput(
            "基准 OID 与对象格式不一致",
        ));
    }
    let query = format!("{}^{{commit}}", plan.intent.base_oid);
    let resolved = read_success(
        git,
        source_root,
        "rev-parse",
        &["--verify", "--end-of-options", &query],
    )
    .await?;
    if resolved.strip_suffix(b"\n") != Some(plan.intent.base_oid.as_bytes()) {
        return Err(WorktreeLifecycleError::IdentityChanged);
    }
    if ref_presence(git, &plan.intent.branch_ref, source_root).await? == RefPresence::Present {
        return Err(WorktreeLifecycleError::RefExists);
    }
    refresh_registration_boundaries(plan)?;
    let target_absent = target_absent(&plan.intent.target)?;
    Ok(PreflightObservation {
        target_absent,
        ref_absent: true,
    })
}

fn refresh_registration_boundaries(plan: &RegistrationPlan) -> Result<(), WorktreeLifecycleError> {
    plan.state_root.refresh()?;
    plan.workspace_root.refresh()?;
    plan.source.refresh()
}

fn target_absent(target: &Path) -> Result<bool, WorktreeLifecycleError> {
    match target.symlink_metadata() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
        Ok(_) => Err(WorktreeLifecycleError::TargetExists),
    }
}

async fn verify_direct_metadata(
    git: &GitProbe,
    source: &SourceIdentity,
) -> Result<(), WorktreeLifecycleError> {
    let common_dir = source.common_dir.path();
    let git_dir = source.git_dir.path();
    let objects = checked_directory(&common_dir.join("objects"))?;
    for name in ["info", "pack"] {
        let directory = checked_directory(&objects.path().join(name))?;
        if !directory.path().is_dir() {
            return Err(WorktreeLifecycleError::UnsupportedObjectSource("对象目录"));
        }
    }
    for path in [
        common_dir.join("info").join("grafts"),
        git_dir.join("info").join("grafts"),
    ] {
        if path.symlink_metadata().is_ok() {
            return Err(WorktreeLifecycleError::UnsupportedObjectSource(
                "grafts 或外部配置来源",
            ));
        }
    }
    for path in [
        objects.path().join("info").join("alternates"),
        objects.path().join("info").join("http-alternates"),
        objects.path().join("info").join("promisor"),
    ] {
        if path.symlink_metadata().is_ok() {
            return Err(WorktreeLifecycleError::UnsupportedObjectSource(
                "外部或 promisor 对象来源",
            ));
        }
    }
    let worktree_config = git_dir.join("config.worktree");
    check_regular_file(&worktree_config, true)?;
    verify_config_file(git, source.root.path(), &common_dir.join("config")).await?;
    if worktree_config.symlink_metadata().is_ok() {
        verify_config_file(git, source.root.path(), &worktree_config).await?;
    }
    check_packs(&objects.path().join("pack"))?;
    Ok(())
}

fn check_regular_file(path: &Path, allow_missing: bool) -> Result<(), WorktreeLifecycleError> {
    match path.symlink_metadata() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if allow_missing {
                Ok(())
            } else {
                Err(error.into())
            }
        }
        Err(error) => Err(error.into()),
        Ok(metadata) => {
            if !metadata.is_file() || metadata.is_symlink() || identity::is_reparse(&metadata) {
                Err(WorktreeLifecycleError::UnsupportedConfiguration(
                    "元数据类型",
                ))
            } else {
                Ok(())
            }
        }
    }
}

async fn verify_config_file(
    git: &GitProbe,
    root: &Path,
    path: &Path,
) -> Result<(), WorktreeLifecycleError> {
    check_regular_file(path, false)?;
    let command_path = native_command_path(path)?;
    let file = command_path
        .to_str()
        .ok_or(WorktreeLifecycleError::Unauthorized("配置路径编码"))?;
    let config = read_success(
        git,
        root,
        "config",
        &["--file", file, "--null", "--list", "--no-includes"],
    )
    .await?;
    check_config(&config)
}

fn check_packs(directory: &Path) -> Result<(), WorktreeLifecycleError> {
    let entries = fs::read_dir(directory)?;
    for (index, entry) in entries.enumerate() {
        if index >= 4096 {
            return Err(WorktreeLifecycleError::InvalidInput("对象包目录项总量超限"));
        }
        let path = entry?.path();
        let metadata = path.symlink_metadata()?;
        if metadata.is_symlink() || identity::is_reparse(&metadata) {
            return Err(WorktreeLifecycleError::UnsupportedObjectSource(
                "对象包路径",
            ));
        }
        if path
            .extension()
            .is_some_and(|extension| extension == "promisor")
        {
            return Err(WorktreeLifecycleError::UnsupportedObjectSource(
                "promisor 对象包",
            ));
        }
    }
    Ok(())
}

async fn read_success(
    git: &GitProbe,
    root: &Path,
    operation: &'static str,
    arguments: &[&str],
) -> Result<Vec<u8>, WorktreeLifecycleError> {
    let output = git.run_command(root, operation, arguments).await?;
    read_success_output(operation, output)
}

async fn read_success_bytes(
    git: &GitProbe,
    root: &Path,
    operation: &'static str,
    arguments: &[&str],
) -> Result<Vec<u8>, WorktreeLifecycleError> {
    let output = git.run_command(root, operation, arguments).await?;
    read_success_output(operation, output)
}

fn read_success_output(
    operation: &'static str,
    output: std::process::Output,
) -> Result<Vec<u8>, WorktreeLifecycleError> {
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(GitError::CommandFailed {
            operation,
            code: output.status.code(),
            stderr: output.stderr,
        }
        .into());
    }
    Ok(output.stdout)
}

#[derive(PartialEq)]
enum RefPresence {
    Present,
    Absent,
}

async fn ref_presence(
    git: &GitProbe,
    branch_ref: &str,
    root: &Path,
) -> Result<RefPresence, WorktreeLifecycleError> {
    let output = git.run_command(root, "show-ref", &["--heads"]).await?;
    if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(RefPresence::Absent);
    }
    let output = read_success_bytes(git, root, "show-ref", &["--heads"]).await?;
    for record in output
        .split(|byte| *byte == b'\n')
        .filter(|record| !record.is_empty())
    {
        let Some(separator) = record.iter().position(|byte| *byte == b' ') else {
            return Err(GitError::InvalidMetadata("分支清单缺少 OID").into());
        };
        let (oid_bytes, ref_bytes) = record.split_at(separator);
        let ref_bytes = &ref_bytes[1..];
        let oid = String::from_utf8(oid_bytes.to_ascii_lowercase())
            .map_err(|_| GitError::InvalidMetadata("分支清单包含非法 OID"))?;
        let found_ref = std::str::from_utf8(ref_bytes)
            .map_err(|_| GitError::InvalidMetadata("分支清单包含非法引用"))?;
        if branch_refs_conflict(found_ref, branch_ref) {
            if !is_full_oid(oid.as_bytes()) {
                return Err(GitError::InvalidMetadata("分支清单包含非法 OID").into());
            }
            return Ok(RefPresence::Present);
        }
    }
    Ok(RefPresence::Absent)
}

fn check_config(bytes: &[u8]) -> Result<(), WorktreeLifecycleError> {
    if bytes.is_empty() {
        return Ok(());
    }
    let Some(body) = bytes.strip_suffix(b"\0") else {
        return Err(GitError::InvalidMetadata("配置记录缺少终止符").into());
    };
    for record in body.split(|byte| *byte == 0) {
        let mut fields = record.splitn(2, |byte| *byte == b'\n');
        let Some(key) = fields.next().filter(|key| !key.is_empty()) else {
            return Err(GitError::InvalidMetadata("配置缺少键名").into());
        };
        let value = fields.next().unwrap_or_default();
        if key.starts_with(b"include.") || key.starts_with(b"includeif.") {
            return Err(WorktreeLifecycleError::UnsupportedConfiguration(
                "配置 include",
            ));
        }
        if key.starts_with(b"filter.") && (key.ends_with(b".clean") || key.ends_with(b".process")) {
            return Err(GitError::ExternalFilter.into());
        }
        if key == b"core.worktree" {
            return Err(WorktreeLifecycleError::UnsupportedConfiguration(
                "core.worktree",
            ));
        }
        if key == b"worktree.userelativepaths" {
            return Err(WorktreeLifecycleError::UnsupportedConfiguration(
                "worktree.useRelativePaths",
            ));
        }
        if key == b"core.alternaterefscommand" {
            return Err(WorktreeLifecycleError::UnsupportedObjectSource(
                "alternate refs 命令",
            ));
        }
        if key.starts_with(b"remote.")
            && (key.ends_with(b".promisor") || key.ends_with(b".partialclonefilter"))
        {
            return Err(WorktreeLifecycleError::UnsupportedObjectSource("部分克隆"));
        }
        if key.starts_with(b"extensions.") {
            match key {
                b"extensions.worktreeconfig" => {}
                b"extensions.objectformat" if value.eq_ignore_ascii_case(b"sha1") => {}
                b"extensions.objectformat" if value.eq_ignore_ascii_case(b"sha256") => {}
                _ => {
                    return Err(WorktreeLifecycleError::UnsupportedConfiguration(
                        "Git 仓库扩展",
                    ));
                }
            }
        }
    }
    Ok(())
}

async fn verify_intent(
    git: &GitProbe,
    intent: &PersistentIntent,
    target_workspace: Option<&AuthorizedDirectory>,
) -> Result<RegistrationObservation, WorktreeLifecycleError> {
    let bound = intent
        .authorized_directories
        .ok_or(WorktreeLifecycleError::IdentityChanged)?;
    let source = canonical_source_identity(&intent.source)?;
    if source.identities() != bound.source {
        return Err(WorktreeLifecycleError::IdentityChanged);
    }
    if let Some(workspace) = target_workspace {
        workspace.refresh()?;
        if workspace.identity() != bound.workspace
            || !intent
                .target
                .parent()
                .is_some_and(|parent| path_components_equivalent(parent, workspace.path()))
        {
            return Err(WorktreeLifecycleError::IdentityChanged);
        }
    }
    let reopened = git.open_local_project(source.root.path()).await?;
    let crate::LocalProject::GitWorktree(source_identity) = reopened else {
        return Err(WorktreeLifecycleError::IdentityChanged);
    };
    if source_identity != intent.source {
        return Err(WorktreeLifecycleError::IdentityChanged);
    }
    let inventory = git.list_worktrees(source.root.path()).await?;
    let entry = registration_entry(&inventory, &intent.target)?;
    let ref_oid = exact_ref_oid(git, intent).await?;
    let target_state = match target_workspace {
        Some(workspace) => {
            workspace.refresh()?;
            target_state(&intent.target)?
        }
        None => TargetState::NotExamined,
    };
    let registered = if target_state == TargetState::Directory
        && entry.is_some_and(|entry| matches!(entry.kind, crate::WorktreeKind::Linked))
    {
        open_registered_target(git, intent).await?
    } else {
        None
    };
    let final_inventory = git.list_worktrees(source.root.path()).await?;
    let final_entry = registration_entry(&final_inventory, &intent.target)?;
    if entry != final_entry || ref_oid != exact_ref_oid(git, intent).await? {
        return Err(WorktreeLifecycleError::IdentityChanged);
    }
    source.refresh()?;
    if let Some(workspace) = target_workspace {
        workspace.refresh()?;
        if target_state != self::target_state(&intent.target)? {
            return Err(WorktreeLifecycleError::IdentityChanged);
        }
    }
    let (target_identity, registered_directories) = match registered {
        Some((identity, target, git_dir)) => {
            target.refresh()?;
            git_dir.refresh()?;
            if target_git_directory(intent, &target)?.map(|directory| directory.identity())
                != Some(git_dir.identity())
            {
                return Err(WorktreeLifecycleError::IdentityChanged);
            }
            (
                Some(identity),
                Some([target.identity(), git_dir.identity()]),
            )
        }
        None => (None, None),
    };
    let matched_registered = final_entry.is_some();
    let inventory_entry_count = final_inventory.entries.len();
    let locked = final_entry
        .and_then(|entry| entry.locked.as_ref())
        .map(|annotation| {
            String::from_utf8_lossy(annotation.reason.as_deref().unwrap_or_default()).into_owned()
        });
    let prunable = final_entry
        .and_then(|entry| entry.prunable.as_ref())
        .map(|annotation| {
            String::from_utf8_lossy(annotation.reason.as_deref().unwrap_or_default()).into_owned()
        });
    let target_matches = final_entry.is_some_and(|entry| {
        ref_oid.as_deref() == Some(intent.base_oid.as_str())
            && target_identity.is_some()
            && matches!(
                &entry.head,
                crate::WorktreeHead::Attached { branch, oid }
                    if *branch == intent.branch_ref.as_bytes() && *oid == intent.base_oid
            )
    });
    Ok(RegistrationObservation {
        observed_at: SystemTime::now(),
        ref_present: ref_oid.is_some(),
        ref_oid,
        target_registered: matched_registered,
        target_matches,
        target_identity,
        inventory: final_inventory,
        inventory_entry_count,
        target_state,
        locked,
        prunable,
        registered_directories,
    })
}

fn registration_entry<'a>(
    inventory: &'a crate::WorktreeInventory,
    target: &Path,
) -> Result<Option<&'a crate::WorktreeEntry>, WorktreeLifecycleError> {
    let mut matched = None;
    for entry in &inventory.entries {
        if entry
            .path
            .to_path_buf()
            .is_ok_and(|path| paths_equivalent(&native_git_path(&path), &native_git_path(target)))
            && matched.replace(entry).is_some()
        {
            return Err(WorktreeLifecycleError::AmbiguousRegisteredPath);
        }
    }
    Ok(matched)
}

async fn exact_ref_oid(
    git: &GitProbe,
    intent: &PersistentIntent,
) -> Result<Option<String>, WorktreeLifecycleError> {
    let ref_output = git
        .run_command(
            &intent.source.root,
            "show-ref",
            &["--exists", &intent.branch_ref],
        )
        .await?;
    match ref_output.status.code() {
        Some(2) if ref_output.stdout.is_empty() => Ok(None),
        Some(0) => {
            if !ref_output.stdout.is_empty() || !ref_output.stderr.is_empty() {
                return Err(GitError::InvalidMetadata("分支存在性返回非预期输出").into());
            }
            let oid_output = read_success(
                git,
                &intent.source.root,
                "rev-parse",
                &["--verify", "--end-of-options", &intent.branch_ref],
            )
            .await?;
            let Some(oid_line) = oid_output.strip_suffix(b"\n") else {
                return Err(GitError::InvalidMetadata("分支 OID 缺少终止符").into());
            };
            let oid = String::from_utf8(oid_line.to_ascii_lowercase())
                .map_err(|_| GitError::InvalidMetadata("show-ref 返回非法 OID"))?;
            if !is_full_oid(oid.as_bytes()) {
                return Err(GitError::InvalidMetadata("show-ref 返回非法 OID").into());
            }
            Ok(Some(oid))
        }
        _ => Err(GitError::CommandFailed {
            operation: "show-ref",
            code: ref_output.status.code(),
            stderr: ref_output.stderr,
        }
        .into()),
    }
}

async fn open_registered_target(
    git: &GitProbe,
    intent: &PersistentIntent,
) -> Result<
    Option<(WorktreeIdentity, AuthorizedDirectory, AuthorizedDirectory)>,
    WorktreeLifecycleError,
> {
    let target = checked_directory(&intent.target)?;
    if intent
        .registered_directories
        .is_some_and(|[previous, _]| previous != target.identity())
    {
        return Ok(None);
    }
    let Some(git_dir) = target_git_directory(intent, &target)? else {
        return Ok(None);
    };
    target.refresh()?;
    git_dir.refresh()?;
    match git.open_local_project(target.path()).await? {
        crate::LocalProject::GitWorktree(identity)
            if paths_equivalent(&identity.root, target.path())
                && paths_equivalent(&identity.git_dir, git_dir.path())
                && paths_equivalent(&identity.common_dir, &intent.source.common_dir) =>
        {
            Ok(Some((identity, target, git_dir)))
        }
        _ => Ok(None),
    }
}

fn target_git_directory(
    intent: &PersistentIntent,
    target: &AuthorizedDirectory,
) -> Result<Option<AuthorizedDirectory>, WorktreeLifecycleError> {
    if !target_contains_only_git_marker(target.path())? {
        return Ok(None);
    }
    let marker = target.path().join(".git");
    if check_regular_file(&marker, false).is_err() {
        return Ok(None);
    }
    let bytes = state::read_regular_file(&marker)?;
    let Some(admin) = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| text.strip_prefix("gitdir: "))
        .and_then(|text| text.strip_suffix('\n'))
        .map(Path::new)
        .filter(|path| {
            path.is_absolute()
                && !path
                    .components()
                    .any(|part| matches!(part, Component::ParentDir))
                && metadata_within_admin(path, &intent.source.common_dir)
        })
    else {
        return Ok(None);
    };
    let git_dir = checked_directory(admin)?;
    if intent
        .registered_directories
        .is_some_and(|[_, previous]| previous != git_dir.identity())
        || state::read_regular_file(&git_dir.path().join("commondir"))? != b"../..\n"
    {
        return Ok(None);
    }
    let backlink = state::read_regular_file(&git_dir.path().join("gitdir"))?;
    if std::str::from_utf8(&backlink)
        .ok()
        .and_then(|text| text.strip_suffix('\n'))
        .is_none_or(|path| Path::new(path) != marker.as_path())
    {
        return Ok(None);
    }
    Ok(Some(git_dir))
}

fn target_state(target: &Path) -> Result<TargetState, WorktreeLifecycleError> {
    match target.symlink_metadata() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(TargetState::Absent),
        Err(_) => Ok(TargetState::Inaccessible),
        Ok(metadata) if metadata.is_symlink() || identity::is_reparse(&metadata) => {
            Ok(TargetState::LinkOrReparse)
        }
        Ok(metadata) if metadata.is_dir() => Ok(TargetState::Directory),
        Ok(_) => Ok(TargetState::File),
    }
}

fn classification(
    intent: &PersistentIntent,
    observation: &RegistrationObservation,
    command_succeeded: bool,
) -> OutcomeClass {
    let complete = observation.target_matches
        && observation
            .target_identity
            .as_ref()
            .is_some_and(|identity| {
                paths_equivalent(&identity.common_dir, &intent.source.common_dir)
                    && !paths_equivalent(&identity.git_dir, &intent.source.git_dir)
                    && metadata_within_admin(&identity.git_dir, &intent.source.common_dir)
            })
        && observation.ref_oid.as_deref() == Some(intent.base_oid.as_str());
    let any_footprint = observation.ref_present
        || observation.target_registered
        || matches!(
            observation.target_state,
            TargetState::Directory | TargetState::File | TargetState::LinkOrReparse
        );
    if complete {
        if command_succeeded {
            OutcomeClass::Verified
        } else {
            OutcomeClass::Partial
        }
    } else if any_footprint {
        OutcomeClass::Partial
    } else if matches!(
        observation.target_state,
        TargetState::NotExamined | TargetState::Inaccessible
    ) {
        OutcomeClass::Unknown("目标目录状态未能核实")
    } else if command_succeeded {
        OutcomeClass::Unknown("命令成功但复核证据不完整")
    } else {
        OutcomeClass::Unknown("当前观察未见残留")
    }
}

fn metadata_within_admin(git_dir: &Path, common_dir: &Path) -> bool {
    let git_dir = native_git_path(git_dir);
    let common_dir = native_git_path(common_dir);
    let Some(worktrees_dir) = git_dir.parent() else {
        return false;
    };
    if worktrees_dir.parent().is_none() {
        return false;
    };
    let expected_worktrees = common_dir.join("worktrees");
    paths_equivalent(worktrees_dir, &expected_worktrees)
}

fn target_contains_only_git_marker(target: &Path) -> Result<bool, WorktreeLifecycleError> {
    let entries = match fs::read_dir(target) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let mut has_git_marker = false;
    for entry in entries {
        let entry = entry?;
        if entry.file_name() == std::ffi::OsStr::new(".git") {
            has_git_marker = true;
        } else {
            return Ok(false);
        }
    }
    Ok(has_git_marker)
}

fn execution_error_reason(error: &WorktreeLifecycleError) -> &'static str {
    match error {
        WorktreeLifecycleError::Git(GitError::Timeout) => "复核超时，结果未知",
        WorktreeLifecycleError::Git(GitError::OutputLimit) => "复核输出超限，结果未知",
        _ => "复核失败，结果未知",
    }
}

#[cfg(test)]
mod request_validation_tests {
    use super::{
        WorktreeLifecycleError, validate_branch_ref, validate_request_id, validate_target_name,
    };

    #[test]
    fn target_name_rejects_traversal_without_running_git() {
        for target_name in ["../escaped", "a..b", "CON"] {
            assert!(matches!(
                validate_target_name(target_name),
                Err(WorktreeLifecycleError::InvalidInput(_))
            ));
        }
    }

    #[test]
    fn request_fields_reject_invalid_names_without_running_git() {
        for request_id in [".request", "request id", "request/one"] {
            assert!(matches!(
                validate_request_id(request_id),
                Err(WorktreeLifecycleError::InvalidInput(_))
            ));
        }
        for branch_ref in [
            "refs/heads/other/invalid",
            "refs/heads/soxu/",
            "refs/heads/soxu/-leading",
            "refs/heads/soxu/trailing.",
            "refs/heads/soxu/segment/locked.lock",
            "refs/heads/soxu/大写",
        ] {
            assert!(matches!(
                validate_branch_ref(branch_ref),
                Err(WorktreeLifecycleError::InvalidInput(_))
            ));
        }
    }
}
