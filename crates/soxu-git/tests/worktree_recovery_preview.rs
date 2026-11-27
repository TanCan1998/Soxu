//! Worktree 只读恢复预览契约的集成验收（M3 #57-2）。
//!
//! 夹具全部位于系统临时目录，只使用真实 Git 建立观察状态：
//! 有效保护锁、prunable（目录移动）、锁定 + prunable、
//! 检出留痕（Pending / Disabled）、Unknown 执行结果与拒绝路径。
//! 预览前后对整个夹具做目录树快照断言，证明零写入。

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use soxu_git::{
    EvidenceGap, GitProbe, PreviewOutcome, PruneRejection, RecoveryOperation,
    RecoveryPreviewRequest, RemoveRegistrationRejection, RemoveRegistrationScope, RepairRejection,
    UnlockRejection, WorktreeObservationKind,
};
// Windows 上协调域 fail-closed 与 Linux 专属用例不编译，相关导入按 cfg 收窄。
#[cfg(not(windows))]
use soxu_git::{TargetState, WorktreeRegistrationCoordinator};

struct Fixture {
    root: PathBuf,
    git: GitProbe,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = loop {
            let root = std::env::temp_dir().join(format!(
                "soxu-recovery-preview-{}-{}",
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

    /// 夹具 Git 命令的唯一构造点：清除宿主 `GIT_*`、隔离系统与全局配置。
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
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// 夹具整树快照：相对路径 + 文件字节，用于零写入证明。
/// 排除 Git 在 list/prune dry-run 期间可能触碰的锁文件残留。
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

/// Unix 协调域要求仅所有者可访问（与 lifecycle 集测同款）。
#[cfg(all(unix, not(windows)))]
fn create_private_directory(path: &Path) {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .expect("创建私有持久状态目录");
}

#[cfg(all(not(unix), not(windows)))]
fn create_private_directory(path: &Path) {
    fs::create_dir_all(path).expect("创建状态根");
}

fn request<'a>(
    repository: &'a Path,
    operation: RecoveryOperation,
    coordinator: Option<&'a Path>,
    target: Option<&'a Path>,
) -> RecoveryPreviewRequest<'a> {
    RecoveryPreviewRequest {
        source_root: repository,
        coordinator_state_root: coordinator,
        target_authorization: target,
        operation,
        partial_cleanup_request_id: None,
    }
}

async fn preview(
    git: &GitProbe,
    repository: &Path,
    req: RecoveryPreviewRequest<'_>,
) -> PreviewOutcome {
    let before = tree_snapshot(repository);
    let outcome = git
        .recovery_preview(req)
        .await
        .expect("预览必须能完成只读观察");
    let after = tree_snapshot(repository);
    assert_eq!(before, after, "预览必须零写入");
    outcome
}

#[tokio::test]
async fn unlock_preview_rejects_unlocked_registration_and_accepts_protected_lock() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let wt = fixture.native("wt");
    fixture.run(
        &repository,
        &["worktree", "add", "-q", &wt.to_string_lossy(), "-b", "b1"],
    );
    fixture.run(
        &repository,
        &["worktree", "lock", "wt", "--reason", "保留待复核"],
    );

    let outcome = preview(
        &fixture.git,
        &repository,
        request(
            &repository,
            RecoveryOperation::Unlock,
            None,
            Some(&fixture.native("wt")),
        ),
    )
    .await;
    let PreviewOutcome::Unlock(unlock) = outcome else {
        panic!("必须返回 unlock 预览");
    };
    assert!(unlock.rejection.is_none(), "有效保护锁 + 授权目标应可授权");
    assert_eq!(
        unlock.candidate.lock_reason.as_deref(),
        Some("保留待复核".as_bytes())
    );
    assert!(
        unlock
            .observation
            .states
            .iter()
            .any(|state| state.state == WorktreeObservationKind::ValidProtectedLock)
    );

    // 未提供授权目标时，锁租约证据不足，保留锁。
    let outcome = preview(
        &fixture.git,
        &repository,
        request(&repository, RecoveryOperation::Unlock, None, None),
    )
    .await;
    let PreviewOutcome::Unlock(unlock) = outcome else {
        panic!("必须返回 unlock 预览");
    };
    assert_eq!(
        unlock.rejection,
        Some(UnlockRejection::LockEvidenceInsufficient)
    );
    assert!(
        unlock
            .observation
            .evidence_gaps
            .iter()
            .any(|gap| matches!(gap, EvidenceGap::LockLeaseUnconfirmed { .. }))
    );

    // 解锁后（用原生命令模拟既有注册无锁状态），预览拒绝 NotLocked。
    fixture.run(&repository, &["worktree", "unlock", "wt"]);
    let outcome = preview(
        &fixture.git,
        &repository,
        request(
            &repository,
            RecoveryOperation::Unlock,
            None,
            Some(&fixture.native("wt")),
        ),
    )
    .await;
    let PreviewOutcome::Unlock(unlock) = outcome else {
        panic!("必须返回 unlock 预览");
    };
    assert_eq!(unlock.rejection, Some(UnlockRejection::NotLocked));
}

#[tokio::test]
async fn prune_preview_lists_moved_registration_and_keeps_locked_candidates() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let wt_free = fixture.native("wt-free");
    let wt_locked = fixture.native("wt-locked");
    fixture.run(
        &repository,
        &[
            "worktree",
            "add",
            "-q",
            &wt_free.to_string_lossy(),
            "-b",
            "b1",
        ],
    );
    fixture.run(
        &repository,
        &[
            "worktree",
            "add",
            "-q",
            &wt_locked.to_string_lossy(),
            "-b",
            "b2",
        ],
    );
    fixture.run(
        &repository,
        &["worktree", "lock", "wt-locked", "--reason", "保护中"],
    );
    // 两个注册的目录都移走：一个有锁，一个无锁。
    fs::rename(fixture.native("wt-free"), fixture.native("wt-free-moved")).expect("移动无锁工作树");
    fs::rename(
        fixture.native("wt-locked"),
        fixture.native("wt-locked-moved"),
    )
    .expect("移动锁定工作树");

    let outcome = preview(
        &fixture.git,
        &repository,
        request(&repository, RecoveryOperation::Prune, None, None),
    )
    .await;
    let PreviewOutcome::Prune(prune) = outcome else {
        panic!("必须返回 prune 预览");
    };
    println!("candidates: {:?}", prune.candidates);
    assert_eq!(prune.candidates.len(), 1, "默认策略只应列出无锁失联候选");
    assert_eq!(prune.candidates[0].admin_dir, b"worktrees/wt-free".to_vec());
    assert!(
        prune.candidates[0]
            .reason
            .windows(b"non-existent location".len())
            .any(|window| window == b"non-existent location"),
        "理由必须保留原生字节"
    );
    assert!(
        prune
            .observation
            .states
            .iter()
            .any(|state| state.state == WorktreeObservationKind::PrunableOrUnreachable)
    );
    // 预览后清单必须仍包含锁定注册（prune dry-run 不改变注册）。
    assert_eq!(prune.inventory_after.entries.len(), 3);

    // 锁定候选存在时拒绝授权，等待锁归属裁决。
    // 注意：锁定候选不在 dry-run 输出中，拒绝由清单 locked+prunable 组合触发。
    let PreviewOutcome::Prune(prune) = preview(
        &fixture.git,
        &repository,
        request(&repository, RecoveryOperation::Prune, None, None),
    )
    .await
    else {
        panic!("必须返回 prune 预览");
    };
    assert_eq!(
        prune.rejection,
        Some(PruneRejection::LockedRegistrationUnadjudicated),
        "锁定失联候选必须阻断整仓 prune 授权"
    );
}

#[tokio::test]
async fn prune_preview_is_empty_for_healthy_repository() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let outcome = preview(
        &fixture.git,
        &repository,
        request(&repository, RecoveryOperation::Prune, None, None),
    )
    .await;
    let PreviewOutcome::Prune(prune) = outcome else {
        panic!("必须返回 prune 预览");
    };
    assert!(prune.candidates.is_empty());
    assert!(prune.rejection.is_none());
}

#[tokio::test]
async fn repair_preview_requires_target_authorization_and_rejects_unbounded_scope() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let wt = fixture.native("wt");
    fixture.run(
        &repository,
        &["worktree", "add", "-q", &wt.to_string_lossy(), "-b", "b1"],
    );
    fs::rename(fixture.native("wt"), fixture.native("wt-moved")).expect("移动工作树");

    // 未授权目标：无法证明移动前后身份，拒绝。
    let outcome = preview(
        &fixture.git,
        &repository,
        request(&repository, RecoveryOperation::Repair, None, None),
    )
    .await;
    let PreviewOutcome::Repair(repair) = outcome else {
        panic!("必须返回 repair 预览");
    };
    assert_eq!(repair.rejection, Some(RepairRejection::NoProvenCandidate));

    // 授权目标可核查为目录：单候选范围可证明。
    let outcome = preview(
        &fixture.git,
        &repository,
        request(
            &repository,
            RecoveryOperation::Repair,
            None,
            Some(&fixture.native("wt-moved")),
        ),
    )
    .await;
    let PreviewOutcome::Repair(repair) = outcome else {
        panic!("必须返回 repair 预览");
    };
    assert!(repair.rejection.is_none(), "单候选且目标可核查应可授权");
    assert_eq!(repair.candidates.len(), 1);

    // 两个失联候选：原生命令按仓库执行，范围无法界定到单个工作树。
    let wt2 = fixture.native("wt2");
    fixture.run(
        &repository,
        &["worktree", "add", "-q", &wt2.to_string_lossy(), "-b", "b2"],
    );
    fs::rename(fixture.native("wt2"), fixture.native("wt2-moved")).expect("移动第二个工作树");
    let outcome = preview(
        &fixture.git,
        &repository,
        request(
            &repository,
            RecoveryOperation::Repair,
            None,
            Some(&fixture.native("wt-moved")),
        ),
    )
    .await;
    let PreviewOutcome::Repair(repair) = outcome else {
        panic!("必须返回 repair 预览");
    };
    assert_eq!(repair.rejection, Some(RepairRejection::ScopeUnbounded));
}

#[tokio::test]
async fn remove_registration_preview_separates_online_clean_and_disconnected() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let wt = fixture.native("wt");
    fixture.run(
        &repository,
        &["worktree", "add", "-q", &wt.to_string_lossy(), "-b", "b1"],
    );

    // 在线干净：授权目标 + 干净源仓库 → 可授权普通 remove。
    let outcome = preview(
        &fixture.git,
        &repository,
        request(
            &repository,
            RecoveryOperation::RemoveRegistration,
            None,
            Some(&fixture.native("wt")),
        ),
    )
    .await;
    let PreviewOutcome::RemoveRegistration(remove) = outcome else {
        panic!("必须返回移除注册预览");
    };
    assert!(remove.rejection.is_none());
    let RemoveRegistrationScope::OnlineClean { clean, locked, .. } = remove.scope else {
        panic!("在线注册必须归类 OnlineClean");
    };
    assert!(clean);
    assert!(!locked);

    // 未授权目标：清洁度无法证明，拒绝。
    let outcome = preview(
        &fixture.git,
        &repository,
        request(
            &repository,
            RecoveryOperation::RemoveRegistration,
            None,
            None,
        ),
    )
    .await;
    let PreviewOutcome::RemoveRegistration(remove) = outcome else {
        panic!("必须返回移除注册预览");
    };
    assert_eq!(
        remove.rejection,
        Some(RemoveRegistrationRejection::CleanlinessUnproven)
    );

    // 锁定的在线注册：先走 unlock 审定。
    fixture.run(
        &repository,
        &["worktree", "lock", "wt", "--reason", "保护中"],
    );
    let outcome = preview(
        &fixture.git,
        &repository,
        request(
            &repository,
            RecoveryOperation::RemoveRegistration,
            None,
            Some(&fixture.native("wt")),
        ),
    )
    .await;
    let PreviewOutcome::RemoveRegistration(remove) = outcome else {
        panic!("必须返回移除注册预览");
    };
    assert_eq!(remove.rejection, Some(RemoveRegistrationRejection::Locked));

    // 失联注册：走受控 prune，预览不拒绝但归为 Disconnected。
    fixture.run(&repository, &["worktree", "unlock", "wt"]);
    let wt_gone = fixture.native("wt-gone");
    fixture.run(
        &repository,
        &[
            "worktree",
            "add",
            "-q",
            &wt_gone.to_string_lossy(),
            "-b",
            "b3",
        ],
    );
    fs::remove_dir_all(fixture.native("wt-gone")).expect("删除失联工作树目录");
    let outcome = preview(
        &fixture.git,
        &repository,
        request(
            &repository,
            RecoveryOperation::RemoveRegistration,
            None,
            Some(&fixture.native("wt-gone")),
        ),
    )
    .await;
    let PreviewOutcome::RemoveRegistration(remove) = outcome else {
        panic!("必须返回移除注册预览");
    };
    assert!(
        remove.rejection.is_none(),
        "失联注册走受控 prune，不属于拒绝"
    );
    let RemoveRegistrationScope::Disconnected {
        prunable_reason, ..
    } = &remove.scope
    else {
        panic!("失联注册必须归类 Disconnected");
    };
    assert!(prunable_reason.is_some());
}

#[tokio::test]
#[cfg(not(windows))] // Windows 协调域 fail-closed（与 lifecycle 集测同款门控）
async fn partial_cleanup_preview_binds_request_id_and_rejects_unknown_execution() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let workspace = fixture.root.join("workspace");
    let state = fixture.root.join("state");
    fs::create_dir_all(&workspace).expect("创建工作区");
    create_private_directory(&state);

    // 注册成功且无部分落盘：目标存在为目录，清理预览拒绝（无宿主资产可证）。
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    let base_oid = {
        let output = fixture
            .command(&repository)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("读取基准 OID");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    let identity = match fixture
        .git
        .open_local_project(&repository)
        .await
        .expect("打开源仓库")
    {
        soxu_git::LocalProject::GitWorktree(identity) => identity,
        _ => panic!("必须是 Git 工作树"),
    };
    let registration = soxu_git::WorktreeRegistrationRequest {
        request_id: "cleanup-ok",
        source: &identity,
        workspace_root: &workspace,
        target_name: "cleanup-ok",
        branch_ref: "refs/heads/soxu/cleanup-ok",
        base_oid: &base_oid,
    };
    let outcome = fixture
        .git
        .register_new_worktree(&coordinator, registration)
        .await
        .expect("注册必须成功");
    let soxu_git::RegistrationOutcome::Verified(_) = outcome else {
        panic!("注册必须验证通过");
    };

    let outcome = preview(
        &fixture.git,
        &repository,
        RecoveryPreviewRequest {
            source_root: &repository,
            coordinator_state_root: Some(&state),
            target_authorization: Some(&workspace.join("cleanup-ok")),
            operation: RecoveryOperation::PartialCleanup,
            partial_cleanup_request_id: Some("cleanup-ok"),
        },
    )
    .await;
    let PreviewOutcome::PartialCleanup(cleanup) = outcome else {
        panic!("必须返回部分落盘清理预览");
    };
    assert_eq!(cleanup.request_id, "cleanup-ok");
    assert_eq!(
        cleanup.rejection,
        Some(soxu_git::PartialCleanupRejection::TargetUnprovable),
        "目标已发布为完整工作树时，清理预览不得声称可删除"
    );

    // 未知请求 ID：拒绝 IntentNotFound。
    let outcome = preview(
        &fixture.git,
        &repository,
        RecoveryPreviewRequest {
            source_root: &repository,
            coordinator_state_root: Some(&state),
            target_authorization: None,
            operation: RecoveryOperation::PartialCleanup,
            partial_cleanup_request_id: Some("missing-request"),
        },
    )
    .await;
    let PreviewOutcome::PartialCleanup(cleanup) = outcome else {
        panic!("必须返回部分落盘清理预览");
    };
    assert_eq!(
        cleanup.rejection,
        Some(soxu_git::PartialCleanupRejection::IntentNotFound)
    );
}

#[tokio::test]
#[cfg(not(windows))] // Windows 协调域 fail-closed（与 lifecycle 集测同款门控）
async fn checkout_journal_status_feeds_observation_states() {
    // 检出留痕读取入口复用 journal：Verified / Pending / Disabled 三态。
    // 该用例用协调域 + 注册成功的请求验证 Pending（无检出记录）。
    let fixture = Fixture::new();
    let state = fixture.root.join("state");
    create_private_directory(&state);
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
    // 通过 checkout_status 读取尚未检出的注册，预期 Pending。
    let status = fixture
        .git
        .checkout_status(&coordinator, "no-such-request")
        .expect("读取检出留痕");
    assert_eq!(status, soxu_git::CheckoutStatus::Pending);
}

#[tokio::test]
async fn preview_rejects_relative_or_non_repository_roots() {
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let plain = fixture.root.join("plain");
    fs::create_dir_all(&plain).expect("创建普通目录");

    let outcome = fixture
        .git
        .recovery_preview(request(
            Path::new("relative"),
            RecoveryOperation::Prune,
            None,
            None,
        ))
        .await;
    assert!(matches!(
        outcome,
        Err(soxu_git::RecoveryPreviewError::InvalidInput(_))
    ));

    let outcome = fixture
        .git
        .recovery_preview(request(&plain, RecoveryOperation::Prune, None, None))
        .await;
    assert!(
        matches!(
            outcome,
            Err(soxu_git::RecoveryPreviewError::InvalidInput(_))
        ),
        "普通目录必须拒绝"
    );

    // 非当前仓库根上的目标授权必须为绝对路径。
    let outcome = fixture
        .git
        .recovery_preview(RecoveryPreviewRequest {
            source_root: &repository,
            coordinator_state_root: None,
            target_authorization: Some(Path::new("relative-target")),
            operation: RecoveryOperation::Prune,
            partial_cleanup_request_id: None,
        })
        .await;
    assert!(matches!(
        outcome,
        Err(soxu_git::RecoveryPreviewError::InvalidInput(_))
    ));
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn linux_head_corruption_is_reported_as_anomaly() {
    // Linux 专属：清单在 HEAD 缺失时输出零 OID detached（本机实测），
    // 预览必须标记 HeadOrRepositoryAnomaly 而不是 Healthy。
    let fixture = Fixture::new();
    let repository = fixture.init_repository();
    let wt = fixture.native("wt");
    fixture.run(
        &repository,
        &["worktree", "add", "-q", &wt.to_string_lossy(), "-b", "b1"],
    );
    let admin = repository.join(".git/worktrees/wt");
    fs::remove_file(admin.join("HEAD")).expect("删除 HEAD 制造异常");
    let outcome = preview(
        &fixture.git,
        &repository,
        RecoveryPreviewRequest {
            source_root: &repository,
            coordinator_state_root: None,
            target_authorization: Some(&fixture.native("wt")),
            operation: RecoveryOperation::Unlock,
            partial_cleanup_request_id: None,
        },
    )
    .await;
    let PreviewOutcome::Unlock(unlock) = outcome else {
        panic!("必须返回 unlock 预览");
    };
    assert!(
        unlock
            .observation
            .states
            .iter()
            .any(|state| state.state == WorktreeObservationKind::HeadOrRepositoryAnomaly),
        "HEAD 异常必须单独分类"
    );
    // TargetState 观察必须如实记录目录可达。
    assert_eq!(unlock.observation.target_state, TargetState::Directory);
}
