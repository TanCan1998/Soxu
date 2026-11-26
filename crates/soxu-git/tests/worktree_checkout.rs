use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use soxu_git::{GitProbe, WorktreeLifecycleError, WorktreeRegistrationCoordinator};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        loop {
            let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("soxu-checkout-api-{}-{number}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path.canonicalize().expect("规范化夹具路径")),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("无法创建夹具：{error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn coordinator_rejects_relative_path_and_platform_limits_remain_explicit() {
    let fixture = Fixture::new();
    assert!(matches!(
        WorktreeRegistrationCoordinator::open("relative", &[]),
        Err(WorktreeLifecycleError::Unauthorized(_))
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o700))
            .expect("核对夹具操作结果");
    }
    #[cfg(windows)]
    assert!(matches!(
        WorktreeRegistrationCoordinator::open(fixture.path(), &[]),
        Err(WorktreeLifecycleError::UnsupportedFilesystem(_))
    ));
    #[cfg(target_os = "linux")]
    {
        use soxu_git::{CheckoutError, CheckoutStatus};
        let coordinator =
            WorktreeRegistrationCoordinator::open(fixture.path(), &[]).expect("核对夹具操作结果");
        let git = GitProbe::discover().expect("发现本机 Git");
        assert_eq!(
            git.checkout_status(&coordinator, "new")
                .expect("读取持久检出状态"),
            CheckoutStatus::Pending
        );
        assert!(matches!(
            git.checkout_status(&coordinator, "../escape"),
            Err(CheckoutError::Registration(
                WorktreeLifecycleError::InvalidInput(_)
            ))
        ));
    }
    #[cfg(not(target_os = "linux"))]
    let _ = GitProbe::discover().expect("发现本机 Git");
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use soxu_git::{
        CheckoutError, CheckoutStage, CheckoutStatus, LocalProject, RegistrationObservation,
        RegistrationOutcome, TargetState, VerifiedRegistration, WorktreeRegistrationRequest,
    };
    use std::os::unix::fs::{DirBuilderExt, symlink};
    use std::process::{Command, Stdio};

    struct Registered {
        fixture: Fixture,
        coordinator: WorktreeRegistrationCoordinator,
        git: GitProbe,
        registration: VerifiedRegistration,
    }

    impl Registered {
        async fn new() -> Self {
            let fixture = Fixture::new();
            let source = fixture.path().join("source");
            let workspace = fixture.path().join("workspace");
            let state = fixture.path().join("state");
            fs::create_dir(&source).expect("创建夹具路径");
            fs::create_dir(&workspace).expect("创建夹具路径");
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&state)
                .expect("创建夹具路径");
            let git = GitProbe::discover().expect("发现本机 Git");
            let run = |args: &[&str]| {
                let mut command = Command::new("git");
                for (name, _) in std::env::vars_os() {
                    if name
                        .to_string_lossy()
                        .to_ascii_uppercase()
                        .starts_with("GIT_")
                    {
                        command.env_remove(name);
                    }
                }
                let output = command
                    .current_dir(&source)
                    .stdin(Stdio::null())
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_ATTR_NOSYSTEM", "1")
                    .env("GIT_ALLOW_PROTOCOL", "")
                    .args([
                        "-c",
                        "core.hooksPath=",
                        "-c",
                        "core.attributesFile=/dev/null",
                    ])
                    .args(args)
                    .output()
                    .expect("核对夹具操作结果");
                assert!(
                    output.status.success(),
                    "{args:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .expect("读取 Git 夹具输出")
                    .trim()
                    .to_owned()
            };
            run(&["init", "--template="]);
            fs::write(source.join("first.txt"), b"first\n").expect("写入夹具文件");
            fs::create_dir(source.join("nested")).expect("创建夹具路径");
            fs::write(source.join("nested/second.txt"), b"second\n").expect("写入夹具文件");
            run(&["add", "--all"]);
            run(&[
                "-c",
                "user.name=Soxu",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
            ]);
            let oid = run(&["rev-parse", "HEAD"]);
            let LocalProject::GitWorktree(source) = git
                .open_local_project(&source)
                .await
                .expect("核对夹具操作结果")
            else {
                panic!("夹具不是 Git 工作树");
            };
            let coordinator =
                WorktreeRegistrationCoordinator::open(&state, &[]).expect("核对夹具操作结果");
            let registered = git
                .register_new_worktree(
                    &coordinator,
                    WorktreeRegistrationRequest {
                        request_id: "fixture",
                        source: &source,
                        workspace_root: &workspace,
                        target_name: "checked-out",
                        branch_ref: "refs/heads/soxu/fixture",
                        base_oid: &oid,
                    },
                )
                .await
                .expect("核对夹具操作结果");
            let RegistrationOutcome::Verified(registration) = registered else {
                panic!("注册未通过复核：{registered:?}");
            };
            Self {
                fixture,
                coordinator,
                git,
                registration,
            }
        }

        fn target(&self) -> &Path {
            &self.registration.intent.target
        }
        fn admin(&self) -> &Path {
            &self
                .registration
                .observation
                .target_identity
                .as_ref()
                .expect("核对夹具操作结果")
                .git_dir
        }
        fn status(&self) -> CheckoutStatus {
            self.git
                .checkout_status(&self.coordinator, "fixture")
                .expect("读取持久检出状态")
        }
        async fn checkout(&self) -> Result<CheckoutStatus, CheckoutError> {
            self.git
                .checkout_registered_worktree(&self.coordinator, &self.registration)
                .await
        }
        fn state(&self) -> PathBuf {
            self.fixture.path().join("state")
        }
        async fn diagnose(&self) -> RegistrationObservation {
            let intent = &self.registration.intent;
            self.git
                .diagnose_registration_with_target(
                    self.state(),
                    &intent.request_id,
                    &intent.source.root,
                    &intent.target,
                )
                .await
                .expect("复核夹具注册状态")
                .expect("持久意图仍然存在")
        }
        async fn register_again(&self) -> RegistrationOutcome {
            let intent = &self.registration.intent;
            self.git
                .register_new_worktree(
                    &self.coordinator,
                    WorktreeRegistrationRequest {
                        request_id: &intent.request_id,
                        source: &intent.source,
                        workspace_root: intent.target.parent().expect("夹具目标父目录"),
                        target_name: intent
                            .target
                            .file_name()
                            .and_then(std::ffi::OsStr::to_str)
                            .expect("夹具目标名"),
                        branch_ref: &intent.branch_ref,
                        base_oid: &intent.base_oid,
                    },
                )
                .await
                .expect("复核夹具注册状态")
        }
    }

    #[tokio::test]
    async fn reopens_persisted_authorization_and_materializes_exact_commit_once() {
        let registered = Registered::new().await;
        let mut forged = registered.registration.clone();
        forged.intent.base_oid = "0".repeat(40);
        assert!(matches!(
            registered
                .git
                .checkout_registered_worktree(&registered.coordinator, &forged)
                .await,
            Err(CheckoutError::Registration(
                WorktreeLifecycleError::RequestConflict
            ))
        ));
        assert_eq!(registered.status(), CheckoutStatus::Pending);
        assert_eq!(
            registered.checkout().await.expect("执行夹具受控检出"),
            CheckoutStatus::Verified
        );
        assert_eq!(
            fs::read(registered.target().join("first.txt")).expect("读取夹具文件"),
            b"first\n"
        );
        assert_eq!(
            fs::read(registered.target().join("nested/second.txt")).expect("读取夹具文件"),
            b"second\n"
        );
        assert!(registered.admin().join("index").is_file());
        assert_eq!(registered.status(), CheckoutStatus::Verified);
        assert!(matches!(
            registered.checkout().await,
            Err(CheckoutError::AlreadyAttempted(CheckoutStatus::Verified))
        ));
    }

    #[tokio::test]
    async fn refuses_user_file_and_target_path_replacement_before_checkout() {
        let registered = Registered::new().await;
        let user = registered.target().join("first.txt");
        fs::write(&user, b"user content").expect("写入夹具文件");
        assert!(registered.checkout().await.is_err());
        assert_eq!(fs::read(&user).expect("读取夹具文件"), b"user content");
        assert!(!registered.admin().join("index").exists());
        assert!(matches!(
            registered.status(),
            CheckoutStatus::Disabled {
                stage: Some(CheckoutStage::Preflight),
                ..
            }
        ));
        assert!(matches!(
            registered.checkout().await,
            Err(CheckoutError::AlreadyAttempted(_))
        ));

        let replaced = Registered::new().await;
        let moved = replaced.fixture.path().join("moved-target");
        let outside = replaced.fixture.path().join("outside");
        fs::create_dir(&outside).expect("创建夹具路径");
        fs::rename(replaced.target(), &moved).expect("核对夹具操作结果");
        symlink(&outside, replaced.target()).expect("核对夹具操作结果");
        assert!(replaced.checkout().await.is_err());
        assert!(
            fs::read_dir(&outside)
                .expect("枚举夹具目录")
                .next()
                .is_none()
        );
        assert!(!moved.join("first.txt").exists());
        assert!(matches!(
            replaced.status(),
            CheckoutStatus::Disabled {
                stage: Some(CheckoutStage::Preflight),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn rejects_dirty_target_before_checkout() {
        let registered = Registered::new().await;
        fs::write(registered.target().join("first.txt"), b"user content").expect("写入夹具文件");
        let observation = registered.diagnose().await;
        // Git 清单里仍登记着该目标，但未被信任的检出记录不得解析出管理目录。
        assert!(observation.target_identity.is_none());
        assert!(!observation.target_matches);
        assert!(matches!(
            registered.register_again().await,
            RegistrationOutcome::PartialFootprint { .. }
                | RegistrationOutcome::UnknownOutcome { .. }
        ));
        assert_eq!(
            fs::read(registered.target().join("first.txt")).expect("读取夹具文件"),
            b"user content"
        );
    }

    #[tokio::test]
    async fn rejects_target_after_failed_checkout() {
        let registered = Registered::new().await;
        let user = registered.target().join("first.txt");
        fs::write(&user, b"user content").expect("写入夹具文件");
        assert!(registered.checkout().await.is_err());
        assert!(matches!(
            registered.status(),
            CheckoutStatus::Disabled { .. }
        ));
        let observation = registered.diagnose().await;
        assert!(observation.target_identity.is_none());
        assert!(!observation.target_matches);
        assert!(!matches!(
            registered.register_again().await,
            RegistrationOutcome::Verified(_) | RegistrationOutcome::ExistingIntent(_)
        ));
        assert_eq!(fs::read(&user).expect("读取夹具文件"), b"user content");
    }

    #[tokio::test]
    async fn accepts_verified_checkout_and_repeat_registration_is_idempotent() {
        let registered = Registered::new().await;
        assert_eq!(
            registered.checkout().await.expect("执行夹具受控检出"),
            CheckoutStatus::Verified
        );
        assert_eq!(
            fs::read(registered.target().join("first.txt")).expect("读取夹具文件"),
            b"first\n"
        );
        assert!(
            registered.target().join("nested/second.txt").is_file(),
            "真实检出内容必须落在目标目录"
        );

        let observation = registered.diagnose().await;
        assert_eq!(observation.target_state, TargetState::Directory);
        assert!(observation.target_registered);
        assert!(observation.target_matches);
        assert_eq!(
            observation.ref_oid.as_deref(),
            Some(registered.registration.intent.base_oid.as_str())
        );

        let again = registered.register_again().await;
        assert!(
            matches!(again, RegistrationOutcome::ExistingIntent(_)),
            "{again:?}"
        );
        assert_eq!(
            fs::read(registered.target().join("first.txt")).expect("读取夹具文件"),
            b"first\n"
        );
    }
}
