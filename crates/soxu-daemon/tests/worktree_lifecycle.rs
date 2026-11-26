use soxu_daemon::{DaemonState, state::WorktreeAdmissionRequest};
use soxu_git::GitProbe;
use soxu_magpie::{EnvironmentMode, WORKTREE_CONTEXT_FILE};
use soxu_protocol::{
    ErrorCode, ProjectSummary, RpcCommand, RpcRequest, RpcResponse, SessionTarget, WorkspaceBucket,
    WorkspaceIsolation, WorkspaceSummary,
};
use std::{fs, path::Path};

fn daemon(repository: &Path) -> DaemonState {
    let state = DaemonState::open_with_environment(
        None,
        "http://localhost",
        false,
        EnvironmentMode::StandaloneNative,
    )
    .expect("夹具操作应成功");
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

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use soxu_daemon::state::WorktreeAdmission;
    use soxu_git::{
        CheckoutStatus, LocalProject, RegistrationOutcome, VerifiedRegistration,
        WorktreeRegistrationCoordinator, WorktreeRegistrationRequest,
    };
    use soxu_magpie::{WorktreeContext, WorktreeContextError};
    use std::{os::unix::fs::DirBuilderExt, path::PathBuf, process::Command};

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
