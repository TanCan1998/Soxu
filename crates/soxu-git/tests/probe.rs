use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use soxu_git::{GitError, GitProbe, HeadState, StatusEntry};

struct Fixture {
    root: PathBuf,
    git: GitProbe,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = loop {
            let root = std::env::temp_dir().join(format!(
                "soxu-git-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => break root.canonicalize().expect("夹具绝对路径"),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("创建夹具失败：{error}"),
            }
        };
        Self {
            root,
            git: GitProbe::discover().expect("测试需要原生 Git"),
        }
    }
    fn directory(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(&path).expect("创建夹具子目录");
        path
    }
    /// 夹具 Git 命令的唯一构造点：清除宿主 `GIT_*`、隔离系统与全局配置。
    ///
    /// `maintenance.auto=false` / `gc.auto=0` 与生产只读入口一致：夹具提交若不禁用后台维护，
    /// 会派生 detached 的 `git maintenance run --auto`，它在 `.git/objects` 下增删
    /// `maintenance.lock`，成为整树快照断言的唯一观察差异。
    /// `guard_overrides` 追加在守卫之后，供回归用例显式反向启用。
    fn command(&self, directory: &Path, guard_overrides: &[&str]) -> Command {
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
        command.args(guard_overrides);
        command
    }

    fn run(&self, directory: &Path, arguments: &[&str]) -> Output {
        self.command(directory, &[])
            .args(arguments)
            .output()
            .expect("启动夹具 Git")
    }
    fn git(&self, directory: &Path, arguments: &[&str]) {
        let output = self.run(directory, arguments);
        assert!(
            output.status.success(),
            "夹具 Git {arguments:?} 失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn repo(&self, name: &str) -> PathBuf {
        let directory = self.directory(name);
        self.git(&directory, &["init", "--initial-branch=main"]);
        directory
    }
    fn commit(&self, directory: &Path) {
        self.git(directory, &["add", "--all"]);
        let output = self
            .commit_command(directory, &[], None)
            .output()
            .expect("启动夹具提交");
        assert!(
            output.status.success(),
            "夹具提交失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// 提交流程的唯一命令构造点：`commit` 与有界 trace2 回归共用同一条路径。
    fn commit_command(
        &self,
        directory: &Path,
        guard_overrides: &[&str],
        trace: Option<&Path>,
    ) -> Command {
        let mut command = self.command(directory, guard_overrides);
        if let Some(trace) = trace {
            command.env("GIT_TRACE2_EVENT", trace);
        }
        command.args(["commit", "-m", "fixture"]);
        command
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = std::env::temp_dir().canonicalize().expect("临时目录");
        assert!(
            self.root.starts_with(root)
                && self
                    .root
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("soxu-git-"))
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn file_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)
            .expect("读取夹具")
            .map(|entry| entry.expect("目录项"))
        {
            if entry.file_type().expect("类型").is_dir() {
                directories.push(entry.path());
            } else {
                files.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .expect("夹具路径")
                        .to_path_buf(),
                    fs::read(entry.path()).expect("读取文件"),
                );
            }
        }
    }
    files
}

#[tokio::test]
async fn detects_unborn_attached_and_detached_without_writing() {
    let fixture = Fixture::new();
    let repository = fixture.repo("普通 仓库中文");
    fs::write(repository.join("tracked 中文.txt"), "initial\n").expect("文件");
    let before = file_bytes(&fixture.root);
    let unborn = fixture.git.probe(&repository).await.expect("unborn");
    assert_eq!(
        unborn.head,
        HeadState::Unborn {
            branch: b"main".to_vec()
        }
    );
    assert_eq!(before, file_bytes(&fixture.root));
    fixture.commit(&repository);
    let child = repository.join("nested");
    fs::create_dir(&child).expect("子目录");
    let cwd = std::env::current_dir().expect("cwd");
    let before = file_bytes(&fixture.root);
    let attached = fixture.git.probe(&child).await.expect("attached");
    assert_eq!(attached.root, repository);
    assert_eq!(
        attached.git_dir,
        repository.join(".git").canonicalize().expect("git dir")
    );
    assert_eq!(attached.common_dir, attached.git_dir);
    let HeadState::Attached { branch, oid } = attached.head else {
        panic!("应为 attached");
    };
    assert_eq!(branch, b"main");
    assert_eq!(oid.len(), 40);
    assert!(attached.status.entries.is_empty());
    assert_eq!(cwd, std::env::current_dir().expect("cwd 不变"));
    assert_eq!(before, file_bytes(&fixture.root));
    fixture.git(&repository, &["checkout", "--detach", "HEAD"]);
    let before = file_bytes(&fixture.root);
    assert!(matches!(
        fixture.git.probe(&repository).await.expect("detached").head,
        HeadState::Detached { .. }
    ));
    assert_eq!(before, file_bytes(&fixture.root));
}

#[tokio::test]
async fn reports_staged_unstaged_rename_untracked_and_linked_worktree() {
    let fixture = Fixture::new();
    let repository = fixture.repo("repo");
    fs::write(repository.join("source 中文.txt"), "rename content\n").expect("源文件");
    fs::write(repository.join("modified.txt"), "base\n").expect("文件");
    fixture.commit(&repository);
    fixture.git(
        &repository,
        &["mv", "source 中文.txt", "renamed space 中文.txt"],
    );
    fs::write(repository.join("modified.txt"), "staged\n").expect("暂存变更");
    fixture.git(&repository, &["add", "modified.txt"]);
    fs::write(repository.join("modified.txt"), "unstaged\n").expect("未暂存变更");
    let nested = repository.join("untracked dir");
    fs::create_dir(&nested).expect("目录");
    fs::write(nested.join("new 中文.txt"), "new").expect("未跟踪");
    let before = file_bytes(&fixture.root);
    let snapshot = fixture.git.probe(&repository).await.expect("变更快照");
    assert_eq!(snapshot.status.entries.len(), 3);
    assert!(snapshot.status.entries.iter().any(|entry| matches!(entry, StatusEntry::Renamed(rename) if rename.original_path.as_bytes() == "source 中文.txt".as_bytes())));
    assert!(snapshot.status.entries.iter().any(|entry| matches!(entry, StatusEntry::Ordinary(change) if change.status.index == 'M' && change.status.worktree == 'M')));
    assert!(snapshot.status.entries.iter().any(|entry| matches!(entry, StatusEntry::Untracked(path) if path.as_bytes() == "untracked dir/new 中文.txt".as_bytes())));
    assert_eq!(before, file_bytes(&fixture.root));
    let linked = fixture.root.join("linked 中文");
    fixture.git(
        &repository,
        &["worktree", "add", "-b", "linked", "../linked 中文"],
    );
    let before = file_bytes(&fixture.root);
    let snapshot = fixture.git.probe(&linked).await.expect("linked worktree");
    assert_eq!(snapshot.root, linked.canonicalize().expect("linked root"));
    assert_eq!(
        snapshot.common_dir,
        repository.join(".git").canonicalize().expect("common dir")
    );
    assert_ne!(snapshot.git_dir, snapshot.common_dir);
    assert!(matches!(snapshot.head, HeadState::Attached { branch, .. } if branch == b"linked"));
    assert_eq!(before, file_bytes(&fixture.root));
}

#[tokio::test]
async fn rejects_relative_missing_file_non_repository_bare_and_git_dir() {
    let fixture = Fixture::new();
    assert!(matches!(GitProbe::new("git"), Err(GitError::RelativePath)));
    let script = fixture.root.join("git.cmd");
    fs::write(&script, "fixture").expect("脚本夹具");
    #[cfg(windows)]
    assert!(matches!(
        GitProbe::new(&script),
        Err(GitError::InvalidProgram)
    ));
    assert!(matches!(
        fixture.git.probe("relative").await,
        Err(GitError::RelativePath)
    ));
    assert!(matches!(
        fixture.git.probe(fixture.root.join("missing")).await,
        Err(GitError::Io(_))
    ));
    assert!(matches!(
        fixture.git.probe(script).await,
        Err(GitError::NotDirectory)
    ));
    assert!(matches!(
        fixture.git.probe(&fixture.root).await,
        Err(GitError::NotRepository)
    ));
    let bare = fixture.directory("bare");
    fixture.git(&bare, &["init", "--bare"]);
    assert!(matches!(
        fixture.git.probe(&bare).await,
        Err(GitError::NoWorktree)
    ));
    let repository = fixture.repo("normal");
    assert!(matches!(
        fixture.git.probe(repository.join(".git")).await,
        Err(GitError::NoWorktree)
    ));
}

#[tokio::test]
async fn preserves_real_merge_conflicts() {
    let fixture = Fixture::new();
    let repository = fixture.repo("conflict");
    fs::write(repository.join("file"), "base\n").expect("base");
    fixture.commit(&repository);
    fixture.git(&repository, &["checkout", "-b", "other"]);
    fs::write(repository.join("file"), "other\n").expect("other");
    fixture.commit(&repository);
    fixture.git(&repository, &["checkout", "main"]);
    fs::write(repository.join("file"), "main\n").expect("main");
    fixture.commit(&repository);
    assert!(
        !fixture
            .run(&repository, &["merge", "other"])
            .status
            .success()
    );
    let before = file_bytes(&fixture.root);
    let snapshot = fixture.git.probe(&repository).await.expect("冲突探测");
    assert!(
        matches!(&snapshot.status.entries[0], StatusEntry::Unmerged(change) if change.status.index == 'U' && change.status.worktree == 'U')
    );
    assert_eq!(before, file_bytes(&fixture.root));
}

#[tokio::test]
async fn marker_scripts_prove_local_include_and_worktree_sources_are_safe() {
    for source in [
        "local",
        "include",
        "conditional_include",
        "worktree",
        "worktree_include",
    ] {
        for hook in ["clean", "process", "fsmonitor"] {
            let fixture = Fixture::new();
            let repository = fixture.repo("repo");
            fs::write(repository.join("file"), "base\n").expect("base");
            fs::write(repository.join(".gitattributes"), "file filter=fixture\n")
                .expect("启用过滤器属性");
            fixture.commit(&repository);
            let worktree = source.starts_with("worktree");
            let directory = if worktree {
                fixture.git(
                    &repository,
                    &["config", "extensions.worktreeConfig", "true"],
                );
                fixture.git(
                    &repository,
                    &["worktree", "add", "-b", "linked", "../linked"],
                );
                fixture.root.join("linked")
            } else {
                repository
            };
            let git_dir = fixture
                .git
                .probe(&directory)
                .await
                .expect("配置前探测")
                .git_dir;
            let key = if hook == "fsmonitor" {
                "core.fsmonitor".to_owned()
            } else {
                format!("filter.fixture.{hook}")
            };
            let command = "sh ../marker.sh";
            let tail = if hook == "clean" { "cat\n" } else { "exit 1\n" };
            fs::write(
                fixture.root.join("marker.sh"),
                format!("printf executed > ../marker\n{tail}"),
            )
            .expect("可执行副作用夹具");
            fs::write(directory.join("file"), "changed content longer than base\n")
                .expect("触发状态内容比较");
            if source.contains("include") {
                let contents = if hook == "fsmonitor" {
                    format!("[core]\nfsmonitor = {command}\n")
                } else {
                    format!("[filter \"fixture\"]\n{hook} = {command}\n")
                };
                fs::write(git_dir.join("marker.config"), contents).expect("include 过滤器配置");
                let include_key = if source == "conditional_include" {
                    "includeIf.onbranch:main.path"
                } else {
                    "include.path"
                };
                let scope = if worktree { "--worktree" } else { "--local" };
                fixture.git(&directory, &["config", scope, include_key, "marker.config"]);
            } else {
                let scope = if worktree { "--worktree" } else { "--local" };
                fixture.git(&directory, &["config", scope, &key, command]);
            }
            let marker = fixture.root.join("marker");
            let before = file_bytes(&fixture.root);
            let result = fixture.git.probe(&directory).await;
            if hook == "fsmonitor" {
                result.expect("强制禁用 fsmonitor");
            } else {
                assert!(
                    matches!(result, Err(GitError::ExternalFilter)),
                    "{source}/{hook}: {result:?}"
                );
            }
            assert!(!marker.exists(), "安全探测不得执行 {source}/{hook}");
            assert_eq!(before, file_bytes(&fixture.root));
            let control = if hook == "fsmonitor" {
                fixture.run(
                    &directory,
                    &["status", "--porcelain=v2", "--untracked-files=no"],
                )
            } else {
                fixture.run(&directory, &["diff", "--", "file"])
            };
            assert!(
                marker.exists(),
                "正向对照必须触发 {source}/{hook}：{}",
                String::from_utf8_lossy(&control.stderr)
            );
            assert_eq!(
                fs::read(marker).expect("正向对照必须真实触发脚本"),
                b"executed",
                "{source}/{hook}"
            );
        }
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn linux_preserves_non_utf8_and_newline_repository_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let fixture = Fixture::new();
    let repository = fixture
        .root
        .join(OsString::from_vec(b"repo\n\xff".to_vec()));
    fs::create_dir(&repository).expect("特殊目录");
    fixture.git(&repository, &["init", "--initial-branch=main"]);
    fs::write(
        repository.join(OsString::from_vec(b"file\n\xfe".to_vec())),
        "raw",
    )
    .expect("特殊文件");
    let before = file_bytes(&fixture.root);
    let snapshot = fixture.git.probe(&repository).await.expect("原始路径探测");
    assert_eq!(snapshot.root, repository);
    assert!(
        matches!(&snapshot.status.entries[0], StatusEntry::Untracked(path) if path.as_bytes() == b"file\n\xfe")
    );
    assert_eq!(before, file_bytes(&fixture.root));
}

#[tokio::test]
async fn rejects_gitlinks_before_reading_submodule_configuration() {
    let fixture = Fixture::new();
    let repository = fixture.repo("gitlinks");
    fs::write(repository.join("file"), "base\n").expect("base");
    fixture.commit(&repository);
    let output = fixture.run(&repository, &["rev-parse", "HEAD"]);
    let oid = std::str::from_utf8(&output.stdout)
        .expect("OID ASCII")
        .trim();
    let entry = format!("160000,{oid},module");
    fixture.git(
        &repository,
        &["update-index", "--add", "--cacheinfo", &entry],
    );
    let before = file_bytes(&fixture.root);
    assert!(matches!(
        fixture.git.probe(&repository).await,
        Err(GitError::SubmodulesUnsupported)
    ));
    assert_eq!(before, file_bytes(&fixture.root));
}

#[tokio::test]
async fn accepts_sha256_repositories() {
    let fixture = Fixture::new();
    let repository = fixture.directory("sha256");
    fixture.git(
        &repository,
        &["init", "--initial-branch=main", "--object-format=sha256"],
    );
    fs::write(repository.join("file"), "base\n").expect("base");
    fixture.commit(&repository);
    let before = file_bytes(&fixture.root);
    let snapshot = fixture.git.probe(&repository).await.expect("SHA-256 仓库");
    assert!(matches!(snapshot.head, HeadState::Attached { oid, .. } if oid.len() == 64));
    assert_eq!(before, file_bytes(&fixture.root));
}

#[tokio::test]
async fn native_status_preserves_untracked_nested_repository_directory_record() {
    let fixture = Fixture::new();
    let repository = fixture.repo("outer");
    let nested = repository.join("inner 中文 repo");
    fs::create_dir(&nested).expect("内嵌仓库目录");
    fixture.git(&nested, &["init", "--initial-branch=main"]);
    fs::write(nested.join("file"), "nested content\n").expect("内嵌仓库文件");
    fixture.commit(&nested);
    let output = fixture.run(
        &repository,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, "? inner 中文 repo/\0".as_bytes());
    println!("真实未跟踪目录记录：{:?}", output.stdout);
    let before = file_bytes(&fixture.root);
    let snapshot = fixture
        .git
        .probe(&repository)
        .await
        .expect("探测合法目录记录");
    assert!(
        matches!(&snapshot.status.entries[0], StatusEntry::Untracked(path) if path.as_bytes() == "inner 中文 repo/".as_bytes())
    );
    assert_eq!(before, file_bytes(&fixture.root));
    fs::write(repository.join(".gitignore"), "inner 中文 repo/\n").expect("忽略目录");
    let output = fixture.run(
        &repository,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--ignored=matching",
        ],
    );
    assert!(output.status.success());
    assert!(
        output
            .stdout
            .split(|byte| *byte == 0)
            .any(|record| record == "! inner 中文 repo/".as_bytes())
    );
    println!("真实忽略目录记录：{:?}", output.stdout);
    let snapshot = soxu_git::parse_status(&output.stdout).expect("解析真实忽略目录输出");
    assert!(snapshot.entries.iter().any(|entry| matches!(entry, StatusEntry::Ignored(path) if path.as_bytes() == "inner 中文 repo/".as_bytes())));
    let before = file_bytes(&fixture.root);
    assert!(
        !fixture
            .git
            .probe(&repository)
            .await
            .expect("默认忽略枚举关闭")
            .status
            .entries
            .iter()
            .any(|entry| matches!(entry, StatusEntry::Ignored(_)))
    );
    assert_eq!(before, file_bytes(&fixture.root));
}

// ---- 夹具后台维护竞态：有界 trace2 对照（#105） ----

/// trace2 在一次夹具提交里观察到的后台维护子进程事实。
///
/// 只保存有界、脱敏的摘要（布尔与计数），回归失败时也只打印这份摘要，
/// 不在 CI 里转储原始 trace2 事件或整份环境。
#[derive(Debug)]
struct MaintenanceTrace {
    /// trace2 通道确实写出了事件；否则“未派生”只是空断言。
    written: bool,
    /// 是否派生了 `git maintenance` 子进程。
    started: bool,
    /// 该子进程是否以 `--detach` 运行（会在夹具外残留后台进程）。
    detached: bool,
    /// 是否观察到同一发出进程 `sid` 下、同一 `child_id` 的 `child_exit`。
    exited: bool,
}

/// 一次有界夹具提交的结局；各状态互斥且可区分。
#[derive(Debug)]
enum CommitOutcome {
    /// 直接子进程在期限内以成功状态退出。
    Succeeded,
    /// 直接子进程在期限内以非零状态退出。
    Failed(Option<i32>),
    /// 超过期限；已尽力终止直接子进程，并在回收窗口内确认它已退出。
    TimedOut,
    /// `try_wait` 观察自身失败；已尽力终止直接子进程，并在回收窗口内确认它已退出。
    WaitErrored,
    /// 上述任一路径下都未能在回收窗口内确认直接子进程退出。
    ReapUnconfirmed,
}

/// 有界提交的结局加同一次提交的 trace2 摘要。
#[derive(Debug)]
struct CommitObservation {
    outcome: CommitOutcome,
    trace: MaintenanceTrace,
}

/// 提交等待上限：正常提交在亚秒级完成，这里留出充足的机器抖动余量。
const COMMIT_DEADLINE: Duration = Duration::from_secs(60);
/// 等待与回收的轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// 终止后只在这个窗口内确认**直接子进程**回收，不断言其后代。
const REAP_WINDOW: Duration = Duration::from_secs(10);

/// 正向对照参数：只重新打开后台维护，`gc.auto=0` 保证不触发真实 gc 工作，
/// 并显式禁止 detach，让提交同步等待维护结束。
const CONTROL_MAINTENANCE_ARGS: [&str; 8] = [
    "-c",
    "maintenance.auto=true",
    "-c",
    "gc.auto=0",
    "-c",
    "maintenance.autoDetach=false",
    "-c",
    "gc.autoDetach=false",
];

/// 从 trace2 事件行取 `"child_id":N`。
///
/// 这是**发出该事件的进程内部**的子进程序号，单独使用不足以定位某个子进程。
fn trace_child_id(line: &str) -> Option<u32> {
    const KEY: &str = "\"child_id\":";
    let rest = line.get(line.find(KEY)? + KEY.len()..)?;
    rest.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// 从 trace2 事件行取发出该事件的进程 `sid`。
///
/// trace2 用 `/` 把父子 sid 串起来，因此逐字比较即可区分不同进程的同类事件。
fn trace_sid(line: &str) -> Option<&str> {
    const KEY: &str = "\"sid\":\"";
    let rest = line.get(line.find(KEY)? + KEY.len()..)?;
    Some(&rest[..rest.find('"')?])
}

/// 汇总 trace2 文本里的子进程事实。
///
/// `.git/objects/maintenance.lock` 由 `git maintenance run` 持有，是平台失败的写源，
/// 因此按 trace2 事件断言子进程的启动与退出。启动与退出必须绑定**同一发出进程
/// `sid` + 同一 `child_id`**：只看数字会把后一层 Git 进程的同号子进程误算成维护已结束。
fn summarize_trace(text: &str) -> MaintenanceTrace {
    let mut started: Vec<(&str, u32)> = Vec::new();
    let mut detached = false;
    for line in text.lines() {
        if !line.contains("\"event\":\"child_start\"") || !line.contains("\"maintenance\"") {
            continue;
        }
        let (Some(sid), Some(child_id)) = (trace_sid(line), trace_child_id(line)) else {
            continue;
        };
        started.push((sid, child_id));
        detached |= line.contains("\"--detach\"");
    }
    let exited = text.lines().any(|line| {
        if !line.contains("\"event\":\"child_exit\"") {
            return false;
        }
        let (Some(sid), Some(child_id)) = (trace_sid(line), trace_child_id(line)) else {
            return false;
        };
        started.contains(&(sid, child_id))
    });
    MaintenanceTrace {
        written: !text.is_empty(),
        started: !started.is_empty(),
        detached,
        exited,
    }
}

/// 读 trace2 文件并汇总；文件缺失或为空时 `written = false`。
fn read_maintenance_trace(path: &Path) -> MaintenanceTrace {
    summarize_trace(&fs::read_to_string(path).unwrap_or_default())
}

/// 在回收窗口内确认直接子进程已退出；超出窗口即判未确认。
fn reap_direct_child(child: &mut std::process::Child) -> bool {
    let deadline = Instant::now() + REAP_WINDOW;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_INTERVAL),
            _ => return false,
        }
    }
}

/// 有界等待直接子进程：到期或观察失败都尽力终止，再走同一条有界回收。
///
/// 这里只对**直接子进程**负责；没有也不会声称其后代已被回收。
/// 超时与 `try_wait` 观察失败分别成 `TimedOut` / `WaitErrored`，两者都不能被静默丢弃。
fn await_child(child: &mut std::process::Child, deadline: Duration) -> CommitOutcome {
    let started = Instant::now();
    let mut observed_error = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    CommitOutcome::Succeeded
                } else {
                    CommitOutcome::Failed(status.code())
                };
            }
            Ok(None) if started.elapsed() < deadline => std::thread::sleep(POLL_INTERVAL),
            Ok(None) => break,
            Err(_) => {
                observed_error = true;
                break;
            }
        }
    }
    let _ = child.kill();
    if !reap_direct_child(child) {
        return CommitOutcome::ReapUnconfirmed;
    }
    if observed_error {
        CommitOutcome::WaitErrored
    } else {
        CommitOutcome::TimedOut
    }
}

/// Git 的 trace2 目标不接受 Windows `canonicalize` 产生的 verbatim 前缀，
/// 只在降级结果仍是绝对路径时去掉它（`\\?\UNC\s\share` → `\\s\share`）。
fn trace_target(root: &Path, name: &str) -> PathBuf {
    let text = root.join(name).to_string_lossy().into_owned();
    let normalized = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        text
    };
    PathBuf::from(normalized)
}

/// 走与 `commit` 完全相同的命令形状做一次提交，但把等待做成有界的。
///
/// trace 写在本夹具真实根内（名字在独占夹具根内唯一，不预删）；stdin 关闭、
/// stdout/stderr 不接管道，证据只从 trace2 读。
fn traced_commit(
    fixture: &Fixture,
    directory: &Path,
    guard_overrides: &[&str],
    label: &str,
) -> CommitObservation {
    static TRACE_SEQ: AtomicUsize = AtomicUsize::new(0);
    let trace = trace_target(
        &fixture.root,
        &format!("trace2-{}.json", TRACE_SEQ.fetch_add(1, Ordering::Relaxed)),
    );
    // 每次写入不同内容，确保 `git commit` 不会因 nothing-to-commit 而失败。
    fs::write(directory.join("traced"), label.as_bytes()).expect("写提交内容");
    fixture.git(directory, &["add", "--all"]);
    let mut command = fixture.commit_command(directory, guard_overrides, Some(&trace));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().expect("启动夹具提交");
    let outcome = await_child(&mut child, COMMIT_DEADLINE);
    let observed = read_maintenance_trace(&trace);
    let _ = fs::remove_file(&trace);
    CommitObservation {
        outcome,
        trace: observed,
    }
}

/// 把结局渲染成有界的一行摘要，供断言失败信息使用。
fn outcome_detail(outcome: &CommitOutcome) -> String {
    match outcome {
        CommitOutcome::Succeeded => "succeeded".to_owned(),
        CommitOutcome::Failed(code) => format!("failed(exit={code:?})"),
        CommitOutcome::TimedOut => "timed_out".to_owned(),
        CommitOutcome::WaitErrored => "wait_errored".to_owned(),
        CommitOutcome::ReapUnconfirmed => "reap_unconfirmed".to_owned(),
    }
}

/// 断言一次有界提交在期限内成功；失败信息带出具体结局。
fn assert_commit_succeeded(observed: &CommitObservation, stage: &str) {
    assert!(
        matches!(observed.outcome, CommitOutcome::Succeeded),
        "{stage}未在期限内成功（{}）",
        outcome_detail(&observed.outcome)
    );
}

/// `child_id` 只是发出进程内的序号：同号但不同 `sid` 不得算作维护进程已退出。
///
/// 用合成 trace2 文本覆盖三类情形：同 `sid` + 同 `child_id` 计入；同 `child_id`
/// 但不同 `sid` 不计入；同 `sid` 但不同 `child_id` 也不计入。
#[test]
fn maintenance_exit_must_come_from_the_same_parent_sid() {
    let started = r#"{"event":"child_start","sid":"trace-root","child_id":0,"argv":["git","maintenance","run","--auto","--quiet","--no-detach"]}"#;
    let nested_sid_exit =
        r#"{"event":"child_exit","sid":"trace-root/nested","child_id":0,"code":0}"#;
    let other_child_exit = r#"{"event":"child_exit","sid":"trace-root","child_id":1,"code":0}"#;
    let same_sid_exit = r#"{"event":"child_exit","sid":"trace-root","child_id":0,"code":0}"#;

    let nested = summarize_trace(&format!("{started}\n{nested_sid_exit}\n"));
    assert!(nested.written && nested.started, "{nested:?}");
    assert!(
        !nested.exited,
        "同 child_id 但不同 sid 不得计为已退出：{nested:?}"
    );

    let other_child = summarize_trace(&format!("{started}\n{other_child_exit}\n"));
    assert!(
        !other_child.exited,
        "同 sid 但不同 child_id 不得计为已退出：{other_child:?}"
    );

    let matched = summarize_trace(&format!("{started}\n{same_sid_exit}\n"));
    assert!(
        matched.exited,
        "同 sid + 同 child_id 必须计为已退出：{matched:?}"
    );
}

/// 夹具提交不得派生后台 `git maintenance`。
///
/// CI run `45976349619` 的 `macos-15` ARM64 job `140429021160` 上，
/// `accepts_sha256_repositories` 与 `detects_unborn_attached_and_detached_without_writing`
/// 的两份快照唯一差异是 `.git/objects/maintenance.lock`；派生来源此前未观测。
/// 本用例用 trace2 直接断言受测路径不派生该子进程，并用同构的正向对照证明观察通道有效，
/// 不靠重跑、sleep、锁文件筛除或跳过平台。
///
/// 对照走同一条提交路径，只重新打开后台维护并要求同步不 detach：
/// 必须同时看到子进程启动、无 `--detach`、以及同 `sid` + 同 `child_id` 的 `child_exit`。
/// 两个提交都走有界等待，阻塞会被判成 `TimedOut` / `WaitErrored` / `ReapUnconfirmed`。
#[test]
fn fixture_commits_never_start_background_maintenance() {
    let guarded = Fixture::new();
    let repository = guarded.repo("guard 受测");
    let observed = traced_commit(&guarded, &repository, &[], "guarded\n");
    assert_commit_succeeded(&observed, "受测提交");
    assert!(
        observed.trace.written,
        "trace2 未写入，本次观察无效：{observed:?}"
    );
    assert!(
        !observed.trace.started,
        "夹具提交派生了后台 git maintenance 子进程：{observed:?}"
    );

    let control = Fixture::new();
    let repository = control.repo("control 对照");
    let observed = traced_commit(
        &control,
        &repository,
        &CONTROL_MAINTENANCE_ARGS,
        "control\n",
    );
    assert_commit_succeeded(&observed, "对照提交");
    assert!(
        observed.trace.written,
        "对照提交的 trace2 未写入：{observed:?}"
    );
    assert!(
        observed.trace.started,
        "对照提交未观察到 maintenance 子进程，观察通道失效：{observed:?}"
    );
    assert!(
        !observed.trace.detached,
        "对照的维护子进程仍以 --detach 运行，会残留后台进程：{observed:?}"
    );
    assert!(
        observed.trace.exited,
        "对照未观察到同 sid + 同 child_id 的 child_exit，无法确认维护已结束：{observed:?}"
    );
}
