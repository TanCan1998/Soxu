//! 真实 Git 恢复执行夹具。仓库、工作树和协调域均为新建临时目录。

#![cfg(any(target_os = "linux", windows))]

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use soxu_git::{GitProbe, WorktreeRegistrationCoordinator};

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    state: PathBuf,
    workspace: PathBuf,
    git: GitProbe,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = loop {
            let path = std::env::temp_dir().join(format!(
                "soxu-recovery-execute-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            match fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("创建夹具失败：{error}"),
            }
        };
        #[cfg(target_os = "linux")]
        let root = root.canonicalize().expect("夹具根规范路径");
        let fixture = Self {
            repo: root.join("repo"),
            state: root.join("state"),
            workspace: root.join("workspace"),
            root,
            git: GitProbe::discover().expect("测试需要原生 Git"),
        };
        fs::create_dir(&fixture.repo).expect("创建源仓库目录");
        fs::create_dir(&fixture.workspace).expect("创建工作区目录");
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&fixture.state)
                .expect("创建私有协调域");
        }
        #[cfg(windows)]
        fs::create_dir(&fixture.state).expect("创建协调域目录");
        fixture.run(&["init", "-q", "-b", "main"]);
        fs::write(fixture.repo.join("tracked.txt"), b"fixture\n").expect("写入夹具文件");
        fixture.run(&["add", "tracked.txt"]);
        fixture.run(&["commit", "-qm", "fixture"]);
        fixture
    }

    fn run(&self, args: &[&str]) {
        let mut command = Command::new(self.git.program());
        command.current_dir(&self.repo);
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
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
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
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ])
            .args(args);
        let mut child = command.spawn().expect("启动夹具 Git");
        let deadline = Instant::now() + Duration::from_secs(15);
        let failure = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    assert!(status.success(), "夹具 Git 非零退出：{status}");
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Ok(None) => break "超时",
                Err(_) => break "状态读取失败",
            }
        };
        let _ = child.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                panic!("夹具 Git {failure}，直接子进程已回收");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("夹具 Git {failure}，回收未确认");
    }

    fn worktree(&self, name: &str) -> PathBuf {
        let target = self.workspace.join(name);
        self.run(&[
            "worktree",
            "add",
            "-qb",
            name,
            target.to_str().expect("夹具路径"),
        ]);
        target
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(windows)]
#[test]
fn windows_coordinator_fails_closed_without_writes() {
    let fixture = Fixture::new();
    let target = fixture.worktree("wt");
    assert!(matches!(
        WorktreeRegistrationCoordinator::open(&fixture.state, &[]),
        Err(soxu_git::WorktreeLifecycleError::UnsupportedFilesystem(_))
    ));
    assert!(target.exists());
    assert!(
        fs::read_dir(&fixture.state)
            .expect("读取协调域")
            .next()
            .is_none()
    );
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::Path;
    use std::time::SystemTime;

    use soxu_git::{
        ExecutionStage, PreviewOutcome, RecoveryCancellation, RecoveryConfirmation,
        RecoveryExecuteError, RecoveryExecutionOutcome, RecoveryExecutionPreview,
        RecoveryOperation, RecoveryPreviewRequest,
    };

    use super::*;

    impl Fixture {
        fn coordinator(&self) -> WorktreeRegistrationCoordinator {
            WorktreeRegistrationCoordinator::open(&self.state, &[]).expect("打开协调域")
        }

        fn locked(&self) -> PathBuf {
            let target = self.worktree("wt");
            self.run(&["worktree", "lock", "wt", "--reason", "所有者待复核"]);
            target
        }

        async fn prepare(
            &self,
            id: &str,
            operation: RecoveryOperation,
            target: Option<&Path>,
        ) -> RecoveryExecutionPreview {
            self.git
                .prepare_recovery(
                    &self.coordinator(),
                    id,
                    RecoveryPreviewRequest {
                        source_root: &self.repo,
                        coordinator_state_root: Some(&self.state),
                        target_authorization: target,
                        operation,
                        partial_cleanup_request_id: None,
                    },
                )
                .await
                .expect("读取审批预览")
        }

        async fn unlock(&self, id: &str, target: &Path) -> RecoveryConfirmation {
            self.prepare(id, RecoveryOperation::Unlock, Some(target))
                .await
                .confirm(SystemTime::now() + Duration::from_secs(30))
        }
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
        fn collect(root: &Path, path: &Path, items: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
            for entry in fs::read_dir(path).expect("读取夹具目录") {
                let entry = entry.expect("读取夹具条目");
                let path = entry.path();
                let directory = entry.file_type().expect("读取夹具类型").is_dir();
                items.push((
                    path.strip_prefix(root).expect("夹具内路径").to_owned(),
                    (!directory).then(|| fs::read(&path).expect("读取夹具文件")),
                ));
                if directory {
                    collect(root, &path, items);
                }
            }
        }
        let mut items = Vec::new();
        collect(root, root, &mut items);
        items.sort();
        items
    }

    fn clone_directory(from: &Path, to: &Path) {
        fs::create_dir(to).expect("创建替代目录");
        for entry in fs::read_dir(from).expect("读取原目录") {
            let entry = entry.expect("读取原目录条目");
            let to = to.join(entry.file_name());
            if entry.file_type().expect("读取条目类型").is_dir() {
                clone_directory(&entry.path(), &to);
            } else {
                fs::copy(entry.path(), to).expect("复制同字节夹具内容");
            }
        }
    }

    #[tokio::test]
    async fn linux_prune_single_and_multiple_candidates_are_readonly_even_with_same_dry_run() {
        let fixture = Fixture::new();
        for name in ["single", "additional"] {
            let target = fixture.worktree(name);
            fs::rename(&target, fixture.workspace.join(format!("{name}-moved"))).expect("模拟失联");
            let before = snapshot(&fixture.root);
            let first = fixture.prepare(name, RecoveryOperation::Prune, None).await;
            let second = fixture.prepare(name, RecoveryOperation::Prune, None).await;
            assert_eq!(first.digest(), second.digest());
            let PreviewOutcome::Prune(candidate) = first.outcome() else {
                panic!("prune 预览")
            };
            assert_eq!(
                candidate.candidates.len(),
                if name == "single" { 1 } else { 2 }
            );
            assert!(matches!(
                fixture
                    .git
                    .execute_recovery(
                        &fixture.coordinator(),
                        &first.confirm(SystemTime::now() + Duration::from_secs(30)),
                        None
                    )
                    .await,
                Err(RecoveryExecuteError::EvidenceUnavailable(
                    "prune_candidate_evidence_and_external_exclusion_required"
                ))
            ));
            assert_eq!(before, snapshot(&fixture.root));
        }
    }

    #[tokio::test]
    async fn linux_repair_and_remove_keep_all_assets_when_write_evidence_is_missing() {
        let fixture = Fixture::new();
        let online = fixture.worktree("online");
        for ignored_file in [false, true] {
            if ignored_file {
                fs::write(fixture.repo.join(".git/info/exclude"), b"ignored.tmp\n")
                    .expect("声明忽略规则");
                fs::write(online.join("ignored.tmp"), b"keep\n").expect("创建被忽略的用户数据");
            }
            let before = snapshot(&fixture.root);
            let removal = fixture
                .prepare(
                    "remove",
                    RecoveryOperation::RemoveRegistration,
                    Some(&online),
                )
                .await;
            let PreviewOutcome::RemoveRegistration(candidate) = removal.outcome() else {
                panic!("移除预览")
            };
            assert!(candidate.rejection.is_none(), "旧只读预览得到源仓库干净");
            assert!(matches!(
                fixture
                    .git
                    .execute_recovery(
                        &fixture.coordinator(),
                        &removal.confirm(SystemTime::now() + Duration::from_secs(30)),
                        None
                    )
                    .await,
                Err(RecoveryExecuteError::EvidenceUnavailable(
                    "remove_target_cleanliness_and_recovery_evidence_required"
                ))
            ));
            assert_eq!(before, snapshot(&fixture.root));
        }
        let lost = fixture.worktree("lost");
        let moved = fixture.workspace.join("moved");
        fs::rename(lost, &moved).expect("移动真实工作树");
        let other = fixture.workspace.join("unrelated");
        fs::create_dir(&other).expect("创建无关联目标");
        for target in [&moved, &other] {
            let before = snapshot(&fixture.root);
            let repair = fixture
                .prepare("repair", RecoveryOperation::Repair, Some(target))
                .await;
            let PreviewOutcome::Repair(candidate) = repair.outcome() else {
                panic!("修复预览")
            };
            assert_eq!(candidate.candidates.len(), 1);
            assert!(candidate.rejection.is_none(), "目录可达不足以证明映射");
            assert!(matches!(
                fixture
                    .git
                    .execute_recovery(
                        &fixture.coordinator(),
                        &repair.confirm(SystemTime::now() + Duration::from_secs(30)),
                        None
                    )
                    .await,
                Err(RecoveryExecuteError::EvidenceUnavailable(
                    "repair_bidirectional_identity_and_full_scope_required"
                ))
            ));
            assert_eq!(before, snapshot(&fixture.root));
        }
    }

    #[tokio::test]
    async fn linux_same_path_same_bytes_replacement_rejects_repository_and_target() {
        for replace_repository in [false, true] {
            let fixture = Fixture::new();
            let target = fixture.locked();
            fs::File::create(fixture.state.join("registration.lock")).expect("建立夹具锁文件");
            let confirmation = fixture.unlock("replacement", &target).await;
            let replaced = if replace_repository {
                &fixture.repo
            } else {
                &target
            };
            let original = fixture.root.join("original");
            let source_bytes = snapshot(replaced);
            fs::rename(replaced, &original).expect("保留原目录");
            clone_directory(&original, replaced);
            assert_eq!(source_bytes, snapshot(replaced));
            let before = snapshot(&fixture.root);
            assert!(matches!(
                fixture
                    .git
                    .execute_recovery(&fixture.coordinator(), &confirmation, None)
                    .await,
                Err(RecoveryExecuteError::IdentityChanged)
            ));
            assert_eq!(before, snapshot(&fixture.root));
        }
    }

    #[tokio::test]
    async fn linux_expiry_during_lock_wait_keeps_journal_absent() {
        let fixture = Fixture::new();
        let target = fixture.locked();
        let preview = fixture
            .prepare("expires", RecoveryOperation::Unlock, Some(&target))
            .await;
        let handle =
            fs::File::create(fixture.state.join("registration.lock")).expect("建立夹具锁文件");
        handle.try_lock().expect("持有夹具预约");
        let before = snapshot(&fixture.root);
        let confirmation = preview.confirm(SystemTime::now() + Duration::from_millis(400));
        let coordinator = fixture.coordinator();
        let execution = fixture
            .git
            .execute_recovery(&coordinator, &confirmation, None);
        tokio::pin!(execution);
        tokio::select! {
            result = &mut execution => panic!("持锁期间不应返回：{result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(500)) => drop(handle),
        }
        assert!(matches!(
            execution.await,
            Err(RecoveryExecuteError::ConfirmationExpired)
        ));
        assert_eq!(before, snapshot(&fixture.root));
    }

    #[tokio::test]
    async fn linux_unlock_rejects_drift_and_serializes_duplicate_confirmation() {
        let fixture = Fixture::new();
        let target = fixture.locked();
        fs::File::create(fixture.state.join("registration.lock")).expect("建立夹具锁文件");
        let stale = fixture.unlock("stale", &target).await;
        fixture.run(&["worktree", "unlock", "wt"]);
        fixture.run(&["worktree", "lock", "wt", "--reason", "锁理由变化"]);
        let before = snapshot(&fixture.root);
        let coordinator = fixture.coordinator();
        assert!(matches!(
            fixture
                .git
                .execute_recovery(&coordinator, &stale, None)
                .await,
            Err(RecoveryExecuteError::PreviewDrift)
        ));
        let confirmation = fixture.unlock("unlock", &target).await;
        let cancel = RecoveryCancellation::new();
        cancel.cancel();
        assert!(matches!(
            fixture
                .git
                .execute_recovery(&coordinator, &confirmation, Some(&cancel))
                .await,
            Err(RecoveryExecuteError::Cancelled)
        ));
        assert_eq!(before, snapshot(&fixture.root));
        let (first, second) = tokio::join!(
            fixture
                .git
                .execute_recovery(&coordinator, &confirmation, None),
            fixture
                .git
                .execute_recovery(&coordinator, &confirmation, None),
        );
        let (outcome, replay) = if first.is_ok() {
            (first, second)
        } else {
            (second, first)
        };
        let RecoveryExecutionOutcome::Completed {
            record,
            observation,
        } = outcome.expect("原生解锁成功")
        else {
            panic!("解锁回读不符")
        };
        assert!(observation.target_registered);
        assert_eq!(record.stage, ExecutionStage::Observed);
        assert!(matches!(
            replay,
            Err(RecoveryExecuteError::AlreadyAttempted(_))
        ));
        let before_diagnosis = snapshot(&fixture.root);
        let reopened = fixture.coordinator();
        assert_eq!(
            fixture
                .git
                .diagnose_recovery(&reopened, "unlock")
                .expect("重启只读诊断"),
            Some(record)
        );
        assert_eq!(before_diagnosis, snapshot(&fixture.root));
    }
}
