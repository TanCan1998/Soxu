//! Worktree 恢复执行的集成验收（M3 #57-3）。
//!
//! 夹具全部位于系统临时目录，只使用真实原生 Git：
//! 单次确认绑定（过期/重放/漂移）、prune 多候选与在线移除的二次确认、
//! 删除后回读、失败足迹与并发预约。所有拒绝路径都对夹具整树做快照断言零写入。
//! 协调域需要平台原生目录身份，Windows 由 fail-closed 用例单独覆盖。

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use soxu_git::GitProbe;
#[cfg(not(windows))]
use std::time::{Duration, SystemTime};

#[cfg(windows)]
use soxu_git::WorktreeRegistrationCoordinator;
#[cfg(not(windows))]
use soxu_git::{
    ExecutionResult, ExecutionStage, LocalProject, PartialCleanupRejection, PreviewOutcome,
    RecoveryCancellation, RecoveryConfirmation, RecoveryExecuteError, RecoveryExecutionOutcome,
    RecoveryNote, RecoveryOperation, RecoveryPreviewRequest, RegistrationOutcome,
    VerifiedRegistration, WorktreeIdentity, WorktreeRegistrationCoordinator,
    WorktreeRegistrationRequest, recovery_preview_digest,
};

struct Fixture {
    root: PathBuf,
    git: GitProbe,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = loop {
            let root = std::env::temp_dir().join(format!(
                "soxu-recovery-execute-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => break root.canonicalize().expect("夹具绝对路径"),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("创建夹具失败：{error}"),
            }
        };
        Self {
            root,
            git: GitProbe::discover().expect("测试需要原生 Git"),
        }
    }

    fn repository(&self) -> PathBuf {
        let path = self.root.join("repo");
        fs::create_dir_all(&path).expect("创建仓库目录");
        path
    }

    /// 夹具内可直接传给 Git 或文件系统的绝对路径（Windows 去除 `\\?\` 前缀）。
    fn native(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        #[cfg(windows)]
        {
            let text = path.to_string_lossy();
            if let Some(stripped) = text.strip_prefix(r"\\?\") {
                return PathBuf::from(stripped);
            }
        }
        path
    }

    /// 夹具 Git 命令的唯一构造点：清除宿主 `GIT_*`、隔离系统与全局配置并禁用后台维护。
    fn command(&self, directory: &Path) -> Command {
        let mut command = Command::new(self.git.program());
        command.current_dir(directory);
        for (name, _) in std::env::vars_os() {
            if name
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("GIT_")
            {
                command.env_remove(name);
            }
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .args([
                "-c",
                "core.hooksPath=",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.autocrlf=false",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ]);
        command
    }

    fn run(&self, directory: &Path, arguments: &[&str]) {
        let output = self
            .command(directory)
            .args(arguments)
            .output()
            .expect("启动夹具 Git");
        assert!(
            output.status.success(),
            "夹具命令失败 {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repository(&self) -> PathBuf {
        let repository = self.repository();
        self.run(&repository, &["init", "-q", "-b", "main"]);
        fs::write(repository.join("README.md"), b"fixture\n").expect("写入夹具文件");
        self.run(&repository, &["add", "."]);
        self.run(&repository, &["commit", "-q", "-m", "init"]);
        repository
    }

    fn add_worktree(&self, repository: &Path, name: &str, branch: &str) -> PathBuf {
        let path = self.native(name);
        self.run(
            repository,
            &[
                "worktree",
                "add",
                "-q",
                &path.to_string_lossy(),
                "-b",
                branch,
            ],
        );
        path
    }

    #[cfg(not(windows))]
    fn head_oid(&self, repository: &Path) -> String {
        let output = self
            .command(repository)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("读取基准 OID");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// 读取原生清单判断指定路径的注册是否仍处于锁定。
    #[cfg(not(windows))]
    fn is_locked(&self, repository: &Path, path: &Path) -> bool {
        let output = self
            .command(repository)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .expect("读取工作树清单");
        let text = String::from_utf8_lossy(&output.stdout);
        let expected = format!("worktree {}", path.to_string_lossy().replace('\\', "/"));
        text.split("\n\n").any(|record| {
            record.lines().any(|line| line == expected)
                && record
                    .lines()
                    .any(|line| line == "locked" || line.starts_with("locked "))
        })
    }

    /// 协调域与工作区根必须是私有目录（Unix 目录身份要求）。
    #[cfg(not(windows))]
    fn private_directory(&self, name: &str) -> PathBuf {
        let path = self.native(name);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .expect("创建私有目录");
        }
        #[cfg(not(unix))]
        fs::create_dir(&path).expect("创建私有目录");
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// 夹具整树快照：相对路径 + 文件字节，用于拒绝路径的零写入证明。
fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut result = Vec::new();
    fn walk(directory: &Path, base: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(directory)
            .expect("读取夹具目录")
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(base)
                .expect("夹具内相对路径")
                .to_path_buf();
            let metadata = entry.metadata().expect("读取夹具条目");
            if metadata.is_dir() {
                walk(&path, base, result);
            } else if path.file_name().is_some_and(|name| {
                name == std::ffi::OsStr::new("index.lock")
                    || name == std::ffi::OsStr::new("config.lock")
            }) {
                continue;
            } else {
                let bytes = fs::read(&path).expect("读取夹具文件");
                result.push((relative, bytes));
            }
        }
    }
    walk(root, root, &mut result);
    result
}

#[cfg(not(windows))]
async fn repository_identity(git: &GitProbe, repository: &Path) -> WorktreeIdentity {
    match git
        .open_local_project(repository)
        .await
        .expect("打开源仓库身份")
    {
        LocalProject::GitWorktree(identity) => identity,
        LocalProject::Directory { .. } => panic!("夹具源根必须是 Git 工作树"),
    }
}

#[cfg(not(windows))]
async fn preview(
    git: &GitProbe,
    repository: &Path,
    state: Option<&Path>,
    operation: RecoveryOperation,
    target: Option<&Path>,
    request_id: Option<&str>,
) -> PreviewOutcome {
    git.recovery_preview(RecoveryPreviewRequest {
        source_root: repository,
        coordinator_state_root: state,
        target_authorization: target,
        operation,
        partial_cleanup_request_id: request_id,
    })
    .await
    .expect("预览必须能完成只读观察")
}

/// 由预览生成单次确认：摘要、有效期与仓库身份都来自同一次观察。
#[cfg(not(windows))]
fn confirmation(
    execution_id: &str,
    operation: RecoveryOperation,
    preview: &PreviewOutcome,
    target: Option<&Path>,
    repository: &WorktreeIdentity,
    risk_acknowledged: bool,
    request_id: Option<&str>,
) -> RecoveryConfirmation {
    let issued_at = SystemTime::now();
    RecoveryConfirmation {
        execution_id: execution_id.to_owned(),
        operation,
        preview_digest: recovery_preview_digest(preview, target),
        source_root: repository.root.clone(),
        repository: repository.clone(),
        target: target.map(Path::to_path_buf),
        issued_at,
        expires_at: issued_at + Duration::from_secs(300),
        risk_acknowledged,
        partial_cleanup_request_id: request_id.map(str::to_owned),
    }
}

#[cfg(not(windows))]
#[allow(clippy::too_many_arguments)]
async fn register_worktree(
    git: &GitProbe,
    state: &Path,
    workspace: &Path,
    identity: &WorktreeIdentity,
    base_oid: &str,
    request_id: &'static str,
    target_name: &'static str,
    branch_ref: &'static str,
) -> VerifiedRegistration {
    let coordinator = WorktreeRegistrationCoordinator::open(state, &[]).expect("打开协调域");
    let request = WorktreeRegistrationRequest {
        request_id,
        source: identity,
        workspace_root: workspace,
        target_name,
        branch_ref,
        base_oid,
    };
    match git
        .register_new_worktree(&coordinator, request)
        .await
        .expect("注册必须成功")
    {
        RegistrationOutcome::Verified(verified) => verified,
        other => panic!("注册必须验证通过：{other:?}"),
    }
}

#[tokio::test]
#[cfg(not(windows))]
async fn unlock_execution_binds_confirmation_and_rejects_replay_expiry_and_drift() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let worktree = fixture.add_worktree(&repository, "wt", "b1");
    fixture.run(
        &repository,
        &["worktree", "lock", "wt", "--reason", "待复核"],
    );
    let state = fixture.private_directory("state");
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let identity = repository_identity(&fixture.git, &repository).await;

    let preview = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::Unlock,
        Some(&worktree),
        None,
    )
    .await;
    let PreviewOutcome::Unlock(ref unlock) = preview else {
        panic!("必须返回 unlock 预览");
    };
    assert!(unlock.rejection.is_none(), "有效保护锁应可授权");

    let confirmation = confirmation(
        "exec-unlock",
        RecoveryOperation::Unlock,
        &preview,
        Some(&worktree),
        &identity,
        false,
        None,
    );
    let outcome = fixture
        .git
        .execute_recovery(&coordinator, &confirmation, None)
        .await
        .expect("单次确认执行必须成功");
    let RecoveryExecutionOutcome::Completed {
        record,
        observation,
    } = &outcome
    else {
        panic!("解锁应完成：{outcome:?}");
    };
    assert_eq!(record.stage, ExecutionStage::Observed);
    assert_eq!(record.note, RecoveryNote::Verified);
    assert!(!record.blocked);
    assert_eq!(record.result, ExecutionResult::Succeeded);
    assert!(record.observed_at.is_some());
    assert!(observation.inventory.entries.iter().any(|entry| {
        entry.path.to_path_buf().is_ok_and(|path| path == worktree) && entry.locked.is_none()
    }));
    let stored = fixture
        .git
        .diagnose_recovery(&coordinator, "exec-unlock")
        .expect("只读诊断")
        .expect("记录必须存在");
    assert_eq!(&stored, record);

    let after_unlock = tree_snapshot(&repository);
    // 重放同一确认：直接拒绝，且仓库零写入。
    let replay = fixture
        .git
        .execute_recovery(&coordinator, &confirmation, None)
        .await;
    assert!(matches!(
        replay,
        Err(RecoveryExecuteError::AlreadyAttempted(record)) if record.execution_id == "exec-unlock"
    ));
    assert_eq!(after_unlock, tree_snapshot(&repository));

    // 过期确认：零写入，也不产生新记录。
    let mut expired = confirmation.clone();
    expired.execution_id = "exec-expired".to_owned();
    expired.issued_at = SystemTime::now() - Duration::from_secs(3600);
    expired.expires_at = SystemTime::now() - Duration::from_secs(1);
    assert!(matches!(
        fixture
            .git
            .execute_recovery(&coordinator, &expired, None)
            .await,
        Err(RecoveryExecuteError::ConfirmationExpired)
    ));
    assert_eq!(after_unlock, tree_snapshot(&repository));
    assert_eq!(
        fixture
            .git
            .recovery_execution_records(&coordinator)
            .expect("列出记录")
            .len(),
        1
    );

    // 漂移：锁定状态被外部修改后，旧摘要作废并零写入。
    fixture.run(
        &repository,
        &["worktree", "lock", "wt", "--reason", "再次锁定"],
    );
    let before_drift = tree_snapshot(&repository);
    let mut drifted = confirmation.clone();
    drifted.execution_id = "exec-drift".to_owned();
    assert!(matches!(
        fixture
            .git
            .execute_recovery(&coordinator, &drifted, None)
            .await,
        Err(RecoveryExecuteError::PreviewDrift)
    ));
    assert_eq!(before_drift, tree_snapshot(&repository));
    assert!(fixture.is_locked(&repository, &worktree));
}

#[tokio::test]
#[cfg(not(windows))]
async fn prune_execution_requires_second_confirmation_and_detects_candidate_growth() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let first = fixture.add_worktree(&repository, "wt-a", "b1");
    let second = fixture.add_worktree(&repository, "wt-b", "b2");
    fs::rename(&first, fixture.native("wt-a-moved")).expect("移动第一个工作树");
    fs::rename(&second, fixture.native("wt-b-moved")).expect("移动第二个工作树");
    let state = fixture.private_directory("state");
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let identity = repository_identity(&fixture.git, &repository).await;

    let initial = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::Prune,
        None,
        None,
    )
    .await;
    let PreviewOutcome::Prune(ref prune) = initial else {
        panic!("必须返回 prune 预览");
    };
    assert_eq!(prune.candidates.len(), 2);
    assert!(prune.rejection.is_none());

    // 多候选没有独立二次确认：拒绝且零写入。
    let unconfirmed = confirmation(
        "exec-prune-unconfirmed",
        RecoveryOperation::Prune,
        &initial,
        None,
        &identity,
        false,
        None,
    );
    let before = tree_snapshot(&repository);
    assert!(matches!(
        fixture
            .git
            .execute_recovery(&coordinator, &unconfirmed, None)
            .await,
        Err(RecoveryExecuteError::ConfirmationRequired)
    ));
    assert_eq!(before, tree_snapshot(&repository));
    assert!(
        fixture
            .git
            .recovery_execution_records(&coordinator)
            .expect("列出记录")
            .is_empty(),
        "未确认的请求不得留下 journal"
    );

    // 候补集合扩张：同一确认在外部新增失联注册后作废。
    let third = fixture.add_worktree(&repository, "wt-c", "b3");
    fs::rename(&third, fixture.native("wt-c-moved")).expect("移动第三个工作树");
    let confirmed = confirmation(
        "exec-prune-drift",
        RecoveryOperation::Prune,
        &initial,
        None,
        &identity,
        true,
        None,
    );
    let before_drift = tree_snapshot(&repository);
    assert!(matches!(
        fixture
            .git
            .execute_recovery(&coordinator, &confirmed, None)
            .await,
        Err(RecoveryExecuteError::PreviewDrift)
    ));
    assert_eq!(before_drift, tree_snapshot(&repository));

    // 重新预览后带二次确认执行：全部候选被原生 prune 清理。
    let regrouped = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::Prune,
        None,
        None,
    )
    .await;
    let PreviewOutcome::Prune(ref at_three) = regrouped else {
        panic!("必须返回 prune 预览");
    };
    assert_eq!(at_three.candidates.len(), 3);
    let confirmed = confirmation(
        "exec-prune",
        RecoveryOperation::Prune,
        &regrouped,
        None,
        &identity,
        true,
        None,
    );
    let outcome = fixture
        .git
        .execute_recovery(&coordinator, &confirmed, None)
        .await
        .expect("二次确认后必须执行");
    let RecoveryExecutionOutcome::Completed {
        record,
        observation,
    } = outcome
    else {
        panic!("prune 应完成：{outcome:?}");
    };
    assert_eq!(record.note, RecoveryNote::Verified);
    assert!(observation.remaining_candidates.is_empty());
    assert_eq!(observation.inventory.entries.len(), 1, "只应保留主工作树");
    let admin = repository.join(".git/worktrees");
    // 全部候选被清理后 Git 可能连空的管理根目录一并移除，两种情形都算清理完成。
    let remaining = match fs::read_dir(&admin) {
        Ok(entries) => entries.filter_map(Result::ok).count(),
        Err(error) if error.kind() == ErrorKind::NotFound => 0,
        Err(error) => panic!("读取管理目录：{error}"),
    };
    assert_eq!(remaining, 0, "失联注册的管理目录必须被原生 prune 移除");
}

#[tokio::test]
#[cfg(not(windows))]
async fn online_remove_requires_second_confirmation_and_reports_deleted_state() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let worktree = fixture.add_worktree(&repository, "wt", "b1");
    let state = fixture.private_directory("state");
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let identity = repository_identity(&fixture.git, &repository).await;

    let preview = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::RemoveRegistration,
        Some(&worktree),
        None,
    )
    .await;
    let PreviewOutcome::RemoveRegistration(ref remove) = preview else {
        panic!("必须返回移除注册预览");
    };
    assert!(remove.rejection.is_none());

    // 在线移除会删除工作目录与管理数据：没有二次确认时零写入。
    let unconfirmed = confirmation(
        "exec-remove-unconfirmed",
        RecoveryOperation::RemoveRegistration,
        &preview,
        Some(&worktree),
        &identity,
        false,
        None,
    );
    let before = tree_snapshot(&repository);
    assert!(matches!(
        fixture
            .git
            .execute_recovery(&coordinator, &unconfirmed, None)
            .await,
        Err(RecoveryExecuteError::ConfirmationRequired)
    ));
    assert_eq!(before, tree_snapshot(&repository));
    assert!(worktree.exists());

    let confirmed = confirmation(
        "exec-remove",
        RecoveryOperation::RemoveRegistration,
        &preview,
        Some(&worktree),
        &identity,
        true,
        None,
    );
    let outcome = fixture
        .git
        .execute_recovery(&coordinator, &confirmed, None)
        .await
        .expect("二次确认后必须执行");
    let RecoveryExecutionOutcome::Completed {
        record,
        observation,
    } = outcome
    else {
        panic!("在线移除应完成：{outcome:?}");
    };
    assert_eq!(record.note, RecoveryNote::Verified);
    assert!(!record.blocked);
    assert!(
        observation
            .inventory
            .entries
            .iter()
            .all(|entry| !entry.path.to_path_buf().is_ok_and(|path| path == worktree))
    );
    assert!(!worktree.exists(), "工作目录必须被原生 remove 删除");
    // 删除后结果：回读确认目标已不存在，而不是把删除当成漂移。
    assert_eq!(observation.target_state, soxu_git::TargetState::Absent);
}

#[tokio::test]
#[cfg(not(windows))]
async fn remove_failure_keeps_footprint_and_blocks_access() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let worktree = fixture.add_worktree(&repository, "wt", "b1");
    // 目标工作树内的未跟踪文件不在源仓库 status 中，预览据此仍可授权；
    // 原生 remove 会拒绝并要求 --force，本实现绝不传 --force。
    fs::write(worktree.join("junk.txt"), b"untracked\n").expect("写入未跟踪文件");
    let state = fixture.private_directory("state");
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let identity = repository_identity(&fixture.git, &repository).await;

    let preview = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::RemoveRegistration,
        Some(&worktree),
        None,
    )
    .await;
    let PreviewOutcome::RemoveRegistration(ref remove) = preview else {
        panic!("必须返回移除注册预览");
    };
    assert!(remove.rejection.is_none());

    let confirmed = confirmation(
        "exec-remove-fail",
        RecoveryOperation::RemoveRegistration,
        &preview,
        Some(&worktree),
        &identity,
        true,
        None,
    );
    let outcome = fixture
        .git
        .execute_recovery(&coordinator, &confirmed, None)
        .await
        .expect("命令失败也必须返回记录而不是错误");
    let RecoveryExecutionOutcome::Failed {
        record,
        observation,
        command,
    } = outcome
    else {
        panic!("非零退出必须归为失败：{outcome:?}");
    };
    assert!(matches!(command, ExecutionResult::Failed { .. }));
    assert_eq!(record.note, RecoveryNote::CommandFailed);
    assert!(record.blocked, "失败必须禁用后续接入");
    assert_eq!(record.stage, ExecutionStage::Observed);
    assert!(observation.is_some(), "失败也要保留回读足迹");
    assert!(worktree.exists(), "失败时不得删除目标目录");
    assert!(
        fixture
            .git
            .diagnose_recovery(&coordinator, "exec-remove-fail")
            .expect("只读诊断")
            .expect("记录必须存在")
            .blocked,
        "重启诊断必须暴露阻断标记"
    );
    // 同一确认不得重放。
    assert!(matches!(
        fixture
            .git
            .execute_recovery(&coordinator, &confirmed, None)
            .await,
        Err(RecoveryExecuteError::AlreadyAttempted(_))
    ));
}

#[tokio::test]
#[cfg(not(windows))]
async fn repair_execution_updates_single_proven_mapping() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let worktree = fixture.add_worktree(&repository, "wt", "b1");
    let moved = fixture.native("wt-moved");
    fs::rename(&worktree, &moved).expect("移动工作树");
    let state = fixture.private_directory("state");
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let identity = repository_identity(&fixture.git, &repository).await;

    let preview = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::Repair,
        Some(&moved),
        None,
    )
    .await;
    let PreviewOutcome::Repair(ref repair) = preview else {
        panic!("必须返回 repair 预览");
    };
    assert!(repair.rejection.is_none(), "单候选范围可证明");

    let confirmed = confirmation(
        "exec-repair",
        RecoveryOperation::Repair,
        &preview,
        Some(&moved),
        &identity,
        false,
        None,
    );
    let outcome = fixture
        .git
        .execute_recovery(&coordinator, &confirmed, None)
        .await
        .expect("repair 必须执行");
    let RecoveryExecutionOutcome::Completed {
        record,
        observation,
    } = outcome
    else {
        panic!("repair 应完成：{outcome:?}");
    };
    assert_eq!(record.note, RecoveryNote::Verified);
    assert!(
        observation
            .inventory
            .entries
            .iter()
            .any(|entry| entry.path.to_path_buf().is_ok_and(|path| path == moved))
    );
    assert!(
        observation
            .inventory
            .entries
            .iter()
            .all(|entry| entry.prunable.is_none())
    );
}

#[tokio::test]
#[cfg(not(windows))]
async fn partial_cleanup_reports_nothing_to_clean_or_refuses_unprovable_scope() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let workspace = fixture.private_directory("workspace");
    let state = fixture.private_directory("state");
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let identity = repository_identity(&fixture.git, &repository).await;
    let base_oid = fixture.head_oid(&repository);
    let cleanup = register_worktree(
        &fixture.git,
        &state,
        &workspace,
        &identity,
        &base_oid,
        "cleanup-ok",
        "cleanup-ok",
        "refs/heads/soxu/cleanup-ok",
    )
    .await;
    let live = register_worktree(
        &fixture.git,
        &state,
        &workspace,
        &identity,
        &base_oid,
        "cleanup-live",
        "cleanup-live",
        "refs/heads/soxu/cleanup-live",
    )
    .await;

    // 已清理：目标目录被删除后，预览只报告「无宿主资产需要处理」。
    fs::remove_dir_all(&cleanup.intent.target).expect("删除目标目录形成清理候选");
    let absent_preview = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::PartialCleanup,
        Some(&cleanup.intent.target),
        Some("cleanup-ok"),
    )
    .await;
    let PreviewOutcome::PartialCleanup(ref p) = absent_preview else {
        panic!("必须返回清理预览");
    };
    assert!(p.rejection.is_none());
    assert!(matches!(
        p.scope,
        soxu_git::PartialCleanupScope::TargetAbsent
    ));
    let confirmed = confirmation(
        "exec-cleanup",
        RecoveryOperation::PartialCleanup,
        &absent_preview,
        Some(&cleanup.intent.target),
        &identity,
        false,
        Some("cleanup-ok"),
    );
    let before = tree_snapshot(&repository);
    let outcome = fixture
        .git
        .execute_recovery(&coordinator, &confirmed, None)
        .await
        .expect("可证明无资产时必须给出结论");
    let RecoveryExecutionOutcome::NothingToClean { record } = outcome else {
        panic!("应报告无需清理：{outcome:?}");
    };
    assert_eq!(record.note, RecoveryNote::NothingToClean);
    assert!(!record.blocked);
    assert_eq!(before, tree_snapshot(&repository), "清理预览结论必须零写入");

    // 在线目标仍存在：归属无法证明，拒绝执行且零写入。
    let live_preview = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::PartialCleanup,
        Some(&live.intent.target),
        Some("cleanup-live"),
    )
    .await;
    let PreviewOutcome::PartialCleanup(ref p) = live_preview else {
        panic!("必须返回清理预览");
    };
    assert_eq!(p.rejection, Some(PartialCleanupRejection::TargetUnprovable));
    let refused = confirmation(
        "exec-cleanup-refused",
        RecoveryOperation::PartialCleanup,
        &live_preview,
        Some(&live.intent.target),
        &identity,
        false,
        Some("cleanup-live"),
    );
    let before = tree_snapshot(&repository);
    assert!(matches!(
        fixture
            .git
            .execute_recovery(&coordinator, &refused, None)
            .await,
        Err(RecoveryExecuteError::PreviewRejected(
            "cleanup_target_unprovable"
        ))
    ));
    assert_eq!(before, tree_snapshot(&repository));
    assert!(live.intent.target.exists());
}

#[tokio::test]
#[cfg(not(windows))]
async fn cancellation_before_start_leaves_no_footprint_and_keeps_confirmation() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let worktree = fixture.add_worktree(&repository, "wt", "b1");
    fixture.run(
        &repository,
        &["worktree", "lock", "wt", "--reason", "待复核"],
    );
    let state = fixture.private_directory("state");
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let identity = repository_identity(&fixture.git, &repository).await;

    let preview = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::Unlock,
        Some(&worktree),
        None,
    )
    .await;
    let confirmed = confirmation(
        "exec-cancel",
        RecoveryOperation::Unlock,
        &preview,
        Some(&worktree),
        &identity,
        false,
        None,
    );
    let cancellation = RecoveryCancellation::new();
    cancellation.cancel();
    let before = tree_snapshot(&repository);
    assert!(matches!(
        fixture
            .git
            .execute_recovery(&coordinator, &confirmed, Some(&cancellation))
            .await,
        Err(RecoveryExecuteError::Cancelled)
    ));
    assert_eq!(before, tree_snapshot(&repository));
    assert!(
        fixture
            .git
            .recovery_execution_records(&coordinator)
            .expect("列出记录")
            .is_empty(),
        "取消不得消费确认"
    );
    assert!(fixture.is_locked(&repository, &worktree));

    // 未被消费的确认可以原样复用。
    let outcome = fixture
        .git
        .execute_recovery(&coordinator, &confirmed, None)
        .await
        .expect("取消后同一确认仍可执行");
    assert!(matches!(
        outcome,
        RecoveryExecutionOutcome::Completed { .. }
    ));
}

#[tokio::test]
#[cfg(not(windows))]
async fn concurrent_confirmations_serialize_to_a_single_write() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let worktree = fixture.add_worktree(&repository, "wt", "b1");
    fixture.run(
        &repository,
        &["worktree", "lock", "wt", "--reason", "待复核"],
    );
    let state = fixture.private_directory("state");
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let identity = repository_identity(&fixture.git, &repository).await;

    let preview = preview(
        &fixture.git,
        &repository,
        Some(&state),
        RecoveryOperation::Unlock,
        Some(&worktree),
        None,
    )
    .await;
    let first = confirmation(
        "exec-concurrent-a",
        RecoveryOperation::Unlock,
        &preview,
        Some(&worktree),
        &identity,
        false,
        None,
    );
    let second = confirmation(
        "exec-concurrent-b",
        RecoveryOperation::Unlock,
        &preview,
        Some(&worktree),
        &identity,
        false,
        None,
    );
    let (left, right) = tokio::join!(
        fixture.git.execute_recovery(&coordinator, &first, None),
        fixture.git.execute_recovery(&coordinator, &second, None)
    );
    let results = [left, right];
    let completed = results
        .iter()
        .filter(|result| matches!(result, Ok(RecoveryExecutionOutcome::Completed { .. })))
        .count();
    assert_eq!(completed, 1, "协调域必须串行化同仓写入");
    let rejected = results
        .iter()
        .filter(|result| {
            matches!(
                result,
                Err(RecoveryExecuteError::PreviewDrift)
                    | Err(RecoveryExecuteError::AlreadyAttempted(_))
                    | Err(RecoveryExecuteError::ExecutionStillUnknown(_))
            )
        })
        .count();
    assert_eq!(rejected, 1, "后到者必须因漂移或重放被拒绝");
    assert_eq!(
        fixture
            .git
            .recovery_execution_records(&coordinator)
            .expect("列出记录")
            .len(),
        1,
        "被拒绝的一侧不得留下 journal"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_coordinator_fails_closed_without_writes() {
    // Windows 缺少稳定目录身份，协调域在打开阶段就失败闭合：不建锁、不建 journal、不写仓库。
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let worktree = fixture.add_worktree(&repository, "wt", "b1");
    let state = fixture.native("state");
    fs::create_dir(&state).expect("创建状态根");
    let before = tree_snapshot(&repository);
    assert!(matches!(
        WorktreeRegistrationCoordinator::open(&state, &[]),
        Err(soxu_git::WorktreeLifecycleError::UnsupportedFilesystem(_))
    ));
    assert_eq!(before, tree_snapshot(&repository));
    assert!(worktree.exists());
    assert!(fs::read_dir(&state).expect("读取状态根").next().is_none());
}
