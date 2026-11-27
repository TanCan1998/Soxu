//! M3 显式受控仓库创建（Init / 本地源 Clone）的专项验收。
//!
//! 全部夹具都建在系统临时目录下的独立子目录内，不访问外网、不修改真实用户配置，也不留下孤儿进程。

use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use soxu_git::{
    GitProbe, InitMode, RepositoryCreateCoordinator, RepositoryCreateError,
    RepositoryCreateRequest, RepositorySource,
};
#[cfg(target_os = "linux")]
use soxu_git::{
    RepositoryCreateOutcome, RepositoryCreateResult, RepositoryCreateStage, RepositoryOperation,
    RepositoryTargetState, RepositoryTerminalState,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("soxu-rc-{tag}-{}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).expect("夹具临时根");
        // 生产路径校验拒绝含链接的祖先，Unix 上夹具根须取自规范化后的真实路径。
        #[cfg(unix)]
        let root = root.canonicalize().expect("规范化夹具根");
        fs::create_dir_all(root.join("parent")).expect("受控父根");
        fs::create_dir_all(root.join("state")).expect("协调域");
        private_directory(&root.join("state"));
        private_directory(&root.join("parent"));
        Self { root }
    }

    fn parent(&self) -> PathBuf {
        self.root.join("parent")
    }

    fn state(&self) -> PathBuf {
        self.root.join("state")
    }

    #[cfg(target_os = "linux")]
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(unix)]
fn private_directory(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).expect("协调域元数据").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).expect("协调域权限");
}

#[cfg(not(unix))]
fn private_directory(_path: &Path) {}

fn probe() -> GitProbe {
    GitProbe::discover().expect("需要本机 Git 原生入口")
}

#[cfg(target_os = "linux")]
fn git(directory: &Path, arguments: &[&str]) -> Output {
    let program = probe().program().to_owned();
    let mut command = Command::new(program);
    command
        .current_dir(directory)
        .args([
            "-c",
            "user.name=Soxu Test",
            "-c",
            "user.email=soxu-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command.output().expect("运行夹具 Git 命令");
    assert!(
        output.status.success(),
        "夹具 Git 命令失败：{arguments:?} {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[cfg(target_os = "linux")]
fn git_stdout(directory: &Path, arguments: &[&str]) -> String {
    String::from_utf8_lossy(&git(directory, arguments).stdout)
        .trim()
        .to_owned()
}

/// 建立一个含单个提交的本地普通仓库。
#[cfg(target_os = "linux")]
fn source_repository(fixture: &Fixture, name: &str) -> PathBuf {
    source_repository_at(&fixture.path(name))
}

#[cfg(target_os = "linux")]
fn source_repository_at(source: &Path) -> PathBuf {
    let source = source.to_owned();
    fs::create_dir_all(&source).expect("源仓库目录");
    git(&source, &["init", "--initial-branch=main"]);
    fs::write(source.join("README.md"), b"# source\n").expect("源文件");
    git(&source, &["add", "README.md"]);
    git(&source, &["commit", "-m", "初始提交"]);
    source
}

#[cfg(target_os = "linux")]
fn create_directory_link(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        false
    }
}

fn init_request<'a>(id: &'a str, root: &'a Path, name: &'a str) -> RepositoryCreateRequest<'a> {
    RepositoryCreateRequest {
        request_id: id,
        target_root: root,
        target_name: name,
        source: RepositorySource::Init {
            mode: InitMode::MissingTarget,
        },
    }
}

#[cfg(target_os = "linux")]
fn empty_directory_request<'a>(
    id: &'a str,
    root: &'a Path,
    name: &'a str,
) -> RepositoryCreateRequest<'a> {
    RepositoryCreateRequest {
        request_id: id,
        target_root: root,
        target_name: name,
        source: RepositorySource::Init {
            mode: InitMode::VerifiedEmptyDirectory,
        },
    }
}

#[cfg(target_os = "linux")]
fn clone_request<'a>(
    id: &'a str,
    root: &'a Path,
    name: &'a str,
    source_root: &'a Path,
) -> RepositoryCreateRequest<'a> {
    RepositoryCreateRequest {
        request_id: id,
        target_root: root,
        target_name: name,
        source: RepositorySource::CloneLocal { source_root },
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn init_creates_target_with_unborn_main_branch_and_no_hooks() {
    let fixture = Fixture::new("init-missing");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let outcome = probe()
        .create_repository(
            &coordinator,
            init_request("rc-init-1", &fixture.parent(), "alpha"),
        )
        .await
        .expect("受控 Init");
    let RepositoryCreateOutcome::Verified {
        record,
        observation,
    } = outcome
    else {
        panic!("初始化应得到 Verified：{outcome:?}");
    };
    assert_eq!(record.operation, RepositoryOperation::Init);
    assert_eq!(record.stage, RepositoryCreateStage::Observed);
    assert_eq!(record.result, RepositoryCreateResult::Succeeded);
    assert_eq!(record.terminal, Some(RepositoryTerminalState::Verified));
    assert!(record.target_absent_before);
    assert!(record.started_at.is_some() && record.observed_at.is_some());
    assert!(observation.git_dir_present);
    assert_eq!(observation.head_branch.as_deref(), Some("refs/heads/main"));
    assert_eq!(observation.head_oid, None);
    let git_dir = fixture.parent().join("alpha").join(".git");
    assert!(git_dir.is_dir());
    let config = fs::read_to_string(git_dir.join("config")).expect("仓库配置");
    assert!(!config.to_lowercase().contains("hookspath"));
    assert!(!config.to_lowercase().contains("templatedir"));
    let hooks = git_dir.join("hooks");
    if hooks.exists() {
        assert_eq!(
            fs::read_dir(&hooks).expect("钩子目录").count(),
            0,
            "不应出现任何钩子"
        );
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn init_accepts_explicit_verified_empty_directory() {
    let fixture = Fixture::new("init-empty");
    let target = fixture.parent().join("beta");
    fs::create_dir(&target).expect("用户预建空目录");
    private_directory(&target);
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let outcome = probe()
        .create_repository(
            &coordinator,
            empty_directory_request("rc-empty-1", &fixture.parent(), "beta"),
        )
        .await
        .expect("显式空目录 Init");
    let RepositoryCreateOutcome::Verified { record, .. } = outcome else {
        panic!("显式空目录应得到 Verified：{outcome:?}");
    };
    assert!(record.target_empty_before);
    assert!(!record.target_absent_before);
    assert!(target.join(".git").is_dir());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn init_rejects_default_mode_when_target_directory_exists() {
    let fixture = Fixture::new("init-exists");
    let target = fixture.parent().join("gamma");
    fs::create_dir(&target).expect("已存在目录");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let error = probe()
        .create_repository(
            &coordinator,
            init_request("rc-exists-1", &fixture.parent(), "gamma"),
        )
        .await
        .expect_err("默认模式不接受已存在目录");
    assert!(matches!(error, RepositoryCreateError::TargetExists));
    assert!(target.is_dir(), "失败不得删除用户目录");
    assert!(!target.join(".git").exists());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn init_rejects_non_empty_or_hidden_content_and_preserves_it() {
    let fixture = Fixture::new("init-nonempty");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    for (name, id) in [("delta", "rc-nonempty-1"), ("epsilon", "rc-nonempty-2")] {
        let target = fixture.parent().join(name);
        fs::create_dir(&target).expect("用户预建目录");
        let marker = if name == "delta" {
            target.join("notes.txt")
        } else {
            target.join(".hidden")
        };
        fs::write(&marker, b"keep me").expect("写入内容");
        let error = probe()
            .create_repository(
                &coordinator,
                empty_directory_request(id, &fixture.parent(), name),
            )
            .await
            .expect_err("非空或含隐藏项的目录必须拒绝");
        assert!(matches!(error, RepositoryCreateError::TargetExists));
        assert_eq!(fs::read(&marker).expect("保留内容"), b"keep me");
        assert!(!target.join(".git").exists());
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn init_rejects_existing_repository_and_ancestor_repository() {
    let fixture = Fixture::new("init-repo");
    let existing = source_repository_at(&fixture.parent().join("existing"));
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let error = probe()
        .create_repository(
            &coordinator,
            empty_directory_request("rc-repo-1", &fixture.parent(), "existing"),
        )
        .await
        .expect_err("已存在仓库的目标必须拒绝");
    assert!(matches!(
        error,
        RepositoryCreateError::TargetExists | RepositoryCreateError::Unauthorized(_)
    ));
    assert!(existing.join(".git").is_dir());

    let outer = fixture.path("outer");
    fs::create_dir_all(&outer).expect("外层目录");
    fs::create_dir(outer.join(".git")).expect("外层 .git");
    let nested_root = outer.join("inner");
    fs::create_dir(&nested_root).expect("嵌套父根");
    private_directory(&nested_root);
    let error = probe()
        .create_repository(
            &coordinator,
            init_request("rc-repo-2", &nested_root, "zeta"),
        )
        .await
        .expect_err("祖先仓库覆盖必须拒绝");
    assert!(matches!(error, RepositoryCreateError::Unauthorized(_)));
    assert!(!nested_root.join("zeta").exists());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn init_rejects_directory_link_or_reparse_target() {
    let fixture = Fixture::new("init-link");
    let real = fixture.path("real");
    fs::create_dir_all(&real).expect("真实目录");
    let link = fixture.parent().join("linked");
    if !create_directory_link(&real, &link) {
        return;
    }
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let error = probe()
        .create_repository(
            &coordinator,
            empty_directory_request("rc-link-1", &fixture.parent(), "linked"),
        )
        .await
        .expect_err("链接目标必须拒绝");
    assert!(matches!(error, RepositoryCreateError::Unauthorized(_)));
    assert!(!real.join(".git").exists());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn init_rejects_bare_and_gitfile_targets() {
    let fixture = Fixture::new("init-bare");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let bare = fixture.parent().join("bare-target");
    fs::create_dir_all(&bare).expect("bare 目录");
    git(&bare, &["init", "--bare"]);
    let error = probe()
        .create_repository(
            &coordinator,
            empty_directory_request("rc-bare-1", &fixture.parent(), "bare-target"),
        )
        .await
        .expect_err("bare 结构必须拒绝");
    assert!(matches!(error, RepositoryCreateError::TargetExists));

    let gitfile = fixture.parent().join("gitfile-target");
    fs::create_dir_all(&gitfile).expect("gitfile 目录");
    fs::write(gitfile.join(".git"), b"gitdir: ../../nowhere\n").expect("损坏 gitfile");
    let error = probe()
        .create_repository(
            &coordinator,
            empty_directory_request("rc-bare-2", &fixture.parent(), "gitfile-target"),
        )
        .await
        .expect_err("含 gitfile 的目录必须拒绝");
    assert!(matches!(error, RepositoryCreateError::TargetExists));
    assert!(gitfile.join(".git").is_file());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_copies_objects_and_publishes_worktree() {
    let fixture = Fixture::new("clone-ok");
    let source = source_repository(&fixture, "src");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let outcome = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-clone-1", &fixture.parent(), "cloned", &source),
        )
        .await
        .expect("受控 Clone");
    let RepositoryCreateOutcome::Verified {
        record,
        observation,
    } = outcome
    else {
        panic!("本地源克隆应得到 Verified：{outcome:?}");
    };
    assert_eq!(record.operation, RepositoryOperation::CloneLocal);
    assert_eq!(observation.published_entries, 1);
    assert!(observation.head_oid.is_some());
    let cloned = fixture.parent().join("cloned");
    assert_eq!(
        fs::read(cloned.join("README.md")).expect("发布文件"),
        b"# source\n"
    );
    let alternates = cloned
        .join(".git")
        .join("objects")
        .join("info")
        .join("alternates");
    assert!(!alternates.exists(), "不得建立外部对象库引用");
    assert!(!cloned.join(".git").join("shallow").exists());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_rejects_existing_empty_target_and_overlap() {
    let fixture = Fixture::new("clone-target");
    let source = source_repository(&fixture, "src2");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let occupied = fixture.parent().join("occupied");
    fs::create_dir(&occupied).expect("已存在空目录");
    let error = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-clone-2", &fixture.parent(), "occupied", &source),
        )
        .await
        .expect_err("已存在空目录按占用处理");
    assert!(matches!(error, RepositoryCreateError::TargetExists));
    assert_eq!(fs::read_dir(&occupied).expect("目录可读").count(), 0);

    let nested_source = fixture.path("nested-source");
    fs::create_dir_all(nested_source.join("inner")).expect("嵌套目录");
    private_directory(&nested_source.join("inner"));
    git(&nested_source, &["init", "--initial-branch=main"]);
    let error = probe()
        .create_repository(
            &coordinator,
            clone_request(
                "rc-clone-3",
                &nested_source.join("inner"),
                "overlapping",
                &nested_source,
            ),
        )
        .await
        .expect_err("源与目标重叠必须拒绝");
    assert!(matches!(
        error,
        RepositoryCreateError::InvalidInput(_) | RepositoryCreateError::Unauthorized(_)
    ));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_rejects_unsupported_url_and_non_local_sources() {
    let fixture = Fixture::new("clone-protocol");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let request = RepositoryCreateRequest {
        request_id: "rc-clone-url",
        target_root: &fixture.parent(),
        target_name: "remote",
        source: RepositorySource::CloneUrl {
            url: "https://example.invalid/repo.git",
        },
    };
    let error = probe()
        .create_repository(&coordinator, request)
        .await
        .expect_err("受控 HTTPS 克隆尚未验收");
    assert!(matches!(error, RepositoryCreateError::Unsupported(_)));
    assert!(!fixture.parent().join("remote").exists());

    for (index, source_text) in ["ext::echo", "file:///tmp/repo", "git@host:repo.git"]
        .into_iter()
        .enumerate()
    {
        let id = format!("rc-clone-scheme-{index}");
        let source = PathBuf::from(source_text);
        let request = RepositoryCreateRequest {
            request_id: &id,
            target_root: &fixture.parent(),
            target_name: "scheme",
            source: RepositorySource::CloneLocal {
                source_root: &source,
            },
        };
        let error = probe()
            .create_repository(&coordinator, request)
            .await
            .expect_err("非法或未开放协议必须拒绝");
        assert!(matches!(
            error,
            RepositoryCreateError::UnsupportedSource(_) | RepositoryCreateError::Unauthorized(_)
        ));
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_reports_empty_remote_without_publishing() {
    let fixture = Fixture::new("clone-empty");
    let source = fixture.path("empty-src");
    fs::create_dir_all(&source).expect("空源仓库");
    git(&source, &["init", "--initial-branch=main"]);
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let outcome = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-clone-empty", &fixture.parent(), "empty-clone", &source),
        )
        .await
        .expect("空远端克隆");
    let RepositoryCreateOutcome::Verified { observation, .. } = outcome else {
        panic!("空远端应得到 Verified：{outcome:?}");
    };
    assert!(observation.git_dir_present);
    assert_eq!(observation.head_oid, None);
    assert_eq!(observation.published_entries, 0);
    assert!(observation.notes.iter().any(|note| note.contains("空仓库")));
    assert!(
        !fixture
            .parent()
            .join("empty-clone")
            .join("README.md")
            .exists()
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_refuses_symlink_and_gitlink_entries() {
    let fixture = Fixture::new("clone-unsafe");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    for (name, mode, id) in [
        ("symlink-src", "120000", "rc-clone-symlink"),
        ("gitlink-src", "160000", "rc-clone-gitlink"),
    ] {
        let source = fixture.path(name);
        fs::create_dir_all(&source).expect("源目录");
        git(&source, &["init", "--initial-branch=main"]);
        fs::write(source.join("placeholder"), b"x").expect("占位文件");
        let blob = git_stdout(&source, &["hash-object", "-w", "placeholder"]);
        let entry = if mode == "120000" {
            format!("{mode},{blob},unsafe-entry")
        } else {
            format!("{mode},{blob},nested-repo")
        };
        git(&source, &["update-index", "--add", "--cacheinfo", &entry]);
        let tree = git_stdout(&source, &["write-tree"]);
        let commit = git_stdout(&source, &["commit-tree", &tree, "-m", name]);
        git(&source, &["update-ref", "refs/heads/main", &commit]);
        git(&source, &["symbolic-ref", "HEAD", "refs/heads/main"]);

        let target_name = format!("blocked-{mode}");
        let outcome = probe()
            .create_repository(
                &coordinator,
                clone_request(id, &fixture.parent(), &target_name, &source),
            )
            .await
            .expect("受控 Clone 返回留痕而不是静默成功");
        let RepositoryCreateOutcome::UnknownOutcome { record, .. } = outcome else {
            panic!("不安全提交树必须阻断发布：{outcome:?}");
        };
        assert_eq!(record.terminal, Some(RepositoryTerminalState::Unknown));
        let blocked = fixture.parent().join(&target_name);
        assert!(!blocked.exists(), "预检失败不得在目标留下任何内容");
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_refuses_source_with_external_filter_or_alternates() {
    let fixture = Fixture::new("clone-source");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let filtered = source_repository(&fixture, "filtered");
    git(&filtered, &["config", "filter.evil.clean", "echo filtered"]);
    let error = probe()
        .create_repository(
            &coordinator,
            clone_request(
                "rc-clone-filter",
                &fixture.parent(),
                "filtered-clone",
                &filtered,
            ),
        )
        .await
        .expect_err("外部过滤器来源必须拒绝");
    assert!(matches!(error, RepositoryCreateError::UnsupportedSource(_)));

    let alternates = source_repository(&fixture, "alternated");
    let info = alternates.join(".git").join("objects").join("info");
    fs::create_dir_all(&info).expect("objects/info");
    fs::write(info.join("alternates"), b"../elsewhere\n").expect("alternates");
    let error = probe()
        .create_repository(
            &coordinator,
            clone_request(
                "rc-clone-alternates",
                &fixture.parent(),
                "alternated-clone",
                &alternates,
            ),
        )
        .await
        .expect_err("外部对象库来源必须拒绝");
    assert!(matches!(error, RepositoryCreateError::UnsupportedSource(_)));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn duplicate_request_id_with_different_arguments_is_rejected() {
    let fixture = Fixture::new("dup-id");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    probe()
        .create_repository(
            &coordinator,
            init_request("rc-dup-1", &fixture.parent(), "first"),
        )
        .await
        .expect("首次 Init");
    let error = probe()
        .create_repository(
            &coordinator,
            init_request("rc-dup-1", &fixture.parent(), "second"),
        )
        .await
        .expect_err("同 ID 变参必须拒绝");
    assert!(matches!(error, RepositoryCreateError::RequestConflict));
    assert!(!fixture.parent().join("second").exists());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn repeated_request_id_returns_existing_record() {
    let fixture = Fixture::new("replay");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let parent = fixture.parent();
    let request = || init_request("rc-replay-1", &parent, "replayed");
    let first = probe()
        .create_repository(&coordinator, request())
        .await
        .expect("首次 Init");
    assert!(matches!(first, RepositoryCreateOutcome::Verified { .. }));
    let second = probe()
        .create_repository(&coordinator, request())
        .await
        .expect("同参重放只读回查");
    let RepositoryCreateOutcome::ExistingRecord {
        record,
        observation,
    } = second
    else {
        panic!("同参重放应得到 ExistingRecord：{second:?}");
    };
    assert_eq!(record.stage, RepositoryCreateStage::Observed);
    assert!(observation.is_some_and(|observation| observation.git_dir_present));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn failed_command_leaves_started_evidence_and_diagnose_is_read_only() {
    let Some(stand_in) = stand_in_failing_program() else {
        return;
    };
    let fixture = Fixture::new("failure");
    let program = GitProbe::new(&stand_in).expect("替身程序必须被接受为原生入口");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let outcome = program
        .create_repository(
            &coordinator,
            init_request("rc-failure-1", &fixture.parent(), "never-created"),
        )
        .await
        .expect("命令失败必须留痕而不是静默返回错误");
    let RepositoryCreateOutcome::PartialObserved {
        record,
        command_succeeded,
        ..
    } = outcome
    else {
        panic!("命令已启动并退出但无残留时不得声称 NotStarted：{outcome:?}");
    };
    assert!(!command_succeeded);
    assert_eq!(record.stage, RepositoryCreateStage::Observed);
    assert!(matches!(
        record.result,
        RepositoryCreateResult::Failed { .. }
    ));
    assert_eq!(
        record.terminal,
        Some(RepositoryTerminalState::PartialObserved),
        "只有确证命令未启动才允许 NotStarted"
    );
    assert!(record.started_at.is_some());
    assert!(!fixture.parent().join("never-created").exists());

    let diagnosed = probe()
        .diagnose_repository_create(fixture.state(), "rc-failure-1")
        .await
        .expect("只读诊断")
        .expect("应存在留痕");
    assert_eq!(diagnosed, record);
    let again = probe()
        .diagnose_repository_create(fixture.state(), "rc-failure-1")
        .await
        .expect("只读诊断")
        .expect("应存在留痕");
    assert_eq!(again, record);
    assert!(!fixture.parent().join("never-created").exists());

    let missing = probe()
        .diagnose_repository_create(fixture.state(), "rc-failure-unknown")
        .await
        .expect("只读诊断");
    assert!(missing.is_none());
}

#[cfg(target_os = "linux")]
fn stand_in_failing_program() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let system_root = std::env::var_os("SystemRoot")?;
        let candidate = PathBuf::from(system_root)
            .join("System32")
            .join("where.exe");
        candidate.is_file().then_some(candidate)
    }
    #[cfg(target_os = "linux")]
    {
        let candidate = PathBuf::from("/bin/false");
        candidate.is_file().then_some(candidate)
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn init_rejects_invalid_request_shapes() {
    let fixture = Fixture::new("shapes");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    for (id, name) in [
        ("", "alpha"),
        (".hidden-id", "alpha"),
        ("rc-shape-1", ""),
        ("rc-shape-2", ".."),
        ("rc-shape-3", ".git"),
        ("rc-shape-4", "has space"),
    ] {
        let error = probe()
            .create_repository(&coordinator, init_request(id, &fixture.parent(), name))
            .await
            .expect_err("非法请求字段必须拒绝");
        assert!(matches!(error, RepositoryCreateError::InvalidInput(_)));
    }
    let target_state = fs::read_dir(fixture.parent()).expect("父根可读").count();
    assert_eq!(target_state, 0, "非法请求不得留下任何目标");
    let _ = RepositoryTargetState::Missing;
}

#[cfg(target_os = "linux")]
#[test]
fn child_init_with_poisoned_environment() {
    if std::env::var("SOXU_RC_CHILD").is_err() {
        return;
    }
    let root = PathBuf::from(std::env::var("SOXU_RC_TARGET_ROOT").expect("子进程父根"));
    let state = PathBuf::from(std::env::var("SOXU_RC_STATE").expect("子进程协调域"));
    let marker = PathBuf::from(std::env::var("SOXU_RC_MARKER").expect("模板钩子标记"));
    let runtime = tokio::runtime::Runtime::new().expect("子进程运行时");
    let outcome = runtime.block_on(async {
        let coordinator = RepositoryCreateCoordinator::open(&state).expect("子进程协调域");
        probe()
            .create_repository(&coordinator, init_request("rc-child-1", &root, "templated"))
            .await
    });
    let RepositoryCreateOutcome::Verified { observation, .. } = outcome.expect("子进程 Init")
    else {
        panic!("子进程 Init 应得到 Verified");
    };
    assert_eq!(observation.head_branch.as_deref(), Some("refs/heads/main"));
    assert!(!marker.exists(), "外部模板钩子不得被执行或复制");
    let hooks = root.join("templated").join(".git").join("hooks");
    if hooks.exists() {
        assert_eq!(fs::read_dir(&hooks).expect("钩子目录").count(), 0);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn init_ignores_inherited_git_environment_and_external_template() {
    let fixture = Fixture::new("poison");
    let template = fixture.path("template");
    fs::create_dir_all(template.join("hooks")).expect("模板钩子目录");
    let marker = fixture.path("hook-fired");
    let hook = template.join("hooks").join("post-checkout");
    let script = format!(
        "#!/bin/sh\ntouch '{}'\n",
        marker.to_string_lossy().replace('\'', "")
    );
    fs::write(&hook, script).expect("模板钩子");
    let global = fixture.path("global-config");
    fs::write(
        &global,
        format!(
            "[init]\n\ttemplateDir = {}\n",
            template.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("全局配置");

    let executable = std::env::current_exe().expect("当前测试可执行文件");
    let status = Command::new(executable)
        .args([
            "--exact",
            "child_init_with_poisoned_environment",
            "--test-threads=1",
        ])
        .env("SOXU_RC_CHILD", "1")
        .env("SOXU_RC_TARGET_ROOT", fixture.parent())
        .env("SOXU_RC_STATE", fixture.state())
        .env("SOXU_RC_MARKER", &marker)
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "init.templateDir")
        .env("GIT_CONFIG_VALUE_0", &template)
        .env("GIT_DIR", fixture.path("poisoned-git-dir"))
        .env("GIT_WORK_TREE", fixture.parent())
        .env("GIT_TEMPLATE_DIR", &template)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("运行子进程");
    assert!(status.success(), "子进程必须在预置 Git 环境下仍然成功");
    assert!(!marker.exists(), "外部模板钩子不得被执行");
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
#[tokio::test]
async fn non_private_parent_root_is_rejected() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("private-root");
    let parent = fixture.parent();
    let mut permissions = fs::metadata(&parent).expect("父根元数据").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&parent, permissions).expect("放宽父根权限");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let error = probe()
        .create_repository(
            &coordinator,
            init_request("rc-private-1", &parent, "rejected"),
        )
        .await
        .expect_err("非私有父根必须 fail-closed");
    assert!(matches!(error, RepositoryCreateError::Unauthorized(_)));
    assert!(!parent.join("rejected").exists());
}

#[cfg(target_os = "linux")]
fn fnv(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(target_os = "linux")]
fn collect_tree(root: &Path, current: &Path, out: &mut Vec<(String, u64, u64)>) {
    let Ok(reader) = fs::read_dir(current) else {
        return;
    };
    for entry in reader.flatten() {
        let path = entry.path();
        let Ok(metadata) = path.symlink_metadata() else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .map(|value| value.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if metadata.is_dir() {
            out.push((rel, 0, 0));
            collect_tree(root, &path, out);
        } else {
            let bytes = fs::read(&path).unwrap_or_default();
            out.push((rel, metadata.len(), fnv(&bytes)));
        }
    }
}

/// 返回目录树快照（相对路径、长度、内容摘要），用于判定一次调用是否零写入。
#[cfg(target_os = "linux")]
fn snapshot_tree(root: &Path) -> Vec<(String, u64, u64)> {
    let mut entries = Vec::new();
    collect_tree(root, root, &mut entries);
    entries.sort();
    entries
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn repeated_request_is_strictly_read_only() {
    let fixture = Fixture::new("replay-readonly");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let parent = fixture.parent();
    probe()
        .create_repository(&coordinator, init_request("rc-ro-1", &parent, "frozen"))
        .await
        .expect("首次 Init");
    let state_before = snapshot_tree(&fixture.state());
    let parent_before = snapshot_tree(&parent);
    let second = probe()
        .create_repository(&coordinator, init_request("rc-ro-1", &parent, "frozen"))
        .await
        .expect("同参重放");
    let RepositoryCreateOutcome::ExistingRecord { observation, .. } = second else {
        panic!("同参重放应得到 ExistingRecord：{second:?}");
    };
    assert!(observation.is_some_and(|value| value.git_dir_present));
    assert_eq!(
        snapshot_tree(&fixture.state()),
        state_before,
        "重复请求不得写协调域"
    );
    assert_eq!(
        snapshot_tree(&parent),
        parent_before,
        "重复请求不得写目标与沙箱之外的任何文件"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn repeated_clone_request_does_not_republish() {
    let fixture = Fixture::new("replay-clone-readonly");
    let source = source_repository(&fixture, "src-ro");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let parent = fixture.parent();
    probe()
        .create_repository(
            &coordinator,
            clone_request("rc-ro-2", &parent, "cloned-ro", &source),
        )
        .await
        .expect("首次 Clone");
    let state_before = snapshot_tree(&fixture.state());
    let parent_before = snapshot_tree(&parent);
    let second = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-ro-2", &parent, "cloned-ro", &source),
        )
        .await
        .expect("同参重放");
    let RepositoryCreateOutcome::ExistingRecord { observation, .. } = second else {
        panic!("同参重放应得到 ExistingRecord：{second:?}");
    };
    assert!(observation.is_some_and(|value| value.git_dir_present));
    assert_eq!(snapshot_tree(&fixture.state()), state_before);
    assert_eq!(
        snapshot_tree(&parent),
        parent_before,
        "重复 Clone 请求不得重新发布工作树"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_publishes_index_matching_head_and_clean_status() {
    let fixture = Fixture::new("clone-index");
    let source = fixture.path("index-src");
    source_repository_at(&source);
    fs::create_dir_all(source.join("nested")).expect("子目录");
    fs::write(source.join("nested").join("deep.txt"), b"deep\n").expect("子文件");
    let script = source.join("run.sh");
    fs::write(&script, b"#!/bin/sh\necho ok\n").expect("脚本");
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "多条目"]);

    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let outcome = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-index-1", &fixture.parent(), "indexed", &source),
        )
        .await
        .expect("受控 Clone");
    assert!(
        matches!(outcome, RepositoryCreateOutcome::Verified { .. }),
        "正常 Clone 应得到 Verified：{outcome:?}"
    );
    let cloned = fixture.parent().join("indexed");
    assert!(
        cloned.join(".git").join("index").is_file(),
        "必须建立 index"
    );
    let head_tree = git_stdout(&cloned, &["ls-tree", "-r", "--full-tree", "HEAD"]);
    let staged = git_stdout(&cloned, &["ls-files", "--stage"]);
    let mut from_tree: Vec<String> = head_tree
        .lines()
        .map(|line| {
            let (head, path) = line.split_once('\t').expect("ls-tree 行");
            let mut parts = head.split(' ');
            let mode = parts.next().expect("mode");
            let _kind = parts.next().expect("kind");
            let oid = parts.next().expect("oid");
            format!("{mode} {oid} 0\t{path}")
        })
        .collect();
    let mut from_index: Vec<String> = staged.lines().map(str::to_owned).collect();
    from_tree.sort();
    from_index.sort();
    assert_eq!(from_index, from_tree, "index 必须与 HEAD 树逐条一致");
    assert!(
        git_stdout(
            &cloned,
            &["status", "--porcelain=v2", "--untracked-files=all"]
        )
        .is_empty(),
        "克隆后 status 必须干净"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn unsafe_tree_is_rejected_before_any_worktree_file_is_published() {
    let fixture = Fixture::new("precheck-publish");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let source = fixture.path("unsafe-src");
    fs::create_dir_all(&source).expect("源目录");
    git(&source, &["init", "--initial-branch=main"]);
    fs::write(source.join("safe.txt"), b"safe").expect("普通文件");
    git(&source, &["add", "safe.txt"]);
    let blob = git_stdout(&source, &["hash-object", "-w", "safe.txt"]);
    git(
        &source,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{blob},link-entry"),
        ],
    );
    let tree = git_stdout(&source, &["write-tree"]);
    let commit = git_stdout(&source, &["commit-tree", &tree, "-m", "含符号链接"]);
    git(&source, &["update-ref", "refs/heads/main", &commit]);

    let outcome = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-precheck-1", &fixture.parent(), "blocked", &source),
        )
        .await
        .expect("预检失败必须留痕");
    assert!(
        matches!(outcome, RepositoryCreateOutcome::UnknownOutcome { .. }),
        "不安全提交树必须阻断：{outcome:?}"
    );
    let blocked = fixture.parent().join("blocked");
    assert!(!blocked.exists(), "预检失败不得在目标留下任何内容");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn unknown_outcome_is_not_replayed() {
    let fixture = Fixture::new("unknown-replay");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let source = fixture.path("unknown-src");
    fs::create_dir_all(&source).expect("源目录");
    git(&source, &["init", "--initial-branch=main"]);
    fs::write(source.join("placeholder"), b"x").expect("占位文件");
    let blob = git_stdout(&source, &["hash-object", "-w", "placeholder"]);
    git(
        &source,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{blob},nested"),
        ],
    );
    let tree = git_stdout(&source, &["write-tree"]);
    let commit = git_stdout(&source, &["commit-tree", &tree, "-m", "gitlink"]);
    git(&source, &["update-ref", "refs/heads/main", &commit]);

    let first = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-unknown-1", &fixture.parent(), "unknown-target", &source),
        )
        .await
        .expect("首次执行");
    assert!(matches!(
        first,
        RepositoryCreateOutcome::UnknownOutcome { .. }
    ));
    let before = snapshot_tree(&fixture.parent());
    let second = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-unknown-1", &fixture.parent(), "unknown-target", &source),
        )
        .await
        .expect("结果不确定的重放必须被拒绝");
    assert!(
        matches!(second, RepositoryCreateOutcome::UnknownOutcome { .. }),
        "不得重放未知结果：{second:?}"
    );
    assert_eq!(
        snapshot_tree(&fixture.parent()),
        before,
        "拒绝重放不得产生新的写入"
    );
}
#[cfg(windows)]
#[tokio::test]
async fn windows_write_is_refused_without_touching_target_or_state() {
    let fixture = Fixture::new("win-refuse");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let error = probe()
        .create_repository(
            &coordinator,
            init_request("rc-win-1", &fixture.parent(), "blocked"),
        )
        .await
        .expect_err("Windows 缺少目录创建与打开的原子锚定，必须 fail-closed");
    assert!(matches!(
        error,
        RepositoryCreateError::UnsupportedPlatform(_)
    ));
    assert!(!fixture.parent().join("blocked").exists(), "不得创建目标");
    assert!(
        !fixture.state().join("sandbox").exists(),
        "不得建立或写入暂存"
    );
    assert!(
        !fixture.state().join("records").exists(),
        "不得写入留痕记录"
    );
    let entries = fs::read_dir(fixture.state()).expect("协调域可读").count();
    assert_eq!(entries, 0, "拒绝必须发生在任何写入之前：{entries} 项残留");
}

#[cfg(windows)]
#[tokio::test]
async fn windows_diagnose_stays_read_only() {
    let fixture = Fixture::new("win-diagnose");
    let record = probe()
        .diagnose_repository_create(fixture.state(), "rc-win-missing")
        .await
        .expect("只读诊断");
    assert!(record.is_none());
    let entries = fs::read_dir(fixture.state()).expect("协调域可读").count();
    assert_eq!(entries, 0, "只读诊断不得写入协调域：{entries} 项残留");
}
#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_rejects_link_inside_source_git_directory() {
    let fixture = Fixture::new("clone-link-inner");
    let source = source_repository(&fixture, "inner-link-src");
    let heads = source.join(".git").join("refs").join("heads");
    let link = heads.join("alias");
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink("main", &link).is_err() {
            return;
        }
    }
    #[cfg(not(unix))]
    {
        if !create_directory_link(&heads.join("main"), &link) {
            return;
        }
    }
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let error = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-inner-link-1", &fixture.parent(), "inner-link", &source),
        )
        .await
        .expect_err("源 .git 内层链接必须拒绝");
    assert!(matches!(error, RepositoryCreateError::UnsupportedSource(_)));
    assert!(!fixture.parent().join("inner-link").exists());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_snapshot_isolates_later_source_changes() {
    let fixture = Fixture::new("snapshot-isolated");
    let source = source_repository(&fixture, "iso-src");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let outcome = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-iso-1", &fixture.parent(), "iso-clone", &source),
        )
        .await
        .expect("受控 Clone");
    let RepositoryCreateOutcome::Verified { observation, .. } = outcome else {
        panic!("应得到 Verified：{outcome:?}");
    };
    let fixed = observation.head_oid.expect("固定 HEAD");

    // 捕获完成后修改源：新增提交并改写其 .git。
    fs::write(source.join("later.txt"), b"later\n").expect("源新文件");
    git(&source, &["add", "later.txt"]);
    git(&source, &["commit", "-m", "后来改动"]);
    assert_ne!(git_stdout(&source, &["rev-parse", "HEAD"]), fixed);

    let snapshot_git = fixture
        .state()
        .join("sandbox")
        .join("rc-iso-1")
        .join("work")
        .join("source");
    assert!(
        snapshot_git.join("HEAD").is_file(),
        "快照必须保留在协调域暂存内"
    );
    let cloned = fixture.parent().join("iso-clone");
    assert_eq!(
        git_stdout(&cloned, &["rev-parse", "HEAD"]),
        fixed,
        "克隆终态必须对照捕获时的固定 HEAD"
    );
    assert!(
        !cloned.join("later.txt").exists(),
        "后续源改动不得进入已发布目标"
    );
}
#[cfg(not(any(target_os = "linux", windows)))]
#[tokio::test]
async fn unsupported_platform_refuses_write_before_touching_state() {
    let fixture = Fixture::new("other-platform");
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let error = probe()
        .create_repository(
            &coordinator,
            init_request("rc-other-1", &fixture.parent(), "blocked"),
        )
        .await
        .expect_err("缺少目录句柄原语的平台必须 fail-closed");
    assert!(matches!(
        error,
        RepositoryCreateError::UnsupportedPlatform(_)
            | RepositoryCreateError::UnsupportedFilesystem(_)
    ));
    assert!(!fixture.parent().join("blocked").exists(), "不得创建目标");
    let entries = fs::read_dir(fixture.state()).expect("协调域可读").count();
    assert_eq!(entries, 0, "拒绝必须发生在任何写入之前：{entries} 项残留");
}
#[cfg(target_os = "linux")]
#[tokio::test]
async fn clone_records_authorized_source_without_temporary_path() {
    let fixture = Fixture::new("origin-url");
    let source = source_repository(&fixture, "origin-src");
    let before = snapshot_tree(&source);
    let coordinator = RepositoryCreateCoordinator::open(fixture.state()).expect("协调域");
    let outcome = probe()
        .create_repository(
            &coordinator,
            clone_request("rc-origin-1", &fixture.parent(), "origin-clone", &source),
        )
        .await
        .expect("受控 Clone");
    assert!(matches!(outcome, RepositoryCreateOutcome::Verified { .. }));
    let cloned = fixture.parent().join("origin-clone");
    let config = fs::read_to_string(cloned.join(".git").join("config")).expect("产物配置");
    let url = config
        .lines()
        .find_map(|line| line.trim().strip_prefix("url = "))
        .map(str::trim)
        .expect("origin url");
    assert!(
        !url.contains("/proc/"),
        "origin 不得保存进程级临时路径：{url}"
    );
    let canonical = fs::canonicalize(&source).expect("源规范路径");
    assert_eq!(
        Path::new(url),
        canonical.as_path(),
        "origin 必须指向已授权本地源"
    );
    assert_eq!(snapshot_tree(&source), before, "克隆不得修改源");
}
