use soxu_daemon::{DaemonState, SqliteStore, build_router, state::WorktreeAdmissionRequest};
use soxu_drivers::{NativeCliKind, NativeDriver};
use soxu_git::GitProbe;
use soxu_magpie::{EnvironmentMode, WORKTREE_CONTEXT_FILE};
use soxu_protocol::{
    ActiveTurnBehavior, ErrorCode, PermissionLevel, ProjectSummary, RpcCommand, RpcRequest,
    RpcResponse, RpcResultData, SessionSummary, SessionTarget, WorkspaceBucket, WorkspaceIsolation,
    WorkspaceSummary,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn daemon(repository: &Path) -> DaemonState {
    daemon_with_environment(repository, None, EnvironmentMode::StandaloneNative)
}

fn daemon_with_environment(
    repository: &Path,
    state_dir: Option<&Path>,
    environment: EnvironmentMode,
) -> DaemonState {
    let state =
        DaemonState::open_with_environment(state_dir, "http://localhost", false, environment)
            .expect("打开临时 Daemon 状态");
    state
        .upsert_project(ProjectSummary {
            project_id: "test-project".to_owned(),
            name: "测试项目".to_owned(),
            display_path: "测试项目".to_owned(),
            root_path: repository.to_str().expect("夹具路径应为 UTF-8").to_owned(),
            is_git_repo: true,
            default_branch: Some("main".to_owned()),
            aggregated_bucket: WorkspaceBucket::Done,
            workspace_count: 0,
        })
        .expect("夹具操作应成功");
    state
}

fn workspace() -> WorkspaceSummary {
    WorkspaceSummary {
        workspace_id: String::new(),
        project_id: "test-project".to_owned(),
        project_name: String::new(),
        display_path: String::new(),
        branch: String::new(),
        isolation: WorkspaceIsolation::Local,
        agent_id: "codex".to_owned(),
        model_id: String::new(),
        bucket: WorkspaceBucket::Done,
        summary: String::new(),
        diff_stat: String::new(),
        label: None,
        archived: false,
        needs_recovery: false,
        recovery_reason: None,
        updated_at: chrono::Utc::now(),
    }
}

fn create_session(state: &DaemonState, workspace_id: &str) -> RpcResponse {
    state.handle_rpc(RpcRequest {
        request_id: "test-create-session".to_owned(),
        command: RpcCommand::CreateSession {
            target: SessionTarget::Workspace {
                workspace_id: workspace_id.to_owned(),
            },
            title: None,
            agent_id: "codex".to_owned(),
            model_id: None,
            permission: None,
            thinking: None,
        },
    })
}

#[test]
fn generic_workspace_upsert_cannot_bypass_admission() {
    let fixture = tempfile::tempdir().expect("创建隔离夹具目录");
    let state = daemon(fixture.path());
    let mut summary = workspace();
    summary.workspace_id = "legacy-id".to_owned();
    summary.isolation = WorkspaceIsolation::Worktree {
        branch: "test".to_owned(),
        base_ref: "main".to_owned(),
    };
    assert_eq!(
        state
            .upsert_workspace(summary.clone())
            .expect_err("夹具操作应被拒绝")
            .code,
        ErrorCode::SafetyRedlineBlocked
    );
    summary.isolation = WorkspaceIsolation::Local;
    summary.workspace_id = "worktree:fixture".to_owned();
    assert_eq!(
        state
            .upsert_workspace(summary)
            .expect_err("夹具操作应被拒绝")
            .code,
        ErrorCode::SafetyRedlineBlocked
    );
    assert_eq!(
        create_session(&state, "worktree:fixture")
            .error
            .expect("夹具操作应成功")
            .code,
        ErrorCode::NotFound
    );
    assert!(
        state
            .snapshot()
            .expect("读取工作区快照")
            .workspaces
            .is_empty()
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_registration_filesystem_denial_leaves_no_workspace_or_asset() {
    let fixture = tempfile::tempdir().expect("创建隔离夹具目录");
    let registration = fixture.path().join("registration");
    let target = fixture.path().join("target");
    fs::create_dir(&registration).expect("创建夹具文件或目录");
    fs::create_dir(&target).expect("创建夹具文件或目录");
    let state = daemon(fixture.path());
    let git = GitProbe::discover().expect("定位夹具 Git");
    let error = state
        .admit_worktree(
            &git,
            &WorktreeAdmissionRequest {
                state_root: &registration,
                request_id: "fixture",
                target: &target,
                environment_mode: EnvironmentMode::MagpieManaged,
                magpie_config_dir: Some(fixture.path()),
            },
            workspace(),
        )
        .await
        .expect_err("夹具操作应被拒绝");
    assert!(matches!(
        error.downcast_ref::<soxu_git::WorktreeLifecycleError>(),
        Some(soxu_git::WorktreeLifecycleError::UnsupportedFilesystem(_))
    ));
    assert!(
        state
            .snapshot()
            .expect("读取工作区快照")
            .workspaces
            .is_empty()
    );
    assert!(!target.join(WORKTREE_CONTEXT_FILE).exists());
    assert!(
        fs::read_dir(registration)
            .expect("读取夹具文件或目录")
            .next()
            .is_none()
    );
}

const QUICK_CHAT_AGENTS: [(NativeCliKind, &str); 3] = [
    (NativeCliKind::Copilot, "copilot"),
    (NativeCliKind::OpenCode, "opencode"),
    (NativeCliKind::Antigravity, "agy"),
];

struct NativeStub {
    script: PathBuf,
    log: PathBuf,
    #[cfg(target_os = "linux")]
    release: PathBuf,
}

impl NativeStub {
    fn new(root: &Path) -> Self {
        let script = root.join("worktree-native.cjs");
        fs::write(&script, br#"
const fs = require('node:fs');
const prompt = fs.readFileSync(0, 'utf8');
const args = process.argv.slice(2);
const session = 'aabbccdd-1234-4234-8234-aabbccddeeff';
const resume = args.includes('resume');
if (resume && args.at(-2) !== session) process.exit(1);
fs.appendFileSync(process.env.SOXU_STUB_LOG, JSON.stringify([process.cwd(), resume, prompt]) + '\n');
const emit = (value) => console.log(JSON.stringify(value));
emit({type: 'thread.started', thread_id: session});
emit({type: 'turn.started'});
const finish = () => {
  emit({type: 'item.completed', item: {type: 'agent_message', text: prompt}});
  emit({type: 'turn.completed'});
};
if (prompt === 'hold') {
  const timer = setInterval(() => {
    if (fs.existsSync(process.env.SOXU_STUB_RELEASE)) {
      clearInterval(timer);
      finish();
    }
  }, 10);
  setTimeout(() => {
    clearInterval(timer);
    process.exitCode = 1;
  }, 8000).unref();
} else {
  finish();
}
"#).expect("写入临时原生桩驱动");
        Self {
            script,
            log: root.join("native-invocations.jsonl"),
            #[cfg(target_os = "linux")]
            release: root.join("release-native"),
        }
    }

    fn configure(&self, state: DaemonState) -> DaemonState {
        let node = soxu_daemon::antigravity::node_program().expect("定位测试 Node 可执行文件");
        let configured = [(NativeCliKind::Codex, "codex")]
            .into_iter()
            .chain(QUICK_CHAT_AGENTS)
            .map(|(kind, _)| {
                let driver = NativeDriver::with_program(
                    kind,
                    node.clone(),
                    [self.script.as_os_str().to_owned()],
                )
                .expect("配置临时原生桩驱动")
                .without_inherited_env()
                .with_env("SOXU_STUB_LOG", Some(self.log.as_os_str().to_owned()));
                #[cfg(target_os = "linux")]
                let driver = driver.with_env(
                    "SOXU_STUB_RELEASE",
                    Some(self.release.as_os_str().to_owned()),
                );
                (kind, Ok(driver))
            });
        state
            .with_native_driver_set(self.script.with_extension("quick-chats"), configured)
            .expect("注册临时原生桩驱动")
    }

    fn invocations(&self) -> Vec<(PathBuf, bool, String)> {
        match fs::read_to_string(&self.log) {
            Ok(log) => log
                .lines()
                .map(|line| serde_json::from_str(line).expect("解码桩驱动调用日志"))
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("读取桩驱动调用日志失败：{error}"),
        }
    }
}

struct NativeRpc {
    state: Arc<DaemonState>,
    address: std::net::SocketAddr,
    server: tokio::task::JoinHandle<()>,
}

impl NativeRpc {
    async fn spawn(state: DaemonState) -> Self {
        let state = Arc::new(state);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("绑定临时回环端口");
        let address = listener.local_addr().expect("读取回环监听地址");
        let router = build_router(Arc::clone(&state));
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("运行回环 Daemon");
        });
        Self {
            state,
            address,
            server,
        }
    }

    async fn call(&self, command: RpcCommand) -> RpcResponse {
        let body = serde_json::to_string(&RpcRequest {
            request_id: "worktree-native".to_owned(),
            command,
        })
        .expect("编码工作树 RPC");
        let request = format!(
            "POST /api/rpc HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.address,
            body.len()
        );
        let response = tokio::time::timeout(Duration::from_secs(10), async {
            let mut stream = TcpStream::connect(self.address)
                .await
                .expect("连接回环 Daemon");
            stream
                .write_all(request.as_bytes())
                .await
                .expect("发送工作树 RPC");
            let mut response = String::new();
            stream
                .read_to_string(&mut response)
                .await
                .expect("读取工作树 RPC 响应");
            response
        })
        .await
        .expect("回环 RPC 应在期限内返回");
        let (headers, body) = response.split_once("\r\n\r\n").expect("解析 HTTP 响应");
        assert!(headers.starts_with("HTTP/1.1 200 "), "{headers}");
        serde_json::from_str(body).expect("解码工作树 RPC 响应")
    }

    async fn submit(&self, session_id: &str, prompt: &str) -> RpcResponse {
        self.call(RpcCommand::SubmitTurn {
            session_id: session_id.to_owned(),
            text: prompt.to_owned(),
            chips: Vec::new(),
            behavior: ActiveTurnBehavior::Queue,
            handoff: None,
            handoff_scope: None,
        })
        .await
    }
}

impl Drop for NativeRpc {
    fn drop(&mut self) {
        self.server.abort();
    }
}

fn create_native_session(
    state: &DaemonState,
    workspace_id: &str,
    agent_id: &str,
) -> SessionSummary {
    let response = state.handle_rpc(RpcRequest {
        request_id: "worktree-session".to_owned(),
        command: RpcCommand::CreateSession {
            target: SessionTarget::Workspace {
                workspace_id: workspace_id.to_owned(),
            },
            title: None,
            agent_id: agent_id.to_owned(),
            model_id: None,
            permission: Some(PermissionLevel::ReadOnly),
            thinking: None,
        },
    });
    let Some(RpcResultData::SessionCreated(session)) = response.result else {
        panic!("创建只读会话失败：{:?}", response.error);
    };
    assert_eq!(session.agent_id, agent_id);
    assert_eq!(session.permission, PermissionLevel::ReadOnly);
    session
}

fn assert_rpc_error(response: RpcResponse, code: ErrorCode) {
    assert!(!response.ok, "RPC 应返回错误：{response:?}");
    assert_eq!(response.error.expect("读取 RPC 错误").code, code);
}

#[cfg(target_os = "linux")]
async fn assert_quick_chat_drivers_rejected(rpc: &NativeRpc, workspace_id: &str) {
    for (_, id) in QUICK_CHAT_AGENTS {
        let session = create_native_session(&rpc.state, workspace_id, id);
        assert_rpc_error(
            rpc.submit(&session.session_id, "边界检查").await,
            ErrorCode::DriverUnavailable,
        );
        assert!(
            rpc.state
                .list_session_messages(&session.session_id)
                .expect("读取会话消息")
                .is_empty()
        );
    }
}

#[tokio::test]
async fn unready_worktrees_reject_native_launch_and_quick_chat_only_drivers() {
    let temp = tempfile::tempdir().expect("创建隔离原生夹具");
    let repository = temp.path().join("project");
    let checkout = temp.path().join("checkout");
    fs::create_dir(&repository).expect("创建项目根目录夹具");
    fs::create_dir(&checkout).expect("创建工作树目录夹具");
    let state_dir = temp.path().join("state");
    let stub = NativeStub::new(temp.path());
    let state = stub.configure(daemon_with_environment(
        &repository,
        Some(&state_dir),
        EnvironmentMode::StandaloneNative,
    ));
    let store = SqliteStore::open(&state_dir.join("soxu-daemon.db")).expect("打开临时数据库");
    let mut local = workspace();
    local.workspace_id = "fixture-local".to_owned();
    local.display_path = checkout.to_str().expect("UTF-8 工作树夹具路径").to_owned();
    state
        .upsert_workspace(local.clone())
        .expect("以本地工作区创建已有会话");
    let codex = create_native_session(&state, &local.workspace_id, "codex");
    let rpc = NativeRpc::spawn(state).await;
    assert_quick_chat_drivers_rejected_on_stored_worktree(&rpc, &store, &local).await;
    local.isolation = WorkspaceIsolation::Worktree {
        branch: "fixture".to_owned(),
        base_ref: "main".to_owned(),
    };
    for (needs_recovery, archived) in [(false, false), (true, false), (false, true)] {
        local.needs_recovery = needs_recovery;
        local.archived = archived;
        store
            .upsert_workspace(&local)
            .expect("写入未经接入的持久工作树夹具");
        assert_rpc_error(
            rpc.submit(&codex.session_id, "禁止回退").await,
            ErrorCode::WorkspaceDamaged,
        );
        assert!(
            rpc.state
                .list_session_messages(&codex.session_id)
                .expect("读取拒绝启动的会话消息")
                .is_empty()
        );
    }
    assert!(stub.invocations().is_empty());
}

async fn assert_quick_chat_drivers_rejected_on_stored_worktree(
    rpc: &NativeRpc,
    store: &SqliteStore,
    local: &WorkspaceSummary,
) {
    let sessions =
        QUICK_CHAT_AGENTS.map(|(_, id)| create_native_session(&rpc.state, &local.workspace_id, id));
    let worktree = WorkspaceSummary {
        isolation: WorkspaceIsolation::Worktree {
            branch: "fixture".to_owned(),
            base_ref: "main".to_owned(),
        },
        ..local.clone()
    };
    store
        .upsert_workspace(&worktree)
        .expect("写入未经接入的持久工作树夹具");
    for session in sessions {
        assert_rpc_error(
            rpc.submit(&session.session_id, "边界检查").await,
            ErrorCode::DriverUnavailable,
        );
        assert!(
            rpc.state
                .list_session_messages(&session.session_id)
                .expect("读取会话消息")
                .is_empty()
        );
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use soxu_daemon::state::WorktreeAdmission;
    use soxu_git::{
        CheckoutStatus, LocalProject, RegistrationOutcome, VerifiedRegistration,
        WorktreeRegistrationCoordinator, WorktreeRegistrationRequest,
    };
    use soxu_magpie::{WorktreeContext, WorktreeContextError};
    use soxu_protocol::{DaemonEvent, EventEnvelope, TurnLifecycleState, TurnMessageRole};
    use std::{os::unix::fs::DirBuilderExt, process::Command};

    struct Fixture {
        _temp: tempfile::TempDir,
        repository: PathBuf,
        registration: PathBuf,
        workspaces: PathBuf,
        target: PathBuf,
        magpie: PathBuf,
        git: GitProbe,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("创建隔离夹具目录");
            let root = temp.path().canonicalize().expect("创建隔离夹具目录");
            let registration = root.join("registration");
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&registration)
                .expect("创建夹具文件或目录");
            let fixture = Self {
                _temp: temp,
                repository: root.join("repository"),
                registration,
                workspaces: root.join("workspaces"),
                target: root.join("workspaces/target"),
                magpie: root.join("magpie"),
                git: GitProbe::discover().expect("定位夹具 Git"),
            };
            fs::create_dir(&fixture.repository).expect("创建夹具文件或目录");
            fs::create_dir(&fixture.workspaces).expect("创建夹具文件或目录");
            fs::create_dir(&fixture.magpie).expect("创建夹具文件或目录");
            fs::write(
                fixture.magpie.join("library.json"),
                br#"{"mcpHub":false,"mcp":[{"name":"aoci"}]}"#,
            )
            .expect("创建夹具文件或目录");
            fixture.run(&["init", "-q", "--template=", "--initial-branch=main"]);
            fs::write(
                fixture.repository.join(".gitattributes"),
                b"story.txt text eol=crlf\n",
            )
            .expect("创建夹具文件或目录");
            fs::write(fixture.repository.join("story.txt"), b"line\n").expect("创建夹具文件或目录");
            fixture
        }

        fn run(&self, args: &[&str]) -> String {
            let output = Command::new(self.git.program())
                .current_dir(&self.repository)
                .env_clear()
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(args)
                .output()
                .expect("运行夹具子进程");
            assert!(
                output.status.success(),
                "Git 夹具准备失败：{:?}",
                output.stderr
            );
            String::from_utf8(output.stdout)
                .expect("解析夹具输出")
                .trim()
                .to_owned()
        }

        fn coordinator(&self) -> WorktreeRegistrationCoordinator {
            WorktreeRegistrationCoordinator::open(&self.registration, &[]).expect("解析夹具输出")
        }

        async fn register(&self) -> VerifiedRegistration {
            self.run(&["add", "--all"]);
            self.run(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ]);
            let LocalProject::GitWorktree(source) = self
                .git
                .open_local_project(&self.repository)
                .await
                .expect("识别夹具 Git 项目")
            else {
                panic!("Git 夹具缺少工作树身份");
            };
            let oid = self.run(&["rev-parse", "HEAD"]);
            let outcome = self
                .git
                .register_new_worktree(
                    &self.coordinator(),
                    WorktreeRegistrationRequest {
                        request_id: "fixture",
                        source: &source,
                        workspace_root: &self.workspaces,
                        target_name: "target",
                        branch_ref: "refs/heads/fixture",
                        base_oid: &oid,
                    },
                )
                .await
                .expect("夹具操作应成功");
            let RegistrationOutcome::Verified(registration) = outcome else {
                panic!("Git 夹具注册未通过：{outcome:?}");
            };
            registration
        }

        fn request(&self, environment_mode: EnvironmentMode) -> WorktreeAdmissionRequest<'_> {
            WorktreeAdmissionRequest {
                state_root: &self.registration,
                request_id: "fixture",
                target: &self.target,
                environment_mode,
                magpie_config_dir: Some(&self.magpie),
            }
        }
    }

    async fn admit_ready_worktree(
        fixture: &Fixture,
        state: &DaemonState,
        mode: EnvironmentMode,
    ) -> WorkspaceSummary {
        let registration = fixture.register().await;
        assert_eq!(
            fixture
                .git
                .checkout_registered_worktree(&fixture.coordinator(), &registration)
                .await
                .expect("执行夹具 Worktree 检出"),
            CheckoutStatus::Verified
        );
        let WorktreeAdmission::Ready { workspace, .. } = state
            .admit_worktree(&fixture.git, &fixture.request(mode), workspace())
            .await
            .expect("复核并接入夹具 Worktree")
        else {
            panic!("夹具工作树应准备就绪");
        };
        workspace
    }

    async fn ready_native_worktree(
        fixture: &Fixture,
        stub: &NativeStub,
    ) -> (NativeRpc, WorkspaceSummary) {
        let state = stub.configure(daemon_with_environment(
            &fixture.repository,
            None,
            EnvironmentMode::MagpieManaged,
        ));
        let workspace = admit_ready_worktree(fixture, &state, EnvironmentMode::MagpieManaged).await;
        (NativeRpc::spawn(state).await, workspace)
    }

    #[tokio::test]
    async fn native_worktree_session_runs_two_turns_from_verified_worktree_directory() {
        let fixture = Fixture::new();
        let stub = NativeStub::new(fixture._temp.path());
        let (rpc, workspace) = ready_native_worktree(&fixture, &stub).await;
        let mut events = rpc.state.subscribe_events();
        let session = create_native_session(&rpc.state, &workspace.workspace_id, "codex");

        for prompt in ["首轮", "续接"] {
            assert!(rpc.submit(&session.session_id, prompt).await.ok);
            wait_for_native_event(&mut events, |event| {
                matches!(event, DaemonEvent::SessionUpdated(item)
                    if item.session_id == session.session_id && item.lifecycle == TurnLifecycleState::Idle)
            })
            .await;
        }

        assert_eq!(
            stub.invocations(),
            [
                (fixture.target.clone(), false, "首轮".to_owned()),
                (fixture.target.clone(), true, "续接".to_owned()),
            ]
        );
        let messages = rpc
            .state
            .list_session_messages(&session.session_id)
            .expect("读取工作树会话消息");
        assert_eq!(messages.len(), 4);
        assert!(
            messages
                .iter()
                .filter(|item| item.role == TurnMessageRole::Assistant)
                .map(|item| item.text.as_str())
                .eq(["首轮", "续接"])
        );
    }

    #[tokio::test]
    async fn native_worktree_resume_rejects_linked_manifest_and_replaced_directory() {
        let fixture = Fixture::new();
        let stub = NativeStub::new(fixture._temp.path());
        let (rpc, workspace) = ready_native_worktree(&fixture, &stub).await;
        let mut events = rpc.state.subscribe_events();
        let session = create_native_session(&rpc.state, &workspace.workspace_id, "codex");
        let session_id = &session.session_id;
        assert!(rpc.submit(session_id, "首轮").await.ok);
        wait_for_native_event(&mut events, |event| {
            matches!(event, DaemonEvent::SessionUpdated(item)
                if item.session_id == *session_id && item.lifecycle == TurnLifecycleState::Idle)
        })
        .await;
        let before = rpc
            .state
            .list_session_messages(session_id)
            .expect("读取首轮消息");
        assert_eq!(before.len(), 2);

        let manifest = fixture.target.join(WORKTREE_CONTEXT_FILE);
        fs::remove_file(&manifest).expect("移除临时清单");
        std::os::unix::fs::symlink("missing.json", manifest).expect("写入悬空清单链接");
        assert_rpc_error(
            rpc.submit(session_id, "续接").await,
            ErrorCode::WorkspaceDamaged,
        );

        replace_worktree_directory(&fixture.target);
        assert_rpc_error(
            rpc.submit(session_id, "续接").await,
            ErrorCode::WorkspaceDamaged,
        );
        assert_eq!(stub.invocations().len(), 1);
        assert_eq!(
            rpc.state
                .list_session_messages(session_id)
                .expect("复查会话消息"),
            before
        );
    }

    #[tokio::test]
    async fn quick_chat_only_drivers_remain_rejected_for_ready_worktrees() {
        let fixture = Fixture::new();
        let stub = NativeStub::new(fixture._temp.path());
        let (rpc, workspace) = ready_native_worktree(&fixture, &stub).await;
        assert_quick_chat_drivers_rejected(&rpc, &workspace.workspace_id).await;
        assert!(stub.invocations().is_empty());
    }

    #[tokio::test]
    async fn queued_worktree_turn_is_preserved_when_directory_fails_before_fifo_start() {
        let fixture = Fixture::new();
        let stub = NativeStub::new(fixture._temp.path());
        let (rpc, workspace) = ready_native_worktree(&fixture, &stub).await;
        let mut events = rpc.state.subscribe_events();
        let session = create_native_session(&rpc.state, &workspace.workspace_id, "codex");
        let session_id = &session.session_id;
        assert!(rpc.submit(session_id, "hold").await.ok);
        tokio::time::timeout(Duration::from_secs(10), async {
            while stub.invocations().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("首轮桩进程应进入等待状态");

        let Some(RpcResultData::TurnSubmitted {
            queued_id: Some(queue_id),
            ..
        }) = rpc.submit(session_id, "FIFO 队首").await.result
        else {
            panic!("第二条输入应进入 FIFO 队列");
        };
        replace_worktree_directory(&fixture.target);
        fs::write(&stub.release, b"finish").expect("结束首轮桩进程");
        wait_for_native_event(&mut events, |event| {
            matches!(event, DaemonEvent::ErrorRaised { session_id: Some(id), error }
                if id == session_id && error.code == ErrorCode::WorkspaceDamaged)
        })
        .await;

        let messages = rpc
            .state
            .list_session_messages(session_id)
            .expect("读取 FIFO 复核后的消息");
        assert_eq!(messages.len(), 2);
        assert!(messages.iter().all(|item| item.text == "hold"));
        assert_eq!(stub.invocations().len(), 1);
        assert!(
            rpc.call(RpcCommand::CancelQueuedTurn {
                session_id: session_id.clone(),
                queue_id,
            })
            .await
            .ok,
            "目录复核失败后 FIFO 队首仍可撤回"
        );
    }

    fn replace_worktree_directory(target: &Path) {
        fs::rename(target, target.with_extension("original")).expect("移走原工作树夹具");
        fs::create_dir(target).expect("以新目录替换工作树夹具");
    }

    async fn wait_for_native_event(
        events: &mut tokio::sync::broadcast::Receiver<EventEnvelope<DaemonEvent>>,
        accept: impl Fn(&DaemonEvent) -> bool,
    ) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let event = events.recv().await.expect("接收原生回合事件");
                if accept(&event.payload) {
                    break;
                }
            }
        })
        .await
        .expect("原生回合应在期限内发布预期事件");
    }

    #[tokio::test]
    async fn verified_checkout_admission_and_context_retry_preserve_git() {
        let fixture = Fixture::new();
        let state = daemon(&fixture.repository);
        let registration = fixture.register().await;
        let request = fixture.request(EnvironmentMode::MagpieManaged);
        assert!(
            state
                .admit_worktree(&fixture.git, &request, workspace())
                .await
                .is_err()
        );
        assert!(
            state
                .snapshot()
                .expect("读取工作区快照")
                .workspaces
                .is_empty()
        );
        let manifest = fixture.target.join(WORKTREE_CONTEXT_FILE);
        assert!(!manifest.exists());

        assert_eq!(
            fixture
                .git
                .checkout_registered_worktree(&fixture.coordinator(), &registration)
                .await
                .expect("执行受控检出"),
            CheckoutStatus::Verified
        );
        assert_eq!(
            fs::read(fixture.target.join("story.txt")).expect("读取夹具文件或目录"),
            b"line\r\n"
        );
        fs::write(&manifest, b"owned by repository user").expect("创建夹具文件或目录");
        let WorktreeAdmission::ContextFailed {
            workspace,
            error: WorktreeContextError::Conflict,
        } = state
            .admit_worktree(&fixture.git, &request, workspace())
            .await
            .expect("执行 Worktree 接入")
        else {
            panic!("路由冲突应单独返回");
        };
        assert!(workspace.needs_recovery);
        assert_eq!(
            fs::read(&manifest).expect("读取夹具文件或目录"),
            b"owned by repository user"
        );
        assert_eq!(
            create_session(&state, &workspace.workspace_id)
                .error
                .expect("读取夹具文件或目录")
                .code,
            ErrorCode::WorkspaceDamaged
        );

        fs::remove_file(&manifest).expect("更新夹具文件");
        let file = fixture.target.join("story.txt");
        fs::write(&file, b"changes\r\n").expect("创建夹具文件或目录");
        assert!(
            state
                .retry_worktree_context(&fixture.git, &request)
                .await
                .is_err()
        );
        assert!(!manifest.exists());
        fs::write(&file, b"line\r\n").expect("创建夹具文件或目录");
        let WorktreeAdmission::Ready {
            workspace,
            context: WorktreeContext::Prepared { .. },
        } = state
            .retry_worktree_context(&fixture.git, &request)
            .await
            .expect("执行工作区上下文重试")
        else {
            panic!("已恢复固定提交内容后应能单独重试路由");
        };
        assert_eq!(
            workspace.isolation,
            WorkspaceIsolation::Worktree {
                branch: "fixture".to_owned(),
                base_ref: registration.intent.base_oid
            }
        );
        assert!(!workspace.needs_recovery);
        assert!(create_session(&state, &workspace.workspace_id).ok);
        assert!(manifest.is_file());
        assert_eq!(
            fixture
                .git
                .checkout_status(&fixture.coordinator(), "fixture")
                .expect("读取检出状态"),
            CheckoutStatus::Verified
        );
        assert_eq!(
            state.snapshot().expect("读取工作区快照").workspaces.len(),
            1
        );
    }

    #[tokio::test]
    async fn failed_checkout_cannot_be_admitted_in_either_mode() {
        let fixture = Fixture::new();
        std::os::unix::fs::symlink("story.txt", fixture.repository.join("link.txt"))
            .expect("夹具操作应成功");
        let registration = fixture.register().await;
        assert!(
            fixture
                .git
                .checkout_registered_worktree(&fixture.coordinator(), &registration)
                .await
                .is_err()
        );
        assert!(matches!(
            fixture
                .git
                .checkout_status(&fixture.coordinator(), "fixture")
                .expect("读取检出状态"),
            CheckoutStatus::Disabled { .. }
        ));
        let state = daemon(&fixture.repository);
        for mode in [
            EnvironmentMode::StandaloneNative,
            EnvironmentMode::MagpieManaged,
        ] {
            assert!(
                state
                    .admit_worktree(&fixture.git, &fixture.request(mode), workspace())
                    .await
                    .is_err()
            );
        }
        assert!(
            state
                .snapshot()
                .expect("读取工作区快照")
                .workspaces
                .is_empty()
        );
        assert!(!fixture.target.join(WORKTREE_CONTEXT_FILE).exists());
    }
}
