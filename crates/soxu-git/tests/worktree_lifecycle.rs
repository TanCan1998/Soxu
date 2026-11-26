use std::fs;
#[cfg(not(windows))]
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(windows))]
use soxu_git::{ExecutionResult, ExecutionStage, RegistrationOutcome, TargetState};
use soxu_git::{
    GitProbe, WorktreeIdentity, WorktreeLifecycleError, WorktreeRegistrationCoordinator,
    WorktreeRegistrationRequest,
};

#[derive(Clone)]
struct Fixture {
    git: GitProbe,
    repository: PathBuf,
    workspace: PathBuf,
    state: PathBuf,
    base_oid: String,
}

#[tokio::test]
#[cfg(not(windows))]
async fn caller_cancellation_preserves_started_intent_and_finishes_write_owner() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let running_marker = fixture
        .workspace
        .parent()
        .expect("运行标记需要工作区夹具的父目录")
        .join("git-running");
    let release_marker = fixture
        .workspace
        .parent()
        .expect("放行标记需要工作区夹具的父目录")
        .join("release-git");
    let git = fixture.proxy(
        "slow-git.sh",
        &format!(
            "for argument in \"$@\"; do\n\
               if [ \"$argument\" = add ]; then\n\
                 printf running > {}\n\
                 until [ -f {} ]; do sleep 0.02; done\n\
                 break\n\
               fi\n\
             done",
            shell_quote(&running_marker),
            shell_quote(&release_marker),
        ),
    );
    let registering_fixture = fixture.clone();
    let registration = tokio::spawn(async move {
        let request = registering_fixture.request(
            &source,
            "started-observer",
            "started-target",
            "refs/heads/soxu/started-observer",
            &registering_fixture.base_oid,
        );
        git.register_new_worktree(&registering_fixture.coordinator(), request)
            .await
    });
    let intent_path = fixture
        .state
        .join("intents")
        .join("started-observer.intent");
    for _ in 0..500 {
        let Ok(bytes) = fs::read(&intent_path) else {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            continue;
        };
        if bytes.windows(7).any(|window| window == b"started")
            && bytes
                .windows(17)
                .any(|window| window == b"unknown:0:running")
            && running_marker.exists()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        running_marker.exists() && intent_path.exists(),
        "未观察到子进程运行期间的 Started 意图"
    );
    let persisted = fs::read(&intent_path).expect("读取运行中意图");
    assert!(
        persisted.windows(7).any(|window| window == b"started")
            && persisted
                .windows(17)
                .any(|window| window == b"unknown:0:running")
    );

    assert!(matches!(
        fixture
            .git
            .diagnose_registration(&fixture.state, "started-observer", &fixture.repository,)
            .await,
        Err(WorktreeLifecycleError::ExecutionStillUnknown)
    ));
    registration.abort();
    assert!(
        registration
            .await
            .expect_err("中止调用方后注册任务应返回取消错误")
            .is_cancelled()
    );
    fs::write(release_marker, b"release").expect("写入标记以放行阻塞的 Git 写入进程");
    let source = fixture.identity().await;
    let request = fixture.request(
        &source,
        "started-observer",
        "started-target",
        "refs/heads/soxu/started-observer",
        &fixture.base_oid,
    );
    let outcome = fixture.register(request).await;
    let RegistrationOutcome::ExistingIntent(registration) = outcome else {
        panic!("预期独立执行者完成注册后只读重放：{outcome:?}");
    };
    assert_eq!(
        registration.observation.ref_oid.as_deref(),
        Some(fixture.base_oid.as_str())
    );
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let git = GitProbe::discover().expect("本机原生 Git");
        let root = loop {
            let candidate = std::env::temp_dir().join(format!(
                "soxu-worktree-registration-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => break candidate.canonicalize().expect("规范化夹具根"),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("创建夹具根失败：{error}"),
            }
        };
        let repository = root.join("repository");
        let workspace = root.join("workspace");
        let state = root.join(".soxu-worktree-registration");
        fs::create_dir(&repository).expect("创建源工作树");
        fs::create_dir(&workspace).expect("创建授权工作区");
        create_state_directory(&state);
        let fixture = Self {
            git,
            repository: repository.clone(),
            workspace,
            state,
            base_oid: String::new(),
        };
        fixture.git(&repository, &["init", "-q", "--initial-branch=main"]);
        fixture.git(&repository, &["config", "core.autocrlf", "false"]);
        fixture.write("seed.txt", b"base\n");
        fixture.git(&repository, &["add", "--all"]);
        fixture.git(
            &repository,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "base",
            ],
        );
        let base_oid =
            String::from_utf8(fixture.git(&repository, &["rev-parse", "--verify", "HEAD"]))
                .expect("基准 OID")
                .trim()
                .to_owned();
        Self {
            base_oid,
            ..fixture
        }
    }

    async fn identity(&self) -> WorktreeIdentity {
        match self.git.open_local_project(&self.repository).await {
            Ok(soxu_git::LocalProject::GitWorktree(identity)) => identity,
            other => panic!("夹具必须是 Git 工作树：{other:?}"),
        }
    }

    fn request<'a>(
        &'a self,
        source: &'a WorktreeIdentity,
        request_id: &'a str,
        target_name: &'a str,
        branch_ref: &'a str,
        base_oid: &'a str,
    ) -> WorktreeRegistrationRequest<'a> {
        WorktreeRegistrationRequest {
            request_id,
            source,
            workspace_root: &self.workspace,
            target_name,
            branch_ref,
            base_oid,
        }
    }

    #[cfg(not(windows))]
    fn coordinator(&self) -> WorktreeRegistrationCoordinator {
        WorktreeRegistrationCoordinator::open(&self.state, &[]).expect("打开夹具协调域")
    }

    #[cfg(not(windows))]
    async fn register(&self, request: WorktreeRegistrationRequest<'_>) -> RegistrationOutcome {
        self.git
            .register_new_worktree(&self.coordinator(), request)
            .await
            .expect("注册调用")
    }

    #[cfg(not(windows))]
    fn proxy(&self, name: &str, body: &str) -> GitProbe {
        use std::os::unix::fs::PermissionsExt;

        let path = self
            .workspace
            .parent()
            .expect("Git 替身需要工作区夹具的父目录")
            .join(name);
        let real_git = shell_quote(self.git.program());
        fs::write(
            &path,
            format!("#!/bin/sh\n{body}\nexec {real_git} \"$@\"\n"),
        )
        .expect("写入 Git 替身脚本");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("为 Git 替身脚本设置仅所有者可读写执行的权限");
        GitProbe::new(path).expect("用 Git 替身脚本初始化探测器")
    }

    fn command(&self, directory: &Path) -> Command {
        let mut command = Command::new(self.git.program());
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
            .current_dir(directory)
            .stdin(Stdio::null())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
        command
    }

    fn git(&self, directory: &Path, arguments: &[&str]) -> Vec<u8> {
        let output = self
            .command(directory)
            .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
            .args(arguments)
            .output()
            .expect("执行夹具 Git");
        success(output)
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        fs::write(self.repository.join(path), bytes).expect("写入夹具文件");
    }
}

fn create_state_directory(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .expect("创建私有持久状态目录");
    }
    #[cfg(not(unix))]
    fs::create_dir(path).expect("创建持久状态目录");
}

#[cfg(not(windows))]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "夹具 Git 失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[cfg(not(windows))]
fn target_children(target: &Path) -> Vec<String> {
    let mut names = fs::read_dir(target)
        .expect("读取注册目标")
        .map(|entry| {
            entry
                .expect("目标条目")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[cfg(not(windows))]
fn intent_count(state: &Path) -> usize {
    fs::read_dir(state.join("intents"))
        .map(|entries| {
            entries
                .map(|entry| entry.expect("状态条目"))
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "intent")
                })
                .count()
        })
        .unwrap_or_default()
}

#[tokio::test]
#[cfg(not(windows))]
async fn registers_fixed_base_and_replays_readonly() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let request = fixture.request(
        &source,
        "request-one",
        "target-one",
        "refs/heads/soxu/registered",
        &fixture.base_oid,
    );
    fixture.git(
        &fixture.repository,
        &["commit", "--allow-empty", "-qm", "next"],
    );
    let outcome = fixture.register(request).await;
    let RegistrationOutcome::Verified(registration) = outcome else {
        let diagnosis = fixture
            .git
            .diagnose_registration(&fixture.state, "request-one", &fixture.repository)
            .await;
        panic!("固定基准注册应成功：{outcome:?}；诊断={diagnosis:?}");
    };
    assert_eq!(registration.intent.base_oid, fixture.base_oid);
    assert_eq!(
        target_children(&registration.intent.target),
        [".git".to_owned()]
    );
    assert!(registration.observation.target_matches);
    assert_eq!(
        registration.observation.ref_oid.as_deref(),
        Some(fixture.base_oid.as_str())
    );
    let head = String::from_utf8(fixture.git(
        &registration.intent.target,
        &["rev-parse", "--symbolic-full-name", "HEAD"],
    ))
    .expect("目标 HEAD");
    assert_eq!(head.trim(), "refs/heads/soxu/registered");
    let branch_oid = String::from_utf8(fixture.git(
        &fixture.repository,
        &["rev-parse", "--verify", "refs/heads/soxu/registered"],
    ))
    .expect("注册分支");
    assert_eq!(branch_oid.trim(), fixture.base_oid);

    let source = fixture.identity().await;
    let replay = fixture.request(
        &source,
        "request-one",
        "target-one",
        "refs/heads/soxu/registered",
        &fixture.base_oid,
    );
    let outcome = fixture.register(replay).await;
    let RegistrationOutcome::ExistingIntent(replay) = outcome else {
        panic!("同名同参数应只读重放：{outcome:?}");
    };
    assert!(replay.observation.target_matches);
    assert_eq!(replay.intent, registration.intent);
    assert_eq!(replay.intent.execution.stage, ExecutionStage::Observed);
    assert_eq!(replay.intent.execution.result, ExecutionResult::Succeeded);
    let diagnosis = fixture
        .git
        .diagnose_registration(&fixture.state, "request-one", &fixture.repository)
        .await
        .expect("只读诊断");
    let diagnosis = diagnosis.expect("诊断请求");
    assert!(diagnosis.target_registered);
    assert!(!diagnosis.target_matches);
    assert_eq!(diagnosis.target_state, TargetState::NotExamined);
    assert_eq!(
        target_children(&fixture.workspace.join("target-one")),
        [".git".to_owned()]
    );
}

#[tokio::test]
#[cfg(not(windows))]
async fn registers_two_linked_siblings_and_rejects_occupied_or_nested_targets() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    for (request_id, name, branch) in [
        ("first", "first", "refs/heads/soxu/first"),
        ("second", "second", "refs/heads/soxu/second"),
    ] {
        let request = fixture.request(&source, request_id, name, branch, &fixture.base_oid);
        let outcome = fixture.register(request).await;
        assert!(
            matches!(outcome, RegistrationOutcome::Verified(_)),
            "{outcome:?}"
        );
    }
    assert_eq!(
        fixture
            .git
            .list_worktrees(&fixture.repository)
            .await
            .expect("读取含两个同级 linked worktree 的注册清单")
            .entries
            .len(),
        3
    );
    let duplicate = fixture.request(
        &source,
        "duplicate",
        "second",
        "refs/heads/soxu/duplicate",
        &fixture.base_oid,
    );
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&fixture.coordinator(), duplicate)
            .await,
        Err(WorktreeLifecycleError::TargetReserved { .. })
    ));
    let nested = fixture.workspace.join("first").join("nested");
    fs::create_dir(&nested).expect("在已注册工作树内创建嵌套工作区夹具");
    let request = WorktreeRegistrationRequest {
        workspace_root: &nested,
        request_id: "nested",
        source: &source,
        target_name: "target",
        branch_ref: "refs/heads/soxu/nested",
        base_oid: &fixture.base_oid,
    };
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&fixture.coordinator(), request)
            .await,
        Err(WorktreeLifecycleError::Unauthorized(_))
    ));
    assert!(!nested.join("target").exists());
}

#[tokio::test]
#[cfg(not(windows))]
async fn preserves_nonzero_exit_and_directory_only_footprint_through_replay() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let git = fixture.proxy(
        "directory-only-git.sh",
        "for argument in \"$@\"; do\n\
           if [ \"$argument\" = add ]; then\n\
             while [ \"$1\" != -- ]; do shift; done\n\
             shift\n\
             mkdir \"$1\" || exit 99\n\
             exit 37\n\
           fi\n\
         done",
    );
    let request = fixture.request(
        &source,
        "directory-only",
        "directory-only",
        "refs/heads/soxu/directory-only",
        &fixture.base_oid,
    );
    let outcome = git
        .register_new_worktree(&fixture.coordinator(), request.clone())
        .await
        .expect("读取 Git 非零退出且仅留下目标目录的注册结果");
    let RegistrationOutcome::PartialFootprint {
        intent,
        observation,
        command_succeeded,
    } = outcome
    else {
        panic!("目录足迹必须被报告：{outcome:?}");
    };
    assert!(!command_succeeded);
    assert_eq!(
        intent.execution.result,
        ExecutionResult::Failed {
            exit_code: Some(37),
        }
    );
    assert_eq!(intent.execution.observed_at, Some(observation.observed_at));
    assert_eq!(observation.target_state, TargetState::Directory);
    assert!(!observation.ref_present);
    assert!(!observation.target_registered);
    assert!(observation.target_identity.is_none());
    let path = fixture.state.join("intents").join("directory-only.intent");
    let persisted = fs::read(&path).expect("读取重放前的目录足迹意图");
    let replay = fixture.register(request).await;
    assert!(matches!(
        replay,
        RegistrationOutcome::PartialFootprint { intent: replayed, .. } if replayed == intent
    ));
    assert_eq!(fs::read(path).expect("读取重放后的目录足迹意图"), persisted);
}

#[tokio::test]
#[cfg(not(windows))]
async fn spawn_failure_preserves_the_reservation_and_never_records_a_start_time() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let name = "fail-to-spawn.sh";
    let intent_path = fixture.state.join("intents").join("spawn-failure.intent");
    let passed_ref_check = fixture.workspace.join("passed-ref-check");
    let git = fixture.proxy(
        name,
        &format!(
            "for argument in \"$@\"; do\n\
               if [ \"$argument\" = --heads ] && [ -f {} ]; then\n\
                 if [ -f {} ]; then chmod 0600 \"$0\" || exit 98;\n\
                 else printf done > {}; fi\n\
               fi\n\
             done",
            shell_quote(&intent_path),
            shell_quote(&passed_ref_check),
            shell_quote(&passed_ref_check),
        ),
    );
    let request = fixture.request(
        &source,
        "spawn-failure",
        "target",
        "refs/heads/soxu/spawn-failure",
        &fixture.base_oid,
    );
    let outcome = git
        .register_new_worktree(&fixture.coordinator(), request.clone())
        .await
        .expect("读取 Git 写入进程启动失败后的持久注册结果");
    let RegistrationOutcome::UnknownOutcome {
        intent,
        observation: None,
        ..
    } = outcome
    else {
        panic!("预期写进程未启动：{outcome:?}");
    };
    assert_eq!(intent.execution.result, ExecutionResult::SpawnFailed);
    assert_eq!(intent.execution.stage, ExecutionStage::Completed);
    assert!(intent.execution.started_at.is_none());
    let bytes = fs::read(&intent_path).expect("读取重放前的启动失败意图");
    let replay = fixture.register(request).await;
    assert!(matches!(
        replay,
        RegistrationOutcome::UnknownOutcome { intent: replayed, .. } if replayed == intent
    ));
    assert_eq!(
        fs::read(&intent_path).expect("读取重放后的启动失败意图"),
        bytes
    );
    assert!(!fixture.workspace.join("target").exists());
}

#[tokio::test]
#[cfg(not(windows))]
async fn ref_or_registration_drift_keeps_exit_evidence_and_prevents_verification() {
    for drift in ["ref", "registration"] {
        let fixture = Fixture::new();
        let source = fixture.identity().await;
        fixture.git(
            &fixture.repository,
            &["commit", "--allow-empty", "-qm", "next"],
        );
        let real_git = shell_quote(fixture.git.program());
        let target = fixture.workspace.join("target");
        let wrote = shell_quote(&fixture.workspace.join("write-started"));
        let listed = shell_quote(&fixture.workspace.join("first-list"));
        let (checkpoint, mutation) = if drift == "ref" {
            (
                "--exists",
                format!("{real_git} update-ref refs/heads/soxu/final-drift HEAD"),
            )
        } else {
            (
                "list",
                format!(
                    "{real_git} worktree lock --reason fixture {}",
                    shell_quote(&target)
                ),
            )
        };
        let git = fixture.proxy(
            "final-drift-git.sh",
            &format!(
                "for argument in \"$@\"; do\n\
                   case \"$argument\" in\n\
                   add) printf start > {wrote};;\n\
                   {checkpoint}) if [ -f {wrote} ]; then\n\
                           if [ -f {listed} ]; then {mutation} || exit 96;\n\
                           else printf list > {listed}; fi\n\
                         fi;;\n\
                   esac\n\
                 done"
            ),
        );
        let request = fixture.request(
            &source,
            drift,
            "target",
            "refs/heads/soxu/final-drift",
            &fixture.base_oid,
        );
        let outcome = git
            .register_new_worktree(&fixture.coordinator(), request)
            .await
            .expect("读取退出后出现 ref 或注册漂移的注册结果");
        let RegistrationOutcome::UnknownOutcome {
            intent,
            observation: None,
            ..
        } = outcome
        else {
            panic!("{drift} 漂移预期复核未知：{outcome:?}");
        };
        assert_eq!(intent.execution.stage, ExecutionStage::Completed);
        assert_eq!(intent.execution.result, ExecutionResult::Succeeded);
        assert!(intent.execution.observed_at.is_none());
    }
}

#[tokio::test]
#[cfg(not(windows))]
async fn rejects_target_and_branch_occupancy_before_intent() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    fs::write(fixture.workspace.join("occupied"), b"external\n").expect("外部占用");
    let target_request = fixture.request(
        &source,
        "target-request",
        "occupied",
        "refs/heads/soxu/target",
        &fixture.base_oid,
    );
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&fixture.coordinator(), target_request)
            .await,
        Err(WorktreeLifecycleError::TargetExists)
    ));
    assert_eq!(intent_count(&fixture.state), 0);

    fixture.git(&fixture.repository, &["branch", "soxu/occupied", "HEAD"]);
    let source = fixture.identity().await;
    let ref_request = fixture.request(
        &source,
        "ref-request",
        "other-target",
        "refs/heads/soxu/occupied",
        &fixture.base_oid,
    );
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&fixture.coordinator(), ref_request)
            .await,
        Err(WorktreeLifecycleError::RefExists)
    ));
    assert_eq!(intent_count(&fixture.state), 0);
}

#[tokio::test]
#[cfg(not(windows))]
async fn rejects_branch_prefix_collision() {
    let fixture = Fixture::new();
    fixture.git(&fixture.repository, &["branch", "soxu/prefix", "HEAD"]);
    let source = fixture.identity().await;
    let request = fixture.request(
        &source,
        "prefix-request",
        "prefix-target",
        "refs/heads/soxu/prefix/child",
        &fixture.base_oid,
    );
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&fixture.coordinator(), request)
            .await,
        Err(WorktreeLifecycleError::RefExists)
    ));
    assert_eq!(intent_count(&fixture.state), 0);
}

#[tokio::test]
#[cfg(not(windows))]
async fn rejects_workspace_or_state_inside_repository_boundary() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let nested = fixture.repository.join("workspace");
    fs::create_dir(&nested).expect("嵌套工作区");
    let request = WorktreeRegistrationRequest {
        request_id: "nested-request",
        source: &source,
        workspace_root: &nested,
        target_name: "target",
        branch_ref: "refs/heads/soxu/nested",
        base_oid: &fixture.base_oid,
    };
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&fixture.coordinator(), request)
            .await,
        Err(WorktreeLifecycleError::Unauthorized(_))
    ));

    let state = fixture.repository.join("soxu-state");
    create_state_directory(&state);
    let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("嵌套状态协调域");
    let request = WorktreeRegistrationRequest {
        request_id: "nested-state",
        source: &source,
        workspace_root: &fixture.workspace,
        target_name: "target",
        branch_ref: "refs/heads/soxu/nested-state",
        base_oid: &fixture.base_oid,
    };
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&coordinator, request)
            .await,
        Err(WorktreeLifecycleError::Unauthorized(_))
    ));
    assert!(!state.join("registration.lock").exists());
}

#[tokio::test]
#[cfg(not(windows))]
async fn serializes_busy_persistent_state() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let lock_path = fixture.state.join("registration.lock");
    let lock = File::create(&lock_path).expect("创建外部状态锁");
    lock.try_lock().expect("持有外部状态锁");
    let request = fixture.request(
        &source,
        "busy-request",
        "busy-target",
        "refs/heads/soxu/busy",
        &fixture.base_oid,
    );
    let result = fixture
        .git
        .register_new_worktree(&fixture.coordinator(), request)
        .await;
    assert!(matches!(result, Err(WorktreeLifecycleError::StateBusy)));
    drop(lock);
}

#[tokio::test]
#[cfg(not(windows))]
async fn reports_partial_footprint_after_target_drift() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let request = fixture.request(
        &source,
        "drift-request",
        "drift-target",
        "refs/heads/soxu/drift",
        &fixture.base_oid,
    );
    let outcome = fixture.register(request).await;
    let RegistrationOutcome::Verified(registration) = outcome else {
        panic!("漂移前置注册应成功：{outcome:?}");
    };
    let moved = fixture.workspace.join("drift-target-moved");
    fs::rename(&registration.intent.target, &moved).expect("移动目标以制造漂移");
    let source = fixture.identity().await;
    let replay = fixture.request(
        &source,
        "drift-request",
        "drift-target",
        "refs/heads/soxu/drift",
        &fixture.base_oid,
    );
    let outcome = fixture.register(replay).await;
    let RegistrationOutcome::PartialFootprint {
        intent: partial_intent,
        observation: partial_observation,
        command_succeeded: partial_command_succeeded,
    } = outcome
    else {
        panic!("移动后的注册应保留部分足迹：{outcome:?}");
    };
    assert_eq!(partial_intent.request_id, "drift-request");
    assert!(partial_command_succeeded);
    assert!(partial_observation.target_registered);
    assert!(!partial_observation.target_matches);
    assert!(partial_observation.target_identity.is_none());

    fs::create_dir(&registration.intent.target).expect("在原目标路径创建替换目录");
    fs::copy(moved.join(".git"), registration.intent.target.join(".git"))
        .expect("将原工作树的 .git 指针复制到替换目录");
    let request = fixture.request(
        &source,
        "drift-request",
        "drift-target",
        "refs/heads/soxu/drift",
        &fixture.base_oid,
    );
    let git = fixture.proxy(
        "forbid-target-probe.sh",
        &format!(
            "if [ \"$PWD\" = {} ]; then printf probed > {}; exit 96; fi",
            shell_quote(&registration.intent.target),
            shell_quote(&fixture.workspace.join("target-probed")),
        ),
    );
    let outcome = git
        .register_new_worktree(&fixture.coordinator(), request)
        .await
        .expect("读取替换目标目录后重放意图的注册结果");
    assert!(matches!(
        outcome,
        RegistrationOutcome::PartialFootprint { observation, .. }
            if observation.target_state == TargetState::Directory
                && observation.target_identity.is_none()
                && !observation.target_matches
    ));
    assert!(!fixture.workspace.join("target-probed").exists());
}

#[tokio::test]
#[cfg(not(windows))]
async fn rejects_local_config_include() {
    let fixture = Fixture::new();
    fixture.write(
        "included.config",
        b"[core]\n\trepositoryformatversion = 0\n",
    );
    fixture.git(
        &fixture.repository,
        &["config", "include.path", "included.config"],
    );
    let source = fixture.identity().await;
    let request = fixture.request(
        &source,
        "include-request",
        "include-target",
        "refs/heads/soxu/include",
        &fixture.base_oid,
    );
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&fixture.coordinator(), request)
            .await,
        Err(WorktreeLifecycleError::UnsupportedConfiguration(_))
    ));
    assert_eq!(intent_count(&fixture.state), 0);
}

#[tokio::test]
#[cfg(not(windows))]
async fn final_preflight_rejects_target_after_intent_persisted() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let intent_path = fixture.state.join("intents").join("late-occupied.intent");
    let target = fixture.workspace.join("late-target");
    let git = fixture.proxy(
        "late-target-git.sh",
        &format!(
            "if [ -f {} ] && [ ! -e {} ]; then printf 'external\\n' > {}; fi",
            shell_quote(&intent_path),
            shell_quote(&target),
            shell_quote(&target),
        ),
    );
    let request = fixture.request(
        &source,
        "late-occupied",
        "late-target",
        "refs/heads/soxu/late-occupied",
        &fixture.base_oid,
    );
    let result = git
        .register_new_worktree(&fixture.coordinator(), request)
        .await
        .expect_err("最终预检应返回错误");
    assert!(intent_path.is_file());
    assert!(matches!(result, WorktreeLifecycleError::TargetExists));
    assert_eq!(fs::read(&target).expect("外部目标未被删除"), b"external\n");

    let branch = fixture
        .command(&fixture.repository)
        .args(["rev-parse", "--verify", "refs/heads/soxu/late-occupied"])
        .output()
        .expect("查询分支");
    assert!(!branch.status.success());
    assert!(String::from_utf8_lossy(&branch.stderr).contains("unknown revision"));

    let observation = fixture
        .git
        .diagnose_registration_with_target(
            &fixture.state,
            "late-occupied",
            &fixture.repository,
            &target,
        )
        .await
        .expect("只读诊断")
        .expect("诊断已知请求");
    assert!(!observation.ref_present);
    assert!(!observation.target_registered);
    assert_eq!(observation.target_state, soxu_git::TargetState::File);
}

#[cfg(windows)]
#[tokio::test]
async fn windows_without_native_directory_identity_fails_closed() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let request = fixture.request(
        &source,
        "windows-gate",
        "windows-target",
        "refs/heads/soxu/windows-gate",
        &fixture.base_oid,
    );
    let result = WorktreeRegistrationCoordinator::open(&fixture.state, &[]);
    assert!(matches!(
        result,
        Err(WorktreeLifecycleError::UnsupportedFilesystem(
            "Windows 稳定 std 不提供目录身份"
        ))
    ));
    assert!(!fixture.state.join("registration.lock").exists());
    assert!(!fixture.state.join("intents").exists());
    assert!(!request.workspace_root.join(request.target_name).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn coordinator_process_worker() {
    let Some(state) = std::env::var_os("SOXU_TEST_COORDINATION_ROOT") else {
        return;
    };
    let coordinator = WorktreeRegistrationCoordinator::open(PathBuf::from(state), &[])
        .expect("子进程打开宿主协调域");
    let git = GitProbe::new(PathBuf::from(
        std::env::var_os("SOXU_TEST_COORDINATION_GIT").expect("子进程 Git 路径"),
    ))
    .expect("子进程绑定 Git");
    let repository =
        PathBuf::from(std::env::var_os("SOXU_TEST_COORDINATION_REPO").expect("子进程仓库"));
    let workspace =
        PathBuf::from(std::env::var_os("SOXU_TEST_COORDINATION_WORKSPACE").expect("子进程工作区"));
    let source = git
        .open_local_project(repository)
        .await
        .expect("子进程打开源仓库");
    let soxu_git::LocalProject::GitWorktree(source) = source else {
        panic!("子进程源目录应为 Git 工作树");
    };
    let request_id = std::env::var("SOXU_TEST_COORDINATION_ID").expect("子进程请求 ID");
    let target = std::env::var("SOXU_TEST_COORDINATION_TARGET").expect("子进程目标");
    let base = std::env::var("SOXU_TEST_COORDINATION_BASE").expect("子进程基准");
    let branch = format!("refs/heads/soxu/{request_id}");
    let ready = std::env::var_os("SOXU_TEST_COORDINATION_READY").expect("子进程就绪标记路径");
    fs::write(ready, b"ready").expect("标记子进程即将申请注册");
    let result = git
        .register_new_worktree(
            &coordinator,
            WorktreeRegistrationRequest {
                request_id: &request_id,
                source: &source,
                workspace_root: &workspace,
                target_name: &target,
                branch_ref: &branch,
                base_oid: &base,
            },
        )
        .await;
    let expected = std::env::var("SOXU_TEST_COORDINATION_EXPECTED").expect("子进程预期结果");
    let actual = match &result {
        Ok(RegistrationOutcome::Verified(_)) => "verified",
        Ok(RegistrationOutcome::ExistingIntent(_)) => "replayed",
        Ok(RegistrationOutcome::UnknownOutcome {
            intent,
            observation: None,
            ..
        }) if intent.execution.stage == ExecutionStage::Started
            && intent.execution.started_at.is_some()
            && matches!(
                &intent.execution.result,
                ExecutionResult::Unknown {
                    stop_confirmed: false,
                    ..
                }
            ) =>
        {
            "unknown"
        }
        Err(WorktreeLifecycleError::StateBusy) => "busy",
        Err(WorktreeLifecycleError::TargetReserved { .. }) => "target-reserved",
        Err(WorktreeLifecycleError::ExecutionStillUnknown) => "repository-fenced",
        _ => panic!("子进程返回未预期的注册结果：{result:?}"),
    };
    assert_eq!(actual, expected, "{result:?}");
}

#[cfg(unix)]
struct RegistrationProcess(std::process::Child);

#[cfg(unix)]
impl RegistrationProcess {
    fn start(fixture: &Fixture, git: &GitProbe, id: &str, target: &str, expected: &str) -> Self {
        let executable = std::env::current_exe().expect("定位当前测试二进制");
        let child = Command::new(executable)
            .args(["--exact", "coordinator_process_worker", "--nocapture"])
            .env("SOXU_TEST_COORDINATION_ROOT", &fixture.state)
            .env("SOXU_TEST_COORDINATION_GIT", git.program())
            .env("SOXU_TEST_COORDINATION_REPO", &fixture.repository)
            .env("SOXU_TEST_COORDINATION_WORKSPACE", &fixture.workspace)
            .env("SOXU_TEST_COORDINATION_ID", id)
            .env("SOXU_TEST_COORDINATION_TARGET", target)
            .env("SOXU_TEST_COORDINATION_BASE", &fixture.base_oid)
            .env(
                "SOXU_TEST_COORDINATION_READY",
                fixture.state.join(format!("{id}.ready")),
            )
            .env("SOXU_TEST_COORDINATION_EXPECTED", expected)
            .spawn()
            .expect("启动独立注册测试进程");
        Self(child)
    }

    fn finish(&mut self) {
        wait_for_fixture("独立注册进程退出", || {
            let Some(status) = self.0.try_wait().expect("查询独立注册进程") else {
                return false;
            };
            assert!(status.success(), "独立注册进程失败：{status}");
            true
        });
    }

    fn interrupt(&mut self) {
        self.0.kill().expect("终止注册所有者夹具");
        assert!(!self.0.wait().expect("回收注册所有者夹具").success());
    }
}

#[cfg(unix)]
impl Drop for RegistrationProcess {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[cfg(unix)]
fn wait_for_fixture(description: &str, mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !ready() {
        assert!(std::time::Instant::now() < deadline, "{description}超时");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn gate_git(fixture: &Fixture, started: &Path, release: &Path, after_release: &str) -> GitProbe {
    let started = shell_quote(started);
    let release = shell_quote(release);
    fixture.proxy(
        "gate-git.sh",
        &format!(
            "for argument in \"$@\"; do
               if [ \"$argument\" = add ]; then
                 printf started > {started} || exit 91
                 attempts=0
                 until [ -f {release} ]; do
                   attempts=$((attempts + 1))
                   [ \"$attempts\" -lt 600 ] || exit 92
                   sleep 0.02
                 done
                 {after_release}
                 break
               fi
             done",
        ),
    )
}

#[cfg(unix)]
fn forbid_registration_git(fixture: &Fixture, marker: &Path) -> GitProbe {
    fixture.proxy(
        "forbid-registration-git.sh",
        &format!(
            "for argument in \"$@\"; do
               if [ \"$argument\" = add ]; then printf unexpected > {}; exit 93; fi
             done",
            shell_quote(marker),
        ),
    )
}

#[cfg(unix)]
fn workspace_in_another_parent(fixture: &Fixture) -> Fixture {
    let parent = fixture.workspace.with_file_name("other-parent");
    let workspace = parent.join("workspace");
    fs::create_dir_all(&workspace).expect("创建另一父目录下的授权工作区");
    Fixture {
        workspace,
        ..fixture.clone()
    }
}

#[cfg(unix)]
#[test]
fn coordinator_serializes_across_parents_and_reserves_targets_across_repositories() {
    let first = Fixture::new();
    let across_parent = workspace_in_another_parent(&first);
    let another_repository = Fixture {
        state: first.state.clone(),
        workspace: first.workspace.clone(),
        ..Fixture::new()
    };
    let started = first.workspace.with_file_name("first-git-started");
    let release = first.workspace.with_file_name("release-first-git");
    let forbidden = first.workspace.with_file_name("unexpected-git-add");
    let gated = gate_git(&first, &started, &release, ":");
    let read_only = forbid_registration_git(&first, &forbidden);
    let mut writer = RegistrationProcess::start(&first, &gated, "first", "shared", "verified");
    wait_for_fixture("首个 Git 写进程到达屏障", || started.is_file());
    let first_intent = first.state.join("intents/first.intent");
    assert!(first_intent.is_file());
    RegistrationProcess::start(&across_parent, &read_only, "across", "second", "busy").finish();
    assert!(!first.state.join("intents/across.intent").exists());
    let mut contender = RegistrationProcess::start(
        &another_repository,
        &read_only,
        "contender",
        "shared",
        "target-reserved",
    );
    wait_for_fixture("跨仓竞争进程到达注册入口", || {
        first.state.join("contender.ready").is_file()
    });
    fs::write(&release, b"release").expect("放行首个 Git 写进程");
    writer.finish();
    contender.finish();
    RegistrationProcess::start(
        &across_parent,
        &across_parent.git,
        "across",
        "second",
        "verified",
    )
    .finish();
    assert!(!first.state.join("intents/contender.intent").exists());
    let original = fs::read(&first_intent).expect("读取完成后的原始意图");
    RegistrationProcess::start(&first, &read_only, "first", "shared", "replayed").finish();
    assert_eq!(
        fs::read(first_intent).expect("重启后读取同一意图"),
        original
    );

    let renamed = first.workspace.with_file_name("renamed-workspace");
    fs::rename(&first.workspace, &renamed).expect("保留原生身份并改名工作区夹具");
    let renamed = Fixture {
        workspace: renamed,
        ..another_repository
    };
    RegistrationProcess::start(&renamed, &read_only, "renamed", "shared", "target-reserved")
        .finish();
    assert!(!forbidden.exists());
}

#[cfg(unix)]
#[test]
fn coordinator_restart_preserves_started_intent_and_fences_repository() {
    let first = Fixture::new();
    let across_parent = workspace_in_another_parent(&first);
    let other_repository = Fixture {
        state: first.state.clone(),
        workspace: first.workspace.clone(),
        ..Fixture::new()
    };
    let started = first.workspace.with_file_name("interrupted-git-started");
    let release = first.workspace.with_file_name("release-interrupted-git");
    let stopped = first.workspace.with_file_name("interrupted-git-stopped");
    let forbidden = first.workspace.with_file_name("unexpected-git-add");
    let gated = gate_git(
        &first,
        &started,
        &release,
        &format!("printf stopped > {}; exit 94", shell_quote(&stopped)),
    );
    let read_only = forbid_registration_git(&first, &forbidden);
    let mut writer = RegistrationProcess::start(&first, &gated, "interrupted", "shared", "unknown");
    wait_for_fixture("Git 写进程在所有者终止前到达屏障", || {
        started.is_file()
    });
    let path = first.state.join("intents/interrupted.intent");
    let original = fs::read(&path).expect("读取终止前持久化的 Started 意图");
    writer.interrupt();
    RegistrationProcess::start(&first, &read_only, "interrupted", "shared", "unknown").finish();
    RegistrationProcess::start(
        &across_parent,
        &read_only,
        "same-repo",
        "different-target",
        "repository-fenced",
    )
    .finish();
    RegistrationProcess::start(
        &other_repository,
        &read_only,
        "other-repo",
        "shared",
        "target-reserved",
    )
    .finish();
    fs::write(release, b"release").expect("放行并停止失去所有者的 Git 替身");
    wait_for_fixture("失去所有者的 Git 替身即将退出", || {
        stopped.is_file()
    });
    assert_eq!(fs::read(path).expect("读取重启后的 Started 意图"), original);
    assert_eq!(intent_count(&first.state), 1);
    assert!(!first.workspace.join("shared").exists());
    assert!(!forbidden.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn coordinator_leaves_legacy_state_read_only() {
    let fixture = Fixture::new();
    let source = fixture.identity().await;
    let request = fixture.request(
        &source,
        "old",
        "old-target",
        "refs/heads/soxu/old",
        &fixture.base_oid,
    );
    let RegistrationOutcome::Verified(registration) = fixture.register(request.clone()).await
    else {
        panic!("建立旧布局的 SOXUWT3 注册意图");
    };
    let path = fixture.state.join("intents/old.intent");
    let version_three = fs::read(&path).expect("读取旧布局原意图");
    let new_domain = fixture.state.with_file_name("private-coordination");
    create_state_directory(&new_domain);
    assert!(matches!(
        WorktreeRegistrationCoordinator::open(&new_domain, &[&fixture.state]),
        Err(WorktreeLifecycleError::LegacyStateRequiresReview)
    ));
    assert!(!new_domain.join("registration.lock").exists());
    assert!(
        fixture
            .git
            .diagnose_registration(&fixture.state, "old", &fixture.repository)
            .await
            .expect("在原目录只读诊断旧布局")
            .is_some()
    );
    assert_eq!(fs::read(&path).expect("核对旧布局原意图"), version_three);

    let foreign_intents = new_domain.join("intents");
    fs::create_dir(&foreign_intents).expect("建立包含外域留痕的夹具");
    fs::write(foreign_intents.join("old.intent"), &version_three).expect("放入外域身份意图夹具");
    let foreign = WorktreeRegistrationCoordinator::open(&new_domain, &[]).expect("打开夹具协调域");
    let fresh = fixture.request(
        &source,
        "fresh",
        "fresh-target",
        "refs/heads/soxu/fresh",
        &fixture.base_oid,
    );
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&foreign, fresh.clone())
            .await,
        Err(WorktreeLifecycleError::LegacyStateRequiresReview)
    ));
    assert_eq!(
        fs::read(foreign_intents.join("old.intent")).expect("核对外域意图原字节"),
        version_three
    );

    let body = version_three
        .strip_prefix(b"SOXUWT3\0unix-dev-ino-birth\0")
        .expect("旧布局记录使用 SOXUWT3");
    let fields = body
        .get(..body.len() - 7 * 32)
        .expect("已验证记录含七份原生目录身份");
    let version_two = [b"SOXUWT2\0".as_slice(), fields].concat();
    fs::write(&path, &version_two).expect("在夹具模拟没有目录身份的旧版记录");
    let replay = fixture.register(request).await;
    assert!(matches!(
        replay,
        RegistrationOutcome::UnknownOutcome { intent, observation: None, .. }
            if intent.recorded_at == registration.intent.recorded_at
                && intent.execution == registration.intent.execution
    ));
    assert!(matches!(
        fixture
            .git
            .register_new_worktree(&fixture.coordinator(), fresh)
            .await,
        Err(WorktreeLifecycleError::LegacyStateRequiresReview)
    ));
    assert_eq!(fs::read(path).expect("核对旧版记录原字节"), version_two);
    assert_eq!(intent_count(&fixture.state), 1);
}
